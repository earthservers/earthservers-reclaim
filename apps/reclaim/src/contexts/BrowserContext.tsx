// BrowserContext - Manages browser navigation state
// Uses single-webview pattern: navigates the main window to external URLs

import React, { createContext, useContext, useState, useCallback, useEffect, useRef } from 'react';
import { invoke } from '../lib/tauri';

// Tab state stored in React (UI state)
export interface TabState {
  id: number;
  title: string;
  url: string;
  favicon?: string;
  isActive: boolean;
  isPinned: boolean;
  // Navigation history for this tab
  historyStack: string[];
  currentHistoryIndex: number;
  // Scroll position (saved when switching away)
  scrollX: number;
  scrollY: number;
  // Loading state
  isLoading: boolean;
}

interface BrowserContextValue {
  // Current browsing mode
  isExternalBrowsing: boolean;
  currentExternalUrl: string | null;

  // Tab management
  tabs: TabState[];
  activeTabId: number | null;

  // Actions
  navigateToExternal: (url: string) => Promise<void>;
  navigateToApp: () => Promise<void>;

  // Tab actions
  createTab: (url: string, title?: string) => Promise<TabState>;
  closeTab: (tabId: number) => Promise<void>;
  switchTab: (tabId: number) => Promise<void>;
  updateTabUrl: (tabId: number, url: string, title?: string) => void;

  // Navigation for current tab
  goBack: () => void;
  goForward: () => void;
  canGoBack: boolean;
  canGoForward: boolean;
}

const BrowserContext = createContext<BrowserContextValue | null>(null);

export function BrowserProvider({
  children,
  profileId = 1
}: {
  children: React.ReactNode;
  profileId?: number;
}) {
  const [isExternalBrowsing, setIsExternalBrowsing] = useState(false);
  const [currentExternalUrl, setCurrentExternalUrl] = useState<string | null>(null);
  const [tabs, setTabs] = useState<TabState[]>([]);
  const [activeTabId, setActiveTabId] = useState<number | null>(null);
  const nextTabIdRef = useRef(1);

  // Get active tab
  const activeTab = tabs.find(t => t.id === activeTabId);

  // Load tabs from backend on mount
  useEffect(() => {
    loadTabs();
  }, [profileId]);

  const loadTabs = async () => {
    try {
      const backendTabs = await invoke<Array<{
        id: number;
        profile_id: number;
        url: string;
        title: string;
        favicon: string | null;
        is_active: boolean;
        is_pinned: boolean;
        position: number;
      }>>('get_all_tabs', { profileId });

      const loadedTabs: TabState[] = backendTabs.map(t => ({
        id: t.id,
        title: t.title,
        url: t.url,
        favicon: t.favicon || undefined,
        isActive: t.is_active,
        isPinned: t.is_pinned,
        historyStack: [t.url],
        currentHistoryIndex: 0,
        scrollX: 0,
        scrollY: 0,
        isLoading: false,
      }));

      setTabs(loadedTabs);

      const activeTab = loadedTabs.find(t => t.isActive);
      if (activeTab) {
        setActiveTabId(activeTab.id);
        // Check if active tab is external
        if (activeTab.url.startsWith('http://') || activeTab.url.startsWith('https://')) {
          setIsExternalBrowsing(true);
          setCurrentExternalUrl(activeTab.url);
        }
      }

      // Update next ID
      const maxId = Math.max(...loadedTabs.map(t => t.id), 0);
      nextTabIdRef.current = maxId + 1;
    } catch (err) {
      console.error('Failed to load tabs:', err);
      // Create default tab
      createTab('earth://search', 'Search');
    }
  };

  // Navigate to external URL (navigates main webview)
  const navigateToExternal = useCallback(async (url: string) => {
    setIsExternalBrowsing(true);
    setCurrentExternalUrl(url);

    // Update active tab's URL
    if (activeTabId) {
      setTabs(prev => prev.map(tab => {
        if (tab.id === activeTabId) {
          const newHistory = tab.historyStack.slice(0, tab.currentHistoryIndex + 1);
          newHistory.push(url);
          return {
            ...tab,
            url,
            historyStack: newHistory,
            currentHistoryIndex: newHistory.length - 1,
          };
        }
        return tab;
      }));

      // Update in backend
      try {
        await invoke('update_tab', {
          tabId: activeTabId,
          url,
          title: new URL(url).hostname,
        });
      } catch (err) {
        console.error('Failed to update tab:', err);
      }
    }

    // Navigate the main webview to external URL
    try {
      await invoke('navigate_main_window', { url });
    } catch (err) {
      console.error('Failed to navigate main window:', err);
    }
  }, [activeTabId]);

  // Navigate back to app
  const navigateToApp = useCallback(async () => {
    setIsExternalBrowsing(false);
    setCurrentExternalUrl(null);

    // Navigate main window back to app
    try {
      await invoke('navigate_main_window', { url: 'tauri://localhost' });
    } catch (err) {
      console.error('Failed to navigate back to app:', err);
      // Fallback: reload the window
      window.location.reload();
    }
  }, []);

  // Create a new tab
  const createTab = useCallback(async (url: string, title?: string): Promise<TabState> => {
    const isExternal = url.startsWith('http://') || url.startsWith('https://');
    const tabTitle = title || (isExternal ? new URL(url).hostname : 'New Tab');

    // Create in backend first
    let backendTab;
    try {
      backendTab = await invoke<{ id: number }>('create_tab', {
        profileId,
        url,
        title: tabTitle,
      });
    } catch (err) {
      console.error('Failed to create tab in backend:', err);
      // Use local ID
      backendTab = { id: nextTabIdRef.current++ };
    }

    const newTab: TabState = {
      id: backendTab.id,
      title: tabTitle,
      url,
      isActive: true,
      isPinned: false,
      historyStack: [url],
      currentHistoryIndex: 0,
      scrollX: 0,
      scrollY: 0,
      isLoading: isExternal,
    };

    // Deactivate other tabs, add new tab
    setTabs(prev => [
      ...prev.map(t => ({ ...t, isActive: false })),
      newTab,
    ]);
    setActiveTabId(newTab.id);

    // Set active in backend
    try {
      await invoke('set_active_tab', { tabId: newTab.id });
    } catch (err) {
      console.error('Failed to set active tab:', err);
    }

    // If external URL, navigate
    if (isExternal) {
      await navigateToExternal(url);
    } else {
      setIsExternalBrowsing(false);
      setCurrentExternalUrl(null);
    }

    return newTab;
  }, [profileId, navigateToExternal]);

  // Close a tab
  const closeTab = useCallback(async (tabId: number) => {
    const tabIndex = tabs.findIndex(t => t.id === tabId);
    if (tabIndex === -1) return;

    const closingActiveTab = tabId === activeTabId;

    // Remove from state
    setTabs(prev => prev.filter(t => t.id !== tabId));

    // Close in backend
    try {
      await invoke('close_tab', { tabId });
    } catch (err) {
      console.error('Failed to close tab in backend:', err);
    }

    // If closing active tab, switch to adjacent
    if (closingActiveTab) {
      const remainingTabs = tabs.filter(t => t.id !== tabId);
      const newActiveTab = remainingTabs[Math.min(tabIndex, remainingTabs.length - 1)];

      if (newActiveTab) {
        await switchTab(newActiveTab.id);
      } else {
        // No tabs left, create new one
        await createTab('earth://search', 'Search');
      }
    }
  }, [tabs, activeTabId, createTab]);

  // Switch to a different tab
  const switchTab = useCallback(async (tabId: number) => {
    const targetTab = tabs.find(t => t.id === tabId);
    if (!targetTab) return;

    // Update active states
    setTabs(prev => prev.map(t => ({
      ...t,
      isActive: t.id === tabId,
    })));
    setActiveTabId(tabId);

    // Set active in backend
    try {
      await invoke('set_active_tab', { tabId });
    } catch (err) {
      console.error('Failed to set active tab:', err);
    }

    // Handle navigation based on URL type
    const isExternal = targetTab.url.startsWith('http://') || targetTab.url.startsWith('https://');

    if (isExternal) {
      await navigateToExternal(targetTab.url);
    } else {
      // Internal URL - make sure we're showing the app
      if (isExternalBrowsing) {
        await navigateToApp();
      }
      setIsExternalBrowsing(false);
      setCurrentExternalUrl(null);
    }
  }, [tabs, isExternalBrowsing, navigateToExternal, navigateToApp]);

  // Update tab URL
  const updateTabUrl = useCallback((tabId: number, url: string, title?: string) => {
    setTabs(prev => prev.map(tab => {
      if (tab.id === tabId) {
        return {
          ...tab,
          url,
          title: title || tab.title,
        };
      }
      return tab;
    }));

    // Update in backend
    invoke('update_tab', { tabId, url, title: title || undefined }).catch(console.error);
  }, []);

  // Go back in history
  const goBack = useCallback(() => {
    if (!activeTab || activeTab.currentHistoryIndex <= 0) return;

    const newIndex = activeTab.currentHistoryIndex - 1;
    const newUrl = activeTab.historyStack[newIndex];

    setTabs(prev => prev.map(tab => {
      if (tab.id === activeTabId) {
        return {
          ...tab,
          url: newUrl,
          currentHistoryIndex: newIndex,
        };
      }
      return tab;
    }));

    const isExternal = newUrl.startsWith('http://') || newUrl.startsWith('https://');
    if (isExternal) {
      navigateToExternal(newUrl);
    } else {
      navigateToApp();
    }
  }, [activeTab, activeTabId, navigateToExternal, navigateToApp]);

  // Go forward in history
  const goForward = useCallback(() => {
    if (!activeTab || activeTab.currentHistoryIndex >= activeTab.historyStack.length - 1) return;

    const newIndex = activeTab.currentHistoryIndex + 1;
    const newUrl = activeTab.historyStack[newIndex];

    setTabs(prev => prev.map(tab => {
      if (tab.id === activeTabId) {
        return {
          ...tab,
          url: newUrl,
          currentHistoryIndex: newIndex,
        };
      }
      return tab;
    }));

    const isExternal = newUrl.startsWith('http://') || newUrl.startsWith('https://');
    if (isExternal) {
      navigateToExternal(newUrl);
    } else {
      navigateToApp();
    }
  }, [activeTab, activeTabId, navigateToExternal, navigateToApp]);

  const canGoBack = activeTab ? activeTab.currentHistoryIndex > 0 : false;
  const canGoForward = activeTab ? activeTab.currentHistoryIndex < activeTab.historyStack.length - 1 : false;

  return (
    <BrowserContext.Provider value={{
      isExternalBrowsing,
      currentExternalUrl,
      tabs,
      activeTabId,
      navigateToExternal,
      navigateToApp,
      createTab,
      closeTab,
      switchTab,
      updateTabUrl,
      goBack,
      goForward,
      canGoBack,
      canGoForward,
    }}>
      {children}
    </BrowserContext.Provider>
  );
}

export function useBrowser() {
  const context = useContext(BrowserContext);
  if (!context) {
    throw new Error('useBrowser must be used within a BrowserProvider');
  }
  return context;
}
