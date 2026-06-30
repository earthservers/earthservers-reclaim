#!/usr/bin/env bash
# Generate integrity-manifest.json (Phase 6 startup self-check). Hashes the built
# binary ("self") + bundled resources so a corrupted/tampered install is flagged.
#
# Honest scope [DEFENSE-IN-DEPTH]: a privileged attacker can edit both the file
# and this manifest. This catches corruption + unsophisticated tampering, and is
# useful for compliance — not anti-tamper against root.
#
# Run AFTER building the release binary, BEFORE bundling, e.g.:
#   cargo build --release --bin reclaim
#   scripts/gen-integrity-manifest.sh target/release/reclaim
set -euo pipefail

BIN="${1:-target/release/reclaim}"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RES="$REPO_ROOT/apps/reclaim/src-tauri/resources"
OUT="$RES/integrity-manifest.json"

[ -f "$BIN" ] || { echo "binary not found: $BIN (build it first)" >&2; exit 1; }
command -v sha256sum >/dev/null || { echo "sha256sum required" >&2; exit 1; }

hash() { sha256sum "$1" | awk '{print $1}'; }

# Collect resource files (paths are stored RELATIVE to the resource dir, matching
# how integrity.rs resolves them at runtime).
entries=()
entries+=("{\"path\":\"self\",\"sha256\":\"$(hash "$BIN")\"}")
while IFS= read -r -d '' f; do
  rel="${f#"$RES"/}"
  # Skip the manifest itself.
  [ "$rel" = "integrity-manifest.json" ] && continue
  entries+=("{\"path\":\"$rel\",\"sha256\":\"$(hash "$f")\"}")
done < <(find "$RES" -type f -not -name 'integrity-manifest.json' -print0)

{
  echo '{'
  echo '  "files": ['
  for i in "${!entries[@]}"; do
    sep=','; [ "$i" -eq $((${#entries[@]} - 1)) ] && sep=''
    echo "    ${entries[$i]}$sep"
  done
  echo '  ]'
  echo '}'
} > "$OUT"

echo "Wrote $OUT (${#entries[@]} entries)."
