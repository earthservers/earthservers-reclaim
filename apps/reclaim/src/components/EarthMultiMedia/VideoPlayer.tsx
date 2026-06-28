// EarthMultiMedia VideoPlayer Component
// Privacy-focused video player with support for local files and URLs

import { useState, useRef, useEffect, useCallback } from 'react';

interface VideoPlayerProps {
  source: string;
  title?: string;
  autoPlay?: boolean;
  onTimeUpdate?: (currentTime: number, duration: number) => void;
  onEnded?: () => void;
  onError?: (error: string) => void;
  className?: string;
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
}

export function VideoPlayer({
  source,
  title,
  autoPlay = false,
  onTimeUpdate,
  onEnded,
  onError,
  className = '',
}: VideoPlayerProps) {
  const videoRef = useRef<HTMLVideoElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const progressRef = useRef<HTMLDivElement>(null);
  const [showControls, setShowControls] = useState(true);
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
  });

  const controlsTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const clickTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Format time as HH:MM:SS or MM:SS
  const formatTime = (seconds: number): string => {
    if (isNaN(seconds)) return '0:00';
    const hrs = Math.floor(seconds / 3600);
    const mins = Math.floor((seconds % 3600) / 60);
    const secs = Math.floor(seconds % 60);
    if (hrs > 0) {
      return `${hrs}:${mins.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}`;
    }
    return `${mins}:${secs.toString().padStart(2, '0')}`;
  };

  // Toggle play/pause
  const togglePlay = useCallback(() => {
    const video = videoRef.current;
    if (!video) return;

    if (video.paused) {
      video.play().catch(() => {
        setState(s => ({ ...s, error: 'Failed to play video' }));
        onError?.('Failed to play video');
      });
    } else {
      video.pause();
    }
  }, [onError]);

  // Handle click with double-click detection
  const handleVideoClick = useCallback(() => {
    if (clickTimeoutRef.current) {
      // Double click detected - clear single click and toggle fullscreen
      clearTimeout(clickTimeoutRef.current);
      clickTimeoutRef.current = null;
      window.dispatchEvent(new CustomEvent('media-toggle-fullscreen'));
    } else {
      // Single click - wait to see if it's a double click
      clickTimeoutRef.current = setTimeout(() => {
        clickTimeoutRef.current = null;
        togglePlay();
      }, 200);
    }
  }, [togglePlay]);

  // Seek to position
  const seek = useCallback((time: number) => {
    const video = videoRef.current;
    if (!video) return;
    video.currentTime = Math.max(0, Math.min(time, video.duration || 0));
  }, []);

  // Handle progress bar click
  const handleProgressClick = (e: React.MouseEvent<HTMLDivElement>) => {
    const progress = progressRef.current;
    const video = videoRef.current;
    if (!progress || !video) return;

    const rect = progress.getBoundingClientRect();
    const pos = (e.clientX - rect.left) / rect.width;
    seek(pos * video.duration);
  };

  // Toggle mute
  const toggleMute = () => {
    const video = videoRef.current;
    if (!video) return;
    video.muted = !video.muted;
    setState(s => ({ ...s, isMuted: video.muted }));
  };

  // Set volume
  const setVolume = (vol: number) => {
    const video = videoRef.current;
    if (!video) return;
    video.volume = Math.max(0, Math.min(1, vol));
    setState(s => ({ ...s, volume: video.volume, isMuted: video.volume === 0 }));
  };

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

  // Set playback rate
  const setPlaybackRate = (rate: number) => {
    const video = videoRef.current;
    if (!video) return;
    video.playbackRate = rate;
    setState(s => ({ ...s, playbackRate: rate }));
  };

  // Skip forward/backward
  const skip = (seconds: number) => {
    const video = videoRef.current;
    if (!video) return;
    seek(video.currentTime + seconds);
  };

  // Video event handlers
  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;

    const handlePlay = () => setState(s => ({ ...s, isPlaying: true }));
    const handlePause = () => setState(s => ({ ...s, isPlaying: false }));
    const handleTimeUpdate = () => {
      setState(s => ({ ...s, currentTime: video.currentTime }));
      onTimeUpdate?.(video.currentTime, video.duration);
      // Emit event for floating controls
      window.dispatchEvent(new CustomEvent('media-timeupdate', {
        detail: {
          currentTime: video.currentTime,
          duration: video.duration,
          isPlaying: !video.paused,
        }
      }));
    };
    const handleDurationChange = () => setState(s => ({ ...s, duration: video.duration }));
    const handleLoadedData = () => setState(s => ({ ...s, isLoading: false }));
    const handleWaiting = () => setState(s => ({ ...s, isLoading: true }));
    const handleCanPlay = () => setState(s => ({ ...s, isLoading: false }));
    const handleEnded = () => {
      setState(s => ({ ...s, isPlaying: false }));
      onEnded?.();
    };
    const handleError = () => {
      const error = 'Failed to load video';
      setState(s => ({ ...s, error, isLoading: false }));
      onError?.(error);
    };

    video.addEventListener('play', handlePlay);
    video.addEventListener('pause', handlePause);
    video.addEventListener('timeupdate', handleTimeUpdate);
    video.addEventListener('durationchange', handleDurationChange);
    video.addEventListener('loadeddata', handleLoadedData);
    video.addEventListener('waiting', handleWaiting);
    video.addEventListener('canplay', handleCanPlay);
    video.addEventListener('ended', handleEnded);
    video.addEventListener('error', handleError);

    return () => {
      video.removeEventListener('play', handlePlay);
      video.removeEventListener('pause', handlePause);
      video.removeEventListener('timeupdate', handleTimeUpdate);
      video.removeEventListener('durationchange', handleDurationChange);
      video.removeEventListener('loadeddata', handleLoadedData);
      video.removeEventListener('waiting', handleWaiting);
      video.removeEventListener('canplay', handleCanPlay);
      video.removeEventListener('ended', handleEnded);
      video.removeEventListener('error', handleError);
    };
  }, [onTimeUpdate, onEnded, onError]);

  // Listen for external control events (from floating controls)
  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;

    const handlePlayPause = () => {
      if (video.paused) {
        video.play().catch(() => {});
      } else {
        video.pause();
      }
    };

    const handleStop = () => {
      video.pause();
      video.currentTime = 0;
    };

    const handleSeek = (e: CustomEvent) => {
      video.currentTime = e.detail.time;
    };

    const handleVolume = (e: CustomEvent) => {
      video.volume = e.detail.volume;
      setState(s => ({ ...s, volume: e.detail.volume }));
    };

    const handleMute = () => {
      video.muted = !video.muted;
      setState(s => ({ ...s, isMuted: video.muted }));
    };

    window.addEventListener('media-playpause', handlePlayPause);
    window.addEventListener('media-stop', handleStop);
    window.addEventListener('media-seek' as any, handleSeek);
    window.addEventListener('media-volume' as any, handleVolume);
    window.addEventListener('media-mute', handleMute);

    return () => {
      window.removeEventListener('media-playpause', handlePlayPause);
      window.removeEventListener('media-stop', handleStop);
      window.removeEventListener('media-seek' as any, handleSeek);
      window.removeEventListener('media-volume' as any, handleVolume);
      window.removeEventListener('media-mute', handleMute);
    };
  }, []);

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;

      switch (e.key) {
        case ' ':
        case 'k':
          e.preventDefault();
          togglePlay();
          break;
        case 'ArrowLeft':
          e.preventDefault();
          skip(-10);
          break;
        case 'ArrowRight':
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
        case 'j':
          e.preventDefault();
          skip(-10);
          break;
        case 'l':
          e.preventDefault();
          skip(10);
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
  }, [state.volume, state.duration, togglePlay]);

  // Auto-hide controls
  const resetControlsTimeout = () => {
    setShowControls(true);
    if (controlsTimeoutRef.current) {
      clearTimeout(controlsTimeoutRef.current);
    }
    if (state.isPlaying) {
      controlsTimeoutRef.current = setTimeout(() => {
        setShowControls(false);
      }, 3000);
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

  // Load source
  useEffect(() => {
    setState(s => ({ ...s, isLoading: true, error: null }));
  }, [source]);

  const progress = state.duration > 0 ? (state.currentTime / state.duration) * 100 : 0;

  return (
    <div
      ref={containerRef}
      className={`relative bg-black rounded-lg overflow-hidden ${className}`}
      onMouseMove={resetControlsTimeout}
      onMouseLeave={() => state.isPlaying && setShowControls(false)}
    >
      {/* Video Element - explicitly disable native controls */}
      <video
        ref={videoRef}
        src={source}
        className="w-full h-full object-contain"
        autoPlay={autoPlay}
        onClick={handleVideoClick}
        playsInline
        controls={false}
        controlsList="nodownload nofullscreen noremoteplayback"
        disablePictureInPicture
        style={{
          // Force hide any native controls that WebKitGTK might try to show
          WebkitAppearance: 'none',
        }}
      />
      {/* CSS to hide ALL native video controls - WebKit, Chromium, Firefox, etc */}
      <style>{`
        /* Hide all webkit/blink media controls */
        video::-webkit-media-controls,
        video::-webkit-media-controls-enclosure,
        video::-webkit-media-controls-panel,
        video::-webkit-media-controls-play-button,
        video::-webkit-media-controls-timeline,
        video::-webkit-media-controls-current-time-display,
        video::-webkit-media-controls-time-remaining-display,
        video::-webkit-media-controls-mute-button,
        video::-webkit-media-controls-volume-slider,
        video::-webkit-media-controls-fullscreen-button,
        video::-webkit-media-controls-overlay-play-button,
        video::-webkit-media-controls-start-playback-button,
        video::-webkit-media-controls-toggle-closed-captions-button,
        video::-webkit-media-controls-seek-back-button,
        video::-webkit-media-controls-seek-forward-button,
        video::-webkit-media-controls-rewind-button,
        video::-webkit-media-controls-return-to-realtime-button,
        video::-webkit-media-controls-volume-slider-container,
        video::-internal-media-controls-overlay-cast-button {
          display: none !important;
          opacity: 0 !important;
          pointer-events: none !important;
          visibility: hidden !important;
          width: 0 !important;
          height: 0 !important;
        }
        /* Firefox specific */
        video::-moz-media-controls {
          display: none !important;
        }
        /* Prevent any default video styling */
        video {
          -webkit-appearance: none !important;
          -moz-appearance: none !important;
          appearance: none !important;
        }
      `}</style>

      {/* Loading Spinner */}
      {state.isLoading && (
        <div className="absolute inset-0 flex items-center justify-center bg-black/50">
          <div className="w-12 h-12 border-4 border-[var(--primary-color)] border-t-transparent rounded-full animate-spin" />
        </div>
      )}

      {/* Error Display */}
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

      {/* Play Button Overlay (when paused) */}
      {!state.isPlaying && !state.isLoading && !state.error && (
        <div
          className="absolute inset-0 flex items-center justify-center cursor-pointer"
          onClick={handleVideoClick}
        >
          <div className="w-20 h-20 bg-white/20 rounded-full flex items-center justify-center backdrop-blur-sm hover:bg-white/30 transition-colors">
            <svg className="w-10 h-10 text-white ml-1" fill="currentColor" viewBox="0 0 24 24">
              <path d="M8 5v14l11-7z" />
            </svg>
          </div>
        </div>
      )}

      {/* Controls Overlay */}
      <div
        className={`absolute bottom-0 left-0 right-0 bg-gradient-to-t from-black/80 to-transparent transition-opacity duration-300 ${
          showControls ? 'opacity-100' : 'opacity-0 pointer-events-none'
        }`}
      >
        {/* Title */}
        {title && (
          <div className="px-4 py-2 text-white text-sm font-medium truncate">
            {title}
          </div>
        )}

        {/* Progress Bar */}
        <div
          ref={progressRef}
          className="h-1 mx-4 bg-white/30 rounded-full cursor-pointer group"
          onClick={handleProgressClick}
        >
          <div
            className="h-full bg-[var(--primary-color)] rounded-full relative"
            style={{ width: `${progress}%` }}
          >
            <div className="absolute right-0 top-1/2 -translate-y-1/2 w-3 h-3 bg-white rounded-full opacity-0 group-hover:opacity-100 transition-opacity" />
          </div>
        </div>

        {/* Controls Row */}
        <div className="flex items-center gap-2 px-4 py-3">
          {/* Play/Pause */}
          <button
            onClick={togglePlay}
            className="p-2 text-white hover:text-[var(--primary-color)] transition-colors"
          >
            {state.isPlaying ? (
              <svg className="w-6 h-6" fill="currentColor" viewBox="0 0 24 24">
                <path d="M6 19h4V5H6v14zm8-14v14h4V5h-4z" />
              </svg>
            ) : (
              <svg className="w-6 h-6" fill="currentColor" viewBox="0 0 24 24">
                <path d="M8 5v14l11-7z" />
              </svg>
            )}
          </button>

          {/* Skip Backward */}
          <button
            onClick={() => skip(-10)}
            className="p-2 text-white hover:text-[var(--primary-color)] transition-colors"
            title="Skip back 10s (J)"
          >
            <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12.066 11.2a1 1 0 000 1.6l5.334 4A1 1 0 0019 16V8a1 1 0 00-1.6-.8l-5.333 4zM4.066 11.2a1 1 0 000 1.6l5.334 4A1 1 0 0011 16V8a1 1 0 00-1.6-.8l-5.334 4z" />
            </svg>
          </button>

          {/* Skip Forward */}
          <button
            onClick={() => skip(10)}
            className="p-2 text-white hover:text-[var(--primary-color)] transition-colors"
            title="Skip forward 10s (L)"
          >
            <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M11.933 12.8a1 1 0 000-1.6L6.6 7.2A1 1 0 005 8v8a1 1 0 001.6.8l5.333-4zM19.933 12.8a1 1 0 000-1.6l-5.333-4A1 1 0 0013 8v8a1 1 0 001.6.8l5.333-4z" />
            </svg>
          </button>

          {/* Time Display */}
          <span className="text-white text-sm font-mono">
            {formatTime(state.currentTime)} / {formatTime(state.duration)}
          </span>

          {/* Spacer */}
          <div className="flex-1" />

          {/* Playback Rate */}
          <select
            value={state.playbackRate}
            onChange={(e) => setPlaybackRate(parseFloat(e.target.value))}
            className="bg-transparent text-white text-sm border border-white/30 rounded px-2 py-1 cursor-pointer"
          >
            <option value="0.25">0.25x</option>
            <option value="0.5">0.5x</option>
            <option value="0.75">0.75x</option>
            <option value="1">1x</option>
            <option value="1.25">1.25x</option>
            <option value="1.5">1.5x</option>
            <option value="1.75">1.75x</option>
            <option value="2">2x</option>
          </select>

          {/* Volume */}
          <div className="flex items-center gap-1 group">
            <button
              onClick={toggleMute}
              className="p-2 text-white hover:text-[var(--primary-color)] transition-colors"
            >
              {state.isMuted || state.volume === 0 ? (
                <svg className="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
                  <path d="M16.5 12c0-1.77-1.02-3.29-2.5-4.03v2.21l2.45 2.45c.03-.2.05-.41.05-.63zm2.5 0c0 .94-.2 1.82-.54 2.64l1.51 1.51C20.63 14.91 21 13.5 21 12c0-4.28-2.99-7.86-7-8.77v2.06c2.89.86 5 3.54 5 6.71zM4.27 3L3 4.27 7.73 9H3v6h4l5 5v-6.73l4.25 4.25c-.67.52-1.42.93-2.25 1.18v2.06c1.38-.31 2.63-.95 3.69-1.81L19.73 21 21 19.73l-9-9L4.27 3zM12 4L9.91 6.09 12 8.18V4z" />
                </svg>
              ) : state.volume < 0.5 ? (
                <svg className="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
                  <path d="M18.5 12c0-1.77-1.02-3.29-2.5-4.03v8.05c1.48-.73 2.5-2.25 2.5-4.02zM5 9v6h4l5 5V4L9 9H5z" />
                </svg>
              ) : (
                <svg className="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
                  <path d="M3 9v6h4l5 5V4L7 9H3zm13.5 3c0-1.77-1.02-3.29-2.5-4.03v8.05c1.48-.73 2.5-2.25 2.5-4.02zM14 3.23v2.06c2.89.86 5 3.54 5 6.71s-2.11 5.85-5 6.71v2.06c4.01-.91 7-4.49 7-8.77s-2.99-7.86-7-8.77z" />
                </svg>
              )}
            </button>
            <input
              type="range"
              min="0"
              max="1"
              step="0.05"
              value={state.volume}
              onChange={(e) => setVolume(parseFloat(e.target.value))}
              className="w-0 group-hover:w-20 transition-all duration-200 accent-[var(--primary-color)]"
            />
          </div>

          {/* Fullscreen */}
          <button
            onClick={toggleFullscreen}
            className="p-2 text-white hover:text-[var(--primary-color)] transition-colors"
            title="Fullscreen (F)"
          >
            {state.isFullscreen ? (
              <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
              </svg>
            ) : (
              <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 8V4m0 0h4M4 4l5 5m11-1V4m0 0h-4m4 0l-5 5M4 16v4m0 0h4m-4 0l5-5m11 5l-5-5m5 5v-4m0 4h-4" />
              </svg>
            )}
          </button>
        </div>
      </div>
    </div>
  );
}

export default VideoPlayer;
