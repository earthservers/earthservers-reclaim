//! Read-time fan-in of the CRAWLER's `scraped_pages` into unified search.
//!
//! Storage stays decoupled: the crawler's `scraped_pages` / `scraping_jobs` are
//! never modified by the search feature, retention GC never touches them, and
//! there are no synthetic job rows. All we do is add a dedicated external-content
//! FTS5 index OVER the existing crawler table (its column is `content`, NOT `body`)
//! so the ranker can grep crawler rows at query time and fuse them via RRF.

use rusqlite::{params, Connection, OptionalExtension};

/// A crawler page surfaced as a search candidate.
#[derive(Debug, Clone)]
pub struct CrawlerCand {
    pub id: i64,
    pub url: String,
    pub title: String,
    pub snippet: String,
    pub job: Option<String>,
}

fn table_exists(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type IN ('table','view') AND name = ?1",
        params![name],
        |_| Ok(()),
    )
    .optional()
    .ok()
    .flatten()
    .is_some()
}

/// Create the crawler FTS index + sync triggers if absent, and populate it once
/// from existing rows. No-op if `scraped_pages` doesn't exist yet. Idempotent.
pub fn ensure_fts(conn: &Connection) -> rusqlite::Result<()> {
    if !table_exists(conn, "scraped_pages") {
        return Ok(()); // crawler tables not created yet — nothing to index
    }
    let already = table_exists(conn, "scraped_pages_fts");
    conn.execute_batch(SCHEMA)?;
    if !already {
        // External-content FTS5 does NOT auto-index pre-existing rows; backfill once.
        conn.execute("INSERT INTO scraped_pages_fts(scraped_pages_fts) VALUES('rebuild')", [])?;
    }
    Ok(())
}

const SCHEMA: &str = r#"
CREATE VIRTUAL TABLE IF NOT EXISTS scraped_pages_fts USING fts5(
  title,
  content,
  content='scraped_pages',
  content_rowid='id',
  tokenize='porter unicode61'
);
CREATE TRIGGER IF NOT EXISTS scraped_pages_fts_ai AFTER INSERT ON scraped_pages BEGIN
  INSERT INTO scraped_pages_fts(rowid, title, content) VALUES (new.id, new.title, new.content);
END;
CREATE TRIGGER IF NOT EXISTS scraped_pages_fts_ad AFTER DELETE ON scraped_pages BEGIN
  INSERT INTO scraped_pages_fts(scraped_pages_fts, rowid, title, content)
    VALUES('delete', old.id, old.title, old.content);
END;
CREATE TRIGGER IF NOT EXISTS scraped_pages_fts_au AFTER UPDATE ON scraped_pages BEGIN
  INSERT INTO scraped_pages_fts(scraped_pages_fts, rowid, title, content)
    VALUES('delete', old.id, old.title, old.content);
  INSERT INTO scraped_pages_fts(rowid, title, content) VALUES (new.id, new.title, new.content);
END;
"#;

/// FTS5/BM25 over the crawler index → row ids best-first, capped at `limit`.
/// Returns empty if the FTS table doesn't exist. `match_expr` must be sanitized.
pub fn fts_rowids(conn: &Connection, match_expr: &str, limit: usize) -> rusqlite::Result<Vec<i64>> {
    if !table_exists(conn, "scraped_pages_fts") {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT rowid FROM scraped_pages_fts
          WHERE scraped_pages_fts MATCH ?1 ORDER BY rank LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![match_expr, limit as i64], |r| r.get::<_, i64>(0))?;
    rows.collect()
}

/// Load display metadata for crawler candidates, keyed by id (join to job name).
pub fn load_candidates(
    conn: &Connection,
    ids: &[i64],
) -> rusqlite::Result<std::collections::HashMap<i64, CrawlerCand>> {
    let mut map = std::collections::HashMap::new();
    if ids.is_empty() {
        return Ok(map);
    }
    let mut stmt = conn.prepare(
        "SELECT sp.id, sp.url, COALESCE(sp.title,''), substr(COALESCE(sp.content,''),1,240), sj.name
           FROM scraped_pages sp
           LEFT JOIN scraping_jobs sj ON sj.id = sp.job_id
          WHERE sp.id = ?1",
    )?;
    for &id in ids {
        if let Some(c) = stmt
            .query_row(params![id], |r| {
                Ok(CrawlerCand {
                    id: r.get(0)?,
                    url: r.get(1)?,
                    title: r.get(2)?,
                    snippet: r.get(3)?,
                    job: r.get(4)?,
                })
            })
            .optional()?
        {
            map.insert(id, c);
        }
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crawler_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        // Minimal stand-in for the crawler tables (matches scraper.rs columns).
        conn.execute_batch(
            "CREATE TABLE scraping_jobs (id INTEGER PRIMARY KEY, name TEXT);
             CREATE TABLE scraped_pages (id INTEGER PRIMARY KEY, job_id INTEGER, url TEXT,
                title TEXT, content TEXT, metadata TEXT, scraped_at TEXT);",
        )
        .unwrap();
        conn
    }

    #[test]
    fn ensure_fts_backfills_existing_rows_and_stays_synced() {
        let conn = crawler_db();
        conn.execute("INSERT INTO scraping_jobs (id, name) VALUES (1, 'docs crawl')", []).unwrap();
        conn.execute(
            "INSERT INTO scraped_pages (id, job_id, url, title, content) VALUES (1, 1, 'https://x.test', 'Old', 'preexisting widget text')",
            [],
        )
        .unwrap();

        // FTS created AFTER the row exists → must backfill via rebuild.
        ensure_fts(&conn).unwrap();
        let ids = fts_rowids(&conn, "\"widget\"", 10).unwrap();
        assert_eq!(ids, vec![1], "pre-existing crawler row is indexed on first ensure");

        // New crawler insert keeps FTS synced via triggers.
        conn.execute(
            "INSERT INTO scraped_pages (id, job_id, url, title, content) VALUES (2, 1, 'https://y.test', 'New', 'another gizmo here')",
            [],
        )
        .unwrap();
        assert_eq!(fts_rowids(&conn, "\"gizmo\"", 10).unwrap(), vec![2]);

        // Metadata + job name load.
        let meta = load_candidates(&conn, &[1]).unwrap();
        assert_eq!(meta.get(&1).unwrap().job.as_deref(), Some("docs crawl"));

        // ensure_fts is idempotent (second call must not error or double-index).
        ensure_fts(&conn).unwrap();
        assert_eq!(fts_rowids(&conn, "\"widget\"", 10).unwrap(), vec![1]);
    }

    #[test]
    fn ensure_fts_is_noop_without_crawler_table() {
        let conn = Connection::open_in_memory().unwrap();
        ensure_fts(&conn).unwrap(); // must not panic/err
        assert!(fts_rowids(&conn, "\"anything\"", 5).unwrap().is_empty());
    }
}
