// Passwords & Security — one place to CHANGE each of Reclaim's passwords. Every
// change requires the current password (no recovery). Changing the password
// manager or authenticator master re-encrypts their stored entries (backend).

import { useState, useEffect } from 'react';
import { invoke } from '../lib/tauri';
import { RightDockPanel } from '../lib/rightDock';

interface Feature {
  key: string;
  label: string;
  hasPassword: (profileId: number) => Promise<boolean>;
  change: (profileId: number, oldPw: string, newPw: string) => Promise<void>;
}

const FEATURES: Feature[] = [
  {
    key: 'password-manager',
    label: 'Password Manager (master)',
    hasPassword: (p) => invoke<boolean>('has_password_manager_master', { profileId: p }),
    change: (p, o, n) => invoke('change_password_manager_master', { profileId: p, oldPassword: o, newPassword: n }),
  },
  {
    key: 'authenticator',
    label: 'Authenticator (master)',
    hasPassword: (p) => invoke<boolean>('has_otp_master', { profileId: p }),
    change: (p, o, n) => invoke('change_otp_master', { profileId: p, oldPassword: o, newPassword: n }),
  },
  {
    key: 'media',
    label: 'Media (history & playlists)',
    hasPassword: async (p) => {
      const s = await invoke<{ password_hash: string | null }>('get_media_privacy_settings', { profileId: p }).catch(() => null);
      return !!s?.password_hash;
    },
    change: async (p, o, n) => {
      const ok = await invoke<boolean>('verify_media_password', { profileId: p, password: o });
      if (!ok) throw new Error('Incorrect current password');
      await invoke('set_media_password', { profileId: p, password: n });
    },
  },
  {
    key: 'bookmarks',
    label: 'Private Bookmarks',
    hasPassword: () => invoke<boolean>('has_private_bookmarks_password'),
    change: async (_p, o, n) => {
      const ok = await invoke<boolean>('verify_private_bookmarks_password', { password: o });
      if (!ok) throw new Error('Incorrect current password');
      await invoke('set_private_bookmarks_password', { password: n });
    },
  },
  {
    key: 'local-ai',
    label: 'Local AI / History',
    hasPassword: () => invoke<boolean>('ai_lock_has_password'),
    change: async (_p, o, n) => {
      const ok = await invoke<boolean>('ai_lock_verify_password', { password: o });
      if (!ok) throw new Error('Incorrect current password');
      await invoke('ai_lock_set_password', { password: n });
    },
  },
];

const inputCls = 'w-full bg-gray-800 border border-gray-600 rounded px-2 py-1.5 text-sm text-white outline-none focus:border-[var(--primary-color)]';

function ChangeRow({ feature, profileId }: { feature: Feature; profileId: number }) {
  const [has, setHas] = useState<boolean | null>(null);
  const [open, setOpen] = useState(false);
  const [cur, setCur] = useState('');
  const [nw, setNw] = useState('');
  const [cf, setCf] = useState('');
  const [err, setErr] = useState('');
  const [done, setDone] = useState(false);
  const [busy, setBusy] = useState(false);

  useEffect(() => { feature.hasPassword(profileId).then(setHas).catch(() => setHas(false)); }, [feature, profileId]);

  const submit = async () => {
    setErr('');
    if (nw.length < 4) { setErr('New password must be at least 4 characters'); return; }
    if (nw !== cf) { setErr('New passwords do not match'); return; }
    setBusy(true);
    try {
      await feature.change(profileId, cur, nw);
      setDone(true); setOpen(false); setCur(''); setNw(''); setCf('');
    } catch (e) {
      setErr(String(e).replace(/^.*?:\s*/, ''));
    } finally { setBusy(false); }
  };

  return (
    <div className="border-b border-white/5 py-2.5">
      <div className="flex items-center justify-between gap-2">
        <span className="text-sm text-white">{feature.label}</span>
        {has === null ? (
          <span className="text-xs text-gray-500">…</span>
        ) : has ? (
          <button onClick={() => { setOpen(o => !o); setErr(''); setDone(false); }} className="text-xs text-[var(--primary-color)] hover:underline">
            {open ? 'Cancel' : 'Change'}
          </button>
        ) : (
          <span className="text-xs text-gray-500">Not set</span>
        )}
      </div>
      {done && <p className="text-green-400 text-xs mt-1">Password changed.</p>}
      {open && has && (
        <div className="mt-2 space-y-2">
          <input type="password" placeholder="Current password" value={cur} onChange={e => setCur(e.target.value)} className={inputCls} autoFocus />
          <input type="password" placeholder="New password" value={nw} onChange={e => setNw(e.target.value)} className={inputCls} />
          <input type="password" placeholder="Confirm new password" value={cf} onChange={e => setCf(e.target.value)} onKeyDown={e => { if (e.key === 'Enter') submit(); }} className={inputCls} />
          {err && <p className="text-red-400 text-xs">{err}</p>}
          <button onClick={submit} disabled={busy || !cur || !nw} className="w-full px-3 py-1.5 text-sm rounded bg-[var(--primary-color)] text-white hover:opacity-90 disabled:opacity-40">
            Change password
          </button>
        </div>
      )}
    </div>
  );
}

export function SecurityButton({ profileId }: { profileId: number }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="relative" data-no-drag>
      <button
        onClick={() => setOpen(o => !o)}
        title="Passwords & Security"
        className="p-1.5 rounded-lg text-white/60 hover:bg-white/10 hover:text-white transition-all cursor-pointer"
      >
        <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z" /></svg>
      </button>
      <RightDockPanel
        id="security"
        open={open}
        title="Passwords & Security"
        subtitle="Change a password (you must know the current one — there's no recovery)."
        onClose={() => setOpen(false)}
      >
        <div className="px-1">
          {FEATURES.map(f => <ChangeRow key={f.key} feature={f} profileId={profileId} />)}
        </div>
      </RightDockPanel>
    </div>
  );
}

export default SecurityButton;
