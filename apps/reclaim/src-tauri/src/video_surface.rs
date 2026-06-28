//! Video Surface Manager for embedded GStreamer playback
//!
//! Uses GTK DrawingArea for proper video rendering integration with GStreamer.
//! The DrawingArea provides an X11 window that GStreamer's VideoOverlay can render into.
//!
//! Architecture:
//! 1. Create a GTK Window (popup type, no decorations)
//! 2. Add a DrawingArea inside for video rendering
//! 3. Get the DrawingArea's X11 window ID (XID)
//! 4. Reparent the GTK window under the Tauri window using X11
//! 5. Pass the XID to GStreamer's VideoOverlay via set_window_handle
//!
//! IMPORTANT: GStreamer must use a specific sink (xvimagesink, ximagesink, glimagesink)
//! NOT autovideosink, which ignores the window handle and creates its own window.

use std::collections::HashMap;
use std::sync::Mutex;
use lazy_static::lazy_static;
use tauri::Manager;

/// Bounds for positioning the video surface
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct SurfaceBounds {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// Emitted to the frontend when the native video surface is clicked, so the UI
/// can focus that pane and toggle play/pause (the native X11 window sits above
/// the DOM, so React click handlers never fire over the video).
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct VideoSurfaceClick {
    player_id: String,
}

/// Store video surface info - both the DrawingArea XID (for GStreamer) and GTK Window XID (for positioning)
/// GTK objects are not Send/Sync, so we only store the window IDs
#[derive(Debug, Clone)]
struct VideoSurfaceInfo {
    /// XID of the DrawingArea - this is what GStreamer renders into
    drawing_area_xid: u64,
    /// XID of the GTK Window - this is what we move/resize
    gtk_window_xid: u64,
}

lazy_static! {
    static ref VIDEO_SURFACES: Mutex<HashMap<String, VideoSurfaceInfo>> = Mutex::new(HashMap::new());
}

/// Create a video surface using GTK DrawingArea
/// Returns the X11 window ID (XID) for GStreamer's VideoOverlay
#[cfg(target_os = "linux")]
#[tauri::command(rename_all = "camelCase")]
pub async fn create_video_surface(
    window: tauri::Window,
    player_id: String,
    bounds: SurfaceBounds,
) -> Result<u64, String> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::sync::mpsc;
    use x11::xlib;
    use std::ptr;

    // Check if we're on X11
    let is_x11 = std::env::var("XDG_SESSION_TYPE").map(|v| v == "x11").unwrap_or(false)
        || std::env::var("GDK_BACKEND").map(|v| v == "x11").unwrap_or(false)
        || std::env::var("DISPLAY").is_ok();

    if !is_x11 {
        return Err("Video embedding requires X11".to_string());
    }

    // Get the parent window XID from Tauri
    let parent_xid = match window.window_handle() {
        Ok(handle) => {
            match handle.as_raw() {
                RawWindowHandle::Xlib(xlib_handle) => xlib_handle.window as u64,
                RawWindowHandle::Xcb(xcb_handle) => xcb_handle.window.get() as u64,
                _ => return Err("Unsupported window handle type".to_string()),
            }
        }
        Err(e) => return Err(format!("Failed to get window handle: {}", e)),
    };

    // Get scale factor for proper sizing
    let scale_factor = window.scale_factor().map_err(|e| e.to_string())?;

    // Apply scale factor and ensure minimum size
    let x = (bounds.x as f64 * scale_factor) as i32;
    let y = (bounds.y as f64 * scale_factor) as i32;
    let width = ((bounds.width as f64 * scale_factor).max(100.0)) as u32;
    let height = ((bounds.height as f64 * scale_factor).max(100.0)) as u32;

    log::info!(
        "Creating GTK video surface '{}' at ({}, {}) size {}x{}, parent XID: 0x{:x}, scale: {}",
        player_id, x, y, width, height, parent_xid, scale_factor
    );

    let player_id_for_closure = player_id.clone();
    let app = window.app_handle().clone();

    // Channel to receive the XID from the GTK thread
    let (tx, rx) = mpsc::channel();

    // GTK operations must run on the main GTK thread
    glib::MainContext::default().invoke(move || {
        use gtk::prelude::*;

        let result: Result<(u64, u64), String> = (|| {
            // Initialize GTK if needed (Tauri may have already done this)
            let _ = gtk::init();

            // Create a DrawingArea for video rendering
            let drawing_area = gtk::DrawingArea::new();
            drawing_area.set_size_request(width as i32, height as i32);

            // Set black background - this will show when video isn't playing
            drawing_area.connect_draw(|_widget, cr| {
                cr.set_source_rgb(0.0, 0.0, 0.0);
                cr.paint().ok();
                glib::Propagation::Stop  // Stop propagation - we handled the draw
            });

            // Forward clicks on the native video surface to the frontend so it can
            // focus this pane and toggle play/pause (handle-events is false on the
            // sink, so button events reach GTK rather than GStreamer navigation).
            drawing_area.add_events(gdk::EventMask::BUTTON_PRESS_MASK);
            let app_click = app.clone();
            let pid_click = player_id_for_closure.clone();
            drawing_area.connect_button_press_event(move |_widget, _event| {
                use tauri::Emitter;
                let _ = app_click.emit("video-surface-clicked", VideoSurfaceClick { player_id: pid_click.clone() });
                glib::Propagation::Proceed
            });

            // Create a window to hold the drawing area
            // Using Popup type to avoid window manager decorations
            let gtk_window = gtk::Window::new(gtk::WindowType::Popup);
            gtk_window.set_title(&format!("video-{}", player_id_for_closure));
            gtk_window.set_default_size(width as i32, height as i32);
            gtk_window.set_decorated(false);
            gtk_window.set_app_paintable(true);
            gtk_window.set_skip_taskbar_hint(true);
            gtk_window.set_skip_pager_hint(true);

            // Add the drawing area to the window
            gtk_window.add(&drawing_area);

            // Show all widgets - this realizes them and creates X11 windows
            gtk_window.show_all();

            // Get the DrawingArea's GDK window (this is what we want GStreamer to render into)
            let gdk_window = drawing_area.window()
                .ok_or("DrawingArea has no GDK window after show_all")?;

            // Get XID from the DrawingArea's GDK window
            let xid: u64 = unsafe {
                use glib::translate::ToGlibPtr;

                extern "C" {
                    fn gdk_x11_window_get_xid(window: *mut std::ffi::c_void) -> u64;
                }

                let ptr: *mut std::ffi::c_void = ToGlibPtr::<*mut gdk::ffi::GdkWindow>::to_glib_none(&gdk_window).0 as *mut _;
                gdk_x11_window_get_xid(ptr)
            };

            if xid == 0 {
                return Err("Failed to get XID from DrawingArea".to_string());
            }

            log::info!("GTK DrawingArea XID: 0x{:x}", xid);

            // Also get the GTK Window's XID for reparenting
            let gtk_window_gdk = gtk_window.window()
                .ok_or("GTK Window has no GDK window")?;

            let gtk_window_xid: u64 = unsafe {
                use glib::translate::ToGlibPtr;

                extern "C" {
                    fn gdk_x11_window_get_xid(window: *mut std::ffi::c_void) -> u64;
                }

                let ptr: *mut std::ffi::c_void = ToGlibPtr::<*mut gdk::ffi::GdkWindow>::to_glib_none(&gtk_window_gdk).0 as *mut _;
                gdk_x11_window_get_xid(ptr)
            };

            log::info!("GTK Window XID: 0x{:x}", gtk_window_xid);

            // Reparent the GTK window under the Tauri window using X11
            unsafe {
                let display = xlib::XOpenDisplay(ptr::null());
                if display.is_null() {
                    return Err("Failed to open X11 display for reparenting".to_string());
                }

                // Reparent the GTK window (not the DrawingArea) under the Tauri window
                xlib::XReparentWindow(
                    display,
                    gtk_window_xid as xlib::Window,
                    parent_xid as xlib::Window,
                    x,
                    y
                );

                // Make sure the window is visible and sized correctly
                xlib::XMoveResizeWindow(display, gtk_window_xid as xlib::Window, x, y, width, height);
                xlib::XMapWindow(display, gtk_window_xid as xlib::Window);

                // Raise to top of stacking order within parent
                xlib::XRaiseWindow(display, gtk_window_xid as xlib::Window);

                xlib::XSync(display, xlib::False);
                xlib::XCloseDisplay(display);

                log::info!(
                    "Reparented GTK window 0x{:x} under Tauri 0x{:x} at ({}, {}) size {}x{}",
                    gtk_window_xid, parent_xid, x, y, width, height
                );
            }

            // Prevent GTK window from being garbage collected
            // The window will be destroyed when we call destroy_video_surface
            std::mem::forget(gtk_window);

            // Return both XIDs: (DrawingArea XID for GStreamer, GTK Window XID for positioning)
            Ok((xid, gtk_window_xid))
        })();

        let _ = tx.send(result);
    });

    // Wait for result from GTK thread
    let (drawing_area_xid, gtk_window_xid) = rx.recv()
        .map_err(|e| format!("Failed to receive from GTK thread: {}", e))?
        .map_err(|e| format!("GTK surface creation failed: {}", e))?;

    // Store the surface info with both XIDs
    // This MUST succeed for video embedding to work, so use blocking lock
    {
        let mut surfaces = VIDEO_SURFACES.lock().map_err(|e| e.to_string())?;
        surfaces.insert(player_id.clone(), VideoSurfaceInfo {
            drawing_area_xid,
            gtk_window_xid,
        });
    }

    log::info!(
        "Video surface '{}' created - DrawingArea XID: 0x{:x}, GTK Window XID: 0x{:x}",
        player_id, drawing_area_xid, gtk_window_xid
    );

    // Return the DrawingArea XID for GStreamer's VideoOverlay
    Ok(drawing_area_xid)
}

#[cfg(not(target_os = "linux"))]
#[tauri::command(rename_all = "camelCase")]
pub async fn create_video_surface(
    _window: tauri::Window,
    _player_id: String,
    _bounds: SurfaceBounds,
) -> Result<u64, String> {
    Err("Video embedding only supported on Linux/X11".to_string())
}

/// Update the position and size of an existing video surface
#[cfg(target_os = "linux")]
#[tauri::command(rename_all = "camelCase")]
pub async fn update_video_surface(
    window: tauri::Window,
    player_id: String,
    bounds: SurfaceBounds,
) -> Result<(), String> {
    use x11::xlib;
    use std::ptr;

    // Get both XIDs quickly and release lock immediately - use try_lock to avoid blocking.
    let (gtk_window_xid, drawing_area_xid) = {
        let surfaces = match VIDEO_SURFACES.try_lock() {
            Ok(s) => s,
            Err(_) => {
                log::warn!("Could not acquire VIDEO_SURFACES lock for update_video_surface");
                return Ok(()); // Don't fail, just skip this update
            }
        };
        match surfaces.get(&player_id) {
            Some(info) => (info.gtk_window_xid, info.drawing_area_xid),
            None => return Ok(()), // Surface doesn't exist
        }
    };
    // Mutex is released here

    let scale_factor = window.scale_factor().map_err(|e| e.to_string())?;
    let x = (bounds.x as f64 * scale_factor) as i32;
    let y = (bounds.y as f64 * scale_factor) as i32;
    let width = ((bounds.width as f64 * scale_factor).max(100.0)) as u32;
    let height = ((bounds.height as f64 * scale_factor).max(100.0)) as u32;

    // Resize the outer GTK window AND the inner DrawingArea. The DrawingArea is
    // the actual VideoOverlay render target; because we resize via raw X11 (GTK
    // is unaware), GTK never re-allocates the child, so we must resize it too —
    // otherwise the video keeps the old size and the pane renders black on a
    // layout change.
    unsafe {
        let display = xlib::XOpenDisplay(ptr::null());
        if !display.is_null() {
            // Outer window: move + resize to the new pane bounds.
            xlib::XMoveResizeWindow(display, gtk_window_xid as xlib::Window, x, y, width, height);
            // Inner render surface: fill the outer window (origin 0,0).
            xlib::XMoveResizeWindow(display, drawing_area_xid as xlib::Window, 0, 0, width, height);
            // Use XFlush instead of XSync to avoid blocking
            xlib::XFlush(display);
            xlib::XCloseDisplay(display);
        }
    }

    log::debug!("Updated video surface '{}' to ({}, {}) size {}x{}", player_id, x, y, width, height);

    Ok(())
}

#[cfg(not(target_os = "linux"))]
#[tauri::command(rename_all = "camelCase")]
pub async fn update_video_surface(
    _window: tauri::Window,
    _player_id: String,
    _bounds: SurfaceBounds,
) -> Result<(), String> {
    Err("Video embedding only supported on Linux/X11".to_string())
}

/// Show the video surface
#[cfg(target_os = "linux")]
#[tauri::command(rename_all = "camelCase")]
pub async fn show_video_surface(player_id: String) -> Result<(), String> {
    use x11::xlib;
    use std::ptr;

    // Get XID - use blocking lock since this is called to check if surface exists
    let xid = {
        let surfaces = VIDEO_SURFACES.lock().map_err(|e| e.to_string())?;
        match surfaces.get(&player_id) {
            Some(info) => info.gtk_window_xid,
            None => return Err(format!("Video surface '{}' not found", player_id)), // Surface doesn't exist
        }
    };
    // Mutex is released here

    // Do X11 operations outside the lock
    unsafe {
        let display = xlib::XOpenDisplay(ptr::null());
        if !display.is_null() {
            xlib::XMapWindow(display, xid as xlib::Window);
            // Use XFlush instead of XSync to avoid blocking
            xlib::XFlush(display);
            xlib::XCloseDisplay(display);
        }
    }

    Ok(())
}

#[cfg(not(target_os = "linux"))]
#[tauri::command(rename_all = "camelCase")]
pub async fn show_video_surface(_player_id: String) -> Result<(), String> {
    Ok(())
}

/// Hide the video surface
#[cfg(target_os = "linux")]
#[tauri::command(rename_all = "camelCase")]
pub async fn hide_video_surface(player_id: String) -> Result<(), String> {
    use x11::xlib;
    use std::ptr;

    // Get XID quickly and release lock immediately - use try_lock to avoid blocking
    let xid = {
        let surfaces = match VIDEO_SURFACES.try_lock() {
            Ok(s) => s,
            Err(_) => {
                log::warn!("Could not acquire VIDEO_SURFACES lock for hide_video_surface");
                return Ok(()); // Don't fail, just skip
            }
        };
        match surfaces.get(&player_id) {
            Some(info) => info.gtk_window_xid,
            None => return Ok(()), // Surface doesn't exist, nothing to hide
        }
    };
    // Mutex is released here

    // Do X11 operations outside the lock
    unsafe {
        let display = xlib::XOpenDisplay(ptr::null());
        if !display.is_null() {
            xlib::XUnmapWindow(display, xid as xlib::Window);
            // Use XFlush instead of XSync to avoid blocking
            xlib::XFlush(display);
            xlib::XCloseDisplay(display);
        }
    }

    Ok(())
}

#[cfg(not(target_os = "linux"))]
#[tauri::command(rename_all = "camelCase")]
pub async fn hide_video_surface(_player_id: String) -> Result<(), String> {
    Ok(())
}

/// Destroy a video surface
#[cfg(target_os = "linux")]
#[tauri::command(rename_all = "camelCase")]
pub async fn destroy_video_surface(player_id: String) -> Result<(), String> {
    use x11::xlib;
    use std::ptr;

    // Get and remove the surface info, releasing lock quickly
    let info = {
        let mut surfaces = match VIDEO_SURFACES.try_lock() {
            Ok(s) => s,
            Err(_) => {
                log::warn!("Could not acquire VIDEO_SURFACES lock for destroy_video_surface");
                return Ok(()); // Don't fail, just skip
            }
        };
        surfaces.remove(&player_id)
    };
    // Mutex is released here

    if let Some(info) = info {
        unsafe {
            let display = xlib::XOpenDisplay(ptr::null());
            if !display.is_null() {
                // Destroy the GTK window (this will also destroy the DrawingArea inside it)
                xlib::XDestroyWindow(display, info.gtk_window_xid as xlib::Window);
                // Use XFlush instead of XSync to avoid blocking
                xlib::XFlush(display);
                xlib::XCloseDisplay(display);
            }
        }
        log::info!(
            "Destroyed video surface '{}' (GTK Window: 0x{:x}, DrawingArea: 0x{:x})",
            player_id, info.gtk_window_xid, info.drawing_area_xid
        );
    }

    Ok(())
}

#[cfg(not(target_os = "linux"))]
#[tauri::command(rename_all = "camelCase")]
pub async fn destroy_video_surface(_player_id: String) -> Result<(), String> {
    Ok(())
}

/// Get the XID for an existing video surface (returns the DrawingArea XID for GStreamer)
/// This MUST succeed for video embedding to work, so use blocking lock
#[tauri::command(rename_all = "camelCase")]
pub async fn get_video_surface_xid(player_id: String) -> Result<u64, String> {
    let surfaces = VIDEO_SURFACES.lock().map_err(|e| e.to_string())?;
    surfaces.get(&player_id)
        .map(|info| info.drawing_area_xid)
        .ok_or_else(|| format!("Video surface '{}' not found", player_id))
}

// ============================================================================
// Controls Overlay Window - GTK-based media controls that render above video
// ============================================================================

use std::sync::Arc;

/// Store for controls overlay windows with their GTK widgets
struct ControlsOverlayInfo {
    window_xid: u64,
    // Store button references for state updates
    is_playing: Arc<std::sync::atomic::AtomicBool>,
}

lazy_static! {
    static ref CONTROLS_OVERLAYS: Mutex<HashMap<String, ControlsOverlayInfo>> = Mutex::new(HashMap::new());
}

/// Create a GTK controls overlay window with actual media control buttons
/// This window renders above the video surface in X11 stacking order
#[cfg(target_os = "linux")]
#[tauri::command(rename_all = "camelCase")]
pub async fn create_controls_overlay(
    window: tauri::Window,
    player_id: String,
    bounds: SurfaceBounds,
) -> Result<u64, String> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::sync::mpsc;
    use x11::xlib;
    use std::ptr;

    // Check if we're on X11
    let is_x11 = std::env::var("XDG_SESSION_TYPE").map(|v| v == "x11").unwrap_or(false)
        || std::env::var("GDK_BACKEND").map(|v| v == "x11").unwrap_or(false)
        || std::env::var("DISPLAY").is_ok();

    if !is_x11 {
        return Err("Controls overlay requires X11".to_string());
    }

    // Get the parent window XID from Tauri
    let parent_xid = match window.window_handle() {
        Ok(handle) => {
            match handle.as_raw() {
                RawWindowHandle::Xlib(xlib_handle) => xlib_handle.window as u64,
                RawWindowHandle::Xcb(xcb_handle) => xcb_handle.window.get() as u64,
                _ => return Err("Unsupported window handle type".to_string()),
            }
        }
        Err(e) => return Err(format!("Failed to get window handle: {}", e)),
    };

    let scale_factor = window.scale_factor().map_err(|e| e.to_string())?;
    let x = (bounds.x as f64 * scale_factor) as i32;
    let y = (bounds.y as f64 * scale_factor) as i32;
    let width = ((bounds.width as f64 * scale_factor).max(200.0)) as u32;
    let height = ((bounds.height as f64 * scale_factor).max(60.0)) as u32;

    let overlay_id = format!("{}-controls", player_id);
    let player_id_for_closure = player_id.clone();

    log::info!(
        "Creating GTK controls overlay '{}' at ({}, {}) size {}x{}",
        overlay_id, x, y, width, height
    );

    let overlay_id_clone = overlay_id.clone();
    let (tx, rx) = mpsc::channel();

    // Create the overlay window on GTK thread
    glib::MainContext::default().invoke(move || {
        use gtk::prelude::*;

        let result: Result<(u64, Arc<std::sync::atomic::AtomicBool>), String> = (|| {
            let _ = gtk::init();

            // Create a GTK window for the controls
            let gtk_window = gtk::Window::new(gtk::WindowType::Popup);
            gtk_window.set_title(&format!("controls-{}", overlay_id_clone));
            gtk_window.set_default_size(width as i32, height as i32);
            gtk_window.set_decorated(false);
            gtk_window.set_app_paintable(true);
            gtk_window.set_skip_taskbar_hint(true);
            gtk_window.set_skip_pager_hint(true);

            // Make the window support transparency (ARGB visual)
            if let Some(screen) = gtk::prelude::WidgetExt::screen(&gtk_window) {
                if let Some(visual) = screen.rgba_visual() {
                    gtk_window.set_visual(Some(&visual));
                }
            }

            // Draw semi-transparent dark background
            gtk_window.connect_draw(|_widget, cr| {
                use gtk::cairo;
                // Dark semi-transparent background
                cr.set_source_rgba(0.1, 0.1, 0.1, 0.85);
                cr.set_operator(cairo::Operator::Source);
                cr.paint().ok();
                glib::Propagation::Proceed
            });

            // Create horizontal box for controls
            let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            hbox.set_margin_start(12);
            hbox.set_margin_end(12);
            hbox.set_margin_top(8);
            hbox.set_margin_bottom(8);
            hbox.set_halign(gtk::Align::Center);
            hbox.set_valign(gtk::Align::Center);

            // Track playing state
            let is_playing = Arc::new(std::sync::atomic::AtomicBool::new(false));

            // Create control buttons with icons
            let create_button = |icon_name: &str, tooltip: &str| -> gtk::Button {
                let btn = gtk::Button::new();
                btn.set_tooltip_text(Some(tooltip));
                btn.set_relief(gtk::ReliefStyle::None);

                // Set icon on button
                let icon = gtk::Image::from_icon_name(Some(icon_name), gtk::IconSize::LargeToolbar);
                btn.set_image(Some(&icon));

                // Style the button
                let css = gtk::CssProvider::new();
                css.load_from_data(b"
                    button {
                        background: transparent;
                        border: none;
                        border-radius: 50%;
                        min-width: 36px;
                        min-height: 36px;
                        padding: 6px;
                        color: white;
                    }
                    button:hover {
                        background: rgba(255,255,255,0.2);
                    }
                    button:active {
                        background: rgba(255,255,255,0.3);
                    }
                ").ok();
                btn.style_context().add_provider(&css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);

                btn
            };

            // Skip backward button
            let skip_back_btn = create_button("media-skip-backward-symbolic", "Previous");
            let player_id_back = player_id_for_closure.clone();
            skip_back_btn.connect_clicked(move |_| {
                log::info!("Controls: Skip backward clicked for player {}", player_id_back);
                // Emit event that TypeScript can listen to
                // For now, just log - we'll connect this to the player
            });
            hbox.pack_start(&skip_back_btn, false, false, 0);

            // Play/Pause button (larger, centered)
            let play_btn = create_button("media-playback-start-symbolic", "Play/Pause");
            let play_css = gtk::CssProvider::new();
            play_css.load_from_data(b"
                button {
                    background: #9333ea;
                    border: none;
                    border-radius: 50%;
                    min-width: 48px;
                    min-height: 48px;
                    padding: 8px;
                    color: white;
                }
                button:hover {
                    background: #a855f7;
                }
                button:active {
                    background: #7e22ce;
                }
            ").ok();
            play_btn.style_context().add_provider(&play_css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1);

            let is_playing_clone = is_playing.clone();
            let player_id_play = player_id_for_closure.clone();
            let play_btn_ref = play_btn.clone();
            play_btn.connect_clicked(move |_| {
                let currently_playing = is_playing_clone.load(std::sync::atomic::Ordering::SeqCst);
                log::info!("Controls: Play/Pause clicked for player {}, currently playing: {}", player_id_play, currently_playing);

                // Toggle state
                is_playing_clone.store(!currently_playing, std::sync::atomic::Ordering::SeqCst);

                // Update button icon
                let icon_name = if currently_playing {
                    "media-playback-start-symbolic"
                } else {
                    "media-playback-pause-symbolic"
                };
                let icon = gtk::Image::from_icon_name(Some(icon_name), gtk::IconSize::LargeToolbar);
                play_btn_ref.set_image(Some(&icon));
            });
            hbox.pack_start(&play_btn, false, false, 4);

            // Skip forward button
            let skip_fwd_btn = create_button("media-skip-forward-symbolic", "Next");
            let player_id_fwd = player_id_for_closure.clone();
            skip_fwd_btn.connect_clicked(move |_| {
                log::info!("Controls: Skip forward clicked for player {}", player_id_fwd);
            });
            hbox.pack_start(&skip_fwd_btn, false, false, 0);

            // Separator
            let sep = gtk::Separator::new(gtk::Orientation::Vertical);
            sep.set_margin_start(8);
            sep.set_margin_end(8);
            hbox.pack_start(&sep, false, false, 0);

            // Volume button
            let vol_btn = create_button("audio-volume-high-symbolic", "Volume");
            hbox.pack_start(&vol_btn, false, false, 0);

            // Fullscreen button
            let fs_btn = create_button("view-fullscreen-symbolic", "Fullscreen");
            hbox.pack_start(&fs_btn, false, false, 0);

            gtk_window.add(&hbox);
            gtk_window.show_all();

            // Get the window XID
            let gdk_window = gtk_window.window()
                .ok_or("GTK Window has no GDK window")?;

            let window_xid: u64 = unsafe {
                use glib::translate::ToGlibPtr;

                extern "C" {
                    fn gdk_x11_window_get_xid(window: *mut std::ffi::c_void) -> u64;
                }

                let ptr: *mut std::ffi::c_void = ToGlibPtr::<*mut gdk::ffi::GdkWindow>::to_glib_none(&gdk_window).0 as *mut _;
                gdk_x11_window_get_xid(ptr)
            };

            if window_xid == 0 {
                return Err("Failed to get XID from controls overlay window".to_string());
            }

            log::info!("Controls overlay window XID: 0x{:x}", window_xid);

            // Reparent under Tauri window and position above video surface
            unsafe {
                let display = xlib::XOpenDisplay(ptr::null());
                if display.is_null() {
                    return Err("Failed to open X11 display".to_string());
                }

                // Reparent under Tauri
                xlib::XReparentWindow(
                    display,
                    window_xid as xlib::Window,
                    parent_xid as xlib::Window,
                    x,
                    y
                );

                // Position and size
                xlib::XMoveResizeWindow(display, window_xid as xlib::Window, x, y, width, height);

                // Map and raise to top
                xlib::XMapWindow(display, window_xid as xlib::Window);
                xlib::XRaiseWindow(display, window_xid as xlib::Window);

                xlib::XSync(display, xlib::False);
                xlib::XCloseDisplay(display);
            }

            std::mem::forget(gtk_window);

            Ok((window_xid, is_playing))
        })();

        let _ = tx.send(result);
    });

    let (window_xid, is_playing) = rx.recv()
        .map_err(|e| format!("Failed to receive from GTK thread: {}", e))?
        .map_err(|e| format!("Controls overlay creation failed: {}", e))?;

    // Store the overlay
    {
        let mut overlays = CONTROLS_OVERLAYS.lock().map_err(|e| e.to_string())?;
        overlays.insert(overlay_id.clone(), ControlsOverlayInfo {
            window_xid,
            is_playing,
        });
    }

    log::info!("GTK Controls overlay created with XID: 0x{:x}", window_xid);

    Ok(window_xid)
}

#[cfg(not(target_os = "linux"))]
#[tauri::command(rename_all = "camelCase")]
pub async fn create_controls_overlay(
    _window: tauri::Window,
    _player_id: String,
    _bounds: SurfaceBounds,
) -> Result<u64, String> {
    Err("Controls overlay only supported on Linux/X11".to_string())
}

/// Update controls overlay position
#[cfg(target_os = "linux")]
#[tauri::command(rename_all = "camelCase")]
pub async fn update_controls_overlay(
    window: tauri::Window,
    player_id: String,
    bounds: SurfaceBounds,
) -> Result<(), String> {
    use x11::xlib;
    use std::ptr;

    let overlay_id = format!("{}-controls", player_id);
    let overlays = CONTROLS_OVERLAYS.lock().map_err(|e| e.to_string())?;
    let info = overlays.get(&overlay_id)
        .ok_or_else(|| format!("Controls overlay '{}' not found", overlay_id))?;

    let scale_factor = window.scale_factor().map_err(|e| e.to_string())?;
    let x = (bounds.x as f64 * scale_factor) as i32;
    let y = (bounds.y as f64 * scale_factor) as i32;
    let width = ((bounds.width as f64 * scale_factor).max(200.0)) as u32;
    let height = ((bounds.height as f64 * scale_factor).max(60.0)) as u32;

    unsafe {
        let display = xlib::XOpenDisplay(ptr::null());
        if display.is_null() {
            return Err("Failed to open X11 display".to_string());
        }

        xlib::XMoveResizeWindow(display, info.window_xid as xlib::Window, x, y, width, height);
        xlib::XRaiseWindow(display, info.window_xid as xlib::Window); // Keep on top
        xlib::XSync(display, xlib::False);
        xlib::XCloseDisplay(display);
    }

    Ok(())
}

#[cfg(not(target_os = "linux"))]
#[tauri::command(rename_all = "camelCase")]
pub async fn update_controls_overlay(
    _window: tauri::Window,
    _player_id: String,
    _bounds: SurfaceBounds,
) -> Result<(), String> {
    Err("Controls overlay only supported on Linux/X11".to_string())
}

/// Show controls overlay
#[cfg(target_os = "linux")]
#[tauri::command(rename_all = "camelCase")]
pub async fn show_controls_overlay(player_id: String) -> Result<(), String> {
    use x11::xlib;
    use std::ptr;

    let overlay_id = format!("{}-controls", player_id);
    let overlays = CONTROLS_OVERLAYS.lock().map_err(|e| e.to_string())?;
    let info = overlays.get(&overlay_id)
        .ok_or_else(|| format!("Controls overlay '{}' not found", overlay_id))?;

    unsafe {
        let display = xlib::XOpenDisplay(ptr::null());
        if !display.is_null() {
            xlib::XMapWindow(display, info.window_xid as xlib::Window);
            xlib::XRaiseWindow(display, info.window_xid as xlib::Window);
            xlib::XSync(display, xlib::False);
            xlib::XCloseDisplay(display);
        }
    }

    Ok(())
}

#[cfg(not(target_os = "linux"))]
#[tauri::command(rename_all = "camelCase")]
pub async fn show_controls_overlay(_player_id: String) -> Result<(), String> {
    Ok(())
}

/// Hide controls overlay
#[cfg(target_os = "linux")]
#[tauri::command(rename_all = "camelCase")]
pub async fn hide_controls_overlay(player_id: String) -> Result<(), String> {
    use x11::xlib;
    use std::ptr;

    let overlay_id = format!("{}-controls", player_id);
    let overlays = CONTROLS_OVERLAYS.lock().map_err(|e| e.to_string())?;
    let info = overlays.get(&overlay_id)
        .ok_or_else(|| format!("Controls overlay '{}' not found", overlay_id))?;

    unsafe {
        let display = xlib::XOpenDisplay(ptr::null());
        if !display.is_null() {
            xlib::XUnmapWindow(display, info.window_xid as xlib::Window);
            xlib::XSync(display, xlib::False);
            xlib::XCloseDisplay(display);
        }
    }

    Ok(())
}

#[cfg(not(target_os = "linux"))]
#[tauri::command(rename_all = "camelCase")]
pub async fn hide_controls_overlay(_player_id: String) -> Result<(), String> {
    Ok(())
}

/// Destroy controls overlay
#[cfg(target_os = "linux")]
#[tauri::command(rename_all = "camelCase")]
pub async fn destroy_controls_overlay(player_id: String) -> Result<(), String> {
    use x11::xlib;
    use std::ptr;

    let overlay_id = format!("{}-controls", player_id);
    let mut overlays = match CONTROLS_OVERLAYS.try_lock() {
        Ok(o) => o,
        Err(_) => return Ok(()),
    };

    if let Some(info) = overlays.remove(&overlay_id) {
        unsafe {
            let display = xlib::XOpenDisplay(ptr::null());
            if !display.is_null() {
                xlib::XDestroyWindow(display, info.window_xid as xlib::Window);
                xlib::XSync(display, xlib::False);
                xlib::XCloseDisplay(display);
            }
        }
        log::info!("Destroyed controls overlay '{}' (XID: 0x{:x})", overlay_id, info.window_xid);
    }

    Ok(())
}

#[cfg(not(target_os = "linux"))]
#[tauri::command(rename_all = "camelCase")]
pub async fn destroy_controls_overlay(_player_id: String) -> Result<(), String> {
    Ok(())
}

// ============================================================================
// Floating Webview Controls Window - Tauri webview that floats above video
// ============================================================================

lazy_static! {
    static ref CONTROLS_WEBVIEW_CREATED: Mutex<bool> = Mutex::new(false);
}

/// Create a floating Tauri webview window for media controls
/// This renders HTML/React controls in a separate window that floats above the video
#[tauri::command(rename_all = "camelCase")]
pub async fn create_floating_controls_window(
    app: tauri::AppHandle,
    bounds: SurfaceBounds,
) -> Result<(), String> {
    use tauri::{WebviewUrl, WebviewWindowBuilder};

    // Only create one controls window
    {
        let mut created = CONTROLS_WEBVIEW_CREATED.lock().map_err(|e| e.to_string())?;
        if *created {
            log::info!("Floating controls window already exists");
            return Ok(());
        }
        *created = true;
    }

    log::info!("Creating floating controls webview window at ({}, {}) size {}x{}",
        bounds.x, bounds.y, bounds.width, bounds.height);

    // Create a new webview window for controls
    let controls_window = WebviewWindowBuilder::new(
        &app,
        "media-controls",
        WebviewUrl::App("/media-controls".into())
    )
    .title("Media Controls")
    .inner_size(bounds.width as f64, bounds.height as f64)
    .position(bounds.x as f64, bounds.y as f64)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(false)
    .focused(false)
    .visible(true)
    .build()
    .map_err(|e| format!("Failed to create controls window: {}", e))?;

    log::info!("Floating controls webview window created: {:?}", controls_window.label());

    Ok(())
}

/// Update floating controls window position
#[tauri::command(rename_all = "camelCase")]
pub async fn update_floating_controls_window(
    app: tauri::AppHandle,
    bounds: SurfaceBounds,
) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("media-controls") {
        window.set_position(tauri::Position::Physical(tauri::PhysicalPosition {
            x: bounds.x,
            y: bounds.y,
        })).map_err(|e| e.to_string())?;

        window.set_size(tauri::Size::Physical(tauri::PhysicalSize {
            width: bounds.width as u32,
            height: bounds.height as u32,
        })).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Show floating controls window
#[tauri::command(rename_all = "camelCase")]
pub async fn show_floating_controls_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("media-controls") {
        window.show().map_err(|e| e.to_string())?;
        window.set_focus().ok(); // Don't fail if focus fails
    }
    Ok(())
}

/// Hide floating controls window
#[tauri::command(rename_all = "camelCase")]
pub async fn hide_floating_controls_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("media-controls") {
        window.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Destroy floating controls window
#[tauri::command(rename_all = "camelCase")]
pub async fn destroy_floating_controls_window(app: tauri::AppHandle) -> Result<(), String> {
    {
        let mut created = CONTROLS_WEBVIEW_CREATED.lock().map_err(|e| e.to_string())?;
        *created = false;
    }

    if let Some(window) = app.get_webview_window("media-controls") {
        window.destroy().map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ============================================================================
// X11 Window with WebKitGTK Webview - HTML controls in a proper X11 window
// ============================================================================

/// Store for X11 webview controls windows
struct X11WebviewControlsInfo {
    gtk_window_xid: u64,
}

lazy_static! {
    static ref X11_WEBVIEW_CONTROLS: tokio::sync::Mutex<Option<X11WebviewControlsInfo>> = tokio::sync::Mutex::new(None);
    /// Flag to prevent race conditions during creation
    static ref X11_WEBVIEW_CONTROLS_CREATING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
}

/// Create an X11 window containing a WebKitGTK webview for HTML media controls
/// This window can be positioned above the video and supports transparency
#[cfg(target_os = "linux")]
#[tauri::command(rename_all = "camelCase")]
pub async fn create_x11_webview_controls(
    window: tauri::Window,
    bounds: SurfaceBounds,
    url: String,
) -> Result<u64, String> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::sync::mpsc;
    use x11::xlib;
    use std::ptr;

    log::info!("=== create_x11_webview_controls CALLED ===");
    log::info!("  URL: {}", url);
    log::info!("  Bounds: x={}, y={}, w={}, h={}", bounds.x, bounds.y, bounds.width, bounds.height);

    // Check if already created or creation in progress (prevent race conditions)
    {
        use std::sync::atomic::Ordering;

        // Check if already being created by another call
        if X11_WEBVIEW_CONTROLS_CREATING.swap(true, Ordering::SeqCst) {
            log::warn!("⚠ X11 webview controls creation already in progress");
            return Ok(0);
        }
        log::info!("✓ Set CREATING flag");

        // Check if already created (async lock)
        log::info!("Acquiring lock to check if controls exist...");
        let existing = X11_WEBVIEW_CONTROLS.lock().await;
        log::info!("✓ Lock acquired");
        if existing.is_some() {
            X11_WEBVIEW_CONTROLS_CREATING.store(false, Ordering::SeqCst);
            log::warn!("⚠ X11 webview controls already exist, aborting");
            return Ok(0);
        }
        log::info!("✓ No existing controls, proceeding with creation");
        drop(existing);
    }

    // Check if we're on X11
    log::info!("Checking X11 environment...");
    let session_type = std::env::var("XDG_SESSION_TYPE").ok();
    let gdk_backend = std::env::var("GDK_BACKEND").ok();
    let display = std::env::var("DISPLAY").ok();
    log::info!("  XDG_SESSION_TYPE: {:?}", session_type);
    log::info!("  GDK_BACKEND: {:?}", gdk_backend);
    log::info!("  DISPLAY: {:?}", display);

    let is_x11 = session_type.as_deref() == Some("x11")
        || gdk_backend.as_deref() == Some("x11")
        || display.is_some();

    if !is_x11 {
        X11_WEBVIEW_CONTROLS_CREATING.store(false, std::sync::atomic::Ordering::SeqCst);
        log::error!("✗ Not running on X11!");
        return Err("X11 webview controls require X11".to_string());
    }
    log::info!("✓ Running on X11");

    // Get the parent window XID from Tauri
    let parent_xid = match window.window_handle() {
        Ok(handle) => {
            match handle.as_raw() {
                RawWindowHandle::Xlib(xlib_handle) => xlib_handle.window as u64,
                RawWindowHandle::Xcb(xcb_handle) => xcb_handle.window.get() as u64,
                _ => {
                    X11_WEBVIEW_CONTROLS_CREATING.store(false, std::sync::atomic::Ordering::SeqCst);
                    return Err("Unsupported window handle type".to_string());
                }
            }
        }
        Err(e) => {
            X11_WEBVIEW_CONTROLS_CREATING.store(false, std::sync::atomic::Ordering::SeqCst);
            return Err(format!("Failed to get window handle: {}", e));
        }
    };

    let scale_factor = window.scale_factor().map_err(|e| {
        X11_WEBVIEW_CONTROLS_CREATING.store(false, std::sync::atomic::Ordering::SeqCst);
        e.to_string()
    })?;
    let x = (bounds.x as f64 * scale_factor) as i32;
    let y = (bounds.y as f64 * scale_factor) as i32;
    // Use exact size - no minimum padding (the HTML controls are self-contained)
    let width = (bounds.width as f64 * scale_factor) as u32;
    let height = (bounds.height as f64 * scale_factor) as u32;

    log::info!(
        "Creating X11 webview controls at ({}, {}) size {}x{}, URL: {}, parent XID: 0x{:x}",
        x, y, width, height, url, parent_xid
    );

    let (tx, rx) = mpsc::channel();
    let url_clone = url.clone();

    // Create the window on GTK thread
    glib::MainContext::default().invoke(move || {
        use gtk::prelude::*;
        use webkit2gtk::{WebView, WebContext, WebViewExt, SettingsExt};

        let result: Result<u64, String> = (|| {
            log::info!("→ GTK thread: Initializing GTK...");
            let _ = gtk::init();
            log::info!("✓ GTK initialized");

            // Create as Popup (override-redirect) so the window manager does NOT
            // manage or re-place it. A managed Toplevel was being force-centered
            // by the WM regardless of our requested position; Popup is bypassed by
            // the WM entirely, so XMoveResizeWindow positions it exactly at the
            // requested bottom-center coordinates (same approach as the video surface).
            log::info!("Creating GTK Popup (override-redirect) window...");
            let gtk_window = gtk::Window::new(gtk::WindowType::Popup);
            gtk_window.set_title("Media Controls");
            gtk_window.set_default_size(width as i32, height as i32);
            gtk_window.set_decorated(false);
            gtk_window.set_app_paintable(true);
            // Don't skip taskbar - we want it to show as a separate window preview
            gtk_window.set_skip_pager_hint(true);
            log::info!("✓ GTK window created: {}x{}", width, height);

            // Enable transparency (ARGB visual)
            if let Some(screen) = gtk::prelude::WidgetExt::screen(&gtk_window) {
                if let Some(visual) = screen.rgba_visual() {
                    gtk_window.set_visual(Some(&visual));
                    log::info!("Set RGBA visual for transparent window");
                } else {
                    log::warn!("No RGBA visual available - transparency may not work");
                }
            }

            // Make window background transparent
            gtk_window.connect_draw(|_widget, cr| {
                use gtk::cairo;
                cr.set_source_rgba(0.0, 0.0, 0.0, 0.0); // Fully transparent
                cr.set_operator(cairo::Operator::Source);
                cr.paint().ok();
                glib::Propagation::Proceed
            });

            // Create WebKitGTK webview with custom context
            let web_context = WebContext::default().expect("Failed to get web context");
            let webview = WebView::with_context(&web_context);

            // IMPORTANT: Set RGBA visual on webview widget for transparency
            if let Some(screen) = gtk::prelude::WidgetExt::screen(&webview) {
                if let Some(visual) = screen.rgba_visual() {
                    webview.set_visual(Some(&visual));
                }
            }
            webview.set_app_paintable(true);

            // Configure webview settings
            if let Some(settings) = WebViewExt::settings(&webview) {
                SettingsExt::set_enable_developer_extras(&settings, true);
                SettingsExt::set_javascript_can_access_clipboard(&settings, true);
                SettingsExt::set_allow_file_access_from_file_urls(&settings, true);
                SettingsExt::set_allow_universal_access_from_file_urls(&settings, true);
            }

            // Make webview background transparent - MUST be called before loading content
            // This tells WebKit to render with alpha channel support
            WebViewExt::set_background_color(&webview, &gdk::RGBA::new(0.0, 0.0, 0.0, 0.0));

            // Set the webview to fill the window
            webview.set_hexpand(true);
            webview.set_vexpand(true);

            // Load the controls URL
            log::info!("Loading URL into webview: {}", url_clone);
            WebViewExt::load_uri(&webview, &url_clone);

            // Add webview to window
            log::info!("Adding webview to window...");
            gtk_window.add(&webview);
            log::info!("Calling show_all()...");
            gtk_window.show_all();
            log::info!("✓ Window and webview shown");

            // Get window XID
            log::info!("Getting GDK window...");
            let gdk_window = gtk_window.window()
                .ok_or("GTK Window has no GDK window")?;
            log::info!("✓ Got GDK window");

            log::info!("Getting X11 XID...");
            let window_xid: u64 = unsafe {
                use glib::translate::ToGlibPtr;

                extern "C" {
                    fn gdk_x11_window_get_xid(window: *mut std::ffi::c_void) -> u64;
                }

                let ptr: *mut std::ffi::c_void = ToGlibPtr::<*mut gdk::ffi::GdkWindow>::to_glib_none(&gdk_window).0 as *mut _;
                gdk_x11_window_get_xid(ptr)
            };

            if window_xid == 0 {
                log::error!("✗ Failed to get XID!");
                return Err("Failed to get XID from webview controls window".to_string());
            }

            log::info!("✓ Got X11 XID: 0x{:x}", window_xid);

            // Position the controls window and set X11 properties
            log::info!("Setting X11 window properties...");
            unsafe {
                log::info!("Opening X11 display...");
                let display = xlib::XOpenDisplay(ptr::null());
                if display.is_null() {
                    log::error!("✗ Failed to open X11 display");
                    return Err("Failed to open X11 display".to_string());
                }
                log::info!("✓ X11 display opened");

                // Set WM_CLASS property (required for taskbar identification)
                log::info!("Setting WM_CLASS...");
                let class_name = std::ffi::CString::new("MediaControls").unwrap();
                let class_class = std::ffi::CString::new("Reclaim").unwrap();
                let mut class_hint = xlib::XClassHint {
                    res_name: class_name.as_ptr() as *mut i8,
                    res_class: class_class.as_ptr() as *mut i8,
                };
                xlib::XSetClassHint(display, window_xid as xlib::Window, &mut class_hint);
                log::info!("✓ WM_CLASS set");

                // Set _NET_WM_WINDOW_TYPE to _NET_WM_WINDOW_TYPE_UTILITY for proper taskbar appearance
                log::info!("Setting _NET_WM_WINDOW_TYPE...");
                let net_wm_window_type = xlib::XInternAtom(display, b"_NET_WM_WINDOW_TYPE\0".as_ptr() as *const i8, xlib::False);
                let net_wm_window_type_utility = xlib::XInternAtom(display, b"_NET_WM_WINDOW_TYPE_UTILITY\0".as_ptr() as *const i8, xlib::False);
                xlib::XChangeProperty(
                    display,
                    window_xid as xlib::Window,
                    net_wm_window_type,
                    xlib::XA_ATOM,
                    32,
                    xlib::PropModeReplace,
                    &net_wm_window_type_utility as *const _ as *const u8,
                    1
                );
                log::info!("✓ _NET_WM_WINDOW_TYPE set");

                // The frontend now passes absolute screen coordinates directly
                // No need to calculate - just use the bounds as-is
                let abs_x = x;
                let abs_y = y;

                log::info!("Controls will be at absolute position ({}, {}) size {}x{}", abs_x, abs_y, width, height);

                // DON'T reparent - keep as toplevel window
                // Just move to the correct screen position
                log::info!("Moving and resizing window...");
                xlib::XMoveResizeWindow(display, window_xid as xlib::Window, abs_x, abs_y, width, height);
                log::info!("✓ Window moved and resized");

                // NOTE: We intentionally do NOT set WM_TRANSIENT_FOR here. A
                // transient window is treated by most window managers as a dialog
                // and CENTERED over its parent — which made the controls spawn in
                // the middle instead of the bottom-center position we requested.
                // _NET_WM_STATE_ABOVE (below) keeps it on top without the centering.

                // Set _NET_WM_STATE to prevent focus stealing
                log::info!("Setting _NET_WM_STATE properties...");
                let net_wm_state = xlib::XInternAtom(display, b"_NET_WM_STATE\0".as_ptr() as *const i8, xlib::False);
                let net_wm_state_above = xlib::XInternAtom(display, b"_NET_WM_STATE_ABOVE\0".as_ptr() as *const i8, xlib::False);
                // Note: _NET_WM_STATE_SKIP_TASKBAR is intentionally not used - we want taskbar visibility

                // Keep window above without stealing focus
                let states = vec![net_wm_state_above];

                xlib::XChangeProperty(
                    display,
                    window_xid as xlib::Window,
                    net_wm_state,
                    xlib::XA_ATOM,
                    32,
                    xlib::PropModeReplace,
                    states.as_ptr() as *const u8,
                    states.len() as i32
                );
                log::info!("✓ _NET_WM_STATE set");

                // Set input hint to prevent focus stealing
                log::info!("Setting WM hints to prevent focus...");
                let wm_hints = xlib::XAllocWMHints();
                if !wm_hints.is_null() {
                    (*wm_hints).flags = xlib::InputHint;
                    (*wm_hints).input = xlib::False; // Don't accept input focus
                    xlib::XSetWMHints(display, window_xid as xlib::Window, wm_hints);
                    xlib::XFree(wm_hints as *mut _);
                    log::info!("✓ WM hints set to prevent focus");
                } else {
                    log::warn!("⚠ Failed to allocate WM hints");
                }

                // NOTE: We intentionally do NOT set override_redirect here
                // This allows the window manager to manage the window and show it in taskbar/previews

                // Map and raise to top
                log::info!("Mapping window...");
                xlib::XMapWindow(display, window_xid as xlib::Window);
                log::info!("✓ Window mapped");
                log::info!("Raising window to top...");
                xlib::XRaiseWindow(display, window_xid as xlib::Window);
                log::info!("✓ Window raised");

                xlib::XSync(display, xlib::False);
                xlib::XCloseDisplay(display);
                log::info!("✓ X11 operations complete");
            }

            // Prevent GTK window from being garbage collected
            std::mem::forget(gtk_window);
            std::mem::forget(webview);

            Ok(window_xid)
        })();

        let _ = tx.send(result);
    });

    use std::sync::atomic::Ordering;

    log::info!("Waiting for GTK thread result...");
    let window_xid = rx.recv()
        .map_err(|e| {
            X11_WEBVIEW_CONTROLS_CREATING.store(false, Ordering::SeqCst);
            log::error!("✗ Failed to receive from GTK thread: {}", e);
            format!("Failed to receive from GTK thread: {}", e)
        })?
        .map_err(|e| {
            X11_WEBVIEW_CONTROLS_CREATING.store(false, Ordering::SeqCst);
            log::error!("✗ X11 webview controls creation failed: {}", e);
            format!("X11 webview controls creation failed: {}", e)
        })?;
    log::info!("✓ Received XID from GTK thread: 0x{:x}", window_xid);

    // Store the info (async lock)
    log::info!("Storing controls info...");
    {
        let mut controls = X11_WEBVIEW_CONTROLS.lock().await;
        *controls = Some(X11WebviewControlsInfo { gtk_window_xid: window_xid });
    }
    log::info!("✓ Controls info stored");

    // Reset creation flag (creation complete)
    X11_WEBVIEW_CONTROLS_CREATING.store(false, Ordering::SeqCst);
    log::info!("✓ CREATING flag cleared");

    log::info!("=== ✓ X11 webview controls created successfully with XID: 0x{:x} ===", window_xid);

    Ok(window_xid)
}

#[cfg(not(target_os = "linux"))]
#[tauri::command(rename_all = "camelCase")]
pub async fn create_x11_webview_controls(
    _window: tauri::Window,
    _bounds: SurfaceBounds,
    _url: String,
) -> Result<u64, String> {
    Err("X11 webview controls only supported on Linux/X11".to_string())
}

/// Update X11 webview controls position
#[cfg(target_os = "linux")]
#[tauri::command(rename_all = "camelCase")]
pub async fn update_x11_webview_controls(
    window: tauri::Window,
    bounds: SurfaceBounds,
) -> Result<(), String> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use x11::xlib;
    use std::ptr;

    // Use async lock
    let controls = X11_WEBVIEW_CONTROLS.lock().await;
    let xid = match controls.as_ref() {
        Some(info) => info.gtk_window_xid,
        None => {
            log::warn!("Controls not created yet in update");
            return Ok(()); // Controls not created yet
        }
    };
    drop(controls); // Release lock before other operations

    // Get the parent window XID
    let parent_xid = match window.window_handle() {
        Ok(handle) => {
            match handle.as_raw() {
                RawWindowHandle::Xlib(xlib_handle) => xlib_handle.window as u64,
                RawWindowHandle::Xcb(xcb_handle) => xcb_handle.window.get() as u64,
                _ => return Err("Unsupported window handle type".to_string()),
            }
        }
        Err(e) => return Err(format!("Failed to get window handle: {}", e)),
    };

    let scale_factor = window.scale_factor().map_err(|e| e.to_string())?;
    let x = (bounds.x as f64 * scale_factor) as i32;
    let y = (bounds.y as f64 * scale_factor) as i32;
    // Use exact size - no minimum padding
    let width = (bounds.width as f64 * scale_factor) as u32;
    let height = (bounds.height as f64 * scale_factor) as u32;

    unsafe {
        let display = xlib::XOpenDisplay(ptr::null());
        if display.is_null() {
            return Err("Failed to open X11 display".to_string());
        }

        // Get root window
        let mut root_return: xlib::Window = 0;
        let mut parent_return: xlib::Window = 0;
        let mut children_return: *mut xlib::Window = ptr::null_mut();
        let mut nchildren: u32 = 0;
        xlib::XQueryTree(display, parent_xid as xlib::Window, &mut root_return, &mut parent_return, &mut children_return, &mut nchildren);
        if !children_return.is_null() {
            xlib::XFree(children_return as *mut _);
        }

        // Get absolute position of main window
        let mut main_x: i32 = 0;
        let mut main_y: i32 = 0;
        let mut child: xlib::Window = 0;
        xlib::XTranslateCoordinates(
            display,
            parent_xid as xlib::Window,
            root_return,
            0, 0,
            &mut main_x, &mut main_y,
            &mut child
        );

        // Calculate absolute screen position
        let abs_x = main_x + x;
        let abs_y = main_y + y;

        xlib::XMoveResizeWindow(display, xid as xlib::Window, abs_x, abs_y, width, height);
        xlib::XRaiseWindow(display, xid as xlib::Window); // Keep on top
        xlib::XSync(display, xlib::False);
        xlib::XCloseDisplay(display);
    }

    Ok(())
}

#[cfg(not(target_os = "linux"))]
#[tauri::command(rename_all = "camelCase")]
pub async fn update_x11_webview_controls(
    _window: tauri::Window,
    _bounds: SurfaceBounds,
) -> Result<(), String> {
    Err("X11 webview controls only supported on Linux/X11".to_string())
}

/// Show X11 webview controls
#[cfg(target_os = "linux")]
#[tauri::command(rename_all = "camelCase")]
pub async fn show_x11_webview_controls() -> Result<(), String> {
    use x11::xlib;
    use std::ptr;

    // Use tokio::sync::Mutex which is async-compatible
    let controls = X11_WEBVIEW_CONTROLS.lock().await;
    let xid = match controls.as_ref() {
        Some(info) => info.gtk_window_xid,
        None => {
            log::warn!("Controls not created yet in show");
            return Ok(()); // Controls not created yet
        }
    };
    drop(controls); // Release lock before X11 operations

    unsafe {
        let display = xlib::XOpenDisplay(ptr::null());
        if !display.is_null() {
            xlib::XMapWindow(display, xid as xlib::Window);
            xlib::XRaiseWindow(display, xid as xlib::Window);
            // Use XFlush instead of XSync to avoid blocking
            xlib::XFlush(display);
            xlib::XCloseDisplay(display);
            log::info!("Showed X11 webview controls (XID: 0x{:x})", xid);
        } else {
            log::error!("Failed to open X11 display in show");
        }
    }

    Ok(())
}

#[cfg(not(target_os = "linux"))]
#[tauri::command(rename_all = "camelCase")]
pub async fn show_x11_webview_controls() -> Result<(), String> {
    Ok(())
}

/// Resize the X11 webview controls window IN PLACE (keeps position). Used when the
/// controls collapse/expand: the DOM container shrinks but the native window must
/// shrink too, or the collapsed bar floats inside a full-size window. `width`/
/// `height` are PHYSICAL pixels (the frontend multiplies by devicePixelRatio).
/// XResizeWindow keeps the current position; GTK re-allocates the inner webview.
#[cfg(target_os = "linux")]
#[tauri::command(rename_all = "camelCase")]
pub async fn resize_x11_webview_controls(width: u32, height: u32) -> Result<(), String> {
    use x11::xlib;
    use std::ptr;

    let xid = {
        let controls = X11_WEBVIEW_CONTROLS.lock().await;
        match controls.as_ref() {
            Some(info) => info.gtk_window_xid,
            None => return Ok(()), // not created yet
        }
    };

    unsafe {
        let display = xlib::XOpenDisplay(ptr::null());
        if display.is_null() {
            return Err("Failed to open X11 display in resize".to_string());
        }
        xlib::XResizeWindow(display, xid as xlib::Window, width.max(1), height.max(1));
        xlib::XFlush(display);
        xlib::XCloseDisplay(display);
    }

    Ok(())
}

#[cfg(not(target_os = "linux"))]
#[tauri::command(rename_all = "camelCase")]
pub async fn resize_x11_webview_controls(_width: u32, _height: u32) -> Result<(), String> {
    Ok(())
}

/// Hide X11 webview controls
#[cfg(target_os = "linux")]
#[tauri::command(rename_all = "camelCase")]
pub async fn hide_x11_webview_controls() -> Result<(), String> {
    use x11::xlib;
    use std::ptr;

    // Use tokio::sync::Mutex which is async-compatible
    let controls = X11_WEBVIEW_CONTROLS.lock().await;
    let xid = match controls.as_ref() {
        Some(info) => info.gtk_window_xid,
        None => {
            log::warn!("Controls not created yet in hide");
            return Ok(()); // Controls not created yet
        }
    };
    drop(controls); // Release lock before X11 operations

    unsafe {
        let display = xlib::XOpenDisplay(ptr::null());
        if !display.is_null() {
            xlib::XUnmapWindow(display, xid as xlib::Window);
            // Use XFlush instead of XSync to avoid blocking
            xlib::XFlush(display);
            xlib::XCloseDisplay(display);
            log::info!("Hid X11 webview controls (XID: 0x{:x})", xid);
        } else {
            log::error!("Failed to open X11 display in hide");
        }
    }

    Ok(())
}

#[cfg(not(target_os = "linux"))]
#[tauri::command(rename_all = "camelCase")]
pub async fn hide_x11_webview_controls() -> Result<(), String> {
    Ok(())
}

/// Destroy X11 webview controls
#[cfg(target_os = "linux")]
#[tauri::command(rename_all = "camelCase")]
pub async fn destroy_x11_webview_controls() -> Result<(), String> {
    use x11::xlib;
    use std::ptr;

    // Use async lock
    let mut controls = X11_WEBVIEW_CONTROLS.lock().await;

    if let Some(info) = controls.take() {
        let xid = info.gtk_window_xid;
        drop(controls); // Release lock before X11 operations

        unsafe {
            let display = xlib::XOpenDisplay(ptr::null());
            if !display.is_null() {
                xlib::XDestroyWindow(display, xid as xlib::Window);
                // Use XFlush instead of XSync to avoid blocking
                xlib::XFlush(display);
                xlib::XCloseDisplay(display);
            }
        }
        log::info!("Destroyed X11 webview controls (XID: 0x{:x})", xid);

        // Reset the creation flag so controls can be recreated
        X11_WEBVIEW_CONTROLS_CREATING.store(false, std::sync::atomic::Ordering::SeqCst);
    }

    Ok(())
}

#[cfg(not(target_os = "linux"))]
#[tauri::command(rename_all = "camelCase")]
pub async fn destroy_x11_webview_controls() -> Result<(), String> {
    Ok(())
}

/// Move X11 webview controls window by delta (for drag functionality)
#[cfg(target_os = "linux")]
pub fn move_x11_webview_controls_by_delta(delta_x: i32, delta_y: i32) -> Result<(), String> {
    use x11::xlib;
    use std::ptr;

    // Try to get XID without blocking - if lock is busy, skip this move
    // This prevents blocking the WebSocket thread during drags
    let xid = match X11_WEBVIEW_CONTROLS.try_lock() {
        Ok(controls) => {
            match controls.as_ref() {
                Some(info) => info.gtk_window_xid,
                None => return Ok(()), // Controls not created
            }
        }
        Err(_) => {
            // Lock is busy, skip this frame - dragging will send many events
            return Ok(());
        }
    };
    // Lock is automatically dropped here

    unsafe {
        let display = xlib::XOpenDisplay(ptr::null());
        if display.is_null() {
            return Err("Failed to open X11 display".to_string());
        }

        // Get current window position
        let mut attrs: xlib::XWindowAttributes = std::mem::zeroed();
        if xlib::XGetWindowAttributes(display, xid as xlib::Window, &mut attrs) == 0 {
            xlib::XCloseDisplay(display);
            return Err("Failed to get window attributes".to_string());
        }

        // Calculate new position
        let new_x = attrs.x + delta_x;
        let new_y = attrs.y + delta_y;

        // Move the window
        xlib::XMoveWindow(display, xid as xlib::Window, new_x, new_y);
        xlib::XRaiseWindow(display, xid as xlib::Window); // Keep on top
        // Use XFlush instead of XSync to avoid blocking
        xlib::XFlush(display);
        xlib::XCloseDisplay(display);
    }

    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn move_x11_webview_controls_by_delta(_delta_x: i32, _delta_y: i32) -> Result<(), String> {
    Ok(())
}
