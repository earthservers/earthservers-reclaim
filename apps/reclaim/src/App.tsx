import { BrowserRouter as Router, Routes, Route } from 'react-router-dom';
import { useState, useEffect, useRef, useCallback, useMemo } from 'react';
import { invoke, isTauri, listen, minimizeWindow, maximizeWindow, closeWindow, startDragging, toggleFullscreen, isWindowMaximized, onWindowResize, toggleDevTools } from './lib/tauri';
import logoSvg from './assets/logo.svg';
import { ProfileManager } from './components/ProfileManager';
import { IncognitoToggle, IncognitoBanner } from './components/IncognitoToggle';
import { DownloadsButton } from './components/DownloadsButton';
import { PrivacyButton } from './components/PrivacyButton';
import { HistoryViewer } from './components/HistoryViewer';
import { ThemeCustomizer } from './components/ThemeCustomizer';
import { ThemeProvider, useTheme } from './contexts/ThemeContext';
import { BrowserProvider } from './contexts/BrowserContext';
import { AnimationLayer } from './components/AnimationLayer';
import { DomainManager } from './components/DomainManager';
import { MemoryManager } from './components/MemoryManager';
import { EarthMultiMedia } from './components/EarthMultiMedia';
import { TabBar, Tab, TabBehavior, TAB_BEHAVIOR_OPTIONS } from './components/TabBar';
import { BookmarkBar, BookmarkManager } from './components/BookmarkComponents';
import { WebView, BrowserNavBar } from './components/WebView';
import { PageLoadSpinner } from './components/PageLoadSpinner';
import { LocalAIHub } from './components/LocalAIHub';
import { MediaPanel } from './components/MediaPanel';
import { RightDockContext, RIGHT_DOCK_WIDTH, RightDockPanel } from './lib/rightDock';
import { WebScraper } from './components/WebScraper';
import { NotesPlugin } from './components/NotesPlugin';
import { PasswordManager } from './components/PasswordManager';
import { OTPAuthenticator } from './components/OTPAuthenticator';
import { MediaControls } from './components/MediaControls';

// Pointer-based button component for WebKitGTK compatibility
// Uses pointerdown instead of click which is more reliable in WebKitGTK
function PointerButton({
  action,
  title,
  icon,
  className = '',
  hoverClass = 'hover:bg-white/10 hover:text-white'
}: {
  action: () => void;
  title: string;
  icon?: React.ReactNode;
  className?: string;
  hoverClass?: string;
}) {
  const buttonRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    const button = buttonRef.current;
    if (!button) return;

    // Use pointerdown - fires before click and is more reliable in WebKitGTK
    const handlePointerDown = (e: PointerEvent) => {
      e.preventDefault();
      e.stopPropagation();
      e.stopImmediatePropagation();
      console.log(`${title} pointerdown triggered`);
      // Use requestAnimationFrame to ensure action runs after event processing
      requestAnimationFrame(() => action());
    };

    // Also handle touchstart for touch devices
    const handleTouchStart = (e: TouchEvent) => {
      e.preventDefault();
      e.stopPropagation();
      console.log(`${title} touchstart triggered`);
      requestAnimationFrame(() => action());
    };

    button.addEventListener('pointerdown', handlePointerDown, { capture: true });
    button.addEventListener('touchstart', handleTouchStart, { capture: true, passive: false });

    return () => {
      button.removeEventListener('pointerdown', handlePointerDown, { capture: true });
      button.removeEventListener('touchstart', handleTouchStart, { capture: true });
    };
  }, [action, title]);

  return (
    <button
      ref={buttonRef}
      type="button"
      className={className || `p-1.5 rounded-lg text-white/60 ${hoverClass} transition-all cursor-pointer`}
      title={title}
      data-no-drag
      style={{
        WebkitAppRegion: 'no-drag',
        pointerEvents: 'auto',
        position: 'relative',
        zIndex: 100,
        touchAction: 'manipulation',
      } as React.CSSProperties}
    >
      {icon}
    </button>
  );
}

// Inner component that uses theme context for animations
function ThemedAnimationLayer({ enabled }: { enabled: boolean }) {
  const { theme } = useTheme();
  const themeKey = theme?.base_preset as 'ocean-turtle' | 'mountain-eagle' | 'sun-fire' | 'lightning-bolt' | 'air-clouds' | 'earthservers-default' | undefined;

  return (
    <AnimationLayer
      enabled={enabled && !!theme}
      theme={themeKey || 'earthservers-default'}
      primaryColor={theme?.primary_color}
      secondaryColor={theme?.secondary_color}
    />
  );
}

// Types
interface Profile {
  id: number | null;
  name: string;
  icon: string | null;
  created_at: string;
  is_active: boolean;
}

// Main service navigation items (shown directly in navbar). "Local AI" is a
// first-class tab (its hub holds the curator/assistant toggles + EarthMemory).
const mainServiceItems = [
  { id: 'search' as const, label: 'Search', icon: 'M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z' },
  { id: 'media' as const, label: 'Media', icon: 'M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M5 18h8a2 2 0 002-2V8a2 2 0 00-2-2H5a2 2 0 00-2 2v8a2 2 0 002 2z' },
  { id: 'scraper' as const, label: 'Scraper', icon: 'M21 12a9 9 0 01-9 9m9-9a9 9 0 00-9-9m9 9H3m9 9a9 9 0 01-9-9m9 9c1.657 0 3-4.03 3-9s-1.343-9-3-9m0 18c-1.657 0-3-4.03-3-9s1.343-9 3-9m-9 9a9 9 0 019-9' },
  { id: 'ai' as const, label: 'Local AI', icon: 'M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z' },
];

// Local-AI on/off settings, persisted in localStorage.
export interface AiSettings {
  curator: boolean;   // transparently summarize visited pages into the knowledge graph
  assistant: boolean; // local chat assistant (model picked by hardware tier)
}
const AI_SETTINGS_KEY = 'reclaim.aiSettings';
function loadAiSettings(): AiSettings {
  try {
    const raw = localStorage.getItem(AI_SETTINGS_KEY);
    if (raw) return { curator: true, assistant: false, ...JSON.parse(raw) };
  } catch { /* ignore */ }
  return { curator: true, assistant: false };
}

function App() {
  const [activeService, setActiveService] = useState<'search' | 'memory' | 'media' | 'scraper' | 'ai'>('search');
  const [activeProfile, setActiveProfile] = useState<Profile | null>(null);
  const [isIncognito, setIsIncognito] = useState(false);
  const [showHistory, setShowHistory] = useState(false);
  const [showThemeCustomizer, setShowThemeCustomizer] = useState(false);
  const [showAbout, setShowAbout] = useState(false);
  const [navbarCollapsed, setNavbarCollapsed] = useState(false);
  const [showBookmarkManager, setShowBookmarkManager] = useState(false);
  const [showBookmarkBar, setShowBookmarkBar] = useState(true);
  const [activeTab, setActiveTab] = useState<Tab | null>(null);
  const [showNotes, setShowNotes] = useState(false);
  const [showPasswordManager, setShowPasswordManager] = useState(false);
  const [showMediaPanel, setShowMediaPanel] = useState(false);
  // GitHub release version check (prompt to download a newer build).
  const [updateInfo, setUpdateInfo] = useState<{ current: string; latest: string; updateAvailable: boolean; url: string } | null>(null);
  const [updateDismissed, setUpdateDismissed] = useState(false);
  // Preserve the search page's scroll position across navigations. The search
  // view unmounts when you open a domain, so we stash the scrollTop and restore
  // it once the (async-loaded) list is tall enough again.
  const searchScrollRef = useRef(0);
  const restoreSearchScroll = useCallback((el: HTMLDivElement | null) => {
    if (!el) return;
    const target = searchScrollRef.current;
    if (target <= 0) return;
    let tries = 0;
    const tick = () => {
      if (el.scrollHeight - el.clientHeight >= target) {
        el.scrollTop = target;
      } else if (tries++ < 40) {
        requestAnimationFrame(tick);
      }
    };
    requestAnimationFrame(tick);
  }, []);
  const [showOTPAuthenticator, setShowOTPAuthenticator] = useState(false);
  const [tabBehavior, setTabBehavior] = useState<TabBehavior>('new-tab');
  const [tabRefreshTrigger, setTabRefreshTrigger] = useState(0);
  const [mediaFullscreen, setMediaFullscreen] = useState(false);
  const [windowMaximized, setWindowMaximized] = useState(true); // Default to maximized

  // Local-AI on/off toggles, persisted across sessions. The curator gate is read
  // before firing page summarization; the assistant flag is forward-looking.
  const [aiSettings, setAiSettings] = useState<AiSettings>(loadAiSettings);
  const updateAiSettings = useCallback((next: Partial<AiSettings>) => {
    setAiSettings(prev => {
      const merged = { ...prev, ...next };
      try { localStorage.setItem(AI_SETTINGS_KEY, JSON.stringify(merged)); } catch { /* ignore */ }
      return merged;
    });
  }, []);

  // Autosave "Save password?" prompt, raised when the page captures a login submit.
  const [savePrompt, setSavePrompt] = useState<{ origin: string; username: string } | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);
  // Autofill "Fill login?" prompt, raised when a login page has a saved credential.
  const [fillPrompt, setFillPrompt] = useState<{ origin: string; username: string; locked: boolean } | null>(null);

  // Handle opening a URL - creates a new tab with that URL
  const handleOpenUrl = async (url: string) => {
    // Handle internal earth:// URLs - just create EarthSearch tab
    if (url.startsWith('earth://')) {
      try {
        const newTab = await invoke<Tab>('create_tab', {
          profileId: activeProfile?.id ?? 1,
          url: 'earth://search',
          title: 'Search',
        });
        await invoke('set_active_tab', { tabId: newTab.id });
        setActiveTab(newTab);
        setTabRefreshTrigger(prev => prev + 1);
        setActiveService('search');
      } catch (err) {
        console.error('Failed to create EarthSearch tab:', err);
      }
      return;
    }

    // Ensure URL has protocol for real URLs
    const fullUrl = url.startsWith('http') ? url : `https://${url}`;

    try {
      // Create a new tab with this URL
      const newTab = await invoke<Tab>('create_tab', {
        profileId: activeProfile?.id ?? 1,
        url: fullUrl,
        title: new URL(fullUrl).hostname,
      });
      // Set it as active
      await invoke('set_active_tab', { tabId: newTab.id });
      setActiveTab(newTab);
      // Trigger TabBar to refresh
      setTabRefreshTrigger(prev => prev + 1);
      // Switch to search service
      setActiveService('search');
    } catch (err) {
      console.error('Failed to open URL in new tab:', err);
    }
  };

  // Load active profile on mount and set up keyboard listeners
  useEffect(() => {
    loadActiveProfile();
    loadIncognitoStatus();

    // Check initial window maximized state
    isWindowMaximized().then(setWindowMaximized);

    // Listen for window resize to update maximized state
    let unlistenResize: (() => void) | null = null;
    onWindowResize((isMaximized) => {
      setWindowMaximized(isMaximized);
    }).then((unlisten) => {
      unlistenResize = unlisten;
    });

    // F11 fullscreen toggle and F12 dev tools handler
    // Use capture phase to catch events before WebKitGTK can intercept them
    const handleKeyDown = (e: KeyboardEvent) => {
      console.log('Key pressed:', e.key, e.code);
      if (e.key === 'F11' || e.code === 'F11') {
        e.preventDefault();
        e.stopPropagation();
        console.log('F11 pressed - toggling fullscreen');
        toggleFullscreen();
      } else if (e.key === 'F12' || e.code === 'F12') {
        e.preventDefault();
        e.stopPropagation();
        console.log('F12 pressed - toggling devtools');
        toggleDevTools();
      }
    };

    // Use capture: true to intercept before WebKitGTK handles it
    window.addEventListener('keydown', handleKeyDown, { capture: true });
    document.addEventListener('keydown', handleKeyDown, { capture: true });
    return () => {
      window.removeEventListener('keydown', handleKeyDown, { capture: true });
      document.removeEventListener('keydown', handleKeyDown, { capture: true });
      if (unlistenResize) unlistenResize();
    };
  }, []);

  const loadActiveProfile = async () => {
    try {
      const profile = await invoke<Profile | null>('get_active_profile');
      setActiveProfile(profile);
    } catch (err) {
      console.error('Failed to load active profile:', err);
    }
  };

  const loadIncognitoStatus = async () => {
    if (!activeProfile?.id) return;
    try {
      const status = await invoke<boolean>('get_incognito_status', { profileId: activeProfile.id });
      setIsIncognito(status);
    } catch (err) {
      console.error('Failed to load incognito status:', err);
    }
  };

  // Reload incognito status when activeProfile changes
  useEffect(() => {
    loadIncognitoStatus();
  }, [activeProfile?.id]);

  const handleProfileChange = async (profile: Profile) => {
    setActiveProfile(profile);
    // Load incognito status for the new profile
    try {
      const status = await invoke<boolean>('get_incognito_status', { profileId: profile.id });
      setIsIncognito(status);
    } catch (err) {
      console.error('Failed to load incognito status for profile:', err);
      setIsIncognito(false); // Default to non-incognito on error
    }
  };

  const handleIncognitoChange = (status: boolean) => {
    setIsIncognito(status);
    // Incognito turns ON every privacy protection by default (max shield level).
    if (status && isTauri()) {
      invoke('privacy_set_config', {
        config: {
          blockWebrtc: true,
          blockThirdPartyCookies: true,
          trackingPrevention: true,
          blockDnsPrefetch: true,
          spoofUserAgent: true,
        },
      }).catch(() => {});
    }
  };

  // Track chrome height for webview positioning
  const [chromeHeight, setChromeHeight] = useState(0);
  // Render engine that drew each tab, reported by WebView after `navigate`
  // ('webkitgtk' | 'servo' | 'internal'). Drives surface show/hide below: a
  // `.earth` tab renders in Servo (separate window), so the embedded WebKitGTK
  // surface must stay HIDDEN for it even though its URL is http(s).
  const [engineByTab, setEngineByTab] = useState<Record<number, string>>({});
  // Right-docked panels (NoScript, Privacy, quick-bookmark, Downloads, etc.)
  // register here while open; the browser surface is then inset from the right so
  // they're visible (the native surface renders above the DOM and would otherwise
  // cover them).
  // Single-open enforcement: only one right-dock panel may be open at a time.
  // Each panel owns its own open state, so we close the previous one by calling
  // the onClose it registered (kept in `panelClosers`). `activePanelRef` tracks
  // which panel is currently open (synchronously, ahead of state) so opening B
  // can close A in the same tick.
  const panelClosers = useRef<Record<string, () => void>>({});
  const panelWidths = useRef<Record<string, number>>({});
  const activePanelRef = useRef<string | null>(null);
  // Width (CSS px) of the currently open panel — drives the surface inset so the
  // page shrinks by exactly the panel's width. 0 when no panel is open.
  const [activePanelWidth, setActivePanelWidth] = useState(0);
  const setPanelOpen = useCallback(
    (id: string, open: boolean, opts?: { onClose?: () => void; width?: number }) => {
      if (opts?.onClose) panelClosers.current[id] = opts.onClose;
      if (opts?.width) panelWidths.current[id] = opts.width;
      if (open) {
        const prev = activePanelRef.current;
        if (prev && prev !== id) panelClosers.current[prev]?.();
        activePanelRef.current = id;
        setActivePanelWidth(panelWidths.current[id] ?? RIGHT_DOCK_WIDTH);
      } else if (activePanelRef.current === id) {
        activePanelRef.current = null;
        setActivePanelWidth(0);
      }
    },
    [],
  );
  const rightDock = useMemo(
    () => ({ setOpen: setPanelOpen, top: Math.max(chromeHeight, 56) }),
    [setPanelOpen, chromeHeight],
  );
  // Inset the browser surface by the open panel's width (0 when none open).
  const rightInset = activePanelWidth;
  // Page-load state for the centered loading spinner, driven by REAL WebKit load
  // events (`browser-load-changed`) — not a timer. `loadingTabId` is the tab whose
  // page is currently loading; the spinner shows only when it's the ACTIVE tab.
  const [loadingTabId, setLoadingTabId] = useState<number | null>(null);
  const loadTimeoutRef = useRef<number | undefined>(undefined);
  // Stable + guarded so reporting an engine never changes WebView's callback
  // identity (which would re-fire its navigate effect) and a no-op report
  // doesn't trigger a needless re-render.
  const handleTabEngine = useCallback((tabId: number, eng: string) => {
    setEngineByTab(prev => (prev[tabId] === eng ? prev : { ...prev, [tabId]: eng }));
  }, []);
  const chromeRef = useRef<HTMLDivElement>(null);

  // Update chrome height when layout changes
  useEffect(() => {
    const updateChromeHeight = () => {
      if (chromeRef.current) {
        // Use the chrome's BOTTOM edge (viewport coords), not its height: the
        // incognito banner sits ABOVE the chrome container, so the content/panels
        // must start below banner+chrome, not just chrome height (which left the
        // surface and right panels riding up into the URL bar in incognito).
        const bottom = chromeRef.current.getBoundingClientRect().bottom;
        setChromeHeight(bottom);
      }
    };

    updateChromeHeight();

    // Observe for size changes
    const resizeObserver = new ResizeObserver(updateChromeHeight);
    if (chromeRef.current) {
      resizeObserver.observe(chromeRef.current);
    }

    return () => resizeObserver.disconnect();
    // isIncognito: banner mounts/unmounts. activeTab?.url: the browser nav bar
    // appears only on real pages, changing chrome height. Both shift where the
    // native surface must start.
  }, [activeService, showBookmarkBar, navbarCollapsed, mediaFullscreen, isIncognito, activeTab?.url]);

  // Central browser-surface visibility. The embedded site webview is a single
  // shared X11 surface, so its show/hide is driven from the active-tab state in
  // ONE place — not from each WebView's mount/unmount, which fought over the
  // shared surface (closing one web tab would hide it for another). Show only
  // when actively browsing an external page; hide otherwise. Safe no-op before
  // the surface exists.
  //
  // This reflects INTENT (is this view supposed to show the page?). Hiding the
  // page DURING a load is owned by the backend (browser_surface) which keeps the
  // page hidden until it has painted, so a load's blank first paint never shows.
  useEffect(() => {
    if (!isTauri()) return;
    const url = activeTab?.url ?? '';
    // `.earth` tabs render in Servo's own window — never show the embedded
    // WebKitGTK surface for them, even though the URL is http(s).
    const engine = activeTab ? engineByTab[activeTab.id] : undefined;
    const isBrowsing =
      activeService === 'search' &&
      !mediaFullscreen &&
      engine !== 'servo' &&
      (url.startsWith('http://') || url.startsWith('https://'));
    invoke(isBrowsing ? 'browser_surface_show' : 'browser_surface_hide').catch(() => {});
  }, [activeService, activeTab?.url, activeTab?.id, mediaFullscreen, engineByTab]);

  // Loading spinner: driven by REAL WebKit load events emitted from the page
  // surface (`browser-load-changed`: started → finished/failed). Covers reloads,
  // URL navigations, back/forward and in-page link clicks alike — they all go
  // through WebKit. A safety timeout is a BACKSTOP so the surface can never get
  // stuck hidden if a finish/fail event is ever missed; it is not the trigger.
  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;
    const clearTimer = () => {
      if (loadTimeoutRef.current !== undefined) {
        clearTimeout(loadTimeoutRef.current);
        loadTimeoutRef.current = undefined;
      }
    };
    listen<{ tabId: number; phase: string }>('browser-load-changed', ({ payload }) => {
      if (payload.phase === 'started') {
        setLoadingTabId(payload.tabId);
        clearTimer();
        loadTimeoutRef.current = window.setTimeout(() => setLoadingTabId(null), 15000);
      } else {
        // finished | failed — keep the spinner up for a short settle so it hands
        // off exactly as the backend reveals the painted page (no dark gap, no
        // flash of the blank first paint). Matches REVEAL_SETTLE_MS in Rust.
        clearTimer();
        loadTimeoutRef.current = window.setTimeout(
          () => setLoadingTabId(prev => (prev === payload.tabId ? null : prev)),
          240,
        );
      }
    }).then(u => { unlisten = u; });
    return () => { unlisten?.(); clearTimer(); };
  }, []);

  // On launch, check GitHub for a newer release.
  useEffect(() => {
    if (!isTauri()) return;
    invoke<{ current: string; latest: string; updateAvailable: boolean; url: string }>('check_for_update')
      .then(setUpdateInfo)
      .catch(() => {});
  }, []);

  // KG curator: when a real page loads (and we're not incognito), ask the backend
  // to fetch + summarize it into EarthMemory. Deduped per-URL (the title event
  // fires several times per page); the backend no-ops if Ollama isn't running.
  const curatedUrlRef = useRef<string>('');
  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;
    listen<{ tabId: number; title: string; url: string }>('browser-title-changed', ({ payload }) => {
      const url = payload.url || '';
      if (!aiSettings.curator || !/^https?:\/\//.test(url) || isIncognito) return;
      if (curatedUrlRef.current === url) return;
      curatedUrlRef.current = url;
      invoke('curate_page', {
        profileId: activeProfile?.id ?? 1,
        url,
        title: payload.title || url,
      }).catch(() => {});
    }).then(u => { unlisten = u; });
    return () => unlisten?.();
  }, [isIncognito, activeProfile?.id, aiSettings.curator]);

  // Autofill: when a page with a login form asks, look up saved credentials and
  // fill them. `vault_find_login` returns null unless the password vault is
  // unlocked, so a locked vault simply never fills.
  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;
    listen<{ origin: string }>('autofill-request', async ({ payload }) => {
      try {
        // hint = [username, locked] if a saved login exists for this site (works
        // even when locked, since it doesn't decrypt). Show a prompt so you're
        // asked — and told if the vault needs unlocking.
        const hint = await invoke<[string, boolean] | null>('vault_login_hint', {
          profileId: activeProfile?.id ?? 1,
          origin: payload.origin,
        });
        if (hint) setFillPrompt({ origin: payload.origin, username: hint[0], locked: hint[1] });
      } catch { /* no saved login — skip */ }
    }).then(u => { unlisten = u; });
    return () => unlisten?.();
  }, [activeProfile?.id]);

  // Autosave: a login submit was captured — offer to save it (the password stays
  // in the backend; only origin + username reach the UI).
  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;
    listen<{ origin: string; username: string }>('autofill-save-request', async ({ payload }) => {
      try {
        // Skip the prompt if this exact login is already saved (e.g. you just
        // autofilled it and submitted unchanged).
        const isNew = await invoke<boolean>('vault_autosave_is_new', { profileId: activeProfile?.id ?? 1 });
        if (!isNew) {
          invoke('vault_autosave_dismiss').catch(() => {});
          return;
        }
      } catch { /* fall through and prompt */ }
      setSaveError(null);
      setSavePrompt({ origin: payload.origin, username: payload.username });
    }).then(u => { unlisten = u; });
    return () => unlisten?.();
  }, [activeProfile?.id]);


  // Real page titles: the embedded surface emits `browser-title-changed` once a
  // page loads (and on later title changes). Persist the title to the tab and
  // refresh the TabBar so it shows the real title instead of just the hostname.
  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;
    listen<{ tabId: number; title: string; url: string }>('browser-title-changed', ({ payload }) => {
      if (!payload.title || payload.tabId < 0) return;
      invoke('update_tab', { tabId: payload.tabId, title: payload.title }).catch(() => {});
      // Track the page's live URL too so the URL bar follows in-page navigation
      // (links/redirects), not just the address the tab was opened with. Only
      // accept real http(s) URLs; navigating to the same URL is a no-op reload-
      // skip downstream, so this won't loop.
      const liveUrl = /^https?:\/\//.test(payload.url) ? payload.url : undefined;
      setActiveTab(prev =>
        prev && prev.id === payload.tabId
          ? { ...prev, title: payload.title, url: liveUrl ?? prev.url }
          : prev,
      );
      setTabRefreshTrigger(prev => prev + 1);
    }).then(u => { unlisten = u; });
    return () => unlisten?.();
  }, []);

  // Restore the last active tab on startup. The TabBar highlights it from its own
  // state, but App's activeTab must be set authoritatively here or the app shows
  // the search page while the right tab looks selected. Runs when the profile
  // resolves (the TabBar→App notify was unreliable across the async profile load).
  useEffect(() => {
    if (!isTauri() || !activeProfile?.id) return;
    invoke<Tab[]>('get_all_tabs', { profileId: activeProfile.id })
      .then(tabs => {
        const active = tabs.find(t => t.is_active) ?? tabs[0];
        if (active) {
          setActiveTab(active);
          if (active.url && !active.url.startsWith('earth://')) setActiveService('search');
        }
      })
      .catch(() => {});
  }, [activeProfile?.id]);

  // NoScript (Phase 1): the web-process extension reports each distinct request
  // origin it sees on a page. For now we just log them to confirm the extension
  // loads and enumerates scripts; Phase 3 feeds these into the per-site modal.
  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;
    listen<string>('noscript-origin', ({ payload }) => {
      console.log('[noscript] origin seen on page:', payload);
    }).then(u => { unlisten = u; });
    return () => unlisten?.();
  }, []);

  // Spinner shows exactly when the active page is loading AND the surface would
  // otherwise be visible (browsing a webkit page, not Servo / media-fullscreen) —
  // i.e. the cases where we hide the surface for it above.
  const spinnerVisible =
    loadingTabId !== null &&
    loadingTabId === activeTab?.id &&
    activeService === 'search' &&
    !mediaFullscreen &&
    (activeTab ? engineByTab[activeTab.id] : undefined) !== 'servo';

  return (
    <Router>
      <Routes>
        {/* Floating media controls window - separate route for Tauri webview */}
        <Route path="/media-controls" element={<MediaControls />} />

        {/* Main app route */}
        <Route path="*" element={
    <ThemeProvider profileId={activeProfile?.id ?? null}>
      <RightDockContext.Provider value={rightDock}>
      <BrowserProvider profileId={activeProfile?.id ?? 1}>
        <div
          className={`h-screen flex flex-col overflow-hidden relative ${
            isIncognito
              ? 'bg-gradient-to-br from-purple-950 via-gray-900 to-purple-900'
              : 'bg-theme-gradient'
          }`}
          style={{
            borderTopLeftRadius: !windowMaximized ? '12px' : 0,
            borderTopRightRadius: !windowMaximized ? '12px' : 0,
          }}
        >
          {/* Animated Background Layer - Hide when media fullscreen */}
          {!mediaFullscreen && <ThemedAnimationLayer enabled={!isIncognito} />}

          {/* Page-loading spinner — centered over the WEBVIEW region (top edge =
              chrome height, inset by any docked panel). Driven by real WebKit
              load state; the native surface is hidden while it shows. */}
          <PageLoadSpinner visible={spinnerVisible} top={chromeHeight} rightInset={rightInset} />

          {/* Autofill "Fill login?" prompt — shown when a login page has a saved
              credential. Surfaces the locked state instead of silently doing nothing. */}
          <RightDockPanel
            id="autofill-login"
            open={!!fillPrompt}
            width={RIGHT_DOCK_WIDTH}
            title="Fill login?"
            onClose={() => setFillPrompt(null)}
          >
            {fillPrompt && (
              <div className="flex flex-col gap-3 p-2">
                <div className="text-sm text-gray-300">
                  Saved login for{' '}
                  <span className="font-medium text-white">{(() => { try { return new URL(fillPrompt.origin).host; } catch { return fillPrompt.origin; } })()}</span>
                  {fillPrompt.username && <span className="text-gray-400"> ({fillPrompt.username})</span>}
                </div>
                {fillPrompt.locked ? (
                  <>
                    <div className="text-xs text-yellow-400">The password vault is locked — unlock it to fill.</div>
                    <button
                      onClick={() => { setShowPasswordManager(true); setFillPrompt(null); }}
                      className="px-3 py-2 rounded-lg text-sm bg-[var(--primary-color)] text-white hover:opacity-90 transition-opacity"
                    >
                      Open Password Manager
                    </button>
                  </>
                ) : (
                  <div className="flex items-center gap-2">
                    <button
                      onClick={async () => {
                        try { await invoke('vault_autofill', { profileId: activeProfile?.id ?? 1, origin: fillPrompt.origin }); } catch { /* ignore */ }
                        setFillPrompt(null);
                      }}
                      className="flex-1 px-3 py-2 rounded-lg text-sm bg-[var(--primary-color)] text-white hover:opacity-90 transition-opacity"
                    >
                      Fill
                    </button>
                    <button
                      onClick={() => setFillPrompt(null)}
                      className="px-3 py-2 rounded-lg text-sm text-gray-300 hover:bg-white/10 transition-colors"
                    >
                      No
                    </button>
                  </div>
                )}
              </div>
            )}
          </RightDockPanel>

          {/* Autosave "Save password?" prompt — a right-dock panel so it squishes
              the page (insets the native surface) instead of hiding it. */}
          <RightDockPanel
            id="save-password"
            open={!!savePrompt}
            width={RIGHT_DOCK_WIDTH}
            title="Save password?"
            onClose={() => { invoke('vault_autosave_dismiss').catch(() => {}); setSavePrompt(null); setSaveError(null); }}
          >
            {savePrompt && (
              <div className="flex flex-col gap-3 p-2">
                <div className="text-sm text-gray-300">
                  Save this login for{' '}
                  <span className="font-medium text-white">{(() => { try { return new URL(savePrompt.origin).host; } catch { return savePrompt.origin; } })()}</span>?
                </div>
                {savePrompt.username && (
                  <div className="text-sm">
                    <span className="text-gray-500">Username: </span>
                    <span className="text-white">{savePrompt.username}</span>
                  </div>
                )}
                <div className="text-xs text-gray-500">The password is kept in the backend and never shown here.</div>
                {saveError && <div className="text-xs text-red-400">{saveError.replace(/^.*?:\s*/, '')}</div>}
                <div className="flex items-center gap-2 mt-1">
                  <button
                    onClick={async () => {
                      try {
                        await invoke('vault_autosave_confirm', { profileId: activeProfile?.id ?? 1 });
                        setSavePrompt(null);
                        setSaveError(null);
                      } catch (e) {
                        setSaveError(String(e));
                      }
                    }}
                    className="flex-1 px-3 py-2 rounded-lg text-sm bg-[var(--primary-color)] text-white hover:opacity-90 transition-opacity"
                  >
                    Save
                  </button>
                  <button
                    onClick={() => { invoke('vault_autosave_dismiss').catch(() => {}); setSavePrompt(null); setSaveError(null); }}
                    className="px-3 py-2 rounded-lg text-sm text-gray-300 hover:bg-white/10 transition-colors"
                  >
                    Not now
                  </button>
                </div>
              </div>
            )}
          </RightDockPanel>

          {/* Browser Chrome Container - Overlays on top of webview with high z-index.
              The incognito banner lives INSIDE this container so chrome-height
              measurement (and therefore the native surface's top edge + docked
              panel `top`) tracks the banner expanding/collapsing — a ResizeObserver
              on chromeRef only fires for size changes of its own subtree, not for a
              sibling above it pushing it down. */}
          <div
            ref={chromeRef}
            className="relative z-[100] flex flex-col"
            style={{ pointerEvents: 'auto' }}
          >
          {/* Update available banner (from GitHub releases) */}
          {updateInfo?.updateAvailable && !updateDismissed && (
            <div className="flex items-center gap-3 px-4 py-1.5 text-xs bg-[var(--primary-color)]/20 border-b border-[var(--primary-color)]/30 text-white">
              <span className="flex-1">
                A new version of Reclaim is available — <span className="font-semibold">v{updateInfo.latest}</span> (you have v{updateInfo.current}).
              </span>
              <button
                onClick={() => invoke('open_in_system_browser', { url: updateInfo.url }).catch(() => window.open(updateInfo.url, '_blank'))}
                className="px-2 py-0.5 rounded bg-[var(--primary-color)] text-white hover:opacity-90"
              >
                Download
              </button>
              <button onClick={() => setUpdateDismissed(true)} className="text-white/70 hover:text-white" title="Dismiss">✕</button>
            </div>
          )}

          {/* Incognito Banner - Hide when media fullscreen */}
          {!mediaFullscreen && <IncognitoBanner isVisible={isIncognito} />}
          {/* Main Navbar - EarthSocial Style (Collapsible + Draggable) - Hide when media fullscreen */}
          {!mediaFullscreen && (
          <nav
            className={`sticky top-0 z-50 w-full border-b backdrop-blur-xl transition-all duration-300 overflow-hidden select-none`}
            style={{
              backgroundColor: isIncognito ? 'rgba(88, 28, 135, 0.9)' : 'var(--color-navbar, #0a0a0f)',
              borderColor: 'rgba(255, 255, 255, 0.15)',
              height: navbarCollapsed ? '28px' : 'auto',
              borderTopLeftRadius: !windowMaximized ? '12px' : 0,
              borderTopRightRadius: !windowMaximized ? '12px' : 0,
            }}
            onMouseDown={(e) => {
              // Only start dragging with left mouse button on navbar background
              if (e.button !== 0) return; // Only left click
              const target = e.target as HTMLElement;
              if (target.closest('button') === null &&
                  target.closest('input') === null &&
                  target.closest('[data-no-drag]') === null &&
                  target.closest('a') === null &&
                  target.closest('select') === null) {
                e.preventDefault();
                console.log('Starting window drag');
                startDragging();
              }
            }}
          >
            <div className="w-full pl-3 sm:pl-4 lg:pl-6 xl:pl-10 pr-2" data-tauri-drag-region>
              <div className={`flex items-center justify-between transition-all duration-300 ${navbarCollapsed ? 'h-7' : 'h-14 lg:h-16 xl:h-20'}`} data-tauri-drag-region>
                {/* Left Side - Logo/Title */}
                <PointerButton
                  action={() => !navbarCollapsed && setShowAbout(true)}
                  title="About Reclaim"
                  className={`flex-shrink-0 group transition-all duration-300 flex items-center gap-2 lg:gap-3 bg-transparent border-0 ${navbarCollapsed ? 'opacity-0 w-0 overflow-hidden' : 'opacity-100'}`}
                  icon={
                    <>
                      <img
                        src={logoSvg}
                        alt="EarthServers"
                        className="w-8 h-8 sm:w-10 sm:h-10 lg:w-12 lg:h-12 xl:w-14 xl:h-14 transition-transform group-hover:scale-105 pointer-events-none"
                      />
                      <h1 className="text-xl sm:text-2xl lg:text-3xl xl:text-4xl font-bold tracking-tight transition-transform group-hover:scale-105 pointer-events-none">
                        <span className="text-white">Reclaim</span>
                      </h1>
                    </>
                  }
                />

                {/* Spacer for collapsed state - fills center */}
                {navbarCollapsed && <div className="flex-1" data-tauri-drag-region />}

                {/* Center - Service Navigation (hidden when collapsed) */}
                {!navbarCollapsed && (
                <div className="flex items-center gap-1 sm:gap-1.5 lg:gap-2">
                  {/* Main service items */}
                  {mainServiceItems.map((item) => (
                    <PointerButton
                      key={item.id}
                      action={() => setActiveService(item.id)}
                      title={item.label}
                      className={`
                        flex items-center gap-1 sm:gap-1.5 px-2 sm:px-3 py-1.5 sm:py-2 rounded-lg font-semibold text-xs sm:text-sm
                        border transition-all duration-200 whitespace-nowrap
                        ${activeService === item.id
                          ? isIncognito
                            ? 'bg-purple-500 border-purple-400 text-white shadow-lg shadow-purple-500/30'
                            : 'bg-white/20 border-white/30 text-white shadow-lg'
                          : 'bg-white/10 border-white/20 text-white/80 hover:bg-white/15 hover:border-white/25'
                        }
                      `}
                      icon={
                        <>
                          <svg className="w-4 h-4 pointer-events-none" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                            <path strokeLinecap="round" strokeLinejoin="round" d={item.icon} />
                          </svg>
                          <span className="hidden sm:inline pointer-events-none">{item.label}</span>
                        </>
                      }
                    />
                  ))}
                </div>
                )}

                {/* Right Side - Controls */}
                <div className={`flex items-center transition-all duration-300 ${navbarCollapsed ? 'gap-0.5' : 'gap-0.5 sm:gap-1 lg:gap-1.5 xl:gap-2'}`}>
                  {/* Search-tab-only tools: password manager, authenticator, notes, theme */}
                  {activeService === 'search' && (<>
                  {/* Password Manager Button - hidden on small screens */}
                  <button
                    onClick={() => setShowPasswordManager(!showPasswordManager)}
                    className={`hidden md:block rounded-lg border transition-all ${navbarCollapsed ? 'p-0.5 opacity-0 w-0 overflow-hidden' : 'p-1 sm:p-1.5 lg:p-2 opacity-100'} ${
                      showPasswordManager
                        ? 'bg-white/20 border-white/30 text-white'
                        : 'bg-white/10 border-white/20 text-white/80 hover:bg-white/15 hover:text-white'
                    }`}
                    title="Password Manager"
                  >
                    <svg className="w-3.5 h-3.5 lg:w-4 lg:h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        strokeWidth={2}
                        d="M15 7a2 2 0 012 2m4 0a6 6 0 01-7.743 5.743L11 17H9v2H7v2H4a1 1 0 01-1-1v-2.586a1 1 0 01.293-.707l5.964-5.964A6 6 0 1121 9z"
                      />
                    </svg>
                  </button>

                  {/* Media downloader — only while viewing a web page */}
                  {activeTab?.url && !activeTab.url.startsWith('earth://') && (
                  <button
                    onClick={() => setShowMediaPanel(v => !v)}
                    className={`hidden md:block rounded-lg border transition-all ${navbarCollapsed ? 'p-0.5 opacity-0 w-0 overflow-hidden' : 'p-1 sm:p-1.5 lg:p-2 opacity-100'} ${
                      showMediaPanel
                        ? 'bg-white/20 border-white/30 text-white'
                        : 'bg-white/10 border-white/20 text-white/80 hover:bg-white/15 hover:text-white'
                    }`}
                    title="Download media on this page"
                  >
                    <svg className="w-3.5 h-3.5 lg:w-4 lg:h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z" />
                    </svg>
                  </button>
                  )}

                  {/* OTP Authenticator Button - hidden on small screens */}
                  <button
                    onClick={() => setShowOTPAuthenticator(!showOTPAuthenticator)}
                    className={`hidden md:block rounded-lg border transition-all ${navbarCollapsed ? 'p-0.5 opacity-0 w-0 overflow-hidden' : 'p-1 sm:p-1.5 lg:p-2 opacity-100'} ${
                      showOTPAuthenticator
                        ? 'bg-white/20 border-white/30 text-white'
                        : 'bg-white/10 border-white/20 text-white/80 hover:bg-white/15 hover:text-white'
                    }`}
                    title="Authenticator"
                  >
                    <svg className="w-3.5 h-3.5 lg:w-4 lg:h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        strokeWidth={2}
                        d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z"
                      />
                    </svg>
                  </button>

                  {/* Notes Button - hidden on small screens */}
                  <button
                    onClick={() => setShowNotes(!showNotes)}
                    className={`hidden sm:block rounded-lg border transition-all ${navbarCollapsed ? 'p-0.5 opacity-0 w-0 overflow-hidden' : 'p-1 sm:p-1.5 lg:p-2 opacity-100'} ${
                      showNotes
                        ? 'bg-white/20 border-white/30 text-white'
                        : 'bg-white/10 border-white/20 text-white/80 hover:bg-white/15 hover:text-white'
                    }`}
                    title="Notes"
                  >
                    <svg className="w-3.5 h-3.5 lg:w-4 lg:h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        strokeWidth={2}
                        d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z"
                      />
                    </svg>
                  </button>

                  {/* Theme Customizer Button - hidden on small screens */}
                  <button
                    onClick={() => setShowThemeCustomizer(!showThemeCustomizer)}
                    className={`hidden sm:block rounded-lg border transition-all ${navbarCollapsed ? 'p-0.5 opacity-0 w-0 overflow-hidden' : 'p-1 sm:p-1.5 lg:p-2 opacity-100'} ${
                      showThemeCustomizer
                        ? 'bg-white/20 border-white/30 text-white'
                        : 'bg-white/10 border-white/20 text-white/80 hover:bg-white/15 hover:text-white'
                    }`}
                    title="Customize Theme"
                  >
                    <svg className="w-3.5 h-3.5 lg:w-4 lg:h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        strokeWidth={2}
                        d="M7 21a4 4 0 01-4-4V5a2 2 0 012-2h4a2 2 0 012 2v12a4 4 0 01-4 4zm0 0h12a2 2 0 002-2v-4a2 2 0 00-2-2h-2.343M11 7.343l1.657-1.657a2 2 0 012.828 0l2.829 2.829a2 2 0 010 2.828l-8.486 8.485M7 17h.01"
                      />
                    </svg>
                  </button>
                  </>)}

                  {/* History Button - hidden on very small screens */}
                  <button
                    onClick={() => setShowHistory(true)}
                    className={`hidden xs:block rounded-lg bg-white/10 border border-white/20 text-white/80 hover:bg-white/15 hover:text-white transition-all ${navbarCollapsed ? 'p-0.5 opacity-0 w-0 overflow-hidden' : 'p-1 sm:p-1.5 lg:p-2 opacity-100'}`}
                    title="View History"
                    style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}
                  >
                    <svg className="w-3.5 h-3.5 lg:w-4 lg:h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        strokeWidth={2}
                        d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z"
                      />
                    </svg>
                  </button>

                  {/* Navbar Collapse Toggle - always visible */}
                  <button
                    onClick={() => setNavbarCollapsed(!navbarCollapsed)}
                    className="p-1 sm:p-1.5 lg:p-2 rounded-lg bg-white/10 border border-white/20 text-white/80 hover:bg-white/15 hover:text-white transition-all"
                    title={navbarCollapsed ? 'Expand navbar' : 'Collapse navbar'}
                    style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}
                  >
                    <svg
                      className={`w-3.5 h-3.5 lg:w-4 lg:h-4 transition-transform duration-300 ${navbarCollapsed ? 'rotate-180' : ''}`}
                      fill="none"
                      viewBox="0 0 24 24"
                      stroke="currentColor"
                    >
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 15l7-7 7 7" />
                    </svg>
                  </button>

                  {/* Privacy protections — search tab only */}
                  {activeService === 'search' && (
                  <div
                    className={`transition-all duration-300 ${navbarCollapsed ? 'opacity-0 w-0 overflow-hidden pointer-events-none' : 'opacity-100'}`}
                    style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}
                  >
                    <PrivacyButton isIncognito={isIncognito} />
                  </div>
                  )}

                  <div
                    className={`transition-all duration-300 ${navbarCollapsed ? 'opacity-0 w-0 overflow-hidden pointer-events-none' : 'opacity-100'}`}
                    style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}
                  >
                    <DownloadsButton />
                  </div>

                  {/* Incognito Toggle */}
                  <div
                    className={`transition-all duration-300 ${navbarCollapsed ? 'opacity-0 w-0 overflow-hidden pointer-events-none' : 'opacity-100'}`}
                    style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}
                  >
                    {activeProfile?.id && <IncognitoToggle profileId={activeProfile.id} onStatusChange={handleIncognitoChange} />}
                  </div>

                  {/* Profile Manager */}
                  <div className={`transition-all duration-300 ${navbarCollapsed ? 'opacity-0 w-0 overflow-hidden' : 'opacity-100'}`}>
                    <ProfileManager onProfileChange={handleProfileChange} />
                  </div>

                  {/* Window Controls (only in Tauri) */}
                  {isTauri() && (
                    <div
                      className="flex items-center gap-1 ml-2 pl-2 border-l border-white/20"
                      data-no-drag
                      style={{
                        WebkitAppRegion: 'no-drag',
                        pointerEvents: 'auto',
                        position: 'relative',
                        zIndex: 1000,
                      } as React.CSSProperties}
                    >
                      <PointerButton
                        action={minimizeWindow}
                        title="Minimize"
                        className="p-1.5 rounded-lg text-white/60 hover:bg-white/10 hover:text-white transition-all cursor-pointer"
                        icon={
                          <svg className="w-4 h-4 pointer-events-none" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                            <path strokeLinecap="round" strokeLinejoin="round" d="M20 12H4" />
                          </svg>
                        }
                      />
                      <PointerButton
                        action={maximizeWindow}
                        title="Maximize"
                        className="p-1.5 rounded-lg text-white/60 hover:bg-white/10 hover:text-white transition-all cursor-pointer"
                        icon={
                          <svg className="w-4 h-4 pointer-events-none" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                            <rect x="4" y="4" width="16" height="16" rx="2" />
                          </svg>
                        }
                      />
                      <PointerButton
                        action={closeWindow}
                        title="Close"
                        className="p-1.5 rounded-lg text-white/60 hover:bg-red-500/80 hover:text-white transition-all cursor-pointer"
                        icon={
                          <svg className="w-4 h-4 pointer-events-none" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                            <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
                          </svg>
                        }
                      />
                    </div>
                  )}
                </div>
              </div>
            </div>
          </nav>
          )}

          {/* Bookmark Bar - Only show for EarthSearch and not in media fullscreen */}
          {activeService === 'search' && showBookmarkBar && !mediaFullscreen && (
            <div className="flex items-center bg-black/20 backdrop-blur-sm border-b border-white/10">
              <BookmarkBar
                profileId={activeProfile?.id ?? 1}
                onNavigate={handleOpenUrl}
                onToggleManager={() => setShowBookmarkManager(true)}
              />
              {/* Toggle Bookmark Bar visibility */}
              <button
                onClick={() => setShowBookmarkBar(false)}
                className="px-2 py-1 mr-2 text-gray-400 hover:text-white transition-colors"
                title="Hide bookmark bar"
              >
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>
          )}

          {/* Tab Bar - Only show for EarthSearch and not in media fullscreen */}
          {activeService === 'search' && !mediaFullscreen && (
            <div className="flex items-center bg-black/30 backdrop-blur-sm border-b border-white/10">
              <TabBar
                profileId={activeProfile?.id ?? 1}
                onTabChange={(tab) => {
                  setActiveTab(tab);
                }}
                refreshTrigger={tabRefreshTrigger}
              />
              {/* Show bookmark bar toggle when hidden */}
              {!showBookmarkBar && (
                <button
                  onClick={() => setShowBookmarkBar(true)}
                  className="px-2 py-1 mr-2 text-gray-400 hover:text-white transition-colors"
                  title="Show bookmark bar"
                >
                  <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 5a2 2 0 012-2h10a2 2 0 012 2v16l-7-3.5L5 21V5z" />
                  </svg>
                </button>
              )}
            </div>
          )}

          {/* Browser Navigation Bar - Show when browsing external URL */}
          {activeService === 'search' && activeTab?.url && !activeTab.url.startsWith('earth://') && !mediaFullscreen && (
            <BrowserNavBar
              url={activeTab.url}
              tabId={activeTab.id}
              profileId={activeProfile?.id ?? 1}
              onNavigate={handleOpenUrl}
            />
          )}
          </div>{/* End Browser Chrome Container */}

          {/* Main Content */}
          <main className="flex-1 min-h-0 flex flex-col">
            {/* When browsing a real URL (not earth://), use full screen without padding */}
            {activeTab?.url && !activeTab.url.startsWith('earth://') ? (
              <div className="flex-1 min-h-0 flex flex-col">
                <Home
                  activeService={activeService}
                  profileId={activeProfile?.id ?? null}
                  onOpenUrl={handleOpenUrl}
                  activeTab={activeTab}
                  onMediaFullscreenChange={setMediaFullscreen}
                  chromeHeight={chromeHeight}
                  onEngine={handleTabEngine}
                  rightInset={rightInset}
                />
              </div>
            ) : activeService === 'media' ? (
              <div className="flex-1 min-h-0">
                <Home
                  activeService={activeService}
                  profileId={activeProfile?.id ?? null}
                  onOpenUrl={handleOpenUrl}
                  activeTab={activeTab}
                  onMediaFullscreenChange={setMediaFullscreen}
                  chromeHeight={chromeHeight}
                  onEngine={handleTabEngine}
                  rightInset={rightInset}
                />
              </div>
            ) : activeService === 'search' ? (
              <div
                className="flex-1 overflow-auto"
                ref={restoreSearchScroll}
                onScroll={(e) => { searchScrollRef.current = e.currentTarget.scrollTop; }}
              >
                <Home
                  activeService={activeService}
                  profileId={activeProfile?.id ?? null}
                  onOpenUrl={handleOpenUrl}
                  activeTab={activeTab}
                  onMediaFullscreenChange={setMediaFullscreen}
                  chromeHeight={chromeHeight}
                  onEngine={handleTabEngine}
                  rightInset={rightInset}
                  tabBehavior={tabBehavior}
                  onTabBehaviorChange={setTabBehavior}
                />
              </div>
            ) : (
              <div className="container mx-auto px-4 py-8 flex-1 overflow-auto">
                <Home
                  activeService={activeService}
                  profileId={activeProfile?.id ?? null}
                  onOpenUrl={handleOpenUrl}
                  activeTab={activeTab}
                  onMediaFullscreenChange={setMediaFullscreen}
                  chromeHeight={chromeHeight}
                  onEngine={handleTabEngine}
                  rightInset={rightInset}
                  aiSettings={aiSettings}
                  onAiSettingsChange={updateAiSettings}
                  onSelectService={setActiveService}
                />
              </div>
            )}
          </main>

          {/* Footer - Hidden when browsing a real URL or when media player is active */}
          {!(activeTab?.url && !activeTab.url.startsWith('earth://')) && activeService !== 'media' && (
          <footer className={`border-t ${isIncognito ? 'border-purple-500/20' : 'border-white/10'} bg-black/20 backdrop-blur-md`}>
            <div className="container mx-auto px-4 py-6">
              <div className="flex flex-col md:flex-row items-center justify-between gap-4">
                <div className="flex items-center gap-4">
                  <span className="text-2xl font-bold text-white">
                    Reclaim
                  </span>
                  <span className="text-gray-500">|</span>
                  <span className="text-sm text-gray-400">Digital Sovereignty</span>
                </div>
                <p className={`text-sm italic ${isIncognito ? 'text-purple-400' : 'text-theme-accent'}`}>
                  "We don't desire to rule the Earth. Only to serve it."
                </p>
              </div>
            </div>
          </footer>
          )}

          {/* History Viewer Modal */}
          <HistoryViewer
            profileId={activeProfile?.id ?? null}
            isOpen={showHistory}
            onClose={() => setShowHistory(false)}
          />

          {/* Theme Customizer Modal (Draggable, non-dimming) */}
          <ThemeCustomizer
            profileId={activeProfile?.id ?? null}
            isOpen={showThemeCustomizer}
            onClose={() => setShowThemeCustomizer(false)}
          />

          {/* Notes Plugin (Draggable, non-dimming) */}
          <NotesPlugin
            isOpen={showNotes}
            onClose={() => setShowNotes(false)}
          />

          {/* Bookmark Manager Modal */}
          <BookmarkManager
            profileId={activeProfile?.id ?? 1}
            isOpen={showBookmarkManager}
            onClose={() => setShowBookmarkManager(false)}
            onNavigate={(url) => console.log('Navigate to:', url)}
          />

          {/* Password Manager Modal */}
          <PasswordManager
            profileId={activeProfile?.id ?? 1}
            isOpen={showPasswordManager}
            onClose={() => setShowPasswordManager(false)}
          />

          <MediaPanel
            profileId={activeProfile?.id ?? 1}
            isOpen={showMediaPanel}
            onClose={() => setShowMediaPanel(false)}
            pageUrl={activeTab?.url}
          />

          {/* OTP Authenticator Modal */}
          <OTPAuthenticator
            profileId={activeProfile?.id ?? 1}
            isOpen={showOTPAuthenticator}
            onClose={() => setShowOTPAuthenticator(false)}
          />

          {/* About Modal */}
          {showAbout && (
            <div className="fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex items-center justify-center p-4">
              <div className="bg-gray-900 border border-white/10 rounded-2xl shadow-2xl w-full max-w-lg p-8">
                <div className="flex justify-between items-start mb-6">
                  <h2 className="text-3xl font-bold">
                    <span className="text-theme-primary">Re</span>
                    <span className="text-theme-secondary">claim</span>
                  </h2>
                  <button
                    onClick={() => setShowAbout(false)}
                    className="p-2 text-gray-400 hover:text-white transition-colors"
                  >
                    <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                    </svg>
                  </button>
                </div>

                <div className="space-y-4 text-gray-300">
                  <p className="text-lg font-medium text-white">
                    Reclaim your digital sovereignty. Reclaim your privacy. Reclaim the Earth from extractive technology.
                  </p>

                  <p className="text-sm">
                    Reclaim is a local-first AI platform that puts you back in control. Your data never leaves your device.
                    Your AI serves you, not shareholders. Your compute stays local, efficient, and environmentally conscious.
                  </p>

                  <div className="pt-4 border-t border-white/10">
                    <h3 className="text-sm font-semibold text-gray-400 mb-3">FEATURES</h3>
                    <ul className="space-y-2 text-sm">
                      <li className="flex items-center gap-2">
                        <span className="text-theme-primary">&#9679;</span>
                        <span><strong>Search</strong> - Curated, privacy-first search</span>
                      </li>
                      <li className="flex items-center gap-2">
                        <span className="text-theme-secondary">&#9679;</span>
                        <span><strong>EarthMemory</strong> - Personal knowledge graph</span>
                      </li>
                      <li className="flex items-center gap-2">
                        <span className="text-theme-accent">&#9679;</span>
                        <span><strong>EarthMultiMedia</strong> - Privacy-focused media player</span>
                      </li>
                    </ul>
                  </div>

                  <div className="pt-4 border-t border-white/10">
                    <p className="text-xs text-gray-500">
                      Built on principles of digital sovereignty, environmental responsibility, and human agency.
                    </p>
                  </div>
                </div>

                <button
                  onClick={() => setShowAbout(false)}
                  className="mt-6 w-full py-3 bg-theme-primary text-white rounded-lg hover:bg-theme-primary/80 transition-colors font-medium"
                >
                  Got it
                </button>
              </div>
            </div>
          )}
        </div>
      </BrowserProvider>
      </RightDockContext.Provider>
    </ThemeProvider>
        } />
      </Routes>
    </Router>
  );
}

function Home({ activeService, profileId, onOpenUrl, activeTab, onMediaFullscreenChange, chromeHeight, onEngine, rightInset, tabBehavior, onTabBehaviorChange, aiSettings, onAiSettingsChange, onSelectService }: {
  activeService: 'search' | 'memory' | 'media' | 'scraper' | 'ai';
  profileId: number | null;
  onOpenUrl?: (url: string) => void;
  activeTab?: Tab | null;
  onMediaFullscreenChange?: (isFullscreen: boolean) => void;
  chromeHeight?: number;
  onEngine?: (tabId: number, engine: string) => void;
  rightInset?: number;
  tabBehavior?: TabBehavior;
  onTabBehaviorChange?: (behavior: TabBehavior) => void;
  aiSettings?: AiSettings;
  onAiSettingsChange?: (next: Partial<AiSettings>) => void;
  onSelectService?: (service: 'search' | 'memory' | 'media' | 'scraper' | 'ai') => void;
}) {
  // Local AI hub (toggles + entry into the knowledge graph).
  if (activeService === 'ai' && aiSettings && onAiSettingsChange) {
    return (
      <LocalAIHub
        profileId={profileId}
        settings={aiSettings}
        onChange={onAiSettingsChange}
        onOpenMemory={() => onSelectService?.('memory')}
      />
    );
  }
  // EarthMultiMedia uses full height - no container padding
  if (activeService === 'media') {
    return (
      <div className="h-full w-full">
        <EarthMultiMedia profileId={profileId || 1} onFullscreenChange={onMediaFullscreenChange} />
      </div>
    );
  }

  // Web Scraper view
  if (activeService === 'scraper') {
    return (
      <div className="max-w-5xl mx-auto">
        <WebScraper profileId={profileId} />
      </div>
    );
  }

  // EarthSearch with integrated browser
  if (activeService === 'search') {
    // If active tab has a real URL (not earth://), show WebView
    if (activeTab?.url && !activeTab.url.startsWith('earth://')) {
      return (
        <WebView
          url={activeTab.url}
          tabId={activeTab.id}
          profileId={profileId || 1}
          onNavigate={(newUrl) => onOpenUrl?.(newUrl)}
          onTitleChange={(title) => console.log('Title:', title)}
          onEngine={onEngine}
          chromeHeight={chromeHeight}
          hideNavBar={true}
          rightInset={rightInset}
        />
      );
    }
    // Otherwise show normal DomainManager
    return (
      <div className="w-full py-8 px-4 flex justify-center">
        <div className="w-full max-w-5xl">
          {/* Link-open behavior selector — lives here (the search page) rather than
              the tab bar so it isn't covered by the native page surface. */}
          {onTabBehaviorChange && (
            <div className="mb-6 flex flex-wrap items-center justify-end gap-2">
              <span className="text-xs text-[var(--text-muted-color)] mr-1">When opening links:</span>
              {TAB_BEHAVIOR_OPTIONS.map(option => {
                const active = (tabBehavior ?? 'new-tab') === option.value;
                return (
                  <button
                    key={option.value}
                    onClick={() => onTabBehaviorChange(option.value)}
                    title={option.description}
                    className={`flex items-center gap-1.5 px-3 py-1.5 rounded-lg border text-xs transition-colors ${
                      active
                        ? 'border-[var(--primary-color)] bg-[var(--primary-color)]/10 text-[var(--text-color)]'
                        : 'border-white/10 hover:border-white/25 text-[var(--text-muted-color)] hover:text-[var(--text-color)]'
                    }`}
                  >
                    <span className="w-2.5 h-2.5 rounded-full" style={{ backgroundColor: option.color }} />
                    <span>{option.label}</span>
                  </button>
                );
              })}
            </div>
          )}
          <DomainManager profileId={profileId} onOpenUrl={onOpenUrl} />
        </div>
      </div>
    );
  }

  // EarthMemory
  return (
    <div className="max-w-5xl mx-auto">
      <MemoryManager profileId={profileId} />
    </div>
  );
}

export default App;
