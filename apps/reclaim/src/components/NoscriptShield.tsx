// NoScript shield + per-origin trust dropdown.
//
// The web-process extension reports every request origin it sees on the current
// page via `noscript-origin` events (origin + first-party flag). This lists them
// NoScript-style and lets each be set Trusted (persistent), Temp (this session),
// or Untrusted (default/blocked). Third-party untrusted origins are blocked by
// the extension; the first-party origin governs the page's own JavaScript.

import { useState, useEffect, useRef, useCallback } from 'react';
import { invoke, isTauri, listen } from '../lib/tauri';
import { useRightDock, RIGHT_DOCK_WIDTH } from '../lib/rightDock';

type TrustState = 'trusted' | 'temp' | 'untrusted';

interface OriginRow {
  origin: string;
  firstParty: boolean;
  state: TrustState;
}

const STATE_META: Record<TrustState, { label: string; cls: string }> = {
  trusted: { label: 'Trusted', cls: 'bg-green-500 text-white' },
  temp: { label: 'Temp', cls: 'bg-yellow-500 text-black' },
  untrusted: { label: 'Blocked', cls: 'bg-red-500/80 text-white' },
};

export function NoscriptShield({ currentUrl }: { currentUrl: string }) {
  const [rows, setRows] = useState<OriginRow[]>([]);
  const [open, setOpen] = useState(false);
  const [top, setTop] = useState(56);
  const wrapRef = useRef<HTMLDivElement>(null);

  // Reset the origin list and close the panel when the page changes.
  useEffect(() => { setRows([]); setOpen(false); }, [currentUrl]);

  // Register as a right-docked panel so App insets the browser surface (shrinks
  // it left) — the page stays visible as a narrower column while this is the
  // right sidebar. The native surface renders above the DOM, so it must be moved
  // out of the way rather than overlaid.
  useRightDock('noscript', open, () => setOpen(false));

  // On open, pull the full origin list from the backend (which accumulated them
  // during page load) — covers the case where this panel mounted after the page
  // already loaded and missed the live events.
  useEffect(() => {
    if (!open || !isTauri()) return;
    invoke<Array<[string, boolean, TrustState]>>('noscript_list_origins')
      .then(list => {
        setRows(prev => {
          const byOrigin = new Map(prev.map(r => [r.origin, r] as const));
          for (const [origin, firstParty, state] of list) {
            byOrigin.set(origin, { origin, firstParty, state });
          }
          return [...byOrigin.values()].sort(
            (a, b) => (a.firstParty === b.firstParty ? a.origin.localeCompare(b.origin) : a.firstParty ? -1 : 1),
          );
        });
      })
      .catch(() => {});
  }, [open]);

  // Accumulate origins reported for the current page; fetch each one's state.
  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;
    listen<{ origin: string; firstParty: boolean }>('noscript-origin', async ({ payload }) => {
      const state = await invoke<TrustState>('noscript_get_trust', { origin: payload.origin }).catch(() => 'untrusted' as TrustState);
      setRows(prev => {
        if (prev.some(r => r.origin === payload.origin)) return prev;
        const next = [...prev, { origin: payload.origin, firstParty: payload.firstParty, state }];
        // First-party first, then alphabetical.
        next.sort((a, b) => (a.firstParty === b.firstParty ? a.origin.localeCompare(b.origin) : a.firstParty ? -1 : 1));
        return next;
      });
    }).then(u => { unlisten = u; });
    return () => unlisten?.();
  }, []);

  // Close on outside click.
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (wrapRef.current && !wrapRef.current.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener('mousedown', onDown);
    return () => window.removeEventListener('mousedown', onDown);
  }, [open]);

  const setTrust = useCallback(async (origin: string, state: TrustState) => {
    setRows(prev => prev.map(r => (r.origin === origin ? { ...r, state } : r)));
    try {
      await invoke('noscript_set_trust', { origin, state });
    } catch (err) {
      console.error('Failed to set trust:', err);
    }
  }, []);

  // Shield is green only when nothing is blocked; orange when something is.
  const anyBlocked = rows.some(r => r.state === 'untrusted');
  const shieldCls = anyBlocked ? 'text-orange-400 hover:text-orange-300' : 'text-green-400 hover:text-green-300';

  return (
    <div ref={wrapRef} className="relative flex-shrink-0">
      <button
        onClick={() => {
          if (!open && wrapRef.current) {
            setTop(wrapRef.current.getBoundingClientRect().bottom + 8);
          }
          setOpen(o => !o);
        }}
        title="NoScript — manage scripts on this site"
        className={`p-1 rounded transition-colors ${shieldCls}`}
      >
        <svg className="w-4 h-4" fill={anyBlocked ? 'none' : 'currentColor'} stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z" />
        </svg>
      </button>

      {open && (
        <div
          onMouseDown={(e) => e.stopPropagation()}
          style={{ position: 'fixed', top, right: 8, bottom: 8, width: RIGHT_DOCK_WIDTH - 16 }}
          className="overflow-y-auto bg-gray-900/97 border border-white/15 rounded-lg shadow-2xl backdrop-blur-sm z-[99999] p-2"
        >
          <div className="px-2 py-1 mb-1 border-b border-white/10">
            <span className="text-sm font-medium text-white">Scripts on this page</span>
            <p className="text-[11px] text-gray-500 mt-0.5">JavaScript is blocked by default. Trust a source to let it run.</p>
          </div>

          {rows.length === 0 ? (
            <p className="text-xs text-gray-500 text-center py-4">No script sources detected yet.</p>
          ) : (
            <div className="space-y-0.5">
              {rows.map(r => (
                <div key={r.origin} className="flex items-center gap-2 px-2 py-1.5 rounded hover:bg-white/5">
                  <div className="flex-1 min-w-0">
                    <div className="text-xs text-white truncate" title={r.origin}>{r.origin}</div>
                    {r.firstParty && <div className="text-[10px] text-[var(--primary-color)]">this site</div>}
                  </div>
                  <div className="flex gap-0.5 flex-shrink-0">
                    {(['trusted', 'temp', 'untrusted'] as TrustState[]).map(s => (
                      <button
                        key={s}
                        onClick={() => setTrust(r.origin, s)}
                        className={`text-[10px] px-1.5 py-0.5 rounded transition-colors ${
                          r.state === s ? STATE_META[s].cls : 'bg-white/5 text-gray-400 hover:bg-white/10'
                        }`}
                        title={STATE_META[s].label}
                      >
                        {STATE_META[s].label}
                      </button>
                    ))}
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

export default NoscriptShield;
