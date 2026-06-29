//! Minimal localhost HTTP server that serves the app's EMBEDDED frontend assets
//! (via Tauri's asset resolver).
//!
//! Why: the floating media controls render in a RAW WebKitGTK window (not a Tauri
//! webview), so they can't use the `tauri://` protocol, and in a packaged build the
//! frontend is embedded in the binary (no files on disk to load via `file://`).
//! This tiny server exposes those embedded assets over `http://127.0.0.1:9877/` so
//! the raw webview can load app routes like `/media-controls` in packaged builds —
//! the same way the dev build loads them from the Vite dev server.

use tauri::AppHandle;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Fixed localhost port for the embedded-asset server (sibling to the controls
/// WebSocket server on 9876). The frontend builds control URLs against this.
pub const ASSETS_PORT: u16 = 9877;

/// Start the asset server in the background. Safe to call once at startup; in dev
/// the embedded assets don't exist, so requests just 404 (dev loads the controls
/// from the Vite dev server instead).
pub fn start(app: AppHandle) {
    // Use Tauri's async runtime: `start` is called from the synchronous setup()
    // hook, where there's no ambient Tokio runtime for a bare `tokio::spawn`.
    tauri::async_runtime::spawn(async move {
        let listener = match TcpListener::bind(("127.0.0.1", ASSETS_PORT)).await {
            Ok(l) => l,
            Err(e) => {
                log::error!("[assets] failed to bind 127.0.0.1:{}: {}", ASSETS_PORT, e);
                return;
            }
        };
        log::info!("[assets] serving embedded frontend on http://127.0.0.1:{}", ASSETS_PORT);

        loop {
            let (mut sock, _) = match listener.accept().await {
                Ok(pair) => pair,
                Err(_) => continue,
            };
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                // A single read captures the GET request line for our purposes.
                let mut buf = vec![0u8; 8192];
                let n = match sock.read(&mut buf).await {
                    Ok(n) if n > 0 => n,
                    _ => return,
                };
                let req = String::from_utf8_lossy(&buf[..n]);
                let raw_path = req.split_whitespace().nth(1).unwrap_or("/");
                // Drop any query string / fragment.
                let path = raw_path.split(['?', '#']).next().unwrap_or("/");

                // SPA fallback: a path with no file extension (an app route like
                // /media-controls) serves index.html so the client router can match.
                let key = if path == "/" {
                    "index.html".to_string()
                } else {
                    let trimmed = path.trim_start_matches('/');
                    let last = trimmed.rsplit('/').next().unwrap_or("");
                    if last.contains('.') {
                        trimmed.to_string()
                    } else {
                        "index.html".to_string()
                    }
                };

                let resolver = app.asset_resolver();
                let asset = resolver
                    .get(format!("/{}", key))
                    .or_else(|| resolver.get(key.clone()));

                let response = match asset {
                    Some(a) => {
                        let mut out = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-cache\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n",
                            a.mime_type,
                            a.bytes.len()
                        )
                        .into_bytes();
                        out.extend_from_slice(&a.bytes);
                        out
                    }
                    None => {
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()
                    }
                };
                let _ = sock.write_all(&response).await;
                let _ = sock.flush().await;
            });
        }
    });
}
