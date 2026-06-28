//! Servo render engine — PHASE 4. Draws `DomainClass::Earth` via an EXTERNAL
//! Servo process (the `earth-servo` crate) in a SEPARATE OS window.
//!
//! Acquisition (Option A): we do NOT link the `servo` crate and do NOT compile
//! Servo into this binary. `earth-servo`'s `ServoManager` spawns a prebuilt
//! `servo` binary, located (in order) via:
//!   1. the `SERVO_PATH` environment variable,
//!   2. `~/Documents/Earth-Runtime/servo/target/release/servo`,
//!   3. `~/servo/target/release/servo`,
//!   4. `/usr/local/bin/servo`,
//!   5. `/usr/bin/servo`,
//!   6. `servo` on `$PATH`.
//! To produce the binary, build the vendored `servo/` tree: `./mach build --release`.
//!
//! If the binary is missing, [`ServoEngine::render`] returns a clear, user-facing
//! error string. The frontend surfaces it as the `.earth` tab's error state
//! ("Servo engine not available …") rather than a silent no-op.
//!
//! Embedding the Servo window INTO the Tauri window (X11 reparenting, like
//! `browser_surface`) is explicitly OUT OF SCOPE here — that is Phase 4b. For
//! this slice the Servo window is a separate, unpositioned OS window.

use crate::router::engine::{RenderCtx, RenderEngine, RenderError};
use crate::router::resolver::ResolvedTarget;
use crate::router::url::DomainClass;

pub struct ServoEngine;

#[async_trait::async_trait]
impl RenderEngine for ServoEngine {
    fn name(&self) -> &'static str {
        "servo"
    }

    /// Servo draws only `.earth`. WebKitGTK handles `.click`/legacy.
    fn handles(&self, class: DomainClass) -> bool {
        matches!(class, DomainClass::Earth)
    }

    async fn render(&self, ctx: &RenderCtx, target: &ResolvedTarget) -> Result<(), RenderError> {
        // One Servo process per tab; re-navigating a tab kills+relaunches it
        // (earth-servo has no IPC yet — documented limitation, fine for the slice).
        let webview_id = format!("tab-{}", ctx.tab_id);
        crate::servo_browser::create_servo_browser(webview_id, target.url.clone())
            .await
            .map_err(|e| {
                RenderError::Engine(format!(
                    "Servo engine not available — build servo/ or set SERVO_PATH ({e})"
                ))
            })
    }
}
