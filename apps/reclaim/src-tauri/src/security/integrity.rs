//! Startup integrity check [DEFENSE-IN-DEPTH] — verify Reclaim's own binary and
//! bundled assets against a hashed manifest produced at build time.
//!
//! HONEST SCOPE: this catches corruption and UNSOPHISTICATED tampering (a flipped
//! bit, a swapped resource), and is useful for compliance. It is NOT anti-tamper
//! against a privileged local attacker: anyone who can edit the binary can also
//! edit the manifest. We surface it as DEFENSE-IN-DEPTH and never imply otherwise.
//!
//! The manifest (`integrity-manifest.json`) is generated post-build by
//! `scripts/gen-integrity-manifest.sh` and bundled as a resource. If it's absent,
//! the status is "not configured" (honest) rather than a false "OK".

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::{PriorityTag, SecurityEvent, Severity};

#[derive(Debug, Clone, serde::Serialize)]
pub struct IntegrityStatus {
    /// Whether a manifest was found at all.
    pub configured: bool,
    /// True if every listed file matched (or no manifest, treated as unknown=false).
    pub ok: bool,
    /// Number of files checked.
    pub checked: usize,
    /// Human-readable mismatches (path: reason). Never contains secrets.
    pub mismatches: Vec<String>,
}

#[derive(serde::Deserialize)]
struct Manifest {
    files: Vec<ManifestEntry>,
}

#[derive(serde::Deserialize)]
struct ManifestEntry {
    /// "self" for the running executable, otherwise a path relative to the
    /// resource directory (or absolute).
    path: String,
    sha256: String,
}

fn sha256_file(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    let mut h = Sha256::new();
    h.update(&bytes);
    Some(format!("{:x}", h.finalize()))
}

fn manifest_path(resource_dir: &Path) -> Option<PathBuf> {
    if let Ok(p) = std::env::var("RECLAIM_INTEGRITY_MANIFEST") {
        let pb = PathBuf::from(p);
        return pb.exists().then_some(pb);
    }
    let pb = resource_dir.join("integrity-manifest.json");
    pb.exists().then_some(pb)
}

/// Run the integrity check against the manifest in `resource_dir`. Resolves "self"
/// to the running executable. Returns a status; never panics.
pub fn check(resource_dir: &Path) -> IntegrityStatus {
    let mpath = match manifest_path(resource_dir) {
        Some(p) => p,
        None => {
            return IntegrityStatus { configured: false, ok: false, checked: 0, mismatches: vec![] }
        }
    };
    let manifest: Manifest = match std::fs::read_to_string(&mpath).ok().and_then(|s| serde_json::from_str(&s).ok()) {
        Some(m) => m,
        None => {
            return IntegrityStatus {
                configured: true,
                ok: false,
                checked: 0,
                mismatches: vec!["manifest unreadable or malformed".into()],
            }
        }
    };

    let exe = std::env::current_exe().ok();
    let mut mismatches = Vec::new();
    let mut checked = 0usize;
    for entry in &manifest.files {
        let target = if entry.path == "self" {
            match &exe {
                Some(e) => e.clone(),
                None => {
                    mismatches.push("self: could not resolve executable path".into());
                    continue;
                }
            }
        } else {
            let p = PathBuf::from(&entry.path);
            if p.is_absolute() { p } else { resource_dir.join(&entry.path) }
        };
        checked += 1;
        match sha256_file(&target) {
            Some(actual) if actual.eq_ignore_ascii_case(&entry.sha256) => {}
            Some(_) => mismatches.push(format!("{}: hash mismatch", entry.path)),
            None => mismatches.push(format!("{}: missing or unreadable", entry.path)),
        }
    }

    IntegrityStatus { configured: true, ok: mismatches.is_empty(), checked, mismatches }
}

/// Run the check and emit a security event describing the result. Called once at
/// startup (off the UI thread).
pub fn check_and_report(resource_dir: &Path) -> IntegrityStatus {
    let status = check(resource_dir);
    let (severity, title, detail) = if !status.configured {
        (
            Severity::Info,
            "Integrity check not configured",
            "No integrity-manifest.json bundled; skipping self-verification.".to_string(),
        )
    } else if status.ok {
        (
            Severity::Info,
            "Integrity verified",
            format!("{} bundled file(s) matched the build manifest.", status.checked),
        )
    } else {
        (
            Severity::Critical,
            "Integrity check FAILED",
            format!("{} mismatch(es): {}", status.mismatches.len(), status.mismatches.join("; ")),
        )
    };
    super::emit(&SecurityEvent {
        ts: super::now_rfc3339(),
        category: "integrity".into(),
        // Honest tag: a tripwire a privileged local attacker can bypass.
        tag: PriorityTag::DefenseInDepth,
        severity,
        title: title.into(),
        detail,
        origin: None,
        decision: None,
    });
    status
}
