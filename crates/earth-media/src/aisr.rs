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
//!     or transmitted.
//!
//! Discovery (no link-time deps; all optional at runtime):
//!   * dir: EARTH_AISR_DIR, default ~/.earthreclaim/aisr
//!   * model: <dir>/realesr-x2.onnx — a 2x SRVGGNetCompact export with
//!     dynamic H/W axes, NCHW f32 RGB in [0,1]
//!   * runtime: <dir>/libonnxruntime.so if present (e.g. the official
//!     onnxruntime-gpu release, giving the CUDA provider), else the system
//!     libonnxruntime.so.1 (CPU — correct but slow; fine for photos/preview,
//!     a warning is logged for video).

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::Tensor;

/// Fixed model scale (the shipped model is a 2x SRVGGNetCompact export).
pub const SCALE: u32 = 2;

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
            // CUDA provider deps (cuDNN/cuBLAS) may live in the same dir —
            // preload so the provider resolves without LD_LIBRARY_PATH (same
            // trick as the Maxine loader: ld.so finds already-loaded SONAMEs).
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

/// Best-effort preload of CUDA provider dependencies dropped into the aisr dir.
fn preload_dir_libs(dir: &std::path::Path) {
    const PREFIXES: &[&str] = &[
        "libcudart", "libnvrtc", "libcurand", "libcufft", "libcublasLt", "libcublas", "libcudnn",
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
        let ok = ort::execution_providers::CUDAExecutionProvider::default()
            .is_available()
            .unwrap_or(false);
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

/// Whether the open AI upscaler can run: model present + runtime loads + CUDA.
pub fn available() -> bool {
    model_path().exists() && ort_ready() && cuda_ready()
}

/// Engage-time preflight: build (and cache) the session once so a broken
/// model/runtime refuses the MODE SWITCH with a clear error instead of failing
/// per-frame mid-playback.
pub fn preflight() -> Result<(), String> {
    session().map(|_| ())
}

fn session() -> Result<&'static Mutex<Session>, String> {
    static SESSION: OnceLock<Result<Mutex<Session>, String>> = OnceLock::new();
    SESSION
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
            // CUDA when the runtime has it; ort silently keeps CPU otherwise.
            let cuda = ort::execution_providers::CUDAExecutionProvider::default().build();
            let builder = Session::builder()
                .map_err(|e| format!("AI session builder failed: {}", e))?;
            let builder = builder
                .with_execution_providers([cuda])
                .map_err(|e| format!("AI execution providers failed: {}", e))?;
            let builder = builder
                .with_optimization_level(GraphOptimizationLevel::Level2)
                .map_err(|e| format!("AI optimization level failed: {}", e))?;
            let mut builder = builder
                .with_intra_threads(num_threads())
                .map_err(|e| format!("AI threads failed: {}", e))?;
            let session = builder
                .commit_from_file(model_path())
                .map_err(|e| format!("AI model failed to load: {}", e))?;
            log::info!(
                "[earth-media] aisr: Real-ESRGAN session ready (inputs: {:?})",
                session.inputs().iter().map(|i| i.name().to_string()).collect::<Vec<_>>()
            );
            Ok(Mutex::new(session))
        })
        .as_ref()
        .map_err(|e| e.clone())
}

fn num_threads() -> usize {
    std::thread::available_parallelism().map(|n| n.get().min(8)).unwrap_or(4)
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
    let session = session()?;
    let (out_w, out_h) = (in_w * SCALE as usize, in_h * SCALE as usize);

    // RGBA u8 rows -> NCHW f32 RGB [0,1].
    let mut chw = vec![0f32; 3 * in_w * in_h];
    let plane = in_w * in_h;
    for y in 0..in_h {
        let row = &input[y * in_stride..y * in_stride + in_w * 4];
        for x in 0..in_w {
            let px = &row[x * 4..x * 4 + 3];
            let idx = y * in_w + x;
            chw[idx] = px[0] as f32 / 255.0;
            chw[plane + idx] = px[1] as f32 / 255.0;
            chw[2 * plane + idx] = px[2] as f32 / 255.0;
        }
    }

    let tensor = Tensor::from_array(([1usize, 3, in_h, in_w], chw))
        .map_err(|e| format!("AI input tensor failed: {}", e))?;

    let mut session = session.lock().map_err(|e| format!("AI session poisoned: {}", e))?;
    let input_name = session
        .inputs()
        .first()
        .map(|i| i.name().to_string())
        .ok_or("AI model has no inputs")?;
    let outputs = session
        .run(ort::inputs![input_name.as_str() => tensor])
        .map_err(|e| format!("AI inference failed: {}", e))?;
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

    // NCHW f32 -> RGBA u8 rows.
    let oplane = out_w * out_h;
    for y in 0..out_h {
        let row = &mut output[y * out_stride..y * out_stride + out_w * 4];
        for x in 0..out_w {
            let idx = y * out_w + x;
            let to_u8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            row[x * 4] = to_u8(data[idx]);
            row[x * 4 + 1] = to_u8(data[oplane + idx]);
            row[x * 4 + 2] = to_u8(data[2 * oplane + idx]);
            row[x * 4 + 3] = 255;
        }
    }
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

    /// Per-frame latency at a realistic video size (640x360 -> 1280x720).
    /// Informational — prints the ms/frame so real-time viability is measurable.
    #[test]
    #[ignore = "needs realesr-x2.onnx + onnxruntime installed"]
    fn realesrgan_timing_640x360() {
        let (w, h) = (640usize, 360usize);
        let input = vec![128u8; w * h * 4];
        let (ow, oh) = (w * 2, h * 2);
        let mut output = vec![0u8; ow * oh * 4];
        // Warm-up (session build + CUDA graph capture happen here).
        process(&input, w, h, w * 4, &mut output, ow * 4).expect("warm-up failed");
        let n = 10;
        let t0 = std::time::Instant::now();
        for _ in 0..n {
            process(&input, w, h, w * 4, &mut output, ow * 4).expect("inference failed");
        }
        let per = t0.elapsed().as_secs_f64() * 1000.0 / n as f64;
        println!("realesrgan 640x360->1280x720: {:.1} ms/frame ({:.1} fps)", per, 1000.0 / per);
    }
}
