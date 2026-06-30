//! Phase 7 — lazy curation hook.
//!
//! Curation is EXPENSIVE (Ollama summarization), so it never runs on every
//! scraped page. It runs only when a result is pinned/favorited/saved, when a page
//! is archived without a prior summary (so nothing is lost), or for the top few
//! ranked results of a completed search. It reuses the existing curator pipeline
//! (`ai::curate_viewed` → `memory::journal_page`), which is what makes pinned
//! results show up later in the assistant's `retrieve_context()`.

use tauri::Emitter;

/// Curate one indexed page by id: summarize its body via the existing curator and
/// journal it into the knowledge graph. Best-effort: if Ollama is down or the body
/// is too thin, `ai::curate_viewed` no-ops and we just don't emit curated.
pub async fn curate_page_by_id(
    app: &tauri::AppHandle,
    db_path: &str,
    page_id: i64,
    profile_id: i64,
) -> Result<(), String> {
    let content = {
        let conn = super::SearchIndexManager::open(db_path).map_err(|e| e.to_string())?;
        super::store::page_content(&conn, page_id).map_err(|e| e.to_string())?
    };
    let (url, title, body) = match content {
        Some((u, t, Some(b))) if !b.trim().is_empty() => (u, t, b),
        // No body to curate (archived or never scraped) — nothing to do.
        _ => return Ok(()),
    };
    // Reuse the existing curator pipeline (summarize + journal_page). Do NOT write
    // a new one.
    crate::ai::curate_viewed(db_path, profile_id, &url, &title, &body).await?;
    let _ = app.emit("local-search-curated", CuratedEvent { page_id });
    Ok(())
}

/// Ensure a curated summary exists for a page before a lossy operation (archive).
/// Curates only if the knowledge graph has no summary yet, so nothing is lost.
pub async fn ensure_curated(
    app: &tauri::AppHandle,
    db_path: &str,
    page_id: i64,
    profile_id: i64,
) -> Result<(), String> {
    let already = {
        let conn = super::SearchIndexManager::open(db_path).map_err(|e| e.to_string())?;
        match super::store::page_content(&conn, page_id).map_err(|e| e.to_string())? {
            Some((url, _, _)) => super::store::has_curated_summary(&conn, &url, profile_id)
                .map_err(|e| e.to_string())?,
            None => return Ok(()),
        }
    };
    if already {
        return Ok(());
    }
    curate_page_by_id(app, db_path, page_id, profile_id).await
}

/// Background curation of the top-K ranked results of a completed search (optional
/// per the spec). Bounded and best-effort; never blocks the search.
pub async fn curate_top_ranked(
    app: &tauri::AppHandle,
    db_path: &str,
    ranked: &[super::rank::RankedResult],
    profile_id: i64,
    k: usize,
) {
    for r in ranked.iter().take(k) {
        let _ = curate_page_by_id(app, db_path, r.page_id, profile_id).await;
    }
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CuratedEvent {
    page_id: i64,
}
