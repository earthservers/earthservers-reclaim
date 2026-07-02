//! Video "Enhance" — in-pipeline super resolution (Tier 1: FSR 1.0 shaders).
//!
//! Builds a GL filter bin for playbin's `video-filter` slot:
//!
//!   glupload ! glcolorconvert ! glshader(EASU) ! caps(2x) ! glshader(RCAS)
//!            ! caps(2x) ! glcolorconvert ! gldownload
//!
//! EASU (edge-adaptive spatial upsampling) does the actual scaling — `glshader`
//! renders the input texture onto an output-sized quad, so forcing larger caps
//! AFTER it is what makes it a scaler (verified: negotiation allows it). RCAS
//! sharpens at the output resolution. Output goes back to SYSTEM memory
//! (`gldownload`) so the proven `xvimagesink` presentation path is untouched —
//! we deliberately never GL-render to the reparented X11 surface (glimagesink
//! hard-crashes on NVIDIA there; see `build_video_sink`).
//!
//! Sizing is dynamic: a caps probe on the bin's sink pad reads each stream's
//! real dimensions and sets the mid-pipeline capsfilters + shader uniforms, so
//! it tracks per-clip resolutions (and mid-stream renegotiation) with no load-
//! order dependency. Scale is 2x, capped at 4K; near-native inputs pass through
//! at 1:1 (RCAS still sharpens).
//!
//! Kill switch: EARTH_VIDEO_SR=off refuses to enable enhancement at runtime.

use gstreamer as gst;
use gstreamer::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU8, Ordering};

use crate::MediaError;

/// Upscale factor and output ceiling. 2x covers the 360p–1080p content SR is
/// for; the 4K cap bounds GPU cost (frames above ~1920p pass through).
const SCALE: f64 = 2.0;
const MAX_W: i32 = 3840;
const MAX_H: i32 = 2160;

/// RCAS sharpness as exp2(-stops): 0.0 stops = 1.0 (max). 0.2 stops is FSR's
/// commonly-shipped default — visibly sharper without halos.
const RCAS_SHARPNESS: f32 = 0.870_55; // exp2(-0.2)

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EnhanceMode {
    /// No enhancement — the default HQ Lanczos scaler path.
    Off,
    /// FSR 1.0 (EASU 2x upscale + RCAS sharpen) on the GPU.
    Fsr,
}

impl EnhanceMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "off" => Some(Self::Off),
            "fsr" => Some(Self::Fsr),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Fsr => "fsr",
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
        _ => EnhanceMode::Off,
    }
}

pub fn set_default_enhance(mode: EnhanceMode) {
    DEFAULT_ENHANCE.store(matches!(mode, EnhanceMode::Fsr) as u8, Ordering::Relaxed);
}

/// EARTH_VIDEO_SR=off disables enhancement entirely (isolation/debug hatch,
/// mirrors EARTH_NO_NVDEC / EARTH_VIDEO_SINK).
pub fn sr_env_disabled() -> bool {
    std::env::var("EARTH_VIDEO_SR").map(|v| v == "off" || v == "0").unwrap_or(false)
}

// FSR 1.0 EASU, 12-tap non-gather port (GLES2-compatible GLSL, GStreamer
// glshader conventions: `v_texcoord` + `tex`). Input/output sizes arrive as
// uniforms from the caps probe. Verified to compile+run on NVIDIA desktop GL.
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

/// Build the FSR filter bin for playbin's `video-filter`. Fails cleanly (Err)
/// if any GL element is missing — the caller falls back to the default filter
/// so playback never breaks.
pub fn build_fsr_bin() -> Result<gst::Element, MediaError> {
    let bin = gst::Bin::builder().name("earth-enhance-fsr").build();

    let upload = make("glupload")?;
    let convert_in = make("glcolorconvert")?;
    let easu = make("glshader")?;
    let caps_scale = make("capsfilter")?;
    let rcas = make("glshader")?;
    let caps_out = make("capsfilter")?;
    let convert_out = make("glcolorconvert")?;
    let download = make("gldownload")?;

    easu.set_property("fragment", EASU_FRAGMENT);
    rcas.set_property("fragment", RCAS_FRAGMENT);

    // Unrestricted until the caps probe learns the stream's real size.
    let any_gl = gst::Caps::builder("video/x-raw")
        .features(["memory:GLMemory"])
        .field("format", "RGBA")
        .build();
    caps_scale.set_property("caps", &any_gl);
    caps_out.set_property("caps", &any_gl);

    bin.add_many([
        &upload, &convert_in, &easu, &caps_scale, &rcas, &caps_out, &convert_out, &download,
    ])
    .map_err(|e| MediaError::PlayerError(format!("enhance: bin add failed: {}", e)))?;
    gst::Element::link_many([
        &upload, &convert_in, &easu, &caps_scale, &rcas, &caps_out, &convert_out, &download,
    ])
    .map_err(|e| MediaError::PlayerError(format!("enhance: bin link failed: {}", e)))?;

    let sink_target = upload
        .static_pad("sink")
        .ok_or_else(|| MediaError::PlayerError("enhance: glupload has no sink pad".into()))?;
    let src_target = download
        .static_pad("src")
        .ok_or_else(|| MediaError::PlayerError("enhance: gldownload has no src pad".into()))?;
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

    // Size everything from the stream's REAL dimensions as they pass by: set the
    // scale capsfilters and the shader uniforms per caps event. This tracks
    // per-clip resolution and mid-stream renegotiation with no load-order coupling.
    let easu_w = easu.clone();
    let rcas_w = rcas.clone();
    let caps_scale_w = caps_scale.clone();
    let caps_out_w = caps_out.clone();
    sink_pad.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |_pad, info| {
        let Some(ev) = info.event() else { return gst::PadProbeReturn::Ok };
        let gst::EventView::Caps(caps_ev) = ev.view() else { return gst::PadProbeReturn::Ok };
        let caps = caps_ev.caps();
        let Some(s) = caps.structure(0) else { return gst::PadProbeReturn::Ok };
        let (Ok(w), Ok(h)) = (s.get::<i32>("width"), s.get::<i32>("height")) else {
            return gst::PadProbeReturn::Ok;
        };
        let Some((ow, oh)) = output_size(w, h) else { return gst::PadProbeReturn::Ok };

        let mut out = gst::Caps::builder("video/x-raw")
            .features(["memory:GLMemory"])
            .field("format", "RGBA")
            .field("width", ow)
            .field("height", oh);
        // Uniform scale keeps the pixel aspect; carry it through explicitly so
        // fixation can't quietly square anamorphic content.
        if let Ok(par) = s.get::<gst::Fraction>("pixel-aspect-ratio") {
            out = out.field("pixel-aspect-ratio", par);
        }
        let out = out.build();
        caps_scale_w.set_property("caps", &out);
        caps_out_w.set_property("caps", &out);

        easu_w.set_property(
            "uniforms",
            shader_uniforms(&[
                ("u_src_w", w as f32),
                ("u_src_h", h as f32),
                ("u_dst_w", ow as f32),
                ("u_dst_h", oh as f32),
            ]),
        );
        rcas_w.set_property(
            "uniforms",
            shader_uniforms(&[
                ("u_dst_w", ow as f32),
                ("u_dst_h", oh as f32),
                ("u_sharpness", RCAS_SHARPNESS),
            ]),
        );
        log::info!("[earth-media] enhance: FSR {}x{} -> {}x{}", w, h, ow, oh);
        gst::PadProbeReturn::Ok
    });

    Ok(bin.upcast())
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
    fn enhance_mode_parses() {
        assert_eq!(EnhanceMode::parse("fsr"), Some(EnhanceMode::Fsr));
        assert_eq!(EnhanceMode::parse("OFF"), Some(EnhanceMode::Off));
        assert_eq!(EnhanceMode::parse("dlss"), None);
    }

    /// Full negotiation test: videotestsrc 320x240 through the FSR bin must
    /// preroll and come out 640x480. Needs a GL-capable display, so it's
    /// ignored by default — run locally with `cargo test -- --ignored`.
    #[test]
    #[ignore = "needs a display + GL context"]
    fn fsr_bin_prerolls_and_doubles_resolution() {
        gst::init().unwrap();
        let pipeline = gst::Pipeline::new();
        let src = gst::ElementFactory::make("videotestsrc")
            .property("num-buffers", 5i32)
            .build()
            .unwrap();
        let incaps = gst::ElementFactory::make("capsfilter").build().unwrap();
        incaps.set_property(
            "caps",
            gst::Caps::builder("video/x-raw")
                .field("width", 320i32)
                .field("height", 240i32)
                .build(),
        );
        let fsr = build_fsr_bin().unwrap();
        let convert = gst::ElementFactory::make("videoconvert").build().unwrap();
        let sink = gst::ElementFactory::make("fakesink").build().unwrap();
        pipeline.add_many([&src, &incaps, &fsr, &convert, &sink]).unwrap();
        gst::Element::link_many([&src, &incaps, &fsr, &convert, &sink]).unwrap();

        pipeline.set_state(gst::State::Paused).unwrap();
        let (res, state, _) = pipeline.state(gst::ClockTime::from_seconds(10));
        assert!(res.is_ok(), "FSR pipeline failed to preroll");
        assert_eq!(state, gst::State::Paused);

        let out_caps = fsr
            .static_pad("src")
            .unwrap()
            .current_caps()
            .expect("no negotiated caps on FSR src pad");
        let s = out_caps.structure(0).unwrap();
        assert_eq!(s.get::<i32>("width").unwrap(), 640);
        assert_eq!(s.get::<i32>("height").unwrap(), 480);

        pipeline.set_state(gst::State::Null).unwrap();
    }
}
