//! RENDER axis — pick the engine that draws a resolved target.
//!
//! Engine selection keys off [`DomainClass`] (`.earth` -> Servo,
//! `.click`/legacy -> WebKitGTK), never off which resolver answered. That is
//! what keeps resolution and rendering orthogonal: a `.click` resolved via P2P
//! still renders in WebKitGTK; a `.earth` resolved via blockchain still renders
//! in Servo.

use super::resolver::ResolvedTarget;
use super::url::DomainClass;
use crate::webview::WebviewBounds;

/// Everything a render engine needs to draw into the app, beyond the target.
pub struct RenderCtx {
    pub app: tauri::AppHandle,
    pub tab_id: i64,
    /// `Some` to create-or-reposition the embedded webview (caller owns layout);
    /// `None` to navigate the existing webview in place (e.g. a chrome nav bar).
    pub bounds: Option<WebviewBounds>,
}

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("render engine error: {0}")]
    Engine(String),
    #[error("no render engine registered for class {0:?}")]
    NoEngine(DomainClass),
}

/// A render backend (WebKitGTK today; Servo in Phase 4).
#[async_trait::async_trait]
pub trait RenderEngine: Send + Sync {
    fn name(&self) -> &'static str;

    /// Whether this engine draws the given domain class.
    fn handles(&self, class: DomainClass) -> bool;

    async fn render(&self, ctx: &RenderCtx, target: &ResolvedTarget) -> Result<(), RenderError>;
}
