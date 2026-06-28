//! No-op resolver stubs — clean fall-through seams for resolution backends that
//! are intentionally NOT implemented in this phase (no P2P / federated /
//! blockchain networking). Each returns `Ok(None)` so the chain proceeds to the
//! next resolver. The types + chain positions exist so wiring real backends
//! later is a drop-in replacement.

use crate::router::resolver::{ResolveError, ResolvedTarget, Resolver};
use crate::router::url::ParsedUrl;

/// Peer-to-peer "company" resolver for `.click` hosts.
pub struct P2pCompanyResolver;

#[async_trait::async_trait]
impl Resolver for P2pCompanyResolver {
    fn name(&self) -> &'static str {
        "P2pCompany"
    }

    async fn resolve(&self, _req: &ParsedUrl) -> Result<Option<ResolvedTarget>, ResolveError> {
        // TODO(p2p): resolve DomainClass::Click hosts via the peer-to-peer
        // network; return Ok(None) for every other class so the chain falls
        // through. No networking implemented yet.
        Ok(None)
    }
}

/// Federated registry resolver.
pub struct FederatedRegistryResolver;

#[async_trait::async_trait]
impl Resolver for FederatedRegistryResolver {
    fn name(&self) -> &'static str {
        "FederatedRegistry"
    }

    async fn resolve(&self, _req: &ParsedUrl) -> Result<Option<ResolvedTarget>, ResolveError> {
        // TODO(federated): query the federated registry servers. Not implemented.
        Ok(None)
    }
}

/// Blockchain registry resolver for `.earth` hosts.
pub struct BlockchainRegistryResolver;

#[async_trait::async_trait]
impl Resolver for BlockchainRegistryResolver {
    fn name(&self) -> &'static str {
        "BlockchainRegistry"
    }

    async fn resolve(&self, _req: &ParsedUrl) -> Result<Option<ResolvedTarget>, ResolveError> {
        // TODO(blockchain): resolve DomainClass::Earth hosts via the blockchain
        // registry; return Ok(None) for every other class. Not implemented.
        Ok(None)
    }
}
