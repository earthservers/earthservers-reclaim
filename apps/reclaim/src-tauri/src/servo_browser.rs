//! Servo browser integration for Tauri commands
//!
//! Provides Tauri commands that wrap the earth-servo crate.

use earth_servo::ServoManager;
use lazy_static::lazy_static;
use std::sync::Mutex;

lazy_static! {
    static ref SERVO_MANAGER: Mutex<Option<ServoManager>> = Mutex::new(None);
}

/// Initialize the Servo manager (called once at startup)
fn get_or_init_servo() -> Result<(), String> {
    let mut manager = SERVO_MANAGER.lock().map_err(|e| e.to_string())?;
    if manager.is_none() {
        *manager = Some(ServoManager::new().map_err(|e| e.to_string())?);
    }
    Ok(())
}

/// Create a new Servo browser window
#[tauri::command(rename_all = "camelCase")]
pub async fn create_servo_browser(webview_id: String, url: String) -> Result<(), String> {
    get_or_init_servo()?;

    let manager = SERVO_MANAGER.lock().map_err(|e| e.to_string())?;
    if let Some(ref m) = *manager {
        m.launch(webview_id, url).map_err(|e| e.to_string())
    } else {
        Err("Servo manager not initialized".to_string())
    }
}

/// Navigate to a new URL
#[tauri::command(rename_all = "camelCase")]
pub async fn servo_navigate(webview_id: String, url: String) -> Result<(), String> {
    let manager = SERVO_MANAGER.lock().map_err(|e| e.to_string())?;
    if let Some(ref m) = *manager {
        m.navigate(&webview_id, url).map_err(|e| e.to_string())
    } else {
        Err("Servo manager not initialized".to_string())
    }
}

/// Close a Servo browser window
#[tauri::command(rename_all = "camelCase")]
pub async fn servo_close(webview_id: String) -> Result<(), String> {
    let manager = SERVO_MANAGER.lock().map_err(|e| e.to_string())?;
    if let Some(ref m) = *manager {
        m.close(&webview_id).map_err(|e| e.to_string())
    } else {
        Err("Servo manager not initialized".to_string())
    }
}

/// Close all Servo browser windows
#[tauri::command(rename_all = "camelCase")]
pub async fn servo_close_all() -> Result<(), String> {
    let manager = SERVO_MANAGER.lock().map_err(|e| e.to_string())?;
    if let Some(ref m) = *manager {
        m.close_all().map_err(|e| e.to_string())
    } else {
        Err("Servo manager not initialized".to_string())
    }
}

/// Get the current URL for a webview
#[tauri::command(rename_all = "camelCase")]
pub async fn servo_get_url(webview_id: String) -> Result<String, String> {
    let manager = SERVO_MANAGER.lock().map_err(|e| e.to_string())?;
    if let Some(ref m) = *manager {
        m.get_url(&webview_id).map_err(|e| e.to_string())
    } else {
        Err("Servo manager not initialized".to_string())
    }
}

/// Check if a webview exists
#[tauri::command(rename_all = "camelCase")]
pub async fn servo_has_webview(webview_id: String) -> Result<bool, String> {
    let manager = SERVO_MANAGER.lock().map_err(|e| e.to_string())?;
    if let Some(ref m) = *manager {
        Ok(m.has_webview(&webview_id))
    } else {
        Ok(false)
    }
}

/// List all active webview IDs
#[tauri::command(rename_all = "camelCase")]
pub async fn servo_list_webviews() -> Result<Vec<String>, String> {
    let manager = SERVO_MANAGER.lock().map_err(|e| e.to_string())?;
    if let Some(ref m) = *manager {
        Ok(m.list_webviews())
    } else {
        Ok(Vec::new())
    }
}
