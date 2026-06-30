//! In-process secret hygiene [HYGIENE] — shrink the window and blast radius for
//! the backend process that legitimately holds plaintext (the vault).
//!
//! HONEST SCOPE: none of this is a boundary. Phase 1's process separation is what
//! keeps secrets away from web content; this layer only reduces the chance a
//! secret leaks to a SECONDARY medium (swap, a core dump) or lingers in freed
//! memory. A code-execution / memory-disclosure bug *inside this process* can
//! still read these secrets while they're live — `mlock`/`DONTDUMP`/`zeroize` do
//! not change that. We say so plainly rather than overselling it.
//!
//! What it buys:
//!  - `zeroize`: decrypted buffers are wiped on drop, not left on the heap.
//!  - `mlock` + `MADV_DONTDUMP`: secret pages stay out of swap and core dumps.
//!  - `harden_process`: the whole process refuses to produce a core dump, so a
//!    crash can't spill plaintext to disk.
//!  - `ct_eq`: constant-time comparison for secrets/MACs (no timing oracle).

use subtle::ConstantTimeEq;
use zeroize::Zeroize;

/// Constant-time byte-equality for secrets/MACs. Use instead of `==` whenever one
/// side is attacker-influenced and the other is secret (passwords, hashes).
pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    // Different lengths can't be equal; `ct_eq` already handles this, but the
    // length check itself is not secret (lengths are not the secret material).
    a.ct_eq(b).into()
}

/// Best-effort: keep `[ptr, ptr+len)` out of swap (`mlock`) and out of core dumps
/// (`madvise(MADV_DONTDUMP)`). Returns whether the lock succeeded (so the caller
/// can unlock on drop). Silent no-op on non-Linux. Never panics.
#[cfg(target_os = "linux")]
fn lock_region(ptr: *const u8, len: usize) -> bool {
    if len == 0 {
        return false;
    }
    unsafe {
        // MADV_DONTDUMP keeps the region out of core dumps even if a crash slips
        // past `harden_process` (e.g. an external dumper). Best-effort.
        libc::madvise(ptr as *mut libc::c_void, len, libc::MADV_DONTDUMP);
        libc::mlock(ptr as *const libc::c_void, len) == 0
    }
}

#[cfg(target_os = "linux")]
fn unlock_region(ptr: *const u8, len: usize) {
    if len == 0 {
        return;
    }
    unsafe {
        libc::munlock(ptr as *const libc::c_void, len);
    }
}

#[cfg(not(target_os = "linux"))]
fn lock_region(_ptr: *const u8, _len: usize) -> bool {
    false
}
#[cfg(not(target_os = "linux"))]
fn unlock_region(_ptr: *const u8, _len: usize) {}

/// A plaintext secret held in a heap buffer that is zeroized on drop and, on
/// Linux, best-effort `mlock`'d + `MADV_DONTDUMP`. Use for long-lived secrets the
/// backend must keep (e.g. the cached master password while the vault is
/// unlocked). Construction copies the secret once into the protected buffer.
pub struct SecretString {
    buf: Vec<u8>,
    locked: bool,
}

impl SecretString {
    pub fn new(s: &str) -> Self {
        let mut buf = s.as_bytes().to_vec();
        // Match capacity to length so the locked region is exact and no secret
        // bytes hide in spare capacity (which `zeroize` of the Vec also clears).
        buf.shrink_to_fit();
        let locked = lock_region(buf.as_ptr(), buf.len());
        Self { buf, locked }
    }

    /// Borrow as `&str`. The returned reference must not outlive this guard.
    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.buf).unwrap_or("")
    }
}

impl Drop for SecretString {
    fn drop(&mut self) {
        if self.locked {
            unlock_region(self.buf.as_ptr(), self.buf.len());
        }
        self.buf.zeroize();
    }
}

/// Process-level secret hygiene, called once at startup on the backend (the
/// process that decrypts the vault):
///  - `PR_SET_DUMPABLE = 0`: this process produces no core dump and is not
///    ptrace-attachable by non-root, so a crash can't spill plaintext to disk.
///  - `RLIMIT_CORE = 0`: belt-and-braces against any core file.
///
/// [HYGIENE] This protects against accidental disk spillage, NOT against a
/// privileged local attacker (root can re-enable dumping / read /proc/<pid>/mem).
/// Honest by design; see module docs.
#[cfg(target_os = "linux")]
pub fn harden_process() {
    unsafe {
        // No core dumps + not dumpable.
        libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0);
        let zero = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
        libc::setrlimit(libc::RLIMIT_CORE, &zero);
    }
}

#[cfg(not(target_os = "linux"))]
pub fn harden_process() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ct_eq_matches_semantics_of_equality() {
        assert!(ct_eq(b"hunter2", b"hunter2"));
        assert!(!ct_eq(b"hunter2", b"hunter3"));
        assert!(!ct_eq(b"short", b"longer"));
        assert!(ct_eq(b"", b""));
    }

    #[test]
    fn secret_string_roundtrips_and_drops() {
        let s = SecretString::new("correct horse battery staple");
        assert_eq!(s.as_str(), "correct horse battery staple");
        drop(s); // must not panic (munlock + zeroize)
    }
}
