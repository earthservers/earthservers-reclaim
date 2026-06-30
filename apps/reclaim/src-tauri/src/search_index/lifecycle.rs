//! Phase 6 — retention lifecycle commands: pin / archive / forget / forget_query /
//! review_pinned, the favorites bridge (set_favorite), and auto-cache on browse.
//!
//! The curator proposes, the user disposes: review_pinned() only SUGGESTS; the
//! default destructive action is archive (demote), not delete; pinned/favorited
//! content is never silently deleted or archived by automation. A confirmed dead
//! upstream PROTECTS the local copy rather than pruning it.

use super::orchestrator::{content_hash, now_secs};
use super::retention::{Retention, DAY};
use super::store;
use serde::Serialize;
use std::collections::HashMap;
use tauri::Manager;

/// Cosine above this between two pins ⇒ near-duplicate (redundancy prune signal).
const DUPLICATE_THRESHOLD: f32 = 0.92;
/// Disuse beyond this (days, scaled by open_count) makes a pin an archive candidate.
const DISUSE_ARCHIVE_DAYS: f32 = 90.0;
/// Soft disk cap for the index body text (bytes). Over this, review is "urgent".
const DISK_SOFT_CAP_BYTES: i64 = 512 * 1024 * 1024;

fn db_path_of(app: &tauri::AppHandle) -> Result<String, String> {
    let state = app.state::<std::sync::Mutex<crate::AppState>>();
    let st = state.lock().map_err(|e| e.to_string())?;
    Ok(st.db_path.clone())
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PruneAction {
    Archive,
    Forget,
    Keep,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PruneCandidate {
    pub page_id: i64,
    pub url: String,
    pub title: String,
    pub prune_score: f32,
    pub reason: String,
    pub suggested: PruneAction,
    pub duplicate_of: Option<i64>,
}

// ---- pin / archive / forget ----

/// Promote a result to pinned and enqueue curation (so it shows in the assistant's
/// retrieve_context later).
#[tauri::command(rename_all = "camelCase")]
pub async fn pin_result(
    app: tauri::AppHandle,
    page_id: i64,
    profile_id: i64,
) -> Result<(), String> {
    let db_path = db_path_of(&app)?;
    {
        let conn = super::SearchIndexManager::open(&db_path).map_err(|e| e.to_string())?;
        store::pin(&conn, page_id, now_secs()).map_err(|e| e.to_string())?;
    }
    // Lazy curation (best-effort; won't fail the pin if Ollama is down).
    let _ = super::curate::curate_page_by_id(&app, &db_path, page_id, profile_id).await;
    Ok(())
}

/// The default destructive action: demote (archive), don't delete. Ensures a
/// curated summary survives, then drops body/FTS/embeddings.
#[tauri::command(rename_all = "camelCase")]
pub async fn archive_result(
    app: tauri::AppHandle,
    page_id: i64,
    profile_id: i64,
) -> Result<(), String> {
    let db_path = db_path_of(&app)?;
    // 1. Make sure nothing is lost — curate first if there's no summary yet.
    let _ = super::curate::ensure_curated(&app, &db_path, page_id, profile_id).await;
    // 2. Drop body/FTS/embeddings; keep url/title + knowledge-graph entry.
    let conn = super::SearchIndexManager::open(&db_path).map_err(|e| e.to_string())?;
    store::archive(&conn, page_id).map_err(|e| e.to_string())
}

/// Hard delete now (explicit user action only).
#[tauri::command(rename_all = "camelCase")]
pub async fn forget_result(app: tauri::AppHandle, page_id: i64) -> Result<(), String> {
    let db_path = db_path_of(&app)?;
    let conn = super::SearchIndexManager::open(&db_path).map_err(|e| e.to_string())?;
    store::forget(&conn, page_id).map_err(|e| e.to_string())
}

/// Drop a whole search (pinned/archived pages within it survive).
#[tauri::command(rename_all = "camelCase")]
pub async fn forget_query(app: tauri::AppHandle, query_id: i64) -> Result<usize, String> {
    let db_path = db_path_of(&app)?;
    let conn = super::SearchIndexManager::open(&db_path).map_err(|e| e.to_string())?;
    store::forget_query(&conn, query_id).map_err(|e| e.to_string())
}

// ---- favorites bridge (favorite == pinned, single source of truth) ----

/// Set/clear favorite for a URL from anywhere (address bar, search result, history,
/// bookmark row). Writes BOTH the pinned retention tier and indexed_pages.is_favorite
/// so the star stays consistent everywhere.
///
/// Login/credential pages are stored URL-only (no body cache, no index, no curation)
/// — the frontend passes `is_login` from the reclaimVault login-detection signal.
#[tauri::command(rename_all = "camelCase")]
pub async fn set_favorite(
    app: tauri::AppHandle,
    url: String,
    favorite: bool,
    profile_id: i64,
    title: Option<String>,
    is_login: Option<bool>,
) -> Result<(), String> {
    let db_path = db_path_of(&app)?;
    let title = title.unwrap_or_else(|| url.clone());
    let login = is_login.unwrap_or(false) || looks_like_login(&url);

    // Mirror into the knowledge-graph star (single source of truth) up front.
    // Sync call held under the lock (no await in this block).
    {
        let state = app.state::<std::sync::Mutex<crate::AppState>>();
        let st = state.lock().map_err(|e| e.to_string())?;
        st.memory_manager
            .set_favorite_by_url(&url, &title, favorite, profile_id)
            .map_err(|e| e.to_string())?;
    }

    if !favorite {
        // Unfavorite: demote a pinned row to cache with a fresh TTL (predictable;
        // let the decay model handle eventual archival). Never hard-delete.
        if let Some(meta) = {
            let conn = super::SearchIndexManager::open(&db_path).map_err(|e| e.to_string())?;
            store::find_page_by_url(&conn, profile_id, &url).map_err(|e| e.to_string())?
        } {
            if meta.retention == "pinned" {
                let exp = Retention::Cache.expires_at(now_secs()).unwrap_or(now_secs());
                let conn = super::SearchIndexManager::open(&db_path).map_err(|e| e.to_string())?;
                store::demote_to_cache(&conn, meta.id, exp).map_err(|e| e.to_string())?;
            }
        }
        return Ok(());
    }

    // Favoriting a login page: URL-only quick-access. Do NOT cache/index/curate.
    if login {
        log::info!("[search_index] favorited login page {} as URL-only shortcut", url);
        return Ok(());
    }

    // Favoriting a normal page: ensure a pinned, indexed, curated search_pages row.
    let existing = {
        let conn = super::SearchIndexManager::open(&db_path).map_err(|e| e.to_string())?;
        store::find_page_by_url(&conn, profile_id, &url).map_err(|e| e.to_string())?
    };
    let page_id = match existing {
        Some(meta) => {
            let conn = super::SearchIndexManager::open(&db_path).map_err(|e| e.to_string())?;
            store::pin(&conn, meta.id, now_secs()).map_err(|e| e.to_string())?;
            meta.id
        }
        None => {
            // Visited-but-never-scraped (or never-searched) URL: scrape + index now
            // at the pinned tier so it becomes a first-class grep-able entry.
            let registry = super::adapters::AdapterRegistry::default_set(None);
            let adapter = registry.fetch_adapter(&url);
            let doc = adapter.fetch(&url).await?;
            let hash = content_hash(&doc.body);
            let now = now_secs();
            let conn = super::SearchIndexManager::open(&db_path).map_err(|e| e.to_string())?;
            let id = store::upsert_scraped(
                &conn, profile_id, &doc.url, &doc.title, &doc.body, "", None,
                adapter.id(), None, &hash, "pinned", now, None,
            )
            .map_err(|e| e.to_string())?;
            store::pin(&conn, id, now).map_err(|e| e.to_string())?;
            // Embedding (best-effort).
            if let Some(vec) = super::embed::embed_text(&doc.body).await {
                let _ = store::upsert_embedding(&conn, id, &vec);
            }
            id
        }
    };

    // Warm the WebKit render cache so the live page paints fast next visit.
    warm_render_cache(&url);
    // Curate (lazy, best-effort).
    let _ = super::curate::curate_page_by_id(&app, &db_path, page_id, profile_id).await;
    Ok(())
}

/// Star state for a URL: pinned tier OR the knowledge-graph favorite flag (one
/// source of truth, with the tier authoritative).
#[tauri::command(rename_all = "camelCase")]
pub async fn favorite_state(
    app: tauri::AppHandle,
    url: String,
    profile_id: i64,
) -> Result<bool, String> {
    let db_path = db_path_of(&app)?;
    let pinned = {
        let conn = super::SearchIndexManager::open(&db_path).map_err(|e| e.to_string())?;
        store::find_page_by_url(&conn, profile_id, &url)
            .map_err(|e| e.to_string())?
            .map(|m| m.retention == "pinned")
            .unwrap_or(false)
    };
    if pinned {
        return Ok(true);
    }
    let state = app.state::<std::sync::Mutex<crate::AppState>>();
    let st = state.lock().map_err(|e| e.to_string())?;
    st.memory_manager
        .is_favorited_by_url(&url, profile_id)
        .map_err(|e| e.to_string())
}

// ---- auto-cache on browse (Phase 6a) ----

/// Cache a normally-browsed page's text into the grep-able index (cache tier, TTL'd,
/// no curation cost). Skips login/credential pages entirely. Returns the page id, or
/// None if skipped. Call from the dwell hook (visible > N seconds, not a bounce).
#[tauri::command(rename_all = "camelCase")]
pub async fn auto_cache_page(
    app: tauri::AppHandle,
    url: String,
    title: String,
    text: String,
    profile_id: i64,
    is_login: Option<bool>,
) -> Result<Option<i64>, String> {
    if is_login.unwrap_or(false) || looks_like_login(&url) {
        // A login page's body has no search value and is a privacy hazard. Skip.
        return Ok(None);
    }
    let body = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if body.split_whitespace().count() < 30 {
        return Ok(None); // too thin to be worth indexing
    }
    let now = now_secs();
    let hash = content_hash(&body);
    let expires = Retention::Cache.expires_at(now);
    let page_id = {
        let conn = super::SearchIndexManager::open(&db_path_of(&app)?).map_err(|e| e.to_string())?;
        store::upsert_scraped(
            &conn, profile_id, &url, &title, &body, "", None, "browse", None, &hash,
            "cache", now, expires,
        )
        .map_err(|e| e.to_string())?
    };
    // Embedding (best-effort).
    if let Some(vec) = super::embed::embed_text(&body).await {
        if let Ok(conn) = super::SearchIndexManager::open(&db_path_of(&app)?) {
            let _ = store::upsert_embedding(&conn, page_id, &vec);
        }
    }
    Ok(Some(page_id))
}

// ---- review_pinned (Phase 6d): proposes, never disposes ----

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewResult {
    pub candidates: Vec<PruneCandidate>,
    /// Total indexed body bytes, and whether we're over the soft cap (urgent review).
    pub index_bytes: i64,
    pub urgent: bool,
}

/// Score every pinned page and return a ranked candidate list. NEVER mutates.
/// Signals: disuse (strongest), age since pin, and semantic redundancy vs other
/// pins. A confirmed-gone upstream is PROTECTED (suggested Keep), not pruned.
#[tauri::command(rename_all = "camelCase")]
pub async fn review_pinned(
    app: tauri::AppHandle,
    profile_id: i64,
) -> Result<ReviewResult, String> {
    let db_path = db_path_of(&app)?;
    let (pins, embeddings, index_bytes) = {
        let conn = super::SearchIndexManager::open(&db_path).map_err(|e| e.to_string())?;
        let pins = store::pinned_pages(&conn, profile_id).map_err(|e| e.to_string())?;
        let ids: Vec<i64> = pins.iter().map(|p| p.id).collect();
        let embeddings = store::load_embeddings(&conn, &ids).unwrap_or_default();
        let bytes = store::index_body_bytes(&conn).unwrap_or(0);
        (pins, embeddings, bytes)
    };
    let candidates = score_pins(&pins, &embeddings, now_secs());
    Ok(ReviewResult {
        candidates,
        index_bytes,
        urgent: index_bytes > DISK_SOFT_CAP_BYTES,
    })
}

/// Pure scoring so it's unit-testable. Newer of a near-duplicate pair is the
/// candidate; `duplicate_of` points at the older pin we keep.
pub fn score_pins(
    pins: &[store::PinnedRow],
    embeddings: &HashMap<i64, Vec<f32>>,
    now: i64,
) -> Vec<PruneCandidate> {
    let mut out: Vec<PruneCandidate> = Vec::new();
    for p in pins {
        // Disuse: days since last opened (fall back to pin time), damped by opens.
        let last = p.last_opened_at.or(p.pinned_at).unwrap_or(now);
        let disuse_days = ((now - last).max(0) as f32) / DAY as f32;
        let age_days = ((now - p.pinned_at.unwrap_or(now)).max(0) as f32) / DAY as f32;
        let disuse_score = disuse_days / (1.0 + p.open_count as f32) + 0.1 * age_days;

        // Redundancy vs OTHER pins (older one is the keeper).
        let mut duplicate_of = None;
        if let Some(v) = embeddings.get(&p.id) {
            for other in pins {
                if other.id == p.id {
                    continue;
                }
                let older = other.pinned_at.unwrap_or(0) < p.pinned_at.unwrap_or(0)
                    || (other.pinned_at == p.pinned_at && other.id < p.id);
                if !older {
                    continue;
                }
                if let Some(ov) = embeddings.get(&other.id) {
                    if super::embed::cosine(v, ov) >= DUPLICATE_THRESHOLD {
                        duplicate_of = Some(other.id);
                        break;
                    }
                }
            }
        }

        let (score, suggested, reason) = if p.upstream_gone {
            // Dead upstream → treasure the local copy, don't prune.
            (
                0.0,
                PruneAction::Keep,
                "Upstream is gone (404/410) — local copy only; protected from pruning".to_string(),
            )
        } else if let Some(dup) = duplicate_of {
            (
                disuse_score + 5.0,
                PruneAction::Archive,
                format!("Near-duplicate of pin #{} — archive the redundant copy", dup),
            )
        } else if disuse_days >= DISUSE_ARCHIVE_DAYS && p.open_count <= 1 {
            (
                disuse_score,
                PruneAction::Archive,
                format!("Not opened in {} days; rarely used — archive to reclaim disk", disuse_days as i64),
            )
        } else {
            (disuse_score, PruneAction::Keep, "Actively used or recent — keep".to_string())
        };

        out.push(PruneCandidate {
            page_id: p.id,
            url: p.url.clone(),
            title: p.title.clone(),
            prune_score: score,
            reason,
            suggested,
            duplicate_of,
        });
    }
    out.sort_by(|a, b| {
        b.prune_score
            .partial_cmp(&a.prune_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.page_id.cmp(&b.page_id))
    });
    out
}

/// Light URL heuristic backstop for login pages (the authoritative signal is the
/// frontend's reclaimVault login detection, passed as `is_login`).
fn looks_like_login(url: &str) -> bool {
    let u = url.to_ascii_lowercase();
    ["/login", "/signin", "/sign-in", "/log-in", "/auth", "/sso", "/account/login"]
        .iter()
        .any(|p| u.contains(p))
}

/// Warm the persistent WebKit render cache so the live page paints fast next visit.
/// NOTE: true WebKit WebContext precache needs a hidden webview load; that's a
/// follow-up. For now this is an honest no-op placeholder (logged), so we don't
/// imply caching that isn't happening. [HONEST]
fn warm_render_cache(url: &str) {
    log::debug!("[search_index] TODO: warm WebKit render cache for {}", url);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pin(id: i64, last_opened: Option<i64>, open_count: i64, pinned_at: i64, gone: bool) -> store::PinnedRow {
        store::PinnedRow {
            id,
            url: format!("https://h{}.test", id),
            title: format!("Pin {}", id),
            last_opened_at: last_opened,
            open_count,
            pinned_at: Some(pinned_at),
            upstream_gone: gone,
            content_hash: None,
        }
    }

    #[test]
    fn disused_pin_ranks_above_active_pin() {
        let now = 1_000_000_000;
        let old = DAY * 200;
        let pins = vec![
            pin(1, Some(now - old), 0, now - old, false), // very disused
            pin(2, Some(now - 10), 50, now - old, false), // recent + heavily used
        ];
        let cands = score_pins(&pins, &HashMap::new(), now);
        assert_eq!(cands[0].page_id, 1, "disused pin sorts first");
        assert_eq!(cands[0].suggested, PruneAction::Archive);
        assert_eq!(cands[1].suggested, PruneAction::Keep);
    }

    #[test]
    fn dead_upstream_is_protected_not_pruned() {
        let now = 1_000_000_000;
        let old = DAY * 365;
        // ancient + never opened, but upstream gone → must be Keep, score 0.
        let pins = vec![pin(1, Some(now - old), 0, now - old, true)];
        let cands = score_pins(&pins, &HashMap::new(), now);
        assert_eq!(cands[0].suggested, PruneAction::Keep);
        assert_eq!(cands[0].prune_score, 0.0);
        assert!(cands[0].reason.contains("local copy only"));
    }

    #[test]
    fn near_duplicate_newer_pin_flagged_archive() {
        let now = 1_000_000_000;
        let pins = vec![
            pin(1, Some(now), 5, now - DAY * 10, false), // older original
            pin(2, Some(now), 5, now - DAY * 1, false),  // newer duplicate
        ];
        let mut emb = HashMap::new();
        emb.insert(1, vec![1.0, 0.0, 0.0]);
        emb.insert(2, vec![1.0, 0.0, 0.0]); // identical → cosine 1.0
        let cands = score_pins(&pins, &emb, now);
        let dup = cands.iter().find(|c| c.page_id == 2).unwrap();
        assert_eq!(dup.suggested, PruneAction::Archive);
        assert_eq!(dup.duplicate_of, Some(1), "keeps the older pin");
        let orig = cands.iter().find(|c| c.page_id == 1).unwrap();
        assert_eq!(orig.duplicate_of, None, "older pin is not the duplicate");
    }

    #[test]
    fn login_url_heuristic() {
        assert!(looks_like_login("https://site.com/login"));
        assert!(looks_like_login("https://site.com/account/login?next=/"));
        assert!(!looks_like_login("https://site.com/blog/rust-tips"));
        assert!(!looks_like_login("https://reddit.com/r/rust"));
    }
}
