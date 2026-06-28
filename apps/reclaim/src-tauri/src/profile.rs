// Profile management for EarthServers Local
// Handles multiple user profiles with isolated data

use rusqlite::{Connection, Result, params};
use serde::{Deserialize, Serialize};

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

        // Create default profile if none exists
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM profiles",
            [],
            |row| row.get(0),
        )?;

        if count == 0 {
            self.create_default_profile(&conn)?;
        }

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

    /// Create a new profile
    pub fn create_profile(&self, name: &str, icon: Option<&str>) -> Result<Profile> {
        let conn = Connection::open(&self.db_path)?;
        let now = chrono_now();

        conn.execute(
            "INSERT INTO profiles (name, icon, created_at, is_active) VALUES (?1, ?2, ?3, 0)",
            params![name, icon, now],
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

    /// Delete a profile and all associated data
    pub fn delete_profile(&self, profile_id: i64) -> Result<()> {
        let conn = Connection::open(&self.db_path)?;

        // Check if this is the only profile
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM profiles",
            [],
            |row| row.get(0),
        )?;

        if count <= 1 {
            return Err(rusqlite::Error::QueryReturnedNoRows); // Can't delete last profile
        }

        // Check if deleting active profile
        let is_active: i64 = conn.query_row(
            "SELECT is_active FROM profiles WHERE id = ?1",
            params![profile_id],
            |row| row.get(0),
        )?;

        // Delete associated data first (CASCADE should handle this, but be explicit)
        conn.execute("DELETE FROM pages WHERE profile_id = ?1", params![profile_id])?;
        conn.execute("DELETE FROM domains WHERE profile_id = ?1", params![profile_id])?;
        conn.execute("DELETE FROM domain_lists WHERE profile_id = ?1", params![profile_id])?;
        conn.execute("DELETE FROM privacy_settings WHERE profile_id = ?1", params![profile_id])?;
        conn.execute("DELETE FROM profiles WHERE id = ?1", params![profile_id])?;

        // If deleted profile was active, activate another one
        if is_active == 1 {
            conn.execute(
                "UPDATE profiles SET is_active = 1 WHERE id = (SELECT MIN(id) FROM profiles)",
                [],
            )?;
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

    #[test]
    fn test_create_profile() {
        let manager = ProfileManager::new(":memory:".to_string());
        manager.init().unwrap();

        let profile = manager.create_profile("Test Profile", Some("star")).unwrap();
        assert_eq!(profile.name, "Test Profile");
        assert_eq!(profile.icon, Some("star".to_string()));
        assert!(!profile.is_active);
    }
}
