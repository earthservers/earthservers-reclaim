//! Phase 5 — hybrid retrieval + Reciprocal Rank Fusion.
//!
//! Independent rankers each produce an ordered list of page_ids scoped to one
//! query's pages: FTS5/BM25 (exact/grep), vector cosine (semantic), and the
//! SearXNG aggregated position (a cheap strong prior). We fuse with RRF (no score
//! normalization needed), then add a private click-log boost. Weights are named
//! constants so they're easy to tune.

use super::store::{self, StoredPage};
use serde::Serialize;
use std::collections::HashMap;

/// RRF dampening constant. fused += w / (K + rank).
const K: f32 = 60.0;
/// Per-ranker weights.
const W_FTS: f32 = 1.0;
const W_VEC: f32 = 1.0;
const W_POS: f32 = 0.5;
/// Click-log personalization: + CLICK_BOOST * ln(1 + clicks_on_domain).
const CLICK_BOOST: f32 = 0.15;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RankSignals {
    /// 0-based rank within each ranker (None = ranker didn't place this result).
    pub fts_rank: Option<usize>,
    pub vec_rank: Option<usize>,
    pub pos_rank: Option<usize>,
    pub click_boost: f32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RankedResult {
    pub page_id: i64,
    pub url: String,
    pub title: String,
    pub snippet: String,
    pub source_engine: String,
    pub fused_score: f32,
    pub signals: RankSignals,
}

/// Sanitize arbitrary user input into a valid FTS5 MATCH expression: keep
/// alphanumeric tokens, quote each (neutralizing FTS operators/quotes), OR them
/// for recall. Returns None for empty/operator-only input so the FTS ranker is
/// simply skipped rather than erroring.
pub fn sanitize_fts_query(q: &str) -> Option<String> {
    let cleaned: String = q
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    let tokens: Vec<&str> = cleaned.split_whitespace().collect();
    if tokens.is_empty() {
        return None;
    }
    Some(
        tokens
            .iter()
            .map(|t| format!("\"{}\"", t))
            .collect::<Vec<_>>()
            .join(" OR "),
    )
}

/// Rank a query's pages. Opens its own connection for the synchronous reads,
/// drops it before the async query-embedding, then fuses in pure code.
pub async fn rank(
    db_path: &str,
    query_id: i64,
    query_text: &str,
    profile_id: i64,
    limit: usize,
) -> Vec<RankedResult> {
    // ---- synchronous DB reads (no statement held across an await) ----
    let (candidates, fts_rank, pos_rank, embeddings, clicks) = {
        let conn = match super::SearchIndexManager::open(db_path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let candidates = store::candidate_pages(&conn, query_id).unwrap_or_default();
        if candidates.is_empty() {
            return Vec::new();
        }
        let id_set: std::collections::HashSet<i64> = candidates.iter().map(|c| c.id).collect();

        // FTS ranking, filtered to this query's candidates.
        let fts_rank: HashMap<i64, usize> = match sanitize_fts_query(query_text) {
            Some(expr) => store::fts_order(&conn, &expr)
                .unwrap_or_default()
                .into_iter()
                .filter(|id| id_set.contains(id))
                .enumerate()
                .map(|(rank, id)| (id, rank))
                .collect(),
            None => HashMap::new(),
        };

        // SearXNG position ranking (lower searxng_pos = better; None sorts last).
        let pos_rank = position_ranking(&candidates);

        let ids: Vec<i64> = candidates.iter().map(|c| c.id).collect();
        let embeddings = store::load_embeddings(&conn, &ids).unwrap_or_default();
        let clicks = store::domain_click_counts(&conn, profile_id).unwrap_or_default();
        (candidates, fts_rank, pos_rank, embeddings, clicks)
    };

    // ---- async: embed the query, then compute vector ranking ----
    let vec_rank: HashMap<i64, usize> = match super::embed::embed_text(query_text).await {
        Some(qvec) => {
            let mut sims: Vec<(i64, f32)> = embeddings
                .iter()
                .map(|(&id, v)| (id, super::embed::cosine(&qvec, v)))
                .collect();
            sims.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            sims.into_iter()
                .enumerate()
                .map(|(rank, (id, _))| (id, rank))
                .collect()
        }
        None => HashMap::new(),
    };

    // ---- pure RRF fusion + click boost ----
    let mut out: Vec<RankedResult> = candidates
        .into_iter()
        .map(|c| {
            let click_boost = domain_click_boost(&c.url, &clicks);
            let f = fts_rank.get(&c.id).copied();
            let v = vec_rank.get(&c.id).copied();
            let p = pos_rank.get(&c.id).copied();
            let fused = rrf_term(W_FTS, f) + rrf_term(W_VEC, v) + rrf_term(W_POS, p) + click_boost;
            RankedResult {
                page_id: c.id,
                url: c.url,
                title: c.title,
                snippet: c.snippet,
                source_engine: c.source_engine,
                fused_score: fused,
                signals: RankSignals {
                    fts_rank: f,
                    vec_rank: v,
                    pos_rank: p,
                    click_boost,
                },
            }
        })
        .collect();

    out.sort_by(|a, b| {
        b.fused_score
            .partial_cmp(&a.fused_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            // stable tiebreak so equal scores keep a deterministic order
            .then(a.page_id.cmp(&b.page_id))
    });
    out.truncate(limit);
    out
}

/// One RRF term: w / (K + rank), or 0 if this ranker didn't place the result.
fn rrf_term(weight: f32, rank: Option<usize>) -> f32 {
    match rank {
        Some(r) => weight / (K + r as f32),
        None => 0.0,
    }
}

/// Map page_id → its rank by SearXNG position (lower position = better rank 0).
/// Candidates without a position are ranked after those with one, by id.
fn position_ranking(candidates: &[StoredPage]) -> HashMap<i64, usize> {
    let mut with_pos: Vec<&StoredPage> = candidates.iter().filter(|c| c.searxng_pos.is_some()).collect();
    with_pos.sort_by(|a, b| {
        a.searxng_pos
            .cmp(&b.searxng_pos)
            .then(a.id.cmp(&b.id))
    });
    with_pos
        .into_iter()
        .enumerate()
        .map(|(rank, c)| (c.id, rank))
        .collect()
}

/// CLICK_BOOST * ln(1 + clicks on this result's domain).
fn domain_click_boost(url: &str, clicks: &HashMap<String, i64>) -> f32 {
    match super::adapters::host_of(url).and_then(|h| clicks.get(&h).copied()) {
        Some(n) if n > 0 => CLICK_BOOST * (1.0 + n as f32).ln(),
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizer_handles_weird_input() {
        assert_eq!(sanitize_fts_query("rust async"), Some("\"rust\" OR \"async\"".into()));
        // operator-only / punctuation-only input must not panic and yields None
        assert_eq!(sanitize_fts_query("   "), None);
        assert_eq!(sanitize_fts_query("\"*()OR AND-"), Some("\"OR\" OR \"AND\"".into()));
        assert_eq!(sanitize_fts_query("()*\"^"), None);
        // a quote in the input can't break out (each token is re-quoted)
        assert_eq!(sanitize_fts_query("a\"b"), Some("\"a\" OR \"b\"".into()));
    }

    #[test]
    fn rrf_term_math() {
        assert!((rrf_term(1.0, Some(0)) - 1.0 / 60.0).abs() < 1e-6);
        assert!((rrf_term(0.5, Some(0)) - 0.5 / 60.0).abs() < 1e-6);
        assert_eq!(rrf_term(1.0, None), 0.0);
        // earlier rank scores higher
        assert!(rrf_term(1.0, Some(0)) > rrf_term(1.0, Some(5)));
    }

    #[test]
    fn position_ranking_orders_by_searxng_pos() {
        let cands = vec![
            sp(10, Some(2)),
            sp(11, Some(0)),
            sp(12, None),
            sp(13, Some(1)),
        ];
        let pr = position_ranking(&cands);
        assert_eq!(pr.get(&11), Some(&0));
        assert_eq!(pr.get(&13), Some(&1));
        assert_eq!(pr.get(&10), Some(&2));
        assert_eq!(pr.get(&12), None); // no position → not placed by this ranker
    }

    #[test]
    fn click_boost_grows_with_clicks() {
        let mut clicks = HashMap::new();
        clicks.insert("reddit.com".to_string(), 10i64);
        let b = domain_click_boost("https://www.reddit.com/r/rust", &clicks);
        assert!(b > 0.0);
        assert!(domain_click_boost("https://example.com", &clicks) == 0.0);
        // more clicks → bigger boost
        let mut more = HashMap::new();
        more.insert("reddit.com".to_string(), 100i64);
        assert!(domain_click_boost("https://reddit.com/x", &more) > b);
    }

    fn sp(id: i64, pos: Option<i64>) -> StoredPage {
        StoredPage {
            id,
            url: format!("https://h{}.test", id),
            title: String::new(),
            snippet: String::new(),
            source_engine: "web".into(),
            searxng_pos: pos,
        }
    }
}
