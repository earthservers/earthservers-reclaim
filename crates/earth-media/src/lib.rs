//! Earth Media - Hardware-accelerated media player for Earth Reclaim
//!
//! Uses GStreamer playbin directly for video/audio playback with hardware acceleration.
//! Bypasses gst_player API to avoid unwanted GTK overlay controls.
//! Includes YouTube support via yt-dlp.

mod youtube;

pub use youtube::{VideoInfo, YouTubeError, YouTubeExtractor};

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_video::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use thiserror::Error;

// ==================== NVDEC hardware decode (NVIDIA) ====================
//
// Native nvcodec NVDEC decoders. We deliberately do NOT use the vaapi /
// nvidia-vaapi-driver bridge (finickier on NVIDIA) — these are the direct
// CUDA/NVDEC elements. Selection is by RANK (not a hardcoded pipeline): we bump
// these above gst-libav so playbin auto-plugs them, with clean software fallback.
const NVDEC_DECODERS: &[&str] = &[
    "nvh264dec", "nvh265dec", "nvav1dec", "nvvp8dec", "nvvp9dec",
    // stateless variants (newer nvcodec builds)
    "nvh264sldec", "nvh265sldec", "nvav1sldec",
];

static NVDEC_INIT: std::sync::Once = std::sync::Once::new();
static NVDEC_AVAILABLE: AtomicBool = AtomicBool::new(false);

/// Detect present NVDEC decoders and bump their rank decisively above gst-libav
/// software decode (the key gotcha — libav silently outranks nvcodec otherwise,
/// so you get no acceleration despite everything being installed). Runs once;
/// must be called AFTER `gst::init()`. If none are present, leaves software
/// decode untouched and logs it — never fails.
fn init_nvdec_ranks() {
    NVDEC_INIT.call_once(|| {
        // Escape hatch: EARTH_NO_NVDEC=1 keeps software decode, to isolate whether
        // a media crash is in NVDEC vs elsewhere.
        if std::env::var("EARTH_NO_NVDEC").map(|v| v == "1").unwrap_or(false) {
            eprintln!("[earth-media] NVDEC disabled by EARTH_NO_NVDEC — software decode");
            return;
        }
        // gst-libav decoders are RANK_PRIMARY (256); push NVDEC clearly above.
        let hw_rank = gst::Rank::PRIMARY + 256;
        let mut activated: Vec<&str> = Vec::new();
        for name in NVDEC_DECODERS {
            if let Some(factory) = gst::ElementFactory::find(name) {
                factory.set_rank(hw_rank);
                activated.push(name);
            }
        }
        if activated.is_empty() {
            eprintln!(
                "[earth-media] NVDEC not found — using SOFTWARE decode. \
                 (Install the nvcodec GStreamer plugin for hardware decode.)"
            );
        } else {
            NVDEC_AVAILABLE.store(true, Ordering::Relaxed);
            eprintln!(
                "[earth-media] NVDEC hardware decode ENABLED (rank-bumped above libav): {:?}",
                activated
            );
        }
    });
}

/// Whether NVDEC hardware decoders were found and rank-bumped.
pub fn nvdec_available() -> bool {
    NVDEC_AVAILABLE.load(Ordering::Relaxed)
}

/// Build the video sink. Defaults to the PROVEN `xvimagesink` path — hardware
/// DECODE (NVDEC) is independent of the sink, so we don't risk presentation by
/// switching sinks. A GL sink (GPU presentation, no GPU->CPU copy) is opt-in via
/// `EARTH_VIDEO_SINK=glimagesink` because GL contexts are fragile on this kind of
/// reparented X11 surface (glimagesink can hard-crash on NVIDIA here).
fn build_video_sink(hw: bool) -> Option<gst::Element> {
    if let Ok(name) = std::env::var("EARTH_VIDEO_SINK") {
        if !name.is_empty() {
            log::info!("[earth-media] EARTH_VIDEO_SINK override: {}", name);
            return gst::ElementFactory::make(&name).build().ok();
        }
    }
    for name in ["xvimagesink", "ximagesink", "glimagesink"] {
        if let Ok(sink) = gst::ElementFactory::make(name).build() {
            eprintln!("[earth-media] video sink: {} (hw_decode={})", name, hw);
            return Some(sink);
        }
        log::warn!("[earth-media] video sink '{}' unavailable, trying next", name);
    }
    None
}

#[derive(Error, Debug)]
pub enum MediaError {
    #[error("GStreamer initialization failed: {0}")]
    InitError(String),
    #[error("Failed to create player: {0}")]
    PlayerError(String),
    #[error("Playback error: {0}")]
    PlaybackError(String),
    #[error("Invalid URI: {0}")]
    InvalidUri(String),
    #[error("Seek error: {0}")]
    SeekError(String),
    #[error("YouTube error: {0}")]
    YouTube(#[from] YouTubeError),
}

/// Current state of the media player
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PlaybackState {
    Stopped,
    Playing,
    Paused,
    Buffering,
    Error(String),
}

/// Media metadata
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MediaInfo {
    pub uri: Option<String>,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration_ms: Option<i64>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub is_video: bool,
    pub is_live: bool,
    /// YouTube-specific metadata
    pub youtube_info: Option<VideoInfo>,
}

/// Media player status for frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerStatus {
    pub state: PlaybackState,
    pub position_ms: i64,
    pub duration_ms: i64,
    pub volume: f64,
    pub muted: bool,
    pub info: MediaInfo,
    /// Whether NVDEC hardware decoders are available + rank-bumped on this system.
    pub hw_decode_available: bool,
    /// The video decoder element that actually instantiated in the LIVE pipeline
    /// (proves hardware vs software — e.g. "nvh264dec" vs "avdec_h264"). None until
    /// a video has loaded.
    pub active_decoder: Option<String>,
    /// True once playback has reached end-of-stream. The frontend edge-detects this
    /// (false -> true) to advance the queue, since playbin reports Playing at EOS.
    pub eos: bool,
}

/// Hardware-accelerated media player using GStreamer playbin directly
///
/// This uses playbin instead of gst_player::Player to avoid the GTK overlay
/// that gst_player adds for controls. We want our React UI to handle all controls.
pub struct MediaPlayer {
    /// The playbin pipeline
    pipeline: gst::Element,
    /// Video sink element for VideoOverlay
    video_sink: Option<gst::Element>,
    /// The video decoder element that actually instantiated (hardware vs software),
    /// observed live via `deep-element-added`. Proves NVDEC is really in use.
    active_decoder: Arc<Mutex<Option<String>>>,
    /// Stored window handle for VideoOverlay
    window_handle: Arc<Mutex<Option<u64>>>,
    info: Arc<Mutex<MediaInfo>>,
    muted: Arc<Mutex<bool>>,
    volume: Arc<Mutex<f64>>,
    last_error: Arc<Mutex<Option<String>>>,
    /// Set true when the pipeline posts an EOS message (playback reached the end).
    /// playbin stays in the Playing state at EOS with position frozen at the
    /// duration, so neither state nor position can detect end-of-stream — the bus
    /// EOS message is the only reliable signal. The frontend polls this via
    /// PlayerStatus to drive queue auto-advance. Reset to false on load/play/seek.
    eos: Arc<Mutex<bool>>,
}

impl MediaPlayer {
    /// Create a new media player using playbin directly
    pub fn new() -> Result<Self, MediaError> {
        // Initialize GStreamer
        gst::init().map_err(|e| MediaError::InitError(e.to_string()))?;

        log::info!("GStreamer initialized successfully (using playbin directly)");

        // Enable NVDEC hardware decode by bumping nvcodec decoder ranks above
        // gst-libav software decode. No-op (logged) on systems without nvcodec.
        init_nvdec_ranks();
        let hw_decode = nvdec_available();

        // Create playbin - this is the core playback element. playbin auto-plugs
        // the highest-ranked decoder for the media (NVDEC when bumped above).
        let pipeline = gst::ElementFactory::make("playbin")
            .build()
            .map_err(|e| MediaError::PlayerError(format!("Failed to create playbin: {}", e)))?;

        log::info!("Created playbin pipeline");

        // Create a video sink for X11 embedding with VideoOverlay support.
        // IMPORTANT: DO NOT use autovideosink - it ignores window handle and creates its own window!
        // With hardware decode active we prefer a GL sink; xvimagesink stays the fallback.
        let video_sink = build_video_sink(hw_decode);

        let stored_sink = match video_sink {
            Some(sink) => {
                let sink_name = sink.factory().map(|f| f.name().to_string()).unwrap_or_default();
                log::info!("Using video sink: {}", sink_name);

                // Configure the video sink for embedding
                // Force aspect ratio to be maintained
                if sink.has_property("force-aspect-ratio", None) {
                    sink.set_property("force-aspect-ratio", true);
                }

                // For xvimagesink: configure for smooth playback
                if sink.has_property("double-buffer", None) {
                    sink.set_property("double-buffer", true);
                }

                // Handle expose events automatically
                if sink.has_property("handle-expose", None) {
                    sink.set_property("handle-expose", true);
                }

                // CRITICAL: Disable navigation/event handling to remove controls
                if sink.has_property("handle-events", None) {
                    sink.set_property("handle-events", false);
                    log::info!("Disabled handle-events on video sink");
                }

                // Set the video sink on playbin
                pipeline.set_property("video-sink", &sink);
                log::info!("Set video-sink on playbin");

                Some(sink)
            }
            None => {
                log::error!("No VideoOverlay-compatible video sink available.");
                log::error!("Video may appear in separate window. Install gstreamer1.0-plugins-base.");
                None
            }
        };

        // Disable playbin flags that might add overlays
        // Flags: video (1), audio (2), text (4), vis (8), soft-volume (16), native-audio (32),
        //        native-video (64), download (128), buffering (256), deinterlace (512),
        //        soft-colorbalance (1024), force-filters (2048), force-sw-decoders (4096)
        // We want: video + audio + soft-volume = 1 + 2 + 16 = 19
        // Explicitly disable text (subtitles), vis (visualization), native-audio/video
        // Note: Use set_property_from_str to avoid type issues with GstPlayFlags
        // The string format uses the flag names separated by +
        let flags_str = "video+audio+soft-volume";
        pipeline.set_property_from_str("flags", flags_str);
        log::info!("Set playbin flags to '{}' (video + audio + soft-volume)", flags_str);

        // High-quality scaling (additive; never touches decode/sink selection).
        // Route any in-pipeline scaling through Lanczos resampling instead of the
        // default bilinear, for sharper output. The scaler is chosen to MATCH the
        // sink's memory path so we never add a needless GPU<->CPU transfer:
        //   * GL sink (glimagesink): glcolorscale — GPU-side, frames stay on the GPU.
        //   * xvimagesink/ximagesink: software videoscale method=lanczos. These sinks
        //     present from SYSTEM memory, so frames are already on the CPU path
        //     (even with NVDEC, which decodes on the GPU and then downloads for the
        //     sink) — a CUDA scaler here would only force an extra round-trip. This
        //     matches the "xvimagesink without GL -> CPU path -> software videoscale"
        //     rule and keeps NVDEC decode fully on the GPU.
        {
            let sink_name = stored_sink
                .as_ref()
                .and_then(|s| s.factory())
                .map(|f| f.name().to_string())
                .unwrap_or_default();

            let scaler: Option<gst::Element> = if sink_name.contains("gl") {
                // GL presentation path: GPU-side colour-convert + scale (no download).
                gst::ElementFactory::make("glcolorscale").build().ok()
            } else {
                // System-memory presentation path: CPU Lanczos resampling.
                gst::ElementFactory::make("videoscale").build().ok().map(|s| {
                    s.set_property_from_str("method", "lanczos");
                    s
                })
            };

            match scaler {
                Some(f) => {
                    let fname = f.factory().map(|x| x.name().to_string()).unwrap_or_default();
                    // playbin inserts this in the video path (surrounded by its own
                    // converters as needed). Bare videoscale = passthrough at native
                    // res, Lanczos only when a scale is actually negotiated.
                    pipeline.set_property("video-filter", &f);
                    log::info!(
                        "[earth-media] HQ scaling: video-filter={} (sink={}, {})",
                        fname,
                        sink_name,
                        if sink_name.contains("gl") { "GPU-side" } else { "CPU-side" }
                    );
                }
                None => {
                    log::warn!("[earth-media] HQ scaler unavailable — using default scaling");
                }
            }
        }

        let info = Arc::new(Mutex::new(MediaInfo::default()));
        let muted = Arc::new(Mutex::new(false));
        let volume = Arc::new(Mutex::new(1.0));
        let last_error = Arc::new(Mutex::new(None::<String>));
        let window_handle = Arc::new(Mutex::new(None::<u64>));

        // VERIFICATION: observe which decoder element actually instantiates in the
        // LIVE pipeline (playbin auto-plugs it). This proves hardware vs software
        // rather than just "what's installed". The decoder line always logs; set
        // EARTH_MEDIA_DEBUG=1 to also log every element added.
        let active_decoder = Arc::new(Mutex::new(None::<String>));
        if let Some(bin) = pipeline.dynamic_cast_ref::<gst::Bin>() {
            let ad = active_decoder.clone();
            let debug = std::env::var("EARTH_MEDIA_DEBUG").map(|v| v == "1").unwrap_or(false);
            bin.connect_deep_element_added(move |_bin, _sub_bin, element| {
                if let Some(factory) = element.factory() {
                    let klass = factory.klass();
                    let name = factory.name().to_string();
                    if klass.contains("Decoder") && klass.contains("Video") {
                        let is_hw = klass.contains("Hardware") || name.starts_with("nv");
                        eprintln!(
                            "[earth-media] video decoder instantiated: {} ({})",
                            name,
                            if is_hw { "HARDWARE / NVDEC" } else { "software" }
                        );
                        if let Ok(mut g) = ad.lock() {
                            *g = Some(name);
                        }
                    } else if debug {
                        eprintln!("[earth-media] element added: {} [{}]", name, klass);
                    }
                }
            });
        }

        // Set up bus handler for messages
        let eos = Arc::new(Mutex::new(false));

        let bus = pipeline.bus().expect("Pipeline should have a bus");
        let error_clone = last_error.clone();
        let window_handle_for_bus = window_handle.clone();
        let video_sink_for_bus = stored_sink.clone();
        let eos_for_bus = eos.clone();

        // Use sync handler to catch prepare-window-handle immediately
        bus.set_sync_handler(move |_bus, msg| {
            use gst::MessageView;

            match msg.view() {
                MessageView::Element(element_msg) => {
                    if let Some(structure) = element_msg.structure() {
                        let name = structure.name().as_str();
                        if name == "prepare-window-handle" {
                            log::info!("Received prepare-window-handle message");

                            if let Ok(handle_guard) = window_handle_for_bus.lock() {
                                if let Some(handle) = *handle_guard {
                                    log::info!("Setting window handle 0x{:x} in sync handler", handle);
                                    if let Some(ref sink) = video_sink_for_bus {
                                        // Use VideoOverlay interface
                                        if let Ok(overlay) = sink.clone().dynamic_cast::<gstreamer_video::VideoOverlay>() {
                                            unsafe {
                                                overlay.set_window_handle(handle as usize);
                                            }
                                            overlay.expose();
                                            log::info!("Window handle set successfully via VideoOverlay");
                                        } else {
                                            log::warn!("Video sink does not implement VideoOverlay");
                                        }
                                    }
                                } else {
                                    log::warn!("prepare-window-handle received but no window handle set!");
                                }
                            }
                            // Drop this message to prevent GStreamer from creating its own window
                            return gst::BusSyncReply::Drop;
                        }
                    }
                }
                MessageView::Error(err) => {
                    let error_msg = format!(
                        "Error from {:?}: {} ({:?})",
                        err.src().map(|s| s.path_string()),
                        err.error(),
                        err.debug()
                    );
                    log::error!("{}", error_msg);
                    if let Ok(mut e) = error_clone.lock() {
                        *e = Some(error_msg);
                    }
                }
                MessageView::Warning(warn) => {
                    log::warn!(
                        "Warning from {:?}: {} ({:?})",
                        warn.src().map(|s| s.path_string()),
                        warn.error(),
                        warn.debug()
                    );
                }
                MessageView::Eos(_) => {
                    // End of stream — playback reached the end. playbin stays in the
                    // Playing state here, so this flag is the only reliable end signal
                    // the frontend can poll to auto-advance the queue.
                    log::info!("Received EOS — playback reached end of stream");
                    if let Ok(mut e) = eos_for_bus.lock() {
                        *e = true;
                    }
                }
                _ => {}
            }

            gst::BusSyncReply::Pass
        });

        Ok(Self {
            pipeline,
            video_sink: stored_sink,
            active_decoder,
            window_handle,
            info,
            muted,
            volume,
            last_error,
            eos,
        })
    }

    /// The decoder element currently instantiated in the live pipeline, if a
    /// video has loaded (e.g. "nvh264dec" = hardware, "avdec_h264" = software).
    pub fn get_active_decoder(&self) -> Option<String> {
        self.active_decoder.lock().ok().and_then(|g| g.clone())
    }

    /// Set the window handle for embedded video playback (VideoOverlay)
    pub fn set_window_handle(&self, handle: u64) -> Result<(), MediaError> {
        if handle == 0 {
            log::error!("Invalid window handle: 0");
            return Err(MediaError::PlayerError("Invalid window handle: 0".to_string()));
        }

        // Store the handle for the sync bus handler
        if let Ok(mut h) = self.window_handle.lock() {
            *h = Some(handle);
            log::info!("Stored window handle 0x{:x}", handle);
        }

        // Also set it immediately if we have a video sink
        if let Some(ref sink) = self.video_sink {
            log::info!("Setting VideoOverlay window handle immediately: 0x{:x}", handle);
            if let Ok(overlay) = sink.clone().dynamic_cast::<gstreamer_video::VideoOverlay>() {
                unsafe {
                    overlay.set_window_handle(handle as usize);
                }
                overlay.expose();
                log::info!("VideoOverlay window handle set successfully");
            } else {
                log::warn!("Video sink does not implement VideoOverlay interface");
            }
            Ok(())
        } else {
            log::warn!("No video sink available for window embedding");
            Err(MediaError::PlayerError(
                "No video sink available for window embedding".to_string()
            ))
        }
    }

    /// Expose/refresh the video overlay (call after window resize)
    pub fn expose(&self) {
        if let Some(ref sink) = self.video_sink {
            if let Ok(overlay) = sink.clone().dynamic_cast::<gstreamer_video::VideoOverlay>() {
                overlay.expose();
            }
        }
    }

    /// Load media from URI (file:// or http://)
    pub fn load(&self, uri: &str) -> Result<(), MediaError> {
        if uri.is_empty() {
            return Err(MediaError::InvalidUri("Empty URI".to_string()));
        }

        if uri.starts_with("blob:") {
            return Err(MediaError::InvalidUri(
                "Blob URLs are not supported. Please use the file dialog to select files.".to_string()
            ));
        }

        let full_uri = if uri.starts_with("http://")
            || uri.starts_with("https://")
            || uri.starts_with("file://")
        {
            uri.to_string()
        } else if uri.starts_with('/') {
            format!("file://{}", uri)
        } else {
            return Err(MediaError::InvalidUri(format!(
                "Invalid URI format: {}",
                uri
            )));
        };

        log::info!("Loading media: {}", full_uri);

        // Clear any previous error
        if let Ok(mut err) = self.last_error.lock() {
            *err = None;
        }

        // New media — clear the end-of-stream flag from the previous clip.
        if let Ok(mut e) = self.eos.lock() {
            *e = false;
        }

        // Update info
        if let Ok(mut info) = self.info.lock() {
            *info = MediaInfo {
                uri: Some(full_uri.clone()),
                ..Default::default()
            };
        }

        // Set to NULL state first, then set URI
        let _ = self.pipeline.set_state(gst::State::Null);
        self.pipeline.set_property("uri", &full_uri);

        log::info!("URI set on playbin");

        Ok(())
    }

    /// Start playback
    pub fn play(&self) -> Result<(), MediaError> {
        log::info!("Starting playback...");

        // Resuming/restarting clears any stale end-of-stream flag (e.g. repeat-one
        // replays the same clip after EOS).
        if let Ok(mut e) = self.eos.lock() {
            *e = false;
        }

        self.pipeline.set_state(gst::State::Playing)
            .map_err(|e| MediaError::PlaybackError(format!("Failed to play: {:?}", e)))?;

        // Query the pipeline state
        let (result, current, pending) = self.pipeline.state(gst::ClockTime::from_mseconds(500));
        log::info!(
            "Pipeline state after play() - result: {:?}, current: {:?}, pending: {:?}",
            result, current, pending
        );

        Ok(())
    }

    /// Pause playback
    pub fn pause(&self) -> Result<(), MediaError> {
        log::info!("Pausing playback");
        self.pipeline.set_state(gst::State::Paused)
            .map_err(|e| MediaError::PlaybackError(format!("Failed to pause: {:?}", e)))?;
        Ok(())
    }

    /// Stop playback
    pub fn stop(&self) -> Result<(), MediaError> {
        log::info!("Stopping playback");
        self.pipeline.set_state(gst::State::Null)
            .map_err(|e| MediaError::PlaybackError(format!("Failed to stop: {:?}", e)))?;
        Ok(())
    }

    /// Seek to position in milliseconds
    pub fn seek(&self, position_ms: i64) -> Result<(), MediaError> {
        if position_ms < 0 {
            return Err(MediaError::SeekError("Negative position".to_string()));
        }

        log::info!("Seeking to {}ms", position_ms);

        // Seeking back into the clip clears the end-of-stream flag.
        if let Ok(mut e) = self.eos.lock() {
            *e = false;
        }

        let position = gst::ClockTime::from_mseconds(position_ms as u64);
        self.pipeline.seek_simple(
            gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
            position
        ).map_err(|e| MediaError::SeekError(e.to_string()))?;

        Ok(())
    }

    /// Set volume (0.0 to 1.0)
    pub fn set_volume(&self, vol: f64) -> Result<(), MediaError> {
        let clamped = vol.clamp(0.0, 1.0);
        log::info!("Setting volume to {}", clamped);
        self.pipeline.set_property("volume", clamped);
        if let Ok(mut v) = self.volume.lock() {
            *v = clamped;
        }
        Ok(())
    }

    /// Get current volume
    pub fn get_volume(&self) -> f64 {
        self.volume.lock().map(|v| *v).unwrap_or(1.0)
    }

    /// Set muted state
    pub fn set_muted(&self, mute: bool) -> Result<(), MediaError> {
        log::info!("Setting muted: {}", mute);
        self.pipeline.set_property("mute", mute);
        if let Ok(mut m) = self.muted.lock() {
            *m = mute;
        }
        Ok(())
    }

    /// Get muted state
    pub fn is_muted(&self) -> bool {
        self.muted.lock().map(|m| *m).unwrap_or(false)
    }

    /// Get current position in milliseconds
    pub fn get_position(&self) -> Option<i64> {
        self.pipeline.query_position::<gst::ClockTime>()
            .map(|p| p.mseconds() as i64)
    }

    /// Get total duration in milliseconds
    pub fn get_duration(&self) -> Option<i64> {
        self.pipeline.query_duration::<gst::ClockTime>()
            .map(|d| d.mseconds() as i64)
    }

    /// Get current playback state
    pub fn get_state(&self) -> PlaybackState {
        // Check for cached error first
        if let Ok(err) = self.last_error.lock() {
            if let Some(ref e) = *err {
                return PlaybackState::Error(e.clone());
            }
        }

        let (_, current, _) = self.pipeline.state(gst::ClockTime::from_mseconds(100));

        match current {
            gst::State::Playing => PlaybackState::Playing,
            gst::State::Paused => PlaybackState::Paused,
            gst::State::Ready | gst::State::Null => PlaybackState::Stopped,
            _ => PlaybackState::Stopped,
        }
    }

    /// Get media info
    pub fn get_info(&self) -> MediaInfo {
        let mut info = self.info.lock().map(|i| i.clone()).unwrap_or_default();

        // Update duration if available
        if let Some(duration) = self.get_duration() {
            info.duration_ms = Some(duration);
        }

        info
    }

    /// Get full player status for frontend
    pub fn get_status(&self) -> PlayerStatus {
        PlayerStatus {
            state: self.get_state(),
            position_ms: self.get_position().unwrap_or(0),
            duration_ms: self.get_duration().unwrap_or(0),
            volume: self.get_volume(),
            muted: self.is_muted(),
            info: self.get_info(),
            hw_decode_available: nvdec_available(),
            active_decoder: self.get_active_decoder(),
            eos: self.eos.lock().map(|e| *e).unwrap_or(false),
        }
    }

    /// Skip forward by seconds
    pub fn skip_forward(&self, seconds: i64) -> Result<(), MediaError> {
        if let Some(pos) = self.get_position() {
            let new_pos = pos + (seconds * 1000);
            self.seek(new_pos)?;
        }
        Ok(())
    }

    /// Skip backward by seconds
    pub fn skip_backward(&self, seconds: i64) -> Result<(), MediaError> {
        if let Some(pos) = self.get_position() {
            let new_pos = (pos - (seconds * 1000)).max(0);
            self.seek(new_pos)?;
        }
        Ok(())
    }

    // ==================== YouTube Support ====================

    /// Check if a URL is a YouTube URL
    pub fn is_youtube_url(url: &str) -> bool {
        YouTubeExtractor::is_youtube_url(url)
    }

    /// Check if yt-dlp is available
    pub fn is_youtube_available() -> bool {
        YouTubeExtractor::is_available()
    }

    /// Get YouTube video info without playing
    pub fn get_youtube_info(url: &str) -> Result<VideoInfo, MediaError> {
        Ok(YouTubeExtractor::get_info(url)?)
    }

    /// Play a YouTube video - extracts stream URL and plays it
    pub fn play_youtube(&self, url: &str) -> Result<VideoInfo, MediaError> {
        log::info!("Playing YouTube video: {}", url);

        // Extract stream URL and info
        let (stream_url, video_info) = YouTubeExtractor::extract_full(url)?;

        // Update info with YouTube metadata
        if let Ok(mut info) = self.info.lock() {
            *info = MediaInfo {
                uri: Some(url.to_string()),
                title: Some(video_info.title.clone()),
                artist: Some(video_info.uploader.clone()),
                duration_ms: Some((video_info.duration * 1000.0) as i64),
                is_video: true,
                youtube_info: Some(video_info.clone()),
                ..Default::default()
            };
        }

        // Set URI and play
        let _ = self.pipeline.set_state(gst::State::Null);
        self.pipeline.set_property("uri", &stream_url);
        self.pipeline.set_state(gst::State::Playing)
            .map_err(|e| MediaError::PlaybackError(format!("Failed to play YouTube: {:?}", e)))?;

        Ok(video_info)
    }
}

impl Drop for MediaPlayer {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

// ==================== Multi-Player Manager ====================

use std::collections::HashMap;

/// Manages multiple media player instances for multi-pane playback
pub struct MediaPlayerManager {
    players: Mutex<HashMap<String, MediaPlayer>>,
    initialized: Mutex<bool>,
}

/// Helper function to recover from poisoned mutex
fn recover_lock<T>(result: std::sync::LockResult<std::sync::MutexGuard<'_, T>>) -> Result<std::sync::MutexGuard<'_, T>, MediaError> {
    match result {
        Ok(guard) => Ok(guard),
        Err(poisoned) => {
            log::warn!("Recovering from poisoned mutex");
            Ok(poisoned.into_inner())
        }
    }
}

impl MediaPlayerManager {
    /// Create a new player manager
    pub fn new() -> Self {
        Self {
            players: Mutex::new(HashMap::new()),
            initialized: Mutex::new(false),
        }
    }

    /// Initialize GStreamer (called once)
    fn ensure_init(&self) -> Result<(), MediaError> {
        let mut init = recover_lock(self.initialized.lock())?;
        if !*init {
            gst::init().map_err(|e| MediaError::InitError(e.to_string()))?;
            *init = true;
            log::info!("GStreamer initialized for multi-player manager");
        }
        Ok(())
    }

    /// Get or create a player for a specific pane
    pub fn get_or_create_player(&self, player_id: &str) -> Result<(), MediaError> {
        self.ensure_init()?;

        let mut players = recover_lock(self.players.lock())?;
        if !players.contains_key(player_id) {
            log::info!("Creating new player: {}", player_id);
            let player = MediaPlayer::new()?;
            players.insert(player_id.to_string(), player);
        }
        Ok(())
    }

    /// Remove a player
    pub fn remove_player(&self, player_id: &str) -> Result<(), MediaError> {
        let mut players = recover_lock(self.players.lock())?;
        if let Some(player) = players.remove(player_id) {
            drop(player); // Explicitly stop the player
            log::info!("Removed player: {}", player_id);
        }
        Ok(())
    }

    /// Load media on a specific player
    pub fn load(&self, player_id: &str, uri: &str) -> Result<(), MediaError> {
        self.get_or_create_player(player_id)?;
        let players = recover_lock(self.players.lock())?;
        if let Some(player) = players.get(player_id) {
            player.load(uri)
        } else {
            Err(MediaError::PlayerError(format!("Player {} not found", player_id)))
        }
    }

    /// Play on a specific player (auto-creates if needed)
    pub fn play(&self, player_id: &str) -> Result<(), MediaError> {
        self.get_or_create_player(player_id)?;
        let players = recover_lock(self.players.lock())?;
        if let Some(player) = players.get(player_id) {
            player.play()
        } else {
            Err(MediaError::PlayerError(format!("Player {} not found", player_id)))
        }
    }

    /// Pause on a specific player (auto-creates if needed)
    pub fn pause(&self, player_id: &str) -> Result<(), MediaError> {
        self.get_or_create_player(player_id)?;
        let players = recover_lock(self.players.lock())?;
        if let Some(player) = players.get(player_id) {
            player.pause()
        } else {
            Err(MediaError::PlayerError(format!("Player {} not found", player_id)))
        }
    }

    /// Stop on a specific player (auto-creates if needed)
    pub fn stop(&self, player_id: &str) -> Result<(), MediaError> {
        self.get_or_create_player(player_id)?;
        let players = recover_lock(self.players.lock())?;
        if let Some(player) = players.get(player_id) {
            player.stop()
        } else {
            Err(MediaError::PlayerError(format!("Player {} not found", player_id)))
        }
    }

    /// Seek on a specific player (auto-creates if needed)
    pub fn seek(&self, player_id: &str, position_ms: i64) -> Result<(), MediaError> {
        self.get_or_create_player(player_id)?;
        let players = recover_lock(self.players.lock())?;
        if let Some(player) = players.get(player_id) {
            player.seek(position_ms)
        } else {
            Err(MediaError::PlayerError(format!("Player {} not found", player_id)))
        }
    }

    /// Set volume on a specific player (auto-creates if needed)
    pub fn set_volume(&self, player_id: &str, volume: f64) -> Result<(), MediaError> {
        self.get_or_create_player(player_id)?;
        let players = recover_lock(self.players.lock())?;
        if let Some(player) = players.get(player_id) {
            player.set_volume(volume)
        } else {
            Err(MediaError::PlayerError(format!("Player {} not found", player_id)))
        }
    }

    /// Set muted state on a specific player (auto-creates if needed)
    pub fn set_muted(&self, player_id: &str, muted: bool) -> Result<(), MediaError> {
        self.get_or_create_player(player_id)?;
        let players = recover_lock(self.players.lock())?;
        if let Some(player) = players.get(player_id) {
            player.set_muted(muted)
        } else {
            Err(MediaError::PlayerError(format!("Player {} not found", player_id)))
        }
    }

    /// Get status of a specific player
    pub fn get_status(&self, player_id: &str) -> Result<PlayerStatus, MediaError> {
        self.get_or_create_player(player_id)?;
        let players = recover_lock(self.players.lock())?;
        if let Some(player) = players.get(player_id) {
            Ok(player.get_status())
        } else {
            Err(MediaError::PlayerError(format!("Player {} not found", player_id)))
        }
    }

    /// Skip forward on a specific player (auto-creates if needed)
    pub fn skip_forward(&self, player_id: &str, seconds: i64) -> Result<(), MediaError> {
        self.get_or_create_player(player_id)?;
        let players = recover_lock(self.players.lock())?;
        if let Some(player) = players.get(player_id) {
            player.skip_forward(seconds)
        } else {
            Err(MediaError::PlayerError(format!("Player {} not found", player_id)))
        }
    }

    /// Skip backward on a specific player (auto-creates if needed)
    pub fn skip_backward(&self, player_id: &str, seconds: i64) -> Result<(), MediaError> {
        self.get_or_create_player(player_id)?;
        let players = recover_lock(self.players.lock())?;
        if let Some(player) = players.get(player_id) {
            player.skip_backward(seconds)
        } else {
            Err(MediaError::PlayerError(format!("Player {} not found", player_id)))
        }
    }

    /// Play YouTube on a specific player
    pub fn play_youtube(&self, player_id: &str, url: &str) -> Result<VideoInfo, MediaError> {
        self.get_or_create_player(player_id)?;
        let players = recover_lock(self.players.lock())?;
        if let Some(player) = players.get(player_id) {
            player.play_youtube(url)
        } else {
            Err(MediaError::PlayerError(format!("Player {} not found", player_id)))
        }
    }

    /// Set window handle for embedded video on a specific player
    ///
    /// This allows GStreamer to render video directly into an existing window
    /// (e.g., the Tauri app window) instead of creating a popup window.
    ///
    /// IMPORTANT: This should be called BEFORE loading media to ensure the
    /// VideoOverlay uses our window instead of creating its own.
    pub fn set_window_handle(&self, player_id: &str, handle: u64) -> Result<(), MediaError> {
        self.get_or_create_player(player_id)?;
        let players = recover_lock(self.players.lock())?;
        if let Some(player) = players.get(player_id) {
            log::info!("Setting window handle 0x{:x} on player '{}'", handle, player_id);
            player.set_window_handle(handle)
        } else {
            Err(MediaError::PlayerError(format!("Player {} not found", player_id)))
        }
    }

    /// Expose/refresh the video overlay on a specific player (call after window resize)
    pub fn expose(&self, player_id: &str) -> Result<(), MediaError> {
        let players = recover_lock(self.players.lock())?;
        if let Some(player) = players.get(player_id) {
            player.expose();
            Ok(())
        } else {
            // Not an error if player doesn't exist yet
            Ok(())
        }
    }

    /// Get status of all players
    pub fn get_all_statuses(&self) -> Result<HashMap<String, PlayerStatus>, MediaError> {
        let players = recover_lock(self.players.lock())?;
        let mut statuses = HashMap::new();
        for (id, player) in players.iter() {
            statuses.insert(id.clone(), player.get_status());
        }
        Ok(statuses)
    }

    /// Stop all players
    pub fn stop_all(&self) -> Result<(), MediaError> {
        let players = recover_lock(self.players.lock())?;
        for player in players.values() {
            let _ = player.stop(); // Ignore errors during cleanup
        }
        Ok(())
    }

    /// List all active player IDs
    pub fn list_players(&self) -> Result<Vec<String>, MediaError> {
        let players = recover_lock(self.players.lock())?;
        Ok(players.keys().cloned().collect())
    }
}

impl Default for MediaPlayerManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if GStreamer is available and properly configured
pub fn check_gstreamer() -> Result<String, MediaError> {
    gst::init().map_err(|e| MediaError::InitError(e.to_string()))?;

    let version = gst::version_string();
    log::info!("GStreamer version: {}", version);

    // Check for required elements instead of plugins (more reliable across distros)
    // Element names are consistent, plugin names vary
    let required_elements = [
        "playbin",       // Core playback element
        "videoconvert",  // Video format conversion
        "audioconvert",  // Audio format conversion
    ];

    let mut missing = Vec::new();
    for element in &required_elements {
        if gst::ElementFactory::find(element).is_none() {
            missing.push(*element);
        }
    }

    if !missing.is_empty() {
        log::warn!("Missing GStreamer elements: {:?}", missing);
        // Don't fail hard - playbin might still work with basic functionality
        // Just log a warning and continue
    }

    // Try to create a playbin to verify it actually works
    match gst::ElementFactory::make("playbin").build() {
        Ok(_) => {
            log::info!("GStreamer playbin verified working");
            Ok(version.to_string())
        }
        Err(e) => {
            log::error!("Failed to create playbin: {}", e);
            Err(MediaError::InitError(format!(
                "GStreamer playbin not available: {}",
                e
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gstreamer_check() {
        let result = check_gstreamer();
        println!("GStreamer check: {:?}", result);
        assert!(result.is_ok());
    }

    #[test]
    fn test_youtube_url_detection() {
        assert!(MediaPlayer::is_youtube_url(
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
        ));
        assert!(MediaPlayer::is_youtube_url("https://youtu.be/dQw4w9WgXcQ"));
        assert!(!MediaPlayer::is_youtube_url(
            "https://example.com/video.mp4"
        ));
    }
}
