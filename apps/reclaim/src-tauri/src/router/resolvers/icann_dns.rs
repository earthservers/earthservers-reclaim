//! IcannDns resolver — standard system DNS behavior (the legacy fallback, last
//! in the chain).
//!
//! For an ordinary domain we confirm it resolves via the system resolver and
//! answer with the request URL unchanged (the engine re-resolves and loads it
//! normally). If DNS fails we fall through (`None`) so the chain's identity
//! fallback still lets the engine try and surface its own error page.

use crate::router::resolver::{ResolveError, ResolvedTarget, Resolver, ResolverSource};
use crate::router::url::ParsedUrl;

pub struct IcannDnsResolver;

#[async_trait::async_trait]
impl Resolver for IcannDnsResolver {
    fn name(&self) -> &'static str {
        "IcannDns"
    }

    async fn resolve(&self, req: &ParsedUrl) -> Result<Option<ResolvedTarget>, ResolveError> {
        if req.host.is_empty() {
            return Ok(None);
        }

        // System DNS lookup (async; tokio runs it on its blocking pool).
        let resolves = tokio::net::lookup_host((req.host.as_str(), 443u16))
            .await
            .map(|mut addrs| addrs.next().is_some())
            .unwrap_or(false);

        if resolves {
            Ok(Some(ResolvedTarget {
                host: req.host.clone(),
                url: req.url.clone(),
                class: req.class,
                source: ResolverSource::IcannDns,
            }))
        } else {
            log::debug!("[router] IcannDns: '{}' did not resolve", req.host);
            Ok(None)
        }
    }
}
