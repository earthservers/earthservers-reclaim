// Domain Manager for EarthSearch
// Manages curated domain whitelists with CRUD operations

import { useState, useEffect, useCallback } from 'react';
import { invoke } from '../lib/tauri';
import { RatingBadge, RatingForm, RatingDisplay } from './RatingComponents';
import { LocalSearchResults } from './LocalSearchResults';

interface Domain {
  id: number | null;
  url: string;
  category: string;
  trust_score: number;
  added_date: string;
  metadata: string | null;
  profile_id: number | null;
}

interface DomainList {
  id: number | null;
  name: string;
  description: string | null;
  author: string | null;
  version: string;
  created_at: string;
  profile_id: number | null;
  domain_count: number | null;
}

interface DomainStats {
  total_domains: number;
  total_lists: number;
  categories: { category: string; count: number }[];
  avg_trust_score: number;
}

interface DomainManagerProps {
  profileId: number | null;
  // opts.fromAddressBar = the URL was TYPED (navigate current tab); a domain CLICK
  // omits it so it respects the "When opening links" toggle (e.g. opens a new tab).
  onOpenUrl?: (url: string, opts?: { fromAddressBar?: boolean }) => void;
}

const DEFAULT_CATEGORIES = [
  'technology',
  'science',
  'education',
  'news',
  'entertainment',
  'social',
  'reference',
  'shopping',
  'health',
  'finance',
  'government',
  'other',
];

export function DomainManager({ profileId, onOpenUrl }: DomainManagerProps) {
  const [domains, setDomains] = useState<Domain[]>([]);
  const [lists, setLists] = useState<DomainList[]>([]);
  const [stats, setStats] = useState<DomainStats | null>(null);
  const [categories, setCategories] = useState<string[]>([]);
  const [searchQuery, setSearchQuery] = useState('');
  const [urlInput, setUrlInput] = useState('');
  // Active local-search query (rendered inline below the bar). nonce re-runs the
  // same query string on repeat Enter.
  const [liveQuery, setLiveQuery] = useState('');
  const [searchNonce, setSearchNonce] = useState(0);
  const [selectedCategory, setSelectedCategory] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Modal states
  const [showAddDomain, setShowAddDomain] = useState(false);
  const [showAddList, setShowAddList] = useState(false);
  const [showImport, setShowImport] = useState(false);
  const [editingDomain, setEditingDomain] = useState<Domain | null>(null);
  const [ratingDomain, setRatingDomain] = useState<Domain | null>(null);
  const [showRatings, setShowRatings] = useState<Domain | null>(null);

  // Form states
  const [newDomain, setNewDomain] = useState({ url: '', category: 'technology', trust_score: 0.5, metadata: '' });
  const [newList, setNewList] = useState({ name: '', description: '', author: '' });
  const [importData, setImportData] = useState('');

  // Load data
  const loadData = useCallback(async () => {
    if (!profileId) return;
    setIsLoading(true);
    setError(null);

    try {
      const [domainsData, listsData, statsData, categoriesData] = await Promise.all([
        invoke<Domain[]>('get_domains', { profileId }),
        invoke<DomainList[]>('get_domain_lists', { profileId }),
        invoke<DomainStats>('get_domain_stats', { profileId }),
        invoke<string[]>('get_domain_categories', { profileId }),
      ]);

      setDomains(domainsData);
      setLists(listsData);
      setStats(statsData);
      setCategories(categoriesData.length > 0 ? categoriesData : DEFAULT_CATEGORIES);
    } catch (err) {
      console.error('Failed to load domain data:', err);
      setError('Failed to load domains');
    } finally {
      setIsLoading(false);
    }
  }, [profileId]);

  useEffect(() => {
    loadData();
  }, [loadData]);

  // Search domains
  const handleSearch = async () => {
    if (!profileId || !searchQuery.trim()) {
      loadData();
      return;
    }

    setIsLoading(true);
    try {
      const results = await invoke<Domain[]>('search_domain_list', { profileId, query: searchQuery });
      setDomains(results);
    } catch (err) {
      console.error('Search failed:', err);
      setError('Search failed');
    } finally {
      setIsLoading(false);
    }
  };

  // Add domain
  const handleAddDomain = async () => {
    if (!profileId || !newDomain.url.trim()) return;

    try {
      await invoke('add_domain_entry', {
        url: newDomain.url.trim(),
        category: newDomain.category,
        trustScore: newDomain.trust_score,
        profileId,
      });
      setShowAddDomain(false);
      setNewDomain({ url: '', category: 'technology', trust_score: 0.5, metadata: '' });
      loadData();
    } catch (err) {
      console.error('Failed to add domain:', err);
      setError('Failed to add domain');
    }
  };

  // Update domain
  const handleUpdateDomain = async () => {
    if (!editingDomain) return;

    try {
      await invoke('update_domain', { domain: editingDomain });
      setEditingDomain(null);
      loadData();
    } catch (err) {
      console.error('Failed to update domain:', err);
      setError('Failed to update domain');
    }
  };

  // Delete domain
  const handleDeleteDomain = async (domainId: number) => {
    if (!profileId) return;

    try {
      await invoke('delete_domain_entry', { domainId, profileId });
      loadData();
    } catch (err) {
      console.error('Failed to delete domain:', err);
      setError('Failed to delete domain');
    }
  };

  // Create list
  const handleCreateList = async () => {
    if (!profileId || !newList.name.trim()) return;

    try {
      await invoke('create_domain_list', {
        name: newList.name.trim(),
        description: newList.description || null,
        profileId,
      });
      setShowAddList(false);
      setNewList({ name: '', description: '', author: '' });
      loadData();
    } catch (err) {
      console.error('Failed to create list:', err);
      setError('Failed to create list');
    }
  };

  // Delete list
  const handleDeleteList = async (listId: number) => {
    if (!profileId) return;

    try {
      await invoke('delete_domain_list', { listId, profileId });
      loadData();
    } catch (err) {
      console.error('Failed to delete list:', err);
      setError('Failed to delete list');
    }
  };

  // Export domains
  const handleExport = async () => {
    if (!profileId) return;

    try {
      const json = await invoke<string>('export_domains', { profileId });
      const blob = new Blob([json], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = 'earthsearch-domains.json';
      a.click();
      URL.revokeObjectURL(url);
    } catch (err) {
      console.error('Export failed:', err);
      setError('Export failed');
    }
  };

  // Import domains
  const handleImport = async () => {
    if (!profileId || !importData.trim()) return;

    try {
      const count = await invoke<number>('import_domains', { profileId, jsonData: importData });
      setShowImport(false);
      setImportData('');
      setError(null);
      loadData();
      alert(`Successfully imported ${count} domains`);
    } catch (err) {
      console.error('Import failed:', err);
      setError('Import failed - check JSON format');
    }
  };

  // Filter domains by category
  const filteredDomains = selectedCategory
    ? domains.filter(d => d.category === selectedCategory)
    : domains;

  // A URL-shaped input (bare host or scheme) jumps straight to a browser tab; a
  // free-text query runs the LOCAL search inline on this page (DuckDuckGo is still
  // offered as an option in the results header, and via openInDuckDuckGo).
  const isUrlShaped = (raw: string) =>
    /^https?:\/\//i.test(raw) || raw.startsWith('earth://') || (raw.includes('.') && !raw.includes(' '));

  const goToUrl = () => {
    const raw = urlInput.trim();
    if (!raw) return;
    if (isUrlShaped(raw)) {
      const target = /^https?:\/\//i.test(raw) || raw.startsWith('earth://') ? raw : `https://${raw}`;
      onOpenUrl?.(target, { fromAddressBar: true });
      setUrlInput('');
      return;
    }
    // Free-text query → local search inline (bump nonce to re-run if unchanged).
    setLiveQuery(raw);
    setSearchNonce(n => n + 1);
  };

  // Escape hatch the user asked to keep: open the query on DuckDuckGo in a tab.
  const openInDuckDuckGo = (q: string) => {
    onOpenUrl?.(`https://duckduckgo.com/?q=${encodeURIComponent(q)}`, { fromAddressBar: true });
  };

  if (!profileId) {
    return (
      <div className="bg-theme-card/80 border border-white/10 rounded-2xl p-8 backdrop-blur-sm">
        <p className="text-gray-400 text-center">Please select a profile to manage domains</p>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Stats Header */}
      {stats && (
        <div className="grid grid-cols-4 gap-4">
          <div className="bg-theme-card/60 border border-white/10 rounded-xl p-4">
            <div className="text-2xl font-bold text-theme-primary">{stats.total_domains}</div>
            <div className="text-sm text-gray-400">Total Domains</div>
          </div>
          <div className="bg-theme-card/60 border border-white/10 rounded-xl p-4">
            <div className="text-2xl font-bold text-theme-secondary">{stats.total_lists}</div>
            <div className="text-sm text-gray-400">Domain Lists</div>
          </div>
          <div className="bg-theme-card/60 border border-white/10 rounded-xl p-4">
            <div className="text-2xl font-bold text-theme-accent">{stats.categories.length}</div>
            <div className="text-sm text-gray-400">Categories</div>
          </div>
          <div className="bg-theme-card/60 border border-white/10 rounded-xl p-4">
            <div className="text-2xl font-bold text-green-400">{(stats.avg_trust_score * 100).toFixed(0)}%</div>
            <div className="text-sm text-gray-400">Avg Trust Score</div>
          </div>
        </div>
      )}

      {/* Go-to-URL bar: type a URL and jump straight to it in a browser tab. */}
      <div className="flex items-center gap-3">
        <div className="flex-1 relative">
          <input
            type="text"
            value={urlInput}
            onChange={(e) => setUrlInput(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && goToUrl()}
            placeholder="EarthSearch or enter a URL to visit"
            className="w-full bg-black/30 border border-white/10 rounded-lg px-4 py-2.5 pl-10 text-white placeholder-gray-500 focus:outline-none focus:border-theme-primary transition-colors"
          />
          <svg className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-gray-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13.828 10.172a4 4 0 010 5.656l-3 3a4 4 0 01-5.656-5.656l1.5-1.5m6.656-1.328a4 4 0 010-5.656l3-3a4 4 0 015.656 5.656l-1.5 1.5" />
          </svg>
        </div>
        <button
          onClick={goToUrl}
          className="px-6 py-2.5 bg-theme-primary text-white rounded-lg hover:bg-theme-primary/80 transition-colors font-medium flex items-center gap-2"
        >
          Go
          <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M14 5l7 7m0 0l-7 7m7-7H3" />
          </svg>
        </button>
      </div>

      {/* Inline local search results (a clicked result navigates this page away). */}
      {liveQuery && (
        <LocalSearchResults
          profileId={profileId}
          query={liveQuery}
          searchNonce={searchNonce}
          onOpenUrl={onOpenUrl}
          onClear={() => setLiveQuery('')}
          onOpenInDuckDuckGo={openInDuckDuckGo}
        />
      )}

      {/* Error Banner */}
      {error && (
        <div className="bg-red-500/20 border border-red-500/30 rounded-lg p-3 text-red-400 text-sm">
          {error}
          <button onClick={() => setError(null)} className="ml-2 text-red-300 hover:text-white">×</button>
        </div>
      )}

      {/* Main Content */}
      <div className="bg-theme-card/80 border border-white/10 rounded-2xl p-6 backdrop-blur-sm">
        {/* Search and Actions Bar */}
        <div className="flex items-center gap-4 mb-6">
          <div className="flex-1 relative">
            <input
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              onKeyDown={(e) => e.key === 'Enter' && handleSearch()}
              placeholder="Search domains..."
              className="w-full bg-black/30 border border-white/10 rounded-lg px-4 py-2 pl-10 text-white placeholder-gray-500 focus:outline-none focus:border-theme-primary transition-colors"
            />
            <svg className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-gray-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
            </svg>
          </div>

          {/* Category Filter */}
          <select
            value={selectedCategory || ''}
            onChange={(e) => setSelectedCategory(e.target.value || null)}
            className="bg-black/30 border border-white/10 rounded-lg px-3 py-2 text-white focus:outline-none focus:border-theme-primary"
          >
            <option value="">All Categories</option>
            {(categories.length > 0 ? categories : DEFAULT_CATEGORIES).map(cat => (
              <option key={cat} value={cat}>{cat}</option>
            ))}
          </select>

          {/* Action Buttons */}
          <button
            onClick={() => setShowAddDomain(true)}
            className="px-4 py-2 bg-theme-primary text-white rounded-lg hover:bg-theme-primary/80 transition-colors flex items-center gap-2"
          >
            <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
            </svg>
            Add Domain
          </button>

          <button
            onClick={() => setShowImport(true)}
            className="px-4 py-2 bg-white/10 text-white rounded-lg hover:bg-white/20 transition-colors"
          >
            Import
          </button>

          <button
            onClick={handleExport}
            className="px-4 py-2 bg-white/10 text-white rounded-lg hover:bg-white/20 transition-colors"
          >
            Export
          </button>
        </div>

        {/* Domain List */}
        {isLoading ? (
          <div className="flex items-center justify-center py-12">
            <div className="w-8 h-8 border-2 border-theme-primary border-t-transparent rounded-full animate-spin" />
          </div>
        ) : filteredDomains.length === 0 ? (
          <div className="text-center py-12">
            <div className="text-gray-400 mb-4">No domains found</div>
            <div className="flex flex-col items-center gap-3">
              <button
                onClick={async () => {
                  if (!profileId) return;
                  setIsLoading(true);
                  try {
                    const count = await invoke<number>('force_reseed_domains', { profileId });
                    console.log(`Force reseeded ${count} domains`);
                    loadData();
                  } catch (err) {
                    console.error('Failed to reseed domains:', err);
                    setError('Failed to load default domains');
                  } finally {
                    setIsLoading(false);
                  }
                }}
                className="px-4 py-2 bg-theme-primary text-white rounded-lg hover:bg-theme-primary/80 transition-colors"
              >
                Load Default Domains
              </button>
              <button
                onClick={() => setShowAddDomain(true)}
                className="text-theme-primary hover:underline"
              >
                Or add your first domain manually
              </button>
            </div>
          </div>
        ) : (
          <div className="space-y-2">
            {filteredDomains.map((domain) => (
              <div
                key={domain.id}
                className="flex items-center gap-4 p-3 bg-black/20 rounded-lg border border-white/5 hover:border-white/10 transition-colors group"
              >
                {/* Trust Score Indicator */}
                <div
                  className="w-2 h-8 rounded-full"
                  style={{
                    backgroundColor: domain.trust_score >= 0.7
                      ? '#22c55e'
                      : domain.trust_score >= 0.4
                        ? '#eab308'
                        : '#ef4444'
                  }}
                  title={`Trust: ${(domain.trust_score * 100).toFixed(0)}%`}
                />

                {/* Domain Info */}
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2">
                    <button
                      onClick={() => onOpenUrl?.(domain.url)}
                      className="text-white font-medium truncate hover:text-[var(--primary-color)] hover:underline transition-colors text-left"
                      title={`Open ${domain.url} in browser`}
                    >
                      {domain.url}
                    </button>
                    {onOpenUrl && (
                      <svg className="w-4 h-4 text-gray-500 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
                      </svg>
                    )}
                  </div>
                  <div className="flex items-center gap-2 text-sm">
                    <span className="text-theme-primary">{domain.category}</span>
                    <span className="text-gray-500">•</span>
                    <span className="text-gray-400">{(domain.trust_score * 100).toFixed(0)}% trust</span>
                    {domain.metadata && (
                      <>
                        <span className="text-gray-500">•</span>
                        <span className="text-gray-500 truncate">{domain.metadata}</span>
                      </>
                    )}
                  </div>
                </div>

                {/* Community Rating Badge */}
                {domain.id && (
                  <div onClick={() => setShowRatings(domain)} className="cursor-pointer">
                    <RatingBadge domainId={domain.id} size="sm" />
                  </div>
                )}

                {/* Actions */}
                <div className="flex items-center gap-2 opacity-0 group-hover:opacity-100 transition-opacity">
                  <button
                    onClick={() => setRatingDomain(domain)}
                    className="p-2 text-gray-400 hover:text-yellow-400 transition-colors"
                    title="Rate"
                  >
                    <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M11.049 2.927c.3-.921 1.603-.921 1.902 0l1.519 4.674a1 1 0 00.95.69h4.915c.969 0 1.371 1.24.588 1.81l-3.976 2.888a1 1 0 00-.363 1.118l1.518 4.674c.3.922-.755 1.688-1.538 1.118l-3.976-2.888a1 1 0 00-1.176 0l-3.976 2.888c-.783.57-1.838-.197-1.538-1.118l1.518-4.674a1 1 0 00-.363-1.118l-3.976-2.888c-.784-.57-.38-1.81.588-1.81h4.914a1 1 0 00.951-.69l1.519-4.674z" />
                    </svg>
                  </button>
                  <button
                    onClick={() => setEditingDomain(domain)}
                    className="p-2 text-gray-400 hover:text-white transition-colors"
                    title="Edit"
                  >
                    <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
                    </svg>
                  </button>
                  <button
                    onClick={() => domain.id && handleDeleteDomain(domain.id)}
                    className="p-2 text-gray-400 hover:text-red-400 transition-colors"
                    title="Delete"
                  >
                    <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                    </svg>
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Domain Lists Section */}
      <div className="bg-theme-card/80 border border-white/10 rounded-2xl p-6 backdrop-blur-sm">
        <div className="flex items-center justify-between mb-4">
          <h3 className="text-lg font-semibold text-white">Domain Lists</h3>
          <button
            onClick={() => setShowAddList(true)}
            className="px-3 py-1.5 bg-theme-secondary text-white text-sm rounded-lg hover:bg-theme-secondary/80 transition-colors flex items-center gap-1"
          >
            <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
            </svg>
            Create List
          </button>
        </div>

        {lists.length === 0 ? (
          <p className="text-gray-400 text-sm">No domain lists created yet</p>
        ) : (
          <div className="grid grid-cols-2 gap-3">
            {lists.map((list) => (
              <div
                key={list.id}
                className="p-4 bg-black/20 rounded-lg border border-white/5 hover:border-white/10 transition-colors"
              >
                <div className="flex items-start justify-between">
                  <div>
                    <div className="font-medium text-white">{list.name}</div>
                    {list.description && (
                      <div className="text-sm text-gray-400 mt-1">{list.description}</div>
                    )}
                    <div className="flex items-center gap-2 mt-2 text-xs text-gray-500">
                      <span>{list.domain_count ?? 0} domains</span>
                      {list.author && (
                        <>
                          <span>•</span>
                          <span>by {list.author}</span>
                        </>
                      )}
                      <span>•</span>
                      <span>v{list.version}</span>
                    </div>
                  </div>
                  <button
                    onClick={() => list.id && handleDeleteList(list.id)}
                    className="p-1 text-gray-400 hover:text-red-400 transition-colors"
                  >
                    <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                    </svg>
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Add Domain Modal */}
      {showAddDomain && (
        <div className="fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex items-center justify-center p-4">
          <div className="bg-gray-900 border border-white/10 rounded-2xl shadow-2xl w-full max-w-md p-6">
            <h3 className="text-xl font-semibold text-white mb-4">Add Domain</h3>

            <div className="space-y-4">
              <div>
                <label className="block text-sm text-gray-400 mb-1">URL</label>
                <input
                  type="text"
                  value={newDomain.url}
                  onChange={(e) => setNewDomain({ ...newDomain, url: e.target.value })}
                  placeholder="example.com"
                  className="w-full bg-black/30 border border-white/10 rounded-lg px-3 py-2 text-white placeholder-gray-500 focus:outline-none focus:border-theme-primary"
                />
              </div>

              <div>
                <label className="block text-sm text-gray-400 mb-1">Category</label>
                <select
                  value={newDomain.category}
                  onChange={(e) => setNewDomain({ ...newDomain, category: e.target.value })}
                  className="w-full bg-black/30 border border-white/10 rounded-lg px-3 py-2 text-white focus:outline-none focus:border-theme-primary"
                >
                  {DEFAULT_CATEGORIES.map(cat => (
                    <option key={cat} value={cat}>{cat}</option>
                  ))}
                </select>
              </div>

              <div>
                <label className="block text-sm text-gray-400 mb-1">Trust Score: {(newDomain.trust_score * 100).toFixed(0)}%</label>
                <input
                  type="range"
                  min="0"
                  max="1"
                  step="0.1"
                  value={newDomain.trust_score}
                  onChange={(e) => setNewDomain({ ...newDomain, trust_score: parseFloat(e.target.value) })}
                  className="w-full accent-theme-primary"
                />
              </div>

              <div>
                <label className="block text-sm text-gray-400 mb-1">Notes (optional)</label>
                <input
                  type="text"
                  value={newDomain.metadata}
                  onChange={(e) => setNewDomain({ ...newDomain, metadata: e.target.value })}
                  placeholder="Any notes about this domain..."
                  className="w-full bg-black/30 border border-white/10 rounded-lg px-3 py-2 text-white placeholder-gray-500 focus:outline-none focus:border-theme-primary"
                />
              </div>
            </div>

            <div className="flex justify-end gap-3 mt-6">
              <button
                onClick={() => setShowAddDomain(false)}
                className="px-4 py-2 text-gray-400 hover:text-white transition-colors"
              >
                Cancel
              </button>
              <button
                onClick={handleAddDomain}
                className="px-4 py-2 bg-theme-primary text-white rounded-lg hover:bg-theme-primary/80 transition-colors"
              >
                Add Domain
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Edit Domain Modal */}
      {editingDomain && (
        <div className="fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex items-center justify-center p-4">
          <div className="bg-gray-900 border border-white/10 rounded-2xl shadow-2xl w-full max-w-md p-6">
            <h3 className="text-xl font-semibold text-white mb-4">Edit Domain</h3>

            <div className="space-y-4">
              <div>
                <label className="block text-sm text-gray-400 mb-1">URL</label>
                <input
                  type="text"
                  value={editingDomain.url}
                  onChange={(e) => setEditingDomain({ ...editingDomain, url: e.target.value })}
                  className="w-full bg-black/30 border border-white/10 rounded-lg px-3 py-2 text-white placeholder-gray-500 focus:outline-none focus:border-theme-primary"
                />
              </div>

              <div>
                <label className="block text-sm text-gray-400 mb-1">Category</label>
                <select
                  value={editingDomain.category}
                  onChange={(e) => setEditingDomain({ ...editingDomain, category: e.target.value })}
                  className="w-full bg-black/30 border border-white/10 rounded-lg px-3 py-2 text-white focus:outline-none focus:border-theme-primary"
                >
                  {DEFAULT_CATEGORIES.map(cat => (
                    <option key={cat} value={cat}>{cat}</option>
                  ))}
                </select>
              </div>

              <div>
                <label className="block text-sm text-gray-400 mb-1">Trust Score: {(editingDomain.trust_score * 100).toFixed(0)}%</label>
                <input
                  type="range"
                  min="0"
                  max="1"
                  step="0.1"
                  value={editingDomain.trust_score}
                  onChange={(e) => setEditingDomain({ ...editingDomain, trust_score: parseFloat(e.target.value) })}
                  className="w-full accent-theme-primary"
                />
              </div>

              <div>
                <label className="block text-sm text-gray-400 mb-1">Notes</label>
                <input
                  type="text"
                  value={editingDomain.metadata || ''}
                  onChange={(e) => setEditingDomain({ ...editingDomain, metadata: e.target.value || null })}
                  className="w-full bg-black/30 border border-white/10 rounded-lg px-3 py-2 text-white placeholder-gray-500 focus:outline-none focus:border-theme-primary"
                />
              </div>
            </div>

            <div className="flex justify-end gap-3 mt-6">
              <button
                onClick={() => setEditingDomain(null)}
                className="px-4 py-2 text-gray-400 hover:text-white transition-colors"
              >
                Cancel
              </button>
              <button
                onClick={handleUpdateDomain}
                className="px-4 py-2 bg-theme-primary text-white rounded-lg hover:bg-theme-primary/80 transition-colors"
              >
                Save Changes
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Add List Modal */}
      {showAddList && (
        <div className="fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex items-center justify-center p-4">
          <div className="bg-gray-900 border border-white/10 rounded-2xl shadow-2xl w-full max-w-md p-6">
            <h3 className="text-xl font-semibold text-white mb-4">Create Domain List</h3>

            <div className="space-y-4">
              <div>
                <label className="block text-sm text-gray-400 mb-1">Name</label>
                <input
                  type="text"
                  value={newList.name}
                  onChange={(e) => setNewList({ ...newList, name: e.target.value })}
                  placeholder="My Trusted Sites"
                  className="w-full bg-black/30 border border-white/10 rounded-lg px-3 py-2 text-white placeholder-gray-500 focus:outline-none focus:border-theme-primary"
                />
              </div>

              <div>
                <label className="block text-sm text-gray-400 mb-1">Description (optional)</label>
                <input
                  type="text"
                  value={newList.description}
                  onChange={(e) => setNewList({ ...newList, description: e.target.value })}
                  placeholder="A collection of..."
                  className="w-full bg-black/30 border border-white/10 rounded-lg px-3 py-2 text-white placeholder-gray-500 focus:outline-none focus:border-theme-primary"
                />
              </div>

              <div>
                <label className="block text-sm text-gray-400 mb-1">Author (optional)</label>
                <input
                  type="text"
                  value={newList.author}
                  onChange={(e) => setNewList({ ...newList, author: e.target.value })}
                  placeholder="Your name"
                  className="w-full bg-black/30 border border-white/10 rounded-lg px-3 py-2 text-white placeholder-gray-500 focus:outline-none focus:border-theme-primary"
                />
              </div>
            </div>

            <div className="flex justify-end gap-3 mt-6">
              <button
                onClick={() => setShowAddList(false)}
                className="px-4 py-2 text-gray-400 hover:text-white transition-colors"
              >
                Cancel
              </button>
              <button
                onClick={handleCreateList}
                className="px-4 py-2 bg-theme-secondary text-white rounded-lg hover:bg-theme-secondary/80 transition-colors"
              >
                Create List
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Import Modal */}
      {showImport && (
        <div className="fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex items-center justify-center p-4">
          <div className="bg-gray-900 border border-white/10 rounded-2xl shadow-2xl w-full max-w-lg p-6">
            <h3 className="text-xl font-semibold text-white mb-4">Import Domains</h3>

            <div>
              <label className="block text-sm text-gray-400 mb-1">Paste JSON data</label>
              <textarea
                value={importData}
                onChange={(e) => setImportData(e.target.value)}
                placeholder='{"domains": [{"url": "example.com", "category": "technology", "trust_score": 0.8}]}'
                rows={8}
                className="w-full bg-black/30 border border-white/10 rounded-lg px-3 py-2 text-white placeholder-gray-500 focus:outline-none focus:border-theme-primary font-mono text-sm"
              />
            </div>

            <div className="flex justify-end gap-3 mt-6">
              <button
                onClick={() => setShowImport(false)}
                className="px-4 py-2 text-gray-400 hover:text-white transition-colors"
              >
                Cancel
              </button>
              <button
                onClick={handleImport}
                className="px-4 py-2 bg-theme-primary text-white rounded-lg hover:bg-theme-primary/80 transition-colors"
              >
                Import
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Rating Form Modal */}
      {ratingDomain && ratingDomain.id && (
        <div className="fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex items-center justify-center p-4">
          <div className="relative">
            <button
              onClick={() => setRatingDomain(null)}
              className="absolute -top-2 -right-2 w-8 h-8 bg-gray-800 rounded-full flex items-center justify-center text-gray-400 hover:text-white z-10"
            >
              ×
            </button>
            <RatingForm
              domainId={ratingDomain.id}
              domainUrl={ratingDomain.url}
              onSubmit={() => setRatingDomain(null)}
              onCancel={() => setRatingDomain(null)}
            />
          </div>
        </div>
      )}

      {/* Rating Display Modal */}
      {showRatings && showRatings.id && (
        <div className="fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex items-center justify-center p-4">
          <div className="relative max-w-xl w-full">
            <button
              onClick={() => setShowRatings(null)}
              className="absolute -top-2 -right-2 w-8 h-8 bg-gray-800 rounded-full flex items-center justify-center text-gray-400 hover:text-white z-10"
            >
              ×
            </button>
            <RatingDisplay
              domainId={showRatings.id}
              domainUrl={showRatings.url}
              showReviews={true}
              maxReviews={10}
            />
            <div className="mt-4 flex justify-center">
              <button
                onClick={() => {
                  setShowRatings(null);
                  setRatingDomain(showRatings);
                }}
                className="px-4 py-2 bg-theme-primary text-white rounded-lg hover:bg-theme-primary/80 transition-colors"
              >
                Add Your Rating
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

export default DomainManager;
