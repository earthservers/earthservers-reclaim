//! GStreamer media player integration for Tauri commands
//!
//! Provides Tauri commands that wrap the earth-media crate.
//! Supports multi-player management for multi-pane playback.
//! Includes YouTube support via yt-dlp.
//! Supports VideoOverlay for embedded video playback in the app window.

use earth_media::{MediaPlayer, MediaPlayerManager, PlayerStatus, VideoInfo};
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::State;
use lazy_static::lazy_static;

#[cfg(target_os = "linux")]
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

// Global MediaPlayerManager for internal use (WebSocket controls, etc.)
lazy_static! {
    static ref GLOBAL_PLAYER_MANAGER: MediaPlayerManager = MediaPlayerManager::new();
}

// ==================== Internal Functions (for WebSocket controls) ====================
// These functions don't require Tauri State and can be called from anywhere

/// Internal: Play on a specific player
pub async fn player_play_internal(player_id: &str) -> Result<(), String> {
    GLOBAL_PLAYER_MANAGER.play(player_id).map_err(|e| e.to_string())
}

/// Internal: Pause on a specific player
pub async fn player_pause_internal(player_id: &str) -> Result<(), String> {
    GLOBAL_PLAYER_MANAGER.pause(player_id).map_err(|e| e.to_string())
}

/// Internal: Stop on a specific player
pub async fn player_stop_internal(player_id: &str) -> Result<(), String> {
    GLOBAL_PLAYER_MANAGER.stop(player_id).map_err(|e| e.to_string())
}

/// Internal: Seek on a specific player
pub async fn player_seek_internal(player_id: &str, position_ms: i64) -> Result<(), String> {
    GLOBAL_PLAYER_MANAGER.seek(player_id, position_ms).map_err(|e| e.to_string())
}

/// Internal: Set volume on a specific player
pub async fn player_set_volume_internal(player_id: &str, volume: f64) -> Result<(), String> {
    GLOBAL_PLAYER_MANAGER.set_volume(player_id, volume).map_err(|e| e.to_string())
}

/// Internal: Set muted state on a specific player
pub async fn player_set_muted_internal(player_id: &str, muted: bool) -> Result<(), String> {
    GLOBAL_PLAYER_MANAGER.set_muted(player_id, muted).map_err(|e| e.to_string())
}

/// Internal: Get status of a specific player
pub async fn player_get_status_internal(player_id: &str) -> Result<PlayerStatus, String> {
    GLOBAL_PLAYER_MANAGER.get_status(player_id).map_err(|e| e.to_string())
}

/// Internal: Skip forward on a specific player
pub async fn player_skip_forward_internal(player_id: &str, seconds: i64) -> Result<(), String> {
    GLOBAL_PLAYER_MANAGER.skip_forward(player_id, seconds).map_err(|e| e.to_string())
}

/// Internal: Skip backward on a specific player
pub async fn player_skip_backward_internal(player_id: &str, seconds: i64) -> Result<(), String> {
    GLOBAL_PLAYER_MANAGER.skip_backward(player_id, seconds).map_err(|e| e.to_string())
}

/// Internal: Get status of all players
pub async fn player_get_all_statuses_internal() -> Result<HashMap<String, PlayerStatus>, String> {
    GLOBAL_PLAYER_MANAGER.get_all_statuses().map_err(|e| e.to_string())
}

/// Internal: Load media on a specific player
pub async fn player_load_internal(player_id: &str, uri: &str) -> Result<(), String> {
    GLOBAL_PLAYER_MANAGER.load(player_id, uri).map_err(|e| e.to_string())
}

/// Internal: Set window handle for embedded video
pub fn player_set_window_handle_internal(player_id: &str, xid: u64) -> Result<(), String> {
    GLOBAL_PLAYER_MANAGER.set_window_handle(player_id, xid).map_err(|e| e.to_string())
}

/// Get reference to global player manager
pub fn get_global_player_manager() -> &'static MediaPlayerManager {
    &GLOBAL_PLAYER_MANAGER
}

/// State wrapper for MediaPlayerManager - supports multiple simultaneous players
/// NOTE: MediaPlayerManager has internal Mutex, so we don't wrap it in another Mutex
/// to avoid double-locking issues and mutex poisoning.
pub struct MediaPlayerManagerState(pub MediaPlayerManager);

impl MediaPlayerManagerState {
    pub fn new() -> Self {
        Self(MediaPlayerManager::new())
    }
}

impl Default for MediaPlayerManagerState {
    fn default() -> Self {
        Self::new()
    }
}

/// Legacy single-player state for backwards compatibility
pub struct MediaPlayerState(pub Mutex<Option<MediaPlayer>>);

impl MediaPlayerState {
    pub fn new() -> Self {
        Self(Mutex::new(None))
    }

    fn get_or_init(&self) -> Result<std::sync::MutexGuard<'_, Option<MediaPlayer>>, String> {
        let mut guard = self.0.lock().map_err(|e| e.to_string())?;
        if guard.is_none() {
            *guard = Some(MediaPlayer::new().map_err(|e| e.to_string())?);
            log::info!("Legacy media player initialized");
        }
        Ok(guard)
    }
}

impl Default for MediaPlayerState {
    fn default() -> Self {
        Self::new()
    }
}

// ==================== Multi-Player Commands (for multi-pane support) ====================
// These Tauri commands delegate to the global GLOBAL_PLAYER_MANAGER so both
// Tauri invoke and WebSocket commands work on the same player instances.

/// Load media on a specific player/pane
#[tauri::command(rename_all = "camelCase")]
pub async fn player_load(
    player_id: String,
    uri: String,
) -> Result<(), String> {
    player_load_internal(&player_id, &uri).await
}

/// Play on a specific player/pane
#[tauri::command(rename_all = "camelCase")]
pub async fn player_play(
    player_id: String,
) -> Result<(), String> {
    player_play_internal(&player_id).await
}

/// Pause on a specific player/pane
#[tauri::command(rename_all = "camelCase")]
pub async fn player_pause(
    player_id: String,
) -> Result<(), String> {
    player_pause_internal(&player_id).await
}

/// Stop on a specific player/pane
#[tauri::command(rename_all = "camelCase")]
pub async fn player_stop(
    player_id: String,
) -> Result<(), String> {
    player_stop_internal(&player_id).await
}

/// Seek on a specific player/pane
#[tauri::command(rename_all = "camelCase")]
pub async fn player_seek(
    player_id: String,
    position_ms: i64,
) -> Result<(), String> {
    player_seek_internal(&player_id, position_ms).await
}

/// Set volume on a specific player/pane
#[tauri::command(rename_all = "camelCase")]
pub async fn player_set_volume(
    player_id: String,
    volume: f64,
) -> Result<(), String> {
    player_set_volume_internal(&player_id, volume).await
}

/// Set muted state on a specific player/pane
#[tauri::command(rename_all = "camelCase")]
pub async fn player_set_muted(
    player_id: String,
    muted: bool,
) -> Result<(), String> {
    player_set_muted_internal(&player_id, muted).await
}

/// Get status of a specific player/pane
#[tauri::command(rename_all = "camelCase")]
pub async fn player_get_status(
    player_id: String,
) -> Result<PlayerStatus, String> {
    player_get_status_internal(&player_id).await
}

/// Skip forward on a specific player/pane
#[tauri::command(rename_all = "camelCase")]
pub async fn player_skip_forward(
    player_id: String,
    seconds: i64,
) -> Result<(), String> {
    player_skip_forward_internal(&player_id, seconds).await
}

/// Skip backward on a specific player/pane
#[tauri::command(rename_all = "camelCase")]
pub async fn player_skip_backward(
    player_id: String,
    seconds: i64,
) -> Result<(), String> {
    player_skip_backward_internal(&player_id, seconds).await
}

/// Play YouTube on a specific player/pane
#[tauri::command(rename_all = "camelCase")]
pub async fn player_play_youtube(
    player_id: String,
    url: String,
) -> Result<VideoInfo, String> {
    GLOBAL_PLAYER_MANAGER.play_youtube(&player_id, &url).map_err(|e| e.to_string())
}

/// Remove/destroy a player/pane
#[tauri::command(rename_all = "camelCase")]
pub async fn player_remove(
    player_id: String,
) -> Result<(), String> {
    GLOBAL_PLAYER_MANAGER.remove_player(&player_id).map_err(|e| e.to_string())
}

/// Get status of all players
#[tauri::command(rename_all = "camelCase")]
pub async fn player_get_all_statuses() -> Result<HashMap<String, PlayerStatus>, String> {
    player_get_all_statuses_internal().await
}

/// The player_ids namespaced to a window (`<label>::pane-N`).
pub fn players_for_window(window_label: &str) -> Vec<String> {
    let prefix = format!("{}::", window_label);
    GLOBAL_PLAYER_MANAGER
        .list_players()
        .unwrap_or_default()
        .into_iter()
        .filter(|id| id.starts_with(&prefix))
        .collect()
}

/// Stop a player's pipeline and BLOCK until it reaches NULL, so the video sink has
/// released its X11 surface before that surface is destroyed (avoids the close-time
/// RenderBadPicture crash). No-op if the player doesn't exist.
pub fn stop_and_wait(player_id: &str) {
    if let Err(e) = GLOBAL_PLAYER_MANAGER.stop_and_wait(player_id) {
        log::warn!("stop_and_wait: {}: {}", player_id, e);
    }
}

/// Remove (drop) a player from the manager.
pub fn remove_player(player_id: &str) {
    if let Err(e) = GLOBAL_PLAYER_MANAGER.remove_player(player_id) {
        log::warn!("remove_player: {}: {}", player_id, e);
    }
}

/// Stop all players
#[tauri::command(rename_all = "camelCase")]
pub async fn player_stop_all() -> Result<(), String> {
    GLOBAL_PLAYER_MANAGER.stop_all().map_err(|e| e.to_string())
}

/// List all active player IDs
#[tauri::command(rename_all = "camelCase")]
pub async fn player_list() -> Result<Vec<String>, String> {
    GLOBAL_PLAYER_MANAGER.list_players().map_err(|e| e.to_string())
}

/// Set window handle for embedded video on a specific player (VideoOverlay)
/// This allows GStreamer to render video inside the app window instead of a popup
#[tauri::command(rename_all = "camelCase")]
pub async fn player_set_window_handle(
    player_id: String,
    handle: u64,
) -> Result<(), String> {
    player_set_window_handle_internal(&player_id, handle)
}

/// Refresh/expose the video overlay after window resize
#[tauri::command(rename_all = "camelCase")]
pub async fn player_expose(
    player_id: String,
) -> Result<(), String> {
    GLOBAL_PLAYER_MANAGER.expose(&player_id).map_err(|e| e.to_string())
}

/// Set which player the floating media controls drive. The frontend calls this
/// whenever the focused pane changes, so the controls' status display and its
/// commands follow the active pane instead of being stuck on pane-0.
#[tauri::command(rename_all = "camelCase")]
pub async fn set_active_media_player(player_id: String) -> Result<(), String> {
    crate::controls_server::set_active_player_id(player_id);
    Ok(())
}

/// Get the X11 window ID (XID) for embedding video
/// This is Linux/X11 specific - returns error on other platforms or Wayland
///
/// Uses raw-window-handle to extract the X11 window ID from the Tauri window.
/// GStreamer's VideoOverlay can then render directly into this window.
#[tauri::command(rename_all = "camelCase")]
pub async fn get_window_xid(window: tauri::Window) -> Result<u64, String> {
    #[cfg(target_os = "linux")]
    {
        // Check if we're on X11 by looking at environment
        let is_x11 = std::env::var("XDG_SESSION_TYPE").map(|v| v == "x11").unwrap_or(false)
            || std::env::var("GDK_BACKEND").map(|v| v == "x11").unwrap_or(false)
            || std::env::var("DISPLAY").is_ok();

        if !is_x11 {
            log::info!("Not running on X11 (Wayland?), VideoOverlay embedding not supported");
            return Err("VideoOverlay requires X11, not supported on Wayland".to_string());
        }

        // Get the raw window handle from Tauri window
        match window.window_handle() {
            Ok(handle) => {
                match handle.as_raw() {
                    RawWindowHandle::Xlib(xlib_handle) => {
                        let xid = xlib_handle.window as u64;
                        log::info!("Got X11 XID via Xlib: 0x{:x}", xid);
                        Ok(xid)
                    }
                    RawWindowHandle::Xcb(xcb_handle) => {
                        let xid = xcb_handle.window.get() as u64;
                        log::info!("Got X11 XID via XCB: 0x{:x}", xid);
                        Ok(xid)
                    }
                    other => {
                        log::warn!("Unexpected window handle type: {:?}", other);
                        Err(format!("Unexpected window handle type on X11: {:?}", other))
                    }
                }
            }
            Err(e) => {
                log::error!("Failed to get window handle: {}", e);
                Err(format!("Failed to get window handle: {}", e))
            }
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        log::info!("VideoOverlay embedding only supported on Linux/X11");
        Err("VideoOverlay embedding only supported on Linux/X11".to_string())
    }
}

// ==================== Legacy Single-Player Commands (for backwards compatibility) ====================

/// Load media from a URI (file:// or http://)
#[tauri::command(rename_all = "camelCase")]
pub async fn media_load(state: State<'_, MediaPlayerState>, uri: String) -> Result<(), String> {
    let guard = state.get_or_init()?;
    if let Some(ref player) = *guard {
        player.load(&uri).map_err(|e| e.to_string())
    } else {
        Err("Media player not initialized".to_string())
    }
}

/// Start playback
#[tauri::command(rename_all = "camelCase")]
pub async fn media_play(state: State<'_, MediaPlayerState>) -> Result<(), String> {
    let guard = state.get_or_init()?;
    if let Some(ref player) = *guard {
        player.play().map_err(|e| e.to_string())
    } else {
        Err("Media player not initialized".to_string())
    }
}

/// Pause playback
#[tauri::command(rename_all = "camelCase")]
pub async fn media_pause(state: State<'_, MediaPlayerState>) -> Result<(), String> {
    let guard = state.get_or_init()?;
    if let Some(ref player) = *guard {
        player.pause().map_err(|e| e.to_string())
    } else {
        Err("Media player not initialized".to_string())
    }
}

/// Stop playback
#[tauri::command(rename_all = "camelCase")]
pub async fn media_stop(state: State<'_, MediaPlayerState>) -> Result<(), String> {
    let guard = state.get_or_init()?;
    if let Some(ref player) = *guard {
        player.stop().map_err(|e| e.to_string())
    } else {
        Err("Media player not initialized".to_string())
    }
}

/// Seek to position in milliseconds
#[tauri::command(rename_all = "camelCase")]
pub async fn media_seek(
    state: State<'_, MediaPlayerState>,
    position_ms: i64,
) -> Result<(), String> {
    let guard = state.get_or_init()?;
    if let Some(ref player) = *guard {
        player.seek(position_ms).map_err(|e| e.to_string())
    } else {
        Err("Media player not initialized".to_string())
    }
}

/// Set volume (0.0 to 1.0)
#[tauri::command(rename_all = "camelCase")]
pub async fn media_set_volume(
    state: State<'_, MediaPlayerState>,
    volume: f64,
) -> Result<(), String> {
    let guard = state.get_or_init()?;
    if let Some(ref player) = *guard {
        player.set_volume(volume).map_err(|e| e.to_string())
    } else {
        Err("Media player not initialized".to_string())
    }
}

/// Set muted state
#[tauri::command(rename_all = "camelCase")]
pub async fn media_set_muted(
    state: State<'_, MediaPlayerState>,
    muted: bool,
) -> Result<(), String> {
    let guard = state.get_or_init()?;
    if let Some(ref player) = *guard {
        player.set_muted(muted).map_err(|e| e.to_string())
    } else {
        Err("Media player not initialized".to_string())
    }
}

/// Get current player status
#[tauri::command(rename_all = "camelCase")]
pub async fn media_get_status(state: State<'_, MediaPlayerState>) -> Result<PlayerStatus, String> {
    let guard = state.get_or_init()?;
    if let Some(ref player) = *guard {
        Ok(player.get_status())
    } else {
        Err("Media player not initialized".to_string())
    }
}

/// Skip forward by seconds
#[tauri::command(rename_all = "camelCase")]
pub async fn media_skip_forward(
    state: State<'_, MediaPlayerState>,
    seconds: i64,
) -> Result<(), String> {
    let guard = state.get_or_init()?;
    if let Some(ref player) = *guard {
        player.skip_forward(seconds).map_err(|e| e.to_string())
    } else {
        Err("Media player not initialized".to_string())
    }
}

/// Skip backward by seconds
#[tauri::command(rename_all = "camelCase")]
pub async fn media_skip_backward(
    state: State<'_, MediaPlayerState>,
    seconds: i64,
) -> Result<(), String> {
    let guard = state.get_or_init()?;
    if let Some(ref player) = *guard {
        player.skip_backward(seconds).map_err(|e| e.to_string())
    } else {
        Err("Media player not initialized".to_string())
    }
}

/// Check if GStreamer is properly installed
#[tauri::command(rename_all = "camelCase")]
pub async fn media_check_gstreamer() -> Result<String, String> {
    earth_media::check_gstreamer().map_err(|e| e.to_string())
}

// ==================== YouTube Commands ====================

/// Play a YouTube video (legacy single player)
#[tauri::command(rename_all = "camelCase")]
pub async fn play_youtube(
    state: State<'_, MediaPlayerState>,
    url: String,
) -> Result<VideoInfo, String> {
    let guard = state.get_or_init()?;
    if let Some(ref player) = *guard {
        player.play_youtube(&url).map_err(|e| e.to_string())
    } else {
        Err("Media player not initialized".to_string())
    }
}

/// Get YouTube video info without playing
#[tauri::command(rename_all = "camelCase")]
pub async fn get_youtube_info(url: String) -> Result<VideoInfo, String> {
    MediaPlayer::get_youtube_info(&url).map_err(|e| e.to_string())
}

/// Check if yt-dlp is available
#[tauri::command(rename_all = "camelCase")]
pub async fn check_youtube_available() -> Result<bool, String> {
    Ok(MediaPlayer::is_youtube_available())
}

/// Check if a URL is a YouTube URL
#[tauri::command(rename_all = "camelCase")]
pub async fn is_youtube_url(url: String) -> Result<bool, String> {
    Ok(MediaPlayer::is_youtube_url(&url))
}
