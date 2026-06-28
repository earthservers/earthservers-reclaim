import { useState, useEffect, useCallback } from 'react';
import { invoke, saveDialog, writeFile } from '../lib/tauri';

// Types matching Rust structs
interface HistoryEntry {
  id: number;
  url: string;
  title: string;
  visited_at: string;
  profile_id: number;
}

interface HistoryStats {
  total_pages: number;
  total_domains: number;
  most_visited: { domain: string; visit_count: number }[];
  recent_pages: HistoryEntry[];
}

interface HistoryViewerProps {
  profileId: number | null;
  isOpen: boolean;
  onClose: () => void;
}

export function HistoryViewer({ profileId, isOpen, onClose }: HistoryViewerProps) {
  const [history, setHistory] = useState<HistoryEntry[]>([]);
  const [stats, setStats] = useState<HistoryStats | null>(null);
  const [searchQuery, setSearchQuery] = useState('');
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<'history' | 'stats'>('history');
  const [showClearConfirm, setShowClearConfirm] = useState(false);

  // Load data when opened or profile changes
  useEffect(() => {
    if (isOpen && profileId) {
      loadHistory();
      loadStats();
    }
  }, [isOpen, profileId]);

  const loadHistory = useCallback(async () => {
    if (!profileId) return;

    setIsLoading(true);
    try {
      const entries = await invoke<HistoryEntry[]>('get_history', {
        profileId,
        searchQuery: searchQuery || null,
        limit: 100,
        offset: 0,
      });
      setHistory(entries);
      setError(null);
    } catch (err) {
      setError(`Failed to load history: ${err}`);
    } finally {
      setIsLoading(false);
    }
  }, [profileId, searchQuery]);

  const loadStats = useCallback(async () => {
    if (!profileId) return;

    try {
      const historyStats = await invoke<HistoryStats>('get_history_stats', { profileId });
      setStats(historyStats);
    } catch (err) {
      console.error('Failed to load stats:', err);
    }
  }, [profileId]);

  const handleSearch = (e: React.FormEvent) => {
    e.preventDefault();
    loadHistory();
  };

  const handleDeleteEntry = async (entryId: number) => {
    if (!profileId) return;

    try {
      await invoke('delete_history_entry', { entryId, profileId });
      setHistory(history.filter(h => h.id !== entryId));
      loadStats(); // Refresh stats
    } catch (err) {
      setError(`Failed to delete entry: ${err}`);
    }
  };

  const handleClearAllHistory = async () => {
    if (!profileId) return;

    try {
      const deleted = await invoke<number>('clear_all_history', { profileId });
      setHistory([]);
      loadStats();
      setShowClearConfirm(false);
      setError(null);
      console.log(`Cleared ${deleted} history entries`);
    } catch (err) {
      setError(`Failed to clear history: ${err}`);
    }
  };

  const handleExportHistory = async () => {
    if (!profileId) return;

    try {
      const json = await invoke<string>('export_history', { profileId });
      const path = await saveDialog({
        filters: [{ name: 'JSON', extensions: ['json'] }],
        defaultPath: 'history-export.json',
      });

      if (path) {
        await writeFile(path, json);
        setError(null);
      }
    } catch (err) {
      setError(`Failed to export history: ${err}`);
    }
  };

  const formatRelativeTime = (timestamp: string): string => {
    try {
      const date = new Date(parseInt(timestamp) * 1000);
      const now = new Date();
      const diff = now.getTime() - date.getTime();

      const minutes = Math.floor(diff / 60000);
      const hours = Math.floor(diff / 3600000);
      const days = Math.floor(diff / 86400000);

      if (minutes < 1) return 'Just now';
      if (minutes < 60) return `${minutes}m ago`;
      if (hours < 24) return `${hours}h ago`;
      if (days < 7) return `${days}d ago`;
      return date.toLocaleDateString();
    } catch {
      return timestamp;
    }
  };

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex items-center justify-center p-4">
      <div className="bg-gray-900 border border-white/10 rounded-2xl shadow-2xl w-full max-w-4xl max-h-[80vh] flex flex-col">
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-white/10">
          <div className="flex items-center gap-4">
            <h2 className="text-xl font-bold text-white">Browsing History</h2>
            <div className="flex bg-white/5 rounded-lg p-1">
              <button
                onClick={() => setActiveTab('history')}
                className={`px-3 py-1 rounded-md text-sm transition-colors ${
                  activeTab === 'history'
                    ? 'bg-earth-teal text-white'
                    : 'text-gray-400 hover:text-white'
                }`}
              >
                History
              </button>
              <button
                onClick={() => setActiveTab('stats')}
                className={`px-3 py-1 rounded-md text-sm transition-colors ${
                  activeTab === 'stats'
                    ? 'bg-earth-teal text-white'
                    : 'text-gray-400 hover:text-white'
                }`}
              >
                Statistics
              </button>
            </div>
          </div>
          <button
            onClick={onClose}
            className="p-2 text-gray-400 hover:text-white transition-colors"
          >
            <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        {/* Error Message */}
        {error && (
          <div className="px-6 py-2 bg-red-500/20 text-red-400 text-sm">
            {error}
          </div>
        )}

        {/* Content */}
        <div className="flex-1 overflow-hidden flex flex-col">
          {activeTab === 'history' ? (
            <>
              {/* Search and Actions */}
              <div className="px-6 py-4 border-b border-white/10 flex gap-4">
                <form onSubmit={handleSearch} className="flex-1">
                  <div className="relative">
                    <input
                      type="text"
                      placeholder="Search history..."
                      value={searchQuery}
                      onChange={(e) => setSearchQuery(e.target.value)}
                      className="w-full pl-10 pr-4 py-2 bg-white/5 border border-white/10 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-earth-teal"
                    />
                    <svg
                      className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-gray-500"
                      fill="none"
                      viewBox="0 0 24 24"
                      stroke="currentColor"
                    >
                      <path
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        strokeWidth={2}
                        d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
                      />
                    </svg>
                  </div>
                </form>
                <button
                  onClick={handleExportHistory}
                  className="px-4 py-2 bg-white/5 border border-white/10 rounded-lg text-gray-300 hover:text-white hover:border-earth-teal transition-colors flex items-center gap-2"
                >
                  <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      strokeWidth={2}
                      d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4"
                    />
                  </svg>
                  Export
                </button>
                <button
                  onClick={() => setShowClearConfirm(true)}
                  className="px-4 py-2 bg-red-500/10 border border-red-500/30 rounded-lg text-red-400 hover:bg-red-500/20 transition-colors flex items-center gap-2"
                >
                  <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      strokeWidth={2}
                      d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"
                    />
                  </svg>
                  Clear All
                </button>
              </div>

              {/* History List */}
              <div className="flex-1 overflow-y-auto px-6 py-2">
                {isLoading ? (
                  <div className="flex items-center justify-center py-12">
                    <div className="text-gray-400">Loading history...</div>
                  </div>
                ) : history.length === 0 ? (
                  <div className="flex flex-col items-center justify-center py-12 text-gray-400">
                    <svg className="w-12 h-12 mb-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        strokeWidth={1.5}
                        d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z"
                      />
                    </svg>
                    <p>No history found</p>
                    {searchQuery && (
                      <p className="text-sm mt-1">Try a different search term</p>
                    )}
                  </div>
                ) : (
                  <div className="space-y-2">
                    {history.map((entry) => (
                      <div
                        key={entry.id}
                        className="flex items-center gap-4 p-3 bg-white/5 rounded-lg hover:bg-white/10 transition-colors group"
                      >
                        <div className="flex-1 min-w-0">
                          <div className="text-white font-medium truncate">{entry.title}</div>
                          <div className="text-sm text-gray-400 truncate">{entry.url}</div>
                        </div>
                        <div className="text-sm text-gray-500 whitespace-nowrap">
                          {formatRelativeTime(entry.visited_at)}
                        </div>
                        <button
                          onClick={() => handleDeleteEntry(entry.id)}
                          className="p-1 text-gray-500 hover:text-red-400 opacity-0 group-hover:opacity-100 transition-all"
                          title="Delete entry"
                        >
                          <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                          </svg>
                        </button>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            </>
          ) : (
            /* Statistics Tab */
            <div className="flex-1 overflow-y-auto p-6">
              {stats ? (
                <div className="grid grid-cols-2 gap-6">
                  {/* Overview Cards */}
                  <div className="bg-white/5 rounded-xl p-6 border border-white/10">
                    <div className="text-4xl font-bold text-earth-teal">{stats.total_pages}</div>
                    <div className="text-gray-400 mt-1">Total Pages Visited</div>
                  </div>
                  <div className="bg-white/5 rounded-xl p-6 border border-white/10">
                    <div className="text-4xl font-bold text-earth-pink">{stats.total_domains}</div>
                    <div className="text-gray-400 mt-1">Unique Domains</div>
                  </div>

                  {/* Most Visited */}
                  <div className="col-span-2 bg-white/5 rounded-xl p-6 border border-white/10">
                    <h3 className="text-lg font-semibold text-white mb-4">Most Visited Domains</h3>
                    {stats.most_visited.length > 0 ? (
                      <div className="space-y-3">
                        {stats.most_visited.map((domain, index) => (
                          <div key={domain.domain} className="flex items-center gap-4">
                            <div className="w-6 text-center text-gray-500 font-medium">
                              {index + 1}
                            </div>
                            <div className="flex-1 text-white">{domain.domain}</div>
                            <div className="text-earth-teal font-medium">
                              {domain.visit_count} visits
                            </div>
                          </div>
                        ))}
                      </div>
                    ) : (
                      <div className="text-gray-400">No data available</div>
                    )}
                  </div>
                </div>
              ) : (
                <div className="flex items-center justify-center py-12">
                  <div className="text-gray-400">Loading statistics...</div>
                </div>
              )}
            </div>
          )}
        </div>

        {/* Clear Confirmation Modal */}
        {showClearConfirm && (
          <div className="absolute inset-0 bg-black/60 flex items-center justify-center rounded-2xl">
            <div className="bg-gray-800 border border-white/10 rounded-xl p-6 max-w-md mx-4">
              <h3 className="text-xl font-bold text-white mb-2">Clear All History?</h3>
              <p className="text-gray-400 mb-6">
                This will permanently delete all browsing history for this profile. This action cannot be undone.
              </p>
              <div className="flex gap-3 justify-end">
                <button
                  onClick={() => setShowClearConfirm(false)}
                  className="px-4 py-2 text-gray-400 hover:text-white transition-colors"
                >
                  Cancel
                </button>
                <button
                  onClick={handleClearAllHistory}
                  className="px-4 py-2 bg-red-500 text-white rounded-lg hover:bg-red-600 transition-colors"
                >
                  Clear All History
                </button>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

export default HistoryViewer;
