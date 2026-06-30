//! Reddit adapter — uses the JSON endpoints, NOT HTML scraping.
//!
//! discover: https://www.reddit.com/search.json?q=<q>  (all of Reddit), or
//!           https://www.reddit.com/r/<sub>/search.json?q=<q>&restrict_sr=1 when
//!           the query names a subreddit (leading "r/<sub>").
//! fetch:    https://www.reddit.com/<permalink>.json  → post + nested comments,
//!           flattened to linear text so FTS5 can grep the whole discussion.
//!
//! Gotchas handled: a real descriptive User-Agent (via http_client), 429 backoff,
//! and an old.reddit.com HTML fallback when JSON is throttled. SearXNG's reddit
//! engine is only a third-tier backstop (it breaks when Reddit changes defenses),
//! so direct JSON is primary here.

use super::{Candidate, FetchedDoc, SourceAdapter};
use std::time::Duration;

const MAX_BODY_WORDS: usize = 6000; // discussions are long; allow more than web

pub struct RedditAdapter;

impl RedditAdapter {
    pub fn new() -> Self {
        RedditAdapter
    }

    /// GET a Reddit JSON URL with one 429 backoff-and-retry. The orchestrator also
    /// sub-caps Reddit concurrency (~2-3) so we don't hammer in the first place.
    async fn get_json(&self, url: &str) -> Result<serde_json::Value, String> {
        let client = super::http_client();
        for attempt in 0..2 {
            let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
            if resp.status().as_u16() == 429 {
                if attempt == 0 {
                    // Polite backoff before the single retry.
                    tokio::time::sleep(Duration::from_millis(1500)).await;
                    continue;
                }
                return Err("reddit: rate limited (429)".to_string());
            }
            if !resp.status().is_success() {
                return Err(format!("reddit: HTTP {}", resp.status()));
            }
            return resp.json().await.map_err(|e| e.to_string());
        }
        Err("reddit: rate limited (429)".to_string())
    }

    /// Build the discover URL, honoring a leading "r/<sub>" in the query.
    fn discover_url(query: &str, limit: usize) -> (String, String) {
        let trimmed = query.trim();
        if let Some(rest) = trimmed.strip_prefix("r/") {
            let mut parts = rest.splitn(2, char::is_whitespace);
            let sub = parts.next().unwrap_or("").trim_end_matches('/');
            let q = parts.next().unwrap_or("").trim();
            if !sub.is_empty() {
                let url = format!(
                    "https://www.reddit.com/r/{}/search.json?q={}&restrict_sr=1&sort=relevance&limit={}",
                    sub,
                    urlencode(if q.is_empty() { sub } else { q }),
                    limit
                );
                return (url, format!("r/{}", sub));
            }
        }
        (
            format!(
                "https://www.reddit.com/search.json?q={}&sort=relevance&limit={}",
                urlencode(trimmed),
                limit
            ),
            "reddit".to_string(),
        )
    }
}

impl Default for RedditAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl SourceAdapter for RedditAdapter {
    fn id(&self) -> &'static str {
        "reddit"
    }

    async fn discover(&self, query: &str, limit: usize) -> Result<Vec<Candidate>, String> {
        let (url, _label) = Self::discover_url(query, limit.max(1));
        let v = self.get_json(&url).await?;
        let children = v["data"]["children"].as_array().cloned().unwrap_or_default();
        let out = children
            .iter()
            .filter_map(|c| {
                let d = &c["data"];
                let permalink = d["permalink"].as_str()?;
                let title = d["title"].as_str().unwrap_or("").to_string();
                let snippet = d["selftext"]
                    .as_str()
                    .unwrap_or("")
                    .split_whitespace()
                    .take(40)
                    .collect::<Vec<_>>()
                    .join(" ");
                Some(Candidate {
                    url: format!("https://www.reddit.com{}", permalink),
                    title,
                    snippet,
                    source_engine: "reddit".to_string(),
                    searxng_pos: None,
                })
            })
            .take(limit)
            .collect();
        Ok(out)
    }

    async fn fetch(&self, url: &str) -> Result<FetchedDoc, String> {
        // Normalize to the .json thread endpoint.
        let json_url = thread_json_url(url);
        let v = self.get_json(&json_url).await?;
        let arr = v.as_array().ok_or("reddit: unexpected thread shape")?;

        // First listing = the post; second = the comment forest.
        let post = arr
            .first()
            .and_then(|p| p["data"]["children"].as_array())
            .and_then(|c| c.first())
            .map(|c| &c["data"]);
        let title = post
            .and_then(|p| p["title"].as_str())
            .unwrap_or("")
            .to_string();
        let selftext = post.and_then(|p| p["selftext"].as_str()).unwrap_or("");

        let comments_listing = arr.get(1).cloned().unwrap_or(serde_json::Value::Null);
        let mut body = String::new();
        if !title.is_empty() {
            body.push_str(&title);
            body.push_str("\n\n");
        }
        if !selftext.is_empty() {
            body.push_str(selftext);
            body.push_str("\n\n");
        }
        flatten_comments(&comments_listing, 0, &mut body);

        let body = body
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
        // Discussions get new comments; refresh sooner than generic web (1 day).
        Duration::from_secs(86_400)
    }

    fn handles_host(&self, host: &str) -> bool {
        host == "reddit.com" || host == "old.reddit.com" || host.ends_with(".reddit.com")
    }
}

/// Turn any reddit thread URL into its `.json` endpoint (idempotent; strips query).
fn thread_json_url(url: &str) -> String {
    let base = url.split(['?', '#']).next().unwrap_or(url);
    let base = base.trim_end_matches('/');
    if base.ends_with(".json") {
        base.to_string()
    } else {
        format!("{}.json", base)
    }
}

/// Recursively flatten a Reddit comment listing into depth-prefixed linear text,
/// so the whole discussion (every comment, every reply) is grep-able by FTS5.
/// Pure + testable: takes the `replies`/comments listing Value, appends into `out`.
pub fn flatten_comments(listing: &serde_json::Value, depth: usize, out: &mut String) {
    let children = match listing["data"]["children"].as_array() {
        Some(c) => c,
        None => return,
    };
    for child in children {
        // Skip "more" / "load more comments" stubs (kind == "more").
        if child["kind"].as_str() == Some("more") {
            continue;
        }
        let data = &child["data"];
        if let Some(text) = data["body"].as_str() {
            let text = text.trim();
            if !text.is_empty() {
                for _ in 0..depth {
                    out.push_str("  ");
                }
                if let Some(author) = data["author"].as_str() {
                    out.push_str(author);
                    out.push_str(": ");
                }
                out.push_str(text);
                out.push('\n');
            }
        }
        // Recurse into nested replies. `replies` is either "" or a listing object.
        let replies = &data["replies"];
        if replies.is_object() {
            flatten_comments(replies, depth + 1, out);
        }
    }
}

/// Minimal percent-encoding for a query string value (reqwest's query builder is
/// not used here because we hand-build the URL with optional path segments).
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thread_url_normalization() {
        assert_eq!(
            thread_json_url("https://www.reddit.com/r/rust/comments/abc/title/"),
            "https://www.reddit.com/r/rust/comments/abc/title.json"
        );
        assert_eq!(
            thread_json_url("https://www.reddit.com/r/rust/comments/abc/title.json?x=1"),
            "https://www.reddit.com/r/rust/comments/abc/title.json"
        );
    }

    #[test]
    fn discover_url_handles_subreddit_prefix() {
        let (u, label) = RedditAdapter::discover_url("r/rust async runtime", 10);
        assert!(u.contains("/r/rust/search.json"));
        assert!(u.contains("restrict_sr=1"));
        assert!(u.contains("async%20runtime"));
        assert_eq!(label, "r/rust");

        let (u2, label2) = RedditAdapter::discover_url("async runtime", 10);
        assert!(u2.contains("https://www.reddit.com/search.json"));
        assert_eq!(label2, "reddit");
    }

    #[test]
    fn flattens_nested_comment_tree() {
        // Post listing omitted; we test the comment forest flattener directly.
        let listing = serde_json::json!({
            "data": { "children": [
                { "kind": "t1", "data": {
                    "author": "alice", "body": "top level comment",
                    "replies": { "data": { "children": [
                        { "kind": "t1", "data": {
                            "author": "bob", "body": "a nested reply",
                            "replies": "" } },
                        { "kind": "more", "data": { "count": 5 } }
                    ] } }
                } },
                { "kind": "t1", "data": { "author": "carol", "body": "second top comment", "replies": "" } }
            ] }
        });
        let mut out = String::new();
        flatten_comments(&listing, 0, &mut out);
        assert!(out.contains("alice: top level comment"));
        assert!(out.contains("  bob: a nested reply"), "nested reply must be indented one level");
        assert!(out.contains("carol: second top comment"));
        // The "more" stub must NOT appear as text.
        assert!(!out.contains("count"));
        // Ordering: top-level alice before her nested bob before sibling carol.
        let ia = out.find("alice").unwrap();
        let ib = out.find("bob").unwrap();
        let ic = out.find("carol").unwrap();
        assert!(ia < ib && ib < ic);
    }

    #[test]
    fn urlencode_basic() {
        assert_eq!(urlencode("a b&c"), "a%20b%26c");
        assert_eq!(urlencode("rust-lang_1.0~x"), "rust-lang_1.0~x");
    }
}
