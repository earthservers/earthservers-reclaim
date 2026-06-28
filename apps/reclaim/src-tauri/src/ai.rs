// AI runtime integration with Ollama
// Handles embeddings, LLM inference, and the knowledge-graph page curator.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Small instruct model used by the background curator to summarize pages. Kept
/// deliberately tiny so summarization never competes with a larger assistant
/// model for VRAM. Override-able later via settings.
pub const DEFAULT_SUMMARY_MODEL: &str = "llama3.2:3b";
/// Lightweight embedding model (for future semantic search over the graph).
#[allow(dead_code)]
pub const DEFAULT_EMBED_MODEL: &str = "all-minilm";

pub struct OllamaClient {
    base_url: String,
    client: reqwest::Client,
}

impl OllamaClient {
    pub fn new() -> Self {
        OllamaClient {
            base_url: "http://localhost:11434".to_string(),
            client: reqwest::Client::new(),
        }
    }

    /// True if a local Ollama daemon answers. Short timeout so a missing daemon
    /// fails fast and the curator silently no-ops.
    pub async fn is_running(&self) -> bool {
        self.client
            .get(format!("{}/api/tags", self.base_url))
            .timeout(Duration::from_secs(2))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    /// One-shot (non-streaming) text generation.
    pub async fn generate(&self, prompt: &str, model: &str) -> Result<String, String> {
        #[derive(Serialize)]
        struct Req<'a> {
            model: &'a str,
            prompt: &'a str,
            stream: bool,
        }
        #[derive(Deserialize)]
        struct Resp {
            response: String,
        }
        let resp = self
            .client
            .post(format!("{}/api/generate", self.base_url))
            .json(&Req { model, prompt, stream: false })
            .timeout(Duration::from_secs(120))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("ollama generate: HTTP {}", resp.status()));
        }
        let body: Resp = resp.json().await.map_err(|e| e.to_string())?;
        Ok(body.response)
    }

    /// Multi-turn chat (Ollama /api/chat). `messages` are (role, content) pairs.
    pub async fn chat(&self, model: &str, messages: &[(String, String)]) -> Result<String, String> {
        #[derive(Serialize)]
        struct M<'a> {
            role: &'a str,
            content: &'a str,
        }
        #[derive(Serialize)]
        struct Req<'a> {
            model: &'a str,
            messages: Vec<M<'a>>,
            stream: bool,
        }
        #[derive(Deserialize)]
        struct RespMsg {
            content: String,
        }
        #[derive(Deserialize)]
        struct Resp {
            message: RespMsg,
        }
        let ms: Vec<M> = messages.iter().map(|(r, c)| M { role: r, content: c }).collect();
        let resp = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .json(&Req { model, messages: ms, stream: false })
            .timeout(Duration::from_secs(300))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("ollama chat: HTTP {}", resp.status()));
        }
        let body: Resp = resp.json().await.map_err(|e| e.to_string())?;
        Ok(body.message.content)
    }

    /// Tool-aware chat. `messages` is a raw JSON array (so it can carry
    /// `tool_calls` and `role:"tool"` results); `tools` is an optional JSON tools
    /// array. Returns the assistant `message` object as-is — the caller inspects
    /// `message.tool_calls` to drive the agentic loop. Non-streaming.
    pub async fn chat_with_tools(
        &self,
        model: &str,
        messages: &serde_json::Value,
        tools: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, String> {
        let mut body = serde_json::json!({
            "model": model,
            "stream": false,
            "messages": messages,
        });
        if let Some(t) = tools {
            body["tools"] = t.clone();
        }
        let resp = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .json(&body)
            .timeout(Duration::from_secs(300))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            let detail = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| v["error"].as_str().map(str::to_string))
                .unwrap_or(body);
            return Err(format!("Ollama: {} (HTTP {})", detail.trim(), status));
        }
        let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
        Ok(v["message"].clone())
    }

    /// Embedding vector for `text` (unused for now; wired for future semantic
    /// search over the knowledge graph).
    #[allow(dead_code)]
    pub async fn generate_embedding(&self, text: &str, model: &str) -> Result<Vec<f32>, String> {
        #[derive(Serialize)]
        struct Req<'a> {
            model: &'a str,
            prompt: &'a str,
        }
        #[derive(Deserialize)]
        struct Resp {
            embedding: Vec<f32>,
        }
        let resp = self
            .client
            .post(format!("{}/api/embeddings", self.base_url))
            .json(&Req { model, prompt: text })
            .timeout(Duration::from_secs(60))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("ollama embeddings: HTTP {}", resp.status()));
        }
        let body: Resp = resp.json().await.map_err(|e| e.to_string())?;
        Ok(body.embedding)
    }
}

/// Background curator: fetch a visited page, summarize it with the local model,
/// and journal it into EarthMemory (the knowledge graph). Caller guarantees the
/// page is http(s) and NOT incognito.
///
/// v1 fetches the page server-side and extracts readable text. That's simple and
/// fully decoupled from the embedded webview, but won't see auth'd/SPA-rendered
/// content — the upgrade path is to pull `document.body.innerText` from the live
/// webview via the page↔Rust bridge.
pub async fn curate(db_path: &str, profile_id: i64, url: &str, title: &str) -> Result<(), String> {
    // Skip low-value pages (homepages/feeds) — they're noise and churn storage.
    if !should_curate(url) {
        return Err("skipped: low-value page (homepage/feed)".into());
    }

    let mm = crate::memory::MemoryManager::new(db_path.to_string());
    // Already summarized recently? Just bump the visit count — no refetch, no LLM,
    // no duplicate content write. (One row per URL; revisits update in place.)
    if mm.curated_recently(url, profile_id, 7 * 24 * 3600) {
        let _ = mm.journal_page(url, title, None, None, profile_id);
        return Ok(());
    }

    let client = OllamaClient::new();
    if !client.is_running().await {
        return Err("ollama not running — skipping".into());
    }

    let http = reqwest::Client::new();
    let html = http
        .get(url)
        .header(
            "User-Agent",
            "Mozilla/5.0 (X11; Linux x86_64; rv:128.0) Gecko/20100101 Firefox/128.0",
        )
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())?;

    let text = crate::scraper::extract_text(&html);
    if text.split_whitespace().next().is_none() {
        return Err("no extractable text".into());
    }

    let summary = summarize(title, &text)
        .await
        .ok_or_else(|| "empty summary".to_string())?;

    // Store only a SMALL excerpt for keyword recall — the assistant grounds on the
    // (now larger) summary, so we keep the saved raw content tight to save space.
    let excerpt: String = text.split_whitespace().take(160).collect::<Vec<_>>().join(" ");

    mm.journal_page(url, title, Some(&excerpt), Some(&summary), profile_id)
        .map_err(|e| e.to_string())
}

/// Curate from text the user ACTUALLY VIEWED in the live webview (sent by the
/// viewed-content bridge) rather than re-fetching the page server-side. This sees
/// auth'd/SPA content and only what was scrolled into view (comments included
/// only if reached). Frontend owns the gating/dedup; here we just summarize +
/// journal. Still skips low-value pages.
pub async fn curate_viewed(
    db_path: &str,
    profile_id: i64,
    url: &str,
    title: &str,
    text: &str,
) -> Result<(), String> {
    if !should_curate(url) {
        return Err("skipped: low-value page (homepage/feed)".into());
    }
    if text.split_whitespace().count() < 30 {
        return Err("not enough viewed text".into());
    }
    let summary = summarize(title, text)
        .await
        .ok_or_else(|| "no summary (ollama not running?)".to_string())?;
    let excerpt: String = text.split_whitespace().take(160).collect::<Vec<_>>().join(" ");
    crate::memory::MemoryManager::new(db_path.to_string())
        .journal_page(url, title, Some(&excerpt), Some(&summary), profile_id)
        .map_err(|e| e.to_string())
}

/// Heuristic: keep content pages, skip site homepages and feed/aggregator
/// landing pages. e.g. `youtube.com/` and `youtube.com/feed/...` are skipped but
/// `youtube.com/watch?v=...` is kept. Tunable.
fn should_curate(url: &str) -> bool {
    let parsed = match url::Url::parse(url) {
        Ok(u) => u,
        Err(_) => return false,
    };
    let path = parsed.path().trim_end_matches('/').to_ascii_lowercase();
    if path.is_empty() {
        return false; // bare homepage
    }
    const SKIP: [&str; 7] = [
        "/feed", "/explore", "/trending", "/subscriptions", "/home", "/results", "/search",
    ];
    if SKIP
        .iter()
        .any(|p| path == *p || path.starts_with(&format!("{p}/")))
    {
        return false;
    }
    true
}

/// Summarize page text with the local model into a richer (one-to-two paragraph)
/// neutral summary. Returns None if Ollama isn't running or yields nothing.
/// Shared by the curator and the opt-in scraper → EarthMemory path.
pub async fn summarize(title: &str, text: &str) -> Option<String> {
    let client = OllamaClient::new();
    if !client.is_running().await {
        return None;
    }
    // Feed the model more context than we store, for a better summary.
    let excerpt: String = text.split_whitespace().take(2500).collect::<Vec<_>>().join(" ");
    if excerpt.trim().is_empty() {
        return None;
    }
    // TRANSPARENT, UNBIASED, NON-JUDGEMENTAL: describe, never editorialize.
    let prompt = format!(
        "You are a neutral archivist building a personal knowledge journal. \
         Write a thorough but concise summary of what this web page is about: the \
         main topic, the key points or claims, and notable specifics (names, \
         numbers, conclusions). Aim for one to two short paragraphs (about 6-10 \
         sentences). Be transparent and strictly objective — describe the content \
         only. Do NOT judge, rate, praise, criticize, moralize, warn, or express \
         any opinion or sentiment about the topic or the reader. No preamble.\n\n\
         Title: {title}\n\nContent:\n{excerpt}"
    );
    client
        .generate(&prompt, DEFAULT_SUMMARY_MODEL)
        .await
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}
