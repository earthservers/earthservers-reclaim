// OTP Authenticator Component for Reclaim
// Stores and generates TOTP codes for 2FA

import { useState, useEffect, useCallback } from 'react';
import { invoke } from '../lib/tauri';
import { RightDockPanel } from '../lib/rightDock';

export interface OTPEntry {
  id: number;
  profile_id: number;
  name: string;
  issuer: string;
  secret: string; // Base32 encoded secret
  algorithm: 'SHA1' | 'SHA256' | 'SHA512';
  digits: number;
  period: number;
  created_at: string;
}

interface OTPAuthenticatorProps {
  profileId: number;
  isOpen: boolean;
  onClose: () => void;
}

// RFC 4648 base32 decode (TOTP secrets are base32). Ignores padding/whitespace
// and any non-alphabet characters.
function base32Decode(input: string) {
  const alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567';
  const clean = input.replace(/=+$/, '').replace(/\s+/g, '').toUpperCase();
  let bits = 0;
  let value = 0;
  const out: number[] = [];
  for (const ch of clean) {
    const idx = alphabet.indexOf(ch);
    if (idx === -1) continue;
    value = (value << 5) | idx;
    bits += 5;
    if (bits >= 8) {
      bits -= 8;
      out.push((value >>> bits) & 0xff);
    }
  }
  return new Uint8Array(out);
}

// Proper RFC 6238 TOTP: HMAC-SHA1/256/512 over the time counter, with dynamic
// truncation. Async because it uses the Web Crypto SubtleCrypto HMAC. Interops
// with Google/Microsoft Authenticator and friends.
async function generateTOTP(
  secret: string,
  algorithm: string = 'SHA1',
  digits: number = 6,
  period: number = 30,
): Promise<string> {
  const key = base32Decode(secret);
  if (key.length === 0) return '•'.repeat(digits);

  const counter = Math.floor(Date.now() / 1000 / period);
  // 8-byte big-endian counter.
  const buf = new ArrayBuffer(8);
  const view = new DataView(buf);
  view.setUint32(0, Math.floor(counter / 2 ** 32));
  view.setUint32(4, counter >>> 0);

  const hashName = { SHA1: 'SHA-1', SHA256: 'SHA-256', SHA512: 'SHA-512' }[algorithm] || 'SHA-1';
  const cryptoKey = await crypto.subtle.importKey(
    'raw',
    key,
    { name: 'HMAC', hash: hashName },
    false,
    ['sign'],
  );
  const hmac = new Uint8Array(await crypto.subtle.sign('HMAC', cryptoKey, buf));

  // Dynamic truncation (RFC 4226 §5.3).
  const offset = hmac[hmac.length - 1] & 0x0f;
  const binCode =
    ((hmac[offset] & 0x7f) << 24) |
    (hmac[offset + 1] << 16) |
    (hmac[offset + 2] << 8) |
    hmac[offset + 3];
  return (binCode % 10 ** digits).toString().padStart(digits, '0');
}

export function OTPAuthenticator({ profileId, isOpen, onClose }: OTPAuthenticatorProps) {
  const [entries, setEntries] = useState<OTPEntry[]>([]);
  const [isUnlocked, setIsUnlocked] = useState(false);
  const [masterPassword, setMasterPassword] = useState('');
  const [showMaster, setShowMaster] = useState(false);
  const [hasMasterPassword, setHasMasterPassword] = useState(false);
  const [showAddModal, setShowAddModal] = useState(false);
  const [editingEntry, setEditingEntry] = useState<OTPEntry | null>(null);
  const [searchQuery, setSearchQuery] = useState('');
  const [currentTime, setCurrentTime] = useState(Date.now());
  const [copiedId, setCopiedId] = useState<number | null>(null);
  // Current TOTP code per entry id. Recomputed (async HMAC) as the time window
  // advances, so render stays synchronous.
  const [codes, setCodes] = useState<Record<number, string>>({});

  // Update time every second for TOTP countdown
  useEffect(() => {
    const interval = setInterval(() => {
      setCurrentTime(Date.now());
    }, 1000);
    return () => clearInterval(interval);
  }, []);

  // Recompute codes whenever the entries or the 1s tick change. HMAC over a
  // handful of entries is cheap; the codes only actually change on period
  // boundaries.
  const tick = Math.floor(currentTime / 1000);
  useEffect(() => {
    let cancelled = false;
    (async () => {
      const next: Record<number, string> = {};
      for (const e of entries) {
        try {
          next[e.id] = await generateTOTP(e.secret, e.algorithm, e.digits, e.period);
        } catch {
          next[e.id] = '•'.repeat(e.digits);
        }
      }
      if (!cancelled) setCodes(next);
    })();
    return () => { cancelled = true; };
  }, [entries, tick]);

  useEffect(() => {
    if (isOpen) {
      checkMasterPassword();
    }
  }, [isOpen, profileId]);

  const checkMasterPassword = async () => {
    try {
      const hasPass = await invoke<boolean>('has_otp_master', { profileId: profileId });
      setHasMasterPassword(hasPass);
    } catch {
      setHasMasterPassword(false);
    }
  };

  const unlock = async () => {
    try {
      const valid = await invoke<boolean>('verify_otp_master', {
        profileId,
        password: masterPassword,
      });
      if (valid) {
        setIsUnlocked(true);
        setMasterPassword('');
        loadEntries();
      } else {
        alert('Incorrect master password');
      }
    } catch {
      setIsUnlocked(true);
      loadEntries();
    }
  };

  const setNewMasterPassword = async () => {
    if (masterPassword.length < 8) {
      alert('Master password must be at least 8 characters');
      return;
    }
    try {
      await invoke('set_otp_master', {
        profileId,
        password: masterPassword,
      });
      setHasMasterPassword(true);
      setIsUnlocked(true);
      setMasterPassword('');
      loadEntries();
    } catch (err) {
      console.error('Failed to set master password:', err);
    }
  };

  const loadEntries = async () => {
    try {
      const data = await invoke<OTPEntry[]>('get_otp_entries', { profileId: profileId });
      setEntries(data);
    } catch (err) {
      console.error('Failed to load OTP entries:', err);
    }
  };

  const deleteEntry = async (entryId: number) => {
    if (!confirm('Are you sure you want to delete this authenticator entry?')) return;
    try {
      await invoke('delete_otp_entry', { entryId });
      loadEntries();
    } catch (err) {
      console.error('Failed to delete entry:', err);
    }
  };

  const copyCode = async (code: string, id: number) => {
    try {
      await navigator.clipboard.writeText(code);
      setCopiedId(id);
      setTimeout(() => setCopiedId(null), 2000);
    } catch (err) {
      console.error('Failed to copy:', err);
    }
  };

  const filteredEntries = entries.filter(e =>
    e.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
    e.issuer.toLowerCase().includes(searchQuery.toLowerCase())
  );

  // Calculate time remaining in current period (default 30s)
  const getTimeRemaining = useCallback((period: number = 30) => {
    return period - (Math.floor(currentTime / 1000) % period);
  }, [currentTime]);

  return (
    <>
    <RightDockPanel id="authenticator" open={isOpen} title="Authenticator" onClose={onClose}>

        {/* Content */}
        {!isUnlocked ? (
          <div className="flex-1 flex items-center justify-center">
            <div className="w-full max-w-sm p-6">
              <div className="text-center mb-6">
                <svg className="w-16 h-16 mx-auto text-[var(--primary-color)] mb-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z" />
                </svg>
                <h3 className="text-xl font-semibold text-[var(--text-color)]">
                  {hasMasterPassword ? 'Unlock Authenticator' : 'Set Master Password'}
                </h3>
                <p className="text-sm text-gray-400 mt-2">
                  {hasMasterPassword
                    ? 'Enter your master password to access 2FA codes'
                    : 'Create a master password to secure your 2FA codes (min 8 characters)'}
                </p>
              </div>

              <div className="space-y-4">
                <div className="relative">
                  <input
                    type={showMaster ? 'text' : 'password'}
                    value={masterPassword}
                    onChange={(e) => setMasterPassword(e.target.value)}
                    onKeyDown={(e) => e.key === 'Enter' && (hasMasterPassword ? unlock() : setNewMasterPassword())}
                    className="w-full px-4 py-3 pr-11 bg-gray-800 border border-gray-600 rounded-lg focus:outline-none focus:border-[var(--primary-color)]"
                    placeholder={hasMasterPassword ? 'Master password' : 'Create master password'}
                    autoFocus
                  />
                  <button type="button" onClick={() => setShowMaster(s => !s)} className="absolute right-3 top-1/2 -translate-y-1/2 text-gray-400 hover:text-white" title={showMaster ? 'Hide password' : 'Show password'}>
                    {showMaster ? (
                      <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13.875 18.825A10.05 10.05 0 0112 19c-4.478 0-8.268-2.943-9.543-7a9.97 9.97 0 011.563-3.029m5.858.908a3 3 0 114.243 4.243M9.878 9.878l4.242 4.242M9.88 9.88l-3.29-3.29m7.532 7.532l3.29 3.29M3 3l18 18" /></svg>
                    ) : (
                      <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" /><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" /></svg>
                    )}
                  </button>
                </div>
                <button
                  onClick={hasMasterPassword ? unlock : setNewMasterPassword}
                  className="w-full px-4 py-3 bg-[var(--primary-color)] text-white rounded-lg hover:opacity-90 transition-opacity font-medium"
                >
                  {hasMasterPassword ? 'Unlock' : 'Create Password'}
                </button>
              </div>
            </div>
          </div>
        ) : (
          <div className="flex-1 flex flex-col overflow-hidden">
            {/* Search and Add */}
            <div className="flex items-center gap-3 p-4 border-b border-gray-700">
              <div className="flex-1">
                <input
                  type="text"
                  placeholder="Search accounts..."
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                  className="w-full px-3 py-2 bg-gray-800 border border-gray-600 rounded-lg text-sm focus:outline-none focus:border-[var(--primary-color)]"
                />
              </div>
              <button
                onClick={() => setShowAddModal(true)}
                className="px-4 py-2 bg-[var(--primary-color)] text-white rounded-lg hover:opacity-90 transition-opacity text-sm flex items-center gap-2"
              >
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
                </svg>
                Add Account
              </button>
            </div>

            {/* OTP List */}
            <div className="flex-1 overflow-y-auto p-4">
              <div className="space-y-3">
                {filteredEntries.map(entry => {
                  const code = codes[entry.id] ?? '•'.repeat(entry.digits);
                  const timeRemaining = getTimeRemaining(entry.period);
                  const isLow = timeRemaining <= 5;

                  return (
                    <div
                      key={entry.id}
                      className="flex items-center gap-4 p-4 bg-gray-800/50 hover:bg-gray-700/50 rounded-lg transition-colors group"
                    >
                      {/* Icon/Avatar */}
                      <div className="w-12 h-12 flex items-center justify-center bg-[var(--primary-color)]/20 text-[var(--primary-color)] rounded-xl text-lg font-semibold">
                        {entry.issuer.charAt(0).toUpperCase()}
                      </div>

                      {/* Info */}
                      <div className="flex-1 min-w-0">
                        <div className="font-medium text-[var(--text-color)] truncate">{entry.issuer}</div>
                        <div className="text-sm text-gray-400 truncate">{entry.name}</div>
                      </div>

                      {/* Code */}
                      <div className="flex items-center gap-3">
                        <button
                          onClick={() => copyCode(code, entry.id)}
                          className="text-2xl font-mono font-bold tracking-wider text-[var(--text-color)] hover:text-[var(--primary-color)] transition-colors"
                        >
                          {copiedId === entry.id ? (
                            <span className="text-green-400 text-base flex items-center gap-1">
                              <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
                              </svg>
                              Copied
                            </span>
                          ) : (
                            <>
                              {code.slice(0, 3)} {code.slice(3)}
                            </>
                          )}
                        </button>

                        {/* Countdown */}
                        <div className={`w-10 h-10 flex items-center justify-center rounded-full border-2 ${
                          isLow ? 'border-red-500 text-red-400' : 'border-[var(--primary-color)] text-[var(--primary-color)]'
                        }`}>
                          <span className="text-sm font-medium">{timeRemaining}</span>
                        </div>
                      </div>

                      {/* Actions */}
                      <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                        <button
                          onClick={() => setEditingEntry(entry)}
                          className="p-1.5 hover:bg-gray-600 rounded transition-colors"
                          title="Edit"
                        >
                          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
                          </svg>
                        </button>
                        <button
                          onClick={() => deleteEntry(entry.id)}
                          className="p-1.5 hover:bg-red-600/50 rounded transition-colors"
                          title="Delete"
                        >
                          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                          </svg>
                        </button>
                      </div>
                    </div>
                  );
                })}

                {filteredEntries.length === 0 && (
                  <div className="text-center py-12 text-gray-400">
                    {searchQuery ? 'No accounts match your search.' : (
                      <div>
                        <svg className="w-16 h-16 mx-auto text-gray-600 mb-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z" />
                        </svg>
                        <p className="text-lg font-medium mb-2">No 2FA accounts yet</p>
                        <p className="text-sm">Add your first account to start generating codes</p>
                      </div>
                    )}
                  </div>
                )}
              </div>
            </div>
          </div>
        )}
    </RightDockPanel>

      {/* Add/Edit Modal */}
      {(showAddModal || editingEntry) && (
        <OTPEntryModal
          profileId={profileId}
          entry={editingEntry}
          onClose={() => {
            setShowAddModal(false);
            setEditingEntry(null);
          }}
          onSave={() => {
            setShowAddModal(false);
            setEditingEntry(null);
            loadEntries();
          }}
        />
      )}
    </>
  );
}

// OTP Entry Modal for Add/Edit
function OTPEntryModal({
  profileId,
  entry,
  onClose,
  onSave,
}: {
  profileId: number;
  entry: OTPEntry | null;
  onClose: () => void;
  onSave: () => void;
}) {
  const [name, setName] = useState(entry?.name || '');
  const [issuer, setIssuer] = useState(entry?.issuer || '');
  const [secret, setSecret] = useState(entry?.secret || '');
  const [algorithm, setAlgorithm] = useState<'SHA1' | 'SHA256' | 'SHA512'>(entry?.algorithm || 'SHA1');
  const [digits, setDigits] = useState(entry?.digits || 6);
  const [period, setPeriod] = useState(entry?.period || 30);

  const handleSave = async () => {
    if (!name.trim() || !issuer.trim() || !secret.trim()) {
      alert('Please fill in all required fields');
      return;
    }

    // Clean up the secret (remove spaces and dashes)
    const cleanSecret = secret.replace(/[\s-]/g, '').toUpperCase();

    try {
      if (entry) {
        await invoke('update_otp_entry', {
          entryId: entry.id,
          name: name.trim(),
          issuer: issuer.trim(),
          secret: cleanSecret,
          algorithm,
          digits,
          period,
        });
      } else {
        await invoke('add_otp_entry', {
          profileId,
          name: name.trim(),
          issuer: issuer.trim(),
          secret: cleanSecret,
          algorithm,
          digits,
          period,
        });
      }
      onSave();
    } catch (err) {
      console.error('Failed to save OTP entry:', err);
    }
  };

  return (
    <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/80">
      <div className="w-full max-w-md bg-gray-900 border border-gray-700 rounded-lg shadow-2xl">
        <div className="flex items-center justify-between p-4 border-b border-gray-700">
          <h3 className="text-lg font-semibold text-[var(--text-color)]">
            {entry ? 'Edit Account' : 'Add Account'}
          </h3>
          <button onClick={onClose} className="p-1 hover:bg-gray-700 rounded transition-colors">
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        <div className="p-4 space-y-4">
          <div>
            <label className="block text-sm text-gray-400 mb-1">Service/Issuer *</label>
            <input
              type="text"
              value={issuer}
              onChange={(e) => setIssuer(e.target.value)}
              className="w-full px-3 py-2 bg-gray-800 border border-gray-600 rounded focus:outline-none focus:border-[var(--primary-color)]"
              placeholder="e.g., Google, GitHub, Discord"
            />
          </div>

          <div>
            <label className="block text-sm text-gray-400 mb-1">Account Name *</label>
            <input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              className="w-full px-3 py-2 bg-gray-800 border border-gray-600 rounded focus:outline-none focus:border-[var(--primary-color)]"
              placeholder="e.g., user@example.com"
            />
          </div>

          <div>
            <label className="block text-sm text-gray-400 mb-1">Secret Key *</label>
            <input
              type="text"
              value={secret}
              onChange={(e) => setSecret(e.target.value)}
              className="w-full px-3 py-2 bg-gray-800 border border-gray-600 rounded focus:outline-none focus:border-[var(--primary-color)] font-mono"
              placeholder="JBSWY3DPEHPK3PXP"
            />
            <p className="text-xs text-gray-500 mt-1">
              Enter the secret key provided by the service (usually shown as a code or under manual setup)
            </p>
          </div>

          {/* Advanced Options */}
          <details className="group">
            <summary className="cursor-pointer text-sm text-gray-400 hover:text-white transition-colors">
              Advanced Options
            </summary>
            <div className="mt-3 space-y-3 pl-2 border-l-2 border-gray-700">
              <div className="grid grid-cols-3 gap-3">
                <div>
                  <label className="block text-xs text-gray-500 mb-1">Algorithm</label>
                  <select
                    value={algorithm}
                    onChange={(e) => setAlgorithm(e.target.value as 'SHA1' | 'SHA256' | 'SHA512')}
                    className="w-full px-2 py-1.5 bg-gray-800 border border-gray-600 rounded text-sm focus:outline-none focus:border-[var(--primary-color)]"
                  >
                    <option value="SHA1">SHA-1</option>
                    <option value="SHA256">SHA-256</option>
                    <option value="SHA512">SHA-512</option>
                  </select>
                </div>
                <div>
                  <label className="block text-xs text-gray-500 mb-1">Digits</label>
                  <select
                    value={digits}
                    onChange={(e) => setDigits(parseInt(e.target.value))}
                    className="w-full px-2 py-1.5 bg-gray-800 border border-gray-600 rounded text-sm focus:outline-none focus:border-[var(--primary-color)]"
                  >
                    <option value={6}>6</option>
                    <option value={8}>8</option>
                  </select>
                </div>
                <div>
                  <label className="block text-xs text-gray-500 mb-1">Period (sec)</label>
                  <select
                    value={period}
                    onChange={(e) => setPeriod(parseInt(e.target.value))}
                    className="w-full px-2 py-1.5 bg-gray-800 border border-gray-600 rounded text-sm focus:outline-none focus:border-[var(--primary-color)]"
                  >
                    <option value={30}>30</option>
                    <option value={60}>60</option>
                  </select>
                </div>
              </div>
            </div>
          </details>
        </div>

        <div className="flex justify-end gap-2 p-4 border-t border-gray-700">
          <button onClick={onClose} className="px-4 py-2 text-sm hover:bg-gray-700 rounded transition-colors">
            Cancel
          </button>
          <button
            onClick={handleSave}
            className="px-4 py-2 text-sm bg-[var(--primary-color)] hover:opacity-90 rounded transition-opacity"
          >
            {entry ? 'Save Changes' : 'Add Account'}
          </button>
        </div>
      </div>
    </div>
  );
}

export default OTPAuthenticator;
