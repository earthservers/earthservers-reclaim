//! LocalCache resolver — SQLite-backed resolution cache with TTL.
//!
//! Caches the *resolution* of a host (its class + which axis answered), NOT the
//! full request URL. So a cache hit for `example.com/a` still serves
//! `example.com/b` correctly: the returned target uses the CURRENT request URL,
//! and the cache only short-circuits the "is this host known/reachable" step.

use std::sync::Arc;
use std::time::Duration;

use rusqlite::Connection;

use crate::router::resolver::{ResolveError, ResolvedTarget, Resolver, ResolverSource};
use crate::router::url::{DomainClass, ParsedUrl};

/// Default time-to-live for a cached resolution (seconds).
pub const DEFAULT_TTL_SECS: i64 = 3600; // 1 hour

fn now_ts() -> i64 {
    chrono::Utc::now().timestamp()
}

fn class_to_str(c: DomainClass) -> &'static str {
    match c {
        DomainClass::Earth => "earth",
        DomainClass::Click => "click",
        DomainClass::Legacy => "legacy",
    }
}

fn class_from_str(s: &str) -> DomainClass {
    match s {
        "earth" => DomainClass::Earth,
        "click" => DomainClass::Click,
        _ => DomainClass::Legacy,
    }
}

/// The cache store (read + write). Shared via `Arc` between the resolver in the
/// chain (reads) and the router (writes successful resolutions).
pub struct LocalCache {
    db_path: String,
}

#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub class: DomainClass,
    #[allow(dead_code)] // provenance, surfaced for debugging/inspection
    pub source: String,
    #[allow(dead_code)]
    pub expires_at: i64,
}

impl LocalCache {
    pub fn new(db_path: String) -> Self {
        Self { db_path }
    }

    fn conn(&self) -> Result<Connection, String> {
        let conn = Connection::open(&self.db_path).map_err(|e| e.to_string())?;
        // Tolerate brief contention with the other managers sharing this DB.
        conn.busy_timeout(Duration::from_millis(2000)).ok();
        Ok(conn)
    }

    pub fn init(&self) -> Result<(), String> {
        let conn = self.conn()?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS resolution_cache (
                host        TEXT PRIMARY KEY,
                class       TEXT NOT NULL,
                source      TEXT NOT NULL,
                resolved_at INTEGER NOT NULL,
                expires_at  INTEGER NOT NULL
            )",
            [],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Return a non-expired entry for `host`, if any.
    pub fn lookup(&self, host: &str) -> Option<CacheEntry> {
        let conn = self.conn().ok()?;
        conn.query_row(
            "SELECT class, source, expires_at FROM resolution_cache \
             WHERE host = ?1 AND expires_at > ?2",
            rusqlite::params![host, now_ts()],
            |row| {
                Ok(CacheEntry {
                    class: class_from_str(&row.get::<_, String>(0)?),
                    source: row.get::<_, String>(1)?,
                    expires_at: row.get::<_, i64>(2)?,
                })
            },
        )
        .ok()
    }

    /// Insert/replace a cache entry. `source` records provenance for debugging.
    pub fn store(
        &self,
        host: &str,
        class: DomainClass,
        source: &str,
        ttl_secs: i64,
    ) -> Result<(), String> {
        let conn = self.conn()?;
        let now = now_ts();
        conn.execute(
            "INSERT OR REPLACE INTO resolution_cache \
             (host, class, source, resolved_at, expires_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![host, class_to_str(class), source, now, now + ttl_secs.max(1)],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn clear(&self) -> Result<usize, String> {
        let conn = self.conn()?;
        conn.execute("DELETE FROM resolution_cache", []).map_err(|e| e.to_string())
    }
}

/// Chain resolver: a fresh cache entry short-circuits resolution.
pub struct LocalCacheResolver {
    cache: Arc<LocalCache>,
}

impl LocalCacheResolver {
    pub fn new(cache: Arc<LocalCache>) -> Self {
        Self { cache }
    }
}

#[async_trait::async_trait]
impl Resolver for LocalCacheResolver {
    fn name(&self) -> &'static str {
        "LocalCache"
    }

    async fn resolve(&self, req: &ParsedUrl) -> Result<Option<ResolvedTarget>, ResolveError> {
        if req.host.is_empty() {
            return Ok(None);
        }
        Ok(self.cache.lookup(&req.host).map(|entry| ResolvedTarget {
            host: req.host.clone(),
            // CURRENT request URL (preserves path); the cache answered "host
            // known", not a specific page.
            url: req.url.clone(),
            class: entry.class,
            source: ResolverSource::LocalCache,
        }))
    }
}
