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
    // Move the controls window by delta
    MoveWindow { #[serde(rename = "deltaX")] delta_x: i32, #[serde(rename = "deltaY")] delta_y: i32 },
    // Resize the controls window (physical px) — used on collapse/expand
    ResizeWindow { width: u32, height: u32 },
    // Playlist/queue controls
    ToggleShuffle { #[serde(rename = "playerId")] player_id: Option<String> },
    ToggleRepeat { #[serde(rename = "playerId")] player_id: Option<String> },
    TogglePlaylist,
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

/// Global controls server instance
lazy_static::lazy_static! {
    pub static ref CONTROLS_SERVER: Arc<ControlsServer> = Arc::new(ControlsServer::new(9876));
    /// Which player the floating controls currently drive. The status-broadcast
    /// loop reads this each tick and the controls echo it back on commands, so
    /// updating it (via `set_active_player_id`) retargets the controls to the
    /// focused pane. Defaults to the first pane.
    static ref ACTIVE_PLAYER_ID: std::sync::RwLock<String> =
        std::sync::RwLock::new("pane-0".to_string());
}

/// Set the player the floating controls drive (called from the frontend when the
/// focused pane changes).
pub fn set_active_player_id(id: String) {
    if let Ok(mut g) = ACTIVE_PLAYER_ID.write() {
        *g = id;
    }
}

/// Current active player id for the controls (defaults to "pane-0").
pub fn get_active_player_id() -> String {
    ACTIVE_PLAYER_ID
        .read()
        .map(|g| g.clone())
        .unwrap_or_else(|_| "pane-0".to_string())
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
