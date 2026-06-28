// Reusable "autofill from password manager" control for Reclaim's feature
// password gates (media, bookmarks, authenticator, Local AI / History).
//
// The password-manager master acts as the one master password: if an app
// password is stored for this feature, this renders a button that asks for the
// master once, decrypts the stored feature password, and fills the gate's input.

import { useState, useEffect } from 'react';
import { invoke, isTauri } from '../lib/tauri';

export function VaultAutofill({
  profileId,
  appKey,
  onFill,
  className = '',
}: {
  profileId: number | null;
  appKey: string;
  onFill: (password: string) => void;
  className?: string;
}) {
  const [exists, setExists] = useState(false);
  const [open, setOpen] = useState(false);
  const [master, setMaster] = useState('');
  const [err, setErr] = useState('');
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!isTauri()) return;
    invoke<boolean>('vault_has_app_password', { profileId: profileId ?? 1, key: appKey })
      .then(setExists)
      .catch(() => setExists(false));
  }, [profileId, appKey]);

  if (!exists) return null;

  const submit = async () => {
    setErr('');
    setBusy(true);
    try {
      const pw = await invoke<string | null>('vault_get_app_password', {
        profileId: profileId ?? 1,
        key: appKey,
        master,
      });
      if (pw) { onFill(pw); setOpen(false); setMaster(''); }
      else setErr('No saved password for this');
    } catch (e) {
      setErr(String(e).replace(/^.*?:\s*/, ''));
    } finally {
      setBusy(false);
    }
  };

  if (!open) {
    return (
      <button
        type="button"
        onClick={() => setOpen(true)}
        className={`text-xs text-[var(--primary-color)] hover:underline inline-flex items-center gap-1 ${className}`}
        title="Fill this from your password manager (master password)"
      >
        <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 7a2 2 0 012 2m4 0a6 6 0 01-7.743 5.743L11 17H9v2H7v2H4a1 1 0 01-1-1v-2.586a1 1 0 01.293-.707l5.964-5.964A6 6 0 1121 9z" /></svg>
        Autofill from password manager
      </button>
    );
  }

  return (
    <div className={className}>
      <div className="flex items-center gap-1.5">
        <input
          type="password"
          autoFocus
          value={master}
          onChange={e => setMaster(e.target.value)}
          onKeyDown={e => { if (e.key === 'Enter') submit(); }}
          placeholder="Master password"
          className="flex-1 bg-gray-800 border border-gray-600 rounded px-2 py-1 text-xs text-white outline-none focus:border-[var(--primary-color)]"
        />
        <button type="button" onClick={submit} disabled={busy || !master} className="text-xs px-2 py-1 rounded bg-[var(--primary-color)] text-white disabled:opacity-40">Fill</button>
        <button type="button" onClick={() => { setOpen(false); setErr(''); }} className="text-xs text-gray-400 hover:text-white px-1">✕</button>
      </div>
      {err && <p className="text-red-400 text-[10px] mt-1">{err}</p>}
    </div>
  );
}

export default VaultAutofill;
