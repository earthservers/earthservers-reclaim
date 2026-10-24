// Local AI hub — the home of Reclaim's on-device AI. Three tabs:
//   • AI       — all AI controls (curator/assistant toggles) + a ChatGPT/Claude
//                style assistant with multiple, persisted chat sessions.
//   • History  — the EarthMemory index of pages the curator has journaled.
//   • Searches — saved searches & the local search history (same lists as the
//                right-dock Searches panel).
// Everything here runs locally; nothing leaves the device.

import { useState, useEffect, useRef, useCallback } from 'react';
import { invoke, listen, openFileDialog, isTauri } from '../lib/tauri';
import type { AiSettings } from '../App';
import { MemoryManager } from './MemoryManager';
import { SearchHistoryList, type RunSearch } from './SearchHistoryPanel';
import { VaultAutofill } from './VaultAutofill';

interface LocalAIHubProps {
  profileId: number | null;
  settings: AiSettings;
  onChange: (next: Partial<AiSettings>) => void;
  onOpenMemory: () => void;             // legacy deep-link; History is now a tab
  onOpenUrl?: (url: string) => void;    // open an indexed page in Search
  onRunSearch?: RunSearch;              // run a saved/recent search in the Search service
  isIncognito?: boolean;                // disabled in incognito / on the Incognito profile
}

// A turn in the transcript. `thinking` and `steps` are kept SEPARATE from
// `content` so the UI can render the model's reasoning as its own collapsible
// block — flattening them into the text made it an unreadable "Thinking" blob.
interface ChatMessage {
  role: 'user' | 'assistant';
  content: string;
  thinking?: string;
  steps?: string;
  files?: AttachedFile[];   // files sent with this turn (user messages)
}
interface ChatSession { id: string; title: string; messages: ChatMessage[]; updatedAt: number; }

// A file/directory pulled in for the model to look at — either drag-dropped
// into the composer, picked via the file dialog, or resolved from a path
// mentioned in the message when filesystem access is on.
interface AttachedFile {
  path: string;
  name: string;
  kind: 'file' | 'dir' | 'error';
  content: string;
  bytes: number;
  truncated: boolean;
}
interface AssistantStatus { ollamaRunning: boolean; model: string; vramMb: number; }
interface ResearchStatus { provider: string; searxngUrl: string; searxngAvailable: boolean; }

// ── Research-mode settings (localStorage) ─────────────────────────────────────
const RESEARCH_ENABLED_KEY = 'reclaim.research.enabled';
const SEARXNG_URL_KEY = 'reclaim.research.searxngUrl';
const DEFAULT_SEARXNG = 'http://localhost:8888';
const loadResearchEnabled = () => { try { return localStorage.getItem(RESEARCH_ENABLED_KEY) === '1'; } catch { return false; } };
const loadSearxngUrl = () => { try { return localStorage.getItem(SEARXNG_URL_KEY) || DEFAULT_SEARXNG; } catch { return DEFAULT_SEARXNG; } };

// ── Filesystem access (localStorage) ──────────────────────────────────────────
// OFF by default: the assistant only ever touches disk when you turn this on, or
// when you explicitly attach a file.
const FS_ACCESS_KEY = 'reclaim.assistant.fsAccess';
const loadFsAccess = () => { try { return localStorage.getItem(FS_ACCESS_KEY) === '1'; } catch { return false; } };

// ── Context window (localStorage) ─────────────────────────────────────────────
// Ollama defaults to 4096 tokens (~16KB of text) and silently drops the FRONT of
// the prompt — system prompt and attached files first — once you exceed it. That
// makes long attachments look like they worked while the model never saw them.
//
// The default here is 16K, which is what a 14B model actually fits alongside its
// weights on a 16GB card. Measured on an RTX 4060 Ti (16GB) + qwen2.5:14b: 16384
// loads, 24576+ kills the llama runner outright. Overshooting fails at LOAD time
// (HTTP 500, "llama runner process has terminated"), so the hints below steer
// toward what fits rather than what the model theoretically supports.
const NUM_CTX_KEY = 'reclaim.assistant.numCtx';
const DEFAULT_NUM_CTX = 16384;
const CTX_CHOICES: { value: number; label: string; hint: string }[] = [
  { value: 4096,   label: '4K',   hint: '~16 KB — Ollama default; truncates most attachments' },
  { value: 8192,   label: '8K',   hint: '~32 KB — short files, minimal VRAM' },
  { value: 16384,  label: '16K',  hint: '~65 KB — recommended: fits a 14B model on a 16 GB card' },
  { value: 32768,  label: '32K',  hint: '~130 KB — needs a smaller model (≤8B) or more than 16 GB VRAM' },
  { value: 65536,  label: '64K',  hint: '~260 KB — large codebases; small models or 24 GB+ only' },
  { value: 131072, label: '128K', hint: '~520 KB — maximum; only for small models with lots of VRAM' },
];
const loadNumCtx = () => {
  try {
    const n = parseInt(localStorage.getItem(NUM_CTX_KEY) || '', 10);
    return Number.isFinite(n) && n >= 2048 ? n : DEFAULT_NUM_CTX;
  } catch { return DEFAULT_NUM_CTX; }
};

const humanBytes = (n: number) => n >= 1 << 20 ? `${(n / (1 << 20)).toFixed(1)} MB`
  : n >= 1 << 10 ? `${Math.round(n / 1024)} KB` : `${n} B`;

// The model's reasoning, as its own collapsible block. Collapsed by default —
// click to read the whole thing (it scrolls rather than flooding the transcript).
function ThinkingBlock({ text, live }: { text: string; live: boolean }) {
  const [open, setOpen] = useState(false);
  const bodyRef = useRef<HTMLDivElement>(null);

  // While streaming and expanded, keep the newest reasoning in view.
  useEffect(() => {
    if (open && live && bodyRef.current) bodyRef.current.scrollTop = bodyRef.current.scrollHeight;
  }, [text, open, live]);

  return (
    <div className="mb-2 rounded-xl border border-white/10 bg-black/20 overflow-hidden">
      <button
        onClick={() => setOpen(o => !o)}
        aria-expanded={open}
        className="w-full flex items-center gap-2 px-3 py-1.5 text-xs text-[var(--text-muted-color)] hover:text-white transition-colors"
      >
        <svg
          className={`w-3 h-3 shrink-0 transition-transform ${open ? 'rotate-90' : ''}`}
          fill="none" viewBox="0 0 24 24" stroke="currentColor"
        >
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
        </svg>
        <span>💭</span>
        <span className="font-medium">{live ? 'Thinking…' : 'Thought process'}</span>
        <span className="ml-auto opacity-60">{open ? 'hide' : 'show'}</span>
      </button>
      {open && (
        <div
          ref={bodyRef}
          className="px-3 pb-2 max-h-64 overflow-y-auto text-xs leading-relaxed text-[var(--text-muted-color)] whitespace-pre-wrap border-t border-white/5 pt-2"
        >
          {text}
        </div>
      )}
    </div>
  );
}

// Compact chip for an attached file / directory.
function FileChip({ file, onRemove }: { file: AttachedFile; onRemove?: () => void }) {
  const icon = file.kind === 'dir'
    ? 'M3 7a2 2 0 012-2h4l2 2h7a2 2 0 012 2v7a2 2 0 01-2 2H5a2 2 0 01-2-2V7z'
    : 'M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z';
  return (
    <span
      title={file.kind === 'error' ? `${file.path} — ${file.content}` : file.path}
      className={`inline-flex items-center gap-1.5 px-2 py-1 rounded-lg border text-xs max-w-[220px] ${
        file.kind === 'error'
          ? 'border-red-500/40 bg-red-500/10 text-red-300'
          : 'border-white/10 bg-white/5 text-[var(--text-color)]'
      }`}
    >
      <svg className="w-3.5 h-3.5 shrink-0 opacity-70" fill="none" viewBox="0 0 24 24" stroke="currentColor">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d={icon} />
      </svg>
      <span className="truncate">{file.name}</span>
      {file.kind === 'file' && file.bytes > 0 && (
        <span className="opacity-50 shrink-0">{humanBytes(file.bytes)}</span>
      )}
      {onRemove && (
        <button onClick={onRemove} title="Remove" className="shrink-0 opacity-60 hover:opacity-100 hover:text-red-400">
          <svg className="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      )}
    </span>
  );
}

// ── Chat-session persistence (encrypted, in SQLite, per profile) ──────────────
// Chats used to live in localStorage, which is scoped to the webview ORIGIN — so
// they silently vanished when the origin changed (dev server vs. tauri:// vs. the
// packaged asset server) or site data was cleared, even though the journaled
// copies in History survived. They now live in the `ai_chats` table, encrypted at
// rest (AES-GCM) so reading the .db file doesn't expose conversations.
//
// The key is derived from the profile's AI/History password when one is set,
// otherwise from the OS-keyring secret. `sessionPassword` holds the unlock
// password in memory ONLY (never persisted) so the key can be derived; it's
// cleared when the tab re-locks.

/// The AI-tab password for this session, per profile. Module-scoped so it
/// survives the component remounting, and in-memory only — it is never written
/// to disk, which is the whole point of password-derived encryption.
const sessionPasswords = new Map<number, string>();
const getSessionPassword = (profileId: number | null) =>
  sessionPasswords.get(profileId ?? 1) ?? null;

interface StoredChat { id: string; title: string; messages: string; updatedAt: number; }

async function loadSessions(profileId: number | null): Promise<ChatSession[]> {
  try {
    const rows = await invoke<StoredChat[]>('ai_chats_load', {
      profileId: profileId ?? 1,
      password: getSessionPassword(profileId),
    });
    return rows.map(r => ({
      id: r.id,
      title: r.title,
      messages: (() => { try { return JSON.parse(r.messages) as ChatMessage[]; } catch { return []; } })(),
      updatedAt: r.updatedAt,
    }));
  } catch {
    return [];
  }
}

async function saveSession(profileId: number | null, s: ChatSession) {
  try {
    await invoke('ai_chats_save', {
      profileId: profileId ?? 1,
      chat: { id: s.id, title: s.title, messages: JSON.stringify(s.messages), updatedAt: s.updatedAt },
      password: getSessionPassword(profileId),
    });
  } catch { /* best-effort; the in-memory session still works this run */ }
}
const titleFrom = (text: string) => text.replace(/\s+/g, ' ').trim().slice(0, 40) || 'New chat';

// ── The sessioned assistant workspace (ChatGPT/Claude style) ──────────────────
function AssistantWorkspace({ profileId, journal }: { profileId: number | null; journal: boolean }) {
  const [status, setStatus] = useState<AssistantStatus | null>(null);
  const [models, setModels] = useState<string[]>([]);
  const [selectedModel, setSelectedModel] = useState('');
  const [sessions, setSessions] = useState<ChatSession[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [input, setInput] = useState('');
  const [busy, setBusy] = useState(false);
  const [research, setResearch] = useState<boolean>(loadResearchEnabled);
  const [searxngUrl, setSearxngUrl] = useState<string>(loadSearxngUrl);
  const [researchStatus, setResearchStatus] = useState<ResearchStatus | null>(null);
  const [showResearchSettings, setShowResearchSettings] = useState(false);
  const [fsAccess, setFsAccess] = useState<boolean>(loadFsAccess);
  const [numCtx, setNumCtx] = useState<number>(loadNumCtx);
  const [attachments, setAttachments] = useState<AttachedFile[]>([]);
  const [dragOver, setDragOver] = useState(false);
  const [attachError, setAttachError] = useState('');
  const scrollRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const idCounter = useRef(0);

  const active = sessions.find(s => s.id === activeId) ?? null;

  // Probe the local model + installed models once, and default the picker to a
  // model that's actually INSTALLED (prefer the GPU-recommended one if pulled,
  // else the first installed) — so we never auto-select a model that 404s.
  useEffect(() => {
    Promise.all([
      invoke<AssistantStatus>('assistant_status').catch(() => null),
      invoke<string[]>('assistant_models').catch(() => [] as string[]),
    ]).then(([s, ms]) => {
      if (s) setStatus(s);
      setModels(ms);
      setSelectedModel(prev => {
        if (prev) return prev;
        if (s && ms.includes(s.model)) return s.model; // recommended is installed
        if (ms.length > 0) return ms[0];               // fall back to an installed one
        return s?.model ?? '';                         // nothing installed → show recommended
      });
    });
  }, []);

  // Persist research settings + (re)check which search provider is available.
  useEffect(() => { try { localStorage.setItem(RESEARCH_ENABLED_KEY, research ? '1' : '0'); } catch { /* ignore */ } }, [research]);
  useEffect(() => { try { localStorage.setItem(SEARXNG_URL_KEY, searxngUrl); } catch { /* ignore */ } }, [searxngUrl]);
  useEffect(() => {
    if (!research) return;
    let cancelled = false;
    invoke<ResearchStatus>('research_status', { searxngUrl })
      .then(s => { if (!cancelled) setResearchStatus(s); })
      .catch(() => { if (!cancelled) setResearchStatus(null); });
    return () => { cancelled = true; };
  }, [research, searxngUrl]);

  // Load sessions for this profile (async — they're decrypted in the backend).
  // Also runs the one-time journal import, so conversations whose localStorage
  // copy was lost are rebuilt from the History entries that survived.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        await invoke('ai_chats_import_journal', {
          profileId: profileId ?? 1,
          password: getSessionPassword(profileId),
        });
      } catch { /* import is best-effort — never block loading on it */ }
      const s = await loadSessions(profileId);
      if (cancelled) return;
      setSessions(s);
      setActiveId(prev => prev ?? s[0]?.id ?? null);
    })();
    return () => { cancelled = true; };
  }, [profileId]);

  // Persist only the sessions that actually changed, and not more than once
  // per pause in activity. Every streamed token bumps updatedAt, and each save
  // runs an Argon2 key derivation plus a SQLite write in the backend, so saving
  // per token would hammer the CPU for the whole reply. A trailing debounce
  // coalesces a burst of tokens into one write once the stream goes quiet.
  const savedStamps = useRef(new Map<string, number>());
  const latestRef = useRef({ profileId, sessions });
  latestRef.current = { profileId, sessions };
  const flushSaves = () => {
    const { profileId: pid, sessions: all } = latestRef.current;
    for (const s of all) {
      if (savedStamps.current.get(s.id) !== s.updatedAt) {
        savedStamps.current.set(s.id, s.updatedAt);
        void saveSession(pid, s);
      }
    }
  };
  useEffect(() => {
    const t = window.setTimeout(flushSaves, 400);
    return () => window.clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [profileId, sessions]);
  // Don't lose a pending write when the hub unmounts mid-stream.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  useEffect(() => () => flushSaves(), []);

  // Keep the transcript pinned to the latest message — but ONLY while the user
  // is already at the bottom. Pinning on every streamed chunk yanked the view
  // back down whenever you tried to scroll up mid-answer; scrolling back to the
  // bottom re-engages the pinning.
  const stickToBottom = useRef(true);
  const onTranscriptScroll = () => {
    const el = scrollRef.current;
    if (!el) return;
    stickToBottom.current = el.scrollHeight - el.scrollTop - el.clientHeight < 80;
  };
  useEffect(() => {
    if (!stickToBottom.current) return;
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight });
  }, [active?.messages, busy]);

  // Grow the composer with its content (up to a cap, then it scrolls) so a
  // multi-line prompt is fully visible while you write and edit it.
  useEffect(() => {
    const el = textareaRef.current;
    if (!el) return;
    el.style.height = 'auto';                       // reset so shrinking works too
    el.style.height = `${Math.min(el.scrollHeight, 320)}px`;
  }, [input]);

  useEffect(() => { try { localStorage.setItem(FS_ACCESS_KEY, fsAccess ? '1' : '0'); } catch { /* ignore */ } }, [fsAccess]);
  useEffect(() => { try { localStorage.setItem(NUM_CTX_KEY, String(numCtx)); } catch { /* ignore */ } }, [numCtx]);

  // Read the given paths through the backend and add them as attachments,
  // de-duplicated by path.
  const attachPaths = useCallback(async (paths: string[]) => {
    if (paths.length === 0) return;
    setAttachError('');
    try {
      const read = await invoke<AttachedFile[]>('assistant_read_files', { paths });
      setAttachments(prev => {
        const seen = new Set(prev.map(f => f.path));
        return [...prev, ...read.filter(f => !seen.has(f.path))];
      });
    } catch (e) {
      setAttachError(String(e).replace(/^.*?:\s*/, ''));
    }
  }, []);

  // Native drag-and-drop. In Tauri the webview swallows HTML5 file drops, so the
  // real paths arrive via the window's drag-drop event instead.
  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    (async () => {
      const { getCurrentWebview } = await import('@tauri-apps/api/webview');
      const un = await getCurrentWebview().onDragDropEvent((event) => {
        const p = event.payload as { type: string; paths?: string[] };
        if (p.type === 'over' || p.type === 'enter') setDragOver(true);
        else if (p.type === 'drop') { setDragOver(false); attachPaths(p.paths ?? []); }
        else setDragOver(false);
      });
      if (cancelled) un(); else unlisten = un;
    })().catch(() => { /* drag-drop unavailable — the picker button still works */ });
    return () => { cancelled = true; unlisten?.(); };
  }, [attachPaths]);

  const pickFiles = async (directory: boolean) => {
    try {
      const paths = await openFileDialog({ multiple: true, directory });
      await attachPaths(paths);
    } catch (e) {
      setAttachError(String(e).replace(/^.*?:\s*/, ''));
    }
  };

  const newId = () => `${Date.now().toString(36)}-${idCounter.current++}`;

  const newChat = () => {
    const s: ChatSession = { id: newId(), title: 'New chat', messages: [], updatedAt: Date.now() };
    setSessions(prev => [s, ...prev]);
    setActiveId(s.id);
    setInput('');
  };

  const deleteChat = (id: string) => {
    setSessions(prev => {
      const next = prev.filter(s => s.id !== id);
      if (id === activeId) setActiveId(next[0]?.id ?? null);
      return next;
    });
    savedStamps.current.delete(id);
    void invoke('ai_chats_delete', { profileId: profileId ?? 1, id }).catch(() => {});
  };

  const send = async () => {
    const text = input.trim();
    if (!text || busy || !status?.ollamaRunning) return;

    // Resolve the target session, creating one if none is active.
    let sessionId = activeId;
    let history: ChatMessage[] = active?.messages ?? [];
    if (!sessionId || !active) {
      const s: ChatSession = { id: newId(), title: titleFrom(text), messages: [], updatedAt: Date.now() };
      sessionId = s.id;
      history = [];
      setSessions(prev => [s, ...prev]);
      setActiveId(s.id);
    }
    const targetId = sessionId;

    // Gather the files this turn can see: everything explicitly attached, plus —
    // when filesystem access is on — any path mentioned in the message itself.
    let turnFiles: AttachedFile[] = attachments;
    if (fsAccess) {
      try {
        const resolved = await invoke<AttachedFile[]>('assistant_resolve_paths', { message: text });
        const seen = new Set(turnFiles.map(f => f.path));
        turnFiles = [...turnFiles, ...resolved.filter(f => !seen.has(f.path))];
      } catch { /* resolution is best-effort; send the explicit attachments anyway */ }
    }

    // Append the user turn + an empty assistant turn to stream into. Name a
    // fresh chat after its first message.
    setSessions(prev => prev.map(s => s.id === targetId ? {
      ...s,
      title: s.messages.length === 0 ? titleFrom(text) : s.title,
      messages: [
        ...s.messages,
        { role: 'user', content: text, ...(turnFiles.length ? { files: turnFiles } : {}) },
        { role: 'assistant', content: '' },
      ],
      updatedAt: Date.now(),
    } : s));
    setInput('');
    setAttachments([]);
    setBusy(true);
    // Sending re-engages pinning so your own message (and the incoming answer)
    // scroll into view even if you'd scrolled up earlier.
    stickToBottom.current = true;

    // Stream tokens into the trailing (empty) assistant message of this session.
    // In research mode, `research-step` lines (🔎 searching / 📄 reading) are
    // shown inline above the streamed answer.
    let steps = '';
    let thinking = '';
    let acc = '';
    // Reasoning and research steps are stored as their own fields — NOT folded
    // into the answer text — so the transcript can render the thinking as a
    // separate, expandable block.
    const writeTail = (patch: Partial<ChatMessage>) => setSessions(prev => prev.map(s => s.id === targetId ? {
      ...s,
      messages: s.messages.map((m, i) => i === s.messages.length - 1 ? { ...m, role: 'assistant', ...patch } : m),
      updatedAt: Date.now(),
    } : s));

    const unlistenChunk = await listen<string>('assistant-chunk', (e) => { acc += e.payload; writeTail({ content: acc }); });
    const unlistenThinking = await listen<string>('assistant-thinking', (e) => { thinking += e.payload; writeTail({ thinking }); });
    const unlistenStep = research
      ? await listen<string>('research-step', (e) => { steps += (steps ? '\n' : '') + e.payload; writeTail({ steps }); })
      : undefined;
    try {
      await invoke(research ? 'assistant_research_stream' : 'assistant_chat_stream', {
        profileId: profileId ?? 1,
        message: text,
        history,
        model: selectedModel || null,
        journal,
        files: turnFiles.length ? turnFiles : null,
        numCtx,
        ...(research ? { searxngUrl } : {}),
      });
    } catch (e) {
      // Keep whatever streamed; only show the error if nothing came through.
      if (!acc) writeTail({ content: `⚠ ${String(e).replace(/^.*?:\s*/, '')}` });
    } finally {
      unlistenChunk();
      unlistenThinking();
      unlistenStep?.();
      setBusy(false);
    }
  };

  return (
    <div className="flex rounded-2xl border border-white/10 bg-theme-card/60 overflow-hidden h-[68vh] min-h-[460px]">
      {/* Sessions sidebar */}
      <div className="w-56 shrink-0 border-r border-white/10 flex flex-col bg-black/20">
        <button
          onClick={newChat}
          className="m-2 px-3 py-2 rounded-lg text-sm font-medium bg-[var(--primary-color)] text-white hover:opacity-90 flex items-center justify-center gap-2"
        >
          <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" /></svg>
          New chat
        </button>
        <div className="flex-1 min-h-0 overflow-y-auto px-2 pb-2 space-y-1">
          {sessions.length === 0 && (
            <p className="text-xs text-[var(--text-muted-color)] px-2 py-3">No conversations yet.</p>
          )}
          {sessions.map(s => (
            <div
              key={s.id}
              onClick={() => setActiveId(s.id)}
              className={`group flex items-center gap-1 px-2 py-2 rounded-lg cursor-pointer text-sm ${
                s.id === activeId ? 'bg-white/10 text-white' : 'text-[var(--text-muted-color)] hover:bg-white/5'
              }`}
            >
              <svg className="w-3.5 h-3.5 shrink-0 opacity-70" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.86 9.86 0 01-4-.83L3 20l1.17-3.5A7.94 7.94 0 013 12c0-4.418 4.03-8 9-8s9 3.582 9 8z" /></svg>
              <span className="flex-1 truncate">{s.title || 'New chat'}</span>
              <button
                onClick={(e) => { e.stopPropagation(); deleteChat(s.id); }}
                title="Delete chat"
                className="opacity-0 group-hover:opacity-100 text-gray-400 hover:text-red-400 transition-opacity"
              >
                <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" /></svg>
              </button>
            </div>
          ))}
        </div>
      </div>

      {/* Chat area */}
      <div className="flex-1 min-w-0 flex flex-col">
        {/* Header: model + status */}
        <div className="flex items-center justify-between gap-2 px-4 py-2 border-b border-white/10 text-xs">
          {status?.ollamaRunning ? (
            <div className="flex items-center gap-2 min-w-0">
              <select
                value={selectedModel}
                onChange={e => setSelectedModel(e.target.value)}
                className="bg-gray-800 border border-gray-600 rounded px-1.5 py-1 text-xs text-white max-w-[200px]"
              >
                {status.model && !models.includes(status.model) && (
                  <option value={status.model}>{status.model} (recommended)</option>
                )}
                {models.map(m => <option key={m} value={m}>{m}</option>)}
              </select>
              <span className="text-[var(--text-muted-color)] whitespace-nowrap">
                {status.vramMb > 0 ? `${(status.vramMb / 1024).toFixed(0)} GB` : 'CPU'}
              </span>
            </div>
          ) : status ? (
            <span className="text-yellow-400">Ollama not running — run <span className="font-mono">ollama serve</span></span>
          ) : (
            <span className="text-[var(--text-muted-color)]">Checking local model…</span>
          )}

          {/* Research toggle + provider status */}
          <div className="flex items-center gap-2 shrink-0">
            {research && (
              <span className="text-[10px] whitespace-nowrap" title="Where web searches are routed">
                {researchStatus?.searxngAvailable
                  ? <span className="text-green-400">Private search via SearXNG ✓</span>
                  : <span className="text-[var(--text-muted-color)]">using DuckDuckGo <button onClick={() => setShowResearchSettings(v => !v)} className="underline hover:text-white">(run SearXNG for max privacy)</button></span>}
              </span>
            )}
            <button
              onClick={() => setFsAccess(v => !v)}
              title="Filesystem access: let the assistant read files and directories you mention by path. Off by default — attachments always work regardless."
              className={`flex items-center gap-1 px-2 py-1 rounded-lg border transition-colors ${
                fsAccess ? 'border-[var(--primary-color)] bg-[var(--primary-color)]/15 text-white' : 'border-white/10 text-[var(--text-muted-color)] hover:text-white'
              }`}
            >
              <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" /></svg>
              Files
            </button>
            <button
              onClick={() => setResearch(v => !v)}
              title="Web research: search the web and read pages to answer (local reasoning)"
              className={`flex items-center gap-1 px-2 py-1 rounded-lg border transition-colors ${
                research ? 'border-[var(--primary-color)] bg-[var(--primary-color)]/15 text-white' : 'border-white/10 text-[var(--text-muted-color)] hover:text-white'
              }`}
            >
              <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" /></svg>
              Research
            </button>
            <button
              onClick={() => setShowResearchSettings(v => !v)}
              title={`Assistant settings — context window (${CTX_CHOICES.find(c => c.value === numCtx)?.label ?? numCtx}) and search provider`}
              className={`flex items-center gap-1 p-1 hover:text-white ${
                numCtx <= 4096 ? 'text-yellow-400' : 'text-[var(--text-muted-color)]'
              }`}
            >
              <span className="text-[10px] tabular-nums">{CTX_CHOICES.find(c => c.value === numCtx)?.label ?? numCtx}</span>
              <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" /><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" /></svg>
            </button>
          </div>
        </div>

        {/* Settings panel: context window + search provider */}
        {showResearchSettings && (
          <div className="px-4 py-2.5 border-b border-white/10 bg-black/20 text-xs">
            {/* Context window — the setting that decides how much of an attached
                file the model actually sees. */}
            <label className="block text-[var(--text-muted-color)] mb-1">
              Context window — how much text the model can hold at once
            </label>
            <div className="flex items-center gap-2 flex-wrap">
              <div className="inline-flex rounded-lg border border-white/10 bg-black/20 p-0.5">
                {CTX_CHOICES.map(c => (
                  <button
                    key={c.value}
                    onClick={() => setNumCtx(c.value)}
                    title={c.hint}
                    className={`px-2.5 py-1 rounded text-xs transition-colors ${
                      numCtx === c.value ? 'bg-[var(--primary-color)] text-white' : 'text-[var(--text-muted-color)] hover:text-white'
                    }`}
                  >
                    {c.label}
                  </button>
                ))}
              </div>
              {numCtx !== DEFAULT_NUM_CTX && (
                <button onClick={() => setNumCtx(DEFAULT_NUM_CTX)} className="text-[var(--text-muted-color)] hover:text-white underline">
                  Reset
                </button>
              )}
            </div>
            <p className="text-[10px] text-[var(--text-muted-color)] mt-1">
              {CTX_CHOICES.find(c => c.value === numCtx)?.hint}
              {numCtx <= 4096 && ' — attachments will likely be cut off.'}
            </p>
            <p className="text-[10px] text-[var(--text-muted-color)] mt-1 leading-relaxed">
              Bigger windows use more VRAM (the KV cache grows with it). If the model gets slow,
              starts swapping to CPU, or fails to load, drop this back down.
            </p>

            <div className="h-px bg-white/10 my-2.5" />

            <label className="block text-[var(--text-muted-color)] mb-1">SearXNG URL (local instance for private search; falls back to DuckDuckGo)</label>
            <div className="flex items-center gap-2">
              <input
                value={searxngUrl}
                onChange={e => setSearxngUrl(e.target.value)}
                placeholder={DEFAULT_SEARXNG}
                className="flex-1 bg-gray-800 border border-gray-600 rounded px-2 py-1 text-white outline-none focus:border-[var(--primary-color)]"
              />
              <button onClick={() => setSearxngUrl(DEFAULT_SEARXNG)} className="text-[var(--text-muted-color)] hover:text-white">Reset</button>
            </div>
            <p className="text-[10px] text-[var(--text-muted-color)] mt-1">
              {researchStatus?.searxngAvailable ? 'SearXNG reachable — searches stay on your network.' : 'SearXNG not reachable — using DuckDuckGo. Run a local SearXNG (json API enabled) for fully private search.'}
            </p>
          </div>
        )}

        {/* Transcript */}
        <div ref={scrollRef} onScroll={onTranscriptScroll} className="flex-1 min-h-0 overflow-y-auto px-4 py-4 space-y-3">
          {(!active || active.messages.length === 0) && (
            <div className="h-full flex flex-col items-center justify-center text-center text-[var(--text-muted-color)]">
              <svg className="w-10 h-10 mb-3 opacity-60" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.86 9.86 0 01-4-.83L3 20l1.17-3.5A7.94 7.94 0 013 12c0-4.418 4.03-8 9-8s9 3.582 9 8z" /></svg>
              <p className="text-sm">Ask anything. The assistant draws on the pages and media your curator has saved{research ? ', and with Research on it searches the web and reads pages to answer.' : '.'}</p>
            </div>
          )}
          {active?.messages.map((m, i) => {
            const isLast = i === active.messages.length - 1;
            const streaming = busy && isLast;
            return (
              <div key={i} className={`text-sm rounded-2xl px-4 py-2.5 max-w-[80%] ${
                m.role === 'user' ? 'ml-auto bg-[var(--primary-color)]/20 text-white' : 'mr-auto bg-white/5 text-[var(--text-color)]'
              }`}>
                {/* Files sent with this turn */}
                {m.files && m.files.length > 0 && (
                  <div className="flex flex-wrap gap-1.5 mb-2">
                    {m.files.map(f => <FileChip key={f.path} file={f} />)}
                  </div>
                )}
                {/* Research progress (🔎 searching / 📄 reading) */}
                {m.steps && (
                  <div className="mb-2 text-xs text-[var(--text-muted-color)] whitespace-pre-wrap">{m.steps}</div>
                )}
                {/* The model's reasoning — collapsed by default, click to read */}
                {m.thinking?.trim() && <ThinkingBlock text={m.thinking.trim()} live={streaming && !m.content} />}
                {m.content
                  ? <div className="whitespace-pre-wrap">{m.content}</div>
                  : streaming && !m.thinking
                    ? <span className="text-[var(--text-muted-color)]">…thinking</span>
                    : null}
              </div>
            );
          })}
        </div>

        {/* Composer */}
        <div className={`border-t border-white/10 p-3 transition-colors ${dragOver ? 'bg-[var(--primary-color)]/10' : ''}`}>
          {dragOver && (
            <div className="mb-2 rounded-xl border-2 border-dashed border-[var(--primary-color)]/60 py-3 text-center text-xs text-[var(--primary-color)]">
              Drop files or folders to attach
            </div>
          )}

          {/* Pending attachments */}
          {attachments.length > 0 && (
            <div className="flex flex-wrap items-center gap-1.5 mb-2">
              {attachments.map(f => (
                <FileChip
                  key={f.path}
                  file={f}
                  onRemove={() => setAttachments(prev => prev.filter(x => x.path !== f.path))}
                />
              ))}
              <button
                onClick={() => setAttachments([])}
                className="text-xs text-[var(--text-muted-color)] hover:text-white underline ml-1"
              >
                Clear all
              </button>
            </div>
          )}
          {attachError && <p className="text-red-400 text-xs mb-2">{attachError}</p>}

          <div className="flex items-end gap-2">
            {/* Attach file / folder */}
            <div className="flex items-center gap-1 pb-1">
              <button
                onClick={() => pickFiles(false)}
                disabled={busy}
                title="Attach files for the assistant to read"
                className="p-2 rounded-lg text-[var(--text-muted-color)] hover:text-white hover:bg-white/5 disabled:opacity-40"
              >
                <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15.172 7l-6.586 6.586a2 2 0 102.828 2.828l6.414-6.586a4 4 0 00-5.656-5.656l-6.415 6.585a6 6 0 108.486 8.486L20.5 13" /></svg>
              </button>
              <button
                onClick={() => pickFiles(true)}
                disabled={busy}
                title="Attach a folder"
                className="p-2 rounded-lg text-[var(--text-muted-color)] hover:text-white hover:bg-white/5 disabled:opacity-40"
              >
                <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" /></svg>
              </button>
            </div>

            <textarea
              ref={textareaRef}
              value={input}
              onChange={e => setInput(e.target.value)}
              onKeyDown={e => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); send(); } }}
              rows={1}
              placeholder={status?.ollamaRunning ? 'Message your local assistant…  (Enter to send, Shift+Enter for newline)' : 'Start Ollama to chat…'}
              disabled={!status?.ollamaRunning || busy}
              className="flex-1 resize-none overflow-y-auto px-3 py-2 text-sm leading-relaxed bg-gray-800 border border-gray-600 rounded-xl outline-none focus:border-[var(--primary-color)] text-white disabled:opacity-50"
            />
            <button
              onClick={send}
              disabled={!status?.ollamaRunning || busy || !input.trim()}
              className="px-4 py-2 text-sm rounded-xl bg-[var(--primary-color)] text-white hover:opacity-90 disabled:opacity-40 shrink-0"
            >
              Send
            </button>
          </div>

          <p className="mt-1.5 text-[10px] text-[var(--text-muted-color)]">
            {fsAccess
              ? 'Filesystem access is on — paths you mention (e.g. ~/notes or /home/you/project) are read on demand.'
              : 'Drag files or folders here to attach them. Turn on Files to let the assistant open paths you mention.'}
          </p>
        </div>
      </div>
    </div>
  );
}

function Toggle({ on, onClick, disabled }: { on: boolean; onClick: () => void; disabled?: boolean }) {
  return (
    <button
      role="switch"
      aria-checked={on}
      disabled={disabled}
      onClick={onClick}
      className={`relative w-11 h-6 rounded-full transition-colors flex-shrink-0 ${
        on ? 'bg-[var(--primary-color)]' : 'bg-white/15'
      } ${disabled ? 'opacity-40 cursor-not-allowed' : 'cursor-pointer'}`}
    >
      <span
        className={`absolute top-0.5 left-0.5 w-5 h-5 rounded-full bg-white shadow transition-transform ${
          on ? 'translate-x-5' : ''
        }`}
      />
    </button>
  );
}

// ── The AI controls + assistant (the "AI" tab) ────────────────────────────────
function AiTab({ profileId, settings, onChange }: { profileId: number | null; settings: AiSettings; onChange: (next: Partial<AiSettings>) => void; }) {
  return (
    <>
      <div className="grid sm:grid-cols-2 gap-4 mb-4">
        {/* Knowledge Curator */}
        <div className="bg-theme-card/80 border border-white/10 rounded-2xl p-5 backdrop-blur-sm">
          <div className="flex items-start gap-4">
            <div className="flex-1">
              <div className="flex items-center gap-2">
                <h2 className="text-base font-semibold text-white">Knowledge Curator</h2>
                <span className="text-[10px] uppercase tracking-wide px-1.5 py-0.5 rounded bg-white/10 text-[var(--text-muted-color)]">background</span>
              </div>
              <p className="text-sm text-[var(--text-muted-color)] mt-1">
                Quietly summarizes the pages you visit into your History — transparent,
                unbiased, non-judgemental. Skips incognito.
              </p>
            </div>
            <Toggle on={settings.curator} onClick={() => onChange({ curator: !settings.curator })} />
          </div>
        </div>

        {/* AI Assistant */}
        <div className="bg-theme-card/80 border border-white/10 rounded-2xl p-5 backdrop-blur-sm">
          <div className="flex items-start gap-4">
            <div className="flex-1">
              <h2 className="text-base font-semibold text-white">AI Assistant</h2>
              <p className="text-sm text-[var(--text-muted-color)] mt-1">
                A local chat model, auto-selected to fit your hardware. Private and
                fully on-device, and it can use what your curator has saved.
              </p>
            </div>
            <Toggle on={settings.assistant} onClick={() => onChange({ assistant: !settings.assistant })} />
          </div>
        </div>
      </div>

      {settings.assistant ? (
        <AssistantWorkspace profileId={profileId} journal={settings.curator} />
      ) : (
        <div className="rounded-2xl border border-dashed border-white/15 bg-black/10 p-8 text-center text-sm text-[var(--text-muted-color)]">
          Turn on <strong className="text-[var(--text-color)]">AI Assistant</strong> above to start chatting with your local model.
        </div>
      )}

      <p className="text-xs text-[var(--text-muted-color)] mt-4">
        The assistant and curator both use a local Ollama model. Install Ollama and run
        <span className="font-mono"> ollama serve</span> to enable them.
      </p>
    </>
  );
}

// Session unlock for the AI / History tab, PER PROFILE. Module-scoped so it
// survives the component's remounts (the page remounts every time you re-open the
// tab); resets to locked on app restart, like the other password gates. Per
// profile so unlocking one profile doesn't unlock another.
const aiUnlockedProfiles = new Set<number>();

/// Clear all AI/History session unlocks (called on profile switch so it re-gates).
export function lockAllAiSessions() {
  aiUnlockedProfiles.clear();
  // Drop the in-memory chat-decryption passwords too — leaving them would keep
  // another profile's chats decryptable after a switch.
  sessionPasswords.clear();
}

// Modal to set / change / remove the AI / History password (its own unique one).
function AiLockModal({ profileId, hasPw, onClose, onChanged }: { profileId: number; hasPw: boolean; onClose: () => void; onChanged: (hasPw: boolean) => void }) {
  const [mode, setMode] = useState<'set' | 'remove'>('set');
  const [pw, setPw] = useState('');
  const [confirm, setConfirm] = useState('');
  const [current, setCurrent] = useState('');
  const [err, setErr] = useState('');
  const [busy, setBusy] = useState(false);

  const doSet = async () => {
    setErr('');
    if (pw.length < 4) { setErr('Password must be at least 4 characters'); return; }
    if (pw !== confirm) { setErr('Passwords do not match'); return; }
    setBusy(true);
    try {
      // Re-encrypt this profile's chats under the NEW password BEFORE changing it.
      // Skipping this would leave every existing chat keyed to the old password
      // and permanently unreadable. When changing an existing password we need
      // the current one to read those rows; when setting a first password the
      // rows are keyed to the OS keyring (oldPassword: null).
      if (hasPw) {
        const ok = await invoke<boolean>('ai_lock_verify_password', { profileId, password: current });
        if (!ok) { setErr('Current password is incorrect'); setBusy(false); return; }
      }
      await invoke('ai_chats_rekey', {
        profileId,
        oldPassword: hasPw ? current : null,
        newPassword: pw,
      });
      await invoke('ai_lock_set_password', { profileId, password: pw });
      // Do NOT auto-unlock here — onChanged(true) makes the parent lock the tab
      // immediately so the gate actually engages (and proves the password works).
      onChanged(true);
      onClose();
    }
    catch (e) { setErr(String(e).replace(/^.*?:\s*/, '')); } finally { setBusy(false); }
  };
  const doRemove = async () => {
    setErr(''); setBusy(true);
    try {
      // Removing the password drops the gate, so the chats must move back to the
      // keyring key — otherwise they'd stay encrypted under a password that no
      // longer exists anywhere.
      // Rekey FIRST: if it fails the gate is still in place and nothing is lost.
      // Doing it the other way round could drop the gate and then leave the chats
      // encrypted under a password that no longer exists anywhere.
      await invoke('ai_chats_rekey', { profileId, oldPassword: current, newPassword: null });
      await invoke('ai_lock_remove_password', { profileId, password: current });
      onChanged(false);
      onClose();
    }
    catch (e) { setErr(String(e).replace(/^.*?:\s*/, '')); } finally { setBusy(false); }
  };

  const inputCls = 'w-full bg-gray-800 border border-gray-600 rounded-lg px-3 py-2 text-sm text-white outline-none focus:border-[var(--primary-color)]';
  return (
    <div className="fixed inset-0 z-[10002] flex items-center justify-center bg-black/70 backdrop-blur-sm" onClick={onClose}>
      <div className="bg-gray-900 border border-white/10 rounded-2xl p-6 w-full max-w-sm" onClick={e => e.stopPropagation()}>
        <h3 className="text-lg font-semibold text-white mb-1">{hasPw ? 'AI / History password' : 'Lock AI / History'}</h3>
        <p className="text-xs text-[var(--text-muted-color)] mb-4">A unique password just for this tab — separate from your vault, media, and bookmark passwords.</p>
        {hasPw && (
          <div className="inline-flex rounded-lg border border-white/10 bg-black/20 p-0.5 mb-3 text-xs">
            <button onClick={() => { setMode('set'); setErr(''); }} className={`px-3 py-1 rounded ${mode === 'set' ? 'bg-[var(--primary-color)] text-white' : 'text-[var(--text-muted-color)]'}`}>Change</button>
            <button onClick={() => { setMode('remove'); setErr(''); }} className={`px-3 py-1 rounded ${mode === 'remove' ? 'bg-[var(--primary-color)] text-white' : 'text-[var(--text-muted-color)]'}`}>Remove</button>
          </div>
        )}
        {mode === 'set' ? (
          <div className="space-y-2">
            {/* Changing the password re-encrypts existing chats, which needs the
                current one to read them first. */}
            {hasPw && (
              <input type="password" autoFocus placeholder="Current password" value={current} onChange={e => setCurrent(e.target.value)} className={inputCls} />
            )}
            <input type="password" autoFocus={!hasPw} placeholder={hasPw ? 'New password' : 'Password'} value={pw} onChange={e => setPw(e.target.value)} className={inputCls} />
            <input type="password" placeholder="Confirm password" value={confirm} onChange={e => setConfirm(e.target.value)} onKeyDown={e => { if (e.key === 'Enter') doSet(); }} className={inputCls} />
            <p className="text-[10px] text-yellow-400/90 leading-relaxed">
              This password also encrypts your saved chats. If you forget it, those conversations
              cannot be recovered — there is no reset.
            </p>
          </div>
        ) : (
          <input type="password" autoFocus placeholder="Current password" value={current} onChange={e => setCurrent(e.target.value)} onKeyDown={e => { if (e.key === 'Enter') doRemove(); }} className={inputCls} />
        )}
        {err && <p className="text-red-400 text-xs mt-2">{err}</p>}
        <div className="flex justify-end gap-2 mt-4">
          <button onClick={onClose} className="px-3 py-1.5 text-sm text-[var(--text-muted-color)] hover:text-white">Cancel</button>
          {mode === 'set'
            ? <button onClick={doSet} disabled={busy} className="px-3 py-1.5 text-sm rounded-lg bg-[var(--primary-color)] text-white hover:opacity-90 disabled:opacity-40">{hasPw ? 'Save' : 'Set password'}</button>
            : <button onClick={doRemove} disabled={busy} className="px-3 py-1.5 text-sm rounded-lg bg-red-500/80 text-white hover:bg-red-500 disabled:opacity-40">Remove password</button>}
        </div>
      </div>
    </div>
  );
}

export function LocalAIHub({ profileId, settings, onChange, onOpenUrl, onRunSearch, isIncognito }: LocalAIHubProps) {
  const [tab, setTab] = useState<'ai' | 'history' | 'searches'>('ai');
  const [locked, setLocked] = useState<boolean | null>(null); // null = still checking
  const [hasPw, setHasPw] = useState(false);
  const [unlockInput, setUnlockInput] = useState('');
  const [unlockError, setUnlockError] = useState('');
  const [showLockModal, setShowLockModal] = useState(false);
  const pid = profileId ?? 1;

  useEffect(() => {
    invoke<boolean>('ai_lock_has_password', { profileId: pid })
      .then(has => { setHasPw(has); setLocked(has && !aiUnlockedProfiles.has(pid)); })
      .catch(() => { setHasPw(false); setLocked(false); });
  }, [pid]);

  const unlock = async () => {
    try {
      const ok = await invoke<boolean>('ai_lock_verify_password', { profileId: pid, password: unlockInput });
      if (ok) {
        // Hold the password in memory for this session — it derives the key that
        // decrypts this profile's chats. Never persisted.
        sessionPasswords.set(pid, unlockInput);
        aiUnlockedProfiles.add(pid);
        setLocked(false); setUnlockInput(''); setUnlockError('');
      }
      else setUnlockError('Incorrect password');
    } catch { setUnlockError('Could not verify password'); }
  };

  // Lock the tab now: drop this session's unlock so the gate (the `if (locked)`
  // lock screen below) engages, hiding the AI toggles and history.
  // Locking drops the in-memory password too, so the chats genuinely become
  // undecryptable again rather than just being hidden by the UI.
  const lockNow = () => { aiUnlockedProfiles.delete(pid); sessionPasswords.delete(pid); setLocked(true); };

  // Local AI / History is disabled in incognito mode (and on the Incognito
  // profile) — nothing is recorded or indexed there, so there's nothing to show.
  if (isIncognito) {
    return (
      <div className="max-w-sm mx-auto py-16 px-4 text-center">
        <div className="inline-flex w-14 h-14 rounded-full bg-purple-500/15 items-center justify-center mb-4">
          <svg className="w-7 h-7 text-purple-400" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13.875 18.825A10.05 10.05 0 0112 19c-4.478 0-8.268-2.943-9.543-7a9.97 9.97 0 011.563-3.029m5.858.908a3 3 0 114.243 4.243M9.878 9.878l4.242 4.242M9.88 9.88l-3.29-3.29m7.532 7.532l3.29 3.29M3 3l3.59 3.59m0 0A9.953 9.953 0 0112 5c4.478 0 8.268 2.943 9.543 7a10.025 10.025 0 01-4.132 5.411m0 0L21 21" /></svg>
        </div>
        <h1 className="text-xl font-bold text-white">Local AI / History is disabled</h1>
        <p className="text-sm text-[var(--text-muted-color)] mt-1">
          This profile is in incognito mode — browsing isn't recorded and the local AI history is turned off. Switch to a normal profile to use it.
        </p>
      </div>
    );
  }

  if (locked === null) {
    return <div className="max-w-5xl mx-auto py-16 px-4 text-center text-sm text-[var(--text-muted-color)]">Checking…</div>;
  }

  if (locked) {
    return (
      <div className="max-w-sm mx-auto py-16 px-4 text-center">
        <div className="inline-flex w-14 h-14 rounded-full bg-[var(--primary-color)]/15 items-center justify-center mb-4">
          <svg className="w-7 h-7 text-[var(--primary-color)]" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" /></svg>
        </div>
        <h1 className="text-xl font-bold text-white">Local AI / History is locked</h1>
        <p className="text-sm text-[var(--text-muted-color)] mt-1 mb-4">Enter the password to view your AI and browsing history.</p>
        <input
          type="password"
          autoFocus
          value={unlockInput}
          onChange={e => setUnlockInput(e.target.value)}
          onKeyDown={e => { if (e.key === 'Enter') unlock(); }}
          placeholder="Password"
          className="w-full bg-gray-800 border border-gray-600 rounded-lg px-3 py-2 text-sm text-white outline-none focus:border-[var(--primary-color)]"
        />
        {unlockError && <p className="text-red-400 text-xs mt-2">{unlockError}</p>}
        <button onClick={unlock} className="mt-4 w-full px-3 py-2 text-sm rounded-lg bg-[var(--primary-color)] text-white hover:opacity-90">Unlock</button>
        <div className="mt-3 flex justify-center">
          <VaultAutofill profileId={profileId} appKey="local-ai" onFill={pw => setUnlockInput(pw)} />
        </div>
      </div>
    );
  }

  return (
    <div className="max-w-5xl mx-auto py-8 px-4">
      <div className="mb-5 flex items-start justify-between gap-3">
        <div>
          <h1 className="text-2xl font-bold text-white">Local AI / History</h1>
          <p className="text-sm text-[var(--text-muted-color)] mt-1">
            On-device intelligence. Runs locally, stays private, and never phones home.
          </p>
        </div>
        <div className="shrink-0 flex items-center gap-1.5">
          {hasPw && (
            <button
              onClick={() => setShowLockModal(true)}
              title="Change or remove the password"
              className="p-1.5 rounded-lg border border-white/10 text-[var(--text-muted-color)] hover:text-white transition-colors"
            >
              <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" /><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" /></svg>
            </button>
          )}
          <button
            onClick={() => { if (hasPw) lockNow(); else setShowLockModal(true); }}
            title={hasPw ? 'Lock this tab now' : 'Password-protect this tab'}
            className={`flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg border text-xs transition-colors ${
              hasPw ? 'border-[var(--primary-color)]/40 text-[var(--primary-color)] hover:bg-[var(--primary-color)]/10' : 'border-white/10 text-[var(--text-muted-color)] hover:text-white'
            }`}
          >
            <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" /></svg>
            {hasPw ? 'Lock now' : 'Lock'}
          </button>
        </div>
      </div>

      {/* Tab switcher */}
      <div className="inline-flex rounded-xl border border-white/10 bg-black/20 p-1 mb-5">
        {([['ai', 'AI'], ['history', 'History'], ['searches', 'Searches']] as const).map(([id, label]) => (
          <button
            key={id}
            onClick={() => setTab(id)}
            className={`px-4 py-1.5 rounded-lg text-sm font-medium transition-colors ${
              tab === id ? 'bg-[var(--primary-color)] text-white' : 'text-[var(--text-muted-color)] hover:text-white'
            }`}
          >
            {label}
          </button>
        ))}
      </div>

      {tab === 'ai' && <AiTab profileId={profileId} settings={settings} onChange={onChange} />}
      {tab === 'history' && <MemoryManager profileId={profileId} onOpenUrl={onOpenUrl} />}
      {tab === 'searches' && (
        <div className="max-w-xl">
          <SearchHistoryList
            profileId={profileId}
            active={tab === 'searches'}
            current={null}
            onRun={(query, cfg) => onRunSearch?.(query, cfg)}
          />
        </div>
      )}

      {showLockModal && (
        <AiLockModal
          profileId={pid}
          hasPw={hasPw}
          onClose={() => setShowLockModal(false)}
          onChanged={(has) => {
            setHasPw(has);
            // Newly set/changed password → lock now so the gate engages.
            // Password removed → no gate, stay open.
            if (has) lockNow();
            else { aiUnlockedProfiles.delete(pid); setLocked(false); }
          }}
        />
      )}
    </div>
  );
}

export default LocalAIHub;
