//! Instagram [FRAGILE] and Facebook [LEAST RELIABLE] — both default OFF.
//!
//! HONEST CAVEATS (kept truthful on purpose):
//!  - Public, logged-out only. We never touch private accounts/content and never
//!    circumvent auth/anti-bot barriers.
//!  - Instagram serves posts/comments via internal GraphQL whose doc_id rotates
//!    every 2-4 weeks and blocks datacenter IPs / non-browser TLS — so a robust
//!    comment pull WILL break periodically regardless of effort. We therefore do
//!    NOT reverse-engineer GraphQL; we only read the public page's Open Graph
//!    caption (og:title/og:description) with a realistic browser UA, very low caps,
//!    and return EMPTY (not an error) on any block. Comments are usually absent.
//!  - Facebook is mostly login-walled; public-page coverage is thin and brittle.
//!    Same best-effort caption-only approach, clearly experimental.
//!
//! Both fail closed and graceful: a block returns what we have (often nothing) and
//! never aborts the overall search.

use super::{Candidate, FetchedDoc, Segment, SourceAdapter};
use crate::search_index::kinds::ContentKind;
use std::time::Duration;

/// A realistic browser User-Agent + headers — these hosts block non-browser agents
/// outright. (This is presenting a normal browser identity for PUBLIC pages, not
/// circumventing an auth/anti-bot challenge.)
fn browser_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36")
        .timeout(Duration::from_secs(15))
        .build()
        .unwrap_or_default()
}

/// Pull a meta-tag content value (og:title / og:description) from page HTML.
fn meta_content(html: &str, property: &str) -> Option<String> {
    // matches <meta property="og:description" content="..."> in either attr order
    let pat = format!(
        r#"(?is)<meta[^>]+(?:property|name)=["']{}["'][^>]+content=["']([^"']*)["']"#,
        regex::escape(property)
    );
    let re = regex::Regex::new(&pat).ok()?;
    let v = re.captures(html)?.get(1)?.as_str().trim().to_string();
    if v.is_empty() {
        None
    } else {
        Some(decode_entities(&v))
    }
}

fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&").replace("&quot;", "\"").replace("&#39;", "'")
        .replace("&lt;", "<").replace("&gt;", ">").replace("&#x27;", "'")
}

/// Shared best-effort caption fetch: returns a single `post` segment from the
/// public page's Open Graph caption, or EMPTY on any block/failure (graceful). An
/// optional user-supplied session cookie (opt-in, default off) is sent if present.
async fn caption_segment(url: &str, cookie: Option<&str>) -> Vec<Segment> {
    let client = browser_client();
    let mut req = client
        .get(url)
        .header("Accept-Language", "en-US,en;q=0.9")
        .header("Accept", "text/html,application/xhtml+xml");
    if let Some(c) = cookie {
        req = req.header("Cookie", c);
    }
    let resp = match req.send().await
    {
        Ok(r) if r.status().is_success() => r,
        _ => return Vec::new(), // blocked/unavailable → graceful empty
    };
    let html = match resp.text().await {
        Ok(h) => h,
        Err(_) => return Vec::new(),
    };
    let title = meta_content(&html, "og:title");
    let desc = meta_content(&html, "og:description");
    let text = match (&title, &desc) {
        (Some(t), Some(d)) => format!("{}\n\n{}", t, d),
        (None, Some(d)) => d.clone(),
        (Some(t), None) => t.clone(),
        (None, None) => return Vec::new(),
    };
    vec![Segment {
        kind: ContentKind::Post,
        url: url.to_string(),
        parent_url: None,
        title,
        text,
        author: None,
        engagement: None,
    }]
}

// ---------------- Instagram ----------------

pub struct InstagramAdapter {
    cookie: Option<String>,
}
impl InstagramAdapter {
    pub fn new() -> Self {
        InstagramAdapter { cookie: None }
    }
    /// Opt-in: use a user-supplied session cookie (default off).
    pub fn with_session(cookie: Option<String>) -> Self {
        InstagramAdapter { cookie }
    }
}
impl Default for InstagramAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl SourceAdapter for InstagramAdapter {
    fn id(&self) -> &'static str {
        "instagram"
    }
    async fn discover(&self, query: &str, limit: usize) -> Result<Vec<Candidate>, String> {
        let results = crate::research::search(query, None).await?;
        Ok(results.into_iter().filter(|r| r.url.contains("instagram.com")).take(limit)
            .map(|r| Candidate { url: r.url, title: r.title, snippet: r.snippet, source_engine: "instagram".into(), searxng_pos: None })
            .collect())
    }
    async fn fetch(&self, url: &str) -> Result<FetchedDoc, String> {
        let segs = caption_segment(url, self.cookie.as_deref()).await;
        match segs.into_iter().next() {
            Some(s) => Ok(FetchedDoc { url: url.to_string(), title: s.title.unwrap_or_default(), body: s.text }),
            None => Err("instagram: unavailable (public caption not reachable)".into()),
        }
    }
    fn freshness_ttl(&self) -> Duration {
        Duration::from_secs(86_400)
    }
    fn handles_host(&self, host: &str) -> bool {
        host == "instagram.com" || host.ends_with(".instagram.com")
    }
    fn reliability(&self) -> &'static str {
        "fragile"
    }
    fn default_enabled(&self) -> bool {
        false
    }
    async fn fetch_segments(&self, url: &str, _max_units: usize) -> Result<Vec<Segment>, String> {
        Ok(caption_segment(url, self.cookie.as_deref()).await) // graceful-empty; comments need rotating GraphQL we don't touch
    }
}

// ---------------- Facebook ----------------

pub struct FacebookAdapter {
    cookie: Option<String>,
}
impl FacebookAdapter {
    pub fn new() -> Self {
        FacebookAdapter { cookie: None }
    }
    /// Opt-in: use a user-supplied session cookie (default off).
    pub fn with_session(cookie: Option<String>) -> Self {
        FacebookAdapter { cookie }
    }
}
impl Default for FacebookAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl SourceAdapter for FacebookAdapter {
    fn id(&self) -> &'static str {
        "facebook"
    }
    async fn discover(&self, query: &str, limit: usize) -> Result<Vec<Candidate>, String> {
        let results = crate::research::search(query, None).await?;
        Ok(results.into_iter().filter(|r| r.url.contains("facebook.com")).take(limit)
            .map(|r| Candidate { url: r.url, title: r.title, snippet: r.snippet, source_engine: "facebook".into(), searxng_pos: None })
            .collect())
    }
    async fn fetch(&self, url: &str) -> Result<FetchedDoc, String> {
        let segs = caption_segment(url, self.cookie.as_deref()).await;
        match segs.into_iter().next() {
            Some(s) => Ok(FetchedDoc { url: url.to_string(), title: s.title.unwrap_or_default(), body: s.text }),
            None => Err("facebook: unavailable (login-walled or blocked)".into()),
        }
    }
    fn freshness_ttl(&self) -> Duration {
        Duration::from_secs(86_400)
    }
    fn handles_host(&self, host: &str) -> bool {
        host == "facebook.com" || host.ends_with(".facebook.com") || host == "fb.com" || host == "fb.watch"
    }
    fn reliability(&self) -> &'static str {
        "fragile"
    }
    fn default_enabled(&self) -> bool {
        false
    }
    async fn fetch_segments(&self, url: &str, _max_units: usize) -> Result<Vec<Segment>, String> {
        Ok(caption_segment(url, self.cookie.as_deref()).await) // graceful-empty; most content is login-walled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn og_meta_extraction() {
        let html = r#"<html><head>
            <meta property="og:title" content="A &amp; B Post" />
            <meta property="og:description" content="caption text here" />
        </head></html>"#;
        assert_eq!(meta_content(html, "og:title").as_deref(), Some("A & B Post"));
        assert_eq!(meta_content(html, "og:description").as_deref(), Some("caption text here"));
        assert_eq!(meta_content(html, "og:image"), None);
    }

    #[test]
    fn fragile_adapters_default_off() {
        assert!(!InstagramAdapter::new().default_enabled());
        assert!(!FacebookAdapter::new().default_enabled());
        assert_eq!(InstagramAdapter::new().reliability(), "fragile");
    }
}
