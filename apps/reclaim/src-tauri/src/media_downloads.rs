//! Manual media downloads with user-written descriptions.
//!
//! Media is NEVER auto-downloaded: the user picks an item from the Media panel,
//! optionally describes it, and downloads it explicitly. The description + source
//! is recorded so the local AI / knowledge graph can reference what was saved.

use std::sync::Mutex;

use rusqlite::{params, Connection};
use tauri::{Manager, State};

use crate::AppState;

const FIREFOX_UA: &str =
    "Mozilla/5.0 (X11; Linux x86_64; rv:128.0) Gecko/20100101 Firefox/128.0";

pub fn init(db_path: &str) -> Result<(), String> {
    let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS media_downloads (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            profile_id    INTEGER,
            url           TEXT NOT NULL,
            file_path     TEXT NOT NULL,
            kind          TEXT,
            description   TEXT,
            page_url      TEXT,
            downloaded_at TEXT NOT NULL
        )",
        [],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaDownload {
    pub id: i64,
    pub url: String,
    pub file_path: String,
    pub kind: String,
    pub description: String,
    pub page_url: String,
    pub downloaded_at: String,
}

/// Pick a non-colliding path in `dir` for `name`.
fn unique_path(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    let p = dir.join(name);
    if !p.exists() {
        return p;
    }
    let stem = std::path::Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("download");
    let ext = std::path::Path::new(name)
        .extension()
        .and_then(|s| s.to_str())
        .map(|e| format!(".{}", e))
        .unwrap_or_default();
    let mut i = 1;
    loop {
        let candidate = dir.join(format!("{}-{}{}", stem, i, ext));
        if !candidate.exists() {
            return candidate;
        }
        i += 1;
    }
}

/// Download one media URL to `<Downloads>/Reclaim/` and record it (with the
/// user's description) for AI reference. Returns the saved file path.
#[tauri::command(rename_all = "camelCase")]
pub async fn download_media(
    app: tauri::AppHandle,
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    url: String,
    kind: String,
    description: String,
    page_url: String,
) -> Result<String, String> {
    let db_path = {
        let s = state.lock().map_err(|e| e.to_string())?;
        s.db_path.clone()
    };

    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .header("User-Agent", FIREFOX_UA)
        .header("Referer", &page_url)
        .timeout(std::time::Duration::from_secs(60))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("download failed: HTTP {}", resp.status()));
    }
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;

    let dir = app
        .path()
        .download_dir()
        .map_err(|e| e.to_string())?
        .join("Reclaim");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let raw = url.split('?').next().unwrap_or(&url).rsplit('/').next().unwrap_or("download");
    let raw = if raw.is_empty() { "download" } else { raw };
    let path = unique_path(&dir, raw);
    std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
    let path_str = path.to_string_lossy().to_string();

    let now = chrono::Utc::now().to_rfc3339();
    let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO media_downloads (profile_id, url, file_path, kind, description, page_url, downloaded_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![profile_id, url, path_str, kind, description, page_url, now],
    )
    .map_err(|e| e.to_string())?;

    Ok(path_str)
}

/// Whether the `yt-dlp` CLI is available on PATH (for streaming-site downloads
/// like YouTube that have no plain file URL).
#[tauri::command(rename_all = "camelCase")]
pub async fn ytdlp_available() -> Result<bool, String> {
    Ok(tauri::async_runtime::spawn_blocking(|| {
        std::process::Command::new("yt-dlp")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    })
    .await
    .unwrap_or(false))
}

/// Download a streaming video from `url` (a page URL like a YouTube watch link)
/// via yt-dlp into `<Downloads>/Reclaim/`, and record it with the user's
/// description. Requires yt-dlp on PATH.
#[tauri::command(rename_all = "camelCase")]
pub async fn download_video_ytdlp(
    app: tauri::AppHandle,
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    url: String,
    description: String,
) -> Result<String, String> {
    let db_path = {
        let s = state.lock().map_err(|e| e.to_string())?;
        s.db_path.clone()
    };
    let dir = app
        .path()
        .download_dir()
        .map_err(|e| e.to_string())?
        .join("Reclaim");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let out_template = dir.join("%(title)s.%(ext)s").to_string_lossy().to_string();

    let url_for_proc = url.clone();
    let output = tauri::async_runtime::spawn_blocking(move || {
        std::process::Command::new("yt-dlp")
            .arg("--no-playlist")
            .arg("--no-progress")
            .arg("--print")
            .arg("after_move:filepath")
            .arg("-o")
            .arg(&out_template)
            .arg(&url_for_proc)
            .output()
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("failed to run yt-dlp (is it installed?): {}", e))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(err.lines().last().unwrap_or("yt-dlp failed").to_string());
    }
    let file_path = String::from_utf8_lossy(&output.stdout)
        .lines()
        .last()
        .unwrap_or("")
        .trim()
        .to_string();

    let now = chrono::Utc::now().to_rfc3339();
    let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO media_downloads (profile_id, url, file_path, kind, description, page_url, downloaded_at)
         VALUES (?1, ?2, ?3, 'video', ?4, ?2, ?5)",
        params![profile_id, url, file_path, description, now],
    )
    .map_err(|e| e.to_string())?;

    Ok(file_path)
}

/// List recorded downloads (most recent first) for the descriptions/AI view.
#[tauri::command(rename_all = "camelCase")]
pub async fn list_media_downloads(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
) -> Result<Vec<MediaDownload>, String> {
    let db_path = {
        let s = state.lock().map_err(|e| e.to_string())?;
        s.db_path.clone()
    };
    let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, url, file_path, kind, description, page_url, downloaded_at
             FROM media_downloads WHERE profile_id = ?1 ORDER BY id DESC LIMIT 200",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![profile_id], |r| {
            Ok(MediaDownload {
                id: r.get(0)?,
                url: r.get(1)?,
                file_path: r.get(2)?,
                kind: r.get(3)?,
                description: r.get(4)?,
                page_url: r.get(5)?,
                downloaded_at: r.get(6)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}
