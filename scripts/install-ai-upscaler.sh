#!/usr/bin/env bash
# Install the optional AI video upscaler runtime for Earth Reclaim.
#
# The media player's Enhance button gains an "AI (Real-ESRGAN)" mode when
# ~/.earthreclaim/aisr contains: the model (shipped in this repo), the official
# onnxruntime-gpu libraries, and the NVIDIA CUDA userland (cuDNN etc. — the
# same freely-redistributable PyPI packages PyTorch uses). Everything runs
# locally and offline; this script's downloads are the ONLY network access.
#
# Requirements: an NVIDIA GPU (RTX recommended) with a current driver,
# python3, curl, unzip. ~2 GB download / ~3.2 GB on disk.
#
# Override the install dir with EARTH_AISR_DIR (the app reads the same var).
set -euo pipefail

AISR_DIR="${EARTH_AISR_DIR:-$HOME/.earthreclaim/aisr}"
ORT_VERSION="1.20.1"
ORT_TGZ="onnxruntime-linux-x64-gpu-${ORT_VERSION}.tgz"
ORT_URL="https://github.com/microsoft/onnxruntime/releases/download/v${ORT_VERSION}/${ORT_TGZ}"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODEL_SRC="$REPO_ROOT/resources/aisr/realesr-x2.onnx"
WHEELS=(
  nvidia-cudnn-cu12
  nvidia-cublas-cu12
  nvidia-curand-cu12
  nvidia-cufft-cu12
  nvidia-cuda-runtime-cu12
  nvidia-cuda-nvrtc-cu12
)

command -v nvidia-smi >/dev/null || {
  echo "error: nvidia-smi not found — the AI upscaler needs an NVIDIA GPU/driver." >&2
  exit 1
}

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$AISR_DIR"

echo "==> Model"
install -m 644 "$MODEL_SRC" "$AISR_DIR/realesr-x2.onnx"

echo "==> onnxruntime-gpu ${ORT_VERSION} (GitHub, MIT)"
curl -L --fail --progress-bar -o "$TMP/$ORT_TGZ" "$ORT_URL"
tar -xzf "$TMP/$ORT_TGZ" -C "$TMP"
cp "$TMP/onnxruntime-linux-x64-gpu-${ORT_VERSION}/lib/libonnxruntime.so.${ORT_VERSION}" "$AISR_DIR/libonnxruntime.so"
cp "$TMP/onnxruntime-linux-x64-gpu-${ORT_VERSION}/lib/"libonnxruntime_providers_{shared,cuda}.so "$AISR_DIR/"

echo "==> NVIDIA CUDA userland (PyPI redistributables: ${WHEELS[*]})"
python3 -m pip download --no-deps -q -d "$TMP/wheels" "${WHEELS[@]}"
for whl in "$TMP"/wheels/*.whl; do
  unzip -oq "$whl" "nvidia/*/lib/*.so*" -d "$TMP/wheels/x" || true
done
find "$TMP/wheels/x" -name "*.so*" -exec cp {} "$AISR_DIR/" \;

echo
echo "Installed to $AISR_DIR ($(du -sh "$AISR_DIR" | cut -f1))."
echo "Restart Reclaim — the media player's Enhance button now cycles"
echo "Off → FSR → AI (Real-ESRGAN). Remove the directory to uninstall."
