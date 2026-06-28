//! Private web-research: search + read for the assistant's agentic research mode.
//!
//! Privacy-first: the only network traffic is the search query + the pages the
//! model chooses to read. Prefers a local SearXNG instance (nothing leaves your
//! network); falls back to DuckDuckGo's HTML endpoint. All reasoning stays local
//! in Ollama.

use std::time::Duration;

use serde::Serialize;

const BROWSER_UA: &str =
    "Mozilla/5.0 (X11; Linux x86_64; rv:128.0) Gecko/20100101 Firefox/128.0";
const DEFAULT_SEARXNG: &str = "http://localhost:8888";

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ResearchStatus {
    /// "searxng" when a local instance answers, else "duckduckgo".
    pub provider: String,
    pub searxng_url: String,
    pub searxng_available: bool,
}

fn searxng_base(searxng_url: Option<&str>) -> String {
    let raw = searxng_url
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_SEARXNG);
    raw.trim_end_matches('/').to_string()
}

/// Ping the SearXNG JSON API (like we ping Ollama). True only if it returns a
/// usable JSON search response — confirms the instance AND that json format is
/// enabled, not just that the host is up.
async fn searxng_available(base: &str) -> bool {
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/search", base))
        .query(&[("q", "ping"), ("format", "json")])
        .header("User-Agent", BROWSER_UA)
        .timeout(Duration::from_secs(2))
        .send()
        .await;
    match resp {
        Ok(r) if r.status().is_success() => r
            .json::<serde_json::Value>()
            .await
            .map(|v| v.get("results").is_some())
            .unwrap_or(false),
        _ => false,
    }
}

async fn searxng_search(base: &str, query: &str) -> Result<Vec<SearchResult>, String> {
    let client = reqwest::Client::new();
    let v: serde_json::Value = client
        .get(format!("{}/search", base))
        .query(&[("q", query), ("format", "json")])
        .header("User-Agent", BROWSER_UA)
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let results = v["results"].as_array().cloned().unwrap_or_default();
    Ok(results
        .iter()
        .filter_map(|r| {
            let url = r["url"].as_str()?.to_string();
            if url.is_empty() {
                return None;
            }
            Some(SearchResult {
                title: r["title"].as_str().unwrap_or("").to_string(),
                url,
                snippet: r["content"].as_str().unwrap_or("").to_string(),
            })
        })
        .take(8)
        .collect())
}

/// Minimal tag-strip + entity decode for DDG result titles/snippets.
fn clean_fragment(s: &str) -> String {
    let no_tags = regex::Regex::new(r"<[^>]+>").unwrap().replace_all(s, "");
    let whitespace = regex::Regex::new(r"\s+").unwrap();
    let collapsed = whitespace.replace_all(&no_tags, " ");
    collapsed
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&#x2F;", "/")
        .trim()
        .to_string()
}

/// DuckDuckGo result links are redirects (`//duckduckgo.com/l/?uddg=<encoded>`);
/// pull the real target out of the `uddg` query param. Returns "" if it's not a
/// resolvable external URL.
fn decode_ddg_url(href: &str) -> String {
    let abs = if href.starts_with("//") {
        format!("https:{}", href)
    } else {
        href.to_string()
    };
    if let Ok(u) = url::Url::parse(&abs) {
        if let Some((_, v)) = u.query_pairs().find(|(k, _)| k == "uddg") {
            return v.to_string();
        }
        // Some results link out directly (not via the redirect).
        if u.host_str().map(|h| !h.contains("duckduckgo")).unwrap_or(false)
            && (u.scheme() == "http" || u.scheme() == "https")
        {
            return abs;
        }
    }
    String::new()
}

fn parse_ddg_html(html: &str) -> Vec<SearchResult> {
    let a_re = regex::Regex::new(
        r#"(?is)<a[^>]+class="[^"]*result__a[^"]*"[^>]+href="([^"]+)"[^>]*>(.*?)</a>"#,
    )
    .unwrap();
    let s_re =
        regex::Regex::new(r#"(?is)class="[^"]*result__snippet[^"]*"[^>]*>(.*?)</a>"#).unwrap();

    let snippets: Vec<String> = s_re
        .captures_iter(html)
        .map(|c| clean_fragment(&c[1]))
        .collect();

    let mut out = Vec::new();
    for (i, cap) in a_re.captures_iter(html).enumerate() {
        let url = decode_ddg_url(&cap[1]);
        let title = clean_fragment(&cap[2]);
        if url.is_empty() || title.is_empty() {
            continue;
        }
        out.push(SearchResult {
            title,
            url,
            snippet: snippets.get(i).cloned().unwrap_or_default(),
        });
        if out.len() >= 8 {
            break;
        }
    }
    out
}

async fn duckduckgo_search(query: &str) -> Result<Vec<SearchResult>, String> {
    let client = reqwest::Client::new();
    let html = client
        .get("https://html.duckduckgo.com/html/")
        .query(&[("q", query)])
        .header("User-Agent", BROWSER_UA)
        .header("Referer", "https://duckduckgo.com/")
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    Ok(parse_ddg_html(&html))
}

/// Search the web. Uses SearXNG when reachable (max privacy), else DuckDuckGo.
/// Internal entry point used by the agentic loop and the `web_search` command.
pub async fn search(query: &str, searxng_url: Option<&str>) -> Result<Vec<SearchResult>, String> {
    let base = searxng_base(searxng_url);
    if searxng_available(&base).await {
        // If SearXNG is up but returns nothing, fall back rather than dead-end.
        match searxng_search(&base, query).await {
            Ok(r) if !r.is_empty() => return Ok(r),
            _ => {}
        }
    }
    duckduckgo_search(query).await
}

/// Fetch a URL and return its readable text, truncated to ~2000 words.
pub async fn fetch(url: &str) -> Result<String, String> {
    let client = reqwest::Client::new();
    let html = client
        .get(url)
        .header("User-Agent", BROWSER_UA)
        .header("Referer", "https://duckduckgo.com/")
        .timeout(Duration::from_secs(20))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    let text = crate::scraper::extract_text(&html);
    Ok(text.split_whitespace().take(2000).collect::<Vec<_>>().join(" "))
}

// ==================== Tauri commands ====================

#[tauri::command(rename_all = "camelCase")]
pub async fn web_search(
    query: String,
    searxng_url: Option<String>,
) -> Result<Vec<SearchResult>, String> {
    search(&query, searxng_url.as_deref()).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn fetch_url(url: String) -> Result<String, String> {
    fetch(&url).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn research_status(searxng_url: Option<String>) -> Result<ResearchStatus, String> {
    let base = searxng_base(searxng_url.as_deref());
    let available = searxng_available(&base).await;
    Ok(ResearchStatus {
        provider: if available { "searxng" } else { "duckduckgo" }.to_string(),
        searxng_url: base,
        searxng_available: available,
    })
}
