// Local search results surface — "Google but completely local".
//
// Renders INLINE on the main search page (a clicked result navigates the page
// away, so it doesn't need to be a separate panel). Drives the two-speed
// local_search backend: shallow SearXNG-speed snippets paint immediately, deep
// scraped+indexed results stream in, then the list re-orders to the fused ranking.
// Every event is filtered by the active queryId so a new search supersedes the old.

import { useEffect, useRef, useState, useCallback } from 'react';
import { invoke, listen, isTauri } from '../lib/tauri';
import { FavoriteStar } from './FavoriteStar';

type Retention = 'ephemeral' | 'cache' | 'pinned';

interface Signals {
  ftsRank: number | null;
  vecRank: number | null;
  posRank: number | null;
  clickBoost: number;
}

interface ResultRow {
  url: string;
  title: string;
  snippet: string;
  sourceEngine: string;
  pageId?: number;
  cacheHit?: boolean;
  fusedScore?: number;
  signals?: Signals;
  curated?: boolean;
  // local lifecycle state for optimistic UI
  pinned?: boolean;
  archived?: boolean;
  forgotten?: boolean;
}

interface ShallowEvt { queryId: number; candidate: { url: string; title: string; snippet: string; sourceEngine: string } }
interface DeepEvt { queryId: number; page: { pageId: number; url: string; title: string; snippet: string; sourceEngine: string; cacheHit: boolean } }
interface RankedEvt { queryId: number; ranked: Array<{ pageId: number; url: string; title: string; snippet: string; sourceEngine: string; fusedScore: number; signals: Signals }> }

export function LocalSearchResults({
  profileId,
  query,
  searchNonce,
  onOpenUrl,
  onClear,
  onOpenInDuckDuckGo,
}: {
  profileId: number | null;
  query: string;
  // bump to re-run the same query string
  searchNonce: number;
  onOpenUrl?: (url: string, opts?: { fromAddressBar?: boolean }) => void;
  onClear?: () => void;
  onOpenInDuckDuckGo?: (query: string) => void;
}) {
  const [retention, setRetention] = useState<Retention>('cache');
  const [phase, setPhase] = useState<'idle' | 'searching' | 'ranking' | 'done'>('idle');
  const [rowsByUrl, setRowsByUrl] = useState<Record<string, ResultRow>>({});
  const [order, setOrder] = useState<string[]>([]);        // arrival order (pre-ranking)
  const [rankedOrder, setRankedOrder] = useState<string[] | null>(null);
  const [showDebug, setShowDebug] = useState(false);

  const activeQueryId = useRef<number | null>(null);
  const queryIdForClicks = useRef<number | null>(null);

  const upsertRow = useCallback((url: string, patch: Partial<ResultRow>, base?: Partial<ResultRow>) => {
    setRowsByUrl(prev => {
      const existing = prev[url];
      if (!existing) {
        setOrder(o => (o.includes(url) ? o : [...o, url]));
        return { ...prev, [url]: { url, title: '', snippet: '', sourceEngine: 'web', ...base, ...patch } };
      }
      return { ...prev, [url]: { ...existing, ...patch } };
    });
  }, []);

  // Run the search whenever the query (or nonce, or retention) changes.
  useEffect(() => {
    if (!query.trim() || !isTauri()) return;
    let cancelled = false;
    const unlisteners: Array<() => void> = [];

    // Reset for the new search.
    setPhase('searching');
    setRowsByUrl({});
    setOrder([]);
    setRankedOrder(null);
    activeQueryId.current = null;

    const isActive = (qid: number) => activeQueryId.current === null || qid === activeQueryId.current;

    const setup = async () => {
      const track = (u: () => void) => { if (cancelled) u(); else unlisteners.push(u); };

      track(await listen<{ queryId: number }>('local-search-started', ({ payload }) => {
        activeQueryId.current = payload.queryId;
        queryIdForClicks.current = payload.queryId;
      }));
      track(await listen<ShallowEvt>('local-search-shallow', ({ payload }) => {
        if (!isActive(payload.queryId)) return;
        const c = payload.candidate;
        upsertRow(c.url, {}, { url: c.url, title: c.title, snippet: c.snippet, sourceEngine: c.sourceEngine });
      }));
      track(await listen<DeepEvt>('local-search-deep', ({ payload }) => {
        if (!isActive(payload.queryId)) return;
        const p = payload.page;
        upsertRow(p.url, { pageId: p.pageId, cacheHit: p.cacheHit, title: p.title || undefined, sourceEngine: p.sourceEngine },
          { url: p.url, title: p.title, snippet: p.snippet, sourceEngine: p.sourceEngine });
      }));
      track(await listen<RankedEvt>('local-search-ranked', ({ payload }) => {
        if (!isActive(payload.queryId)) return;
        setPhase('ranking');
        for (const r of payload.ranked) {
          upsertRow(r.url, { pageId: r.pageId, fusedScore: r.fusedScore, signals: r.signals, sourceEngine: r.sourceEngine },
            { url: r.url, title: r.title, snippet: r.snippet, sourceEngine: r.sourceEngine });
        }
        setRankedOrder(payload.ranked.map(r => r.url));
      }));
      track(await listen<{ queryId: number }>('local-search-done', ({ payload }) => {
        if (!isActive(payload.queryId)) return;
        setPhase('done');
      }));
      track(await listen<{ pageId: number }>('local-search-curated', ({ payload }) => {
        setRowsByUrl(prev => {
          const url = Object.keys(prev).find(u => prev[u].pageId === payload.pageId);
          if (!url) return prev;
          return { ...prev, [url]: { ...prev[url], curated: true } };
        });
      }));

      try {
        await invoke('local_search', {
          query,
          retention,
          profileId: profileId ?? 1,
          sources: null,
          limit: 20,
          searxngUrl: null,
        });
      } catch {
        if (!cancelled) setPhase('done');
      }
    };
    setup();

    return () => { cancelled = true; unlisteners.forEach(u => u()); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query, searchNonce, retention, profileId]);

  const openResult = (row: ResultRow) => {
    if (queryIdForClicks.current != null) {
      invoke('log_result_click', { queryId: queryIdForClicks.current, url: row.url, profileId: profileId ?? 1 }).catch(() => {});
    }
    onOpenUrl?.(row.url, { fromAddressBar: true });
  };

  const archive = (row: ResultRow) => {
    if (row.pageId == null) return;
    invoke('archive_result', { pageId: row.pageId, profileId: profileId ?? 1 }).catch(() => {});
    upsertRow(row.url, { archived: true });
  };
  const forget = (row: ResultRow) => {
    if (row.pageId == null) return;
    invoke('forget_result', { pageId: row.pageId }).catch(() => {});
    upsertRow(row.url, { forgotten: true });
  };

  const displayOrder = rankedOrder ?? order;
  const rows = displayOrder.map(u => rowsByUrl[u]).filter((r): r is ResultRow => !!r && !r.forgotten);

  return (
    <div className="bg-theme-card/60 border border-white/10 rounded-2xl p-5 backdrop-blur-sm">
      {/* Header: query, retention control, status, clear */}
      <div className="flex items-center justify-between gap-3 mb-4 flex-wrap">
        <div className="min-w-0">
          <div className="text-sm text-gray-400">Local results for</div>
          <div className="text-lg font-semibold text-white truncate">“{query}”</div>
        </div>
        <div className="flex items-center gap-2">
          <span className="text-xs text-[var(--text-muted-color)]">Keep:</span>
          {(['ephemeral', 'cache', 'pinned'] as Retention[]).map(t => (
            <button
              key={t}
              onClick={() => setRetention(t)}
              className={`px-2.5 py-1 rounded-md border text-xs capitalize transition-colors ${
                retention === t
                  ? 'border-[var(--primary-color)] bg-[var(--primary-color)]/10 text-white'
                  : 'border-white/10 text-gray-400 hover:text-white hover:border-white/25'
              }`}
              title={t === 'ephemeral' ? 'This session only (~1h)' : t === 'cache' ? 'Cache locally (~7d)' : 'Pin permanently + curate'}
            >
              {t === 'pinned' ? 'pin' : t}
            </button>
          ))}
          <button onClick={() => setShowDebug(s => !s)} className={`px-2 py-1 rounded-md border text-xs ${showDebug ? 'border-[var(--primary-color)] text-white' : 'border-white/10 text-gray-500 hover:text-white'}`} title="Show ranking signals">
            🐛
          </button>
          {onOpenInDuckDuckGo && (
            <button
              onClick={() => onOpenInDuckDuckGo(query)}
              className="px-2.5 py-1 rounded-md border border-white/10 text-xs text-gray-400 hover:text-white hover:border-white/25 transition-colors"
              title="Search this query on DuckDuckGo instead"
            >
              DuckDuckGo ↗
            </button>
          )}
          {onClear && (
            <button onClick={onClear} className="p-1 rounded hover:bg-white/10 text-gray-400 hover:text-white" title="Clear results">
              <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" /></svg>
            </button>
          )}
        </div>
      </div>

      {/* Status line */}
      <div className="text-xs text-[var(--text-muted-color)] mb-3 flex items-center gap-2">
        {phase === 'searching' && <><Spinner /> searching &amp; indexing locally…</>}
        {phase === 'ranking' && <><Spinner /> fusing ranking…</>}
        {phase === 'done' && <span>{rows.length} result{rows.length === 1 ? '' : 's'} · ranked</span>}
      </div>

      {/* Results */}
      <div className="space-y-2">
        {rows.length === 0 && phase !== 'done' && (
          <div className="text-sm text-gray-500 py-6 text-center">Fetching first results…</div>
        )}
        {rows.length === 0 && phase === 'done' && (
          <div className="text-sm text-gray-500 py-6 text-center">
            No local results. Is SearXNG running locally?
            {onOpenInDuckDuckGo && (
              <button onClick={() => onOpenInDuckDuckGo(query)} className="ml-2 text-[var(--primary-color)] hover:underline">
                Search DuckDuckGo ↗
              </button>
            )}
          </div>
        )}
        {rows.map(row => (
          <div key={row.url} className="group flex items-start gap-3 p-3 rounded-lg bg-white/5 hover:bg-white/10 transition-colors">
            <div className="flex-1 min-w-0 cursor-pointer" onClick={() => openResult(row)}>
              <div className="flex items-center gap-2">
                <span className="text-[var(--primary-color)] text-sm font-medium truncate">{row.title || row.url}</span>
                <Badge label={row.sourceEngine} />
                {row.cacheHit ? <Badge label="from local index" tone="green" /> : row.pageId != null ? <Badge label="freshly scraped" tone="blue" /> : null}
                {row.pinned && <Badge label="pinned" tone="amber" />}
                {row.archived && <Badge label="archived" tone="gray" />}
                {row.curated && <Badge label="curated" tone="violet" />}
              </div>
              <div className="text-xs text-gray-500 truncate">{row.url}</div>
              {row.snippet && <div className="text-sm text-gray-300 mt-1 line-clamp-2">{row.snippet}</div>}
              {showDebug && row.signals && (
                <div className="text-[10px] text-gray-500 mt-1 font-mono">
                  fts:{fmt(row.signals.ftsRank)} vec:{fmt(row.signals.vecRank)} pos:{fmt(row.signals.posRank)} click:+{row.signals.clickBoost.toFixed(2)} → {row.fusedScore?.toFixed(4)}
                </div>
              )}
            </div>
            {/* Per-result actions. Favorite = the shared pin (single source of truth);
                archive/forget are maintenance. Need a pageId (= indexed) for these. */}
            {row.pageId != null && !row.archived && (
              <div className="flex items-center gap-1 flex-shrink-0">
                {/* Always-visible favorite pin so its state reads at a glance. */}
                <FavoriteStar url={row.url} profileId={profileId} title={row.title} className="p-1.5" />
                <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                  <IconBtn title="Archive (keep summary, drop body)" onClick={() => archive(row)} d="M4 7h16M6 7l1 12h10l1-12M9 11v5M15 11v5" />
                  <IconBtn title="Forget (delete now)" tone="red" onClick={() => forget(row)} d="M6 18L18 6M6 6l12 12" />
                </div>
              </div>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}

function fmt(n: number | null) { return n == null ? '–' : String(n); }

function Spinner() {
  return <span className="inline-block w-3 h-3 border-2 border-white/30 border-t-white/80 rounded-full animate-spin" />;
}

function Badge({ label, tone = 'slate' }: { label: string; tone?: 'slate' | 'green' | 'blue' | 'amber' | 'gray' | 'violet' }) {
  const tones: Record<string, string> = {
    slate: 'bg-white/10 text-gray-300',
    green: 'bg-green-500/15 text-green-300',
    blue: 'bg-blue-500/15 text-blue-300',
    amber: 'bg-amber-500/15 text-amber-300',
    gray: 'bg-white/5 text-gray-400',
    violet: 'bg-violet-500/15 text-violet-300',
  };
  return <span className={`px-1.5 py-0.5 rounded text-[10px] ${tones[tone]} flex-shrink-0`}>{label}</span>;
}

function IconBtn({ title, onClick, d, tone }: { title: string; onClick: () => void; d: string; tone?: 'red' }) {
  return (
    <button
      onClick={(e) => { e.stopPropagation(); onClick(); }}
      title={title}
      className={`p-1.5 rounded hover:bg-white/15 ${tone === 'red' ? 'text-gray-500 hover:text-red-400' : 'text-gray-400 hover:text-white'}`}
    >
      <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d={d} /></svg>
    </button>
  );
}

export default LocalSearchResults;
