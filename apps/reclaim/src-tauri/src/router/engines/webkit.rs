//! WebKitGTK render engine — the first [`RenderEngine`] impl.
//!
//! Delegates to [`crate::browser_surface`], which embeds a WebKitGTK `WebView`
//! into the Tauri window via X11 reparenting. This replaces Tauri's `add_child`
//! webview (which tiles vertically on WebKitGTK instead of overlaying at the
//! requested bounds). The engine passes the resolved URL + bounds through; the
//! surface module owns the GTK/X11 details.

use crate::router::engine::{RenderCtx, RenderEngine, RenderError};
use crate::router::resolver::ResolvedTarget;
use crate::router::url::DomainClass;
use tauri::Manager;

pub struct WebKitEngine;

#[async_trait::async_trait]
impl RenderEngine for WebKitEngine {
    fn name(&self) -> &'static str {
        "webkitgtk"
    }

    /// WebKitGTK handles every class EXCEPT `.earth`, which the Servo engine
    /// (registered before this one) draws instead.
    fn handles(&self, class: DomainClass) -> bool {
        !matches!(class, DomainClass::Earth)
    }

    async fn render(&self, ctx: &RenderCtx, target: &ResolvedTarget) -> Result<(), RenderError> {
        let window = ctx
            .app
            .get_window("main")
            .ok_or_else(|| RenderError::Engine("main window not found".to_string()))?;

        // Default: embed the page as a GtkOverlay child of the Tauri toplevel —
        // same toplevel (no click→scroll render-stall) and manual overlay/inset
        // positioning. The legacy X11-reparented surface is the `EARTH_EMBED=x11`
        // fallback below.
        // bounds Some => create/reposition; None => navigate the existing page in place.
        if crate::browser_overlay::enabled() {
            return crate::browser_overlay::navigate(
                window,
                ctx.tab_id,
                target.url.clone(),
                ctx.bounds.clone(),
            )
            .map_err(RenderError::Engine);
        }

        crate::browser_surface::navigate_surface(window, ctx.tab_id, target.url.clone(), ctx.bounds.clone())
            .await
            .map_err(RenderError::Engine)
    }
}
