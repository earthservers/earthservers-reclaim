// Privacy protections toolbar button + dropdown.
// Toggles WebKit-native privacy features applied to the embedded browser surface.
// Changes persist and apply live (the page reloads).

import { useState, useEffect, useRef } from 'react';
import { invoke, isTauri } from '../lib/tauri';
import { RightDockPanel } from '../lib/rightDock';

interface PrivacyConfig {
  blockWebrtc: boolean;
  blockThirdPartyCookies: boolean;
  trackingPrevention: boolean;
  blockDnsPrefetch: boolean;
  spoofUserAgent: boolean;
}

const ITEMS: Array<{ key: keyof PrivacyConfig; label: string; desc: string }> = [
  { key: 'blockWebrtc', label: 'Block WebRTC', desc: 'Stops IP-address leaks. Turn OFF for browser video calls.' },
  { key: 'blockThirdPartyCookies', label: 'Block third-party cookies', desc: "Cookies can't follow you across sites." },
  { key: 'trackingPrevention', label: 'Tracking prevention (ITP)', desc: "WebKit's cross-site tracker blocker (Safari's engine)." },
  { key: 'blockDnsPrefetch', label: 'Block DNS prefetch', desc: "Don't pre-resolve link domains (leaks intent)." },
  { key: 'spoofUserAgent', label: 'Mask user agent', desc: 'Present a common UA to shrink your fingerprint.' },
];

export function PrivacyButton({ isIncognito = false }: { isIncognito?: boolean }) {
  const [cfg, setCfg] = useState<PrivacyConfig | null>(null);
  const [open, setOpen] = useState(false);
  const wrapRef = useRef<HTMLDivElement>(null);

  // Refetch whenever incognito flips: entering incognito the backend force-enables
  // every protection (and locks it), so the displayed config would otherwise go
  // stale. Refetching on exit restores the real persisted state.
  useEffect(() => {
    if (!isTauri()) return;
    invoke<PrivacyConfig>('privacy_get_config').then(setCfg).catch(() => {});
  }, [isIncognito]);

  // While incognito, all protections are forced ON and the toggles are locked.
  const isOn = (key: keyof PrivacyConfig) => (isIncognito ? true : !!cfg?.[key]);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (wrapRef.current && !wrapRef.current.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener('mousedown', onDown);
    return () => window.removeEventListener('mousedown', onDown);
  }, [open]);

  const toggle = async (key: keyof PrivacyConfig) => {
    if (!cfg || isIncognito) return; // locked while incognito
    const next = { ...cfg, [key]: !cfg[key] };
    setCfg(next);
    try {
      await invoke('privacy_set_config', { config: next });
    } catch (err) {
      console.error('Failed to set privacy config:', err);
    }
  };

  const onCount = isIncognito ? ITEMS.length : cfg ? ITEMS.filter(i => cfg[i.key]).length : 0;

  return (
    <div ref={wrapRef} className="relative" data-no-drag>
      <button
        onClick={() => setOpen(o => !o)}
        title="Privacy protections"
        className="relative p-1.5 rounded-lg text-white/60 hover:bg-white/10 hover:text-white transition-all cursor-pointer"
      >
        <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z" />
        </svg>
        {onCount > 0 && (
          <span className="absolute -top-0.5 -right-0.5 min-w-[16px] h-4 px-1 rounded-full bg-green-500 text-white text-[10px] flex items-center justify-center">
            {onCount}
          </span>
        )}
      </button>

      <RightDockPanel
        id="privacy"
        open={open}
        title="Privacy protections"
        subtitle="Applied to every page. Changes reload the current page."
        onClose={() => setOpen(false)}
      >
        {isIncognito && (
          <div className="mb-1 px-2 py-1.5 rounded bg-purple-500/15 border border-purple-500/30 text-[11px] text-purple-300 flex items-center gap-1.5">
            <svg className="w-3.5 h-3.5 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
            </svg>
            <span>Locked on by Incognito mode.</span>
          </div>
        )}
        {!cfg && !isIncognito ? (
          <p className="text-xs text-gray-500 text-center py-4">Loading…</p>
        ) : (
          <div className="space-y-0.5">
            {ITEMS.map(item => (
              <button
                key={item.key}
                onClick={() => toggle(item.key)}
                disabled={isIncognito}
                className={`w-full flex items-start gap-3 px-2 py-2 rounded text-left ${isIncognito ? 'cursor-not-allowed opacity-70' : 'hover:bg-white/5'}`}
              >
                <div className={`mt-0.5 w-9 h-5 rounded-full flex-shrink-0 transition-colors ${isOn(item.key) ? 'bg-green-500' : 'bg-white/15'}`}>
                  <div className={`w-4 h-4 mt-0.5 rounded-full bg-white transition-transform ${isOn(item.key) ? 'translate-x-[18px]' : 'translate-x-0.5'}`} />
                </div>
                <div className="flex-1 min-w-0">
                  <div className="text-xs text-white">{item.label}</div>
                  <div className="text-[11px] text-gray-500">{item.desc}</div>
                </div>
              </button>
            ))}
          </div>
        )}
      </RightDockPanel>
    </div>
  );
}

export default PrivacyButton;
