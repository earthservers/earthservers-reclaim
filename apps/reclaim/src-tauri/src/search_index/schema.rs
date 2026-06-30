//! Phase 1 — schema & migration for the local search index.
//!
//! All statements are idempotent (`IF NOT EXISTS`) and forward-only. The FTS5
//! virtual table is EXTERNAL-CONTENT over `search_pages`, which does NOT auto-sync
//! — the three triggers below are mandatory; without them the index silently
//! drifts. Column names in the triggers MUST match `search_pages` exactly.

use rusqlite::Connection;

/// Apply (or re-apply) the full search-index schema.
pub fn apply(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(SCHEMA_SQL)?;
    Ok(())
}

pub const SCHEMA_SQL: &str = r#"
-- One row per scraped/indexed page in the QUERY-DRIVEN index. Separate from the
-- crawler's job-scoped `scraped_pages`. `body` is nullable so the archive op can
-- drop it while keeping url/title and the knowledge-graph summary.
CREATE TABLE IF NOT EXISTS search_pages (
  id             INTEGER PRIMARY KEY AUTOINCREMENT,
  url            TEXT NOT NULL,
  title          TEXT NOT NULL DEFAULT '',
  body           TEXT,
  query_id       INTEGER,                       -- FK -> search_queries.id (nullable: cache/browse rows)
  source_engine  TEXT,                          -- 'reddit' | 'web' | 'hn' | ...
  content_hash   TEXT,                          -- sha256 of normalized body, for dedup
  fetched_at     INTEGER,                       -- unix seconds
  expires_at     INTEGER,                       -- NULL = never expires
  retention      TEXT NOT NULL DEFAULT 'ephemeral', -- 'ephemeral'|'cache'|'pinned'|'archived'
  last_opened_at INTEGER,                        -- bumped on navigate
  open_count     INTEGER NOT NULL DEFAULT 0,
  pinned_at      INTEGER,                        -- when promoted to pinned
  upstream_gone  INTEGER NOT NULL DEFAULT 0,     -- 1 = live URL now 404/410; protect local copy
  profile_id     INTEGER,                        -- per-profile isolation, matches the rest of the app
  UNIQUE(url, profile_id)
);
CREATE INDEX IF NOT EXISTS idx_searchpages_hash   ON search_pages(content_hash);
CREATE INDEX IF NOT EXISTS idx_searchpages_url    ON search_pages(url);
CREATE INDEX IF NOT EXISTS idx_searchpages_expiry ON search_pages(retention, expires_at);
CREATE INDEX IF NOT EXISTS idx_searchpages_query  ON search_pages(query_id);

-- One row per user search, so results group and the click-log attaches.
CREATE TABLE IF NOT EXISTS search_queries (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  query_text  TEXT NOT NULL,
  created_at  INTEGER NOT NULL,
  retention   TEXT NOT NULL DEFAULT 'ephemeral',
  profile_id  INTEGER
);

-- Full-text "grep" layer. External-content FTS5 over search_pages.
CREATE VIRTUAL TABLE IF NOT EXISTS search_pages_fts USING fts5(
  title,
  body,
  content='search_pages',
  content_rowid='id',
  tokenize='porter unicode61'
);

-- MANDATORY external-content sync triggers. Missing/wrong = silent index drift.
CREATE TRIGGER IF NOT EXISTS search_pages_ai AFTER INSERT ON search_pages BEGIN
  INSERT INTO search_pages_fts(rowid, title, body) VALUES (new.id, new.title, new.body);
END;
CREATE TRIGGER IF NOT EXISTS search_pages_ad AFTER DELETE ON search_pages BEGIN
  INSERT INTO search_pages_fts(search_pages_fts, rowid, title, body)
    VALUES('delete', old.id, old.title, old.body);
END;
CREATE TRIGGER IF NOT EXISTS search_pages_au AFTER UPDATE ON search_pages BEGIN
  INSERT INTO search_pages_fts(search_pages_fts, rowid, title, body)
    VALUES('delete', old.id, old.title, old.body);
  INSERT INTO search_pages_fts(rowid, title, body) VALUES (new.id, new.title, new.body);
END;

-- Semantic layer. sqlite-vec is NOT available in this build (rusqlite bundled),
-- so v1 stores f32 little-endian vectors as a BLOB and does cosine in Rust over
-- the per-query candidate set. TODO(sqlite-vec): swap in a vec0 KNN if ever wired.
CREATE TABLE IF NOT EXISTS page_embeddings (
  page_id INTEGER PRIMARY KEY REFERENCES search_pages(id) ON DELETE CASCADE,
  dim     INTEGER NOT NULL,
  vec     BLOB    NOT NULL          -- f32 little-endian, length = dim*4
);

-- Private click-log = personalization signal. Never leaves the device.
CREATE TABLE IF NOT EXISTS result_clicks (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  query_id   INTEGER,
  url        TEXT NOT NULL,
  domain     TEXT,
  clicked_at INTEGER NOT NULL,
  profile_id INTEGER
);
CREATE INDEX IF NOT EXISTS idx_clicks_domain ON result_clicks(domain);
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn mem() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        apply(&conn).unwrap();
        conn
    }

    fn insert_page(conn: &Connection, url: &str, title: &str, body: &str) -> i64 {
        conn.execute(
            "INSERT INTO search_pages (url, title, body, fetched_at, retention)
             VALUES (?1, ?2, ?3, 0, 'cache')",
            rusqlite::params![url, title, body],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn fts_count(conn: &Connection, term: &str) -> i64 {
        conn.query_row(
            "SELECT count(*) FROM search_pages_fts WHERE search_pages_fts MATCH ?1",
            rusqlite::params![term],
            |r| r.get(0),
        )
        .unwrap()
    }

    #[test]
    fn migration_is_idempotent() {
        let conn = mem();
        // Re-applying must not error (forward-only / IF NOT EXISTS).
        apply(&conn).unwrap();
        apply(&conn).unwrap();
    }

    #[test]
    fn fts_syncs_on_insert_update_delete() {
        let conn = mem();
        let id = insert_page(&conn, "https://a.test", "Rust async", "tokio runtime grep me");
        assert_eq!(fts_count(&conn, "tokio"), 1, "insert trigger must populate FTS");
        assert_eq!(fts_count(&conn, "grep"), 1);

        // UPDATE: body changes -> old terms gone, new terms present.
        conn.execute(
            "UPDATE search_pages SET body = ?2 WHERE id = ?1",
            rusqlite::params![id, "replaced wholly different words"],
        )
        .unwrap();
        assert_eq!(fts_count(&conn, "tokio"), 0, "update trigger must drop old terms");
        assert_eq!(fts_count(&conn, "replaced"), 1, "update trigger must add new terms");

        // Archive path: body -> NULL empties FTS for that row.
        conn.execute(
            "UPDATE search_pages SET body = NULL WHERE id = ?1",
            rusqlite::params![id],
        )
        .unwrap();
        assert_eq!(fts_count(&conn, "replaced"), 0, "nulling body must clear FTS");

        // DELETE: removes the row from FTS entirely.
        let id2 = insert_page(&conn, "https://b.test", "Another", "uniqueword here");
        assert_eq!(fts_count(&conn, "uniqueword"), 1);
        conn.execute("DELETE FROM search_pages WHERE id = ?1", rusqlite::params![id2])
            .unwrap();
        assert_eq!(fts_count(&conn, "uniqueword"), 0, "delete trigger must remove from FTS");
    }

    #[test]
    fn embeddings_cascade_on_page_delete() {
        let conn = mem();
        let id = insert_page(&conn, "https://c.test", "Vec", "vector body");
        conn.execute(
            "INSERT INTO page_embeddings (page_id, dim, vec) VALUES (?1, 2, ?2)",
            rusqlite::params![id, vec![0u8; 8]],
        )
        .unwrap();
        let before: i64 = conn
            .query_row("SELECT count(*) FROM page_embeddings", [], |r| r.get(0))
            .unwrap();
        assert_eq!(before, 1);
        conn.execute("DELETE FROM search_pages WHERE id = ?1", rusqlite::params![id])
            .unwrap();
        let after: i64 = conn
            .query_row("SELECT count(*) FROM page_embeddings", [], |r| r.get(0))
            .unwrap();
        assert_eq!(after, 0, "ON DELETE CASCADE must drop the embedding (FK pragma ON)");
    }
}
