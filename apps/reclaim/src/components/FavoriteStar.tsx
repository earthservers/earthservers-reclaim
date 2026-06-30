// Favorite (PIN) control — the SINGLE source of truth for "pinned", usable from
// anywhere (address bar, search result, History row, bookmark row). Deliberately a
// PIN icon (not a star): the star already means "bookmark" elsewhere, and a
// favorite is a different, heavier thing (cache + index + curate + permanent).
// Its filled/empty state is derived from the backend retention tier (pinned ⇒
// filled) via favorite_state; toggling calls set_favorite, which writes both the
// pinned tier and the knowledge-graph favorite flag. A login page is saved URL-only.

import { useEffect, useState, useCallback } from 'react';
import { invoke, isTauri } from '../lib/tauri';

export function FavoriteStar({
  url,
  profileId,
  title,
  isLogin,
  size = 4,
  className = '',
  onChange,
}: {
  url: string;
  profileId: number | null;
  title?: string;
  isLogin?: boolean;
  size?: number;
  className?: string;
  onChange?: (favorited: boolean) => void;
}) {
  const [fav, setFav] = useState(false);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!isTauri() || !url) return;
    let cancelled = false;
    invoke<boolean>('favorite_state', { url, profileId: profileId ?? 1 })
      .then(v => { if (!cancelled) setFav(!!v); })
      .catch(() => {});
    return () => { cancelled = true; };
  }, [url, profileId]);

  const toggle = useCallback(async (e: React.MouseEvent) => {
    e.stopPropagation();
    if (busy || !isTauri()) return;
    const next = !fav;
    setBusy(true);
    setFav(next); // optimistic
    try {
      await invoke('set_favorite', {
        url,
        favorite: next,
        profileId: profileId ?? 1,
        title: title ?? url,
        isLogin: !!isLogin,
      });
      onChange?.(next);
    } catch {
      setFav(!next); // revert on failure
    } finally {
      setBusy(false);
    }
  }, [busy, fav, url, profileId, title, isLogin, onChange]);

  const px = `${size * 0.25}rem`;
  return (
    <button
      onClick={toggle}
      disabled={busy}
      title={
        isLogin
          ? 'Login page — saved as a shortcut only, contents not stored'
          : fav
            ? 'Favorited (pinned + indexed). Click to unpin.'
            : 'Favorite: pin, cache & index this page'
      }
      className={`rounded transition-colors flex-shrink-0 ${fav ? 'text-[var(--primary-color)]' : 'text-gray-500 hover:text-[var(--primary-color)]'} ${className}`}
    >
      {/* Pushpin glyph — visually distinct from the bookmark star. */}
      <svg style={{ width: px, height: px }} fill={fav ? 'currentColor' : 'none'} stroke="currentColor" viewBox="0 0 24 24">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 3h6l-1 5 3 3v2h-4v6l-1 2-1-2v-6H7v-2l3-3-1-5z" />
      </svg>
    </button>
  );
}

export default FavoriteStar;
