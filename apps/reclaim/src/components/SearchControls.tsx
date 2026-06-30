// Search configuration bar — shown on the search page so the user can set these
// BEFORE searching (they used to be buried inside the results, which only appeared
// after a search). Controlled by the parent (DomainManager).

import { useState } from 'react';
import { SearchSourcesPanel } from './SearchSourcesPanel';
import type { Retention, KindsMode } from './LocalSearchResults';

export function SearchControls({
  profileId,
  retention, setRetention,
  kindsMode, setKindsMode,
  sources, setSources,
  showDebug, setShowDebug,
  query,
  onOpenInDuckDuckGo,
}: {
  profileId: number | null;
  retention: Retention; setRetention: (r: Retention) => void;
  kindsMode: KindsMode; setKindsMode: (k: KindsMode) => void;
  sources: string[]; setSources: (s: string[]) => void;
  showDebug: boolean; setShowDebug: (b: boolean) => void;
  query: string;
  onOpenInDuckDuckGo?: (q: string) => void;
}) {
  const [showSources, setShowSources] = useState(false);

  return (
    <div className="space-y-2">
      <div className="flex items-center gap-3 flex-wrap">
        {/* Kinds */}
        <div className="flex items-center gap-1 bg-white/5 rounded-lg p-0.5">
          {([['all', 'All'], ['discussions', 'Comments & discussions'], ['comments', 'Comments only']] as [KindsMode, string][]).map(([m, label]) => (
            <button key={m} onClick={() => setKindsMode(m)}
              className={`px-2.5 py-1 rounded-md text-xs transition-colors ${kindsMode === m ? 'bg-[var(--primary-color)] text-white' : 'text-gray-400 hover:text-white'}`}>
              {label}
            </button>
          ))}
        </div>

        {/* Retention */}
        <div className="flex items-center gap-1">
          <span className="text-xs text-[var(--text-muted-color)]">Keep:</span>
          {(['ephemeral', 'cache', 'pinned'] as Retention[]).map(t => (
            <button key={t} onClick={() => setRetention(t)}
              className={`px-2.5 py-1 rounded-md border text-xs capitalize transition-colors ${retention === t ? 'border-[var(--primary-color)] bg-[var(--primary-color)]/10 text-white' : 'border-white/10 text-gray-400 hover:text-white hover:border-white/25'}`}
              title={t === 'ephemeral' ? 'This session only (~1h)' : t === 'cache' ? 'Cache locally (~7d)' : 'Pin permanently + curate'}>
              {t === 'pinned' ? 'pin' : t}
            </button>
          ))}
        </div>

        <button onClick={() => setShowSources(s => !s)} className="text-xs px-2.5 py-1 rounded-md border border-white/10 text-gray-400 hover:text-white">
          Sources ({sources.length})
        </button>
        <button onClick={() => setShowDebug(!showDebug)}
          className={`px-2 py-1 rounded-md border text-xs ${showDebug ? 'border-[var(--primary-color)] text-white' : 'border-white/10 text-gray-500 hover:text-white'}`}
          title="Show ranking signals on each result">
          🐛 signals
        </button>
        {onOpenInDuckDuckGo && query.trim() && (
          <button onClick={() => onOpenInDuckDuckGo(query)} className="text-xs px-2.5 py-1 rounded-md border border-white/10 text-gray-400 hover:text-white hover:border-white/25"
            title="Search this query on DuckDuckGo instead">
            DuckDuckGo ↗
          </button>
        )}
      </div>

      {kindsMode !== 'all' && (
        <p className="text-[11px] text-[var(--text-muted-color)] leading-snug">
          Note: platforms don't allow searching all comments globally. This finds comments &amp;
          discussions <em>within</em> the threads/videos/posts your query surfaces — not the entire corpus.
        </p>
      )}

      {showSources && (
        <SearchSourcesPanel profileId={profileId} selected={sources} onChange={setSources} onClose={() => setShowSources(false)} />
      )}
    </div>
  );
}

export default SearchControls;
