// MediaControls - Floating media controls component
// Rendered in a separate X11/WebKitGTK window that floats above the video
// Communicates with the main Tauri app via WebSocket for real-time updates

import { useState, useEffect, useRef, useCallback } from 'react';

interface MediaState {
  playerId: string;
  isPlaying: boolean;
  currentTime: number;  // milliseconds
  duration: number;     // milliseconds
  volume: number;       // 0.0 - 1.0
  isMuted: boolean;
  title: string;
  isShuffled: boolean;
  repeatMode: 'none' | 'all' | 'one';
  isFullscreen: boolean;
}

const WS_URL = 'ws://127.0.0.1:9876';

// This controls window belongs to one app window, passed as ?win=<label>. The
// single shared WebSocket server broadcasts every window's status, so we filter to
// our own (by the player-id prefix) and tag every command we send with our label.
function readWindowLabel(): string {
  try {
    return new URLSearchParams(window.location.search).get('win') || 'main';
  } catch {
    return 'main';
  }
}

export function MediaControls() {
  const winLabelRef = useRef<string>(readWindowLabel());
  const [state, setState] = useState<MediaState>({
    playerId: '',
    isPlaying: false,
    currentTime: 0,
    duration: 0,
    volume: 1,
    isMuted: false,
    title: '',
    isShuffled: false,
    repeatMode: 'none',
    isFullscreen: false,
  });
  const [isConnected, setIsConnected] = useState(false);
  const [isCollapsed, setIsCollapsed] = useState(false);
  const [isDragging, setIsDragging] = useState(false);
  const [dragStart, setDragStart] = useState({ x: 0, y: 0 });
  const wsRef = useRef<WebSocket | null>(null);
  const reconnectTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const progressRef = useRef<HTMLDivElement>(null);

  // WebSocket connection and message handling
  useEffect(() => {
    let mounted = true;

    const connect = () => {
      if (!mounted) return;

      try {
        const ws = new WebSocket(WS_URL);
        wsRef.current = ws;

        ws.onopen = () => {
          console.log('[MediaControls] WebSocket connected');
          if (mounted) {
            setIsConnected(true);
            // Request current status
            ws.send(JSON.stringify({ cmd: 'getStatus' }));
          }
        };

        ws.onmessage = (event) => {
          try {
            const data = JSON.parse(event.data);
            // ONE shared controls webview follows the last-clicked pane across all
            // windows, so we show whatever the server broadcasts (the globally-active
            // player) — no per-window filtering.
            if (mounted) {
              setState(prev => ({
                ...prev,
                playerId: data.playerId || prev.playerId,
                isPlaying: data.isPlaying ?? prev.isPlaying,
                currentTime: data.currentTime ?? prev.currentTime,
                duration: data.duration ?? prev.duration,
                volume: data.volume ?? prev.volume,
                isMuted: data.isMuted ?? prev.isMuted,
                title: data.title || prev.title,
                isShuffled: data.isShuffled ?? prev.isShuffled,
                repeatMode: data.repeatMode || prev.repeatMode,
                isFullscreen: data.isFullscreen ?? prev.isFullscreen,
              }));
            }
          } catch (err) {
            console.error('[MediaControls] Failed to parse message:', err);
          }
        };

        ws.onclose = () => {
          console.log('[MediaControls] WebSocket disconnected');
          if (mounted) {
            setIsConnected(false);
            // Reconnect after delay
            reconnectTimeoutRef.current = setTimeout(connect, 2000);
          }
        };

        ws.onerror = (err) => {
          console.error('[MediaControls] WebSocket error:', err);
        };
      } catch (err) {
        console.error('[MediaControls] Failed to connect:', err);
        if (mounted) {
          reconnectTimeoutRef.current = setTimeout(connect, 2000);
        }
      }
    };

    connect();

    return () => {
      mounted = false;
      if (reconnectTimeoutRef.current) {
        clearTimeout(reconnectTimeoutRef.current);
      }
      if (wsRef.current) {
        wsRef.current.close();
      }
    };
  }, []);

  // Send command via WebSocket. Always tag it with our window label so the server
  // routes window-scoped actions (move/resize/shuffle/exit-fullscreen/…) back to
  // the correct app window and its controls.
  const sendCommand = useCallback((cmd: string, params: Record<string, unknown> = {}) => {
    if (wsRef.current?.readyState === WebSocket.OPEN) {
      wsRef.current.send(JSON.stringify({
        cmd,
        playerId: state.playerId || undefined,
        window: winLabelRef.current,
        ...params,
      }));
    }
  }, [state.playerId]);

  // Root container of the controls — measured to resize the native window so it
  // hugs the collapsed/expanded content (DOM width changes, but the raw X11
  // window doesn't, leaving the collapsed bar floating in a full-size window).
  const rootRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    // Resize after the DOM reflows to the new size. The window is a raw WebKitGTK
    // surface (no Tauri window API here), so we ask the backend over the WS.
    const t = setTimeout(() => {
      const el = rootRef.current;
      if (!el) return;
      const r = el.getBoundingClientRect();
      const dpr = window.devicePixelRatio || 1;
      sendCommand('resizeWindow', {
        width: Math.max(1, Math.round(r.width * dpr)),
        height: Math.max(1, Math.round(r.height * dpr)),
      });
    }, 50);
    return () => clearTimeout(t);
  }, [isCollapsed, sendCommand]);

  // Format time as MM:SS or HH:MM:SS
  const formatTime = (ms: number): string => {
    const seconds = Math.floor(ms / 1000);
    if (isNaN(seconds) || seconds < 0) return '0:00';
    const hrs = Math.floor(seconds / 3600);
    const mins = Math.floor((seconds % 3600) / 60);
    const secs = Math.floor(seconds % 60);
    if (hrs > 0) {
      return `${hrs}:${mins.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}`;
    }
    return `${mins}:${secs.toString().padStart(2, '0')}`;
  };

  // Control actions
  const togglePlay = () => sendCommand('togglePlay');
  const stop = () => sendCommand('stop');
  const skipBack = () => sendCommand('skipBackward', { seconds: 10 });
  const skipForward = () => sendCommand('skipForward', { seconds: 10 });
  const toggleMute = () => sendCommand('toggleMute');
  const setVolume = (vol: number) => sendCommand('setVolume', { volume: vol });
  const seek = (positionMs: number) => sendCommand('seek', { positionMs: Math.floor(positionMs) });
  const toggleRepeat = () => sendCommand('toggleRepeat');
  const togglePlaylist = () => sendCommand('togglePlaylist');
  const previousVideo = () => sendCommand('previousVideo');
  const nextVideo = () => sendCommand('nextVideo');
  const exitFullscreen = () => sendCommand('exitFullscreen');

  const handleProgressClick = (e: React.MouseEvent<HTMLDivElement>) => {
    if (!progressRef.current || state.duration <= 0) return;
    const rect = progressRef.current.getBoundingClientRect();
    const percent = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
    seek(percent * state.duration);
  };

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;

      switch (e.key) {
        case ' ':
          e.preventDefault();
          togglePlay();
          break;
        case 'ArrowUp':
          e.preventDefault();
          setVolume(Math.min(1, state.volume + 0.1));
          break;
        case 'ArrowDown':
          e.preventDefault();
          setVolume(Math.max(0, state.volume - 0.1));
          break;
        case 'ArrowLeft':
          e.preventDefault();
          seek(Math.max(0, state.currentTime - 10000));
          break;
        case 'ArrowRight':
          e.preventDefault();
          seek(Math.min(state.duration, state.currentTime + 10000));
          break;
        case 'Enter':
          e.preventDefault();
          skipForward();
          break;
        case 'm':
          e.preventDefault();
          toggleMute();
          break;
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [state.volume, state.currentTime, state.duration]);

  // Drag handling for moving the controls window
  const handleMouseDown = (e: React.MouseEvent<HTMLDivElement>) => {
    // Only drag from the header area (not buttons or inputs)
    const target = e.target as HTMLElement;
    if (target.tagName === 'BUTTON' || target.tagName === 'INPUT') return;
    e.preventDefault();
    setIsDragging(true);
    setDragStart({ x: e.screenX, y: e.screenY });
  };

  const handleMouseMove = useCallback((e: MouseEvent) => {
    if (!isDragging) return;
    const deltaX = e.screenX - dragStart.x;
    const deltaY = e.screenY - dragStart.y;
    // Send move command via WebSocket
    if (wsRef.current?.readyState === WebSocket.OPEN) {
      wsRef.current.send(JSON.stringify({ cmd: 'moveWindow', deltaX, deltaY, window: winLabelRef.current }));
    }
    setDragStart({ x: e.screenX, y: e.screenY });
  }, [isDragging, dragStart]);

  const handleMouseUp = useCallback(() => {
    setIsDragging(false);
  }, []);

  // Attach global mouse event listeners for dragging
  useEffect(() => {
    if (isDragging) {
      window.addEventListener('mousemove', handleMouseMove);
      window.addEventListener('mouseup', handleMouseUp);
    }
    return () => {
      window.removeEventListener('mousemove', handleMouseMove);
      window.removeEventListener('mouseup', handleMouseUp);
    };
  }, [isDragging, handleMouseMove, handleMouseUp]);

  const progress = state.duration > 0 ? (state.currentTime / state.duration) * 100 : 0;

  // Set document background to transparent for the X11 webview window
  useEffect(() => {
    document.documentElement.style.setProperty('background', 'transparent', 'important');
    document.documentElement.style.setProperty('background-color', 'transparent', 'important');
    document.body.style.setProperty('background', 'transparent', 'important');
    document.body.style.setProperty('background-color', 'transparent', 'important');
    document.body.style.margin = '0';
    document.body.style.padding = '0';
    document.body.style.overflow = 'hidden';

    const root = document.getElementById('root');
    if (root) {
      root.style.setProperty('background', 'transparent', 'important');
      root.style.setProperty('background-color', 'transparent', 'important');
      root.style.minHeight = 'auto';
    }
  }, []);

  // Render the control bar
  return (
    <div
      ref={rootRef}
      className="select-none"
      style={{
        width: isCollapsed ? '180px' : '500px',
        margin: 0,
        padding: 0,
      }}
    >
      {/* Header - draggable */}
      <div
        className={`flex items-center justify-between px-2 py-1 bg-gray-900/95 backdrop-blur-xl border border-white/20 ${
          isCollapsed ? 'rounded-lg' : 'rounded-t-lg border-b-0'
        }`}
        style={{ cursor: isDragging ? 'grabbing' : 'grab' }}
        onMouseDown={handleMouseDown}
      >
        <div className="flex items-center gap-2 flex-1 min-w-0">
          {/* Collapse/Expand toggle */}
          <button
            // Toggle on mousedown and stop it reaching the header's drag handler
            // (onMouseDown above). With onClick the press started a window drag,
            // so the collapse only fired after a no-move press ("move it around
            // and click a bit"). pointerdown/mousedown here is reliable in the
            // embedded WebKitGTK controls window.
            onMouseDown={(e) => { e.stopPropagation(); e.preventDefault(); setIsCollapsed(v => !v); }}
            className="p-0.5 text-gray-400 hover:text-white transition-colors flex-shrink-0"
            title={isCollapsed ? "Expand" : "Collapse"}
          >
            <svg className={`w-3.5 h-3.5 transition-transform ${isCollapsed ? 'rotate-180' : ''}`} fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
            </svg>
          </button>
          {/* Title fills the width up to the timestamp, then truncates. */}
          <span className="text-[10px] text-gray-400 font-medium truncate min-w-0 flex-1">
            {state.title || 'Media Controls'}
          </span>
          {!isConnected && (
            <span className="text-[9px] text-yellow-500 flex-shrink-0">(reconnecting...)</span>
          )}
        </div>
        <div className="text-[10px] text-gray-400 font-mono whitespace-nowrap flex-shrink-0 pl-2">
          {formatTime(state.currentTime)} / {formatTime(state.duration)}
        </div>
      </div>

      {/* Main Controls - Collapsible */}
      {!isCollapsed && (
        <div className="bg-gray-900/95 backdrop-blur-xl rounded-b-lg border border-t-0 border-white/20 px-3 py-2">
          {/* Progress Bar */}
          <div
            ref={progressRef}
            className="h-1.5 bg-white/20 rounded-full cursor-pointer mb-2 group relative"
            onClick={handleProgressClick}
          >
            <div
              className="h-full bg-purple-500 rounded-full relative transition-all"
              style={{ width: `${progress}%` }}
            >
              <div className="absolute right-0 top-1/2 -translate-y-1/2 w-2.5 h-2.5 bg-white rounded-full shadow-lg opacity-0 group-hover:opacity-100 transition-opacity" />
            </div>
          </div>

          {/* Control Buttons */}
          <div className="flex items-center justify-between gap-2">
            {/* Left: Previous video & Seek back (-10s) */}
            <div className="flex items-center">
              <button
                onClick={previousVideo}
                className="p-1.5 text-gray-400 hover:text-white transition-colors"
                title="Previous video"
              >
                {/* track-skip: bar + triangle */}
                <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 24 24">
                  <path d="M6 6h2v12H6zm3.5 6l8.5 6V6z" />
                </svg>
              </button>
              <button
                onClick={skipBack}
                className="p-1.5 text-gray-400 hover:text-white transition-colors"
                title="Back 10s"
              >
                {/* seek: double chevron */}
                <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M11 19l-7-7 7-7M18 19l-7-7 7-7" />
                </svg>
              </button>
            </div>

            {/* Center: Stop, Play/Pause, Seek forward (+10s), Next video */}
            <div className="flex items-center gap-1">
              <button
                onClick={stop}
                className="p-1.5 text-gray-400 hover:text-white transition-colors"
                title="Stop"
              >
                <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 24 24">
                  <path d="M6 6h12v12H6z" />
                </svg>
              </button>
              <button
                onClick={togglePlay}
                className="p-2 bg-purple-600 text-white rounded-full hover:bg-purple-500 transition-colors"
                title={state.isPlaying ? "Pause (Space)" : "Play (Space)"}
              >
                {state.isPlaying ? (
                  <svg className="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
                    <path d="M6 19h4V5H6v14zm8-14v14h4V5h-4z" />
                  </svg>
                ) : (
                  <svg className="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
                    <path d="M8 5v14l11-7z" />
                  </svg>
                )}
              </button>
              <button
                onClick={skipForward}
                className="p-1.5 text-gray-400 hover:text-white transition-colors"
                title="Forward 10s"
              >
                {/* seek: double chevron */}
                <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 5l7 7-7 7M6 5l7 7-7 7" />
                </svg>
              </button>
              <button
                onClick={nextVideo}
                className="p-1.5 text-gray-400 hover:text-white transition-colors"
                title="Next video"
              >
                {/* track-skip: triangle + bar */}
                <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 24 24">
                  <path d="M6 18l8.5-6L6 6v12zM16 6v12h2V6h-2z" />
                </svg>
              </button>
            </div>

            {/* Right: Repeat, Volume, Playlist */}
            <div className="flex items-center">
              <button
                onClick={toggleRepeat}
                className={`p-1.5 rounded transition-colors relative ${state.repeatMode !== 'none' ? 'text-green-400' : 'text-gray-500 hover:text-white'}`}
                title={`Repeat: ${state.repeatMode}`}
              >
                <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
                </svg>
                {state.repeatMode === 'one' && (
                  <span className="absolute -top-0.5 -right-0.5 text-[7px] font-bold bg-green-500 text-black rounded-full w-2.5 h-2.5 flex items-center justify-center">1</span>
                )}
              </button>

              {/* Volume */}
              <button
                onClick={toggleMute}
                className="p-1.5 text-gray-500 hover:text-white transition-colors"
                title={state.isMuted ? "Unmute (M)" : "Mute (M)"}
              >
                {state.isMuted || state.volume === 0 ? (
                  <svg className="w-3.5 h-3.5" fill="currentColor" viewBox="0 0 24 24">
                    <path d="M16.5 12c0-1.77-1.02-3.29-2.5-4.03v2.21l2.45 2.45c.03-.2.05-.41.05-.63zm2.5 0c0 .94-.2 1.82-.54 2.64l1.51 1.51C20.63 14.91 21 13.5 21 12c0-4.28-2.99-7.86-7-8.77v2.06c2.89.86 5 3.54 5 6.71zM4.27 3L3 4.27 7.73 9H3v6h4l5 5v-6.73l4.25 4.25c-.67.52-1.42.93-2.25 1.18v2.06c1.38-.31 2.63-.95 3.69-1.81L19.73 21 21 19.73l-9-9L4.27 3zM12 4L9.91 6.09 12 8.18V4z" />
                  </svg>
                ) : (
                  <svg className="w-3.5 h-3.5" fill="currentColor" viewBox="0 0 24 24">
                    <path d="M3 9v6h4l5 5V4L7 9H3zm13.5 3c0-1.77-1.02-3.29-2.5-4.03v8.05c1.48-.73 2.5-2.25 2.5-4.02zM14 3.23v2.06c2.89.86 5 3.54 5 6.71s-2.11 5.85-5 6.71v2.06c4.01-.91 7-4.49 7-8.77s-2.99-7.86-7-8.77z" />
                  </svg>
                )}
              </button>
              <input
                type="range"
                min="0"
                max="1"
                step="0.05"
                value={state.isMuted ? 0 : state.volume}
                onChange={(e) => setVolume(parseFloat(e.target.value))}
                className="w-12 h-1 accent-purple-500 cursor-pointer"
                title="Volume (Arrow Up/Down)"
              />

              <button
                onClick={togglePlaylist}
                className="p-1.5 text-gray-500 hover:text-white transition-colors"
                title="Queue/Playlist"
              >
                <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6h16M4 10h16M4 14h16M4 18h16" />
                </svg>
              </button>

              {/* Exit fullscreen — only shown while the app is in media fullscreen,
                  where the DOM exit affordance is hidden behind the video surface. */}
              {state.isFullscreen && (
                <button
                  onClick={exitFullscreen}
                  className="p-1.5 text-gray-500 hover:text-white transition-colors"
                  title="Exit fullscreen (Esc)"
                >
                  {/* compress / exit-fullscreen icon */}
                  <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 9L4 4m0 0v4m0-4h4m7 5l5-5m0 0v4m0-4h-4m-3 11l5 5m0 0v-4m0 4h-4M9 15l-5 5m0 0v-4m0 4h4" />
                  </svg>
                </button>
              )}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

export default MediaControls;
