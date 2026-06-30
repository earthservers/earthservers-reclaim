//! WebSocket server for media controls communication
//!
//! This server enables real-time bidirectional communication between the main Tauri app
//! and floating media controls rendered in a separate X11/WebKitGTK window.
//!
//! Architecture:
//! - Server runs on localhost:9876 (configurable)
//! - Controls window connects via WebSocket
//! - Status updates are broadcast to all connected clients (250ms intervals)
//! - Control commands are received and forwarded to the media player

use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, RwLock};
use tokio_tungstenite::accept_async;
use futures_util::{StreamExt, SinkExt};
use serde::{Deserialize, Serialize};

/// Current player status broadcast to controls
#[derive(Clone, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PlayerStatus {
    pub player_id: String,
    pub is_playing: bool,
    pub current_time: i64,  // milliseconds
    pub duration: i64,      // milliseconds
    pub volume: f64,        // 0.0 - 1.0
    pub is_muted: bool,
    pub title: String,
    pub is_shuffled: bool,
    pub repeat_mode: String, // "none", "all", "one"
    /// Whether the owning app window is in media fullscreen — drives the floating
    /// controls' exit-fullscreen button (the DOM exit affordance is occluded by the
    /// native video surface in fullscreen).
    pub is_fullscreen: bool,
}

impl Default for PlayerStatus {
    fn default() -> Self {
        Self {
            player_id: String::new(),
            is_playing: false,
            current_time: 0,
            duration: 0,
            volume: 1.0,
            is_muted: false,
            title: String::new(),
            is_shuffled: false,
            repeat_mode: "none".to_string(),
            is_fullscreen: false,
        }
    }
}

/// Commands received from controls
/// Note: rename_all applies to both the tag value AND field names
#[derive(Deserialize, Debug)]
#[serde(tag = "cmd", rename_all = "camelCase")]
pub enum ControlCommand {
    Play { #[serde(rename = "playerId")] player_id: Option<String> },
    Pause { #[serde(rename = "playerId")] player_id: Option<String> },
    Stop { #[serde(rename = "playerId")] player_id: Option<String> },
    TogglePlay { #[serde(rename = "playerId")] player_id: Option<String> },
    Seek { #[serde(rename = "playerId")] player_id: Option<String>, #[serde(rename = "positionMs")] position_ms: i64 },
    SetVolume { #[serde(rename = "playerId")] player_id: Option<String>, volume: f64 },
    ToggleMute { #[serde(rename = "playerId")] player_id: Option<String> },
    SkipForward { #[serde(rename = "playerId")] player_id: Option<String>, seconds: i64 },
    SkipBackward { #[serde(rename = "playerId")] player_id: Option<String>, seconds: i64 },
    // Request current status (controls just connected)
    GetStatus,
    // Move the controls window by delta. `window` = the sending controls' window
    // label, so we move that window's controls (not some other window's).
    MoveWindow { #[serde(rename = "deltaX")] delta_x: i32, #[serde(rename = "deltaY")] delta_y: i32, window: Option<String> },
    // Resize the controls window (physical px) — used on collapse/expand
    ResizeWindow { width: u32, height: u32, window: Option<String> },
    // Playlist/queue controls. `window` routes the emitted action to that app window.
    ToggleShuffle { #[serde(rename = "playerId")] player_id: Option<String>, window: Option<String> },
    ToggleRepeat { #[serde(rename = "playerId")] player_id: Option<String>, window: Option<String> },
    TogglePlaylist { window: Option<String> },
    // Skip to the previous / next item in the queue (whole video, not seek).
    PreviousVideo { window: Option<String> },
    NextVideo { window: Option<String> },
    // Leave media fullscreen (the DOM exit affordance is hidden behind the video).
    ExitFullscreen { window: Option<String> },
}

/// Callback type for handling commands
pub type CommandHandler = Arc<dyn Fn(ControlCommand) + Send + Sync>;

/// WebSocket server for media controls
pub struct ControlsServer {
    status_tx: broadcast::Sender<PlayerStatus>,
    current_status: Arc<RwLock<PlayerStatus>>,
    command_handler: Arc<RwLock<Option<CommandHandler>>>,
    port: u16,
}

impl ControlsServer {
    pub fn new(port: u16) -> Self {
        let (status_tx, _) = broadcast::channel(64);
        Self {
            status_tx,
            current_status: Arc::new(RwLock::new(PlayerStatus::default())),
            command_handler: Arc::new(RwLock::new(None)),
            port,
        }
    }

    /// Set the command handler callback
    pub async fn set_command_handler<F>(&self, handler: F)
    where
        F: Fn(ControlCommand) + Send + Sync + 'static,
    {
        let mut h = self.command_handler.write().await;
        *h = Some(Arc::new(handler));
    }

    /// Broadcast a status update to all connected controls
    pub fn broadcast_status(&self, status: PlayerStatus) {
        // Update stored status
        let current_status = self.current_status.clone();
        let status_clone = status.clone();
        tokio::spawn(async move {
            let mut current = current_status.write().await;
            *current = status_clone;
        });

        // Broadcast to all subscribers
        let _ = self.status_tx.send(status);
    }

    /// Get the current stored status
    pub async fn get_current_status(&self) -> PlayerStatus {
        self.current_status.read().await.clone()
    }

    /// Start the WebSocket server
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let listener = TcpListener::bind(format!("127.0.0.1:{}", self.port)).await?;
        log::info!("Controls WebSocket server started on ws://127.0.0.1:{}", self.port);

        let status_tx = self.status_tx.clone();
        let current_status = self.current_status.clone();
        let command_handler = self.command_handler.clone();

        tokio::spawn(async move {
            while let Ok((stream, addr)) = listener.accept().await {
                log::info!("Controls client connected from {}", addr);

                let mut status_rx = status_tx.subscribe();
                let current_status = current_status.clone();
                let command_handler = command_handler.clone();

                tokio::spawn(async move {
                    let ws_stream = match accept_async(stream).await {
                        Ok(ws) => ws,
                        Err(e) => {
                            log::error!("WebSocket handshake failed: {}", e);
                            return;
                        }
                    };

                    let (mut write, mut read) = ws_stream.split();

                    // Send current status immediately on connect
                    {
                        let status = current_status.read().await.clone();
                        if let Ok(json) = serde_json::to_string(&status) {
                            let _ = write.send(tokio_tungstenite::tungstenite::Message::Text(json)).await;
                        }
                    }

                    // Handle incoming commands from controls
                    let cmd_handler = {
                        let command_handler = command_handler.clone();
                        async move {
                            while let Some(msg_result) = read.next().await {
                                match msg_result {
                                    Ok(msg) => {
                                        if let Ok(text) = msg.to_text() {
                                            match serde_json::from_str::<ControlCommand>(text) {
                                                Ok(cmd) => {
                                                    log::debug!("Received control command: {:?}", cmd);

                                                    // Handle GetStatus specially
                                                    if matches!(cmd, ControlCommand::GetStatus) {
                                                        // Status will be sent via the broadcast channel
                                                        continue;
                                                    }

                                                    // Forward to handler
                                                    let handler = command_handler.read().await;
                                                    if let Some(ref h) = *handler {
                                                        h(cmd);
                                                    }
                                                }
                                                Err(e) => {
                                                    log::warn!("Invalid control command: {} - {}", text, e);
                                                }
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        log::debug!("WebSocket read error: {}", e);
                                        break;
                                    }
                                }
                            }
                        }
                    };

                    // Push status updates to controls
                    let status_pusher = async {
                        while let Ok(status) = status_rx.recv().await {
                            match serde_json::to_string(&status) {
                                Ok(json) => {
                                    if write.send(tokio_tungstenite::tungstenite::Message::Text(json)).await.is_err() {
                                        break;
                                    }
                                }
                                Err(e) => {
                                    log::error!("Failed to serialize status: {}", e);
                                }
                            }
                        }
                    };

                    // Run both handlers concurrently
                    tokio::select! {
                        _ = cmd_handler => {
                            log::info!("Controls client command handler ended");
                        },
                        _ = status_pusher => {
                            log::info!("Controls client status pusher ended");
                        },
                    }

                    log::info!("Controls client disconnected");
                });
            }
        });

        Ok(())
    }
}

/// Window label embedded in a namespaced player id like "main::pane-2". Falls back
/// to "main" for legacy/un-namespaced ids ("pane-0").
pub fn window_label_of(player_id: &str) -> String {
    player_id
        .split_once("::")
        .map(|(label, _)| label.to_string())
        .unwrap_or_else(|| "main".to_string())
}

/// Global controls server instance.
///
/// SINGLE shared controls model: there is ONE native controls webview (a 2nd one
/// crashes the WebKit web process), so it follows the LAST-CLICKED media pane across
/// all app windows. `LAST_ACTIVE_PLAYER` is that globally-focused player id; the
/// broadcast publishes only it. Per-window maps below still hold each window's
/// flags/title/fullscreen, looked up by the active player's window label.
lazy_static::lazy_static! {
    pub static ref CONTROLS_SERVER: Arc<ControlsServer> = Arc::new(ControlsServer::new(9876));
    /// The globally last-focused player id (last-clicked pane in any window).
    static ref LAST_ACTIVE_PLAYER: std::sync::RwLock<String> = std::sync::RwLock::new(String::new());
    /// window label -> (is_shuffled, repeat_mode), pushed from the frontend.
    static ref PLAYBACK_FLAGS: std::sync::RwLock<std::collections::HashMap<String, (bool, String)>> =
        std::sync::RwLock::new(std::collections::HashMap::new());
    /// window label -> active media title (frontend-owned; GStreamer tags are usually empty).
    static ref ACTIVE_TITLE: std::sync::RwLock<std::collections::HashMap<String, String>> =
        std::sync::RwLock::new(std::collections::HashMap::new());
    /// window label -> media fullscreen flag (drives the exit-fullscreen button).
    static ref FULLSCREEN: std::sync::RwLock<std::collections::HashMap<String, bool>> =
        std::sync::RwLock::new(std::collections::HashMap::new());
}

/// Update whether a window is in media fullscreen (called from the frontend).
pub fn set_fullscreen(window: String, is_fullscreen: bool) {
    if let Ok(mut g) = FULLSCREEN.write() {
        g.insert(window, is_fullscreen);
    }
}

/// A window's media fullscreen state for the controls broadcast.
pub fn get_fullscreen(window: &str) -> bool {
    FULLSCREEN.read().ok().and_then(|g| g.get(window).copied()).unwrap_or(false)
}

/// Update a window's active media title shown by its controls (from the frontend).
pub fn set_active_title(window: String, title: String) {
    if let Ok(mut g) = ACTIVE_TITLE.write() {
        g.insert(window, title);
    }
}

/// A window's active media title for the controls broadcast.
pub fn get_active_title(window: &str) -> String {
    ACTIVE_TITLE.read().ok().and_then(|g| g.get(window).cloned()).unwrap_or_default()
}

/// Update a window's shuffle + repeat flags (from the frontend).
pub fn set_playback_flags(window: String, is_shuffled: bool, repeat_mode: String) {
    if let Ok(mut g) = PLAYBACK_FLAGS.write() {
        g.insert(window, (is_shuffled, repeat_mode));
    }
}

/// A window's (is_shuffled, repeat_mode) for the controls broadcast.
pub fn get_playback_flags(window: &str) -> (bool, String) {
    PLAYBACK_FLAGS
        .read()
        .ok()
        .and_then(|g| g.get(window).cloned())
        .unwrap_or_else(|| (false, "none".to_string()))
}

/// Set the player the single shared controls drive: this is the last-clicked pane in
/// any window. Stored globally (drives the broadcast) and per-window (so flags/title
/// lookups by label still work).
pub fn set_active_player_id(id: String) {
    if let Ok(mut g) = LAST_ACTIVE_PLAYER.write() {
        *g = id;
    }
}

/// The globally last-focused player id (empty string if none yet).
pub fn get_last_active_player() -> String {
    LAST_ACTIVE_PLAYER.read().map(|g| g.clone()).unwrap_or_default()
}

/// Drop all per-window state for a closed window so the broadcast loop stops polling
/// its (now-gone) player and the maps don't accumulate dead entries.
pub fn forget_window(window: &str) {
    if let Ok(mut g) = PLAYBACK_FLAGS.write() { g.remove(window); }
    if let Ok(mut g) = ACTIVE_TITLE.write() { g.remove(window); }
    if let Ok(mut g) = FULLSCREEN.write() { g.remove(window); }
    // If the globally-active player belonged to the closed window, clear it so the
    // broadcast stops polling a dead player.
    if let Ok(mut g) = LAST_ACTIVE_PLAYER.write() {
        if window_label_of(g.as_str()) == window { g.clear(); }
    }
}

/// Initialize and start the controls server
pub async fn init_controls_server() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    CONTROLS_SERVER.start().await
}

/// Broadcast player status update
pub fn broadcast_player_status(status: PlayerStatus) {
    CONTROLS_SERVER.broadcast_status(status);
}

/// Set the command handler for the controls server
pub async fn set_controls_command_handler<F>(handler: F)
where
    F: Fn(ControlCommand) + Send + Sync + 'static,
{
    CONTROLS_SERVER.set_command_handler(handler).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_serialization() {
        let status = PlayerStatus {
            player_id: "pane-0".to_string(),
            is_playing: true,
            current_time: 5000,
            duration: 120000,
            volume: 0.8,
            is_muted: false,
            title: "Test Video".to_string(),
            is_shuffled: false,
            repeat_mode: "none".to_string(),
            is_fullscreen: false,
        };

        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("isPlaying"));
        assert!(json.contains("currentTime"));
        assert!(json.contains("isShuffled"));
        assert!(json.contains("repeatMode"));
    }

    #[test]
    fn test_command_deserialization() {
        let json = r#"{"cmd":"play","playerId":"pane-0"}"#;
        let cmd: ControlCommand = serde_json::from_str(json).unwrap();
        assert!(matches!(cmd, ControlCommand::Play { .. }));

        let json = r#"{"cmd":"seek","positionMs":5000}"#;
        let cmd: ControlCommand = serde_json::from_str(json).unwrap();
        assert!(matches!(cmd, ControlCommand::Seek { position_ms: 5000, .. }));
    }
}
