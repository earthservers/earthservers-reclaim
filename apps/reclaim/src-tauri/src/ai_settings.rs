//! Per-profile Local-AI on/off settings, persisted in SQLite.
//!
//! These used to live in the WebView's localStorage, but the browser window is
//! incognito (ephemeral storage), so localStorage is wiped on every restart and
//! the settings reverted to their defaults — the knowledge curator switched itself
//! back ON after each restart. Persisting per-profile in earthservers.db (mirroring
//! privacy.rs's incognito_state) fixes that: the user's choice sticks, per profile.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// Defaults for a profile that has never set these: curator on, assistant off
/// (matches the previous product default).
const DEFAULT_CURATOR: bool = true;
const DEFAULT_ASSISTANT: bool = false;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSettings {
    pub curator: bool,
    pub assistant: bool,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            curator: DEFAULT_CURATOR,
            assistant: DEFAULT_ASSISTANT,
        }
    }
}

/// Create the table if needed. Called once at startup.
pub fn init(db_path: &str) -> rusqlite::Result<()> {
    let conn = Connection::open(db_path)?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS ai_settings (
            profile_id INTEGER PRIMARY KEY,
            curator    INTEGER NOT NULL DEFAULT 1,
            assistant  INTEGER NOT NULL DEFAULT 0
         )",
        [],
    )?;
    Ok(())
}

fn read(db_path: &str, profile_id: i64) -> rusqlite::Result<AiSettings> {
    let conn = Connection::open(db_path)?;
    let row = conn
        .query_row(
            "SELECT curator, assistant FROM ai_settings WHERE profile_id = ?1",
            params![profile_id],
            |r| Ok((r.get::<_, i64>(0)? != 0, r.get::<_, i64>(1)? != 0)),
        )
        .ok();
    Ok(match row {
        Some((curator, assistant)) => AiSettings { curator, assistant },
        None => AiSettings::default(),
    })
}

fn write(db_path: &str, profile_id: i64, s: AiSettings) -> rusqlite::Result<()> {
    let conn = Connection::open(db_path)?;
    conn.execute(
        "INSERT INTO ai_settings (profile_id, curator, assistant) VALUES (?1, ?2, ?3)
         ON CONFLICT(profile_id) DO UPDATE SET curator = ?2, assistant = ?3",
        params![profile_id, s.curator as i64, s.assistant as i64],
    )?;
    Ok(())
}

fn db_path_of(app: &tauri::AppHandle) -> Result<String, String> {
    use tauri::Manager;
    let state = app.state::<std::sync::Mutex<crate::AppState>>();
    let st = state.lock().map_err(|e| e.to_string())?;
    Ok(st.db_path.clone())
}

/// Load a profile's Local-AI settings (falls back to defaults if never set).
#[tauri::command(rename_all = "camelCase")]
pub async fn get_ai_settings(
    app: tauri::AppHandle,
    profile_id: i64,
) -> Result<AiSettings, String> {
    let db_path = db_path_of(&app)?;
    read(&db_path, profile_id).map_err(|e| e.to_string())
}

/// Persist a profile's Local-AI settings.
#[tauri::command(rename_all = "camelCase")]
pub async fn set_ai_settings(
    app: tauri::AppHandle,
    profile_id: i64,
    curator: bool,
    assistant: bool,
) -> Result<(), String> {
    let db_path = db_path_of(&app)?;
    write(&db_path, profile_id, AiSettings { curator, assistant }).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> String {
        std::env::temp_dir()
            .join(format!("reclaim_aisettings_{}.db", std::process::id()))
            .to_string_lossy()
            .to_string()
    }

    #[test]
    fn persists_per_profile_and_defaults() {
        let path = tmp();
        let _ = std::fs::remove_file(&path);
        init(&path).unwrap();

        // Unset profile → defaults (curator on).
        let d = read(&path, 1).unwrap();
        assert!(d.curator && !d.assistant);

        // Turn curator OFF for profile 1; profile 2 stays default.
        write(&path, 1, AiSettings { curator: false, assistant: true }).unwrap();
        let p1 = read(&path, 1).unwrap();
        assert!(!p1.curator && p1.assistant, "profile 1 choice persists");
        let p2 = read(&path, 2).unwrap();
        assert!(p2.curator, "profile 2 is independent");

        // Re-open (simulates restart) → still off for profile 1.
        let p1b = read(&path, 1).unwrap();
        assert!(!p1b.curator, "survives 'restart' (re-open)");

        let _ = std::fs::remove_file(&path);
    }
}
