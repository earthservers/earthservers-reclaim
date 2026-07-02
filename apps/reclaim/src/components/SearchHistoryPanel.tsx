// Saved searches & search history — a right-dock panel (same dock as NoScript /
// Privacy / autofill). History is the local search_queries log grouped by text;
// saved searches also carry their config (retention / kinds / sources) so
// re-running restores the exact same search. Clicking either runs the search in
// the current search tab (App decides the target tab).

import { useCallback, useEffect, useState } from 'react';
import { invoke, listen, isTauri } from '../lib/tauri';
import { RightDockPanel, RIGHT_DOCK_WIDTH } from '../lib/rightDock';
import type { Retention, KindsMode } from './LocalSearchResults';

export interface SearchHistoryEntry {
  queryId: number;
  queryText: string;
  lastSearchedAt: number;
  timesSearched: number;
}

export interface SavedSearch {
  id: number;
  queryText: string;
  retention: Retention;
  kindsMode: KindsMode;
  sources: string[] | null;
  createdAt: number;
}

/// The active search the panel can save (query + config), if any.
export interface CurrentSearch {
  query: string;
  retention: Retention;
  kindsMode: KindsMode;
  sources: string[] | null;
}

export function formatRelativeTime(unixSecs: number): string {
  const delta = Math.max(0, Math.floor(Date.now() / 1000) - unixSecs);
  if (delta < 60) return 'just now';
  if (delta < 3600) return `${Math.floor(delta / 60)}m ago`;
  if (delta < 86400) return `${Math.floor(delta / 3600)}h ago`;
  if (delta < 7 * 86400) return `${Math.floor(delta / 86400)}d ago`;
  return new Date(unixSecs * 1000).toLocaleDateString();
}

/// Run a query (saved entries pass their stored config along).
export type RunSearch = (query: string, cfg?: { retention: Retention; kindsMode: KindsMode; sources: string[] | null }) => void;

/// The saved + recent lists themselves. Shared between the right-dock panel and
/// the "Searches" tab on the Local AI / History page.
export function SearchHistoryList({
  profileId,
  active,
  current,
  onRun,
}: {
  profileId: number | null;
  /// Load (and live-refresh) only while actually visible.
  active: boolean;
  /// The active tab's live search (to offer "Save this search"), if one is running.
  current: CurrentSearch | null;
  onRun: RunSearch;
}) {
  const [saved, setSaved] = useState<SavedSearch[]>([]);
  const [history, setHistory] = useState<SearchHistoryEntry[]>([]);
  const [confirmClear, setConfirmClear] = useState(false);

  const reload = useCallback(() => {
    if (!profileId || !isTauri()) return;
    invoke<SavedSearch[]>('list_saved_searches', { profileId }).then(setSaved).catch(() => {});
    invoke<SearchHistoryEntry[]>('list_search_history', { profileId, limit: 50 }).then(setHistory).catch(() => {});
  }, [profileId]);

  // Load when shown; refresh while visible whenever a new search starts anywhere.
  useEffect(() => {
    if (!active) return;
    reload();
    let unlisten: (() => void) | undefined;
    listen('local-search-started', () => reload()).then(u => { unlisten = u; });
    return () => unlisten?.();
  }, [active, reload]);

  const saveCurrent = async () => {
    if (!profileId || !current?.query.trim()) return;
    try {
      await invoke('save_search', {
        profileId,
        queryText: current.query,
        retention: current.retention,
        kindsMode: current.kindsMode,
        sources: current.sources,
      });
      reload();
    } catch (err) {
      console.error('Failed to save search:', err);
    }
  };

  const saveFromHistory = async (entry: SearchHistoryEntry) => {
    if (!profileId) return;
    try {
      // History rows don't carry config; save with the standard defaults.
      await invoke('save_search', {
        profileId,
        queryText: entry.queryText,
        retention: 'cache',
        kindsMode: 'all',
        sources: null,
      });
      reload();
    } catch (err) {
      console.error('Failed to save search:', err);
    }
  };

  const removeSaved = async (id: number) => {
    try {
      await invoke('delete_saved_search', { id });
      setSaved(prev => prev.filter(s => s.id !== id));
    } catch (err) {
      console.error('Failed to delete saved search:', err);
    }
  };

  const removeHistory = async (entry: SearchHistoryEntry) => {
    if (!profileId) return;
    try {
      await invoke('delete_search_history', { profileId, queryText: entry.queryText });
      setHistory(prev => prev.filter(h => h.queryText !== entry.queryText));
    } catch (err) {
      console.error('Failed to delete history entry:', err);
    }
  };

  const clearHistory = async () => {
    if (!profileId) return;
    try {
      await invoke('clear_search_history', { profileId });
      setHistory([]);
      setConfirmClear(false);
    } catch (err) {
      console.error('Failed to clear search history:', err);
    }
  };

  const savedTexts = new Set(saved.map(s => s.queryText));

  return (
      <div className="flex flex-col gap-4 p-1">
        {/* Save the search currently on screen */}
        {current?.query.trim() && !savedTexts.has(current.query.trim()) && (
          <button
            onClick={saveCurrent}
            className="flex items-center gap-2 px-3 py-2 rounded-lg text-sm bg-[var(--primary-color)]/15 border border-[var(--primary-color)]/40 text-white hover:bg-[var(--primary-color)]/25 transition-colors text-left"
          >
            <svg className="w-4 h-4 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 5a2 2 0 012-2h10a2 2 0 012 2v16l-7-3.5L5 21V5z" />
            </svg>
            <span className="truncate">Save “{current.query}”</span>
          </button>
        )}

        {/* Saved searches */}
        <div>
          <div className="px-1 mb-1.5 text-[11px] font-semibold uppercase tracking-wide text-gray-500">Saved searches</div>
          {saved.length === 0 ? (
            <p className="px-1 text-xs text-gray-500">Nothing saved yet — save a search to re-run it with the same sources &amp; filters.</p>
          ) : (
            <div className="space-y-1">
              {saved.map(s => (
                <div key={s.id} className="group flex items-center gap-2 px-2 py-1.5 rounded-lg hover:bg-white/10 transition-colors">
                  <button
                    onClick={() => onRun(s.queryText, { retention: s.retention, kindsMode: s.kindsMode, sources: s.sources })}
                    className="flex-1 min-w-0 text-left"
                    title={`Run “${s.queryText}”`}
                  >
                    <div className="text-sm text-white truncate">{s.queryText}</div>
                    <div className="text-[10px] text-gray-500 truncate">
                      {s.retention}{s.kindsMode !== 'all' ? ` · ${s.kindsMode}` : ''}{s.sources ? ` · ${s.sources.length} source${s.sources.length === 1 ? '' : 's'}` : ''}
                    </div>
                  </button>
                  <button
                    onClick={() => removeSaved(s.id)}
                    className="p-1 rounded opacity-0 group-hover:opacity-100 text-gray-500 hover:text-red-400 hover:bg-white/10 transition-all flex-shrink-0"
                    title="Remove saved search"
                  >
                    <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                    </svg>
                  </button>
                </div>
              ))}
            </div>
          )}
        </div>

        {/* Recent searches */}
        <div>
          <div className="px-1 mb-1.5 flex items-center justify-between">
            <span className="text-[11px] font-semibold uppercase tracking-wide text-gray-500">Recent searches</span>
            {history.length > 0 && (
              confirmClear ? (
                <span className="flex items-center gap-2 text-[11px]">
                  <button onClick={clearHistory} className="text-red-400 hover:text-red-300">Clear all?</button>
                  <button onClick={() => setConfirmClear(false)} className="text-gray-500 hover:text-white">Cancel</button>
                </span>
              ) : (
                <button onClick={() => setConfirmClear(true)} className="text-[11px] text-gray-500 hover:text-white">Clear</button>
              )
            )}
          </div>
          {history.length === 0 ? (
            <p className="px-1 text-xs text-gray-500">No searches yet.</p>
          ) : (
            <div className="space-y-0.5">
              {history.map(h => (
                <div key={h.queryText} className="group flex items-center gap-2 px-2 py-1.5 rounded-lg hover:bg-white/10 transition-colors">
                  <svg className="w-3.5 h-3.5 text-gray-500 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
                  </svg>
                  <button onClick={() => onRun(h.queryText)} className="flex-1 min-w-0 text-left" title={`Search “${h.queryText}” again`}>
                    <div className="text-sm text-gray-200 truncate">{h.queryText}</div>
                    <div className="text-[10px] text-gray-500">
                      {formatRelativeTime(h.lastSearchedAt)}{h.timesSearched > 1 ? ` · ${h.timesSearched}×` : ''}
                    </div>
                  </button>
                  <div className="flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity flex-shrink-0">
                    {!savedTexts.has(h.queryText) && (
                      <button
                        onClick={() => saveFromHistory(h)}
                        className="p-1 rounded text-gray-500 hover:text-white hover:bg-white/10 transition-all"
                        title="Save this search"
                      >
                        <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 5a2 2 0 012-2h10a2 2 0 012 2v16l-7-3.5L5 21V5z" />
                        </svg>
                      </button>
                    )}
                    <button
                      onClick={() => removeHistory(h)}
                      className="p-1 rounded text-gray-500 hover:text-red-400 hover:bg-white/10 transition-all"
                      title="Remove from history"
                    >
                      <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                      </svg>
                    </button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
  );
}

/// The right-dock variant (same dock as NoScript / Privacy / autofill panels).
export function SearchHistoryPanel({
  profileId,
  isOpen,
  onClose,
  current,
  onRun,
}: {
  profileId: number | null;
  isOpen: boolean;
  onClose: () => void;
  current: CurrentSearch | null;
  onRun: RunSearch;
}) {
  return (
    <RightDockPanel
      id="search-history"
      open={isOpen}
      width={RIGHT_DOCK_WIDTH}
      title="Searches"
      subtitle="Saved searches & history — local only, never leaves this device"
      onClose={onClose}
    >
      <SearchHistoryList profileId={profileId} active={isOpen} current={current} onRun={onRun} />
    </RightDockPanel>
  );
}

export default SearchHistoryPanel;
