//! Browser routing — a single front door for all navigation.
//!
//! Every navigation flows through the [`navigate`] command, which runs two
//! orthogonal axes:
//!
//! * **RESOLUTION** ([`resolver`]): an ordered chain that finds a host
//!   (LocalCache -> P2P(.click) -> Federated -> Blockchain(.earth) -> ICANN).
//! * **RENDER** ([`engine`]): the engine that draws the resolved target
//!   (`.earth` -> Servo; `.click`/legacy -> WebKitGTK).
//!
//! They are independent: a `.click` resolves via P2P but renders in WebKitGTK;
//! a `.earth` resolves via blockchain but renders in Servo. The [`url::DomainClass`]
//! is computed once at parse time and flows through unchanged, so the render
//! axis never depends on which resolver answered.
//!
//! Phase 1: the resolver chain is empty (identity fallback) and WebKitGTK is the
//! only render engine, wired to the existing `webview::create_browser_webview`.

pub mod engine;
pub mod engines;
pub mod resolver;
pub mod resolvers;
pub mod url;

use std::sync::Arc;

use crate::webview::WebviewBounds;
use engine::{RenderCtx, RenderEngine};
use engines::servo::ServoEngine;
use engines::webkit::WebKitEngine;
use resolver::{ResolverChain, ResolverSource};
use resolvers::icann_dns::IcannDnsResolver;
use resolvers::local_cache::{LocalCache, LocalCacheResolver, DEFAULT_TTL_SECS};
use resolvers::stubs::{BlockchainRegistryResolver, FederatedRegistryResolver, P2pCompanyResolver};
use url::DomainClass;

/// Router state, managed by Tauri SEPARATELY from `Mutex<AppState>` so that a
/// navigation never contends on the database lock. Read-only and `Send + Sync`.
pub struct Router {
    chain: ResolverChain,
    engines: Vec<Box<dyn RenderEngine>>,
    /// Resolution cache store — shared with the `LocalCacheResolver` in the
    /// chain; the router writes successful resolutions back to it.
    cache: Arc<LocalCache>,
}

impl Router {
    /// Build the router: the ordered resolver chain
    /// (LocalCache -> P2P(.click) -> Federated -> Blockchain(.earth) -> ICANN)
    /// and the render engines (Servo for `.earth`, WebKitGTK for the rest).
    /// `.click`/Federated/Blockchain are no-op stubs; LocalCache + IcannDns are real.
    pub fn new(db_path: String) -> Self {
        let cache = Arc::new(LocalCache::new(db_path));
        if let Err(e) = cache.init() {
            log::error!("[router] failed to init resolution cache: {}", e);
        }

        let chain = ResolverChain::new(vec![
            Box::new(LocalCacheResolver::new(cache.clone())),
            Box::new(P2pCompanyResolver),
            Box::new(FederatedRegistryResolver),
            Box::new(BlockchainRegistryResolver),
            Box::new(IcannDnsResolver),
        ]);

        // Engine selection is first-match, so Servo (Earth-only) is registered
        // BEFORE WebKitGTK (everything-but-Earth).
        Self {
            chain,
            engines: vec![Box::new(ServoEngine), Box::new(WebKitEngine)],
            cache,
        }
    }

    /// First registered engine that handles `class`.
    fn select_engine(&self, class: DomainClass) -> Option<&dyn RenderEngine> {
        self.engines.iter().find(|e| e.handles(class)).map(|e| e.as_ref())
    }
}

/// What the frontend gets back from a navigation. Title is NOT here — it is
/// unknown at navigation time and arrives later via the `browser-title-changed`
/// event (emitted in Phase 3). Callers use `host` as the provisional title.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NavOutcome {
    pub final_url: String,
    pub host: String,
    pub class: DomainClass,
    pub resolver_source: ResolverSource,
    pub engine: String,
    pub is_internal: bool,
}

/// The single navigation front door. ALL navigation flows through here.
///
/// Parse + classify -> resolver chain (RESOLUTION) -> engine by class (RENDER).
/// Internal app routes (`earth://`, `tauri://`) short-circuit before either axis.
#[tauri::command(rename_all = "camelCase")]
pub async fn navigate(
    app: tauri::AppHandle,
    router: tauri::State<'_, Router>,
    tab_id: i64,
    url: String,
    bounds: Option<WebviewBounds>,
) -> Result<NavOutcome, String> {
    let parsed = url::parse(&url);

    // Internal app route: don't resolve, don't touch the external webview.
    if parsed.is_internal {
        return Ok(NavOutcome {
            final_url: parsed.url,
            host: parsed.host,
            class: parsed.class,
            resolver_source: ResolverSource::Identity,
            engine: "internal".to_string(),
            is_internal: true,
        });
    }

    // RESOLUTION axis.
    let target = router.chain.resolve(&parsed).await.map_err(|e| e.to_string())?;

    // Cache real resolutions so the next hit short-circuits the chain. Skip
    // identity fallbacks and entries that already came from the cache.
    if !matches!(target.source, ResolverSource::Identity | ResolverSource::LocalCache) {
        let provenance = format!("{:?}", target.source);
        if let Err(e) = router.cache.store(&target.host, target.class, &provenance, DEFAULT_TTL_SECS) {
            log::warn!("[router] cache store failed for '{}': {}", target.host, e);
        }
    }

    // RENDER axis — engine chosen by class, independent of the resolver.
    let engine = router
        .select_engine(target.class)
        .ok_or_else(|| format!("no render engine for {:?}", target.class))?;
    let engine_name = engine.name().to_string();

    let ctx = RenderCtx { app, tab_id, bounds };
    engine.render(&ctx, &target).await.map_err(|e| e.to_string())?;

    Ok(NavOutcome {
        final_url: target.url,
        host: target.host,
        class: target.class,
        resolver_source: target.source,
        engine: engine_name,
        is_internal: false,
    })
}

/// DEV: manually seed a resolution-cache entry so a host resolves via the cache
/// (LocalCache hit) before the real P2P/blockchain resolvers exist. This lets
/// you test the `.click` -> WebKitGTK and `.earth` -> Servo render paths today:
/// seed the host, then `navigate` to it — the chain hits the cache and the
/// render engine is chosen by the cached class.
///
/// `class` is one of "earth" | "click" | "legacy"; if omitted it is inferred
/// from the host's TLD. `ttlSecs` defaults to the standard cache TTL.
#[tauri::command(rename_all = "camelCase")]
pub async fn router_seed_cache(
    router: tauri::State<'_, Router>,
    host: String,
    class: Option<String>,
    ttl_secs: Option<i64>,
) -> Result<(), String> {
    let cls = match class.as_deref() {
        Some("earth") => DomainClass::Earth,
        Some("click") => DomainClass::Click,
        Some("legacy") => DomainClass::Legacy,
        _ => url::classify_tld(&host),
    };
    router
        .cache
        .store(&host, cls, "seed", ttl_secs.unwrap_or(DEFAULT_TTL_SECS))
}

/// DEV: clear the entire resolution cache. Returns the number of rows removed.
#[tauri::command(rename_all = "camelCase")]
pub async fn router_clear_cache(router: tauri::State<'_, Router>) -> Result<usize, String> {
    router.cache.clear()
}
