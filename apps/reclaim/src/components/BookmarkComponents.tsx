import { useState, useEffect, useCallback, useRef } from 'react';
import { createPortal } from 'react-dom';
import { invoke } from '../lib/tauri';
import { RightDockPanel } from '../lib/rightDock';
import { VaultAutofill } from './VaultAutofill';

// Native button component using pointerdown (more reliable in WebKitGTK)
function NativeButton({
  onClick,
  className,
  title,
  children,
  draggable,
  onDragStart,
  onDragOver,
  onDrop,
  onDragEnd,
}: {
  onClick: () => void;
  className?: string;
  title?: string;
  children: React.ReactNode;
  draggable?: boolean;
  onDragStart?: (e: React.DragEvent) => void;
  onDragOver?: (e: React.DragEvent) => void;
  onDrop?: (e: React.DragEvent) => void;
  onDragEnd?: () => void;
}) {
  const buttonRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    const button = buttonRef.current;
    if (!button) return;

    // Use pointerdown - fires before click and is more reliable in WebKitGTK
    const handlePointerDown = (e: PointerEvent) => {
      // Don't trigger click on drag start
      if (e.button !== 0) return; // Only left click
      e.preventDefault();
      e.stopPropagation();
      e.stopImmediatePropagation();
      requestAnimationFrame(() => onClick());
    };

    const handleTouchStart = (e: TouchEvent) => {
      e.preventDefault();
      e.stopPropagation();
      requestAnimationFrame(() => onClick());
    };

    button.addEventListener('pointerdown', handlePointerDown, { capture: true });
    button.addEventListener('touchstart', handleTouchStart, { capture: true, passive: false });

    return () => {
      button.removeEventListener('pointerdown', handlePointerDown, { capture: true });
      button.removeEventListener('touchstart', handleTouchStart, { capture: true });
    };
  }, [onClick]);

  return (
    <button
      ref={buttonRef}
      type="button"
      className={className}
      title={title}
      draggable={draggable}
      onDragStart={onDragStart}
      onDragOver={onDragOver}
      onDrop={onDrop}
      onDragEnd={onDragEnd}
      style={{ pointerEvents: 'auto', WebkitUserSelect: 'none', userSelect: 'none' }}
    >
      {children}
    </button>
  );
}

// Types
export type BookmarkLocation = 'toolbar' | 'list' | 'private';

export interface Bookmark {
  id: number;
  profile_id: number;
  title: string;
  url: string;
  favicon: string | null;
  folder_id: number | null;
  folder_name: string | null;
  tags: string[];
  notes: string | null;
  position: number;
  location: BookmarkLocation;
  created_at: string;
  updated_at: string;
}

export interface BookmarkFolder {
  id: number;
  profile_id: number;
  name: string;
  parent_id: number | null;
  position: number;
  created_at: string;
  bookmark_count: number | null;
  show_in_toolbar?: boolean;
}

interface BookmarkBarProps {
  profileId: number;
  visible?: boolean;
  onNavigate?: (url: string) => void;
  onToggleManager?: () => void;
}

interface BookmarkManagerProps {
  profileId: number;
  isOpen: boolean;
  onClose: () => void;
  onNavigate?: (url: string) => void;
}

interface AddBookmarkModalProps {
  profileId: number;
  isOpen: boolean;
  onClose: () => void;
  initialUrl?: string;
  initialTitle?: string;
  onSave?: (bookmark: Bookmark) => void;
}

// ==================== BookmarkBar ====================
export function BookmarkBar({ profileId, visible = true, onNavigate, onToggleManager }: BookmarkBarProps) {
  const [bookmarks, setBookmarks] = useState<Bookmark[]>([]);
  const [folders, setFolders] = useState<BookmarkFolder[]>([]);
  const [allBookmarks, setAllBookmarks] = useState<Bookmark[]>([]);
  const [showBookmarkList, setShowBookmarkList] = useState(false);
  const [showPrivateBookmarks, setShowPrivateBookmarks] = useState(false);
  const [privateBookmarks, setPrivateBookmarks] = useState<Bookmark[]>([]);
  const [privateUnlocked, setPrivateUnlocked] = useState(false);
  const [passwordInput, setPasswordInput] = useState('');
  const [hasPassword, setHasPassword] = useState(false);
  const [openFolderId, setOpenFolderId] = useState<number | null>(null);

  // Refs for dropdown positioning
  const bookmarkListButtonRef = useRef<HTMLButtonElement>(null);
  const privateButtonRef = useRef<HTMLButtonElement>(null);
  const folderButtonRefs = useRef<Record<number, HTMLButtonElement | null>>({});
  const [folderDropdownPosition, setFolderDropdownPosition] = useState({ top: 0, left: 0 });

  // Drag and drop state
  const [draggedBookmark, setDraggedBookmark] = useState<Bookmark | null>(null);
  const [dropTarget, setDropTarget] = useState<{ type: 'bookmark' | 'folder'; id: number } | null>(null);
  const [showCreateFolderModal, setShowCreateFolderModal] = useState(false);
  const [folderCreateBookmarks, setFolderCreateBookmarks] = useState<Bookmark[]>([]);
  const [newFolderName, setNewFolderName] = useState('');

  // Mandatory EarthSearch bookmark - always shown first
  const earthSearchBookmark: Bookmark = {
    id: -1,
    profile_id: profileId,
    title: 'Search',
    url: 'earth://search',
    favicon: null,
    folder_id: null,
    folder_name: null,
    tags: ['home'],
    notes: 'Return to Search home',
    position: 0,
    location: 'toolbar',
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
  };

  useEffect(() => {
    if (visible) {
      loadBookmarks();
      loadFolders();
      checkPrivatePasswordStatus();
    }
  }, [profileId, visible]);

  const loadBookmarks = async () => {
    try {
      const data = await invoke<Bookmark[]>('get_all_bookmarks', { profileId: profileId });
      // Filter out earth:// URLs
      const userBookmarks = data.filter(b => !b.url.startsWith('earth://'));
      // Get toolbar bookmarks that are NOT in a folder
      const toolbarBookmarks = userBookmarks.filter(b => (b.location === 'toolbar' || !b.location) && !b.folder_id);
      setAllBookmarks(userBookmarks);
      setBookmarks(toolbarBookmarks.slice(0, 14));
      // Load private bookmarks separately
      const privateData = userBookmarks.filter(b => b.location === 'private');
      setPrivateBookmarks(privateData);
    } catch (err) {
      console.error('Failed to load bookmarks:', err);
    }
  };

  const loadFolders = async () => {
    try {
      const data = await invoke<BookmarkFolder[]>('get_bookmark_folders', { profileId: profileId });
      setFolders(data);
    } catch (err) {
      console.error('Failed to load folders:', err);
    }
  };

  const checkPrivatePasswordStatus = async () => {
    try {
      const hasPass = await invoke<boolean>('has_private_bookmarks_password', { profileId });
      setHasPassword(hasPass);
    } catch {
      setHasPassword(false);
    }
  };

  const unlockPrivateBookmarks = async () => {
    try {
      const valid = await invoke<boolean>('verify_private_bookmarks_password', { profileId, password: passwordInput });
      if (valid) {
        setPrivateUnlocked(true);
        setPasswordInput('');
      } else {
        alert('Incorrect password');
      }
    } catch {
      setPrivateUnlocked(true); // No password set
    }
  };

  const setPrivatePassword = async () => {
    if (passwordInput.length < 4) {
      alert('Password must be at least 4 characters');
      return;
    }
    try {
      await invoke('set_private_bookmarks_password', { profileId, password: passwordInput });
      setHasPassword(true);
      setPrivateUnlocked(true);
      setPasswordInput('');
    } catch (err) {
      console.error('Failed to set password:', err);
    }
  };

  // Get bookmarks in a folder
  const getBookmarksInFolder = (folderId: number) => {
    return allBookmarks.filter(b => b.folder_id === folderId);
  };

  // Handle drag start
  const handleDragStart = (e: React.DragEvent, bookmark: Bookmark) => {
    setDraggedBookmark(bookmark);
    e.dataTransfer.effectAllowed = 'move';
    e.dataTransfer.setData('text/plain', bookmark.id.toString());
  };

  // Handle drag over bookmark (for creating folder)
  const handleDragOverBookmark = (e: React.DragEvent, targetBookmark: Bookmark) => {
    e.preventDefault();
    if (draggedBookmark && draggedBookmark.id !== targetBookmark.id) {
      setDropTarget({ type: 'bookmark', id: targetBookmark.id });
    }
  };

  // Handle drag over folder
  const handleDragOverFolder = (e: React.DragEvent, folderId: number) => {
    e.preventDefault();
    setDropTarget({ type: 'folder', id: folderId });
  };

  // Handle drop on bookmark (create folder)
  const handleDropOnBookmark = async (e: React.DragEvent, targetBookmark: Bookmark) => {
    e.preventDefault();
    if (draggedBookmark && draggedBookmark.id !== targetBookmark.id) {
      // Open modal to create folder with these two bookmarks
      setFolderCreateBookmarks([draggedBookmark, targetBookmark]);
      setNewFolderName('');
      setShowCreateFolderModal(true);
    }
    setDraggedBookmark(null);
    setDropTarget(null);
  };

  // Handle drop on folder (move bookmark to folder)
  const handleDropOnFolder = async (e: React.DragEvent, folderId: number) => {
    e.preventDefault();
    if (draggedBookmark) {
      try {
        await invoke('update_bookmark', {
          bookmark_id: draggedBookmark.id,
          folder_id: folderId,
        });
        loadBookmarks();
      } catch (err) {
        console.error('Failed to move bookmark to folder:', err);
      }
    }
    setDraggedBookmark(null);
    setDropTarget(null);
  };

  // Handle drag end
  const handleDragEnd = () => {
    setDraggedBookmark(null);
    setDropTarget(null);
  };

  // Create folder from dragged bookmarks
  const createFolderFromBookmarks = async () => {
    if (!newFolderName.trim() || folderCreateBookmarks.length < 2) return;
    try {
      // Create the folder
      const folder = await invoke<BookmarkFolder>('create_bookmark_folder', {
        profileId,
        name: newFolderName.trim(),
        parent_id: null,
      });
      // Move both bookmarks into the folder
      for (const bookmark of folderCreateBookmarks) {
        await invoke('update_bookmark', {
          bookmark_id: bookmark.id,
          folder_id: folder.id,
        });
      }
      setShowCreateFolderModal(false);
      setFolderCreateBookmarks([]);
      setNewFolderName('');
      loadBookmarks();
      loadFolders();
    } catch (err) {
      console.error('Failed to create folder:', err);
    }
  };

  // Navigate helper that handles closing dropdowns
  const navigateTo = (url: string) => {
    console.log('BookmarkBar navigateTo called with:', url);
    // Close all dropdowns first
    setShowBookmarkList(false);
    setShowPrivateBookmarks(false);
    setOpenFolderId(null);
    // Then trigger navigation
    if (onNavigate) {
      console.log('Calling onNavigate');
      onNavigate(url);
    } else {
      console.warn('onNavigate is not defined!');
    }
  };

  // Close dropdowns on outside click
  useEffect(() => {
    const handleClickOutside = (e: MouseEvent) => {
      const target = e.target as HTMLElement;
      // Don't close if clicking inside a dropdown area
      if (target.closest('.bookmark-dropdown')) {
        return;
      }
      // Don't close if clicking on a bookmark item
      if (target.closest('.bookmark-item')) {
        return;
      }
      setShowBookmarkList(false);
      setShowPrivateBookmarks(false);
      setOpenFolderId(null);
    };
    // Use click event in capture phase so it runs after button handlers
    document.addEventListener('click', handleClickOutside);
    return () => document.removeEventListener('click', handleClickOutside);
  }, []);

  if (!visible) return null;

  return (
    <div className="flex items-center gap-1 px-2 py-1 bg-[var(--navbar-color)] border-b border-gray-700/30 flex-1 overflow-hidden">
      <div className="flex items-center gap-1 overflow-x-auto scrollbar-none flex-1">
        {/* Mandatory EarthSearch bookmark - always first */}
        <button
          onClick={() => navigateTo(earthSearchBookmark.url)}
          className="flex items-center gap-1.5 px-2 py-1 rounded hover:bg-gray-700/30 transition-colors max-w-[150px] group flex-shrink-0 border-r border-gray-600/30 pr-3 mr-1"
          title="Search Home"
        >
          <svg className="w-4 h-4 text-[var(--primary-color)] flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
          </svg>
          <span className="text-xs truncate text-[var(--text-color)] font-medium">
            Search
          </span>
        </button>

        {/* Toolbar Folders */}
        {folders.map(folder => {
          const folderBookmarks = getBookmarksInFolder(folder.id);
          if (folderBookmarks.length === 0) return null;

          return (
            <div key={folder.id} className="relative bookmark-dropdown">
              <button
                ref={(el) => { folderButtonRefs.current[folder.id] = el; }}
                onClick={(e) => {
                  e.stopPropagation();
                  const btn = folderButtonRefs.current[folder.id];
                  if (btn) {
                    const rect = btn.getBoundingClientRect();
                    setFolderDropdownPosition({
                      top: rect.bottom + 4,
                      left: rect.left,
                    });
                  }
                  setOpenFolderId(openFolderId === folder.id ? null : folder.id);
                  setShowBookmarkList(false);
                  setShowPrivateBookmarks(false);
                }}
                onDragOver={(e) => handleDragOverFolder(e, folder.id)}
                onDrop={(e) => handleDropOnFolder(e, folder.id)}
                className={`flex items-center gap-1.5 px-2 py-1 rounded hover:bg-gray-700/30 transition-colors max-w-[150px] group flex-shrink-0 ${
                  dropTarget?.type === 'folder' && dropTarget.id === folder.id ? 'bg-[var(--primary-color)]/30 ring-2 ring-[var(--primary-color)]' : ''
                }`}
                title={`${folder.name} (${folderBookmarks.length} bookmarks)`}
              >
                <svg className="w-4 h-4 text-[var(--primary-color)] flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
                </svg>
                <span className="text-xs truncate text-[var(--text-color)]">
                  {folder.name}
                </span>
                <svg className={`w-3 h-3 text-gray-400 transition-transform ${openFolderId === folder.id ? 'rotate-180' : ''}`} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
                </svg>
              </button>
            </div>
          );
        })}

        {/* User bookmarks (not in folders) */}
        {bookmarks.map(bookmark => (
          <NativeButton
            key={bookmark.id}
            draggable
            onDragStart={(e) => handleDragStart(e, bookmark)}
            onDragOver={(e) => handleDragOverBookmark(e, bookmark)}
            onDrop={(e) => handleDropOnBookmark(e, bookmark)}
            onDragEnd={handleDragEnd}
            onClick={() => navigateTo(bookmark.url)}
            className={`flex items-center gap-1.5 px-2 py-1 rounded hover:bg-gray-700/30 transition-colors max-w-[150px] group flex-shrink-0 cursor-pointer ${
              dropTarget?.type === 'bookmark' && dropTarget.id === bookmark.id ? 'bg-[var(--primary-color)]/30 ring-2 ring-[var(--primary-color)]' : ''
            } ${draggedBookmark?.id === bookmark.id ? 'opacity-50' : ''}`}
            title={`${bookmark.title} - ${bookmark.url}\nDrag onto another bookmark to create a folder`}
          >
            {bookmark.favicon ? (
              <img src={bookmark.favicon} className="w-4 h-4 flex-shrink-0 pointer-events-none" alt="" />
            ) : (
              <span className="w-4 h-4 flex items-center justify-center text-xs text-[var(--primary-color)] flex-shrink-0 bg-gray-700/50 rounded pointer-events-none">
                {bookmark.title.charAt(0).toUpperCase()}
              </span>
            )}
            <span className="text-xs truncate text-[var(--text-color)] pointer-events-none">
              {bookmark.title}
            </span>
          </NativeButton>
        ))}

        {bookmarks.length === 0 && folders.length === 0 && (
          <span className="text-xs text-gray-500 px-2">No bookmarks yet. Press Ctrl+D to add one.</span>
        )}
      </div>

      <div className="flex items-center gap-1 flex-shrink-0 ml-auto">
        {/* Private Bookmarks Button - Always shown */}
        <div className="relative bookmark-dropdown">
          <button
            ref={privateButtonRef}
            onClick={(e) => {
              e.stopPropagation();
              setShowPrivateBookmarks(!showPrivateBookmarks);
              setShowBookmarkList(false);
              setOpenFolderId(null);
            }}
            className={`p-1.5 rounded transition-colors ${
              showPrivateBookmarks || privateUnlocked
                ? 'text-yellow-400 hover:text-yellow-300 bg-yellow-500/10'
                : 'text-gray-400 hover:text-yellow-400 hover:bg-gray-700/30'
            }`}
            title="Private Bookmarks"
          >
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
            </svg>
          </button>
        </div>

        <button
          onClick={onToggleManager}
          className="px-2 py-1 text-xs text-gray-400 hover:text-white transition-colors"
        >
          Manage
        </button>

        {/* Bookmark List Dropdown Button */}
        <div className="relative bookmark-dropdown">
          <button
            ref={bookmarkListButtonRef}
            onClick={(e) => {
              e.stopPropagation();
              setShowBookmarkList(!showBookmarkList);
              setShowPrivateBookmarks(false);
              setOpenFolderId(null);
            }}
            className="p-1.5 text-gray-400 hover:text-white hover:bg-gray-700/30 rounded transition-colors"
            title="All Bookmarks"
          >
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 5l7 7-7 7M5 5l7 7-7 7" />
            </svg>
          </button>
        </div>
      </div>

      {/* Folder Dropdown - Portal */}
      {openFolderId !== null && createPortal(
        <div
          className="fixed z-[9999] bg-gray-900 border border-gray-700 rounded-lg shadow-2xl py-1 min-w-[220px] max-w-[300px]"
          style={{ top: folderDropdownPosition.top, left: folderDropdownPosition.left }}
          onClick={(e) => e.stopPropagation()}
        >
          {(() => {
            const folder = folders.find(f => f.id === openFolderId);
            const folderBookmarks = folder ? getBookmarksInFolder(folder.id) : [];
            if (!folder) return null;
            return (
              <>
                <div className="px-3 py-2 border-b border-gray-700 flex items-center justify-between">
                  <span className="text-xs font-semibold text-[var(--primary-color)] flex items-center gap-1.5">
                    <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
                    </svg>
                    {folder.name}
                  </span>
                  <span className="text-xs text-gray-500">{folderBookmarks.length}</span>
                </div>
                <div className="max-h-[300px] overflow-y-auto">
                  {folderBookmarks.map(bookmark => (
                    <button
                      key={bookmark.id}
                      onClick={() => navigateTo(bookmark.url)}
                      className="bookmark-item w-full flex items-center gap-2 px-3 py-2 hover:bg-gray-700/50 transition-colors text-left"
                    >
                      {bookmark.favicon ? (
                        <img src={bookmark.favicon} className="w-4 h-4 flex-shrink-0" alt="" />
                      ) : (
                        <span className="w-4 h-4 flex items-center justify-center text-[10px] text-[var(--primary-color)] flex-shrink-0 bg-gray-700/50 rounded">
                          {bookmark.title.charAt(0).toUpperCase()}
                        </span>
                      )}
                      <div className="flex-1 min-w-0">
                        <div className="text-sm text-[var(--text-color)] truncate">{bookmark.title}</div>
                        <div className="text-xs text-gray-500 truncate">{bookmark.url}</div>
                      </div>
                    </button>
                  ))}
                </div>
              </>
            );
          })()}
        </div>,
        document.body
      )}

      {/* Private Bookmarks - right-side panel */}
      <RightDockPanel
        id="private-bookmarks"
        open={showPrivateBookmarks}
        title="Private Bookmarks"
        onClose={() => setShowPrivateBookmarks(false)}
      >
          {!privateUnlocked ? (
            <div className="p-1">
              {hasPassword ? (
                <>
                  <p className="text-xs text-gray-400 mb-2">Enter your password to unlock private bookmarks</p>
                  <div className="flex gap-2">
                    <input
                      type="password"
                      value={passwordInput}
                      onChange={(e) => setPasswordInput(e.target.value)}
                      onKeyDown={(e) => e.key === 'Enter' && unlockPrivateBookmarks()}
                      className="flex-1 min-w-0 px-3 py-1.5 bg-gray-800 border border-gray-600 rounded text-sm focus:outline-none focus:border-yellow-500"
                      placeholder="Password..."
                      autoFocus
                    />
                    <button
                      onClick={unlockPrivateBookmarks}
                      className="px-3 py-1.5 bg-yellow-500/20 text-yellow-400 hover:bg-yellow-500/30 rounded text-sm transition-colors flex-shrink-0"
                    >
                      Unlock
                    </button>
                  </div>
                  <div className="mt-2">
                    <VaultAutofill profileId={profileId} appKey="bookmarks" onFill={pw => setPasswordInput(pw)} />
                  </div>
                </>
              ) : (
                <>
                  <p className="text-xs text-gray-400 mb-2">Set a password to protect your private bookmarks</p>
                  <div className="flex gap-2">
                    <input
                      type="password"
                      value={passwordInput}
                      onChange={(e) => setPasswordInput(e.target.value)}
                      onKeyDown={(e) => e.key === 'Enter' && setPrivatePassword()}
                      className="flex-1 min-w-0 px-3 py-1.5 bg-gray-800 border border-gray-600 rounded text-sm focus:outline-none focus:border-yellow-500"
                      placeholder="New password..."
                      autoFocus
                    />
                    <button
                      onClick={setPrivatePassword}
                      className="px-3 py-1.5 bg-yellow-500/20 text-yellow-400 hover:bg-yellow-500/30 rounded text-sm transition-colors flex-shrink-0"
                    >
                      Set
                    </button>
                  </div>
                </>
              )}
            </div>
          ) : (
            <>
              {privateBookmarks.length === 0 ? (
                <div className="px-3 py-4 text-center text-gray-500 text-sm">
                  No private bookmarks yet
                </div>
              ) : (
                privateBookmarks.map(bookmark => (
                  <button
                    key={bookmark.id}
                    onClick={() => navigateTo(bookmark.url)}
                    className="bookmark-item w-full flex items-center gap-2 px-3 py-2 hover:bg-gray-700/50 transition-colors text-left"
                  >
                    {bookmark.favicon ? (
                      <img src={bookmark.favicon} className="w-4 h-4 flex-shrink-0" alt="" />
                    ) : (
                      <span className="w-4 h-4 flex items-center justify-center text-[10px] text-yellow-400 flex-shrink-0 bg-gray-700/50 rounded">
                        {bookmark.title.charAt(0).toUpperCase()}
                      </span>
                    )}
                    <div className="flex-1 min-w-0">
                      <div className="text-sm text-[var(--text-color)] truncate">{bookmark.title}</div>
                      <div className="text-xs text-gray-500 truncate">{bookmark.url}</div>
                    </div>
                  </button>
                ))
              )}
              <div className="border-t border-gray-700 mt-1 pt-1">
                <button
                  onClick={() => {
                    setPrivateUnlocked(false);
                    setShowPrivateBookmarks(false);
                  }}
                  className="w-full px-3 py-2 text-left text-sm text-gray-400 hover:bg-gray-700/50 transition-colors flex items-center gap-2"
                >
                  <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
                  </svg>
                  Lock Private Bookmarks
                </button>
              </div>
            </>
          )}
      </RightDockPanel>

      {/* Bookmark List Dropdown - Portal */}
      <RightDockPanel
        id="all-bookmarks"
        open={showBookmarkList}
        title="All Bookmarks"
        subtitle={`${allBookmarks.length} items`}
        onClose={() => setShowBookmarkList(false)}
      >
          {allBookmarks.length === 0 ? (
            <div className="px-3 py-4 text-center text-gray-500 text-sm">
              No bookmarks yet
            </div>
          ) : (
            allBookmarks.map(bookmark => (
              <button
                key={bookmark.id}
                onClick={() => navigateTo(bookmark.url)}
                className="bookmark-item w-full flex items-center gap-2 px-3 py-2 hover:bg-gray-700/50 transition-colors text-left"
              >
                {bookmark.favicon ? (
                  <img src={bookmark.favicon} className="w-4 h-4 flex-shrink-0" alt="" />
                ) : (
                  <span className="w-4 h-4 flex items-center justify-center text-[10px] text-[var(--primary-color)] flex-shrink-0 bg-gray-700/50 rounded">
                    {bookmark.title.charAt(0).toUpperCase()}
                  </span>
                )}
                <div className="flex-1 min-w-0">
                  <div className="text-sm text-[var(--text-color)] truncate">{bookmark.title}</div>
                  <div className="text-xs text-gray-500 truncate">{bookmark.url}</div>
                </div>
              </button>
            ))
          )}

          <div className="border-t border-gray-700 mt-1 pt-1">
            <button
              onClick={() => {
                onToggleManager?.();
                setShowBookmarkList(false);
              }}
              className="w-full px-3 py-2 text-left text-sm text-[var(--primary-color)] hover:bg-gray-700/50 transition-colors flex items-center gap-2"
            >
              <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
              </svg>
              Manage Bookmarks
            </button>
          </div>
      </RightDockPanel>

      {/* Create Folder Modal - Using Portal to render at document root */}
      {showCreateFolderModal && createPortal(
        <div className="fixed inset-0 z-[9999] flex items-center justify-center bg-black/80">
          <div
            className="w-full max-w-sm bg-gray-900 border border-gray-700 rounded-lg shadow-2xl mx-4"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex items-center justify-between p-4 border-b border-gray-700">
              <h3 className="text-sm font-semibold text-[var(--text-color)]">Create Folder</h3>
              <button
                onClick={() => {
                  setShowCreateFolderModal(false);
                  setFolderCreateBookmarks([]);
                }}
                className="p-1 hover:bg-gray-700 rounded transition-colors"
              >
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>
            <div className="p-4 space-y-3">
              <div>
                <label className="block text-xs text-gray-400 mb-1">Folder Name</label>
                <input
                  type="text"
                  value={newFolderName}
                  onChange={(e) => setNewFolderName(e.target.value)}
                  onKeyDown={(e) => e.key === 'Enter' && createFolderFromBookmarks()}
                  className="w-full px-3 py-2 bg-gray-800 border border-gray-600 rounded text-sm focus:outline-none focus:border-[var(--primary-color)]"
                  placeholder="Enter folder name..."
                  autoFocus
                />
              </div>
              <div className="text-xs text-gray-400">
                This folder will contain:
                <ul className="mt-1 space-y-1">
                  {folderCreateBookmarks.map(b => (
                    <li key={b.id} className="flex items-center gap-2 text-[var(--text-color)]">
                      <span className="w-3 h-3 bg-gray-700 rounded flex items-center justify-center text-[8px]">
                        {b.title.charAt(0).toUpperCase()}
                      </span>
                      {b.title}
                    </li>
                  ))}
                </ul>
              </div>
            </div>
            <div className="flex justify-end gap-2 p-4 border-t border-gray-700">
              <button
                onClick={() => {
                  setShowCreateFolderModal(false);
                  setFolderCreateBookmarks([]);
                }}
                className="px-3 py-1.5 text-xs hover:bg-gray-700 rounded transition-colors"
              >
                Cancel
              </button>
              <button
                onClick={createFolderFromBookmarks}
                disabled={!newFolderName.trim()}
                className="px-4 py-1.5 text-xs bg-[var(--primary-color)] hover:opacity-90 rounded transition-opacity disabled:opacity-50"
              >
                Create Folder
              </button>
            </div>
          </div>
        </div>,
        document.body
      )}
    </div>
  );
}

// ==================== BookmarkManager ====================
export function BookmarkManager({ profileId, isOpen, onClose, onNavigate }: BookmarkManagerProps) {
  const [bookmarks, setBookmarks] = useState<Bookmark[]>([]);
  const [folders, setFolders] = useState<BookmarkFolder[]>([]);
  const [selectedFolder, setSelectedFolder] = useState<number | null>(null);
  const [searchQuery, setSearchQuery] = useState('');
  const [editingBookmark, setEditingBookmark] = useState<Bookmark | null>(null);
  const [showAddFolder, setShowAddFolder] = useState(false);
  const [newFolderName, setNewFolderName] = useState('');

  useEffect(() => {
    if (isOpen) {
      loadBookmarks();
      loadFolders();
    }
  }, [isOpen, profileId]);

  const loadBookmarks = async () => {
    try {
      const data = await invoke<Bookmark[]>('get_all_bookmarks', { profileId: profileId });
      // Filter out system bookmarks (earth:// URLs) - they can't be removed
      setBookmarks(data.filter(b => !b.url.startsWith('earth://')));
    } catch (err) {
      console.error('Failed to load bookmarks:', err);
    }
  };

  const loadFolders = async () => {
    try {
      const data = await invoke<BookmarkFolder[]>('get_bookmark_folders', { profileId: profileId });
      setFolders(data);
    } catch (err) {
      console.error('Failed to load folders:', err);
    }
  };

  const searchBookmarks = async (query: string) => {
    if (!query.trim()) {
      loadBookmarks();
      return;
    }
    try {
      const results = await invoke<Bookmark[]>('search_bookmarks', { profileId: profileId, query });
      // Filter out system bookmarks (earth:// URLs) - they can't be removed
      setBookmarks(results.filter(b => !b.url.startsWith('earth://')));
    } catch (err) {
      console.error('Failed to search bookmarks:', err);
    }
  };

  const deleteBookmark = async (bookmarkId: number) => {
    try {
      await invoke('delete_bookmark', { bookmarkId: bookmarkId });
      loadBookmarks();
    } catch (err) {
      console.error('Failed to delete bookmark:', err);
    }
  };

  const createFolder = async () => {
    if (!newFolderName.trim()) return;
    try {
      await invoke('create_bookmark_folder', {
        profileId,
        name: newFolderName,
        parent_id: null,
      });
      setNewFolderName('');
      setShowAddFolder(false);
      loadFolders();
    } catch (err) {
      console.error('Failed to create folder:', err);
    }
  };

  const deleteFolder = async (folderId: number) => {
    try {
      await invoke('delete_bookmark_folder', { folderId: folderId });
      loadFolders();
      loadBookmarks();
    } catch (err) {
      console.error('Failed to delete folder:', err);
    }
  };

  const exportBookmarks = async (format: 'json' | 'html') => {
    try {
      const data = await invoke<string>('export_bookmarks', { profileId: profileId, format });
      const blob = new Blob([data], { type: format === 'html' ? 'text/html' : 'application/json' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `bookmarks.${format}`;
      a.click();
      URL.revokeObjectURL(url);
    } catch (err) {
      console.error('Failed to export bookmarks:', err);
    }
  };

  const filteredBookmarks = selectedFolder !== null
    ? bookmarks.filter(b => b.folder_id === selectedFolder)
    : bookmarks;

  return (
    <>
    <RightDockPanel id="bookmark-manager" open={isOpen} width={600} title="Bookmark Manager" onClose={onClose}>
        <div className="flex items-center gap-2 mb-2 flex-shrink-0">
          <button
            onClick={() => exportBookmarks('json')}
            className="px-3 py-1.5 text-xs bg-gray-700 hover:bg-gray-600 rounded transition-colors"
          >
            Export JSON
          </button>
          <button
            onClick={() => exportBookmarks('html')}
            className="px-3 py-1.5 text-xs bg-gray-700 hover:bg-gray-600 rounded transition-colors"
          >
            Export HTML
          </button>
        </div>

        <div className="flex flex-1 overflow-hidden h-full">
          {/* Sidebar - Folders */}
          <div className="w-60 flex-shrink-0 border-r border-gray-700 p-2 overflow-y-auto">
            <div className="mb-3">
              <input
                type="text"
                placeholder="Search bookmarks..."
                value={searchQuery}
                onChange={(e) => {
                  setSearchQuery(e.target.value);
                  searchBookmarks(e.target.value);
                }}
                className="w-full px-3 py-2 bg-gray-800 border border-gray-600 rounded text-sm focus:outline-none focus:border-[var(--primary-color)]"
              />
            </div>

            <button
              onClick={() => setSelectedFolder(null)}
              className={`w-full px-3 py-2 text-left text-sm rounded mb-1 transition-colors ${
                selectedFolder === null ? 'bg-[var(--primary-color)]/20 text-[var(--primary-color)]' : 'hover:bg-gray-700'
              }`}
            >
              All Bookmarks ({bookmarks.length})
            </button>

            <div className="mt-3 mb-2 flex items-center justify-between">
              <span className="text-xs text-gray-400 uppercase">Folders</span>
              <button
                onClick={() => setShowAddFolder(true)}
                className="p-1 hover:bg-gray-700 rounded transition-colors"
              >
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
                </svg>
              </button>
            </div>

            {showAddFolder && (
              <div className="flex gap-1 mb-2">
                <input
                  type="text"
                  value={newFolderName}
                  onChange={(e) => setNewFolderName(e.target.value)}
                  placeholder="Folder name"
                  className="flex-1 px-2 py-1 bg-gray-800 border border-gray-600 rounded text-xs focus:outline-none focus:border-[var(--primary-color)]"
                  onKeyDown={(e) => e.key === 'Enter' && createFolder()}
                  autoFocus
                />
                <button onClick={createFolder} className="px-2 py-1 bg-[var(--primary-color)] rounded text-xs flex-shrink-0">
                  Add
                </button>
              </div>
            )}

            {folders.map(folder => (
              <div
                key={folder.id}
                className={`flex items-center justify-between px-3 py-2 text-sm rounded mb-1 cursor-pointer group transition-colors ${
                  selectedFolder === folder.id ? 'bg-[var(--primary-color)]/20 text-[var(--primary-color)]' : 'hover:bg-gray-700'
                }`}
                onClick={() => setSelectedFolder(folder.id)}
              >
                <span className="flex items-center gap-2">
                  <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
                  </svg>
                  {folder.name}
                </span>
                <button
                  onClick={(e) => { e.stopPropagation(); deleteFolder(folder.id); }}
                  className="p-1 hover:bg-gray-600 rounded opacity-0 group-hover:opacity-100 transition-opacity"
                >
                  <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                  </svg>
                </button>
              </div>
            ))}
          </div>

          {/* Main content - Bookmarks */}
          <div className="flex-1 min-w-0 p-2 overflow-y-auto">
            <div className="grid grid-cols-1 gap-2">
              {filteredBookmarks.map(bookmark => (
                <div
                  key={bookmark.id}
                  onClick={() => onNavigate?.(bookmark.url)}
                  title={`${bookmark.title} — ${bookmark.url}`}
                  className="flex items-center gap-2 px-2 py-1.5 hover:bg-white/5 rounded transition-colors group cursor-pointer"
                >
                  {bookmark.favicon ? (
                    <img src={bookmark.favicon} className="w-5 h-5 flex-shrink-0" alt="" />
                  ) : (
                    <span className="w-5 h-5 flex-shrink-0 flex items-center justify-center text-xs bg-gray-700 rounded">
                      {bookmark.title.charAt(0).toUpperCase()}
                    </span>
                  )}
                  <div className="flex-1 min-w-0">
                    <div className="text-sm truncate text-[var(--text-color)]">{bookmark.title}</div>
                    <div className="text-xs text-gray-500 truncate">{bookmark.url}</div>
                  </div>
                  <div
                    className="flex gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity flex-shrink-0"
                    onClick={(e) => e.stopPropagation()}
                  >
                    <button
                      onClick={() => setEditingBookmark(bookmark)}
                      className="p-1 hover:bg-gray-600 rounded transition-colors"
                      title="Edit"
                    >
                      <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
                      </svg>
                    </button>
                    <button
                      onClick={() => deleteBookmark(bookmark.id)}
                      className="p-1 hover:bg-red-600/50 rounded transition-colors"
                      title="Delete"
                    >
                      <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                      </svg>
                    </button>
                  </div>
                </div>
              ))}

              {filteredBookmarks.length === 0 && (
                <div className="text-center py-12 text-gray-400">
                  {searchQuery ? 'No bookmarks match your search.' : 'No bookmarks in this folder.'}
                </div>
              )}
            </div>
          </div>
        </div>
    </RightDockPanel>

      {/* Edit Bookmark Modal */}
      {editingBookmark && (
        <EditBookmarkModal
          bookmark={editingBookmark}
          folders={folders}
          onClose={() => setEditingBookmark(null)}
          onSave={() => {
            setEditingBookmark(null);
            loadBookmarks();
          }}
        />
      )}
    </>
  );
}

// ==================== AddBookmarkModal ====================
export function AddBookmarkModal({ profileId, isOpen, onClose, initialUrl, initialTitle, onSave }: AddBookmarkModalProps) {
  const [title, setTitle] = useState(initialTitle || '');
  const [url, setUrl] = useState(initialUrl || '');
  const [tags, setTags] = useState('');
  const [notes, setNotes] = useState('');
  const [folderId, setFolderId] = useState<number | null>(null);
  const [folders, setFolders] = useState<BookmarkFolder[]>([]);

  useEffect(() => {
    if (isOpen) {
      setTitle(initialTitle || '');
      setUrl(initialUrl || '');
      loadFolders();
    }
  }, [isOpen, initialTitle, initialUrl]);

  const loadFolders = async () => {
    try {
      const data = await invoke<BookmarkFolder[]>('get_bookmark_folders', { profileId: profileId });
      setFolders(data);
    } catch (err) {
      console.error('Failed to load folders:', err);
    }
  };

  const handleSave = async () => {
    if (!title.trim() || !url.trim()) return;
    try {
      const bookmark = await invoke<Bookmark>('add_bookmark', {
        profileId,
        title: title.trim(),
        url: url.trim(),
        folder_id: folderId,
        tags: tags.split(',').map(t => t.trim()).filter(Boolean),
        notes: notes.trim() || null,
      });
      onSave?.(bookmark);
      onClose();
    } catch (err) {
      console.error('Failed to add bookmark:', err);
    }
  };

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/80">
      <div className="w-full max-w-md bg-gray-900 border border-gray-700 rounded-lg shadow-2xl">
        <div className="flex items-center justify-between p-4 border-b border-gray-700">
          <h3 className="text-lg font-semibold text-[var(--text-color)]">Add Bookmark</h3>
          <button onClick={onClose} className="p-1 hover:bg-gray-700 rounded transition-colors">
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
        <div className="p-4 space-y-4">
          <div>
            <label className="block text-sm text-gray-400 mb-1">Title</label>
            <input
              type="text"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              className="w-full px-3 py-2 bg-gray-800 border border-gray-600 rounded focus:outline-none focus:border-[var(--primary-color)]"
              placeholder="Bookmark title"
            />
          </div>
          <div>
            <label className="block text-sm text-gray-400 mb-1">URL</label>
            <input
              type="text"
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              className="w-full px-3 py-2 bg-gray-800 border border-gray-600 rounded focus:outline-none focus:border-[var(--primary-color)]"
              placeholder="https://..."
            />
          </div>
          <div>
            <label className="block text-sm text-gray-400 mb-1">Folder</label>
            <select
              value={folderId || ''}
              onChange={(e) => setFolderId(e.target.value ? parseInt(e.target.value) : null)}
              className="w-full px-3 py-2 bg-gray-800 border border-gray-600 rounded focus:outline-none focus:border-[var(--primary-color)]"
            >
              <option value="">No folder</option>
              {folders.map(f => (
                <option key={f.id} value={f.id}>{f.name}</option>
              ))}
            </select>
          </div>
          <div>
            <label className="block text-sm text-gray-400 mb-1">Tags (comma separated)</label>
            <input
              type="text"
              value={tags}
              onChange={(e) => setTags(e.target.value)}
              className="w-full px-3 py-2 bg-gray-800 border border-gray-600 rounded focus:outline-none focus:border-[var(--primary-color)]"
              placeholder="tech, news, important"
            />
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
          <button
            onClick={onClose}
            className="px-4 py-2 text-sm hover:bg-gray-700 rounded transition-colors"
          >
            Cancel
          </button>
          <button
            onClick={handleSave}
            className="px-4 py-2 text-sm bg-[var(--primary-color)] hover:opacity-90 rounded transition-opacity"
          >
            Save Bookmark
          </button>
        </div>
      </div>
    </div>
  );
}

// ==================== EditBookmarkModal ====================
function EditBookmarkModal({ bookmark, folders, onClose, onSave }: {
  bookmark: Bookmark;
  folders: BookmarkFolder[];
  onClose: () => void;
  onSave: () => void;
}) {
  const [title, setTitle] = useState(bookmark.title);
  const [url, setUrl] = useState(bookmark.url);
  const [tags, setTags] = useState(bookmark.tags.join(', '));
  const [notes, setNotes] = useState(bookmark.notes || '');
  const [folderId, setFolderId] = useState<number | null>(bookmark.folder_id);
  const [location, setLocation] = useState<BookmarkLocation>(bookmark.location || 'toolbar');

  const handleSave = async () => {
    try {
      await invoke('update_bookmark', {
        bookmark_id: bookmark.id,
        title,
        url,
        folder_id: folderId,
        tags: tags.split(',').map(t => t.trim()).filter(Boolean),
        notes: notes.trim() || null,
        location,
      });
      onSave();
    } catch (err) {
      console.error('Failed to update bookmark:', err);
    }
  };

  return (
    <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/80">
      <div className="w-full max-w-md bg-gray-900 border border-gray-700 rounded-lg shadow-2xl">
        <div className="flex items-center justify-between p-4 border-b border-gray-700">
          <h3 className="text-lg font-semibold text-[var(--text-color)]">Edit Bookmark</h3>
          <button onClick={onClose} className="p-1 hover:bg-gray-700 rounded transition-colors">
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
        <div className="p-4 space-y-4">
          <div>
            <label className="block text-sm text-gray-400 mb-1">Title</label>
            <input
              type="text"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              className="w-full px-3 py-2 bg-gray-800 border border-gray-600 rounded focus:outline-none focus:border-[var(--primary-color)]"
            />
          </div>
          <div>
            <label className="block text-sm text-gray-400 mb-1">URL</label>
            <input
              type="text"
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              className="w-full px-3 py-2 bg-gray-800 border border-gray-600 rounded focus:outline-none focus:border-[var(--primary-color)]"
            />
          </div>
          <div>
            <label className="block text-sm text-gray-400 mb-1">Folder</label>
            <select
              value={folderId || ''}
              onChange={(e) => setFolderId(e.target.value ? parseInt(e.target.value) : null)}
              className="w-full px-3 py-2 bg-gray-800 border border-gray-600 rounded focus:outline-none focus:border-[var(--primary-color)]"
            >
              <option value="">No folder</option>
              {folders.map(f => (
                <option key={f.id} value={f.id}>{f.name}</option>
              ))}
            </select>
          </div>
          <div>
            <label className="block text-sm text-gray-400 mb-1">Location</label>
            <div className="grid grid-cols-3 gap-2">
              <button
                type="button"
                onClick={() => setLocation('toolbar')}
                className={`px-3 py-2 text-xs rounded border transition-colors ${
                  location === 'toolbar'
                    ? 'bg-[var(--primary-color)]/20 border-[var(--primary-color)] text-[var(--primary-color)]'
                    : 'bg-gray-800 border-gray-600 hover:border-gray-500'
                }`}
              >
                Toolbar
              </button>
              <button
                type="button"
                onClick={() => setLocation('list')}
                className={`px-3 py-2 text-xs rounded border transition-colors ${
                  location === 'list'
                    ? 'bg-[var(--primary-color)]/20 border-[var(--primary-color)] text-[var(--primary-color)]'
                    : 'bg-gray-800 border-gray-600 hover:border-gray-500'
                }`}
              >
                List Only
              </button>
              <button
                type="button"
                onClick={() => setLocation('private')}
                className={`px-3 py-2 text-xs rounded border transition-colors ${
                  location === 'private'
                    ? 'bg-yellow-500/20 border-yellow-500 text-yellow-400'
                    : 'bg-gray-800 border-gray-600 hover:border-gray-500'
                }`}
              >
                Private
              </button>
            </div>
          </div>
          <div>
            <label className="block text-sm text-gray-400 mb-1">Tags</label>
            <input
              type="text"
              value={tags}
              onChange={(e) => setTags(e.target.value)}
              className="w-full px-3 py-2 bg-gray-800 border border-gray-600 rounded focus:outline-none focus:border-[var(--primary-color)]"
            />
          </div>
          <div>
            <label className="block text-sm text-gray-400 mb-1">Notes</label>
            <textarea
              value={notes}
              onChange={(e) => setNotes(e.target.value)}
              className="w-full px-3 py-2 bg-gray-800 border border-gray-600 rounded focus:outline-none focus:border-[var(--primary-color)] resize-none"
              rows={3}
            />
          </div>
        </div>
        <div className="flex justify-end gap-2 p-4 border-t border-gray-700">
          <button onClick={onClose} className="px-4 py-2 text-sm hover:bg-gray-700 rounded transition-colors">
            Cancel
          </button>
          <button onClick={handleSave} className="px-4 py-2 text-sm bg-[var(--primary-color)] hover:opacity-90 rounded transition-opacity">
            Save Changes
          </button>
        </div>
      </div>
    </div>
  );
}

// ==================== QuickBookmarkModal ====================
// Used by the star button in WebView URL bar
interface QuickBookmarkModalProps {
  profileId: number;
  isOpen: boolean;
  onClose: () => void;
  url: string;
  initialTitle?: string;
  existingBookmark?: Bookmark | null;
  onSave?: () => void;
  onDelete?: () => void;
}

export function QuickBookmarkModal({
  profileId,
  isOpen,
  onClose,
  url,
  initialTitle,
  existingBookmark,
  onSave,
  onDelete,
}: QuickBookmarkModalProps) {
  const [title, setTitle] = useState(existingBookmark?.title || initialTitle || '');
  const [location, setLocation] = useState<BookmarkLocation>(existingBookmark?.location || 'toolbar');
  const [tags, setTags] = useState(existingBookmark?.tags.join(', ') || '');

  useEffect(() => {
    if (isOpen) {
      setTitle(existingBookmark?.title || initialTitle || getDomain(url));
      setLocation(existingBookmark?.location || 'toolbar');
      setTags(existingBookmark?.tags.join(', ') || '');
    }
  }, [isOpen, existingBookmark, initialTitle, url]);

  const getDomain = (urlString: string) => {
    try {
      return new URL(urlString).hostname;
    } catch {
      return urlString;
    }
  };

  const handleSave = async () => {
    if (!title.trim()) return;
    try {
      if (existingBookmark) {
        await invoke('update_bookmark', {
          bookmark_id: existingBookmark.id,
          title: title.trim(),
          url,
          location,
          tags: tags.split(',').map(t => t.trim()).filter(Boolean),
        });
      } else {
        await invoke('add_bookmark', {
          profileId,
          title: title.trim(),
          url,
          folder_id: null,
          tags: tags.split(',').map(t => t.trim()).filter(Boolean),
          notes: null,
          location,
        });
      }
      onSave?.();
      onClose();
    } catch (err) {
      console.error('Failed to save bookmark:', err);
    }
  };

  const handleDelete = async () => {
    if (existingBookmark) {
      try {
        await invoke('delete_bookmark', { bookmarkId: existingBookmark.id });
        onDelete?.();
        onClose();
      } catch (err) {
        console.error('Failed to delete bookmark:', err);
      }
    }
  };

  return (
    <RightDockPanel
      id="bookmark"
      open={isOpen}
      title={existingBookmark ? 'Edit Bookmark' : 'Add Bookmark'}
      onClose={onClose}
    >
        <div className="p-4 space-y-3">
          {/* Name */}
          <div>
            <label className="block text-xs text-gray-400 mb-1">Name</label>
            <input
              type="text"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              className="w-full px-3 py-2 bg-gray-800 border border-gray-600 rounded text-sm focus:outline-none focus:border-[var(--primary-color)]"
              placeholder="Bookmark name"
              autoFocus
            />
          </div>

          {/* Location */}
          <div>
            <label className="block text-xs text-gray-400 mb-1">Location</label>
            <div className="grid grid-cols-3 gap-2">
              <button
                onClick={() => setLocation('toolbar')}
                className={`px-3 py-2 text-xs rounded border transition-colors ${
                  location === 'toolbar'
                    ? 'bg-[var(--primary-color)]/20 border-[var(--primary-color)] text-[var(--primary-color)]'
                    : 'bg-gray-800 border-gray-600 hover:border-gray-500'
                }`}
              >
                Toolbar
              </button>
              <button
                onClick={() => setLocation('list')}
                className={`px-3 py-2 text-xs rounded border transition-colors ${
                  location === 'list'
                    ? 'bg-[var(--primary-color)]/20 border-[var(--primary-color)] text-[var(--primary-color)]'
                    : 'bg-gray-800 border-gray-600 hover:border-gray-500'
                }`}
              >
                List Only
              </button>
              <button
                onClick={() => setLocation('private')}
                className={`px-3 py-2 text-xs rounded border transition-colors ${
                  location === 'private'
                    ? 'bg-yellow-500/20 border-yellow-500 text-yellow-400'
                    : 'bg-gray-800 border-gray-600 hover:border-gray-500'
                }`}
              >
                Private
              </button>
            </div>
          </div>

          {/* Tags */}
          <div>
            <label className="block text-xs text-gray-400 mb-1">Tags (comma separated)</label>
            <input
              type="text"
              value={tags}
              onChange={(e) => setTags(e.target.value)}
              className="w-full px-3 py-2 bg-gray-800 border border-gray-600 rounded text-sm focus:outline-none focus:border-[var(--primary-color)]"
              placeholder="work, important, news"
            />
          </div>

          {/* URL display */}
          <div className="text-xs text-gray-500 truncate">
            {url}
          </div>
        </div>

        <div className="flex items-center justify-between px-4 py-3 border-t border-gray-700">
          {existingBookmark ? (
            <button
              onClick={handleDelete}
              className="px-3 py-1.5 text-xs text-red-400 hover:bg-red-500/20 rounded transition-colors"
            >
              Remove
            </button>
          ) : (
            <div />
          )}
          <div className="flex gap-2">
            <button
              onClick={onClose}
              className="px-3 py-1.5 text-xs hover:bg-gray-700 rounded transition-colors"
            >
              Cancel
            </button>
            <button
              onClick={handleSave}
              className="px-4 py-1.5 text-xs bg-[var(--primary-color)] hover:opacity-90 rounded transition-opacity"
            >
              {existingBookmark ? 'Update' : 'Save'}
            </button>
          </div>
        </div>
    </RightDockPanel>
  );
}

// ==================== Bookmark Hook ====================
export function useBookmarks(profileId: number) {
  const [bookmarks, setBookmarks] = useState<Bookmark[]>([]);

  const loadBookmarks = useCallback(async () => {
    try {
      const data = await invoke<Bookmark[]>('get_all_bookmarks', { profileId: profileId });
      setBookmarks(data);
    } catch (err) {
      console.error('Failed to load bookmarks:', err);
    }
  }, [profileId]);

  useEffect(() => {
    loadBookmarks();
  }, [loadBookmarks]);

  const isBookmarked = useCallback(async (url: string): Promise<number | null> => {
    try {
      return await invoke<number | null>('is_url_bookmarked', { profileId: profileId, url });
    } catch {
      return null;
    }
  }, [profileId]);

  const addBookmark = async (title: string, url: string, options?: { folderId?: number; tags?: string[] }) => {
    try {
      const bookmark = await invoke<Bookmark>('add_bookmark', {
        profileId,
        title,
        url,
        folder_id: options?.folderId || null,
        tags: options?.tags || [],
        notes: null,
      });
      loadBookmarks();
      return bookmark;
    } catch (err) {
      console.error('Failed to add bookmark:', err);
      return null;
    }
  };

  const removeBookmark = async (bookmarkId: number) => {
    try {
      await invoke('delete_bookmark', { bookmarkId: bookmarkId });
      loadBookmarks();
    } catch (err) {
      console.error('Failed to remove bookmark:', err);
    }
  };

  return {
    bookmarks,
    loadBookmarks,
    isBookmarked,
    addBookmark,
    removeBookmark,
  };
}

export default BookmarkBar;
