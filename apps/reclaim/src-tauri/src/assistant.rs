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

/// Extra system guidance for research mode (appended to SYSTEM_PROMPT). Describes
/// the tools and how to use them; reinforces the no-fabrication rule.
const RESEARCH_GUIDE: &str = "\
You are in RESEARCH MODE and have two tools:
- web_search(query): search the web for current information (returns title/url/snippet).
- fetch_url(url): fetch and read the readable text of a page.

Use them whenever the question involves current events, recent facts, or anything you're \
unsure of. Search first, then fetch the most relevant 1-3 results to read the details. \
Take as many tool steps as you need (you'll be cut off after a few). When you have enough, \
write the answer grounded in what you actually read, and cite the source URLs you used. \
Never fabricate facts, quotes, or sources — only cite pages you actually fetched.";

/// JSON tools array advertised to Ollama for research mode.
fn research_tools() -> serde_json::Value {
    serde_json::json!([
        { "type": "function", "function": {
            "name": "web_search",
            "description": "Search the web for current information. Returns results with title, url, and snippet.",
            "parameters": { "type": "object",
                "properties": { "query": { "type": "string", "description": "The search query" } },
                "required": ["query"] } } },
        { "type": "function", "function": {
            "name": "fetch_url",
            "description": "Fetch and read the readable text content of a web page by its URL.",
            "parameters": { "type": "object",
                "properties": { "url": { "type": "string", "description": "The URL to fetch" } },
                "required": ["url"] } } }
    ])
}

fn format_search_results(rs: &[crate::research::SearchResult]) -> String {
    if rs.is_empty() {
        return "No results.".to_string();
    }
    rs.iter()
        .enumerate()
        .map(|(i, r)| format!("{}. {}\n   {}\n   {}", i + 1, r.title, r.url, r.snippet))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Stream a final answer from a prepared messages array (no tools — so the model
/// commits to prose). Emits `assistant-chunk` per token; returns the full text.
async fn stream_answer(
    app: &tauri::AppHandle,
    model: &str,
    messages: &serde_json::Value,
) -> Result<String, String> {
    use tauri::Emitter;
    let client = reqwest::Client::new();

    // Ask reasoning models to expose their thinking as a separate `thinking` field
    // (`think: true`). Models that don't support it make Ollama reject the request,
    // so we retry once without `think`.
    let send = |think: bool| {
        let mut payload = serde_json::json!({ "model": model, "stream": true, "messages": messages });
        if think {
            payload["think"] = serde_json::Value::Bool(true);
        }
        client.post("http://localhost:11434/api/chat").json(&payload).send()
    };

    let mut resp = send(true).await.map_err(|e| format!("Ollama isn't reachable: {}", e))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        // Ollama puts a useful message in the body (e.g. model not found) — surface it.
        let detail = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|v| v["error"].as_str().map(str::to_string))
            .unwrap_or(body);
        // If the model just doesn't support thinking, retry without it.
        if detail.to_lowercase().contains("think") {
            resp = send(false).await.map_err(|e| format!("Ollama isn't reachable: {}", e))?;
            if !resp.status().is_success() {
                let s = resp.status();
                let b = resp.text().await.unwrap_or_default();
                return Err(format!("Ollama: {} (HTTP {})", b.trim(), s));
            }
        } else {
            return Err(format!("Ollama: {} (HTTP {})", detail.trim(), status));
        }
    }
    let mut buf = String::new();
    let mut full = String::new();
    while let Some(chunk) = resp.chunk().await.map_err(|e| e.to_string())? {
        buf.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(nl) = buf.find('\n') {
            let line: String = buf.drain(..=nl).collect();
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                // The model's reasoning, streamed separately so the UI can show it.
                if let Some(t) = v["message"]["thinking"].as_str() {
                    if !t.is_empty() {
                        let _ = app.emit("assistant-thinking", t);
                    }
                }
                if let Some(c) = v["message"]["content"].as_str() {
                    if !c.is_empty() {
                        full.push_str(c);
                        let _ = app.emit("assistant-chunk", c);
                    }
                }
            }
        }
    }
    Ok(full)
}

/// Research mode: agentic search + read. Lets the model call `web_search` /
/// `fetch_url` (Ollama tool-calling) up to a few steps, emitting `research-step`
/// progress, then streams the final grounded answer (`assistant-chunk` /
/// `assistant-done`). Falls back to a plain answer if the model lacks tool
/// support. Honors `journal` (Q + answer + sources → EarthMemory).
#[tauri::command(rename_all = "camelCase")]
pub async fn assistant_research_stream(
    app: tauri::AppHandle,
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    message: String,
    history: Vec<ChatMsg>,
    model: Option<String>,
    journal: bool,
    searxng_url: Option<String>,
) -> Result<(), String> {
    use tauri::Emitter;

    let db_path = {
        let s = state.lock().map_err(|e| e.to_string())?;
        s.db_path.clone()
    };
    let client = OllamaClient::new();
    if !client.is_running().await {
        return Err("Ollama isn't running — start it with `ollama serve`.".to_string());
    }
    let model = resolve_model(model).await;

    // Build messages: persona + research guide + local-knowledge context, history, user.
    let context = retrieve_context(&db_path, profile_id, &message);
    let system = format!(
        "{}\n\n{}\n\n--- User's saved knowledge ---\n{}",
        SYSTEM_PROMPT,
        RESEARCH_GUIDE,
        if context.trim().is_empty() {
            "(nothing relevant found for this message)".to_string()
        } else {
            context
        }
    );
    let mut messages: Vec<serde_json::Value> =
        vec![serde_json::json!({ "role": "system", "content": system })];
    for h in history.iter().rev().take(12).rev() {
        let role = if h.role == "assistant" { "assistant" } else { "user" };
        messages.push(serde_json::json!({ "role": role, "content": h.content }));
    }
    messages.push(serde_json::json!({ "role": "user", "content": message }));

    let tools = research_tools();
    let mut sources: Vec<String> = Vec::new();

    // Agentic loop — search/read up to 5 steps.
    for step in 0..5 {
        let msg = match client
            .chat_with_tools(&model, &serde_json::Value::Array(messages.clone()), Some(&tools))
            .await
        {
            Ok(m) => m,
            Err(e) => {
                if step == 0 {
                    let _ = app.emit(
                        "research-step",
                        "⚠ this model can't use tools — answering without web search (try llama3.1 or qwen2.5)",
                    );
                    break;
                }
                return Err(e);
            }
        };

        let tool_calls = msg
            .get("tool_calls")
            .and_then(|t| t.as_array())
            .cloned()
            .unwrap_or_default();
        if tool_calls.is_empty() {
            break; // model is ready to answer
        }

        // Record the assistant's tool-call turn, then run each requested tool.
        messages.push(msg.clone());
        for tc in &tool_calls {
            let name = tc["function"]["name"].as_str().unwrap_or("");
            let args = &tc["function"]["arguments"];
            let result = match name {
                "web_search" => {
                    let q = args["query"].as_str().unwrap_or("").to_string();
                    let _ = app.emit("research-step", format!("🔎 searching: {}", q));
                    match crate::research::search(&q, searxng_url.as_deref()).await {
                        Ok(rs) => {
                            for r in &rs {
                                sources.push(r.url.clone());
                            }
                            format_search_results(&rs)
                        }
                        Err(e) => format!("(search failed: {})", e),
                    }
                }
                "fetch_url" => {
                    let u = args["url"].as_str().unwrap_or("").to_string();
                    let _ = app.emit("research-step", format!("📄 reading: {}", u));
                    match crate::research::fetch(&u).await {
                        Ok(t) => {
                            sources.push(u.clone());
                            t
                        }
                        Err(e) => format!("(fetch failed: {})", e),
                    }
                }
                other => format!("(unknown tool: {})", other),
            };
            messages.push(serde_json::json!({ "role": "tool", "name": name, "content": result }));
        }
    }

    // Final streamed answer using everything gathered.
    let full = stream_answer(&app, &model, &serde_json::Value::Array(messages)).await?;
    let _ = app.emit("assistant-done", ());

    if journal && !full.trim().is_empty() {
        let mut seen = std::collections::HashSet::new();
        let cites: Vec<String> = sources.into_iter().filter(|s| seen.insert(s.clone())).collect();
        let journaled = if cites.is_empty() {
            full.clone()
        } else {
            format!("{}\n\nSources:\n{}", full, cites.join("\n"))
        };
        journal_conversation(&state, profile_id, &message, &journaled);
    }
    Ok(())
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
    // Stream via the shared helper so plain chat also exposes the model's thinking
    // (and the think-unsupported fallback) consistently with research mode.
    let messages = serde_json::Value::Array(
        msgs.iter().map(|(r, c)| serde_json::json!({ "role": r, "content": c })).collect(),
    );
    let full = stream_answer(&app, &model, &messages).await?;
    let _ = app.emit("assistant-done", ());

    if journal && !full.trim().is_empty() {
        journal_conversation(&state, profile_id, &message, &full);
    }
    Ok(())
}
