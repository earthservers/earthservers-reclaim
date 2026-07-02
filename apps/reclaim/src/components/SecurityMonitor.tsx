// Security monitor — toolbar button + right-dock panel.
//
// This is the DETERMINISTIC foundation (Phase 6): a posture header over the real
// boundaries and a live event feed from the backend monitor. NO LLM is involved
// here — every value is a checkable fact from the kernel/boundary layer. The
// AI-curator sections (Phase 7) are an optional OVERLAY added below the feed; the
// header and feed must keep working with the curator disabled.
//
// Registers in the standardized right-side dock (same mutual-exclusivity as the
// other panels: opening this closes the others).

import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke, listen, isTauri } from '../lib/tauri';
import { RightDockPanel } from '../lib/rightDock';
import { SecurityCuratorSections, ExplainButton, curatorAvailable } from './SecurityCurator';

// The Security panel is content-heavy (posture header + live feed + AI overlay),
// so it docks wider than the standard panels. App insets the page surface to match.
const SECURITY_PANEL_WIDTH = 680;

// Mirrors security::SecurityEvent (serde field names; enums are kebab-case).
export type PriorityTag = 'boundary' | 'hardening' | 'hygiene' | 'defense-in-depth' | 'real-signal';
export type Severity = 'info' | 'notice' | 'warning' | 'critical';
export interface SecurityEvent {
  ts: string;
  category: string;
  tag: PriorityTag;
  severity: Severity;
  title: string;
  detail: string;
  origin: string | null;
  decision: string | null;
}

interface Posture {
  webkit_sandbox: boolean;
  helper_confinement: boolean;
  hardened_malloc: boolean;
  compiled_hardening: boolean;
  integrity: string; // verified | failed | not-configured | unknown
}

type Light = 'green' | 'amber' | 'red' | 'neutral';

const TAG_LABEL: Record<PriorityTag, string> = {
  boundary: 'BOUNDARY',
  hardening: 'HARDENING',
  hygiene: 'HYGIENE',
  'defense-in-depth': 'DEFENSE-IN-DEPTH',
  'real-signal': 'REAL SIGNAL',
};

// Honest per-tag color: a BOUNDARY is the real wall; DEFENSE-IN-DEPTH is a
// tripwire a privileged local attacker can bypass — never styled as a wall.
const TAG_STYLE: Record<PriorityTag, string> = {
  boundary: 'bg-emerald-500/15 text-emerald-300 border-emerald-500/30',
  hardening: 'bg-sky-500/15 text-sky-300 border-sky-500/30',
  hygiene: 'bg-violet-500/15 text-violet-300 border-violet-500/30',
  'defense-in-depth': 'bg-amber-500/15 text-amber-300 border-amber-500/30',
  'real-signal': 'bg-slate-500/15 text-slate-300 border-slate-500/30',
};

const SEV_DOT: Record<Severity, string> = {
  info: 'bg-slate-400',
  notice: 'bg-sky-400',
  warning: 'bg-amber-400',
  critical: 'bg-red-500',
};

const LIGHT_DOT: Record<Light, string> = {
  green: 'bg-emerald-500',
  amber: 'bg-amber-400',
  red: 'bg-red-500',
  neutral: 'bg-slate-500',
};

function PostureRow({ light, label, value, tag }: { light: Light; label: string; value: string; tag: string }) {
  return (
    <div className="flex items-center gap-2 py-1">
      <span className={`w-2.5 h-2.5 rounded-full flex-shrink-0 ${LIGHT_DOT[light]}`} />
      <span className="text-xs text-white flex-1 min-w-0 truncate">{label}</span>
      <span className="text-[11px] text-gray-400">{value}</span>
      <span className="text-[9px] text-gray-600 font-mono">{tag}</span>
    </div>
  );
}

export function SecurityMonitor({ activeEngine }: { activeEngine?: string }) {
  // Normalize the engine id ('servo' | 'webkitgtk' | 'internal' | undefined) into
  // the safety story: Servo = memory-safe Rust; anything WebKit = C/C++.
  const engine: 'servo' | 'webkit' | undefined =
    activeEngine === 'servo' ? 'servo' : activeEngine?.includes('webkit') ? 'webkit' : undefined;
  const [open, setOpen] = useState(false);
  const [posture, setPosture] = useState<Posture | null>(null);
  const [events, setEvents] = useState<SecurityEvent[]>([]);
  const [aiAvailable, setAiAvailable] = useState(false);
  const wrapRef = useRef<HTMLDivElement>(null);

  const refresh = useCallback(async () => {
    if (!isTauri()) return;
    try {
      const [p, ev] = await Promise.all([
        invoke<Posture>('security_posture'),
        invoke<SecurityEvent[]>('security_events', { limit: 200 }),
      ]);
      setPosture(p);
      setEvents(ev);
    } catch (e) {
      console.error('security panel load failed', e);
    }
  }, []);

  useEffect(() => {
    if (open) {
      refresh();
      // Probe curator availability once per open; the deterministic panel never
      // depends on the result.
      curatorAvailable().then(setAiAvailable);
    }
  }, [open, refresh]);

  // Live feed: prepend new events as they arrive (works whether or not the panel
  // is open, so the badge stays current).
  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;
    listen<SecurityEvent>('security-event', ({ payload }) => {
      setEvents(prev => [payload, ...prev].slice(0, 300));
    }).then(u => { unlisten = u; });
    return () => unlisten?.();
  }, []);

  // Badge: number of warning/critical events (the things worth a glance).
  const alertCount = events.filter(e => e.severity === 'warning' || e.severity === 'critical').length;

  // Overall posture light: red if a real wall is down or integrity failed; amber
  // if a layer is off; else green.
  const overall: Light = (() => {
    if (!posture) return 'neutral';
    if (!posture.webkit_sandbox || posture.integrity === 'failed') return 'red';
    if (!posture.helper_confinement || !posture.hardened_malloc) return 'amber';
    return 'green';
  })();

  const integrityLight: Light =
    posture?.integrity === 'verified' ? 'green'
      : posture?.integrity === 'failed' ? 'red'
        : 'neutral';

  return (
    <div ref={wrapRef} className="relative" data-no-drag>
      <button
        onClick={() => setOpen(o => !o)}
        title="Security monitor"
        className="relative p-1.5 rounded-lg text-white/60 hover:bg-white/10 hover:text-white transition-all cursor-pointer"
      >
        {/* radar / activity icon */}
        <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z" />
        </svg>
        {alertCount > 0 && (
          <span className="absolute -top-0.5 -right-0.5 min-w-[16px] h-4 px-1 rounded-full bg-amber-500 text-black text-[10px] font-semibold flex items-center justify-center">
            {alertCount > 99 ? '99+' : alertCount}
          </span>
        )}
      </button>

      <RightDockPanel
        id="security-monitor"
        open={open}
        width={SECURITY_PANEL_WIDTH}
        title="Security"
        subtitle={aiAvailable
          ? 'Live posture & events. Readings are deterministic; AI sections are labeled & advisory.'
          : 'Live posture & events. Deterministic — no AI active in this section.'}
        onClose={() => setOpen(false)}
      >
        {/* ---- Posture header (top) — real boundaries, honest tags. No LLM. ---- */}
        <div className="mb-3 rounded-lg border border-white/10 bg-black/20 p-2.5">
          <div className="flex items-center gap-2 mb-1.5">
            <span className={`w-3 h-3 rounded-full ${LIGHT_DOT[overall]}`} />
            <span className="text-sm font-medium text-white">Posture</span>
            <span className="text-[11px] text-gray-500 ml-auto">
              {overall === 'green' ? 'All boundaries up' : overall === 'amber' ? 'A layer is off' : overall === 'red' ? 'Attention needed' : '—'}
            </span>
          </div>
          <PostureRow
            light={engine === 'servo' ? 'green' : engine === 'webkit' ? 'amber' : 'neutral'}
            label="Active page engine"
            value={engine === 'servo' ? 'Servo (Rust, memory-safe)' : engine === 'webkit' ? 'WebKit (C/C++)' : '—'}
            tag="BOUNDARY"
          />
          <PostureRow
            light={posture?.webkit_sandbox ? 'green' : 'red'}
            label="WebKit renderer sandbox"
            value={posture?.webkit_sandbox ? 'on (bubblewrap+seccomp)' : 'OFF'}
            tag="BOUNDARY"
          />
          <PostureRow
            light={posture?.helper_confinement ? 'green' : 'amber'}
            label="Helper confinement"
            value={posture?.helper_confinement ? 'on (Landlock+seccomp)' : 'off'}
            tag="BOUNDARY"
          />
          <PostureRow
            light={posture?.hardened_malloc ? 'green' : 'neutral'}
            label="Hardened allocator"
            value={posture?.hardened_malloc ? 'active' : 'not loaded'}
            tag="HARDENING"
          />
          <PostureRow
            light={posture?.compiled_hardening ? 'green' : 'neutral'}
            label="Compile-time hardening"
            value={posture?.compiled_hardening ? 'RELRO/PIE/NX' : '—'}
            tag="HARDENING"
          />
          <PostureRow
            light={integrityLight}
            label="Integrity self-check"
            value={posture?.integrity ?? 'unknown'}
            tag="DEFENSE-IN-DEPTH"
          />
        </div>

        {/* ---- AI curator overlay (Phase 7) — clearly labeled, optional. The
                header above and the feed below never depend on it. ---- */}
        <SecurityCuratorSections available={aiAvailable} />

        {/* ---- Live event feed (below) — the source of truth. No LLM. ---- */}
        <div className="flex items-center justify-between mt-1 mb-1.5">
          <span className="text-xs font-medium text-white">Event feed</span>
          <button onClick={refresh} className="text-[11px] text-gray-400 hover:text-white">Refresh</button>
        </div>
        <div className="space-y-1">
          {events.length === 0 && (
            <p className="text-xs text-gray-500 text-center py-6">No security events recorded.</p>
          )}
          {events.map((e, i) => (
            <div key={`${e.ts}-${i}`} className="rounded border border-white/5 bg-white/[0.02] px-2 py-1.5">
              <div className="flex items-center gap-1.5">
                <span className={`w-2 h-2 rounded-full flex-shrink-0 ${SEV_DOT[e.severity]}`} />
                <span className="text-xs text-white flex-1 min-w-0 truncate">{e.title}</span>
                <span className={`text-[8px] font-mono px-1 py-0.5 rounded border ${TAG_STYLE[e.tag]}`}>{TAG_LABEL[e.tag]}</span>
              </div>
              {e.detail && <div className="text-[11px] text-gray-400 mt-0.5 break-words">{e.detail}</div>}
              <div className="flex items-center gap-2 mt-0.5 text-[10px] text-gray-600">
                {e.origin && <span className="truncate">{e.origin}</span>}
                {e.decision && (
                  <span className={e.decision === 'denied' ? 'text-red-400' : 'text-emerald-400'}>{e.decision}</span>
                )}
                <span className="ml-auto">{new Date(e.ts).toLocaleTimeString()}</span>
              </div>
              {/* Per-event AI explain (Phase 7) — user-triggered, advisory; absent
                  when the curator is unavailable. */}
              <ExplainButton event={e} available={aiAvailable} />
            </div>
          ))}
        </div>
      </RightDockPanel>
    </div>
  );
}

export default SecurityMonitor;
