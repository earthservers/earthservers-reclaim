# AI upscaler model

`realesr-x2.onnx` — Real-ESRGAN **compact** 2x video model
(SRVGGNetCompact), fp16, exported to ONNX with dynamic height/width axes.

- Derived from the official weights `RealESRGANv2-animevideo-xsx2.pth`
  (<https://github.com/xinntao/Real-ESRGAN>, BSD-3-Clause — see below).
- Inference contract: input `1x3xHxW` float32 RGB in [0,1] (fp32 I/O with
  fp16 internals), output `1x3x(2H)x(2W)`.
- Installed to `~/.earthreclaim/aisr/` by `scripts/install-ai-upscaler.sh`,
  where the media player's Enhance "AI" mode picks it up (NVIDIA GPU +
  CUDA runtime required — the script installs those too).
- Executed as an fp16 **TensorRT** engine when the TensorRT 10 libs are in
  the same dir (the script installs them; `EARTH_AISR_TRT=off` forces the
  CUDA provider). The first AI engage compiles the engine — one-time,
  can take minutes — and caches it in `~/.earthreclaim/aisr/trt-cache/`.

## Upstream license (Real-ESRGAN)

BSD 3-Clause License. Copyright (c) 2021, Xintao Wang.
Redistribution and use in source and binary forms, with or without
modification, are permitted per the BSD-3-Clause terms:
<https://github.com/xinntao/Real-ESRGAN/blob/master/LICENSE>
