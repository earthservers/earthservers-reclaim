//! Saved searches & search history for the right-dock panel.
//!
//! History is the existing `search_queries` log (one row per `local_search` run),
//! grouped by query text so repeat runs collapse into one entry. Saved searches
//! are explicit user bookmarks of a query PLUS its config (retention / kinds /
//! sources) so re-running restores the exact same search. Both are per-profile
//! and never leave the device.

use super::orchestrator::now_secs;
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use tauri::Manager;

fn db_path_of(app: &tauri::AppHandle) -> Result<String, String> {
    let state = app.state::<std::sync::Mutex<crate::AppState>>();
    let st = state.lock().map_err(|e| e.to_string())?;
    Ok(st.db_path.clone())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    /// id of the MOST RECENT search_queries row for this text (display/anchor only —
    /// deletes go by text so older duplicates don't resurface).
    pub query_id: i64,
    pub query_text: String,
    pub last_searched_at: i64,
    pub times_searched: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedSearch {
    pub id: i64,
    pub query_text: String,
    pub retention: String,
    pub kinds_mode: String,
    /// JSON array of source ids; None = the default-enabled set at run time.
    pub sources: Option<Vec<String>>,
    pub created_at: i64,
}

/// Recent searches, newest first, deduped by query text.
#[tauri::command(rename_all = "camelCase")]
pub async fn list_search_history(
    app: tauri::AppHandle,
    profile_id: i64,
    limit: Option<i64>,
) -> Result<Vec<HistoryEntry>, String> {
    let db_path = db_path_of(&app)?;
    let conn = super::SearchIndexManager::open(&db_path).map_err(|e| e.to_string())?;
    let limit = limit.unwrap_or(50).clamp(1, 200);
    let mut stmt = conn
        .prepare(
            "SELECT MAX(id), query_text, MAX(created_at), COUNT(*)
             FROM search_queries
             WHERE profile_id = ?1 AND TRIM(query_text) != ''
             GROUP BY query_text
             ORDER BY MAX(created_at) DESC
             LIMIT ?2",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![profile_id, limit], |r| {
            Ok(HistoryEntry {
                query_id: r.get(0)?,
                query_text: r.get(1)?,
                last_searched_at: r.get(2)?,
                times_searched: r.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

/// Remove one query from history (ALL rows with this text, so it doesn't
/// resurface as an older entry). Indexed pages are untouched — this deletes the
/// log only; page retention/GC is a separate concern.
#[tauri::command(rename_all = "camelCase")]
pub async fn delete_search_history(
    app: tauri::AppHandle,
    profile_id: i64,
    query_text: String,
) -> Result<usize, String> {
    let db_path = db_path_of(&app)?;
    let conn = super::SearchIndexManager::open(&db_path).map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM search_queries WHERE profile_id = ?1 AND query_text = ?2",
        params![profile_id, query_text],
    )
    .map_err(|e| e.to_string())
}

/// Wipe the whole search history for a profile (log only, pages untouched).
#[tauri::command(rename_all = "camelCase")]
pub async fn clear_search_history(
    app: tauri::AppHandle,
    profile_id: i64,
) -> Result<usize, String> {
    let db_path = db_path_of(&app)?;
    let conn = super::SearchIndexManager::open(&db_path).map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM search_queries WHERE profile_id = ?1",
        params![profile_id],
    )
    .map_err(|e| e.to_string())
}

/// Save (or re-save, replacing config) a search. Unique per (profile, query text).
#[tauri::command(rename_all = "camelCase")]
pub async fn save_search(
    app: tauri::AppHandle,
    profile_id: i64,
    query_text: String,
    retention: String,
    kinds_mode: String,
    sources: Option<Vec<String>>,
) -> Result<i64, String> {
    let query_text = query_text.trim().to_string();
    if query_text.is_empty() {
        return Err("cannot save an empty search".into());
    }
    let sources_json = match &sources {
        Some(list) => Some(serde_json::to_string(list).map_err(|e| e.to_string())?),
        None => None,
    };
    let db_path = db_path_of(&app)?;
    let conn = super::SearchIndexManager::open(&db_path).map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO saved_searches (profile_id, query_text, retention, kinds_mode, sources, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(profile_id, query_text)
         DO UPDATE SET retention = ?3, kinds_mode = ?4, sources = ?5",
        params![profile_id, query_text, retention, kinds_mode, sources_json, now_secs()],
    )
    .map_err(|e| e.to_string())?;
    conn.query_row(
        "SELECT id FROM saved_searches WHERE profile_id = ?1 AND query_text = ?2",
        params![profile_id, query_text],
        |r| r.get(0),
    )
    .optional()
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "saved search not found after insert".into())
}

/// All saved searches for a profile, newest first.
#[tauri::command(rename_all = "camelCase")]
pub async fn list_saved_searches(
    app: tauri::AppHandle,
    profile_id: i64,
) -> Result<Vec<SavedSearch>, String> {
    let db_path = db_path_of(&app)?;
    let conn = super::SearchIndexManager::open(&db_path).map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, query_text, retention, kinds_mode, sources, created_at
             FROM saved_searches
             WHERE profile_id = ?1
             ORDER BY created_at DESC, id DESC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![profile_id], |r| {
            let sources_json: Option<String> = r.get(4)?;
            Ok(SavedSearch {
                id: r.get(0)?,
                query_text: r.get(1)?,
                retention: r.get(2)?,
                kinds_mode: r.get(3)?,
                sources: sources_json
                    .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok()),
                created_at: r.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn delete_saved_search(app: tauri::AppHandle, id: i64) -> Result<(), String> {
    let db_path = db_path_of(&app)?;
    let conn = super::SearchIndexManager::open(&db_path).map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM saved_searches WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::schema;
    use rusqlite::{params, Connection};

    fn mem() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        schema::apply(&conn).unwrap();
        conn
    }

    #[test]
    fn saved_searches_upsert_by_profile_and_text() {
        let conn = mem();
        conn.execute(
            "INSERT INTO saved_searches (profile_id, query_text, retention, kinds_mode, sources, created_at)
             VALUES (1, 'rust', 'cache', 'all', NULL, 10)
             ON CONFLICT(profile_id, query_text)
             DO UPDATE SET retention = 'cache', kinds_mode = 'all', sources = NULL",
            [],
        )
        .unwrap();
        // Same text, new config → replaces, no duplicate row.
        conn.execute(
            "INSERT INTO saved_searches (profile_id, query_text, retention, kinds_mode, sources, created_at)
             VALUES (1, 'rust', 'pinned', 'comments', '[\"web\"]', 20)
             ON CONFLICT(profile_id, query_text)
             DO UPDATE SET retention = 'pinned', kinds_mode = 'comments', sources = '[\"web\"]'",
            [],
        )
        .unwrap();
        let (count, retention): (i64, String) = conn
            .query_row(
                "SELECT COUNT(*), MAX(retention) FROM saved_searches WHERE profile_id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(count, 1, "upsert must not duplicate");
        assert_eq!(retention, "pinned", "config must be replaced");
    }

    #[test]
    fn history_groups_by_text_and_deletes_all_rows() {
        let conn = mem();
        for (t, at) in [("rust", 10), ("rust", 20), ("sqlite", 15)] {
            conn.execute(
                "INSERT INTO search_queries (query_text, created_at, retention, profile_id)
                 VALUES (?1, ?2, 'cache', 1)",
                params![t, at],
            )
            .unwrap();
        }
        let grouped: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM (SELECT query_text FROM search_queries WHERE profile_id = 1 GROUP BY query_text)",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(grouped, 2, "history groups by text");

        conn.execute(
            "DELETE FROM search_queries WHERE profile_id = 1 AND query_text = 'rust'",
            [],
        )
        .unwrap();
        let left: i64 = conn
            .query_row("SELECT COUNT(*) FROM search_queries WHERE profile_id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(left, 1, "delete-by-text removes ALL duplicates so none resurface");
    }
}
