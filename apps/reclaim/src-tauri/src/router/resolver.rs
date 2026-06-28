//! RESOLUTION axis — an ordered resolver chain.
//!
//! The chain tries each resolver in sequence and the first one that answers
//! (returns `Some`) wins; a `None` falls through to the next. The intended
//! order is:
//!
//! ```text
//! LocalCache -> P2pCompany(.click) -> Federated -> Blockchain(.earth) -> IcannDns(legacy)
//! ```
//!
//! Resolution is independent of rendering: [`ResolvedTarget`] carries the
//! [`DomainClass`] from the parsed URL so the render axis can pick an engine
//! regardless of which resolver answered.
//!
//! Phase 1 ships with an EMPTY chain and an identity fallback (see
//! [`ResolverChain::resolve`]). Real resolvers (LocalCache, IcannDns) and the
//! no-op stubs (P2pCompany, FederatedRegistry, BlockchainRegistry) arrive in
//! Phase 2 under `router/resolvers/`.

use super::url::{DomainClass, ParsedUrl};

/// Which resolver produced a [`ResolvedTarget`]. Surfaced to the UI/telemetry
/// and used by Phase 2 to decide whether to write a cache entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ResolverSource {
    /// No resolver answered; host/url used as-is (chain fallthrough).
    Identity,
    LocalCache,
    P2pCompany,
    Federated,
    Blockchain,
    IcannDns,
}

/// A successfully resolved navigation target, ready for a render engine.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedTarget {
    pub host: String,
    /// URL the engine should actually load. May differ from the input host once
    /// real resolvers exist (e.g. a gateway URL or rewritten endpoint).
    pub url: String,
    /// Carried from [`ParsedUrl`]; the render axis selects an engine from this.
    pub class: DomainClass,
    pub source: ResolverSource,
    // Phase 2+ seams (add when a resolver needs them):
    //   pub expires_at: Option<i64>,   // cache TTL
    //   pub peers: Vec<String>,        // P2P
    //   pub content_hash: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error("resolver backend error: {0}")]
    Backend(String),
}

/// A single step in the resolution chain.
///
/// Implementors return `Ok(Some(_))` to answer and stop the chain, `Ok(None)`
/// to fall through to the next resolver, or `Err(_)` on a hard backend failure.
/// A resolver that only serves certain TLDs should inspect `req.class` and
/// return `Ok(None)` for classes it does not handle. The full [`ParsedUrl`] is
/// passed (not just the host) so a resolver can preserve the request path when
/// building its [`ResolvedTarget`].
#[async_trait::async_trait]
pub trait Resolver: Send + Sync {
    fn name(&self) -> &'static str;

    async fn resolve(&self, req: &ParsedUrl) -> Result<Option<ResolvedTarget>, ResolveError>;
}

/// Ordered collection of resolvers run front-to-back.
pub struct ResolverChain {
    resolvers: Vec<Box<dyn Resolver>>,
}

impl ResolverChain {
    pub fn new(resolvers: Vec<Box<dyn Resolver>>) -> Self {
        Self { resolvers }
    }

    /// Run the chain. The first resolver to return `Some` wins. If every
    /// resolver falls through (or the chain is empty, as in Phase 1), fall back
    /// to an identity target so the render axis always has something to draw.
    pub async fn resolve(&self, parsed: &ParsedUrl) -> Result<ResolvedTarget, ResolveError> {
        for r in &self.resolvers {
            if let Some(target) = r.resolve(parsed).await? {
                log::info!("[router] '{}' resolved by {}", parsed.host, r.name());
                return Ok(target);
            }
        }

        log::debug!("[router] '{}' fell through to identity", parsed.host);
        Ok(ResolvedTarget {
            host: parsed.host.clone(),
            url: parsed.url.clone(),
            class: parsed.class,
            source: ResolverSource::Identity,
        })
    }
}
