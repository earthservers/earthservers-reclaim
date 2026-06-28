// Bookmark management for Earth Reclaim
// Full bookmark system with folders, tags, and import/export

use rusqlite::{Connection, Result, params};
use serde::{Deserialize, Serialize};

fn chrono_now() -> String {
    chrono::Utc::now().to_rfc3339()
}

// ==================== At-rest encryption ====================
// Bookmarks' sensitive fields (title, url, tags, notes) are stored as AES-256-GCM
// ciphertext, transparently keyed to THIS device (machine-uid) — the same model
// as multimedia playlists. No password is needed to read them on-device, but the
// DB file reveals nothing in plaintext. Reads fall back to the raw value for any
// legacy plaintext rows written before encryption, so no migration is required.

fn bookmark_key() -> String {
    crate::multimedia::local_data_secret()
}

/// Legacy machine-id bookmark key — only to decrypt + migrate pre-keyring rows.
fn legacy_bookmark_key() -> String {
    let machine = machine_uid::get().unwrap_or_else(|_| "earthservers-default-device".to_string());
    format!("EarthBrowser::bookmark::{}", machine)
}

fn enc(s: &str) -> String {
    crate::multimedia::encrypt_data(s, &bookmark_key()).unwrap_or_else(|_| s.to_string())
}

fn dec(s: &str) -> String {
    crate::multimedia::decrypt_data(s, &bookmark_key())
        .or_else(|_| crate::multimedia::decrypt_data(s, &legacy_bookmark_key()))
        .unwrap_or_else(|_| s.to_string())
}

fn dec_opt(s: Option<String>) -> Option<String> {
    s.map(|v| dec(&v))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bookmark {
    pub id: i64,
    pub profile_id: i64,
    pub title: String,
    pub url: String,
    pub favicon: Option<String>,
    pub folder_id: Option<i64>,
    pub folder_name: Option<String>,
    pub tags: Vec<String>,
    pub notes: Option<String>,
    pub position: i32,
    /// Where the bookmark shows: "toolbar" | "list" | "private".
    pub location: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookmarkFolder {
    pub id: i64,
    pub profile_id: i64,
    pub name: String,
    pub parent_id: Option<i64>,
    pub position: i32,
    pub created_at: String,
    pub bookmark_count: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBookmarkRequest {
    pub profile_id: i64,
    pub title: String,
    pub url: String,
    pub folder_id: Option<i64>,
    pub tags: Vec<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookmarkExport {
    pub version: i32,
    pub exported_at: String,
    pub bookmarks: Vec<BookmarkExportItem>,
    pub folders: Vec<FolderExportItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookmarkExportItem {
    pub title: String,
    pub url: String,
    pub folder: Option<String>,
    pub tags: Vec<String>,
    pub notes: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderExportItem {
    pub name: String,
    pub parent: Option<String>,
}

pub struct BookmarkManager {
    db_path: String,
}

impl BookmarkManager {
    pub fn new(db_path: String) -> Self {
        BookmarkManager { db_path }
    }

    // ---- private-bookmarks password (Argon2id, PER PROFILE) ----
    //
    // Each profile has its OWN private-bookmarks password (keyed by profile_id),
    // so it's independent across profiles and is removed when the profile is wiped
    // or deleted. Earlier versions used a single global row shared by all profiles;
    // `ensure_private_auth_table` migrates that away. The bookmarks themselves are
    // encrypted with a device key (not this password), so dropping the old shared
    // gate loses no data — it just resets to "no password" until one is set again.

    fn ensure_private_auth_table(conn: &Connection) -> Result<(), String> {
        let exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='private_bookmarks_auth'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if exists > 0 {
            // Legacy global schema has an `id` column and no `profile_id`.
            let has_profile_id = conn
                .prepare("PRAGMA table_info(private_bookmarks_auth)")
                .and_then(|mut s| {
                    let cols: Vec<String> = s
                        .query_map([], |r| r.get::<_, String>(1))?
                        .filter_map(|r| r.ok())
                        .collect();
                    Ok(cols.iter().any(|c| c == "profile_id"))
                })
                .unwrap_or(false);
            if !has_profile_id {
                conn.execute("DROP TABLE private_bookmarks_auth", [])
                    .map_err(|e| e.to_string())?;
            }
        }
        conn.execute(
            "CREATE TABLE IF NOT EXISTS private_bookmarks_auth (profile_id INTEGER PRIMARY KEY, hash TEXT NOT NULL)",
            [],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn set_private_password(&self, profile_id: i64, password: &str) -> Result<(), String> {
        use argon2::password_hash::{rand_core::OsRng, PasswordHasher, SaltString};
        use argon2::Argon2;
        let conn = Connection::open(&self.db_path).map_err(|e| e.to_string())?;
        Self::ensure_private_auth_table(&conn)?;
        let salt = SaltString::generate(&mut OsRng);
        let hash = Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| e.to_string())?
            .to_string();
        conn.execute(
            "INSERT OR REPLACE INTO private_bookmarks_auth (profile_id, hash) VALUES (?1, ?2)",
            params![profile_id, hash],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn has_private_password(&self, profile_id: i64) -> bool {
        Connection::open(&self.db_path)
            .ok()
            .and_then(|conn| {
                let _ = Self::ensure_private_auth_table(&conn);
                conn.query_row(
                    "SELECT COUNT(*) FROM private_bookmarks_auth WHERE profile_id = ?1",
                    params![profile_id],
                    |r| r.get::<_, i64>(0),
                )
                .ok()
            })
            .map(|n| n > 0)
            .unwrap_or(false)
    }

    pub fn verify_private_password(&self, profile_id: i64, password: &str) -> bool {
        let stored: Option<String> = Connection::open(&self.db_path).ok().and_then(|conn| {
            let _ = Self::ensure_private_auth_table(&conn);
            conn.query_row(
                "SELECT hash FROM private_bookmarks_auth WHERE profile_id = ?1",
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
            None => true, // no password set => treat as unlocked
        }
    }

    /// Add a new bookmark
    #[allow(clippy::too_many_arguments)]
    pub fn add_bookmark(
        &self,
        profile_id: i64,
        title: &str,
        url: &str,
        folder_id: Option<i64>,
        tags: Vec<String>,
        notes: Option<&str>,
        location: &str,
    ) -> Result<Bookmark> {
        let conn = Connection::open(&self.db_path)?;
        let now = chrono_now();
        let tags_json = serde_json::to_string(&tags).unwrap_or_else(|_| "[]".to_string());

        // Get max position
        let max_pos: i32 = conn
            .query_row(
                "SELECT COALESCE(MAX(position), -1) FROM bookmarks WHERE profile_id = ?1",
                params![profile_id],
                |row| row.get(0),
            )
            .unwrap_or(-1);

        conn.execute(
            "INSERT INTO bookmarks (profile_id, title, url, folder_id, tags, notes, position, location, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
            params![profile_id, enc(title), enc(url), folder_id, enc(&tags_json), notes.map(enc), max_pos + 1, location, now],
        )?;

        let id = conn.last_insert_rowid();

        Ok(Bookmark {
            id,
            profile_id,
            title: title.to_string(),
            url: url.to_string(),
            favicon: None,
            folder_id,
            folder_name: None,
            tags,
            notes: notes.map(String::from),
            position: max_pos + 1,
            location: location.to_string(),
            created_at: now.clone(),
            updated_at: now,
        })
    }

    /// Delete a bookmark
    pub fn delete_bookmark(&self, bookmark_id: i64) -> Result<()> {
        let conn = Connection::open(&self.db_path)?;
        conn.execute("DELETE FROM bookmarks WHERE id = ?1", params![bookmark_id])?;
        Ok(())
    }

    /// Get all bookmarks for a profile
    pub fn get_all_bookmarks(&self, profile_id: i64) -> Result<Vec<Bookmark>> {
        let conn = Connection::open(&self.db_path)?;
        let mut stmt = conn.prepare(
            "SELECT b.id, b.profile_id, b.title, b.url, b.favicon, b.folder_id, f.name as folder_name,
                    b.tags, b.notes, b.position, b.created_at, b.updated_at, b.location
             FROM bookmarks b
             LEFT JOIN bookmark_folders f ON b.folder_id = f.id
             WHERE b.profile_id = ?1
             ORDER BY b.position ASC"
        )?;

        let bookmarks = stmt.query_map(params![profile_id], |row| {
            let tags_str: String = row.get::<_, Option<String>>(7)?.unwrap_or_else(|| "[]".to_string());
            let tags: Vec<String> = serde_json::from_str(&dec(&tags_str)).unwrap_or_default();

            Ok(Bookmark {
                id: row.get(0)?,
                profile_id: row.get(1)?,
                title: dec(&row.get::<_, String>(2)?),
                url: dec(&row.get::<_, String>(3)?),
                favicon: row.get(4)?,
                folder_id: row.get(5)?,
                folder_name: row.get(6)?,
                tags,
                notes: dec_opt(row.get(8)?),
                position: row.get(9)?,
                created_at: row.get(10)?,
                updated_at: row.get(11)?,
                location: row.get(12)?,
            })
        })?;

        bookmarks.collect()
    }

    /// Get bookmarks by folder
    pub fn get_bookmarks_by_folder(&self, profile_id: i64, folder_id: Option<i64>) -> Result<Vec<Bookmark>> {
        let conn = Connection::open(&self.db_path)?;

        let mut stmt = if folder_id.is_some() {
            conn.prepare(
                "SELECT b.id, b.profile_id, b.title, b.url, b.favicon, b.folder_id, f.name as folder_name,
                        b.tags, b.notes, b.position, b.created_at, b.updated_at, b.location
                 FROM bookmarks b
                 LEFT JOIN bookmark_folders f ON b.folder_id = f.id
                 WHERE b.profile_id = ?1 AND b.folder_id = ?2
                 ORDER BY b.position ASC"
            )?
        } else {
            conn.prepare(
                "SELECT b.id, b.profile_id, b.title, b.url, b.favicon, b.folder_id, NULL as folder_name,
                        b.tags, b.notes, b.position, b.created_at, b.updated_at, b.location
                 FROM bookmarks b
                 WHERE b.profile_id = ?1 AND b.folder_id IS NULL
                 ORDER BY b.position ASC"
            )?
        };

        // Helper closure to map row to Bookmark
        let map_row = |row: &rusqlite::Row| -> rusqlite::Result<Bookmark> {
            let tags_str: String = row.get::<_, Option<String>>(7)?.unwrap_or_else(|| "[]".to_string());
            let tags: Vec<String> = serde_json::from_str(&dec(&tags_str)).unwrap_or_default();

            Ok(Bookmark {
                id: row.get(0)?,
                profile_id: row.get(1)?,
                title: dec(&row.get::<_, String>(2)?),
                url: dec(&row.get::<_, String>(3)?),
                favicon: row.get(4)?,
                folder_id: row.get(5)?,
                folder_name: row.get(6)?,
                tags,
                notes: dec_opt(row.get(8)?),
                position: row.get(9)?,
                created_at: row.get(10)?,
                updated_at: row.get(11)?,
                location: row.get(12)?,
            })
        };

        if let Some(fid) = folder_id {
            stmt.query_map(params![profile_id, fid], map_row)?
                .collect()
        } else {
            stmt.query_map(params![profile_id], map_row)?
                .collect()
        }
    }

    /// Search bookmarks. Fields are encrypted at rest, so we can't filter with
    /// SQL `LIKE` — decrypt via `get_all_bookmarks` and match in Rust instead.
    pub fn search_bookmarks(&self, profile_id: i64, query: &str) -> Result<Vec<Bookmark>> {
        let q = query.to_lowercase();
        let all = self.get_all_bookmarks(profile_id)?;
        Ok(all
            .into_iter()
            .filter(|b| {
                b.title.to_lowercase().contains(&q)
                    || b.url.to_lowercase().contains(&q)
                    || b.tags.iter().any(|t| t.to_lowercase().contains(&q))
                    || b.notes.as_deref().map(|n| n.to_lowercase().contains(&q)).unwrap_or(false)
            })
            .collect())
    }

    /// Update a bookmark
    #[allow(clippy::too_many_arguments)]
    pub fn update_bookmark(
        &self,
        bookmark_id: i64,
        title: Option<&str>,
        url: Option<&str>,
        folder_id: Option<Option<i64>>,
        tags: Option<Vec<String>>,
        notes: Option<Option<&str>>,
        favicon: Option<&str>,
        location: Option<&str>,
    ) -> Result<Bookmark> {
        let conn = Connection::open(&self.db_path)?;
        let now = chrono_now();

        // Get current bookmark
        let current = self.get_bookmark(bookmark_id)?;

        let new_title = title.unwrap_or(&current.title);
        let new_url = url.unwrap_or(&current.url);
        let new_folder_id = folder_id.unwrap_or(current.folder_id);
        let new_tags = tags.unwrap_or(current.tags.clone());
        let new_notes = notes.map(|n| n.map(String::from)).unwrap_or(current.notes.clone());
        let new_favicon = favicon.map(String::from).or(current.favicon.clone());
        let new_location = location.unwrap_or(&current.location);

        let tags_json = serde_json::to_string(&new_tags).unwrap_or_else(|_| "[]".to_string());

        // current came from get_bookmark (already decrypted); re-encrypt on write.
        conn.execute(
            "UPDATE bookmarks SET title = ?1, url = ?2, folder_id = ?3, tags = ?4, notes = ?5, favicon = ?6, location = ?7, updated_at = ?8
             WHERE id = ?9",
            params![enc(new_title), enc(new_url), new_folder_id, enc(&tags_json), new_notes.as_deref().map(enc), new_favicon, new_location, now, bookmark_id],
        )?;

        self.get_bookmark(bookmark_id)
    }

    /// Get a single bookmark
    pub fn get_bookmark(&self, bookmark_id: i64) -> Result<Bookmark> {
        let conn = Connection::open(&self.db_path)?;
        conn.query_row(
            "SELECT b.id, b.profile_id, b.title, b.url, b.favicon, b.folder_id, f.name as folder_name,
                    b.tags, b.notes, b.position, b.created_at, b.updated_at, b.location
             FROM bookmarks b
             LEFT JOIN bookmark_folders f ON b.folder_id = f.id
             WHERE b.id = ?1",
            params![bookmark_id],
            |row| {
                let tags_str: String = row.get::<_, Option<String>>(7)?.unwrap_or_else(|| "[]".to_string());
                let tags: Vec<String> = serde_json::from_str(&dec(&tags_str)).unwrap_or_default();

                Ok(Bookmark {
                    id: row.get(0)?,
                    profile_id: row.get(1)?,
                    title: dec(&row.get::<_, String>(2)?),
                    url: dec(&row.get::<_, String>(3)?),
                    favicon: row.get(4)?,
                    folder_id: row.get(5)?,
                    folder_name: row.get(6)?,
                    tags,
                    notes: dec_opt(row.get(8)?),
                    position: row.get(9)?,
                    created_at: row.get(10)?,
                    updated_at: row.get(11)?,
                    location: row.get(12)?,
                })
            },
        )
    }

    /// Check if URL is bookmarked. URLs are encrypted at rest (random nonce), so
    /// an equality match in SQL is impossible — decrypt and compare in Rust.
    pub fn is_bookmarked(&self, profile_id: i64, url: &str) -> Result<Option<i64>> {
        let all = self.get_all_bookmarks(profile_id)?;
        Ok(all.into_iter().find(|b| b.url == url).map(|b| b.id))
    }

    // ==================== Folder Operations ====================

    /// Create a folder
    pub fn create_folder(&self, profile_id: i64, name: &str, parent_id: Option<i64>) -> Result<BookmarkFolder> {
        let conn = Connection::open(&self.db_path)?;
        let now = chrono_now();

        let max_pos: i32 = conn
            .query_row(
                "SELECT COALESCE(MAX(position), -1) FROM bookmark_folders WHERE profile_id = ?1 AND parent_id IS ?2",
                params![profile_id, parent_id],
                |row| row.get(0),
            )
            .unwrap_or(-1);

        conn.execute(
            "INSERT INTO bookmark_folders (profile_id, name, parent_id, position, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![profile_id, name, parent_id, max_pos + 1, now],
        )?;

        let id = conn.last_insert_rowid();

        Ok(BookmarkFolder {
            id,
            profile_id,
            name: name.to_string(),
            parent_id,
            position: max_pos + 1,
            created_at: now,
            bookmark_count: Some(0),
        })
    }

    /// Get all folders for a profile
    pub fn get_all_folders(&self, profile_id: i64) -> Result<Vec<BookmarkFolder>> {
        let conn = Connection::open(&self.db_path)?;
        let mut stmt = conn.prepare(
            "SELECT f.id, f.profile_id, f.name, f.parent_id, f.position, f.created_at,
                    (SELECT COUNT(*) FROM bookmarks WHERE folder_id = f.id) as bookmark_count
             FROM bookmark_folders f
             WHERE f.profile_id = ?1
             ORDER BY f.position ASC"
        )?;

        let folders = stmt.query_map(params![profile_id], |row| {
            Ok(BookmarkFolder {
                id: row.get(0)?,
                profile_id: row.get(1)?,
                name: row.get(2)?,
                parent_id: row.get(3)?,
                position: row.get(4)?,
                created_at: row.get(5)?,
                bookmark_count: row.get(6)?,
            })
        })?;

        folders.collect()
    }

    /// Delete a folder (moves bookmarks to root)
    pub fn delete_folder(&self, folder_id: i64) -> Result<()> {
        let conn = Connection::open(&self.db_path)?;

        // Move bookmarks to root
        conn.execute(
            "UPDATE bookmarks SET folder_id = NULL WHERE folder_id = ?1",
            params![folder_id],
        )?;

        // Move child folders to root
        conn.execute(
            "UPDATE bookmark_folders SET parent_id = NULL WHERE parent_id = ?1",
            params![folder_id],
        )?;

        // Delete folder
        conn.execute(
            "DELETE FROM bookmark_folders WHERE id = ?1",
            params![folder_id],
        )?;

        Ok(())
    }

    /// Rename a folder
    pub fn rename_folder(&self, folder_id: i64, name: &str) -> Result<BookmarkFolder> {
        let conn = Connection::open(&self.db_path)?;

        conn.execute(
            "UPDATE bookmark_folders SET name = ?1 WHERE id = ?2",
            params![name, folder_id],
        )?;

        conn.query_row(
            "SELECT f.id, f.profile_id, f.name, f.parent_id, f.position, f.created_at,
                    (SELECT COUNT(*) FROM bookmarks WHERE folder_id = f.id) as bookmark_count
             FROM bookmark_folders f WHERE f.id = ?1",
            params![folder_id],
            |row| {
                Ok(BookmarkFolder {
                    id: row.get(0)?,
                    profile_id: row.get(1)?,
                    name: row.get(2)?,
                    parent_id: row.get(3)?,
                    position: row.get(4)?,
                    created_at: row.get(5)?,
                    bookmark_count: row.get(6)?,
                })
            },
        )
    }

    // ==================== Import/Export ====================

    /// Export bookmarks as JSON
    pub fn export_bookmarks_json(&self, profile_id: i64) -> Result<String> {
        let bookmarks = self.get_all_bookmarks(profile_id)?;
        let folders = self.get_all_folders(profile_id)?;

        let export = BookmarkExport {
            version: 1,
            exported_at: chrono_now(),
            bookmarks: bookmarks.into_iter().map(|b| BookmarkExportItem {
                title: b.title,
                url: b.url,
                folder: b.folder_name,
                tags: b.tags,
                notes: b.notes,
                created_at: b.created_at,
            }).collect(),
            folders: folders.into_iter().map(|f| {
                // Get parent name if exists
                let parent_name = if let Some(pid) = f.parent_id {
                    self.get_folder_name(pid).ok()
                } else {
                    None
                };
                FolderExportItem {
                    name: f.name,
                    parent: parent_name,
                }
            }).collect(),
        };

        Ok(serde_json::to_string_pretty(&export).unwrap_or_else(|_| "{}".to_string()))
    }

    fn get_folder_name(&self, folder_id: i64) -> Result<String> {
        let conn = Connection::open(&self.db_path)?;
        conn.query_row(
            "SELECT name FROM bookmark_folders WHERE id = ?1",
            params![folder_id],
            |row| row.get(0),
        )
    }

    /// Export bookmarks as HTML (Netscape Bookmark format)
    pub fn export_bookmarks_html(&self, profile_id: i64) -> Result<String> {
        let bookmarks = self.get_all_bookmarks(profile_id)?;
        let folders = self.get_all_folders(profile_id)?;

        let mut html = String::from(r#"<!DOCTYPE NETSCAPE-Bookmark-file-1>
<!-- This is an automatically generated file. -->
<META HTTP-EQUIV="Content-Type" CONTENT="text/html; charset=UTF-8">
<TITLE>Bookmarks</TITLE>
<H1>Bookmarks</H1>
<DL><p>
"#);

        // Group bookmarks by folder
        let mut folder_map: std::collections::HashMap<Option<i64>, Vec<&Bookmark>> = std::collections::HashMap::new();
        for bookmark in &bookmarks {
            folder_map.entry(bookmark.folder_id).or_insert_with(Vec::new).push(bookmark);
        }

        // Output folders and their bookmarks
        for folder in &folders {
            html.push_str(&format!("    <DT><H3>{}</H3>\n    <DL><p>\n", folder.name));
            if let Some(folder_bookmarks) = folder_map.get(&Some(folder.id)) {
                for bookmark in folder_bookmarks {
                    html.push_str(&format!(
                        "        <DT><A HREF=\"{}\">{}</A>\n",
                        bookmark.url, bookmark.title
                    ));
                }
            }
            html.push_str("    </DL><p>\n");
        }

        // Output root bookmarks
        if let Some(root_bookmarks) = folder_map.get(&None) {
            for bookmark in root_bookmarks {
                html.push_str(&format!(
                    "    <DT><A HREF=\"{}\">{}</A>\n",
                    bookmark.url, bookmark.title
                ));
            }
        }

        html.push_str("</DL><p>\n");

        Ok(html)
    }

    /// Import bookmarks from JSON
    pub fn import_bookmarks_json(&self, profile_id: i64, data: &str) -> Result<i32> {
        let export: BookmarkExport = serde_json::from_str(data)
            .map_err(|e| rusqlite::Error::InvalidParameterName(format!("Invalid JSON: {}", e)))?;

        let mut imported = 0;

        // Create folders first
        let mut folder_name_to_id: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
        for folder in &export.folders {
            let parent_id = folder.parent.as_ref().and_then(|p| folder_name_to_id.get(p).copied());
            if let Ok(created) = self.create_folder(profile_id, &folder.name, parent_id) {
                folder_name_to_id.insert(folder.name.clone(), created.id);
            }
        }

        // Import bookmarks
        for bookmark in &export.bookmarks {
            let folder_id = bookmark.folder.as_ref().and_then(|f| folder_name_to_id.get(f).copied());
            if self.add_bookmark(
                profile_id,
                &bookmark.title,
                &bookmark.url,
                folder_id,
                bookmark.tags.clone(),
                bookmark.notes.as_deref(),
                "toolbar",
            ).is_ok() {
                imported += 1;
            }
        }

        Ok(imported)
    }

    /// Import bookmarks from HTML (basic Netscape format parsing)
    pub fn import_bookmarks_html(&self, profile_id: i64, data: &str) -> Result<i32> {
        let mut imported = 0;
        let mut current_folder: Option<String> = None;
        let mut folder_name_to_id: std::collections::HashMap<String, i64> = std::collections::HashMap::new();

        for line in data.lines() {
            let trimmed = line.trim();

            // Check for folder
            if trimmed.starts_with("<DT><H3") || trimmed.starts_with("<dt><h3") {
                if let Some(start) = trimmed.find('>') {
                    if let Some(end) = trimmed[start+1..].find('<') {
                        let folder_name = &trimmed[start+1..start+1+end];
                        if let Ok(folder) = self.create_folder(profile_id, folder_name, None) {
                            folder_name_to_id.insert(folder_name.to_string(), folder.id);
                            current_folder = Some(folder_name.to_string());
                        }
                    }
                }
            }

            // Check for bookmark link
            if (trimmed.starts_with("<DT><A") || trimmed.starts_with("<dt><a")) && trimmed.contains("HREF=") {
                // Extract URL
                let href_pattern = if trimmed.contains("HREF=\"") { "HREF=\"" } else { "href=\"" };
                if let Some(href_start) = trimmed.find(href_pattern) {
                    let url_start = href_start + href_pattern.len();
                    if let Some(url_end) = trimmed[url_start..].find('"') {
                        let url = &trimmed[url_start..url_start+url_end];

                        // Extract title
                        if let Some(title_start) = trimmed.find('>') {
                            if let Some(title_end) = trimmed[title_start+1..].find('<') {
                                let title = &trimmed[title_start+1..title_start+1+title_end];

                                let folder_id = current_folder.as_ref()
                                    .and_then(|f| folder_name_to_id.get(f).copied());

                                if self.add_bookmark(
                                    profile_id,
                                    title,
                                    url,
                                    folder_id,
                                    vec![],
                                    None,
                                    "toolbar",
                                ).is_ok() {
                                    imported += 1;
                                }
                            }
                        }
                    }
                }
            }

            // Check for folder close
            if trimmed == "</DL><p>" || trimmed == "</dl><p>" {
                current_folder = None;
            }
        }

        Ok(imported)
    }

    /// Seed default bookmarks for a new profile
    /// Only runs if the profile has no bookmarks
    pub fn seed_default_bookmarks(&self, profile_id: i64) -> Result<i32> {
        let conn = Connection::open(&self.db_path)?;

        // Check if bookmarks already exist for this profile
        let existing_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM bookmarks WHERE profile_id = ?1",
            params![profile_id],
            |row| row.get(0),
        )?;

        if existing_count > 0 {
            return Ok(0); // Already has bookmarks
        }

        // Default bookmarks to seed
        let default_bookmarks = vec![
            ("EarthServers", "https://earthservers.net", vec!["home", "earthservers"]),
            ("EarthServers Social", "https://social.earthservers.net", vec!["social", "earthservers"]),
        ];

        let mut seeded = 0;
        for (title, url, tags) in default_bookmarks {
            if self.add_bookmark(
                profile_id,
                title,
                url,
                None, // No folder
                tags.into_iter().map(String::from).collect(),
                None, // No notes
                "toolbar",
            ).is_ok() {
                seeded += 1;
            }
        }

        Ok(seeded)
    }
}
