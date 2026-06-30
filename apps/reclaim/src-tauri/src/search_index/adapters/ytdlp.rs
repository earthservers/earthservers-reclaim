//! yt-dlp-backed adapters: YouTube [RELIABLE per-video] and TikTok [BEST-EFFORT].
//!
//! No API keys. We shell out to `yt-dlp` (the same binary the media downloader
//! uses) with a hard timeout + kill-on-drop so a throttled/hung fetch can never
//! stall the search. If yt-dlp is missing or the platform throttles, we return what
//! we have (often just the caption/description) and never abort the batch.

use super::{Candidate, FetchedDoc, Segment, SourceAdapter};
use crate::search_index::kinds::ContentKind;
use std::process::Stdio;
use std::time::Duration;

const YTDLP_TIMEOUT: Duration = Duration::from_secs(90);
const YOUTUBE_MAX_COMMENTS: usize = 50; // modest; YouTube rate-limits comment pulls
const TIKTOK_MAX_UNITS: usize = 20;

/// Run yt-dlp and parse one JSON object from stdout. `kill_on_drop` + timeout means
/// a hung child is reaped, not leaked into a stall.
async fn run_ytdlp_json(target: &str, args: &[&str]) -> Result<serde_json::Value, String> {
    let mut child = tokio::process::Command::new("yt-dlp")
        .args(args)
        .arg(target)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("yt-dlp not available: {}", e))?;
    let out = match tokio::time::timeout(YTDLP_TIMEOUT, async {
        let mut buf = Vec::new();
        use tokio::io::AsyncReadExt;
        if let Some(mut so) = child.stdout.take() {
            let _ = so.read_to_end(&mut buf).await;
        }
        let status = child.wait().await;
        (buf, status)
    })
    .await
    {
        Ok(v) => v,
        Err(_) => return Err("yt-dlp timed out".to_string()),
    };
    let (buf, status) = out;
    match status {
        Ok(s) if s.success() => {}
        _ => return Err("yt-dlp exited with error".to_string()),
    }
    serde_json::from_slice(&buf).map_err(|e| format!("yt-dlp json parse: {}", e))
}

// ---------------- YouTube ----------------

pub struct YoutubeAdapter;
impl YoutubeAdapter {
    pub fn new() -> Self {
        YoutubeAdapter
    }
}
impl Default for YoutubeAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl SourceAdapter for YoutubeAdapter {
    fn id(&self) -> &'static str {
        "youtube"
    }

    async fn discover(&self, query: &str, limit: usize) -> Result<Vec<Candidate>, String> {
        // `ytsearchN:` discovery via yt-dlp — no API key needed.
        let target = format!("ytsearch{}:{}", limit.clamp(1, 25), query);
        let v = run_ytdlp_json(&target, &["-J", "--flat-playlist", "--no-warnings"]).await?;
        let entries = v["entries"].as_array().cloned().unwrap_or_default();
        Ok(entries
            .iter()
            .filter_map(|e| {
                let id = e["id"].as_str()?;
                Some(Candidate {
                    url: e["url"].as_str().map(|s| s.to_string())
                        .unwrap_or_else(|| format!("https://www.youtube.com/watch?v={}", id)),
                    title: e["title"].as_str().unwrap_or("").to_string(),
                    snippet: e["description"].as_str().unwrap_or("").chars().take(160).collect(),
                    source_engine: "youtube".to_string(),
                    searxng_pos: None,
                })
            })
            .take(limit)
            .collect())
    }

    async fn fetch(&self, url: &str) -> Result<FetchedDoc, String> {
        let v = run_ytdlp_json(url, &["-J", "--no-warnings"]).await?;
        Ok(FetchedDoc {
            url: url.to_string(),
            title: v["title"].as_str().unwrap_or("").to_string(),
            body: v["description"].as_str().unwrap_or("").to_string(),
        })
    }

    fn freshness_ttl(&self) -> Duration {
        Duration::from_secs(86_400)
    }

    fn handles_host(&self, host: &str) -> bool {
        host == "youtube.com" || host == "youtu.be" || host == "m.youtube.com" || host.ends_with(".youtube.com")
    }

    fn max_units(&self) -> usize {
        YOUTUBE_MAX_COMMENTS
    }

    async fn fetch_segments(&self, url: &str, max_units: usize) -> Result<Vec<Segment>, String> {
        let n = max_units.min(YOUTUBE_MAX_COMMENTS);
        let extractor = format!("youtube:max_comments={}", n);
        let v = run_ytdlp_json(
            url,
            &["-J", "--write-comments", "--no-warnings", "--extractor-args", &extractor],
        )
        .await?;
        let title = v["title"].as_str().unwrap_or("").to_string();
        let description = v["description"].as_str().unwrap_or("");
        let uploader = v["uploader"].as_str().map(|s| s.to_string());

        let mut segs = Vec::new();
        // Description/caption as the post.
        let post_text = if description.trim().is_empty() {
            title.clone()
        } else {
            format!("{}\n\n{}", title, description)
        };
        segs.push(Segment {
            kind: ContentKind::Post,
            url: url.to_string(),
            parent_url: None,
            title: Some(title),
            text: post_text,
            author: uploader,
            engagement: v["like_count"].as_i64(),
        });
        // Comments.
        if let Some(comments) = v["comments"].as_array() {
            for c in comments.iter().take(n) {
                let text = c["text"].as_str().unwrap_or("").trim().to_string();
                if text.is_empty() {
                    continue;
                }
                let cid = c["id"].as_str().unwrap_or("");
                segs.push(Segment {
                    kind: ContentKind::Comment,
                    url: format!("{}&lc={}", url.split('&').next().unwrap_or(url), cid),
                    parent_url: Some(url.to_string()),
                    title: None,
                    text,
                    author: c["author"].as_str().map(|s| s.to_string()),
                    engagement: c["like_count"].as_i64(),
                });
            }
        }
        Ok(segs)
    }
}

// ---------------- TikTok ----------------

pub struct TiktokAdapter;
impl TiktokAdapter {
    pub fn new() -> Self {
        TiktokAdapter
    }
}
impl Default for TiktokAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl SourceAdapter for TiktokAdapter {
    fn id(&self) -> &'static str {
        "tiktok"
    }

    async fn discover(&self, query: &str, limit: usize) -> Result<Vec<Candidate>, String> {
        // No reliable keyless TikTok search; discover via meta-search, keep TikTok URLs.
        let results = crate::research::search(query, None).await?;
        Ok(results
            .into_iter()
            .filter(|r| r.url.contains("tiktok.com"))
            .take(limit)
            .map(|r| Candidate {
                url: r.url,
                title: r.title,
                snippet: r.snippet,
                source_engine: "tiktok".to_string(),
                searxng_pos: None,
            })
            .collect())
    }

    async fn fetch(&self, url: &str) -> Result<FetchedDoc, String> {
        let v = run_ytdlp_json(url, &["-J", "--no-warnings"]).await?;
        Ok(FetchedDoc {
            url: url.to_string(),
            title: v["title"].as_str().or(v["description"].as_str()).unwrap_or("").to_string(),
            body: v["description"].as_str().unwrap_or("").to_string(),
        })
    }

    fn freshness_ttl(&self) -> Duration {
        Duration::from_secs(86_400)
    }

    fn handles_host(&self, host: &str) -> bool {
        host == "tiktok.com" || host.ends_with(".tiktok.com")
    }

    fn max_units(&self) -> usize {
        TIKTOK_MAX_UNITS
    }

    fn reliability(&self) -> &'static str {
        "best-effort"
    }

    async fn fetch_segments(&self, url: &str, _max_units: usize) -> Result<Vec<Segment>, String> {
        // yt-dlp gives the caption reliably; comment support is fragile/absent, so
        // we surface the caption as a `post` and best-effort any comments present.
        let v = run_ytdlp_json(url, &["-J", "--no-warnings"]).await?;
        let caption = v["description"].as_str().or(v["title"].as_str()).unwrap_or("").to_string();
        let mut segs = Vec::new();
        if !caption.trim().is_empty() {
            segs.push(Segment {
                kind: ContentKind::Post,
                url: url.to_string(),
                parent_url: None,
                title: v["title"].as_str().map(|s| s.to_string()),
                text: caption,
                author: v["uploader"].as_str().map(|s| s.to_string()),
                engagement: v["like_count"].as_i64(),
            });
        }
        if let Some(comments) = v["comments"].as_array() {
            for (i, c) in comments.iter().take(TIKTOK_MAX_UNITS).enumerate() {
                let text = c["text"].as_str().unwrap_or("").trim().to_string();
                if text.is_empty() {
                    continue;
                }
                segs.push(Segment {
                    kind: ContentKind::Comment,
                    url: format!("{}#c{}", url, c["id"].as_str().map(|s| s.to_string()).unwrap_or_else(|| i.to_string())),
                    parent_url: Some(url.to_string()),
                    title: None,
                    text,
                    author: c["author"].as_str().map(|s| s.to_string()),
                    engagement: c["like_count"].as_i64(),
                });
            }
        }
        Ok(segs)
    }
}
