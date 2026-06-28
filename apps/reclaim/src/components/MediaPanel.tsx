// Media downloader — a right-dock panel that lists the current page's images,
// gifs and videos as thumbnails. Nothing is auto-downloaded: you pick an item,
// optionally describe it (saved for your local AI to reference), and download it.

import { useState, useEffect } from 'react';
import { invoke, listen } from '../lib/tauri';
import { RightDockPanel, RIGHT_DOCK_WIDTH } from '../lib/rightDock';

interface MediaItem { kind: string; url: string; thumb: string; }

interface MediaPanelProps {
  profileId: number | null;
  isOpen: boolean;
  onClose: () => void;
  pageUrl?: string;
}

export function MediaPanel({ profileId, isOpen, onClose, pageUrl }: MediaPanelProps) {
  const [items, setItems] = useState<MediaItem[]>([]);
  const [descriptions, setDescriptions] = useState<Record<string, string>>({});
  const [status, setStatus] = useState<Record<string, string>>({}); // url -> 'saving' | 'saved' | error
  const [loading, setLoading] = useState(false);
  // yt-dlp (streaming sites like YouTube — no plain file URL to fetch).
  const [ytdlp, setYtdlp] = useState(false);
  const [ytDesc, setYtDesc] = useState('');
  const [ytStatus, setYtStatus] = useState('');

  useEffect(() => {
    invoke<boolean>('ytdlp_available').then(setYtdlp).catch(() => setYtdlp(false));
  }, []);

  const downloadPageVideo = async () => {
    setYtStatus('saving');
    try {
      await invoke('download_video_ytdlp', {
        profileId: profileId ?? 1,
        url: pageUrl || '',
        description: ytDesc,
      });
      setYtStatus('saved');
    } catch (e) {
      setYtStatus(String(e).replace(/^.*?:\s*/, ''));
    }
  };

  // The page posts its media list back via this event.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<{ origin: string; items: MediaItem[] }>('media-list', ({ payload }) => {
      setItems(payload.items || []);
      setLoading(false);
    }).then(u => { unlisten = u; });
    return () => unlisten?.();
  }, []);

  // On open (or page change while open), ask the page to enumerate its media.
  useEffect(() => {
    if (!isOpen) return;
    setItems([]);
    setStatus({});
    setLoading(true);
    invoke('browser_collect_media').catch(() => setLoading(false));
  }, [isOpen, pageUrl]);

  const download = async (item: MediaItem) => {
    setStatus(s => ({ ...s, [item.url]: 'saving' }));
    try {
      await invoke('download_media', {
        profileId: profileId ?? 1,
        url: item.url,
        kind: item.kind,
        description: descriptions[item.url] || '',
        pageUrl: pageUrl || '',
      });
      setStatus(s => ({ ...s, [item.url]: 'saved' }));
    } catch (e) {
      setStatus(s => ({ ...s, [item.url]: String(e).replace(/^.*?:\s*/, '') }));
    }
  };

  return (
    <RightDockPanel
      id="media-downloader"
      open={isOpen}
      width={RIGHT_DOCK_WIDTH}
      title="Media on page"
      subtitle="Download & describe for your AI"
      onClose={onClose}
    >
      <div className="flex flex-col gap-3 p-1">
        {/* Whole-page video via yt-dlp (YouTube etc.) */}
        <div className="border border-white/10 rounded-lg p-2 bg-white/5">
          <div className="text-xs font-medium text-white mb-1">This page's video (yt-dlp)</div>
          {ytdlp ? (
            <>
              <input
                value={ytDesc}
                onChange={e => setYtDesc(e.target.value)}
                placeholder="Describe this video (for your AI)…"
                className="w-full px-2 py-1 mb-1.5 text-xs bg-gray-800 border border-gray-600 rounded outline-none focus:border-[var(--primary-color)] text-white"
              />
              <div className="flex items-center gap-2">
                <button
                  onClick={downloadPageVideo}
                  disabled={ytStatus === 'saving' || ytStatus === 'saved'}
                  className="px-2.5 py-1 text-xs rounded bg-[var(--primary-color)] text-white hover:opacity-90 disabled:opacity-50"
                >
                  {ytStatus === 'saving' ? 'Downloading…' : ytStatus === 'saved' ? 'Saved ✓' : 'Download video'}
                </button>
                {ytStatus && ytStatus !== 'saving' && ytStatus !== 'saved' && (
                  <span className="text-xs text-red-400 truncate">{ytStatus}</span>
                )}
              </div>
            </>
          ) : (
            <div className="text-xs text-gray-400">
              Install <span className="font-mono text-gray-300">yt-dlp</span> to download videos from streaming sites (YouTube, etc.).
            </div>
          )}
        </div>

        <button
          onClick={() => { setItems([]); setLoading(true); invoke('browser_collect_media').catch(() => setLoading(false)); }}
          className="self-end text-xs text-[var(--primary-color)] hover:underline"
        >
          Rescan
        </button>
        {loading && <div className="text-sm text-gray-400">Scanning page…</div>}
        {!loading && items.length === 0 && (
          <div className="text-sm text-gray-400">No downloadable media found on this page.</div>
        )}
        {items.map((item, i) => {
          const st = status[item.url];
          return (
            <div key={item.url + i} className="border border-white/10 rounded-lg p-2 flex gap-3">
              <div className="w-20 h-20 flex-shrink-0 rounded bg-black/30 overflow-hidden flex items-center justify-center">
                {item.thumb ? (
                  <img src={item.thumb} loading="lazy" className="w-full h-full object-cover"
                    onError={(e) => { (e.target as HTMLImageElement).style.display = 'none'; }} />
                ) : (
                  <span className="text-[10px] text-gray-500">{item.kind}</span>
                )}
              </div>
              <div className="flex-1 min-w-0 flex flex-col gap-1">
                <div className="flex items-center gap-2">
                  <span className="text-[10px] uppercase px-1.5 py-0.5 rounded bg-white/10 text-gray-300">{item.kind}</span>
                  <span className="text-xs text-gray-500 truncate flex-1">{decodeURIComponent(item.url.split('/').pop() || '')}</span>
                </div>
                <input
                  value={descriptions[item.url] || ''}
                  onChange={e => setDescriptions(d => ({ ...d, [item.url]: e.target.value }))}
                  placeholder="Describe this (for your AI)…"
                  className="w-full px-2 py-1 text-xs bg-gray-800 border border-gray-600 rounded outline-none focus:border-[var(--primary-color)] text-white"
                />
                <div className="flex items-center gap-2">
                  <button
                    onClick={() => download(item)}
                    disabled={st === 'saving' || st === 'saved'}
                    className="px-2.5 py-1 text-xs rounded bg-[var(--primary-color)] text-white hover:opacity-90 disabled:opacity-50"
                  >
                    {st === 'saving' ? 'Saving…' : st === 'saved' ? 'Saved ✓' : 'Download'}
                  </button>
                  {st && st !== 'saving' && st !== 'saved' && (
                    <span className="text-xs text-red-400 truncate">{st}</span>
                  )}
                </div>
              </div>
            </div>
          );
        })}
      </div>
    </RightDockPanel>
  );
}

export default MediaPanel;
