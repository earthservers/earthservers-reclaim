//! Forums adapter [RELIABLE-ish], default ON. Three tiers, each degrading
//! gracefully to the next:
//!   1. Discourse  — many modern forums; append `.json` to a topic for structured
//!      post+reply JSON.
//!   2. Stack Exchange — official API for the question + answers (+ scores).
//!   3. Generic HTML — SearXNG discovery + scraper::extract_text; when we can't
//!      split posts we emit the whole page as one `forum_post` (honest degradation).
//!
//! Opening post → `forum_post`; replies/answers → `forum_comment`. Any failure
//! returns what we have (often just the opening post) and never aborts the search.

use super::{Candidate, FetchedDoc, Segment, SourceAdapter};
use crate::search_index::kinds::ContentKind;
use std::time::Duration;

const MAX_REPLIES: usize = 100;

pub struct ForumAdapter;

impl ForumAdapter {
    pub fn new() -> Self {
        ForumAdapter
    }
}

impl Default for ForumAdapter {
    fn default() -> Self {
        Self::new()
    }
}

/// Stack Exchange API `site` param for a host, if it's an SE-family site.
fn se_site(host: &str) -> Option<&'static str> {
    match host {
        "stackoverflow.com" => Some("stackoverflow"),
        "serverfault.com" => Some("serverfault"),
        "superuser.com" => Some("superuser"),
        "askubuntu.com" => Some("askubuntu"),
        "mathoverflow.net" => Some("mathoverflow.net"),
        h if h.ends_with(".stackexchange.com") => {
            // <sub>.stackexchange.com → "<sub>"; leak the &'static via a match isn't
            // possible for arbitrary subs, so we signal "generic SE" with a marker.
            // Handled specially by se_site_dynamic below.
            let _ = h;
            Some("__sub_stackexchange__")
        }
        _ => None,
    }
}

/// Resolve the real SE site string (handles the dynamic <sub>.stackexchange.com).
fn se_site_dynamic(host: &str) -> Option<String> {
    match se_site(host)? {
        "__sub_stackexchange__" => host.strip_suffix(".stackexchange.com").map(|s| s.to_string()),
        s => Some(s.to_string()),
    }
}

/// Extract the SE question id from a `/questions/<id>/...` URL.
fn se_question_id(url: &str) -> Option<String> {
    let after = url.split("/questions/").nth(1)?;
    let id: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
    if id.is_empty() {
        None
    } else {
        Some(id)
    }
}

/// Whether a URL looks like a Discourse topic (`/t/<slug>/<id>` or `/t/<id>`).
fn discourse_topic_json(url: &str) -> Option<String> {
    let base = url.split(['?', '#']).next().unwrap_or(url).trim_end_matches('/');
    // .../t/<slug>/<id>[/<post_no>]
    let segs: Vec<&str> = base.split('/').collect();
    let t_pos = segs.iter().position(|s| *s == "t")?;
    // Topic id is the first all-digits segment after /t/.
    let id = segs[t_pos + 1..]
        .iter()
        .find(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))?;
    // Reconstruct scheme://host.
    let idx = base.find("://")?;
    let rest = &base[idx + 3..];
    let host_end = rest.find('/').unwrap_or(rest.len());
    let scheme_host = format!("{}{}", &base[..idx + 3], &rest[..host_end]);
    Some(format!("{}/t/{}.json", scheme_host, id))
}

#[async_trait::async_trait]
impl SourceAdapter for ForumAdapter {
    fn id(&self) -> &'static str {
        "forums"
    }

    async fn discover(&self, query: &str, limit: usize) -> Result<Vec<Candidate>, String> {
        // Generic discovery via the existing meta-search; fetch routing handles the
        // forum-specific extraction. Tag the engine "forum" for the UI.
        let results = crate::research::search(query, None).await?;
        Ok(results
            .into_iter()
            .take(limit)
            .enumerate()
            .map(|(i, r)| Candidate {
                url: r.url,
                title: r.title,
                snippet: r.snippet,
                source_engine: "forum".to_string(),
                searxng_pos: Some(i),
            })
            .collect())
    }

    async fn fetch(&self, url: &str) -> Result<FetchedDoc, String> {
        // Single-doc fallback (used only if fetch_segments somehow isn't called).
        let client = super::http_client();
        let html = client.get(url).send().await.map_err(|e| e.to_string())?
            .text().await.map_err(|e| e.to_string())?;
        Ok(FetchedDoc { url: url.to_string(), title: url.to_string(), body: crate::scraper::extract_text(&html) })
    }

    fn freshness_ttl(&self) -> Duration {
        Duration::from_secs(2 * 86_400)
    }

    fn handles_host(&self, host: &str) -> bool {
        se_site(host).is_some()
    }

    fn max_units(&self) -> usize {
        MAX_REPLIES
    }

    async fn fetch_segments(&self, url: &str, max_units: usize) -> Result<Vec<Segment>, String> {
        let host = super::host_of(url).unwrap_or_default();

        // Tier 2: Stack Exchange API.
        if let Some(site) = se_site_dynamic(&host) {
            if let Some(qid) = se_question_id(url) {
                if let Ok(segs) = self.fetch_stackexchange(&site, &qid, url, max_units).await {
                    if !segs.is_empty() {
                        return Ok(segs);
                    }
                }
            }
        }

        // Tier 1: Discourse topic JSON.
        if let Some(json_url) = discourse_topic_json(url) {
            if let Ok(segs) = self.fetch_discourse(&json_url, url, max_units).await {
                if !segs.is_empty() {
                    return Ok(segs);
                }
            }
        }

        // Tier 3: generic — whole page as one forum_post (can't reliably split).
        let doc = self.fetch(url).await?;
        Ok(vec![Segment {
            kind: ContentKind::ForumPost,
            url: doc.url,
            parent_url: None,
            title: Some(doc.title),
            text: doc.body,
            author: None,
            engagement: None,
        }])
    }
}

impl ForumAdapter {
    async fn fetch_stackexchange(
        &self,
        site: &str,
        qid: &str,
        url: &str,
        max_units: usize,
    ) -> Result<Vec<Segment>, String> {
        let client = super::http_client();
        // withbody filter returns the rendered HTML body.
        let q_url = format!(
            "https://api.stackexchange.com/2.3/questions/{}?site={}&filter=withbody",
            qid, site
        );
        let qv: serde_json::Value =
            client.get(&q_url).send().await.map_err(|e| e.to_string())?
                .json().await.map_err(|e| e.to_string())?;
        let mut segs = Vec::new();
        if let Some(item) = qv["items"].as_array().and_then(|a| a.first()) {
            segs.push(Segment {
                kind: ContentKind::ForumPost,
                url: url.to_string(),
                parent_url: None,
                title: item["title"].as_str().map(|s| s.to_string()),
                text: crate::scraper::extract_text(item["body"].as_str().unwrap_or("")),
                author: item["owner"]["display_name"].as_str().map(|s| s.to_string()),
                engagement: item["score"].as_i64(),
            });
        }
        let a_url = format!(
            "https://api.stackexchange.com/2.3/questions/{}/answers?site={}&filter=withbody&sort=votes&order=desc&pagesize={}",
            qid, site, max_units.min(MAX_REPLIES)
        );
        let av: serde_json::Value =
            client.get(&a_url).send().await.map_err(|e| e.to_string())?
                .json().await.map_err(|e| e.to_string())?;
        if let Some(answers) = av["items"].as_array() {
            for a in answers.iter().take(max_units) {
                let aid = a["answer_id"].as_i64().unwrap_or(0);
                segs.push(Segment {
                    kind: ContentKind::ForumComment,
                    url: format!("{}#answer-{}", url.split('#').next().unwrap_or(url), aid),
                    parent_url: Some(url.to_string()),
                    title: None,
                    text: crate::scraper::extract_text(a["body"].as_str().unwrap_or("")),
                    author: a["owner"]["display_name"].as_str().map(|s| s.to_string()),
                    engagement: a["score"].as_i64(),
                });
            }
        }
        Ok(segs)
    }

    async fn fetch_discourse(
        &self,
        json_url: &str,
        canonical: &str,
        max_units: usize,
    ) -> Result<Vec<Segment>, String> {
        let client = super::http_client();
        let v: serde_json::Value =
            client.get(json_url).send().await.map_err(|e| e.to_string())?
                .json().await.map_err(|e| e.to_string())?;
        let title = v["title"].as_str().map(|s| s.to_string());
        let posts = v["post_stream"]["posts"].as_array().cloned().unwrap_or_default();
        let mut segs = Vec::new();
        for (i, p) in posts.iter().take(max_units + 1).enumerate() {
            let text = crate::scraper::extract_text(p["cooked"].as_str().unwrap_or(""));
            if text.trim().is_empty() {
                continue;
            }
            let post_no = p["post_number"].as_i64().unwrap_or((i + 1) as i64);
            segs.push(Segment {
                kind: if i == 0 { ContentKind::ForumPost } else { ContentKind::ForumComment },
                url: format!("{}/{}", canonical.trim_end_matches('/'), post_no),
                parent_url: if i == 0 { None } else { Some(canonical.to_string()) },
                title: if i == 0 { title.clone() } else { None },
                text,
                author: p["username"].as_str().map(|s| s.to_string()),
                engagement: p["score"].as_f64().map(|f| f as i64),
            });
        }
        Ok(segs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn se_url_parsing() {
        assert_eq!(se_question_id("https://stackoverflow.com/questions/12345/how-to"), Some("12345".into()));
        assert_eq!(se_question_id("https://stackoverflow.com/tags/rust"), None);
        assert_eq!(se_site_dynamic("stackoverflow.com").as_deref(), Some("stackoverflow"));
        assert_eq!(se_site_dynamic("rust.stackexchange.com").as_deref(), Some("rust"));
        assert_eq!(se_site_dynamic("example.com"), None);
    }

    #[test]
    fn discourse_topic_detection() {
        assert_eq!(
            discourse_topic_json("https://meta.discourse.org/t/some-slug/12345"),
            Some("https://meta.discourse.org/t/12345.json".into())
        );
        assert_eq!(
            discourse_topic_json("https://forum.example.org/t/slug/999/4"),
            Some("https://forum.example.org/t/999.json".into())
        );
        assert_eq!(discourse_topic_json("https://example.com/article/foo"), None);
    }
}
