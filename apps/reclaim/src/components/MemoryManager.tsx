// Memory Manager for EarthMemory
// Manages indexed pages, favorites, notes, and semantic search

import { useState, useEffect, useCallback } from 'react';
import { invoke } from '../lib/tauri';

interface IndexedPage {
  id: number | null;
  url: string;
  title: string;
  content: string | null;
  summary: string | null;
  indexed_at: string;
  last_visited: string;
  visit_count: number;
  is_favorite: boolean;
  tags: string | null;
  profile_id: number | null;
}

interface PageNote {
  id: number | null;
  page_id: number;
  content: string;
  created_at: string;
  updated_at: string;
  profile_id: number | null;
}

interface MemoryStats {
  total_pages: number;
  total_notes: number;
  favorites_count: number;
  total_visits: number;
  tags: { tag: string; count: number }[];
}

interface MemoryManagerProps {
  profileId: number | null;
}

export function MemoryManager({ profileId }: MemoryManagerProps) {
  const [pages, setPages] = useState<IndexedPage[]>([]);
  const [stats, setStats] = useState<MemoryStats | null>(null);
  const [tags, setTags] = useState<string[]>([]);
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedTag, setSelectedTag] = useState<string | null>(null);
  const [showFavorites, setShowFavorites] = useState(false);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Modal states
  const [selectedPage, setSelectedPage] = useState<IndexedPage | null>(null);
  const [showAddPage, setShowAddPage] = useState(false);
  const [showImport, setShowImport] = useState(false);

  // Form states
  const [newPage, setNewPage] = useState({ url: '', title: '', content: '', summary: '', tags: '' });
  const [importData, setImportData] = useState('');
  const [notes, setNotes] = useState<PageNote[]>([]);
  const [newNote, setNewNote] = useState('');
  const [editingTags, setEditingTags] = useState<string | null>(null);

  // Load data
  const loadData = useCallback(async () => {
    if (!profileId) return;
    setIsLoading(true);
    setError(null);

    try {
      const [pagesData, statsData, tagsData] = await Promise.all([
        showFavorites
          ? invoke<IndexedPage[]>('get_favorite_pages', { profileId })
          : invoke<IndexedPage[]>('get_indexed_pages', { profileId, limit: 100, offset: 0 }),
        invoke<MemoryStats>('get_memory_stats', { profileId }),
        invoke<string[]>('get_memory_tags', { profileId }),
      ]);

      setPages(pagesData);
      setStats(statsData);
      setTags(tagsData);
    } catch (err) {
      console.error('Failed to load memory data:', err);
      setError('Failed to load indexed pages');
    } finally {
      setIsLoading(false);
    }
  }, [profileId, showFavorites]);

  useEffect(() => {
    loadData();
  }, [loadData]);

  // Search pages
  const handleSearch = async () => {
    if (!profileId || !searchQuery.trim()) {
      loadData();
      return;
    }

    setIsLoading(true);
    try {
      const results = await invoke<IndexedPage[]>('search_memory', { profileId, query: searchQuery });
      setPages(results);
    } catch (err) {
      console.error('Search failed:', err);
      setError('Search failed');
    } finally {
      setIsLoading(false);
    }
  };

  // Index new page
  const handleAddPage = async () => {
    if (!profileId || !newPage.url.trim() || !newPage.title.trim()) return;

    try {
      const page: IndexedPage = {
        id: null,
        url: newPage.url.trim(),
        title: newPage.title.trim(),
        content: newPage.content || null,
        summary: newPage.summary || null,
        indexed_at: '',
        last_visited: '',
        visit_count: 1,
        is_favorite: false,
        tags: newPage.tags || null,
        profile_id: profileId,
      };

      await invoke('index_page', { page, profileId });
      setShowAddPage(false);
      setNewPage({ url: '', title: '', content: '', summary: '', tags: '' });
      loadData();
    } catch (err) {
      console.error('Failed to index page:', err);
      setError('Failed to index page');
    }
  };

  // Toggle favorite
  const handleToggleFavorite = async (pageId: number) => {
    if (!profileId) return;

    try {
      await invoke('toggle_page_favorite', { pageId, profileId });
      loadData();
    } catch (err) {
      console.error('Failed to toggle favorite:', err);
    }
  };

  // Update tags
  const handleUpdateTags = async (pageId: number, newTags: string) => {
    if (!profileId) return;

    try {
      await invoke('update_page_tags', { pageId, profileId, tags: newTags });
      setEditingTags(null);
      loadData();
    } catch (err) {
      console.error('Failed to update tags:', err);
    }
  };

  // Delete page
  const handleDeletePage = async (pageId: number) => {
    if (!profileId) return;

    try {
      await invoke('delete_indexed_page', { pageId, profileId });
      setSelectedPage(null);
      loadData();
    } catch (err) {
      console.error('Failed to delete page:', err);
    }
  };

  // Load notes for a page
  const loadNotes = async (pageId: number) => {
    try {
      const notesData = await invoke<PageNote[]>('get_page_notes', { pageId });
      setNotes(notesData);
    } catch (err) {
      console.error('Failed to load notes:', err);
    }
  };

  // Add note
  const handleAddNote = async (pageId: number) => {
    if (!profileId || !newNote.trim()) return;

    try {
      await invoke('add_page_note', { pageId, content: newNote, profileId });
      setNewNote('');
      loadNotes(pageId);
    } catch (err) {
      console.error('Failed to add note:', err);
    }
  };

  // Delete note
  const handleDeleteNote = async (noteId: number, pageId: number) => {
    if (!profileId) return;

    try {
      await invoke('delete_page_note', { noteId, profileId });
      loadNotes(pageId);
    } catch (err) {
      console.error('Failed to delete note:', err);
    }
  };

  // Export memory
  const handleExport = async () => {
    if (!profileId) return;

    try {
      const json = await invoke<string>('export_memory', { profileId });
      const blob = new Blob([json], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = 'earthmemory-export.json';
      a.click();
      URL.revokeObjectURL(url);
    } catch (err) {
      console.error('Export failed:', err);
    }
  };

  // Import memory
  const handleImport = async () => {
    if (!profileId || !importData.trim()) return;

    try {
      const count = await invoke<number>('import_memory', { profileId, jsonData: importData });
      setShowImport(false);
      setImportData('');
      loadData();
      alert(`Successfully imported ${count} pages`);
    } catch (err) {
      console.error('Import failed:', err);
      setError('Import failed - check JSON format');
    }
  };

  // Filter pages by tag
  const filteredPages = selectedTag
    ? pages.filter(p => p.tags?.toLowerCase().includes(selectedTag.toLowerCase()))
    : pages;

  // Open page detail
  const openPageDetail = (page: IndexedPage) => {
    setSelectedPage(page);
    if (page.id) loadNotes(page.id);
  };

  // Format timestamp
  const formatDate = (timestamp: string) => {
    if (!timestamp) return 'Unknown';
    const date = new Date(parseInt(timestamp) * 1000);
    return date.toLocaleDateString() + ' ' + date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  };

  if (!profileId) {
    return (
      <div className="bg-theme-card/80 border border-white/10 rounded-2xl p-8 backdrop-blur-sm">
        <p className="text-gray-400 text-center">Please select a profile to view memory</p>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Stats Header */}
      {stats && (
        <div className="grid grid-cols-4 gap-4">
          <div className="bg-theme-card/60 border border-white/10 rounded-xl p-4">
            <div className="text-2xl font-bold text-theme-secondary">{stats.total_pages}</div>
            <div className="text-sm text-gray-400">Indexed Pages</div>
          </div>
          <div className="bg-theme-card/60 border border-white/10 rounded-xl p-4">
            <div className="text-2xl font-bold text-theme-primary">{stats.favorites_count}</div>
            <div className="text-sm text-gray-400">Favorites</div>
          </div>
          <div className="bg-theme-card/60 border border-white/10 rounded-xl p-4">
            <div className="text-2xl font-bold text-theme-accent">{stats.total_notes}</div>
            <div className="text-sm text-gray-400">Notes</div>
          </div>
          <div className="bg-theme-card/60 border border-white/10 rounded-xl p-4">
            <div className="text-2xl font-bold text-yellow-400">{stats.total_visits}</div>
            <div className="text-sm text-gray-400">Total Visits</div>
          </div>
        </div>
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
              placeholder="Search your memory..."
              className="w-full bg-black/30 border border-white/10 rounded-lg px-4 py-2 pl-10 text-white placeholder-gray-500 focus:outline-none focus:border-theme-secondary transition-colors"
            />
            <svg className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-gray-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
            </svg>
          </div>

          {/* Tag Filter */}
          {tags.length > 0 && (
            <select
              value={selectedTag || ''}
              onChange={(e) => setSelectedTag(e.target.value || null)}
              className="bg-black/30 border border-white/10 rounded-lg px-3 py-2 text-white focus:outline-none focus:border-theme-secondary"
            >
              <option value="">All Tags</option>
              {tags.map(tag => (
                <option key={tag} value={tag}>{tag}</option>
              ))}
            </select>
          )}

          {/* Favorites Toggle */}
          <button
            onClick={() => setShowFavorites(!showFavorites)}
            className={`px-4 py-2 rounded-lg transition-colors flex items-center gap-2 ${
              showFavorites
                ? 'bg-yellow-500/20 text-yellow-400 border border-yellow-500/30'
                : 'bg-white/10 text-gray-400 hover:text-white'
            }`}
          >
            <svg className="w-4 h-4" fill={showFavorites ? 'currentColor' : 'none'} viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M11.049 2.927c.3-.921 1.603-.921 1.902 0l1.519 4.674a1 1 0 00.95.69h4.915c.969 0 1.371 1.24.588 1.81l-3.976 2.888a1 1 0 00-.363 1.118l1.518 4.674c.3.922-.755 1.688-1.538 1.118l-3.976-2.888a1 1 0 00-1.176 0l-3.976 2.888c-.783.57-1.838-.197-1.538-1.118l1.518-4.674a1 1 0 00-.363-1.118l-3.976-2.888c-.784-.57-.38-1.81.588-1.81h4.914a1 1 0 00.951-.69l1.519-4.674z" />
            </svg>
            Favorites
          </button>

          {/* Action Buttons */}
          <button
            onClick={() => setShowAddPage(true)}
            className="px-4 py-2 bg-theme-secondary text-white rounded-lg hover:bg-theme-secondary/80 transition-colors flex items-center gap-2"
          >
            <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
            </svg>
            Index Page
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

        {/* Tags Bar */}
        {stats && stats.tags.length > 0 && (
          <div className="flex flex-wrap gap-2 mb-4 pb-4 border-b border-white/5">
            {stats.tags.slice(0, 10).map(({ tag, count }) => (
              <button
                key={tag}
                onClick={() => setSelectedTag(selectedTag === tag ? null : tag)}
                className={`px-3 py-1 rounded-full text-sm transition-colors ${
                  selectedTag === tag
                    ? 'bg-theme-secondary text-white'
                    : 'bg-white/10 text-gray-400 hover:text-white'
                }`}
              >
                {tag} <span className="opacity-50">({count})</span>
              </button>
            ))}
          </div>
        )}

        {/* Pages List */}
        {isLoading ? (
          <div className="flex items-center justify-center py-12">
            <div className="w-8 h-8 border-2 border-theme-secondary border-t-transparent rounded-full animate-spin" />
          </div>
        ) : filteredPages.length === 0 ? (
          <div className="text-center py-12">
            <div className="text-gray-400 mb-4">
              {showFavorites ? 'No favorites yet' : 'No indexed pages found'}
            </div>
            <button
              onClick={() => setShowAddPage(true)}
              className="text-theme-secondary hover:underline"
            >
              Index your first page
            </button>
          </div>
        ) : (
          <div className="space-y-2">
            {filteredPages.map((page) => (
              <div
                key={page.id}
                onClick={() => openPageDetail(page)}
                className="flex items-center gap-4 p-4 bg-black/20 rounded-lg border border-white/5 hover:border-white/10 transition-colors cursor-pointer group"
              >
                {/* Favicon placeholder */}
                <div className="w-10 h-10 rounded-lg bg-theme-secondary/20 flex items-center justify-center flex-shrink-0">
                  <span className="text-theme-secondary text-lg font-bold">
                    {page.title.charAt(0).toUpperCase()}
                  </span>
                </div>

                {/* Page Info */}
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="text-white font-medium truncate">{page.title}</span>
                    {page.is_favorite && (
                      <svg className="w-4 h-4 text-yellow-400 flex-shrink-0" fill="currentColor" viewBox="0 0 24 24">
                        <path d="M11.049 2.927c.3-.921 1.603-.921 1.902 0l1.519 4.674a1 1 0 00.95.69h4.915c.969 0 1.371 1.24.588 1.81l-3.976 2.888a1 1 0 00-.363 1.118l1.518 4.674c.3.922-.755 1.688-1.538 1.118l-3.976-2.888a1 1 0 00-1.176 0l-3.976 2.888c-.783.57-1.838-.197-1.538-1.118l1.518-4.674a1 1 0 00-.363-1.118l-3.976-2.888c-.784-.57-.38-1.81.588-1.81h4.914a1 1 0 00.951-.69l1.519-4.674z" />
                      </svg>
                    )}
                  </div>
                  <div className="text-sm text-gray-400 truncate">{page.url}</div>
                  <div className="flex items-center gap-3 mt-1 text-xs text-gray-500">
                    <span>{page.visit_count} visits</span>
                    <span>•</span>
                    <span>Last: {formatDate(page.last_visited)}</span>
                    {page.tags && (
                      <>
                        <span>•</span>
                        <span className="text-theme-secondary">{page.tags}</span>
                      </>
                    )}
                  </div>
                </div>

                {/* Quick Actions */}
                <div className="flex items-center gap-2 opacity-0 group-hover:opacity-100 transition-opacity" onClick={(e) => e.stopPropagation()}>
                  <button
                    onClick={() => page.id && handleToggleFavorite(page.id)}
                    className={`p-2 transition-colors ${
                      page.is_favorite ? 'text-yellow-400' : 'text-gray-400 hover:text-yellow-400'
                    }`}
                    title={page.is_favorite ? 'Remove from favorites' : 'Add to favorites'}
                  >
                    <svg className="w-4 h-4" fill={page.is_favorite ? 'currentColor' : 'none'} viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M11.049 2.927c.3-.921 1.603-.921 1.902 0l1.519 4.674a1 1 0 00.95.69h4.915c.969 0 1.371 1.24.588 1.81l-3.976 2.888a1 1 0 00-.363 1.118l1.518 4.674c.3.922-.755 1.688-1.538 1.118l-3.976-2.888a1 1 0 00-1.176 0l-3.976 2.888c-.783.57-1.838-.197-1.538-1.118l1.518-4.674a1 1 0 00-.363-1.118l-3.976-2.888c-.784-.57-.38-1.81.588-1.81h4.914a1 1 0 00.951-.69l1.519-4.674z" />
                    </svg>
                  </button>
                  <button
                    onClick={() => page.id && handleDeletePage(page.id)}
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

      {/* Add Page Modal */}
      {showAddPage && (
        <div className="fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex items-center justify-center p-4">
          <div className="bg-gray-900 border border-white/10 rounded-2xl shadow-2xl w-full max-w-lg p-6">
            <h3 className="text-xl font-semibold text-white mb-4">Index Page</h3>

            <div className="space-y-4">
              <div>
                <label className="block text-sm text-gray-400 mb-1">URL *</label>
                <input
                  type="text"
                  value={newPage.url}
                  onChange={(e) => setNewPage({ ...newPage, url: e.target.value })}
                  placeholder="https://example.com/article"
                  className="w-full bg-black/30 border border-white/10 rounded-lg px-3 py-2 text-white placeholder-gray-500 focus:outline-none focus:border-theme-secondary"
                />
              </div>

              <div>
                <label className="block text-sm text-gray-400 mb-1">Title *</label>
                <input
                  type="text"
                  value={newPage.title}
                  onChange={(e) => setNewPage({ ...newPage, title: e.target.value })}
                  placeholder="Page title"
                  className="w-full bg-black/30 border border-white/10 rounded-lg px-3 py-2 text-white placeholder-gray-500 focus:outline-none focus:border-theme-secondary"
                />
              </div>

              <div>
                <label className="block text-sm text-gray-400 mb-1">Summary (optional)</label>
                <textarea
                  value={newPage.summary}
                  onChange={(e) => setNewPage({ ...newPage, summary: e.target.value })}
                  placeholder="Brief summary of the page..."
                  rows={2}
                  className="w-full bg-black/30 border border-white/10 rounded-lg px-3 py-2 text-white placeholder-gray-500 focus:outline-none focus:border-theme-secondary"
                />
              </div>

              <div>
                <label className="block text-sm text-gray-400 mb-1">Content (optional)</label>
                <textarea
                  value={newPage.content}
                  onChange={(e) => setNewPage({ ...newPage, content: e.target.value })}
                  placeholder="Full page content for search..."
                  rows={3}
                  className="w-full bg-black/30 border border-white/10 rounded-lg px-3 py-2 text-white placeholder-gray-500 focus:outline-none focus:border-theme-secondary"
                />
              </div>

              <div>
                <label className="block text-sm text-gray-400 mb-1">Tags (comma-separated)</label>
                <input
                  type="text"
                  value={newPage.tags}
                  onChange={(e) => setNewPage({ ...newPage, tags: e.target.value })}
                  placeholder="tech, tutorial, rust"
                  className="w-full bg-black/30 border border-white/10 rounded-lg px-3 py-2 text-white placeholder-gray-500 focus:outline-none focus:border-theme-secondary"
                />
              </div>
            </div>

            <div className="flex justify-end gap-3 mt-6">
              <button
                onClick={() => setShowAddPage(false)}
                className="px-4 py-2 text-gray-400 hover:text-white transition-colors"
              >
                Cancel
              </button>
              <button
                onClick={handleAddPage}
                className="px-4 py-2 bg-theme-secondary text-white rounded-lg hover:bg-theme-secondary/80 transition-colors"
              >
                Index Page
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Page Detail Modal */}
      {selectedPage && (
        <div className="fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex items-center justify-center p-4">
          <div className="bg-gray-900 border border-white/10 rounded-2xl shadow-2xl w-full max-w-2xl max-h-[80vh] overflow-hidden flex flex-col">
            <div className="p-6 border-b border-white/10">
              <div className="flex items-start justify-between">
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2">
                    <h3 className="text-xl font-semibold text-white truncate">{selectedPage.title}</h3>
                    <button
                      onClick={() => selectedPage.id && handleToggleFavorite(selectedPage.id)}
                      className={selectedPage.is_favorite ? 'text-yellow-400' : 'text-gray-400 hover:text-yellow-400'}
                    >
                      <svg className="w-5 h-5" fill={selectedPage.is_favorite ? 'currentColor' : 'none'} viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M11.049 2.927c.3-.921 1.603-.921 1.902 0l1.519 4.674a1 1 0 00.95.69h4.915c.969 0 1.371 1.24.588 1.81l-3.976 2.888a1 1 0 00-.363 1.118l1.518 4.674c.3.922-.755 1.688-1.538 1.118l-3.976-2.888a1 1 0 00-1.176 0l-3.976 2.888c-.783.57-1.838-.197-1.538-1.118l1.518-4.674a1 1 0 00-.363-1.118l-3.976-2.888c-.784-.57-.38-1.81.588-1.81h4.914a1 1 0 00.951-.69l1.519-4.674z" />
                      </svg>
                    </button>
                  </div>
                  <a href={selectedPage.url} target="_blank" rel="noopener noreferrer" className="text-sm text-theme-secondary hover:underline truncate block">
                    {selectedPage.url}
                  </a>
                </div>
                <button
                  onClick={() => setSelectedPage(null)}
                  className="p-2 text-gray-400 hover:text-white transition-colors"
                >
                  <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                  </svg>
                </button>
              </div>

              {/* Tags */}
              <div className="mt-3">
                {editingTags !== null ? (
                  <div className="flex items-center gap-2">
                    <input
                      type="text"
                      value={editingTags}
                      onChange={(e) => setEditingTags(e.target.value)}
                      className="flex-1 bg-black/30 border border-white/10 rounded px-2 py-1 text-sm text-white"
                      placeholder="tag1, tag2, tag3"
                    />
                    <button
                      onClick={() => selectedPage.id && handleUpdateTags(selectedPage.id, editingTags)}
                      className="text-theme-secondary text-sm"
                    >
                      Save
                    </button>
                    <button
                      onClick={() => setEditingTags(null)}
                      className="text-gray-400 text-sm"
                    >
                      Cancel
                    </button>
                  </div>
                ) : (
                  <div className="flex items-center gap-2">
                    <span className="text-sm text-gray-400">Tags:</span>
                    <span className="text-sm text-theme-secondary">{selectedPage.tags || 'None'}</span>
                    <button
                      onClick={() => setEditingTags(selectedPage.tags || '')}
                      className="text-xs text-gray-500 hover:text-white"
                    >
                      Edit
                    </button>
                  </div>
                )}
              </div>

              {/* Stats */}
              <div className="flex items-center gap-4 mt-2 text-xs text-gray-500">
                <span>{selectedPage.visit_count} visits</span>
                <span>Indexed: {formatDate(selectedPage.indexed_at)}</span>
                <span>Last visited: {formatDate(selectedPage.last_visited)}</span>
              </div>
            </div>

            {/* Content */}
            <div className="flex-1 overflow-y-auto p-6">
              {selectedPage.summary && (
                <div className="mb-4">
                  <h4 className="text-sm font-medium text-gray-400 mb-1">Summary</h4>
                  <p className="text-gray-300">{selectedPage.summary}</p>
                </div>
              )}

              {selectedPage.content && (
                <div className="mb-4">
                  <h4 className="text-sm font-medium text-gray-400 mb-1">Content</h4>
                  <p className="text-gray-300 text-sm whitespace-pre-wrap">{selectedPage.content}</p>
                </div>
              )}

              {/* Notes Section */}
              <div className="mt-6 pt-4 border-t border-white/10">
                <h4 className="text-sm font-medium text-gray-400 mb-3">Notes ({notes.length})</h4>

                <div className="space-y-3 mb-4">
                  {notes.map((note) => (
                    <div key={note.id} className="p-3 bg-black/20 rounded-lg">
                      <p className="text-gray-300 text-sm">{note.content}</p>
                      <div className="flex items-center justify-between mt-2">
                        <span className="text-xs text-gray-500">{formatDate(note.created_at)}</span>
                        <button
                          onClick={() => note.id && selectedPage.id && handleDeleteNote(note.id, selectedPage.id)}
                          className="text-xs text-gray-500 hover:text-red-400"
                        >
                          Delete
                        </button>
                      </div>
                    </div>
                  ))}
                </div>

                <div className="flex gap-2">
                  <input
                    type="text"
                    value={newNote}
                    onChange={(e) => setNewNote(e.target.value)}
                    placeholder="Add a note..."
                    className="flex-1 bg-black/30 border border-white/10 rounded-lg px-3 py-2 text-white text-sm placeholder-gray-500 focus:outline-none focus:border-theme-secondary"
                    onKeyDown={(e) => e.key === 'Enter' && selectedPage.id && handleAddNote(selectedPage.id)}
                  />
                  <button
                    onClick={() => selectedPage.id && handleAddNote(selectedPage.id)}
                    className="px-4 py-2 bg-theme-secondary text-white rounded-lg text-sm hover:bg-theme-secondary/80 transition-colors"
                  >
                    Add
                  </button>
                </div>
              </div>
            </div>

            {/* Footer */}
            <div className="p-4 border-t border-white/10 flex justify-between">
              <button
                onClick={() => selectedPage.id && handleDeletePage(selectedPage.id)}
                className="px-4 py-2 text-red-400 hover:text-red-300 transition-colors"
              >
                Delete Page
              </button>
              <button
                onClick={() => setSelectedPage(null)}
                className="px-4 py-2 bg-white/10 text-white rounded-lg hover:bg-white/20 transition-colors"
              >
                Close
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Import Modal */}
      {showImport && (
        <div className="fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex items-center justify-center p-4">
          <div className="bg-gray-900 border border-white/10 rounded-2xl shadow-2xl w-full max-w-lg p-6">
            <h3 className="text-xl font-semibold text-white mb-4">Import Memory</h3>

            <div>
              <label className="block text-sm text-gray-400 mb-1">Paste JSON data</label>
              <textarea
                value={importData}
                onChange={(e) => setImportData(e.target.value)}
                placeholder='{"pages": [{"url": "https://example.com", "title": "Example", "tags": "tech"}]}'
                rows={8}
                className="w-full bg-black/30 border border-white/10 rounded-lg px-3 py-2 text-white placeholder-gray-500 focus:outline-none focus:border-theme-secondary font-mono text-sm"
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
                className="px-4 py-2 bg-theme-secondary text-white rounded-lg hover:bg-theme-secondary/80 transition-colors"
              >
                Import
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

export default MemoryManager;
