#![forbid(unsafe_code)] // Phase 5: this crate needs no unsafe; enforce it.
//! Earth Servo - Servo browser integration for Earth Reclaim
//!
//! This crate provides process-based Servo integration.
//! Servo windows open separately (not embedded) as an initial implementation.

use std::collections::HashMap;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ServoError {
    #[error("Failed to launch Servo: {0}")]
    LaunchError(String),
    #[error("Servo not found at path: {0}")]
    NotFound(String),
    #[error("Servo process error: {0}")]
    ProcessError(String),
    #[error("Webview not found: {0}")]
    WebviewNotFound(String),
    #[error("Lock error: {0}")]
    LockError(String),
}

/// Manages Servo browser processes
pub struct ServoManager {
    processes: Mutex<HashMap<String, ServoProcess>>,
    servo_path: String,
}

struct ServoProcess {
    child: Child,
    current_url: String,
}

impl ServoManager {
    /// Create a new ServoManager
    ///
    /// Looks for Servo in the following locations:
    /// 1. SERVO_PATH environment variable
    /// 2. ~/Documents/Earth-Runtime/servo/target/release/servo
    /// 3. /usr/local/bin/servo
    /// 4. servo (in PATH)
    pub fn new() -> Result<Self, ServoError> {
        let servo_path = Self::find_servo_path()?;
        log::info!("ServoManager initialized with Servo at: {}", servo_path);

        Ok(Self {
            processes: Mutex::new(HashMap::new()),
            servo_path,
        })
    }

    fn find_servo_path() -> Result<String, ServoError> {
        // Check SERVO_PATH env var first
        if let Ok(path) = std::env::var("SERVO_PATH") {
            if std::path::Path::new(&path).exists() {
                return Ok(path);
            }
        }

        // Check common locations
        let home = std::env::var("HOME").unwrap_or_default();
        let possible_paths = [
            format!("{}/Documents/Earth-Runtime/servo/target/release/servo", home),
            format!("{}/servo/target/release/servo", home),
            "/usr/local/bin/servo".to_string(),
            "/usr/bin/servo".to_string(),
        ];

        for path in &possible_paths {
            if std::path::Path::new(path).exists() {
                return Ok(path.clone());
            }
        }

        // Try finding in PATH
        if let Ok(output) = Command::new("which").arg("servo").output() {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !path.is_empty() {
                    return Ok(path);
                }
            }
        }

        Err(ServoError::NotFound(
            "Servo not found. Set SERVO_PATH environment variable or install Servo.".to_string(),
        ))
    }

    /// Launch a new Servo window with the given URL
    pub fn launch(&self, webview_id: String, url: String) -> Result<(), ServoError> {
        let mut processes = self
            .processes
            .lock()
            .map_err(|e| ServoError::LockError(e.to_string()))?;

        // If this webview already exists, navigate instead
        if processes.contains_key(&webview_id) {
            drop(processes);
            return self.navigate(&webview_id, url);
        }

        log::info!("Launching Servo for webview '{}' with URL: {}", webview_id, url);

        let child = Command::new(&self.servo_path)
            .arg(&url)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| ServoError::LaunchError(e.to_string()))?;

        processes.insert(
            webview_id.clone(),
            ServoProcess {
                child,
                current_url: url,
            },
        );

        Ok(())
    }

    /// Navigate an existing webview to a new URL
    /// Note: Currently this kills and restarts Servo (no IPC yet)
    pub fn navigate(&self, webview_id: &str, url: String) -> Result<(), ServoError> {
        log::info!("Navigating webview '{}' to: {}", webview_id, url);

        // Close existing process
        self.close(webview_id)?;

        // Launch new process with new URL
        self.launch(webview_id.to_string(), url)
    }

    /// Close a Servo webview
    pub fn close(&self, webview_id: &str) -> Result<(), ServoError> {
        let mut processes = self
            .processes
            .lock()
            .map_err(|e| ServoError::LockError(e.to_string()))?;

        if let Some(mut process) = processes.remove(webview_id) {
            log::info!("Closing Servo webview: {}", webview_id);
            let _ = process.child.kill();
            let _ = process.child.wait();
        }

        Ok(())
    }

    /// Close all Servo webviews
    pub fn close_all(&self) -> Result<(), ServoError> {
        let mut processes = self
            .processes
            .lock()
            .map_err(|e| ServoError::LockError(e.to_string()))?;

        for (id, mut process) in processes.drain() {
            log::info!("Closing Servo webview: {}", id);
            let _ = process.child.kill();
            let _ = process.child.wait();
        }

        Ok(())
    }

    /// Get the current URL for a webview
    pub fn get_url(&self, webview_id: &str) -> Result<String, ServoError> {
        let processes = self
            .processes
            .lock()
            .map_err(|e| ServoError::LockError(e.to_string()))?;

        processes
            .get(webview_id)
            .map(|p| p.current_url.clone())
            .ok_or_else(|| ServoError::WebviewNotFound(webview_id.to_string()))
    }

    /// Check if a webview exists
    pub fn has_webview(&self, webview_id: &str) -> bool {
        self.processes
            .lock()
            .map(|p| p.contains_key(webview_id))
            .unwrap_or(false)
    }

    /// Get list of active webview IDs
    pub fn list_webviews(&self) -> Vec<String> {
        self.processes
            .lock()
            .map(|p| p.keys().cloned().collect())
            .unwrap_or_default()
    }
}

impl Drop for ServoManager {
    fn drop(&mut self) {
        if let Ok(mut processes) = self.processes.lock() {
            for (_, mut process) in processes.drain() {
                let _ = process.child.kill();
                let _ = process.child.wait();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_servo_path_lookup() {
        // This test will fail if Servo isn't installed, which is expected
        let result = ServoManager::find_servo_path();
        println!("Servo path result: {:?}", result);
    }
}
