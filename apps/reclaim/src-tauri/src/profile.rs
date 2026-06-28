// Profile management for EarthServers Local
// Handles multiple user profiles with isolated data

use rusqlite::{Connection, Result, params};
use serde::{Deserialize, Serialize};

// ==================== Delete-code (PIN) helpers ====================
// Deleting a profile is irreversible and wipes ALL of its data, so it's gated by
// a 4-digit "delete code" set when the profile is created (or added later for
// legacy profiles). The code is stored as an Argon2id hash — never in plaintext —
// the same scheme used by the vault / ai-lock / bookmark password gates.

/// Validate that a delete code is exactly 4 ASCII digits.
fn is_valid_pin(pin: &str) -> bool {
    pin.len() == 4 && pin.bytes().all(|b| b.is_ascii_digit())
}

fn hash_pin(pin: &str) -> std::result::Result<String, String> {
    use argon2::password_hash::{rand_core::OsRng, PasswordHasher, SaltString};
    use argon2::Argon2;
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(pin.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| e.to_string())
}

fn verify_pin_hash(pin: &str, stored: &str) -> bool {
    use argon2::password_hash::{PasswordHash, PasswordVerifier};
    use argon2::Argon2;
    PasswordHash::new(stored)
        .map(|ph| Argon2::default().verify_password(pin.as_bytes(), &ph).is_ok())
        .unwrap_or(false)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: Option<i64>,
    pub name: String,
    pub icon: Option<String>,
    pub created_at: String,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacySettings {
    pub profile_id: i64,
    pub auto_delete_days: Option<i32>,
    pub ai_enabled_in_incognito: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileWithSettings {
    pub profile: Profile,
    pub privacy: PrivacySettings,
}

pub struct ProfileManager {
    db_path: String,
}

impl ProfileManager {
    pub fn new(db_path: String) -> Self {
        ProfileManager { db_path }
    }

    /// Initialize profile tables in the database
    pub fn init(&self) -> Result<()> {
        let conn = Connection::open(&self.db_path)?;

        // Create profiles table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS profiles (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                icon TEXT,
                created_at TEXT NOT NULL,
                is_active INTEGER NOT NULL DEFAULT 0
            )",
            [],
        )?;

        // Create privacy settings table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS privacy_settings (
                profile_id INTEGER PRIMARY KEY,
                auto_delete_days INTEGER,
                ai_enabled_in_incognito INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY (profile_id) REFERENCES profiles(id) ON DELETE CASCADE
            )",
            [],
        )?;

        // Migration: add the delete-code column to existing databases. Ignore the
        // "duplicate column name" error when it already exists (no IF NOT EXISTS
        // for ADD COLUMN in SQLite).
        let _ = conn.execute("ALTER TABLE profiles ADD COLUMN delete_pin TEXT", []);

        // Create default profile if none exists
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM profiles",
            [],
            |row| row.get(0),
        )?;

        if count == 0 {
            self.create_default_profile(&conn)?;
        }

        // Ensure the always-available, always-private "Incognito" profile exists.
        // It's protected (wipe-only) and forced into incognito mode (see privacy.rs).
        self.ensure_incognito_profile(&conn)?;

        Ok(())
    }

    fn create_default_profile(&self, conn: &Connection) -> Result<i64> {
        let now = chrono_now();

        conn.execute(
            "INSERT INTO profiles (name, icon, created_at, is_active) VALUES (?1, ?2, ?3, 1)",
            params!["Default", "user", now],
        )?;

        let profile_id = conn.last_insert_rowid();

        // Create default privacy settings
        conn.execute(
            "INSERT INTO privacy_settings (profile_id, auto_delete_days, ai_enabled_in_incognito) VALUES (?1, NULL, 0)",
            params![profile_id],
        )?;

        Ok(profile_id)
    }

    /// Create the dedicated "Incognito" profile if it doesn't already exist. Like
    /// the Default profile it's seeded directly (no delete code up front — it's
    /// protected from deletion anyway, and a code is requested before it can be
    /// wiped). It is never the active profile on creation.
    fn ensure_incognito_profile(&self, conn: &Connection) -> Result<()> {
        let exists: i64 = conn.query_row(
            "SELECT COUNT(*) FROM profiles WHERE LOWER(TRIM(name)) = 'incognito'",
            [],
            |row| row.get(0),
        )?;
        if exists > 0 {
            return Ok(());
        }
        let now = chrono_now();
        conn.execute(
            "INSERT INTO profiles (name, icon, created_at, is_active) VALUES (?1, ?2, ?3, 0)",
            params!["Incognito", "shield", now],
        )?;
        let profile_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO privacy_settings (profile_id, auto_delete_days, ai_enabled_in_incognito) VALUES (?1, NULL, 0)",
            params![profile_id],
        )?;
        Ok(())
    }

    /// Create a new profile. A 4-digit `delete_pin` is required up front — it's
    /// the code that will later be needed to delete (and fully wipe) the profile.
    pub fn create_profile(&self, name: &str, icon: Option<&str>, delete_pin: &str) -> Result<Profile> {
        if !is_valid_pin(delete_pin) {
            // Surface a clear, user-facing reason rather than a SQLite error.
            return Err(rusqlite::Error::InvalidParameterName(
                "Delete code must be exactly 4 digits".to_string(),
            ));
        }
        let pin_hash = hash_pin(delete_pin)
            .map_err(|e| rusqlite::Error::InvalidParameterName(format!("Failed to secure delete code: {}", e)))?;

        let conn = Connection::open(&self.db_path)?;
        let now = chrono_now();

        conn.execute(
            "INSERT INTO profiles (name, icon, created_at, is_active, delete_pin) VALUES (?1, ?2, ?3, 0, ?4)",
            params![name, icon, now, pin_hash],
        )?;

        let profile_id = conn.last_insert_rowid();

        // Create default privacy settings for new profile
        conn.execute(
            "INSERT INTO privacy_settings (profile_id, auto_delete_days, ai_enabled_in_incognito) VALUES (?1, NULL, 0)",
            params![profile_id],
        )?;

        Ok(Profile {
            id: Some(profile_id),
            name: name.to_string(),
            icon: icon.map(String::from),
            created_at: now,
            is_active: false,
        })
    }

    /// Get all profiles
    pub fn get_profiles(&self) -> Result<Vec<Profile>> {
        let conn = Connection::open(&self.db_path)?;
        let mut stmt = conn.prepare(
            "SELECT id, name, icon, created_at, is_active FROM profiles ORDER BY created_at ASC"
        )?;

        let profiles = stmt.query_map([], |row| {
            Ok(Profile {
                id: Some(row.get(0)?),
                name: row.get(1)?,
                icon: row.get(2)?,
                created_at: row.get(3)?,
                is_active: row.get::<_, i64>(4)? == 1,
            })
        })?;

        profiles.collect()
    }

    /// Get the active profile
    pub fn get_active_profile(&self) -> Result<Option<Profile>> {
        let conn = Connection::open(&self.db_path)?;
        let mut stmt = conn.prepare(
            "SELECT id, name, icon, created_at, is_active FROM profiles WHERE is_active = 1"
        )?;

        let mut profiles = stmt.query_map([], |row| {
            Ok(Profile {
                id: Some(row.get(0)?),
                name: row.get(1)?,
                icon: row.get(2)?,
                created_at: row.get(3)?,
                is_active: true,
            })
        })?;

        match profiles.next() {
            Some(Ok(profile)) => Ok(Some(profile)),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }

    /// Switch to a different profile
    pub fn switch_profile(&self, profile_id: i64) -> Result<Profile> {
        let conn = Connection::open(&self.db_path)?;

        // Deactivate all profiles
        conn.execute("UPDATE profiles SET is_active = 0", [])?;

        // Activate the selected profile
        conn.execute(
            "UPDATE profiles SET is_active = 1 WHERE id = ?1",
            params![profile_id],
        )?;

        // Return the now-active profile
        let mut stmt = conn.prepare(
            "SELECT id, name, icon, created_at, is_active FROM profiles WHERE id = ?1"
        )?;

        stmt.query_row(params![profile_id], |row| {
            Ok(Profile {
                id: Some(row.get(0)?),
                name: row.get(1)?,
                icon: row.get(2)?,
                created_at: row.get(3)?,
                is_active: true,
            })
        })
    }

    /// Update profile details
    pub fn update_profile(&self, profile_id: i64, name: &str, icon: Option<&str>) -> Result<Profile> {
        let conn = Connection::open(&self.db_path)?;

        conn.execute(
            "UPDATE profiles SET name = ?1, icon = ?2 WHERE id = ?3",
            params![name, icon, profile_id],
        )?;

        let mut stmt = conn.prepare(
            "SELECT id, name, icon, created_at, is_active FROM profiles WHERE id = ?1"
        )?;

        stmt.query_row(params![profile_id], |row| {
            Ok(Profile {
                id: Some(row.get(0)?),
                name: row.get(1)?,
                icon: row.get(2)?,
                created_at: row.get(3)?,
                is_active: row.get::<_, i64>(4)? == 1,
            })
        })
    }

    // ==================== Delete code (PIN) ====================

    /// Whether a delete code has been set for this profile. Profiles created via
    /// `create_profile` always have one; the auto-created "Default" profile (and
    /// any legacy profile) starts without one and must have one set before it can
    /// be deleted.
    pub fn has_delete_pin(&self, profile_id: i64) -> Result<bool> {
        let conn = Connection::open(&self.db_path)?;
        let stored: Option<String> = conn.query_row(
            "SELECT delete_pin FROM profiles WHERE id = ?1",
            params![profile_id],
            |row| row.get(0),
        )?;
        Ok(stored.map(|s| !s.is_empty()).unwrap_or(false))
    }

    /// Set (or change) a profile's 4-digit delete code.
    pub fn set_delete_pin(&self, profile_id: i64, pin: &str) -> Result<()> {
        if !is_valid_pin(pin) {
            return Err(rusqlite::Error::InvalidParameterName(
                "Delete code must be exactly 4 digits".to_string(),
            ));
        }
        let pin_hash = hash_pin(pin)
            .map_err(|e| rusqlite::Error::InvalidParameterName(format!("Failed to secure delete code: {}", e)))?;
        let conn = Connection::open(&self.db_path)?;
        conn.execute(
            "UPDATE profiles SET delete_pin = ?1 WHERE id = ?2",
            params![pin_hash, profile_id],
        )?;
        Ok(())
    }

    /// Verify a candidate delete code against the stored hash.
    pub fn verify_delete_pin(&self, profile_id: i64, pin: &str) -> Result<bool> {
        let conn = Connection::open(&self.db_path)?;
        let stored: Option<String> = conn.query_row(
            "SELECT delete_pin FROM profiles WHERE id = ?1",
            params![profile_id],
            |row| row.get(0),
        )?;
        Ok(match stored {
            Some(h) if !h.is_empty() => verify_pin_hash(pin, &h),
            _ => false, // no code set => cannot verify (caller must set one first)
        })
    }

    /// Whether a profile is "protected" — the built-in Default and Incognito
    /// profiles. These can never be deleted (you'd lose the only home base / the
    /// always-available private profile); they can only be WIPED, which clears
    /// their data but keeps the profile itself. Matched by name, case-insensitive.
    pub fn is_protected_profile(&self, profile_id: i64) -> Result<bool> {
        let conn = Connection::open(&self.db_path)?;
        let name: String = conn.query_row(
            "SELECT name FROM profiles WHERE id = ?1",
            params![profile_id],
            |row| row.get(0),
        )?;
        Ok(Self::is_protected_name(&name))
    }

    fn is_protected_name(name: &str) -> bool {
        let n = name.trim().to_lowercase();
        n == "default" || n == "incognito"
    }

    /// Clear every per-profile table for `profile_id` within `tx`. Shared by both
    /// `wipe_profile` (keeps the profile row) and `delete_profile` (removes it).
    ///
    /// This is a COMPLETE wipe: every per-profile table is cleared (browsing
    /// history, bookmarks, domains, media history/playlists, scraper jobs, tabs,
    /// themes, the password & authenticator vaults, etc.) — not just the handful
    /// the old delete implementation touched.
    fn wipe_profile_data(tx: &Connection, profile_id: i64) -> Result<()> {
        // Child tables keyed by a per-profile parent (no profile_id of their own).
        // Delete these before their parents.
        let child_deletes = [
            "DELETE FROM notes WHERE page_id IN (SELECT id FROM pages WHERE profile_id = ?1)",
            "DELETE FROM multimedia_playlist_items WHERE playlist_id IN (SELECT id FROM multimedia_playlists WHERE profile_id = ?1)",
            "DELETE FROM scraped_pages WHERE job_id IN (SELECT id FROM scraping_jobs WHERE profile_id = ?1)",
            "DELETE FROM tab_history WHERE tab_id IN (SELECT id FROM tabs WHERE profile_id = ?1)",
        ];
        // Every table that carries a profile_id directly. Note: privacy_settings is
        // intentionally NOT cleared here — it holds the profile's own config, which
        // delete_profile removes explicitly and wipe_profile keeps.
        let profile_deletes = [
            "DELETE FROM pages WHERE profile_id = ?1",
            "DELETE FROM page_notes WHERE profile_id = ?1",
            "DELETE FROM indexed_pages WHERE profile_id = ?1",
            "DELETE FROM bookmarks WHERE profile_id = ?1",
            "DELETE FROM bookmark_folders WHERE profile_id = ?1",
            "DELETE FROM domains WHERE profile_id = ?1",
            "DELETE FROM domain_lists WHERE profile_id = ?1",
            "DELETE FROM media_downloads WHERE profile_id = ?1",
            "DELETE FROM multimedia_history WHERE profile_id = ?1",
            "DELETE FROM multimedia_playlists WHERE profile_id = ?1",
            "DELETE FROM multimedia_privacy WHERE profile_id = ?1",
            "DELETE FROM scraping_jobs WHERE profile_id = ?1",
            "DELETE FROM split_view_config WHERE profile_id = ?1",
            "DELETE FROM tabs WHERE profile_id = ?1",
            "DELETE FROM themes WHERE profile_id = ?1",
            "DELETE FROM password_entries WHERE profile_id = ?1",
            "DELETE FROM otp_entries WHERE profile_id = ?1",
            "DELETE FROM vault_master WHERE profile_id = ?1",
        ];

        for sql in child_deletes.iter().chain(profile_deletes.iter()) {
            // Tolerate tables that don't exist on older databases — a missing
            // table just means there's nothing of that kind to wipe.
            if let Err(e) = tx.execute(sql, params![profile_id]) {
                let msg = e.to_string();
                if !msg.contains("no such table") {
                    return Err(e);
                }
            }
        }
        Ok(())
    }

    /// Wipe ALL of a profile's data but KEEP the profile (a fresh start). This is
    /// the only destructive option for the protected Default/Incognito profiles,
    /// and is available for any profile. Gated by the 4-digit delete code.
    pub fn wipe_profile(&self, profile_id: i64, pin: &str) -> Result<()> {
        self.require_pin(profile_id, pin)?;
        let conn = Connection::open(&self.db_path)?;
        let tx = conn.unchecked_transaction()?;
        Self::wipe_profile_data(&tx, profile_id)?;
        tx.commit()?;
        Ok(())
    }

    /// Delete a profile and ALL of its data, gated by the 4-digit delete code.
    /// The protected Default/Incognito profiles cannot be deleted — only wiped.
    /// Runs in one transaction (all-or-nothing); the profile row is removed last.
    pub fn delete_profile(&self, profile_id: i64, pin: &str) -> Result<()> {
        if self.is_protected_profile(profile_id)? {
            return Err(rusqlite::Error::InvalidParameterName(
                "The Default and Incognito profiles can't be deleted — wipe them instead".to_string(),
            ));
        }

        let conn = Connection::open(&self.db_path)?;

        // Can't delete the last remaining profile.
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM profiles", [], |row| row.get(0))?;
        if count <= 1 {
            return Err(rusqlite::Error::InvalidParameterName(
                "Cannot delete the only profile".to_string(),
            ));
        }

        self.require_pin(profile_id, pin)?;

        let is_active: i64 = conn.query_row(
            "SELECT is_active FROM profiles WHERE id = ?1",
            params![profile_id],
            |row| row.get(0),
        )?;

        let tx = conn.unchecked_transaction()?;
        Self::wipe_profile_data(&tx, profile_id)?;
        tx.execute("DELETE FROM privacy_settings WHERE profile_id = ?1", params![profile_id])?;
        tx.execute("DELETE FROM profiles WHERE id = ?1", params![profile_id])?;

        // If the deleted profile was active, promote another one.
        if is_active == 1 {
            tx.execute(
                "UPDATE profiles SET is_active = 1 WHERE id = (SELECT MIN(id) FROM profiles)",
                [],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    /// Shared guard: a delete code must be set for the profile, and `pin` must match.
    fn require_pin(&self, profile_id: i64, pin: &str) -> Result<()> {
        if !self.has_delete_pin(profile_id)? {
            return Err(rusqlite::Error::InvalidParameterName(
                "Set a delete code for this profile first".to_string(),
            ));
        }
        if !self.verify_delete_pin(profile_id, pin)? {
            return Err(rusqlite::Error::InvalidParameterName(
                "Incorrect delete code".to_string(),
            ));
        }
        Ok(())
    }

    /// Get privacy settings for a profile
    pub fn get_privacy_settings(&self, profile_id: i64) -> Result<PrivacySettings> {
        let conn = Connection::open(&self.db_path)?;
        let mut stmt = conn.prepare(
            "SELECT profile_id, auto_delete_days, ai_enabled_in_incognito FROM privacy_settings WHERE profile_id = ?1"
        )?;

        stmt.query_row(params![profile_id], |row| {
            Ok(PrivacySettings {
                profile_id: row.get(0)?,
                auto_delete_days: row.get(1)?,
                ai_enabled_in_incognito: row.get::<_, i64>(2)? == 1,
            })
        })
    }

    /// Update privacy settings for a profile
    pub fn update_privacy_settings(&self, settings: &PrivacySettings) -> Result<()> {
        let conn = Connection::open(&self.db_path)?;

        conn.execute(
            "UPDATE privacy_settings SET auto_delete_days = ?1, ai_enabled_in_incognito = ?2 WHERE profile_id = ?3",
            params![
                settings.auto_delete_days,
                if settings.ai_enabled_in_incognito { 1 } else { 0 },
                settings.profile_id
            ],
        )?;

        Ok(())
    }

    /// Export profile data as JSON
    pub fn export_profile(&self, profile_id: i64) -> Result<String> {
        let conn = Connection::open(&self.db_path)?;

        // Get profile
        let profile = self.get_profile_by_id(&conn, profile_id)?;
        let privacy = self.get_privacy_settings(profile_id)?;

        // Get pages for this profile
        let mut pages_stmt = conn.prepare(
            "SELECT id, url, title, content, visited_at FROM pages WHERE profile_id = ?1"
        )?;
        let pages: Vec<serde_json::Value> = pages_stmt
            .query_map(params![profile_id], |row| {
                Ok(serde_json::json!({
                    "id": row.get::<_, i64>(0)?,
                    "url": row.get::<_, String>(1)?,
                    "title": row.get::<_, String>(2)?,
                    "content": row.get::<_, Option<String>>(3)?,
                    "visited_at": row.get::<_, String>(4)?
                }))
            })?
            .filter_map(|r| r.ok())
            .collect();

        // Get domains for this profile
        let mut domains_stmt = conn.prepare(
            "SELECT id, url, category, trust_score, added_date FROM domains WHERE profile_id = ?1"
        )?;
        let domains: Vec<serde_json::Value> = domains_stmt
            .query_map(params![profile_id], |row| {
                Ok(serde_json::json!({
                    "id": row.get::<_, i64>(0)?,
                    "url": row.get::<_, String>(1)?,
                    "category": row.get::<_, String>(2)?,
                    "trust_score": row.get::<_, f64>(3)?,
                    "added_date": row.get::<_, String>(4)?
                }))
            })?
            .filter_map(|r| r.ok())
            .collect();

        let export = serde_json::json!({
            "version": 1,
            "exported_at": chrono_now(),
            "profile": profile,
            "privacy_settings": privacy,
            "pages": pages,
            "domains": domains
        });

        Ok(serde_json::to_string_pretty(&export).unwrap_or_default())
    }

    fn get_profile_by_id(&self, conn: &Connection, profile_id: i64) -> Result<Profile> {
        let mut stmt = conn.prepare(
            "SELECT id, name, icon, created_at, is_active FROM profiles WHERE id = ?1"
        )?;

        stmt.query_row(params![profile_id], |row| {
            Ok(Profile {
                id: Some(row.get(0)?),
                name: row.get(1)?,
                icon: row.get(2)?,
                created_at: row.get(3)?,
                is_active: row.get::<_, i64>(4)? == 1,
            })
        })
    }
}

// Simple timestamp helper (avoiding chrono dependency for now)
fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", duration.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Each ProfileManager method opens its own Connection, so `:memory:` would
    /// give every call a SEPARATE empty database. Use a unique temp FILE per test
    /// (auto-removed) so all calls share one DB.
    struct TestDb {
        path: String,
    }
    impl TestDb {
        fn new() -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("reclaim_profile_test_{}_{}.db", std::process::id(), n))
                .to_string_lossy()
                .to_string();
            let _ = std::fs::remove_file(&path);
            TestDb { path }
        }
        fn manager(&self) -> ProfileManager {
            ProfileManager::new(self.path.clone())
        }
    }
    impl Drop for TestDb {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    #[test]
    fn test_create_profile() {
        let db = TestDb::new();
        let manager = db.manager();
        manager.init().unwrap();

        let profile = manager.create_profile("Test Profile", Some("star"), "1234").unwrap();
        assert_eq!(profile.name, "Test Profile");
        assert_eq!(profile.icon, Some("star".to_string()));
        assert!(!profile.is_active);
    }

    #[test]
    fn test_create_profile_requires_valid_pin() {
        let db = TestDb::new();
        let manager = db.manager();
        manager.init().unwrap();
        assert!(manager.create_profile("Bad", None, "12").is_err());
        assert!(manager.create_profile("Bad2", None, "abcd").is_err());
        assert!(manager.create_profile("Good", None, "4242").is_ok());
    }

    #[test]
    fn test_delete_profile_gated_by_pin() {
        let db = TestDb::new();
        let manager = db.manager();
        manager.init().unwrap();
        // Default profile exists (no pin); create a second with a pin.
        let p = manager.create_profile("Second", None, "1234").unwrap();
        let id = p.id.unwrap();

        // Wrong code is rejected; correct code succeeds.
        assert!(manager.delete_profile(id, "0000").is_err());
        assert!(manager.delete_profile(id, "1234").is_ok());
        assert!(manager.get_profiles().unwrap().iter().all(|p| p.id != Some(id)));
    }

    #[test]
    fn test_default_profile_is_protected_wipe_only() {
        let db = TestDb::new();
        let manager = db.manager();
        manager.init().unwrap();
        let default = manager.get_profiles().unwrap().into_iter().find(|p| p.name == "Default").unwrap();
        let default_id = default.id.unwrap();

        // Default can never be deleted, even with a code set.
        manager.set_delete_pin(default_id, "9999").unwrap();
        assert!(manager.delete_profile(default_id, "9999").is_err());

        // But it CAN be wiped (data cleared, profile kept) with the right code.
        assert!(manager.wipe_profile(default_id, "0000").is_err()); // wrong code
        assert!(manager.wipe_profile(default_id, "9999").is_ok());
        assert!(manager.get_profiles().unwrap().iter().any(|p| p.id == Some(default_id)));
    }

    #[test]
    fn test_wipe_requires_pin_to_be_set() {
        let db = TestDb::new();
        let manager = db.manager();
        manager.init().unwrap();
        // The auto-created Default profile has no delete code yet.
        let default = manager.get_profiles().unwrap().into_iter().find(|p| p.name == "Default").unwrap();
        let default_id = default.id.unwrap();
        assert!(manager.wipe_profile(default_id, "1234").is_err());
        manager.set_delete_pin(default_id, "4321").unwrap();
        assert!(manager.wipe_profile(default_id, "4321").is_ok());
    }
}
