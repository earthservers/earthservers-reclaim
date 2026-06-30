// Review-pinned panel — the curator PROPOSES, the user DISPOSES. Calls
// review_pinned() (which only scores, never mutates) and lets the user act per row
// (Archive / Forget / Keep) or batch-approve the suggestions. Nothing here deletes
// or archives a pin without an explicit click. When the disk soft-cap fires the
// header shows an "urgent" banner.

import { useEffect, useState, useCallback } from 'react';
import { invoke, isTauri } from '../lib/tauri';

type Action = 'archive' | 'forget' | 'keep';

interface Candidate {
  pageId: number;
  url: string;
  title: string;
  pruneScore: number;
  reason: string;
  suggested: Action;
  duplicateOf: number | null;
}

interface ReviewResult {
  candidates: Candidate[];
  indexBytes: number;
  urgent: boolean;
}

function fmtBytes(n: number) {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
  return `${(n / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

export function ReviewPinnedPanel({
  profileId,
  isOpen,
  onClose,
}: {
  profileId: number | null;
  isOpen: boolean;
  onClose: () => void;
}) {
  const [result, setResult] = useState<ReviewResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [pending, setPending] = useState<Record<number, Action>>({}); // user's chosen action per row
  const [done, setDone] = useState<Record<number, Action>>({});       // applied rows

  const load = useCallback(() => {
    if (!isTauri()) return;
    setLoading(true);
    setPending({});
    setDone({});
    invoke<ReviewResult>('review_pinned', { profileId: profileId ?? 1 })
      .then(r => setResult(r))
      .catch(() => setResult({ candidates: [], indexBytes: 0, urgent: false }))
      .finally(() => setLoading(false));
  }, [profileId]);

  useEffect(() => { if (isOpen) load(); }, [isOpen, load]);

  const applyOne = useCallback(async (c: Candidate, action: Action) => {
    if (action === 'keep') { setDone(d => ({ ...d, [c.pageId]: 'keep' })); return; }
    try {
      if (action === 'archive') await invoke('archive_result', { pageId: c.pageId, profileId: profileId ?? 1 });
      else if (action === 'forget') await invoke('forget_result', { pageId: c.pageId });
      setDone(d => ({ ...d, [c.pageId]: action }));
    } catch { /* leave row actionable */ }
  }, [profileId]);

  const applyAllSuggested = useCallback(async () => {
    if (!result) return;
    for (const c of result.candidates) {
      if (done[c.pageId]) continue;
      const action = pending[c.pageId] ?? c.suggested;
      // Never auto-forget; batch only applies non-destructive-by-default suggestions.
      if (action === 'archive') await applyOne(c, 'archive');
      else setDone(d => ({ ...d, [c.pageId]: 'keep' }));
    }
  }, [result, pending, done, applyOne]);

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-[99998] flex items-center justify-center bg-black/60 backdrop-blur-sm p-4" onClick={onClose}>
      <div
        className="w-full max-w-3xl max-h-[85vh] flex flex-col bg-gray-900/97 border border-white/15 rounded-2xl shadow-2xl"
        onClick={e => e.stopPropagation()}
      >
        {/* Header */}
        <div className="px-5 py-4 border-b border-white/10 flex items-start justify-between gap-3">
          <div className="min-w-0">
            <h2 className="text-lg font-semibold text-white">Review pinned pages</h2>
            <p className="text-xs text-[var(--text-muted-color)] mt-0.5">
              The curator suggests — you decide. Nothing is removed without your click.
              {result && <> · index uses {fmtBytes(result.indexBytes)}</>}
            </p>
          </div>
          <button onClick={onClose} className="p-1 rounded hover:bg-white/10 text-gray-400 hover:text-white flex-shrink-0">
            <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" /></svg>
          </button>
        </div>

        {result?.urgent && (
          <div className="mx-5 mt-3 px-3 py-2 rounded-lg bg-amber-500/15 border border-amber-500/30 text-amber-200 text-sm">
            ⚠ The local index is over its soft size cap. Consider archiving the candidates below to reclaim disk.
          </div>
        )}

        {/* Body */}
        <div className="flex-1 overflow-y-auto px-5 py-4 space-y-2">
          {loading && <div className="text-center text-gray-500 py-8">Scoring pinned pages…</div>}
          {!loading && result && result.candidates.length === 0 && (
            <div className="text-center text-gray-500 py-8">No pinned pages to review. 🎉</div>
          )}
          {!loading && result?.candidates.map(c => {
            const applied = done[c.pageId];
            const choice = pending[c.pageId] ?? c.suggested;
            return (
              <div key={c.pageId} className={`p-3 rounded-lg border ${applied ? 'border-white/5 bg-white/5 opacity-60' : 'border-white/10 bg-white/5'}`}>
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <div className="text-sm font-medium text-white truncate">{c.title || c.url}</div>
                    <div className="text-xs text-gray-500 truncate">{c.url}</div>
                    <div className="text-xs text-[var(--text-muted-color)] mt-1">
                      {c.reason}
                      {c.duplicateOf != null && <span className="text-violet-300"> · duplicate of #{c.duplicateOf}</span>}
                    </div>
                  </div>
                  <div className="text-[10px] text-gray-600 font-mono flex-shrink-0">score {c.pruneScore.toFixed(2)}</div>
                </div>
                <div className="flex items-center gap-2 mt-2">
                  {applied ? (
                    <span className="text-xs text-gray-400 capitalize">✓ {applied === 'keep' ? 'kept' : applied + 'd'}</span>
                  ) : (
                    <>
                      {(['archive', 'forget', 'keep'] as Action[]).map(a => (
                        <button
                          key={a}
                          onClick={() => setPending(p => ({ ...p, [c.pageId]: a }))}
                          className={`px-2.5 py-1 rounded-md border text-xs capitalize transition-colors ${
                            choice === a
                              ? a === 'forget'
                                ? 'border-red-500/50 bg-red-500/10 text-red-300'
                                : 'border-[var(--primary-color)] bg-[var(--primary-color)]/10 text-white'
                              : 'border-white/10 text-gray-400 hover:text-white hover:border-white/25'
                          }`}
                        >
                          {a}{c.suggested === a ? ' •' : ''}
                        </button>
                      ))}
                      <button
                        onClick={() => applyOne(c, choice)}
                        className="ml-auto px-3 py-1 rounded-md bg-[var(--primary-color)] text-white text-xs hover:opacity-90"
                      >
                        Apply
                      </button>
                    </>
                  )}
                </div>
              </div>
            );
          })}
        </div>

        {/* Footer */}
        {!loading && result && result.candidates.length > 0 && (
          <div className="px-5 py-3 border-t border-white/10 flex items-center justify-between gap-3">
            <span className="text-xs text-gray-500">“•” marks the curator's suggestion. Batch applies suggested archives only (never forget).</span>
            <button onClick={applyAllSuggested} className="px-4 py-1.5 rounded-lg bg-[var(--primary-color)] text-white text-sm hover:opacity-90">
              Apply suggested
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

export default ReviewPinnedPanel;
