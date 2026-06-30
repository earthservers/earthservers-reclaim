//! Phase 6b — auto-GC. Expires ephemeral/cache rows with no human in the loop.
//! NEVER touches pinned or archived. Runs once on startup and hourly thereafter.

use super::orchestrator::now_secs;

/// One sweep. Returns the number of pages removed.
pub fn sweep(db_path: &str) -> Result<usize, String> {
    let conn = super::SearchIndexManager::open(db_path).map_err(|e| e.to_string())?;
    super::store::gc_sweep(&conn, now_secs()).map_err(|e| e.to_string())
}

/// Sweep now, then every hour. Spawned from setup. Cheap; bails quietly on error.
pub fn start(db_path: String) {
    match sweep(&db_path) {
        Ok(n) if n > 0 => log::info!("[search_index] startup GC removed {} expired pages", n),
        Ok(_) => {}
        Err(e) => log::warn!("[search_index] startup GC failed: {}", e),
    }
    // Use Tauri's async runtime — `start` is called from the setup hook, which is
    // NOT inside a Tokio reactor context, so a bare `tokio::spawn` panics.
    tauri::async_runtime::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(3600));
        // The first tick fires immediately; skip it (we just swept above).
        tick.tick().await;
        loop {
            tick.tick().await;
            match sweep(&db_path) {
                Ok(n) if n > 0 => log::info!("[search_index] hourly GC removed {} expired pages", n),
                Ok(_) => {}
                Err(e) => log::warn!("[search_index] hourly GC failed: {}", e),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    fn setup() -> (rusqlite::Connection, String) {
        // Use a temp file DB so sweep() (which opens by path) sees the same data.
        let dir = std::env::temp_dir();
        let path = dir
            .join(format!("reclaim_gc_test_{}.db", std::process::id()))
            .to_string_lossy()
            .to_string();
        let _ = std::fs::remove_file(&path);
        let conn = super::super::SearchIndexManager::open(&path).unwrap();
        super::super::schema::apply(&conn).unwrap();
        (conn, path)
    }

    #[test]
    fn sweep_removes_only_expired_cheap_tiers() {
        let (conn, path) = setup();
        let now = now_secs();
        let mk = |retention: &str, expires: Option<i64>| {
            conn.execute(
                "INSERT INTO search_pages (url, title, body, retention, expires_at, fetched_at, profile_id)
                 VALUES (?1, 'T', 'b', ?2, ?3, ?4, 1)",
                params![format!("https://{}-{}.test", retention, expires.unwrap_or(-1)), retention, expires, now],
            )
            .unwrap();
            conn.last_insert_rowid()
        };
        let expired_eph = mk("ephemeral", Some(now - 10));
        let expired_cache = mk("cache", Some(now - 10));
        let fresh_cache = mk("cache", Some(now + 1000));
        let pinned = mk("pinned", None);
        let archived = mk("archived", None);
        // pinned page with a (nonsense) past expiry must STILL survive
        let pinned_with_expiry = mk("pinned", Some(now - 10));

        let removed = sweep(&path).unwrap();
        assert_eq!(removed, 2, "only the two expired cheap-tier rows");

        let alive = |id: i64| -> bool {
            conn.query_row("SELECT COUNT(*) FROM search_pages WHERE id=?1", params![id], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap()
                == 1
        };
        assert!(!alive(expired_eph));
        assert!(!alive(expired_cache));
        assert!(alive(fresh_cache));
        assert!(alive(pinned));
        assert!(alive(archived));
        assert!(alive(pinned_with_expiry), "pinned is never auto-GC'd even if expires_at is past");

        let _ = std::fs::remove_file(&path);
    }
}
