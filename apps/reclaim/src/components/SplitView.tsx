import { useState, useEffect, useCallback, ReactNode } from 'react';
import { invoke } from '../lib/tauri';

// Types
export type SplitLayout = 'single' | 'horizontal' | 'vertical' | 'quad';

export interface PaneSizes {
  pane_1: number;
  pane_2: number;
  pane_3?: number;
  pane_4?: number;
}

export interface SplitViewConfig {
  profile_id: number;
  layout: SplitLayout;
  pane_1_tab_id: number | null;
  pane_2_tab_id: number | null;
  pane_3_tab_id: number | null;
  pane_4_tab_id: number | null;
  active_pane: number;
  pane_sizes: PaneSizes | null;
}

interface SplitViewProps {
  profileId: number;
  children?: (paneNumber: number, tabId: number | null, isActive: boolean) => ReactNode;
  onLayoutChange?: (layout: SplitLayout) => void;
  onActivePaneChange?: (paneNumber: number) => void;
}

interface LayoutButtonsProps {
  currentLayout: SplitLayout;
  onLayoutChange: (layout: SplitLayout) => void;
}

// ==================== SplitView Component ====================
export function SplitView({ profileId, children, onLayoutChange, onActivePaneChange }: SplitViewProps) {
  const [config, setConfig] = useState<SplitViewConfig | null>(null);
  const [isDragging, setIsDragging] = useState(false);
  const [dragDirection, setDragDirection] = useState<'horizontal' | 'vertical' | null>(null);

  // Load split config
  useEffect(() => {
    loadConfig();
  }, [profileId]);

  const loadConfig = async () => {
    try {
      const cfg = await invoke<SplitViewConfig>('get_split_config', { profileId: profileId });
      setConfig(cfg);
    } catch (err) {
      console.error('Failed to load split config:', err);
    }
  };

  // Set layout
  const setLayout = async (layout: SplitLayout) => {
    try {
      const cfg = await invoke<SplitViewConfig>('set_split_layout', { profileId: profileId, layout });
      setConfig(cfg);
      onLayoutChange?.(layout);
    } catch (err) {
      console.error('Failed to set layout:', err);
    }
  };

  // Set active pane
  const setActivePane = async (paneNumber: number) => {
    try {
      const cfg = await invoke<SplitViewConfig>('set_active_pane', { profileId: profileId, paneNumber: paneNumber });
      setConfig(cfg);
      onActivePaneChange?.(paneNumber);
    } catch (err) {
      console.error('Failed to set active pane:', err);
    }
  };

  // Cycle through panes
  const cyclePanes = async (direction: number = 1) => {
    try {
      const cfg = await invoke<SplitViewConfig>('cycle_pane', { profileId: profileId, direction });
      setConfig(cfg);
      onActivePaneChange?.(cfg.active_pane);
    } catch (err) {
      console.error('Failed to cycle panes:', err);
    }
  };

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Alt+1-4 - Switch to pane
      if (e.altKey && e.key >= '1' && e.key <= '4') {
        e.preventDefault();
        setActivePane(parseInt(e.key));
      }
      // Alt+H - Horizontal split
      if (e.altKey && e.key === 'h') {
        e.preventDefault();
        setLayout('horizontal');
      }
      // Alt+V - Vertical split
      if (e.altKey && e.key === 'v') {
        e.preventDefault();
        setLayout('vertical');
      }
      // Alt+Q - Quad view
      if (e.altKey && e.key === 'q') {
        e.preventDefault();
        setLayout('quad');
      }
      // Alt+S or Escape - Single view
      if (e.altKey && e.key === 's') {
        e.preventDefault();
        setLayout('single');
      }
      // Alt+Tab - Cycle panes
      if (e.altKey && e.key === 'Tab') {
        e.preventDefault();
        cyclePanes(e.shiftKey ? -1 : 1);
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [config]);

  if (!config) return null;

  const renderPane = (paneNumber: number, tabId: number | null, isActive: boolean) => {
    return (
      <div
        className={`
          relative flex-1 min-w-0 min-h-0 overflow-hidden
          ${isActive ? 'ring-2 ring-[var(--primary-color)] ring-inset' : ''}
          ${config.layout !== 'single' ? 'border border-gray-700/50' : ''}
        `}
        onClick={() => setActivePane(paneNumber)}
      >
        {children ? children(paneNumber, tabId, isActive) : (
          <div className="flex items-center justify-center h-full text-gray-500">
            Pane {paneNumber}
            {!tabId && <span className="ml-2 text-xs">(No tab assigned)</span>}
          </div>
        )}
        {config.layout !== 'single' && isActive && (
          <div className="absolute top-1 left-1 px-1.5 py-0.5 bg-[var(--primary-color)]/80 rounded text-[10px] font-medium">
            {paneNumber}
          </div>
        )}
      </div>
    );
  };

  const renderDivider = (direction: 'horizontal' | 'vertical') => (
    <div
      className={`
        ${direction === 'horizontal' ? 'w-1 cursor-col-resize' : 'h-1 cursor-row-resize'}
        bg-gray-700/50 hover:bg-[var(--primary-color)]/50 transition-colors
        ${isDragging && dragDirection === direction ? 'bg-[var(--primary-color)]' : ''}
      `}
      onMouseDown={() => {
        setIsDragging(true);
        setDragDirection(direction);
      }}
    />
  );

  // Single layout
  if (config.layout === 'single') {
    return (
      <div className="flex-1 flex flex-col overflow-hidden">
        {renderPane(1, config.pane_1_tab_id, true)}
      </div>
    );
  }

  // Horizontal split (side by side)
  if (config.layout === 'horizontal') {
    return (
      <div className="flex-1 flex flex-row overflow-hidden">
        {renderPane(1, config.pane_1_tab_id, config.active_pane === 1)}
        {renderDivider('horizontal')}
        {renderPane(2, config.pane_2_tab_id, config.active_pane === 2)}
      </div>
    );
  }

  // Vertical split (top/bottom)
  if (config.layout === 'vertical') {
    return (
      <div className="flex-1 flex flex-col overflow-hidden">
        {renderPane(1, config.pane_1_tab_id, config.active_pane === 1)}
        {renderDivider('vertical')}
        {renderPane(2, config.pane_2_tab_id, config.active_pane === 2)}
      </div>
    );
  }

  // Quad layout (2x2 grid)
  if (config.layout === 'quad') {
    return (
      <div className="flex-1 flex flex-col overflow-hidden">
        <div className="flex-1 flex flex-row min-h-0">
          {renderPane(1, config.pane_1_tab_id, config.active_pane === 1)}
          {renderDivider('horizontal')}
          {renderPane(2, config.pane_2_tab_id, config.active_pane === 2)}
        </div>
        {renderDivider('vertical')}
        <div className="flex-1 flex flex-row min-h-0">
          {renderPane(3, config.pane_3_tab_id, config.active_pane === 3)}
          {renderDivider('horizontal')}
          {renderPane(4, config.pane_4_tab_id, config.active_pane === 4)}
        </div>
      </div>
    );
  }

  return null;
}

// ==================== Layout Buttons ====================
export function LayoutButtons({ currentLayout, onLayoutChange }: LayoutButtonsProps) {
  const layouts: { id: SplitLayout; icon: ReactNode; title: string; shortcut: string }[] = [
    {
      id: 'single',
      title: 'Single View',
      shortcut: 'Alt+S',
      icon: (
        <svg className="w-4 h-4" viewBox="0 0 16 16" fill="currentColor">
          <rect x="2" y="2" width="12" height="12" rx="1" />
        </svg>
      ),
    },
    {
      id: 'horizontal',
      title: 'Horizontal Split',
      shortcut: 'Alt+H',
      icon: (
        <svg className="w-4 h-4" viewBox="0 0 16 16" fill="currentColor">
          <rect x="2" y="2" width="5" height="12" rx="1" />
          <rect x="9" y="2" width="5" height="12" rx="1" />
        </svg>
      ),
    },
    {
      id: 'vertical',
      title: 'Vertical Split',
      shortcut: 'Alt+V',
      icon: (
        <svg className="w-4 h-4" viewBox="0 0 16 16" fill="currentColor">
          <rect x="2" y="2" width="12" height="5" rx="1" />
          <rect x="2" y="9" width="12" height="5" rx="1" />
        </svg>
      ),
    },
    {
      id: 'quad',
      title: 'Quad View',
      shortcut: 'Alt+Q',
      icon: (
        <svg className="w-4 h-4" viewBox="0 0 16 16" fill="currentColor">
          <rect x="2" y="2" width="5" height="5" rx="1" />
          <rect x="9" y="2" width="5" height="5" rx="1" />
          <rect x="2" y="9" width="5" height="5" rx="1" />
          <rect x="9" y="9" width="5" height="5" rx="1" />
        </svg>
      ),
    },
  ];

  return (
    <div className="flex items-center gap-1">
      {layouts.map(layout => (
        <button
          key={layout.id}
          onClick={() => onLayoutChange(layout.id)}
          className={`
            p-1.5 rounded transition-colors
            ${currentLayout === layout.id
              ? 'bg-[var(--primary-color)]/20 text-[var(--primary-color)]'
              : 'hover:bg-gray-700/50 text-gray-400'
            }
          `}
          title={`${layout.title} (${layout.shortcut})`}
        >
          {layout.icon}
        </button>
      ))}
    </div>
  );
}

// ==================== Split View Hook ====================
export function useSplitView(profileId: number) {
  const [config, setConfig] = useState<SplitViewConfig | null>(null);

  const loadConfig = useCallback(async () => {
    try {
      const cfg = await invoke<SplitViewConfig>('get_split_config', { profileId: profileId });
      setConfig(cfg);
    } catch (err) {
      console.error('Failed to load split config:', err);
    }
  }, [profileId]);

  useEffect(() => {
    loadConfig();
  }, [loadConfig]);

  const setLayout = async (layout: SplitLayout) => {
    try {
      const cfg = await invoke<SplitViewConfig>('set_split_layout', { profileId: profileId, layout });
      setConfig(cfg);
      return cfg;
    } catch (err) {
      console.error('Failed to set layout:', err);
      return null;
    }
  };

  const setPaneTab = async (paneNumber: number, tabId: number | null) => {
    try {
      const cfg = await invoke<SplitViewConfig>('set_pane_tab', {
        profile_id: profileId,
        pane_number: paneNumber,
        tab_id: tabId,
      });
      setConfig(cfg);
      return cfg;
    } catch (err) {
      console.error('Failed to set pane tab:', err);
      return null;
    }
  };

  const setActivePane = async (paneNumber: number) => {
    try {
      const cfg = await invoke<SplitViewConfig>('set_active_pane', { profileId: profileId, paneNumber: paneNumber });
      setConfig(cfg);
      return cfg;
    } catch (err) {
      console.error('Failed to set active pane:', err);
      return null;
    }
  };

  const swapPanes = async (paneA: number, paneB: number) => {
    try {
      const cfg = await invoke<SplitViewConfig>('swap_panes', {
        profile_id: profileId,
        pane_a: paneA,
        pane_b: paneB,
      });
      setConfig(cfg);
      return cfg;
    } catch (err) {
      console.error('Failed to swap panes:', err);
      return null;
    }
  };

  const resetToSingle = async () => {
    try {
      const cfg = await invoke<SplitViewConfig>('reset_split_view', { profileId: profileId });
      setConfig(cfg);
      return cfg;
    } catch (err) {
      console.error('Failed to reset split view:', err);
      return null;
    }
  };

  return {
    config,
    loadConfig,
    setLayout,
    setPaneTab,
    setActivePane,
    swapPanes,
    resetToSingle,
    layout: config?.layout || 'single',
    activePane: config?.active_pane || 1,
  };
}

export default SplitView;
