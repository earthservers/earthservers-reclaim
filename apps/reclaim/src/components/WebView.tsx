// WebView Component for browsing websites within Reclaim
// GTK WEBVIEW PATTERN: Uses direct GTK manipulation for proper positioning on Linux

import { useState, useEffect, useRef, useCallback } from 'react';
import { invoke, isTauri, listen } from '../lib/tauri';
import { QuickBookmarkModal, Bookmark } from './BookmarkComponents';
import { RatingForm } from './RatingComponents';
import { NoscriptShield } from './NoscriptShield';

interface WebViewProps {
  url: string;
  tabId: number;
  profileId?: number;
  isActive?: boolean;
  onNavigate?: (newUrl: string) => void;
  onTitleChange?: (title: string) => void;
  onEngine?: (tabId: number, engine: string) => void; // Reports which render engine drew the tab (webkitgtk | servo | internal)
  chromeHeight?: number; // Height of browser chrome (navbar, tabs, bookmarks) for webview positioning
  hideNavBar?: boolean; // Hide the navigation bar (when it's rendered separately in chrome)
  rightInset?: number; // CSS px to shrink the surface from the right (for the docked NoScript panel)
}

// Generate a unique webview ID per tab - this ensures each tab has its own persistent webview
const getWebviewId = (tabId: number) => `browser-tab-${tabId}`;

export function WebView({
  url,
  tabId,
  profileId = 1,
  isActive = true,
  onNavigate,
  onTitleChange,
  onEngine,
  chromeHeight = 0,
  hideNavBar = false,
  rightInset = 0,
}: WebViewProps) {
  // Tab-specific webview ID - each tab gets its own persistent GTK webview
  const webviewId = getWebviewId(tabId);
  const [currentUrl, setCurrentUrl] = useState(url);
  const [inputUrl, setInputUrl] = useState(url);
  const [isLoading, setIsLoading] = useState(false);
  const [canGoBack, setCanGoBack] = useState(false);
  const [canGoForward, setCanGoForward] = useState(false);
  const [isBookmarked, setIsBookmarked] = useState(false);
  const [currentBookmark, setCurrentBookmark] = useState<Bookmark | null>(null);
  const [showBookmarkModal, setShowBookmarkModal] = useState(false);
  const [showRatingModal, setShowRatingModal] = useState(false);
  const [isInDomainList, setIsInDomainList] = useState(false);
  const [domainListLoading, setDomainListLoading] = useState(false);
  const [webviewError, setWebviewError] = useState<string | null>(null);
  // True when the active nav rendered in Servo (a separate OS window). The
  // in-app WebKitGTK surface is hidden in that case, so we show a notice here.
  const [servoActive, setServoActive] = useState(false);
  // Hold onEngine in a ref so it is NOT a dependency of createOrUpdateWebview.
  // (A fresh callback identity each render would otherwise re-fire the navigate
  // effect every render — an endless reload loop.)
  const onEngineRef = useRef(onEngine);
  onEngineRef.current = onEngine;

  const containerRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const iframeRef = useRef<HTMLIFrameElement>(null);
  const historyRef = useRef<string[]>([url]);
  const historyIndexRef = useRef(0);
  const webviewCreatedRef = useRef(false);
  const _resizeObserverRef = useRef<ResizeObserver | null>(null);
  void _resizeObserverRef; // Reserved for future resize handling
  const lastChromeHeightRef = useRef(0);

  // Check if URL is external (http/https)
  const isExternalUrl = (urlStr: string) => {
    return urlStr.startsWith('http://') || urlStr.startsWith('https://');
  };

  // Latest right-dock inset, read via a ref so computeBounds can subtract it
  // WITHOUT making rightInset a navigation dependency (which would reload the page
  // every time a panel opens/closes).
  const rightInsetRef = useRef(rightInset);
  rightInsetRef.current = rightInset;

  // Embedded-webview bounds: fill the window below the chrome (navbar/tabs/bookmarks),
  // inset from the right by any open dock panel so a navigation doesn't resize the
  // surface to full width and cover the panel.
  const computeBounds = useCallback(() => {
    const b = {
      x: 0,
      y: chromeHeight,
      width: Math.max(0, window.innerWidth - rightInsetRef.current),
      height: Math.max(0, window.innerHeight - chromeHeight),
    };
    return b;
  }, [chromeHeight]);

  // Navigate via the single router front door. The backend resolves the host
  // (resolution axis) and renders it in the engine chosen by domain class
  // (render axis: WebKitGTK today; Servo for .earth in a later phase).
  const createOrUpdateWebview = useCallback(async (targetUrl: string) => {
    if (!isTauri() || !isExternalUrl(targetUrl)) return;

    // Wait for chromeHeight to be calculated
    if (chromeHeight <= 0) {
      console.log('Waiting for chromeHeight...');
      return;
    }

    setIsLoading(true);
    setWebviewError(null);

    try {
      const outcome = await invoke<{ finalUrl: string; host: string; engine: string }>('navigate', {
        tabId,
        url: targetUrl,
        bounds: computeBounds(),
      });
      webviewCreatedRef.current = true;
      lastChromeHeightRef.current = chromeHeight;
      setIsLoading(false);
      // RENDER axis: `.earth` draws in Servo (a separate OS window); everything
      // else draws in the embedded WebKitGTK surface. Surface show/hide is driven
      // centrally in App.tsx from the reported engine — here we just report it and
      // flag the Servo case for the in-app notice.
      setServoActive(outcome.engine === 'servo');
      onEngineRef.current?.(tabId, outcome.engine);
      console.log(`[router] ${webviewId} -> ${outcome.finalUrl} (engine: ${outcome.engine})`);
    } catch (err) {
      console.error('Failed to navigate webview:', err);
      setWebviewError(String(err));
      setServoActive(false);
      setIsLoading(false);
    }
  }, [chromeHeight, tabId, webviewId, computeBounds]);

  // Re-apply the surface bounds whenever the chrome height, the NoScript panel
  // inset, or the WINDOW SIZE changes — so the embedded page follows window/
  // devtools resizes and shrinks to the left when the panel docks. Computed
  // inline (with the inset) and kept OUT of computeBounds so it never re-triggers
  // a navigation/reload.
  const applyBounds = useCallback(() => {
    if (!isTauri() || !webviewCreatedRef.current || chromeHeight <= 0) return;
    const bounds = {
      x: 0,
      y: chromeHeight,
      width: Math.max(0, window.innerWidth - rightInset),
      height: Math.max(0, window.innerHeight - chromeHeight),
    };
    lastChromeHeightRef.current = chromeHeight;
    invoke('browser_surface_set_bounds', { bounds }).catch(() => {});
  }, [chromeHeight, rightInset]);

  // Check if current URL is bookmarked
  const checkBookmarkStatus = async (urlToCheck: string) => {
    try {
      const bookmarkId = await invoke<number | null>('is_url_bookmarked', { profileId, url: urlToCheck });
      if (bookmarkId) {
        const bookmarks = await invoke<Bookmark[]>('get_all_bookmarks', { profileId });
        const bookmark = bookmarks.find(b => b.id === bookmarkId);
        setIsBookmarked(true);
        setCurrentBookmark(bookmark || null);
      } else {
        setIsBookmarked(false);
        setCurrentBookmark(null);
      }
    } catch {
      setIsBookmarked(false);
      setCurrentBookmark(null);
    }
  };

  // Check if domain is in the domain list
  const checkDomainListStatus = async (urlToCheck: string) => {
    if (!isExternalUrl(urlToCheck)) {
      setIsInDomainList(false);
      return;
    }
    try {
      const norm = (h: string) => h.replace(/^www\./, '');
      const hostname = norm(new URL(urlToCheck).hostname);
      // search_domain_list is the real command; bare `search_domains` is a stub.
      const domains = await invoke<{ id: number; url: string }[]>('search_domain_list', {
        profileId,
        query: hostname,
      });
      const isFound = domains.some(d => {
        try {
          const domainHost = norm(new URL(d.url).hostname);
          return domainHost === hostname || hostname.endsWith('.' + domainHost) || domainHost.endsWith('.' + hostname);
        } catch {
          return d.url.includes(hostname);
        }
      });
      setIsInDomainList(isFound);
    } catch {
      setIsInDomainList(false);
    }
  };

  // Add current domain to the domain list
  const addToDomainList = async () => {
    if (!isExternalUrl(currentUrl)) return;
    setDomainListLoading(true);
    try {
      const urlObj = new URL(currentUrl);
      const hostname = urlObj.hostname;
      const baseUrl = `${urlObj.protocol}//${hostname}`;

      // add_domain_entry actually persists (the bare `add_domain` is a no-op stub).
      await invoke('add_domain_entry', {
        url: baseUrl,
        category: 'general',
        trustScore: 0.5,
        profileId,
      });
      setIsInDomainList(true);
    } catch (err) {
      console.error('Failed to add domain:', err);
    } finally {
      setDomainListLoading(false);
    }
  };

  // Initialize webview when URL changes or component mounts
  useEffect(() => {
    setCurrentUrl(url);
    setInputUrl(url);
    checkBookmarkStatus(url);
    checkDomainListStatus(url);

    if (isActive && isExternalUrl(url) && chromeHeight > 0) {
      // Small delay to ensure the container is properly sized
      const timer = setTimeout(() => {
        createOrUpdateWebview(url);
      }, 100);
      return () => clearTimeout(timer);
    }
  }, [url, isActive, chromeHeight, createOrUpdateWebview]);

  // Re-fit the surface on chrome-height / inset changes AND on window resize.
  useEffect(() => {
    applyBounds();
    window.addEventListener('resize', applyBounds);
    return () => window.removeEventListener('resize', applyBounds);
  }, [applyBounds]);

  // NOTE: surface visibility (show/hide) is managed centrally in App.tsx from the
  // active-tab state, since the embedded webview is a single shared X11 surface.

  // Backfill the real page <title> after load. The backend emits
  // `browser-title-changed` once navigation completes (emission lands in a later
  // phase); until then titles stay at the provisional hostname.
  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;
    listen<{ tabId: number; title: string; url: string }>('browser-title-changed', ({ payload }) => {
      if (payload.tabId === tabId && payload.title) {
        onTitleChange?.(payload.title);
      }
    }).then((u) => { unlisten = u; });
    return () => { unlisten?.(); };
  }, [tabId, onTitleChange]);

  const navigateTo = async (targetUrl: string) => {
    // Ensure URL has protocol
    let fullUrl = targetUrl;
    if (!targetUrl.startsWith('http://') && !targetUrl.startsWith('https://') && !targetUrl.startsWith('earth://')) {
      if (targetUrl.includes('.') && !targetUrl.includes(' ')) {
        fullUrl = `https://${targetUrl}`;
      } else {
        fullUrl = `https://duckduckgo.com/?q=${encodeURIComponent(targetUrl)}`;
      }
    }

    setCurrentUrl(fullUrl);
    setInputUrl(fullUrl);

    // Update history
    const newHistory = historyRef.current.slice(0, historyIndexRef.current + 1);
    newHistory.push(fullUrl);
    historyRef.current = newHistory;
    historyIndexRef.current = newHistory.length - 1;

    setCanGoBack(historyIndexRef.current > 0);
    setCanGoForward(false);

    onNavigate?.(fullUrl);

    if (isExternalUrl(fullUrl)) {
      if (isTauri()) {
        setIsLoading(true);
        try {
          // Single router front door (resolution + render axes).
          const outcome = await invoke<{ finalUrl: string; engine: string }>('navigate', {
            tabId,
            url: fullUrl,
            bounds: computeBounds(),
          });
          webviewCreatedRef.current = true;
          setIsLoading(false);
          console.log(`[router] -> ${outcome.finalUrl} (engine: ${outcome.engine})`);
        } catch (err) {
          console.error('Navigation failed:', err);
          setIsLoading(false);
        }
      } else if (iframeRef.current) {
        setIsLoading(true);
        iframeRef.current.src = fullUrl;
      }
    }
  };

  const goBack = async () => {
    if (isTauri() && webviewCreatedRef.current) {
      try {
        await invoke('browser_surface_back');
        // Update local state
        if (historyIndexRef.current > 0) {
          historyIndexRef.current--;
          const prevUrl = historyRef.current[historyIndexRef.current];
          setCurrentUrl(prevUrl);
          setInputUrl(prevUrl);
          setCanGoBack(historyIndexRef.current > 0);
          setCanGoForward(true);
          onNavigate?.(prevUrl);
        }
      } catch (err) {
        console.error('Go back failed:', err);
      }
    }
  };

  const goForward = async () => {
    if (isTauri() && webviewCreatedRef.current) {
      try {
        await invoke('browser_surface_forward');
        // Update local state
        if (historyIndexRef.current < historyRef.current.length - 1) {
          historyIndexRef.current++;
          const nextUrl = historyRef.current[historyIndexRef.current];
          setCurrentUrl(nextUrl);
          setInputUrl(nextUrl);
          setCanGoBack(true);
          setCanGoForward(historyIndexRef.current < historyRef.current.length - 1);
          onNavigate?.(nextUrl);
        }
      } catch (err) {
        console.error('Go forward failed:', err);
      }
    }
  };

  const reload = async () => {
    setIsLoading(true);
    if (isTauri() && webviewCreatedRef.current) {
      try {
        await invoke('browser_surface_reload');
        setIsLoading(false);
      } catch (err) {
        console.error('Failed to reload:', err);
        setIsLoading(false);
      }
    } else if (iframeRef.current) {
      iframeRef.current.src = currentUrl;
    }
  };

  const detachWebview = async () => {
    if (isTauri()) {
      try {
        await invoke('detach_browser_to_window', {
          tabId,
          url: currentUrl,
          title: getDomain(currentUrl),
        });
        webviewCreatedRef.current = false;
      } catch (err) {
        console.error('Failed to detach webview:', err);
      }
    }
  };

  const openInSystemBrowser = async () => {
    if (isTauri()) {
      try {
        await invoke('open_in_system_browser', { url: currentUrl });
      } catch (err) {
        console.error('Failed to open in system browser:', err);
        window.open(currentUrl, '_blank');
      }
    } else {
      window.open(currentUrl, '_blank');
    }
  };

  const handleIframeLoad = () => {
    setIsLoading(false);
  };

  const handleIframeError = () => {
    setIsLoading(false);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') {
      navigateTo(inputUrl);
    }
  };

  const handleStarClick = () => {
    setShowBookmarkModal(true);
  };

  const handleBookmarkSaved = () => {
    checkBookmarkStatus(currentUrl);
  };

  const handleBookmarkDeleted = () => {
    setIsBookmarked(false);
    setCurrentBookmark(null);
  };

  const getDomain = (urlString: string) => {
    try {
      return new URL(urlString).hostname;
    } catch {
      return urlString;
    }
  };

  return (
    <div ref={containerRef} className="flex flex-col flex-1 min-h-0 h-full bg-gray-900">
      {/* Navigation Bar - Hidden when rendered separately in chrome area */}
      {!hideNavBar && (
      <div className="flex items-center gap-2 px-3 py-2 bg-gray-800 border-b border-gray-700 z-10">
        <button
          onClick={goBack}
          disabled={!canGoBack}
          className="p-2 rounded-lg hover:bg-gray-700 disabled:opacity-30 disabled:cursor-not-allowed transition-colors"
          title="Go back"
        >
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 19l-7-7 7-7" />
          </svg>
        </button>

        <button
          onClick={goForward}
          disabled={!canGoForward}
          className="p-2 rounded-lg hover:bg-gray-700 disabled:opacity-30 disabled:cursor-not-allowed transition-colors"
          title="Go forward"
        >
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
          </svg>
        </button>

        <button
          onClick={reload}
          className="p-2 rounded-lg hover:bg-gray-700 transition-colors"
          title="Reload"
        >
          <svg className={`w-4 h-4 ${isLoading ? 'animate-spin' : ''}`} fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
          </svg>
        </button>

        {/* URL Bar */}
        <div className="flex-1 flex items-center gap-2 px-3 py-1.5 bg-gray-900 border border-gray-600 rounded-lg">
          {currentUrl.startsWith('https://') ? (
            <svg className="w-4 h-4 text-green-500 flex-shrink-0" fill="currentColor" viewBox="0 0 24 24">
              <path d="M12 1L3 5v6c0 5.55 3.84 10.74 9 12 5.16-1.26 9-6.45 9-12V5l-9-4zm-2 16l-4-4 1.41-1.41L10 14.17l6.59-6.59L18 9l-8 8z" />
            </svg>
          ) : (
            <svg className="w-4 h-4 text-gray-500 flex-shrink-0" fill="currentColor" viewBox="0 0 24 24">
              <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm-1 17.93c-3.95-.49-7-3.85-7-7.93 0-.62.08-1.21.21-1.79L9 15v1c0 1.1.9 2 2 2v1.93zm6.9-2.54c-.26-.81-1-1.39-1.9-1.39h-1v-3c0-.55-.45-1-1-1H8v-2h2c.55 0 1-.45 1-1V7h2c1.1 0 2-.9 2-2v-.41c2.93 1.19 5 4.06 5 7.41 0 2.08-.8 3.97-2.1 5.39z" />
            </svg>
          )}

          <input
            type="text"
            value={inputUrl}
            onChange={(e) => setInputUrl(e.target.value)}
            onKeyDown={handleKeyDown}
            className="flex-1 bg-transparent text-sm text-white outline-none placeholder-gray-500"
            placeholder="Enter URL or search..."
          />

          {isLoading && (
            <div className="w-4 h-4 border-2 border-gray-500 border-t-white rounded-full animate-spin flex-shrink-0" />
          )}

          {/* Domain List Indicator */}
          {isExternalUrl(currentUrl) && (
            <button
              onClick={isInDomainList ? undefined : addToDomainList}
              disabled={domainListLoading || isInDomainList}
              className={`p-1 rounded transition-colors flex-shrink-0 ${
                isInDomainList ? 'text-green-400 cursor-default' : domainListLoading ? 'text-gray-500 cursor-wait' : 'text-gray-500 hover:text-green-400'
              }`}
              title={isInDomainList ? 'Domain in search list' : 'Add domain to search list'}
            >
              {domainListLoading ? (
                <div className="w-4 h-4 border-2 border-gray-500 border-t-green-400 rounded-full animate-spin" />
              ) : (
                <svg className="w-4 h-4" fill={isInDomainList ? 'currentColor' : 'none'} stroke="currentColor" viewBox="0 0 24 24">
                  {isInDomainList ? (
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
                  ) : (
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v3m0 0v3m0-3h3m-3 0H9m12 0a9 9 0 11-18 0 9 9 0 0118 0z" />
                  )}
                </svg>
              )}
            </button>
          )}

          {/* Rate Button */}
          <button
            onClick={() => setShowRatingModal(true)}
            className="p-1 rounded transition-colors flex-shrink-0 text-gray-500 hover:text-purple-400"
            title="Rate this site"
          >
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
            </svg>
          </button>

          {/* Bookmark Star */}
          <button
            onClick={handleStarClick}
            className={`p-1 rounded transition-colors flex-shrink-0 ${isBookmarked ? 'text-yellow-400 hover:text-yellow-300' : 'text-gray-500 hover:text-yellow-400'}`}
            title={isBookmarked ? 'Edit bookmark' : 'Add bookmark'}
          >
            <svg className="w-4 h-4" fill={isBookmarked ? 'currentColor' : 'none'} stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M11.049 2.927c.3-.921 1.603-.921 1.902 0l1.519 4.674a1 1 0 00.95.69h4.915c.969 0 1.371 1.24.588 1.81l-3.976 2.888a1 1 0 00-.363 1.118l1.518 4.674c.3.922-.755 1.688-1.538 1.118l-3.976-2.888a1 1 0 00-1.176 0l-3.976 2.888c-.783.57-1.838-.197-1.538-1.118l1.518-4.674a1 1 0 00-.363-1.118l-3.976-2.888c-.784-.57-.38-1.81.588-1.81h4.914a1 1 0 00.951-.69l1.519-4.674z" />
            </svg>
          </button>
        </div>

        {/* Open in system browser */}
        <button
          onClick={openInSystemBrowser}
          className="p-2 rounded-lg hover:bg-gray-700 transition-colors text-gray-400 hover:text-white"
          title="Open in system browser"
        >
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
          </svg>
        </button>

        {/* Detach button */}
        <button
          onClick={detachWebview}
          className="p-2 rounded-lg hover:bg-gray-700 transition-colors text-gray-400 hover:text-white"
          title="Detach to separate window"
        >
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 7h12m0 0v12m0-12l-12 12" />
          </svg>
        </button>

        {/* Bookmark Modal */}
        <QuickBookmarkModal
          profileId={profileId}
          isOpen={showBookmarkModal}
          onClose={() => setShowBookmarkModal(false)}
          url={currentUrl}
          existingBookmark={currentBookmark}
          onSave={handleBookmarkSaved}
          onDelete={handleBookmarkDeleted}
        />

        {/* Rating Modal */}
        {showRatingModal && (
          <div className="fixed inset-0 z-50 flex items-center justify-center">
            <div className="absolute inset-0 bg-black/60" onClick={() => setShowRatingModal(false)} />
            <div className="relative z-10 bg-gray-800 rounded-xl shadow-2xl border border-gray-700 max-w-md w-full mx-4 max-h-[90vh] overflow-y-auto">
              <div className="p-4 border-b border-gray-700 flex items-center justify-between">
                <h2 className="text-lg font-semibold text-white">Rate Website</h2>
                <button onClick={() => setShowRatingModal(false)} className="p-1 rounded hover:bg-gray-700 transition-colors">
                  <svg className="w-5 h-5 text-gray-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                  </svg>
                </button>
              </div>
              <div className="p-4">
                <div className="mb-4 text-sm text-gray-400">
                  <span className="font-medium text-white">{getDomain(currentUrl)}</span>
                </div>
                <RatingForm domainUrl={currentUrl} onSubmit={() => setShowRatingModal(false)} onCancel={() => setShowRatingModal(false)} />
              </div>
            </div>
          </div>
        )}
      </div>
      )}

      {/* Content Area - This is where the webview will be positioned */}
      <div ref={contentRef} className="flex-1 min-h-0 relative bg-gray-900">
        {/* Loading overlay — iframe (non-Tauri dev) path only. In the Tauri app the
            centered PageLoadSpinner (App.tsx), driven by real WebKit load events,
            owns the loading UI. */}
        {isLoading && !isTauri() && (
          <div className="absolute inset-0 flex items-center justify-center bg-gray-900/80 z-10">
            <div className="text-center">
              <div className="w-12 h-12 border-4 border-gray-600 border-t-[var(--primary-color)] rounded-full animate-spin mb-4 mx-auto" />
              <p className="text-gray-400 text-sm">Loading {getDomain(currentUrl)}...</p>
            </div>
          </div>
        )}

        {/* Error display */}
        {webviewError && (
          <div className="absolute inset-0 flex items-center justify-center bg-gray-900 z-10">
            <div className="text-center max-w-md mx-4">
              <svg className="w-16 h-16 mx-auto mb-4 text-red-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
              </svg>
              <h3 className="text-lg font-semibold text-white mb-2">Failed to load page</h3>
              <p className="text-gray-400 text-sm mb-4">{webviewError}</p>
              <button
                onClick={() => {
                  setWebviewError(null);
                  createOrUpdateWebview(currentUrl);
                }}
                className="px-4 py-2 bg-[var(--primary-color)] text-white rounded-lg hover:opacity-80 transition-opacity"
              >
                Try Again
              </button>
            </div>
          </div>
        )}

        {/* Servo render notice: `.earth` opens in a separate Servo OS window, so
            the embedded surface is hidden and this area would otherwise be blank. */}
        {servoActive && !webviewError && (
          <div className="absolute inset-0 flex items-center justify-center bg-gray-900 z-10">
            <div className="text-center max-w-md mx-4">
              <div className="w-14 h-14 mx-auto mb-4 rounded-full bg-[var(--primary-color)]/20 flex items-center justify-center">
                <svg className="w-7 h-7 text-[var(--primary-color)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3.055 11H5a2 2 0 012 2v1a2 2 0 002 2 2 2 0 012 2v2.945M8 3.935V5.5A2.5 2.5 0 0010.5 8h.5a2 2 0 012 2 2 2 0 104 0 2 2 0 012-2h1.064M15 20.488V18a2 2 0 012-2h3.064M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                </svg>
              </div>
              <h3 className="text-lg font-semibold text-white mb-2">Rendering in Servo</h3>
              <p className="text-gray-400 text-sm">
                This <span className="text-white font-medium">.earth</span> page is rendered by the Servo engine in a separate window.
              </p>
            </div>
          </div>
        )}

        {/* In browser mode (non-Tauri), use iframe */}
        {!isTauri() && isExternalUrl(currentUrl) && (
          <iframe
            ref={iframeRef}
            src={currentUrl}
            className="absolute inset-0 w-full h-full border-0"
            title={`WebView - ${getDomain(currentUrl)}`}
            sandbox="allow-same-origin allow-scripts allow-popups allow-forms allow-downloads"
            onLoad={handleIframeLoad}
            onError={handleIframeError}
          />
        )}

        {/* Placeholder when not external URL */}
        {!isExternalUrl(currentUrl) && (
          <div className="absolute inset-0 flex items-center justify-center">
            <p className="text-gray-400">Enter a URL to browse</p>
          </div>
        )}

        {/* The embedded Tauri webview will be positioned over this area */}
        {/* It's managed by Rust and positioned based on this container's bounds */}
      </div>

      {/* Status Bar */}
      <div className="flex items-center justify-between px-3 py-1 bg-gray-800 border-t border-gray-700 text-xs text-gray-400 z-10">
        <span>{getDomain(currentUrl)}</span>
        <span>{isLoading ? 'Loading...' : 'Ready'}</span>
      </div>
    </div>
  );
}

export default WebView;

// Separate BrowserNavBar component for rendering in chrome area
interface BrowserNavBarProps {
  url: string;
  tabId: number;
  profileId?: number;
  onNavigate?: (newUrl: string) => void;
}

export function BrowserNavBar({
  url,
  tabId,
  profileId = 1,
  onNavigate,
}: BrowserNavBarProps) {
  const [currentUrl, setCurrentUrl] = useState(url);
  const [inputUrl, setInputUrl] = useState(url);
  const [isLoading, setIsLoading] = useState(false);
  const [isBookmarked, setIsBookmarked] = useState(false);
  const [currentBookmark, setCurrentBookmark] = useState<Bookmark | null>(null);
  const [showBookmarkModal, setShowBookmarkModal] = useState(false);
  const [showRatingModal, setShowRatingModal] = useState(false);
  const [isInDomainList, setIsInDomainList] = useState(false);
  const [domainListLoading, setDomainListLoading] = useState(false);

  const isExternalUrl = (urlStr: string) => {
    return urlStr.startsWith('http://') || urlStr.startsWith('https://');
  };

  const getDomain = (urlString: string) => {
    try {
      return new URL(urlString).hostname;
    } catch {
      return urlString;
    }
  };

  // Sync URL when prop changes
  useEffect(() => {
    setCurrentUrl(url);
    setInputUrl(url);
  }, [url]);

  // Re-check bookmark + domain-list status when the URL OR the profile changes.
  // After a restart the profile resolves asynchronously, so keying only on `url`
  // would check before the profile (and its domains) are loaded — leaving the
  // "in list" indicator wrongly un-highlighted.
  useEffect(() => {
    checkBookmarkStatus(url);
    checkDomainListStatus(url);
  }, [url, profileId]);

  const checkBookmarkStatus = async (urlToCheck: string) => {
    try {
      const bookmarkId = await invoke<number | null>('is_url_bookmarked', { profileId, url: urlToCheck });
      if (bookmarkId) {
        const bookmarks = await invoke<Bookmark[]>('get_all_bookmarks', { profileId });
        const bookmark = bookmarks.find(b => b.id === bookmarkId);
        setIsBookmarked(true);
        setCurrentBookmark(bookmark || null);
      } else {
        setIsBookmarked(false);
        setCurrentBookmark(null);
      }
    } catch {
      setIsBookmarked(false);
      setCurrentBookmark(null);
    }
  };

  const checkDomainListStatus = async (urlToCheck: string) => {
    if (!isExternalUrl(urlToCheck)) {
      setIsInDomainList(false);
      return;
    }
    try {
      const norm = (h: string) => h.replace(/^www\./, '');
      const hostname = norm(new URL(urlToCheck).hostname);
      // search_domain_list is the real command; bare `search_domains` is a stub.
      const domains = await invoke<{ id: number; url: string }[]>('search_domain_list', {
        profileId,
        query: hostname,
      });
      const isFound = domains.some(d => {
        try {
          const domainHost = norm(new URL(d.url).hostname);
          return domainHost === hostname || hostname.endsWith('.' + domainHost) || domainHost.endsWith('.' + hostname);
        } catch {
          return d.url.includes(hostname);
        }
      });
      setIsInDomainList(isFound);
    } catch {
      setIsInDomainList(false);
    }
  };

  const addToDomainList = async () => {
    if (!isExternalUrl(currentUrl)) return;
    setDomainListLoading(true);
    try {
      const urlObj = new URL(currentUrl);
      const hostname = urlObj.hostname;
      const baseUrl = `${urlObj.protocol}//${hostname}`;
      // add_domain_entry actually persists (the bare `add_domain` is a no-op stub).
      await invoke('add_domain_entry', {
        url: baseUrl,
        category: 'general',
        trustScore: 0.5,
        profileId,
      });
      setIsInDomainList(true);
    } catch (err) {
      console.error('Failed to add domain:', err);
    } finally {
      setDomainListLoading(false);
    }
  };

  const navigateTo = async (targetUrl: string) => {
    let fullUrl = targetUrl;
    if (!targetUrl.startsWith('http://') && !targetUrl.startsWith('https://') && !targetUrl.startsWith('earth://')) {
      if (targetUrl.includes('.') && !targetUrl.includes(' ')) {
        fullUrl = `https://${targetUrl}`;
      } else {
        fullUrl = `https://duckduckgo.com/?q=${encodeURIComponent(targetUrl)}`;
      }
    }
    setCurrentUrl(fullUrl);
    setInputUrl(fullUrl);
    onNavigate?.(fullUrl);

    if (isExternalUrl(fullUrl) && isTauri()) {
      setIsLoading(true);
      try {
        // Single router front door. No bounds here: the nav bar doesn't own
        // layout, so this navigates the existing webview in place.
        await invoke('navigate', { tabId, url: fullUrl });
        setIsLoading(false);
      } catch (err) {
        console.error('Navigation failed:', err);
        setIsLoading(false);
      }
    }
  };

  const goBack = async () => {
    if (isTauri()) {
      try {
        await invoke('browser_surface_back');
      } catch (err) {
        console.error('Go back failed:', err);
      }
    }
  };

  const goForward = async () => {
    if (isTauri()) {
      try {
        await invoke('browser_surface_forward');
      } catch (err) {
        console.error('Go forward failed:', err);
      }
    }
  };

  const reload = async () => {
    setIsLoading(true);
    if (isTauri()) {
      try {
        await invoke('browser_surface_reload');
        setIsLoading(false);
      } catch (err) {
        console.error('Failed to reload:', err);
        setIsLoading(false);
      }
    }
  };

  const openInSystemBrowser = async () => {
    if (isTauri()) {
      try {
        await invoke('open_in_system_browser', { url: currentUrl });
      } catch (err) {
        console.error('Failed to open in system browser:', err);
        window.open(currentUrl, '_blank');
      }
    } else {
      window.open(currentUrl, '_blank');
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') {
      navigateTo(inputUrl);
    }
  };

  return (
    <div className="flex items-center gap-2 px-3 py-2 bg-gray-800 border-b border-gray-700">
      <button
        onClick={goBack}
        className="p-2 rounded-lg hover:bg-gray-700 transition-colors"
        title="Go back"
      >
        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 19l-7-7 7-7" />
        </svg>
      </button>

      <button
        onClick={goForward}
        className="p-2 rounded-lg hover:bg-gray-700 transition-colors"
        title="Go forward"
      >
        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
        </svg>
      </button>

      <button
        onClick={reload}
        className="p-2 rounded-lg hover:bg-gray-700 transition-colors"
        title="Reload"
      >
        <svg className={`w-4 h-4 ${isLoading ? 'animate-spin' : ''}`} fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
        </svg>
      </button>

      {/* URL Bar */}
      <div className="flex-1 flex items-center gap-2 px-3 py-1.5 bg-gray-900 border border-gray-600 rounded-lg">
        {currentUrl.startsWith('https://') ? (
          <svg className="w-4 h-4 text-green-500 flex-shrink-0" fill="currentColor" viewBox="0 0 24 24">
            <path d="M12 1L3 5v6c0 5.55 3.84 10.74 9 12 5.16-1.26 9-6.45 9-12V5l-9-4zm-2 16l-4-4 1.41-1.41L10 14.17l6.59-6.59L18 9l-8 8z" />
          </svg>
        ) : (
          <svg className="w-4 h-4 text-gray-500 flex-shrink-0" fill="currentColor" viewBox="0 0 24 24">
            <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm-1 17.93c-3.95-.49-7-3.85-7-7.93 0-.62.08-1.21.21-1.79L9 15v1c0 1.1.9 2 2 2v1.93zm6.9-2.54c-.26-.81-1-1.39-1.9-1.39h-1v-3c0-.55-.45-1-1-1H8v-2h2c.55 0 1-.45 1-1V7h2c1.1 0 2-.9 2-2v-.41c2.93 1.19 5 4.06 5 7.41 0 2.08-.8 3.97-2.1 5.39z" />
          </svg>
        )}

        <input
          type="text"
          value={inputUrl}
          onChange={(e) => setInputUrl(e.target.value)}
          onKeyDown={handleKeyDown}
          className="flex-1 bg-transparent text-sm text-white outline-none placeholder-gray-500"
          placeholder="Enter URL or search..."
        />

        {isLoading && (
          <div className="w-4 h-4 border-2 border-gray-500 border-t-white rounded-full animate-spin flex-shrink-0" />
        )}

        {/* Domain List Indicator */}
        {isExternalUrl(currentUrl) && (
          <button
            onClick={isInDomainList ? undefined : addToDomainList}
            disabled={domainListLoading || isInDomainList}
            className={`p-1 rounded transition-colors flex-shrink-0 ${
              isInDomainList ? 'text-green-400 cursor-default' : domainListLoading ? 'text-gray-500 cursor-wait' : 'text-gray-500 hover:text-green-400'
            }`}
            title={isInDomainList ? 'Domain in search list' : 'Add domain to search list'}
          >
            {domainListLoading ? (
              <div className="w-4 h-4 border-2 border-gray-500 border-t-green-400 rounded-full animate-spin" />
            ) : (
              <svg className="w-4 h-4" fill={isInDomainList ? 'currentColor' : 'none'} stroke="currentColor" viewBox="0 0 24 24">
                {isInDomainList ? (
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
                ) : (
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v3m0 0v3m0-3h3m-3 0H9m12 0a9 9 0 11-18 0 9 9 0 0118 0z" />
                )}
              </svg>
            )}
          </button>
        )}

        {/* NoScript: per-origin script trust (Trusted / Temp / Blocked). */}
        {isExternalUrl(currentUrl) && <NoscriptShield currentUrl={currentUrl} />}

        {/* Rate Button */}
        <button
          onClick={() => setShowRatingModal(true)}
          className="p-1 rounded transition-colors flex-shrink-0 text-gray-500 hover:text-purple-400"
          title="Rate this site"
        >
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
          </svg>
        </button>

        {/* Bookmark Star */}
        <button
          onClick={() => setShowBookmarkModal(true)}
          className={`p-1 rounded transition-colors flex-shrink-0 ${isBookmarked ? 'text-yellow-400 hover:text-yellow-300' : 'text-gray-500 hover:text-yellow-400'}`}
          title={isBookmarked ? 'Edit bookmark' : 'Add bookmark'}
        >
          <svg className="w-4 h-4" fill={isBookmarked ? 'currentColor' : 'none'} stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M11.049 2.927c.3-.921 1.603-.921 1.902 0l1.519 4.674a1 1 0 00.95.69h4.915c.969 0 1.371 1.24.588 1.81l-3.976 2.888a1 1 0 00-.363 1.118l1.518 4.674c.3.922-.755 1.688-1.538 1.118l-3.976-2.888a1 1 0 00-1.176 0l-3.976 2.888c-.783.57-1.838-.197-1.538-1.118l1.518-4.674a1 1 0 00-.363-1.118l-3.976-2.888c-.784-.57-.38-1.81.588-1.81h4.914a1 1 0 00.951-.69l1.519-4.674z" />
          </svg>
        </button>
      </div>

      {/* Open in system browser */}
      <button
        onClick={openInSystemBrowser}
        className="p-2 rounded-lg hover:bg-gray-700 transition-colors text-gray-400 hover:text-white"
        title="Open in system browser"
      >
        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
        </svg>
      </button>

      {/* Bookmark Modal */}
      <QuickBookmarkModal
        profileId={profileId}
        isOpen={showBookmarkModal}
        onClose={() => setShowBookmarkModal(false)}
        url={currentUrl}
        existingBookmark={currentBookmark}
        onSave={() => checkBookmarkStatus(currentUrl)}
        onDelete={() => { setIsBookmarked(false); setCurrentBookmark(null); }}
      />

      {/* Rating Modal */}
      {showRatingModal && (
        <div className="fixed inset-0 z-50 flex items-center justify-center">
          <div className="absolute inset-0 bg-black/60" onClick={() => setShowRatingModal(false)} />
          <div className="relative z-10 bg-gray-800 rounded-xl shadow-2xl border border-gray-700 max-w-md w-full mx-4 max-h-[90vh] overflow-y-auto">
            <div className="p-4 border-b border-gray-700 flex items-center justify-between">
              <h2 className="text-lg font-semibold text-white">Rate Website</h2>
              <button onClick={() => setShowRatingModal(false)} className="p-1 rounded hover:bg-gray-700 transition-colors">
                <svg className="w-5 h-5 text-gray-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>
            <div className="p-4">
              <div className="mb-4 text-sm text-gray-400">
                <span className="font-medium text-white">{getDomain(currentUrl)}</span>
              </div>
              <RatingForm domainUrl={currentUrl} onSubmit={() => setShowRatingModal(false)} onCancel={() => setShowRatingModal(false)} />
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
