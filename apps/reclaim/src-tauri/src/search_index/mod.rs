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
