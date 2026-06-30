//! Allocator hardening [HARDENING] — preload GrapheneOS hardened_malloc for the
//! Reclaim process tree.
//!
//! hardened_malloc adds guard pages, allocation canaries, zero-on-free with
//! write-after-free detection, and a use-after-free quarantine. We enable it via
//! `LD_PRELOAD` and a one-shot self-re-exec at startup, so the host AND every
//! child it spawns (yt-dlp, Servo, …) inherit it.
//!
//! HONEST CAVEATS (kept truthful on purpose):
//!  - WebKitGTK's web-content process uses its OWN internal allocators (bmalloc /
//!    IsoHeaps / Gigacage), so this mainly hardens the Rust host and helper
//!    processes — NOT JavaScriptCore's heap. Still worthwhile, just not total.
//!  - It is opt-out-able (`RECLAIM_HARDENED_MALLOC=0`) because a preloaded
//!    allocator can occasionally conflict with a system library.
//!  - hardened_malloc creates many guard-page mappings; the host may need a higher
//!    `vm.max_map_count` (see `resources/hardened-malloc/99-reclaim-hardened-malloc.conf`).
//!
//! The `.so` is not bundled by default (it must be built for the target — see
//! `scripts/build-hardened-malloc.sh`). When absent this is a silent no-op, so a
//! plain build/run is unaffected; building the library opts you in.

/// Env guard set on the child after we set LD_PRELOAD, so the re-exec'd process
/// doesn't try to preload again (which would loop).
const ACTIVE_GUARD: &str = "RECLAIM_HMALLOC_ACTIVE";
/// Set to "0"/"off"/"false" to disable preloading entirely.
const DISABLE_ENV: &str = "RECLAIM_HARDENED_MALLOC";
/// Explicit path to the hardened_malloc `.so` (overrides discovery).
const PATH_ENV: &str = "RECLAIM_HMALLOC_PATH";
/// "light" or "standard" (default). Light trades some checks for speed.
const VARIANT_ENV: &str = "RECLAIM_HMALLOC_VARIANT";

/// Whether hardened_malloc is currently active in THIS process (i.e. we already
/// re-exec'd with it preloaded). Used by the Security panel's posture header.
pub fn is_active() -> bool {
    std::env::var(ACTIVE_GUARD).as_deref() == Ok("1")
}

/// Whether preloading is disabled by the user.
fn is_disabled() -> bool {
    matches!(std::env::var(DISABLE_ENV).as_deref(), Ok("0") | Ok("off") | Ok("false"))
}

/// Locate the hardened_malloc `.so`: explicit env override first, else common
/// locations relative to the executable (next to it, ../lib, resources/). Returns
/// the first existing candidate.
#[cfg(target_os = "linux")]
fn find_library() -> Option<std::path::PathBuf> {
    use std::path::PathBuf;
    if let Ok(p) = std::env::var(PATH_ENV) {
        let pb = PathBuf::from(p);
        return pb.exists().then_some(pb);
    }
    let light = matches!(std::env::var(VARIANT_ENV).as_deref(), Ok("light"));
    let names: &[&str] = if light {
        &["libhardened_malloc-light.so", "libhardened_malloc.so"]
    } else {
        &["libhardened_malloc.so", "libhardened_malloc-light.so"]
    };
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?.to_path_buf();
    let bases = [
        dir.clone(),
        dir.join("hardened-malloc"),
        dir.join("../lib/reclaim"),
        dir.join("../lib"),
        dir.join("resources/hardened-malloc"),
        // Tauri bundles `resources/` next to the binary in packaged builds.
        dir.join("../share/reclaim/hardened-malloc"),
    ];
    for base in bases {
        for name in names {
            let cand = base.join(name);
            if cand.exists() {
                return Some(cand);
            }
        }
    }
    None
}

/// Preload hardened_malloc by setting `LD_PRELOAD` and re-exec'ing ourselves, once,
/// before any heavy initialization. No-op if already active, disabled, the library
/// is absent, or not on Linux. On a successful re-exec this function does not
/// return (the process image is replaced).
#[cfg(target_os = "linux")]
pub fn maybe_preload() {
    use std::os::unix::process::CommandExt;

    if is_active() || is_disabled() {
        return;
    }
    let lib = match find_library() {
        Some(l) => l,
        None => return, // not built/bundled — silent no-op, plain run unaffected
    };
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(_) => return,
    };
    // Prepend to any existing LD_PRELOAD rather than clobbering it.
    let preload = match std::env::var("LD_PRELOAD") {
        Ok(prev) if !prev.is_empty() => format!("{}:{}", lib.display(), prev),
        _ => lib.display().to_string(),
    };
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    eprintln!("[security] preloading hardened_malloc: {}", lib.display());
    let err = std::process::Command::new(exe)
        .args(&args)
        .env("LD_PRELOAD", preload)
        .env(ACTIVE_GUARD, "1")
        .exec(); // replaces this process on success
    // Only reached if exec failed — continue WITHOUT hardened_malloc rather than die.
    eprintln!("[security] hardened_malloc preload re-exec failed ({err}); continuing without it");
}

#[cfg(not(target_os = "linux"))]
pub fn maybe_preload() {}
