//! Vault-access audit log + per-origin rate limiting — the trustworthy record of
//! every gated secret access, allowed or denied.
//!
//! This is the source of truth for the "something tried to read the vault from
//! where it shouldn't" signal in the Security panel. It records at the GATE (the
//! single backend access path), so it can't be bypassed by simply not calling a
//! logging helper elsewhere — the only way to read a secret is through a call that
//! audits.
//!
//! **Blast-radius rationale (why this matters even though web pages can't reach
//! the vault):** Phase 1's process boundary already stops a malicious *web page*.
//! This log + rate limit defend the next ring in: a compromised *frontend* (a
//! backdoored npm dep, a self-XSS in our own React chrome). Such an attacker is
//! inside the trusted UI, so it can call vault commands — but redact-by-default
//! means the list holds no plaintext, and every reveal is one rate-limited,
//! logged call. That turns "instant silent total dump" into "slow, capped,
//! detectable, one entry at a time" — and the Phase 7 curator can flag a reveal
//! burst as anomalous. We are buying least-privilege + detection, not a wall:
//! a fully compromised, trusted frontend is still bad. We say so honestly.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use rusqlite::{params, Connection};

use super::{PriorityTag, SecurityEvent, Severity};

/// What kind of gated vault action was attempted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Origin-bound autofill into the active page.
    Autofill,
    /// On-demand single-entry plaintext reveal (manager show/copy).
    Reveal,
    /// A page/frontend supplied an origin that did not match the real page origin.
    OriginMismatch,
    /// Rate limit tripped for an origin/bucket.
    RateLimited,
}

impl Action {
    fn as_str(self) -> &'static str {
        match self {
            Action::Autofill => "autofill",
            Action::Reveal => "reveal",
            Action::OriginMismatch => "origin-mismatch",
            Action::RateLimited => "rate-limited",
        }
    }
}

lazy_static::lazy_static! {
    /// DB path for the append-only audit table. Set in [`init`].
    static ref DB_PATH: Mutex<Option<String>> = Mutex::new(None);
    /// Sliding-window timestamps per bucket key, for rate limiting. In-memory only
    /// (a restart resets the window — acceptable; the persistent record remains).
    static ref WINDOWS: Mutex<HashMap<String, Vec<Instant>>> = Mutex::new(HashMap::new());
}

/// Max gated accesses allowed per bucket within [`WINDOW`]. Generous enough for
/// real human use (filling/​revealing a handful of logins), tight enough that an
/// automated mass-dump is throttled and leaves an obvious burst in the log.
const MAX_PER_WINDOW: usize = 8;
const WINDOW: Duration = Duration::from_secs(10);

pub fn init(db_path: &str) -> Result<(), String> {
    let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS vault_audit_log (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            ts         TEXT NOT NULL,
            profile_id INTEGER,
            action     TEXT NOT NULL,
            origin     TEXT,
            field      TEXT,
            allowed    INTEGER NOT NULL,
            reason     TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_vault_audit_ts ON vault_audit_log(ts);",
    )
    .map_err(|e| e.to_string())?;
    *DB_PATH.lock().map_err(|e| e.to_string())? = Some(db_path.to_string());
    Ok(())
}

fn db() -> Option<Connection> {
    let path = DB_PATH.lock().ok()?.clone()?;
    let c = Connection::open(path).ok()?;
    c.busy_timeout(Duration::from_millis(2000)).ok();
    Some(c)
}

/// Check the rate limit for `bucket` (typically an origin) WITHOUT recording a
/// hit. Returns true if a further access is allowed. Pruning of the window happens
/// here so callers don't grow it unbounded.
fn within_rate(bucket: &str) -> bool {
    let mut w = match WINDOWS.lock() {
        Ok(w) => w,
        Err(_) => return true, // fail open on a poisoned lock — we still audit
    };
    let now = Instant::now();
    let hits = w.entry(bucket.to_string()).or_default();
    hits.retain(|t| now.duration_since(*t) < WINDOW);
    hits.len() < MAX_PER_WINDOW
}

/// Record one rate-limit hit for `bucket` (call only on an ALLOWED access).
fn note_hit(bucket: &str) {
    if let Ok(mut w) = WINDOWS.lock() {
        w.entry(bucket.to_string()).or_default().push(Instant::now());
    }
}

/// Append a record to the audit table and emit a live `security-event`. This is
/// the single place the two stay in sync. Never panics; logging failure must not
/// break the gated operation.
fn record(profile_id: Option<i64>, action: Action, origin: Option<&str>, field: Option<&str>, allowed: bool, reason: &str) {
    let ts = super::now_rfc3339();
    if let Some(conn) = db() {
        let _ = conn.execute(
            "INSERT INTO vault_audit_log (ts, profile_id, action, origin, field, allowed, reason)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![ts, profile_id, action.as_str(), origin, field, allowed as i64, reason],
        );
    }
    // A denial at a real boundary is a trustworthy signal; an allowed access is
    // informational. Mismatch/rate-limit are warnings worth the user's eye.
    let (tag, severity) = match (action, allowed) {
        (Action::OriginMismatch, _) => (PriorityTag::Boundary, Severity::Warning),
        (Action::RateLimited, _) => (PriorityTag::Hardening, Severity::Warning),
        (_, false) => (PriorityTag::Boundary, Severity::Notice),
        (_, true) => (PriorityTag::Boundary, Severity::Info),
    };
    let title = match action {
        Action::Autofill => "Vault autofill",
        Action::Reveal => "Vault reveal",
        Action::OriginMismatch => "Vault origin mismatch (blocked)",
        Action::RateLimited => "Vault access rate-limited",
    };
    super::emit(&SecurityEvent {
        ts,
        category: "vault".into(),
        tag,
        severity,
        title: title.into(),
        detail: reason.into(),
        origin: origin.map(|s| s.to_string()),
        decision: Some(if allowed { "allowed".into() } else { "denied".into() }),
    });
}

/// Enforce the per-bucket rate limit for a gated access. Returns `Ok(())` if the
/// access may proceed (and counts the attempt), or `Err(reason)` if the limit is
/// exceeded (recording a RateLimited denial). The caller MUST treat `Err` as
/// fail-closed (return no secret) and, on success, follow up with [`allow`] or
/// [`deny`] to record the actual outcome. `field` is a short, non-secret label
/// (e.g. an origin host or "entry:<id>") used for the log and the rate bucket.
pub fn gate(profile_id: Option<i64>, _action: Action, origin: Option<&str>, field: Option<&str>) -> Result<(), String> {
    let bucket = origin.or(field).unwrap_or("<none>");
    if !within_rate(bucket) {
        record(profile_id, Action::RateLimited, origin, field, false, "rate limit exceeded for this origin/bucket");
        return Err("Too many vault accesses in a short window. Slow down and try again.".into());
    }
    note_hit(bucket);
    Ok(())
}

/// Record an access that completed successfully at the gate.
pub fn allow(profile_id: Option<i64>, action: Action, origin: Option<&str>, field: Option<&str>, reason: &str) {
    record(profile_id, action, origin, field, true, reason);
}

/// Record a denied access that did not pass the gate's preconditions (locked
/// vault, no match, origin mismatch). Always fail-closed at the call site.
pub fn deny(profile_id: Option<i64>, action: Action, origin: Option<&str>, field: Option<&str>, reason: &str) {
    record(profile_id, action, origin, field, false, reason);
}

/// One row of the audit log for the Security panel.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AuditRow {
    pub ts: String,
    pub profile_id: Option<i64>,
    pub action: String,
    pub origin: Option<String>,
    pub field: Option<String>,
    pub allowed: bool,
    pub reason: Option<String>,
}

/// Most recent audit rows (newest first), capped at `limit`. Read-only; the table
/// is append-only by construction (no UPDATE/DELETE path exists in this module).
pub fn recent(limit: i64) -> Result<Vec<AuditRow>, String> {
    let conn = db().ok_or("audit log not initialized")?;
    let mut stmt = conn
        .prepare(
            "SELECT ts, profile_id, action, origin, field, allowed, reason
             FROM vault_audit_log ORDER BY id DESC LIMIT ?1",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![limit], |r| {
            Ok(AuditRow {
                ts: r.get(0)?,
                profile_id: r.get(1)?,
                action: r.get(2)?,
                origin: r.get(3)?,
                field: r.get(4)?,
                allowed: r.get::<_, i64>(5)? != 0,
                reason: r.get(6)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // The per-origin rate limit caps a burst: an automated mass-dump is throttled
    // and leaves a visible RateLimited trail, turning "instant total loss" into
    // "slow, capped, detectable".
    #[test]
    fn gate_rate_limits_a_burst_per_bucket() {
        // Unique bucket so this test doesn't interact with others' windows.
        let origin = "https://rate-test.example";
        let mut ok = 0;
        let mut denied = 0;
        for _ in 0..(MAX_PER_WINDOW + 5) {
            match gate(Some(1), Action::Reveal, Some(origin), Some("entry:1")) {
                Ok(()) => ok += 1,
                Err(_) => denied += 1,
            }
        }
        assert_eq!(ok, MAX_PER_WINDOW, "exactly MAX_PER_WINDOW allowed before the cap");
        assert_eq!(denied, 5, "the rest are rate-limited (fail closed)");
    }

    // A different bucket has its own independent budget.
    #[test]
    fn gate_buckets_are_independent() {
        let a = "https://bucket-a.example";
        let b = "https://bucket-b.example";
        for _ in 0..MAX_PER_WINDOW {
            assert!(gate(Some(1), Action::Reveal, Some(a), None).is_ok());
        }
        assert!(gate(Some(1), Action::Reveal, Some(a), None).is_err());
        // b is untouched.
        assert!(gate(Some(1), Action::Reveal, Some(b), None).is_ok());
    }
}
