// EarthMultiMedia GStreamer Player Component
// Hardware-accelerated video player using GStreamer backend with YouTube support
// Note: Video renders in a separate native window; this component provides controls

import { useState, useEffect, useRef, useCallback } from 'react';
import { invoke } from '../../lib/tauri';

interface PlayerStatus {
  state: 'Stopped' | 'Playing' | 'Paused' | 'Buffering' | { Error: string };
  position_ms: number;
  duration_ms: number;
  volume: number;
  muted: boolean;
  info: MediaInfo;
}

interface MediaInfo {
  uri: string | null;
  title: string | null;
  artist: string | null;
  album: string | null;
  duration_ms: number | null;
  width: number | null;
  height: number | null;
  is_video: boolean;
  is_live: boolean;
  youtube_info: VideoInfo | null;
}

interface VideoInfo {
  title: string;
  duration: number;
  thumbnail: string;
  uploader: string;
  description: string | null;
  view_count: number | null;
}

interface GStreamerPlayerProps {
  onClose?: () => void;
  className?: string;
}

export function GStreamerPlayer({ onClose, className = '' }: GStreamerPlayerProps) {
  const [url, setUrl] = useState('');
  const [status, setStatus] = useState<PlayerStatus | null>(null);
  const [videoInfo, setVideoInfo] = useState<VideoInfo | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [youtubeAvailable, setYoutubeAvailable] = useState(false);
  const [gstreamerVersion, setGstreamerVersion] = useState<string | null>(null);
  const statusInterval = useRef<ReturnType<typeof setInterval> | null>(null);

  // Format time from milliseconds
  const formatTime = (ms: number | null): string => {
    if (ms === null || isNaN(ms)) return '--:--';
    const seconds = Math.floor(ms / 1000);
    const mins = Math.floor(seconds / 60);
    const secs = seconds % 60;
    const hrs = Math.floor(mins / 60);
    if (hrs > 0) {
      return `${hrs}:${(mins % 60).toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}`;
    }
    return `${mins}:${secs.toString().padStart(2, '0')}`;
  };

  // Check if URL is YouTube
  const isYouTubeUrl = useCallback((url: string): boolean => {
    return url.includes('youtube.com') || url.includes('youtu.be');
  }, []);

  // Initialize on mount
  useEffect(() => {
    const init = async () => {
      try {
        // Check GStreamer availability
        const version = await invoke<string>('media_check_gstreamer');
        setGstreamerVersion(version);
      } catch (err) {
        setError(`GStreamer not available: ${err}`);
      }

      try {
        // Check YouTube availability
        const available = await invoke<boolean>('check_youtube_available');
        setYoutubeAvailable(available);
      } catch (err) {
        console.error('Failed to check YouTube availability:', err);
      }
    };

    init();

    // Start status polling
    statusInterval.current = setInterval(async () => {
      try {
        const newStatus = await invoke<PlayerStatus>('media_get_status');
        setStatus(newStatus);

        // Check for error state
        if (typeof newStatus.state === 'object' && 'Error' in newStatus.state) {
          setError(newStatus.state.Error);
        }
      } catch (err) {
        // Don't spam errors during polling
      }
    }, 500);

    return () => {
      if (statusInterval.current) {
        clearInterval(statusInterval.current);
      }
      // Stop playback on unmount
      invoke('media_stop').catch(() => {});
    };
  }, []);

  // Handle play button
  const handlePlay = async () => {
    if (!url) return;

    setLoading(true);
    setError(null);
    setVideoInfo(null);

    try {
      if (isYouTubeUrl(url)) {
        if (!youtubeAvailable) {
          setError('yt-dlp is not installed. Install with: sudo dnf install yt-dlp');
          setLoading(false);
          return;
        }

        // Play YouTube video
        const info = await invoke<VideoInfo>('play_youtube', { url });
        setVideoInfo(info);
      } else {
        // Regular playback
        await invoke('media_load', { uri: url });
        await invoke('media_play');
      }
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  };

  // Playback controls
  const handlePlayPause = async () => {
    if (!status) return;
    try {
      if (status.state === 'Playing') {
        await invoke('media_pause');
      } else {
        await invoke('media_play');
      }
    } catch (err) {
      setError(String(err));
    }
  };

  const handleStop = async () => {
    try {
      await invoke('media_stop');
      setVideoInfo(null);
      setError(null);
    } catch (err) {
      setError(String(err));
    }
  };

  const handleSeek = async (positionMs: number) => {
    try {
      await invoke('media_seek', { positionMs });
    } catch (err) {
      setError(String(err));
    }
  };

  const handleVolumeChange = async (volume: number) => {
    try {
      await invoke('media_set_volume', { volume });
    } catch (err) {
      setError(String(err));
    }
  };

  const handleMuteToggle = async () => {
    if (!status) return;
    try {
      await invoke('media_set_muted', { muted: !status.muted });
    } catch (err) {
      setError(String(err));
    }
  };

  const handleSkip = async (seconds: number) => {
    try {
      if (seconds > 0) {
        await invoke('media_skip_forward', { seconds });
      } else {
        await invoke('media_skip_backward', { seconds: Math.abs(seconds) });
      }
    } catch (err) {
      setError(String(err));
    }
  };

  const getStateLabel = (): string => {
    if (!status) return 'Initializing...';
    if (typeof status.state === 'object' && 'Error' in status.state) {
      return 'Error';
    }
    return status.state;
  };

  const isPlaying = status?.state === 'Playing';
  const progress = status && status.duration_ms > 0
    ? (status.position_ms / status.duration_ms) * 100
    : 0;

  return (
    <div className={`bg-[var(--card-bg-color)] rounded-lg overflow-hidden ${className}`}>
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-white/10">
        <div className="flex items-center gap-2">
          <svg className="w-5 h-5 text-[var(--primary-color)]" fill="currentColor" viewBox="0 0 24 24">
            <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm-2 14.5v-9l6 4.5-6 4.5z" />
          </svg>
          <h2 className="text-white font-medium">Earth Media Player</h2>
          {gstreamerVersion && (
            <span className="text-xs text-white/50">({gstreamerVersion})</span>
          )}
        </div>
        {onClose && (
          <button
            onClick={onClose}
            className="p-1 text-white/50 hover:text-white transition-colors"
          >
            <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        )}
      </div>

      <div className="p-4 space-y-4">
        {/* URL Input */}
        <div className="flex gap-2">
          <input
            type="text"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && handlePlay()}
            placeholder="Enter video URL, file path, or YouTube link..."
            className="flex-1 px-3 py-2 bg-black/30 border border-white/20 rounded text-white placeholder-white/40 focus:outline-none focus:border-[var(--primary-color)]"
          />
          <button
            onClick={handlePlay}
            disabled={loading || !url}
            className="px-4 py-2 bg-[var(--primary-color)] text-white rounded font-medium hover:opacity-90 disabled:opacity-50 disabled:cursor-not-allowed transition-opacity"
          >
            {loading ? (
              <span className="flex items-center gap-2">
                <svg className="w-4 h-4 animate-spin" fill="none" viewBox="0 0 24 24">
                  <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                  <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
                </svg>
                Loading
              </span>
            ) : (
              'Play'
            )}
          </button>
        </div>

        {/* YouTube Video Info */}
        {videoInfo && (
          <div className="flex gap-3 p-3 bg-black/20 rounded">
            <img
              src={videoInfo.thumbnail}
              alt={videoInfo.title}
              className="w-32 h-20 object-cover rounded"
            />
            <div className="flex-1 min-w-0">
              <h3 className="text-white font-medium truncate">{videoInfo.title}</h3>
              <p className="text-white/60 text-sm">{videoInfo.uploader}</p>
              {videoInfo.view_count && (
                <p className="text-white/40 text-xs mt-1">
                  {videoInfo.view_count.toLocaleString()} views
                </p>
              )}
            </div>
          </div>
        )}

        {/* Error Display */}
        {error && (
          <div className="flex items-center gap-2 p-3 bg-red-500/20 border border-red-500/30 rounded text-red-400">
            <svg className="w-5 h-5 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
            </svg>
            <span className="text-sm">{error}</span>
          </div>
        )}

        {/* Progress Bar */}
        {status && status.duration_ms > 0 && (
          <div className="space-y-1">
            <input
              type="range"
              min={0}
              max={status.duration_ms}
              value={status.position_ms}
              onChange={(e) => handleSeek(parseInt(e.target.value))}
              className="w-full h-1 bg-white/20 rounded-full appearance-none cursor-pointer accent-[var(--primary-color)]"
              style={{
                background: `linear-gradient(to right, var(--primary-color) ${progress}%, rgba(255,255,255,0.2) ${progress}%)`
              }}
            />
            <div className="flex justify-between text-xs text-white/50 font-mono">
              <span>{formatTime(status.position_ms)}</span>
              <span>{formatTime(status.duration_ms)}</span>
            </div>
          </div>
        )}

        {/* Playback Controls */}
        <div className="flex items-center justify-center gap-2">
          <button
            onClick={() => handleSkip(-10)}
            className="p-2 text-white/70 hover:text-white transition-colors"
            title="Skip back 10s"
          >
            <svg className="w-6 h-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12.066 11.2a1 1 0 000 1.6l5.334 4A1 1 0 0019 16V8a1 1 0 00-1.6-.8l-5.333 4zM4.066 11.2a1 1 0 000 1.6l5.334 4A1 1 0 0011 16V8a1 1 0 00-1.6-.8l-5.334 4z" />
            </svg>
          </button>

          <button
            onClick={handlePlayPause}
            className="p-3 bg-[var(--primary-color)] text-white rounded-full hover:opacity-90 transition-opacity"
          >
            {isPlaying ? (
              <svg className="w-6 h-6" fill="currentColor" viewBox="0 0 24 24">
                <path d="M6 19h4V5H6v14zm8-14v14h4V5h-4z" />
              </svg>
            ) : (
              <svg className="w-6 h-6" fill="currentColor" viewBox="0 0 24 24">
                <path d="M8 5v14l11-7z" />
              </svg>
            )}
          </button>

          <button
            onClick={handleStop}
            className="p-2 text-white/70 hover:text-white transition-colors"
            title="Stop"
          >
            <svg className="w-6 h-6" fill="currentColor" viewBox="0 0 24 24">
              <path d="M6 6h12v12H6z" />
            </svg>
          </button>

          <button
            onClick={() => handleSkip(10)}
            className="p-2 text-white/70 hover:text-white transition-colors"
            title="Skip forward 10s"
          >
            <svg className="w-6 h-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M11.933 12.8a1 1 0 000-1.6L6.6 7.2A1 1 0 005 8v8a1 1 0 001.6.8l5.333-4zM19.933 12.8a1 1 0 000-1.6l-5.333-4A1 1 0 0013 8v8a1 1 0 001.6.8l5.333-4z" />
            </svg>
          </button>
        </div>

        {/* Volume Control */}
        <div className="flex items-center gap-3 px-4">
          <button
            onClick={handleMuteToggle}
            className="p-1 text-white/70 hover:text-white transition-colors"
          >
            {status?.muted || (status?.volume === 0) ? (
              <svg className="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
                <path d="M16.5 12c0-1.77-1.02-3.29-2.5-4.03v2.21l2.45 2.45c.03-.2.05-.41.05-.63zm2.5 0c0 .94-.2 1.82-.54 2.64l1.51 1.51C20.63 14.91 21 13.5 21 12c0-4.28-2.99-7.86-7-8.77v2.06c2.89.86 5 3.54 5 6.71zM4.27 3L3 4.27 7.73 9H3v6h4l5 5v-6.73l4.25 4.25c-.67.52-1.42.93-2.25 1.18v2.06c1.38-.31 2.63-.95 3.69-1.81L19.73 21 21 19.73l-9-9L4.27 3zM12 4L9.91 6.09 12 8.18V4z" />
              </svg>
            ) : (
              <svg className="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
                <path d="M3 9v6h4l5 5V4L7 9H3zm13.5 3c0-1.77-1.02-3.29-2.5-4.03v8.05c1.48-.73 2.5-2.25 2.5-4.02zM14 3.23v2.06c2.89.86 5 3.54 5 6.71s-2.11 5.85-5 6.71v2.06c4.01-.91 7-4.49 7-8.77s-2.99-7.86-7-8.77z" />
              </svg>
            )}
          </button>
          <input
            type="range"
            min={0}
            max={1}
            step={0.05}
            value={status?.volume ?? 1}
            onChange={(e) => handleVolumeChange(parseFloat(e.target.value))}
            className="flex-1 h-1 bg-white/20 rounded-full appearance-none cursor-pointer accent-[var(--primary-color)]"
          />
          <span className="text-white/50 text-xs w-10 text-right">
            {Math.round((status?.volume ?? 1) * 100)}%
          </span>
        </div>

        {/* Status Info */}
        <div className="text-center space-y-1">
          <p className="text-white/60 text-sm">
            Status: <span className={isPlaying ? 'text-green-400' : 'text-white/80'}>{getStateLabel()}</span>
          </p>
          {status?.info?.uri && (
            <p className="text-white/40 text-xs truncate">
              {status.info.title || status.info.uri.split('/').pop()}
            </p>
          )}
        </div>

        {/* Feature Status */}
        <div className="flex items-center justify-center gap-4 text-xs text-white/40">
          <span className="flex items-center gap-1">
            <span className={gstreamerVersion ? 'text-green-400' : 'text-red-400'}>●</span>
            GStreamer
          </span>
          <span className="flex items-center gap-1">
            <span className={youtubeAvailable ? 'text-green-400' : 'text-yellow-400'}>●</span>
            YouTube {youtubeAvailable ? '' : '(install yt-dlp)'}
          </span>
        </div>

        {/* Example URLs */}
        <details className="text-sm">
          <summary className="text-white/50 cursor-pointer hover:text-white/70">
            Example URLs
          </summary>
          <div className="mt-2 space-y-1">
            <button
              onClick={() => setUrl('/home/tommy/Videos/sample.mp4')}
              className="block w-full text-left px-2 py-1 text-white/60 hover:text-white hover:bg-white/5 rounded"
            >
              📁 Local file: /home/tommy/Videos/sample.mp4
            </button>
            <button
              onClick={() => setUrl('https://www.youtube.com/watch?v=dQw4w9WgXcQ')}
              className="block w-full text-left px-2 py-1 text-white/60 hover:text-white hover:bg-white/5 rounded"
            >
              🎬 YouTube: youtube.com/watch?v=...
            </button>
            <button
              onClick={() => setUrl('https://sample-videos.com/video321/mp4/720/big_buck_bunny_720p_1mb.mp4')}
              className="block w-full text-left px-2 py-1 text-white/60 hover:text-white hover:bg-white/5 rounded"
            >
              🌐 Remote: Big Buck Bunny (test video)
            </button>
          </div>
        </details>

        {/* Note about video window */}
        <p className="text-center text-white/30 text-xs">
          Video renders in a separate native window with hardware acceleration
        </p>
      </div>
    </div>
  );
}

export default GStreamerPlayer;
