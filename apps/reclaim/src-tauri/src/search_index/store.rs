//! Shared synchronous DB operations for the search index. Kept separate from the
//! async orchestrator/ranker so no `rusqlite::Statement` is ever held across an
//! `.await` (statements aren't Send). Each function does its work and returns
//! owned data.

use rusqlite::{params, Connection, OptionalExtension};

/// A page row as the ranker / UI needs it (no body — that's loaded only when
/// indexing or curating).
#[derive(Debug, Clone)]
pub struct StoredPage {
    pub id: i64,
    pub url: String,
    pub title: String,
    pub snippet: String,
    pub source_engine: String,
    pub searxng_pos: Option<i64>,
    pub content_kind: String,
    pub parent_url: Option<String>,
    pub author: Option<String>,
    pub engagement: Option<i64>,
}

/// Lightweight metadata for the orchestrator's cache check.
#[derive(Debug, Clone)]
pub struct PageMeta {
    pub id: i64,
    pub fetched_at: Option<i64>,
    pub expires_at: Option<i64>,
    pub retention: String,
    pub content_hash: Option<String>,
}

/// Insert a search_queries row, return its id.
pub fn insert_query(
    conn: &Connection,
    query_text: &str,
    retention: &str,
    now: i64,
    profile_id: i64,
) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO search_queries (query_text, created_at, retention, profile_id)
         VALUES (?1, ?2, ?3, ?4)",
        params![query_text, now, retention, profile_id],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Look up an existing page by URL within a profile (for the cache check).
pub fn find_page_by_url(
    conn: &Connection,
    profile_id: i64,
    url: &str,
) -> rusqlite::Result<Option<PageMeta>> {
    conn.query_row(
        "SELECT id, fetched_at, expires_at, retention, content_hash
         FROM search_pages WHERE url = ?1 AND profile_id = ?2",
        params![url, profile_id],
        |r| {
            Ok(PageMeta {
                id: r.get(0)?,
                fetched_at: r.get(1)?,
                expires_at: r.get(2)?,
                retention: r.get(3)?,
                content_hash: r.get(4)?,
            })
        },
    )
    .optional()
}

/// Re-attach an existing (cache-hit) page to the current query so it ranks within
/// this search, and bump usage. Does not touch body/retention.
pub fn attach_to_query(
    conn: &Connection,
    page_id: i64,
    query_id: i64,
    searxng_pos: Option<usize>,
    now: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE search_pages
            SET query_id = ?2,
                searxng_pos = COALESCE(?3, searxng_pos),
                last_opened_at = ?4,
                open_count = open_count + 1
          WHERE id = ?1",
        params![page_id, query_id, searxng_pos.map(|p| p as i64), now],
    )?;
    Ok(())
}

/// Insert a freshly scraped page (or update the existing row for this URL),
/// returning its id. Protects pinned/archived retention + expiry from being
/// downgraded by an ephemeral search that re-scrapes the same URL.
#[allow(clippy::too_many_arguments)]
pub fn upsert_scraped(
    conn: &Connection,
    profile_id: i64,
    url: &str,
    title: &str,
    body: &str,
    snippet: &str,
    query_id: Option<i64>,
    source_engine: &str,
    searxng_pos: Option<usize>,
    content_hash: &str,
    retention: &str,
    now: i64,
    expires_at: Option<i64>,
) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO search_pages
            (url, title, body, query_id, source_engine, content_hash, fetched_at,
             expires_at, retention, searxng_pos, snippet, last_opened_at, open_count, profile_id)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?7,0,?12)
         ON CONFLICT(url, profile_id) DO UPDATE SET
            title = excluded.title,
            body = excluded.body,
            -- browse (NULL query) keeps the existing query attribution
            query_id = COALESCE(excluded.query_id, search_pages.query_id),
            source_engine = excluded.source_engine,
            content_hash = excluded.content_hash,
            fetched_at = excluded.fetched_at,
            searxng_pos = excluded.searxng_pos,
            snippet = excluded.snippet,
            -- never silently downgrade a pinned/archived row to a cheap tier
            retention = CASE WHEN search_pages.retention IN ('pinned','archived')
                             THEN search_pages.retention ELSE excluded.retention END,
            expires_at = CASE WHEN search_pages.retention IN ('pinned','archived')
                              THEN search_pages.expires_at ELSE excluded.expires_at END",
        params![
            url, title, body, query_id, source_engine, content_hash, now, expires_at,
            retention, searxng_pos.map(|p| p as i64), snippet, profile_id
        ],
    )?;
    let id = conn.query_row(
        "SELECT id FROM search_pages WHERE url = ?1 AND profile_id = ?2",
        params![url, profile_id],
        |r| r.get(0),
    )?;
    Ok(id)
}

/// Store (or replace) a page's embedding vector.
pub fn upsert_embedding(
    conn: &Connection,
    page_id: i64,
    vec: &[f32],
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO page_embeddings (page_id, dim, vec) VALUES (?1, ?2, ?3)
         ON CONFLICT(page_id) DO UPDATE SET dim = excluded.dim, vec = excluded.vec",
        params![page_id, vec.len() as i64, super::embed::to_blob(vec)],
    )?;
    Ok(())
}

/// All pages attached to a query, as the ranker/UI needs them, optionally filtered
/// to a set of content_kinds ("comments only", etc.). `kinds = None` = all kinds.
pub fn candidate_pages(
    conn: &Connection,
    query_id: i64,
    kinds: Option<&[String]>,
) -> rusqlite::Result<Vec<StoredPage>> {
    let map = |r: &rusqlite::Row| -> rusqlite::Result<StoredPage> {
        Ok(StoredPage {
            id: r.get(0)?,
            url: r.get(1)?,
            title: r.get(2)?,
            snippet: r.get(3)?,
            source_engine: r.get(4)?,
            searxng_pos: r.get(5)?,
            content_kind: r.get(6)?,
            parent_url: r.get(7)?,
            author: r.get(8)?,
            engagement: r.get(9)?,
        })
    };
    const COLS: &str = "id, url, title, COALESCE(snippet,''), COALESCE(source_engine,'web'), \
                        searxng_pos, COALESCE(content_kind,'article'), parent_url, author, engagement";
    match kinds {
        Some(ks) if !ks.is_empty() => {
            // Dynamic IN-list of placeholders, bound positionally after query_id.
            let placeholders = std::iter::repeat("?").take(ks.len()).collect::<Vec<_>>().join(",");
            let sql = format!(
                "SELECT {COLS} FROM search_pages WHERE query_id = ?1 AND content_kind IN ({placeholders})"
            );
            let mut stmt = conn.prepare(&sql)?;
            let mut binds: Vec<&dyn rusqlite::ToSql> = vec![&query_id];
            for k in ks {
                binds.push(k);
            }
            let rows = stmt.query_map(binds.as_slice(), map)?;
            rows.collect()
        }
        _ => {
            let mut stmt =
                conn.prepare(&format!("SELECT {COLS} FROM search_pages WHERE query_id = ?1"))?;
            let rows = stmt.query_map(params![query_id], map)?;
            rows.collect()
        }
    }
}

/// Insert/update one typed segment (post/comment/forum_*). Mirrors upsert_scraped's
/// pinned-retention protection, and additionally writes the typed columns.
#[allow(clippy::too_many_arguments)]
pub fn upsert_segment(
    conn: &Connection,
    profile_id: i64,
    seg: &super::adapters::Segment,
    query_id: Option<i64>,
    source_engine: &str,
    content_hash: &str,
    retention: &str,
    now: i64,
    expires_at: Option<i64>,
) -> rusqlite::Result<i64> {
    let title = seg.title.clone().unwrap_or_default();
    let snippet: String = seg.text.split_whitespace().take(40).collect::<Vec<_>>().join(" ");
    conn.execute(
        "INSERT INTO search_pages
            (url, title, body, query_id, source_engine, content_hash, fetched_at,
             expires_at, retention, snippet, content_kind, parent_url, author, engagement,
             last_opened_at, open_count, profile_id)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?7,0,?15)
         ON CONFLICT(url, profile_id) DO UPDATE SET
            title = excluded.title,
            body = excluded.body,
            query_id = COALESCE(excluded.query_id, search_pages.query_id),
            source_engine = excluded.source_engine,
            content_hash = excluded.content_hash,
            fetched_at = excluded.fetched_at,
            snippet = excluded.snippet,
            content_kind = excluded.content_kind,
            parent_url = excluded.parent_url,
            author = excluded.author,
            engagement = excluded.engagement,
            retention = CASE WHEN search_pages.retention IN ('pinned','archived')
                             THEN search_pages.retention ELSE excluded.retention END,
            expires_at = CASE WHEN search_pages.retention IN ('pinned','archived')
                              THEN search_pages.expires_at ELSE excluded.expires_at END",
        params![
            seg.url, title, seg.text, query_id, source_engine, content_hash, now, expires_at,
            retention, snippet, seg.kind.as_str(), seg.parent_url, seg.author, seg.engagement,
            profile_id
        ],
    )?;
    conn.query_row(
        "SELECT id FROM search_pages WHERE url = ?1 AND profile_id = ?2",
        params![seg.url, profile_id],
        |r| r.get(0),
    )
}

/// FTS5 rowids for a MATCH expression, best-first (BM25 `rank` ascending = best).
/// `match_expr` MUST already be sanitized (see rank::sanitize_fts_query).
pub fn fts_order(conn: &Connection, match_expr: &str) -> rusqlite::Result<Vec<i64>> {
    let mut stmt = conn.prepare(
        "SELECT rowid FROM search_pages_fts
          WHERE search_pages_fts MATCH ?1 ORDER BY rank",
    )?;
    let rows = stmt.query_map(params![match_expr], |r| r.get::<_, i64>(0))?;
    rows.collect()
}

/// Load embeddings for a set of page ids.
pub fn load_embeddings(
    conn: &Connection,
    ids: &[i64],
) -> rusqlite::Result<std::collections::HashMap<i64, Vec<f32>>> {
    let mut map = std::collections::HashMap::new();
    let mut stmt = conn.prepare("SELECT vec FROM page_embeddings WHERE page_id = ?1")?;
    for &id in ids {
        if let Some(blob) = stmt
            .query_row(params![id], |r| r.get::<_, Vec<u8>>(0))
            .optional()?
        {
            map.insert(id, super::embed::from_blob(&blob));
        }
    }
    Ok(map)
}

/// Per-domain click counts for the personalization boost (profile-scoped).
pub fn domain_click_counts(
    conn: &Connection,
    profile_id: i64,
) -> rusqlite::Result<std::collections::HashMap<String, i64>> {
    let mut map = std::collections::HashMap::new();
    let mut stmt = conn.prepare(
        "SELECT domain, COUNT(*) FROM result_clicks
          WHERE domain IS NOT NULL AND (profile_id = ?1 OR profile_id IS NULL)
          GROUP BY domain",
    )?;
    let rows = stmt.query_map(params![profile_id], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
    })?;
    for row in rows {
        let (d, c) = row?;
        map.insert(d, c);
    }
    Ok(map)
}

/// Record a result click (personalization signal; never leaves the device).
pub fn insert_click(
    conn: &Connection,
    query_id: Option<i64>,
    url: &str,
    domain: Option<&str>,
    now: i64,
    profile_id: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO result_clicks (query_id, url, domain, clicked_at, profile_id)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![query_id, url, domain, now, profile_id],
    )?;
    Ok(())
}

/// Bump usage counters for a page when the user navigates to it.
pub fn bump_usage(conn: &Connection, page_id: i64, now: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE search_pages SET last_opened_at = ?2, open_count = open_count + 1 WHERE id = ?1",
        params![page_id, now],
    )?;
    Ok(())
}

// ---- Phase 6 lifecycle DB ops ----

/// url/title/body for a page (body may be NULL for archived rows).
pub fn page_content(
    conn: &Connection,
    page_id: i64,
) -> rusqlite::Result<Option<(String, String, Option<String>)>> {
    conn.query_row(
        "SELECT url, title, body FROM search_pages WHERE id = ?1",
        params![page_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )
    .optional()
}

/// Promote a page to the pinned tier (permanent, GC-protected).
pub fn pin(conn: &Connection, page_id: i64, now: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE search_pages
            SET retention = 'pinned', expires_at = NULL, pinned_at = COALESCE(pinned_at, ?2)
          WHERE id = ?1",
        params![page_id, now],
    )?;
    Ok(())
}

/// Demote a page to the cache tier with a fresh TTL (used on unfavorite — never a
/// hard delete; let the decay model handle eventual archival).
pub fn demote_to_cache(conn: &Connection, page_id: i64, expires_at: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE search_pages
            SET retention = 'cache', expires_at = ?2, pinned_at = NULL
          WHERE id = ?1",
        params![page_id, expires_at],
    )?;
    Ok(())
}

/// Archive: drop body (and its FTS terms via the update trigger) + embeddings,
/// keep url/title and the knowledge-graph entry. Reclaims ~all disk.
pub fn archive(conn: &Connection, page_id: i64) -> rusqlite::Result<()> {
    // Delete embedding explicitly (don't rely solely on the FK cascade).
    conn.execute("DELETE FROM page_embeddings WHERE page_id = ?1", params![page_id])?;
    // Setting body = NULL fires the update trigger, emptying FTS for this row.
    conn.execute(
        "UPDATE search_pages
            SET retention = 'archived', body = NULL, expires_at = NULL
          WHERE id = ?1",
        params![page_id],
    )?;
    Ok(())
}

/// Hard delete a page now (FTS via delete trigger, embedding via cascade).
pub fn forget(conn: &Connection, page_id: i64) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM page_embeddings WHERE page_id = ?1", params![page_id])?;
    conn.execute("DELETE FROM search_pages WHERE id = ?1", params![page_id])?;
    Ok(())
}

/// Forget a whole search: drop its query row and any of its pages that the user
/// never committed to (pinned/archived pages survive — a promise the curator must
/// not break).
pub fn forget_query(conn: &Connection, query_id: i64) -> rusqlite::Result<usize> {
    conn.execute(
        "DELETE FROM page_embeddings WHERE page_id IN
           (SELECT id FROM search_pages WHERE query_id = ?1 AND retention NOT IN ('pinned','archived'))",
        params![query_id],
    )?;
    let n = conn.execute(
        "DELETE FROM search_pages WHERE query_id = ?1 AND retention NOT IN ('pinned','archived')",
        params![query_id],
    )?;
    conn.execute("DELETE FROM search_queries WHERE id = ?1", params![query_id])?;
    Ok(n)
}

/// Flag a pinned page whose upstream is confirmed gone (404/410). PROTECT, don't
/// prune — the local copy may be the only one left.
pub fn mark_upstream_gone(conn: &Connection, page_id: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE search_pages SET upstream_gone = 1 WHERE id = ?1",
        params![page_id],
    )?;
    Ok(())
}

/// A pinned page with the signals review_pinned scores on.
#[derive(Debug, Clone)]
pub struct PinnedRow {
    pub id: i64,
    pub url: String,
    pub title: String,
    pub last_opened_at: Option<i64>,
    pub open_count: i64,
    pub pinned_at: Option<i64>,
    pub upstream_gone: bool,
    pub content_hash: Option<String>,
}

/// All pinned pages for a profile (input to the curator's review).
pub fn pinned_pages(conn: &Connection, profile_id: i64) -> rusqlite::Result<Vec<PinnedRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, url, title, last_opened_at, open_count, pinned_at, upstream_gone, content_hash
           FROM search_pages WHERE retention = 'pinned' AND profile_id = ?1",
    )?;
    let rows = stmt.query_map(params![profile_id], |r| {
        Ok(PinnedRow {
            id: r.get(0)?,
            url: r.get(1)?,
            title: r.get(2)?,
            last_opened_at: r.get(3)?,
            open_count: r.get(4)?,
            pinned_at: r.get(5)?,
            upstream_gone: r.get::<_, i64>(6)? != 0,
            content_hash: r.get(7)?,
        })
    })?;
    rows.collect()
}

/// Total bytes of indexed body text (disk soft-cap backstop signal).
pub fn index_body_bytes(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row(
        "SELECT COALESCE(SUM(LENGTH(body)), 0) FROM search_pages",
        [],
        |r| r.get(0),
    )
}

/// Whether the knowledge graph already has a non-empty curated summary for a URL.
pub fn has_curated_summary(conn: &Connection, url: &str, profile_id: i64) -> rusqlite::Result<bool> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM indexed_pages
          WHERE url = ?1 AND profile_id = ?2 AND summary IS NOT NULL AND TRIM(summary) <> ''",
        params![url, profile_id],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

/// GC sweep: delete expired ephemeral/cache rows (never pinned/archived). Returns
/// the number of pages removed. Embeddings dropped explicitly + via cascade; FTS
/// self-cleans via the delete trigger.
pub fn gc_sweep(conn: &Connection, now: i64) -> rusqlite::Result<usize> {
    conn.execute(
        "DELETE FROM page_embeddings WHERE page_id IN (
            SELECT id FROM search_pages
             WHERE retention IN ('ephemeral','cache')
               AND expires_at IS NOT NULL AND expires_at < ?1)",
        params![now],
    )?;
    let n = conn.execute(
        "DELETE FROM search_pages
          WHERE retention IN ('ephemeral','cache')
            AND expires_at IS NOT NULL AND expires_at < ?1",
        params![now],
    )?;
    Ok(n)
}
