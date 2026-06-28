//! Password gate for the Local AI / History tab — its OWN unique password, PER
//! PROFILE, separate from the vault, media, and private-bookmarks passwords.
//! Argon2id hash keyed by profile_id. The "unlocked for this session" state lives
//! in the frontend; the backend only stores/sets/verifies the hash.
//!
//! Earlier versions stored a single global row shared by every profile;
//! `ensure_table` migrates that away (drops it) so each profile has its own gate.
//! Nothing the gate protects is encrypted with this password, so dropping the old
//! shared gate loses no data — it just resets to "no password" until one is set.

use std::sync::Mutex;

use rusqlite::{params, Connection};
use tauri::State;

use crate::AppState;

fn db_path(state: &State<'_, Mutex<AppState>>) -> Result<String, String> {
    Ok(state.lock().map_err(|e| e.to_string())?.db_path.clone())
}

fn ensure_table(conn: &Connection) -> Result<(), String> {
    let exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='ai_lock_auth'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    if exists > 0 {
        // Legacy global schema has an `id` column and no `profile_id`.
        let has_profile_id = conn
            .prepare("PRAGMA table_info(ai_lock_auth)")
            .and_then(|mut s| {
                let cols: Vec<String> = s
                    .query_map([], |r| r.get::<_, String>(1))?
                    .filter_map(|r| r.ok())
                    .collect();
                Ok(cols.iter().any(|c| c == "profile_id"))
            })
            .unwrap_or(false);
        if !has_profile_id {
            conn.execute("DROP TABLE ai_lock_auth", [])
                .map_err(|e| e.to_string())?;
        }
    }
    conn.execute(
        "CREATE TABLE IF NOT EXISTS ai_lock_auth (profile_id INTEGER PRIMARY KEY, hash TEXT NOT NULL)",
        [],
    )
    .map(|_| ())
    .map_err(|e| e.to_string())
}

fn has(db: &str, profile_id: i64) -> bool {
    Connection::open(db)
        .ok()
        .and_then(|conn| {
            let _ = ensure_table(&conn);
            conn.query_row(
                "SELECT COUNT(*) FROM ai_lock_auth WHERE profile_id = ?1",
                params![profile_id],
                |r| r.get::<_, i64>(0),
            )
            .ok()
        })
        .map(|n| n > 0)
        .unwrap_or(false)
}

/// Verify a password. Returns true if it matches OR if no password is set
/// (an unset gate is "open").
fn verify(db: &str, profile_id: i64, password: &str) -> bool {
    let stored: Option<String> = Connection::open(db).ok().and_then(|conn| {
        let _ = ensure_table(&conn);
        conn.query_row(
            "SELECT hash FROM ai_lock_auth WHERE profile_id = ?1",
            params![profile_id],
            |r| r.get(0),
        )
        .ok()
    });
    match stored {
        Some(h) => {
            use argon2::password_hash::{PasswordHash, PasswordVerifier};
            use argon2::Argon2;
            PasswordHash::new(&h)
                .map(|ph| Argon2::default().verify_password(password.as_bytes(), &ph).is_ok())
                .unwrap_or(false)
        }
        None => true,
    }
}

/// Whether a password is set for the Local AI / History tab (this profile).
#[tauri::command(rename_all = "camelCase")]
pub async fn ai_lock_has_password(state: State<'_, Mutex<AppState>>, profile_id: i64) -> Result<bool, String> {
    Ok(has(&db_path(&state)?, profile_id))
}

/// Verify the password (used to unlock the tab for the session).
#[tauri::command(rename_all = "camelCase")]
pub async fn ai_lock_verify_password(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    password: String,
) -> Result<bool, String> {
    Ok(verify(&db_path(&state)?, profile_id, &password))
}

/// Set (or change) the password.
#[tauri::command(rename_all = "camelCase")]
pub async fn ai_lock_set_password(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    password: String,
) -> Result<(), String> {
    use argon2::password_hash::{rand_core::OsRng, PasswordHasher, SaltString};
    use argon2::Argon2;
    if password.trim().is_empty() {
        return Err("Password cannot be empty".into());
    }
    let db = db_path(&state)?;
    let conn = Connection::open(&db).map_err(|e| e.to_string())?;
    ensure_table(&conn)?;
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| e.to_string())?
        .to_string();
    conn.execute(
        "INSERT OR REPLACE INTO ai_lock_auth (profile_id, hash) VALUES (?1, ?2)",
        params![profile_id, hash],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Remove the password (requires the current one).
#[tauri::command(rename_all = "camelCase")]
pub async fn ai_lock_remove_password(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    password: String,
) -> Result<(), String> {
    let db = db_path(&state)?;
    if !verify(&db, profile_id, &password) {
        return Err("Incorrect password".into());
    }
    let conn = Connection::open(&db).map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM ai_lock_auth WHERE profile_id = ?1", params![profile_id])
        .map_err(|e| e.to_string())?;
    Ok(())
}
