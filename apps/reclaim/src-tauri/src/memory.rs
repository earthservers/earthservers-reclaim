// Memory functionality for EarthMemory
// Manages indexed pages, notes, and semantic search

use rusqlite::{Connection, Result, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedPage {
    pub id: Option<i64>,
    pub url: String,
    pub title: String,
    pub content: Option<String>,
    pub summary: Option<String>,
    pub indexed_at: String,
    pub last_visited: String,
    pub visit_count: i64,
    pub is_favorite: bool,
    pub tags: Option<String>,
    pub profile_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageNote {
    pub id: Option<i64>,
    pub page_id: i64,
    pub content: String,
    pub created_at: String,
    pub updated_at: String,
    pub profile_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    pub total_pages: i64,
    pub total_notes: i64,
    pub favorites_count: i64,
    pub total_visits: i64,
    pub tags: Vec<TagCount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagCount {
    pub tag: String,
    pub count: i64,
}

pub struct MemoryManager {
    db_path: String,
}

impl MemoryManager {
    pub fn new(db_path: String) -> Self {
        MemoryManager { db_path }
    }

    /// Initialize memory tables
    pub fn init(&self) -> Result<()> {
        let conn = Connection::open(&self.db_path)?;

        // Indexed pages table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS indexed_pages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url TEXT NOT NULL,
                title TEXT NOT NULL,
                content TEXT,
                summary TEXT,
                indexed_at TEXT NOT NULL,
                last_visited TEXT NOT NULL,
                visit_count INTEGER NOT NULL DEFAULT 1,
                is_favorite INTEGER NOT NULL DEFAULT 0,
                tags TEXT,
                profile_id INTEGER,
                UNIQUE(url, profile_id),
                FOREIGN KEY (profile_id) REFERENCES profiles(id) ON DELETE CASCADE
            )",
            [],
        )?;

        // Page notes table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS page_notes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                page_id INTEGER NOT NULL,
                content TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                profile_id INTEGER,
                FOREIGN KEY (page_id) REFERENCES indexed_pages(id) ON DELETE CASCADE,
                FOREIGN KEY (profile_id) REFERENCES profiles(id) ON DELETE CASCADE
            )",
            [],
        )?;

        // Indexes for faster lookups
        conn.execute("CREATE INDEX IF NOT EXISTS idx_pages_url ON indexed_pages(url)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_pages_profile ON indexed_pages(profile_id)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_pages_favorite ON indexed_pages(is_favorite)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_notes_page ON page_notes(page_id)", [])?;

        Ok(())
    }

    // ==================== Page CRUD ====================

    /// Index a new page or update existing
    pub fn index_page(&self, page: &IndexedPage, profile_id: i64) -> Result<IndexedPage> {
        let conn = Connection::open(&self.db_path)?;
        let now = chrono_now();

        // Check if page exists
        let existing: Option<i64> = conn.query_row(
            "SELECT id FROM indexed_pages WHERE url = ?1 AND profile_id = ?2",
            params![page.url, profile_id],
            |row| row.get(0),
        ).ok();

        if let Some(id) = existing {
            // Update existing page
            conn.execute(
                "UPDATE indexed_pages SET
                    title = ?1,
                    content = ?2,
                    summary = ?3,
                    last_visited = ?4,
                    visit_count = visit_count + 1,
                    tags = ?5
                WHERE id = ?6",
                params![
                    page.title,
                    page.content,
                    page.summary,
                    now,
                    page.tags,
                    id
                ],
            )?;

            // Return updated page
            self.get_page_by_id(id)
        } else {
            // Insert new page
            conn.execute(
                "INSERT INTO indexed_pages (url, title, content, summary, indexed_at, last_visited, visit_count, is_favorite, tags, profile_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    page.url,
                    page.title,
                    page.content,
                    page.summary,
                    now,
                    now,
                    1,
                    page.is_favorite,
                    page.tags,
                    profile_id
                ],
            )?;

            let id = conn.last_insert_rowid();
            Ok(IndexedPage {
                id: Some(id),
                url: page.url.clone(),
                title: page.title.clone(),
                content: page.content.clone(),
                summary: page.summary.clone(),
                indexed_at: now.clone(),
                last_visited: now,
                visit_count: 1,
                is_favorite: page.is_favorite,
                tags: page.tags.clone(),
                profile_id: Some(profile_id),
            })
        }
    }

    /// Upsert a page from the background AUTO-CURATOR. Unlike `index_page`, this
    /// never touches user-owned fields: it refreshes content/summary (only when a
    /// new value is provided) and bumps the visit count, but leaves tags and
    /// favorite alone. New auto rows are tagged `auto` so they're distinguishable.
    pub fn journal_page(
        &self,
        url: &str,
        title: &str,
        content: Option<&str>,
        summary: Option<&str>,
        profile_id: i64,
    ) -> Result<()> {
        let conn = Connection::open(&self.db_path)?;
        let now = chrono_now();
        let existing: Option<i64> = conn
            .query_row(
                "SELECT id FROM indexed_pages WHERE url = ?1 AND profile_id = ?2",
                params![url, profile_id],
                |row| row.get(0),
            )
            .ok();

        if let Some(id) = existing {
            conn.execute(
                "UPDATE indexed_pages SET
                    title = ?1,
                    content = COALESCE(?2, content),
                    summary = COALESCE(?3, summary),
                    last_visited = ?4,
                    visit_count = visit_count + 1
                 WHERE id = ?5",
                params![title, content, summary, now, id],
            )?;
        } else {
            conn.execute(
                "INSERT INTO indexed_pages (url, title, content, summary, indexed_at, last_visited, visit_count, is_favorite, tags, profile_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, 0, 'auto', ?7)",
                params![url, title, content, summary, now, now, profile_id],
            )?;
        }
        Ok(())
    }

    /// Get page by ID
    fn get_page_by_id(&self, id: i64) -> Result<IndexedPage> {
        let conn = Connection::open(&self.db_path)?;
        conn.query_row(
            "SELECT id, url, title, content, summary, indexed_at, last_visited, visit_count, is_favorite, tags, profile_id
             FROM indexed_pages WHERE id = ?1",
            params![id],
            |row| {
                Ok(IndexedPage {
                    id: Some(row.get(0)?),
                    url: row.get(1)?,
                    title: row.get(2)?,
                    content: row.get(3)?,
                    summary: row.get(4)?,
                    indexed_at: row.get(5)?,
                    last_visited: row.get(6)?,
                    visit_count: row.get(7)?,
                    is_favorite: row.get::<_, i64>(8)? == 1,
                    tags: row.get(9)?,
                    profile_id: row.get(10)?,
                })
            },
        )
    }

    /// Get all indexed pages for a profile
    pub fn get_pages(&self, profile_id: i64, limit: Option<i64>, offset: Option<i64>) -> Result<Vec<IndexedPage>> {
        let conn = Connection::open(&self.db_path)?;
        let limit = limit.unwrap_or(100);
        let offset = offset.unwrap_or(0);

        let mut stmt = conn.prepare(
            "SELECT id, url, title, content, summary, indexed_at, last_visited, visit_count, is_favorite, tags, profile_id
             FROM indexed_pages
             WHERE profile_id = ?1
             ORDER BY last_visited DESC
             LIMIT ?2 OFFSET ?3"
        )?;

        let pages = stmt.query_map(params![profile_id, limit, offset], |row| {
            Ok(IndexedPage {
                id: Some(row.get(0)?),
                url: row.get(1)?,
                title: row.get(2)?,
                content: row.get(3)?,
                summary: row.get(4)?,
                indexed_at: row.get(5)?,
                last_visited: row.get(6)?,
                visit_count: row.get(7)?,
                is_favorite: row.get::<_, i64>(8)? == 1,
                tags: row.get(9)?,
                profile_id: row.get(10)?,
            })
        })?;

        pages.collect()
    }

    /// Get favorite pages
    pub fn get_favorites(&self, profile_id: i64) -> Result<Vec<IndexedPage>> {
        let conn = Connection::open(&self.db_path)?;
        let mut stmt = conn.prepare(
            "SELECT id, url, title, content, summary, indexed_at, last_visited, visit_count, is_favorite, tags, profile_id
             FROM indexed_pages
             WHERE profile_id = ?1 AND is_favorite = 1
             ORDER BY last_visited DESC"
        )?;

        let pages = stmt.query_map(params![profile_id], |row| {
            Ok(IndexedPage {
                id: Some(row.get(0)?),
                url: row.get(1)?,
                title: row.get(2)?,
                content: row.get(3)?,
                summary: row.get(4)?,
                indexed_at: row.get(5)?,
                last_visited: row.get(6)?,
                visit_count: row.get(7)?,
                is_favorite: row.get::<_, i64>(8)? == 1,
                tags: row.get(9)?,
                profile_id: row.get(10)?,
            })
        })?;

        pages.collect()
    }

    /// Search pages by title, URL, content, or tags
    pub fn search_pages(&self, profile_id: i64, query: &str) -> Result<Vec<IndexedPage>> {
        let conn = Connection::open(&self.db_path)?;
        let pattern = format!("%{}%", query.to_lowercase());

        let mut stmt = conn.prepare(
            "SELECT id, url, title, content, summary, indexed_at, last_visited, visit_count, is_favorite, tags, profile_id
             FROM indexed_pages
             WHERE profile_id = ?1 AND (
                 LOWER(url) LIKE ?2 OR
                 LOWER(title) LIKE ?2 OR
                 LOWER(content) LIKE ?2 OR
                 LOWER(tags) LIKE ?2 OR
                 LOWER(summary) LIKE ?2
             )
             ORDER BY visit_count DESC, last_visited DESC
             LIMIT 50"
        )?;

        let pages = stmt.query_map(params![profile_id, pattern], |row| {
            Ok(IndexedPage {
                id: Some(row.get(0)?),
                url: row.get(1)?,
                title: row.get(2)?,
                content: row.get(3)?,
                summary: row.get(4)?,
                indexed_at: row.get(5)?,
                last_visited: row.get(6)?,
                visit_count: row.get(7)?,
                is_favorite: row.get::<_, i64>(8)? == 1,
                tags: row.get(9)?,
                profile_id: row.get(10)?,
            })
        })?;

        pages.collect()
    }

    /// Toggle favorite status
    pub fn toggle_favorite(&self, page_id: i64, profile_id: i64) -> Result<bool> {
        let conn = Connection::open(&self.db_path)?;

        let current: i64 = conn.query_row(
            "SELECT is_favorite FROM indexed_pages WHERE id = ?1 AND profile_id = ?2",
            params![page_id, profile_id],
            |row| row.get(0),
        )?;

        let new_value = if current == 1 { 0 } else { 1 };

        conn.execute(
            "UPDATE indexed_pages SET is_favorite = ?1 WHERE id = ?2 AND profile_id = ?3",
            params![new_value, page_id, profile_id],
        )?;

        Ok(new_value == 1)
    }

    /// Update page tags
    pub fn update_tags(&self, page_id: i64, profile_id: i64, tags: &str) -> Result<()> {
        let conn = Connection::open(&self.db_path)?;

        conn.execute(
            "UPDATE indexed_pages SET tags = ?1 WHERE id = ?2 AND profile_id = ?3",
            params![tags, page_id, profile_id],
        )?;

        Ok(())
    }

    /// Delete an indexed page
    pub fn delete_page(&self, page_id: i64, profile_id: i64) -> Result<bool> {
        let conn = Connection::open(&self.db_path)?;
        let affected = conn.execute(
            "DELETE FROM indexed_pages WHERE id = ?1 AND profile_id = ?2",
            params![page_id, profile_id],
        )?;
        Ok(affected > 0)
    }

    // ==================== Notes ====================

    /// Add a note to a page
    pub fn add_note(&self, page_id: i64, content: &str, profile_id: i64) -> Result<PageNote> {
        let conn = Connection::open(&self.db_path)?;
        let now = chrono_now();

        conn.execute(
            "INSERT INTO page_notes (page_id, content, created_at, updated_at, profile_id)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![page_id, content, now, now, profile_id],
        )?;

        let id = conn.last_insert_rowid();
        Ok(PageNote {
            id: Some(id),
            page_id,
            content: content.to_string(),
            created_at: now.clone(),
            updated_at: now,
            profile_id: Some(profile_id),
        })
    }

    /// Get notes for a page
    pub fn get_page_notes(&self, page_id: i64) -> Result<Vec<PageNote>> {
        let conn = Connection::open(&self.db_path)?;
        let mut stmt = conn.prepare(
            "SELECT id, page_id, content, created_at, updated_at, profile_id
             FROM page_notes WHERE page_id = ?1 ORDER BY created_at DESC"
        )?;

        let notes = stmt.query_map(params![page_id], |row| {
            Ok(PageNote {
                id: Some(row.get(0)?),
                page_id: row.get(1)?,
                content: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
                profile_id: row.get(5)?,
            })
        })?;

        notes.collect()
    }

    /// Update a note
    pub fn update_note(&self, note_id: i64, content: &str, profile_id: i64) -> Result<()> {
        let conn = Connection::open(&self.db_path)?;
        let now = chrono_now();

        conn.execute(
            "UPDATE page_notes SET content = ?1, updated_at = ?2 WHERE id = ?3 AND profile_id = ?4",
            params![content, now, note_id, profile_id],
        )?;

        Ok(())
    }

    /// Delete a note
    pub fn delete_note(&self, note_id: i64, profile_id: i64) -> Result<bool> {
        let conn = Connection::open(&self.db_path)?;
        let affected = conn.execute(
            "DELETE FROM page_notes WHERE id = ?1 AND profile_id = ?2",
            params![note_id, profile_id],
        )?;
        Ok(affected > 0)
    }

    // ==================== Statistics ====================

    /// Get memory statistics
    pub fn get_stats(&self, profile_id: i64) -> Result<MemoryStats> {
        let conn = Connection::open(&self.db_path)?;

        let total_pages: i64 = conn.query_row(
            "SELECT COUNT(*) FROM indexed_pages WHERE profile_id = ?1",
            params![profile_id],
            |row| row.get(0),
        )?;

        let total_notes: i64 = conn.query_row(
            "SELECT COUNT(*) FROM page_notes WHERE profile_id = ?1",
            params![profile_id],
            |row| row.get(0),
        )?;

        let favorites_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM indexed_pages WHERE profile_id = ?1 AND is_favorite = 1",
            params![profile_id],
            |row| row.get(0),
        )?;

        let total_visits: i64 = conn.query_row(
            "SELECT COALESCE(SUM(visit_count), 0) FROM indexed_pages WHERE profile_id = ?1",
            params![profile_id],
            |row| row.get(0),
        )?;

        // Get tag counts
        let mut stmt = conn.prepare(
            "SELECT tags FROM indexed_pages WHERE profile_id = ?1 AND tags IS NOT NULL AND tags != ''"
        )?;

        let mut tag_counts: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
        let rows = stmt.query_map(params![profile_id], |row| {
            let tags: String = row.get(0)?;
            Ok(tags)
        })?;

        for row in rows.flatten() {
            for tag in row.split(',').map(|t| t.trim().to_lowercase()) {
                if !tag.is_empty() {
                    *tag_counts.entry(tag).or_insert(0) += 1;
                }
            }
        }

        let mut tags: Vec<TagCount> = tag_counts
            .into_iter()
            .map(|(tag, count)| TagCount { tag, count })
            .collect();
        tags.sort_by(|a, b| b.count.cmp(&a.count));

        Ok(MemoryStats {
            total_pages,
            total_notes,
            favorites_count,
            total_visits,
            tags,
        })
    }

    /// Get all unique tags
    pub fn get_all_tags(&self, profile_id: i64) -> Result<Vec<String>> {
        let conn = Connection::open(&self.db_path)?;
        let mut stmt = conn.prepare(
            "SELECT tags FROM indexed_pages WHERE profile_id = ?1 AND tags IS NOT NULL AND tags != ''"
        )?;

        let mut all_tags: std::collections::HashSet<String> = std::collections::HashSet::new();
        let rows = stmt.query_map(params![profile_id], |row| {
            let tags: String = row.get(0)?;
            Ok(tags)
        })?;

        for row in rows.flatten() {
            for tag in row.split(',').map(|t| t.trim().to_lowercase()) {
                if !tag.is_empty() {
                    all_tags.insert(tag);
                }
            }
        }

        let mut tags: Vec<String> = all_tags.into_iter().collect();
        tags.sort();
        Ok(tags)
    }

    // ==================== Export/Import ====================

    /// Export all memory data as JSON
    pub fn export_memory(&self, profile_id: i64) -> Result<String> {
        let pages = self.get_pages(profile_id, Some(10000), None)?;

        // Get notes for each page
        let mut pages_with_notes: Vec<serde_json::Value> = Vec::new();
        for page in pages {
            let notes = self.get_page_notes(page.id.unwrap_or(0))?;
            pages_with_notes.push(serde_json::json!({
                "url": page.url,
                "title": page.title,
                "content": page.content,
                "summary": page.summary,
                "visit_count": page.visit_count,
                "is_favorite": page.is_favorite,
                "tags": page.tags,
                "notes": notes.iter().map(|n| serde_json::json!({
                    "content": n.content,
                    "created_at": n.created_at
                })).collect::<Vec<_>>()
            }));
        }

        let export = serde_json::json!({
            "version": 1,
            "type": "earthservers-memory",
            "exported_at": chrono_now(),
            "pages": pages_with_notes
        });

        Ok(serde_json::to_string_pretty(&export).unwrap_or_default())
    }

    /// Import memory data from JSON
    pub fn import_memory(&self, profile_id: i64, json_data: &str) -> Result<i64> {
        let data: serde_json::Value = serde_json::from_str(json_data)
            .map_err(|_| rusqlite::Error::InvalidQuery)?;

        let pages = data["pages"].as_array()
            .ok_or(rusqlite::Error::InvalidQuery)?;

        let mut imported = 0i64;
        for p in pages {
            let page = IndexedPage {
                id: None,
                url: p["url"].as_str().unwrap_or_default().to_string(),
                title: p["title"].as_str().unwrap_or_default().to_string(),
                content: p["content"].as_str().map(String::from),
                summary: p["summary"].as_str().map(String::from),
                indexed_at: String::new(),
                last_visited: String::new(),
                visit_count: p["visit_count"].as_i64().unwrap_or(1),
                is_favorite: p["is_favorite"].as_bool().unwrap_or(false),
                tags: p["tags"].as_str().map(String::from),
                profile_id: Some(profile_id),
            };

            if !page.url.is_empty() {
                if let Ok(indexed_page) = self.index_page(&page, profile_id) {
                    imported += 1;

                    // Import notes if present
                    if let Some(notes) = p["notes"].as_array() {
                        for note in notes {
                            if let Some(content) = note["content"].as_str() {
                                let _ = self.add_note(indexed_page.id.unwrap_or(0), content, profile_id);
                            }
                        }
                    }
                }
            }
        }

        Ok(imported)
    }
}

fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", duration.as_secs())
}
