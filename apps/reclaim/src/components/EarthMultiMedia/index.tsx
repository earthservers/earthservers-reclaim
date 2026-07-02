// EarthMultiMedia - Privacy-focused media player
// Video, Image, Audio player with split view support and optional history

import { useState, useEffect, useCallback, useRef } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { invoke, listen, emitTo, isTauri } from '../../lib/tauri';
import VideoPlayer from './VideoPlayer';
import ImageViewer from './ImageViewer';
import GStreamerPlayer from './GStreamerPlayer';
import GStreamerVideoPlayer, { PlayerStatusExport } from './GStreamerVideoPlayer';
import { VaultAutofill } from '../VaultAutofill';

// Types
export type MediaType = 'video' | 'image' | 'audio';
export type ViewLayout = 'single' | 'horizontal' | 'vertical' | 'quad';
// Photo slideshow modes:
//  - 'shuffle':     all panes change together; random order, reshuffled each pass, never a dupe on screen
//  - 'consecutive': all panes change together; photos in queue order
//  - 'staggered':   one pane changes per interval, round-robin (pane 1, then 2, ...), random non-dupe photo
export type SlideshowMode = 'shuffle' | 'consecutive' | 'staggered';

// Fisher-Yates shuffle (returns a new array)
function shuffleArray<T>(arr: T[]): T[] {
  const a = [...arr];
  for (let i = a.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [a[i], a[j]] = [a[j], a[i]];
  }
  return a;
}

interface MediaItem {
  source: string;
  type: MediaType;
  title?: string;
}

// Queue item with unique ID for tracking played status
interface QueueItem {
  id: string;
  source: string;
  type: MediaType;
  title?: string;
  played: boolean;
}

// Per-pane state for multi-instance playback
interface PaneState {
  currentItem: QueueItem | null;
  isPlaying: boolean;
  currentTime: number;
  duration: number;
}

// Slideshow settings
interface SlideshowSettings {
  enabled: boolean;
  interval: number; // seconds
  mode: SlideshowMode;
}

// A media tab is an independent workspace (browser-style): its own queue, pane
// contents, layout, and playback focus. Only the active tab's state is "live"
// (mounted players); switching tabs snapshots the current tab and restores the
// target, so each tab's media/queue is hidden when you switch away from it.
interface MediaTab {
  id: string;
  title: string;
  queue: QueueItem[];
  mediaItems: (MediaItem | null)[];
  paneStates: PaneState[];
  layout: ViewLayout;
  activePane: number;
  playedItems: string[]; // QueueItem ids already played (Set serialized for storage)
}

const emptyPaneStates = (): PaneState[] => [
  { currentItem: null, isPlaying: false, currentTime: 0, duration: 0 },
  { currentItem: null, isPlaying: false, currentTime: 0, duration: 0 },
  { currentItem: null, isPlaying: false, currentTime: 0, duration: 0 },
  { currentItem: null, isPlaying: false, currentTime: 0, duration: 0 },
];

interface PrivacySettings {
  profile_id: number;
  history_enabled: boolean;
  playlist_history_enabled: boolean;
  require_password: boolean;
  require_otp: boolean;
  password_hash: string | null;
  otp_secret: string | null;
  auto_clear_history_days: number | null;
}

interface Playlist {
  id: number;
  profile_id: number;
  name: string;
  description: string | null;
  thumbnail: string | null;
  is_encrypted: boolean;
  created_at: string;
  updated_at: string | null;
  item_count: number;
}

interface PlaylistItem {
  id: number;
  playlist_id: number;
  source: string;
  media_type: string;
  title: string | null;
  thumbnail: string | null;
  position: number;
  added_at: string;
}

interface EarthMultiMediaProps {
  profileId: number;
  initialSource?: string;
  initialType?: MediaType;
}

// Detect media type from URL/path
const VIDEO_EXTS = ['mp4', 'webm', 'ogg', 'mov', 'avi', 'mkv', 'm4v'];
const IMAGE_EXTS = ['jpg', 'jpeg', 'png', 'gif', 'webp', 'bmp', 'svg', 'ico'];
const AUDIO_EXTS = ['mp3', 'wav', 'ogg', 'flac', 'm4a', 'aac'];

function detectMediaType(source: string): MediaType {
  const ext = source.split('.').pop()?.toLowerCase() || '';

  if (VIDEO_EXTS.includes(ext)) return 'video';
  if (IMAGE_EXTS.includes(ext)) return 'image';
  if (AUDIO_EXTS.includes(ext)) return 'audio';

  // Check URL patterns
  if (source.includes('youtube.com') || source.includes('youtu.be') || source.includes('vimeo.com')) {
    return 'video';
  }

  return 'video'; // Default to video
}

// Whether a filename has a recognised media extension. Unlike detectMediaType
// (which defaults unknown names to 'video'), this returns false for anything
// without a known extension — used to filter folder contents on drop.
function isMediaFile(name: string): boolean {
  const ext = name.split('.').pop()?.toLowerCase() || '';
  return VIDEO_EXTS.includes(ext) || IMAGE_EXTS.includes(ext) || AUDIO_EXTS.includes(ext);
}

// Persists media UI state ACROSS REMOUNTS. Switching to the Search tab unmounts
// this component, but the backend GStreamer players keep playing — so without
// this the UI forgets the queue/panes (shows "empty"/"No media loaded") while a
// video is still playing, and you can't stop it. We cache the panes/queue/layout
// here (module scope survives unmount) and restore them on the next mount; the
// players themselves are resumed (not reloaded) by GStreamerVideoPlayer.
// NOTE: fullscreen is deliberately NOT cached — it resets to windowed on return,
// avoiding the fullscreen-state desync after a remount.
const mediaStateCache: {
  layout?: ViewLayout;
  mediaItems?: (MediaItem | null)[];
  queue?: QueueItem[];
  slideshow?: SlideshowSettings;
} = {};

// Whether we've already auto-prompted for a media password this app session.
// The prompt is a full-screen overlay, and EarthMultiMedia remounts every time
// you return to the Media tab — without this guard the modal re-opens on each
// return (its dialog hidden behind the native video surfaces), blacking out and
// blocking the whole UI while videos keep playing.
let mediaPasswordPrompted = false;

// Profiles whose media tab has been unlocked THIS app session. Module-scoped so it
// survives the component's remounts (the tab remounts every time you return to it);
// resets to locked on app restart, like the other password gates. Incognito does
// NOT bypass this — the gate is enforced whatever the privacy mode.
const mediaUnlockedProfiles = new Set<number>();

/// Clear all media session unlocks (called on profile switch so media re-gates).
export function lockAllMediaSessions() {
  mediaUnlockedProfiles.clear();
}

export function EarthMultiMedia({ profileId, initialSource, initialType, onFullscreenChange }: EarthMultiMediaProps & { onFullscreenChange?: (isFullscreen: boolean) => void }) {
  // State
  const [layout, setLayout] = useState<ViewLayout>(() => mediaStateCache.layout ?? 'single');
  const [mediaItems, setMediaItems] = useState<(MediaItem | null)[]>(() => mediaStateCache.mediaItems ?? [null, null, null, null]);
  const [activePane, setActivePane] = useState(0);
  const [privacySettings, setPrivacySettings] = useState<PrivacySettings | null>(null);
  const [playlists, setPlaylists] = useState<Playlist[]>([]);
  const [currentPlaylist, setCurrentPlaylist] = useState<Playlist | null>(null);
  const [playlistItems, setPlaylistItems] = useState<PlaylistItem[]>([]);
  const [showPlaylistPanel, setShowPlaylistPanel] = useState(false);
  const [showPrivacyPanel, setShowPrivacyPanel] = useState(false);
  const [urlInput, setUrlInput] = useState('');
  const [newPlaylistName, setNewPlaylistName] = useState('');
  // Which queue item's "add to playlist" menu is currently open (by queue item id)
  const [addToPlaylistMenuId, setAddToPlaylistMenuId] = useState<string | null>(null);
  // In-app "name this playlist" modal (replaces the native prompt() dialog)
  const [playlistNamePrompt, setPlaylistNamePrompt] = useState<{ title: string; onConfirm: (name: string) => void } | null>(null);
  const [playlistNameInput, setPlaylistNameInput] = useState('');
  const [isFullscreen, setIsFullscreen] = useState(false);
  // Video "Enhance" (FSR super-resolution upscaling) — session-scoped; the
  // backend keeps it as the default for panes created later.
  const [enhanceOn, setEnhanceOn] = useState(false);
  const [showFullscreenHeader, setShowFullscreenHeader] = useState(true);
  const fullscreenHeaderTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  // Per-window namespace for BACKEND player IDs. "New Window" opens additional app
  // windows that all share the global GStreamer player manager / video surfaces /
  // controls server, so each window must use distinct pane player IDs — prefix them
  // with this window's label (e.g. "main::pane-0"). DOM event names (media-*-pane-N)
  // stay index-based since each window has its own DOM/event bus.
  const [winLabel, setWinLabel] = useState<string>(() => {
    try { return isTauri() ? (getCurrentWindow().label || 'main') : 'main'; }
    catch { return 'main'; }
  });
  // The BACKEND is authoritative for the window label — getCurrentWindow().label in
  // JS proved unreliable across "New Window" instances (it collapsed to "main", so
  // a second window's controls drove the first). Correct it as soon as we know, and
  // gate all backend pushes on `winReady` so a second window never pushes state under
  // the wrong ("main") label before the real one resolves.
  const [winReady, setWinReady] = useState<boolean>(() => !isTauri());
  useEffect(() => {
    if (!isTauri()) return;
    invoke<string>('media_window_label')
      .then((l) => { if (l) setWinLabel(l); })
      .catch(() => {})
      .finally(() => setWinReady(true));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  // Always-current mirror for mount-only listeners/closures (they'd otherwise
  // capture the pre-correction label).
  const winLabelRef = useRef(winLabel);
  winLabelRef.current = winLabel;
  const panePid = useCallback((i: number) => `${winLabel}::pane-${i}`, [winLabel]);

  // Toggle FSR super-resolution on every pane of THIS window. The backend
  // restarts each pipeline around the filter swap (position is restored) and
  // remembers the mode as the session default for new panes. Reverts the UI
  // state if the backend refuses (e.g. GL plugins missing / EARTH_VIDEO_SR=off).
  const toggleEnhance = useCallback(async () => {
    const next = !enhanceOn;
    setEnhanceOn(next);
    const mode = next ? 'fsr' : 'off';
    try {
      const ids = await invoke<string[]>('player_list').catch(() => [] as string[]);
      const mine = ids.filter(id => id.startsWith(`${winLabelRef.current}::`));
      const targets = mine.length ? mine : [`${winLabelRef.current}::pane-0`];
      await Promise.all(targets.map(id => invoke('player_set_enhance', { playerId: id, mode })));
    } catch (err) {
      console.error('Failed to set video enhancement:', err);
      setEnhanceOn(!next);
    }
  }, [enhanceOn]);
  // Extract the pane index from a (possibly namespaced) player id like "main::pane-2".
  const paneIndexOf = useCallback((id: string | null | undefined) => {
    const m = (id || '').match(/pane-(\d+)/);
    return m ? parseInt(m[1], 10) : 0;
  }, []);

  // Tell the backend to drop this window's controls state when it unloads, so the
  // status broadcast stops polling a now-dead player and the maps don't leak.
  useEffect(() => {
    if (!isTauri()) return;
    const onUnload = () => { invoke('forget_media_window', { window: winLabelRef.current }).catch(() => {}); };
    window.addEventListener('beforeunload', onUnload);
    return () => window.removeEventListener('beforeunload', onUnload);
  }, []);

  // Playback state for floating controls
  const [playbackState, setPlaybackState] = useState({
    isPlaying: false,
    currentTime: 0,
    duration: 0,
    volume: 1,
    isMuted: false,
    isShuffled: false,
    repeatMode: 'none' as 'none' | 'one' | 'all',
  });

  // Queue management for multi-pane playback
  const [queue, setQueue] = useState<QueueItem[]>(() => mediaStateCache.queue ?? []);
  const [playedItems, setPlayedItems] = useState<Set<string>>(new Set());
  // Drag-to-reorder state for the (temporary) queue list.
  const [queueDragIndex, setQueueDragIndex] = useState<number | null>(null);
  const [queueDropIndex, setQueueDropIndex] = useState<number | null>(null);

  // Per-pane state tracking
  const [paneStates, setPaneStates] = useState<PaneState[]>([
    { currentItem: null, isPlaying: false, currentTime: 0, duration: 0 },
    { currentItem: null, isPlaying: false, currentTime: 0, duration: 0 },
    { currentItem: null, isPlaying: false, currentTime: 0, duration: 0 },
    { currentItem: null, isPlaying: false, currentTime: 0, duration: 0 },
  ]);

  // Player status tracking for stacked controls (multi-pane mode)
  const [playerStatuses, setPlayerStatuses] = useState<Record<string, PlayerStatusExport>>({});

  // Slideshow settings
  const [slideshow, setSlideshow] = useState<SlideshowSettings>(() => mediaStateCache.slideshow ?? {
    enabled: false,
    interval: 5,
    // Rotation STYLE only ('consecutive' all panes together | 'staggered' one at a
    // time). Randomization is the separate Shuffle toggle (playbackState.isShuffled),
    // which applies to photos AND videos. (Legacy 'shuffle' is treated as consecutive.)
    mode: 'consecutive',
  });
  // Current ordering of image ids + cursor, driving rotation. A ref so ticks
  // don't churn React state / re-create the interval.
  const slideshowOrderRef = useRef<{ order: string[]; cursor: number }>({ order: [], cursor: 0 });
  // Staggered mode: which pane changes next (round-robin).
  const staggerPaneRef = useRef(0);

  // Media tabs state for consolidating/separating instances
  // Workspace tabs. The first tab mirrors the initial (cached) working state;
  // all other tabs are independent sessions. The currently-active tab's data is
  // the live working state above (queue/mediaItems/paneStates/layout/...).
  const [mediaTabs, setMediaTabs] = useState<MediaTab[]>(() => [{
    id: 'tab-1',
    title: 'Tab 1',
    queue: mediaStateCache.queue ?? [],
    mediaItems: mediaStateCache.mediaItems ?? [null, null, null, null],
    paneStates: emptyPaneStates(),
    layout: mediaStateCache.layout ?? 'single',
    activePane: 0,
    playedItems: [],
  }]);
  const [activeTabId, setActiveTabId] = useState<string>('tab-1');
  const tabIdCounter = useRef(0);

  // Refs to each pane container, used to hit-test OS file drops against pane
  // bounds. Native (Tauri) drag-drop only reports a window position, not the
  // DOM element under the cursor — and the video panes render as native
  // GStreamer surfaces over the DOM, so HTML5 onDrop never fires for them.
  // We therefore resolve the target pane geometrically at drop time.
  const paneRefs = useRef<Array<HTMLDivElement | null>>([]);
  // Pane currently highlighted by an in-progress OS file drag-over (null = none).
  const [osDropPane, setOsDropPane] = useState<number | null>(null);
  // Mirror of activePane for use inside stable native-event listeners.
  const activePaneRef = useRef(0);
  activePaneRef.current = activePane;

  // Mirror persistent UI state into the module cache so it survives a remount
  // (switch to Search and back). The players keep running in the backend; this
  // keeps the UI (panes/queue) in sync with them.
  useEffect(() => { mediaStateCache.layout = layout; }, [layout]);
  useEffect(() => { mediaStateCache.mediaItems = mediaItems; }, [mediaItems]);
  useEffect(() => { mediaStateCache.queue = queue; }, [queue]);
  useEffect(() => { mediaStateCache.slideshow = slideshow; }, [slideshow]);

  // Mirror shuffle + repeat state to the backend so the floating media controls
  // reflect (and stay in sync with) it via the status broadcast.
  useEffect(() => {
    if (!isTauri() || !winReady) return;
    invoke('set_media_playback_flags', {
      window: winLabel,
      isShuffled: playbackState.isShuffled,
      repeatMode: playbackState.repeatMode,
    }).catch(() => {});
  }, [playbackState.isShuffled, playbackState.repeatMode, winLabel, winReady]);

  // Mirror the ACTIVE pane's media title to the backend so the floating controls
  // can show it (the backend's own GStreamer-tag title is usually empty for local
  // files). Driven by activePane + its media, so it follows the focused video.
  useEffect(() => {
    if (!isTauri() || !winReady) return;
    const title = mediaItems[activePane]?.title || '';
    invoke('set_media_active_title', { window: winLabel, title }).catch(() => {});
  }, [activePane, mediaItems, winLabel, winReady]);

  // Mirror fullscreen state to the backend so the floating controls can show an
  // exit-fullscreen button (the DOM exit affordance is occluded by the video).
  useEffect(() => {
    if (!isTauri() || !winReady) return;
    invoke('set_media_fullscreen', { window: winLabel, isFullscreen }).catch(() => {});
  }, [isFullscreen, winLabel, winReady]);

  // When the layout changes (queue panel opens/closes, fullscreen toggles), the
  // video panes resize. Nudge a window 'resize' after the DOM settles so every
  // GStreamerVideoPlayer re-syncs its native surface bounds to the new pane size
  // (its ResizeObserver/resize listener drives the update). Without this the
  // surface can keep its old size and cover the queue — so you can't remove or
  // stop a video, especially right after returning to the Media tab.
  useEffect(() => {
    if (!isTauri()) return;
    const ids = [
      setTimeout(() => window.dispatchEvent(new Event('resize')), 60),
      setTimeout(() => window.dispatchEvent(new Event('resize')), 260),
    ];
    return () => ids.forEach(clearTimeout);
  }, [showPlaylistPanel, isFullscreen]);

  // Show video surfaces when mounting (returning to Media tab), hide when unmounting
  // Keep playback running in background - only hide/show the visual surfaces
  useEffect(() => {
    let mounted = true;

    // CRITICAL: do NOT touch any native surface until we know THIS window's real
    // label (winReady). Otherwise a second window, whose label is briefly the
    // initial getCurrentWindow() value, would hide/show surfaces under the wrong
    // ("main") namespace — unmapping the FIRST window's actively-playing surface and
    // crashing the WebKit/GStreamer pipeline. Always use winLabelRef.current (the
    // confirmed label), never a captured panePid.
    if (isTauri() && winReady) {
      const surfPid = (i: number) => `${winLabelRef.current}::pane-${i}`;
      const showSurfaces = () => {
        const showOne = async (i: number) => {
          if (!mounted || i >= 4) return;
          const m = mediaItemsRef.current[i];
          const hasMedia = !!m && (m.type === 'video' || m.type === 'audio');
          // ONLY ever SHOW panes that actually hold media. Never hide on mount: a
          // freshly-opened window has no stale surfaces to clear, and hiding under a
          // shared namespace would unmap ANOTHER window's live surface and crash it.
          if (hasMedia) {
            try {
              await Promise.race([
                invoke('show_video_surface', { playerId: surfPid(i) }),
                new Promise((_, reject) => setTimeout(() => reject('timeout'), 100))
              ]);
            } catch {
              // Surface might not exist yet or timed out, that's fine
            }
          }
          requestAnimationFrame(() => {
            if (mounted) setTimeout(() => showOne(i + 1), 20);
          });
        };
        showOne(0);
      };
      requestAnimationFrame(() => {
        if (mounted) setTimeout(showSurfaces, 100);
      });
    }

    return () => {
      mounted = false;
      if (isTauri() && winReady) {
        for (let i = 0; i < 4; i++) {
          invoke('hide_video_surface', { playerId: `${winLabelRef.current}::pane-${i}` }).catch(() => {});
        }
      }
    };
  }, [winReady]);

  // Track which player is currently active (last clicked/interacted with)
  const [activePlayerId, setActivePlayerId] = useState<string>(`${winLabel}::pane-0`);
  // Mirror so the (mount-only) control-action listener targets the CURRENT active
  // player instead of the value captured at mount — without it the floating
  // controls always drove pane-0.
  const activePlayerIdRef = useRef(activePlayerId);
  activePlayerIdRef.current = activePlayerId;
  const floatingControlsCreatedRef = useRef(false);
  const playerStatusesRef = useRef(playerStatuses);
  // Mirror of mediaItems so the slideshow tick can read current pane contents
  // (for staggered dupe-avoidance) without re-creating the interval each change.
  const mediaItemsRef = useRef(mediaItems);
  // Mirrors so the mount-only control-action handler (next/prev video) reads the
  // CURRENT queue + shuffle/repeat state.
  const queueRef = useRef(queue);
  const playbackStateRef = useRef(playbackState);
  // Mirrors so the queue-advance logic (auto EOS + next/prev buttons) always reads
  // the CURRENT played-set and pane contents — avoids stale closures that left a
  // pane stuck after a couple of advances.
  const playedItemsRef = useRef(playedItems);
  const paneStatesRef = useRef(paneStates);
  // Stable handle to the idle-auto-hide "activity" pinger, so the mount-only
  // control-action listener can call it without a forward reference.
  const markMediaActivityRef = useRef<(() => void) | undefined>(undefined);

  // Keep refs in sync with state
  useEffect(() => {
    playerStatusesRef.current = playerStatuses;
  }, [playerStatuses]);
  useEffect(() => {
    mediaItemsRef.current = mediaItems;
  }, [mediaItems]);
  useEffect(() => { queueRef.current = queue; }, [queue]);
  useEffect(() => { playbackStateRef.current = playbackState; }, [playbackState]);
  useEffect(() => { playedItemsRef.current = playedItems; }, [playedItems]);
  useEffect(() => { paneStatesRef.current = paneStates; }, [paneStates]);

  // The floating controls follow the focused pane. Clicking any pane updates
  // activePane (DOM click handler / native video-surface-clicked / tab restore);
  // mirror that into activePlayerId so the controls retarget to that pane's
  // player. Each pane's playerId is `pane-${index}`.
  useEffect(() => {
    setActivePlayerId(panePid(activePane));
  }, [activePane, panePid]);

  // Tell the backend which player the floating controls should drive. The
  // controls talk to a WebSocket server that broadcasts ONE player's status and
  // routes commands to it — so without this the controls are stuck on pane-0.
  //
  // Only claim the shared (cross-window) active player when THIS window actually has
  // media — otherwise merely opening an empty second window would hijack the active
  // player to its empty pane-0 and freeze the controls that another window is driving.
  useEffect(() => {
    if (!isTauri() || !winReady) return;
    const hasMedia = mediaItems.some(Boolean) || queue.length > 0;
    if (!hasMedia) return;
    invoke('set_active_media_player', { playerId: activePlayerId }).catch(() => {});
  }, [activePlayerId, winReady, mediaItems, queue]);

  // Create X11 webview controls window with HTML controls
  // This creates a GTK window with WebKitGTK that loads the /media-controls route
  useEffect(() => {
    if (!isTauri()) {
      return;
    }

    let mounted = true;
    const unlisteners: (() => void)[] = [];

    // Controls are now created on-demand when video plays (see player status effect below)
    // This function is no longer called on mount

    // Listen for control actions from the floating controls window
    const setupEventListeners = async () => {
      // Frontend-state control actions (shuffle/repeat/playlist/skip/exit-fullscreen)
      // arrive as a DOM CustomEvent eval'd by the backend straight into THIS window's
      // webview (see dispatch_media_control_action in lib.rs). A broadcast Tauri event
      // never reached a 2nd window's listener, so the controls couldn't drive it. No
      // window filter is needed — the backend already targeted the right webview.
      const onControlAction = (e: Event) => {
        if (!mounted) return;
        const detail = (e as CustomEvent).detail as { action: string; playerId?: string; volume?: number; time?: number };

        // Interacting with the floating controls counts as activity (those events
        // originate in a separate window the main app's mousemove can't see), so the
        // idle auto-hide doesn't pull the controls out from under the user.
        markMediaActivityRef.current?.();

        const { action, playerId, volume, time } = detail;
        const targetPlayerId = playerId || activePlayerIdRef.current;

        void (async () => {
        try {
          switch (action) {
            case 'toggle-play':
              // Check current state and toggle
              const status = await invoke<{ state: string }>('player_get_status', { playerId: targetPlayerId }).catch(() => null);
              if (status?.state === 'Playing') {
                await invoke('player_pause', { playerId: targetPlayerId });
              } else {
                await invoke('player_play', { playerId: targetPlayerId });
              }
              break;
            case 'stop':
              await invoke('player_stop', { playerId: targetPlayerId });
              break;
            case 'skip-back':
              await invoke('player_skip_backward', { playerId: targetPlayerId, seconds: 10 });
              break;
            case 'skip-forward':
              await invoke('player_skip_forward', { playerId: targetPlayerId, seconds: 10 });
              break;
            case 'toggle-mute':
              const muteStatus = await invoke<{ muted: boolean }>('player_get_status', { playerId: targetPlayerId }).catch(() => null);
              await invoke('player_set_muted', { playerId: targetPlayerId, muted: !(muteStatus?.muted) });
              break;
            case 'set-volume':
              if (typeof volume === 'number') {
                await invoke('player_set_volume', { playerId: targetPlayerId, volume });
              }
              break;
            case 'seek':
              if (typeof time === 'number') {
                await invoke('player_seek', { playerId: targetPlayerId, positionMs: Math.floor(time) });
              }
              break;
            case 'toggle-shuffle':
              setPlaybackState(prev => ({ ...prev, isShuffled: !prev.isShuffled }));
              break;
            case 'cycle-repeat': {
              const order = ['none', 'all', 'one'] as const;
              setPlaybackState(prev => ({
                ...prev,
                repeatMode: order[(order.indexOf(prev.repeatMode) + 1) % order.length],
              }));
              break;
            }
            case 'toggle-playlist':
              setShowPlaylistPanel(p => !p);
              break;
            case 'exit-fullscreen':
              if (isFullscreenRef.current) {
                if (document.fullscreenElement) document.exitFullscreen().catch(() => {});
                setIsFullscreen(false);
                onFullscreenChange?.(false);
              }
              break;
            case 'next-video': {
              // Reuse the auto-advance path so it respects shuffle (random next when
              // on) and repeat (repeat-all loops the queue). manual:true so it still
              // ADVANCES under repeat-one instead of replaying the same clip.
              const idx = paneIndexOf(activePlayerIdRef.current);
              window.dispatchEvent(new CustomEvent(`media-ended-pane-${idx}`, { detail: { manual: true } }));
              break;
            }
            case 'prev-video': {
              // Positional previous in queue order; wraps to the end under repeat-all.
              const idx = paneIndexOf(activePlayerIdRef.current);
              const q = queueRef.current;
              if (q.length === 0) break;
              const curSrc = mediaItemsRef.current[idx]?.source;
              let pos = q.findIndex(it => it.source === curSrc);
              if (pos < 0) pos = 0;
              let prevPos = pos - 1;
              if (prevPos < 0) prevPos = playbackStateRef.current.repeatMode !== 'none' ? q.length - 1 : 0;
              const item = q[prevPos];
              if (!item) break;
              setMediaItems(prev => { const u = [...prev]; u[idx] = { source: item.source, type: item.type, title: item.title }; return u; });
              setPaneStates(prev => { const u = [...prev]; u[idx] = { ...u[idx], currentItem: item, isPlaying: true }; return u; });
              break;
            }
          }
        } catch (err) {
          console.error('[EarthMultiMedia] Control action failed:', err);
        }
        })();
      };
      window.addEventListener('__earth_media_control_action', onControlAction);
      unlisteners.push(() => window.removeEventListener('__earth_media_control_action', onControlAction));

      // Listen for controls ready event to send initial state
      const unlistenReady = await listen('media-controls-ready', () => {
        // Use ref to get fresh state (closure would have stale data)
        const currentStatuses = playerStatusesRef.current;
        const activeId = activePlayerIdRef.current;
        const status = currentStatuses[activeId];
        if (status) {
          emitTo('media-controls', 'media-state-update', {
            playerId: activeId,
            isPlaying: status.isPlaying,
            currentTime: status.currentTime * 1000,
            duration: status.duration * 1000,
            volume: status.volume,
            isMuted: status.isMuted,
            title: status.title || '',
          }).catch(console.error);
        }
      });
      unlisteners.push(unlistenReady);
    };

    // Don't create controls immediately - wait until a video is actually playing
    // setupX11WebviewControls(); // REMOVED - controls now created on-demand when video plays
    setupEventListeners();

    return () => {
      mounted = false;
      unlisteners.forEach(unlisten => unlisten());
      // DON'T hide controls on unmount - React Strict Mode causes rapid mount/unmount in dev
      // The controls should persist and just be shown/hidden as needed
      // Hiding immediately can interfere with creation in progress
    };
  }, []); // Empty deps - only run on mount/unmount, not on activePlayerId change

  // Send state updates to floating controls window when player status changes
  // Create controls on-demand when a video is actually playing
  useEffect(() => {
    if (!isTauri()) return;

    const status = playerStatuses[activePlayerId];
    if (!status) return;

    // If a video is playing and controls haven't been created yet, create them now.
    // Gate on the CURRENT tab actually having media — otherwise a stale "isPlaying"
    // status (e.g. just after switching to a new/empty tab) would re-show controls
    // that the no-media effect just hid.
    const createControlsIfNeeded = async () => {
      const hasMedia = mediaItems.some(Boolean) || queue.length > 0;
      // Don't create the controls window until we know our real window label, or a
      // second window would create/tag its controls under "main".
      if (status.isPlaying && hasMedia && winReady && !floatingControlsCreatedRef.current) {
        try {
          // Get window dimensions and position for positioning controls
          const { getCurrentWindow } = await import('@tauri-apps/api/window');
          const currentWindow = getCurrentWindow();
          const innerSize = await currentWindow.innerSize();
          const outerSize = await currentWindow.outerSize();
          const outerPosition = await currentWindow.outerPosition();

          // Calculate title bar height (difference between outer and inner height)
          const titleBarHeight = outerSize.height - innerSize.height;

          // Position controls at bottom center of window (absolute screen coordinates)
          const controlsWidth = 500;
          const controlsHeight = 94;
          const relativeX = Math.round((innerSize.width - controlsWidth) / 2);
          const relativeY = innerSize.height - controlsHeight - 20;

          // Calculate absolute screen position
          // outerPosition.y is at top of title bar, add titleBarHeight to get to content area
          const bounds = {
            x: outerPosition.x + relativeX,
            y: outerPosition.y + titleBarHeight + relativeY,
            width: controlsWidth,
            height: controlsHeight,
          };


          // Separate X11 webview overlay for the controls. In dev it loads from the
          // Vite dev server; in packaged builds it loads from the embedded-asset
          // server (assets_server.rs on :9877) since a raw WebKitGTK webview can't
          // use tauri:// and the assets aren't on disk.
          // Carry this window's label so the controls webview can (a) filter the
          // status broadcast to its own window and (b) tag commands it sends back.
          const winQuery = `?win=${encodeURIComponent(winLabelRef.current)}`;
          const controlsUrl = import.meta.env.DEV
            ? `http://localhost:1420/media-controls${winQuery}`
            : `http://127.0.0.1:9877/media-controls${winQuery}`;
          // create_x11_webview_controls is idempotent (no-ops if the window already
          // exists), so this also covers re-entering the media tab. Always show
          // afterwards so a previously HIDDEN controls window becomes visible again.
          await invoke('create_x11_webview_controls', { bounds, url: controlsUrl });
          floatingControlsCreatedRef.current = true;
          invoke('show_x11_webview_controls').catch(() => {});
        } catch (err) {
          console.warn('[EarthMultiMedia] Failed to create controls on-demand:', err);
        }
      }
    };

    if (winReady) createControlsIfNeeded();

    // Try to emit state update to controls window
    const sendUpdate = () => {
      emitTo('media-controls', 'media-state-update', {
        playerId: activePlayerId,
        isPlaying: status.isPlaying,
        currentTime: status.currentTime * 1000,
        duration: status.duration * 1000,
        volume: status.volume,
        isMuted: status.isMuted,
        title: status.title || '',
      }).catch(() => {
        // Controls window might not exist yet, that's ok
      });
    };

    // Send immediately
    sendUpdate();
  }, [playerStatuses, activePlayerId, mediaItems, queue, winReady]);

  // Tear down the floating controls when the active media session has nothing to
  // control — no media loaded in any pane and an empty queue (e.g. a freshly
  // opened media tab, or after closing the tab that had the videos). Without this
  // the controls linger and can keep driving a now-hidden player.
  useEffect(() => {
    if (!isTauri()) return;
    const hasMedia = mediaItems.some(Boolean) || queue.length > 0;
    if (!hasMedia && floatingControlsCreatedRef.current) {
      floatingControlsCreatedRef.current = false;
      // The controls are a single SHARED window. Only hide them if THIS window is the
      // one currently driving them (or nobody is) — otherwise an empty window would
      // hide controls another window is actively using. HIDE, don't destroy:
      // destroying the X11 window mid-render (e.g. while a video surface is tearing
      // down on tab switch) crashes the app with an X11 RenderBadPicture error.
      invoke<string>('controls_active_window').then(owner => {
        if (!owner || owner === winLabelRef.current) {
          invoke('hide_x11_webview_controls').catch(() => {});
        }
      }).catch(() => {});
    }
  }, [mediaItems, queue]);

  // Generate unique tab ID
  const generateTabId = useCallback(() => {
    tabIdCounter.current += 1;
    return `tab-${Date.now()}-${tabIdCounter.current}`;
  }, []);

  // A readable title for a tab from its first loaded media (or first queue item).
  const sessionTitle = (items: (MediaItem | null)[], q: QueueItem[]): string => {
    const first = (items.find(Boolean) as MediaItem | undefined);
    const src = first?.title || first?.source || q[0]?.title || q[0]?.source;
    return src ? (src.split('/').pop() || src) : 'Empty Tab';
  };

  // Stop the native players backing the CURRENT panes — call before swapping the
  // visible tab so the previous tab's video/audio doesn't keep playing underneath.
  const stopCurrentPlayers = () => {
    mediaItems.forEach((m, i) => {
      if (m && (m.type === 'video' || m.type === 'audio')) {
        const playerId = panePid(i);
        invoke('player_stop', { playerId }).catch(() => {});
        invoke('hide_video_surface', { playerId }).catch(() => {});
      }
    });
  };

  // Pane states with each video/audio pane's LIVE playback position and play
  // state folded in (from the player status poller). Captured at snapshot time so
  // a tab restores to exactly where it was paused/playing. currentTime is seconds.
  const livePaneStates = (): PaneState[] => paneStates.map((ps, i) => {
    const st = playerStatusesRef.current[panePid(i)];
    return st ? { ...ps, currentTime: st.currentTime, isPlaying: st.isPlaying } : ps;
  });

  // Fold the live working state back into the active tab's stored snapshot.
  const snapshotActive = (tabs: MediaTab[]): MediaTab[] => {
    const live = livePaneStates();
    return tabs.map(t => t.id === activeTabId
      ? { ...t, queue, mediaItems, paneStates: live, layout, activePane, playedItems: [...playedItems], title: sessionTitle(mediaItems, queue) }
      : t);
  };

  // Make a stored tab the live working state.
  const loadSession = (s: MediaTab) => {
    setQueue(s.queue);
    setMediaItems(s.mediaItems);
    setPaneStates(s.paneStates);
    setLayout(s.layout);
    setActivePane(s.activePane);
    setPlayedItems(new Set(s.playedItems));
  };

  // Switch to another tab: snapshot the current one, reveal the target's queue
  // and panes (the previous tab's media is now hidden / unmounted).
  const switchToTab = (id: string) => {
    if (id === activeTabId) return;
    const target = mediaTabs.find(t => t.id === id);
    if (!target) return;
    stopCurrentPlayers();
    setMediaTabs(prev => snapshotActive(prev));
    loadSession(target);
    setActiveTabId(id);
  };

  // New tab: snapshot the current one, then open a fresh empty workspace (empty
  // queue + empty panes), keeping the current layout.
  const addTab = () => {
    stopCurrentPlayers();
    const fresh: MediaTab = {
      id: generateTabId(),
      title: 'New Tab',
      queue: [],
      mediaItems: [null, null, null, null],
      paneStates: emptyPaneStates(),
      layout,
      activePane: 0,
      playedItems: [],
    };
    setMediaTabs(prev => [...snapshotActive(prev), fresh]);
    loadSession(fresh);
    setActiveTabId(fresh.id);
  };

  // Close a tab. Closing the active tab reveals a neighbour; the last tab can't
  // be closed (there's always one workspace).
  const removeTab = (id: string) => {
    if (mediaTabs.length <= 1) return;
    const idx = mediaTabs.findIndex(t => t.id === id);
    const filtered = mediaTabs.filter(t => t.id !== id);
    if (id === activeTabId) {
      stopCurrentPlayers();
      const next = filtered[Math.min(idx, filtered.length - 1)];
      setMediaTabs(filtered);
      loadSession(next);
      setActiveTabId(next.id);
    } else {
      // Background tab: keep the active tab's snapshot fresh, just drop the one.
      setMediaTabs(snapshotActive(filtered));
    }
  };

  // Media password state (separate from bookmarks)
  const [showPasswordSetupModal, setShowPasswordSetupModal] = useState(false);
  const [newPassword, setNewPassword] = useState('');
  const [confirmPassword, setConfirmPassword] = useState('');
  const [showMediaPassword, setShowMediaPassword] = useState(false);
  const [passwordError, setPasswordError] = useState('');
  const [isSettingPassword, setIsSettingPassword] = useState(false);
  // Access gate: when a media password is set, the tab is locked until it's
  // entered (per session). null = still checking.
  const [mediaLocked, setMediaLocked] = useState<boolean | null>(null);
  const [unlockInput, setUnlockInput] = useState('');
  const [unlockError, setUnlockError] = useState('');
  const [isUnlocking, setIsUnlocking] = useState(false);

  // Toggle fullscreen mode using Fullscreen API
  const toggleFullscreen = useCallback(async () => {
    try {
      if (!document.fullscreenElement) {
        // Enter fullscreen
        const container = containerRef.current;
        if (container) {
          await container.requestFullscreen();
        }
      } else {
        // Exit fullscreen
        await document.exitFullscreen();
      }
    } catch (err) {
      console.error('Fullscreen error:', err);
      // Fallback to CSS-based fullscreen
      const newState = !isFullscreen;
      setIsFullscreen(newState);
      onFullscreenChange?.(newState);
    }
  }, [isFullscreen, onFullscreenChange]);

  // Sync fullscreen state with Fullscreen API
  useEffect(() => {
    const handleFullscreenChange = () => {
      const isNowFullscreen = !!document.fullscreenElement;
      setIsFullscreen(isNowFullscreen);
      onFullscreenChange?.(isNowFullscreen);
    };

    document.addEventListener('fullscreenchange', handleFullscreenChange);
    return () => document.removeEventListener('fullscreenchange', handleFullscreenChange);
  }, [onFullscreenChange]);

  // Escape must always leave fullscreen. For VIDEO the native X11 surface renders
  // above the DOM and we fall back to CSS-based fullscreen (no real document
  // fullscreen), so the browser's built-in Escape does nothing — handle it
  // explicitly here, covering both the native and CSS paths.
  const isFullscreenRef = useRef(isFullscreen);
  isFullscreenRef.current = isFullscreen;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape' || !isFullscreenRef.current) return;
      e.preventDefault();
      if (document.fullscreenElement) {
        document.exitFullscreen().catch(() => {});
      }
      setIsFullscreen(false);
      onFullscreenChange?.(false);
    };
    // Capture phase so it wins even if a child stops propagation.
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [onFullscreenChange]);

  // Show the fullscreen header on movement. Hiding is owned by the unified idle
  // timer (markMediaActivity) so the two don't fight (which caused a header flicker
  // when the pointer crossed from the DOM chrome onto the native video surface).
  const handleFullscreenMouseMove = useCallback(() => {
    if (!isFullscreen) return;
    setShowFullscreenHeader(true);
  }, [isFullscreen]);

  // Cleanup fullscreen header timeout
  useEffect(() => {
    return () => {
      if (fullscreenHeaderTimeoutRef.current) {
        clearTimeout(fullscreenHeaderTimeoutRef.current);
      }
    };
  }, []);

  // Reset header visibility when entering/exiting fullscreen
  useEffect(() => {
    if (isFullscreen) {
      setShowFullscreenHeader(true);
      handleFullscreenMouseMove();
    }
  }, [isFullscreen, handleFullscreenMouseMove]);

  // --- Idle auto-hide: hide the floating media controls + the mouse cursor when
  // the pointer hasn't moved for a few seconds while media is loaded. Activity is
  // any mouse move / click / key / wheel anywhere in the window, plus pointer
  // motion forwarded from the native video surface (which sits above the DOM, so
  // DOM mousemove never fires over the actual video). ---
  const [controlsIdle, setControlsIdle] = useState(false);
  const idleTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const controlsIdleRef = useRef(false);
  useEffect(() => { controlsIdleRef.current = controlsIdle; }, [controlsIdle]);

  const markMediaActivity = useCallback(() => {
    // Wake up: show cursor + controls + fullscreen header again.
    if (controlsIdleRef.current) {
      setControlsIdle(false);
      if (isTauri()) {
        // Reveal the cursor over the native video surfaces (CSS can't reach them).
        invoke('set_video_surfaces_cursor_hidden', { window: winLabelRef.current, hidden: false }).catch(() => {});
        if (floatingControlsCreatedRef.current) {
          invoke('show_x11_webview_controls').catch(() => {});
        }
      }
    }
    setShowFullscreenHeader(true);
    if (idleTimerRef.current) clearTimeout(idleTimerRef.current);
    idleTimerRef.current = setTimeout(() => {
      // Only auto-hide when there's actually media to control.
      const hasMedia = mediaItemsRef.current.some(Boolean) || queueRef.current.length > 0;
      if (!hasMedia) return;
      setControlsIdle(true);
      setShowFullscreenHeader(false);
      if (isTauri()) {
        // Hide the cursor over the native video surfaces too (CSS can't reach them).
        invoke('set_video_surfaces_cursor_hidden', { window: winLabelRef.current, hidden: true }).catch(() => {});
        if (floatingControlsCreatedRef.current) {
          invoke('hide_x11_webview_controls').catch(() => {});
        }
      }
    }, 3000);
  }, []);
  markMediaActivityRef.current = markMediaActivity;

  useEffect(() => {
    const onActivity = () => markMediaActivity();
    window.addEventListener('mousemove', onActivity);
    window.addEventListener('mousedown', onActivity);
    window.addEventListener('keydown', onActivity);
    window.addEventListener('wheel', onActivity, { passive: true });

    // Pointer motion forwarded from the native video surface (Tauri event). The
    // payload is the namespaced player id; only count motion over OUR window's
    // surfaces so another window's video doesn't keep this window's controls awake.
    let unlistenMotion: (() => void) | undefined;
    if (isTauri()) {
      import('@tauri-apps/api/event').then(({ listen }) => {
        listen<string>('video-surface-motion', (e) => {
          if ((e.payload || '').startsWith(`${winLabelRef.current}::`)) onActivity();
        }).then((un) => { unlistenMotion = un; });
      });
    }

    markMediaActivity(); // arm the timer on mount
    return () => {
      window.removeEventListener('mousemove', onActivity);
      window.removeEventListener('mousedown', onActivity);
      window.removeEventListener('keydown', onActivity);
      window.removeEventListener('wheel', onActivity);
      if (idleTimerRef.current) clearTimeout(idleTimerRef.current);
      unlistenMotion?.();
    };
  }, [markMediaActivity]);

  // (Playback control handlers were removed — unused; VideoPlayer handles its
  // own controls. Re-add wired versions here if a global control bar returns.)

  // Handle player status update from GStreamerVideoPlayer (for stacked controls)
  const handlePlayerStatusChange = useCallback((status: PlayerStatusExport) => {
    setPlayerStatuses(prev => ({
      ...prev,
      [status.playerId]: status,
    }));
  }, []);

  // Generate unique ID for queue items
  const generateQueueId = () => `${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;

  // Add items to queue
  const addToQueue = useCallback((items: Array<{ source: string; type?: MediaType; title?: string }>) => {
    const newItems: QueueItem[] = items.map(item => ({
      id: generateQueueId(),
      source: item.source,
      type: item.type || detectMediaType(item.source),
      title: item.title || item.source.split('/').pop(),
      played: false,
    }));
    setQueue(prev => [...prev, ...newItems]);
    return newItems;
  }, []);

  // Move a queue item from one position to another (drag-to-reorder).
  const reorderQueue = useCallback((from: number, to: number) => {
    setQueue(prev => {
      if (from === to || from < 0 || to < 0 || from >= prev.length || to >= prev.length) return prev;
      const updated = [...prev];
      const [moved] = updated.splice(from, 1);
      updated.splice(to, 0, moved);
      return updated;
    });
  }, []);

  // Get max panes for current layout
  const getMaxPanesForLayout = useCallback((l: ViewLayout): number => {
    switch (l) {
      case 'horizontal':
      case 'vertical':
        return 2;
      case 'quad':
        return 4;
      default:
        return 1;
    }
  }, []);

  // Change layout and, when it exposes more panes, fill the newly-visible empty
  // panes from the queue (unplayed items not already showing). This is why
  // switching single -> quad now populates panes 2..N instead of leaving them empty.
  const changeLayout = useCallback((l: ViewLayout) => {
    setLayout(l);
    const maxPanes = getMaxPanesForLayout(l);
    const newItems = [...mediaItemsRef.current];
    const showing = new Set(newItems.map(m => m?.source).filter(Boolean) as string[]);
    const candidates = queue.filter(it => !playedItems.has(it.id) && !showing.has(it.source));
    let ci = 0;
    const updates: { index: number; item: QueueItem }[] = [];
    for (let i = 0; i < maxPanes; i++) {
      if (newItems[i] == null && ci < candidates.length) {
        const item = candidates[ci++];
        newItems[i] = { source: item.source, type: item.type, title: item.title };
        updates.push({ index: i, item });
      }
    }
    if (updates.length > 0) {
      setMediaItems(newItems);
      setPaneStates(prev => {
        const u = [...prev];
        updates.forEach(({ index, item }) => { u[index] = { ...u[index], currentItem: item, isPlaying: true }; });
        return u;
      });
    }
  }, [getMaxPanesForLayout, queue, playedItems]);


  // Advance a single pane to the next video when its clip ends (auto, via EOS) or
  // when the user hits Next (manual). Reads everything from refs so it always sees
  // the CURRENT queue / played-set / pane contents — earlier versions closed over
  // stale state and a hard "not playing elsewhere" filter, which left panes (and the
  // Next button) stuck after a couple of advances. Selection rules, in priority order:
  //   1. an unplayed clip not already showing in another pane (ideal — no dupes)
  //   2. if unplayed clips remain but are all on screen, advance to one anyway so the
  //      pane never freezes (queue smaller than pane count)
  //   3. if everything has played: repeat-all restarts the cycle; repeat-none stops
  // Shuffle picks randomly from the chosen pool; otherwise it walks queue order.
  const handlePaneVideoEnded = useCallback((paneIndex: number, opts?: { manual?: boolean }) => {
    const q = queueRef.current;
    if (q.length === 0) return;

    const { isShuffled, repeatMode } = playbackStateRef.current;
    const current = paneStatesRef.current[paneIndex]?.currentItem
      ?? (mediaItemsRef.current[paneIndex]
        ? (q.find(it => it.source === mediaItemsRef.current[paneIndex]?.source) ?? null)
        : null);
    const curSrc = current?.source;

    // Repeat-one: auto-EOS replays the same clip; a manual Next still advances.
    if (repeatMode === 'one' && !opts?.manual) {
      window.dispatchEvent(new CustomEvent(`media-seek-pane-${paneIndex}`, { detail: { time: 0 } }));
      window.dispatchEvent(new CustomEvent(`media-play-pane-${paneIndex}`));
      return;
    }

    // Mark the finished clip as played for this cycle.
    const played = new Set(playedItemsRef.current);
    if (current) played.add(current.id);

    // Sources currently on the OTHER panes (avoid showing the same clip twice).
    const showingElsewhere = new Set(
      paneStatesRef.current
        .map((p, i) => (i !== paneIndex ? p?.currentItem?.source : null))
        .filter(Boolean) as string[]
    );

    const pick = (pool: QueueItem[]) =>
      isShuffled ? pool[Math.floor(Math.random() * pool.length)] : pool[0];

    const unplayed = q.filter(it => !played.has(it.id));
    let chosen: QueueItem | undefined;
    let cycleReset = false;

    const offScreenUnplayed = unplayed.filter(it => !showingElsewhere.has(it.source));
    if (offScreenUnplayed.length > 0) {
      // 1. ideal pick
      chosen = pick(offScreenUnplayed);
    } else if (unplayed.length > 0) {
      // 2. unplayed remain but all on screen — advance anyway (prefer not self)
      const notSelf = unplayed.filter(it => it.source !== curSrc);
      chosen = pick(notSelf.length > 0 ? notSelf : unplayed);
    } else if (repeatMode === 'all') {
      // 3. exhausted + repeat-all → restart the cycle
      cycleReset = true;
      let pool = q.filter(it => !showingElsewhere.has(it.source) && it.source !== curSrc);
      if (pool.length === 0) pool = q.filter(it => it.source !== curSrc);
      if (pool.length === 0) pool = q; // single-item queue: replay it
      chosen = pick(pool);
    }
    // else: repeat-none and nothing left — leave the pane on its last frame.

    if (!chosen) {
      // Still record the finished clip so the played count stays accurate.
      if (current) setPlayedItems(played);
      return;
    }

    // Commit: mark the chosen clip played for this cycle and move it into the pane.
    const nextPlayed = cycleReset ? new Set<string>([chosen.id]) : (played.add(chosen.id), played);
    setPlayedItems(nextPlayed);
    playedItemsRef.current = nextPlayed;

    const picked = chosen;
    setMediaItems(prev => {
      const u = [...prev];
      u[paneIndex] = { source: picked.source, type: picked.type, title: picked.title };
      return u;
    });
    setPaneStates(prev => {
      const u = [...prev];
      u[paneIndex] = { ...u[paneIndex], currentItem: picked, isPlaying: true };
      return u;
    });
  }, []);

  // Listen for video ended events from each pane
  useEffect(() => {
    const handlers: Array<(e: Event) => void> = [];

    // Create handlers for each pane (0-3 for quad layout support)
    for (let i = 0; i < 4; i++) {
      const handler = (e: Event) =>
        handlePaneVideoEnded(i, { manual: !!(e as CustomEvent).detail?.manual });
      handlers.push(handler);
      window.addEventListener(`media-ended-pane-${i}`, handler);
    }

    return () => {
      for (let i = 0; i < 4; i++) {
        window.removeEventListener(`media-ended-pane-${i}`, handlers[i]);
      }
    };
  }, [handlePaneVideoEnded]);

  // Flow image items into the photo/empty panes, one per pane. Panes currently
  // playing VIDEO/AUDIO are left untouched (they advance on their own via
  // handlePaneVideoEnded), so photos and videos can auto-play side by side.
  const applyImagesToPanes = useCallback((items: (QueueItem | undefined)[]) => {
    const maxPanes = getMaxPanesForLayout(layout);
    setMediaItems(prev => {
      const updated = [...prev];
      let ii = 0;
      for (let p = 0; p < maxPanes; p++) {
        const cur = prev[p];
        if (cur && (cur.type === 'video' || cur.type === 'audio')) continue;
        const it = items[ii++];
        updated[p] = it ? { source: it.source, type: it.type, title: it.title } : null;
      }
      return updated;
    });
    setPaneStates(prev => {
      const updated = [...prev];
      let ii = 0;
      for (let p = 0; p < maxPanes; p++) {
        const cur = prev[p]?.currentItem;
        if (cur && (cur.type === 'video' || cur.type === 'audio')) continue;
        const it = items[ii++];
        updated[p] = { ...updated[p], currentItem: it ?? null, isPlaying: false };
      }
      return updated;
    });
  }, [layout, getMaxPanesForLayout]);

  // Slideshow tick: rotate the panes to the next set of distinct photos.
  // Guarantees never two of the same photo on screen at once; 'shuffle' reshuffles
  // on each full pass ("changes every repeat"); 'consecutive' walks queue order.
  const advanceSlideshow = useCallback(() => {
    const maxPanes = getMaxPanesForLayout(layout);
    const imageItems = queue.filter(item => item.type === 'image');
    if (imageItems.length === 0) return;

    // Staggered: change exactly one pane per tick, round-robin (pane 0, 1, 2, 3, 0...),
    // to a random photo not currently shown in any pane.
    if (slideshow.mode === 'staggered') {
      const pane = staggerPaneRef.current % maxPanes;
      const current = mediaItemsRef.current;
      const shownElsewhere = new Set(
        current.map((m, i) => (i !== pane && m ? m.source : null)).filter(Boolean) as string[]
      );
      // Prefer a photo not on screen at all (incl. not the one already in this pane).
      let pool = imageItems.filter(it => !shownElsewhere.has(it.source) && it.source !== current[pane]?.source);
      if (pool.length === 0) pool = imageItems.filter(it => !shownElsewhere.has(it.source));
      if (pool.length === 0) pool = imageItems;
      // Shuffle toggle decides random vs. next-in-queue-order.
      const pick = playbackState.isShuffled ? pool[Math.floor(Math.random() * pool.length)] : pool[0];
      setMediaItems(prev => {
        const u = [...prev];
        u[pane] = { source: pick.source, type: pick.type, title: pick.title };
        return u;
      });
      setPaneStates(prev => {
        const u = [...prev];
        u[pane] = { ...u[pane], currentItem: pick, isPlaying: false };
        return u;
      });
      staggerPaneRef.current = (pane + 1) % maxPanes;
      return;
    }

    const byId = new Map(imageItems.map(it => [it.id, it] as const));
    const imageIds = imageItems.map(it => it.id);
    const len = imageIds.length;

    let { order, cursor } = slideshowOrderRef.current;
    // (Re)build the ordering if the image set changed or it's uninitialised.
    const sameSet = order.length === len && order.every(id => byId.has(id));
    if (!sameSet) {
      order = playbackState.isShuffled ? shuffleArray(imageIds) : [...imageIds];
      cursor = 0;
    }

    // Take N distinct, consecutive entries from the ordering (wrapping). Since
    // len >= count, N consecutive (mod len) entries are always distinct.
    const count = Math.min(maxPanes, len);
    const selected: (QueueItem | undefined)[] = [];
    for (let i = 0; i < count; i++) {
      selected.push(byId.get(order[(cursor + i) % len]));
    }

    // Advance; on completing a full pass, reshuffle (when shuffle is on) for the next pass.
    let nextCursor = cursor + count;
    if (nextCursor >= len) {
      nextCursor = nextCursor % len;
      if (playbackState.isShuffled) {
        order = shuffleArray(imageIds);
      }
    }
    slideshowOrderRef.current = { order, cursor: nextCursor };

    applyImagesToPanes(selected);
  }, [layout, queue, slideshow.mode, playbackState.isShuffled, getMaxPanesForLayout, applyImagesToPanes]);

  // Auto-load the first N photos into the panes when switching to a multi-pane
  // layout (photos only). Fills empty panes so a video the user placed isn't
  // disrupted; slideshow rotation (below) takes over from here.
  useEffect(() => {
    if (layout === 'single') return;
    if (slideshow.enabled) return; // slideshow manages panes itself
    const maxPanes = getMaxPanesForLayout(layout);
    const imageItems = queue.filter(item => item.type === 'image');
    if (imageItems.length === 0) return;
    setMediaItems(prev => {
      // Only place images that aren't already on screen. This makes the effect fill
      // empty panes when NEW images are added, but NOT resurrect a pane the user just
      // cleared by removing its item from the queue (the remaining images are already
      // shown elsewhere, so nothing gets pulled in) — and it never duplicates an image.
      const shown = new Set(prev.map(m => m?.source).filter(Boolean) as string[]);
      const candidates = imageItems.filter(it => !shown.has(it.source));
      if (candidates.length === 0) return prev;
      const updated = [...prev];
      let imgIdx = 0;
      for (let p = 0; p < maxPanes && imgIdx < candidates.length; p++) {
        if (!updated[p]) {
          const it = candidates[imgIdx++];
          updated[p] = { source: it.source, type: it.type, title: it.title };
        }
      }
      return updated;
    });
  }, [layout, queue, slideshow.enabled, getMaxPanesForLayout]);

  // Slideshow timer: repeating tick while enabled and there are photos.
  useEffect(() => {
    if (!slideshow.enabled) return;
    if (!queue.some(item => item.type === 'image')) return;
    const id = setInterval(advanceSlideshow, Math.max(1, slideshow.interval) * 1000);
    return () => clearInterval(id);
  }, [slideshow.enabled, slideshow.interval, advanceSlideshow, queue]);

  // Toggle slideshow
  const toggleSlideshow = useCallback(() => {
    setSlideshow(prev => {
      // Starting: rebuild the ordering so it begins fresh from the current photos.
      if (!prev.enabled) {
        slideshowOrderRef.current = { order: [], cursor: 0 };
        staggerPaneRef.current = 0;
      }
      return { ...prev, enabled: !prev.enabled };
    });
  }, []);

  // Set slideshow interval
  const setSlideshowInterval = useCallback((interval: number) => {
    setSlideshow(prev => ({ ...prev, interval: Math.max(1, interval) }));
  }, []);

  // Set slideshow mode (rebuild the ordering so the new mode takes effect now)
  const setSlideshowMode = useCallback((mode: SlideshowMode) => {
    slideshowOrderRef.current = { order: [], cursor: 0 };
    staggerPaneRef.current = 0;
    setSlideshow(prev => ({ ...prev, mode }));
  }, []);

  // Staggered only makes sense with multiple panes; fall back to consecutive in single.
  useEffect(() => {
    if (layout === 'single' && slideshow.mode === 'staggered') {
      setSlideshowMode('consecutive');
    }
  }, [layout, slideshow.mode, setSlideshowMode]);

  // Initialize queue from files (for "Open with" functionality)
  const initializeFromFiles = useCallback((files: Array<{ source: string; title?: string }>, startPaneIndex: number = 0) => {
    const newItems = addToQueue(files);
    if (newItems.length === 0) return;
    const maxPanes = getMaxPanesForLayout(layout);
    const current = mediaItemsRef.current;

    // New media only fills EMPTY panes. If the relevant panes already hold
    // content, the items just stay queued (and autoplay into a pane as each video
    // there finishes — media-ended-pane-* -> assignNextToPaneIndex). A batch fills
    // the empty panes left-to-right (pane 1..N); a single file prefers the
    // targeted (dropped-on) pane when it's empty.
    let targetPanes: number[];
    if (newItems.length === 1) {
      const t = Math.max(0, Math.min(startPaneIndex, maxPanes - 1));
      targetPanes = current[t] == null ? [t] : [];
    } else {
      targetPanes = [];
      for (let i = 0; i < maxPanes; i++) if (current[i] == null) targetPanes.push(i);
    }

    // Pair each empty pane with the next new item (extras stay in the queue).
    const assignments = targetPanes
      .map((paneIndex, k) => ({ paneIndex, item: newItems[k] }))
      .filter(a => a.item);
    if (assignments.length === 0) return;

    // Apply all assignments in one batched update so the 4 panes mount together
    // with the final quad layout already settled (stable surface bounds).
    setPaneStates(prev => {
      const updated = [...prev];
      assignments.forEach(({ paneIndex, item }) => {
        updated[paneIndex] = { ...updated[paneIndex], currentItem: item, isPlaying: true };
      });
      return updated;
    });
    setMediaItems(prev => {
      const updated = [...prev];
      assignments.forEach(({ paneIndex, item }) => {
        updated[paneIndex] = { source: item.source, type: item.type, title: item.title };
      });
      return updated;
    });
  }, [addToQueue, layout, getMaxPanesForLayout]);

  // Expand a list of dropped OS paths into media files. A dropped directory is
  // read one level deep (its own files only, no subfolders) and its media files
  // are returned sorted by name; a dropped file passes through unchanged. This
  // is why a folder of pictures no longer loads as a single (mis-detected) video.
  const expandDroppedPaths = useCallback(async (paths: string[]): Promise<Array<{ source: string; title?: string }>> => {
    const { stat, readDir } = await import('@tauri-apps/plugin-fs');
    const out: Array<{ source: string; title?: string }> = [];

    for (const path of paths) {
      let isDir = false;
      try {
        isDir = (await stat(path)).isDirectory;
      } catch {
        // If we can't stat it, treat it as a plain file path below.
      }

      if (isDir) {
        try {
          const sep = path.includes('\\') ? '\\' : '/';
          const base = path.endsWith(sep) ? path.slice(0, -1) : path;
          const entries = await readDir(path); // top level only — never recurses
          entries
            .filter(e => e.isFile && isMediaFile(e.name))
            .sort((a, b) => a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: 'base' }))
            .forEach(e => out.push({ source: `file://${base}${sep}${e.name}`, title: e.name }));
        } catch (err) {
          console.error('Failed to read dropped folder:', path, err);
        }
      } else {
        out.push({
          source: path.startsWith('file://') ? path : `file://${path}`,
          title: path.split('/').pop() || path.split('\\').pop(),
        });
      }
    }

    return out;
  }, []);

  // Hit-test a viewport (client) point against each visible pane's bounds and
  // return the pane index under it, or -1 if none. Used to route OS file drops
  // to the pane the cursor is actually over.
  const hitTestPane = useCallback((clientX: number, clientY: number): number => {
    const maxPanes = getMaxPanesForLayout(layout);
    for (let i = 0; i < maxPanes; i++) {
      const el = paneRefs.current[i];
      if (!el) continue;
      const r = el.getBoundingClientRect();
      if (clientX >= r.left && clientX <= r.right && clientY >= r.top && clientY <= r.bottom) {
        return i;
      }
    }
    return -1;
  }, [layout, getMaxPanesForLayout]);

  // Listen for playback updates from VideoPlayer
  useEffect(() => {
    const handleTimeUpdate = (e: CustomEvent) => {
      setPlaybackState(s => ({
        ...s,
        currentTime: e.detail.currentTime,
        duration: e.detail.duration,
        isPlaying: e.detail.isPlaying,
      }));
    };

    window.addEventListener('media-timeupdate' as any, handleTimeUpdate);
    return () => window.removeEventListener('media-timeupdate' as any, handleTimeUpdate);
  }, []);

  // Listen for double-click fullscreen toggle from VideoPlayer
  useEffect(() => {
    const handleToggleFullscreen = () => {
      toggleFullscreen();
    };

    window.addEventListener('media-toggle-fullscreen', handleToggleFullscreen);
    return () => window.removeEventListener('media-toggle-fullscreen', handleToggleFullscreen);
  }, [isFullscreen, onFullscreenChange]);

  // Listen for Tauri file-open events ("Open with Reclaim")
  useEffect(() => {
    // Guard against the async-listener leak: if this effect is torn down (React
    // StrictMode double-mount, or a deps change) before `await listen` resolves,
    // immediately unlisten the resolved handler instead of leaking it — leaking
    // caused drag-drop to register twice and add every file twice.
    let cancelled = false;
    const unlisteners: Array<() => void> = [];
    const track = (u: () => void) => { if (cancelled) u(); else unlisteners.push(u); };

    const setupFileOpenListener = async () => {
      try {
        // Listen for file association events (when opening files with "Open with")
        const unlistenOpen = await listen<string[]>('tauri://file-open', (event) => {
          if (event.payload && event.payload.length > 0) {
            const files = event.payload.map(path => ({
              source: path.startsWith('file://') ? path : `file://${path}`,
              title: path.split('/').pop() || path.split('\\').pop(),
            }));
            initializeFromFiles(files);
          }
        });

        // Also listen for custom event from Rust backend for cli args
        const unlistenCliFiles = await listen<string[]>('open-files', (event) => {
          if (event.payload && event.payload.length > 0) {
            const files = event.payload.map(path => ({
              source: path.startsWith('file://') ? path : `file://${path}`,
              title: path.split('/').pop() || path.split('\\').pop(),
            }));
            initializeFromFiles(files);
          }
        });

        track(unlistenOpen);
        track(unlistenCliFiles);
      } catch (err) {
        console.error('Failed to setup file open listeners:', err);
      }
    };

    setupFileOpenListener();

    return () => {
      cancelled = true;
      unlisteners.forEach(u => u());
    };
  }, [initializeFromFiles]);

  // OS file drag/drop arrives from the BACKEND as a DOM CustomEvent eval'd straight
  // into THIS window's webview (see wire_media_drag_drop in lib.rs). We do NOT use
  // Tauri's event bus or webview drag-drop APIs: for code-created windows the JS
  // identity reports label "main", and a broadcast `app.emit` never reached a 2nd
  // window's `listen(...)` (the JS-listener registry is keyed by the listening
  // webview's label). The backend holds the exact webview handle, so an eval'd
  // CustomEvent lands in the right window with no routing.
  //
  // Registered ONCE for the component's lifetime and delegated to refs (updated every
  // render, below) so the latest hitTestPane/expandDroppedPaths/initializeFromFiles
  // are used without re-registering — re-registering on a multi-dep async effect
  // accumulated listeners and fired every drop 2-3×.
  const dragOverHandlerRef = useRef<(d: { x: number; y: number }) => void>(() => {});
  const dropHandlerRef = useRef<(d: { paths: string[]; x: number; y: number }) => void>(() => {});
  // Tauri reports native drag positions in *physical* pixels relative to the webview's
  // top-left; getBoundingClientRect() is in CSS (client) pixels — divide by DPR.
  const toClientPoint = (x: number, y: number) => {
    const dpr = window.devicePixelRatio || 1;
    return { x: x / dpr, y: y / dpr };
  };
  dragOverHandlerRef.current = (d) => {
    const point = toClientPoint(d.x, d.y);
    const pane = hitTestPane(point.x, point.y);
    setOsDropPane(pane >= 0 ? pane : null);
  };
  dropHandlerRef.current = (d) => {
    setOsDropPane(null);
    const paths = d?.paths;
    if (!paths || paths.length === 0) return;
    // Route the drop to the pane under the cursor; fall back to the active pane when
    // the point matches none (e.g. dropped on chrome).
    const point = toClientPoint(d.x, d.y);
    const targetPane = hitTestPane(point.x, point.y);
    const startPane = targetPane >= 0 ? targetPane : activePaneRef.current;
    // Dropped folders are expanded into their (top-level) media files.
    expandDroppedPaths(paths).then(files => {
      if (files.length > 0) initializeFromFiles(files, startPane);
    });
  };
  useEffect(() => {
    if (!isTauri()) return;
    const onOver = (e: Event) => dragOverHandlerRef.current((e as CustomEvent).detail);
    const onLeave = () => setOsDropPane(null);
    const onDrop = (e: Event) => dropHandlerRef.current((e as CustomEvent).detail);
    window.addEventListener('__earth_media_drag_over', onOver);
    window.addEventListener('__earth_media_drag_leave', onLeave);
    window.addEventListener('__earth_media_drop', onDrop);
    return () => {
      window.removeEventListener('__earth_media_drag_over', onOver);
      window.removeEventListener('__earth_media_drag_leave', onLeave);
      window.removeEventListener('__earth_media_drop', onDrop);
    };
  }, []);

  // Native video surface clicks. The X11 video window renders above the DOM, so the
  // backend eval's a `__earth_video_surface_clicked` DOM event into THIS window's
  // webview (a broadcast Tauri event never reaches a 2nd window). Two-stage: the first
  // click on an unfocused pane just FOCUSES it (so the floating controls target it);
  // clicking the already-focused pane toggles its play/pause.
  useEffect(() => {
    if (!isTauri()) return;
    const onSurfaceClick = (e: Event) => {
      const pid = ((e as CustomEvent).detail as { playerId: string }).playerId;
      // Eval already targeted this window, but guard anyway.
      if (!pid || !pid.startsWith(`${winLabelRef.current}::`)) return;
      const idx = paneIndexOf(pid);
      const alreadyActive = activePaneRef.current === idx;
      setActivePane(idx);
      setActivePlayerId(pid);
      // Re-claim the shared controls for THIS pane on EVERY click. The claim effect
      // only fires when activePlayerId changes, so clicking back to a window whose
      // video was already its focused pane (e.g. after another window grabbed the
      // controls) wouldn't otherwise switch the controls back. A native surface click
      // always means this pane has a video, so claim it directly.
      invoke('set_active_media_player', { playerId: pid }).catch(() => {});
      // Only toggle play/pause when this pane was already the focused one — a
      // focus click shouldn't also start/stop the video.
      if (alreadyActive) {
        window.dispatchEvent(new CustomEvent('media-playpause-player', { detail: { playerId: pid } }));
      }
    };
    window.addEventListener('__earth_video_surface_clicked', onSurfaceClick);
    return () => window.removeEventListener('__earth_video_surface_clicked', onSurfaceClick);
  }, []);

  // Hide the browser (web page) native surface while the Media tab is mounted, so
  // the page you were on doesn't bleed through behind the media UI. The native
  // WebKit surface renders above the DOM, so it must be explicitly unmapped here
  // (belt-and-suspenders alongside App's own show/hide on service change).
  // ONLY the main window owns that (single, global) browser surface — a secondary
  // window must not hide it or it would blank the main window's page.
  useEffect(() => {
    if (!isTauri() || !winReady || winLabelRef.current !== 'main') return;
    invoke('browser_surface_hide').catch(() => {});
  }, [winReady]);

  // Load initial data
  useEffect(() => {
    loadPrivacySettings();
    loadPlaylists();

    if (initialSource) {
      const type = initialType || detectMediaType(initialSource);
      setMediaItems([{ source: initialSource, type }, null, null, null]);
    }
  }, [profileId, initialSource, initialType]);

  // Load privacy settings
  const loadPrivacySettings = async () => {
    try {
      const settings = await invoke<PrivacySettings>('get_media_privacy_settings', { profileId });
      setPrivacySettings(settings);
      // Access gate: if a password is set and protection is on, lock the tab until
      // it's entered this session. This is enforced regardless of incognito.
      const gated = !!settings.password_hash && settings.require_password;
      setMediaLocked(gated && !mediaUnlockedProfiles.has(profileId));
      // Show the password setup modal if no password is set — but only ONCE per
      // session. It's a full-screen overlay; re-prompting on every return to the
      // Media tab blacks out and blocks the UI (the dialog hides behind the
      // native video surfaces).
      if (!settings.password_hash && !mediaPasswordPrompted) {
        mediaPasswordPrompted = true;
        setShowPasswordSetupModal(true);
      }
    } catch (err) {
      console.error('Failed to load privacy settings:', err);
      setMediaLocked(false); // don't trap the user if settings can't be read
    }
  };

  // Verify the media password and unlock the tab for this session.
  const handleUnlockMedia = async () => {
    setUnlockError('');
    if (!unlockInput) { setUnlockError('Enter your media password'); return; }
    setIsUnlocking(true);
    try {
      const ok = await invoke<boolean>('verify_media_password', { profileId, password: unlockInput });
      if (ok) {
        mediaUnlockedProfiles.add(profileId);
        setMediaLocked(false);
        setUnlockInput('');
      } else {
        setUnlockError('Incorrect password');
      }
    } catch (err) {
      console.error('Failed to verify media password:', err);
      setUnlockError('Could not verify password');
    } finally {
      setIsUnlocking(false);
    }
  };

  // Set media password
  const handleSetPassword = async () => {
    setPasswordError('');

    if (!newPassword) {
      setPasswordError('Please enter a password');
      return;
    }

    if (newPassword.length < 4) {
      setPasswordError('Password must be at least 4 characters');
      return;
    }

    if (newPassword !== confirmPassword) {
      setPasswordError('Passwords do not match');
      return;
    }

    setIsSettingPassword(true);
    try {
      await invoke('set_media_password', {
        profileId: profileId,
        password: newPassword,
      });
      // Reload settings to reflect the change
      await loadPrivacySettings();
      setShowPasswordSetupModal(false);
      setNewPassword('');
      setConfirmPassword('');
    } catch (err) {
      console.error('Failed to set password:', err);
      setPasswordError('Failed to set password. Please try again.');
    } finally {
      setIsSettingPassword(false);
    }
  };

  // Skip password setup (allow using without password)
  const handleSkipPassword = () => {
    setShowPasswordSetupModal(false);
  };

  // Load playlists
  const loadPlaylists = async () => {
    try {
      const lists = await invoke<Playlist[]>('get_media_playlists', { profileId: profileId });
      setPlaylists(lists);
    } catch (err) {
      console.error('Failed to load playlists:', err);
    }
  };

  // Close media in the specified pane
  const closeMedia = useCallback((paneIndex: number) => {
    // Explicit close: stop & free the player so it doesn't keep playing audio in
    // the background. Only HIDE the surface — do NOT destroy it: destroying the
    // X11 window here races the player teardown / unmount and triggers a fatal
    // X11 BadWindow error that crashes the whole app. Hidden surfaces are reused
    // when the pane is reopened.
    const playerId = panePid(paneIndex);
    invoke('player_stop', { playerId }).catch(() => {});
    invoke('player_remove', { playerId }).catch(() => {});
    invoke('hide_video_surface', { playerId }).catch(() => {});

    // Functional updates (so several closeMedia calls in a loop don't clobber each
    // other) and clear BOTH mediaItems AND paneStates for this pane — leaving
    // paneStates.currentItem set let the pane get re-populated.
    setMediaItems(prev => {
      const u = [...prev];
      u[paneIndex] = null;
      return u;
    });
    setPaneStates(prev => {
      const u = [...prev];
      u[paneIndex] = { ...u[paneIndex], currentItem: null, isPlaying: false };
      return u;
    });
  }, []);

  // Open media in active pane (no automatic tab creation)
  const openMedia = useCallback((source: string, type?: MediaType, title?: string) => {
    const mediaType = type || detectMediaType(source);
    const mediaItem: MediaItem = { source, type: mediaType, title };

    // Update the pane
    const newItems = [...mediaItems];
    newItems[activePane] = mediaItem;
    setMediaItems(newItems);
  }, [mediaItems, activePane]);

  // Listen for media-prev and media-next events (from ImageViewer arrow keys)
  useEffect(() => {
    const handlePrev = () => {
      // Find current index in queue and go to previous
      const currentSource = mediaItems[activePane]?.source;
      if (!currentSource || queue.length === 0) return;

      const currentIndex = queue.findIndex(item => item.source === currentSource);
      if (currentIndex > 0) {
        const prevItem = queue[currentIndex - 1];
        openMedia(prevItem.source, prevItem.type, prevItem.title);
      } else if (currentIndex === -1 && queue.length > 0) {
        // Current item not in queue, go to last queue item
        const lastItem = queue[queue.length - 1];
        openMedia(lastItem.source, lastItem.type, lastItem.title);
      }
    };

    const handleNext = () => {
      // Find current index in queue and go to next
      const currentSource = mediaItems[activePane]?.source;
      if (!currentSource || queue.length === 0) return;

      const currentIndex = queue.findIndex(item => item.source === currentSource);
      if (currentIndex >= 0 && currentIndex < queue.length - 1) {
        const nextItem = queue[currentIndex + 1];
        openMedia(nextItem.source, nextItem.type, nextItem.title);
      } else if (currentIndex === -1 && queue.length > 0) {
        // Current item not in queue, go to first queue item
        const firstItem = queue[0];
        openMedia(firstItem.source, firstItem.type, firstItem.title);
      }
    };

    window.addEventListener('media-prev', handlePrev);
    window.addEventListener('media-next', handleNext);
    return () => {
      window.removeEventListener('media-prev', handlePrev);
      window.removeEventListener('media-next', handleNext);
    };
  }, [queue, mediaItems, activePane, openMedia]);

  // Open file dialog - supports multiple file selection
  const openFile = async () => {
    // In Tauri, use the native file dialog to get actual file paths
    if (isTauri()) {
      try {
        // Import Tauri dialog API dynamically
        const { open } = await import('@tauri-apps/plugin-dialog');
        const selected = await open({
          multiple: true,
          filters: [{
            name: 'Media',
            extensions: ['mp4', 'webm', 'ogg', 'mov', 'avi', 'mkv', 'm4v', 'mp3', 'wav', 'flac', 'aac', 'ogg', 'jpg', 'jpeg', 'png', 'gif', 'webp', 'bmp', 'svg'],
          }],
        });

        if (selected) {
          // selected can be string or string[] depending on multiple flag
          const paths = Array.isArray(selected) ? selected : [selected];

          // Convert to file:// URIs for GStreamer
          const queueItems = paths.map(path => ({
            source: path.startsWith('file://') ? path : `file://${path}`,
            type: detectMediaType(path),
            title: path.split('/').pop() || path.split('\\').pop() || path,
          }));

          addToQueue(queueItems);

          // Open first file in active pane
          if (queueItems.length > 0) {
            openMedia(queueItems[0].source, queueItems[0].type, queueItems[0].title);
          }
        }
      } catch (err) {
        console.error('Failed to open file dialog:', err);
      }
      return;
    }

    // Fallback for browser mode - use HTML file input (blob URLs, limited functionality)
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = 'video/*,image/*,audio/*';
    input.multiple = true;
    input.onchange = (e) => {
      const files = (e.target as HTMLInputElement).files;
      if (files && files.length > 0) {
        const fileArray = Array.from(files);

        // Add all files to queue first (blob URLs - won't work with GStreamer)
        const queueItems = fileArray.map(file => ({
          source: URL.createObjectURL(file),
          type: detectMediaType(file.name),
          title: file.name,
        }));
        addToQueue(queueItems);

        // Open first file in active pane
        if (queueItems.length > 0) {
          openMedia(queueItems[0].source, queueItems[0].type, queueItems[0].title);
        }
      }
    };
    input.click();
  };

  // Open URL
  const openUrl = () => {
    if (urlInput.trim()) {
      openMedia(urlInput.trim());
      setUrlInput('');
    }
  };

  // Create playlist
  const createPlaylist = async () => {
    if (!newPlaylistName.trim()) return;
    try {
      await invoke('create_media_playlist', {
        profile_id: profileId,
        name: newPlaylistName,
        description: null,
        encrypted: false,
      });
      setNewPlaylistName('');
      loadPlaylists();
    } catch (err) {
      console.error('Failed to create playlist:', err);
    }
  };

  // Load playlist items
  const loadPlaylistItems = async (playlist: Playlist) => {
    try {
      const items = await invoke<PlaylistItem[]>('get_media_playlist_items', { playlist_id: playlist.id });
      setPlaylistItems(items);
      setCurrentPlaylist(playlist);
    } catch (err) {
      console.error('Failed to load playlist items:', err);
    }
  };

  // Add current media to playlist
  const addToPlaylist = async (playlistId: number) => {
    const media = mediaItems[activePane];
    if (!media) return;
    try {
      await invoke('add_to_media_playlist', {
        playlist_id: playlistId,
        source: media.source,
        media_type: media.type,
        title: media.title || null,
        thumbnail: null,
      });
      if (currentPlaylist?.id === playlistId) {
        loadPlaylistItems(currentPlaylist);
      }
      loadPlaylists();
    } catch (err) {
      console.error('Failed to add to playlist:', err);
    }
  };

  // Add a specific queue item to an existing playlist (filepath stored encrypted by the backend)
  const addQueueItemToPlaylist = async (playlistId: number, item: QueueItem) => {
    try {
      await invoke('add_to_media_playlist', {
        playlist_id: playlistId,
        source: item.source,
        media_type: item.type || 'video',
        title: item.title || null,
        thumbnail: null,
      });
      setAddToPlaylistMenuId(null);
      if (currentPlaylist?.id === playlistId) loadPlaylistItems(currentPlaylist);
      loadPlaylists();
    } catch (err) {
      console.error('Failed to add item to playlist:', err);
    }
  };

  // Create a new playlist and immediately add this queue item to it
  const createPlaylistWithItem = (item: QueueItem) => {
    setAddToPlaylistMenuId(null);
    setPlaylistNameInput('');
    setPlaylistNamePrompt({
      title: 'New playlist',
      onConfirm: async (name) => {
        try {
          const pl = await invoke<Playlist>('create_media_playlist', {
            profile_id: profileId,
            name,
            description: null,
            encrypted: false,
          });
          await addQueueItemToPlaylist(pl.id, item);
        } catch (err) {
          console.error('Failed to create playlist:', err);
        }
      },
    });
  };

  // Render media pane
  const renderPane = (index: number) => {
    const media = mediaItems[index];
    const isActive = activePane === index;
    // Highlight the pane an OS file drag is currently hovering (resolved via
    // geometric hit-testing in the Tauri drag-drop handler).
    const isDropTarget = osDropPane === index;

    return (
      <div
        ref={(el) => { paneRefs.current[index] = el; }}
        className={`relative flex-1 min-w-0 min-h-0 overflow-hidden bg-black/50 ${
          isActive && layout !== 'single' ? 'ring-2 ring-[var(--primary-color)]' : ''
        } ${layout !== 'single' && mediaItems.every(m => m === null) ? 'border border-gray-700/50' : ''} ${
          isDropTarget ? 'ring-2 ring-[var(--primary-color)] ring-dashed bg-[var(--primary-color)]/10' : ''
        }`}
        onClick={() => setActivePane(index)}
      >
        {media ? (
          media.type === 'video' || media.type === 'audio' ? (
            <GStreamerVideoPlayer
              key={`pane-${index}-${layout}-${activeTabId}`}
              source={media.source}
              title={media.title}
              playerId={panePid(index)}
              // Resume in the tab's saved play state at its saved position when
              // this pane is (re)mounted by a tab switch. Fresh media has
              // isPlaying=true / currentTime=0, so it just plays from the start.
              autoPlay={paneStates[index]?.isPlaying !== false}
              startPositionMs={Math.floor((paneStates[index]?.currentTime || 0) * 1000)}
              isActive={activePane === index}
              className="w-full h-full"
              hideControls={layout !== 'single'}  // Hide in multi-pane (StackedControls handles it)
              onStatusChange={handlePlayerStatusChange}
              onEnded={() => {
                // Dispatch event so the parent can handle queue advancement
                window.dispatchEvent(new CustomEvent(`media-ended-pane-${index}`));
              }}
            />
          ) : (
            <ImageViewer
              source={media.source}
              title={media.title}
              className="w-full h-full"
              // The pane indicator (multi-pane) and fullscreen header already show
              // the title — suppress the viewer's own overlay to avoid a duplicate.
              showTitle={false}
            />
          )
        ) : (
          <div className="flex flex-col items-center justify-center h-full w-full bg-gray-900/50 text-gray-400">
            <svg className="w-16 h-16 mb-4 opacity-60" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1} d="M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M5 18h8a2 2 0 002-2V8a2 2 0 00-2-2H5a2 2 0 00-2 2v8a2 2 0 002 2z" />
            </svg>
            <p className="text-sm font-medium">Drop media here</p>
            <p className="text-xs text-gray-500 mt-1">or use the toolbar to open files</p>
            <div className="mt-3 px-3 py-1 rounded bg-gray-800/50 text-xs text-gray-400">
              Pane {index + 1}
            </div>
          </div>
        )}

        {/* Pane indicator and tab actions */}
        {layout !== 'single' && (
          <div className="absolute top-2 left-2 flex items-center gap-1">
            <div className={`px-2 py-0.5 rounded text-xs font-medium ${
              isActive ? 'bg-[var(--primary-color)] text-white' : 'bg-black/50 text-gray-400'
            }`}>
              {index + 1}
            </div>
          </div>
        )}


        {/* Drop overlay indicator */}
        {isDropTarget && (
          <div className="absolute inset-0 flex items-center justify-center bg-[var(--primary-color)]/20 border-2 border-dashed border-[var(--primary-color)] pointer-events-none z-10">
            <div className="bg-black/70 rounded-lg px-4 py-2 text-white text-sm font-medium">
              Drop media on Pane {index + 1}
            </div>
          </div>
        )}
      </div>
    );
  };

  // Get max panes for current layout
  // Get current media title
  const currentMedia = mediaItems[activePane];
  const currentTitle = currentMedia?.title || (currentMedia?.source ? currentMedia.source.split('/').pop() : 'No media loaded');

  // While we're still checking the gate, render nothing heavy — this avoids
  // flashing the media UI/surfaces for a frame before a locked tab resolves.
  if (mediaLocked === null) {
    return <div className="h-full bg-[var(--background-color)]" />;
  }

  // Access gate: when the media password is set, lock the tab until it's entered.
  // Rendered as an early return so no media surfaces/players are mounted while
  // locked. Enforced in every privacy mode, including incognito.
  if (mediaLocked) {
    return (
      <div className="flex flex-col items-center justify-center h-full bg-[var(--background-color)] p-6">
        <svg className="w-12 h-12 text-[var(--primary-color)] mb-3" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
        </svg>
        <h1 className="text-xl font-bold text-white">Media is locked</h1>
        <p className="text-sm text-[var(--text-muted-color)] mt-1 mb-4 text-center max-w-xs">
          Enter your media password to access your media, history, and playlists.
        </p>
        <div className="w-72">
          <input
            type="password"
            autoFocus
            placeholder="Media password"
            value={unlockInput}
            onChange={(e) => setUnlockInput(e.target.value)}
            onKeyDown={(e) => { if (e.key === 'Enter') handleUnlockMedia(); }}
            className="w-full bg-gray-800 border border-gray-600 rounded px-3 py-2 text-sm text-white outline-none focus:border-[var(--primary-color)]"
          />
          {unlockError && <p className="text-red-400 text-xs mt-2">{unlockError}</p>}
          <button
            onClick={handleUnlockMedia}
            disabled={isUnlocking}
            className="mt-3 w-full px-3 py-2 text-sm rounded-lg bg-[var(--primary-color)] text-white hover:opacity-90 disabled:opacity-40"
          >
            {isUnlocking ? 'Unlocking…' : 'Unlock'}
          </button>
          <div className="mt-2 flex justify-center">
            <VaultAutofill profileId={profileId} appKey="media" onFill={(pw) => setUnlockInput(pw)} />
          </div>
        </div>
      </div>
    );
  }

  return (
    <div
      ref={containerRef}
      className={`flex flex-col h-full ${isFullscreen ? 'bg-black' : 'bg-[var(--background-color)]'} ${controlsIdle ? 'media-cursor-idle' : ''}`}
      onMouseMove={isFullscreen ? handleFullscreenMouseMove : undefined}
    >
      {/* Fullscreen Header - Auto-hides */}
      {isFullscreen && (
        <div
          className={`absolute top-0 left-0 z-[10001] bg-gradient-to-b from-black/80 to-transparent transition-opacity duration-300 ${
            showFullscreenHeader ? 'opacity-100' : 'opacity-0 pointer-events-none'
          }`}
          style={{ right: showPlaylistPanel ? '320px' : '0' }} // 320px = 20rem (w-80)
        >
          <div className="flex items-center justify-between px-4 py-3">
            {/* Left: Title */}
            <div className="flex items-center gap-3">
              <span className="text-white font-medium truncate max-w-[400px]">{currentTitle}</span>
            </div>

            {/* Right: Controls */}
            <div className="flex items-center gap-2">
              {/* Layout buttons */}
              <div className="flex items-center gap-1 bg-black/30 rounded p-1">
                {(['single', 'horizontal', 'vertical', 'quad'] as ViewLayout[]).map((l) => (
                  <button
                    key={l}
                    onClick={() => changeLayout(l)}
                    className={`p-1.5 rounded transition-colors ${
                      layout === l ? 'bg-[var(--primary-color)]/20 text-[var(--primary-color)]' : 'text-gray-400 hover:text-white'
                    }`}
                    title={l.charAt(0).toUpperCase() + l.slice(1)}
                  >
                    {l === 'single' && (
                      <svg className="w-4 h-4" viewBox="0 0 16 16" fill="currentColor">
                        <rect x="2" y="2" width="12" height="12" rx="1" />
                      </svg>
                    )}
                    {l === 'horizontal' && (
                      <svg className="w-4 h-4" viewBox="0 0 16 16" fill="currentColor">
                        <rect x="2" y="2" width="5" height="12" rx="1" />
                        <rect x="9" y="2" width="5" height="12" rx="1" />
                      </svg>
                    )}
                    {l === 'vertical' && (
                      <svg className="w-4 h-4" viewBox="0 0 16 16" fill="currentColor">
                        <rect x="2" y="2" width="12" height="5" rx="1" />
                        <rect x="2" y="9" width="12" height="5" rx="1" />
                      </svg>
                    )}
                    {l === 'quad' && (
                      <svg className="w-4 h-4" viewBox="0 0 16 16" fill="currentColor">
                        <rect x="2" y="2" width="5" height="5" rx="1" />
                        <rect x="9" y="2" width="5" height="5" rx="1" />
                        <rect x="2" y="9" width="5" height="5" rx="1" />
                        <rect x="9" y="9" width="5" height="5" rx="1" />
                      </svg>
                    )}
                  </button>
                ))}
              </div>

              {/* Shuffle + Repeat (apply to the whole queue — videos & photos).
                  Sits to the LEFT of the slideshow controls. */}
              <div className="flex items-center gap-1 bg-black/30 rounded p-1">
                <button
                  onClick={() => setPlaybackState(prev => ({ ...prev, isShuffled: !prev.isShuffled }))}
                  className={`p-1.5 rounded transition-colors ${playbackState.isShuffled ? 'bg-[var(--primary-color)]/20 text-[var(--primary-color)]' : 'text-gray-400 hover:text-white'}`}
                  title={playbackState.isShuffled ? 'Shuffle: on (re-shuffles each repeat pass)' : 'Shuffle: off'}
                >
                  <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M16 3h5v5 M4 20L21 3 M21 16v5h-5 M15 15l6 6 M4 4l5 5" />
                  </svg>
                </button>
                <button
                  onClick={() => setPlaybackState(prev => {
                    const order = ['none', 'all', 'one'] as const;
                    return { ...prev, repeatMode: order[(order.indexOf(prev.repeatMode) + 1) % order.length] };
                  })}
                  className={`relative p-1.5 rounded transition-colors ${playbackState.repeatMode !== 'none' ? 'bg-[var(--primary-color)]/20 text-[var(--primary-color)]' : 'text-gray-400 hover:text-white'}`}
                  title={playbackState.repeatMode === 'none' ? 'Repeat: off' : playbackState.repeatMode === 'all' ? 'Repeat: all (loop the queue)' : 'Repeat: one'}
                >
                  <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M17 1l4 4-4 4 M3 11V9a4 4 0 014-4h14 M7 23l-4-4 4-4 M21 13v2a4 4 0 01-4 4H3" />
                  </svg>
                  {playbackState.repeatMode === 'one' && (
                    <span className="absolute -top-1 -right-1 text-[9px] font-bold leading-none">1</span>
                  )}
                </button>
              </div>

              {/* Slideshow Controls */}
              <div className="flex items-center gap-1 bg-black/30 rounded p-1">
                {/* Slideshow toggle */}
                <button
                  onClick={toggleSlideshow}
                  className={`p-1.5 rounded transition-colors ${
                    slideshow.enabled ? 'bg-[var(--primary-color)]/20 text-[var(--primary-color)]' : 'text-gray-400 hover:text-white'
                  }`}
                  title={slideshow.enabled ? 'Stop Slideshow' : 'Start Slideshow'}
                >
                  {slideshow.enabled ? (
                    <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 24 24">
                      <path d="M6 19h4V5H6v14zm8-14v14h4V5h-4z" />
                    </svg>
                  ) : (
                    <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 24 24">
                      <path d="M8 5v14l11-7z" />
                    </svg>
                  )}
                </button>

                {/* Slideshow interval */}
                <input
                  type="number"
                  min="1"
                  max="60"
                  value={slideshow.interval}
                  onChange={(e) => setSlideshowInterval(parseInt(e.target.value) || 5)}
                  className="w-10 bg-black/30 border border-gray-700/50 rounded px-1 py-0.5 text-xs text-white text-center"
                  title="Slideshow photo interval (seconds)"
                />
                <span className="text-xs text-gray-400">s</span>

                {/* Rotation style — Consecutive ↔ Staggered (multi-pane only;
                    randomization is the separate Shuffle toggle, so "staggered +
                    shuffle on" = one random pane at a time). */}
                {layout !== 'single' && (
                  <button
                    onClick={() => setSlideshowMode(slideshow.mode === 'staggered' ? 'consecutive' : 'staggered')}
                    className="p-1.5 rounded transition-colors text-gray-400 hover:text-white"
                    title={slideshow.mode === 'staggered' ? 'Rotation: Staggered (one pane at a time)' : 'Rotation: Consecutive (all panes change together)'}
                  >
                    {slideshow.mode === 'staggered' ? (
                      // Staggered: one pane at a time (offset lines)
                      <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6h14M4 12h9M4 18h16" />
                      </svg>
                    ) : (
                      // Consecutive: all panes change (right arrow)
                      <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 12h14m-6-6l6 6-6 6" />
                      </svg>
                    )}
                  </button>
                )}
              </div>

              {/* Queue info */}
              {queue.length > 0 && (
                <span className="text-xs text-gray-400 px-2">
                  {playedItems.size}/{queue.length}
                </span>
              )}

              {/* Enhance (FSR super-resolution) toggle */}
              <button
                onClick={toggleEnhance}
                className={`p-2 rounded transition-colors ${
                  enhanceOn ? 'bg-[var(--primary-color)]/20 text-[var(--primary-color)]' : 'text-gray-400 hover:text-white'
                }`}
                title={enhanceOn ? 'Enhance: FSR upscaling ON' : 'Enhance video (FSR upscaling)'}
              >
                <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 3v4M3 5h4M6 17v4m-2-2h4m5-16l2.286 6.857L21 12l-5.714 2.143L13 21l-2.286-6.857L5 12l5.714-2.143L13 3z" />
                </svg>
              </button>

              {/* Playlist toggle */}
              <button
                onClick={() => setShowPlaylistPanel(!showPlaylistPanel)}
                className={`p-2 rounded transition-colors ${
                  showPlaylistPanel ? 'bg-[var(--primary-color)]/20 text-[var(--primary-color)]' : 'text-gray-400 hover:text-white'
                }`}
                title="Playlists"
              >
                <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6h16M4 10h16M4 14h16M4 18h16" />
                </svg>
              </button>

              {/* Privacy toggle */}
              <button
                onClick={() => setShowPrivacyPanel(!showPrivacyPanel)}
                className={`p-2 rounded transition-colors ${
                  showPrivacyPanel ? 'bg-[var(--primary-color)]/20 text-[var(--primary-color)]' : 'text-gray-400 hover:text-white'
                }`}
                title="Privacy Settings"
              >
                <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
                </svg>
              </button>

              {/* Close media */}
              {currentMedia && (
                <button
                  onClick={() => {
                    closeMedia(activePane);
                    setQueue(prev => prev.filter(q => q.source !== currentMedia.source));
                  }}
                  className="p-2 rounded text-gray-400 hover:text-red-400 transition-colors"
                  title="Close media"
                >
                  <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                  </svg>
                </button>
              )}

              {/* Exit fullscreen */}
              <button
                onClick={toggleFullscreen}
                className="p-2 rounded text-gray-400 hover:text-white transition-colors"
                title="Exit Fullscreen (Esc)"
              >
                <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 9V4.5M9 9H4.5M9 9L3.75 3.75M9 15v4.5M9 15H4.5M9 15l-5.25 5.25M15 9h4.5M15 9V4.5M15 9l5.25-5.25M15 15h4.5M15 15v4.5m0-4.5l5.25 5.25" />
                </svg>
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Floating Controls - DISABLED: Now using X11 webview controls via WebSocket */}
      {/* The X11 webview controls (MediaControls component) render above video in a separate window */}
      {/* and communicate with this component via WebSocket for real-time updates */}

      {/* Toolbar - Hidden in fullscreen */}
      {!isFullscreen && (
      <div className="flex items-center gap-2 p-2 bg-[var(--navbar-color)] border-b border-gray-700/50">
        {/* File/URL input */}
        <button
          onClick={openFile}
          className="px-3 py-1.5 bg-[var(--primary-color)] text-white rounded hover:bg-[var(--primary-color)]/80 transition-colors flex items-center gap-1"
        >
          <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
          </svg>
          Open
        </button>

        <div className="flex-1 flex items-center gap-2 max-w-md">
          <input
            type="text"
            value={urlInput}
            onChange={(e) => setUrlInput(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && openUrl()}
            placeholder="Enter URL or file path..."
            className="flex-1 bg-black/30 border border-gray-700/50 rounded px-3 py-1.5 text-sm text-white placeholder-gray-500 focus:outline-none focus:border-[var(--primary-color)]"
          />
          <button
            onClick={openUrl}
            disabled={!urlInput.trim()}
            className="px-3 py-1.5 bg-gray-700/50 text-white rounded hover:bg-gray-700 transition-colors disabled:opacity-50"
          >
            Go
          </button>
        </div>

        {/* Workspace Tabs — each is an independent queue + panes + layout */}
        <div className="flex items-center gap-1 flex-1 min-w-0 bg-black/20 rounded px-1 py-0.5 overflow-x-auto scrollbar-thin">
          {mediaTabs.map((tab) => {
            const tabMedia = tab.id === activeTabId ? mediaItems : tab.mediaItems;
            const firstType = (tabMedia.find(Boolean) as MediaItem | undefined)?.type;
            const title = tab.id === activeTabId ? sessionTitle(mediaItems, queue) : tab.title;
            return (
              <div
                key={tab.id}
                onClick={() => switchToTab(tab.id)}
                title={title}
                className={`flex items-center gap-1.5 px-2 py-1 rounded cursor-pointer transition-colors min-w-0 max-w-[170px] ${
                  activeTabId === tab.id
                    ? 'bg-[var(--primary-color)]/20 text-white'
                    : 'bg-black/30 text-gray-400 hover:text-white hover:bg-black/40'
                }`}
              >
                {/* Tab icon based on its first loaded media */}
                {firstType === 'video' && (
                  <svg className="w-3.5 h-3.5 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M5 18h8a2 2 0 002-2V8a2 2 0 00-2-2H5a2 2 0 00-2 2v8a2 2 0 002 2z" />
                  </svg>
                )}
                {firstType === 'image' && (
                  <svg className="w-3.5 h-3.5 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z" />
                  </svg>
                )}
                {firstType === 'audio' && (
                  <svg className="w-3.5 h-3.5 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 19V6l12-3v13M9 19c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zm12-3c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zM9 10l12-3" />
                  </svg>
                )}
                {!firstType && (
                  <svg className="w-3.5 h-3.5 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6h16M4 12h16M4 18h7" />
                  </svg>
                )}

                {/* Tab title */}
                <span className="text-xs truncate">{title}</span>

                {/* Close button (hidden when only one tab remains) */}
                {mediaTabs.length > 1 && (
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      removeTab(tab.id);
                    }}
                    className="p-0.5 hover:bg-white/10 rounded flex-shrink-0"
                    title="Close tab"
                  >
                    <svg className="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                    </svg>
                  </button>
                )}
              </div>
            );
          })}

          {/* Add new tab button */}
          <button
            onClick={() => addTab()}
            className="p-1.5 text-gray-500 hover:text-white hover:bg-black/30 rounded transition-colors flex-shrink-0"
            title="New Tab"
          >
            <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 6v6m0 0v6m0-6h6m-6 0H6" />
            </svg>
          </button>
        </div>

        {/* Active-pane selector — picks which pane the floating media controls
            drive. Clicks on an embedded, playing video don't reliably reach the
            app (the native surface composites over the webview and the video sink
            swallows the click), so this is the dependable way to retarget the
            controls in multi-pane layouts. */}
        {layout !== 'single' && (
          <div className="flex items-center gap-1 bg-black/30 rounded p-1">
            <span className="text-[10px] text-gray-500 px-1 select-none">Controls</span>
            {Array.from({ length: getMaxPanesForLayout(layout) }, (_, i) => i).map((i) => (
              <button
                key={i}
                onClick={() => setActivePane(i)}
                title={`Drive controls for pane ${i + 1}`}
                className={`w-6 h-6 rounded text-xs font-medium transition-colors ${
                  activePane === i
                    ? 'bg-[var(--primary-color)] text-white'
                    : mediaItems[i]
                    ? 'bg-black/40 text-gray-300 hover:text-white'
                    : 'bg-black/20 text-gray-600'
                }`}
              >
                {i + 1}
              </button>
            ))}
          </div>
        )}

        {/* Layout buttons */}
        <div className="flex items-center gap-1 bg-black/30 rounded p-1">
          {(['single', 'horizontal', 'vertical', 'quad'] as ViewLayout[]).map((l) => (
            <button
              key={l}
              onClick={() => changeLayout(l)}
              className={`p-1.5 rounded transition-colors ${
                layout === l ? 'bg-[var(--primary-color)]/20 text-[var(--primary-color)]' : 'text-gray-400 hover:text-white'
              }`}
              title={l.charAt(0).toUpperCase() + l.slice(1)}
            >
              {l === 'single' && (
                <svg className="w-4 h-4" viewBox="0 0 16 16" fill="currentColor">
                  <rect x="2" y="2" width="12" height="12" rx="1" />
                </svg>
              )}
              {l === 'horizontal' && (
                <svg className="w-4 h-4" viewBox="0 0 16 16" fill="currentColor">
                  <rect x="2" y="2" width="5" height="12" rx="1" />
                  <rect x="9" y="2" width="5" height="12" rx="1" />
                </svg>
              )}
              {l === 'vertical' && (
                <svg className="w-4 h-4" viewBox="0 0 16 16" fill="currentColor">
                  <rect x="2" y="2" width="12" height="5" rx="1" />
                  <rect x="2" y="9" width="12" height="5" rx="1" />
                </svg>
              )}
              {l === 'quad' && (
                <svg className="w-4 h-4" viewBox="0 0 16 16" fill="currentColor">
                  <rect x="2" y="2" width="5" height="5" rx="1" />
                  <rect x="9" y="2" width="5" height="5" rx="1" />
                  <rect x="2" y="9" width="5" height="5" rx="1" />
                  <rect x="9" y="9" width="5" height="5" rx="1" />
                </svg>
              )}
            </button>
          ))}
        </div>

        {/* Shuffle + Repeat (whole queue — videos & photos). Left of Slideshow. */}
        <div className="flex items-center gap-1 bg-black/30 rounded p-1">
          <button
            onClick={() => setPlaybackState(prev => ({ ...prev, isShuffled: !prev.isShuffled }))}
            className={`p-1.5 rounded transition-colors ${playbackState.isShuffled ? 'bg-[var(--primary-color)]/20 text-[var(--primary-color)]' : 'text-gray-400 hover:text-white'}`}
            title={playbackState.isShuffled ? 'Shuffle: on (re-shuffles each repeat pass)' : 'Shuffle: off'}
          >
            <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M16 3h5v5 M4 20L21 3 M21 16v5h-5 M15 15l6 6 M4 4l5 5" />
            </svg>
          </button>
          <button
            onClick={() => setPlaybackState(prev => {
              const order = ['none', 'all', 'one'] as const;
              return { ...prev, repeatMode: order[(order.indexOf(prev.repeatMode) + 1) % order.length] };
            })}
            className={`relative p-1.5 rounded transition-colors ${playbackState.repeatMode !== 'none' ? 'bg-[var(--primary-color)]/20 text-[var(--primary-color)]' : 'text-gray-400 hover:text-white'}`}
            title={playbackState.repeatMode === 'none' ? 'Repeat: off' : playbackState.repeatMode === 'all' ? 'Repeat: all (loop the queue)' : 'Repeat: one'}
          >
            <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M17 1l4 4-4 4 M3 11V9a4 4 0 014-4h14 M7 23l-4-4 4-4 M21 13v2a4 4 0 01-4 4H3" />
            </svg>
            {playbackState.repeatMode === 'one' && (
              <span className="absolute -top-1 -right-1 text-[9px] font-bold leading-none">1</span>
            )}
          </button>
        </div>

        {/* Slideshow Controls */}
        <div className="flex items-center gap-1 bg-black/30 rounded p-1">
          {/* Slideshow toggle */}
          <button
            onClick={toggleSlideshow}
            className={`p-1.5 rounded transition-colors ${
              slideshow.enabled ? 'bg-[var(--primary-color)]/20 text-[var(--primary-color)]' : 'text-gray-400 hover:text-white'
            }`}
            title={slideshow.enabled ? 'Stop Slideshow' : 'Start Slideshow'}
          >
            {slideshow.enabled ? (
              <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 24 24">
                <path d="M6 19h4V5H6v14zm8-14v14h4V5h-4z" />
              </svg>
            ) : (
              <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 24 24">
                <path d="M8 5v14l11-7z" />
              </svg>
            )}
          </button>

          {/* Slideshow interval */}
          <input
            type="number"
            min="1"
            max="60"
            value={slideshow.interval}
            onChange={(e) => setSlideshowInterval(parseInt(e.target.value) || 5)}
            className="w-12 bg-black/30 border border-gray-700/50 rounded px-1 py-0.5 text-xs text-white text-center"
            title="Slideshow photo interval (seconds)"
          />
          <span className="text-xs text-gray-400">s</span>

          {/* Rotation style — Consecutive ↔ Staggered (multi-pane only;
              randomization is the separate Shuffle toggle). */}
          {layout !== 'single' && (
            <button
              onClick={() => setSlideshowMode(slideshow.mode === 'staggered' ? 'consecutive' : 'staggered')}
              className={`p-1.5 rounded transition-colors text-gray-400 hover:text-white`}
              title={slideshow.mode === 'staggered' ? 'Rotation: Staggered (one pane at a time)' : 'Rotation: Consecutive (all panes change together)'}
            >
              {slideshow.mode === 'staggered' ? (
                // Staggered: one pane at a time (offset lines)
                <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6h14M4 12h9M4 18h16" />
                </svg>
              ) : (
                // Consecutive: all panes change (right arrow)
                <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 12h14m-6-6l6 6-6 6" />
                </svg>
              )}
            </button>
          )}
        </div>

        {/* Queue info */}
        {queue.length > 0 && (
          <span className="text-xs text-gray-400 px-2">
            {playedItems.size}/{queue.length} played
          </span>
        )}

        {/* Enhance (FSR super-resolution) toggle */}
        <button
          onClick={toggleEnhance}
          className={`p-2 rounded transition-colors ${
            enhanceOn ? 'bg-[var(--primary-color)]/20 text-[var(--primary-color)]' : 'text-gray-400 hover:text-white'
          }`}
          title={enhanceOn ? 'Enhance: FSR upscaling ON' : 'Enhance video (FSR upscaling)'}
        >
          <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 3v4M3 5h4M6 17v4m-2-2h4m5-16l2.286 6.857L21 12l-5.714 2.143L13 21l-2.286-6.857L5 12l5.714-2.143L13 3z" />
          </svg>
        </button>

        {/* Playlist toggle */}
        <button
          onClick={() => setShowPlaylistPanel(!showPlaylistPanel)}
          className={`p-2 rounded transition-colors ${
            showPlaylistPanel ? 'bg-[var(--primary-color)]/20 text-[var(--primary-color)]' : 'text-gray-400 hover:text-white'
          }`}
          title="Playlists"
        >
          <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6h16M4 10h16M4 14h16M4 18h16" />
          </svg>
        </button>

        {/* Privacy toggle */}
        <button
          onClick={() => setShowPrivacyPanel(!showPrivacyPanel)}
          className={`p-2 rounded transition-colors ${
            showPrivacyPanel ? 'bg-[var(--primary-color)]/20 text-[var(--primary-color)]' : 'text-gray-400 hover:text-white'
          }`}
          title="Privacy Settings"
        >
          <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
          </svg>
        </button>

        {/* Fullscreen toggle */}
        <button
          onClick={toggleFullscreen}
          className={`p-2 rounded transition-colors ${
            isFullscreen ? 'bg-[var(--primary-color)]/20 text-[var(--primary-color)]' : 'text-gray-400 hover:text-white'
          }`}
          title={isFullscreen ? 'Exit Fullscreen' : 'Fullscreen'}
        >
          {isFullscreen ? (
            <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 9V4.5M9 9H4.5M9 9L3.75 3.75M9 15v4.5M9 15H4.5M9 15l-5.25 5.25M15 9h4.5M15 9V4.5M15 9l5.25-5.25M15 15h4.5M15 15v4.5m0-4.5l5.25 5.25" />
            </svg>
          ) : (
            <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3.75 3.75v4.5m0-4.5h4.5m-4.5 0L9 9M3.75 20.25v-4.5m0 4.5h4.5m-4.5 0L9 15M20.25 3.75h-4.5m4.5 0v4.5m0-4.5L15 9m5.25 11.25h-4.5m4.5 0v-4.5m0 4.5L15 15" />
            </svg>
          )}
        </button>
      </div>
      )}

      {/* Main Content */}
      <div className="flex flex-1 min-h-0">
        {/* Media Viewer */}
        <div className="flex-1 flex flex-col">
          {layout === 'single' && renderPane(0)}

          {layout === 'horizontal' && (
            <div className="flex-1 flex flex-row relative min-h-0 min-w-0">
              {renderPane(0)}
              {mediaItems.every(m => m === null) && <div className="w-1 bg-gray-600 cursor-col-resize z-20" />}
              {renderPane(1)}
            </div>
          )}

          {layout === 'vertical' && (
            <div className="flex-1 flex flex-col relative min-h-0 min-w-0">
              {renderPane(0)}
              {mediaItems.every(m => m === null) && <div className="h-1 bg-gray-600 cursor-row-resize z-20" />}
              {renderPane(1)}
            </div>
          )}

          {layout === 'quad' && (
            <div className="flex-1 flex flex-col relative">
              <div className="flex-1 flex flex-row min-h-0">
                {renderPane(0)}
                {mediaItems.every(m => m === null) && <div className="w-1 bg-gray-600 cursor-col-resize z-20" />}
                {renderPane(1)}
              </div>
              {mediaItems.every(m => m === null) && <div className="h-1 bg-gray-600 cursor-row-resize z-20" />}
              <div className="flex-1 flex flex-row min-h-0">
                {renderPane(2)}
                {mediaItems.every(m => m === null) && <div className="w-1 bg-gray-600 cursor-col-resize z-20" />}
                {renderPane(3)}
              </div>
            </div>
          )}

          {/* Stacked Controls - DISABLED: Now using X11 webview controls for all layouts */}
          {/* The X11 webview controls handle all playback via WebSocket */}
          {/* Click on a pane to make it the active player */}
        </div>

        {/* Playlist / Queue Panel. Always a flex sibling (never an absolute
            overlay) so it SHRINKS the video area — the native video surface
            renders above the DOM, so an overlay here would be covered by it and
            you couldn't remove/stop a queued video. Shrinking the video area
            makes the surface follow (via the player's ResizeObserver), leaving
            the queue clear. */}
        {showPlaylistPanel && (
          <div className="w-80 bg-[var(--card-bg-color)] border-l border-gray-700/50 flex flex-col flex-shrink-0">
            <div className="p-3 border-b border-gray-700/50">
              <h3 className="font-medium text-white mb-2">Playlists / Queue</h3>
            </div>

            <div className="flex-1 overflow-y-auto">
              {/* Current Queue Section */}
              <div className="p-3 border-b border-gray-700/50">
                <div className="flex items-center justify-between mb-2">
                  <span className="text-gray-300 text-sm font-medium flex items-center gap-2">
                    <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6h16M4 10h16M4 14h16M4 18h16" />
                    </svg>
                    Queue (Temporary)
                  </span>
                  <span className="text-gray-500 text-xs">{queue.length}</span>
                  {queue.length > 0 && (
                    <button
                      onClick={() => {
                        setPlaylistNameInput('');
                        setPlaylistNamePrompt({
                          title: 'Save queue as playlist',
                          onConfirm: async (name) => {
                            try {
                              const newPlaylist = await invoke<Playlist>('create_media_playlist', {
                                profile_id: profileId,
                                name,
                                description: `Saved from queue on ${new Date().toLocaleDateString()}`,
                                encrypted: false,
                              });
                              for (const item of queue) {
                                await invoke('add_to_media_playlist', {
                                  playlist_id: newPlaylist.id,
                                  source: item.source,
                                  media_type: item.type,
                                  title: item.title || null,
                                  thumbnail: null,
                                });
                              }
                              loadPlaylists();
                            } catch (err) {
                              console.error('Failed to save queue as playlist:', err);
                            }
                          },
                        });
                      }}
                      className="text-xs text-[var(--primary-color)] hover:underline"
                    >
                      Save as playlist
                    </button>
                  )}
                </div>
                {queue.length === 0 ? (
                  <p className="text-gray-500 text-xs text-center py-2">Queue is empty. Open files to add them.</p>
                ) : (
                  <div className="space-y-1 max-h-48 overflow-y-auto pr-2">
                    {queue.map((item, index) => (
                      <div
                        key={item.id}
                        draggable
                        onDragStart={(e) => { setQueueDragIndex(index); e.dataTransfer.effectAllowed = 'move'; }}
                        onDragOver={(e) => {
                          e.preventDefault();
                          e.dataTransfer.dropEffect = 'move';
                          if (queueDragIndex !== null && queueDragIndex !== index) setQueueDropIndex(index);
                        }}
                        onDragLeave={() => setQueueDropIndex(d => (d === index ? null : d))}
                        onDrop={(e) => {
                          e.preventDefault();
                          if (queueDragIndex !== null) reorderQueue(queueDragIndex, index);
                          setQueueDragIndex(null);
                          setQueueDropIndex(null);
                        }}
                        onDragEnd={() => { setQueueDragIndex(null); setQueueDropIndex(null); }}
                        className={`${queueDropIndex === index ? 'border-t-2 border-[var(--primary-color)]' : ''} ${queueDragIndex === index ? 'opacity-40' : ''}`}
                      >
                      <div
                        className={`p-1.5 rounded cursor-pointer flex items-center gap-2 ${
                          playedItems.has(item.id) ? 'opacity-50' : ''
                        } hover:bg-white/5`}
                        onClick={() => openMedia(item.source, item.type, item.title)}
                      >
                        {/* Drag handle */}
                        <svg className="w-3 h-3 text-gray-600 flex-shrink-0 cursor-grab active:cursor-grabbing" fill="currentColor" viewBox="0 0 24 24">
                          <title>Drag to reorder</title>
                          <circle cx="9" cy="6" r="1.5" /><circle cx="15" cy="6" r="1.5" />
                          <circle cx="9" cy="12" r="1.5" /><circle cx="15" cy="12" r="1.5" />
                          <circle cx="9" cy="18" r="1.5" /><circle cx="15" cy="18" r="1.5" />
                        </svg>
                        <span className="text-gray-500 text-xs w-4">{index + 1}</span>
                        {item.type === 'video' && (
                          <svg className="w-3 h-3 text-gray-400 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M5 18h8a2 2 0 002-2V8a2 2 0 00-2-2H5a2 2 0 00-2 2v8a2 2 0 002 2z" />
                          </svg>
                        )}
                        {item.type === 'image' && (
                          <svg className="w-3 h-3 text-gray-400 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z" />
                          </svg>
                        )}
                        {item.type === 'audio' && (
                          <svg className="w-3 h-3 text-gray-400 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 19V6l12-3v13M9 19c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zm12-3c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zM9 10l12-3" />
                          </svg>
                        )}
                        <span className="text-white text-xs truncate flex-1">
                          {item.title || item.source.split('/').pop()}
                        </span>
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            loadPlaylists();
                            setAddToPlaylistMenuId(addToPlaylistMenuId === item.id ? null : item.id);
                          }}
                          className={`p-0.5 ${addToPlaylistMenuId === item.id ? 'text-[var(--primary-color)]' : 'text-gray-500 hover:text-[var(--primary-color)]'}`}
                          title="Add to playlist"
                        >
                          <svg className="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 6v6m0 0v6m0-6h6m-6 0H6" />
                          </svg>
                        </button>
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            // Remove from queue
                            setQueue(prev => prev.filter(q => q.id !== item.id));
                            setPlayedItems(prev => { const n = new Set(prev); n.delete(item.id); return n; });
                            // Close any pane currently showing this item (read the live
                            // pane contents via ref so it's correct mid-removal).
                            mediaItemsRef.current.forEach((media, paneIndex) => {
                              if (media?.source === item.source) {
                                closeMedia(paneIndex);
                              }
                            });
                          }}
                          className="text-gray-500 hover:text-red-400 p-0.5"
                          title="Remove from queue"
                        >
                          <svg className="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                          </svg>
                        </button>
                      </div>
                      {/* Per-item: add this queue item to a playlist */}
                      {addToPlaylistMenuId === item.id && (
                        <div className="ml-6 mb-1 mt-0.5 space-y-0.5 border-l border-gray-700/50 pl-2">
                          <div className="text-gray-500 text-[10px] px-1">Add to playlist:</div>
                          {playlists.length === 0 && (
                            <div className="text-gray-600 text-[10px] px-1">No playlists yet</div>
                          )}
                          {playlists.map((pl) => (
                            <button
                              key={pl.id}
                              onClick={(e) => { e.stopPropagation(); addQueueItemToPlaylist(pl.id, item); }}
                              className="block w-full text-left text-xs text-white/80 hover:text-white hover:bg-white/5 rounded px-1 py-0.5 truncate"
                            >
                              {pl.name}
                            </button>
                          ))}
                          <button
                            onClick={(e) => { e.stopPropagation(); createPlaylistWithItem(item); }}
                            className="block w-full text-left text-xs text-[var(--primary-color)] hover:bg-white/5 rounded px-1 py-0.5"
                          >
                            + New playlist…
                          </button>
                        </div>
                      )}
                      </div>
                    ))}
                  </div>
                )}
                {queue.length > 0 && (
                  <button
                    onClick={() => {
                      setQueue([]);
                      setPlayedItems(new Set());
                      // Also clear the panes and stop any players/surfaces — clearing
                      // the queue should empty what's on screen, not just the list.
                      setMediaItems([null, null, null, null]);
                      setPaneStates(prev => prev.map(p => ({ ...p, currentItem: null, isPlaying: false })));
                      for (let i = 0; i < 4; i++) {
                        const playerId = panePid(i);
                        invoke('player_stop', { playerId }).catch(() => {});
                        invoke('player_remove', { playerId }).catch(() => {});
                        invoke('hide_video_surface', { playerId }).catch(() => {});
                      }
                    }}
                    className="w-full mt-2 text-xs text-gray-400 hover:text-red-400 py-1"
                  >
                    Clear queue
                  </button>
                )}
              </div>

              {/* Playlists Section */}
              <div className="p-3">
                <div className="flex items-center justify-between mb-2">
                  <span className="text-gray-300 text-sm font-medium">Saved Playlists</span>
                </div>
                <div className="flex gap-2 mb-3">
                  <input
                    type="text"
                    value={newPlaylistName}
                    onChange={(e) => setNewPlaylistName(e.target.value)}
                    onKeyDown={(e) => e.key === 'Enter' && createPlaylist()}
                    placeholder="New playlist..."
                    className="flex-1 bg-black/30 border border-gray-700/50 rounded px-2 py-1 text-sm text-white placeholder-gray-500 focus:outline-none focus:border-[var(--primary-color)]"
                  />
                  <button
                    onClick={createPlaylist}
                    disabled={!newPlaylistName.trim()}
                    className="px-2 py-1 bg-[var(--primary-color)] text-white rounded text-sm disabled:opacity-50"
                  >
                    +
                  </button>
                </div>
                {playlists.length === 0 ? (
                  <p className="text-gray-500 text-xs text-center py-2">No saved playlists</p>
                ) : (
                  <div className="space-y-1">
                    {playlists.map((playlist) => (
                      <div
                        key={playlist.id}
                        className={`p-2 rounded cursor-pointer transition-colors ${
                          currentPlaylist?.id === playlist.id
                            ? 'bg-[var(--primary-color)]/20'
                            : 'hover:bg-white/5'
                        }`}
                        onClick={() => loadPlaylistItems(playlist)}
                      >
                        <div className="flex items-center justify-between">
                          <span className="text-white text-sm truncate">{playlist.name}</span>
                          <span className="text-gray-500 text-xs">{playlist.item_count}</span>
                        </div>
                      </div>
                    ))}
                  </div>
                )}

                {/* Playlist items */}
                {currentPlaylist && (
                  <div className="border-t border-gray-700/50 mt-3 pt-3">
                    <div className="flex items-center justify-between mb-2">
                      <span className="text-gray-400 text-xs">{currentPlaylist.name}</span>
                      <button
                        onClick={() => addToPlaylist(currentPlaylist.id)}
                        disabled={!mediaItems[activePane]}
                        className="text-xs text-[var(--primary-color)] hover:underline disabled:opacity-50"
                      >
                        + Add current
                      </button>
                    </div>
                    {playlistItems.length === 0 ? (
                      <p className="text-gray-600 text-xs text-center">Empty playlist</p>
                    ) : (
                      <div className="space-y-1">
                        {playlistItems.map((item) => (
                          <div
                            key={item.id}
                            className="p-1.5 rounded hover:bg-white/5 cursor-pointer"
                            onClick={() => openMedia(item.source, item.media_type as MediaType)}
                          >
                            <span className="text-white text-xs truncate block">
                              {item.title || item.source.split('/').pop()}
                            </span>
                          </div>
                        ))}
                      </div>
                    )}
                  </div>
                )}
              </div>
            </div>
          </div>
        )}

        {/* Privacy Panel */}
        {showPrivacyPanel && (
          <div className="w-72 bg-[var(--card-bg-color)] border-l border-gray-700/50 p-4">
            <h3 className="font-medium text-white mb-4 flex items-center gap-2">
              <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z" />
              </svg>
              Privacy Settings
            </h3>

            {privacySettings && (
              <div className="space-y-4">
                {/* Privacy First Notice */}
                <div className="bg-green-500/10 border border-green-500/30 rounded-lg p-3">
                  <p className="text-green-400 text-xs">
                    Privacy-first: History and logging are disabled by default.
                  </p>
                </div>

                {/* History Toggle */}
                <label className="flex items-center justify-between">
                  <span className="text-gray-300 text-sm">Enable history</span>
                  <input
                    type="checkbox"
                    checked={privacySettings.history_enabled}
                    onChange={async (e) => {
                      const newSettings = { ...privacySettings, history_enabled: e.target.checked };
                      try {
                        await invoke('update_media_privacy_settings', { settings: newSettings });
                        setPrivacySettings(newSettings);
                      } catch (err) {
                        console.error('Failed to update settings:', err);
                      }
                    }}
                    className="w-5 h-5 rounded accent-[var(--primary-color)]"
                  />
                </label>

                {/* Playlist History Toggle */}
                <label className="flex items-center justify-between">
                  <span className="text-gray-300 text-sm">Track playlist history</span>
                  <input
                    type="checkbox"
                    checked={privacySettings.playlist_history_enabled}
                    onChange={async (e) => {
                      const newSettings = { ...privacySettings, playlist_history_enabled: e.target.checked };
                      try {
                        await invoke('update_media_privacy_settings', { settings: newSettings });
                        setPrivacySettings(newSettings);
                      } catch (err) {
                        console.error('Failed to update settings:', err);
                      }
                    }}
                    className="w-5 h-5 rounded accent-[var(--primary-color)]"
                  />
                </label>

                {/* Password Protection */}
                <div className="border-t border-gray-700/50 pt-4">
                  <label className="flex items-center justify-between mb-2">
                    <span className="text-gray-300 text-sm">Require password</span>
                    <input
                      type="checkbox"
                      checked={privacySettings.require_password}
                      onChange={async (e) => {
                        const newSettings = { ...privacySettings, require_password: e.target.checked };
                        try {
                          await invoke('update_media_privacy_settings', { settings: newSettings });
                          setPrivacySettings(newSettings);
                        } catch (err) {
                          console.error('Failed to update settings:', err);
                        }
                      }}
                      className="w-5 h-5 rounded accent-[var(--primary-color)]"
                    />
                  </label>
                  <p className="text-gray-500 text-xs">
                    Require password to access history and playlists
                  </p>
                </div>

                {/* OTP Protection */}
                <div className="border-t border-gray-700/50 pt-4">
                  <label className="flex items-center justify-between mb-2">
                    <span className="text-gray-300 text-sm">Require OTP</span>
                    <input
                      type="checkbox"
                      checked={privacySettings.require_otp}
                      onChange={async (e) => {
                        const newSettings = { ...privacySettings, require_otp: e.target.checked };
                        try {
                          await invoke('update_media_privacy_settings', { settings: newSettings });
                          setPrivacySettings(newSettings);
                        } catch (err) {
                          console.error('Failed to update settings:', err);
                        }
                      }}
                      className="w-5 h-5 rounded accent-[var(--primary-color)]"
                    />
                  </label>
                  <p className="text-gray-500 text-xs">
                    Two-factor authentication for sensitive actions
                  </p>
                </div>

                {/* Auto Clear */}
                <div className="border-t border-gray-700/50 pt-4">
                  <label className="block text-gray-300 text-sm mb-2">Auto-clear history</label>
                  <select
                    value={privacySettings.auto_clear_history_days || 0}
                    onChange={async (e) => {
                      const days = parseInt(e.target.value) || null;
                      const newSettings = { ...privacySettings, auto_clear_history_days: days };
                      try {
                        await invoke('update_media_privacy_settings', { settings: newSettings });
                        setPrivacySettings(newSettings);
                      } catch (err) {
                        console.error('Failed to update settings:', err);
                      }
                    }}
                    className="w-full bg-black/30 border border-gray-700/50 rounded px-3 py-2 text-sm text-white"
                  >
                    <option value="0">Never</option>
                    <option value="1">After 1 day</option>
                    <option value="7">After 7 days</option>
                    <option value="30">After 30 days</option>
                    <option value="90">After 90 days</option>
                  </select>
                </div>

                {/* Clear History Button */}
                {privacySettings.history_enabled && (
                  <button
                    onClick={async () => {
                      if (confirm('Clear all media history?')) {
                        try {
                          await invoke('clear_media_history', { profileId: profileId });
                        } catch (err) {
                          console.error('Failed to clear history:', err);
                        }
                      }
                    }}
                    className="w-full mt-4 px-4 py-2 bg-red-500/20 text-red-400 rounded hover:bg-red-500/30 transition-colors text-sm"
                  >
                    Clear All History
                  </button>
                )}
              </div>
            )}
          </div>
        )}
      </div>

      {/* Playlist name modal (in-app replacement for the native prompt() dialog) */}
      {playlistNamePrompt && (
        <div
          className="fixed inset-0 z-[10003] flex items-center justify-center bg-black/70 backdrop-blur-sm"
          onClick={() => setPlaylistNamePrompt(null)}
        >
          <div
            className="bg-gray-900 border border-gray-700 rounded-xl shadow-2xl w-full max-w-sm mx-4 p-5"
            onClick={(e) => e.stopPropagation()}
          >
            <h3 className="text-white font-medium mb-3">{playlistNamePrompt.title}</h3>
            <input
              autoFocus
              type="text"
              value={playlistNameInput}
              onChange={(e) => setPlaylistNameInput(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' && playlistNameInput.trim()) {
                  playlistNamePrompt.onConfirm(playlistNameInput.trim());
                  setPlaylistNamePrompt(null);
                } else if (e.key === 'Escape') {
                  setPlaylistNamePrompt(null);
                }
              }}
              placeholder="Playlist name…"
              className="w-full bg-black/30 border border-gray-700 rounded px-3 py-2 text-sm text-white placeholder-gray-500 focus:outline-none focus:border-[var(--primary-color)]"
            />
            <div className="flex justify-end gap-2 mt-4">
              <button
                onClick={() => setPlaylistNamePrompt(null)}
                className="px-3 py-1.5 text-sm text-gray-300 hover:text-white"
              >
                Cancel
              </button>
              <button
                onClick={() => {
                  if (playlistNameInput.trim()) {
                    playlistNamePrompt.onConfirm(playlistNameInput.trim());
                    setPlaylistNamePrompt(null);
                  }
                }}
                disabled={!playlistNameInput.trim()}
                className="px-3 py-1.5 text-sm bg-[var(--primary-color)] text-white rounded disabled:opacity-50"
              >
                Create
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Password Setup Modal */}
      {showPasswordSetupModal && (
        <div className="fixed inset-0 z-[10002] flex items-center justify-center bg-black/70 backdrop-blur-sm">
          <div className="bg-gray-900 border border-gray-700 rounded-2xl shadow-2xl max-w-md w-full mx-4 overflow-hidden">
            {/* Header */}
            <div className="bg-gradient-to-r from-[var(--primary-color)]/20 to-purple-500/20 px-6 py-4 border-b border-gray-700">
              <div className="flex items-center gap-3">
                <div className="w-10 h-10 rounded-full bg-[var(--primary-color)]/20 flex items-center justify-center">
                  <svg className="w-5 h-5 text-[var(--primary-color)]" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
                  </svg>
                </div>
                <div>
                  <h2 className="text-lg font-semibold text-white">Set Up Password Protection</h2>
                  <p className="text-sm text-gray-400">Secure your media history and playlists</p>
                </div>
              </div>
            </div>

            {/* Content */}
            <div className="px-6 py-5">
              {/* Warning Banner */}
              <div className="bg-amber-500/10 border border-amber-500/30 rounded-lg p-4 mb-5">
                <div className="flex items-start gap-3">
                  <svg className="w-5 h-5 text-amber-500 flex-shrink-0 mt-0.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
                  </svg>
                  <div>
                    <p className="text-amber-400 font-medium text-sm">Important Warning</p>
                    <p className="text-amber-300/80 text-xs mt-1">
                      If you lose your password, your media history and playlists will be <strong>permanently unretrievable</strong>. There is no password recovery option.
                    </p>
                  </div>
                </div>
              </div>

              {/* Password Fields */}
              <div className="space-y-4">
                <div>
                  <label className="block text-sm text-gray-300 mb-1.5">New Password</label>
                  <div className="relative">
                    <input
                      type={showMediaPassword ? 'text' : 'password'}
                      value={newPassword}
                      onChange={(e) => setNewPassword(e.target.value)}
                      placeholder="Enter password..."
                      className="w-full bg-black/30 border border-gray-700 rounded-lg px-4 py-2.5 pr-11 text-white placeholder-gray-500 focus:outline-none focus:border-[var(--primary-color)] transition-colors"
                      autoFocus
                    />
                    <button type="button" onClick={() => setShowMediaPassword(s => !s)} className="absolute right-3 top-1/2 -translate-y-1/2 text-gray-400 hover:text-white" title={showMediaPassword ? 'Hide password' : 'Show password'}>
                      {showMediaPassword ? (
                        <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13.875 18.825A10.05 10.05 0 0112 19c-4.478 0-8.268-2.943-9.543-7a9.97 9.97 0 011.563-3.029m5.858.908a3 3 0 114.243 4.243M9.878 9.878l4.242 4.242M9.88 9.88l-3.29-3.29m7.532 7.532l3.29 3.29M3 3l18 18" /></svg>
                      ) : (
                        <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" /><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" /></svg>
                      )}
                    </button>
                  </div>
                </div>

                <div>
                  <label className="block text-sm text-gray-300 mb-1.5">Confirm Password</label>
                  <div className="relative">
                    <input
                      type={showMediaPassword ? 'text' : 'password'}
                      value={confirmPassword}
                      onChange={(e) => setConfirmPassword(e.target.value)}
                      onKeyDown={(e) => e.key === 'Enter' && handleSetPassword()}
                      placeholder="Confirm password..."
                      className="w-full bg-black/30 border border-gray-700 rounded-lg px-4 py-2.5 pr-11 text-white placeholder-gray-500 focus:outline-none focus:border-[var(--primary-color)] transition-colors"
                    />
                    <button type="button" onClick={() => setShowMediaPassword(s => !s)} className="absolute right-3 top-1/2 -translate-y-1/2 text-gray-400 hover:text-white" title={showMediaPassword ? 'Hide password' : 'Show password'}>
                      {showMediaPassword ? (
                        <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13.875 18.825A10.05 10.05 0 0112 19c-4.478 0-8.268-2.943-9.543-7a9.97 9.97 0 011.563-3.029m5.858.908a3 3 0 114.243 4.243M9.878 9.878l4.242 4.242M9.88 9.88l-3.29-3.29m7.532 7.532l3.29 3.29M3 3l18 18" /></svg>
                      ) : (
                        <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" /><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" /></svg>
                      )}
                    </button>
                  </div>
                </div>

                {/* Error Message */}
                {passwordError && (
                  <div className="flex items-center gap-2 text-red-400 text-sm">
                    <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                    </svg>
                    {passwordError}
                  </div>
                )}
              </div>

              {/* Info Text */}
              <p className="text-xs text-gray-500 mt-4">
                Your password will be used to encrypt and protect your media history, playlists, and privacy settings.
              </p>
            </div>

            {/* Footer */}
            <div className="bg-black/20 px-6 py-4 border-t border-gray-700 flex items-center justify-between">
              <button
                onClick={handleSkipPassword}
                className="px-4 py-2 text-gray-400 hover:text-white transition-colors text-sm"
              >
                Skip for now
              </button>
              <button
                onClick={handleSetPassword}
                disabled={isSettingPassword}
                className="px-6 py-2 bg-[var(--primary-color)] text-white rounded-lg hover:bg-[var(--primary-color)]/80 transition-colors text-sm font-medium disabled:opacity-50 flex items-center gap-2"
              >
                {isSettingPassword ? (
                  <>
                    <svg className="w-4 h-4 animate-spin" fill="none" viewBox="0 0 24 24">
                      <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                      <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
                    </svg>
                    Setting...
                  </>
                ) : (
                  <>
                    <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
                    </svg>
                    Set Password
                  </>
                )}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

export { VideoPlayer, ImageViewer, GStreamerPlayer, GStreamerVideoPlayer };
export default EarthMultiMedia;
