// Web Scraper for Reclaim
// Allows users to scrape and index web content for local search

use rusqlite::{Connection, Result, params};
use serde::{Deserialize, Serialize};
use reqwest::Client;
use std::collections::HashSet;

// ==================== Types ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapingJob {
    pub id: Option<i64>,
    pub profile_id: i64,
    pub name: String,
    pub base_url: String,
    pub url_pattern: Option<String>,
    pub max_depth: i32,
    pub max_pages: i32,
    pub content_selectors: Vec<ContentSelector>,
    pub schedule_cron: Option<String>,
    pub status: String,
    pub last_run_at: Option<String>,
    pub pages_scraped: i32,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentSelector {
    pub name: String,
    pub selector: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapedPage {
    pub id: Option<i64>,
    pub job_id: i64,
    pub url: String,
    pub title: Option<String>,
    pub content: String,
    pub metadata: Option<String>,
    pub scraped_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobStatus {
    pub status: String,
    pub pages_scraped: i32,
    pub current_url: Option<String>,
    pub error: Option<String>,
}

// ==================== Database ====================

pub fn init_scraper_tables(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS scraping_jobs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            profile_id INTEGER NOT NULL,
            name TEXT NOT NULL,
            base_url TEXT NOT NULL,
            url_pattern TEXT,
            max_depth INTEGER DEFAULT 2,
            max_pages INTEGER DEFAULT 100,
            content_selectors TEXT,
            schedule_cron TEXT,
            status TEXT DEFAULT 'pending',
            last_run_at TEXT,
            pages_scraped INTEGER DEFAULT 0,
            created_at TEXT NOT NULL
        )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS scraped_pages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            job_id INTEGER NOT NULL,
            url TEXT NOT NULL,
            title TEXT,
            content TEXT,
            metadata TEXT,
            scraped_at TEXT NOT NULL,
            FOREIGN KEY (job_id) REFERENCES scraping_jobs(id) ON DELETE CASCADE,
            UNIQUE(job_id, url)
        )",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_scraped_pages_job ON scraped_pages(job_id)",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_scraped_pages_url ON scraped_pages(url)",
        [],
    )?;

    Ok(())
}

// ==================== Manager ====================

pub struct ScraperManager {
    db_path: String,
}

impl ScraperManager {
    pub fn new(db_path: String) -> Self {
        // Initialize tables
        if let Ok(conn) = Connection::open(&db_path) {
            let _ = init_scraper_tables(&conn);
        }
        ScraperManager { db_path }
    }

    /// Create a new scraping job
    pub fn create_job(
        &self,
        profile_id: i64,
        name: &str,
        base_url: &str,
        url_pattern: Option<&str>,
        max_depth: i32,
        max_pages: i32,
        content_selectors: Vec<ContentSelector>,
    ) -> Result<i64> {
        let conn = Connection::open(&self.db_path)?;
        let now = chrono::Utc::now().to_rfc3339();
        let selectors_json = serde_json::to_string(&content_selectors).unwrap_or_default();

        conn.execute(
            "INSERT INTO scraping_jobs (profile_id, name, base_url, url_pattern, max_depth, max_pages, content_selectors, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![profile_id, name, base_url, url_pattern, max_depth, max_pages, selectors_json, now],
        )?;

        Ok(conn.last_insert_rowid())
    }

    /// Get all scraping jobs for a profile
    pub fn get_jobs(&self, profile_id: i64) -> Result<Vec<ScrapingJob>> {
        let conn = Connection::open(&self.db_path)?;
        let mut stmt = conn.prepare(
            "SELECT id, profile_id, name, base_url, url_pattern, max_depth, max_pages,
                    content_selectors, schedule_cron, status, last_run_at, pages_scraped, created_at
             FROM scraping_jobs
             WHERE profile_id = ?1
             ORDER BY created_at DESC"
        )?;

        let jobs = stmt.query_map(params![profile_id], |row| {
            let selectors_json: String = row.get::<_, Option<String>>(7)?.unwrap_or_else(|| "[]".to_string());
            let selectors: Vec<ContentSelector> = serde_json::from_str(&selectors_json).unwrap_or_default();

            Ok(ScrapingJob {
                id: row.get(0)?,
                profile_id: row.get(1)?,
                name: row.get(2)?,
                base_url: row.get(3)?,
                url_pattern: row.get(4)?,
                max_depth: row.get(5)?,
                max_pages: row.get(6)?,
                content_selectors: selectors,
                schedule_cron: row.get(8)?,
                status: row.get(9)?,
                last_run_at: row.get(10)?,
                pages_scraped: row.get(11)?,
                created_at: row.get(12)?,
            })
        })?;

        jobs.collect()
    }

    /// Get a single job by ID
    pub fn get_job(&self, job_id: i64) -> Result<ScrapingJob> {
        let conn = Connection::open(&self.db_path)?;

        conn.query_row(
            "SELECT id, profile_id, name, base_url, url_pattern, max_depth, max_pages,
                    content_selectors, schedule_cron, status, last_run_at, pages_scraped, created_at
             FROM scraping_jobs WHERE id = ?1",
            params![job_id],
            |row| {
                let selectors_json: String = row.get::<_, Option<String>>(7)?.unwrap_or_else(|| "[]".to_string());
                let selectors: Vec<ContentSelector> = serde_json::from_str(&selectors_json).unwrap_or_default();

                Ok(ScrapingJob {
                    id: row.get(0)?,
                    profile_id: row.get(1)?,
                    name: row.get(2)?,
                    base_url: row.get(3)?,
                    url_pattern: row.get(4)?,
                    max_depth: row.get(5)?,
                    max_pages: row.get(6)?,
                    content_selectors: selectors,
                    schedule_cron: row.get(8)?,
                    status: row.get(9)?,
                    last_run_at: row.get(10)?,
                    pages_scraped: row.get(11)?,
                    created_at: row.get(12)?,
                })
            },
        )
    }

    /// Delete a scraping job
    pub fn delete_job(&self, job_id: i64) -> Result<()> {
        let conn = Connection::open(&self.db_path)?;

        // Delete scraped pages first
        conn.execute("DELETE FROM scraped_pages WHERE job_id = ?1", params![job_id])?;

        // Delete the job
        conn.execute("DELETE FROM scraping_jobs WHERE id = ?1", params![job_id])?;

        Ok(())
    }

    /// Update job status
    pub fn update_job_status(&self, job_id: i64, status: &str, pages_scraped: i32) -> Result<()> {
        let conn = Connection::open(&self.db_path)?;
        let now = chrono::Utc::now().to_rfc3339();

        conn.execute(
            "UPDATE scraping_jobs SET status = ?1, pages_scraped = ?2, last_run_at = ?3 WHERE id = ?4",
            params![status, pages_scraped, now, job_id],
        )?;

        Ok(())
    }

    /// Save a scraped page
    pub fn save_page(&self, job_id: i64, url: &str, title: Option<&str>, content: &str, metadata: Option<&str>) -> Result<()> {
        let conn = Connection::open(&self.db_path)?;
        let now = chrono::Utc::now().to_rfc3339();

        conn.execute(
            "INSERT OR REPLACE INTO scraped_pages (job_id, url, title, content, metadata, scraped_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![job_id, url, title, content, metadata, now],
        )?;

        Ok(())
    }

    /// Get scraped pages for a job
    pub fn get_pages(&self, job_id: i64, limit: i32) -> Result<Vec<ScrapedPage>> {
        let conn = Connection::open(&self.db_path)?;
        let mut stmt = conn.prepare(
            "SELECT id, job_id, url, title, content, metadata, scraped_at
             FROM scraped_pages
             WHERE job_id = ?1
             ORDER BY scraped_at DESC
             LIMIT ?2"
        )?;

        let pages = stmt.query_map(params![job_id, limit], |row| {
            Ok(ScrapedPage {
                id: row.get(0)?,
                job_id: row.get(1)?,
                url: row.get(2)?,
                title: row.get(3)?,
                content: row.get(4)?,
                metadata: row.get(5)?,
                scraped_at: row.get(6)?,
            })
        })?;

        pages.collect()
    }

    /// Search scraped content
    pub fn search_content(&self, profile_id: i64, query: &str, limit: i32) -> Result<Vec<ScrapedPage>> {
        let conn = Connection::open(&self.db_path)?;
        let search_pattern = format!("%{}%", query);

        let mut stmt = conn.prepare(
            "SELECT sp.id, sp.job_id, sp.url, sp.title, sp.content, sp.metadata, sp.scraped_at
             FROM scraped_pages sp
             JOIN scraping_jobs sj ON sp.job_id = sj.id
             WHERE sj.profile_id = ?1
               AND (sp.title LIKE ?2 OR sp.content LIKE ?2 OR sp.url LIKE ?2)
             ORDER BY sp.scraped_at DESC
             LIMIT ?3"
        )?;

        let pages = stmt.query_map(params![profile_id, search_pattern, limit], |row| {
            Ok(ScrapedPage {
                id: row.get(0)?,
                job_id: row.get(1)?,
                url: row.get(2)?,
                title: row.get(3)?,
                content: row.get(4)?,
                metadata: row.get(5)?,
                scraped_at: row.get(6)?,
            })
        })?;

        pages.collect()
    }

    /// Run a scraping job (simplified version - actual scraping would be more complex)
    pub async fn run_job(&self, job_id: i64) -> std::result::Result<(), String> {
        let job = self.get_job(job_id).map_err(|e| e.to_string())?;

        self.update_job_status(job_id, "running", 0).map_err(|e| e.to_string())?;

        let client = Client::builder()
            .user_agent("Reclaim Web Scraper/1.0")
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| e.to_string())?;

        let mut visited: HashSet<String> = HashSet::new();
        let mut to_visit = vec![(job.base_url.clone(), 0)];
        let url_regex = job.url_pattern.as_ref()
            .and_then(|p| regex::Regex::new(p).ok());

        let mut pages_scraped = 0;

        while let Some((url, depth)) = to_visit.pop() {
            if visited.len() >= job.max_pages as usize || depth > job.max_depth {
                break;
            }

            if visited.contains(&url) {
                continue;
            }

            // Check URL pattern
            if let Some(ref regex) = url_regex {
                if !regex.is_match(&url) {
                    continue;
                }
            }

            // Fetch the page
            match client.get(&url).send().await {
                Ok(response) => {
                    if let Ok(html) = response.text().await {
                        // Simple text extraction (remove HTML tags)
                        let text_content = extract_text(&html);
                        let title = extract_title(&html);

                        // Save the page
                        if let Err(e) = self.save_page(job_id, &url, title.as_deref(), &text_content, None) {
                            eprintln!("Failed to save page {}: {}", url, e);
                        } else {
                            pages_scraped += 1;
                            self.update_job_status(job_id, "running", pages_scraped).ok();
                        }

                        // Extract links for crawling
                        if depth < job.max_depth {
                            for link in extract_links(&html, &url) {
                                if !visited.contains(&link) {
                                    to_visit.push((link, depth + 1));
                                }
                            }
                        }

                        visited.insert(url);
                    }
                }
                Err(e) => {
                    eprintln!("Failed to fetch {}: {}", url, e);
                }
            }

            // Small delay to be polite
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }

        self.update_job_status(job_id, "completed", pages_scraped).map_err(|e| e.to_string())?;
        Ok(())
    }
}

// ==================== Helper Functions ====================

/// Extract text content from HTML (simple implementation)
pub(crate) fn extract_text(html: &str) -> String {
    // Remove script and style tags and their content
    let re_script = regex::Regex::new(r"(?is)<script[^>]*>.*?</script>").unwrap();
    let re_style = regex::Regex::new(r"(?is)<style[^>]*>.*?</style>").unwrap();
    let re_tags = regex::Regex::new(r"<[^>]+>").unwrap();
    let re_whitespace = regex::Regex::new(r"\s+").unwrap();

    let text = re_script.replace_all(html, "");
    let text = re_style.replace_all(&text, "");
    let text = re_tags.replace_all(&text, " ");
    let text = re_whitespace.replace_all(&text, " ");

    text.trim().to_string()
}

/// Extract title from HTML
fn extract_title(html: &str) -> Option<String> {
    let re = regex::Regex::new(r"(?is)<title[^>]*>(.*?)</title>").ok()?;
    re.captures(html)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().trim().to_string())
}

/// Extract links from HTML
fn extract_links(html: &str, base_url: &str) -> Vec<String> {
    let re = regex::Regex::new(r#"href=["']([^"']+)["']"#).unwrap();
    let base = url::Url::parse(base_url).ok();

    re.captures_iter(html)
        .filter_map(|caps| caps.get(1))
        .filter_map(|m| {
            let href = m.as_str();
            if href.starts_with("http://") || href.starts_with("https://") {
                Some(href.to_string())
            } else if let Some(ref base) = base {
                base.join(href).ok().map(|u| u.to_string())
            } else {
                None
            }
        })
        .filter(|url| !url.contains('#') && !url.ends_with(".pdf") && !url.ends_with(".jpg") && !url.ends_with(".png"))
        .collect()
}
