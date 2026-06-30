//! Query-driven local search index — "Google but completely local".
//!
//! Fuses the existing SearXNG meta-search (`research::`), the web scraper
//! (`scraper::`) and the AI curator (`ai::` / `memory::`) behind a single
//! `local_search` command, plus a NEW unified FTS5 + vector index with a fusion
//! ranker. The first search for a topic is slow (live discover → scrape → index);
//! every search after that hits a warm local index and is instant.
//!
//! The index lives in its OWN tables (`search_pages`, `search_pages_fts`,
//! `page_embeddings`, `search_queries`, `result_clicks`), deliberately separate
//! from the crawler's `scraped_pages` (which is job-scoped, `job_id NOT NULL`).
//! We reuse the crawler/research/ai code, we do not rewrite or entangle it.
//!
//! Privacy: the index and the click-log only ever contain things the user
//! actually searched/visited, and never leave the device.

pub mod schema;
pub mod retention;
pub mod adapters;
pub mod embed;
pub mod store;
pub mod rank;
pub mod orchestrator;
pub mod gc;
pub mod curate;
pub mod lifecycle;

use rusqlite::Connection;

/// Manager for the local search index. Follows the project convention: holds the
/// db path and opens a fresh connection per operation (no shared pool).
#[derive(Clone)]
pub struct SearchIndexManager {
    pub db_path: String,
}

impl SearchIndexManager {
    pub fn new(db_path: String) -> Self {
        Self { db_path }
    }

    /// Open a connection with the pragmas this feature relies on:
    /// `foreign_keys=ON` (so `page_embeddings`' ON DELETE CASCADE actually fires —
    /// rusqlite does NOT enable FK enforcement by default) and a busy timeout so
    /// concurrent scrape-writers don't trip "database is locked".
    pub fn open(db_path: &str) -> rusqlite::Result<Connection> {
        let conn = Connection::open(db_path)?;
        conn.busy_timeout(std::time::Duration::from_secs(10))?;
        let _ = conn.pragma_update(None, "foreign_keys", "ON");
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        Ok(conn)
    }

    pub fn conn(&self) -> rusqlite::Result<Connection> {
        Self::open(&self.db_path)
    }

    /// Idempotent, forward-only migration. Safe to run on every startup.
    pub fn init(&self) -> rusqlite::Result<()> {
        let conn = self.conn()?;
        schema::apply(&conn)
    }
}

#[cfg(test)]
mod integration {
    //! End-to-end exercises over real SQLite: triggers, FK cascade, dedup, archive,
    //! retention protection, forget_query, GC — the things unit tests can't prove.
    use super::*;
    use rusqlite::{params, Connection};

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        schema::apply(&conn).unwrap();
        conn
    }

    fn fts_has(conn: &Connection, term: &str, id: i64) -> bool {
        store::fts_order(conn, &format!("\"{}\"", term))
            .unwrap()
            .contains(&id)
    }

    #[test]
    fn upsert_dedups_by_url_and_resyncs_fts() {
        let conn = db();
        let qid = store::insert_query(&conn, "q", "cache", 100, 1).unwrap();
        let id1 = store::upsert_scraped(
            &conn, 1, "https://a.test", "T", "alpha beta", "s", Some(qid),
            "web", Some(0), "h1", "cache", 100, Some(700),
        ).unwrap();
        assert!(fts_has(&conn, "beta", id1));

        // Same URL again → same row id (dedup), body replaced, FTS resynced.
        let id2 = store::upsert_scraped(
            &conn, 1, "https://a.test", "T2", "gamma delta", "s", Some(qid),
            "web", Some(1), "h2", "cache", 200, Some(800),
        ).unwrap();
        assert_eq!(id1, id2, "dedup by (url, profile)");
        assert!(!fts_has(&conn, "beta", id1), "old terms gone from FTS");
        assert!(fts_has(&conn, "gamma", id1), "new terms present in FTS");
    }

    #[test]
    fn archive_drops_body_fts_embedding_keeps_row() {
        let conn = db();
        let id = store::upsert_scraped(
            &conn, 1, "https://b.test", "T", "searchable words here", "s", None,
            "web", None, "h", "pinned", 100, None,
        ).unwrap();
        store::upsert_embedding(&conn, id, &[0.1, 0.2, 0.3]).unwrap();
        assert!(fts_has(&conn, "searchable", id));

        store::archive(&conn, id).unwrap();

        assert!(!fts_has(&conn, "searchable", id), "FTS emptied on archive");
        let emb = store::load_embeddings(&conn, &[id]).unwrap();
        assert!(emb.is_empty(), "embedding dropped on archive");
        let (retention, body): (String, Option<String>) = conn
            .query_row("SELECT retention, body FROM search_pages WHERE id=?1", params![id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(retention, "archived");
        assert!(body.is_none(), "body dropped");
    }

    #[test]
    fn pinned_retention_is_not_downgraded_by_a_later_search() {
        let conn = db();
        // First favorited/pinned.
        let id = store::upsert_scraped(
            &conn, 1, "https://c.test", "T", "body", "s", None,
            "web", None, "h", "pinned", 100, None,
        ).unwrap();
        // A later ephemeral search re-scrapes the same URL.
        store::upsert_scraped(
            &conn, 1, "https://c.test", "T", "body2", "s", Some(5),
            "web", None, "h2", "ephemeral", 200, Some(900),
        ).unwrap();
        let (retention, expires): (String, Option<i64>) = conn
            .query_row("SELECT retention, expires_at FROM search_pages WHERE id=?1", params![id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(retention, "pinned", "must not downgrade a pin to ephemeral");
        assert!(expires.is_none(), "pinned stays non-expiring");
    }

    #[test]
    fn forget_query_spares_pinned_and_removes_the_rest() {
        let conn = db();
        let qid = store::insert_query(&conn, "q", "cache", 100, 1).unwrap();
        let cache_id = store::upsert_scraped(
            &conn, 1, "https://keep-not.test", "T", "x", "s", Some(qid),
            "web", None, "h1", "cache", 100, Some(700),
        ).unwrap();
        let pinned_id = store::upsert_scraped(
            &conn, 1, "https://keep.test", "T", "y", "s", Some(qid),
            "web", None, "h2", "pinned", 100, None,
        ).unwrap();

        let removed = store::forget_query(&conn, qid).unwrap();
        assert_eq!(removed, 1, "only the non-pinned page is removed");

        let cache_gone: i64 = conn
            .query_row("SELECT COUNT(*) FROM search_pages WHERE id=?1", params![cache_id], |r| r.get(0))
            .unwrap();
        let pinned_alive: i64 = conn
            .query_row("SELECT COUNT(*) FROM search_pages WHERE id=?1", params![pinned_id], |r| r.get(0))
            .unwrap();
        let query_gone: i64 = conn
            .query_row("SELECT COUNT(*) FROM search_queries WHERE id=?1", params![qid], |r| r.get(0))
            .unwrap();
        assert_eq!(cache_gone, 0);
        assert_eq!(pinned_alive, 1, "pinned page survives forget_query");
        assert_eq!(query_gone, 0, "the query row itself is dropped");
    }

    #[test]
    fn favorite_pin_then_unfavorite_demotes_to_cache() {
        let conn = db();
        // Simulate set_favorite(true) on an indexed page: pin it.
        let id = store::upsert_scraped(
            &conn, 1, "https://d.test", "T", "body", "s", Some(1),
            "web", None, "h", "cache", 100, Some(700),
        ).unwrap();
        store::pin(&conn, id, 150).unwrap();
        let meta = store::find_page_by_url(&conn, 1, "https://d.test").unwrap().unwrap();
        assert_eq!(meta.retention, "pinned");
        assert!(meta.expires_at.is_none());

        // set_favorite(false): demote to cache with a fresh TTL (no hard delete).
        store::demote_to_cache(&conn, id, 9999).unwrap();
        let meta2 = store::find_page_by_url(&conn, 1, "https://d.test").unwrap().unwrap();
        assert_eq!(meta2.retention, "cache");
        assert_eq!(meta2.expires_at, Some(9999));
        let alive: i64 = conn
            .query_row("SELECT COUNT(*) FROM search_pages WHERE id=?1", params![id], |r| r.get(0))
            .unwrap();
        assert_eq!(alive, 1, "unfavorite never hard-deletes");
    }
}
