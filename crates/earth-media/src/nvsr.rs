//! Tier 2 video enhancement — NVIDIA Maxine "SuperRes" AI upscaling.
//!
//! The Maxine Video Effects (VFX) SDK is a PROPRIETARY, separately-installed
//! runtime (login-gated NVIDIA download; Linux needs driver ≥ 570.190 / 580.82
//! / 590.44 and an RTX GPU). It is therefore loaded at RUNTIME via dlopen —
//! never linked — so builds and machines without it are completely unaffected:
//! `available()` is simply false and the `nvai` mode reports itself missing.
//!
//! Install layout (SDK default): /usr/local/VideoFX/lib/libVideoFX.so,
//! libNVCVImage.so and models under /usr/local/VideoFX/lib/models. Override
//! with EARTH_NVVFX_DIR and EARTH_NVVFX_MODEL_DIR.
//!
//! Integration: `earthnvsr`, a GstVideoFilter registered on demand and placed
//! at the FRONT of the resident enhance bin (see `enhance`). While disengaged
//! it runs in BaseTransform passthrough (zero-copy, zero cost). Engaged, it
//! negotiates a 2x output (capped at 4K) and per frame:
//!
//!   CPU RGBA u8 → NvCVImage_Transfer → GPU BGR f32 planar → NvVFX_Run
//!     (SuperRes) → GPU BGR f32 planar (2x) → NvCVImage_Transfer → CPU RGBA u8
//!
//! NOTE ON VERIFICATION: everything here compiles and is exercised in tests up
//! to the dlopen boundary; the actual AI path can only run where the SDK is
//! installed (it is not on the dev machine — first run against a real SDK
//! should be smoke-tested with EARTH_MEDIA_DEBUG=1).

use gstreamer as gst;
use gstreamer::glib;
use gstreamer::prelude::*;
use libloading::Library;
use std::ffi::{c_char, c_int, c_uint, c_void, CString};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// Output ceiling (matches the FSR stage's policy in `enhance`).
const MAX_W: u32 = 3840;
const MAX_H: u32 = 2160;

// ---------------------------------------------------------------------------
// FFI surface (from nvCVImage.h / nvVideoEffects.h — stable public C API)
// ---------------------------------------------------------------------------

type NvCvStatus = c_int; // NVCV_SUCCESS = 0
type NvVfxHandle = *mut c_void;

// NvCVImage_PixelFormat
const NVCV_RGBA: c_int = 6;
const NVCV_BGR: c_int = 5;
// NvCVImage_ComponentType
const NVCV_U8: c_int = 1;
const NVCV_F32: c_int = 7;
// Layout
const NVCV_CHUNKY: c_uint = 0;
const NVCV_PLANAR: c_uint = 1;
// Memory space
const NVCV_CPU: c_uint = 0;
const NVCV_GPU: c_uint = 1;

/// Mirror of the SDK's NvCVImage struct (nvCVImage.h). Field order and types
/// must match the C header exactly — this is the ABI contract with the dlopen'd
/// library.
#[repr(C)]
struct NvCVImage {
    width: c_uint,
    height: c_uint,
    pitch: c_int,
    pixel_format: c_int,
    component_type: c_int,
    pixel_bytes: u8,
    component_bytes: u8,
    num_components: u8,
    planar: u8,
    gpu_mem: u8,
    colorspace: u8,
    reserved: [u8; 2],
    pixels: *mut c_void,
    delete_ptr: *mut c_void,
    delete_proc: Option<unsafe extern "C" fn(*mut c_void)>,
    buffer_bytes: u64,
}

impl NvCVImage {
    fn zeroed() -> Self {
        // Safety: all-zero is the documented "empty image" state (pixels NULL).
        unsafe { std::mem::zeroed() }
    }
}

type FnCreateEffect = unsafe extern "C" fn(*const c_char, *mut NvVfxHandle) -> NvCvStatus;
type FnDestroyEffect = unsafe extern "C" fn(NvVfxHandle) -> NvCvStatus;
type FnSetString = unsafe extern "C" fn(NvVfxHandle, *const c_char, *const c_char) -> NvCvStatus;
type FnSetU32 = unsafe extern "C" fn(NvVfxHandle, *const c_char, c_uint) -> NvCvStatus;
type FnSetImage = unsafe extern "C" fn(NvVfxHandle, *const c_char, *mut NvCVImage) -> NvCvStatus;
type FnLoad = unsafe extern "C" fn(NvVfxHandle) -> NvCvStatus;
type FnRun = unsafe extern "C" fn(NvVfxHandle, c_int) -> NvCvStatus;
type FnImgAlloc = unsafe extern "C" fn(
    *mut NvCVImage, c_uint, c_uint, c_int, c_int, c_uint, c_uint, c_uint,
) -> NvCvStatus;
type FnImgDealloc = unsafe extern "C" fn(*mut NvCVImage) -> NvCvStatus;
type FnImgInit = unsafe extern "C" fn(
    *mut NvCVImage, c_uint, c_uint, c_int, *mut c_void, c_int, c_int, c_uint, c_uint,
) -> NvCvStatus;
type FnImgTransfer = unsafe extern "C" fn(
    *const NvCVImage, *mut NvCVImage, f32, *mut c_void, *mut NvCVImage,
) -> NvCvStatus;

/// The dlopen'd SDK. Loaded once; `None` if the libraries aren't installed.
struct Runtime {
    // Field order = drop order: symbols must not outlive the libraries, and
    // NVCVImage must outlive VideoFX (which links against it).
    create_effect: FnCreateEffect,
    destroy_effect: FnDestroyEffect,
    set_string: FnSetString,
    set_u32: FnSetU32,
    set_image: FnSetImage,
    load: FnLoad,
    run: FnRun,
    img_alloc: FnImgAlloc,
    img_dealloc: FnImgDealloc,
    img_init: FnImgInit,
    img_transfer: FnImgTransfer,
    _vfx: Library,
    _nvcv: Library,
    model_dir: CString,
}

unsafe impl Send for Runtime {}
unsafe impl Sync for Runtime {}

fn sdk_dir() -> PathBuf {
    std::env::var("EARTH_NVVFX_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/usr/local/VideoFX/lib"))
}

fn model_dir() -> PathBuf {
    std::env::var("EARTH_NVVFX_MODEL_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| sdk_dir().join("models"))
}

/// Best-effort preload of libVideoFX's private dependencies (TensorRT / CUDA
/// runtime pieces the feature installer drops NEXT TO it). dlopen-by-absolute-
/// path does NOT search that directory for dependencies, and a desktop-launched
/// app has no LD_LIBRARY_PATH pointing there — but ld.so resolves a dependency
/// by SONAME if a matching library is ALREADY loaded, so loading them first
/// (and leaking them resident) makes libVideoFX link without any env setup.
fn preload_sdk_deps(dir: &std::path::Path) {
    const PREFIXES: &[&str] = &[
        "libcudart", "libcublasLt", "libcublas", "libcudnn",
        "libnvonnxparser", "libnvinfer_plugin", "libnvinfer",
    ];
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            name.contains(".so") && PREFIXES.iter().any(|pre| name.starts_with(pre))
        })
        .collect();
    // Load in the PREFIXES order (rough dependency order), stable within a prefix.
    files.sort_by_key(|p| {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        let rank = PREFIXES.iter().position(|pre| name.starts_with(pre)).unwrap_or(usize::MAX);
        (rank, name)
    });
    for path in files {
        // Safety: loading NVIDIA's own redistributables. Failures are fine —
        // some are alternate versions; libVideoFX's own load reports the truth.
        match unsafe { Library::new(&path) } {
            Ok(lib) => std::mem::forget(lib), // keep resident for SONAME resolution
            Err(e) => log::debug!("[earth-media] VFX dep {} skipped: {}", path.display(), e),
        }
    }
}

fn runtime() -> Option<&'static Runtime> {
    static RT: OnceLock<Option<Runtime>> = OnceLock::new();
    RT.get_or_init(|| {
        let dir = sdk_dir();
        let nvcv_path = dir.join("libNVCVImage.so");
        let vfx_path = dir.join("libVideoFX.so");
        if !vfx_path.exists() {
            log::info!(
                "[earth-media] NVIDIA VFX SDK not found at {} — 'nvai' enhance unavailable \
                 (install the Maxine Video Effects SDK or set EARTH_NVVFX_DIR)",
                dir.display()
            );
            return None;
        }
        preload_sdk_deps(&dir);
        // Safety: loading NVIDIA's own redistributable libraries by absolute path.
        let (nvcv, vfx) = unsafe {
            let nvcv = Library::new(&nvcv_path)
                .map_err(|e| log::warn!("[earth-media] {} failed to load: {}", nvcv_path.display(), e))
                .ok()?;
            let vfx = Library::new(&vfx_path)
                .map_err(|e| log::warn!("[earth-media] {} failed to load: {}", vfx_path.display(), e))
                .ok()?;
            (nvcv, vfx)
        };
        macro_rules! sym {
            ($lib:expr, $name:literal, $ty:ty) => {
                match unsafe { $lib.get::<$ty>($name) } {
                    Ok(s) => *s,
                    Err(e) => {
                        log::warn!("[earth-media] VFX SDK symbol {:?} missing: {}",
                            String::from_utf8_lossy($name), e);
                        return None;
                    }
                }
            };
        }
        let rt = Runtime {
            create_effect: sym!(vfx, b"NvVFX_CreateEffect", FnCreateEffect),
            destroy_effect: sym!(vfx, b"NvVFX_DestroyEffect", FnDestroyEffect),
            set_string: sym!(vfx, b"NvVFX_SetString", FnSetString),
            set_u32: sym!(vfx, b"NvVFX_SetU32", FnSetU32),
            set_image: sym!(vfx, b"NvVFX_SetImage", FnSetImage),
            load: sym!(vfx, b"NvVFX_Load", FnLoad),
            run: sym!(vfx, b"NvVFX_Run", FnRun),
            img_alloc: sym!(nvcv, b"NvCVImage_Alloc", FnImgAlloc),
            img_dealloc: sym!(nvcv, b"NvCVImage_Dealloc", FnImgDealloc),
            img_init: sym!(nvcv, b"NvCVImage_Init", FnImgInit),
            img_transfer: sym!(nvcv, b"NvCVImage_Transfer", FnImgTransfer),
            _vfx: vfx,
            _nvcv: nvcv,
            model_dir: CString::new(model_dir().to_string_lossy().as_bytes()).ok()?,
        };
        log::info!(
            "[earth-media] NVIDIA VFX SDK loaded from {} (models: {})",
            dir.display(),
            model_dir().display()
        );
        Some(rt)
    })
    .as_ref()
}

/// Whether the Maxine runtime is installed and loadable.
pub fn available() -> bool {
    runtime().is_some()
}

/// Engage-time preflight: build (and cache) a tiny SuperRes engine once, so a
/// missing/mismatched model directory or driver refuses the MODE SWITCH with a
/// clear error instead of failing per-frame mid-playback.
pub fn preflight() -> Result<(), String> {
    static CHECK: OnceLock<Result<(), String>> = OnceLock::new();
    CHECK
        .get_or_init(|| SuperRes::new(192, 108).map(|_| ()))
        .clone()
}

/// One SuperRes effect instance bound to fixed in/out sizes.
struct SuperRes {
    handle: NvVfxHandle,
    src_gpu: NvCVImage,
    dst_gpu: NvCVImage,
    tmp_gpu: NvCVImage,
    in_w: u32,
    in_h: u32,
    out_w: u32,
    out_h: u32,
}

unsafe impl Send for SuperRes {}

impl SuperRes {
    fn new(in_w: u32, in_h: u32) -> Result<Self, String> {
        let rt = runtime().ok_or("NVIDIA VFX SDK not loaded")?;
        let (out_w, out_h) = (in_w * 2, in_h * 2);
        let check = |what: &str, s: NvCvStatus| -> Result<(), String> {
            if s == 0 { Ok(()) } else { Err(format!("{} failed (NvCV status {})", what, s)) }
        };
        unsafe {
            let mut handle: NvVfxHandle = std::ptr::null_mut();
            let selector = CString::new("SuperRes").unwrap();
            check("NvVFX_CreateEffect(SuperRes)", (rt.create_effect)(selector.as_ptr(), &mut handle))?;

            let destroy_on_err = |handle: NvVfxHandle| {
                let _ = (rt.destroy_effect)(handle);
            };
            let mut build = || -> Result<(NvCVImage, NvCVImage, NvCVImage), String> {
                let model_dir_key = CString::new("ModelDir").unwrap();
                check("NvVFX_SetString(ModelDir)",
                    (rt.set_string)(handle, model_dir_key.as_ptr(), rt.model_dir.as_ptr()))?;
                // Mode 1 = stronger enhancement (0 = weaker), per SDK docs.
                let mode_key = CString::new("Mode").unwrap();
                check("NvVFX_SetU32(Mode)", (rt.set_u32)(handle, mode_key.as_ptr(), 1))?;

                // SuperRes I/O: BGR f32 PLANAR on the GPU, values in [0,1].
                let mut src = NvCVImage::zeroed();
                check("NvCVImage_Alloc(src)",
                    (rt.img_alloc)(&mut src, in_w, in_h, NVCV_BGR, NVCV_F32, NVCV_PLANAR, NVCV_GPU, 1))?;
                let mut dst = NvCVImage::zeroed();
                check("NvCVImage_Alloc(dst)",
                    (rt.img_alloc)(&mut dst, out_w, out_h, NVCV_BGR, NVCV_F32, NVCV_PLANAR, NVCV_GPU, 1))?;
                // Staging buffer for CPU<->GPU format conversion transfers.
                let mut tmp = NvCVImage::zeroed();
                check("NvCVImage_Alloc(tmp)",
                    (rt.img_alloc)(&mut tmp, out_w, out_h, NVCV_RGBA, NVCV_U8, NVCV_CHUNKY, NVCV_GPU, 1))?;

                let src_key = CString::new("SrcImage0").unwrap();
                check("NvVFX_SetImage(SrcImage0)", (rt.set_image)(handle, src_key.as_ptr(), &mut src))?;
                let dst_key = CString::new("DstImage0").unwrap();
                check("NvVFX_SetImage(DstImage0)", (rt.set_image)(handle, dst_key.as_ptr(), &mut dst))?;
                check("NvVFX_Load", (rt.load)(handle))?;
                Ok((src, dst, tmp))
            };
            match build() {
                Ok((src_gpu, dst_gpu, tmp_gpu)) => Ok(Self {
                    handle, src_gpu, dst_gpu, tmp_gpu, in_w, in_h, out_w, out_h,
                }),
                Err(e) => {
                    destroy_on_err(handle);
                    Err(e)
                }
            }
        }
    }

    /// Run one frame: RGBA u8 in (row `in_stride` bytes) → RGBA u8 out.
    fn process(
        &mut self,
        input: &[u8],
        in_stride: i32,
        output: &mut [u8],
        out_stride: i32,
    ) -> Result<(), String> {
        let rt = runtime().ok_or("NVIDIA VFX SDK not loaded")?;
        let check = |what: &str, s: NvCvStatus| -> Result<(), String> {
            if s == 0 { Ok(()) } else { Err(format!("{} failed (NvCV status {})", what, s)) }
        };
        unsafe {
            let mut cpu_in = NvCVImage::zeroed();
            check("NvCVImage_Init(in)", (rt.img_init)(
                &mut cpu_in, self.in_w, self.in_h, in_stride,
                input.as_ptr() as *mut c_void, NVCV_RGBA, NVCV_U8, NVCV_CHUNKY, NVCV_CPU,
            ))?;
            let mut cpu_out = NvCVImage::zeroed();
            check("NvCVImage_Init(out)", (rt.img_init)(
                &mut cpu_out, self.out_w, self.out_h, out_stride,
                output.as_mut_ptr() as *mut c_void, NVCV_RGBA, NVCV_U8, NVCV_CHUNKY, NVCV_CPU,
            ))?;
            // u8 [0,255] -> f32 [0,1] on the way up, and back on the way down.
            check("NvCVImage_Transfer(H2D)",
                (rt.img_transfer)(&cpu_in, &mut self.src_gpu, 1.0 / 255.0, std::ptr::null_mut(), &mut self.tmp_gpu))?;
            check("NvVFX_Run", (rt.run)(self.handle, 0))?;
            check("NvCVImage_Transfer(D2H)",
                (rt.img_transfer)(&self.dst_gpu, &mut cpu_out, 255.0, std::ptr::null_mut(), &mut self.tmp_gpu))?;
        }
        Ok(())
    }
}

impl Drop for SuperRes {
    fn drop(&mut self) {
        if let Some(rt) = runtime() {
            unsafe {
                let _ = (rt.img_dealloc)(&mut self.src_gpu);
                let _ = (rt.img_dealloc)(&mut self.dst_gpu);
                let _ = (rt.img_dealloc)(&mut self.tmp_gpu);
                let _ = (rt.destroy_effect)(self.handle);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The `earthnvsr` element — a VideoFilter that is passthrough until engaged.
// ---------------------------------------------------------------------------

/// Whether nvai would scale this input. AI 2x is gated to sources the
/// upscaler holds a real-time budget for (`aisr::MAX_IN_W/H` — also the
/// TensorRT engine profile ceiling); above that the element negotiates 1:1
/// (cheap row copy) and the GL stage falls back to FSR (see
/// EnhanceCtl::apply), so 'AI mode' never crawls or stalls.
pub(crate) fn nv_factor(w: i32, h: i32) -> i32 {
    if w >= crate::aisr::MIN_IN
        && h >= crate::aisr::MIN_IN
        && w <= crate::aisr::MAX_IN_W
        && h <= crate::aisr::MAX_IN_H
        && (2 * w) as u32 <= MAX_W
        && (2 * h) as u32 <= MAX_H
    {
        2
    } else {
        1
    }
}

/// Blend an AI-upscaled 2x RGBA frame (in `output`) against a nearest-
/// neighbour 2x of the source, in place: out = ai*strength + orig*(1-strength).
/// Integer math per channel; alpha forced opaque. `strength` is 0.0..=1.0.
pub(crate) fn blend_with_nearest(
    input: &[u8],
    in_w: usize,
    in_h: usize,
    in_stride: usize,
    output: &mut [u8],
    out_w: usize,
    out_h: usize,
    out_stride: usize,
    strength: f32,
) {
    let q = (strength.clamp(0.0, 1.0) * 256.0 + 0.5) as u32; // 256 = pure AI
    let r = 256 - q;
    for oy in 0..out_h {
        let iy = (oy / 2).min(in_h - 1);
        let src = &input[iy * in_stride..iy * in_stride + in_w * 4];
        let dst = &mut output[oy * out_stride..oy * out_stride + out_w * 4];
        for (ox, px) in dst.chunks_exact_mut(4).enumerate() {
            let ix = (ox / 2).min(in_w - 1);
            let sp = &src[ix * 4..ix * 4 + 4];
            px[0] = ((px[0] as u32 * q + sp[0] as u32 * r) >> 8) as u8;
            px[1] = ((px[1] as u32 * q + sp[1] as u32 * r) >> 8) as u8;
            px[2] = ((px[2] as u32 * q + sp[2] as u32 * r) >> 8) as u8;
            px[3] = 255;
        }
    }
}

mod imp {
    use super::*;
    use gst::subclass::prelude::*;
    use gstreamer_base::subclass::base_transform::BaseTransformMode;
    use gstreamer_base::prelude::*;
    use gstreamer_base::subclass::prelude::*;
    use gstreamer_video as gst_video;
    use gst_video::prelude::*;
    use gst_video::subclass::prelude::*;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

    /// The AI backend actually driving an engaged element.
    pub enum Engine {
        /// NVIDIA Maxine SuperRes (proprietary SDK, per-size effect instance).
        Maxine(SuperRes),
        /// Open Real-ESRGAN via onnxruntime (module-level session, any size).
        Onnx,
    }

    pub struct EarthNvSr {
        pub engaged: AtomicBool,
        /// Whether the CURRENT negotiation actually scales (out == 2x in).
        /// Set in set_info — i.e. BEFORE the new caps event reaches the
        /// downstream GL stage — so the enhance ctl can trust it from its
        /// caps probe (reading pad caps there raced renegotiation and
        /// double-scaled the GL stage on live size switches).
        pub scaling: AtomicBool,
        /// AI blend strength 0.0..=1.0 as f32 bits (1.0 = pure AI output). A
        /// plain property read per frame — changing it never renegotiates.
        pub strength: AtomicU32,
        pub engine: Mutex<Option<Engine>>,
        pub infos: Mutex<Option<(gst_video::VideoInfo, gst_video::VideoInfo)>>,
    }

    impl Default for EarthNvSr {
        fn default() -> Self {
            Self {
                engaged: AtomicBool::new(false),
                scaling: AtomicBool::new(false),
                strength: AtomicU32::new(1.0f32.to_bits()),
                engine: Mutex::new(None),
                infos: Mutex::new(None),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for EarthNvSr {
        const NAME: &'static str = "EarthNvSr";
        type Type = super::EarthNvSr;
        type ParentType = gst_video::VideoFilter;
    }

    impl ObjectImpl for EarthNvSr {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPS: OnceLock<Vec<glib::ParamSpec>> = OnceLock::new();
            PROPS.get_or_init(|| {
                vec![
                    glib::ParamSpecBoolean::builder("engaged")
                        .nick("Engaged")
                        .blurb("Run NVIDIA SuperRes (false = zero-cost passthrough)")
                        .default_value(false)
                        .build(),
                    glib::ParamSpecBoolean::builder("scaling")
                        .nick("Scaling")
                        .blurb("Whether the current negotiation scales 2x (read-only)")
                        .default_value(false)
                        .read_only()
                        .build(),
                    glib::ParamSpecDouble::builder("strength")
                        .nick("AI strength")
                        .blurb("Blend of AI output vs nearest-upscaled source (1.0 = pure AI)")
                        .minimum(0.0)
                        .maximum(1.0)
                        .default_value(1.0)
                        .build(),
                ]
            })
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            if pspec.name() == "strength" {
                let s = value.get::<f64>().unwrap_or(1.0).clamp(0.0, 1.0) as f32;
                // Property-only: transform_frame reads this per frame; no
                // renegotiation, no reconfigure (see the stability rules).
                self.strength.store(s.to_bits(), Ordering::Relaxed);
                return;
            }
            if pspec.name() == "engaged" {
                let engaged = value.get::<bool>().unwrap_or(false);
                self.engaged.store(engaged, Ordering::Relaxed);
                // Renegotiation here is DANGEROUS: BaseTransform passthrough
                // flips and even no-op reconfigures propagate through the
                // decoder into decodebin's internals, which cannot renegotiate
                // a compressed stream mid-flight -> "streaming stopped,
                // not-negotiated" from qtdemux (the reported freeze). So:
                // never toggle passthrough (the element always runs
                // transform_frame — a cheap row copy at 1:1), and request
                // renegotiation ONLY when the output size would actually
                // change (engaging on a <=720p source, where the 2x announce
                // is absorbed by the GL stage downstream — its filters un-fix
                // sizes — and never reaches upstream of this element).
                let obj = self.obj();
                let would_scale = obj
                    .static_pad("sink")
                    .and_then(|p| p.current_caps())
                    .and_then(|c| c.structure(0).map(|s| s.to_owned()))
                    .and_then(|s| {
                        let w = s.get::<i32>("width").ok()?;
                        let h = s.get::<i32>("height").ok()?;
                        Some(nv_factor(w, h) == 2)
                    })
                    .unwrap_or(false);
                if would_scale {
                    if let Some(pad) = obj.static_pad("src") {
                        pad.mark_reconfigure();
                    }
                }
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "engaged" => self.engaged.load(Ordering::Relaxed).to_value(),
                "scaling" => self.scaling.load(Ordering::Relaxed).to_value(),
                "strength" => (f32::from_bits(self.strength.load(Ordering::Relaxed)) as f64).to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl GstObjectImpl for EarthNvSr {}

    impl ElementImpl for EarthNvSr {
        fn metadata() -> Option<&'static gst::subclass::ElementMetadata> {
            static META: OnceLock<gst::subclass::ElementMetadata> = OnceLock::new();
            Some(META.get_or_init(|| {
                gst::subclass::ElementMetadata::new(
                    "NVIDIA Maxine SuperRes video filter",
                    "Filter/Effect/Video",
                    "AI super resolution via the NVIDIA Video Effects SDK (RTX only)",
                    "EarthServers",
                )
            }))
        }

        fn pad_templates() -> &'static [gst::PadTemplate] {
            static TEMPLATES: OnceLock<Vec<gst::PadTemplate>> = OnceLock::new();
            TEMPLATES.get_or_init(|| {
                let caps = gst_video::VideoCapsBuilder::new()
                    .format(gst_video::VideoFormat::Rgba)
                    .build();
                vec![
                    gst::PadTemplate::new(
                        "sink",
                        gst::PadDirection::Sink,
                        gst::PadPresence::Always,
                        &caps,
                    )
                    .unwrap(),
                    gst::PadTemplate::new(
                        "src",
                        gst::PadDirection::Src,
                        gst::PadPresence::Always,
                        &caps,
                    )
                    .unwrap(),
                ]
            })
        }
    }

    impl BaseTransformImpl for EarthNvSr {
        const MODE: BaseTransformMode = BaseTransformMode::NeverInPlace;
        const PASSTHROUGH_ON_SAME_CAPS: bool = false;
        const TRANSFORM_IP_ON_PASSTHROUGH: bool = false;

        fn transform_caps(
            &self,
            direction: gst::PadDirection,
            caps: &gst::Caps,
            filter: Option<&gst::Caps>,
        ) -> Option<gst::Caps> {
            let engaged = self.engaged.load(Ordering::Relaxed);
            let mut out = gst::Caps::new_empty();
            {
                let out = out.get_mut().unwrap();
                for s in caps.iter() {
                    if !engaged {
                        out.append_structure(s.to_owned());
                        continue;
                    }
                    // Engaged. For RANGE sizes stay UNCONSTRAINED (scaler
                    // pattern, like videoscale) — scaling ranges broke initial
                    // negotiation when a player STARTS with AI on. But for
                    // FIXED sizes map EXACTLY size*nv_factor: advertising
                    // unconstrained sizes for fixed inputs let a downstream
                    // caps pin force this element to scale a source the gate
                    // rejected (e.g. a GL-FSR 2x pin from a vertical video
                    // made 'nvai' scale it anyway — TensorRT then fails HARD
                    // out-of-profile, playback degrades to nearest-neighbour,
                    // and the enhance stage FSR-scales the result AGAIN,
                    // ending at wrong, aspect-distorting geometry).
                    let fixed = (s.get::<i32>("width").ok(), s.get::<i32>("height").ok());
                    let (Some(w), Some(h)) = fixed else {
                        let mut s = s.to_owned();
                        s.set("width", gst::IntRange::new(1i32, i32::MAX));
                        s.set("height", gst::IntRange::new(1i32, i32::MAX));
                        out.append_structure(s);
                        continue;
                    };
                    if direction == gst::PadDirection::Sink {
                        // sink -> src: exactly one possible output.
                        let f = nv_factor(w, h);
                        let mut s = s.to_owned();
                        s.set("width", w * f);
                        s.set("height", h * f);
                        out.append_structure(s);
                    } else {
                        // src -> sink: enumerate the inputs that can yield this
                        // fixed output — half size (if the gate admits it at
                        // 2x) and/or identity (if the gate keeps it 1:1).
                        let mut any = false;
                        if w % 2 == 0 && h % 2 == 0 && nv_factor(w / 2, h / 2) == 2 {
                            let mut s2 = s.to_owned();
                            s2.set("width", w / 2);
                            s2.set("height", h / 2);
                            out.append_structure(s2);
                            any = true;
                        }
                        if nv_factor(w, h) == 1 {
                            out.append_structure(s.to_owned());
                            any = true;
                        }
                        if !any {
                            // No input maps to this output (e.g. an odd-sized
                            // 2x proposal); answer unconstrained so the query
                            // can continue rather than returning EMPTY.
                            let mut s = s.to_owned();
                            s.set("width", gst::IntRange::new(1i32, i32::MAX));
                            s.set("height", gst::IntRange::new(1i32, i32::MAX));
                            out.append_structure(s);
                        }
                    }
                }
            }
            if let Some(filter) = filter {
                out = out.intersect_with_mode(filter, gst::CapsIntersectMode::First);
            }
            Some(out)
        }

        fn fixate_caps(
            &self,
            direction: gst::PadDirection,
            caps: &gst::Caps,
            othercaps: gst::Caps,
        ) -> gst::Caps {
            let engaged = self.engaged.load(Ordering::Relaxed);
            let mut othercaps = othercaps;
            if engaged {
                if let Some(s) = caps.structure(0) {
                    if let (Ok(w), Ok(h)) = (s.get::<i32>("width"), s.get::<i32>("height")) {
                        // sink->src: output = input * factor (2x for <=720p,
                        // else 1:1). src->sink: input = output / factor.
                        let (tw, th) = if direction == gst::PadDirection::Sink {
                            let f = nv_factor(w, h);
                            (w * f, h * f)
                        } else {
                            let f = nv_factor(w / 2, h / 2);
                            (w / f, h / f)
                        };
                        if let Some(m) = othercaps.make_mut().structure_mut(0) {
                            m.fixate_field_nearest_int("width", tw);
                            m.fixate_field_nearest_int("height", th);
                        }
                    }
                }
            }
            othercaps.fixate();
            othercaps
        }
    }

    impl VideoFilterImpl for EarthNvSr {
        fn set_info(
            &self,
            _incaps: &gst::Caps,
            in_info: &gst_video::VideoInfo,
            _outcaps: &gst::Caps,
            out_info: &gst_video::VideoInfo,
        ) -> Result<(), gst::LoggableError> {
            let engaged = self.engaged.load(Ordering::Relaxed);
            let scaling = engaged && out_info.width() == in_info.width() * 2;
            self.scaling.store(scaling, Ordering::Relaxed);
            let mut engine = self.engine.lock().unwrap();
            *engine = None;
            if scaling {
                // Prefer the Maxine SDK when installed; otherwise the open
                // Real-ESRGAN/onnxruntime backend (the one users can freely get).
                if available() {
                    match SuperRes::new(in_info.width(), in_info.height()) {
                        Ok(e) => *engine = Some(Engine::Maxine(e)),
                        Err(err) => {
                            log::warn!("[earth-media] Maxine engine for {}x{} failed: {} — trying ONNX",
                                in_info.width(), in_info.height(), err);
                        }
                    }
                }
                if engine.is_none() && crate::aisr::available() {
                    *engine = Some(Engine::Onnx);
                }
                if engine.is_none() {
                    // Playback survives via the fallback scaling in transform_frame.
                    log::warn!("[earth-media] no AI backend usable — fallback scaling");
                }
            }
            *self.infos.lock().unwrap() = Some((in_info.clone(), out_info.clone()));
            Ok(())
        }

        fn transform_frame(
            &self,
            inframe: &gst_video::VideoFrameRef<&gst::BufferRef>,
            outframe: &mut gst_video::VideoFrameRef<&mut gst::BufferRef>,
        ) -> Result<gst::FlowSuccess, gst::FlowError> {
            let in_w = inframe.info().width() as usize;
            let in_h = inframe.info().height() as usize;
            let out_w = outframe.info().width() as usize;
            let out_h = outframe.info().height() as usize;
            let in_stride = inframe.plane_stride()[0];
            let out_stride = outframe.plane_stride()[0];

            // AI path.
            if out_w == in_w * 2 {
                let mut engine = self.engine.lock().unwrap();
                if let Some(e) = engine.as_mut() {
                    let input = inframe.plane_data(0).map_err(|_| gst::FlowError::Error)?;
                    let output = outframe.plane_data_mut(0).map_err(|_| gst::FlowError::Error)?;
                    let res = match e {
                        Engine::Maxine(m) => m.process(input, in_stride, output, out_stride),
                        Engine::Onnx => crate::aisr::process(
                            input, in_w, in_h, in_stride as usize, output, out_stride as usize,
                        ),
                    };
                    match res {
                        Ok(()) => {
                            // AI strength < 100%: mix the AI result against a
                            // plain nearest-neighbour 2x of the source. Skipped
                            // entirely at full strength (the common case).
                            let s = f32::from_bits(self.strength.load(Ordering::Relaxed))
                                .clamp(0.0, 1.0);
                            if s < 1.0 {
                                blend_with_nearest(
                                    input, in_w, in_h, in_stride as usize,
                                    output, out_w, out_h, out_stride as usize, s,
                                );
                            }
                            return Ok(gst::FlowSuccess::Ok);
                        }
                        Err(err) => {
                            log::warn!("[earth-media] AI frame failed: {} — fallback scaling", err);
                            *engine = None; // don't retry every frame
                        }
                    }
                }
            }

            // Fallback: same size → copy rows; 2x → nearest-neighbor doubling.
            let input = inframe.plane_data(0).map_err(|_| gst::FlowError::Error)?;
            let output = outframe.plane_data_mut(0).map_err(|_| gst::FlowError::Error)?;
            let (isr, osr) = (in_stride as usize, out_stride as usize);
            if out_w == in_w && out_h == in_h {
                let row = in_w * 4;
                for y in 0..in_h {
                    output[y * osr..y * osr + row].copy_from_slice(&input[y * isr..y * isr + row]);
                }
            } else {
                for oy in 0..out_h {
                    let iy = (oy * in_h / out_h).min(in_h - 1);
                    let src_row = &input[iy * isr..iy * isr + in_w * 4];
                    let dst_row = &mut output[oy * osr..oy * osr + out_w * 4];
                    for ox in 0..out_w {
                        let ix = (ox * in_w / out_w).min(in_w - 1);
                        dst_row[ox * 4..ox * 4 + 4].copy_from_slice(&src_row[ix * 4..ix * 4 + 4]);
                    }
                }
            }
            Ok(gst::FlowSuccess::Ok)
        }
    }
}

glib::wrapper! {
    pub struct EarthNvSr(ObjectSubclass<imp::EarthNvSr>)
        @extends gstreamer_video::VideoFilter, gstreamer_base::BaseTransform, gst::Element, gst::Object;
}

fn ensure_registered() -> bool {
    static REG: OnceLock<bool> = OnceLock::new();
    *REG.get_or_init(|| {
        gst::Element::register(None, "earthnvsr", gst::Rank::NONE, EarthNvSr::static_type())
            .map_err(|e| log::warn!("[earth-media] earthnvsr registration failed: {}", e))
            .is_ok()
    })
}

/// Whether ANY AI super-resolution backend can run (Maxine SDK or the open
/// Real-ESRGAN/onnxruntime path).
pub fn ai_available() -> bool {
    available() || crate::aisr::available()
}

/// Human-readable name of the AI model/backend that would drive 'nvai', for
/// the UI's Enhance settings. None when no backend is installed.
pub fn ai_model_label() -> Option<String> {
    if available() {
        return Some("NVIDIA Maxine SuperRes 2x".to_string());
    }
    if crate::aisr::available() {
        return Some(crate::aisr::model_label());
    }
    None
}

/// Engage-time preflight for the active backend, so switching to the AI mode
/// fails fast with a clear message instead of degrading per-frame.
pub fn ai_preflight() -> Result<(), String> {
    if available() {
        return preflight();
    }
    if crate::aisr::available() {
        return crate::aisr::preflight();
    }
    Err("no AI upscaler installed (Real-ESRGAN model + onnxruntime, or the NVIDIA VFX SDK)".into())
}

/// The AI stage for the enhance bin, or None when no backend is installed
/// (the bin then simply omits the stage).
pub fn make_element() -> Option<gst::Element> {
    if !ai_available() || !ensure_registered() {
        return None;
    }
    let e = gst::ElementFactory::make("earthnvsr").build().ok()?;
    e.set_property("engaged", false);
    // QoS is essential here: AI frames can exceed the frame budget, and with
    // QoS off BaseTransform BLOCKS on every late frame — the pipeline stalls
    // into a perceived freeze. With QoS on, late frames drop upstream of the
    // expensive transform and playback stays fluid (fewer, enhanced frames).
    e.set_property("qos", true);
    Some(e)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nvcvimage_layout_matches_c_header() {
        // The C struct is 64 bytes on x86_64 (3x4 + 2x4 + 6x1 + pad2 → 32-align
        // → 3 pointers/u64). A mismatch here would corrupt memory at the FFI
        // boundary, so pin it.
        assert_eq!(std::mem::size_of::<NvCVImage>(), 64);
        assert_eq!(std::mem::align_of::<NvCVImage>(), 8);
        assert_eq!(std::mem::offset_of!(NvCVImage, pixels), 32);
        assert_eq!(std::mem::offset_of!(NvCVImage, buffer_bytes), 56);
    }

    #[test]
    fn nv_factor_respects_cap() {
        assert_eq!(nv_factor(640, 360), 2);
        assert_eq!(nv_factor(1280, 720), 2);
        // >720p sources: AI passes through (GL stage falls back to FSR) —
        // fp32 inference above 720p can't hold a real-time frame budget.
        assert_eq!(nv_factor(1920, 1080), 1);
        assert_eq!(nv_factor(2560, 1440), 1);
        // Below the TensorRT profile floor: 1:1 (GL-FSR covers it) — the
        // engine would hard-fail on sub-profile shapes.
        assert_eq!(nv_factor(24, 18), 1);
        assert_eq!(nv_factor(0, 0), 1);
    }

    /// AI-strength blend: 0 = pure nearest-upscale of the source, 1 → left as
    /// AI output (the caller skips the call), midpoints mix; alpha opaque.
    #[test]
    fn blend_with_nearest_mixes() {
        let (in_w, in_h) = (2usize, 1usize);
        let input = [
            100u8, 20, 40, 255, // src px 0
            200, 60, 80, 255, // src px 1
        ];
        let (out_w, out_h) = (4usize, 2usize);
        // "AI output": all-zero so the blend result is (1-s) * nearest.
        let mut output = vec![0u8; out_w * out_h * 4];
        blend_with_nearest(&input, in_w, in_h, in_w * 4, &mut output, out_w, out_h, out_w * 4, 0.0);
        // strength 0 → exactly the nearest-neighbour doubling.
        assert_eq!(&output[0..4], &[100, 20, 40, 255], "px(0,0) <- src 0");
        assert_eq!(&output[8..12], &[200, 60, 80, 255], "px(2,0) <- src 1");
        assert_eq!(&output[out_w * 4..out_w * 4 + 4], &[100, 20, 40, 255], "row doubled");

        let mut output = vec![0u8; out_w * out_h * 4];
        blend_with_nearest(&input, in_w, in_h, in_w * 4, &mut output, out_w, out_h, out_w * 4, 0.5);
        // strength 0.5 over zero AI output → half the source (integer >>8).
        assert!((output[0] as i32 - 50).abs() <= 1, "half blend, got {}", output[0]);
        assert_eq!(output[3], 255, "alpha stays opaque");
    }

    /// Element sanity without the SDK: registers, negotiates passthrough when
    /// disengaged. (With the SDK absent, make_element() is None by design, so
    /// register the type directly for the test.)
    #[test]
    fn earthnvsr_passthrough_negotiates() {
        gst::init().unwrap();
        assert!(ensure_registered());
        let e = gst::ElementFactory::make("earthnvsr").build().unwrap();
        e.set_property("engaged", false);

        let pipeline = gst::Pipeline::new();
        let src = gst::ElementFactory::make("videotestsrc")
            .property("num-buffers", 3i32)
            .build()
            .unwrap();
        let capsf = gst::ElementFactory::make("capsfilter").build().unwrap();
        capsf.set_property(
            "caps",
            gst::Caps::builder("video/x-raw")
                .field("format", "RGBA")
                .field("width", 320i32)
                .field("height", 240i32)
                .build(),
        );
        let sink = gst::ElementFactory::make("fakesink").build().unwrap();
        pipeline.add_many([&src, &capsf, &e, &sink]).unwrap();
        gst::Element::link_many([&src, &capsf, &e, &sink]).unwrap();
        pipeline.set_state(gst::State::Paused).unwrap();
        let (res, _, _) = pipeline.state(gst::ClockTime::from_seconds(5));
        assert!(res.is_ok(), "earthnvsr passthrough failed to preroll");
        let out = e.static_pad("src").unwrap().current_caps().unwrap();
        let s = out.structure(0).unwrap();
        assert_eq!(s.get::<i32>("width").unwrap(), 320);
        pipeline.set_state(gst::State::Null).unwrap();
    }

    /// ENGAGED element in a real pipeline: needs an AI backend installed
    /// (Real-ESRGAN model + onnxruntime in ~/.earthreclaim/aisr, or Maxine).
    /// Verifies set_info -> engine creation -> transform_frame produce 2x caps.
    #[test]
    #[ignore = "needs an AI backend installed"]
    fn earthnvsr_engaged_doubles_resolution() {
        gst::init().unwrap();
        assert!(ensure_registered());
        assert!(ai_available(), "no AI backend installed");
        let e = gst::ElementFactory::make("earthnvsr").build().unwrap();
        e.set_property("engaged", true);

        let pipeline = gst::Pipeline::new();
        let src = gst::ElementFactory::make("videotestsrc")
            .property("num-buffers", 3i32)
            .build()
            .unwrap();
        let capsf = gst::ElementFactory::make("capsfilter").build().unwrap();
        capsf.set_property(
            "caps",
            gst::Caps::builder("video/x-raw")
                .field("format", "RGBA")
                .field("width", 320i32)
                .field("height", 240i32)
                .build(),
        );
        let sink = gst::ElementFactory::make("fakesink").build().unwrap();
        pipeline.add_many([&src, &capsf, &e, &sink]).unwrap();
        gst::Element::link_many([&src, &capsf, &e, &sink]).unwrap();
        pipeline.set_state(gst::State::Playing).unwrap();
        let bus = pipeline.bus().unwrap();
        let msg = bus.timed_pop_filtered(
            gst::ClockTime::from_seconds(30),
            &[gst::MessageType::Eos, gst::MessageType::Error],
        );
        let got_eos = matches!(msg.as_ref().map(|m| m.type_()), Some(gst::MessageType::Eos));
        let out = e.static_pad("src").unwrap().current_caps().unwrap();
        let s = out.structure(0).unwrap();
        let w = s.get::<i32>("width").unwrap();
        pipeline.set_state(gst::State::Null).unwrap();
        assert!(got_eos, "pipeline errored: {:?}", msg);
        assert_eq!(w, 640, "engaged earthnvsr must negotiate 2x output");
    }
}
