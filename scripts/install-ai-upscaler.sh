#!/usr/bin/env bash
# Install the optional AI video upscaler runtime for Earth Reclaim.
#
# The media player's Enhance button gains an "AI (Real-ESRGAN)" mode when
# ~/.earthreclaim/aisr contains: the model (shipped in this repo), the official
# onnxruntime-gpu libraries, the NVIDIA CUDA userland (cuDNN etc. — the
# same freely-redistributable PyPI packages PyTorch uses), and TensorRT 10
# (same source: NVIDIA's PyPI wheels; the app runs the model as an fp16
# TensorRT engine — ~3x faster than the CUDA provider — and falls back to
# CUDA automatically if TensorRT is absent). Everything runs locally and
# offline; this script's downloads are the ONLY network access. The first
# AI engage compiles a TensorRT engine (one-time, can take minutes) and
# caches it under <dir>/trt-cache for every later run.
#
# Requirements: an NVIDIA GPU (RTX recommended) with a current driver,
# python3, curl, unzip. ~4 GB download / ~4.8 GB on disk.
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
# TensorRT version must match what this onnxruntime release was built against
# (ORT 1.20.x -> TensorRT 10.4). Wheels live on NVIDIA's own index.
TRT_VERSION="10.4.0"

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
cp "$TMP/onnxruntime-linux-x64-gpu-${ORT_VERSION}/lib/"libonnxruntime_providers_{shared,cuda,tensorrt}.so "$AISR_DIR/"

echo "==> NVIDIA CUDA userland (PyPI redistributables: ${WHEELS[*]})"
python3 -m pip download --no-deps -q -d "$TMP/wheels" "${WHEELS[@]}"
for whl in "$TMP"/wheels/*.whl; do
  unzip -oq "$whl" "nvidia/*/lib/*.so*" -d "$TMP/wheels/x" || true
done
find "$TMP/wheels/x" -name "*.so*" -exec cp {} "$AISR_DIR/" \;

echo "==> TensorRT ${TRT_VERSION} (NVIDIA PyPI redistributable)"
python3 -m pip download --no-deps -q -d "$TMP/trt" \
  --index-url https://pypi.nvidia.com "tensorrt-cu12-libs==${TRT_VERSION}"
unzip -oq "$TMP"/trt/tensorrt_cu12_libs-*.whl \
  "tensorrt_libs/libnvinfer.so.*" \
  "tensorrt_libs/libnvinfer_builder_resource.so.*" \
  "tensorrt_libs/libnvinfer_plugin.so.*" \
  "tensorrt_libs/libnvonnxparser.so.*" \
  -x "tensorrt_libs/*_win.so.*" -d "$TMP/trt/x"
find "$TMP/trt/x" -name "*.so*" -exec cp {} "$AISR_DIR/" \;
# NVIDIA ships libnvinfer_builder_resource with an executable-stack ELF flag;
# glibc >= 2.41 refuses to dlopen such objects, so clear the flag in the
# PT_GNU_STACK program header (equivalent to `patchelf --clear-execstack`).
python3 - "$AISR_DIR"/libnvinfer_builder_resource.so.* <<'PYEOF'
import struct, sys
for path in sys.argv[1:]:
    with open(path, "r+b") as f:
        hdr = f.read(64)
        assert hdr[:4] == b"\x7fELF" and hdr[4] == 2, f"{path}: not ELF64"
        e_phoff, = struct.unpack_from("<Q", hdr, 0x20)
        e_phentsize, = struct.unpack_from("<H", hdr, 0x36)
        e_phnum, = struct.unpack_from("<H", hdr, 0x38)
        for i in range(e_phnum):
            off = e_phoff + i * e_phentsize
            f.seek(off)
            p_type, p_flags = struct.unpack("<II", f.read(8))
            if p_type == 0x6474E551 and p_flags & 1:  # PT_GNU_STACK, PF_X
                f.seek(off + 4)
                f.write(struct.pack("<I", p_flags & ~1))
                print(f"cleared execstack flag: {path}")
                break
PYEOF

echo
echo "Installed to $AISR_DIR ($(du -sh "$AISR_DIR" | cut -f1))."
echo "Restart Reclaim — the media player's Enhance button now cycles"
echo "Off → FSR → AI (Real-ESRGAN). Remove the directory to uninstall."
