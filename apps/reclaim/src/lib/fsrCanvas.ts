// FSR 1.0 super-resolution for STILL IMAGES, on a WebGL canvas.
//
// The media player's "Enhance" runs these exact shaders inside the GStreamer
// pipeline for video (crates/earth-media/src/enhance.rs — keep the GLSL in
// sync); photos aren't in that pipeline, so ImageViewer renders them here
// instead: pass 1 EASU (edge-adaptive 2x upscale) into a framebuffer, pass 2
// RCAS (contrast-adaptive sharpen) onto the visible canvas. One-shot per
// image — nothing keeps running afterwards.

const VERTEX = `
attribute vec2 a_pos;
varying vec2 v_texcoord;
void main() {
  v_texcoord = a_pos * 0.5 + 0.5;
  gl_Position = vec4(a_pos, 0.0, 1.0);
}
`;

const EASU_FRAGMENT = `
precision highp float;
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
`;

const RCAS_FRAGMENT = `
precision highp float;
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
`;

/// Default RCAS sharpening in stops (0 = sharpest, 2 = softest) — matches the
/// video pipeline's EnhanceSettings default.
const DEFAULT_SHARPNESS_STOPS = 0.2;

function compile(gl: WebGLRenderingContext, type: number, src: string): WebGLShader | null {
  const sh = gl.createShader(type);
  if (!sh) return null;
  gl.shaderSource(sh, src);
  gl.compileShader(sh);
  if (!gl.getShaderParameter(sh, gl.COMPILE_STATUS)) {
    console.error('[fsrCanvas] shader compile failed:', gl.getShaderInfoLog(sh));
    gl.deleteShader(sh);
    return null;
  }
  return sh;
}

function program(gl: WebGLRenderingContext, fragSrc: string): WebGLProgram | null {
  const vs = compile(gl, gl.VERTEX_SHADER, VERTEX);
  const fs = compile(gl, gl.FRAGMENT_SHADER, fragSrc);
  if (!vs || !fs) return null;
  const p = gl.createProgram();
  if (!p) return null;
  gl.attachShader(p, vs);
  gl.attachShader(p, fs);
  gl.bindAttribLocation(p, 0, 'a_pos');
  gl.linkProgram(p);
  gl.deleteShader(vs);
  gl.deleteShader(fs);
  if (!gl.getProgramParameter(p, gl.LINK_STATUS)) {
    console.error('[fsrCanvas] program link failed:', gl.getProgramInfoLog(p));
    gl.deleteProgram(p);
    return null;
  }
  return p;
}

/// The FSR output size for an image: 2x, capped per-dimension at 4K (and the
/// GL max texture size). Returns null when upscaling wouldn't help (factor 1).
export function fsrOutputSize(
  w: number,
  h: number,
  maxTex = 8192,
): { w: number; h: number } | null {
  if (!w || !h) return null;
  const cap = Math.min(4096, maxTex);
  const f = Math.min(2, cap / w, cap / h);
  if (f < 1.2) return null; // near/at cap already — not worth the pass
  return { w: Math.floor((w * f) / 2) * 2, h: Math.floor((h * f) / 2) * 2 };
}

/// Render `img` super-resolved (EASU + RCAS) into `canvas`. Sets the canvas
/// dimensions itself. Returns true on success; on any failure the canvas is
/// untouched and the caller keeps showing the plain <img>.
/// `sharpnessStops` is RCAS sharpening in stops (0 sharpest ..= 2 softest),
/// mirroring the video pipeline's Enhance sharpness setting.
export function fsrUpscaleToCanvas(
  img: HTMLImageElement,
  canvas: HTMLCanvasElement,
  sharpnessStops: number = DEFAULT_SHARPNESS_STOPS,
): boolean {
  try {
    const sw = img.naturalWidth;
    const sh = img.naturalHeight;
    const gl = canvas.getContext('webgl', {
      premultipliedAlpha: false,
      preserveDrawingBuffer: true,
    }) as WebGLRenderingContext | null;
    if (!gl) return false;
    const maxTex = gl.getParameter(gl.MAX_TEXTURE_SIZE) as number;
    if (sw > maxTex || sh > maxTex) return false;
    const out = fsrOutputSize(sw, sh, maxTex);
    if (!out) return false;

    canvas.width = out.w;
    canvas.height = out.h;

    const easu = program(gl, EASU_FRAGMENT);
    const rcas = program(gl, RCAS_FRAGMENT);
    if (!easu || !rcas) return false;

    // Fullscreen quad.
    const buf = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, buf);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 1, -1, -1, 1, 1, 1]), gl.STATIC_DRAW);
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);

    const linearTex = (w: number, h: number, source?: HTMLImageElement) => {
      const t = gl.createTexture();
      gl.bindTexture(gl.TEXTURE_2D, t);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
      if (source) {
        gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, true);
        gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, source);
        gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, false);
      } else {
        gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, w, h, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
      }
      return t;
    };

    const srcTex = linearTex(sw, sh, img);
    const midTex = linearTex(out.w, out.h);
    const fbo = gl.createFramebuffer();
    gl.bindFramebuffer(gl.FRAMEBUFFER, fbo);
    gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, midTex, 0);
    if (gl.checkFramebufferStatus(gl.FRAMEBUFFER) !== gl.FRAMEBUFFER_COMPLETE) return false;

    const setF = (p: WebGLProgram, name: string, v: number) =>
      gl.uniform1f(gl.getUniformLocation(p, name), v);

    // Pass 1: EASU into the framebuffer at output resolution.
    gl.useProgram(easu);
    gl.viewport(0, 0, out.w, out.h);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, srcTex);
    gl.uniform1i(gl.getUniformLocation(easu, 'tex'), 0);
    setF(easu, 'u_src_w', sw);
    setF(easu, 'u_src_h', sh);
    setF(easu, 'u_dst_w', out.w);
    setF(easu, 'u_dst_h', out.h);
    gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);

    // Pass 2: RCAS onto the visible canvas.
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    gl.useProgram(rcas);
    gl.viewport(0, 0, out.w, out.h);
    gl.bindTexture(gl.TEXTURE_2D, midTex);
    gl.uniform1i(gl.getUniformLocation(rcas, 'tex'), 0);
    setF(rcas, 'u_dst_w', out.w);
    setF(rcas, 'u_dst_h', out.h);
    const stops = Number.isFinite(sharpnessStops)
      ? Math.min(2, Math.max(0, sharpnessStops))
      : DEFAULT_SHARPNESS_STOPS;
    setF(rcas, 'u_sharpness', Math.pow(2, -stops));
    gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
    gl.finish();

    // One-shot render — free GPU resources, keep the canvas contents.
    gl.deleteTexture(srcTex);
    gl.deleteTexture(midTex);
    gl.deleteFramebuffer(fbo);
    gl.deleteBuffer(buf);
    gl.deleteProgram(easu);
    gl.deleteProgram(rcas);
    return true;
  } catch (err) {
    console.error('[fsrCanvas] enhance failed:', err);
    return false;
  }
}
