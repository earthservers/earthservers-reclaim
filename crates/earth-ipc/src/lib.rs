//! Earth IPC - Inter-process communication utilities
//!
//! This crate provides IPC mechanisms for communication between:
//! - Tauri main process and Servo browser
//! - Tauri main process and media player
//! - Future: Plugin system

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum IpcError {
    #[error("Connection failed: {0}")]
    ConnectionError(String),
    #[error("Send failed: {0}")]
    SendError(String),
    #[error("Receive failed: {0}")]
    ReceiveError(String),
    #[error("Serialization failed: {0}")]
    SerializationError(String),
    #[error("Timeout: {0}")]
    Timeout(String),
}

/// Message types for browser IPC
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BrowserMessage {
    /// Navigate to a URL
    Navigate { url: String },
    /// Go back in history
    GoBack,
    /// Go forward in history
    GoForward,
    /// Reload the page
    Reload,
    /// Stop loading
    Stop,
    /// Execute JavaScript
    ExecuteJs { script: String },
    /// Request current URL
    GetUrl,
    /// Request page title
    GetTitle,
    /// Close the browser
    Close,
}

/// Response types from browser
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BrowserResponse {
    /// Operation successful
    Ok,
    /// Current URL
    Url(String),
    /// Page title
    Title(String),
    /// JavaScript result
    JsResult(String),
    /// Error occurred
    Error(String),
    /// Navigation started
    NavigationStarted { url: String },
    /// Navigation completed
    NavigationCompleted { url: String },
    /// Page load progress (0.0 - 1.0)
    LoadProgress(f64),
}

/// Message types for media player IPC
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MediaMessage {
    /// Load media from URI
    Load { uri: String },
    /// Start playback
    Play,
    /// Pause playback
    Pause,
    /// Stop playback
    Stop,
    /// Seek to position (milliseconds)
    Seek { position_ms: i64 },
    /// Set volume (0.0 - 1.0)
    SetVolume { volume: f64 },
    /// Set muted state
    SetMuted { muted: bool },
    /// Request status
    GetStatus,
}

/// Response types from media player
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MediaResponse {
    /// Operation successful
    Ok,
    /// Current status
    Status {
        playing: bool,
        position_ms: i64,
        duration_ms: i64,
        volume: f64,
        muted: bool,
    },
    /// Error occurred
    Error(String),
    /// Playback state changed
    StateChanged { playing: bool },
    /// Position updated
    PositionUpdated { position_ms: i64 },
    /// End of stream
    EndOfStream,
}

/// Serialize a message to JSON bytes
pub fn serialize_message<T: Serialize>(msg: &T) -> Result<Vec<u8>, IpcError> {
    serde_json::to_vec(msg).map_err(|e| IpcError::SerializationError(e.to_string()))
}

/// Deserialize a message from JSON bytes
pub fn deserialize_message<T: for<'de> Deserialize<'de>>(data: &[u8]) -> Result<T, IpcError> {
    serde_json::from_slice(data).map_err(|e| IpcError::SerializationError(e.to_string()))
}

/// Simple message frame format: length prefix (4 bytes) + data
pub fn frame_message(data: &[u8]) -> Vec<u8> {
    let len = data.len() as u32;
    let mut frame = len.to_le_bytes().to_vec();
    frame.extend_from_slice(data);
    frame
}

/// Read a framed message (returns None if incomplete)
pub fn read_frame(buffer: &[u8]) -> Option<(Vec<u8>, usize)> {
    if buffer.len() < 4 {
        return None;
    }

    let len = u32::from_le_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]) as usize;

    if buffer.len() < 4 + len {
        return None;
    }

    Some((buffer[4..4 + len].to_vec(), 4 + len))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_browser_message() {
        let msg = BrowserMessage::Navigate {
            url: "https://example.com".to_string(),
        };
        let bytes = serialize_message(&msg).unwrap();
        let decoded: BrowserMessage = deserialize_message(&bytes).unwrap();

        match decoded {
            BrowserMessage::Navigate { url } => {
                assert_eq!(url, "https://example.com");
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_frame_message() {
        let data = b"hello world";
        let framed = frame_message(data);

        let (decoded, consumed) = read_frame(&framed).unwrap();
        assert_eq!(decoded, data);
        assert_eq!(consumed, framed.len());
    }
}
