import { useState, useEffect, useCallback } from 'react';
import { invoke } from '../lib/tauri';

// Types
interface ContentSelector {
  name: string;
  selector: string;
}

interface ScrapingJob {
  id: number | null;
  profile_id: number;
  name: string;
  base_url: string;
  url_pattern: string | null;
  max_depth: number;
  max_pages: number;
  content_selectors: ContentSelector[];
  schedule_cron: string | null;
  status: string;
  last_run_at: string | null;
  pages_scraped: number;
  created_at: string;
}

interface ScrapedPage {
  id: number | null;
  job_id: number;
  url: string;
  title: string | null;
  content: string;
  metadata: string | null;
  scraped_at: string;
}

interface WebScraperProps {
  profileId: number | null;
}

export function WebScraper({ profileId }: WebScraperProps) {
  const [jobs, setJobs] = useState<ScrapingJob[]>([]);
  const [loading, setLoading] = useState(true);
  const [showCreateModal, setShowCreateModal] = useState(false);
  const [selectedJob, setSelectedJob] = useState<ScrapingJob | null>(null);
  const [scrapedPages, setScrapedPages] = useState<ScrapedPage[]>([]);
  const [searchQuery, setSearchQuery] = useState('');
  const [searchResults, setSearchResults] = useState<ScrapedPage[]>([]);

  const loadJobs = useCallback(async () => {
    if (!profileId) return;
    try {
      setLoading(true);
      const loadedJobs = await invoke<ScrapingJob[]>('get_scraping_jobs', { profileId: profileId });
      setJobs(loadedJobs);
    } catch (err) {
      console.error('Failed to load scraping jobs:', err);
    } finally {
      setLoading(false);
    }
  }, [profileId]);

  useEffect(() => {
    loadJobs();
  }, [loadJobs]);

  const handleDeleteJob = async (jobId: number) => {
    if (!confirm('Are you sure you want to delete this scraping job and all scraped pages?')) return;
    try {
      await invoke('delete_scraping_job', { jobId: jobId });
      loadJobs();
      if (selectedJob?.id === jobId) {
        setSelectedJob(null);
        setScrapedPages([]);
      }
    } catch (err) {
      console.error('Failed to delete job:', err);
    }
  };

  const handleViewPages = async (job: ScrapingJob) => {
    setSelectedJob(job);
    try {
      const pages = await invoke<ScrapedPage[]>('get_scraped_pages', { jobId: job.id, limit: 100 });
      setScrapedPages(pages);
    } catch (err) {
      console.error('Failed to load scraped pages:', err);
    }
  };

  const handleSearch = async () => {
    if (!profileId || !searchQuery.trim()) return;
    try {
      const results = await invoke<ScrapedPage[]>('search_scraped_content', {
        profileId: profileId,
        query: searchQuery,
        limit: 50,
      });
      setSearchResults(results);
    } catch (err) {
      console.error('Failed to search content:', err);
    }
  };

  if (!profileId) {
    return (
      <div className="flex items-center justify-center h-64 text-[var(--text-muted-color)]">
        Please select a profile to use the Web Scraper
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-xl font-bold text-[var(--text-color)]">Web Scraper</h2>
          <p className="text-sm text-[var(--text-muted-color)]">
            Scrape and index web content for local search
          </p>
        </div>
        <button
          onClick={() => setShowCreateModal(true)}
          className="flex items-center gap-2 px-4 py-2 bg-[var(--primary-color)] text-white rounded-lg hover:opacity-90 transition-opacity"
        >
          <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
          </svg>
          New Scraping Job
        </button>
      </div>

      {/* Search */}
      <div className="bg-[var(--card-bg-color)] rounded-xl p-4 border border-gray-700/50">
        <div className="flex gap-3">
          <div className="flex-1 relative">
            <input
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              onKeyDown={(e) => e.key === 'Enter' && handleSearch()}
              placeholder="Search scraped content..."
              className="w-full px-4 py-2 pl-10 bg-[var(--bg-color)] border border-gray-700 rounded-lg text-[var(--text-color)] placeholder-gray-500 focus:outline-none focus:border-[var(--primary-color)]"
            />
            <svg className="w-5 h-5 absolute left-3 top-1/2 -translate-y-1/2 text-gray-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
            </svg>
          </div>
          <button
            onClick={handleSearch}
            className="px-4 py-2 bg-[var(--primary-color)] text-white rounded-lg hover:opacity-90 transition-opacity"
          >
            Search
          </button>
        </div>

        {/* Search Results */}
        {searchResults.length > 0 && (
          <div className="mt-4 space-y-2 max-h-64 overflow-y-auto">
            <h4 className="text-sm font-medium text-[var(--text-muted-color)] mb-2">
              Found {searchResults.length} results
            </h4>
            {searchResults.map((page) => (
              <div
                key={page.id}
                className="p-3 bg-[var(--bg-color)] rounded-lg border border-gray-700/50 hover:border-[var(--primary-color)]/50 transition-colors"
              >
                <a
                  href={page.url}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-[var(--primary-color)] hover:underline font-medium"
                >
                  {page.title || page.url}
                </a>
                <p className="text-xs text-[var(--text-muted-color)] mt-1 line-clamp-2">
                  {page.content.substring(0, 200)}...
                </p>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Jobs List */}
      <div className="grid gap-4">
        {loading ? (
          <div className="flex items-center justify-center py-12">
            <svg className="w-8 h-8 animate-spin text-[var(--primary-color)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
            </svg>
          </div>
        ) : jobs.length === 0 ? (
          <div className="text-center py-12 text-[var(--text-muted-color)]">
            <svg className="w-16 h-16 mx-auto mb-4 opacity-50" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 12a9 9 0 01-9 9m9-9a9 9 0 00-9-9m9 9H3m9 9a9 9 0 01-9-9m9 9c1.657 0 3-4.03 3-9s-1.343-9-3-9m0 18c-1.657 0-3-4.03-3-9s1.343-9 3-9m-9 9a9 9 0 019-9" />
            </svg>
            <p>No scraping jobs yet</p>
            <p className="text-sm mt-1">Create a new job to start scraping web content</p>
          </div>
        ) : (
          jobs.map((job) => (
            <ScrapingJobCard
              key={job.id}
              job={job}
              onView={() => handleViewPages(job)}
              onDelete={() => job.id && handleDeleteJob(job.id)}
              isSelected={selectedJob?.id === job.id}
            />
          ))
        )}
      </div>

      {/* Selected Job Pages */}
      {selectedJob && (
        <div className="bg-[var(--card-bg-color)] rounded-xl p-4 border border-gray-700/50">
          <div className="flex items-center justify-between mb-4">
            <h3 className="font-medium text-[var(--text-color)]">
              Scraped Pages from "{selectedJob.name}"
            </h3>
            <button
              onClick={() => {
                setSelectedJob(null);
                setScrapedPages([]);
              }}
              className="text-[var(--text-muted-color)] hover:text-[var(--text-color)]"
            >
              <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>

          <div className="space-y-2 max-h-96 overflow-y-auto">
            {scrapedPages.length === 0 ? (
              <p className="text-center py-8 text-[var(--text-muted-color)]">
                No pages scraped yet
              </p>
            ) : (
              scrapedPages.map((page) => (
                <div
                  key={page.id}
                  className="p-3 bg-[var(--bg-color)] rounded-lg border border-gray-700/50"
                >
                  <a
                    href={page.url}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="text-[var(--primary-color)] hover:underline font-medium text-sm"
                  >
                    {page.title || page.url}
                  </a>
                  <p className="text-xs text-[var(--text-muted-color)] mt-1 line-clamp-2">
                    {page.content.substring(0, 150)}...
                  </p>
                  <p className="text-xs text-gray-500 mt-1">
                    Scraped: {new Date(page.scraped_at).toLocaleString()}
                  </p>
                </div>
              ))
            )}
          </div>
        </div>
      )}

      {/* Create Modal */}
      {showCreateModal && (
        <CreateScrapingJobModal
          profileId={profileId}
          onClose={() => setShowCreateModal(false)}
          onCreated={() => {
            setShowCreateModal(false);
            loadJobs();
          }}
        />
      )}
    </div>
  );
}

// Scraping Job Card Component
interface ScrapingJobCardProps {
  job: ScrapingJob;
  onView: () => void;
  onDelete: () => void;
  isSelected: boolean;
}

function ScrapingJobCard({ job, onView, onDelete, isSelected }: ScrapingJobCardProps) {
  const getStatusColor = (status: string) => {
    switch (status) {
      case 'completed': return 'text-green-400 bg-green-400/10';
      case 'running': return 'text-blue-400 bg-blue-400/10';
      case 'failed': return 'text-red-400 bg-red-400/10';
      default: return 'text-gray-400 bg-gray-400/10';
    }
  };

  return (
    <div
      className={`bg-[var(--card-bg-color)] rounded-xl p-4 border transition-colors ${
        isSelected ? 'border-[var(--primary-color)]' : 'border-gray-700/50 hover:border-gray-600'
      }`}
    >
      <div className="flex items-start justify-between">
        <div className="flex-1">
          <div className="flex items-center gap-3">
            <h3 className="font-medium text-[var(--text-color)]">{job.name}</h3>
            <span className={`px-2 py-0.5 rounded-full text-xs font-medium ${getStatusColor(job.status)}`}>
              {job.status}
            </span>
          </div>
          <a
            href={job.base_url}
            target="_blank"
            rel="noopener noreferrer"
            className="text-sm text-[var(--primary-color)] hover:underline mt-1 block"
          >
            {job.base_url}
          </a>

          <div className="flex items-center gap-4 mt-3 text-xs text-[var(--text-muted-color)]">
            <span className="flex items-center gap-1">
              <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
              </svg>
              {job.pages_scraped} pages
            </span>
            <span className="flex items-center gap-1">
              <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 8V4m0 0h4M4 4l5 5m11-1V4m0 0h-4m4 0l-5 5M4 16v4m0 0h4m-4 0l5-5m11 5l-5-5m5 5v-4m0 4h-4" />
              </svg>
              Depth: {job.max_depth}
            </span>
            <span className="flex items-center gap-1">
              <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
              </svg>
              Max: {job.max_pages} pages
            </span>
          </div>

          {job.last_run_at && (
            <p className="text-xs text-gray-500 mt-2">
              Last run: {new Date(job.last_run_at).toLocaleString()}
            </p>
          )}
        </div>

        <div className="flex items-center gap-2">
          <button
            onClick={onView}
            className="p-2 text-[var(--text-muted-color)] hover:text-[var(--primary-color)] hover:bg-[var(--primary-color)]/10 rounded-lg transition-colors"
            title="View scraped pages"
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
            </svg>
          </button>
          <button
            onClick={onDelete}
            className="p-2 text-[var(--text-muted-color)] hover:text-red-400 hover:bg-red-400/10 rounded-lg transition-colors"
            title="Delete job"
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
            </svg>
          </button>
        </div>
      </div>
    </div>
  );
}

// Create Scraping Job Modal
interface CreateScrapingJobModalProps {
  profileId: number;
  onClose: () => void;
  onCreated: () => void;
}

function CreateScrapingJobModal({ profileId, onClose, onCreated }: CreateScrapingJobModalProps) {
  const [name, setName] = useState('');
  const [baseUrl, setBaseUrl] = useState('');
  const [urlPattern, setUrlPattern] = useState('');
  const [maxDepth, setMaxDepth] = useState(2);
  const [maxPages, setMaxPages] = useState(100);
  const [selectors, setSelectors] = useState<ContentSelector[]>([]);
  const [newSelectorName, setNewSelectorName] = useState('');
  const [newSelectorValue, setNewSelectorValue] = useState('');
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleAddSelector = () => {
    if (newSelectorName.trim() && newSelectorValue.trim()) {
      setSelectors([...selectors, { name: newSelectorName.trim(), selector: newSelectorValue.trim() }]);
      setNewSelectorName('');
      setNewSelectorValue('');
    }
  };

  const handleRemoveSelector = (index: number) => {
    setSelectors(selectors.filter((_, i) => i !== index));
  };

  const handleCreate = async () => {
    if (!name.trim() || !baseUrl.trim()) {
      setError('Name and Base URL are required');
      return;
    }

    try {
      setCreating(true);
      setError(null);

      await invoke('create_scraping_job', {
        profileId: profileId,
        name: name.trim(),
        baseUrl: baseUrl.trim(),
        urlPattern: urlPattern.trim() || null,
        maxDepth: maxDepth,
        maxPages: maxPages,
        contentSelectors: selectors,
      });

      onCreated();
    } catch (err) {
      setError(String(err));
    } finally {
      setCreating(false);
    }
  };

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div className="bg-[var(--card-bg-color)] rounded-xl p-6 w-full max-w-lg max-h-[90vh] overflow-y-auto border border-gray-700">
        <div className="flex items-center justify-between mb-6">
          <h2 className="text-xl font-bold text-[var(--text-color)]">Create Scraping Job</h2>
          <button
            onClick={onClose}
            className="text-[var(--text-muted-color)] hover:text-[var(--text-color)]"
          >
            <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        {error && (
          <div className="mb-4 p-3 bg-red-500/10 border border-red-500/50 rounded-lg text-red-400 text-sm">
            {error}
          </div>
        )}

        <div className="space-y-4">
          {/* Name */}
          <div>
            <label className="block text-sm font-medium text-[var(--text-color)] mb-1">
              Job Name *
            </label>
            <input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="e.g., Documentation Scraper"
              className="w-full px-4 py-2 bg-[var(--bg-color)] border border-gray-700 rounded-lg text-[var(--text-color)] placeholder-gray-500 focus:outline-none focus:border-[var(--primary-color)]"
            />
          </div>

          {/* Base URL */}
          <div>
            <label className="block text-sm font-medium text-[var(--text-color)] mb-1">
              Base URL *
            </label>
            <input
              type="url"
              value={baseUrl}
              onChange={(e) => setBaseUrl(e.target.value)}
              placeholder="https://example.com/docs"
              className="w-full px-4 py-2 bg-[var(--bg-color)] border border-gray-700 rounded-lg text-[var(--text-color)] placeholder-gray-500 focus:outline-none focus:border-[var(--primary-color)]"
            />
          </div>

          {/* URL Pattern */}
          <div>
            <label className="block text-sm font-medium text-[var(--text-color)] mb-1">
              URL Pattern (Regex)
            </label>
            <input
              type="text"
              value={urlPattern}
              onChange={(e) => setUrlPattern(e.target.value)}
              placeholder="e.g., ^https://example\.com/docs/.*"
              className="w-full px-4 py-2 bg-[var(--bg-color)] border border-gray-700 rounded-lg text-[var(--text-color)] placeholder-gray-500 focus:outline-none focus:border-[var(--primary-color)]"
            />
            <p className="text-xs text-[var(--text-muted-color)] mt-1">
              Only scrape URLs matching this pattern. Leave empty to scrape all links.
            </p>
          </div>

          {/* Depth and Pages */}
          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className="block text-sm font-medium text-[var(--text-color)] mb-1">
                Max Depth
              </label>
              <input
                type="number"
                value={maxDepth}
                onChange={(e) => setMaxDepth(parseInt(e.target.value) || 1)}
                min={1}
                max={10}
                className="w-full px-4 py-2 bg-[var(--bg-color)] border border-gray-700 rounded-lg text-[var(--text-color)] focus:outline-none focus:border-[var(--primary-color)]"
              />
            </div>
            <div>
              <label className="block text-sm font-medium text-[var(--text-color)] mb-1">
                Max Pages
              </label>
              <input
                type="number"
                value={maxPages}
                onChange={(e) => setMaxPages(parseInt(e.target.value) || 10)}
                min={1}
                max={1000}
                className="w-full px-4 py-2 bg-[var(--bg-color)] border border-gray-700 rounded-lg text-[var(--text-color)] focus:outline-none focus:border-[var(--primary-color)]"
              />
            </div>
          </div>

          {/* Content Selectors */}
          <div>
            <label className="block text-sm font-medium text-[var(--text-color)] mb-1">
              Content Selectors (CSS)
            </label>
            <div className="space-y-2">
              {selectors.map((selector, index) => (
                <div key={index} className="flex items-center gap-2 p-2 bg-[var(--bg-color)] rounded-lg">
                  <span className="text-sm text-[var(--text-color)] font-medium">{selector.name}:</span>
                  <code className="text-xs text-[var(--primary-color)] flex-1">{selector.selector}</code>
                  <button
                    onClick={() => handleRemoveSelector(index)}
                    className="text-red-400 hover:text-red-300"
                  >
                    <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                    </svg>
                  </button>
                </div>
              ))}

              <div className="flex gap-2">
                <input
                  type="text"
                  value={newSelectorName}
                  onChange={(e) => setNewSelectorName(e.target.value)}
                  placeholder="Name"
                  className="w-24 px-3 py-1.5 bg-[var(--bg-color)] border border-gray-700 rounded-lg text-sm text-[var(--text-color)] placeholder-gray-500 focus:outline-none focus:border-[var(--primary-color)]"
                />
                <input
                  type="text"
                  value={newSelectorValue}
                  onChange={(e) => setNewSelectorValue(e.target.value)}
                  placeholder="CSS Selector (e.g., article.content)"
                  className="flex-1 px-3 py-1.5 bg-[var(--bg-color)] border border-gray-700 rounded-lg text-sm text-[var(--text-color)] placeholder-gray-500 focus:outline-none focus:border-[var(--primary-color)]"
                />
                <button
                  onClick={handleAddSelector}
                  className="px-3 py-1.5 bg-[var(--primary-color)]/20 text-[var(--primary-color)] rounded-lg hover:bg-[var(--primary-color)]/30 transition-colors"
                >
                  Add
                </button>
              </div>
            </div>
            <p className="text-xs text-[var(--text-muted-color)] mt-1">
              Specify CSS selectors to extract specific content. Leave empty to extract all text.
            </p>
          </div>
        </div>

        {/* Actions */}
        <div className="flex justify-end gap-3 mt-6 pt-4 border-t border-gray-700">
          <button
            onClick={onClose}
            className="px-4 py-2 text-[var(--text-color)] hover:bg-gray-700/50 rounded-lg transition-colors"
          >
            Cancel
          </button>
          <button
            onClick={handleCreate}
            disabled={creating}
            className="px-4 py-2 bg-[var(--primary-color)] text-white rounded-lg hover:opacity-90 transition-opacity disabled:opacity-50 flex items-center gap-2"
          >
            {creating && (
              <svg className="w-4 h-4 animate-spin" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
              </svg>
            )}
            Create Job
          </button>
        </div>
      </div>
    </div>
  );
}

export default WebScraper;
