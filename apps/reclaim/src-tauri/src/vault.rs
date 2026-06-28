//! Password Manager + OTP Authenticator vaults (per profile).
//!
//! A per-profile master password gates access (SHA-256 hash stored and verified).
//! Sensitive fields (the saved password / the OTP secret) are encrypted **with
//! the master password itself** (AES-256-GCM, key derived from the password), so
//! they are unreadable on disk until the vault is unlocked — even on this device.
//!
//! Unlocking caches the master password in memory only (never written to disk).
//! `verify_*_master` / `set_*_master` populate the session; `lock_*` clears it.
//! While locked there is no key, so entries can't be read or written.

use std::collections::HashMap;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use tauri::State;

use crate::multimedia::{decrypt_data, encrypt_data};
use crate::AppState;

const KIND_PASSWORD: &str = "password";
const KIND_OTP: &str = "otp";

lazy_static::lazy_static! {
    /// In-memory unlocked master passwords, keyed by (profile_id, kind). Holds
    /// the plaintext master password ONLY in RAM for the duration the vault is
    /// unlocked; used to derive the encryption key. Never persisted.
    static ref SESSIONS: Mutex<HashMap<(i64, String), String>> = Mutex::new(HashMap::new());
}

fn unlock_session(profile_id: i64, kind: &str, password: &str) {
    if let Ok(mut m) = SESSIONS.lock() {
        m.insert((profile_id, kind.to_string()), password.to_string());
    }
}
fn session_key(profile_id: i64, kind: &str) -> Option<String> {
    SESSIONS.lock().ok().and_then(|m| m.get(&(profile_id, kind.to_string())).cloned())
}
fn lock_session(profile_id: i64, kind: &str) {
    if let Ok(mut m) = SESSIONS.lock() {
        m.remove(&(profile_id, kind.to_string()));
    }
}
/// Lock every unlocked vault for every profile (clears all cached master keys).
/// Used on profile switch so the new profile starts fully gated.
fn lock_all_sessions() {
    if let Ok(mut m) = SESSIONS.lock() {
        m.clear();
    }
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Whether two hosts should be considered the same site for autofill — ignoring a
/// leading `www.` and allowing one to be a subdomain of the other (so a login
/// saved on `example.com` matches `accounts.example.com`, and vice versa).
fn host_matches(a: &str, b: &str) -> bool {
    let strip = |h: &str| h.strip_prefix("www.").unwrap_or(h).to_string();
    let a = strip(a);
    let b = strip(b);
    a == b || b.ends_with(&format!(".{}", a)) || a.ends_with(&format!(".{}", b))
}

/// Extract a lowercase host from a URL or origin (`https://u:p@host:443/x` → `host`).
/// Returns None for empty/host-less input.
fn host_from(url: &str) -> Option<String> {
    let after_scheme = url.split("://").nth(1).unwrap_or(url);
    let authority = after_scheme.split(['/', '?', '#']).next()?;
    let host_port = authority.rsplit('@').next()?; // drop userinfo
    let host = host_port.split(':').next()?; // drop port
    let host = host.trim().to_lowercase();
    if host.is_empty() {
        None
    } else {
        Some(host)
    }
}

/// Legacy master-password hash (unsalted SHA-256). Retained ONLY to verify and
/// transparently upgrade vaults created before Argon2id. Never used for new hashes.
fn legacy_sha256(password: &str) -> String {
    let mut h = Sha256::new();
    h.update(password.as_bytes());
    format!("{:x}", h.finalize())
}

/// Hash a master password with Argon2id (memory-hard, random per-hash salt). The
/// returned PHC string embeds the salt + parameters, so it's self-describing for
/// verification. This gates vault UNLOCK; it is independent of the AES key the
/// entries are encrypted with (see the note on encryption-KDF hardening).
fn hash_password(password: &str) -> Result<String, String> {
    use argon2::password_hash::{rand_core::OsRng, PasswordHasher, SaltString};
    use argon2::Argon2;
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| e.to_string())
}

/// Verify `password` against a stored hash, accepting both the new Argon2id PHC
/// strings and legacy unsalted SHA-256 hex. Returns whether it matched and
/// whether the stored hash is legacy (so the caller can upgrade it in place).
fn verify_hash(password: &str, stored: &str) -> (bool, bool) {
    if stored.starts_with("$argon2") {
        use argon2::password_hash::{PasswordHash, PasswordVerifier};
        use argon2::Argon2;
        let ok = PasswordHash::new(stored)
            .map(|ph| Argon2::default().verify_password(password.as_bytes(), &ph).is_ok())
            .unwrap_or(false);
        (ok, false)
    } else {
        (stored == legacy_sha256(password), true)
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PasswordEntry {
    pub id: i64,
    pub profile_id: i64,
    pub title: String,
    pub username: String,
    pub password: String,
    pub url: Option<String>,
    pub notes: Option<String>,
    pub category: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct OtpEntry {
    pub id: i64,
    pub profile_id: i64,
    pub name: String,
    pub issuer: String,
    pub secret: String,
    pub algorithm: String,
    pub digits: i64,
    pub period: i64,
    pub created_at: String,
}

pub struct VaultManager {
    db_path: String,
}

impl VaultManager {
    pub fn new(db_path: String) -> Self {
        Self { db_path }
    }

    fn conn(&self) -> Result<Connection, String> {
        let conn = Connection::open(&self.db_path).map_err(|e| e.to_string())?;
        conn.busy_timeout(std::time::Duration::from_millis(2000)).ok();
        Ok(conn)
    }

    pub fn init(&self) -> Result<(), String> {
        let conn = self.conn()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS vault_master (
                profile_id INTEGER NOT NULL,
                kind       TEXT NOT NULL,
                hash       TEXT NOT NULL,
                PRIMARY KEY (profile_id, kind)
            );
            CREATE TABLE IF NOT EXISTS password_entries (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                profile_id INTEGER NOT NULL,
                title      TEXT NOT NULL,
                username   TEXT NOT NULL,
                password   TEXT NOT NULL,
                url        TEXT,
                notes      TEXT,
                category   TEXT NOT NULL DEFAULT 'General',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS otp_entries (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                profile_id INTEGER NOT NULL,
                name       TEXT NOT NULL,
                issuer     TEXT NOT NULL,
                secret     TEXT NOT NULL,
                algorithm  TEXT NOT NULL DEFAULT 'SHA1',
                digits     INTEGER NOT NULL DEFAULT 6,
                period     INTEGER NOT NULL DEFAULT 30,
                created_at TEXT NOT NULL
            );",
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    // ---- master password (access control + unlock) ----

    fn has_master(&self, profile_id: i64, kind: &str) -> Result<bool, String> {
        let conn = self.conn()?;
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM vault_master WHERE profile_id = ?1 AND kind = ?2",
                params![profile_id, kind],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        Ok(n > 0)
    }

    fn set_master(&self, profile_id: i64, kind: &str, password: &str) -> Result<(), String> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT OR REPLACE INTO vault_master (profile_id, kind, hash) VALUES (?1, ?2, ?3)",
            params![profile_id, kind, hash_password(password)?],
        )
        .map_err(|e| e.to_string())?;
        // Unlock for this session so the new entries can be encrypted.
        unlock_session(profile_id, kind, password);
        Ok(())
    }

    fn verify_master(&self, profile_id: i64, kind: &str, password: &str) -> Result<bool, String> {
        let conn = self.conn()?;
        let stored: Option<String> = conn
            .query_row(
                "SELECT hash FROM vault_master WHERE profile_id = ?1 AND kind = ?2",
                params![profile_id, kind],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        match stored {
            Some(h) => {
                let (ok, is_legacy) = verify_hash(password, &h);
                if ok {
                    unlock_session(profile_id, kind, password); // cache the key for this session
                    // Transparently upgrade legacy unsalted SHA-256 vaults to Argon2id
                    // on first successful unlock.
                    if is_legacy {
                        if let Ok(upgraded) = hash_password(password) {
                            let _ = conn.execute(
                                "UPDATE vault_master SET hash = ?1 WHERE profile_id = ?2 AND kind = ?3",
                                params![upgraded, profile_id, kind],
                            );
                        }
                    }
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            None => Ok(true), // no master set => nothing to unlock
        }
    }

    fn entry_profile(&self, table: &str, entry_id: i64) -> Result<Option<i64>, String> {
        let conn = self.conn()?;
        // `table` is a fixed internal constant, never user input.
        conn.query_row(
            &format!("SELECT profile_id FROM {} WHERE id = ?1", table),
            params![entry_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())
    }

    // ---- autofill / autosave (credential bridge) ----

    /// Find a saved login whose stored URL host matches `origin`'s host, returning
    /// (username, decrypted password). Returns None if the vault is locked (no
    /// session key → `get_password_entries` yields nothing) or there's no match.
    pub fn find_login_for_origin(&self, profile_id: i64, origin: &str) -> Option<(String, String)> {
        let host = host_from(origin)?;
        let entries = self.get_password_entries(profile_id).ok()?;
        entries.into_iter().find_map(|e| {
            let matches = e.url.as_deref().and_then(host_from).map(|h| host_matches(&h, &host)).unwrap_or(false);
            if matches && !e.username.is_empty() {
                Some((e.username, e.password))
            } else {
                None
            }
        })
    }

    /// Lightweight check for the autofill prompt: is there a saved login for this
    /// origin, and is the vault locked? Reads only the PLAINTEXT url/username
    /// columns, so it works even when locked (no decryption needed). Returns
    /// (username, locked).
    pub fn login_hint_for_origin(&self, profile_id: i64, origin: &str) -> Option<(String, bool)> {
        let host = host_from(origin)?;
        let conn = self.conn().ok()?;
        let mut stmt = conn
            .prepare("SELECT username, url FROM password_entries WHERE profile_id = ?1")
            .ok()?;
        let rows = stmt
            .query_map(params![profile_id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
            })
            .ok()?;
        for row in rows.flatten() {
            let (username, url) = row;
            if url.as_deref().and_then(host_from).map(|h| host_matches(&h, &host)).unwrap_or(false) {
                let locked = session_key(profile_id, KIND_PASSWORD).is_none();
                return Some((username, locked));
            }
        }
        None
    }

    /// Whether an entry already exists for this origin + username with the SAME
    /// password — i.e. nothing to save. Requires the vault unlocked to compare the
    /// (decrypted) password; returns false when locked (so we'd still offer to save).
    pub fn has_exact_login(
        &self,
        profile_id: i64,
        origin: &str,
        username: &str,
        password: &str,
    ) -> bool {
        let host = match host_from(origin) {
            Some(h) => h,
            None => return false,
        };
        let entries = match self.get_password_entries(profile_id) {
            Ok(e) => e,
            Err(_) => return false,
        };
        entries.iter().any(|e| {
            e.url.as_deref().and_then(host_from).map(|h| host_matches(&h, &host)).unwrap_or(false)
                && e.username == username
                && e.password == password
        })
    }

    /// Persist a captured login: update the existing entry for this host+username
    /// (or skip if unchanged), else insert a new "Autosaved" entry. Requires the
    /// vault to be unlocked (the new secret is encrypted with the session key).
    pub fn autosave_login(
        &self,
        profile_id: i64,
        origin: &str,
        username: &str,
        password: &str,
    ) -> Result<(), String> {
        if session_key(profile_id, KIND_PASSWORD).is_none() {
            return Err("Vault is locked. Unlock it to save passwords.".into());
        }
        let host = host_from(origin).unwrap_or_else(|| origin.to_string());
        let existing = self.get_password_entries(profile_id)?.into_iter().find(|e| {
            e.url.as_deref().and_then(host_from).as_deref() == Some(host.as_str())
                && e.username == username
        });
        match existing {
            Some(e) if e.password == password => Ok(()), // nothing changed
            Some(e) => self.update_password_entry(
                e.id,
                &e.title,
                username,
                password,
                e.url.as_deref(),
                e.notes.as_deref(),
                &e.category,
            ),
            None => self.add_password_entry(
                profile_id,
                &host,
                username,
                password,
                Some(origin),
                None,
                "Autosaved",
            ),
        }
    }

    // ---- password entries ----

    fn get_password_entries(&self, profile_id: i64) -> Result<Vec<PasswordEntry>, String> {
        // Locked => nothing readable.
        let key = match session_key(profile_id, KIND_PASSWORD) {
            Some(k) => k,
            None => return Ok(Vec::new()),
        };
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, profile_id, title, username, password, url, notes, category, created_at, updated_at
                 FROM password_entries WHERE profile_id = ?1 ORDER BY title COLLATE NOCASE",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![profile_id], |row| {
                let stored: String = row.get(4)?;
                let password = decrypt_data(&stored, &key).unwrap_or_default();
                Ok(PasswordEntry {
                    id: row.get(0)?,
                    profile_id: row.get(1)?,
                    title: row.get(2)?,
                    username: row.get(3)?,
                    password,
                    url: row.get(5)?,
                    notes: row.get(6)?,
                    category: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    #[allow(clippy::too_many_arguments)]
    fn add_password_entry(
        &self,
        profile_id: i64,
        title: &str,
        username: &str,
        password: &str,
        url: Option<&str>,
        notes: Option<&str>,
        category: &str,
    ) -> Result<(), String> {
        let key = session_key(profile_id, KIND_PASSWORD).ok_or("Vault is locked. Unlock it first.")?;
        let enc = encrypt_data(password, &key)?;
        let conn = self.conn()?;
        let ts = now();
        conn.execute(
            "INSERT INTO password_entries (profile_id, title, username, password, url, notes, category, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            params![profile_id, title, username, enc, url, notes, category, ts],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    // ---- Reclaim app passwords (non-site entries, master-gated, autofillable) ----
    // The password manager master acts as the one master password. App passwords
    // (media / bookmarks / authenticator / local-ai …) are stored as ordinary
    // password_entries keyed by a `reclaim://<key>` URL in the "Reclaim" category,
    // so the manager lists/edits them like any entry, but feature gates can look
    // one up by key and autofill it after the user enters the master once.

    pub fn has_app_password(&self, profile_id: i64, key: &str) -> Result<bool, String> {
        let conn = self.conn()?;
        let url = format!("reclaim://{}", key);
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM password_entries WHERE profile_id = ?1 AND url = ?2",
                params![profile_id, url],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        Ok(n > 0)
    }

    /// Decrypt and return a stored app password — after verifying the master
    /// (which also unlocks the session for decryption). None if no entry exists.
    pub fn get_app_password(&self, profile_id: i64, key: &str, master: &str) -> Result<Option<String>, String> {
        if !self.has_master(profile_id, KIND_PASSWORD)? {
            return Err("Set up your password manager master password first.".into());
        }
        if !self.verify_master(profile_id, KIND_PASSWORD, master)? {
            return Err("Incorrect master password".into());
        }
        let sk = session_key(profile_id, KIND_PASSWORD).ok_or("Vault is locked")?;
        let conn = self.conn()?;
        let url = format!("reclaim://{}", key);
        let stored: Option<String> = conn
            .query_row(
                "SELECT password FROM password_entries WHERE profile_id = ?1 AND url = ?2",
                params![profile_id, url],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        Ok(stored.map(|enc| decrypt_data(&enc, &sk).unwrap_or_default()))
    }

    /// Store (or replace) an app password — master-gated. One entry per key.
    pub fn set_app_password(&self, profile_id: i64, key: &str, label: &str, password: &str, master: &str) -> Result<(), String> {
        if !self.has_master(profile_id, KIND_PASSWORD)? {
            return Err("Set up your password manager master password first.".into());
        }
        if !self.verify_master(profile_id, KIND_PASSWORD, master)? {
            return Err("Incorrect master password".into());
        }
        let url = format!("reclaim://{}", key);
        {
            let conn = self.conn()?;
            conn.execute(
                "DELETE FROM password_entries WHERE profile_id = ?1 AND url = ?2",
                params![profile_id, url],
            )
            .map_err(|e| e.to_string())?;
        }
        // verify_master unlocked the session, so add_password_entry can encrypt.
        self.add_password_entry(profile_id, label, "", password, Some(&url), Some("Reclaim app password"), "Reclaim")
    }

    /// Change a vault master (password manager or authenticator/OTP), requiring
    /// the current password and RE-ENCRYPTING every entry of that kind under the
    /// new password — otherwise the stored secrets would become unreadable. The
    /// hash swap + re-encryption happen in one transaction (all-or-nothing).
    pub fn change_master(&self, profile_id: i64, kind: &str, old: &str, new: &str) -> Result<(), String> {
        if new.trim().is_empty() {
            return Err("New password cannot be empty".into());
        }
        if !self.has_master(profile_id, kind)? {
            return Err("No password is set yet".into());
        }
        if !self.verify_master(profile_id, kind, old)? {
            return Err("Incorrect current password".into());
        }
        let (table, col) = match kind {
            KIND_PASSWORD => ("password_entries", "password"),
            KIND_OTP => ("otp_entries", "secret"),
            _ => return Err("unknown vault".into()),
        };
        let conn = self.conn()?;
        // Decrypt each entry's secret with the OLD password (it IS the key).
        let mut decrypted: Vec<(i64, String)> = Vec::new();
        {
            let mut stmt = conn
                .prepare(&format!("SELECT id, {} FROM {} WHERE profile_id = ?1", col, table))
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![profile_id], |r| {
                    Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
                })
                .map_err(|e| e.to_string())?;
            for row in rows {
                let (id, enc) = row.map_err(|e| e.to_string())?;
                decrypted.push((id, decrypt_data(&enc, old).unwrap_or_default()));
            }
        }
        let new_hash = hash_password(new)?;
        let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE vault_master SET hash = ?1 WHERE profile_id = ?2 AND kind = ?3",
            params![new_hash, profile_id, kind],
        )
        .map_err(|e| e.to_string())?;
        for (id, plain) in &decrypted {
            let enc = encrypt_data(plain, new)?;
            tx.execute(
                &format!("UPDATE {} SET {} = ?1 WHERE id = ?2", table, col),
                params![enc, id],
            )
            .map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())?;
        unlock_session(profile_id, kind, new);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn update_password_entry(
        &self,
        entry_id: i64,
        title: &str,
        username: &str,
        password: &str,
        url: Option<&str>,
        notes: Option<&str>,
        category: &str,
    ) -> Result<(), String> {
        let profile_id = self.entry_profile("password_entries", entry_id)?.ok_or("Entry not found")?;
        let key = session_key(profile_id, KIND_PASSWORD).ok_or("Vault is locked. Unlock it first.")?;
        let enc = encrypt_data(password, &key)?;
        let conn = self.conn()?;
        conn.execute(
            "UPDATE password_entries SET title=?2, username=?3, password=?4, url=?5, notes=?6, category=?7, updated_at=?8 WHERE id=?1",
            params![entry_id, title, username, enc, url, notes, category, now()],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn delete_password_entry(&self, entry_id: i64) -> Result<(), String> {
        let conn = self.conn()?;
        conn.execute("DELETE FROM password_entries WHERE id = ?1", params![entry_id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    // ---- otp entries ----

    fn get_otp_entries(&self, profile_id: i64) -> Result<Vec<OtpEntry>, String> {
        let key = match session_key(profile_id, KIND_OTP) {
            Some(k) => k,
            None => return Ok(Vec::new()),
        };
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, profile_id, name, issuer, secret, algorithm, digits, period, created_at
                 FROM otp_entries WHERE profile_id = ?1 ORDER BY issuer COLLATE NOCASE, name COLLATE NOCASE",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![profile_id], |row| {
                let stored: String = row.get(4)?;
                let secret = decrypt_data(&stored, &key).unwrap_or_default();
                Ok(OtpEntry {
                    id: row.get(0)?,
                    profile_id: row.get(1)?,
                    name: row.get(2)?,
                    issuer: row.get(3)?,
                    secret,
                    algorithm: row.get(5)?,
                    digits: row.get(6)?,
                    period: row.get(7)?,
                    created_at: row.get(8)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    #[allow(clippy::too_many_arguments)]
    fn add_otp_entry(
        &self,
        profile_id: i64,
        name: &str,
        issuer: &str,
        secret: &str,
        algorithm: &str,
        digits: i64,
        period: i64,
    ) -> Result<(), String> {
        let key = session_key(profile_id, KIND_OTP).ok_or("Vault is locked. Unlock it first.")?;
        let enc = encrypt_data(secret, &key)?;
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO otp_entries (profile_id, name, issuer, secret, algorithm, digits, period, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![profile_id, name, issuer, enc, algorithm, digits, period, now()],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn update_otp_entry(
        &self,
        entry_id: i64,
        name: &str,
        issuer: &str,
        secret: &str,
        algorithm: &str,
        digits: i64,
        period: i64,
    ) -> Result<(), String> {
        let profile_id = self.entry_profile("otp_entries", entry_id)?.ok_or("Entry not found")?;
        let key = session_key(profile_id, KIND_OTP).ok_or("Vault is locked. Unlock it first.")?;
        let enc = encrypt_data(secret, &key)?;
        let conn = self.conn()?;
        conn.execute(
            "UPDATE otp_entries SET name=?2, issuer=?3, secret=?4, algorithm=?5, digits=?6, period=?7 WHERE id=?1",
            params![entry_id, name, issuer, enc, algorithm, digits, period],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn delete_otp_entry(&self, entry_id: i64) -> Result<(), String> {
        let conn = self.conn()?;
        conn.execute("DELETE FROM otp_entries WHERE id = ?1", params![entry_id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

// ============================================================================
// Tauri commands — all camelCase so the frontend uses { profileId, entryId, ... }
// ============================================================================

// ---- Password Manager ----

#[tauri::command(rename_all = "camelCase")]
pub async fn has_password_manager_master(state: State<'_, Mutex<AppState>>, profile_id: i64) -> Result<bool, String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    s.vault_manager.has_master(profile_id, KIND_PASSWORD)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn verify_password_manager_master(state: State<'_, Mutex<AppState>>, profile_id: i64, password: String) -> Result<bool, String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    s.vault_manager.verify_master(profile_id, KIND_PASSWORD, &password)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn set_password_manager_master(state: State<'_, Mutex<AppState>>, profile_id: i64, password: String) -> Result<(), String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    s.vault_manager.set_master(profile_id, KIND_PASSWORD, &password)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn lock_password_manager(profile_id: i64) -> Result<(), String> {
    lock_session(profile_id, KIND_PASSWORD);
    Ok(())
}

/// Lock ALL vaults for ALL profiles (password manager + authenticator). Called on
/// profile switch so passwords are re-gated.
#[tauri::command(rename_all = "camelCase")]
pub async fn lock_all_vaults() -> Result<(), String> {
    lock_all_sessions();
    Ok(())
}

// ---- Reclaim app passwords (master-gated autofill for feature gates) ----

#[tauri::command(rename_all = "camelCase")]
pub async fn vault_has_app_password(state: State<'_, Mutex<AppState>>, profile_id: i64, key: String) -> Result<bool, String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    s.vault_manager.has_app_password(profile_id, &key)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn vault_get_app_password(state: State<'_, Mutex<AppState>>, profile_id: i64, key: String, master: String) -> Result<Option<String>, String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    s.vault_manager.get_app_password(profile_id, &key, &master)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn vault_set_app_password(state: State<'_, Mutex<AppState>>, profile_id: i64, key: String, label: String, password: String, master: String) -> Result<(), String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    s.vault_manager.set_app_password(profile_id, &key, &label, &password, &master)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn change_password_manager_master(state: State<'_, Mutex<AppState>>, profile_id: i64, old_password: String, new_password: String) -> Result<(), String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    s.vault_manager.change_master(profile_id, KIND_PASSWORD, &old_password, &new_password)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn change_otp_master(state: State<'_, Mutex<AppState>>, profile_id: i64, old_password: String, new_password: String) -> Result<(), String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    s.vault_manager.change_master(profile_id, KIND_OTP, &old_password, &new_password)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn get_password_entries(state: State<'_, Mutex<AppState>>, profile_id: i64) -> Result<Vec<PasswordEntry>, String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    s.vault_manager.get_password_entries(profile_id)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn add_password_entry(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    title: String,
    username: String,
    password: String,
    url: Option<String>,
    notes: Option<String>,
    category: String,
) -> Result<(), String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    s.vault_manager.add_password_entry(profile_id, &title, &username, &password, url.as_deref(), notes.as_deref(), &category)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn update_password_entry(
    state: State<'_, Mutex<AppState>>,
    entry_id: i64,
    title: String,
    username: String,
    password: String,
    url: Option<String>,
    notes: Option<String>,
    category: String,
) -> Result<(), String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    s.vault_manager.update_password_entry(entry_id, &title, &username, &password, url.as_deref(), notes.as_deref(), &category)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn delete_password_entry(state: State<'_, Mutex<AppState>>, entry_id: i64) -> Result<(), String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    s.vault_manager.delete_password_entry(entry_id)
}

// ---- OTP Authenticator ----

#[tauri::command(rename_all = "camelCase")]
pub async fn has_otp_master(state: State<'_, Mutex<AppState>>, profile_id: i64) -> Result<bool, String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    s.vault_manager.has_master(profile_id, KIND_OTP)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn verify_otp_master(state: State<'_, Mutex<AppState>>, profile_id: i64, password: String) -> Result<bool, String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    s.vault_manager.verify_master(profile_id, KIND_OTP, &password)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn set_otp_master(state: State<'_, Mutex<AppState>>, profile_id: i64, password: String) -> Result<(), String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    s.vault_manager.set_master(profile_id, KIND_OTP, &password)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn lock_otp(profile_id: i64) -> Result<(), String> {
    lock_session(profile_id, KIND_OTP);
    Ok(())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn get_otp_entries(state: State<'_, Mutex<AppState>>, profile_id: i64) -> Result<Vec<OtpEntry>, String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    s.vault_manager.get_otp_entries(profile_id)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn add_otp_entry(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    name: String,
    issuer: String,
    secret: String,
    algorithm: String,
    digits: i64,
    period: i64,
) -> Result<(), String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    s.vault_manager.add_otp_entry(profile_id, &name, &issuer, &secret, &algorithm, digits, period)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn update_otp_entry(
    state: State<'_, Mutex<AppState>>,
    entry_id: i64,
    name: String,
    issuer: String,
    secret: String,
    algorithm: String,
    digits: i64,
    period: i64,
) -> Result<(), String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    s.vault_manager.update_otp_entry(entry_id, &name, &issuer, &secret, &algorithm, digits, period)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn delete_otp_entry(state: State<'_, Mutex<AppState>>, entry_id: i64) -> Result<(), String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    s.vault_manager.delete_otp_entry(entry_id)
}

// ---- autofill / autosave bridge commands ----

/// Look up a saved login for `origin` (used by autofill). Returns null if the
/// vault is locked or there's no match — so a locked vault simply never fills.
#[tauri::command(rename_all = "camelCase")]
pub async fn vault_find_login(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    origin: String,
) -> Result<Option<(String, String)>, String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    Ok(s.vault_manager.find_login_for_origin(profile_id, &origin))
}

/// Persist the pending captured login (after the user confirms the save prompt).
/// Is there a saved login for `origin`, and is the vault locked? Drives the
/// autofill prompt (works even when locked, since it doesn't decrypt).
#[tauri::command(rename_all = "camelCase")]
pub async fn vault_login_hint(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    origin: String,
) -> Result<Option<(String, bool)>, String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    let hint = s.vault_manager.login_hint_for_origin(profile_id, &origin);
    eprintln!("[autofill] login_hint profile={} origin={} -> match={}", profile_id, origin, hint.is_some());
    Ok(hint)
}

/// Fill the active page's login form with the saved credentials for `origin`.
/// The password is looked up + injected entirely in the backend (never reaches
/// the JS frontend). Returns false if locked / no match.
#[tauri::command(rename_all = "camelCase")]
pub async fn vault_autofill(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    origin: String,
) -> Result<bool, String> {
    let creds = {
        let s = state.lock().map_err(|e| e.to_string())?;
        s.vault_manager.find_login_for_origin(profile_id, &origin)
    };
    match creds {
        Some((u, p)) => {
            crate::browser_surface::fill_login(&u, &p);
            Ok(true)
        }
        None => Ok(false),
    }
}

/// Whether the pending captured login is worth prompting to save — i.e. it's new
/// or its password changed. False when it's already saved unchanged (e.g. the user
/// just autofilled + submitted), so we skip the redundant "Save password?" prompt.
#[tauri::command(rename_all = "camelCase")]
pub async fn vault_autosave_is_new(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
) -> Result<bool, String> {
    let pending = crate::browser_surface::imp::peek_pending_autosave();
    let (origin, username, password) = match pending {
        Some(p) => p,
        None => return Ok(false),
    };
    let s = state.lock().map_err(|e| e.to_string())?;
    Ok(!s.vault_manager.has_exact_login(profile_id, &origin, &username, &password))
}

#[tauri::command(rename_all = "camelCase")]
pub async fn vault_autosave_confirm(
    app: tauri::AppHandle,
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
) -> Result<(), String> {
    let (origin, username, password) =
        crate::browser_surface::imp::take_pending_autosave().ok_or("Nothing to save")?;
    {
        let s = state.lock().map_err(|e| e.to_string())?;
        s.vault_manager.autosave_login(profile_id, &origin, &username, &password)?;
    }
    // Tell any open Password Manager to refresh its list.
    use tauri::Emitter;
    let _ = app.emit("password-saved", ());
    Ok(())
}

/// Discard the pending captured login (user dismissed the save prompt).
#[tauri::command(rename_all = "camelCase")]
pub async fn vault_autosave_dismiss() -> Result<(), String> {
    let _ = crate::browser_surface::imp::take_pending_autosave();
    Ok(())
}
