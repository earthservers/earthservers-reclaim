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
/// How many crawler (scraped_pages) FTS hits to fan in per query.
const CRAWLER_TOP_N: usize = 30;
/// Max crawler hits from any single domain, so one heavily-crawled site can't
/// flood the results (e.g. a site you crawled exhaustively matching every query).
const CRAWLER_PER_DOMAIN: usize = 3;

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
    pub content_kind: String,
    pub parent_url: Option<String>,
    pub fused_score: f32,
    pub signals: RankSignals,
    /// Which index this row came from: "search_pages" (query-driven) or
    /// "scraped_pages" (the crawler). The UI badges crawler rows and disables the
    /// pin/archive/forget maintenance actions for them (page_id is a crawler rowid,
    /// not a search_pages id).
    pub source_table: &'static str,
    /// For crawler rows: the crawl job name, for a "from crawl: <job>" badge.
    pub provenance: Option<String>,
}

/// Normalize a URL for cross-index dedup: lowercase, drop fragment + trailing slash.
fn norm_url(u: &str) -> String {
    let s = u.split('#').next().unwrap_or(u).trim().trim_end_matches('/');
    s.to_ascii_lowercase()
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
    kinds: Option<&[String]>,
) -> Vec<RankedResult> {
    // ---- synchronous DB reads (no statement held across an await) ----
    let (candidates, fts_rank, pos_rank, embeddings, clicks, crawler_cands, crawler_fts_rank) = {
        let conn = match super::SearchIndexManager::open(db_path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let candidates = store::candidate_pages(&conn, query_id, kinds).unwrap_or_default();
        let id_set: std::collections::HashSet<i64> = candidates.iter().map(|c| c.id).collect();
        let expr = sanitize_fts_query(query_text);
        // Crawler rows are all 'article'; only fan them in when articles are wanted.
        let want_articles = kinds.map(|k| k.iter().any(|s| s == "article")).unwrap_or(true);

        // FTS ranking over search_pages, filtered to this query's candidates.
        let fts_rank: HashMap<i64, usize> = match &expr {
            Some(e) => store::fts_order(&conn, e)
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

        // ---- read-time fan-in of the crawler index (its own id space + FTS) ----
        let mut crawler_cands: Vec<super::crawler::CrawlerCand> = Vec::new();
        let mut crawler_fts_rank: HashMap<i64, usize> = HashMap::new();
        if let (true, Some(e)) = (want_articles, &expr) {
            let sp_urls: std::collections::HashSet<String> =
                candidates.iter().map(|c| norm_url(&c.url)).collect();
            let ids = super::crawler::fts_rowids(&conn, e, CRAWLER_TOP_N).unwrap_or_default();
            let meta = super::crawler::load_candidates(&conn, &ids).unwrap_or_default();
            let mut rank = 0usize;
            let mut per_domain: HashMap<String, usize> = HashMap::new();
            for id in ids {
                if let Some(c) = meta.get(&id) {
                    // Dedup vs search_pages by normalized URL — prefer the (fresher,
                    // retention-carrying) search_pages copy; drop the crawler dupe.
                    if sp_urls.contains(&norm_url(&c.url)) {
                        continue;
                    }
                    // Cap per domain so one exhaustively-crawled site can't flood.
                    let domain = super::adapters::host_of(&c.url).unwrap_or_default();
                    let n = per_domain.entry(domain).or_insert(0);
                    if *n >= CRAWLER_PER_DOMAIN {
                        continue;
                    }
                    *n += 1;
                    crawler_fts_rank.insert(id, rank);
                    rank += 1;
                    crawler_cands.push(c.clone());
                }
            }
        }

        if candidates.is_empty() && crawler_cands.is_empty() {
            return Vec::new();
        }
        (candidates, fts_rank, pos_rank, embeddings, clicks, crawler_cands, crawler_fts_rank)
    };

    // ---- async: embed the query, then compute vector ranking (search_pages only;
    //      crawler rows participate via FTS alone in v1, which RRF tolerates) ----
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

    // ---- pure RRF fusion + click boost, over the union of both indices ----
    let mut out: Vec<RankedResult> = Vec::with_capacity(candidates.len() + crawler_cands.len());
    for c in candidates {
        let click_boost = domain_click_boost(&c.url, &clicks);
        let f = fts_rank.get(&c.id).copied();
        let v = vec_rank.get(&c.id).copied();
        let p = pos_rank.get(&c.id).copied();
        let fused = rrf_term(W_FTS, f) + rrf_term(W_VEC, v) + rrf_term(W_POS, p) + click_boost;
        out.push(RankedResult {
            page_id: c.id,
            url: c.url,
            title: c.title,
            snippet: c.snippet,
            source_engine: c.source_engine,
            content_kind: c.content_kind,
            parent_url: c.parent_url,
            fused_score: fused,
            signals: RankSignals { fts_rank: f, vec_rank: v, pos_rank: p, click_boost },
            source_table: "search_pages",
            provenance: None,
        });
    }
    for c in crawler_cands {
        let click_boost = domain_click_boost(&c.url, &clicks);
        let f = crawler_fts_rank.get(&c.id).copied();
        let fused = rrf_term(W_FTS, f) + click_boost; // no vec/pos for crawler in v1
        out.push(RankedResult {
            page_id: c.id,
            url: c.url,
            title: if c.title.is_empty() { c.snippet.chars().take(80).collect() } else { c.title },
            snippet: c.snippet,
            source_engine: "crawl".to_string(),
            content_kind: "article".to_string(),
            parent_url: None,
            fused_score: fused,
            signals: RankSignals { fts_rank: f, vec_rank: None, pos_rank: None, click_boost },
            source_table: "scraped_pages",
            provenance: c.job,
        });
    }

    out.sort_by(|a, b| {
        b.fused_score
            .partial_cmp(&a.fused_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            // stable tiebreak across the two id spaces: source first, then id
            .then(a.source_table.cmp(b.source_table))
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
            content_kind: "article".into(),
            parent_url: None,
            author: None,
            engagement: None,
        }
    }
}
