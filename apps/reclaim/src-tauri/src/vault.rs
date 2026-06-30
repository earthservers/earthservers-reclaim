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
use zeroize::Zeroizing;

use crate::multimedia::{decrypt_data, encrypt_data};
use crate::security::secrets::{ct_eq, SecretString};
use crate::AppState;

const KIND_PASSWORD: &str = "password";
const KIND_OTP: &str = "otp";

lazy_static::lazy_static! {
    /// In-memory unlocked master passwords, keyed by (profile_id, kind). Holds
    /// the plaintext master password ONLY in RAM for the duration the vault is
    /// unlocked; used to derive the encryption key. Never persisted. Each value is
    /// a [`SecretString`]: zeroized on drop and (on Linux) mlock'd + DONTDUMP, so
    /// the cached master stays out of swap and core dumps. [HYGIENE]
    static ref SESSIONS: Mutex<HashMap<(i64, String), SecretString>> = Mutex::new(HashMap::new());
}

fn unlock_session(profile_id: i64, kind: &str, password: &str) {
    if let Ok(mut m) = SESSIONS.lock() {
        m.insert((profile_id, kind.to_string()), SecretString::new(password));
    }
}
/// The cached master key for a session, copied into a zeroizing buffer for the
/// caller's transient use (wiped when the returned value drops).
fn session_key(profile_id: i64, kind: &str) -> Option<Zeroizing<String>> {
    SESSIONS
        .lock()
        .ok()
        .and_then(|m| m.get(&(profile_id, kind.to_string())).map(|s| Zeroizing::new(s.as_str().to_string())))
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
        // Constant-time compare so a legacy hash check leaks no timing oracle.
        (ct_eq(stored.as_bytes(), legacy_sha256(password).as_bytes()), true)
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

/// Metadata-only view of a saved login — everything the manager list needs to
/// render EXCEPT the secret. This is the redact-by-default boundary: the plaintext
/// password is never serialized into a list, so a compromised frontend (backdoored
/// npm dep / self-XSS in our own chrome) that calls the list command cannot
/// mass-exfiltrate the vault. To see one password the UI must call `vault_reveal`,
/// which is gated, rate-limited and audited — one entry at a time, leaving a trail.
/// Honest scope: this is least-privilege + blast-radius containment against a
/// compromised *trusted* frontend; it is NOT a defense against a malicious web
/// page (that is the process boundary, which already blocks pages entirely).
#[derive(Debug, Clone, serde::Serialize)]
pub struct PasswordEntryMeta {
    pub id: i64,
    pub profile_id: i64,
    pub title: String,
    pub username: String,
    pub url: Option<String>,
    pub notes: Option<String>,
    pub category: String,
    pub created_at: String,
    pub updated_at: String,
    /// Whether a non-empty password is stored (lets the UI show a reveal affordance
    /// without exposing the value).
    pub has_password: bool,
}

/// Metadata-only view of an OTP entry — no secret. Codes are generated in the
/// backend (`vault_otp_codes`) so the TOTP seed never reaches page-readable JS.
#[derive(Debug, Clone, serde::Serialize)]
pub struct OtpEntryMeta {
    pub id: i64,
    pub profile_id: i64,
    pub name: String,
    pub issuer: String,
    pub algorithm: String,
    pub digits: i64,
    pub period: i64,
    pub created_at: String,
}

/// A current TOTP code for one entry — computed in the backend, secret never sent.
#[derive(Debug, Clone, serde::Serialize)]
pub struct OtpCode {
    pub id: i64,
    /// The current code, zero-padded to `digits`. Empty if generation failed.
    pub code: String,
    /// Seconds until this code rolls over (for the countdown ring).
    pub remaining: i64,
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
                && ct_eq(e.password.as_bytes(), password.as_bytes())
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

    /// Metadata-only listing (no decryption, no secrets in the result). Still gated
    /// on an unlocked session so a locked vault reveals nothing — not even which
    /// entries exist. This is what the manager UI's list now consumes.
    fn get_password_entries_meta(&self, profile_id: i64) -> Result<Vec<PasswordEntryMeta>, String> {
        if session_key(profile_id, KIND_PASSWORD).is_none() {
            return Ok(Vec::new());
        }
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, profile_id, title, username, url, notes, category, created_at, updated_at, length(password)
                 FROM password_entries WHERE profile_id = ?1 ORDER BY title COLLATE NOCASE",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![profile_id], |row| {
                let pwlen: i64 = row.get(9)?;
                Ok(PasswordEntryMeta {
                    id: row.get(0)?,
                    profile_id: row.get(1)?,
                    title: row.get(2)?,
                    username: row.get(3)?,
                    url: row.get(4)?,
                    notes: row.get(5)?,
                    category: row.get(6)?,
                    created_at: row.get(7)?,
                    updated_at: row.get(8)?,
                    has_password: pwlen > 0,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    /// Reveal ONE entry's plaintext password (manager show/copy). Requires the
    /// vault unlocked and the entry to belong to `profile_id` (never reveal another
    /// profile's secret). The audit/rate-limit gate is applied by the command
    /// wrapper, not here, so this method stays a pure data accessor.
    fn reveal_password(&self, profile_id: i64, entry_id: i64) -> Result<String, String> {
        let key = session_key(profile_id, KIND_PASSWORD).ok_or("Vault is locked. Unlock it first.")?;
        let owner = self.entry_profile("password_entries", entry_id)?.ok_or("Entry not found")?;
        if owner != profile_id {
            return Err("Entry does not belong to this profile".into());
        }
        let conn = self.conn()?;
        let stored: String = conn
            .query_row("SELECT password FROM password_entries WHERE id = ?1", params![entry_id], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        Ok(decrypt_data(&stored, &key).unwrap_or_default())
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

    /// Metadata-only OTP listing (no secret). Gated on an unlocked session.
    fn get_otp_entries_meta(&self, profile_id: i64) -> Result<Vec<OtpEntryMeta>, String> {
        Ok(self
            .get_otp_entries(profile_id)?
            .into_iter()
            .map(|e| OtpEntryMeta {
                id: e.id,
                profile_id: e.profile_id,
                name: e.name,
                issuer: e.issuer,
                algorithm: e.algorithm,
                digits: e.digits,
                period: e.period,
                created_at: e.created_at,
            })
            .collect())
    }

    /// Generate the CURRENT TOTP code for every entry, in the backend, so the
    /// base32 seed never crosses into page-readable JS. Returns empty when locked.
    fn otp_codes(&self, profile_id: i64) -> Result<Vec<OtpCode>, String> {
        let entries = self.get_otp_entries(profile_id)?; // empty if locked
        let unix = chrono::Utc::now().timestamp();
        Ok(entries
            .into_iter()
            .map(|e| {
                let period = if e.period > 0 { e.period } else { 30 };
                let code = totp::generate(&e.secret, &e.algorithm, e.digits.clamp(6, 10) as u32, period as u64, unix as u64)
                    .unwrap_or_default();
                OtpCode { id: e.id, code, remaining: period - (unix % period) }
            })
            .collect())
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
        let _ = session_key(profile_id, KIND_OTP).ok_or("Vault is locked. Unlock it first.")?;
        let conn = self.conn()?;
        // Redact-by-default: the seed is not returned to the UI, so an edit of the
        // other fields arrives with an EMPTY secret meaning "keep the existing one".
        // Only re-encrypt + overwrite the seed when a new one is actually supplied.
        if secret.is_empty() {
            conn.execute(
                "UPDATE otp_entries SET name=?2, issuer=?3, algorithm=?4, digits=?5, period=?6 WHERE id=?1",
                params![entry_id, name, issuer, algorithm, digits, period],
            )
            .map_err(|e| e.to_string())?;
        } else {
            let key = session_key(profile_id, KIND_OTP).ok_or("Vault is locked. Unlock it first.")?;
            let enc = encrypt_data(secret, &key)?;
            conn.execute(
                "UPDATE otp_entries SET name=?2, issuer=?3, secret=?4, algorithm=?5, digits=?6, period=?7 WHERE id=?1",
                params![entry_id, name, issuer, enc, algorithm, digits, period],
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn delete_otp_entry(&self, entry_id: i64) -> Result<(), String> {
        let conn = self.conn()?;
        conn.execute("DELETE FROM otp_entries WHERE id = ?1", params![entry_id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

/// RFC 6238 TOTP, computed in the backend so OTP seeds never reach the UI. This
/// is a STANDARD construction over the vetted `hmac` + `sha1`/`sha2` crates — not
/// home-rolled crypto and not a new primitive; only the (well-specified) HOTP/TOTP
/// assembly lives here.
mod totp {
    use hmac::{Mac, SimpleHmac};

    /// RFC 4648 base32 decode (TOTP seeds are base32, no padding required).
    fn base32_decode(s: &str) -> Option<Vec<u8>> {
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
        let mut buffer: u32 = 0;
        let mut bits: u32 = 0;
        let mut out = Vec::new();
        for c in s.chars() {
            if c == '=' || c.is_whitespace() || c == '-' {
                continue;
            }
            let up = c.to_ascii_uppercase() as u8;
            let val = ALPHABET.iter().position(|&a| a == up)? as u32;
            buffer = (buffer << 5) | val;
            bits += 5;
            if bits >= 8 {
                bits -= 8;
                out.push((buffer >> bits) as u8);
            }
        }
        Some(out)
    }

    fn truncate(hs: &[u8], digits: u32) -> String {
        let offset = (hs[hs.len() - 1] & 0x0f) as usize;
        let bin = ((hs[offset] as u32 & 0x7f) << 24)
            | ((hs[offset + 1] as u32) << 16)
            | ((hs[offset + 2] as u32) << 8)
            | (hs[offset + 3] as u32);
        let modulo = 10u32.pow(digits);
        format!("{:0width$}", bin % modulo, width = digits as usize)
    }

    /// Current TOTP code for `secret` (base32) at unix time `now`. Supports the
    /// three RFC-permitted HMAC algorithms; defaults to SHA1.
    pub fn generate(secret_b32: &str, algorithm: &str, digits: u32, period: u64, now: u64) -> Option<String> {
        let key = base32_decode(secret_b32)?;
        if key.is_empty() {
            return None;
        }
        let counter = (now / period.max(1)).to_be_bytes();
        let hs = match algorithm.to_ascii_uppercase().as_str() {
            "SHA256" => {
                let mut m = SimpleHmac::<sha2::Sha256>::new_from_slice(&key).ok()?;
                m.update(&counter);
                m.finalize().into_bytes().to_vec()
            }
            "SHA512" => {
                let mut m = SimpleHmac::<sha2::Sha512>::new_from_slice(&key).ok()?;
                m.update(&counter);
                m.finalize().into_bytes().to_vec()
            }
            _ => {
                let mut m = SimpleHmac::<sha1::Sha1>::new_from_slice(&key).ok()?;
                m.update(&counter);
                m.finalize().into_bytes().to_vec()
            }
        };
        Some(truncate(&hs, digits))
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
    use crate::security::audit;
    let field = format!("app:{key}");
    audit::gate(Some(profile_id), audit::Action::Reveal, None, Some(&field))?;
    let s = state.lock().map_err(|e| e.to_string())?;
    match s.vault_manager.get_app_password(profile_id, &key, &master) {
        Ok(v) => {
            audit::allow(Some(profile_id), audit::Action::Reveal, None, Some(&field), "app password revealed (master verified)");
            Ok(v)
        }
        Err(e) => {
            audit::deny(Some(profile_id), audit::Action::Reveal, None, Some(&field), &e);
            Err(e)
        }
    }
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

/// List saved logins as METADATA ONLY (no plaintext passwords). Redact-by-default:
/// the manager renders the list from this, and calls `vault_reveal` for a single
/// password only when the user clicks show/copy. See [`PasswordEntryMeta`].
#[tauri::command(rename_all = "camelCase")]
pub async fn get_password_entries(state: State<'_, Mutex<AppState>>, profile_id: i64) -> Result<Vec<PasswordEntryMeta>, String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    s.vault_manager.get_password_entries_meta(profile_id)
}

/// Reveal ONE entry's plaintext password (manager show/copy). Gated: requires an
/// unlocked vault, rate-limited per profile/entry, and AUDITED — every reveal
/// leaves an append-only record the Security panel surfaces. This is the only path
/// by which a stored site password leaves the backend to the chrome, and it is
/// deliberately one-entry-at-a-time so a compromised frontend cannot mass-dump.
#[tauri::command(rename_all = "camelCase")]
pub async fn vault_reveal(state: State<'_, Mutex<AppState>>, profile_id: i64, entry_id: i64) -> Result<String, String> {
    use crate::security::audit;
    let field = format!("entry:{entry_id}");
    // Rate-limit BEFORE doing the work; fail closed on limit.
    audit::gate(Some(profile_id), audit::Action::Reveal, None, Some(&field))?;
    let s = state.lock().map_err(|e| e.to_string())?;
    match s.vault_manager.reveal_password(profile_id, entry_id) {
        Ok(pw) => {
            audit::allow(Some(profile_id), audit::Action::Reveal, None, Some(&field), "password revealed to manager UI");
            Ok(pw)
        }
        Err(e) => {
            audit::deny(Some(profile_id), audit::Action::Reveal, None, Some(&field), &e);
            Err(e)
        }
    }
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

/// List OTP entries as METADATA ONLY (no seeds). Codes come from `vault_otp_codes`.
#[tauri::command(rename_all = "camelCase")]
pub async fn get_otp_entries(state: State<'_, Mutex<AppState>>, profile_id: i64) -> Result<Vec<OtpEntryMeta>, String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    s.vault_manager.get_otp_entries_meta(profile_id)
}

/// Current TOTP codes for every entry, generated in the backend so the base32
/// seed never reaches page-readable JS. The frontend polls this on the countdown.
#[tauri::command(rename_all = "camelCase")]
pub async fn vault_otp_codes(state: State<'_, Mutex<AppState>>, profile_id: i64) -> Result<Vec<OtpCode>, String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    s.vault_manager.otp_codes(profile_id)
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

// NOTE: the former `vault_find_login` command (which returned a decrypted
// (username, password) tuple to the frontend) has been REMOVED. Fills now go
// exclusively through `vault_autofill`, which injects directly into the page and
// never returns the secret to JS — removing both a redundant plaintext exit and
// the cross-origin-fill risk of a caller-supplied origin.

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

/// Fill the active page's login form with the saved credentials for the page that
/// is ACTUALLY on screen. The origin is taken from the live webview the backend
/// controls (`browser_surface::active_page_url`), NOT from the caller — so a
/// compromised/forging caller cannot pull `bank.com`'s credential while a
/// different site is loaded. The password is looked up + injected entirely in the
/// backend (never reaches the JS frontend). Every attempt is audited and
/// rate-limited per origin. Returns false if locked / no match.
///
/// The `_origin_hint` parameter is accepted for frontend compatibility but is
/// treated as advisory only and never used for the security decision.
#[tauri::command(rename_all = "camelCase")]
pub async fn vault_autofill(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    #[allow(unused_variables)] origin_hint: Option<String>,
) -> Result<bool, String> {
    use crate::security::audit;
    // Authoritative origin = the page actually loaded in the active webview.
    let origin = match crate::browser_surface::active_page_url() {
        Some(u) => u,
        None => {
            audit::deny(Some(profile_id), audit::Action::Autofill, None, Some("autofill"),
                "no active page origin available; refusing to fill");
            return Ok(false);
        }
    };
    // Rate-limit per origin (fail closed on limit).
    audit::gate(Some(profile_id), audit::Action::Autofill, Some(&origin), Some("autofill"))?;
    let creds = {
        let s = state.lock().map_err(|e| e.to_string())?;
        s.vault_manager.find_login_for_origin(profile_id, &origin)
    };
    match creds {
        Some((u, p)) => {
            crate::browser_surface::fill_login(&u, &p);
            audit::allow(Some(profile_id), audit::Action::Autofill, Some(&origin), Some("autofill"),
                "credential injected into active page");
            Ok(true)
        }
        None => {
            audit::deny(Some(profile_id), audit::Action::Autofill, Some(&origin), Some("autofill"),
                "no saved login matched the active page origin");
            Ok(false)
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    // RFC 6238 Appendix B official test vectors. The SHA1 seed is ASCII
    // "12345678901234567890" → base32 "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ".
    // Confirms our backend HOTP/TOTP assembly matches the standard (so it
    // interops with Google/Microsoft Authenticator after we moved it off the UI).
    #[test]
    fn totp_rfc6238_sha1_vectors() {
        let seed = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
        assert_eq!(totp::generate(seed, "SHA1", 8, 30, 59).unwrap(), "94287082");
        assert_eq!(totp::generate(seed, "SHA1", 8, 30, 1111111109).unwrap(), "07081804");
        assert_eq!(totp::generate(seed, "SHA1", 8, 30, 1234567890).unwrap(), "89005924");
    }

    #[test]
    fn totp_rejects_empty_and_bad_secret() {
        assert!(totp::generate("", "SHA1", 6, 30, 59).is_none());
        // Non-base32 chars are skipped; an all-invalid seed yields no key.
        assert!(totp::generate("!!!!", "SHA1", 6, 30, 59).is_none());
    }

    // host_matches is the origin-equality used to bind a fill to the page on
    // screen. These pin the same-site / subdomain / cross-site behavior so a
    // regression can't silently widen what an origin may autofill.
    #[test]
    fn host_matches_same_site_and_subdomains() {
        assert!(host_matches("example.com", "example.com"));
        assert!(host_matches("example.com", "www.example.com"));
        assert!(host_matches("example.com", "accounts.example.com"));
        assert!(host_matches("accounts.example.com", "example.com"));
    }

    #[test]
    fn host_matches_rejects_cross_site() {
        assert!(!host_matches("example.com", "evil.com"));
        assert!(!host_matches("example.com", "notexample.com"));
        assert!(!host_matches("bank.com", "bank.com.evil.com"));
    }

    #[test]
    fn host_from_extracts_host() {
        assert_eq!(host_from("https://user:pass@Example.com:443/path?x#y").as_deref(), Some("example.com"));
        assert_eq!(host_from("http://sub.example.com").as_deref(), Some("sub.example.com"));
        assert_eq!(host_from("").as_deref(), None);
    }

    use std::sync::atomic::{AtomicU32, Ordering as AtOrd};
    static TEST_SEQ: AtomicU32 = AtomicU32::new(0);

    fn temp_vault() -> (VaultManager, std::path::PathBuf) {
        let n = TEST_SEQ.fetch_add(1, AtOrd::SeqCst);
        let path = std::env::temp_dir().join(format!("reclaim_vault_test_{}_{}.db", std::process::id(), n));
        let _ = std::fs::remove_file(&path);
        let vm = VaultManager::new(path.to_string_lossy().to_string());
        vm.init().unwrap();
        (vm, path)
    }

    // The core Phase 1 guarantee at the data layer: a saved login is bound to its
    // origin's host, so a different origin can never retrieve it, and the vault
    // cannot be enumerated cross-origin. (The bridge layer additionally derives the
    // origin from the real webview, tested by host_matches above.)
    #[test]
    fn autofill_is_origin_bound_and_not_cross_origin() {
        let pid = 4242;
        let (vm, path) = temp_vault();
        vm.set_master(pid, KIND_PASSWORD, "master-pw").unwrap();
        vm.add_password_entry(pid, "Bank", "alice", "s3cret", Some("https://bank.com/login"), None, "General").unwrap();

        // Same site → found.
        assert_eq!(
            vm.find_login_for_origin(pid, "https://bank.com"),
            Some(("alice".to_string(), "s3cret".to_string()))
        );
        // Cross-origin → nothing, by construction (not merely denied).
        assert_eq!(vm.find_login_for_origin(pid, "https://evil.com"), None);
        // A look-alike suffix must not match.
        assert_eq!(vm.find_login_for_origin(pid, "https://bank.com.evil.com"), None);

        let _ = std::fs::remove_file(path);
    }

    // Redact-by-default: the list view carries NO plaintext (the struct has no
    // password field); the secret is only obtainable via the gated reveal path.
    #[test]
    fn list_is_metadata_only_and_reveal_returns_secret() {
        let pid = 4343;
        let (vm, path) = temp_vault();
        vm.set_master(pid, KIND_PASSWORD, "master-pw").unwrap();
        vm.add_password_entry(pid, "Site", "bob", "pw123", Some("https://site.com"), None, "General").unwrap();

        let meta = vm.get_password_entries_meta(pid).unwrap();
        assert_eq!(meta.len(), 1);
        assert!(meta[0].has_password);
        let id = meta[0].id;
        // The one gated reveal returns the real secret...
        assert_eq!(vm.reveal_password(pid, id).unwrap(), "pw123");
        // ...but never for another profile's view of that id.
        assert!(vm.reveal_password(9999, id).is_err());

        let _ = std::fs::remove_file(path);
    }

    // A locked vault yields nothing — not even metadata (no "which entries exist"
    // leak), and reveal fails closed.
    #[test]
    fn locked_vault_reveals_nothing() {
        let pid = 4444;
        let (vm, path) = temp_vault();
        vm.set_master(pid, KIND_PASSWORD, "master-pw").unwrap();
        vm.add_password_entry(pid, "Site", "bob", "pw123", Some("https://site.com"), None, "General").unwrap();
        let id = vm.get_password_entries_meta(pid).unwrap()[0].id;

        lock_session(pid, KIND_PASSWORD);
        assert!(vm.get_password_entries_meta(pid).unwrap().is_empty());
        assert!(vm.reveal_password(pid, id).is_err());
        assert_eq!(vm.find_login_for_origin(pid, "https://site.com"), None);

        let _ = std::fs::remove_file(path);
    }
}
