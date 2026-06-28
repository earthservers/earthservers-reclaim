#!/usr/bin/env bash
# Release build for Reclaim.
#
# `tauri build` reliably produces the .deb and .rpm, but its AppImage step shells
# out to linuxdeploy (itself an AppImage) which can't FUSE-mount its sub-plugin on
# Fedora/Nobara — so it dies with an opaque "failed to run linuxdeploy". Running
# linuxdeploy directly with APPIMAGE_EXTRACT_AND_RUN=1 works, so if Tauri fails
# ONLY at that step (deb + rpm already built), we finish the AppImage ourselves.
set -uo pipefail
cd "$(dirname "$0")/.."

B=target/release/bundle
VER=$(grep -m1 '"version"' apps/reclaim/src-tauri/tauri.conf.json | grep -oE '[0-9]+\.[0-9]+\.[0-9]+')

echo ">> Building Reclaim v${VER} ..."
APPIMAGE_EXTRACT_AND_RUN=1 NO_STRIP=1 pnpm --filter reclaim tauri:build
status=$?

if [ "$status" -ne 0 ]; then
  if ls "$B"/deb/*.deb >/dev/null 2>&1 && ls "$B"/rpm/*.rpm >/dev/null 2>&1; then
    echo ">> Tauri failed only at the AppImage step (deb + rpm OK) — finishing AppImage..."
  else
    echo ">> Build failed before bundling completed — see errors above." >&2
    exit "$status"
  fi
fi

OUT="$B/appimage/Reclaim_${VER}_amd64.AppImage"
if [ ! -f "$OUT" ]; then
  APPDIR=$(find "$B/appimage" -maxdepth 1 -name '*.AppDir' 2>/dev/null | head -1)
  LD=$(find "$HOME/.cache/tauri" -iname 'linuxdeploy*.AppImage' 2>/dev/null | head -1)
  if [ -n "$APPDIR" ] && [ -n "$LD" ]; then
    APPIMAGE_EXTRACT_AND_RUN=1 NO_STRIP=1 "$LD" --appdir "$APPDIR" --output appimage \
      || { echo ">> linuxdeploy failed" >&2; exit 1; }
    mv -f Reclaim*.AppImage "$OUT"
  else
    echo ">> Could not locate the AppDir or linuxdeploy; skipping AppImage." >&2
  fi
fi

echo ""
echo "=== Bundles (v${VER}) ==="
ls -lh "$B"/deb/*.deb "$B"/rpm/*.rpm "$B"/appimage/*.AppImage 2>/dev/null
