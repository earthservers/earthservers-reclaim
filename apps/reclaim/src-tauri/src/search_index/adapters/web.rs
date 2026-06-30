//! Generic web adapter — the default and the backstop for everything.
//!
//! discover = `research::search` (SearXNG, DuckDuckGo fallback).
//! fetch    = HTTP GET + `scraper::extract_text` (the same extractor the crawler
//!            and research::fetch use), bounded to a sane body size for indexing.

use super::{Candidate, FetchedDoc, SourceAdapter};
use std::time::Duration;

/// Cap indexed body length so one huge page can't bloat the index / embeddings.
const MAX_BODY_WORDS: usize = 4000;

pub struct GenericWebAdapter {
    searxng_url: Option<String>,
}

impl GenericWebAdapter {
    pub fn new(searxng_url: Option<String>) -> Self {
        Self { searxng_url }
    }
}

#[async_trait::async_trait]
impl SourceAdapter for GenericWebAdapter {
    fn id(&self) -> &'static str {
        "web"
    }

    async fn discover(&self, query: &str, limit: usize) -> Result<Vec<Candidate>, String> {
        self.discover_page(query, limit, 0).await
    }

    async fn discover_page(&self, query: &str, limit: usize, page: usize) -> Result<Vec<Candidate>, String> {
        let results = crate::research::search_paged(query, self.searxng_url.as_deref(), page).await?;
        Ok(results
            .into_iter()
            .take(limit)
            .enumerate()
            .map(|(i, r)| Candidate {
                url: r.url,
                title: r.title,
                snippet: r.snippet,
                source_engine: "web".to_string(),
                // SearXNG already aggregated cross-engine relevance; the position
                // in the returned list IS that aggregated rank (lower = better).
                // Offset by page so later-page results rank after earlier ones.
                searxng_pos: Some(page * limit + i),
            })
            .collect())
    }

    async fn fetch(&self, url: &str) -> Result<FetchedDoc, String> {
        let client = super::http_client();
        let resp = client
            .get(url)
            .header("Referer", "https://duckduckgo.com/")
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("fetch {}: HTTP {}", url, resp.status()));
        }
        let html = resp.text().await.map_err(|e| e.to_string())?;
        let title = extract_title(&html).unwrap_or_else(|| url.to_string());
        let body = crate::scraper::extract_text(&html)
            .split_whitespace()
            .take(MAX_BODY_WORDS)
            .collect::<Vec<_>>()
            .join(" ");
        Ok(FetchedDoc {
            url: url.to_string(),
            title,
            body,
        })
    }

    fn freshness_ttl(&self) -> Duration {
        // Web pages are reused for a week before a re-fetch (matches the cache tier).
        Duration::from_secs(7 * 86_400)
    }
}

/// Minimal <title> extractor (scraper.rs's is private; this mirrors it).
fn extract_title(html: &str) -> Option<String> {
    let re = regex::Regex::new(r"(?is)<title[^>]*>(.*?)</title>").ok()?;
    re.captures(html)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    #[test]
    fn title_extraction() {
        assert_eq!(
            super::extract_title("<html><head><TITLE> Hi There </TITLE></head>").as_deref(),
            Some("Hi There")
        );
        assert_eq!(super::extract_title("<html>no title</html>"), None);
    }
}
