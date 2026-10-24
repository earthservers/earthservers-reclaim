//! Encrypted, durable storage for assistant chat sessions.
//!
//! Chats used to live ONLY in the webview's `localStorage`, which meant they
//! vanished whenever the origin changed (dev server vs. `tauri://` vs. the
//! packaged asset server) or the user cleared site data — while the journaled
//! copies in `indexed_pages` survived. Sessions now live in SQLite so they
//! persist, and are ENCRYPTED AT REST so reading the .db file doesn't expose
//! conversations.
//!
//! Key material, in order of preference:
//!   1. The profile's AI/History password (Argon2id-derived) when one is set.
//!      Strongest: the ciphertext is unreadable without the password.
//!   2. The OS-keyring `local_data_secret()` when no password is set. The gate
//!      treats "no password" as open, so chats must still work — this at least
//!      keeps them out of plaintext on disk.
//!
//! Each row records which scheme encrypted it (`key_scheme`), so switching a
//! password can re-encrypt existing rows instead of orphaning them.

use std::sync::Mutex;

use rusqlite::{params, Connection};
use tauri::State;

use crate::AppState;

/// How a row's ciphertext was keyed. Stored per-row so a password change can
/// find and re-encrypt the rows it needs to.
const SCHEME_KEYRING: &str = "keyring-v1";
const SCHEME_PASSWORD: &str = "argon2-aes-v1";

/// A stored conversation. `messages` is the JSON the frontend owns — the backend
/// treats it as an opaque blob, so adding fields to a message needs no migration.
#[derive(serde::Serialize, serde::Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StoredChat {
    pub id: String,
    pub title: String,
    /// JSON array of ChatMessage, as produced by the frontend.
    pub messages: String,
    pub updated_at: i64,
}

fn db_path(state: &State<'_, Mutex<AppState>>) -> Result<String, String> {
    Ok(state.lock().map_err(|e| e.to_string())?.db_path.clone())
}

fn ensure_table(conn: &Connection) -> Result<(), String> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS ai_chats (
            id          TEXT NOT NULL,
            profile_id  INTEGER NOT NULL,
            title       TEXT NOT NULL,
            payload     TEXT NOT NULL,
            key_scheme  TEXT NOT NULL,
            updated_at  INTEGER NOT NULL,
            PRIMARY KEY (profile_id, id)
        )",
        [],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Derive a 32-byte AES key from a user password with Argon2id, salted per
/// profile so the same password on two profiles yields different keys.
///
/// Argon2id (memory-hard) rather than the SHA-256 derivation used for
/// low-sensitivity data — this key guards conversation contents, and a fast hash
/// would make an offline dictionary attack cheap.
fn key_from_password(password: &str, profile_id: i64) -> Result<[u8; 32], String> {
    use argon2::Argon2;
    let salt = format!("earthservers::aichats::{}", profile_id);
    let mut key = [0u8; 32];
    Argon2::default()
        .hash_password_into(password.as_bytes(), salt.as_bytes(), &mut key)
        .map_err(|e| format!("key derivation failed: {}", e))?;
    Ok(key)
}

fn encrypt_with_key(plaintext: &str, key: &[u8; 32]) -> Result<String, String> {
    use aes_gcm::aead::{Aead, KeyInit};
    use aes_gcm::{Aes256Gcm, Nonce};
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
    use rand::Rng;

    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| e.to_string())?;
    let nonce_bytes: [u8; 12] = rand::thread_rng().gen();
    let ct = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), plaintext.as_bytes())
        .map_err(|e| format!("encryption failed: {}", e))?;
    let mut out = nonce_bytes.to_vec();
    out.extend(ct);
    Ok(BASE64.encode(out))
}

fn decrypt_with_key(encrypted: &str, key: &[u8; 32]) -> Result<String, String> {
    use aes_gcm::aead::{Aead, KeyInit};
    use aes_gcm::{Aes256Gcm, Nonce};
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| e.to_string())?;
    let data = BASE64.decode(encrypted).map_err(|e| format!("invalid base64: {}", e))?;
    if data.len() < 12 {
        return Err("encrypted payload too short".into());
    }
    let pt = cipher
        .decrypt(Nonce::from_slice(&data[..12]), &data[12..])
        .map_err(|_| "decryption failed — wrong password or corrupted data".to_string())?;
    String::from_utf8(pt).map_err(|e| format!("invalid UTF-8: {}", e))
}

/// The no-password fallback key: the random OS-keyring secret, hashed to 32
/// bytes. Not password-strength, but keeps chats out of plaintext on disk.
fn keyring_key() -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let secret = crate::multimedia::local_data_secret();
    let mut hasher = Sha256::new();
    hasher.update(b"earthservers::aichats::keyring::v1");
    hasher.update(secret.as_bytes());
    hasher.finalize().into()
}

/// Resolve the key to use for a write, plus the scheme tag to record with it.
fn key_for_write(password: Option<&str>, profile_id: i64) -> Result<([u8; 32], &'static str), String> {
    match password {
        Some(p) if !p.is_empty() => Ok((key_from_password(p, profile_id)?, SCHEME_PASSWORD)),
        _ => Ok((keyring_key(), SCHEME_KEYRING)),
    }
}

/// Load every chat for a profile, newest first.
///
/// Rows that can't be decrypted (e.g. a password-encrypted row with no password
/// supplied) are SKIPPED rather than failing the whole load — one unreadable
/// chat shouldn't hide the rest.
#[tauri::command(rename_all = "camelCase")]
pub async fn ai_chats_load(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    password: Option<String>,
) -> Result<Vec<StoredChat>, String> {
    let db = db_path(&state)?;
    let conn = Connection::open(&db).map_err(|e| e.to_string())?;
    ensure_table(&conn)?;

    let pw_key = match password.as_deref() {
        Some(p) if !p.is_empty() => Some(key_from_password(p, profile_id)?),
        _ => None,
    };
    let kr_key = keyring_key();

    let mut stmt = conn
        .prepare(
            "SELECT id, title, payload, key_scheme, updated_at FROM ai_chats
             WHERE profile_id = ?1 ORDER BY updated_at DESC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![profile_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
            ))
        })
        .map_err(|e| e.to_string())?;

    let mut out = Vec::new();
    for (id, title, payload, scheme, updated_at) in rows.flatten() {
        let key = if scheme == SCHEME_PASSWORD {
            match pw_key {
                Some(k) => k,
                None => continue, // locked: no password supplied
            }
        } else {
            kr_key
        };
        // Titles are encrypted too — they leak the gist of a conversation.
        let title = decrypt_with_key(&title, &key).unwrap_or_else(|_| "(locked)".to_string());
        match decrypt_with_key(&payload, &key) {
            Ok(messages) => out.push(StoredChat { id, title, messages, updated_at }),
            Err(_) => continue,
        }
    }
    Ok(out)
}

/// Insert or update one chat.
#[tauri::command(rename_all = "camelCase")]
pub async fn ai_chats_save(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    chat: StoredChat,
    password: Option<String>,
) -> Result<(), String> {
    let db = db_path(&state)?;
    let conn = Connection::open(&db).map_err(|e| e.to_string())?;
    ensure_table(&conn)?;

    let (key, scheme) = key_for_write(password.as_deref(), profile_id)?;
    let payload = encrypt_with_key(&chat.messages, &key)?;
    let title = encrypt_with_key(&chat.title, &key)?;
    conn.execute(
        "INSERT INTO ai_chats (id, profile_id, title, payload, key_scheme, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(profile_id, id) DO UPDATE SET
            title = excluded.title, payload = excluded.payload,
            key_scheme = excluded.key_scheme, updated_at = excluded.updated_at
         WHERE excluded.updated_at >= ai_chats.updated_at",
        params![chat.id, profile_id, title, payload, scheme, chat.updated_at],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Delete one chat.
#[tauri::command(rename_all = "camelCase")]
pub async fn ai_chats_delete(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    id: String,
) -> Result<(), String> {
    let db = db_path(&state)?;
    let conn = Connection::open(&db).map_err(|e| e.to_string())?;
    ensure_table(&conn)?;
    conn.execute(
        "DELETE FROM ai_chats WHERE profile_id = ?1 AND id = ?2",
        params![profile_id, id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Re-encrypt every chat for a profile under a new key. Called when the AI
/// password is set, changed, or removed — otherwise rows keyed to the old
/// password become permanently unreadable.
///
/// `old_password`/`new_password` are None to mean "the keyring key".
#[tauri::command(rename_all = "camelCase")]
pub async fn ai_chats_rekey(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    old_password: Option<String>,
    new_password: Option<String>,
) -> Result<u32, String> {
    let db = db_path(&state)?;
    let conn = Connection::open(&db).map_err(|e| e.to_string())?;
    ensure_table(&conn)?;

    let old_pw_key = match old_password.as_deref() {
        Some(p) if !p.is_empty() => Some(key_from_password(p, profile_id)?),
        _ => None,
    };
    let kr_key = keyring_key();
    let (new_key, new_scheme) = key_for_write(new_password.as_deref(), profile_id)?;

    let mut stmt = conn
        .prepare("SELECT id, title, payload, key_scheme FROM ai_chats WHERE profile_id = ?1")
        .map_err(|e| e.to_string())?;
    let rows: Vec<(String, String, String, String)> = stmt
        .query_map(params![profile_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })
        .map_err(|e| e.to_string())?
        .flatten()
        .collect();

    let mut changed = 0u32;
    for (id, title, payload, scheme) in rows {
        let old_key = if scheme == SCHEME_PASSWORD {
            match old_pw_key {
                Some(k) => k,
                None => continue, // can't read it without the old password
            }
        } else {
            kr_key
        };
        let (Ok(plain_title), Ok(plain_payload)) = (
            decrypt_with_key(&title, &old_key),
            decrypt_with_key(&payload, &old_key),
        ) else {
            continue; // leave undecryptable rows untouched rather than destroying them
        };
        let title_enc = encrypt_with_key(&plain_title, &new_key)?;
        let payload_enc = encrypt_with_key(&plain_payload, &new_key)?;
        conn.execute(
            "UPDATE ai_chats SET title = ?1, payload = ?2, key_scheme = ?3
             WHERE profile_id = ?4 AND id = ?5",
            params![title_enc, payload_enc, new_scheme, profile_id, id],
        )
        .map_err(|e| e.to_string())?;
        changed += 1;
    }
    Ok(changed)
}

/// Bumped when the import algorithm changes, so an already-migrated profile
/// re-runs it once with the new logic. v1 created one session per exchange
/// (every turn showed up as its own conversation); v2 groups them.
const JOURNAL_IMPORT_VERSION: i64 = 2;

/// Gap between two journaled exchanges that starts a NEW conversation. Turns
/// within one sitting are seconds-to-minutes apart; a fresh conversation
/// realistically comes after a break.
const SESSION_GAP_MS: i64 = 30 * 60 * 1000;

fn ensure_meta_table(conn: &Connection) -> Result<(), String> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS ai_chats_meta (
            profile_id      INTEGER PRIMARY KEY,
            import_version  INTEGER NOT NULL
        )",
        [],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// One-time migration: rebuild sessions from the journaled `reclaim://chat/...`
/// entries in `indexed_pages`, for conversations whose localStorage copy was
/// lost. Journaled rows store `Q: <question>\nA: <answer>`, ONE ROW PER EXCHANGE
/// — so consecutive rows are grouped back into a single conversation by time
/// proximity rather than each becoming its own chat.
///
/// Runs at most once per profile per `JOURNAL_IMPORT_VERSION`. This matters: the
/// curator keeps journaling new exchanges as you chat, so re-running would
/// re-import conversations that already live in `ai_chats` as duplicate
/// fragments. Journal rows at or after the first live chat are skipped for the
/// same reason — those conversations are already stored properly.
///
/// Returns how many conversations were imported (0 when already migrated).
#[tauri::command(rename_all = "camelCase")]
pub async fn ai_chats_import_journal(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    password: Option<String>,
) -> Result<u32, String> {
    let db = db_path(&state)?;
    let conn = Connection::open(&db).map_err(|e| e.to_string())?;
    ensure_table(&conn)?;
    ensure_meta_table(&conn)?;

    // Already migrated at this version? Nothing to do.
    let done: i64 = conn
        .query_row(
            "SELECT import_version FROM ai_chats_meta WHERE profile_id = ?1",
            params![profile_id],
            |r| r.get(0),
        )
        .unwrap_or(0);
    if done >= JOURNAL_IMPORT_VERSION {
        return Ok(0);
    }

    // Only import journal rows that predate the earliest REAL (non-imported)
    // chat — anything newer belongs to a conversation already saved properly.
    let cutoff: i64 = conn
        .query_row(
            "SELECT MIN(updated_at) FROM ai_chats WHERE profile_id = ?1 AND id NOT LIKE 'journal-%'",
            params![profile_id],
            |r| r.get::<_, Option<i64>>(0),
        )
        .ok()
        .flatten()
        .unwrap_or(i64::MAX);

    // Drop previously imported fragments — they're derived data, and v1 split
    // every exchange into its own chat. Never touches real chats.
    conn.execute(
        "DELETE FROM ai_chats WHERE profile_id = ?1 AND id LIKE 'journal-%'",
        params![profile_id],
    )
    .map_err(|e| e.to_string())?;

    let mut stmt = conn
        .prepare(
            "SELECT url, title, COALESCE(content, '') FROM indexed_pages
             WHERE profile_id = ?1 AND url LIKE 'reclaim://chat/%'
             ORDER BY url ASC",
        )
        .map_err(|e| e.to_string())?;
    let rows: Vec<(String, String, String)> = stmt
        .query_map(params![profile_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .map_err(|e| e.to_string())?
        .flatten()
        .collect();

    let groups = group_exchanges(&rows, cutoff);

    let (key, scheme) = key_for_write(password.as_deref(), profile_id)?;
    let mut imported = 0u32;
    for g in groups {
        let messages: Vec<serde_json::Value> = g
            .turns
            .iter()
            .flat_map(|(q, a)| {
                [
                    serde_json::json!({ "role": "user", "content": q }),
                    serde_json::json!({ "role": "assistant", "content": a }),
                ]
            })
            .collect();
        conn.execute(
            "INSERT OR REPLACE INTO ai_chats (id, profile_id, title, payload, key_scheme, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                g.id,
                profile_id,
                encrypt_with_key(&g.title, &key)?,
                encrypt_with_key(&serde_json::Value::Array(messages).to_string(), &key)?,
                scheme,
                g.last_ts
            ],
        )
        .map_err(|e| e.to_string())?;
        imported += 1;
    }

    conn.execute(
        "INSERT OR REPLACE INTO ai_chats_meta (profile_id, import_version) VALUES (?1, ?2)",
        params![profile_id, JOURNAL_IMPORT_VERSION],
    )
    .map_err(|e| e.to_string())?;

    Ok(imported)
}

/// A run of consecutive journaled exchanges that belong to one conversation.
struct JournalGroup {
    id: String,
    title: String,
    turns: Vec<(String, String)>,
    last_ts: i64,
}

/// Group journaled exchanges into conversations by time proximity.
///
/// `rows` are `(url, title, content)` ordered by url ASC — the url embeds an
/// RFC3339 timestamp, so lexical order is chronological. Rows at or after
/// `cutoff` are ignored (they belong to chats already stored properly).
fn group_exchanges(rows: &[(String, String, String)], cutoff: i64) -> Vec<JournalGroup> {
    let mut groups: Vec<JournalGroup> = Vec::new();
    for (url, title, content) in rows {
        let stamp = url.trim_start_matches("reclaim://chat/").to_string();
        let ts = parse_ts_millis(&stamp);
        if ts >= cutoff {
            continue;
        }
        let Some((question, answer)) = split_qa(content) else { continue };

        match groups.last_mut() {
            // Same sitting → another turn in the conversation we're building.
            Some(g) if ts.saturating_sub(g.last_ts) <= SESSION_GAP_MS => {
                g.turns.push((question, answer));
                g.last_ts = ts;
            }
            _ => groups.push(JournalGroup {
                id: format!("journal-{}", stamp),
                title: title.strip_prefix("Chat: ").unwrap_or(title).trim().to_string(),
                turns: vec![(question, answer)],
                last_ts: ts,
            }),
        }
    }
    groups
}

/// Split a journaled `Q: ...\nA: ...` body into its question and answer.
fn split_qa(content: &str) -> Option<(String, String)> {
    let rest = content.strip_prefix("Q: ")?;
    // The answer starts at the first line that begins with "A: ".
    let idx = if rest.starts_with("A: ") {
        Some(0)
    } else {
        rest.find("\nA: ").map(|i| i + 1)
    }?;
    let question = rest[..idx].trim_end().to_string();
    let answer = rest[idx + 3..].trim().to_string();
    if question.is_empty() && answer.is_empty() {
        return None;
    }
    Some((question, answer))
}

/// Best-effort timestamp (millis) from an RFC3339 / SQLite datetime string.
fn parse_ts_millis(s: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|d| d.timestamp_millis())
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
                .map(|d| d.and_utc().timestamp_millis())
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_through_encryption() {
        let key = [7u8; 32];
        let ct = encrypt_with_key("hello chats", &key).unwrap();
        assert_ne!(ct, "hello chats", "must not store plaintext");
        assert_eq!(decrypt_with_key(&ct, &key).unwrap(), "hello chats");
    }

    #[test]
    fn wrong_key_fails_to_decrypt() {
        let ct = encrypt_with_key("secret", &[1u8; 32]).unwrap();
        assert!(decrypt_with_key(&ct, &[2u8; 32]).is_err());
    }

    #[test]
    fn nonce_is_random_per_write() {
        let key = [3u8; 32];
        assert_ne!(
            encrypt_with_key("same", &key).unwrap(),
            encrypt_with_key("same", &key).unwrap(),
            "identical plaintext must not produce identical ciphertext"
        );
    }

    #[test]
    fn password_key_is_profile_scoped() {
        let a = key_from_password("hunter2", 1).unwrap();
        let b = key_from_password("hunter2", 2).unwrap();
        assert_ne!(a, b, "same password on different profiles must differ");
        assert_eq!(a, key_from_password("hunter2", 1).unwrap(), "must be deterministic");
    }

    #[test]
    fn splits_journaled_qa() {
        let (q, a) = split_qa("Q: what is 2+2\nA: 4").unwrap();
        assert_eq!(q, "what is 2+2");
        assert_eq!(a, "4");
    }

    #[test]
    fn splits_multiline_question_and_answer() {
        let (q, a) = split_qa("Q: line one\nline two\nA: answer one\nanswer two").unwrap();
        assert_eq!(q, "line one\nline two");
        assert_eq!(a, "answer one\nanswer two");
    }

    #[test]
    fn rejects_non_journal_content() {
        assert!(split_qa("just some text").is_none());
    }

    fn row(stamp: &str, q: &str) -> (String, String, String) {
        (
            format!("reclaim://chat/{}", stamp),
            format!("Chat: {}", q),
            format!("Q: {}\nA: answer to {}", q, q),
        )
    }

    #[test]
    fn groups_consecutive_turns_into_one_conversation() {
        // The real bug: turns minutes apart were each becoming their own chat.
        let rows = vec![
            row("2026-07-29T07:45:57+00:00", "first"),
            row("2026-07-29T07:47:00+00:00", "second"),
            row("2026-07-29T07:51:08+00:00", "third"),
        ];
        let g = group_exchanges(&rows, i64::MAX);
        assert_eq!(g.len(), 1, "one sitting must be ONE conversation");
        assert_eq!(g[0].turns.len(), 3);
        assert_eq!(g[0].title, "first", "titled after the opening question");
    }

    #[test]
    fn splits_conversations_separated_by_a_long_gap() {
        let rows = vec![
            row("2026-07-07T18:44:48+00:00", "old chat"),
            row("2026-07-29T06:39:33+00:00", "new chat"),
            row("2026-07-29T06:41:00+00:00", "same as new"),
        ];
        let g = group_exchanges(&rows, i64::MAX);
        assert_eq!(g.len(), 2, "a 3-week gap starts a new conversation");
        assert_eq!(g[0].turns.len(), 1);
        assert_eq!(g[1].turns.len(), 2);
    }

    #[test]
    fn cutoff_excludes_live_era_rows() {
        let rows = vec![
            row("2026-07-29T07:00:00+00:00", "before"),
            row("2026-07-29T09:00:00+00:00", "after"),
        ];
        let cutoff = parse_ts_millis("2026-07-29T08:00:00+00:00");
        let g = group_exchanges(&rows, cutoff);
        assert_eq!(g.len(), 1, "rows at/after the cutoff must be skipped");
        assert_eq!(g[0].title, "before");
    }

    #[test]
    fn group_id_is_stable_across_runs() {
        let rows = vec![row("2026-07-29T07:45:57+00:00", "x")];
        assert_eq!(
            group_exchanges(&rows, i64::MAX)[0].id,
            group_exchanges(&rows, i64::MAX)[0].id
        );
        assert!(group_exchanges(&rows, i64::MAX)[0].id.starts_with("journal-"));
    }
}


