//! Phase 4 — the `local_search` orchestrator command.
//!
//! Fans out across adapters, serves two-speed (shallow snippets immediately, deep
//! scraped+indexed results streamed as they land), writes the index, then emits a
//! final fused ranking. Mirrors the streaming style of `assistant_research_stream`
//! (app.emit per step). Every event is tagged with `query_id` so the frontend can
//! supersede a stale search.

use super::adapters::{host_of, AdapterRegistry, Candidate};
use super::retention::Retention;
use super::store;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{Emitter, Manager};
use tokio::sync::Semaphore;

/// Global cap on concurrent scrapes, and a tighter sub-cap for Reddit (post-2023
/// rate limits — hammering earns 429s).
const GLOBAL_SCRAPE_CONCURRENCY: usize = 6;
const REDDIT_SCRAPE_CONCURRENCY: usize = 2;
/// Default candidate count and how many of the misses we actually scrape.
const DEFAULT_LIMIT: usize = 20;
const SCRAPE_TOP_N: usize = 10;

pub fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// sha256 of the normalized (whitespace-collapsed, lowercased) body — for dedup
/// and the curator's redundancy detection.
pub fn content_hash(body: &str) -> String {
    let norm = body.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
    let mut h = Sha256::new();
    h.update(norm.as_bytes());
    format!("{:x}", h.finalize())
}

// ---- event payloads (camelCase for the React side) ----

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct StartedEvent {
    query_id: i64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ShallowCandidate {
    url: String,
    title: String,
    snippet: String,
    source_engine: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ShallowEvent {
    query_id: i64,
    candidate: ShallowCandidate,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DeepPage {
    page_id: i64,
    url: String,
    title: String,
    snippet: String,
    source_engine: String,
    /// true = served from the warm local index, false = freshly scraped now.
    cache_hit: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DeepEvent {
    query_id: i64,
    page: DeepPage,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct RankedEvent {
    query_id: i64,
    ranked: Vec<super::rank::RankedResult>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DoneEvent {
    query_id: i64,
}

/// The central command. Returns the query_id (also emitted via local-search-started
/// so the frontend can set its "active" id before events stream in).
#[tauri::command(rename_all = "camelCase")]
pub async fn local_search(
    app: tauri::AppHandle,
    query: String,
    retention: String,
    profile_id: i64,
    sources: Option<Vec<String>>,
    limit: Option<usize>,
    searxng_url: Option<String>,
) -> Result<i64, String> {
    let db_path = {
        let state = app.state::<std::sync::Mutex<crate::AppState>>();
        let st = state.lock().map_err(|e| e.to_string())?;
        st.db_path.clone()
    };

    let tier = Retention::parse(&retention);
    let now = now_secs();
    let limit = limit.unwrap_or(DEFAULT_LIMIT).clamp(1, 50);

    // 1. Record the search; emit started so the UI learns query_id immediately.
    let query_id = {
        let conn = super::SearchIndexManager::open(&db_path).map_err(|e| e.to_string())?;
        store::insert_query(&conn, &query, tier.as_str(), now, profile_id).map_err(|e| e.to_string())?
    };
    let _ = app.emit("local-search-started", StartedEvent { query_id });

    let registry = Arc::new(AdapterRegistry::default_set(searxng_url));

    // 2. Shallow: discover across the selected adapters concurrently, emitting each
    //    candidate as it arrives so the box is never empty. Dedup by URL.
    let discover_adapters = registry.discovery_adapters(&sources);
    let mut discover_futs = futures_util::stream::FuturesUnordered::new();
    for adapter in discover_adapters {
        let q = query.clone();
        discover_futs.push(async move {
            (adapter.id(), adapter.discover(&q, limit).await)
        });
    }

    let mut seen: HashSet<String> = HashSet::new();
    let mut candidates: Vec<Candidate> = Vec::new();
    {
        use futures_util::StreamExt;
        while let Some((id, res)) = discover_futs.next().await {
            match res {
                Ok(list) => {
                    for c in list {
                        if c.url.is_empty() || !seen.insert(c.url.clone()) {
                            continue;
                        }
                        let _ = app.emit(
                            "local-search-shallow",
                            ShallowEvent {
                                query_id,
                                candidate: ShallowCandidate {
                                    url: c.url.clone(),
                                    title: c.title.clone(),
                                    snippet: c.snippet.clone(),
                                    source_engine: c.source_engine.clone(),
                                },
                            },
                        );
                        candidates.push(c);
                    }
                }
                Err(e) => log::warn!("[local_search] discover via {} failed: {}", id, e),
            }
        }
    }

    // 3. Cache check + 4. deep scrape, both bounded. Cache hits are cheap (just
    //    re-attach to this query); misses are scraped up to SCRAPE_TOP_N.
    let global_sem = Arc::new(Semaphore::new(GLOBAL_SCRAPE_CONCURRENCY));
    let reddit_sem = Arc::new(Semaphore::new(REDDIT_SCRAPE_CONCURRENCY));
    let mut scrape_budget = SCRAPE_TOP_N;
    let mut deep_futs = Vec::new();

    for c in candidates {
        // Cache check (sync, fast).
        let meta = {
            let conn = super::SearchIndexManager::open(&db_path).map_err(|e| e.to_string())?;
            store::find_page_by_url(&conn, profile_id, &c.url).map_err(|e| e.to_string())?
        };
        let fetch_adapter = registry.fetch_adapter(&c.url);
        if let Some(meta) = &meta {
            if reuse_cached(meta, fetch_adapter.freshness_ttl(), now) {
                // Cache hit: attach to this query, emit straight to the deep stream.
                let conn = super::SearchIndexManager::open(&db_path).map_err(|e| e.to_string())?;
                let _ = store::attach_to_query(&conn, meta.id, query_id, c.searxng_pos, now);
                let _ = app.emit(
                    "local-search-deep",
                    DeepEvent {
                        query_id,
                        page: DeepPage {
                            page_id: meta.id,
                            url: c.url.clone(),
                            title: c.title.clone(),
                            snippet: c.snippet.clone(),
                            source_engine: c.source_engine.clone(),
                            cache_hit: true,
                        },
                    },
                );
                continue;
            }
        }

        // Miss: schedule a bounded scrape, up to the budget.
        if scrape_budget == 0 {
            continue;
        }
        scrape_budget -= 1;

        let app = app.clone();
        let db_path = db_path.clone();
        let registry = registry.clone();
        let global_sem = global_sem.clone();
        let reddit_sem = reddit_sem.clone();
        deep_futs.push(async move {
            let _g = global_sem.acquire().await;
            let is_reddit = host_of(&c.url).map(|h| h.ends_with("reddit.com")).unwrap_or(false);
            let _r = if is_reddit {
                Some(reddit_sem.acquire().await)
            } else {
                None
            };
            let adapter = registry.fetch_adapter(&c.url);
            let source_engine = adapter.id().to_string();
            match adapter.fetch(&c.url).await {
                Ok(doc) => {
                    let hash = content_hash(&doc.body);
                    let expires_at = tier.expires_at(now);
                    let page_id = {
                        let conn = match super::SearchIndexManager::open(&db_path) {
                            Ok(c) => c,
                            Err(e) => {
                                log::warn!("[local_search] db open failed: {}", e);
                                return;
                            }
                        };
                        match store::upsert_scraped(
                            &conn, profile_id, &doc.url, &doc.title, &doc.body, &c.snippet,
                            query_id, &source_engine, c.searxng_pos, &hash, tier.as_str(), now, expires_at,
                        ) {
                            Ok(id) => id,
                            Err(e) => {
                                log::warn!("[local_search] index {} failed: {}", doc.url, e);
                                return;
                            }
                        }
                    };
                    // Embed (async, best-effort). Skipped silently if Ollama is down.
                    if let Some(vec) = super::embed::embed_text(&doc.body).await {
                        if let Ok(conn) = super::SearchIndexManager::open(&db_path) {
                            let _ = store::upsert_embedding(&conn, page_id, &vec);
                        }
                    }
                    let _ = app.emit(
                        "local-search-deep",
                        DeepEvent {
                            query_id,
                            page: DeepPage {
                                page_id,
                                url: doc.url,
                                title: doc.title,
                                snippet: c.snippet,
                                source_engine,
                                cache_hit: false,
                            },
                        },
                    );
                }
                Err(e) => {
                    // One failed fetch must not abort the batch — log and move on.
                    log::warn!("[local_search] fetch {} failed: {}", c.url, e);
                }
            }
        });
    }

    // Run all scrapes concurrently (semaphores bound real in-flight work). A single
    // failure resolves to () inside the block, so join never aborts the batch.
    futures_util::future::join_all(deep_futs).await;

    // 5. Fused ranking over this query's pages, then done.
    let ranked = super::rank::rank(&db_path, query_id, &query, profile_id, limit).await;
    let _ = app.emit("local-search-ranked", RankedEvent { query_id, ranked });
    let _ = app.emit("local-search-done", DoneEvent { query_id });

    Ok(query_id)
}

/// Whether an existing row should be reused instead of re-fetched. Pinned/archived
/// rows are always reused (the user committed to them); cheap tiers reuse while
/// within the adapter's freshness TTL and not past their own expiry.
fn reuse_cached(meta: &store::PageMeta, ttl: Duration, now: i64) -> bool {
    if meta.retention == "pinned" || meta.retention == "archived" {
        return true;
    }
    let fresh_by_ttl = meta
        .fetched_at
        .map(|f| now - f < ttl.as_secs() as i64)
        .unwrap_or(false);
    let not_expired = meta.expires_at.map(|e| e > now).unwrap_or(true);
    fresh_by_ttl && not_expired
}

/// Click-logging command: records the click (private personalization signal) and
/// bumps the page's usage if it's in the index.
#[tauri::command(rename_all = "camelCase")]
pub async fn log_result_click(
    app: tauri::AppHandle,
    query_id: Option<i64>,
    url: String,
    profile_id: i64,
) -> Result<(), String> {
    let db_path = {
        let state = app.state::<std::sync::Mutex<crate::AppState>>();
        let st = state.lock().map_err(|e| e.to_string())?;
        st.db_path.clone()
    };
    let now = now_secs();
    let domain = host_of(&url);
    let conn = super::SearchIndexManager::open(&db_path).map_err(|e| e.to_string())?;
    store::insert_click(&conn, query_id, &url, domain.as_deref(), now, profile_id)
        .map_err(|e| e.to_string())?;
    if let Ok(Some(meta)) = store::find_page_by_url(&conn, profile_id, &url) {
        let _ = store::bump_usage(&conn, meta.id, now);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_hash_is_normalized_and_dedups() {
        // whitespace + case differences collapse to the same hash
        let a = content_hash("Hello   World\n\tFoo");
        let b = content_hash("hello world foo");
        assert_eq!(a, b);
        let c = content_hash("hello world bar");
        assert_ne!(a, c);
        assert_eq!(a.len(), 64); // sha256 hex
    }

    #[test]
    fn reuse_rules() {
        let now = 1_000_000;
        let pinned = store::PageMeta {
            id: 1, fetched_at: Some(now - 999_999), expires_at: None,
            retention: "pinned".into(), content_hash: None,
        };
        assert!(reuse_cached(&pinned, Duration::from_secs(60), now), "pinned always reused");

        let fresh = store::PageMeta {
            id: 2, fetched_at: Some(now - 10), expires_at: Some(now + 1000),
            retention: "cache".into(), content_hash: None,
        };
        assert!(reuse_cached(&fresh, Duration::from_secs(3600), now));

        let stale = store::PageMeta {
            id: 3, fetched_at: Some(now - 7200), expires_at: Some(now + 1000),
            retention: "cache".into(), content_hash: None,
        };
        assert!(!reuse_cached(&stale, Duration::from_secs(3600), now), "past TTL → re-fetch");

        let expired = store::PageMeta {
            id: 4, fetched_at: Some(now - 10), expires_at: Some(now - 1),
            retention: "ephemeral".into(), content_hash: None,
        };
        assert!(!reuse_cached(&expired, Duration::from_secs(3600), now), "expired → re-fetch");
    }
}
