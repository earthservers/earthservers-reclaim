// Tab management for Earth Reclaim
// Browser-like tab system with history and state

use rusqlite::{Connection, Result, params};
use serde::{Deserialize, Serialize};

fn chrono_now() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tab {
    pub id: i64,
    pub profile_id: i64,
    pub title: Option<String>,
    pub url: String,
    pub favicon: Option<String>,
    pub position: i32,
    pub is_pinned: bool,
    pub is_active: bool,
    pub scroll_position: i32,
    pub created_at: String,
    pub last_accessed: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabHistoryEntry {
    pub id: i64,
    pub tab_id: i64,
    pub url: String,
    pub title: Option<String>,
    pub visited_at: String,
    pub position: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTabRequest {
    pub profile_id: i64,
    pub url: String,
    pub title: Option<String>,
}

pub struct TabManager {
    db_path: String,
}

impl TabManager {
    pub fn new(db_path: String) -> Self {
        TabManager { db_path }
    }

    /// Create a new tab
    pub fn create_tab(&self, profile_id: i64, url: &str, title: Option<&str>) -> Result<Tab> {
        let conn = Connection::open(&self.db_path)?;
        let now = chrono_now();

        // Get max position
        let max_pos: i32 = conn
            .query_row(
                "SELECT COALESCE(MAX(position), -1) FROM tabs WHERE profile_id = ?1",
                params![profile_id],
                |row| row.get(0),
            )
            .unwrap_or(-1);

        let position = max_pos + 1;

        conn.execute(
            "INSERT INTO tabs (profile_id, title, url, position, is_pinned, is_active, scroll_position, created_at, last_accessed)
             VALUES (?1, ?2, ?3, ?4, 0, 0, 0, ?5, ?5)",
            params![profile_id, title, url, position, now],
        )?;

        let id = conn.last_insert_rowid();

        // Add to tab history
        conn.execute(
            "INSERT INTO tab_history (tab_id, url, title, visited_at, position)
             VALUES (?1, ?2, ?3, ?4, 0)",
            params![id, url, title, now],
        )?;

        Ok(Tab {
            id,
            profile_id,
            title: title.map(String::from),
            url: url.to_string(),
            favicon: None,
            position,
            is_pinned: false,
            is_active: false,
            scroll_position: 0,
            created_at: now.clone(),
            last_accessed: now,
        })
    }

    /// Close/delete a tab
    pub fn close_tab(&self, tab_id: i64) -> Result<()> {
        let conn = Connection::open(&self.db_path)?;

        // Get the tab's profile and position
        let (profile_id, position): (i64, i32) = conn.query_row(
            "SELECT profile_id, position FROM tabs WHERE id = ?1",
            params![tab_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;

        // Delete the tab (cascade deletes history)
        conn.execute("DELETE FROM tabs WHERE id = ?1", params![tab_id])?;

        // Reorder remaining tabs
        conn.execute(
            "UPDATE tabs SET position = position - 1 WHERE profile_id = ?1 AND position > ?2",
            params![profile_id, position],
        )?;

        Ok(())
    }

    /// Get all tabs for a profile
    pub fn get_all_tabs(&self, profile_id: i64) -> Result<Vec<Tab>> {
        let conn = Connection::open(&self.db_path)?;
        let mut stmt = conn.prepare(
            "SELECT id, profile_id, title, url, favicon, position, is_pinned, is_active, scroll_position, created_at, last_accessed
             FROM tabs WHERE profile_id = ?1 ORDER BY is_pinned DESC, position ASC"
        )?;

        let tabs = stmt.query_map(params![profile_id], |row| {
            Ok(Tab {
                id: row.get(0)?,
                profile_id: row.get(1)?,
                title: row.get(2)?,
                url: row.get(3)?,
                favicon: row.get(4)?,
                position: row.get(5)?,
                is_pinned: row.get::<_, i32>(6)? != 0,
                is_active: row.get::<_, i32>(7)? != 0,
                scroll_position: row.get(8)?,
                created_at: row.get(9)?,
                last_accessed: row.get(10)?,
            })
        })?;

        tabs.collect()
    }

    /// Update tab details
    pub fn update_tab(&self, tab_id: i64, title: Option<&str>, url: Option<&str>, favicon: Option<&str>) -> Result<Tab> {
        let conn = Connection::open(&self.db_path)?;
        let now = chrono_now();

        // Note: We use explicit SQL statements below instead of dynamic query building
        // to avoid borrow checker issues with temporary format strings

        // Execute update based on provided fields
        if let Some(t) = title {
            if let Some(u) = url {
                if let Some(f) = favicon {
                    conn.execute(
                        &format!("UPDATE tabs SET last_accessed = ?1, title = ?2, url = ?3, favicon = ?4 WHERE id = ?5"),
                        params![now, t, u, f, tab_id],
                    )?;
                } else {
                    conn.execute(
                        &format!("UPDATE tabs SET last_accessed = ?1, title = ?2, url = ?3 WHERE id = ?4"),
                        params![now, t, u, tab_id],
                    )?;
                }
            } else if let Some(f) = favicon {
                conn.execute(
                    &format!("UPDATE tabs SET last_accessed = ?1, title = ?2, favicon = ?3 WHERE id = ?4"),
                    params![now, t, f, tab_id],
                )?;
            } else {
                conn.execute(
                    &format!("UPDATE tabs SET last_accessed = ?1, title = ?2 WHERE id = ?3"),
                    params![now, t, tab_id],
                )?;
            }
        } else if let Some(u) = url {
            if let Some(f) = favicon {
                conn.execute(
                    &format!("UPDATE tabs SET last_accessed = ?1, url = ?2, favicon = ?3 WHERE id = ?4"),
                    params![now, u, f, tab_id],
                )?;
            } else {
                conn.execute(
                    &format!("UPDATE tabs SET last_accessed = ?1, url = ?2 WHERE id = ?3"),
                    params![now, u, tab_id],
                )?;
            }
        } else if let Some(f) = favicon {
            conn.execute(
                &format!("UPDATE tabs SET last_accessed = ?1, favicon = ?2 WHERE id = ?3"),
                params![now, f, tab_id],
            )?;
        } else {
            conn.execute(
                "UPDATE tabs SET last_accessed = ?1 WHERE id = ?2",
                params![now, tab_id],
            )?;
        }

        // If URL changed, add to history
        if let Some(u) = url {
            let history_pos: i32 = conn
                .query_row(
                    "SELECT COALESCE(MAX(position), -1) FROM tab_history WHERE tab_id = ?1",
                    params![tab_id],
                    |row| row.get(0),
                )
                .unwrap_or(-1);

            conn.execute(
                "INSERT INTO tab_history (tab_id, url, title, visited_at, position)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![tab_id, u, title, now, history_pos + 1],
            )?;
        }

        // Return updated tab
        self.get_tab(tab_id)
    }

    /// Get a single tab
    pub fn get_tab(&self, tab_id: i64) -> Result<Tab> {
        let conn = Connection::open(&self.db_path)?;
        conn.query_row(
            "SELECT id, profile_id, title, url, favicon, position, is_pinned, is_active, scroll_position, created_at, last_accessed
             FROM tabs WHERE id = ?1",
            params![tab_id],
            |row| {
                Ok(Tab {
                    id: row.get(0)?,
                    profile_id: row.get(1)?,
                    title: row.get(2)?,
                    url: row.get(3)?,
                    favicon: row.get(4)?,
                    position: row.get(5)?,
                    is_pinned: row.get::<_, i32>(6)? != 0,
                    is_active: row.get::<_, i32>(7)? != 0,
                    scroll_position: row.get(8)?,
                    created_at: row.get(9)?,
                    last_accessed: row.get(10)?,
                })
            },
        )
    }

    /// Reorder tabs
    pub fn reorder_tabs(&self, tab_ids: Vec<i64>) -> Result<()> {
        let conn = Connection::open(&self.db_path)?;

        for (index, tab_id) in tab_ids.iter().enumerate() {
            conn.execute(
                "UPDATE tabs SET position = ?1 WHERE id = ?2",
                params![index as i32, tab_id],
            )?;
        }

        Ok(())
    }

    /// Pin/unpin a tab
    pub fn pin_tab(&self, tab_id: i64, pinned: bool) -> Result<Tab> {
        let conn = Connection::open(&self.db_path)?;

        conn.execute(
            "UPDATE tabs SET is_pinned = ?1 WHERE id = ?2",
            params![if pinned { 1 } else { 0 }, tab_id],
        )?;

        self.get_tab(tab_id)
    }

    /// Set active tab (deactivates others in profile)
    pub fn set_active_tab(&self, tab_id: i64) -> Result<Tab> {
        let conn = Connection::open(&self.db_path)?;
        let now = chrono_now();

        // Get profile_id
        let profile_id: i64 = conn.query_row(
            "SELECT profile_id FROM tabs WHERE id = ?1",
            params![tab_id],
            |row| row.get(0),
        )?;

        // Deactivate all tabs in profile
        conn.execute(
            "UPDATE tabs SET is_active = 0 WHERE profile_id = ?1",
            params![profile_id],
        )?;

        // Activate this tab
        conn.execute(
            "UPDATE tabs SET is_active = 1, last_accessed = ?1 WHERE id = ?2",
            params![now, tab_id],
        )?;

        self.get_tab(tab_id)
    }

    /// Get tab history
    pub fn get_tab_history(&self, tab_id: i64) -> Result<Vec<TabHistoryEntry>> {
        let conn = Connection::open(&self.db_path)?;
        let mut stmt = conn.prepare(
            "SELECT id, tab_id, url, title, visited_at, position
             FROM tab_history WHERE tab_id = ?1 ORDER BY position ASC"
        )?;

        let entries = stmt.query_map(params![tab_id], |row| {
            Ok(TabHistoryEntry {
                id: row.get(0)?,
                tab_id: row.get(1)?,
                url: row.get(2)?,
                title: row.get(3)?,
                visited_at: row.get(4)?,
                position: row.get(5)?,
            })
        })?;

        entries.collect()
    }

    /// Navigate back in tab history
    pub fn navigate_back(&self, tab_id: i64) -> Result<Option<String>> {
        let conn = Connection::open(&self.db_path)?;

        // Get current tab's URL
        let current_url: String = conn.query_row(
            "SELECT url FROM tabs WHERE id = ?1",
            params![tab_id],
            |row| row.get(0),
        )?;

        // Find current position in history
        let current_pos: Option<i32> = conn.query_row(
            "SELECT position FROM tab_history WHERE tab_id = ?1 AND url = ?2 ORDER BY position DESC LIMIT 1",
            params![tab_id, current_url],
            |row| row.get(0),
        ).ok();

        if let Some(pos) = current_pos {
            if pos > 0 {
                // Get previous URL
                let prev_url: Option<String> = conn.query_row(
                    "SELECT url FROM tab_history WHERE tab_id = ?1 AND position = ?2",
                    params![tab_id, pos - 1],
                    |row| row.get(0),
                ).ok();

                return Ok(prev_url);
            }
        }

        Ok(None)
    }

    /// Navigate forward in tab history
    pub fn navigate_forward(&self, tab_id: i64) -> Result<Option<String>> {
        let conn = Connection::open(&self.db_path)?;

        // Get current tab's URL
        let current_url: String = conn.query_row(
            "SELECT url FROM tabs WHERE id = ?1",
            params![tab_id],
            |row| row.get(0),
        )?;

        // Find current position in history
        let current_pos: Option<i32> = conn.query_row(
            "SELECT position FROM tab_history WHERE tab_id = ?1 AND url = ?2 ORDER BY position DESC LIMIT 1",
            params![tab_id, current_url],
            |row| row.get(0),
        ).ok();

        // Get max position
        let max_pos: i32 = conn.query_row(
            "SELECT COALESCE(MAX(position), 0) FROM tab_history WHERE tab_id = ?1",
            params![tab_id],
            |row| row.get(0),
        )?;

        if let Some(pos) = current_pos {
            if pos < max_pos {
                // Get next URL
                let next_url: Option<String> = conn.query_row(
                    "SELECT url FROM tab_history WHERE tab_id = ?1 AND position = ?2",
                    params![tab_id, pos + 1],
                    |row| row.get(0),
                ).ok();

                return Ok(next_url);
            }
        }

        Ok(None)
    }

    /// Duplicate a tab
    pub fn duplicate_tab(&self, tab_id: i64) -> Result<Tab> {
        let original = self.get_tab(tab_id)?;
        self.create_tab(original.profile_id, &original.url, original.title.as_deref())
    }

    /// Update scroll position
    pub fn update_scroll_position(&self, tab_id: i64, scroll_position: i32) -> Result<()> {
        let conn = Connection::open(&self.db_path)?;
        conn.execute(
            "UPDATE tabs SET scroll_position = ?1 WHERE id = ?2",
            params![scroll_position, tab_id],
        )?;
        Ok(())
    }

    /// Close all tabs except pinned
    pub fn close_unpinned_tabs(&self, profile_id: i64) -> Result<()> {
        let conn = Connection::open(&self.db_path)?;
        conn.execute(
            "DELETE FROM tabs WHERE profile_id = ?1 AND is_pinned = 0",
            params![profile_id],
        )?;
        Ok(())
    }

    /// Close tabs to the right of a given tab
    pub fn close_tabs_to_right(&self, tab_id: i64) -> Result<()> {
        let conn = Connection::open(&self.db_path)?;

        let (profile_id, position): (i64, i32) = conn.query_row(
            "SELECT profile_id, position FROM tabs WHERE id = ?1",
            params![tab_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;

        conn.execute(
            "DELETE FROM tabs WHERE profile_id = ?1 AND position > ?2 AND is_pinned = 0",
            params![profile_id, position],
        )?;

        Ok(())
    }
}
