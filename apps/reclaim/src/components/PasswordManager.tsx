// Password Manager Component for Reclaim
// Securely stores and manages passwords per profile

import { useState, useEffect } from 'react';
import { invoke, listen } from '../lib/tauri';
import { RightDockPanel, RIGHT_DOCK_WIDTH_WIDE } from '../lib/rightDock';

export interface PasswordEntry {
  id: number;
  profile_id: number;
  title: string;
  username: string;
  password: string; // Encrypted in production
  url: string | null;
  notes: string | null;
  category: string;
  created_at: string;
  updated_at: string;
}

interface PasswordManagerProps {
  profileId: number;
  isOpen: boolean;
  onClose: () => void;
}

export function PasswordManager({ profileId, isOpen, onClose }: PasswordManagerProps) {
  const [entries, setEntries] = useState<PasswordEntry[]>([]);
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedCategory, setSelectedCategory] = useState<string | null>(null);
  const [isUnlocked, setIsUnlocked] = useState(false);
  const [masterPassword, setMasterPassword] = useState('');
  const [showMaster, setShowMaster] = useState(false);
  const [hasMasterPassword, setHasMasterPassword] = useState(false);
  const [showAddModal, setShowAddModal] = useState(false);
  const [editingEntry, setEditingEntry] = useState<PasswordEntry | null>(null);
  const [copiedField, setCopiedField] = useState<string | null>(null);

  useEffect(() => {
    if (isOpen) {
      checkMasterPassword();
    }
  }, [isOpen, profileId]);

  // Refresh the list when the panel opens while unlocked — an autosave (or another
  // tab) may have added an entry while it was closed, leaving the list stale.
  useEffect(() => {
    if (isOpen && isUnlocked) loadEntries();
  }, [isOpen, isUnlocked, profileId]);

  // Live refresh when a password is saved anywhere (e.g. the autosave prompt),
  // so an already-open panel updates immediately.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen('password-saved', () => { loadEntries(); }).then(u => { unlisten = u; });
    return () => unlisten?.();
  }, [profileId]);

  const checkMasterPassword = async () => {
    try {
      const hasPass = await invoke<boolean>('has_password_manager_master', { profileId: profileId });
      setHasMasterPassword(hasPass);
      if (!hasPass) {
        setIsUnlocked(false);
      }
    } catch {
      setHasMasterPassword(false);
    }
  };

  const unlock = async () => {
    try {
      const valid = await invoke<boolean>('verify_password_manager_master', {
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
      await invoke('set_password_manager_master', {
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
      const data = await invoke<PasswordEntry[]>('get_password_entries', { profileId: profileId });
      setEntries(data);
    } catch (err) {
      console.error('Failed to load password entries:', err);
    }
  };

  const deleteEntry = async (entryId: number) => {
    if (!confirm('Are you sure you want to delete this password entry?')) return;
    try {
      await invoke('delete_password_entry', { entryId });
      loadEntries();
    } catch (err) {
      console.error('Failed to delete entry:', err);
    }
  };

  const copyToClipboard = async (text: string, field: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopiedField(field);
      setTimeout(() => setCopiedField(null), 2000);
    } catch (err) {
      console.error('Failed to copy:', err);
    }
  };

  const categories = [...new Set(entries.map(e => e.category))];
  const filteredEntries = entries.filter(e => {
    const matchesSearch = !searchQuery ||
      e.title.toLowerCase().includes(searchQuery.toLowerCase()) ||
      e.username.toLowerCase().includes(searchQuery.toLowerCase()) ||
      (e.url && e.url.toLowerCase().includes(searchQuery.toLowerCase()));
    const matchesCategory = !selectedCategory || e.category === selectedCategory;
    return matchesSearch && matchesCategory;
  });

  return (
    <>
    <RightDockPanel id="password-manager" open={isOpen} width={RIGHT_DOCK_WIDTH_WIDE} title="Password Manager" onClose={onClose}>

        {/* Content */}
        {!isUnlocked ? (
          <div className="flex-1 flex items-center justify-center">
            <div className="w-full max-w-sm p-6">
              <div className="text-center mb-6">
                <svg className="w-16 h-16 mx-auto text-[var(--primary-color)] mb-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
                </svg>
                <h3 className="text-xl font-semibold text-[var(--text-color)]">
                  {hasMasterPassword ? 'Unlock Password Manager' : 'Set Master Password'}
                </h3>
                <p className="text-sm text-gray-400 mt-2">
                  {hasMasterPassword
                    ? 'Enter your master password to access saved passwords'
                    : 'Create a master password to secure your passwords (min 8 characters)'}
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
          <div className="flex flex-1 overflow-hidden">
            {/* Sidebar */}
            <div className="w-48 border-r border-gray-700 p-3 overflow-y-auto">
              <div className="mb-3">
                <input
                  type="text"
                  placeholder="Search..."
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                  className="w-full px-3 py-2 bg-gray-800 border border-gray-600 rounded text-sm focus:outline-none focus:border-[var(--primary-color)]"
                />
              </div>

              <button
                onClick={() => setSelectedCategory(null)}
                className={`w-full px-3 py-2 text-left text-sm rounded mb-1 transition-colors ${
                  selectedCategory === null ? 'bg-[var(--primary-color)]/20 text-[var(--primary-color)]' : 'hover:bg-gray-700'
                }`}
              >
                All Passwords ({entries.length})
              </button>

              <div className="mt-3 mb-2">
                <span className="text-xs text-gray-400 uppercase">Categories</span>
              </div>

              {categories.map(category => (
                <button
                  key={category}
                  onClick={() => setSelectedCategory(category)}
                  className={`w-full px-3 py-2 text-left text-sm rounded mb-1 transition-colors ${
                    selectedCategory === category ? 'bg-[var(--primary-color)]/20 text-[var(--primary-color)]' : 'hover:bg-gray-700'
                  }`}
                >
                  {category} ({entries.filter(e => e.category === category).length})
                </button>
              ))}

              <button
                onClick={() => setShowAddModal(true)}
                className="w-full mt-4 px-3 py-2 bg-[var(--primary-color)]/20 text-[var(--primary-color)] rounded text-sm hover:bg-[var(--primary-color)]/30 transition-colors flex items-center justify-center gap-2"
              >
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
                </svg>
                Add Password
              </button>
            </div>

            {/* Main content */}
            <div className="flex-1 p-4 overflow-y-auto">
              <div className="space-y-2">
                {filteredEntries.map(entry => (
                  <PasswordEntryCard
                    key={entry.id}
                    entry={entry}
                    copiedField={copiedField}
                    onCopy={copyToClipboard}
                    onEdit={() => setEditingEntry(entry)}
                    onDelete={() => deleteEntry(entry.id)}
                  />
                ))}

                {filteredEntries.length === 0 && (
                  <div className="text-center py-12 text-gray-400">
                    {searchQuery ? 'No passwords match your search.' : 'No passwords saved yet.'}
                  </div>
                )}
              </div>
            </div>
          </div>
        )}
        {/* Add/Edit form — rendered INSIDE the dock panel (which insets the page),
            so the native page surface can't cover it like a full-screen modal would. */}
        {(showAddModal || editingEntry) && (
          <PasswordEntryModal
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
    </RightDockPanel>
    </>
  );
}

// Password Entry Card Component
function PasswordEntryCard({
  entry,
  copiedField,
  onCopy,
  onEdit,
  onDelete,
}: {
  entry: PasswordEntry;
  copiedField: string | null;
  onCopy: (text: string, field: string) => void;
  onEdit: () => void;
  onDelete: () => void;
}) {
  const getCategoryIcon = (category: string) => {
    switch (category.toLowerCase()) {
      case 'social':
        return (
          <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M17 20h5v-2a3 3 0 00-5.356-1.857M17 20H7m10 0v-2c0-.656-.126-1.283-.356-1.857M7 20H2v-2a3 3 0 015.356-1.857M7 20v-2c0-.656.126-1.283.356-1.857m0 0a5.002 5.002 0 019.288 0M15 7a3 3 0 11-6 0 3 3 0 016 0zm6 3a2 2 0 11-4 0 2 2 0 014 0zM7 10a2 2 0 11-4 0 2 2 0 014 0z" />
          </svg>
        );
      case 'finance':
        return (
          <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 10h18M7 15h1m4 0h1m-7 4h12a3 3 0 003-3V8a3 3 0 00-3-3H6a3 3 0 00-3 3v8a3 3 0 003 3z" />
          </svg>
        );
      case 'work':
        return (
          <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 13.255A23.931 23.931 0 0112 15c-3.183 0-6.22-.62-9-1.745M16 6V4a2 2 0 00-2-2h-4a2 2 0 00-2 2v2m4 6h.01M5 20h14a2 2 0 002-2V8a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
          </svg>
        );
      default:
        return (
          <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 7a2 2 0 012 2m4 0a6 6 0 01-7.743 5.743L11 17H9v2H7v2H4a1 1 0 01-1-1v-2.586a1 1 0 01.293-.707l5.964-5.964A6 6 0 1121 9z" />
          </svg>
        );
    }
  };

  return (
    <div className="flex items-center gap-3 p-3 bg-gray-800/50 hover:bg-gray-700/50 rounded-lg transition-colors group">
      <div className="w-10 h-10 flex items-center justify-center bg-[var(--primary-color)]/20 text-[var(--primary-color)] rounded-lg">
        {getCategoryIcon(entry.category)}
      </div>

      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2">
          <span className="font-medium text-[var(--text-color)] truncate">{entry.title}</span>
          <span className="text-xs text-gray-500 bg-gray-700 px-1.5 py-0.5 rounded">{entry.category}</span>
        </div>
        <div className="text-sm text-gray-400 truncate">{entry.username}</div>
        {entry.url && (
          <div className="text-xs text-gray-500 break-all">{entry.url}</div>
        )}
      </div>

      <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
        {/* Copy Username */}
        <button
          onClick={() => onCopy(entry.username, `username-${entry.id}`)}
          className="p-1.5 hover:bg-gray-600 rounded transition-colors"
          title="Copy username"
        >
          {copiedField === `username-${entry.id}` ? (
            <svg className="w-4 h-4 text-green-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
            </svg>
          ) : (
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M16 7a4 4 0 11-8 0 4 4 0 018 0zM12 14a7 7 0 00-7 7h14a7 7 0 00-7-7z" />
            </svg>
          )}
        </button>

        {/* Copy Password */}
        <button
          onClick={() => onCopy(entry.password, `password-${entry.id}`)}
          className="p-1.5 hover:bg-gray-600 rounded transition-colors"
          title="Copy password"
        >
          {copiedField === `password-${entry.id}` ? (
            <svg className="w-4 h-4 text-green-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
            </svg>
          ) : (
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 7a2 2 0 012 2m4 0a6 6 0 01-7.743 5.743L11 17H9v2H7v2H4a1 1 0 01-1-1v-2.586a1 1 0 01.293-.707l5.964-5.964A6 6 0 1121 9z" />
            </svg>
          )}
        </button>

        {/* Edit */}
        <button onClick={onEdit} className="p-1.5 hover:bg-gray-600 rounded transition-colors" title="Edit">
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
          </svg>
        </button>

        {/* Delete */}
        <button onClick={onDelete} className="p-1.5 hover:bg-red-600/50 rounded transition-colors" title="Delete">
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
          </svg>
        </button>
      </div>
    </div>
  );
}

// Password Entry Modal for Add/Edit
function PasswordEntryModal({
  profileId,
  entry,
  onClose,
  onSave,
}: {
  profileId: number;
  entry: PasswordEntry | null;
  onClose: () => void;
  onSave: () => void;
}) {
  const [title, setTitle] = useState(entry?.title || '');
  const [username, setUsername] = useState(entry?.username || '');
  const [password, setPassword] = useState(entry?.password || '');
  const [url, setUrl] = useState(entry?.url || '');
  const [notes, setNotes] = useState(entry?.notes || '');
  const [category, setCategory] = useState(entry?.category || 'General');
  const [showPassword, setShowPassword] = useState(false);

  const generatePassword = () => {
    const chars = 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!@#$%^&*()_+-=[]{}|;:,.<>?';
    let newPassword = '';
    for (let i = 0; i < 20; i++) {
      newPassword += chars.charAt(Math.floor(Math.random() * chars.length));
    }
    setPassword(newPassword);
    setShowPassword(true);
  };

  const handleSave = async () => {
    if (!title.trim() || !username.trim() || !password.trim()) {
      alert('Please fill in title, username, and password');
      return;
    }

    try {
      if (entry) {
        await invoke('update_password_entry', {
          entryId: entry.id,
          title: title.trim(),
          username: username.trim(),
          password: password.trim(),
          url: url.trim() || null,
          notes: notes.trim() || null,
          category,
        });
      } else {
        await invoke('add_password_entry', {
          profileId,
          title: title.trim(),
          username: username.trim(),
          password: password.trim(),
          url: url.trim() || null,
          notes: notes.trim() || null,
          category,
        });
      }
      onSave();
    } catch (err) {
      console.error('Failed to save password entry:', err);
    }
  };

  return (
    <div className="absolute inset-0 z-[60] flex items-center justify-center bg-black/80 rounded-lg p-2">
      <div className="w-full max-h-full overflow-y-auto bg-gray-900 border border-gray-700 rounded-lg shadow-2xl">
        <div className="flex items-center justify-between p-4 border-b border-gray-700">
          <h3 className="text-lg font-semibold text-[var(--text-color)]">
            {entry ? 'Edit Password' : 'Add Password'}
          </h3>
          <button onClick={onClose} className="p-1 hover:bg-gray-700 rounded transition-colors">
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        <div className="p-4 space-y-4">
          <div>
            <label className="block text-sm text-gray-400 mb-1">Title *</label>
            <input
              type="text"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              className="w-full px-3 py-2 bg-gray-800 border border-gray-600 rounded focus:outline-none focus:border-[var(--primary-color)]"
              placeholder="e.g., Google Account"
            />
          </div>

          <div>
            <label className="block text-sm text-gray-400 mb-1">Username/Email *</label>
            <input
              type="text"
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              className="w-full px-3 py-2 bg-gray-800 border border-gray-600 rounded focus:outline-none focus:border-[var(--primary-color)]"
              placeholder="user@example.com"
            />
          </div>

          <div>
            <label className="block text-sm text-gray-400 mb-1">Password *</label>
            <div className="flex gap-2">
              <div className="flex-1 relative">
                <input
                  type={showPassword ? 'text' : 'password'}
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  className="w-full px-3 py-2 pr-10 bg-gray-800 border border-gray-600 rounded focus:outline-none focus:border-[var(--primary-color)]"
                  placeholder="Enter password"
                />
                <button
                  type="button"
                  onClick={() => setShowPassword(!showPassword)}
                  className="absolute right-2 top-1/2 -translate-y-1/2 p-1 text-gray-400 hover:text-white"
                >
                  {showPassword ? (
                    <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13.875 18.825A10.05 10.05 0 0112 19c-4.478 0-8.268-2.943-9.543-7a9.97 9.97 0 011.563-3.029m5.858.908a3 3 0 114.243 4.243M9.878 9.878l4.242 4.242M9.88 9.88l-3.29-3.29m7.532 7.532l3.29 3.29M3 3l3.59 3.59m0 0A9.953 9.953 0 0112 5c4.478 0 8.268 2.943 9.543 7a10.025 10.025 0 01-4.132 5.411m0 0L21 21" />
                    </svg>
                  ) : (
                    <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
                    </svg>
                  )}
                </button>
              </div>
              <button
                type="button"
                onClick={generatePassword}
                className="px-3 py-2 bg-[var(--primary-color)]/20 text-[var(--primary-color)] rounded hover:bg-[var(--primary-color)]/30 transition-colors text-sm"
                title="Generate password"
              >
                Generate
              </button>
            </div>
          </div>

          <div>
            <label className="block text-sm text-gray-400 mb-1">Website URL</label>
            <input
              type="text"
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              className="w-full px-3 py-2 bg-gray-800 border border-gray-600 rounded focus:outline-none focus:border-[var(--primary-color)]"
              placeholder="https://example.com"
            />
          </div>

          <div>
            <label className="block text-sm text-gray-400 mb-1">Category</label>
            <select
              value={category}
              onChange={(e) => setCategory(e.target.value)}
              className="w-full px-3 py-2 bg-gray-800 border border-gray-600 rounded focus:outline-none focus:border-[var(--primary-color)]"
            >
              <option value="General">General</option>
              <option value="Social">Social</option>
              <option value="Finance">Finance</option>
              <option value="Work">Work</option>
              <option value="Shopping">Shopping</option>
              <option value="Entertainment">Entertainment</option>
            </select>
          </div>

          <div>
            <label className="block text-sm text-gray-400 mb-1">Notes</label>
            <textarea
              value={notes}
              onChange={(e) => setNotes(e.target.value)}
              className="w-full px-3 py-2 bg-gray-800 border border-gray-600 rounded focus:outline-none focus:border-[var(--primary-color)] resize-none"
              rows={3}
              placeholder="Optional notes..."
            />
          </div>
        </div>

        <div className="flex justify-end gap-2 p-4 border-t border-gray-700">
          <button onClick={onClose} className="px-4 py-2 text-sm hover:bg-gray-700 rounded transition-colors">
            Cancel
          </button>
          <button
            onClick={handleSave}
            className="px-4 py-2 text-sm bg-[var(--primary-color)] hover:opacity-90 rounded transition-opacity"
          >
            {entry ? 'Save Changes' : 'Add Password'}
          </button>
        </div>
      </div>
    </div>
  );
}

export default PasswordManager;
