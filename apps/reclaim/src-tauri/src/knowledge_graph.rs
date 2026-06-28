// Knowledge graph for EarthMemory
// Stores and queries personal browsing history and notes
// Profile-aware: all operations are scoped to the active profile
// Privacy-aware: respects incognito mode (nothing saved when active)

use rusqlite::{Connection, Result, params};
use serde::{Deserialize, Serialize};

use crate::privacy::PrivacyManager;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page {
    pub id: Option<i64>,
    pub url: String,
    pub title: String,
    pub content: String,
    pub visited_at: String,
    pub embedding: Option<Vec<f32>>,
    pub profile_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub id: Option<i64>,
    pub page_id: i64,
    pub content: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub page: Page,
    pub relevance: f64,
    pub snippet: String,
}

pub struct KnowledgeGraph {
    db_path: String,
}

impl KnowledgeGraph {
    pub fn new(db_path: String) -> Self {
        KnowledgeGraph { db_path }
    }

    pub fn init(&self) -> Result<()> {
        let conn = Connection::open(&self.db_path)?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS pages (
                id INTEGER PRIMARY KEY,
                url TEXT NOT NULL,
                title TEXT NOT NULL,
                content TEXT,
                visited_at TEXT NOT NULL,
                embedding BLOB,
                profile_id INTEGER,
                UNIQUE(url, profile_id)
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS notes (
                id INTEGER PRIMARY KEY,
                page_id INTEGER NOT NULL,
                content TEXT NOT NULL,
                created_at TEXT NOT NULL,
                FOREIGN KEY (page_id) REFERENCES pages(id) ON DELETE CASCADE
            )",
            [],
        )?;

        // Create indexes for faster searches
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_pages_url ON pages(url)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_pages_profile ON pages(profile_id)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_pages_visited ON pages(visited_at)",
            [],
        )?;

        Ok(())
    }

    /// Add a page to the knowledge graph
    /// Returns None if in incognito mode (page not saved)
    /// Returns Some(id) if page was saved successfully
    pub fn add_page(&self, page: &Page, profile_id: i64) -> Result<Option<i64>> {
        // Check incognito mode - if active for this profile, don't save anything
        if PrivacyManager::is_incognito(profile_id) {
            return Ok(None);
        }

        let conn = Connection::open(&self.db_path)?;
        let now = chrono_now();

        // Use INSERT OR REPLACE to update if URL already exists for this profile
        conn.execute(
            "INSERT OR REPLACE INTO pages (url, title, content, visited_at, profile_id)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![page.url, page.title, page.content, now, profile_id],
        )?;

        Ok(Some(conn.last_insert_rowid()))
    }

    /// Get a page by URL for a specific profile
    pub fn get_page_by_url(&self, url: &str, profile_id: i64) -> Result<Option<Page>> {
        let conn = Connection::open(&self.db_path)?;
        let mut stmt = conn.prepare(
            "SELECT id, url, title, content, visited_at, embedding, profile_id
             FROM pages WHERE url = ?1 AND profile_id = ?2"
        )?;

        let mut rows = stmt.query(params![url, profile_id])?;

        if let Some(row) = rows.next()? {
            Ok(Some(Page {
                id: Some(row.get(0)?),
                url: row.get(1)?,
                title: row.get(2)?,
                content: row.get(3)?,
                visited_at: row.get(4)?,
                embedding: None, // Skip blob for now
                profile_id: row.get(6)?,
            }))
        } else {
            Ok(None)
        }
    }

    /// Search pages by text query within a profile
    pub fn search_pages(&self, query: &str, profile_id: i64, limit: i64) -> Result<Vec<SearchResult>> {
        let conn = Connection::open(&self.db_path)?;
        let pattern = format!("%{}%", query.to_lowercase());

        let mut stmt = conn.prepare(
            "SELECT id, url, title, content, visited_at, profile_id
             FROM pages
             WHERE profile_id = ?1
               AND (LOWER(title) LIKE ?2 OR LOWER(content) LIKE ?2 OR LOWER(url) LIKE ?2)
             ORDER BY visited_at DESC
             LIMIT ?3"
        )?;

        let results = stmt.query_map(params![profile_id, pattern, limit], |row| {
            let content: String = row.get::<_, Option<String>>(3)?.unwrap_or_default();
            let title: String = row.get(2)?;

            // Create a snippet from content or title
            let snippet = create_snippet(&content, query, 150)
                .unwrap_or_else(|| title.chars().take(150).collect());

            Ok(SearchResult {
                page: Page {
                    id: Some(row.get(0)?),
                    url: row.get(1)?,
                    title,
                    content,
                    visited_at: row.get(4)?,
                    embedding: None,
                    profile_id: row.get(5)?,
                },
                relevance: 1.0, // TODO: Implement proper relevance scoring
                snippet,
            })
        })?;

        results.collect()
    }

    /// Get all pages for a profile (for semantic search indexing)
    pub fn get_all_pages(&self, profile_id: i64) -> Result<Vec<Page>> {
        let conn = Connection::open(&self.db_path)?;
        let mut stmt = conn.prepare(
            "SELECT id, url, title, content, visited_at, profile_id
             FROM pages WHERE profile_id = ?1 ORDER BY visited_at DESC"
        )?;

        let pages = stmt.query_map(params![profile_id], |row| {
            Ok(Page {
                id: Some(row.get(0)?),
                url: row.get(1)?,
                title: row.get(2)?,
                content: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                visited_at: row.get(4)?,
                embedding: None,
                profile_id: row.get(5)?,
            })
        })?;

        pages.collect()
    }

    /// Add a note to a page
    pub fn add_note(&self, page_id: i64, content: &str, profile_id: i64) -> Result<Option<i64>> {
        // Check incognito mode for this profile
        if PrivacyManager::is_incognito(profile_id) {
            return Ok(None);
        }

        let conn = Connection::open(&self.db_path)?;
        let now = chrono_now();

        conn.execute(
            "INSERT INTO notes (page_id, content, created_at) VALUES (?1, ?2, ?3)",
            params![page_id, content, now],
        )?;

        Ok(Some(conn.last_insert_rowid()))
    }

    /// Get notes for a page
    pub fn get_notes_for_page(&self, page_id: i64) -> Result<Vec<Note>> {
        let conn = Connection::open(&self.db_path)?;
        let mut stmt = conn.prepare(
            "SELECT id, page_id, content, created_at FROM notes WHERE page_id = ?1 ORDER BY created_at DESC"
        )?;

        let notes = stmt.query_map(params![page_id], |row| {
            Ok(Note {
                id: Some(row.get(0)?),
                page_id: row.get(1)?,
                content: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?;

        notes.collect()
    }

    /// Delete a note
    pub fn delete_note(&self, note_id: i64) -> Result<bool> {
        let conn = Connection::open(&self.db_path)?;
        let affected = conn.execute("DELETE FROM notes WHERE id = ?1", params![note_id])?;
        Ok(affected > 0)
    }

    /// Update page embedding (for semantic search)
    pub fn update_embedding(&self, page_id: i64, embedding: &[f32], profile_id: i64) -> Result<()> {
        if PrivacyManager::is_incognito(profile_id) {
            return Ok(());
        }

        let conn = Connection::open(&self.db_path)?;
        let embedding_bytes: Vec<u8> = embedding
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();

        conn.execute(
            "UPDATE pages SET embedding = ?1 WHERE id = ?2",
            params![embedding_bytes, page_id],
        )?;

        Ok(())
    }
}

/// Create a snippet around the search query
fn create_snippet(content: &str, query: &str, max_len: usize) -> Option<String> {
    let content_lower = content.to_lowercase();
    let query_lower = query.to_lowercase();

    if let Some(pos) = content_lower.find(&query_lower) {
        let start = pos.saturating_sub(max_len / 2);
        let end = (pos + query.len() + max_len / 2).min(content.len());

        let snippet: String = content[start..end].to_string();
        let prefix = if start > 0 { "..." } else { "" };
        let suffix = if end < content.len() { "..." } else { "" };

        Some(format!("{}{}{}", prefix, snippet.trim(), suffix))
    } else {
        None
    }
}

/// Get current timestamp as string
fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", duration.as_secs())
}
