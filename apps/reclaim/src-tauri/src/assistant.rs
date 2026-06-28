//! Local AI assistant — a private chat backed by Ollama, grounded in the user's
//! own knowledge (the curator's page summaries + described media downloads).
//!
//! Nothing leaves the device: detection, retrieval, and inference are all local.

use std::sync::Mutex;

use rusqlite::{params, Connection};
use tauri::State;

use crate::ai::OllamaClient;
use crate::AppState;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantStatus {
    pub ollama_running: bool,
    pub model: String,
    pub vram_mb: u64,
}

/// Total GPU VRAM in MB (NVIDIA via nvidia-smi). 0 if no GPU / tool missing.
fn detect_vram_mb() -> u64 {
    std::process::Command::new("nvidia-smi")
        .args(["--query-gpu=memory.total", "--format=csv,noheader,nounits"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.lines().next().map(|l| l.trim().to_string()))
        .and_then(|l| l.parse::<u64>().ok())
        .unwrap_or(0)
}

/// Pick a default chat model by hardware tier (VRAM). Bigger card → bigger model.
fn recommend_model(vram_mb: u64) -> &'static str {
    if vram_mb >= 24_000 {
        "qwen2.5:32b"
    } else if vram_mb >= 12_000 {
        "qwen2.5:14b"
    } else if vram_mb >= 6_000 {
        "llama3.1:8b"
    } else if vram_mb >= 3_000 {
        "llama3.2:3b"
    } else {
        "llama3.2:1b"
    }
}

/// Local model status + the hardware-tier default model.
#[tauri::command(rename_all = "camelCase")]
pub async fn assistant_status() -> Result<AssistantStatus, String> {
    let vram = tauri::async_runtime::spawn_blocking(detect_vram_mb)
        .await
        .unwrap_or(0);
    let running = OllamaClient::new().is_running().await;
    Ok(AssistantStatus {
        ollama_running: running,
        model: recommend_model(vram).to_string(),
        vram_mb: vram,
    })
}

/// Retrieve the most relevant snippets from the user's knowledge: curated page
/// summaries (EarthMemory) + described media downloads. Naive keyword match — a
/// good-enough grounding pass until embeddings land.
fn retrieve_context(db_path: &str, profile_id: i64, query: &str) -> String {
    let conn = match Connection::open(db_path) {
        Ok(c) => c,
        Err(_) => return String::new(),
    };
    let pat = format!("%{}%", query.to_lowercase());
    let mut ctx = String::new();

    if let Ok(mut stmt) = conn.prepare(
        "SELECT title, url, COALESCE(summary, '') FROM indexed_pages
         WHERE profile_id = ?1 AND (LOWER(title) LIKE ?2 OR LOWER(summary) LIKE ?2 OR LOWER(content) LIKE ?2)
         ORDER BY last_visited DESC LIMIT 5",
    ) {
        if let Ok(rows) = stmt.query_map(params![profile_id, pat], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
        }) {
            for (title, url, summary) in rows.flatten() {
                ctx.push_str(&format!("- Page: {} ({})\n  {}\n", title, url, summary));
            }
        }
    }

    if let Ok(mut stmt) = conn.prepare(
        "SELECT COALESCE(description, ''), url, COALESCE(kind, 'media') FROM media_downloads
         WHERE profile_id = ?1 AND LOWER(description) LIKE ?2
         ORDER BY id DESC LIMIT 5",
    ) {
        if let Ok(rows) = stmt.query_map(params![profile_id, pat], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
        }) {
            for (desc, url, kind) in rows.flatten() {
                if !desc.is_empty() {
                    ctx.push_str(&format!("- Saved {}: {} ({})\n", kind, desc, url));
                }
            }
        }
    }

    ctx
}

#[derive(serde::Deserialize)]
pub struct ChatMsg {
    pub role: String,
    pub content: String,
}

/// List the models installed in Ollama (for the model picker). Empty if Ollama
/// isn't reachable.
#[tauri::command(rename_all = "camelCase")]
pub async fn assistant_models() -> Result<Vec<String>, String> {
    let client = reqwest::Client::new();
    let resp = match client
        .get("http://localhost:11434/api/tags")
        .timeout(std::time::Duration::from_secs(3))
        .send()
        .await
    {
        Ok(r) => r,
        Err(_) => return Ok(Vec::new()),
    };
    let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    Ok(v["models"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|m| m["name"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default())
}

async fn resolve_model(model: Option<String>) -> String {
    match model {
        Some(m) if !m.is_empty() => m,
        _ => {
            let vram = tauri::async_runtime::spawn_blocking(detect_vram_mb).await.unwrap_or(0);
            recommend_model(vram).to_string()
        }
    }
}

/// The assistant's pre-prompt (persona + behavior). This is the single source of
/// truth for how the assistant behaves — edit here to tune it. (The curator has
/// its own, separate prompt in `ai.rs::curate`.)
pub const SYSTEM_PROMPT: &str = "\
You are Reclaim — the private, on-device AI assistant built into the Reclaim browser, a \
privacy-first, local-first platform founded on digital sovereignty. Everything you do runs \
locally on the user's machine; their data never leaves the device.

How to behave:
- Be genuinely helpful, clear, and concise. Give the direct answer first; add detail only when it helps.
- Be honest and grounded. If you're unsure or don't know, say so. Never fabricate facts, quotes, URLs, or citations.
- Respect privacy: prefer local/offline solutions; don't push cloud services unless the user asks.
- Be neutral and non-judgemental. Format with light Markdown when it aids readability.

Using the user's knowledge:
- The section below holds context retrieved from the user's OWN saved knowledge — curated page \
summaries, descriptions of media they downloaded, and snippets of past conversations. \
When it's relevant to the question, draw on it and note which item it came from. \
When it isn't relevant, ignore it and answer from general knowledge. \
Never invent knowledge-base entries that aren't shown.";

/// Build the grounded chat message list: the pre-prompt + relevant saved
/// knowledge, then the (capped) history, then the new user message.
fn build_chat_messages(
    db_path: &str,
    profile_id: i64,
    message: &str,
    history: &[ChatMsg],
) -> Vec<(String, String)> {
    let context = retrieve_context(db_path, profile_id, message);
    let system = format!(
        "{}\n\n--- User's saved knowledge ---\n{}",
        SYSTEM_PROMPT,
        if context.trim().is_empty() { "(nothing relevant found for this message)".to_string() } else { context }
    );
    let mut msgs: Vec<(String, String)> = vec![("system".to_string(), system)];
    for h in history.iter().rev().take(12).rev() {
        let role = if h.role == "assistant" { "assistant" } else { "user" };
        msgs.push((role.to_string(), h.content.clone()));
    }
    msgs.push(("user".to_string(), message.to_string()));
    msgs
}

/// Journal a completed exchange into EarthMemory so the assistant (and curator)
/// can reference past conversations. Keyed by a unique `reclaim://chat/...` URL.
fn journal_conversation(
    state: &State<'_, Mutex<AppState>>,
    profile_id: i64,
    question: &str,
    answer: &str,
) {
    if let Ok(s) = state.lock() {
        let title: String = format!("Chat: {}", question.chars().take(60).collect::<String>());
        let content = format!("Q: {}\nA: {}", question, answer);
        let url = format!("reclaim://chat/{}", chrono::Utc::now().to_rfc3339());
        let _ = s
            .memory_manager
            .journal_page(&url, &title, Some(&content), Some(answer), profile_id);
    }
}

/// Non-streaming chat (kept for callers that want a single response).
#[tauri::command(rename_all = "camelCase")]
pub async fn assistant_chat(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    message: String,
    history: Vec<ChatMsg>,
    model: Option<String>,
) -> Result<String, String> {
    let db_path = {
        let s = state.lock().map_err(|e| e.to_string())?;
        s.db_path.clone()
    };
    let client = OllamaClient::new();
    if !client.is_running().await {
        return Err("Ollama isn't running — start it with `ollama serve`.".to_string());
    }
    let model = resolve_model(model).await;
    let msgs = build_chat_messages(&db_path, profile_id, &message, &history);
    client.chat(&model, &msgs).await
}

/// Streaming chat: emits `assistant-chunk` (token text) events as the model
/// generates, then `assistant-done`. Optionally journals the finished exchange.
#[tauri::command(rename_all = "camelCase")]
pub async fn assistant_chat_stream(
    app: tauri::AppHandle,
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    message: String,
    history: Vec<ChatMsg>,
    model: Option<String>,
    journal: bool,
) -> Result<(), String> {
    use tauri::Emitter;

    let db_path = {
        let s = state.lock().map_err(|e| e.to_string())?;
        s.db_path.clone()
    };
    let model = resolve_model(model).await;
    let msgs = build_chat_messages(&db_path, profile_id, &message, &history);
    let payload = serde_json::json!({
        "model": model,
        "stream": true,
        "messages": msgs.iter().map(|(r, c)| serde_json::json!({"role": r, "content": c})).collect::<Vec<_>>(),
    });

    let client = reqwest::Client::new();
    let mut resp = client
        .post("http://localhost:11434/api/chat")
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Ollama isn't reachable: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("ollama chat: HTTP {}", resp.status()));
    }

    let mut buf = String::new();
    let mut full = String::new();
    while let Some(chunk) = resp.chunk().await.map_err(|e| e.to_string())? {
        buf.push_str(&String::from_utf8_lossy(&chunk));
        // Ollama streams newline-delimited JSON objects.
        while let Some(nl) = buf.find('\n') {
            let line: String = buf.drain(..=nl).collect();
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                if let Some(c) = v["message"]["content"].as_str() {
                    if !c.is_empty() {
                        full.push_str(c);
                        let _ = app.emit("assistant-chunk", c);
                    }
                }
            }
        }
    }
    let _ = app.emit("assistant-done", ());

    if journal && !full.trim().is_empty() {
        journal_conversation(&state, profile_id, &message, &full);
    }
    Ok(())
}
