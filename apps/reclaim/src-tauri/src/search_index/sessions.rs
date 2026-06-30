//! Optional user-supplied logged-in sessions for the social adapters (Part B5).
//!
//! Default OFF. The ONLY supported mode is "the user pasted their OWN session
//! cookie/token and explicitly opted in" — there is NO credential automation,
//! account creation, CAPTCHA/checkpoint solving, or auth/anti-bot circumvention
//! here. When OFF (default), adapters use the public logged-out path only.
//!
//! The session string is sensitive, so it's encrypted at rest with the device key
//! (same mechanism as bookmarks) and NEVER returned to the frontend — the UI only
//! learns whether a session is set + the enabled flag.

use rusqlite::{params, Connection};

fn key() -> String {
    crate::multimedia::local_data_secret()
}
fn enc(s: &str) -> String {
    crate::multimedia::encrypt_data(s, &key()).unwrap_or_else(|_| s.to_string())
}
fn dec(s: &str) -> Option<String> {
    crate::multimedia::decrypt_data(s, &key()).ok()
}

pub fn init(db_path: &str) -> rusqlite::Result<()> {
    let conn = Connection::open(db_path)?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS adapter_sessions (
            profile_id INTEGER NOT NULL,
            adapter_id TEXT NOT NULL,
            enabled    INTEGER NOT NULL DEFAULT 0,
            session    TEXT,
            PRIMARY KEY (profile_id, adapter_id)
         )",
        [],
    )?;
    Ok(())
}

/// Enabled adapter sessions for a profile: adapter_id → decrypted session string.
/// Only rows that are BOTH enabled AND have a stored session are returned.
pub fn load_enabled(db_path: &str, profile_id: i64) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let conn = match Connection::open(db_path) {
        Ok(c) => c,
        Err(_) => return map,
    };
    if let Ok(mut stmt) = conn.prepare(
        "SELECT adapter_id, session FROM adapter_sessions
          WHERE profile_id = ?1 AND enabled = 1 AND session IS NOT NULL AND TRIM(session) <> ''",
    ) {
        if let Ok(rows) = stmt.query_map(params![profile_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        }) {
            for row in rows.flatten() {
                if let Some(plain) = dec(&row.1) {
                    map.insert(row.0, plain);
                }
            }
        }
    }
    map
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionState {
    pub adapter_id: String,
    pub enabled: bool,
    pub has_session: bool,
}

fn db_path_of(app: &tauri::AppHandle) -> Result<String, String> {
    use tauri::Manager;
    let state = app.state::<std::sync::Mutex<crate::AppState>>();
    let st = state.lock().map_err(|e| e.to_string())?;
    Ok(st.db_path.clone())
}

/// Per-profile session state for the UI (never returns the secret itself).
#[tauri::command(rename_all = "camelCase")]
pub async fn get_adapter_sessions(
    app: tauri::AppHandle,
    profile_id: i64,
) -> Result<Vec<SessionState>, String> {
    let db_path = db_path_of(&app)?;
    let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT adapter_id, enabled, session FROM adapter_sessions WHERE profile_id = ?1")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![profile_id], |r| {
            let session: Option<String> = r.get(2)?;
            Ok(SessionState {
                adapter_id: r.get(0)?,
                enabled: r.get::<_, i64>(1)? != 0,
                has_session: session.map(|s| !s.trim().is_empty()).unwrap_or(false),
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(|e| e.to_string())
}

/// Enable/disable an adapter's logged-in session and (optionally) update the pasted
/// session string. `session = None` leaves the stored value as-is (so the user can
/// toggle without re-pasting); `Some("")` clears it.
#[tauri::command(rename_all = "camelCase")]
pub async fn set_adapter_session(
    app: tauri::AppHandle,
    profile_id: i64,
    adapter_id: String,
    enabled: bool,
    session: Option<String>,
) -> Result<(), String> {
    let db_path = db_path_of(&app)?;
    let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
    match session {
        Some(s) => {
            let stored = if s.trim().is_empty() { None } else { Some(enc(s.trim())) };
            conn.execute(
                "INSERT INTO adapter_sessions (profile_id, adapter_id, enabled, session)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(profile_id, adapter_id) DO UPDATE SET enabled = ?3, session = ?4",
                params![profile_id, adapter_id, enabled as i64, stored],
            )
            .map_err(|e| e.to_string())?;
        }
        None => {
            conn.execute(
                "INSERT INTO adapter_sessions (profile_id, adapter_id, enabled, session)
                 VALUES (?1, ?2, ?3, NULL)
                 ON CONFLICT(profile_id, adapter_id) DO UPDATE SET enabled = ?3",
                params![profile_id, adapter_id, enabled as i64],
            )
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
