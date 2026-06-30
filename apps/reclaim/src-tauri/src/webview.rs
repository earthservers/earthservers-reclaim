// WebView management for Reclaim browser functionality
// EMBEDDED WEBVIEW PATTERN: Create child webviews within the main window
// positioned to fit within the app layout (below navbar/tabs)
// Note: External browsing is handled by Servo (earth-servo crate)

use std::sync::Mutex;
use tauri::{Manager, AppHandle, Emitter, WebviewUrl};

lazy_static::lazy_static! {
    // Track devtools open state
    static ref DEVTOOLS_OPEN: Mutex<bool> = Mutex::new(false);
    // Track current browser webview and its URL
    static ref BROWSER_STATE: Mutex<BrowserState> = Mutex::new(BrowserState::default());
}

#[derive(Default)]
struct BrowserState {
    current_tab_id: Option<i64>,
    current_url: Option<String>,
    is_visible: bool,
}

/// Bounds for webview positioning
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebviewBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Create or update the browser webview with given URL and bounds
#[tauri::command(rename_all = "camelCase")]
pub async fn create_browser_webview(
    app: AppHandle,
    tab_id: i64,
    url: String,
    bounds: WebviewBounds,
) -> Result<(), String> {
    let webview_label = "browser-content";

    // Update state
    if let Ok(mut state) = BROWSER_STATE.lock() {
        state.current_tab_id = Some(tab_id);
        state.current_url = Some(url.clone());
        state.is_visible = true;
    }

    // Check if webview already exists
    if let Some(existing) = app.get_webview(webview_label) {
        // Just navigate and reposition
        let nav_url = tauri::Url::parse(&url).map_err(|e| e.to_string())?;
        existing.navigate(nav_url).map_err(|e| e.to_string())?;
        existing.set_position(tauri::Position::Logical(tauri::LogicalPosition::new(bounds.x, bounds.y)))
            .map_err(|e| e.to_string())?;
        existing.set_size(tauri::Size::Logical(tauri::LogicalSize::new(bounds.width, bounds.height)))
            .map_err(|e| e.to_string())?;
        existing.show().map_err(|e| e.to_string())?;
        return Ok(());
    }

    // Get the main window to attach the webview to
    let main_window = app.get_window("main")
        .ok_or("Main window not found")?;

    // Create URL for the webview
    let webview_url = if url.starts_with("http://") || url.starts_with("https://") {
        WebviewUrl::External(url.parse().map_err(|e: url::ParseError| e.to_string())?)
    } else {
        WebviewUrl::App(url.clone().into())
    };

    // Create the child webview attached to the main window
    // Note: On WebKitGTK (Linux), the position may be ignored. The React app's
    // chrome uses high z-index to overlay on top of the webview.
    let webview = main_window.add_child(
        tauri::webview::WebviewBuilder::new(webview_label, webview_url),
        tauri::LogicalPosition::new(bounds.x, bounds.y),
        tauri::LogicalSize::new(bounds.width, bounds.height),
    ).map_err(|e| format!("Failed to create webview: {}", e))?;

    // Explicitly set position and size after creation (helps on some platforms)
    webview.set_position(tauri::Position::Logical(tauri::LogicalPosition::new(bounds.x, bounds.y))).ok();
    webview.set_size(tauri::Size::Logical(tauri::LogicalSize::new(bounds.width, bounds.height))).ok();

    // Bring to front
    webview.show().ok();

    // Emit event
    app.emit("browser-webview-created", &tab_id).ok();

    Ok(())
}

/// Navigate the browser webview to a URL
#[tauri::command(rename_all = "camelCase")]
pub async fn browser_navigate(
    app: AppHandle,
    tab_id: i64,
    url: String,
) -> Result<(), String> {
    let webview_label = "browser-content";

    // Update state
    if let Ok(mut state) = BROWSER_STATE.lock() {
        state.current_tab_id = Some(tab_id);
        state.current_url = Some(url.clone());
    }

    if let Some(webview) = app.get_webview(webview_label) {
        let nav_url = tauri::Url::parse(&url).map_err(|e| e.to_string())?;
        webview.navigate(nav_url).map_err(|e| e.to_string())?;
        Ok(())
    } else {
        Err("Browser webview not found".to_string())
    }
}

/// Switch to a different tab's content
#[tauri::command(rename_all = "camelCase")]
pub async fn switch_tab_webview(
    app: AppHandle,
    _from_tab_id: Option<i64>,
    to_tab_id: i64,
    to_url: String,
    bounds: WebviewBounds,
) -> Result<(), String> {
    create_browser_webview(app, to_tab_id, to_url, bounds).await
}

/// Update webview bounds (position and size)
#[tauri::command(rename_all = "camelCase")]
pub async fn update_browser_bounds(
    app: AppHandle,
    bounds: WebviewBounds,
) -> Result<(), String> {
    let webview_label = "browser-content";

    if let Some(webview) = app.get_webview(webview_label) {
        webview.set_position(tauri::Position::Logical(tauri::LogicalPosition::new(bounds.x, bounds.y)))
            .map_err(|e| e.to_string())?;
        webview.set_size(tauri::Size::Logical(tauri::LogicalSize::new(bounds.width, bounds.height)))
            .map_err(|e| e.to_string())?;

        Ok(())
    } else {
        // No webview exists yet - that's okay
        Ok(())
    }
}

/// Hide the browser webview
#[tauri::command(rename_all = "camelCase")]
pub async fn hide_browser_webview(app: AppHandle) -> Result<(), String> {
    let webview_label = "browser-content";

    if let Ok(mut state) = BROWSER_STATE.lock() {
        state.is_visible = false;
    }

    if let Some(webview) = app.get_webview(webview_label) {
        webview.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Show the browser webview
#[tauri::command(rename_all = "camelCase")]
pub async fn show_browser_webview(app: AppHandle) -> Result<(), String> {
    let webview_label = "browser-content";

    if let Ok(mut state) = BROWSER_STATE.lock() {
        state.is_visible = true;
    }

    if let Some(webview) = app.get_webview(webview_label) {
        webview.show().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Check if browser webview exists
#[tauri::command(rename_all = "camelCase")]
pub async fn has_browser_webview(app: AppHandle) -> Result<bool, String> {
    Ok(app.get_webview("browser-content").is_some())
}

/// Close/destroy the browser webview
#[tauri::command(rename_all = "camelCase")]
pub async fn close_browser_webview(app: AppHandle) -> Result<(), String> {
    let webview_label = "browser-content";

    if let Ok(mut state) = BROWSER_STATE.lock() {
        state.current_tab_id = None;
        state.current_url = None;
        state.is_visible = false;
    }

    if let Some(webview) = app.get_webview(webview_label) {
        webview.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Go back in browser history
#[tauri::command(rename_all = "camelCase")]
pub async fn browser_go_back(app: AppHandle) -> Result<(), String> {
    if let Some(webview) = app.get_webview("browser-content") {
        webview.eval("window.history.back()").map_err(|e| e.to_string())
    } else {
        Err("Browser webview not found".to_string())
    }
}

/// Go forward in browser history
#[tauri::command(rename_all = "camelCase")]
pub async fn browser_go_forward(app: AppHandle) -> Result<(), String> {
    if let Some(webview) = app.get_webview("browser-content") {
        webview.eval("window.history.forward()").map_err(|e| e.to_string())
    } else {
        Err("Browser webview not found".to_string())
    }
}

/// Reload the browser webview
#[tauri::command(rename_all = "camelCase")]
pub async fn browser_reload(app: AppHandle) -> Result<(), String> {
    if let Some(webview) = app.get_webview("browser-content") {
        webview.eval("window.location.reload()").map_err(|e| e.to_string())
    } else {
        Err("Browser webview not found".to_string())
    }
}

/// Get current URL
#[tauri::command(rename_all = "camelCase")]
pub async fn browser_get_url(_app: AppHandle) -> Result<String, String> {
    if let Ok(state) = BROWSER_STATE.lock() {
        Ok(state.current_url.clone().unwrap_or_default())
    } else {
        Ok(String::new())
    }
}

/// Execute JavaScript in browser webview
#[tauri::command(rename_all = "camelCase")]
pub async fn browser_execute_js(app: AppHandle, script: String) -> Result<(), String> {
    if let Some(webview) = app.get_webview("browser-content") {
        webview.eval(&script).map_err(|e| e.to_string())
    } else {
        Err("Browser webview not found".to_string())
    }
}

/// Detach browser to a new window
#[tauri::command(rename_all = "camelCase")]
pub async fn detach_browser_to_window(
    app: AppHandle,
    tab_id: i64,
    url: String,
    title: String,
    x: Option<i32>,
    y: Option<i32>,
) -> Result<String, String> {
    use tauri::WebviewWindowBuilder;

    let window_label = format!("detached-{}", tab_id);

    // Create URL
    let webview_url = if url.starts_with("http://") || url.starts_with("https://") {
        WebviewUrl::External(url.parse().map_err(|e: url::ParseError| e.to_string())?)
    } else {
        WebviewUrl::App(url.into())
    };

    // Create a new detached window
    let mut builder = WebviewWindowBuilder::new(
        &app,
        &window_label,
        webview_url,
    )
    .title(&title)
    .inner_size(1200.0, 800.0)
    .min_inner_size(400.0, 300.0)
    .decorations(true)
    .resizable(true)
    .visible(true)
    // Match main's incognito web context (tauri.conf.json) so the asset protocol
    // resolves for the App fallback in packaged builds.
    .incognito(true);

    // Set position if provided
    if let (Some(x), Some(y)) = (x, y) {
        builder = builder.position(x as f64, y as f64);
    }

    builder.build().map_err(|e| e.to_string())?;

    // Close the embedded webview
    close_browser_webview(app).await?;

    Ok(window_label)
}

/// Clear state for a closed tab
#[tauri::command(rename_all = "camelCase")]
pub async fn clear_tab_state(tab_id: i64) -> Result<(), String> {
    if let Ok(mut state) = BROWSER_STATE.lock() {
        if state.current_tab_id == Some(tab_id) {
            state.current_tab_id = None;
            state.current_url = None;
        }
    }
    Ok(())
}

// ============================================================================
// DEVTOOLS MANAGEMENT
// ============================================================================

/// Check if devtools are open (tracked state)
pub fn is_devtools_open_tracked() -> bool {
    DEVTOOLS_OPEN.lock().map(|guard| *guard).unwrap_or(false)
}

/// Set devtools open state
pub fn set_devtools_open(open: bool) {
    if let Ok(mut guard) = DEVTOOLS_OPEN.lock() {
        *guard = open;
    }
}

/// Toggle devtools for the browser webview
#[tauri::command(rename_all = "camelCase")]
pub async fn toggle_browser_devtools(app: AppHandle) -> Result<(), String> {
    // Try browser webview first, then main window
    let target = app.get_webview("browser-content")
        .map(|w| ("browser", w))
        .or_else(|| app.get_webview_window("main").map(|w| ("main", w.as_ref().clone())));

    if let Some((name, _webview)) = target {
        let is_open = is_devtools_open_tracked();
        println!("Toggling devtools for {} (currently {})", name, if is_open { "open" } else { "closed" });

        if is_open {
            // For Tauri 2, we need to use the webview window
            if let Some(window) = app.get_webview_window("main") {
                window.close_devtools();
            }
            set_devtools_open(false);
        } else {
            if let Some(window) = app.get_webview_window("main") {
                window.open_devtools();
            }
            set_devtools_open(true);
        }
        Ok(())
    } else {
        Err("No webview found".to_string())
    }
}

/// Open devtools
#[tauri::command(rename_all = "camelCase")]
pub async fn open_browser_devtools(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        window.open_devtools();
        set_devtools_open(true);
        Ok(())
    } else {
        Err("Main window not found".to_string())
    }
}

/// Close devtools
#[tauri::command(rename_all = "camelCase")]
pub async fn close_browser_devtools(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        window.close_devtools();
        set_devtools_open(false);
        Ok(())
    } else {
        Err("Main window not found".to_string())
    }
}

/// Open URL in system browser
#[tauri::command(rename_all = "camelCase")]
pub async fn open_in_system_browser(url: String) -> Result<(), String> {
    open::that(&url).map_err(|e| e.to_string())
}

// ============================================================================
// LEGACY COMPATIBILITY - These map to the new browser_* functions
// ============================================================================

#[tauri::command(rename_all = "camelCase")]
pub async fn create_tab_webview(
    app: AppHandle,
    tab_id: i64,
    url: String,
    bounds: WebviewBounds,
) -> Result<(), String> {
    create_browser_webview(app, tab_id, url, bounds).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn webview_navigate(
    app: AppHandle,
    tab_id: i64,
    url: String,
) -> Result<(), String> {
    browser_navigate(app, tab_id, url).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn show_tab_webview(app: AppHandle, _tab_id: i64) -> Result<(), String> {
    show_browser_webview(app).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn hide_tab_webview(app: AppHandle, _tab_id: i64) -> Result<(), String> {
    hide_browser_webview(app).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn close_tab_webview(app: AppHandle, _tab_id: i64) -> Result<(), String> {
    close_browser_webview(app).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn update_webview_bounds(
    app: AppHandle,
    _tab_id: i64,
    bounds: WebviewBounds,
) -> Result<(), String> {
    update_browser_bounds(app, bounds).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn webview_go_back(app: AppHandle, _tab_id: i64) -> Result<(), String> {
    browser_go_back(app).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn webview_go_forward(app: AppHandle, _tab_id: i64) -> Result<(), String> {
    browser_go_forward(app).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn webview_reload(app: AppHandle, _tab_id: i64) -> Result<(), String> {
    browser_reload(app).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn webview_get_url(app: AppHandle, _tab_id: i64) -> Result<String, String> {
    browser_get_url(app).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn webview_execute_js(
    app: AppHandle,
    _tab_id: i64,
    script: String,
) -> Result<(), String> {
    browser_execute_js(app, script).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn has_tab_webview(app: AppHandle, _tab_id: i64) -> Result<bool, String> {
    has_browser_webview(app).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn is_webview_embedded(_tab_id: i64) -> Result<Option<bool>, String> {
    // Real state: whether the embedded browser surface currently exists.
    Ok(Some(crate::browser_surface::is_embedded().await.unwrap_or(false)))
}

#[tauri::command(rename_all = "camelCase")]
pub async fn detach_tab_webview(
    app: AppHandle,
    tab_id: i64,
    url: String,
    title: String,
    x: Option<i32>,
    y: Option<i32>,
) -> Result<String, String> {
    detach_browser_to_window(app, tab_id, url, title, x, y).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn webview_get_title(_app: AppHandle, _tab_id: i64) -> Result<String, String> {
    // Real title from the embedded browser surface (updated via load/title signals).
    crate::browser_surface::get_title().await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn webview_get_html(_app: AppHandle, _tab_id: i64) -> Result<String, String> {
    // Real page HTML from the embedded browser surface.
    crate::browser_surface::get_html().await
}

// ============================================================================
// NO LONGER USED - kept for compile compatibility
// ============================================================================

#[tauri::command(rename_all = "camelCase")]
pub async fn navigate_main_window(_app: AppHandle, _url: String) -> Result<(), String> {
    // This was navigating the main window away from the app - don't do that
    Ok(())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn navigate_to_app(_app: AppHandle) -> Result<(), String> {
    Ok(())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn is_external_browsing() -> Result<bool, String> {
    Ok(BROWSER_STATE.lock().map(|s| s.is_visible).unwrap_or(false))
}

#[tauri::command(rename_all = "camelCase")]
pub async fn get_current_external_url() -> Result<Option<String>, String> {
    Ok(BROWSER_STATE.lock().map(|s| s.current_url.clone()).unwrap_or(None))
}

#[tauri::command(rename_all = "camelCase")]
pub async fn execute_js_in_main(app: AppHandle, script: String) -> Result<(), String> {
    browser_execute_js(app, script).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn reload_main_window(app: AppHandle) -> Result<(), String> {
    browser_reload(app).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn main_window_go_back(app: AppHandle) -> Result<(), String> {
    browser_go_back(app).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn main_window_go_forward(app: AppHandle) -> Result<(), String> {
    browser_go_forward(app).await
}
