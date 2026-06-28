// Privacy and incognito mode management for EarthServers Local
// Handles per-profile incognito state and history management

use rusqlite::{Connection, Result, params};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{RwLock, LazyLock};

/// Per-profile incognito state - in-memory only, not persisted across restarts
static INCOGNITO_PROFILES: LazyLock<RwLock<HashMap<i64, bool>>> = LazyLock::new(|| RwLock::new(HashMap::new()));

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: i64,
    pub url: String,
    pub title: String,
    pub visited_at: String,
    pub profile_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryStats {
    pub total_pages: i64,
    pub total_domains: i64,
    pub most_visited: Vec<DomainVisitCount>,
    pub recent_pages: Vec<HistoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainVisitCount {
    pub domain: String,
    pub visit_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DateRange {
    pub start: String,
    pub end: String,
}

pub struct PrivacyManager {
    db_path: String,
}

impl PrivacyManager {
    pub fn new(db_path: String) -> Self {
        PrivacyManager { db_path }
    }

    // ==================== Per-Profile Incognito Mode ====================

    /// Check if incognito mode is currently active for a specific profile
    pub fn is_incognito(profile_id: i64) -> bool {
        let profiles = INCOGNITO_PROFILES.read().unwrap();
        *profiles.get(&profile_id).unwrap_or(&false)
    }

    /// Enable incognito mode for a specific profile
    pub fn enable_incognito(profile_id: i64) {
        let mut profiles = INCOGNITO_PROFILES.write().unwrap();
        profiles.insert(profile_id, true);
    }

    /// Disable incognito mode for a specific profile
    pub fn disable_incognito(profile_id: i64) {
        let mut profiles = INCOGNITO_PROFILES.write().unwrap();
        profiles.insert(profile_id, false);
    }

    /// Set incognito mode for a specific profile
    pub fn set_incognito(profile_id: i64, enabled: bool) {
        let mut profiles = INCOGNITO_PROFILES.write().unwrap();
        profiles.insert(profile_id, enabled);
    }

    /// Toggle incognito mode for a specific profile, returns new state
    pub fn toggle_incognito(profile_id: i64) -> bool {
        let mut profiles = INCOGNITO_PROFILES.write().unwrap();
        let current = *profiles.get(&profile_id).unwrap_or(&false);
        let new_state = !current;
        profiles.insert(profile_id, new_state);
        new_state
    }

    /// Get all profiles currently in incognito mode
    pub fn get_incognito_profiles() -> Vec<i64> {
        let profiles = INCOGNITO_PROFILES.read().unwrap();
        profiles.iter()
            .filter(|(_, &is_incognito)| is_incognito)
            .map(|(&profile_id, _)| profile_id)
            .collect()
    }

    // ==================== History Management ====================

    /// Get browsing history for a profile with optional search
    pub fn get_history(
        &self,
        profile_id: i64,
        search_query: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<HistoryEntry>> {
        let conn = Connection::open(&self.db_path)?;

        let entries: Vec<HistoryEntry> = match search_query {
            Some(q) => {
                let pattern = format!("%{}%", q);
                let mut stmt = conn.prepare(
                    "SELECT id, url, title, visited_at, profile_id
                     FROM pages
                     WHERE profile_id = ?1 AND (url LIKE ?2 OR title LIKE ?2)
                     ORDER BY visited_at DESC
                     LIMIT ?3 OFFSET ?4"
                )?;
                let rows = stmt.query_map(params![profile_id, pattern, limit, offset], |row| {
                    Ok(HistoryEntry {
                        id: row.get(0)?,
                        url: row.get(1)?,
                        title: row.get(2)?,
                        visited_at: row.get(3)?,
                        profile_id: row.get(4)?,
                    })
                })?;
                rows.filter_map(|r| r.ok()).collect()
            }
            None => {
                let mut stmt = conn.prepare(
                    "SELECT id, url, title, visited_at, profile_id
                     FROM pages
                     WHERE profile_id = ?1
                     ORDER BY visited_at DESC
                     LIMIT ?2 OFFSET ?3"
                )?;
                let rows = stmt.query_map(params![profile_id, limit, offset], |row| {
                    Ok(HistoryEntry {
                        id: row.get(0)?,
                        url: row.get(1)?,
                        title: row.get(2)?,
                        visited_at: row.get(3)?,
                        profile_id: row.get(4)?,
                    })
                })?;
                rows.filter_map(|r| r.ok()).collect()
            }
        };

        Ok(entries)
    }

    /// Delete a single history entry
    pub fn delete_history_entry(&self, entry_id: i64, profile_id: i64) -> Result<bool> {
        let conn = Connection::open(&self.db_path)?;

        // First delete associated notes
        conn.execute(
            "DELETE FROM notes WHERE page_id = ?1",
            params![entry_id],
        )?;

        // Then delete the page
        let affected = conn.execute(
            "DELETE FROM pages WHERE id = ?1 AND profile_id = ?2",
            params![entry_id, profile_id],
        )?;

        Ok(affected > 0)
    }

    /// Delete history entries within a date range
    pub fn delete_history_by_date_range(
        &self,
        profile_id: i64,
        start_date: &str,
        end_date: &str,
    ) -> Result<i64> {
        let conn = Connection::open(&self.db_path)?;

        // First get the IDs to delete
        let mut stmt = conn.prepare(
            "SELECT id FROM pages WHERE profile_id = ?1 AND visited_at BETWEEN ?2 AND ?3"
        )?;
        let ids: Vec<i64> = stmt
            .query_map(params![profile_id, start_date, end_date], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        // Delete associated notes
        for id in &ids {
            conn.execute("DELETE FROM notes WHERE page_id = ?1", params![id])?;
        }

        // Delete the pages
        let affected = conn.execute(
            "DELETE FROM pages WHERE profile_id = ?1 AND visited_at BETWEEN ?2 AND ?3",
            params![profile_id, start_date, end_date],
        )?;

        Ok(affected as i64)
    }

    /// Clear all history for a profile
    pub fn clear_all_history(&self, profile_id: i64) -> Result<i64> {
        let conn = Connection::open(&self.db_path)?;

        // First delete all notes for this profile's pages
        conn.execute(
            "DELETE FROM notes WHERE page_id IN (SELECT id FROM pages WHERE profile_id = ?1)",
            params![profile_id],
        )?;

        // Then delete all pages
        let affected = conn.execute(
            "DELETE FROM pages WHERE profile_id = ?1",
            params![profile_id],
        )?;

        Ok(affected as i64)
    }

    /// Auto-delete history older than specified days
    pub fn auto_delete_old_history(&self, profile_id: i64, days: i32) -> Result<i64> {
        let conn = Connection::open(&self.db_path)?;

        // Calculate cutoff timestamp (days ago in seconds)
        let cutoff = chrono_days_ago(days);

        // Get IDs to delete
        let mut stmt = conn.prepare(
            "SELECT id FROM pages WHERE profile_id = ?1 AND visited_at < ?2"
        )?;
        let ids: Vec<i64> = stmt
            .query_map(params![profile_id, cutoff], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        // Delete associated notes
        for id in &ids {
            conn.execute("DELETE FROM notes WHERE page_id = ?1", params![id])?;
        }

        // Delete the pages
        let affected = conn.execute(
            "DELETE FROM pages WHERE profile_id = ?1 AND visited_at < ?2",
            params![profile_id, cutoff],
        )?;

        Ok(affected as i64)
    }

    // ==================== History Statistics ====================

    /// Get statistics about browsing history
    pub fn get_history_stats(&self, profile_id: i64) -> Result<HistoryStats> {
        let conn = Connection::open(&self.db_path)?;

        // Total pages
        let total_pages: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pages WHERE profile_id = ?1",
            params![profile_id],
            |row| row.get(0),
        )?;

        // Total unique domains
        let total_domains: i64 = conn.query_row(
            "SELECT COUNT(DISTINCT
                SUBSTR(url, INSTR(url, '://') + 3,
                    CASE
                        WHEN INSTR(SUBSTR(url, INSTR(url, '://') + 3), '/') > 0
                        THEN INSTR(SUBSTR(url, INSTR(url, '://') + 3), '/') - 1
                        ELSE LENGTH(SUBSTR(url, INSTR(url, '://') + 3))
                    END
                )
            ) FROM pages WHERE profile_id = ?1",
            params![profile_id],
            |row| row.get(0),
        )?;

        // Most visited domains
        let mut most_visited_stmt = conn.prepare(
            "SELECT
                SUBSTR(url, INSTR(url, '://') + 3,
                    CASE
                        WHEN INSTR(SUBSTR(url, INSTR(url, '://') + 3), '/') > 0
                        THEN INSTR(SUBSTR(url, INSTR(url, '://') + 3), '/') - 1
                        ELSE LENGTH(SUBSTR(url, INSTR(url, '://') + 3))
                    END
                ) as domain,
                COUNT(*) as visit_count
             FROM pages
             WHERE profile_id = ?1
             GROUP BY domain
             ORDER BY visit_count DESC
             LIMIT 10"
        )?;
        let most_visited: Vec<DomainVisitCount> = most_visited_stmt
            .query_map(params![profile_id], |row| {
                Ok(DomainVisitCount {
                    domain: row.get(0)?,
                    visit_count: row.get(1)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        // Recent pages
        let recent_pages = self.get_history(profile_id, None, 10, 0)?;

        Ok(HistoryStats {
            total_pages,
            total_domains,
            most_visited,
            recent_pages,
        })
    }

    // ==================== Export ====================

    /// Export history as JSON
    pub fn export_history(&self, profile_id: i64) -> Result<String> {
        let conn = Connection::open(&self.db_path)?;

        let mut stmt = conn.prepare(
            "SELECT id, url, title, content, visited_at FROM pages WHERE profile_id = ?1 ORDER BY visited_at DESC"
        )?;

        let entries: Vec<serde_json::Value> = stmt
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

        let export = serde_json::json!({
            "version": 1,
            "exported_at": chrono_now(),
            "profile_id": profile_id,
            "entry_count": entries.len(),
            "history": entries
        });

        Ok(serde_json::to_string_pretty(&export).unwrap_or_default())
    }
}

// Helper: Get current timestamp as string (seconds since epoch)
fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", duration.as_secs())
}

// Helper: Get timestamp for N days ago
fn chrono_days_ago(days: i32) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let days_in_secs = days as u64 * 24 * 60 * 60;
    format!("{}", duration.as_secs().saturating_sub(days_in_secs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_per_profile_incognito() {
        let profile1 = 1;
        let profile2 = 2;

        // Start both profiles in non-incognito
        PrivacyManager::disable_incognito(profile1);
        PrivacyManager::disable_incognito(profile2);
        assert!(!PrivacyManager::is_incognito(profile1));
        assert!(!PrivacyManager::is_incognito(profile2));

        // Enable incognito for profile1 only
        PrivacyManager::enable_incognito(profile1);
        assert!(PrivacyManager::is_incognito(profile1));
        assert!(!PrivacyManager::is_incognito(profile2));

        // Toggle profile2 on
        let state = PrivacyManager::toggle_incognito(profile2);
        assert!(state);
        assert!(PrivacyManager::is_incognito(profile2));

        // Both should now be incognito
        assert!(PrivacyManager::is_incognito(profile1));
        assert!(PrivacyManager::is_incognito(profile2));

        // Get incognito profiles
        let incognito_profiles = PrivacyManager::get_incognito_profiles();
        assert!(incognito_profiles.contains(&profile1));
        assert!(incognito_profiles.contains(&profile2));

        // Toggle profile1 off
        let state = PrivacyManager::toggle_incognito(profile1);
        assert!(!state);
        assert!(!PrivacyManager::is_incognito(profile1));
        assert!(PrivacyManager::is_incognito(profile2));
    }
}
