import { useState, useEffect, useRef } from 'react';
import { createPortal } from 'react-dom';
import { invoke } from '../lib/tauri';

// Types matching Rust structs
interface Profile {
  id: number | null;
  name: string;
  icon: string | null;
  created_at: string;
  is_active: boolean;
}

interface ProfileManagerProps {
  onProfileChange?: (profile: Profile) => void;
}

// Available profile icons
const PROFILE_ICONS = ['user', 'star', 'heart', 'bolt', 'shield', 'globe', 'code', 'book'];

export function ProfileManager({ onProfileChange }: ProfileManagerProps) {
  const [profiles, setProfiles] = useState<Profile[]>([]);
  const [activeProfile, setActiveProfile] = useState<Profile | null>(null);
  const [isOpen, setIsOpen] = useState(false);
  const [isCreating, setIsCreating] = useState(false);
  const [newProfileName, setNewProfileName] = useState('');
  const [newProfileIcon, setNewProfileIcon] = useState('user');
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [dropdownPosition, setDropdownPosition] = useState({ top: 0, right: 0 });
  const buttonRef = useRef<HTMLButtonElement>(null);

  // Load profiles on mount
  useEffect(() => {
    loadProfiles();
  }, []);

  const loadProfiles = async () => {
    try {
      setIsLoading(true);
      const [profileList, active] = await Promise.all([
        invoke<Profile[]>('get_profiles'),
        invoke<Profile | null>('get_active_profile'),
      ]);
      setProfiles(profileList);
      setActiveProfile(active);
      setError(null);
    } catch (err) {
      setError(`Failed to load profiles: ${err}`);
    } finally {
      setIsLoading(false);
    }
  };

  const handleCreateProfile = async () => {
    if (!newProfileName.trim()) {
      setError('Profile name is required');
      return;
    }

    try {
      const profile = await invoke<Profile>('create_profile', {
        name: newProfileName.trim(),
        icon: newProfileIcon,
      });
      setProfiles([...profiles, profile]);
      setNewProfileName('');
      setNewProfileIcon('user');
      setIsCreating(false);
      setError(null);
    } catch (err) {
      setError(`Failed to create profile: ${err}`);
    }
  };

  const handleSwitchProfile = async (profileId: number) => {
    try {
      const profile = await invoke<Profile>('switch_profile', { profileId });
      setActiveProfile(profile);
      setProfiles(profiles.map(p => ({
        ...p,
        is_active: p.id === profileId,
      })));
      setIsOpen(false);
      onProfileChange?.(profile);
    } catch (err) {
      setError(`Failed to switch profile: ${err}`);
    }
  };

  const handleDeleteProfile = async (profileId: number) => {
    if (profiles.length <= 1) {
      setError('Cannot delete the only profile');
      return;
    }

    const confirmed = window.confirm(
      'Are you sure you want to delete this profile? All associated data will be permanently deleted.'
    );

    if (!confirmed) return;

    try {
      await invoke('delete_profile', { profileId });
      setProfiles(profiles.filter(p => p.id !== profileId));
      if (activeProfile?.id === profileId) {
        loadProfiles(); // Reload to get new active profile
      }
      setError(null);
    } catch (err) {
      setError(`Failed to delete profile: ${err}`);
    }
  };

  const getIconEmoji = (icon: string | null): string => {
    const iconMap: Record<string, string> = {
      user: '\u{1F464}',
      star: '\u{2B50}',
      heart: '\u{2764}\u{FE0F}',
      bolt: '\u{26A1}',
      shield: '\u{1F6E1}\u{FE0F}',
      globe: '\u{1F310}',
      code: '\u{1F4BB}',
      book: '\u{1F4DA}',
    };
    return iconMap[icon || 'user'] || '\u{1F464}';
  };

  if (isLoading) {
    return (
      <div className="relative">
        <button
          disabled
          className="flex items-center gap-2 px-3 py-2 rounded-lg bg-white/5 border border-white/10 text-gray-400"
        >
          <span className="animate-pulse">Loading...</span>
        </button>
      </div>
    );
  }

  const handleOpenDropdown = () => {
    if (buttonRef.current) {
      const rect = buttonRef.current.getBoundingClientRect();
      setDropdownPosition({
        top: rect.bottom + 8,
        right: window.innerWidth - rect.right,
      });
    }
    setIsOpen(!isOpen);
  };

  return (
    <div className="relative">
      {/* Profile Button */}
      <button
        ref={buttonRef}
        onClick={handleOpenDropdown}
        className="flex items-center gap-2 px-3 py-2 rounded-lg bg-white/5 border border-white/10 hover:border-earth-teal transition-all"
      >
        <span className="text-lg">{getIconEmoji(activeProfile?.icon ?? null)}</span>
        <span className="text-sm text-white">{activeProfile?.name || 'Select Profile'}</span>
        <svg
          className={`w-4 h-4 text-gray-400 transition-transform ${isOpen ? 'rotate-180' : ''}`}
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
        >
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
        </svg>
      </button>

      {/* Dropdown - rendered via portal to escape overflow hidden */}
      {isOpen && createPortal(
        <div
          className="fixed w-72 bg-gray-900/95 border border-white/10 rounded-xl shadow-xl backdrop-blur-md z-[9999]"
          style={{ top: dropdownPosition.top, right: dropdownPosition.right }}
        >
          {/* Error Message */}
          {error && (
            <div className="px-4 py-2 bg-red-500/20 text-red-400 text-sm border-b border-white/10">
              {error}
            </div>
          )}

          {/* Profile List */}
          <div className="max-h-64 overflow-y-auto">
            {profiles.map((profile) => (
              <div
                key={profile.id}
                className={`flex items-center justify-between px-4 py-3 hover:bg-white/5 transition-colors ${
                  profile.is_active ? 'bg-earth-teal/10 border-l-2 border-earth-teal' : ''
                }`}
              >
                <button
                  onClick={() => profile.id && handleSwitchProfile(profile.id)}
                  className="flex items-center gap-3 flex-1 text-left"
                >
                  <span className="text-xl">{getIconEmoji(profile.icon)}</span>
                  <div>
                    <div className="text-white font-medium">{profile.name}</div>
                    {profile.is_active && (
                      <div className="text-xs text-earth-teal">Active</div>
                    )}
                  </div>
                </button>
                {!profile.is_active && profiles.length > 1 && (
                  <button
                    onClick={() => profile.id && handleDeleteProfile(profile.id)}
                    className="p-1 text-gray-500 hover:text-red-400 transition-colors"
                    title="Delete profile"
                  >
                    <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        strokeWidth={2}
                        d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"
                      />
                    </svg>
                  </button>
                )}
              </div>
            ))}
          </div>

          {/* Create New Profile */}
          <div className="border-t border-white/10">
            {isCreating ? (
              <div className="p-4 space-y-3">
                <input
                  type="text"
                  placeholder="Profile name"
                  value={newProfileName}
                  onChange={(e) => setNewProfileName(e.target.value)}
                  onKeyDown={(e) => e.key === 'Enter' && handleCreateProfile()}
                  className="w-full px-3 py-2 bg-white/5 border border-white/10 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-earth-teal"
                  autoFocus
                />
                <div className="flex gap-2 flex-wrap">
                  {PROFILE_ICONS.map((icon) => (
                    <button
                      key={icon}
                      onClick={() => setNewProfileIcon(icon)}
                      className={`p-2 rounded-lg transition-colors ${
                        newProfileIcon === icon
                          ? 'bg-earth-teal/20 ring-2 ring-earth-teal'
                          : 'bg-white/5 hover:bg-white/10'
                      }`}
                    >
                      {getIconEmoji(icon)}
                    </button>
                  ))}
                </div>
                <div className="flex gap-2">
                  <button
                    onClick={handleCreateProfile}
                    className="flex-1 px-3 py-2 bg-earth-teal text-white rounded-lg hover:opacity-90 transition-opacity"
                  >
                    Create
                  </button>
                  <button
                    onClick={() => {
                      setIsCreating(false);
                      setNewProfileName('');
                      setError(null);
                    }}
                    className="px-3 py-2 text-gray-400 hover:text-white transition-colors"
                  >
                    Cancel
                  </button>
                </div>
              </div>
            ) : (
              <button
                onClick={() => setIsCreating(true)}
                className="w-full px-4 py-3 text-left text-earth-teal hover:bg-white/5 transition-colors flex items-center gap-2"
              >
                <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
                </svg>
                Create New Profile
              </button>
            )}
          </div>
        </div>,
        document.body
      )}

      {/* Click outside to close */}
      {isOpen && createPortal(
        <div
          className="fixed inset-0 z-[9998]"
          onClick={() => {
            setIsOpen(false);
            setIsCreating(false);
            setError(null);
          }}
        />,
        document.body
      )}
    </div>
  );
}

export default ProfileManager;
