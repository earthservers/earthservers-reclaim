//! Retention tiers, TTL math, and the lifecycle ladder.
//!
//! browse → auto-cache (cheap, TTL'd, no curation) → favorite/pin (permanent,
//! curated) → archived (summary kept, body/FTS/embeddings dropped) → forgotten.
//!
//! Pinning is a promise: pinned/archived rows are NEVER auto-GC'd. Only ephemeral
//! and cache tiers expire without a human in the loop.

/// Seconds in an hour / week, for TTL math.
pub const HOUR: i64 = 3_600;
pub const DAY: i64 = 86_400;
pub const WEEK: i64 = 7 * DAY;

/// Ephemeral one-off searches live ~1h; cache lives ~7d.
pub const EPHEMERAL_TTL: i64 = HOUR;
pub const CACHE_TTL: i64 = WEEK;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Retention {
    Ephemeral,
    Cache,
    Pinned,
    Archived,
}

impl Retention {
    pub fn as_str(self) -> &'static str {
        match self {
            Retention::Ephemeral => "ephemeral",
            Retention::Cache => "cache",
            Retention::Pinned => "pinned",
            Retention::Archived => "archived",
        }
    }

    /// Parse a tier string; unknown/garbage falls back to the safest cheap tier.
    pub fn parse(s: &str) -> Retention {
        match s.trim().to_ascii_lowercase().as_str() {
            "cache" => Retention::Cache,
            "pinned" | "pin" | "favorite" | "favourite" => Retention::Pinned,
            "archived" | "archive" => Retention::Archived,
            _ => Retention::Ephemeral,
        }
    }

    /// `expires_at` for a freshly-written row in this tier, given `now` (unix s).
    /// Pinned/archived never expire (NULL).
    pub fn expires_at(self, now: i64) -> Option<i64> {
        match self {
            Retention::Ephemeral => Some(now + EPHEMERAL_TTL),
            Retention::Cache => Some(now + CACHE_TTL),
            Retention::Pinned | Retention::Archived => None,
        }
    }

    /// Whether automation may ever delete this tier without asking the user.
    pub fn auto_gc_eligible(self) -> bool {
        matches!(self, Retention::Ephemeral | Retention::Cache)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_is_lenient_and_safe() {
        assert_eq!(Retention::parse("CACHE"), Retention::Cache);
        assert_eq!(Retention::parse("pin"), Retention::Pinned);
        assert_eq!(Retention::parse("nonsense"), Retention::Ephemeral);
        assert_eq!(Retention::parse(""), Retention::Ephemeral);
    }

    #[test]
    fn expiry_math() {
        let now = 1_000_000;
        assert_eq!(Retention::Ephemeral.expires_at(now), Some(now + HOUR));
        assert_eq!(Retention::Cache.expires_at(now), Some(now + WEEK));
        assert_eq!(Retention::Pinned.expires_at(now), None);
        assert_eq!(Retention::Archived.expires_at(now), None);
    }

    #[test]
    fn only_cheap_tiers_auto_gc() {
        assert!(Retention::Ephemeral.auto_gc_eligible());
        assert!(Retention::Cache.auto_gc_eligible());
        assert!(!Retention::Pinned.auto_gc_eligible());
        assert!(!Retention::Archived.auto_gc_eligible());
    }
}
