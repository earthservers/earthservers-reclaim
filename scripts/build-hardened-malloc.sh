#!/usr/bin/env bash
# Build GrapheneOS hardened_malloc and stage it for Reclaim's allocator hardening
# (Phase 3). The resulting .so is loaded at startup via LD_PRELOAD + self-re-exec
# (see security/allocator.rs). This is OPT-IN: Reclaim runs fine without it (the
# preload is a silent no-op when the .so is absent); building it turns it on.
#
# Honest scope [HARDENING]: hardens the Rust host + helper processes (yt-dlp,
# Servo). It does NOT harden WebKitGTK's web-content heap (bmalloc/IsoHeaps/
# Gigacage), which uses its own allocators.
#
# Usage:  scripts/build-hardened-malloc.sh [standard|light|both]   (default: both)
set -euo pipefail

VARIANT="${1:-both}"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="$REPO_ROOT/apps/reclaim/src-tauri/resources/hardened-malloc"
WORK="${HMALLOC_SRC:-$REPO_ROOT/.hardened-malloc-build}"
REF="${HMALLOC_REF:-main}"

command -v git >/dev/null  || { echo "git required" >&2; exit 1; }
command -v make >/dev/null || { echo "make required" >&2; exit 1; }

mkdir -p "$DEST"

if [ ! -d "$WORK/.git" ]; then
  echo "[hardened_malloc] cloning GrapheneOS/hardened_malloc -> $WORK"
  git clone --depth 1 --branch "$REF" https://github.com/GrapheneOS/hardened_malloc "$WORK"
fi

cd "$WORK"
git fetch --depth 1 origin "$REF" || true
git checkout "$REF" || true

build_one() {
  local variant="$1" config soname out
  case "$variant" in
    standard) config="N_ARENA=1 CONFIG_NATIVE=false"; soname="libhardened_malloc.so" ;;
    light)    config="VARIANT=light";                  soname="libhardened_malloc-light.so" ;;
    *) echo "unknown variant: $variant" >&2; return 1 ;;
  esac
  echo "[hardened_malloc] building $variant"
  make clean >/dev/null 2>&1 || true
  # shellcheck disable=SC2086
  make $config
  out="$(find out* -name 'libhardened_malloc*.so' | head -1)"
  [ -n "$out" ] || { echo "build produced no .so" >&2; return 1; }
  cp "$out" "$DEST/$soname"
  echo "[hardened_malloc] staged $DEST/$soname"
}

case "$VARIANT" in
  both) build_one standard; build_one light ;;
  standard|light) build_one "$VARIANT" ;;
  *) echo "usage: $0 [standard|light|both]" >&2; exit 1 ;;
esac

echo
echo "Done. Reclaim will preload it automatically on next launch."
echo "Disable with RECLAIM_HARDENED_MALLOC=0 ; pick light with RECLAIM_HMALLOC_VARIANT=light."
echo "If you see many mmap failures, raise vm.max_map_count (see resources/hardened-malloc/99-reclaim-hardened-malloc.conf)."
