// Page-loading spinner — a funky conic-gradient ring shown over the content area
// while a page is reloading / navigating. Visibility is driven by REAL WebKit
// load state (see App.tsx's `browser-load-changed` listener), not a timer.
//
// It floats over the WEBVIEW region (not the whole window): `top` is the chrome
// height and `rightInset` matches the docked-panel inset, so it lines up exactly
// with where the embedded page renders. The native page surface is hidden while
// loading, so this React/CSS overlay reads on top. Pure CSS animation (conic
// gradient sweep + hue-shift + glow) — GPU-cheap, no JS loop, no GL/GBM.

interface PageLoadSpinnerProps {
  visible: boolean;
  top: number; // chrome height (px) — top edge of the webview region
  rightInset?: number; // px shaved off the right for a docked panel
}

const CSS = `
.rc-overlay{
  position:fixed; left:0; bottom:0; z-index:40;
  display:flex; align-items:center; justify-content:center;
  opacity:0; pointer-events:none; transition:opacity .22s ease;
  background:radial-gradient(circle at center,
    rgba(8,10,18,0.38), rgba(8,10,18,0.14) 55%, transparent 75%);
  backdrop-filter:blur(1.5px); -webkit-backdrop-filter:blur(1.5px);
}
.rc-overlay[data-visible="true"]{opacity:1}
.rc-wrap{
  position:relative; width:60px; height:60px;
  display:flex; align-items:center; justify-content:center;
  animation:rc-glow 2.4s ease-in-out infinite;
}
.rc-ring{
  position:absolute; inset:0; border-radius:50%;
  background:conic-gradient(from 0deg,
    rgba(124,77,255,0) 0deg, rgba(124,77,255,0.12) 70deg,
    #7c4dff 180deg, #00e5ff 260deg, #00ffa3 320deg, #ffffff 360deg);
  -webkit-mask:radial-gradient(farthest-side, transparent calc(100% - 7px), #000 calc(100% - 6px));
  mask:radial-gradient(farthest-side, transparent calc(100% - 7px), #000 calc(100% - 6px));
  animation:rc-spin .9s linear infinite, rc-hue 3s linear infinite;
}
.rc-core{
  width:13px; height:13px; border-radius:50%;
  background:radial-gradient(circle, rgba(255,255,255,0.92), rgba(124,77,255,0) 70%);
  animation:rc-hue 3s linear infinite, rc-pulse 1.4s ease-in-out infinite;
}
@keyframes rc-spin{to{transform:rotate(360deg)}}
@keyframes rc-hue{to{filter:hue-rotate(360deg)}}
@keyframes rc-pulse{0%,100%{transform:scale(.65);opacity:.55}50%{transform:scale(1.1);opacity:1}}
@keyframes rc-glow{
  0%,100%{filter:drop-shadow(0 0 5px rgba(124,77,255,0.45))}
  50%{filter:drop-shadow(0 0 14px rgba(0,229,255,0.6))}
}
@media (prefers-reduced-motion: reduce){
  .rc-ring{animation:rc-spin 2s linear infinite}
  .rc-core,.rc-wrap{animation:none}
}
`;

export function PageLoadSpinner({ visible, top, rightInset = 0 }: PageLoadSpinnerProps) {
  return (
    <div
      className="rc-overlay"
      data-visible={visible ? 'true' : 'false'}
      style={{ top, right: rightInset }}
      role="status"
      aria-label="Loading page"
      aria-hidden={!visible}
    >
      <style>{CSS}</style>
      <div className="rc-wrap">
        <div className="rc-ring" />
        <div className="rc-core" />
      </div>
    </div>
  );
}

export default PageLoadSpinner;
