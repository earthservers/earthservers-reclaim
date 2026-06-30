// Per-source picker for local search: which adapters to query, their reliability,
// and the opt-in "use my logged-in session" flow (default OFF) with a blunt ToS
// warning. The session secret is stored encrypted in the backend and never read
// back here — we only see enabled + hasSession.

import { useEffect, useState, useCallback } from 'react';
import { invoke, isTauri } from '../lib/tauri';

interface SourceMeta { id: string; reliability: string; defaultEnabled: boolean }
interface SessionState { adapterId: string; enabled: boolean; hasSession: boolean }

const LABELS: Record<string, string> = {
  web: 'Web', reddit: 'Reddit', forums: 'Forums', youtube: 'YouTube',
  tiktok: 'TikTok', instagram: 'Instagram', facebook: 'Facebook',
};
// Adapters that can use an optional user-supplied session.
const SESSION_CAPABLE = new Set(['instagram', 'facebook', 'tiktok']);

const RELIABILITY_TONE: Record<string, string> = {
  reliable: 'text-green-400', 'best-effort': 'text-amber-400', fragile: 'text-red-400',
};

export function SearchSourcesPanel({
  profileId,
  selected,
  onChange,
  onClose,
}: {
  profileId: number | null;
  selected: string[];
  onChange: (ids: string[]) => void;
  onClose: () => void;
}) {
  const [sources, setSources] = useState<SourceMeta[]>([]);
  const [sessions, setSessions] = useState<Record<string, SessionState>>({});
  const [editingSession, setEditingSession] = useState<string | null>(null);
  const [pasteValue, setPasteValue] = useState('');
  const [confirmedWarning, setConfirmedWarning] = useState(false);

  const loadSessions = useCallback(() => {
    if (!isTauri()) return;
    invoke<SessionState[]>('get_adapter_sessions', { profileId: profileId ?? 1 })
      .then(list => setSessions(Object.fromEntries(list.map(s => [s.adapterId, s]))))
      .catch(() => {});
  }, [profileId]);

  useEffect(() => {
    if (!isTauri()) return;
    invoke<SourceMeta[]>('list_search_sources').then(setSources).catch(() => {});
    loadSessions();
  }, [loadSessions]);

  const toggle = (id: string) => {
    onChange(selected.includes(id) ? selected.filter(s => s !== id) : [...selected, id]);
  };

  const saveSession = async (id: string, enabled: boolean, session: string | null) => {
    if (!isTauri()) return;
    await invoke('set_adapter_session', {
      profileId: profileId ?? 1, adapterId: id, enabled, session,
    }).catch(() => {});
    setEditingSession(null);
    setPasteValue('');
    setConfirmedWarning(false);
    loadSessions();
  };

  return (
    <div className="bg-theme-card/80 border border-white/10 rounded-xl p-4 space-y-2">
      <div className="flex items-center justify-between mb-1">
        <span className="text-sm font-medium text-white">Sources</span>
        <button onClick={onClose} className="text-gray-500 hover:text-white text-xs">Done</button>
      </div>
      {sources.map(src => {
        const sess = sessions[src.id];
        const sessionOn = sess?.enabled && sess?.hasSession;
        return (
          <div key={src.id} className="rounded-lg bg-white/5 p-2">
            <div className="flex items-center gap-2">
              <label className="flex items-center gap-2 cursor-pointer flex-1">
                <input type="checkbox" checked={selected.includes(src.id)} onChange={() => toggle(src.id)}
                  className="accent-[var(--primary-color)]" />
                <span className="text-sm text-white">{LABELS[src.id] ?? src.id}</span>
                <span className={`text-[10px] ${RELIABILITY_TONE[src.reliability] ?? 'text-gray-500'}`}>
                  {src.reliability}{!src.defaultEnabled ? ' · off by default' : ''}
                </span>
              </label>
              {SESSION_CAPABLE.has(src.id) && (
                <button
                  onClick={() => { setEditingSession(editingSession === src.id ? null : src.id); setConfirmedWarning(false); }}
                  className={`text-[11px] px-2 py-0.5 rounded border ${sessionOn ? 'border-amber-500/50 text-amber-300' : 'border-white/15 text-gray-400 hover:text-white'}`}
                  title="Use your own logged-in session (advanced, ToS risk)"
                >
                  {sessionOn ? 'session on' : 'use session'}
                </button>
              )}
            </div>

            {editingSession === src.id && (
              <div className="mt-2 p-2 rounded bg-red-500/10 border border-red-500/30 space-y-2">
                <p className="text-[11px] text-red-200 leading-snug">
                  ⚠ Using your logged-in session for automated requests violates these platforms'
                  Terms of Service and can get the account rate-limited or banned. Use a throwaway
                  account, not your main. Reclaim never automates credentials — you supply your own
                  session string.
                </p>
                {sessionOn ? (
                  <button onClick={() => saveSession(src.id, false, '')} className="text-xs px-2 py-1 rounded bg-white/10 text-white">
                    Turn off & clear session
                  </button>
                ) : (
                  <>
                    <label className="flex items-center gap-2 text-[11px] text-red-200">
                      <input type="checkbox" checked={confirmedWarning} onChange={e => setConfirmedWarning(e.target.checked)} />
                      I understand the risk and am using a throwaway account.
                    </label>
                    <input
                      type="password"
                      value={pasteValue}
                      onChange={e => setPasteValue(e.target.value)}
                      placeholder="Paste your session cookie/token"
                      className="w-full bg-black/40 border border-white/10 rounded px-2 py-1 text-xs text-white"
                    />
                    <button
                      disabled={!confirmedWarning || !pasteValue.trim()}
                      onClick={() => saveSession(src.id, true, pasteValue)}
                      className="text-xs px-3 py-1 rounded bg-amber-600 text-white disabled:opacity-40"
                    >
                      Enable session
                    </button>
                  </>
                )}
              </div>
            )}
          </div>
        );
      })}
      <p className="text-[10px] text-gray-500 pt-1">
        Only public, logged-out data is used by default. Logged-in sessions are opt-in and your own.
      </p>
    </div>
  );
}

export default SearchSourcesPanel;
