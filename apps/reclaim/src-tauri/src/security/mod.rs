//! Security subsystem — the deterministic spine the Security panel is built on.
//!
//! This module owns the *honest* security signals Reclaim can actually produce:
//! the vault-access audit log (Phase 1) and, later, the runtime monitor (Phase 6)
//! that also watches sandbox/allocator/crash signals. Everything here is
//! deterministic and kernel-/boundary-grounded — no LLM, no guessing. The AI
//! curator (Phase 7) sits strictly DOWNSTREAM of this and can never feed back
//! into a security decision.
//!
//! Priority tags are carried on every event and surfaced verbatim in the panel so
//! we never oversell a tripwire. See [`PriorityTag`].

pub mod allocator;
pub mod audit;
pub mod curator;
pub mod integrity;
pub mod monitor;
pub mod sandbox;
pub mod secrets;

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};

/// How much a real security boundary an event represents. Mirrors the tags in the
/// hardening brief so the UI can show the honest strength of each signal. NEVER
/// relabel a tripwire (`DefenseInDepth`) as a wall.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PriorityTag {
    /// OS/MMU/kernel-enforced boundary (process isolation, seccomp, Landlock,
    /// the origin-bound vault gate). Highest value.
    Boundary,
    /// Raises exploitation cost meaningfully; not an absolute wall.
    Hardening,
    /// Reduces leak surface (swap, dumps, post-free zeroing). Cheap, worthwhile.
    Hygiene,
    /// Catches accidents / unsophisticated tampering; a privileged local attacker
    /// can bypass it. Worth it for layering, never sold as anti-root.
    DefenseInDepth,
    /// A trustworthy observed event from a real boundary (a denial actually
    /// happened at the gate). Not a posture claim — a fact.
    RealSignal,
}

/// Severity for ordering/highlighting in the feed. Independent of [`PriorityTag`]:
/// a `DefenseInDepth` event can still be `Critical` (e.g. an integrity mismatch).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Severity {
    Info,
    Notice,
    Warning,
    Critical,
}

/// A single security signal. The unified currency between the deterministic
/// producers (vault gate, future monitor) and the panel. Attacker-influenced
/// fields (`origin`, `detail`) are DATA — never instructions; the Phase 7 curator
/// must treat them as inert (see Phase 7 contract).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SecurityEvent {
    /// RFC3339 UTC timestamp.
    pub ts: String,
    /// Coarse source: "vault", "sandbox", "allocator", "integrity", "crash".
    pub category: String,
    pub tag: PriorityTag,
    pub severity: Severity,
    /// Short, fixed, non-attacker-controlled label (safe to render as a heading).
    pub title: String,
    /// Free-form context. MAY contain attacker-controlled substrings (an origin a
    /// page supplied, a path an exploit tried). Render as plain text; never eval.
    pub detail: String,
    /// Origin involved, if any (attacker-influenced — treat as data).
    pub origin: Option<String>,
    /// "allowed" | "denied" for gated decisions; None for pure observations.
    pub decision: Option<String>,
}

/// App handle used to push live events to the panel. Set once at startup. We keep
/// it here (rather than threading it everywhere) so deep, sync code paths like the
/// vault gate can emit without plumbing an `AppHandle` through every call.
static APP: OnceLock<tauri::AppHandle> = OnceLock::new();

/// Recent events ring buffer — the deterministic feed the Security panel reads on
/// open (live events also arrive via the `security-event` Tauri event, but the
/// panel may mount after events fired, so we keep a short history here too).
static RING: OnceLock<Mutex<VecDeque<SecurityEvent>>> = OnceLock::new();
const RING_CAP: usize = 500;

fn ring() -> &'static Mutex<VecDeque<SecurityEvent>> {
    RING.get_or_init(|| Mutex::new(VecDeque::with_capacity(RING_CAP)))
}

/// Most recent events (newest first), capped at `limit`. Combined source of truth
/// for the panel feed across all producers (vault gate, sandbox, crashes, integrity).
pub fn recent_events(limit: usize) -> Vec<SecurityEvent> {
    let r = ring().lock().unwrap_or_else(|e| e.into_inner());
    r.iter().rev().take(limit).cloned().collect()
}

/// Wire the security subsystem at startup: remember the app handle (for live
/// events) and initialize the persistent audit store.
pub fn init(app: tauri::AppHandle, db_path: &str) {
    let _ = APP.set(app);
    if let Err(e) = audit::init(db_path) {
        eprintln!("[security] audit log init failed: {e}");
    }
    // Isolated store for the advisory AI curator (separate table from the browsing
    // knowledge graph — security analysis must never leak into ordinary answers).
    if let Err(e) = curator::init(db_path) {
        eprintln!("[security] curator store init failed: {e}");
    }
    // In-process secret hygiene for the backend that holds the decrypted vault:
    // no core dumps so a crash can't spill plaintext to disk. [HYGIENE]
    secrets::harden_process();
}

/// Emit a live event to the frontend Security panel. Best-effort: a missing app
/// handle (e.g. in unit tests) is a silent no-op — persistence is the source of
/// truth, the live event is only for immediacy.
pub fn emit(ev: &SecurityEvent) {
    // Keep a bounded history for the panel's on-open fetch.
    if let Ok(mut r) = ring().lock() {
        if r.len() >= RING_CAP {
            r.pop_front();
        }
        r.push_back(ev.clone());
    }
    if let Some(app) = APP.get() {
        use tauri::Emitter;
        let _ = app.emit("security-event", ev);
    }
}

/// Construct + emit an event in one call (convenience for producers).
pub fn report(category: &str, tag: PriorityTag, severity: Severity, title: &str, detail: &str) {
    emit(&SecurityEvent {
        ts: now_rfc3339(),
        category: category.to_string(),
        tag,
        severity,
        title: title.to_string(),
        detail: detail.to_string(),
        origin: None,
        decision: None,
    });
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

// ============================================================================
// Tauri commands — deterministic security data for the panel (no LLM).
// ============================================================================

/// Recent vault-access audit rows (newest first). Read-only view of the
/// append-only log; the source of truth for "what tried to touch the vault".
#[tauri::command(rename_all = "camelCase")]
pub async fn security_audit_recent(limit: Option<i64>) -> Result<Vec<audit::AuditRow>, String> {
    audit::recent(limit.unwrap_or(200).clamp(1, 1000))
}
