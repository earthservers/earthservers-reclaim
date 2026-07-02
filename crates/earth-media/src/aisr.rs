//! Open-source AI super resolution — Real-ESRGAN "compact" (SRVGGNetCompact)
//! as an ONNX model executed by onnxruntime.
//!
//! This is the freely-shippable AI backend for the Enhance "AI" mode (the
//! Maxine SDK in `nvsr` stays as an alternative backend, but it's paywalled
//! behind NVIDIA AI Enterprise on Linux, so this is the one users can actually
//! have). Everything runs LOCALLY and OFFLINE:
//!   * the model is an inert weights file (no code, no network access — the
//!     runtime just does the math it describes),
//!   * onnxruntime is loaded dynamically and never phones home (telemetry is
//!     explicitly disabled at init as belt-and-braces; the Linux builds have
//!     none to begin with),
//!   * frames go engine-in, engine-out in this process — nothing is written
//!     or transmitted (the TensorRT engine cache under <dir>/trt-cache is a
//!     compiled form of the same local model, written locally).
//!
//! Discovery (no link-time deps; all optional at runtime):
//!   * dir: EARTH_AISR_DIR, default ~/.earthreclaim/aisr
//!   * model: <dir>/realesr-x2.onnx — a 2x SRVGGNetCompact export with
//!     dynamic H/W axes, NCHW f32 RGB in [0,1]
//!   * runtime: <dir>/libonnxruntime.so if present (e.g. the official
//!     onnxruntime-gpu release, giving the CUDA provider), else the system
//!     libonnxruntime.so.1 (CPU — correct but slow; fine for photos/preview,
//!     a warning is logged for video).
//!   * TensorRT (optional, fastest): <dir>/libonnxruntime_providers_tensorrt.so
//!     (in the same official onnxruntime-gpu release) + the TensorRT 10 libs
//!     from the `tensorrt-cu12-libs` PyPI wheel (libnvinfer & co) in <dir>.
//!     The first engage compiles a TensorRT engine (one-time, minutes) and
//!     caches it in <dir>/trt-cache; every later run loads in seconds. Kill
//!     switch: EARTH_AISR_TRT=off falls back to the CUDA provider.
//!
//! Per-frame path: reusable IOBound buffers (CUDA-pinned host memory when
//! available) — no per-frame tensor allocation, and DMA-friendly transfers.

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use ort::execution_providers::{CUDAExecutionProvider, TensorRTExecutionProvider};
use ort::memory::{AllocationDevice, Allocator, AllocatorType, MemoryInfo, MemoryType};
use ort::session::builder::GraphOptimizationLevel;
use ort::session::{IoBinding, Session};
use ort::value::Tensor;

/// Fixed model scale (the shipped model is a 2x SRVGGNetCompact export).
pub const SCALE: u32 = 2;

/// Largest input the AI path scales (bigger sources stay 1:1 and the GL stage
/// falls back to FSR — see `nvsr::nv_factor`, which gates on these). Also the
/// ceiling of the TensorRT optimization profile, so one cached engine covers
/// every size the gate can admit. Change the gate ONLY here: TensorRT fails
/// inference HARD on shapes outside the profile, so a gate above the profile
/// ceiling would produce broken (fallback-scaled) playback, not just a slow
/// path. Measured on an RTX 4060 Ti (fp16 TRT engine): 360p ~13 ms, 480p
/// ~26 ms, 720p ~65 ms/frame — a raise past 720p is far off real-time
/// anyway.
pub(crate) const MAX_IN_W: i32 = 1280;
pub(crate) const MAX_IN_H: i32 = 720;

/// Smallest input the AI path scales — the TensorRT profile floor. Sources
/// tinier than this (absurd for video) stay 1:1 and get GL-FSR like
/// oversized ones; the same hard-failure rule as MAX_IN_* applies below the
/// profile, so keep `nvsr::nv_factor` gated on it.
pub(crate) const MIN_IN: i32 = 32;

fn aisr_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("EARTH_AISR_DIR") {
        return PathBuf::from(dir);
    }
    std::env::var("HOME")
        .map(|h| PathBuf::from(h).join(".earthreclaim").join("aisr"))
        .unwrap_or_else(|_| PathBuf::from("/tmp/earthreclaim-aisr"))
}

fn model_path() -> PathBuf {
    aisr_dir().join("realesr-x2.onnx")
}

/// Initialise the ort environment once (dynamic runtime, telemetry off).
/// Returns false when no usable onnxruntime library could be loaded.
fn ort_ready() -> bool {
    static INIT: OnceLock<bool> = OnceLock::new();
    *INIT.get_or_init(|| {
        // Prefer a runtime dropped next to the model (e.g. onnxruntime-gpu for
        // the CUDA provider); fall back to the system library.
        let local = aisr_dir().join("libonnxruntime.so");
        let path = if local.exists() {
            // CUDA/TensorRT provider deps (cuDNN/cuBLAS/libnvinfer) may live in
            // the same dir — preload so the providers resolve without
            // LD_LIBRARY_PATH (same trick as the Maxine loader: ld.so finds
            // already-loaded SONAMEs).
            preload_dir_libs(&aisr_dir());
            local.to_string_lossy().into_owned()
        } else {
            "libonnxruntime.so.1".to_string()
        };
        let init = ort::init_from(&path).map(|b| b.with_telemetry(false).commit());
        match init {
            Ok(_) => {
                log::info!("[earth-media] aisr: onnxruntime loaded from {}", path);
                true
            }
            Err(e) => {
                log::info!("[earth-media] aisr: onnxruntime unavailable ({}): {}", path, e);
                false
            }
        }
    })
}

/// Best-effort preload of CUDA/TensorRT provider dependencies dropped into the
/// aisr dir. Order matters: each dlopen resolves its own deps against what is
/// already resident, so bases come before the libs that link them.
fn preload_dir_libs(dir: &std::path::Path) {
    const PREFIXES: &[&str] = &[
        "libcudart", "libnvrtc", "libcurand", "libcufft", "libcublasLt", "libcublas", "libcudnn",
        // TensorRT 10 (from the tensorrt-cu12-libs wheel): core first, then
        // the libs linking it. builder_resource is dlopen'd BY libnvinfer at
        // engine-build time by bare SONAME — resident-preloading it is the
        // only way that resolves outside LD_LIBRARY_PATH.
        "libnvinfer.so", "libnvinfer_builder_resource", "libnvinfer_plugin", "libnvonnxparser",
    ];
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            name.contains(".so") && PREFIXES.iter().any(|pre| name.starts_with(pre))
        })
        .collect();
    files.sort_by_key(|p| {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        (PREFIXES.iter().position(|pre| name.starts_with(pre)).unwrap_or(usize::MAX), name)
    });
    for path in files {
        match unsafe { libloading::Library::new(&path) } {
            Ok(lib) => std::mem::forget(lib),
            Err(e) => log::debug!("[earth-media] aisr dep {} skipped: {}", path.display(), e),
        }
    }
}

/// Whether CUDA execution is actually usable (provider lib + cuDNN resolve).
/// The AI mode is gated on this for VIDEO: CPU inference takes seconds per
/// frame at video resolutions, which would stall the pipeline into uselessness.
fn cuda_ready() -> bool {
    static CUDA: OnceLock<bool> = OnceLock::new();
    *CUDA.get_or_init(|| {
        if !ort_ready() {
            return false;
        }
        use ort::execution_providers::ExecutionProvider;
        let ok = CUDAExecutionProvider::default().is_available().unwrap_or(false);
        if !ok {
            log::info!(
                "[earth-media] aisr: CUDA execution provider unavailable — AI video \
                 upscaling disabled (CPU would be far too slow). Install the \
                 onnxruntime-gpu runtime + cuDNN into {}",
                aisr_dir().display()
            );
        }
        ok
    })
}

/// Whether the TensorRT provider should be attempted: its provider lib and the
/// TensorRT runtime are installed, and it isn't switched off.
fn trt_enabled() -> bool {
    if matches!(std::env::var("EARTH_AISR_TRT").as_deref(), Ok("off") | Ok("0")) {
        return false;
    }
    let dir = aisr_dir();
    dir.join("libonnxruntime_providers_tensorrt.so").exists()
        && dir.join("libnvinfer.so.10").exists()
}

/// Whether the open AI upscaler can run: model present + runtime loads + CUDA.
pub fn available() -> bool {
    model_path().exists() && ort_ready() && cuda_ready()
}

/// Engage-time preflight: build (and cache) the session AND run one warm-up
/// inference, so a broken model/runtime refuses the MODE SWITCH with a clear
/// error instead of failing per-frame mid-playback — and so the one-time
/// TensorRT engine build (minutes on the very first engage, then cached in
/// trt-cache/) happens HERE rather than stalling the streaming thread.
pub fn preflight() -> Result<(), String> {
    static WARM: OnceLock<Result<(), String>> = OnceLock::new();
    WARM.get_or_init(|| {
        engine()?;
        let (w, h) = (640usize, 360usize);
        let t0 = std::time::Instant::now();
        let input = vec![0u8; w * h * 4];
        let mut out = vec![0u8; (w * 2) * (h * 2) * 4];
        process(&input, w, h, w * 4, &mut out, w * 2 * 4)?;
        log::info!(
            "[earth-media] aisr: warm-up inference done in {:.1}s",
            t0.elapsed().as_secs_f64()
        );
        Ok(())
    })
    .clone()
}

/// The live inference state: one session for the process plus the reusable
/// per-size IO binding (rebuilt only when the input size changes).
struct Engine {
    session: Session,
    bound: Option<Bound>,
}

fn engine() -> Result<&'static Mutex<Engine>, String> {
    static ENGINE: OnceLock<Result<Mutex<Engine>, String>> = OnceLock::new();
    ENGINE
        .get_or_init(|| {
            if !model_path().exists() {
                return Err(format!(
                    "AI model not installed (expected {})",
                    model_path().display()
                ));
            }
            if !ort_ready() {
                return Err("onnxruntime could not be loaded".to_string());
            }
            let session = build_session()?;
            log::info!(
                "[earth-media] aisr: Real-ESRGAN session ready (inputs: {:?})",
                session.inputs().iter().map(|i| i.name().to_string()).collect::<Vec<_>>()
            );
            Ok(Mutex::new(Engine { session, bound: None }))
        })
        .as_ref()
        .map_err(|e| e.clone())
}

fn session_builder() -> Result<ort::session::builder::SessionBuilder, String> {
    let builder = Session::builder()
        .map_err(|e| format!("AI session builder failed: {}", e))?;
    let builder = builder
        .with_optimization_level(GraphOptimizationLevel::Level2)
        .map_err(|e| format!("AI optimization level failed: {}", e))?;
    builder
        .with_intra_threads(num_threads())
        .map_err(|e| format!("AI threads failed: {}", e))
}

/// Build the session on the fastest usable provider: TensorRT (fp16, cached
/// engine) when its libs are installed, else CUDA (ort silently keeps CPU when
/// even that is missing — `available()` gates video on CUDA regardless).
fn build_session() -> Result<Session, String> {
    if trt_enabled() {
        let cache = aisr_dir().join("trt-cache");
        let _ = std::fs::create_dir_all(&cache);
        let cache = cache.to_string_lossy().into_owned();
        // One explicit optimization profile spanning everything the gate can
        // send (shape names match the realesr-x2.onnx export's "input"): the
        // cached engine serves ALL sizes — no rebuild when the video size
        // changes mid-session.
        let trt = TensorRTExecutionProvider::default()
            .with_fp16(true)
            .with_engine_cache(true)
            .with_engine_cache_path(&cache)
            .with_timing_cache(true)
            .with_timing_cache_path(&cache)
            .with_profile_min_shapes(format!("input:1x3x{}x{}", MIN_IN, MIN_IN))
            .with_profile_opt_shapes("input:1x3x360x640")
            .with_profile_max_shapes(format!("input:1x3x{}x{}", MAX_IN_H, MAX_IN_W))
            .build()
            .error_on_failure();
        let built = session_builder()?
            .with_execution_providers([trt])
            .map_err(|e| format!("AI execution providers failed: {}", e))
            .and_then(|mut b| {
                b.commit_from_file(model_path())
                    .map_err(|e| format!("AI model failed to load: {}", e))
            });
        match built {
            Ok(s) => {
                log::info!("[earth-media] aisr: TensorRT execution provider active (engine cache: {})", cache);
                return Ok(s);
            }
            Err(e) => log::warn!(
                "[earth-media] aisr: TensorRT provider failed ({}) — falling back to CUDA",
                e
            ),
        }
    }
    let cuda = CUDAExecutionProvider::default().build();
    let session = session_builder()?
        .with_execution_providers([cuda])
        .map_err(|e| format!("AI execution providers failed: {}", e))?
        .commit_from_file(model_path())
        .map_err(|e| format!("AI model failed to load: {}", e))?;
    log::info!("[earth-media] aisr: CUDA execution provider active");
    Ok(session)
}

fn num_threads() -> usize {
    std::thread::available_parallelism().map(|n| n.get().min(8)).unwrap_or(4)
}

/// Reusable per-size IO: input/output tensors bound once, written in place
/// every frame. CUDA-pinned (page-locked) host memory when the provider offers
/// it — DMA transfers without an intermediate pageable copy — else plain CPU
/// tensors (still reused; only the copies are slower).
struct Bound {
    in_w: usize,
    in_h: usize,
    // Declaration order = drop order: the binding (which holds the output
    // tensor) and the input tensor must drop BEFORE the allocators that own
    // their pinned memory.
    input: Tensor<f32>,
    binding: IoBinding,
    _allocs: Vec<Allocator>,
}

impl Bound {
    fn new(session: &Session, in_w: usize, in_h: usize) -> Result<Self, String> {
        let (out_w, out_h) = (in_w * SCALE as usize, in_h * SCALE as usize);
        let in_shape = [1usize, 3, in_h, in_w];
        let out_shape = [1usize, 3, out_h, out_w];

        let pinned = |mem_type: MemoryType| -> Result<Allocator, ort::Error> {
            Allocator::new(
                session,
                MemoryInfo::new(AllocationDevice::CUDA_PINNED, 0, AllocatorType::Device, mem_type)?,
            )
        };
        let make = || -> Result<(Tensor<f32>, Tensor<f32>, Vec<Allocator>), ort::Error> {
            let in_alloc = pinned(MemoryType::CPUInput)?;
            let out_alloc = pinned(MemoryType::CPUOutput)?;
            let input = Tensor::<f32>::new(&in_alloc, in_shape)?;
            let output = Tensor::<f32>::new(&out_alloc, out_shape)?;
            Ok((input, output, vec![in_alloc, out_alloc]))
        };
        let (input, output, allocs) = match make() {
            Ok(v) => v,
            Err(e) => {
                log::debug!("[earth-media] aisr: pinned buffers unavailable ({}) — CPU buffers", e);
                let input = Tensor::from_array((in_shape, vec![0f32; 3 * in_w * in_h]))
                    .map_err(|e| format!("AI input buffer failed: {}", e))?;
                let output = Tensor::from_array((out_shape, vec![0f32; 3 * out_w * out_h]))
                    .map_err(|e| format!("AI output buffer failed: {}", e))?;
                (input, output, Vec::new())
            }
        };

        let input_name = session
            .inputs()
            .first()
            .map(|i| i.name().to_string())
            .ok_or("AI model has no inputs")?;
        let output_name = session
            .outputs()
            .first()
            .map(|o| o.name().to_string())
            .ok_or("AI model has no outputs")?;
        let mut binding = session
            .create_binding()
            .map_err(|e| format!("AI io-binding failed: {}", e))?;
        binding
            .bind_input(input_name, &input)
            .map_err(|e| format!("AI input bind failed: {}", e))?;
        binding
            .bind_output(output_name, output)
            .map_err(|e| format!("AI output bind failed: {}", e))?;
        Ok(Self { in_w, in_h, input, binding, _allocs: allocs })
    }
}

/// RGBA u8 rows (stride-padded) -> NCHW f32 RGB [0,1]. Plane-split slices and
/// exact chunks keep the inner loops bounds-check-free and autovectorizable.
fn rgba_to_chw(input: &[u8], w: usize, h: usize, stride: usize, chw: &mut [f32]) {
    const INV: f32 = 1.0 / 255.0;
    let plane = w * h;
    let (r_pl, rest) = chw.split_at_mut(plane);
    let (g_pl, b_pl) = rest.split_at_mut(plane);
    for y in 0..h {
        let row = &input[y * stride..y * stride + w * 4];
        let r_row = &mut r_pl[y * w..(y + 1) * w];
        let g_row = &mut g_pl[y * w..(y + 1) * w];
        let b_row = &mut b_pl[y * w..(y + 1) * w];
        for (i, px) in row.chunks_exact(4).enumerate() {
            r_row[i] = px[0] as f32 * INV;
            g_row[i] = px[1] as f32 * INV;
            b_row[i] = px[2] as f32 * INV;
        }
    }
}

/// NCHW f32 [0,1] -> RGBA u8 rows (stride-padded, opaque alpha).
fn chw_to_rgba(chw: &[f32], w: usize, h: usize, output: &mut [u8], stride: usize) {
    #[inline(always)]
    fn to_u8(v: f32) -> u8 {
        (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
    }
    let plane = w * h;
    let (r_pl, rest) = chw.split_at(plane);
    let (g_pl, b_pl) = rest.split_at(plane);
    for y in 0..h {
        let row = &mut output[y * stride..y * stride + w * 4];
        let r_row = &r_pl[y * w..(y + 1) * w];
        let g_row = &g_pl[y * w..(y + 1) * w];
        let b_row = &b_pl[y * w..(y + 1) * w];
        for (i, px) in row.chunks_exact_mut(4).enumerate() {
            px[0] = to_u8(r_row[i]);
            px[1] = to_u8(g_row[i]);
            px[2] = to_u8(b_row[i]);
            px[3] = 255;
        }
    }
}

/// Upscale one RGBA frame 2x. `input` is tightly usable via `in_stride`;
/// output is written with `out_stride`. Alpha is dropped (opaque out) — video
/// frames are opaque anyway.
pub fn process(
    input: &[u8],
    in_w: usize,
    in_h: usize,
    in_stride: usize,
    output: &mut [u8],
    out_stride: usize,
) -> Result<(), String> {
    let engine = engine()?;
    let (out_w, out_h) = (in_w * SCALE as usize, in_h * SCALE as usize);

    let mut guard = engine.lock().map_err(|e| format!("AI session poisoned: {}", e))?;
    let Engine { session, bound } = &mut *guard;
    if bound.as_ref().map_or(true, |b| b.in_w != in_w || b.in_h != in_h) {
        *bound = None; // release the old buffers before allocating replacements
        *bound = Some(Bound::new(session, in_w, in_h)?);
    }
    let b = bound.as_mut().expect("bound just ensured");

    {
        let (_, chw) = b.input.extract_tensor_mut();
        rgba_to_chw(input, in_w, in_h, in_stride, chw);
    }

    let outputs = session
        .run_binding(&b.binding)
        .map_err(|e| format!("AI inference failed: {}", e))?;
    b.binding
        .synchronize_outputs()
        .map_err(|e| format!("AI output sync failed: {}", e))?;
    let (shape, data) = outputs[0]
        .try_extract_tensor::<f32>()
        .map_err(|e| format!("AI output extract failed: {}", e))?;

    // Expect [1, 3, out_h, out_w].
    let dims: Vec<usize> = shape.iter().map(|d| *d as usize).collect();
    if dims.len() != 4 || dims[1] < 3 || dims[2] != out_h || dims[3] != out_w {
        return Err(format!(
            "AI output shape {:?} != expected [1,3,{},{}]",
            dims, out_h, out_w
        ));
    }

    chw_to_rgba(data, out_w, out_h, output, out_stride);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real end-to-end inference (needs the model in EARTH_AISR_DIR and a
    /// loadable onnxruntime): 64x48 gradient in, 128x96 out, sane pixel range.
    /// Run with: cargo test -- --ignored
    #[test]
    #[ignore = "needs realesr-x2.onnx + onnxruntime installed"]
    fn realesrgan_upscales_2x() {
        let (w, h) = (64usize, 48usize);
        let mut input = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) * 4;
                input[i] = (x * 255 / w) as u8;
                input[i + 1] = (y * 255 / h) as u8;
                input[i + 2] = 128;
                input[i + 3] = 255;
            }
        }
        let (ow, oh) = (w * 2, h * 2);
        let mut output = vec![0u8; ow * oh * 4];
        process(&input, w, h, w * 4, &mut output, ow * 4).expect("inference failed");

        // Output must be non-degenerate and roughly track the gradient.
        let mid = (oh / 2 * ow + ow / 2) * 4;
        assert_eq!(output[mid + 3], 255, "alpha must be opaque");
        let sum: u64 = output.iter().step_by(4).map(|&v| v as u64).sum();
        let mean = sum / (ow as u64 * oh as u64);
        assert!(mean > 60 && mean < 190, "red-channel mean {} looks degenerate", mean);
    }

    /// The pure conversions must survive strides and round-trip losslessly
    /// enough (u8 -> f32 -> u8 is exact for in-range values).
    #[test]
    fn rgba_chw_roundtrip_with_strides() {
        let (w, h) = (5usize, 3usize);
        let in_stride = w * 4 + 12; // deliberately padded
        let mut rgba = vec![0u8; in_stride * h];
        for y in 0..h {
            for x in 0..w {
                let i = y * in_stride + x * 4;
                rgba[i] = (x * 40) as u8;
                rgba[i + 1] = (y * 70) as u8;
                rgba[i + 2] = (x * y * 20) as u8;
                rgba[i + 3] = 7; // alpha ignored
            }
        }
        let mut chw = vec![0f32; 3 * w * h];
        rgba_to_chw(&rgba, w, h, in_stride, &mut chw);
        let out_stride = w * 4 + 8;
        let mut back = vec![0u8; out_stride * h];
        chw_to_rgba(&chw, w, h, &mut back, out_stride);
        for y in 0..h {
            for x in 0..w {
                let a = y * in_stride + x * 4;
                let b = y * out_stride + x * 4;
                assert_eq!(&rgba[a..a + 3], &back[b..b + 3], "pixel ({},{})", x, y);
                assert_eq!(back[b + 3], 255, "alpha must be forced opaque");
            }
        }
    }

    /// Per-frame latency ladder over the sizes the nv_factor gate can admit.
    /// Prints ms/frame so real-time viability is measurable. Targets: <=16 ms
    /// at 360p, <=33 ms at 480p; a gate raise would need 720p <=50 ms
    /// (measured 62.6 ms on the RTX 4060 Ti — gate stays at 720p). Do NOT add
    /// sizes above MAX_IN_W/H: they exceed the TensorRT optimization profile
    /// and fail hard (production can't reach that — the gate caps at the
    /// profile ceiling).
    #[test]
    #[ignore = "needs realesr-x2.onnx + onnxruntime installed"]
    fn realesrgan_timing_640x360() {
        for (w, h) in [(640usize, 360usize), (854, 480), (1280, 720)] {
            let input = vec![128u8; w * h * 4];
            let (ow, oh) = (w * 2, h * 2);
            let mut output = vec![0u8; ow * oh * 4];
            // Warm-up (session + engine build + per-size binding happen here).
            process(&input, w, h, w * 4, &mut output, ow * 4).expect("warm-up failed");
            let n = 30;
            let t0 = std::time::Instant::now();
            for _ in 0..n {
                process(&input, w, h, w * 4, &mut output, ow * 4).expect("inference failed");
            }
            let per = t0.elapsed().as_secs_f64() * 1000.0 / n as f64;
            println!(
                "realesrgan {}x{} -> {}x{}: {:.1} ms/frame ({:.1} fps)",
                w, h, ow, oh, per, 1000.0 / per
            );
        }
    }
}
