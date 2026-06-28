// Downloads toolbar button + dropdown panel.
// Tracks downloads in-memory (cleared on exit), driven by backend events
// emitted from the browser surface: download-started / -progress / -finished.

import { useState, useEffect, useRef } from 'react';
import { invoke, isTauri, listen } from '../lib/tauri';
import { RightDockPanel } from '../lib/rightDock';

interface DownloadItem {
  id: number;
  url: string;
  filename: string;
  progress: number; // 0..1
  done: boolean;
  ok: boolean;
  path: string;
}

export function DownloadsButton() {
  const [downloads, setDownloads] = useState<DownloadItem[]>([]);
  const [open, setOpen] = useState(false);
  const wrapRef = useRef<HTMLDivElement>(null);

  // Subscribe to backend download events.
  useEffect(() => {
    if (!isTauri()) return;
    let cancelled = false;
    const unlisteners: Array<() => void> = [];
    const track = (u: () => void) => { if (cancelled) u(); else unlisteners.push(u); };

    const setup = async () => {
      track(await listen<{ id: number; url: string; filename: string }>('download-started', ({ payload }) => {
        setDownloads(prev => [
          { id: payload.id, url: payload.url, filename: payload.filename, progress: 0, done: false, ok: false, path: '' },
          ...prev.filter(d => d.id !== payload.id),
        ]);
      }));
      track(await listen<{ id: number; progress: number }>('download-progress', ({ payload }) => {
        setDownloads(prev => prev.map(d => (d.id === payload.id ? { ...d, progress: payload.progress } : d)));
      }));
      track(await listen<{ id: number; path: string; ok: boolean }>('download-finished', ({ payload }) => {
        setDownloads(prev => prev.map(d => (d.id === payload.id ? { ...d, done: true, ok: payload.ok, path: payload.path, progress: 1 } : d)));
      }));
    };
    setup();
    return () => { cancelled = true; unlisteners.forEach(u => u()); };
  }, []);

  // Close the panel on outside click.
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (wrapRef.current && !wrapRef.current.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener('mousedown', onDown);
    return () => window.removeEventListener('mousedown', onDown);
  }, [open]);

  const active = downloads.filter(d => !d.done).length;

  return (
    <div ref={wrapRef} className="relative" data-no-drag>
      <button
        onClick={() => setOpen(o => !o)}
        title="Downloads"
        className="relative p-1.5 rounded-lg text-white/60 hover:bg-white/10 hover:text-white transition-all cursor-pointer"
      >
        <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16v2a2 2 0 002 2h12a2 2 0 002-2v-2M7 10l5 5 5-5M12 15V3" />
        </svg>
        {active > 0 && (
          <span className="absolute -top-0.5 -right-0.5 min-w-[16px] h-4 px-1 rounded-full bg-[var(--primary-color)] text-white text-[10px] flex items-center justify-center">
            {active}
          </span>
        )}
      </button>

      <RightDockPanel
        id="downloads"
        open={open}
        title="Downloads"
        subtitle="Cleared when you close the app."
        onClose={() => setOpen(false)}
      >
        {downloads.length > 0 && (
          <div className="flex justify-end px-2 pb-1">
            <button onClick={() => setDownloads([])} className="text-xs text-gray-400 hover:text-red-400">Clear</button>
          </div>
        )}
        {downloads.length === 0 ? (
          <p className="text-xs text-gray-500 text-center py-4">No downloads yet</p>
        ) : (
          <div className="space-y-1">
            {downloads.map(d => (
              <div key={d.id} className="px-2 py-1.5 rounded hover:bg-white/5">
                <div className="flex items-center justify-between gap-2">
                  <span className="text-xs text-white truncate flex-1" title={d.filename || d.url}>
                    {d.filename || d.url.split('/').pop() || d.url}
                  </span>
                  {d.done ? (
                    d.ok ? (
                      <button
                        onClick={() => invoke('open_download', { path: d.path }).catch(() => {})}
                        className="text-xs text-[var(--primary-color)] hover:underline flex-shrink-0"
                      >
                        Open
                      </button>
                    ) : (
                      <span className="text-xs text-red-400 flex-shrink-0">Failed</span>
                    )
                  ) : (
                    <span className="text-xs text-gray-400 flex-shrink-0">{Math.round((d.progress || 0) * 100)}%</span>
                  )}
                </div>
                {!d.done && (
                  <div className="mt-1 h-1 bg-white/10 rounded overflow-hidden">
                    <div className="h-full bg-[var(--primary-color)] transition-all" style={{ width: `${Math.round((d.progress || 0) * 100)}%` }} />
                  </div>
                )}
              </div>
            ))}
          </div>
        )}
      </RightDockPanel>
    </div>
  );
}

export default DownloadsButton;
