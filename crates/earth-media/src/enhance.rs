//! Video "Enhance" — in-pipeline super resolution.
//!
//! Tier 1: FSR 1.0 (EASU 2x upscale + RCAS sharpen) as GLSL shaders.
//! Tier 2: NVIDIA Maxine SuperRes (`nvsr` module) as an optional AI stage.
//!
//! A single RESIDENT bin sits in playbin's `video-filter` slot for the whole
//! life of the player:
//!
//!   videoconvert ! [earthnvsr] ! glupload ! glcolorconvert ! glshader(A)
//!     ! caps ! glshader(B) ! caps ! glcolorconvert ! gldownload ! videoconvert
//!
//! Mode switching happens INSIDE the bin, live — no pipeline restart (swapping
//! `video-filter` itself requires a NULL round-trip, which blanked playback):
//!   * off  — NvSR passthrough + passthrough shaders at 1:1 (visually identical
//!            to the source; the GL round-trip stays resident).
//!   * fsr  — shaders swapped to EASU/RCAS via glshader's `update-shader`, the
//!            mid-bin capsfilters forced to 2x (renegotiates in-place).
//!   * nvai — the `earthnvsr` element engages (AI 2x on CUDA), GL stage at 1:1.
//!
//! `glshader` renders the input texture onto an output-sized quad, so forcing
//! larger caps after it is what makes it a scaler (verified empirically). The
//! GL stage's sizes come from a caps probe on `glupload`'s sink pad — that pad
//! sees the POST-NvSR resolution, so the GL stage never needs to know whether
//! the AI stage scaled.
//!
//! Output returns to SYSTEM memory (`gldownload`) so the proven `xvimagesink`
//! presentation path is untouched — we never GL-render to the reparented X11
//! surface (glimagesink hard-crashes on NVIDIA there; see `build_video_sink`).
//! Because a broken GL stack would otherwise take ALL playback down with the
//! resident bin, `gl_available()` proves GL works with a one-shot throwaway
//! pipeline before the bin is ever installed; failure means "enhance
//! unavailable", never broken playback.
//!
//! Kill switch: EARTH_VIDEO_SR=off (no GL elements are created at all).

use gstreamer as gst;
use gstreamer::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};

use crate::MediaError;

/// Upscale factor and output ceiling. 2x covers the 360p–1080p content SR is
/// for; the 4K cap bounds GPU cost (larger frames pass through at 1:1).
const SCALE: f64 = 2.0;
const MAX_W: i32 = 3840;
const MAX_H: i32 = 2160;

/// User-tunable Enhance parameters (see `EnhanceCtl::set_settings`). Both are
/// LIVE-safe: sharpness is a shader uniform (no caps change, no recompile) and
/// AI strength is a plain element property read per frame — neither ever
/// triggers renegotiation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EnhanceSettings {
    /// RCAS sharpening in stops, 0.0 (maximum) ..= 2.0 (softest). The RCAS
    /// uniform is exp2(-stops); 0.2 stops is FSR's commonly-shipped default —
    /// visibly sharper without halos.
    pub fsr_sharpness: f32,
    /// AI blend strength 0.0 ..= 1.0: the NvSR output mixed against a plain
    /// nearest-neighbour upscale of the source (1.0 = pure AI).
    pub ai_strength: f32,
}

impl Default for EnhanceSettings {
    fn default() -> Self {
        Self { fsr_sharpness: 0.2, ai_strength: 1.0 }
    }
}

impl EnhanceSettings {
    pub fn clamped(self) -> Self {
        Self {
            fsr_sharpness: if self.fsr_sharpness.is_finite() { self.fsr_sharpness.clamp(0.0, 2.0) } else { 0.2 },
            ai_strength: if self.ai_strength.is_finite() { self.ai_strength.clamp(0.0, 1.0) } else { 1.0 },
        }
    }

    /// The RCAS `u_sharpness` uniform for this setting.
    fn rcas_uniform(&self) -> f32 {
        (-self.fsr_sharpness).exp2()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EnhanceMode {
    /// No enhancement (bin stays resident but passes frames through 1:1).
    Off,
    /// FSR 1.0 (EASU 2x upscale + RCAS sharpen) on the GPU via GL shaders.
    Fsr,
    /// NVIDIA Maxine SuperRes AI upscaling (RTX GPUs, needs the VFX SDK runtime).
    NvAi,
}

impl EnhanceMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "off" => Some(Self::Off),
            "fsr" => Some(Self::Fsr),
            "nvai" => Some(Self::NvAi),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Fsr => "fsr",
            Self::NvAi => "nvai",
        }
    }
}

/// Session-wide default, inherited by players created AFTER a toggle so new
/// panes match the user's last choice (per-profile persistence can layer on
/// later; the browser webview is incognito so localStorage can't hold this).
static DEFAULT_ENHANCE: AtomicU8 = AtomicU8::new(0);

pub fn default_enhance() -> EnhanceMode {
    match DEFAULT_ENHANCE.load(Ordering::Relaxed) {
        1 => EnhanceMode::Fsr,
        2 => EnhanceMode::NvAi,
        _ => EnhanceMode::Off,
    }
}

pub fn set_default_enhance(mode: EnhanceMode) {
    let v = match mode {
        EnhanceMode::Off => 0,
        EnhanceMode::Fsr => 1,
        EnhanceMode::NvAi => 2,
    };
    DEFAULT_ENHANCE.store(v, Ordering::Relaxed);
}

/// Session-wide default Enhance settings, same lifecycle as DEFAULT_ENHANCE
/// (new panes inherit; incognito webview can't persist them). Stored in
/// milli-units so plain integer atomics carry the floats.
static DEFAULT_SHARPNESS_MILLI: AtomicU32 = AtomicU32::new(200); // 0.2 stops
static DEFAULT_AI_STRENGTH_MILLI: AtomicU32 = AtomicU32::new(1000); // 1.0

pub fn default_enhance_settings() -> EnhanceSettings {
    EnhanceSettings {
        fsr_sharpness: DEFAULT_SHARPNESS_MILLI.load(Ordering::Relaxed) as f32 / 1000.0,
        ai_strength: DEFAULT_AI_STRENGTH_MILLI.load(Ordering::Relaxed) as f32 / 1000.0,
    }
    .clamped()
}

pub fn set_default_enhance_settings(s: EnhanceSettings) {
    let s = s.clamped();
    DEFAULT_SHARPNESS_MILLI.store((s.fsr_sharpness * 1000.0).round() as u32, Ordering::Relaxed);
    DEFAULT_AI_STRENGTH_MILLI.store((s.ai_strength * 1000.0).round() as u32, Ordering::Relaxed);
}

/// EARTH_VIDEO_SR=off disables enhancement entirely (isolation/debug hatch,
/// mirrors EARTH_NO_NVDEC / EARTH_VIDEO_SINK).
pub fn sr_env_disabled() -> bool {
    std::env::var("EARTH_VIDEO_SR").map(|v| v == "off" || v == "0").unwrap_or(false)
}

/// One-shot GL viability probe: run a single frame through
/// videotestsrc ! glupload ! glshader(passthrough) ! gldownload ! fakesink.
/// The resident enhance bin is only installed when this succeeds, so a machine
/// with a broken GL stack keeps plain, working playback (enhance unavailable).
pub fn gl_available() -> bool {
    static PROBE: OnceLock<bool> = OnceLock::new();
    *PROBE.get_or_init(|| {
        if sr_env_disabled() {
            return false;
        }
        let ok = (|| -> Option<bool> {
            let pipeline = gst::Pipeline::new();
            let src = gst::ElementFactory::make("videotestsrc")
                .property("num-buffers", 1i32)
                .build()
                .ok()?;
            let upload = gst::ElementFactory::make("glupload").build().ok()?;
            let convert = gst::ElementFactory::make("glcolorconvert").build().ok()?;
            let shader = gst::ElementFactory::make("glshader").build().ok()?;
            shader.set_property("fragment", PASSTHROUGH_FRAGMENT);
            let download = gst::ElementFactory::make("gldownload").build().ok()?;
            let sink = gst::ElementFactory::make("fakesink").build().ok()?;
            pipeline
                .add_many([&src, &upload, &convert, &shader, &download, &sink])
                .ok()?;
            gst::Element::link_many([&src, &upload, &convert, &shader, &download, &sink]).ok()?;
            pipeline.set_state(gst::State::Playing).ok()?;
            let bus = pipeline.bus()?;
            let msg = bus.timed_pop_filtered(
                gst::ClockTime::from_seconds(5),
                &[gst::MessageType::Eos, gst::MessageType::Error],
            );
            let ok = matches!(msg.map(|m| m.type_()), Some(gst::MessageType::Eos));
            let _ = pipeline.set_state(gst::State::Null);
            Some(ok)
        })()
        .unwrap_or(false);
        if ok {
            log::info!("[earth-media] enhance: GL probe OK — resident enhance bin enabled");
        } else {
            log::warn!("[earth-media] enhance: GL probe FAILED — enhance unavailable");
        }
        ok
    })
}

// ---------------------------------------------------------------------------
// Shaders (GLES2-compatible GLSL, GStreamer glshader conventions: `v_texcoord`
// + `tex`). Sizes arrive as uniforms from the caps probe. Verified to
// compile+run on NVIDIA desktop GL.
// ---------------------------------------------------------------------------

const PASSTHROUGH_FRAGMENT: &str = r#"
#ifdef GL_ES
precision mediump float;
#endif
varying vec2 v_texcoord;
uniform sampler2D tex;
void main() { gl_FragColor = texture2D(tex, v_texcoord); }
"#;

// FSR 1.0 EASU, 12-tap non-gather port.
const EASU_FRAGMENT: &str = r#"
#ifdef GL_ES
precision highp float;
#endif
varying vec2 v_texcoord;
uniform sampler2D tex;
uniform float u_src_w;
uniform float u_src_h;
uniform float u_dst_w;
uniform float u_dst_h;

vec3 srcTex(vec2 p) { return texture2D(tex, p).rgb; }

void easuSet(
    inout vec2 dir, inout float len, vec2 pp,
    bool biS, bool biT, bool biU, bool biV,
    float lA, float lB, float lC, float lD, float lE)
{
    float w = 0.0;
    if (biS) w = (1.0 - pp.x) * (1.0 - pp.y);
    if (biT) w =        pp.x  * (1.0 - pp.y);
    if (biU) w = (1.0 - pp.x) *        pp.y;
    if (biV) w =        pp.x  *        pp.y;
    float dc = lD - lC;
    float cb = lC - lB;
    float lenX = max(abs(dc), abs(cb));
    lenX = 1.0 / max(lenX, 1.0 / 32768.0);
    float dirX = lD - lB;
    dir.x += dirX * w;
    lenX = clamp(abs(dirX) * lenX, 0.0, 1.0);
    lenX *= lenX;
    len += lenX * w;
    float ec = lE - lC;
    float ca = lC - lA;
    float lenY = max(abs(ec), abs(ca));
    lenY = 1.0 / max(lenY, 1.0 / 32768.0);
    float dirY = lE - lA;
    dir.y += dirY * w;
    lenY = clamp(abs(dirY) * lenY, 0.0, 1.0);
    lenY *= lenY;
    len += lenY * w;
}

void easuTap(
    inout vec3 aC, inout float aW, vec2 off, vec2 dir, vec2 len,
    float lob, float clp, vec3 c)
{
    vec2 v = vec2(dot(off, dir), dot(off, vec2(-dir.y, dir.x)));
    v *= len;
    float d2 = min(dot(v, v), clp);
    float wB = 0.4 * d2 - 1.0;
    float wA = lob * d2 - 1.0;
    wB *= wB;
    wA *= wA;
    wB = 1.5625 * wB - 0.5625;
    float w = wB * wA;
    aC += c * w;
    aW += w;
}

void main() {
    vec2 srcSize = vec2(u_src_w, u_src_h);
    vec2 dstSize = vec2(u_dst_w, u_dst_h);
    vec4 con0 = vec4(srcSize / dstSize, 0.5 * srcSize / dstSize - 0.5);
    vec4 con1 = vec4(1.0, 1.0, 1.0, -1.0) / srcSize.xyxy;
    vec4 con2 = vec4(-1.0, 2.0, 1.0, 2.0) / srcSize.xyxy;
    vec4 con3 = vec4(0.0, 4.0, 0.0, 0.0) / srcSize.xyxy;

    vec2 ip = floor(v_texcoord * dstSize);
    vec2 pp = ip * con0.xy + con0.zw;
    vec2 fp = floor(pp);
    pp -= fp;
    vec2 p0 = fp * con1.xy + con1.zw;
    vec2 p1 = p0 + con2.xy;
    vec2 p2 = p0 + con2.zw;
    vec2 p3 = p0 + con3.xy;
    vec4 off = vec4(-0.5, 0.5, -0.5, 0.5) * con1.xxyy;

    vec3 bC = srcTex(p0 + off.xw); float bL = bC.g + 0.5 * (bC.r + bC.b);
    vec3 cC = srcTex(p0 + off.yw); float cL = cC.g + 0.5 * (cC.r + cC.b);
    vec3 iC = srcTex(p1 + off.xw); float iL = iC.g + 0.5 * (iC.r + iC.b);
    vec3 jC = srcTex(p1 + off.yw); float jL = jC.g + 0.5 * (jC.r + jC.b);
    vec3 fC = srcTex(p1 + off.yz); float fL = fC.g + 0.5 * (fC.r + fC.b);
    vec3 eC = srcTex(p1 + off.xz); float eL = eC.g + 0.5 * (eC.r + eC.b);
    vec3 kC = srcTex(p2 + off.xw); float kL = kC.g + 0.5 * (kC.r + kC.b);
    vec3 lC = srcTex(p2 + off.yw); float lL = lC.g + 0.5 * (lC.r + lC.b);
    vec3 hC = srcTex(p2 + off.yz); float hL = hC.g + 0.5 * (hC.r + hC.b);
    vec3 gC = srcTex(p2 + off.xz); float gL = gC.g + 0.5 * (gC.r + gC.b);
    vec3 oC = srcTex(p3 + off.yz); float oL = oC.g + 0.5 * (oC.r + oC.b);
    vec3 nC = srcTex(p3 + off.xz); float nL = nC.g + 0.5 * (nC.r + nC.b);

    vec2 dir = vec2(0.0);
    float len = 0.0;
    easuSet(dir, len, pp, true, false, false, false, bL, eL, fL, gL, jL);
    easuSet(dir, len, pp, false, true, false, false, cL, fL, gL, hL, kL);
    easuSet(dir, len, pp, false, false, true, false, fL, iL, jL, kL, nL);
    easuSet(dir, len, pp, false, false, false, true, gL, jL, kL, lL, oL);

    vec2 dir2 = dir * dir;
    float dirR = dir2.x + dir2.y;
    bool zro = dirR < (1.0 / 32768.0);
    dirR = inversesqrt(max(dirR, 1.0 / 32768.0));
    dirR = zro ? 1.0 : dirR;
    dir.x = zro ? 1.0 : dir.x;
    dir *= vec2(dirR);
    len = len * 0.5;
    len *= len;
    float stretch = dot(dir, dir) / max(max(abs(dir.x), abs(dir.y)), 1.0 / 32768.0);
    vec2 len2 = vec2(1.0 + (stretch - 1.0) * len, 1.0 - 0.5 * len);
    float lob = 0.5 + ((1.0 / 4.0 - 0.04) - 0.5) * len;
    float clp = 1.0 / max(lob, 1.0 / 32768.0);

    vec3 min4 = min(min(fC, gC), min(jC, kC));
    vec3 max4 = max(max(fC, gC), max(jC, kC));
    vec3 aC = vec3(0.0);
    float aW = 0.0;
    easuTap(aC, aW, vec2( 0.0, -1.0) - pp, dir, len2, lob, clp, bC);
    easuTap(aC, aW, vec2( 1.0, -1.0) - pp, dir, len2, lob, clp, cC);
    easuTap(aC, aW, vec2(-1.0,  1.0) - pp, dir, len2, lob, clp, iC);
    easuTap(aC, aW, vec2( 0.0,  1.0) - pp, dir, len2, lob, clp, jC);
    easuTap(aC, aW, vec2( 0.0,  0.0) - pp, dir, len2, lob, clp, fC);
    easuTap(aC, aW, vec2(-1.0,  0.0) - pp, dir, len2, lob, clp, eC);
    easuTap(aC, aW, vec2( 1.0,  1.0) - pp, dir, len2, lob, clp, kC);
    easuTap(aC, aW, vec2( 2.0,  1.0) - pp, dir, len2, lob, clp, lC);
    easuTap(aC, aW, vec2( 2.0,  0.0) - pp, dir, len2, lob, clp, hC);
    easuTap(aC, aW, vec2( 1.0,  0.0) - pp, dir, len2, lob, clp, gC);
    easuTap(aC, aW, vec2( 1.0,  2.0) - pp, dir, len2, lob, clp, oC);
    easuTap(aC, aW, vec2( 0.0,  2.0) - pp, dir, len2, lob, clp, nC);

    vec3 pix = min(max4, max(min4, aC / max(aW, 1.0 / 32768.0)));
    gl_FragColor = vec4(pix, 1.0);
}
"#;

// FSR 1.0 RCAS (robust contrast-adaptive sharpening), same-size pass at the
// output resolution. Denoise variant omitted.
const RCAS_FRAGMENT: &str = r#"
#ifdef GL_ES
precision highp float;
#endif
varying vec2 v_texcoord;
uniform sampler2D tex;
uniform float u_dst_w;
uniform float u_dst_h;
uniform float u_sharpness;

void main() {
    vec2 px = 1.0 / vec2(u_dst_w, u_dst_h);
    vec3 b = texture2D(tex, v_texcoord + vec2( 0.0, -1.0) * px).rgb;
    vec3 d = texture2D(tex, v_texcoord + vec2(-1.0,  0.0) * px).rgb;
    vec3 e = texture2D(tex, v_texcoord).rgb;
    vec3 f = texture2D(tex, v_texcoord + vec2( 1.0,  0.0) * px).rgb;
    vec3 h = texture2D(tex, v_texcoord + vec2( 0.0,  1.0) * px).rgb;

    vec3 mn4 = min(min(b, d), min(f, h));
    vec3 mx4 = max(max(b, d), max(f, h));
    vec2 peakC = vec2(1.0, -4.0);
    vec3 hitMin = mn4 / max(4.0 * mx4, vec3(1.0 / 32768.0));
    vec3 hitMax = (peakC.x - mx4) / (4.0 * mn4 + peakC.y);
    vec3 lobeRGB = max(-hitMin, hitMax);
    float lobe = max(-0.1875, min(max(lobeRGB.r, max(lobeRGB.g, lobeRGB.b)), 0.0)) * u_sharpness;
    float rcpL = 1.0 / (4.0 * lobe + 1.0);
    vec3 c = (lobe * (b + d + f + h) + e) * rcpL;
    gl_FragColor = vec4(c, 1.0);
}
"#;

// ---------------------------------------------------------------------------
// Resident bin + live controls
// ---------------------------------------------------------------------------

/// Output size for a given input: uniform 2x, capped at 4K, even dimensions.
/// Inputs already at/above the ceiling get 1:1 (EASU passes through; RCAS still
/// sharpens). Returns None for degenerate inputs.
fn output_size(w: i32, h: i32) -> Option<(i32, i32)> {
    if w <= 0 || h <= 0 {
        return None;
    }
    let f = SCALE
        .min(MAX_W as f64 / w as f64)
        .min(MAX_H as f64 / h as f64)
        .max(1.0);
    let even = |v: f64| ((v as i32) / 2 * 2).max(2);
    Some((even(w as f64 * f), even(h as f64 * f)))
}

fn make(name: &str) -> Result<gst::Element, MediaError> {
    gst::ElementFactory::make(name)
        .build()
        .map_err(|_| MediaError::PlayerError(format!("enhance: '{}' element unavailable", name)))
}

/// Uniforms for one shader stage, as glshader's `uniforms` GstStructure.
fn shader_uniforms(pairs: &[(&str, f32)]) -> gst::Structure {
    let mut s = gst::Structure::builder("uniforms");
    for (k, v) in pairs {
        s = s.field(*k, *v);
    }
    s.build()
}

struct CtlState {
    mode: EnhanceMode,
    /// Tunables (sharpness/strength). NOT part of `applied` — settings changes
    /// go straight to uniforms/properties in set_settings and never touch caps.
    settings: EnhanceSettings,
    /// Last input caps seen at the GL stage (post-NvSR): width, height, PAR.
    last_in: Option<(i32, i32, Option<gst::Fraction>)>,
    /// What apply() last wrote to the GL stage: (fsr_shaders, out_w, out_h, par).
    /// apply() is a strict no-op when this wouldn't change — re-setting even
    /// IDENTICAL capsfilter caps mid-play churns renegotiation, and that took
    /// down live pipelines (qtdemux "not-negotiated") on no-op mode switches.
    applied: Option<(bool, i32, i32, Option<gst::Fraction>)>,
}

/// Live controls for the resident enhance bin. Held by the player; the caps
/// probe only holds a Weak so the bin/probe/ctl reference cycle breaks when
/// the player is dropped.
pub struct EnhanceCtl {
    easu: gst::Element,
    rcas: gst::Element,
    caps_scale: gst::Element,
    caps_out: gst::Element,
    /// The Maxine AI stage, present only when the SDK runtime was found.
    nvsr: Option<gst::Element>,
    state: Mutex<CtlState>,
}

impl EnhanceCtl {
    /// Whether NVIDIA AI SR is usable in this bin.
    pub fn nvai_available(&self) -> bool {
        self.nvsr.is_some()
    }

    /// Switch mode LIVE — no pipeline restart. Shader programs swap via
    /// glshader's `update-shader` (recompiled for the next frame) and the mid-
    /// bin capsfilters renegotiate in place.
    pub fn set_mode(&self, mode: EnhanceMode) -> Result<(), MediaError> {
        if mode == EnhanceMode::NvAi {
            if self.nvsr.is_none() {
                return Err(MediaError::PlayerError(
                    "AI upscaling is not available (install the Real-ESRGAN model + onnxruntime, \
                     or the NVIDIA VFX SDK)".to_string(),
                ));
            }
            // Fail the SWITCH fast (clear error, UI reverts) rather than
            // degrading per-frame mid-playback.
            crate::nvsr::ai_preflight().map_err(MediaError::PlayerError)?;
        }
        {
            let mut st = self
                .state
                .lock()
                .map_err(|e| MediaError::PlayerError(format!("enhance state poisoned: {}", e)))?;
            if st.mode == mode {
                return Ok(());
            }
            st.mode = mode;
        }
        if let Some(nvsr) = &self.nvsr {
            nvsr.set_property("engaged", mode == EnhanceMode::NvAi);
        }
        self.apply();
        log::info!("[earth-media] enhance mode -> '{}' (live)", mode.as_str());
        Ok(())
    }

    pub fn mode(&self) -> EnhanceMode {
        self.state.lock().map(|s| s.mode).unwrap_or(EnhanceMode::Off)
    }

    pub fn settings(&self) -> EnhanceSettings {
        self.state.lock().map(|s| s.settings).unwrap_or_default()
    }

    /// Tune Enhance LIVE. Deliberately renegotiation-free (see the stability
    /// rules): sharpness lands as a glshader UNIFORM (picked up next frame, no
    /// `update-shader`, no caps change) and AI strength as a plain property on
    /// the NvSR element (read per frame in transform). Strict no-op when the
    /// values don't change.
    pub fn set_settings(&self, settings: EnhanceSettings) -> Result<(), MediaError> {
        let settings = settings.clamped();
        let applied = {
            let mut st = self
                .state
                .lock()
                .map_err(|e| MediaError::PlayerError(format!("enhance state poisoned: {}", e)))?;
            if st.settings == settings {
                return Ok(());
            }
            st.settings = settings;
            st.applied
        };
        if let Some(nvsr) = &self.nvsr {
            nvsr.set_property("strength", settings.ai_strength as f64);
        }
        // Refresh the RCAS uniforms only when FSR shaders are live; otherwise
        // apply() writes them (with these settings) whenever FSR next engages.
        if let Some((true, ow, oh, _)) = applied {
            self.rcas.set_property(
                "uniforms",
                shader_uniforms(&[
                    ("u_dst_w", ow as f32),
                    ("u_dst_h", oh as f32),
                    ("u_sharpness", settings.rcas_uniform()),
                ]),
            );
        }
        log::info!(
            "[earth-media] enhance settings -> sharpness {:.2} stops, AI strength {:.0}% (live)",
            settings.fsr_sharpness,
            settings.ai_strength * 100.0
        );
        Ok(())
    }

    /// (Re)apply shaders, caps and uniforms for the current mode + last-seen
    /// input size. Called on mode changes and from the caps probe. Strictly a
    /// no-op when nothing would change (see CtlState::applied).
    fn apply(&self) {
        let (mode, last_in, settings) = match self.state.lock() {
            Ok(s) => (s.mode, s.last_in, s.settings),
            Err(_) => return,
        };
        let Some((w, h, par)) = last_in else { return };

        // What the GL stage should do. FSR scales here. NvAi normally scales
        // UPSTREAM (this stage sees the post-NvSR size via its caps probe) — but
        // the AI element only engages for <=720p sources (fp32 real-time
        // budget); for bigger inputs it stays 1:1 and the GL stage steps in
        // with FSR so "AI mode" degrades to FSR instead of crawling.
        let gl_fsr = match mode {
            EnhanceMode::Fsr => true,
            EnhanceMode::Off => false,
            EnhanceMode::NvAi => {
                let ai_scaled = self
                    .nvsr
                    .as_ref()
                    .and_then(|n| n.static_pad("sink"))
                    .and_then(|p| p.current_caps())
                    .and_then(|c| c.structure(0).map(|s| s.to_owned()))
                    .and_then(|s| {
                        let iw = s.get::<i32>("width").ok()?;
                        let ih = s.get::<i32>("height").ok()?;
                        Some(crate::nvsr::nv_factor(iw, ih) == 2)
                    })
                    .unwrap_or(false);
                !ai_scaled
            }
        };
        let (ow, oh) = if gl_fsr { output_size(w, h).unwrap_or((w, h)) } else { (0, 0) };

        // No-op guard: don't touch live elements unless something changes.
        // (ow, oh) = (0, 0) encodes "unconstrained".
        {
            let mut st = match self.state.lock() {
                Ok(s) => s,
                Err(_) => return,
            };
            let next = (gl_fsr, ow, oh, par);
            if st.applied == Some(next) {
                return;
            }
            st.applied = Some(next);
        }

        // Pin the mid-bin caps ONLY when the GL stage itself scales (FSR) —
        // that pin is what forces EASU's 2x. In Off/NvAi the filters must stay
        // UNCONSTRAINED: pinning them to sizes computed from stale pre-NvSR
        // dimensions made negotiation lock onto the old size when the AI stage
        // scaled (glshader can downscale, so the stale pin "won"), leaving the
        // AI output stuck at 1x.
        let out = if gl_fsr {
            let mut out = gst::Caps::builder("video/x-raw")
                .features(["memory:GLMemory"])
                .field("format", "RGBA")
                .field("width", ow)
                .field("height", oh);
            // Uniform scale keeps the pixel aspect; carry it through explicitly
            // so fixation can't quietly square anamorphic content.
            if let Some(par) = par {
                out = out.field("pixel-aspect-ratio", par);
            }
            out.build()
        } else {
            gst::Caps::builder("video/x-raw")
                .features(["memory:GLMemory"])
                .field("format", "RGBA")
                .build()
        };
        self.caps_scale.set_property("caps", &out);
        self.caps_out.set_property("caps", &out);

        if gl_fsr {
            self.easu.set_property("fragment", EASU_FRAGMENT);
            self.easu.set_property(
                "uniforms",
                shader_uniforms(&[
                    ("u_src_w", w as f32),
                    ("u_src_h", h as f32),
                    ("u_dst_w", ow as f32),
                    ("u_dst_h", oh as f32),
                ]),
            );
            self.rcas.set_property("fragment", RCAS_FRAGMENT);
            self.rcas.set_property(
                "uniforms",
                shader_uniforms(&[
                    ("u_dst_w", ow as f32),
                    ("u_dst_h", oh as f32),
                    ("u_sharpness", settings.rcas_uniform()),
                ]),
            );
            log::info!("[earth-media] enhance: FSR {}x{} -> {}x{}", w, h, ow, oh);
        } else {
            self.easu.set_property("fragment", PASSTHROUGH_FRAGMENT);
            self.rcas.set_property("fragment", PASSTHROUGH_FRAGMENT);
        }
        // Recompile both programs on the next frame.
        self.easu.set_property("update-shader", true);
        self.rcas.set_property("update-shader", true);
    }
}

/// Build the resident enhance bin. Returns the bin (for playbin's
/// `video-filter`) plus its live controls. The caller decides the initial mode.
pub fn build_enhance_bin(initial: EnhanceMode) -> Result<(gst::Element, Arc<EnhanceCtl>), MediaError> {
    let bin = gst::Bin::builder().name("earth-enhance").build();

    // System-memory-only gate at the bin entry: NVDEC otherwise proposes
    // video/x-raw(memory:CUDAMemory) during MID-PLAY renegotiation (mode
    // toggles), which videoconvert can't accept — the chain ends up
    // not-negotiated and qtdemux stops streaming (the reported freeze).
    let caps_entry = make("capsfilter")?;
    caps_entry.set_property("caps", gst::Caps::new_empty_simple("video/x-raw"));
    // Leading/trailing videoconvert: passthrough when caps already fit, and
    // they make negotiation succeed for decoder formats glupload can't take.
    let convert_pre = make("videoconvert")?;
    let upload = make("glupload")?;
    let convert_in = make("glcolorconvert")?;
    let easu = make("glshader")?;
    let caps_scale = make("capsfilter")?;
    let rcas = make("glshader")?;
    let caps_out = make("capsfilter")?;
    let convert_gl_out = make("glcolorconvert")?;
    let download = make("gldownload")?;
    let convert_post = make("videoconvert")?;

    easu.set_property("fragment", PASSTHROUGH_FRAGMENT);
    rcas.set_property("fragment", PASSTHROUGH_FRAGMENT);

    // Unrestricted until the caps probe learns the stream's real size.
    let any_gl = gst::Caps::builder("video/x-raw")
        .features(["memory:GLMemory"])
        .field("format", "RGBA")
        .build();
    caps_scale.set_property("caps", &any_gl);
    caps_out.set_property("caps", &any_gl);

    // Optional Tier-2 AI stage (only when the Maxine runtime is present).
    let nvsr = crate::nvsr::make_element();

    let mut chain: Vec<&gst::Element> = vec![&caps_entry, &convert_pre];
    if let Some(n) = &nvsr {
        chain.push(n);
    }
    chain.extend([
        &upload, &convert_in, &easu, &caps_scale, &rcas, &caps_out, &convert_gl_out, &download,
        &convert_post,
    ]);

    bin.add_many(chain.iter().copied())
        .map_err(|e| MediaError::PlayerError(format!("enhance: bin add failed: {}", e)))?;
    gst::Element::link_many(chain.iter().copied())
        .map_err(|e| MediaError::PlayerError(format!("enhance: bin link failed: {}", e)))?;

    let sink_target = caps_entry
        .static_pad("sink")
        .ok_or_else(|| MediaError::PlayerError("enhance: no sink pad".into()))?;
    let src_target = convert_post
        .static_pad("src")
        .ok_or_else(|| MediaError::PlayerError("enhance: no src pad".into()))?;
    let sink_pad = gst::GhostPad::builder_with_target(&sink_target)
        .map_err(|e| MediaError::PlayerError(format!("enhance: ghost sink failed: {}", e)))?
        .name("sink")
        .build();
    let src_pad = gst::GhostPad::builder_with_target(&src_target)
        .map_err(|e| MediaError::PlayerError(format!("enhance: ghost src failed: {}", e)))?
        .name("src")
        .build();
    bin.add_pad(&sink_pad)
        .map_err(|e| MediaError::PlayerError(format!("enhance: add sink pad failed: {}", e)))?;
    bin.add_pad(&src_pad)
        .map_err(|e| MediaError::PlayerError(format!("enhance: add src pad failed: {}", e)))?;

    // New panes inherit the session's tunables (like the mode default below).
    let settings = default_enhance_settings();
    if let Some(n) = &nvsr {
        n.set_property("strength", settings.ai_strength as f64);
    }

    let ctl = Arc::new(EnhanceCtl {
        easu,
        rcas,
        caps_scale,
        caps_out,
        nvsr: nvsr.clone(),
        state: Mutex::new(CtlState { mode: EnhanceMode::Off, settings, last_in: None, applied: None }),
    });

    // Track the GL stage's input size from the caps flowing into glupload —
    // that pad sees the POST-NvSR resolution, so FSR sizing and the AI stage
    // compose without knowing about each other. Weak ref breaks the
    // pad→probe→ctl→element cycle when the player drops.
    let weak: Weak<EnhanceCtl> = Arc::downgrade(&ctl);
    let upload_sink = upload
        .static_pad("sink")
        .ok_or_else(|| MediaError::PlayerError("enhance: glupload has no sink pad".into()))?;
    upload_sink.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |_pad, info| {
        let Some(ctl) = weak.upgrade() else { return gst::PadProbeReturn::Ok };
        let Some(ev) = info.event() else { return gst::PadProbeReturn::Ok };
        let gst::EventView::Caps(caps_ev) = ev.view() else { return gst::PadProbeReturn::Ok };
        let caps = caps_ev.caps();
        let Some(s) = caps.structure(0) else { return gst::PadProbeReturn::Ok };
        let (Ok(w), Ok(h)) = (s.get::<i32>("width"), s.get::<i32>("height")) else {
            return gst::PadProbeReturn::Ok;
        };
        let par = s.get::<gst::Fraction>("pixel-aspect-ratio").ok();
        if let Ok(mut st) = ctl.state.lock() {
            st.last_in = Some((w, h, par));
        }
        ctl.apply();
        gst::PadProbeReturn::Ok
    });

    // Initial mode (errors here mean "requested backend unavailable"; the bin
    // itself is fine, so fall back to Off rather than failing the build).
    if initial != EnhanceMode::Off {
        if let Err(e) = ctl.set_mode(initial) {
            log::warn!("[earth-media] enhance: initial mode '{}' unavailable: {}", initial.as_str(), e);
        }
    }

    Ok((bin.upcast(), ctl))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_size_scales_and_caps() {
        assert_eq!(output_size(640, 360), Some((1280, 720)));
        assert_eq!(output_size(1920, 1080), Some((3840, 2160)));
        // Above the ceiling → 1:1 passthrough (RCAS-only), never downscale.
        assert_eq!(output_size(3840, 2160), Some((3840, 2160)));
        // Cap binds on the larger axis; scale stays uniform.
        assert_eq!(output_size(2560, 1440), Some((3840, 2160)));
        assert_eq!(output_size(0, 720), None);
    }

    #[test]
    fn enhance_settings_clamp_and_defaults() {
        let d = EnhanceSettings::default();
        assert_eq!(d.fsr_sharpness, 0.2);
        assert_eq!(d.ai_strength, 1.0);
        let c = EnhanceSettings { fsr_sharpness: 9.0, ai_strength: -3.0 }.clamped();
        assert_eq!(c.fsr_sharpness, 2.0);
        assert_eq!(c.ai_strength, 0.0);
        let n = EnhanceSettings { fsr_sharpness: f32::NAN, ai_strength: f32::INFINITY }.clamped();
        assert_eq!(n.fsr_sharpness, 0.2);
        assert_eq!(n.ai_strength, 1.0);
        // RCAS uniform: 0 stops = 1.0 (max), 1 stop = 0.5.
        assert!((EnhanceSettings { fsr_sharpness: 0.0, ai_strength: 1.0 }.rcas_uniform() - 1.0).abs() < 1e-6);
        assert!((EnhanceSettings { fsr_sharpness: 1.0, ai_strength: 1.0 }.rcas_uniform() - 0.5).abs() < 1e-6);
        // Session-default roundtrip (milli-unit atomics).
        set_default_enhance_settings(EnhanceSettings { fsr_sharpness: 0.55, ai_strength: 0.7 });
        let r = default_enhance_settings();
        assert!((r.fsr_sharpness - 0.55).abs() < 1e-3 && (r.ai_strength - 0.7).abs() < 1e-3);
        set_default_enhance_settings(EnhanceSettings::default()); // restore for other tests
    }

    #[test]
    fn enhance_mode_parses() {
        assert_eq!(EnhanceMode::parse("fsr"), Some(EnhanceMode::Fsr));
        assert_eq!(EnhanceMode::parse("OFF"), Some(EnhanceMode::Off));
        assert_eq!(EnhanceMode::parse("nvai"), Some(EnhanceMode::NvAi));
        assert_eq!(EnhanceMode::parse("dlss"), None);
    }

    /// Full live-toggle test: preroll OFF at 1:1, switch to FSR mid-stream and
    /// verify the output caps double WITHOUT the pipeline leaving PLAYING.
    /// Needs a GL-capable display — run locally with `cargo test -- --ignored`.
    #[test]
    #[ignore = "needs a display + GL context"]
    fn enhance_bin_toggles_live_without_restart() {
        gst::init().unwrap();
        let pipeline = gst::Pipeline::new();
        let src = gst::ElementFactory::make("videotestsrc")
            .property("num-buffers", 300i32)
            .property("is-live", true)
            .build()
            .unwrap();
        let incaps = gst::ElementFactory::make("capsfilter").build().unwrap();
        incaps.set_property(
            "caps",
            gst::Caps::builder("video/x-raw")
                .field("width", 320i32)
                .field("height", 240i32)
                .field("framerate", gst::Fraction::new(30, 1))
                .build(),
        );
        let (enhance, ctl) = build_enhance_bin(EnhanceMode::Off).unwrap();
        let convert = gst::ElementFactory::make("videoconvert").build().unwrap();
        let sink = gst::ElementFactory::make("fakesink")
            .property("sync", false)
            .build()
            .unwrap();
        pipeline.add_many([&src, &incaps, &enhance, &convert, &sink]).unwrap();
        gst::Element::link_many([&src, &incaps, &enhance, &convert, &sink]).unwrap();

        pipeline.set_state(gst::State::Playing).unwrap();
        let (res, _, _) = pipeline.state(gst::ClockTime::from_seconds(10));
        assert!(res.is_ok(), "enhance pipeline failed to start (Off/passthrough)");

        let out_pad = enhance.static_pad("src").unwrap();
        let caps_size = |pad: &gst::Pad| -> Option<(i32, i32)> {
            let c = pad.current_caps()?;
            let s = c.structure(0)?;
            Some((s.get("width").ok()?, s.get("height").ok()?))
        };
        // Wait for initial negotiation at 1:1.
        for _ in 0..50 {
            if caps_size(&out_pad).is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        assert_eq!(caps_size(&out_pad), Some((320, 240)), "Off mode must be 1:1");

        // LIVE switch to FSR — no state change on the pipeline.
        ctl.set_mode(EnhanceMode::Fsr).unwrap();
        let mut scaled = None;
        for _ in 0..50 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            if caps_size(&out_pad) == Some((640, 480)) {
                scaled = Some(true);
                break;
            }
        }
        let (_, state, _) = pipeline.state(gst::ClockTime::ZERO);
        assert_eq!(state, gst::State::Playing, "pipeline must stay PLAYING across the toggle");
        assert_eq!(scaled, Some(true), "FSR must renegotiate to 2x live");

        // And back off again, still live.
        ctl.set_mode(EnhanceMode::Off).unwrap();
        let mut back = None;
        for _ in 0..50 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            if caps_size(&out_pad) == Some((320, 240)) {
                back = Some(true);
                break;
            }
        }
        assert_eq!(back, Some(true), "Off must renegotiate back to 1:1 live");

        pipeline.set_state(gst::State::Null).unwrap();
    }

    /// Full mode cycle INCLUDING the AI stage (needs GL + an AI backend
    /// installed): Off -> Fsr -> NvAi -> Off, live, with a bus watch — any
    /// ERROR message (e.g. not-negotiated) fails the test. Reproduces the
    /// mid-play freeze reported when engaging AI.
    #[test]
    #[ignore = "needs display + GL + AI backend installed"]
    fn enhance_bin_full_cycle_with_ai() {
        gst::init().unwrap();
        assert!(crate::nvsr::ai_available(), "no AI backend installed");
        let pipeline = gst::Pipeline::new();
        let src = gst::ElementFactory::make("videotestsrc")
            .property("is-live", true)
            .build()
            .unwrap();
        let incaps = gst::ElementFactory::make("capsfilter").build().unwrap();
        incaps.set_property(
            "caps",
            gst::Caps::builder("video/x-raw")
                .field("width", 320i32)
                .field("height", 240i32)
                .field("framerate", gst::Fraction::new(30, 1))
                .build(),
        );
        let (enhance, ctl) = build_enhance_bin(EnhanceMode::Off).unwrap();
        let convert = gst::ElementFactory::make("videoconvert").build().unwrap();
        let sink = gst::ElementFactory::make("fakesink")
            .property("sync", false)
            .build()
            .unwrap();
        pipeline.add_many([&src, &incaps, &enhance, &convert, &sink]).unwrap();
        gst::Element::link_many([&src, &incaps, &enhance, &convert, &sink]).unwrap();
        pipeline.set_state(gst::State::Playing).unwrap();
        let (res, _, _) = pipeline.state(gst::ClockTime::from_seconds(10));
        assert!(res.is_ok(), "failed to start");

        let bus = pipeline.bus().unwrap();
        let no_errors = |bus: &gst::Bus, phase: &str| {
            while let Some(msg) = bus.pop_filtered(&[gst::MessageType::Error]) {
                let gst::MessageView::Error(e) = msg.view() else { continue };
                panic!("pipeline ERROR during {}: {} ({:?})", phase, e.error(), e.debug());
            }
        };
        let out_pad = enhance.static_pad("src").unwrap();
        let caps_w = |pad: &gst::Pad| -> Option<i32> {
            pad.current_caps()?.structure(0)?.get("width").ok()
        };
        let settle = |target: i32, phase: &str, bus: &gst::Bus| {
            for _ in 0..80 {
                std::thread::sleep(std::time::Duration::from_millis(100));
                no_errors(bus, phase);
                if caps_w(&out_pad) == Some(target) {
                    return;
                }
            }
            panic!("{}: output caps never reached width={} (got {:?})", phase, target, caps_w(&out_pad));
        };

        ctl.set_mode(EnhanceMode::Fsr).unwrap();
        settle(640, "fsr", &bus);
        // Tune LIVE mid-FSR: uniform-only update — must not renegotiate or error.
        ctl.set_settings(EnhanceSettings { fsr_sharpness: 0.8, ai_strength: 0.5 }).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));
        no_errors(&bus, "fsr-settings");
        ctl.set_mode(EnhanceMode::NvAi).unwrap();
        settle(640, "nvai", &bus);
        // Tune LIVE mid-AI: strength is a plain element property — same rules.
        ctl.set_settings(EnhanceSettings { fsr_sharpness: 0.2, ai_strength: 0.25 }).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));
        no_errors(&bus, "nvai-settings");
        ctl.set_settings(EnhanceSettings::default()).unwrap();
        ctl.set_mode(EnhanceMode::Off).unwrap();
        settle(320, "off", &bus);

        let (_, state, _) = pipeline.state(gst::ClockTime::ZERO);
        assert_eq!(state, gst::State::Playing, "pipeline must stay PLAYING across all toggles");
        pipeline.set_state(gst::State::Null).unwrap();
    }

    /// REAL-topology repro of the reported freeze: an H.264 mp4 through the
    /// actual MediaPlayer (playbin + qtdemux + NVDEC + xvimagesink + resident
    /// enhance bin), toggling Off -> Fsr -> NvAi mid-play. Set
    /// EARTH_ENHANCE_TEST_MEDIA to the clip path. A pipeline error (e.g.
    /// not-negotiated) or a non-Playing state fails the test.
    #[test]
    #[ignore = "needs display + GL + AI backend + EARTH_ENHANCE_TEST_MEDIA"]
    fn enhance_full_cycle_in_real_player() {
        let media = std::env::var("EARTH_ENHANCE_TEST_MEDIA").expect("set EARTH_ENHANCE_TEST_MEDIA");
        let player = crate::MediaPlayer::new().unwrap();
        player.load(&media).unwrap();
        player.play().unwrap();
        std::thread::sleep(std::time::Duration::from_secs(1));

        let assert_healthy = |phase: &str| {
            for _ in 0..30 {
                std::thread::sleep(std::time::Duration::from_millis(100));
                if let crate::PlaybackState::Error(e) = player.get_state() {
                    panic!("player ERROR during {}: {}", phase, e);
                }
            }
            let pos_a = player.get_position().unwrap_or(0);
            std::thread::sleep(std::time::Duration::from_millis(700));
            if let crate::PlaybackState::Error(e) = player.get_state() {
                panic!("player ERROR during {}: {}", phase, e);
            }
            let pos_b = player.get_position().unwrap_or(0);
            assert!(pos_b > pos_a, "{}: playback position frozen ({} -> {})", phase, pos_a, pos_b);
        };

        assert_healthy("off");
        player.set_enhance(EnhanceMode::Fsr).unwrap();
        assert_healthy("fsr");
        // Isolate the NvAi engage sub-steps when EARTH_TEST_STEP is set:
        //   preflight  = only build the ONNX session
        //   engage     = only flip the earthnvsr property
        //   (unset)    = full set_enhance(NvAi)
        match std::env::var("EARTH_TEST_STEP").as_deref() {
            Ok("preflight") => {
                crate::nvsr::ai_preflight().unwrap();
                assert_healthy("preflight-only");
                return;
            }
            Ok("engage") => {
                if let Some(ctl) = player.enhance_ctl.as_ref() {
                    if let Some(nvsr) = ctl.nvsr.as_ref() {
                        nvsr.set_property("engaged", true);
                    }
                }
                assert_healthy("engage-only");
                return;
            }
            _ => {}
        }
        player.set_enhance(EnhanceMode::NvAi).unwrap();
        assert_healthy("nvai");
        player.set_enhance(EnhanceMode::Off).unwrap();
        assert_healthy("off-again");
        player.stop_and_wait().unwrap();

        // Second pipeline STARTING with AI already engaged (session default) —
        // the reported at-load failure path: initial negotiation must succeed
        // with the engaged element in the chain.
        player.set_enhance(EnhanceMode::NvAi).unwrap();
        player.load(&media).unwrap();
        player.play().unwrap();
        std::thread::sleep(std::time::Duration::from_secs(1));
        assert_healthy("nvai-at-load");
        player.stop_and_wait().unwrap();
    }
}
