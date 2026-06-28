//! YouTube video extraction using yt-dlp
//!
//! Provides functionality to extract stream URLs and metadata from YouTube videos.

use serde::{Deserialize, Serialize};
use std::process::Command;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum YouTubeError {
    #[error("yt-dlp not found or failed to execute: {0}")]
    ExtractorError(String),
    #[error("Failed to parse video info: {0}")]
    ParseError(String),
    #[error("Invalid YouTube URL")]
    InvalidUrl,
}

/// Video metadata extracted from YouTube
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoInfo {
    pub title: String,
    pub duration: f64,
    pub thumbnail: String,
    pub uploader: String,
    pub description: Option<String>,
    pub view_count: Option<i64>,
}

/// YouTube video extractor using yt-dlp
pub struct YouTubeExtractor;

impl YouTubeExtractor {
    /// Check if yt-dlp is installed and available
    pub fn is_available() -> bool {
        Command::new("yt-dlp").arg("--version").output().is_ok()
    }

    /// Check if URL is a YouTube URL
    pub fn is_youtube_url(url: &str) -> bool {
        url.contains("youtube.com")
            || url.contains("youtu.be")
            || url.contains("youtube-nocookie.com")
    }

    /// Extract direct stream URL from YouTube video
    ///
    /// Returns the best available video+audio stream URL
    pub fn extract_stream_url(youtube_url: &str) -> Result<String, YouTubeError> {
        if !Self::is_youtube_url(youtube_url) {
            return Err(YouTubeError::InvalidUrl);
        }

        log::info!("Extracting stream URL for: {}", youtube_url);

        let output = Command::new("yt-dlp")
            .arg("-g") // Get URL only
            .arg("-f") // Format selection
            .arg("bestvideo[ext=mp4]+bestaudio[ext=m4a]/best[ext=mp4]/best")
            .arg("--no-playlist") // Single video only
            .arg(youtube_url)
            .output()
            .map_err(|e| YouTubeError::ExtractorError(e.to_string()))?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            log::error!("yt-dlp failed: {}", error);
            return Err(YouTubeError::ExtractorError(error.to_string()));
        }

        let stream_url = String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()
            .ok_or_else(|| YouTubeError::ExtractorError("No URL returned".to_string()))?
            .trim()
            .to_string();

        log::info!("Extracted stream URL successfully");
        Ok(stream_url)
    }

    /// Get video metadata from YouTube
    pub fn get_info(youtube_url: &str) -> Result<VideoInfo, YouTubeError> {
        if !Self::is_youtube_url(youtube_url) {
            return Err(YouTubeError::InvalidUrl);
        }

        log::info!("Getting video info for: {}", youtube_url);

        let output = Command::new("yt-dlp")
            .arg("-J") // JSON output
            .arg("--no-playlist") // Single video only
            .arg(youtube_url)
            .output()
            .map_err(|e| YouTubeError::ExtractorError(e.to_string()))?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            log::error!("yt-dlp info failed: {}", error);
            return Err(YouTubeError::ExtractorError(error.to_string()));
        }

        // Parse JSON response
        let json: serde_json::Value = serde_json::from_slice(&output.stdout)
            .map_err(|e| YouTubeError::ParseError(e.to_string()))?;

        Ok(VideoInfo {
            title: json["title"].as_str().unwrap_or("Unknown").to_string(),
            duration: json["duration"].as_f64().unwrap_or(0.0),
            thumbnail: json["thumbnail"].as_str().unwrap_or("").to_string(),
            uploader: json["uploader"].as_str().unwrap_or("Unknown").to_string(),
            description: json["description"].as_str().map(|s| s.to_string()),
            view_count: json["view_count"].as_i64(),
        })
    }

    /// Extract both stream URL and metadata in one call (more efficient)
    pub fn extract_full(youtube_url: &str) -> Result<(String, VideoInfo), YouTubeError> {
        if !Self::is_youtube_url(youtube_url) {
            return Err(YouTubeError::InvalidUrl);
        }

        log::info!("Extracting full info for: {}", youtube_url);

        // Get JSON with URL
        let output = Command::new("yt-dlp")
            .arg("-J") // JSON output
            .arg("--no-playlist")
            .arg(youtube_url)
            .output()
            .map_err(|e| YouTubeError::ExtractorError(e.to_string()))?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(YouTubeError::ExtractorError(error.to_string()));
        }

        let json: serde_json::Value = serde_json::from_slice(&output.stdout)
            .map_err(|e| YouTubeError::ParseError(e.to_string()))?;

        let info = VideoInfo {
            title: json["title"].as_str().unwrap_or("Unknown").to_string(),
            duration: json["duration"].as_f64().unwrap_or(0.0),
            thumbnail: json["thumbnail"].as_str().unwrap_or("").to_string(),
            uploader: json["uploader"].as_str().unwrap_or("Unknown").to_string(),
            description: json["description"].as_str().map(|s| s.to_string()),
            view_count: json["view_count"].as_i64(),
        };

        // Now get stream URL
        let stream_url = Self::extract_stream_url(youtube_url)?;

        Ok((stream_url, info))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_youtube_url() {
        assert!(YouTubeExtractor::is_youtube_url(
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
        ));
        assert!(YouTubeExtractor::is_youtube_url(
            "https://youtu.be/dQw4w9WgXcQ"
        ));
        assert!(YouTubeExtractor::is_youtube_url(
            "https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ"
        ));
        assert!(!YouTubeExtractor::is_youtube_url(
            "https://example.com/video.mp4"
        ));
        assert!(!YouTubeExtractor::is_youtube_url(
            "file:///home/user/video.mkv"
        ));
    }

    #[test]
    fn test_yt_dlp_available() {
        let available = YouTubeExtractor::is_available();
        println!("yt-dlp available: {}", available);
        // This test just checks if the function runs without panicking
    }
}
