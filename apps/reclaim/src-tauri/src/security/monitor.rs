//! Runtime monitor [REAL SIGNAL] — the deterministic posture + event surface the
//! Security panel is built on. No LLM, no signature scanning, no "is this process
//! evil" guessing. It reports only things that actually happened at a real
//! boundary (a vault denial, a helper that was contained/crashed, an integrity
//! mismatch) and the honest on/off state of each protection.
//!
//! The AI curator (Phase 7) sits strictly downstream of this and can never change
//! a posture value or suppress an event.

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use super::{PriorityTag, Severity};

/// Resource dir, stored at init so on-demand integrity re-checks can find the
/// manifest without re-plumbing the app handle.
static RESOURCE_DIR: OnceLock<PathBuf> = OnceLock::new();
/// Last integrity result, as a short status word for the posture header.
static INTEGRITY: OnceLock<Mutex<String>> = OnceLock::new();

fn integrity_slot() -> &'static Mutex<String> {
    INTEGRITY.get_or_init(|| Mutex::new("unknown".to_string()))
}

/// Called once at startup with the resource dir; kicks off the integrity check on
/// a background thread (hashing the binary can take a beat) and records the result.
pub fn init(resource_dir: PathBuf) {
    let _ = RESOURCE_DIR.set(resource_dir.clone());
    std::thread::spawn(move || {
        let status = super::integrity::check_and_report(&resource_dir);
        let word = if !status.configured {
            "not-configured"
        } else if status.ok {
            "verified"
        } else {
            "failed"
        };
        if let Ok(mut g) = integrity_slot().lock() {
            *g = word.to_string();
        }
    });
}

/// Whether the WebKitGTK renderer sandbox is on (mirrors the gate in
/// browser_surface; off only if explicitly disabled for debugging).
fn webkit_sandbox_enabled() -> bool {
    std::env::var("RECLAIM_WEBKIT_SANDBOX").as_deref() != Ok("0")
}

/// The deterministic posture snapshot for the panel header. Every field is a real,
/// checkable fact — not an LLM opinion.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Posture {
    /// WebKitGTK bubblewrap+seccomp renderer sandbox active. [BOUNDARY]
    pub webkit_sandbox: bool,
    /// Helper subprocesses (yt-dlp) confined with no_new_privs+Landlock+seccomp. [BOUNDARY]
    pub helper_confinement: bool,
    /// hardened_malloc preloaded into the process tree. [HARDENING]
    pub hardened_malloc: bool,
    /// Compile-time hardening (RELRO/PIE/NX) baked into this build. [HARDENING]
    /// Always true for our release pipeline; the CI `hardening-flags` job verifies it.
    pub compiled_hardening: bool,
    /// Integrity self-check: "verified" | "failed" | "not-configured" | "unknown". [DEFENSE-IN-DEPTH]
    pub integrity: String,
}

/// Build the current posture. Cheap; safe to call on every panel open.
pub fn posture() -> Posture {
    Posture {
        webkit_sandbox: webkit_sandbox_enabled(),
        helper_confinement: super::sandbox::enabled(),
        hardened_malloc: super::allocator::is_active(),
        compiled_hardening: true,
        integrity: integrity_slot().lock().map(|g| g.clone()).unwrap_or_else(|_| "unknown".into()),
    }
}

/// Record the outcome of a confined helper subprocess. If it exited abnormally
/// (killed by a signal — e.g. SIGSYS from seccomp, SIGABRT from hardened_malloc,
/// SIGSEGV from a crash) we surface it as "a component was contained/killed". A
/// clean exit is not reported (no noise).
#[cfg(target_os = "linux")]
pub fn note_helper_exit(label: &str, status: &std::process::ExitStatus) {
    use std::os::unix::process::ExitStatusExt;
    if let Some(sig) = status.signal() {
        let (name, why) = match sig {
            libc::SIGSYS => ("SIGSYS", "blocked syscall (seccomp) — the helper tried something outside its allowlist"),
            libc::SIGABRT => ("SIGABRT", "abort — possibly a hardened_malloc heap-corruption catch"),
            libc::SIGSEGV => ("SIGSEGV", "segfault — the helper crashed"),
            _ => ("signal", "the helper was killed"),
        };
        super::report(
            "crash",
            PriorityTag::RealSignal,
            Severity::Warning,
            &format!("Helper '{label}' contained/killed ({name})"),
            why,
        );
    }
}

#[cfg(not(target_os = "linux"))]
pub fn note_helper_exit(_label: &str, _status: &std::process::ExitStatus) {}

// ============================================================================
// Tauri commands (deterministic; no LLM).
// ============================================================================

/// Current security posture for the panel header.
#[tauri::command(rename_all = "camelCase")]
pub async fn security_posture() -> Result<Posture, String> {
    Ok(posture())
}

/// Recent security events (newest first) — the panel's live feed source. Spans all
/// producers: vault gate, sandbox/crash, integrity.
#[tauri::command(rename_all = "camelCase")]
pub async fn security_events(limit: Option<usize>) -> Result<Vec<super::SecurityEvent>, String> {
    Ok(super::recent_events(limit.unwrap_or(200).clamp(1, 500)))
}

/// Re-run the integrity check on demand (e.g. a "re-verify" button).
#[tauri::command(rename_all = "camelCase")]
pub async fn security_run_integrity() -> Result<super::integrity::IntegrityStatus, String> {
    let dir = RESOURCE_DIR.get().ok_or("resource dir not set")?.clone();
    let status = super::integrity::check_and_report(&dir);
    let word = if !status.configured { "not-configured" } else if status.ok { "verified" } else { "failed" };
    if let Ok(mut g) = integrity_slot().lock() {
        *g = word.to_string();
    }
    Ok(status)
}
