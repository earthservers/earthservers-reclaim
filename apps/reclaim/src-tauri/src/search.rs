// Search engine functionality for EarthSearch
// Manages domain whitelists and search within curated domains

use rusqlite::{Connection, Result, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Domain {
    pub id: Option<i64>,
    pub url: String,
    pub category: String,
    pub trust_score: f64,
    pub added_date: String,
    pub metadata: Option<String>,
    pub profile_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainList {
    pub id: Option<i64>,
    pub name: String,
    pub description: Option<String>,
    pub author: Option<String>,
    pub version: String,
    pub created_at: String,
    pub profile_id: Option<i64>,
    pub domain_count: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub url: String,
    pub title: String,
    pub snippet: String,
    pub relevance: f64,
    pub domain_trust: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainStats {
    pub total_domains: i64,
    pub total_lists: i64,
    pub categories: Vec<CategoryCount>,
    pub avg_trust_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryCount {
    pub category: String,
    pub count: i64,
}

pub struct SearchManager {
    db_path: String,
}

impl SearchManager {
    pub fn new(db_path: String) -> Self {
        SearchManager { db_path }
    }

    /// Initialize search tables
    pub fn init(&self) -> Result<()> {
        let conn = Connection::open(&self.db_path)?;

        // Domains table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS domains (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url TEXT NOT NULL,
                category TEXT NOT NULL,
                trust_score REAL NOT NULL DEFAULT 0.5,
                added_date TEXT NOT NULL,
                metadata TEXT,
                profile_id INTEGER,
                UNIQUE(url, profile_id),
                FOREIGN KEY (profile_id) REFERENCES profiles(id) ON DELETE CASCADE
            )",
            [],
        )?;

        // Domain lists table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS domain_lists (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                description TEXT,
                author TEXT,
                version TEXT DEFAULT '1.0',
                created_at TEXT NOT NULL,
                profile_id INTEGER,
                UNIQUE(name, profile_id),
                FOREIGN KEY (profile_id) REFERENCES profiles(id) ON DELETE CASCADE
            )",
            [],
        )?;

        // List-domain associations
        conn.execute(
            "CREATE TABLE IF NOT EXISTS list_domains (
                list_id INTEGER NOT NULL,
                domain_id INTEGER NOT NULL,
                PRIMARY KEY (list_id, domain_id),
                FOREIGN KEY (list_id) REFERENCES domain_lists(id) ON DELETE CASCADE,
                FOREIGN KEY (domain_id) REFERENCES domains(id) ON DELETE CASCADE
            )",
            [],
        )?;

        // Domain ratings table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS domain_ratings (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                domain_id INTEGER NOT NULL,
                user_id TEXT NOT NULL,
                trust_rating INTEGER NOT NULL CHECK (trust_rating BETWEEN 1 AND 5),
                bias_rating INTEGER NOT NULL CHECK (bias_rating BETWEEN 1 AND 4),
                review_text TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT,
                helpful_count INTEGER DEFAULT 0,
                reported BOOLEAN DEFAULT FALSE,
                FOREIGN KEY (domain_id) REFERENCES domains(id) ON DELETE CASCADE,
                UNIQUE(domain_id, user_id)
            )",
            [],
        )?;

        // Aggregated ratings for quick lookup
        conn.execute(
            "CREATE TABLE IF NOT EXISTS domain_rating_aggregates (
                domain_id INTEGER PRIMARY KEY,
                avg_trust REAL NOT NULL DEFAULT 3.0,
                avg_bias REAL NOT NULL DEFAULT 2.5,
                avg_independence REAL NOT NULL DEFAULT 2.5,
                total_ratings INTEGER NOT NULL DEFAULT 0,
                trust_distribution TEXT,
                bias_distribution TEXT,
                independence_distribution TEXT,
                last_updated TEXT,
                FOREIGN KEY (domain_id) REFERENCES domains(id) ON DELETE CASCADE
            )",
            [],
        )?;

        // Migration: Add missing columns to existing tables
        // This handles existing databases that were created before these columns were added
        let _ = conn.execute(
            "ALTER TABLE domain_rating_aggregates ADD COLUMN avg_independence REAL NOT NULL DEFAULT 2.5",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE domain_rating_aggregates ADD COLUMN independence_distribution TEXT",
            [],
        );

        // Subdomain-specific ratings
        conn.execute(
            "CREATE TABLE IF NOT EXISTS subdomain_ratings (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                parent_domain_id INTEGER NOT NULL,
                subdomain TEXT NOT NULL,
                avg_trust REAL NOT NULL DEFAULT 3.0,
                avg_bias REAL NOT NULL DEFAULT 2.5,
                total_ratings INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY (parent_domain_id) REFERENCES domains(id) ON DELETE CASCADE,
                UNIQUE(parent_domain_id, subdomain)
            )",
            [],
        )?;

        // Rating categories for detailed breakdowns
        conn.execute(
            "CREATE TABLE IF NOT EXISTS rating_categories (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                domain_rating_id INTEGER NOT NULL,
                category TEXT NOT NULL,
                score INTEGER NOT NULL CHECK (score BETWEEN 1 AND 5),
                FOREIGN KEY (domain_rating_id) REFERENCES domain_ratings(id) ON DELETE CASCADE
            )",
            [],
        )?;

        // Indexes
        conn.execute("CREATE INDEX IF NOT EXISTS idx_domains_url ON domains(url)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_domains_category ON domains(category)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_domains_profile ON domains(profile_id)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_domain_lists_profile ON domain_lists(profile_id)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_domain_ratings_domain ON domain_ratings(domain_id)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_domain_ratings_user ON domain_ratings(user_id)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_subdomain_ratings_parent ON subdomain_ratings(parent_domain_id)", [])?;

        // ==================== Tabs System ====================

        // Tabs table (per profile)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS tabs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                profile_id INTEGER NOT NULL,
                title TEXT,
                url TEXT NOT NULL,
                favicon TEXT,
                position INTEGER NOT NULL,
                is_pinned INTEGER DEFAULT 0,
                is_active INTEGER DEFAULT 0,
                scroll_position INTEGER DEFAULT 0,
                created_at TEXT NOT NULL,
                last_accessed TEXT NOT NULL,
                FOREIGN KEY (profile_id) REFERENCES profiles(id) ON DELETE CASCADE
            )",
            [],
        )?;

        // Tab history (back/forward navigation per tab)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS tab_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                tab_id INTEGER NOT NULL,
                url TEXT NOT NULL,
                title TEXT,
                visited_at TEXT NOT NULL,
                position INTEGER NOT NULL,
                FOREIGN KEY (tab_id) REFERENCES tabs(id) ON DELETE CASCADE
            )",
            [],
        )?;

        // Tab indexes
        conn.execute("CREATE INDEX IF NOT EXISTS idx_tabs_profile ON tabs(profile_id)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_tabs_position ON tabs(profile_id, position)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_tab_history_tab ON tab_history(tab_id)", [])?;

        // ==================== Bookmarks System ====================

        // Bookmark folders
        conn.execute(
            "CREATE TABLE IF NOT EXISTS bookmark_folders (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                profile_id INTEGER NOT NULL,
                name TEXT NOT NULL,
                parent_id INTEGER,
                position INTEGER DEFAULT 0,
                created_at TEXT NOT NULL,
                FOREIGN KEY (profile_id) REFERENCES profiles(id) ON DELETE CASCADE,
                FOREIGN KEY (parent_id) REFERENCES bookmark_folders(id) ON DELETE CASCADE
            )",
            [],
        )?;

        // Bookmarks
        conn.execute(
            "CREATE TABLE IF NOT EXISTS bookmarks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                profile_id INTEGER NOT NULL,
                title TEXT NOT NULL,
                url TEXT NOT NULL,
                favicon TEXT,
                folder_id INTEGER,
                tags TEXT,
                notes TEXT,
                position INTEGER DEFAULT 0,
                location TEXT NOT NULL DEFAULT 'toolbar',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                FOREIGN KEY (profile_id) REFERENCES profiles(id) ON DELETE CASCADE,
                FOREIGN KEY (folder_id) REFERENCES bookmark_folders(id) ON DELETE SET NULL
            )",
            [],
        )?;

        // Migration: add the location column ('toolbar' | 'list' | 'private') to
        // databases created before it existed. Ignore the duplicate-column error.
        let _ = conn.execute("ALTER TABLE bookmarks ADD COLUMN location TEXT NOT NULL DEFAULT 'toolbar'", []);

        // Bookmark indexes
        conn.execute("CREATE INDEX IF NOT EXISTS idx_bookmarks_profile ON bookmarks(profile_id)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_bookmarks_folder ON bookmarks(folder_id)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_bookmark_folders_profile ON bookmark_folders(profile_id)", [])?;

        // ==================== Split View System ====================

        // Split view configuration (per profile)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS split_view_config (
                profile_id INTEGER PRIMARY KEY,
                layout TEXT NOT NULL DEFAULT 'single',
                pane_1_tab_id INTEGER,
                pane_2_tab_id INTEGER,
                pane_3_tab_id INTEGER,
                pane_4_tab_id INTEGER,
                active_pane INTEGER DEFAULT 1,
                pane_sizes TEXT,
                FOREIGN KEY (profile_id) REFERENCES profiles(id) ON DELETE CASCADE,
                FOREIGN KEY (pane_1_tab_id) REFERENCES tabs(id) ON DELETE SET NULL,
                FOREIGN KEY (pane_2_tab_id) REFERENCES tabs(id) ON DELETE SET NULL,
                FOREIGN KEY (pane_3_tab_id) REFERENCES tabs(id) ON DELETE SET NULL,
                FOREIGN KEY (pane_4_tab_id) REFERENCES tabs(id) ON DELETE SET NULL
            )",
            [],
        )?;

        // ==================== EarthMultiMedia System ====================

        // Multimedia privacy settings (per profile)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS multimedia_privacy (
                profile_id INTEGER PRIMARY KEY,
                history_enabled INTEGER DEFAULT 0,
                playlist_history_enabled INTEGER DEFAULT 0,
                require_password INTEGER DEFAULT 0,
                require_otp INTEGER DEFAULT 0,
                password_hash TEXT,
                otp_secret TEXT,
                auto_clear_history_days INTEGER,
                FOREIGN KEY (profile_id) REFERENCES profiles(id) ON DELETE CASCADE
            )",
            [],
        )?;

        // Multimedia history (only used if history_enabled)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS multimedia_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                profile_id INTEGER NOT NULL,
                media_id INTEGER,
                source TEXT NOT NULL,
                media_type TEXT NOT NULL,
                title TEXT,
                thumbnail TEXT,
                position INTEGER DEFAULT 0,
                duration INTEGER,
                played_at TEXT NOT NULL,
                encrypted INTEGER DEFAULT 0,
                FOREIGN KEY (profile_id) REFERENCES profiles(id) ON DELETE CASCADE
            )",
            [],
        )?;

        // Multimedia playlists
        conn.execute(
            "CREATE TABLE IF NOT EXISTS multimedia_playlists (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                profile_id INTEGER NOT NULL,
                name TEXT NOT NULL,
                description TEXT,
                thumbnail TEXT,
                is_encrypted INTEGER DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT,
                FOREIGN KEY (profile_id) REFERENCES profiles(id) ON DELETE CASCADE
            )",
            [],
        )?;

        // Playlist items
        conn.execute(
            "CREATE TABLE IF NOT EXISTS multimedia_playlist_items (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                playlist_id INTEGER NOT NULL,
                source TEXT NOT NULL,
                media_type TEXT NOT NULL,
                title TEXT,
                thumbnail TEXT,
                position INTEGER NOT NULL,
                added_at TEXT NOT NULL,
                FOREIGN KEY (playlist_id) REFERENCES multimedia_playlists(id) ON DELETE CASCADE
            )",
            [],
        )?;

        // Multimedia indexes
        conn.execute("CREATE INDEX IF NOT EXISTS idx_multimedia_history_profile ON multimedia_history(profile_id)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_multimedia_history_played ON multimedia_history(played_at DESC)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_multimedia_playlists_profile ON multimedia_playlists(profile_id)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_multimedia_playlist_items_playlist ON multimedia_playlist_items(playlist_id)", [])?;

        Ok(())
    }

    // ==================== Domain CRUD ====================

    /// Add a new domain
    pub fn add_domain(&self, domain: &Domain, profile_id: i64) -> Result<Domain> {
        let conn = Connection::open(&self.db_path)?;
        let now = chrono_now();

        conn.execute(
            "INSERT INTO domains (url, category, trust_score, added_date, metadata, profile_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                domain.url,
                domain.category,
                domain.trust_score,
                now,
                domain.metadata,
                profile_id
            ],
        )?;

        let id = conn.last_insert_rowid();
        Ok(Domain {
            id: Some(id),
            url: domain.url.clone(),
            category: domain.category.clone(),
            trust_score: domain.trust_score,
            added_date: now,
            metadata: domain.metadata.clone(),
            profile_id: Some(profile_id),
        })
    }

    /// Get all domains for a profile
    pub fn get_domains(&self, profile_id: i64) -> Result<Vec<Domain>> {
        let conn = Connection::open(&self.db_path)?;
        let mut stmt = conn.prepare(
            "SELECT id, url, category, trust_score, added_date, metadata, profile_id
             FROM domains WHERE profile_id = ?1 ORDER BY trust_score DESC, url ASC"
        )?;

        let domains = stmt.query_map(params![profile_id], |row| {
            Ok(Domain {
                id: Some(row.get(0)?),
                url: row.get(1)?,
                category: row.get(2)?,
                trust_score: row.get(3)?,
                added_date: row.get(4)?,
                metadata: row.get(5)?,
                profile_id: row.get(6)?,
            })
        })?;

        domains.collect()
    }

    /// Get domains by category
    pub fn get_domains_by_category(&self, profile_id: i64, category: &str) -> Result<Vec<Domain>> {
        let conn = Connection::open(&self.db_path)?;
        let mut stmt = conn.prepare(
            "SELECT id, url, category, trust_score, added_date, metadata, profile_id
             FROM domains WHERE profile_id = ?1 AND category = ?2 ORDER BY trust_score DESC"
        )?;

        let domains = stmt.query_map(params![profile_id, category], |row| {
            Ok(Domain {
                id: Some(row.get(0)?),
                url: row.get(1)?,
                category: row.get(2)?,
                trust_score: row.get(3)?,
                added_date: row.get(4)?,
                metadata: row.get(5)?,
                profile_id: row.get(6)?,
            })
        })?;

        domains.collect()
    }

    /// Update a domain
    pub fn update_domain(&self, domain: &Domain) -> Result<Domain> {
        let conn = Connection::open(&self.db_path)?;

        conn.execute(
            "UPDATE domains SET url = ?1, category = ?2, trust_score = ?3, metadata = ?4
             WHERE id = ?5",
            params![
                domain.url,
                domain.category,
                domain.trust_score,
                domain.metadata,
                domain.id
            ],
        )?;

        Ok(domain.clone())
    }

    /// Delete a domain
    pub fn delete_domain(&self, domain_id: i64, profile_id: i64) -> Result<bool> {
        let conn = Connection::open(&self.db_path)?;
        let affected = conn.execute(
            "DELETE FROM domains WHERE id = ?1 AND profile_id = ?2",
            params![domain_id, profile_id],
        )?;
        Ok(affected > 0)
    }

    /// Clear all domains for a profile (for force-reseeding)
    pub fn clear_all_domains(&self, profile_id: i64) -> Result<i64> {
        let conn = Connection::open(&self.db_path)?;

        // First delete list-domain associations
        conn.execute(
            "DELETE FROM list_domains WHERE list_id IN (SELECT id FROM domain_lists WHERE profile_id = ?1)",
            params![profile_id],
        )?;

        // Delete domain lists
        conn.execute(
            "DELETE FROM domain_lists WHERE profile_id = ?1",
            params![profile_id],
        )?;

        // Delete domains
        let affected = conn.execute(
            "DELETE FROM domains WHERE profile_id = ?1",
            params![profile_id],
        )?;

        println!("Cleared {} domains for profile {}", affected, profile_id);
        Ok(affected as i64)
    }

    /// Search domains by URL pattern
    pub fn search_domains(&self, profile_id: i64, query: &str) -> Result<Vec<Domain>> {
        let conn = Connection::open(&self.db_path)?;
        let pattern = format!("%{}%", query.to_lowercase());

        let mut stmt = conn.prepare(
            "SELECT id, url, category, trust_score, added_date, metadata, profile_id
             FROM domains
             WHERE profile_id = ?1 AND (LOWER(url) LIKE ?2 OR LOWER(category) LIKE ?2)
             ORDER BY trust_score DESC"
        )?;

        let domains = stmt.query_map(params![profile_id, pattern], |row| {
            Ok(Domain {
                id: Some(row.get(0)?),
                url: row.get(1)?,
                category: row.get(2)?,
                trust_score: row.get(3)?,
                added_date: row.get(4)?,
                metadata: row.get(5)?,
                profile_id: row.get(6)?,
            })
        })?;

        domains.collect()
    }

    // ==================== Domain Lists ====================

    /// Create a new domain list
    pub fn create_list(&self, list: &DomainList, profile_id: i64) -> Result<DomainList> {
        let conn = Connection::open(&self.db_path)?;
        let now = chrono_now();

        conn.execute(
            "INSERT INTO domain_lists (name, description, author, version, created_at, profile_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                list.name,
                list.description,
                list.author,
                list.version,
                now,
                profile_id
            ],
        )?;

        let id = conn.last_insert_rowid();
        Ok(DomainList {
            id: Some(id),
            name: list.name.clone(),
            description: list.description.clone(),
            author: list.author.clone(),
            version: list.version.clone(),
            created_at: now,
            profile_id: Some(profile_id),
            domain_count: Some(0),
        })
    }

    /// Get all domain lists for a profile
    pub fn get_lists(&self, profile_id: i64) -> Result<Vec<DomainList>> {
        let conn = Connection::open(&self.db_path)?;
        let mut stmt = conn.prepare(
            "SELECT dl.id, dl.name, dl.description, dl.author, dl.version, dl.created_at, dl.profile_id,
                    COUNT(ld.domain_id) as domain_count
             FROM domain_lists dl
             LEFT JOIN list_domains ld ON dl.id = ld.list_id
             WHERE dl.profile_id = ?1
             GROUP BY dl.id
             ORDER BY dl.name ASC"
        )?;

        let lists = stmt.query_map(params![profile_id], |row| {
            Ok(DomainList {
                id: Some(row.get(0)?),
                name: row.get(1)?,
                description: row.get(2)?,
                author: row.get(3)?,
                version: row.get(4)?,
                created_at: row.get(5)?,
                profile_id: row.get(6)?,
                domain_count: Some(row.get(7)?),
            })
        })?;

        lists.collect()
    }

    /// Add domain to a list
    pub fn add_domain_to_list(&self, list_id: i64, domain_id: i64) -> Result<bool> {
        let conn = Connection::open(&self.db_path)?;
        let result = conn.execute(
            "INSERT OR IGNORE INTO list_domains (list_id, domain_id) VALUES (?1, ?2)",
            params![list_id, domain_id],
        );
        Ok(result.is_ok())
    }

    /// Remove domain from a list
    pub fn remove_domain_from_list(&self, list_id: i64, domain_id: i64) -> Result<bool> {
        let conn = Connection::open(&self.db_path)?;
        let affected = conn.execute(
            "DELETE FROM list_domains WHERE list_id = ?1 AND domain_id = ?2",
            params![list_id, domain_id],
        )?;
        Ok(affected > 0)
    }

    /// Get domains in a list
    pub fn get_list_domains(&self, list_id: i64) -> Result<Vec<Domain>> {
        let conn = Connection::open(&self.db_path)?;
        let mut stmt = conn.prepare(
            "SELECT d.id, d.url, d.category, d.trust_score, d.added_date, d.metadata, d.profile_id
             FROM domains d
             INNER JOIN list_domains ld ON d.id = ld.domain_id
             WHERE ld.list_id = ?1
             ORDER BY d.trust_score DESC"
        )?;

        let domains = stmt.query_map(params![list_id], |row| {
            Ok(Domain {
                id: Some(row.get(0)?),
                url: row.get(1)?,
                category: row.get(2)?,
                trust_score: row.get(3)?,
                added_date: row.get(4)?,
                metadata: row.get(5)?,
                profile_id: row.get(6)?,
            })
        })?;

        domains.collect()
    }

    /// Delete a list
    pub fn delete_list(&self, list_id: i64, profile_id: i64) -> Result<bool> {
        let conn = Connection::open(&self.db_path)?;
        let affected = conn.execute(
            "DELETE FROM domain_lists WHERE id = ?1 AND profile_id = ?2",
            params![list_id, profile_id],
        )?;
        Ok(affected > 0)
    }

    // ==================== Import/Export ====================

    /// Export domains as JSON
    pub fn export_domains(&self, profile_id: i64) -> Result<String> {
        let domains = self.get_domains(profile_id)?;
        let export = serde_json::json!({
            "version": 1,
            "type": "earthservers-domains",
            "exported_at": chrono_now(),
            "domains": domains
        });
        Ok(serde_json::to_string_pretty(&export).unwrap_or_default())
    }

    /// Export a list with its domains
    pub fn export_list(&self, list_id: i64) -> Result<String> {
        let conn = Connection::open(&self.db_path)?;

        // Get list info
        let list: DomainList = conn.query_row(
            "SELECT id, name, description, author, version, created_at, profile_id
             FROM domain_lists WHERE id = ?1",
            params![list_id],
            |row| {
                Ok(DomainList {
                    id: Some(row.get(0)?),
                    name: row.get(1)?,
                    description: row.get(2)?,
                    author: row.get(3)?,
                    version: row.get(4)?,
                    created_at: row.get(5)?,
                    profile_id: row.get(6)?,
                    domain_count: None,
                })
            },
        )?;

        let domains = self.get_list_domains(list_id)?;

        let export = serde_json::json!({
            "version": 1,
            "type": "earthservers-list",
            "exported_at": chrono_now(),
            "list": {
                "name": list.name,
                "description": list.description,
                "author": list.author,
                "version": list.version
            },
            "domains": domains.into_iter().map(|d| serde_json::json!({
                "url": d.url,
                "category": d.category,
                "trust_score": d.trust_score
            })).collect::<Vec<_>>()
        });

        Ok(serde_json::to_string_pretty(&export).unwrap_or_default())
    }

    /// Import domains from JSON
    pub fn import_domains(&self, profile_id: i64, json_data: &str) -> Result<i64> {
        let data: serde_json::Value = serde_json::from_str(json_data)
            .map_err(|_| rusqlite::Error::InvalidQuery)?;

        let domains = data["domains"].as_array()
            .ok_or(rusqlite::Error::InvalidQuery)?;

        let mut imported = 0i64;
        for d in domains {
            let domain = Domain {
                id: None,
                url: d["url"].as_str().unwrap_or_default().to_string(),
                category: d["category"].as_str().unwrap_or("uncategorized").to_string(),
                trust_score: d["trust_score"].as_f64().unwrap_or(0.5),
                added_date: String::new(),
                metadata: d["metadata"].as_str().map(String::from),
                profile_id: Some(profile_id),
            };

            if !domain.url.is_empty() {
                if self.add_domain(&domain, profile_id).is_ok() {
                    imported += 1;
                }
            }
        }

        Ok(imported)
    }

    // ==================== Statistics ====================

    /// Get domain statistics
    pub fn get_stats(&self, profile_id: i64) -> Result<DomainStats> {
        let conn = Connection::open(&self.db_path)?;

        let total_domains: i64 = conn.query_row(
            "SELECT COUNT(*) FROM domains WHERE profile_id = ?1",
            params![profile_id],
            |row| row.get(0),
        )?;

        let total_lists: i64 = conn.query_row(
            "SELECT COUNT(*) FROM domain_lists WHERE profile_id = ?1",
            params![profile_id],
            |row| row.get(0),
        )?;

        let avg_trust: f64 = conn.query_row(
            "SELECT COALESCE(AVG(trust_score), 0.5) FROM domains WHERE profile_id = ?1",
            params![profile_id],
            |row| row.get(0),
        )?;

        let mut stmt = conn.prepare(
            "SELECT category, COUNT(*) as count FROM domains
             WHERE profile_id = ?1 GROUP BY category ORDER BY count DESC"
        )?;

        let categories: Vec<CategoryCount> = stmt.query_map(params![profile_id], |row| {
            Ok(CategoryCount {
                category: row.get(0)?,
                count: row.get(1)?,
            })
        })?.filter_map(|r| r.ok()).collect();

        Ok(DomainStats {
            total_domains,
            total_lists,
            categories,
            avg_trust_score: avg_trust,
        })
    }

    /// Get all unique categories
    pub fn get_categories(&self, profile_id: i64) -> Result<Vec<String>> {
        let conn = Connection::open(&self.db_path)?;
        let mut stmt = conn.prepare(
            "SELECT DISTINCT category FROM domains WHERE profile_id = ?1 ORDER BY category"
        )?;

        let categories: Vec<String> = stmt.query_map(params![profile_id], |row| {
            row.get(0)
        })?.filter_map(|r| r.ok()).collect();

        Ok(categories)
    }
}

fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", duration.as_secs())
}

// ==================== Domain Seeding ====================

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct EarthListFile {
    #[serde(alias = "list_name")]
    name: Option<String>,
    #[serde(alias = "version")]
    list_version: Option<String>,
    author: Option<String>,
    description: Option<String>,
    category: Option<String>,
    domains: Vec<EarthListDomain>,
}

#[derive(Debug, Deserialize)]
struct EarthListDomain {
    url: String,
    category: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default = "default_trust_score")]
    trust_score: f64,
}

fn default_trust_score() -> f64 {
    0.8 // Default trust score for domains without explicit score
}

impl SearchManager {
    /// Seed default domains from bundled .earth files
    /// Only runs if the database has no domains for the given profile
    pub fn seed_default_domains(&self, profile_id: i64, resource_dir: &std::path::Path) -> Result<i64> {
        let conn = Connection::open(&self.db_path)?;

        // Check if domains already exist for this profile
        let existing_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM domains WHERE profile_id = ?1",
            params![profile_id],
            |row| row.get(0),
        )?;

        if existing_count > 0 {
            return Ok(0); // Already seeded
        }

        // Try multiple possible locations for domain-lists:
        // 1. resource_dir/domain-lists (direct)
        // 2. resource_dir/resources/domain-lists (Tauri bundles resources here)
        let possible_paths = [
            resource_dir.join("domain-lists"),
            resource_dir.join("resources").join("domain-lists"),
        ];

        let domain_lists_dir = possible_paths
            .iter()
            .find(|p| p.exists())
            .cloned();

        let domain_lists_dir = match domain_lists_dir {
            Some(dir) => {
                println!("Found domain lists at: {:?}", dir);
                dir
            }
            None => {
                println!("Domain lists not found in any of: {:?}", possible_paths);
                return Ok(0);
            }
        };

        let mut total_imported = 0i64;

        // Read all .earth files
        if let Ok(entries) = std::fs::read_dir(&domain_lists_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.extension().map_or(false, |ext| ext == "earth") {
                    if let Ok(imported) = self.import_earth_file(&path, profile_id) {
                        total_imported += imported;
                    }
                }
            }
        }

        Ok(total_imported)
    }

    /// Import a single .earth file
    fn import_earth_file(&self, path: &std::path::Path, profile_id: i64) -> Result<i64> {
        let contents = std::fs::read_to_string(path)
            .map_err(|_| rusqlite::Error::InvalidQuery)?;

        let list_data: EarthListFile = serde_json::from_str(&contents)
            .map_err(|_| rusqlite::Error::InvalidQuery)?;

        // Get list name from file or use filename
        let list_name = list_data.name.clone().unwrap_or_else(|| {
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Unknown")
                .to_string()
        });

        // Create the list
        let list = DomainList {
            id: None,
            name: list_name,
            description: list_data.description.clone(),
            author: list_data.author.clone(),
            version: list_data.list_version.unwrap_or_else(|| "1.0".to_string()),
            created_at: String::new(),
            profile_id: Some(profile_id),
            domain_count: None,
        };

        let created_list = self.create_list(&list, profile_id)?;
        let list_id = created_list.id.unwrap_or(0);

        let mut imported = 0i64;

        // Import domains
        for d in list_data.domains {
            let domain = Domain {
                id: None,
                url: d.url,
                category: d.category,
                trust_score: d.trust_score,
                added_date: String::new(),
                metadata: None,
                profile_id: Some(profile_id),
            };

            if let Ok(created_domain) = self.add_domain(&domain, profile_id) {
                // Add to list
                if let Some(domain_id) = created_domain.id {
                    let _ = self.add_domain_to_list(list_id, domain_id);
                }
                imported += 1;
            }
        }

        Ok(imported)
    }
}
