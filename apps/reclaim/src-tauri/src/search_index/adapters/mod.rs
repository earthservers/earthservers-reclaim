//! Phase 3 — source adapters.
//!
//! High-value sources get pluggable adapters; everything else falls through to
//! SearXNG + the generic scraper. The orchestrator uses `discover` to find
//! candidates fast (shallow) and `fetch` to pull full text for indexing (deep).
//!
//! Error type is `Result<T, String>` to match the rest of the backend
//! (`research::`, `scraper::`), not anyhow.

pub mod web;
pub mod reddit;

use std::sync::Arc;
use std::time::Duration;

/// A shallow search hit: enough to paint a result immediately, before scraping.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub url: String,
    pub title: String,
    pub snippet: String,
    /// Which adapter/engine produced it: "reddit" | "web" | ...
    pub source_engine: String,
    /// Original aggregated rank from SearXNG (lower = better), if applicable.
    pub searxng_pos: Option<usize>,
}

/// A fully fetched document, cleaned to plain text and ready to index.
#[derive(Debug, Clone)]
pub struct FetchedDoc {
    pub url: String,
    pub title: String,
    pub body: String,
}

#[async_trait::async_trait]
pub trait SourceAdapter: Send + Sync {
    /// Stable id, e.g. "reddit", "web".
    fn id(&self) -> &'static str;

    /// Find candidate results for a query (shallow, fast).
    async fn discover(&self, query: &str, limit: usize) -> Result<Vec<Candidate>, String>;

    /// Fetch + clean a single document for indexing (deep).
    async fn fetch(&self, url: &str) -> Result<FetchedDoc, String>;

    /// Per-source cache freshness: a `cache`/`ephemeral` row younger than this is
    /// reused instead of re-fetched.
    fn freshness_ttl(&self) -> Duration;

    /// Whether this adapter is the host-specialist for a given URL host. The
    /// generic web adapter returns false; host adapters (Reddit) match their hosts.
    fn handles_host(&self, _host: &str) -> bool {
        false
    }
}

/// Shared reqwest client builder with a real, descriptive User-Agent. Reddit (and
/// some CDNs) 403/429 default/empty agents, so every adapter goes through this.
pub fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent("EarthReclaim/0.1 local-search (+https://earthservers.com)")
        .timeout(Duration::from_secs(20))
        .build()
        .unwrap_or_default()
}

/// Lowercased host of a URL, `www.` stripped. None if unparseable.
pub fn host_of(url: &str) -> Option<String> {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.trim_start_matches("www.").to_ascii_lowercase()))
}

/// Registry of available adapters. Keyed lookup by id, host routing for fetch.
pub struct AdapterRegistry {
    adapters: Vec<Arc<dyn SourceAdapter>>,
    web: Arc<dyn SourceAdapter>,
}

impl AdapterRegistry {
    /// The default set: generic web (backstop) + Reddit.
    pub fn default_set(searxng_url: Option<String>) -> Self {
        let web: Arc<dyn SourceAdapter> = Arc::new(web::GenericWebAdapter::new(searxng_url));
        let reddit: Arc<dyn SourceAdapter> = Arc::new(reddit::RedditAdapter::new());
        Self {
            adapters: vec![web.clone(), reddit],
            web,
        }
    }

    pub fn by_id(&self, id: &str) -> Option<Arc<dyn SourceAdapter>> {
        self.adapters.iter().find(|a| a.id() == id).cloned()
    }

    /// The host-specialist adapter for a URL, or the generic web adapter as the
    /// backstop. Used to route a candidate's `fetch` to the right adapter.
    pub fn fetch_adapter(&self, url: &str) -> Arc<dyn SourceAdapter> {
        if let Some(host) = host_of(url) {
            if let Some(a) = self.adapters.iter().find(|a| a.handles_host(&host)) {
                return a.clone();
            }
        }
        self.web.clone()
    }

    /// Which adapters to run `discover` on for this search. `sources` selects by
    /// id; None (or empty) defaults to ["web"]. Unknown ids are ignored. The web
    /// adapter is always included so the box is never empty.
    pub fn discovery_adapters(&self, sources: &Option<Vec<String>>) -> Vec<Arc<dyn SourceAdapter>> {
        let ids: Vec<String> = match sources {
            Some(s) if !s.is_empty() => s.clone(),
            _ => vec!["web".to_string()],
        };
        let mut out: Vec<Arc<dyn SourceAdapter>> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for id in &ids {
            if let Some(a) = self.by_id(id) {
                if seen.insert(a.id()) {
                    out.push(a);
                }
            }
        }
        if seen.insert(self.web.id()) {
            out.push(self.web.clone());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_parsing() {
        assert_eq!(host_of("https://www.Reddit.com/r/rust").as_deref(), Some("reddit.com"));
        assert_eq!(host_of("http://news.ycombinator.com/x").as_deref(), Some("news.ycombinator.com"));
        assert_eq!(host_of("not a url"), None);
    }

    #[test]
    fn registry_routes_and_defaults() {
        let reg = AdapterRegistry::default_set(None);
        assert!(reg.by_id("web").is_some());
        assert!(reg.by_id("reddit").is_some());
        assert!(reg.by_id("bogus").is_none());

        // Reddit URL routes to the reddit adapter; others fall back to web.
        assert_eq!(reg.fetch_adapter("https://reddit.com/r/rust/comments/abc").id(), "reddit");
        assert_eq!(reg.fetch_adapter("https://example.com/post").id(), "web");

        // Default discovery always includes web; explicit reddit adds reddit + web.
        let d = reg.discovery_adapters(&None);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].id(), "web");
        let d2 = reg.discovery_adapters(&Some(vec!["reddit".into()]));
        let ids: Vec<&str> = d2.iter().map(|a| a.id()).collect();
        assert!(ids.contains(&"reddit") && ids.contains(&"web"));
    }
}
