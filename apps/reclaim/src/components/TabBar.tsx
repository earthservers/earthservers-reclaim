import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke, closeWindow, isTauri } from '../lib/tauri';

// Threshold for detecting drag-out (pixels from tab bar edge)
const DRAG_OUT_THRESHOLD = 50;

// Native button component using pointerdown (more reliable in WebKitGTK)
function NativeButton({
  onClick,
  className,
  title,
  children,
  style,
}: {
  onClick: () => void;
  className?: string;
  title?: string;
  children: React.ReactNode;
  style?: React.CSSProperties;
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
      console.log('NativeButton pointerdown:', title);
      requestAnimationFrame(() => onClick());
    };

    const handleTouchStart = (e: TouchEvent) => {
      e.preventDefault();
      e.stopPropagation();
      console.log('NativeButton touchstart:', title);
      requestAnimationFrame(() => onClick());
    };

    button.addEventListener('pointerdown', handlePointerDown, { capture: true });
    button.addEventListener('touchstart', handleTouchStart, { capture: true, passive: false });

    return () => {
      button.removeEventListener('pointerdown', handlePointerDown, { capture: true });
      button.removeEventListener('touchstart', handleTouchStart, { capture: true });
    };
  }, [onClick, title]);

  return (
    <button
      ref={buttonRef}
      type="button"
      className={className}
      title={title}
      style={{
        ...style,
        pointerEvents: 'auto',
        position: 'relative',
        zIndex: 50,
        touchAction: 'manipulation',
      }}
    >
      {children}
    </button>
  );
}

// Native clickable div using pointerdown (for tabs)
function NativeClickableDiv({
  onClick,
  onContextMenu,
  onDragStart,
  className,
  title,
  children,
  isDragging,
  tabId,
  registerRef,
}: {
  onClick: () => void;
  onContextMenu?: (e: React.MouseEvent) => void;
  onDragStart?: (e: React.MouseEvent) => void;
  className?: string;
  title?: string;
  children: React.ReactNode;
  isDragging?: boolean;
  tabId?: number;
  registerRef?: (id: number, el: HTMLDivElement | null) => void;
}) {
  const divRef = useRef<HTMLDivElement>(null);
  const isDraggingRef = useRef(false);
  const startPosRef = useRef({ x: 0, y: 0 });

  // Register the ref when element mounts
  useEffect(() => {
    if (tabId !== undefined && registerRef && divRef.current) {
      registerRef(tabId, divRef.current);
      return () => registerRef(tabId, null);
    }
  }, [tabId, registerRef]);

  useEffect(() => {
    const div = divRef.current;
    if (!div) return;

    // Use pointerdown for WebKitGTK compatibility
    const handlePointerDown = (e: PointerEvent) => {
      // Check if the click was on a button or inside a button (close button)
      const target = e.target as HTMLElement;
      if (target.closest('button')) {
        // Let the button handle its own click - don't interfere
        return;
      }

      e.preventDefault();
      e.stopPropagation();

      // Record start position for drag detection
      startPosRef.current = { x: e.clientX, y: e.clientY };
      isDraggingRef.current = false;

      // If drag support is enabled, wait briefly to detect drag vs click
      if (onDragStart) {
        const handlePointerMove = (moveEvent: PointerEvent) => {
          const dx = Math.abs(moveEvent.clientX - startPosRef.current.x);
          const dy = Math.abs(moveEvent.clientY - startPosRef.current.y);

          // If moved more than 5px, it's a drag
          if (dx > 5 || dy > 5) {
            isDraggingRef.current = true;
            // Trigger drag start with a synthetic React event
            onDragStart({ clientX: startPosRef.current.x, clientY: startPosRef.current.y } as React.MouseEvent);
            window.removeEventListener('pointermove', handlePointerMove);
            window.removeEventListener('pointerup', handlePointerUp);
          }
        };

        const handlePointerUp = () => {
          window.removeEventListener('pointermove', handlePointerMove);
          window.removeEventListener('pointerup', handlePointerUp);

          // If not dragging, it's a click
          if (!isDraggingRef.current) {
            requestAnimationFrame(() => onClick());
          }
        };

        window.addEventListener('pointermove', handlePointerMove);
        window.addEventListener('pointerup', handlePointerUp);
      } else {
        // No drag support, just click
        requestAnimationFrame(() => onClick());
      }
    };

    const handleTouchStart = (e: TouchEvent) => {
      // Check if the touch was on a button
      const target = e.target as HTMLElement;
      if (target.closest('button')) {
        return;
      }

      e.preventDefault();
      e.stopPropagation();
      requestAnimationFrame(() => onClick());
    };

    div.addEventListener('pointerdown', handlePointerDown, { capture: false });
    div.addEventListener('touchstart', handleTouchStart, { capture: false, passive: false });

    return () => {
      div.removeEventListener('pointerdown', handlePointerDown, { capture: false });
      div.removeEventListener('touchstart', handleTouchStart, { capture: false });
    };
  }, [onClick, onDragStart, title]);

  return (
    <div
      ref={divRef}
      className={className}
      title={title}
      onContextMenu={onContextMenu}
      style={{
        pointerEvents: 'auto',
        position: 'relative',
        zIndex: 10,
        touchAction: 'manipulation',
        cursor: isDragging ? 'grabbing' : 'pointer',
        opacity: isDragging ? 0.6 : 1,
      }}
    >
      {children}
    </div>
  );
}

// Types
export interface Tab {
  id: number;
  profile_id: number;
  title: string | null;
  url: string;
  favicon: string | null;
  position: number;
  is_pinned: boolean;
  is_active: boolean;
  scroll_position: number;
  created_at: string;
  last_accessed: string;
}

// Tab behavior modes for link navigation
export type TabBehavior = 'new-tab' | 'overwrite-search' | 'all-new-tabs';

export const TAB_BEHAVIOR_OPTIONS: { value: TabBehavior; label: string; color: string; description: string }[] = [
  { value: 'new-tab', label: 'New Tab', color: '#EAB308', description: 'Open links in new tabs (default navigation)' },
  { value: 'overwrite-search', label: 'Overwrite', color: '#EF4444', description: 'Overwrite Search tab with new content' },
  { value: 'all-new-tabs', label: 'All New', color: '#22C55E', description: 'Open all links in new tabs' },
];

interface TabBarProps {
  profileId: number;
  onTabChange?: (tab: Tab) => void;
  refreshTrigger?: number;
}

export function TabBar({ profileId, onTabChange, refreshTrigger }: TabBarProps) {
  const [tabs, setTabs] = useState<Tab[]>([]);
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number; tabId: number } | null>(null);

  // Drag state - for both reordering and drag-out
  const [draggingTab, setDraggingTab] = useState<{ tabId: number; startX: number; startY: number } | null>(null);
  const [isDragOutMode, setIsDragOutMode] = useState(false);
  const [dropTargetIndex, setDropTargetIndex] = useState<number | null>(null); // For reordering visual feedback
  const [currentDragX, setCurrentDragX] = useState(0);
  const tabBarRef = useRef<HTMLDivElement>(null);
  const tabElementsRef = useRef<Map<number, HTMLElement>>(new Map());

  // Track if we've notified parent of initial active tab
  const hasNotifiedInitialTab = useRef(false);

  // Load tabs
  const loadTabs = useCallback(async () => {
    try {
      const loadedTabs = await invoke<Tab[]>('get_all_tabs', { profileId: profileId });
      setTabs(loadedTabs);

      // On initial load, notify parent of the active tab
      if (!hasNotifiedInitialTab.current && loadedTabs.length > 0) {
        const activeTab = loadedTabs.find(t => t.is_active);
        if (activeTab && onTabChange) {
          onTabChange(activeTab);
        }
        hasNotifiedInitialTab.current = true;
      }
    } catch (err) {
      console.error('Failed to load tabs:', err);
    }
  }, [profileId, onTabChange]);

  // The profile loads async, so the first loadTabs can run with the default
  // profileId before the real one resolves. Reset the notify-once guard whenever
  // the profile changes so the parent gets the CORRECT profile's active tab —
  // otherwise the restored tab is highlighted but the app shows the search page.
  useEffect(() => {
    hasNotifiedInitialTab.current = false;
  }, [profileId]);

  useEffect(() => {
    loadTabs();
  }, [loadTabs, refreshTrigger]);

  // Create new tab - defaults to EarthSearch home
  const createTab = async (url: string = 'earth://search', title: string = 'Search') => {
    try {
      const newTab = await invoke<Tab>('create_tab', {
        profileId: profileId,
        url,
        title,
      });
      await invoke('set_active_tab', { tabId: newTab.id });
      loadTabs();
      if (onTabChange) onTabChange(newTab);
    } catch (err) {
      console.error('Failed to create tab:', err);
    }
  };

  // Close tab
  const closeTab = async (tabId: number, e?: React.MouseEvent) => {
    if (e) {
      e.stopPropagation();
    }
    try {
      // If this is the last tab, close the window (in Tauri) or don't close (in browser)
      if (tabs.length === 1) {
        if (isTauri()) {
          closeWindow();
        }
        // In browser dev mode, don't close the last tab
        return;
      }

      const tab = tabs.find(t => t.id === tabId);
      await invoke('close_tab', { tabId: tabId });

      // If closing active tab, activate another
      if (tab?.is_active && tabs.length > 1) {
        const remaining = tabs.filter(t => t.id !== tabId);
        const nextTab = remaining[Math.min(tab.position, remaining.length - 1)];
        if (nextTab) {
          await invoke('set_active_tab', { tabId: nextTab.id });
          if (onTabChange) onTabChange(nextTab);
        }
      }

      loadTabs();
    } catch (err) {
      console.error('Failed to close tab:', err);
    }
  };

  // Switch to tab - just switches, doesn't create new tabs
  const switchTab = async (tabId: number) => {
    try {
      const tab = await invoke<Tab>('set_active_tab', { tabId: tabId });
      loadTabs();
      if (onTabChange && tab) onTabChange(tab);
      // Note: We don't call onUrlNavigate here - that's for creating new tabs
      // The parent component will handle display based on activeTab state
    } catch (err) {
      console.error('Failed to switch tab:', err);
    }
  };

  // Pin/unpin tab
  const togglePin = async (tabId: number) => {
    const tab = tabs.find(t => t.id === tabId);
    if (!tab) return;
    try {
      await invoke('pin_tab', { tabId: tabId, pinned: !tab.is_pinned });
      loadTabs();
    } catch (err) {
      console.error('Failed to toggle pin:', err);
    }
  };

  // Duplicate tab
  const duplicateTab = async (tabId: number) => {
    try {
      await invoke('duplicate_tab', { tabId: tabId });
      loadTabs();
    } catch (err) {
      console.error('Failed to duplicate tab:', err);
    }
  };

  // Close tabs to right
  const closeTabsToRight = async (tabId: number) => {
    try {
      await invoke('close_tabs_to_right', { tabId: tabId });
      loadTabs();
    } catch (err) {
      console.error('Failed to close tabs to right:', err);
    }
  };

  // Context menu handler
  const handleContextMenu = (e: React.MouseEvent, tabId: number) => {
    e.preventDefault();
    setContextMenu({ x: e.clientX, y: e.clientY, tabId });
  };

  // Close context menu on outside click
  useEffect(() => {
    const handleClick = () => {
      setContextMenu(null);
    };
    window.addEventListener('click', handleClick);
    return () => window.removeEventListener('click', handleClick);
  }, []);

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Ctrl+T - New tab
      if (e.ctrlKey && e.key === 't') {
        e.preventDefault();
        createTab();
      }
      // Ctrl+W - Close current tab
      if (e.ctrlKey && e.key === 'w') {
        e.preventDefault();
        const activeTab = tabs.find(t => t.is_active);
        if (activeTab && tabs.length > 1) {
          closeTab(activeTab.id);
        }
      }
      // Ctrl+1-9 - Switch to tab by index
      if (e.ctrlKey && e.key >= '1' && e.key <= '9') {
        e.preventDefault();
        const index = parseInt(e.key) - 1;
        if (index < tabs.length) {
          switchTab(tabs[index].id);
        }
      }
      // Ctrl+Tab - Next tab
      if (e.ctrlKey && e.key === 'Tab' && !e.shiftKey) {
        e.preventDefault();
        const activeIndex = tabs.findIndex(t => t.is_active);
        const nextIndex = (activeIndex + 1) % tabs.length;
        switchTab(tabs[nextIndex].id);
      }
      // Ctrl+Shift+Tab - Previous tab
      if (e.ctrlKey && e.shiftKey && e.key === 'Tab') {
        e.preventDefault();
        const activeIndex = tabs.findIndex(t => t.is_active);
        const prevIndex = (activeIndex - 1 + tabs.length) % tabs.length;
        switchTab(tabs[prevIndex].id);
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [tabs]);

  // Handle drag-out to new window
  const handleTabDragStart = (tabId: number, e: React.MouseEvent) => {
    // Only allow drag-out if there's more than one tab and we're in Tauri
    if (tabs.length <= 1 || !isTauri()) return;

    setDraggingTab({ tabId, startX: e.clientX, startY: e.clientY });
  };

  const handleTabDragMove = useCallback((e: MouseEvent) => {
    if (!draggingTab || !tabBarRef.current) return;

    const tabBarRect = tabBarRef.current.getBoundingClientRect();
    setCurrentDragX(e.clientX);

    // Check if cursor has moved outside the tab bar area (for drag-out)
    const isOutsideY = e.clientY > tabBarRect.bottom + DRAG_OUT_THRESHOLD ||
                       e.clientY < tabBarRect.top - DRAG_OUT_THRESHOLD;
    const isOutsideX = e.clientX < tabBarRect.left - DRAG_OUT_THRESHOLD ||
                       e.clientX > tabBarRect.right + DRAG_OUT_THRESHOLD;

    if (isOutsideY || isOutsideX) {
      setIsDragOutMode(true);
      setDropTargetIndex(null);
    } else {
      setIsDragOutMode(false);

      // Calculate drop target for reordering (only for regular tabs, not pinned)
      const regularTabsOnly = tabs.filter(t => !t.is_pinned);
      let newDropIndex: number | null = null;

      // Find which tab we're hovering over
      for (let i = 0; i < regularTabsOnly.length; i++) {
        const tab = regularTabsOnly[i];
        const tabElement = tabElementsRef.current.get(tab.id);
        if (tabElement) {
          const rect = tabElement.getBoundingClientRect();
          const midpoint = rect.left + rect.width / 2;

          if (e.clientX < midpoint) {
            newDropIndex = i;
            break;
          } else if (i === regularTabsOnly.length - 1) {
            // After the last tab
            newDropIndex = i + 1;
          }
        }
      }

      // Don't show drop indicator at the dragged tab's current position
      const draggedTabIndex = regularTabsOnly.findIndex(t => t.id === draggingTab.tabId);
      if (newDropIndex === draggedTabIndex || newDropIndex === draggedTabIndex + 1) {
        newDropIndex = null;
      }

      setDropTargetIndex(newDropIndex);
    }
  }, [draggingTab, tabs]);

  const handleTabDragEnd = useCallback(async (e: MouseEvent) => {
    if (!draggingTab) return;

    if (isDragOutMode && isTauri()) {
      // Drag-out: Create new window
      const tab = tabs.find(t => t.id === draggingTab.tabId);
      if (tab && tabs.length > 1) {
        try {
          // Create a new window at the drop position
          await invoke('create_detached_window', {
            tabId: tab.id,
            url: 'index.html',
            title: tab.title || 'Reclaim',
            x: Math.round(e.screenX - 100),
            y: Math.round(e.screenY - 50),
          });

          // Close the tab in the current window
          await invoke('close_tab', { tabId: tab.id });

          // Switch to next available tab
          const remaining = tabs.filter(t => t.id !== tab.id);
          if (remaining.length > 0) {
            const nextTab = remaining[Math.min(tab.position, remaining.length - 1)];
            await invoke('set_active_tab', { tabId: nextTab.id });
            if (onTabChange) onTabChange(nextTab);
          }

          loadTabs();
        } catch (err) {
          console.error('Failed to detach tab:', err);
        }
      }
    } else if (dropTargetIndex !== null) {
      // Reorder tabs
      const regularTabsOnly = tabs.filter(t => !t.is_pinned);
      const draggedTabIndex = regularTabsOnly.findIndex(t => t.id === draggingTab.tabId);

      if (draggedTabIndex !== -1 && dropTargetIndex !== draggedTabIndex && dropTargetIndex !== draggedTabIndex + 1) {
        // Calculate new order
        const newOrder = [...regularTabsOnly];
        const [draggedTab] = newOrder.splice(draggedTabIndex, 1);

        // Adjust target index after removal
        let insertAt = dropTargetIndex;
        if (dropTargetIndex > draggedTabIndex) {
          insertAt--;
        }

        newOrder.splice(insertAt, 0, draggedTab);

        // Get the tab IDs in new order (including pinned tabs first)
        const pinnedTabIds = tabs.filter(t => t.is_pinned).map(t => t.id);
        const reorderedIds = [...pinnedTabIds, ...newOrder.map(t => t.id)];

        try {
          await invoke('reorder_tabs', { tabIds: reorderedIds });
          loadTabs();
        } catch (err) {
          console.error('Failed to reorder tabs:', err);
        }
      }
    }

    setDraggingTab(null);
    setIsDragOutMode(false);
    setDropTargetIndex(null);
  }, [draggingTab, isDragOutMode, dropTargetIndex, tabs, onTabChange, loadTabs]);

  // Global pointer event listeners for drag - use pointer events for WebKitGTK compatibility
  useEffect(() => {
    if (draggingTab) {
      // Use pointer events instead of mouse events for WebKitGTK compatibility
      const handlePointerMove = (e: PointerEvent) => handleTabDragMove(e as unknown as MouseEvent);
      const handlePointerUp = (e: PointerEvent) => handleTabDragEnd(e as unknown as MouseEvent);

      window.addEventListener('pointermove', handlePointerMove);
      window.addEventListener('pointerup', handlePointerUp);
      // Also listen for mouse events as fallback
      window.addEventListener('mousemove', handleTabDragMove);
      window.addEventListener('mouseup', handleTabDragEnd);
      return () => {
        window.removeEventListener('pointermove', handlePointerMove);
        window.removeEventListener('pointerup', handlePointerUp);
        window.removeEventListener('mousemove', handleTabDragMove);
        window.removeEventListener('mouseup', handleTabDragEnd);
      };
    }
  }, [draggingTab, handleTabDragMove, handleTabDragEnd]);

  const pinnedTabs = tabs.filter(t => t.is_pinned);
  const regularTabs = tabs.filter(t => !t.is_pinned);

  // Callback to register tab element refs
  const registerTabRef = useCallback((tabId: number, el: HTMLDivElement | null) => {
    if (el) {
      tabElementsRef.current.set(tabId, el);
    } else {
      tabElementsRef.current.delete(tabId);
    }
  }, []);

  return (
    <div
      ref={tabBarRef}
      className={`flex items-center bg-[var(--navbar-color)] border-b border-gray-700/50 h-9 select-none ${
        isDragOutMode ? 'border-b-2 border-b-[var(--primary-color)]' : ''
      }`}
    >
      {/* Drag-out indicator */}
      {draggingTab && isDragOutMode && (
        <div className="fixed inset-0 z-[100] pointer-events-none flex items-center justify-center">
          <div className="bg-[var(--primary-color)]/20 border-2 border-dashed border-[var(--primary-color)] rounded-lg px-6 py-3 text-[var(--primary-color)] font-medium shadow-xl">
            Release to open in new window
          </div>
        </div>
      )}

      {/* Floating tab preview during drag (Firefox-style) */}
      {draggingTab && !isDragOutMode && (() => {
        const tab = tabs.find(t => t.id === draggingTab.tabId);
        if (!tab) return null;
        return (
          <div
            className="fixed z-[200] pointer-events-none"
            style={{
              left: currentDragX - 60,
              top: draggingTab.startY - 18,
            }}
          >
            <div className="flex items-center gap-2 min-w-[120px] max-w-[200px] h-9 px-3 bg-[var(--navbar-color)] border border-[var(--primary-color)] rounded shadow-lg opacity-90">
              {tab.favicon ? (
                <img src={tab.favicon} className="w-4 h-4 flex-shrink-0" alt="" />
              ) : (
                <span className="w-4 h-4 flex items-center justify-center text-xs flex-shrink-0 text-[var(--primary-color)]">
                  {(tab.title || tab.url).charAt(0).toUpperCase()}
                </span>
              )}
              <span className="text-xs truncate flex-1 text-white">
                {tab.title || tab.url}
              </span>
            </div>
          </div>
        );
      })()}

      {/* Pinned tabs */}
      {pinnedTabs.map(tab => (
        <NativeClickableDiv
          key={tab.id}
          className={`
            flex items-center justify-center w-10 h-full cursor-pointer
            border-r border-gray-700/30 transition-all relative
            ${tab.is_active
              ? 'bg-[var(--primary-color)]/20 border-b-2 border-b-[var(--primary-color)]'
              : 'hover:bg-gray-700/30'}
          `}
          onClick={() => switchTab(tab.id)}
          onContextMenu={(e) => handleContextMenu(e, tab.id)}
          title={tab.title || tab.url}
        >
          {tab.favicon ? (
            <img src={tab.favicon} className="w-4 h-4 pointer-events-none" alt="" />
          ) : (
            <span className={`text-xs pointer-events-none ${tab.is_active ? 'text-[var(--primary-color)] font-bold' : 'text-[var(--primary-color)]'}`}>
              {(tab.title || tab.url).charAt(0).toUpperCase()}
            </span>
          )}
        </NativeClickableDiv>
      ))}

      {/* Regular tabs — scrolls horizontally when tabs overflow the width */}
      <div className="flex flex-1 overflow-x-auto scrollbar-none min-w-0">
        {regularTabs.map((tab, index) => (
          <div key={tab.id} className="flex relative flex-shrink-0">
            {/* Drop indicator before this tab */}
            {draggingTab && !isDragOutMode && dropTargetIndex === index && (
              <div className="absolute left-0 top-0 bottom-0 w-0.5 bg-[var(--primary-color)] z-20 -ml-0.5" />
            )}
            <NativeClickableDiv
              className={`
                flex items-center gap-2 min-w-[120px] max-w-[200px] h-full px-3 cursor-pointer
                border-r border-gray-700/30 transition-all group relative
                ${tab.is_active
                  ? 'bg-[var(--primary-color)]/20 border-b-2 border-b-[var(--primary-color)]'
                  : 'hover:bg-gray-700/30'}
                ${draggingTab?.tabId === tab.id ? 'ring-2 ring-[var(--primary-color)] opacity-50' : ''}
              `}
              onClick={() => switchTab(tab.id)}
              onContextMenu={(e) => handleContextMenu(e, tab.id)}
              onDragStart={(e) => handleTabDragStart(tab.id, e)}
              isDragging={draggingTab?.tabId === tab.id}
              title={tab.title || tab.url}
              tabId={tab.id}
              registerRef={registerTabRef}
            >
              {tab.favicon ? (
                <img src={tab.favicon} className="w-4 h-4 flex-shrink-0 pointer-events-none" alt="" />
              ) : (
                <span className={`w-4 h-4 flex items-center justify-center text-xs flex-shrink-0 pointer-events-none ${
                  tab.is_active ? 'text-[var(--primary-color)] font-bold' : 'text-[var(--primary-color)]'
                }`}>
                  {(tab.title || tab.url).charAt(0).toUpperCase()}
                </span>
              )}
              <span className={`text-xs truncate flex-1 pointer-events-none ${
                tab.is_active ? 'text-white font-medium' : 'text-[var(--text-color)]'
              }`}>
                {tab.title || tab.url}
              </span>
              <NativeButton
                onClick={() => closeTab(tab.id)}
                className="p-0.5 rounded hover:bg-gray-600/50 opacity-0 group-hover:opacity-100 transition-opacity cursor-pointer"
                title="Close tab"
              >
                <svg className="w-3 h-3 pointer-events-none" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                </svg>
              </NativeButton>
            </NativeClickableDiv>
            {/* Drop indicator after last tab */}
            {draggingTab && !isDragOutMode && dropTargetIndex === regularTabs.length && index === regularTabs.length - 1 && (
              <div className="absolute right-0 top-0 bottom-0 w-0.5 bg-[var(--primary-color)] z-20 -mr-0.5" />
            )}
          </div>
        ))}
      </div>

      {/* New tab button */}
      <NativeButton
        onClick={() => createTab()}
        className="flex items-center justify-center w-8 h-full hover:bg-gray-700/30 transition-colors cursor-pointer"
        title="New Tab (Ctrl+T)"
      >
        <svg className="w-4 h-4 pointer-events-none" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
        </svg>
      </NativeButton>


      {/* Context menu */}
      {contextMenu && (
        <div
          className="fixed z-50 bg-[var(--card-bg-color)] border border-gray-700 rounded-lg shadow-xl py-1 min-w-[160px]"
          style={{ top: contextMenu.y, left: contextMenu.x }}
        >
          <NativeButton
            className="w-full px-4 py-2 text-left text-sm hover:bg-gray-700/50 transition-colors cursor-pointer"
            onClick={() => {
              togglePin(contextMenu.tabId);
              setContextMenu(null);
            }}
          >
            <span className="pointer-events-none">
              {tabs.find(t => t.id === contextMenu.tabId)?.is_pinned ? 'Unpin Tab' : 'Pin Tab'}
            </span>
          </NativeButton>
          <NativeButton
            className="w-full px-4 py-2 text-left text-sm hover:bg-gray-700/50 transition-colors cursor-pointer"
            onClick={() => {
              duplicateTab(contextMenu.tabId);
              setContextMenu(null);
            }}
          >
            <span className="pointer-events-none">Duplicate Tab</span>
          </NativeButton>
          <div className="border-t border-gray-700 my-1" />
          <NativeButton
            className="w-full px-4 py-2 text-left text-sm hover:bg-gray-700/50 transition-colors cursor-pointer"
            onClick={() => {
              closeTab(contextMenu.tabId);
              setContextMenu(null);
            }}
          >
            <span className="pointer-events-none">Close Tab</span>
          </NativeButton>
          <NativeButton
            className="w-full px-4 py-2 text-left text-sm hover:bg-gray-700/50 transition-colors cursor-pointer"
            onClick={() => {
              closeTabsToRight(contextMenu.tabId);
              setContextMenu(null);
            }}
          >
            <span className="pointer-events-none">Close Tabs to Right</span>
          </NativeButton>
        </div>
      )}
    </div>
  );
}

// Hook for using tabs
export function useTabs(profileId: number) {
  const [tabs, setTabs] = useState<Tab[]>([]);
  const [activeTab, setActiveTab] = useState<Tab | null>(null);

  const loadTabs = useCallback(async () => {
    try {
      const loadedTabs = await invoke<Tab[]>('get_all_tabs', { profileId: profileId });
      setTabs(loadedTabs);
      setActiveTab(loadedTabs.find(t => t.is_active) || null);
    } catch (err) {
      console.error('Failed to load tabs:', err);
    }
  }, [profileId]);

  useEffect(() => {
    loadTabs();
  }, [loadTabs]);

  const createTab = async (url: string = 'earth://newtab', title?: string) => {
    try {
      const newTab = await invoke<Tab>('create_tab', {
        profileId: profileId,
        url,
        title,
      });
      await invoke('set_active_tab', { tabId: newTab.id });
      loadTabs();
      return newTab;
    } catch (err) {
      console.error('Failed to create tab:', err);
      return null;
    }
  };

  const updateTab = async (tabId: number, updates: { title?: string; url?: string; favicon?: string }) => {
    try {
      await invoke('update_tab', { tabId: tabId, ...updates });
      loadTabs();
    } catch (err) {
      console.error('Failed to update tab:', err);
    }
  };

  return {
    tabs,
    activeTab,
    loadTabs,
    createTab,
    updateTab,
  };
}

export default TabBar;
