// EarthMultiMedia GStreamer Video Player Component
// Native GStreamer-based video player with multi-pane support
// Uses earth-media crate via Tauri commands for hardware-accelerated playback
// Supports VideoOverlay for embedded playback on Linux/X11

import { useState, useRef, useEffect, useCallback } from 'react';
import { invoke, isTauri } from '../../lib/tauri';

interface GStreamerVideoPlayerProps {
  source: string;
  title?: string;
  autoPlay?: boolean;
  playerId: string; // Unique player ID for multi-pane support
  onTimeUpdate?: (currentTime: number, duration: number) => void;
  onEnded?: () => void;
  onError?: (error: string) => void;
  onStatusChange?: (status: PlayerStatusExport) => void; // For parent to track player state
  className?: string;
  hideControls?: boolean; // Hide built-in controls (for stacked controls in parent)
  startPositionMs?: number; // Seek here right after load (used to restore a tab's playback position)
  isActive?: boolean; // Whether this is the focused pane — gates global keyboard/control events
}

// Exported status for parent components to use
export interface PlayerStatusExport {
  playerId: string;
  title?: string;
  isPlaying: boolean;
  currentTime: number;
  duration: number;
  volume: number;
  isMuted: boolean;
  isLoading: boolean;
  error: string | null;
}

// PlaybackState can be a simple string or an object with Error variant
type PlaybackState = 'Stopped' | 'Playing' | 'Paused' | 'Buffering' | { Error: string };

interface PlayerStatus {
  state: PlaybackState;
  position_ms: number;
  duration_ms: number;
  volume: number;
  muted: boolean;
  info: {
    uri: string | null;
    title: string | null;
    artist: string | null;
    album: string | null;
    duration_ms: number | null;
    width: number | null;
    height: number | null;
    is_video: boolean;
    is_live: boolean;
  };
  /// True once playback reached end-of-stream (playbin stays "Playing" at EOS,
  /// so this flag — not state/position — is the reliable end signal).
  eos?: boolean;
}

// Helper to check playback state
function isStatePlaying(state: PlaybackState): boolean {
  return state === 'Playing';
}

function isStateError(state: PlaybackState): state is { Error: string } {
  return typeof state === 'object' && 'Error' in state;
}

interface VideoState {
  isPlaying: boolean;
  currentTime: number;
  duration: number;
  volume: number;
  isMuted: boolean;
  isFullscreen: boolean;
  playbackRate: number;
  isLoading: boolean;
  error: string | null;
  gstreamerAvailable: boolean;
  embeddedMode: boolean; // True if video renders inside app window
  embeddedError: string | null; // Error when setting up embedded mode
}

export function GStreamerVideoPlayer({
  source,
  title,
  autoPlay = false,
  playerId,
  onTimeUpdate,
  onEnded,
  onError,
  onStatusChange,
  className = '',
  startPositionMs = 0,
  isActive = true,
}: GStreamerVideoPlayerProps) {
  // Only the focused pane should respond to global keyboard shortcuts and the
  // app-wide media-* events; otherwise arrow-seek / play-pause hit every pane.
  const isActiveRef = useRef(isActive);
  isActiveRef.current = isActive;
  const containerRef = useRef<HTMLDivElement>(null);
  const videoAreaRef = useRef<HTMLDivElement>(null);
  const progressRef = useRef<HTMLDivElement>(null);
  // Latest desired restore position, read inside the (source-keyed) load effect
  // without re-triggering it — it changes as the live position is tracked.
  const startPositionMsRef = useRef(startPositionMs);
  startPositionMsRef.current = startPositionMs;
  const statusPollRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const surfaceCreatedRef = useRef(false);
  // Retry count for transient surface-creation failures. Without retry, a single
  // failure (e.g. X11 contention when several panes/controls are created at once)
  // falls straight through to the "plays in its own window" path — the detach.
  const surfaceRetryRef = useRef(0);
  const [state, setState] = useState<VideoState>({
    isPlaying: false,
    currentTime: 0,
    duration: 0,
    volume: 1,
    isMuted: false,
    isFullscreen: false,
    playbackRate: 1,
    isLoading: true,
    error: null,
    gstreamerAvailable: false,
    embeddedMode: false,
    embeddedError: null,
  });

  const controlsTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const clickTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const lastPositionRef = useRef<number>(0);
  // Tracks the previous poll's EOS flag so onEnded fires exactly once on the
  // false -> true edge (the flag stays true until the next clip loads).
  const eosFiredRef = useRef<boolean>(false);

  // Check GStreamer availability on mount
  useEffect(() => {
    const checkGStreamer = async () => {
      if (!isTauri()) {
        setState(s => ({ ...s, gstreamerAvailable: false }));
        return;
      }
      try {
        await invoke<string>('media_check_gstreamer');
        setState(s => ({ ...s, gstreamerAvailable: true }));
      } catch {
        setState(s => ({ ...s, gstreamerAvailable: false }));
      }
    };
    checkGStreamer();
  }, []);

  // Set up embedded video mode by creating an X11 child window for the video area
  // This creates a dedicated window that GStreamer's VideoOverlay can render into
  // For background playback: first try to reuse existing surface, only create if needed
  useEffect(() => {
    if (!state.gstreamerAvailable || !isTauri()) {
      // Not in Tauri or GStreamer not available - set error so media can load anyway
      if (!isTauri()) {
        setState(s => ({ ...s, embeddedError: 'Not in Tauri environment' }));
      }
      return;
    }
    if (surfaceCreatedRef.current) return; // Only setup once per mount

    const setupVideoSurface = async () => {
      try {
        const videoArea = videoAreaRef.current;
        if (!videoArea) {
          console.error('[GStreamer] Video area ref not available');
          setState(s => ({ ...s, embeddedError: 'Video area not available' }));
          return;
        }

        // Get the video area bounds relative to the window
        const rect = videoArea.getBoundingClientRect();

        // Validate bounds - must have reasonable size
        if (rect.width < 10 || rect.height < 10) {
          console.warn('[GStreamer] Video area too small, will retry...', rect);
          // Don't set error - retry will happen when component re-renders
          // But set a timeout to retry
          setTimeout(setupVideoSurface, 200);
          return;
        }

        const bounds = {
          x: Math.round(rect.left),
          y: Math.round(rect.top),
          width: Math.round(rect.width),
          height: Math.round(rect.height),
        };

        // First, try to show an existing surface (for background playback support)
        // If the user switched tabs and is coming back, the surface already exists
        try {
          await invoke('show_video_surface', { playerId });
          // Surface exists! Update its position and mark as ready
          console.log('[GStreamer] Reusing existing video surface for player:', playerId);
          await invoke('update_video_surface', { playerId, bounds });
          // RE-ATTACH the window handle. The surface outlives the player (close /
          // remove-from-queue does player_remove but only HIDES the surface), so a
          // reused surface may now front a brand-new, handle-less player. Without
          // re-attaching, the reloaded video renders into its own top-level window
          // (the "pane detaches" bug after remove → re-add). set_window_handle
          // get-or-creates the player, so the handle is in place before load.
          const reuseXid = await invoke<number>('get_video_surface_xid', { playerId }).catch(() => 0);
          if (reuseXid && reuseXid > 0) {
            await invoke('player_set_window_handle', { playerId, handle: reuseXid }).catch(() => {});
          }
          // Repaint the (possibly paused) current frame into the resized surface.
          // Re-mapping + resizing an X11 window clears it and the resize lands
          // async, so a single expose often paints into an unsettled window =>
          // black. Re-expose across the settle window, and re-apply the bounds
          // once more after layout finalizes (the videoArea rect can still be
          // shifting right after remount).
          await invoke('player_expose', { playerId });
          const repaint = async () => {
            const r = videoArea.getBoundingClientRect();
            if (r.width > 10 && r.height > 10) {
              await invoke('update_video_surface', {
                playerId,
                bounds: { x: Math.round(r.left), y: Math.round(r.top), width: Math.round(r.width), height: Math.round(r.height) },
              }).catch(() => {});
            }
            invoke('player_expose', { playerId }).catch(() => {});
          };
          setTimeout(repaint, 80);
          setTimeout(repaint, 220);
          setTimeout(repaint, 450);
          surfaceCreatedRef.current = true;
          setState(s => ({ ...s, embeddedMode: true, embeddedError: null }));
          return;
        } catch {
          // Surface doesn't exist yet, create a new one
          console.log('[GStreamer] No existing surface, creating new one for player:', playerId);
        }

        console.log('[GStreamer] Creating video surface for player:', playerId, 'at:', bounds);

        // Create X11 child window for video rendering
        const xid = await invoke<number>('create_video_surface', {
          playerId,
          bounds,
        });

        console.log('[GStreamer] create_video_surface returned XID:', xid ? `0x${xid.toString(16)}` : 'null');

        if (xid && xid > 0) {
          // Set the window handle on the player for VideoOverlay rendering BEFORE
          // marking the surface ready. The media-load effect is gated on
          // surfaceCreatedRef/embeddedMode; if we flipped the flag first, playback
          // could start before the handle was attached and GStreamer would render
          // into its OWN top-level window (the "pane detaches into a window" bug,
          // most visible when several panes mount at once).
          console.log('[GStreamer] Setting window handle on player...');
          await invoke('player_set_window_handle', { playerId, handle: xid });
          surfaceCreatedRef.current = true;
          console.log('[GStreamer] Video surface created and handle set successfully');
          setState(s => ({ ...s, embeddedMode: true, embeddedError: null }));
        } else {
          // Transient bad XID — retry before giving up (giving up pops the video
          // out into its own window).
          if (surfaceRetryRef.current < 4) {
            surfaceRetryRef.current += 1;
            console.warn(`[GStreamer] invalid XID, retry ${surfaceRetryRef.current} for`, playerId);
            setTimeout(setupVideoSurface, 200);
          } else {
            console.error('[GStreamer] Invalid XID returned:', xid);
            setState(s => ({ ...s, embeddedError: 'Invalid XID returned from surface creation' }));
          }
        }
      } catch (err) {
        const errorMsg = String(err);
        // Genuine non-X11 environments fall back immediately; otherwise this is
        // likely transient X11 contention (several surfaces/controls created at
        // once) — retry a few times so the pane embeds instead of detaching.
        const hardFail = errorMsg.includes('Wayland') || errorMsg.includes('X11');
        if (!hardFail && surfaceRetryRef.current < 4) {
          surfaceRetryRef.current += 1;
          console.warn(`[GStreamer] surface setup failed, retry ${surfaceRetryRef.current} for ${playerId}:`, errorMsg);
          setTimeout(setupVideoSurface, 200);
          return;
        }
        console.error('[GStreamer] Video surface creation failed:', errorMsg);
        setState(s => ({
          ...s,
          embeddedMode: false,
          embeddedError: errorMsg.includes('Wayland')
            ? 'Video plays in separate window (Wayland detected)'
            : errorMsg.includes('X11')
            ? 'Video plays in separate window (X11 required)'
            : `Embedded mode failed: ${errorMsg}`
        }));
      }
    };

    // Small delay to ensure DOM is ready
    const timer = setTimeout(setupVideoSurface, 100);
    return () => clearTimeout(timer);
  }, [playerId, state.gstreamerAvailable]);

  // Handle window resize/move - update video surface position
  useEffect(() => {
    if (!state.embeddedMode || !state.gstreamerAvailable || !isTauri() || !videoAreaRef.current) return;

    const updateSurface = async () => {
      const videoArea = videoAreaRef.current;
      if (!videoArea) return;

      const rect = videoArea.getBoundingClientRect();
      const bounds = {
        x: Math.round(rect.left),
        y: Math.round(rect.top),
        width: Math.round(rect.width),
        height: Math.round(rect.height),
      };

      // Resize FIRST, then expose — order matters. An X11 resize can clear the
      // window, so a paused frame needs an expose AFTER the resize to repaint
      // (otherwise it stays black until the next decoded frame, i.e. until play).
      try {
        await invoke('update_video_surface', { playerId, bounds });
        await invoke('player_expose', { playerId });
        // One more expose after the resize settles, to catch the repaint.
        setTimeout(() => { invoke('player_expose', { playerId }).catch(() => {}); }, 60);
      } catch {
        // ignore transient update failures
      }
    };

    // Immediate update when embedded mode is set
    updateSurface();

    window.addEventListener('resize', updateSurface);
    window.addEventListener('scroll', updateSurface);

    // Also update on any layout changes
    const observer = new ResizeObserver(updateSurface);
    observer.observe(videoAreaRef.current);

    return () => {
      window.removeEventListener('resize', updateSurface);
      window.removeEventListener('scroll', updateSurface);
      observer.disconnect();
    };
  }, [playerId, state.embeddedMode, state.gstreamerAvailable]);

  // GTK controls overlay is DISABLED - now using floating webview controls
  // managed at the EarthMultiMedia level instead of per-player GTK overlays
  // The floating webview controls are created in EarthMultiMedia/index.tsx
  // and communicate with players via Tauri events

  // Cleanup video surface on unmount
  // For background playback support: only hide, don't stop or destroy
  // The parent EarthMultiMedia handles showing/hiding surfaces on tab switches
  useEffect(() => {
    const currentPlayerId = playerId;

    return () => {
      // Check if surface was created at cleanup time
      const surfaceWasCreated = surfaceCreatedRef.current;

      if (isTauri()) {
        console.log('[GStreamer] Cleanup starting for', currentPlayerId, 'surface:', surfaceWasCreated);

        // Only hide the surface - keep player running for background playback
        // The parent EarthMultiMedia will show the surface again when returning to Media tab
        if (surfaceWasCreated) {
          invoke('hide_video_surface', { playerId: currentPlayerId }).catch(() => {});
        }

        // Don't reset surfaceCreatedRef - we want to remember the surface exists
        // Don't stop the player - keep playing in background
        // Don't destroy the surface - we want to show it again later
        console.log('[GStreamer] Surface hidden (keeping playback):', currentPlayerId);
      }
    };
  }, [playerId]);

  // Format time as HH:MM:SS or MM:SS
  const formatTime = (ms: number): string => {
    const seconds = Math.floor(ms / 1000);
    if (isNaN(seconds)) return '0:00';
    const hrs = Math.floor(seconds / 3600);
    const mins = Math.floor((seconds % 3600) / 60);
    const secs = Math.floor(seconds % 60);
    if (hrs > 0) {
      return `${hrs}:${mins.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}`;
    }
    return `${mins}:${secs.toString().padStart(2, '0')}`;
  };

  // Check if source is a YouTube URL
  const isYouTubeUrl = (url: string): boolean => {
    return url.includes('youtube.com') || url.includes('youtu.be');
  };

  // Load media source - use ref to track if we've already loaded this source
  const loadedSourceRef = useRef<string | null>(null);

  // Load source when it changes or GStreamer becomes available
  // IMPORTANT: Wait for embedded mode to be set up first (if on X11)
  // For background playback: check if player already has this source loaded
  useEffect(() => {
    if (!source || !state.gstreamerAvailable) return;

    // Prevent duplicate loads of the same source
    if (loadedSourceRef.current === source) return;

    // Wait for embedded mode to be ready before loading media
    // This ensures the window handle is set before playback starts
    // If embeddedMode is true or embeddedError is set, we can proceed
    // If neither, the surface setup is still in progress
    if (!state.embeddedMode && !state.embeddedError && surfaceCreatedRef.current === false) {
      console.log('[GStreamer] Waiting for video surface setup before loading media...');
      return; // Will be re-triggered when embeddedMode changes
    }

    const loadMedia = async () => {
      setState(s => ({ ...s, isLoading: true, error: null }));

      try {
        // Check if player already has media loaded (for background playback support)
        // When returning to the Media tab, the player might still be playing
        interface PlayerStatus {
          state: string;
          position_ms: number;
          duration_ms: number;
          volume: number;
          muted: boolean;
          info: { uri?: string };
        }
        let status: PlayerStatus | null = null;

        try {
          status = await invoke<PlayerStatus>('player_get_status', { playerId });
        } catch {
          // Player doesn't exist yet, will create it below
        }

        if (status && status.info?.uri === source && (status.state === 'Playing' || status.state === 'Paused')) {
          console.log('[GStreamer] Player already has this source loaded, resuming:', source);
          loadedSourceRef.current = source;
          // Update state from existing player status
          setState(s => ({
            ...s,
            isLoading: false,
            isPlaying: status.state === 'Playing',
            currentTime: status.position_ms,
            duration: status.duration_ms,
            volume: status.volume,
            isMuted: status.muted,
          }));
          return;
        }

        loadedSourceRef.current = source;
        console.log('[GStreamer] Loading media:', source, 'playerId:', playerId, 'embeddedMode:', state.embeddedMode);

        if (isYouTubeUrl(source)) {
          // Use YouTube extraction (this also plays)
          await invoke('player_play_youtube', { playerId, url: source });
          console.log('[GStreamer] YouTube video loaded and playing');
        } else {
          // Regular media load
          await invoke('player_load', { playerId, uri: source });
          console.log('[GStreamer] Media loaded, autoPlay:', autoPlay);

          // Small delay to let pipeline initialize before playing
          await new Promise(resolve => setTimeout(resolve, 100));

          // Restore a saved playback position (tab switch) before (re)starting.
          const startMs = startPositionMsRef.current;
          if (startMs > 0) {
            await invoke('player_seek', { playerId, positionMs: Math.floor(startMs) }).catch(() => {});
          }

          // Resume in the saved play/pause state. autoPlay === false means the
          // tab was paused when the user switched away — keep it paused at the
          // restored position; otherwise play (the default for opening media).
          if (autoPlay === false) {
            await invoke('player_pause', { playerId });
            console.log('[GStreamer] Restored paused at', startMs, 'ms');
          } else {
            await invoke('player_play', { playerId });
            console.log('[GStreamer] Play command sent');
          }
        }

        setState(s => ({ ...s, isLoading: false }));
      } catch (err) {
        const errorMsg = `Failed to load media: ${err}`;
        console.error('[GStreamer] Load error:', errorMsg);
        setState(s => ({ ...s, error: errorMsg, isLoading: false }));
        onError?.(errorMsg);
      }
    };

    loadMedia();
  }, [source, playerId, state.gstreamerAvailable, state.embeddedMode, state.embeddedError, autoPlay, onError]);

  // Poll player status
  useEffect(() => {
    if (!state.gstreamerAvailable) return;

    const pollStatus = async () => {
      try {
        const status = await invoke<PlayerStatus>('player_get_status', { playerId });

        const isPlaying = isStatePlaying(status.state);
        const currentTime = status.position_ms;
        const duration = status.duration_ms || status.info?.duration_ms || 0;

        // Check for error state
        if (isStateError(status.state)) {
          const errorMsg = status.state.Error;
          setState(s => ({
            ...s,
            error: errorMsg,
            isLoading: false,
            isPlaying: false,
          }));
          onError?.(errorMsg);
          return;
        }

        setState(s => ({
          ...s,
          isPlaying,
          currentTime,
          duration,
          volume: status.volume,
          isMuted: status.muted,
          isLoading: false,
          error: null, // Clear any previous error
        }));

        // Emit time update
        onTimeUpdate?.(currentTime / 1000, duration / 1000);

        // Emit status change for parent to track (used for stacked controls)
        onStatusChange?.({
          playerId,
          title,
          isPlaying,
          currentTime: currentTime / 1000,
          duration: duration / 1000,
          volume: status.volume,
          isMuted: status.muted,
          isLoading: false,
          error: null,
        });

        // Emit event for floating controls
        window.dispatchEvent(new CustomEvent('media-timeupdate', {
          detail: {
            currentTime: currentTime / 1000,
            duration: duration / 1000,
            isPlaying,
          }
        }));

        // Check if playback ended. playbin STAYS in the Playing state at the end
        // with the position frozen at the duration (it does NOT reset to 0 or pause),
        // so we detect end-of-stream two ways and fire onEnded once, on the rising edge:
        //   1. Primary: the backend's eos flag (GStreamer EOS bus message).
        //   2. Fallback: the position is parked at (or just shy of) the duration and
        //      has stopped advancing while still "playing" — covers cases where the
        //      EOS bus message never surfaces to our handler.
        const frozenAtEnd =
          isPlaying &&
          duration > 0 &&
          currentTime > 0 &&
          currentTime >= duration - 400 &&
          Math.abs(currentTime - lastPositionRef.current) < 50;
        const ended = status.eos === true || frozenAtEnd;
        if (ended && !eosFiredRef.current) {
          console.log('[GStreamer] End of stream detected', { playerId, eos: status.eos, frozenAtEnd, currentTime, duration });
          onEnded?.();
        }
        eosFiredRef.current = ended;
        lastPositionRef.current = currentTime;

      } catch (err) {
        console.warn('Failed to poll player status:', err);
      }
    };

    statusPollRef.current = setInterval(pollStatus, 250); // Poll every 250ms
    pollStatus(); // Initial poll

    return () => {
      if (statusPollRef.current) {
        clearInterval(statusPollRef.current);
      }
    };
  }, [playerId, state.gstreamerAvailable, onTimeUpdate, onEnded, onStatusChange, title]);

  // Note: Player cleanup is handled in the video surface cleanup effect above
  // to ensure proper ordering (stop player -> remove player -> destroy surface)

  // Toggle play/pause
  const togglePlay = useCallback(async () => {
    if (!state.gstreamerAvailable) return;

    try {
      if (state.isPlaying) {
        await invoke('player_pause', { playerId });
      } else {
        await invoke('player_play', { playerId });
      }
    } catch (err) {
      const errorMsg = `Playback error: ${err}`;
      setState(s => ({ ...s, error: errorMsg }));
      onError?.(errorMsg);
    }
  }, [playerId, state.isPlaying, state.gstreamerAvailable, onError]);

  // Handle click with double-click detection
  const handleVideoClick = useCallback(() => {
    if (clickTimeoutRef.current) {
      clearTimeout(clickTimeoutRef.current);
      clickTimeoutRef.current = null;
      window.dispatchEvent(new CustomEvent('media-toggle-fullscreen'));
    } else {
      clickTimeoutRef.current = setTimeout(() => {
        clickTimeoutRef.current = null;
        togglePlay();
      }, 200);
    }
  }, [togglePlay]);

  // Seek to position
  const seek = useCallback(async (timeMs: number) => {
    if (!state.gstreamerAvailable) return;

    try {
      await invoke('player_seek', { playerId, positionMs: Math.floor(timeMs) });
    } catch (err) {
      console.error('Seek failed:', err);
    }
  }, [playerId, state.gstreamerAvailable]);

  // Handle progress bar click - used by external controls when needed
  const _handleProgressClick = (e: React.MouseEvent<HTMLDivElement>) => {
    const progress = progressRef.current;
    if (!progress || state.duration === 0) return;

    const rect = progress.getBoundingClientRect();
    const pos = (e.clientX - rect.left) / rect.width;
    seek(pos * state.duration);
  };
  void _handleProgressClick; // Reserved for future use

  // Toggle mute
  const toggleMute = useCallback(async () => {
    if (!state.gstreamerAvailable) return;

    try {
      await invoke('player_set_muted', { playerId, muted: !state.isMuted });
      setState(s => ({ ...s, isMuted: !s.isMuted }));
    } catch (err) {
      console.error('Mute toggle failed:', err);
    }
  }, [playerId, state.isMuted, state.gstreamerAvailable]);

  // Set volume
  const setVolume = useCallback(async (vol: number) => {
    if (!state.gstreamerAvailable) return;

    const clampedVol = Math.max(0, Math.min(1, vol));
    try {
      await invoke('player_set_volume', { playerId, volume: clampedVol });
      setState(s => ({ ...s, volume: clampedVol, isMuted: clampedVol === 0 }));
    } catch (err) {
      console.error('Volume set failed:', err);
    }
  }, [playerId, state.gstreamerAvailable]);

  // Toggle fullscreen
  const toggleFullscreen = async () => {
    const container = containerRef.current;
    if (!container) return;

    try {
      if (!document.fullscreenElement) {
        await container.requestFullscreen();
        setState(s => ({ ...s, isFullscreen: true }));
      } else {
        await document.exitFullscreen();
        setState(s => ({ ...s, isFullscreen: false }));
      }
    } catch (err) {
      console.error('Fullscreen error:', err);
    }
  };

  // Skip forward/backward
  const skip = useCallback(async (seconds: number) => {
    if (!state.gstreamerAvailable) return;

    try {
      if (seconds > 0) {
        await invoke('player_skip_forward', { playerId, seconds: Math.abs(seconds) });
      } else {
        await invoke('player_skip_backward', { playerId, seconds: Math.abs(seconds) });
      }
    } catch (err) {
      console.error('Skip failed:', err);
    }
  }, [playerId, state.gstreamerAvailable]);

  // Listen for external control events (from floating controls and stacked controls)
  useEffect(() => {
    // Global events (from the toolbar / keyboard). Only the focused pane reacts,
    // so these don't fan out to every pane at once.
    const handlePlayPause = () => { if (isActiveRef.current) togglePlay(); };
    const handleStop = async () => {
      if (isActiveRef.current && state.gstreamerAvailable) {
        await invoke('player_stop', { playerId });
      }
    };
    const handleSeek = (e: CustomEvent) => { if (isActiveRef.current) seek(e.detail.time * 1000); };
    const handleVolume = (e: CustomEvent) => { if (isActiveRef.current) setVolume(e.detail.volume); };
    const handleMute = () => { if (isActiveRef.current) toggleMute(); };

    // Player-specific events (for stacked controls in multi-pane mode)
    const handlePlayerPlayPause = (e: CustomEvent) => {
      if (e.detail.playerId === playerId) togglePlay();
    };
    const handlePlayerSeek = (e: CustomEvent) => {
      if (e.detail.playerId === playerId) seek(e.detail.time * 1000);
    };
    const handlePlayerVolume = (e: CustomEvent) => {
      if (e.detail.playerId === playerId) setVolume(e.detail.volume);
    };
    const handlePlayerMute = (e: CustomEvent) => {
      if (e.detail.playerId === playerId) toggleMute();
    };

    window.addEventListener('media-playpause', handlePlayPause);
    window.addEventListener('media-stop', handleStop);
    window.addEventListener('media-seek' as any, handleSeek);
    window.addEventListener('media-volume' as any, handleVolume);
    window.addEventListener('media-mute', handleMute);

    // Player-specific listeners
    window.addEventListener('media-playpause-player' as any, handlePlayerPlayPause);
    window.addEventListener('media-seek-player' as any, handlePlayerSeek);
    window.addEventListener('media-volume-player' as any, handlePlayerVolume);
    window.addEventListener('media-mute-player' as any, handlePlayerMute);

    return () => {
      window.removeEventListener('media-playpause', handlePlayPause);
      window.removeEventListener('media-stop', handleStop);
      window.removeEventListener('media-seek' as any, handleSeek);
      window.removeEventListener('media-volume' as any, handleVolume);
      window.removeEventListener('media-mute', handleMute);

      window.removeEventListener('media-playpause-player' as any, handlePlayerPlayPause);
      window.removeEventListener('media-seek-player' as any, handlePlayerSeek);
      window.removeEventListener('media-volume-player' as any, handlePlayerVolume);
      window.removeEventListener('media-mute-player' as any, handlePlayerMute);
    };
  }, [togglePlay, seek, setVolume, toggleMute, playerId, state.gstreamerAvailable]);

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
      // Only the focused pane handles shortcuts — otherwise arrow-seek/space hit
      // every pane simultaneously.
      if (!isActiveRef.current) return;

      switch (e.key) {
        case ' ':
        case 'k':
          e.preventDefault();
          togglePlay();
          break;
        case 'ArrowLeft':
        case 'j':
          e.preventDefault();
          skip(-10);
          break;
        case 'ArrowRight':
        case 'l':
          e.preventDefault();
          skip(10);
          break;
        case 'ArrowUp':
          e.preventDefault();
          setVolume(state.volume + 0.1);
          break;
        case 'ArrowDown':
          e.preventDefault();
          setVolume(state.volume - 0.1);
          break;
        case 'm':
          e.preventDefault();
          toggleMute();
          break;
        case 'f':
          e.preventDefault();
          toggleFullscreen();
          break;
        case '0':
        case '1':
        case '2':
        case '3':
        case '4':
        case '5':
        case '6':
        case '7':
        case '8':
        case '9':
          e.preventDefault();
          const percent = parseInt(e.key) / 10;
          seek(percent * state.duration);
          break;
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [state.volume, state.duration, togglePlay, skip, setVolume, toggleMute, seek]);

  // Auto-hide controls (resets timeout on mouse move)
  const resetControlsTimeout = () => {
    if (controlsTimeoutRef.current) {
      clearTimeout(controlsTimeoutRef.current);
    }
  };

  useEffect(() => {
    return () => {
      if (controlsTimeoutRef.current) {
        clearTimeout(controlsTimeoutRef.current);
      }
      if (clickTimeoutRef.current) {
        clearTimeout(clickTimeoutRef.current);
      }
    };
  }, []);

  const _progress = state.duration > 0 ? (state.currentTime / state.duration) * 100 : 0;
  void _progress; // Reserved for future controls implementation

  // If GStreamer not available, show fallback message
  if (!state.gstreamerAvailable && isTauri()) {
    return (
      <div className={`relative bg-black rounded-lg overflow-hidden flex items-center justify-center ${className}`}>
        <div className="text-center p-6">
          <svg className="w-16 h-16 mx-auto text-yellow-500 mb-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
          </svg>
          <p className="text-white text-lg mb-2">GStreamer Not Available</p>
          <p className="text-gray-400 text-sm">Install GStreamer for native video playback</p>
        </div>
      </div>
    );
  }

  return (
    <div
      ref={containerRef}
      className={`relative bg-black rounded-lg overflow-hidden ${className}`}
      onMouseMove={resetControlsTimeout}
    >
      {/* Video Display Area - X11 child window renders here when embedded */}
      {/* Full height: controls are a separate floating X11 window that overlays the
          bottom of the video, so we don't reserve inline space (that left a black bar). */}
      <div
        ref={videoAreaRef}
        className={`w-full h-full flex items-center justify-center cursor-pointer ${
          state.embeddedMode
            ? 'bg-transparent' // Transparent so X11 child window shows through
            : 'bg-gradient-to-br from-gray-900 to-black'
        }`}
        onClick={handleVideoClick}
      >
        {/* Show placeholder when not embedded or when loading/paused */}
        {!state.isLoading && !state.error && !state.embeddedMode && (
          <div className="text-center select-none">
            {/* Status indicator */}
            <div className="mb-4 flex items-center justify-center gap-3">
              <div className={`w-3 h-3 rounded-full ${state.isPlaying ? 'bg-green-500 animate-pulse' : 'bg-gray-500'}`} />
              <span className="text-white/80 text-sm font-medium">
                {state.isPlaying ? 'Playing' : state.duration > 0 ? 'Paused' : 'Ready'}
              </span>
            </div>

            {/* Title */}
            <p className="text-white/90 text-lg font-medium mb-2 max-w-md truncate px-4">
              {title || 'Media'}
            </p>

            {/* Time indicator */}
            {state.duration > 0 && (
              <p className="text-white/50 text-sm font-mono">
                {formatTime(state.currentTime)} / {formatTime(state.duration)}
              </p>
            )}

            {/* Info text - shows mode status */}
            <p className="text-white/30 text-xs mt-4">
              {state.embeddedError || 'Video renders in native window'} • Controls below
            </p>
          </div>
        )}

        {/* Embedded mode: minimal overlay when paused (video renders behind) */}
        {state.embeddedMode && !state.isPlaying && !state.isLoading && !state.error && (
          <div className="absolute inset-0 flex items-center justify-center pointer-events-none">
            <p className="text-white/50 text-sm select-none">
              {title || 'Click to play'}
            </p>
          </div>
        )}

        {/* Loading Spinner - inside video area */}
        {state.isLoading && (
          <div className="absolute inset-0 flex items-center justify-center bg-black/50">
            <div className="w-12 h-12 border-4 border-[var(--primary-color)] border-t-transparent rounded-full animate-spin" />
          </div>
        )}

        {/* Error Display - inside video area */}
        {state.error && (
          <div className="absolute inset-0 flex items-center justify-center bg-black/80">
            <div className="text-center p-6">
              <svg className="w-16 h-16 mx-auto text-red-500 mb-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
              </svg>
              <p className="text-white text-lg">{state.error}</p>
            </div>
          </div>
        )}

        {/* Play Button Overlay (when paused) - inside video area */}
        {!state.isPlaying && !state.isLoading && !state.error && !state.embeddedMode && (
          <div className="absolute inset-0 flex items-center justify-center">
            <div className="w-20 h-20 bg-white/20 rounded-full flex items-center justify-center backdrop-blur-sm hover:bg-white/30 transition-colors">
              <svg className="w-10 h-10 text-white ml-1" fill="currentColor" viewBox="0 0 24 24">
                <path d="M8 5v14l11-7z" />
              </svg>
            </div>
          </div>
        )}
      </div>

      {/* Native GTK Controls Overlay - rendered as separate X11 window above video */}
      {/* Controls are created/managed via Tauri invoke calls in useEffect hooks above */}
      {/* The GTK window contains actual GTK buttons that render above the X11 video surface */}
    </div>
  );
}

export default GStreamerVideoPlayer;
