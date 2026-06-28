//! Resolution-axis backends — the steps of the resolver chain, in order:
//! LocalCache -> P2pCompany(.click) -> Federated -> Blockchain(.earth) -> IcannDns.

pub mod icann_dns;
pub mod local_cache;
pub mod stubs;
