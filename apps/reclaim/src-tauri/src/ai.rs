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
    // Cap the prompt so summarization stays fast and within context.
    let excerpt: String = text.split_whitespace().take(1200).collect::<Vec<_>>().join(" ");
    if excerpt.trim().is_empty() {
        return Err("no extractable text".into());
    }

    // The curator must be TRANSPARENT, UNBIASED, and NON-JUDGEMENTAL: it records
    // what a page is about, it does not editorialize, rate, moralize, or take a
    // stance. Neutral, factual, descriptive only.
    let prompt = format!(
        "You are a neutral archivist building a personal knowledge journal. \
         Summarize what this web page is about in 2-3 concise, factual sentences. \
         Be transparent and strictly objective: describe the content and key points \
         only. Do NOT judge, rate, praise, criticize, moralize, warn, or express any \
         opinion or sentiment about the topic or the reader. No preamble.\n\n\
         Title: {title}\n\nContent:\n{excerpt}"
    );
    let summary = client.generate(&prompt, DEFAULT_SUMMARY_MODEL).await?.trim().to_string();
    if summary.is_empty() {
        return Err("empty summary".into());
    }

    crate::memory::MemoryManager::new(db_path.to_string())
        .journal_page(url, title, Some(&excerpt), Some(&summary), profile_id)
        .map_err(|e| e.to_string())
}
