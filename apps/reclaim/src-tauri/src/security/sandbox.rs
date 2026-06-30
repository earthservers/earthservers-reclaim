//! Helper-process confinement [BOUNDARY/HARDENING] — drop privileges and restrict
//! what a spawned helper (yt-dlp, …) can touch, applied in the child's `pre_exec`
//! BEFORE it runs the foreign binary.
//!
//! Three layers, each fail-closed where it can be without breaking the helper:
//!  - `PR_SET_NO_NEW_PRIVS` [BOUNDARY]: the helper (and anything it execs) can
//!    never gain privileges (no setuid escalation). Cheap, always applied.
//!  - Landlock [BOUNDARY]: a real kernel filesystem boundary. We allow read+exec
//!    broadly (the helper is a real program that reads libs/config) but confine
//!    WRITES to exactly the directories it needs (e.g. the downloads dir + tmp).
//!    So an exploited yt-dlp cannot write outside Downloads/Reclaim.
//!  - seccomp-bpf [HARDENING]: deny a set of clearly-unnecessary, dangerous
//!    syscalls (ptrace, process_vm_readv/writev, …) so the helper can't inspect or
//!    tamper with other processes' memory. We use a deny-list (default allow)
//!    rather than a strict whitelist on purpose: a tight whitelist around a Python
//!    helper like yt-dlp is impractical and would break it. We say so honestly —
//!    this layer raises cost, it is not an absolute syscall wall.
//!
//! Everything is best-effort: on a kernel without Landlock/seccomp, or when
//! disabled via `RECLAIM_SANDBOX_HELPERS=0`, the helper still runs (unconfined),
//! and we record that fact rather than silently implying confinement.
//!
//! Denials (seccomp ERRNO, Landlock EACCES) surface to the user via the Phase 6
//! monitor as "a component was contained."

#[cfg(target_os = "linux")]
use std::path::PathBuf;

/// What a helper is allowed to do. Read+execute is broad (real programs need it);
/// writes and the dangerous-syscall deny-list are the teeth.
#[derive(Debug, Clone)]
pub struct HelperProfile {
    /// Directories the helper may WRITE to (read+write+create). Everything else is
    /// read+execute only.
    pub writable_dirs: Vec<String>,
    /// Human label for logs/monitor (e.g. "yt-dlp").
    pub label: String,
}

impl HelperProfile {
    /// Profile for the media downloader: may only write into its downloads dir
    /// (and the system temp dir, which yt-dlp uses for partial files).
    pub fn downloader(downloads_dir: &str) -> Self {
        let mut writable = vec![downloads_dir.to_string()];
        writable.push(std::env::temp_dir().to_string_lossy().to_string());
        Self { writable_dirs: writable, label: "yt-dlp".into() }
    }
}

/// Whether helper confinement is enabled (default on; `RECLAIM_SANDBOX_HELPERS=0`
/// turns it off).
pub fn enabled() -> bool {
    !matches!(std::env::var("RECLAIM_SANDBOX_HELPERS").as_deref(), Ok("0") | Ok("off") | Ok("false"))
}

/// Attach confinement to `cmd` so the spawned helper runs with no_new_privs +
/// Landlock + seccomp applied in its `pre_exec` (before the foreign binary runs).
/// The allocation-heavy setup happens NOW in the parent; the child only applies it.
/// No-op when disabled. Best-effort layers degrade on unsupporting kernels.
#[cfg(target_os = "linux")]
pub fn confine_command(cmd: &mut std::process::Command, profile: &HelperProfile) {
    use std::os::unix::process::CommandExt;
    if !enabled() {
        eprintln!("[security] helper confinement disabled (RECLAIM_SANDBOX_HELPERS=0): {}", profile.label);
        return;
    }
    let mut conf = Confinement::prepare(profile);
    // SAFETY: the closure runs post-fork/pre-exec. It only invokes prctl/landlock/
    // seccomp syscalls on data prepared in the parent — no allocation, no shared
    // locks — which is safe in that context.
    unsafe {
        cmd.pre_exec(move || conf.apply());
    }
}

#[cfg(not(target_os = "linux"))]
pub fn confine_command(_cmd: &mut std::process::Command, _profile: &HelperProfile) {}

/// Syscalls we deny for helpers: inspecting/tampering with other processes, kernel
/// module / kexec, and other clearly-unnecessary capabilities for a downloader.
/// A denied call returns EPERM (the helper sees a normal error, isn't killed).
#[cfg(target_os = "linux")]
const DENIED_SYSCALLS: &[libc::c_long] = &[
    libc::SYS_ptrace,
    libc::SYS_process_vm_readv,
    libc::SYS_process_vm_writev,
    libc::SYS_kcmp,
    libc::SYS_perf_event_open,
    libc::SYS_init_module,
    libc::SYS_finit_module,
    libc::SYS_delete_module,
    libc::SYS_kexec_load,
    libc::SYS_mount,
    libc::SYS_umount2,
    libc::SYS_ptrace, // (dup harmless)
];

/// Apply `PR_SET_NO_NEW_PRIVS`. Returns Ok on success. Async-signal-safe (single
/// prctl), so callable from `pre_exec`.
#[cfg(target_os = "linux")]
fn set_no_new_privs() -> std::io::Result<()> {
    let rc = unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) };
    if rc != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// Build the Landlock ruleset in the PARENT (opening the path fds), to be applied
/// in the child. Best-effort: degrades on kernels with partial/no Landlock support
/// instead of erroring. Returns None if Landlock can't be set up at all.
#[cfg(target_os = "linux")]
fn build_landlock(profile: &HelperProfile) -> Option<landlock::RulesetCreated> {
    use landlock::{
        Access, AccessFs, CompatLevel, Compatible, PathBeneath, PathFd, Ruleset, RulesetAttr,
        RulesetCreatedAttr, ABI,
    };
    let abi = ABI::V1;
    let read_exec = AccessFs::from_read(abi) | AccessFs::Execute;
    let all = AccessFs::from_all(abi);

    let mut ruleset = Ruleset::default()
        .set_compatibility(CompatLevel::BestEffort)
        .handle_access(all)
        .ok()?
        .create()
        .ok()?;

    // Broad read+execute on the filesystem root (libs, interpreters, certs, config).
    if let Ok(fd) = PathFd::new("/") {
        ruleset = ruleset.add_rule(PathBeneath::new(fd, read_exec)).ok()?;
    }
    // Full access only on the writable dirs.
    for dir in &profile.writable_dirs {
        let _ = std::fs::create_dir_all(dir);
        if let Ok(fd) = PathFd::new(PathBuf::from(dir)) {
            ruleset = ruleset.add_rule(PathBeneath::new(fd, all)).ok()?;
        }
    }
    Some(ruleset)
}

/// Compile the seccomp deny-list filter in the PARENT (allocates), to be loaded in
/// the child. Returns None if it can't be built.
#[cfg(target_os = "linux")]
fn build_seccomp() -> Option<seccompiler::BpfProgram> {
    use seccompiler::{SeccompAction, SeccompFilter};
    use std::collections::BTreeMap;

    let mut rules: BTreeMap<i64, Vec<seccompiler::SeccompRule>> = BTreeMap::new();
    for &sc in DENIED_SYSCALLS {
        rules.insert(sc, vec![]); // empty rule vec = unconditional match (c_long == i64 here)
    }
    let filter = SeccompFilter::new(
        rules,
        SeccompAction::Allow,                 // default: allow
        SeccompAction::Errno(libc::EPERM as u32), // matched (denied) → EPERM
        std::env::consts::ARCH.try_into().ok()?,
    )
    .ok()?;
    filter.try_into().ok()
}

/// A prepared confinement: parent-built Landlock ruleset + seccomp program, ready
/// to apply once in the child's `pre_exec`.
#[cfg(target_os = "linux")]
pub struct Confinement {
    landlock: Option<landlock::RulesetCreated>,
    seccomp: Option<seccompiler::BpfProgram>,
}

#[cfg(target_os = "linux")]
impl Confinement {
    /// Prepare confinement for `profile` (does the allocation-heavy work now, in
    /// the parent, so the child's `pre_exec` stays minimal).
    pub fn prepare(profile: &HelperProfile) -> Self {
        Confinement {
            landlock: build_landlock(profile),
            seccomp: build_seccomp(),
        }
    }

    /// Apply in the child after fork, before exec. Order matters: no_new_privs
    /// first (required for seccomp without CAP_SYS_ADMIN and to make Landlock
    /// stick), then Landlock, then seccomp. Returns Err to abort the exec if a
    /// layer that WAS set up fails to apply (fail closed); a layer that isn't
    /// available is skipped (best-effort).
    pub fn apply(&mut self) -> std::io::Result<()> {
        set_no_new_privs()?;
        if let Some(rs) = self.landlock.take() {
            // restrict_self consumes the ruleset; ignore the (best-effort) status
            // detail but fail closed if the syscall itself errors.
            rs.restrict_self()
                .map_err(|e| std::io::Error::other(format!("landlock: {e}")))?;
        }
        if let Some(prog) = &self.seccomp {
            seccompiler::apply_filter(prog)
                .map_err(|e| std::io::Error::other(format!("seccomp: {e}")))?;
        }
        Ok(())
    }
}
