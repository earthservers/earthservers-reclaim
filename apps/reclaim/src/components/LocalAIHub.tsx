// Local AI hub — the home of Reclaim's on-device AI. Holds the on/off toggles
// for the two AIs and surfaces the knowledge graph (EarthMemory) the curator
// writes into. Everything here runs locally; nothing leaves the device.

import { useState, useEffect, useRef } from 'react';
import { invoke, listen } from '../lib/tauri';
import type { AiSettings } from '../App';

interface LocalAIHubProps {
  profileId: number | null;
  settings: AiSettings;
  onChange: (next: Partial<AiSettings>) => void;
  onOpenMemory: () => void;
}

interface ChatMessage { role: 'user' | 'assistant'; content: string; }
interface AssistantStatus { ollamaRunning: boolean; model: string; vramMb: number; }

// The local chat assistant — grounded in the user's curated knowledge.
function AssistantChat({ profileId, journal }: { profileId: number | null; journal: boolean }) {
  const [status, setStatus] = useState<AssistantStatus | null>(null);
  const [models, setModels] = useState<string[]>([]);
  const [selectedModel, setSelectedModel] = useState<string>('');
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState('');
  const [busy, setBusy] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    invoke<AssistantStatus>('assistant_status')
      .then(s => { setStatus(s); setSelectedModel(prev => prev || s.model); })
      .catch(() => setStatus(null));
    invoke<string[]>('assistant_models').then(setModels).catch(() => setModels([]));
  }, []);

  useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight });
  }, [messages, busy]);

  const send = async () => {
    const text = input.trim();
    if (!text || busy) return;
    const history = messages;
    setMessages(m => [...m, { role: 'user', content: text }, { role: 'assistant', content: '' }]);
    setInput('');
    setBusy(true);

    // Stream tokens into the trailing (empty) assistant message.
    let acc = '';
    const unlisten = await listen<string>('assistant-chunk', (e) => {
      acc += e.payload;
      setMessages(m => { const c = [...m]; c[c.length - 1] = { role: 'assistant', content: acc }; return c; });
    });
    try {
      await invoke('assistant_chat_stream', {
        profileId: profileId ?? 1,
        message: text,
        history,
        model: selectedModel || null,
        journal,
      });
    } catch (e) {
      const err = `⚠ ${String(e).replace(/^.*?:\s*/, '')}`;
      setMessages(m => { const c = [...m]; c[c.length - 1] = { role: 'assistant', content: acc || err }; return c; });
    } finally {
      unlisten();
      setBusy(false);
    }
  };

  return (
    <div className="bg-theme-card/80 border border-white/10 rounded-2xl p-4 mt-4 backdrop-blur-sm flex flex-col" style={{ height: 440 }}>
      <div className="flex items-center justify-between mb-2 text-xs gap-2">
        <div className="flex items-center gap-2 min-w-0">
          {status?.ollamaRunning ? (
            <>
              <select
                value={selectedModel}
                onChange={e => setSelectedModel(e.target.value)}
                className="bg-gray-800 border border-gray-600 rounded px-1.5 py-1 text-xs text-white max-w-[170px]"
              >
                {status.model && !models.includes(status.model) && (
                  <option value={status.model}>{status.model} (recommended)</option>
                )}
                {models.map(m => <option key={m} value={m}>{m}</option>)}
              </select>
              <span className="text-[var(--text-muted-color)] whitespace-nowrap">
                {status.vramMb > 0 ? `${(status.vramMb / 1024).toFixed(0)} GB` : 'CPU'}
              </span>
            </>
          ) : status ? (
            <span className="text-yellow-400">Ollama not running — <span className="font-mono">ollama serve</span></span>
          ) : (
            <span className="text-[var(--text-muted-color)]">Checking local model…</span>
          )}
        </div>
        {messages.length > 0 && (
          <button onClick={() => setMessages([])} className="text-[var(--text-muted-color)] hover:text-white whitespace-nowrap">Clear</button>
        )}
      </div>
      <div ref={scrollRef} className="flex-1 min-h-0 overflow-y-auto space-y-2 pr-1">
        {messages.length === 0 && (
          <div className="text-sm text-[var(--text-muted-color)] mt-2">
            Ask anything. The assistant can use your saved pages and media descriptions when relevant.
          </div>
        )}
        {messages.map((m, i) => (
          <div key={i} className={`text-sm rounded-lg px-3 py-2 max-w-[85%] whitespace-pre-wrap ${
            m.role === 'user' ? 'ml-auto bg-[var(--primary-color)]/20 text-white' : 'mr-auto bg-white/5 text-[var(--text-color)]'
          }`}>
            {m.content}
          </div>
        ))}
        {busy && <div className="mr-auto bg-white/5 rounded-lg px-3 py-2 text-sm text-[var(--text-muted-color)]">…thinking</div>}
      </div>
      <div className="flex items-center gap-2 mt-2">
        <input
          value={input}
          onChange={e => setInput(e.target.value)}
          onKeyDown={e => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); send(); } }}
          placeholder={status?.ollamaRunning ? 'Message your local assistant…' : 'Start Ollama to chat…'}
          disabled={!status?.ollamaRunning || busy}
          className="flex-1 px-3 py-2 text-sm bg-gray-800 border border-gray-600 rounded-lg outline-none focus:border-[var(--primary-color)] text-white disabled:opacity-50"
        />
        <button
          onClick={send}
          disabled={!status?.ollamaRunning || busy || !input.trim()}
          className="px-3 py-2 text-sm rounded-lg bg-[var(--primary-color)] text-white hover:opacity-90 disabled:opacity-40"
        >
          Send
        </button>
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

export function LocalAIHub({ profileId, settings, onChange, onOpenMemory }: LocalAIHubProps) {
  return (
    <div className="max-w-3xl mx-auto py-8 px-4">
      <div className="mb-6">
        <h1 className="text-2xl font-bold text-white">Local AI</h1>
        <p className="text-sm text-[var(--text-muted-color)] mt-1">
          On-device intelligence. Runs locally, stays private, and never phones home.
        </p>
      </div>

      {/* Knowledge Curator */}
      <div className="bg-theme-card/80 border border-white/10 rounded-2xl p-5 mb-4 backdrop-blur-sm">
        <div className="flex items-start gap-4">
          <div className="flex-1">
            <div className="flex items-center gap-2">
              <h2 className="text-lg font-semibold text-white">Knowledge Curator</h2>
              <span className="text-[10px] uppercase tracking-wide px-1.5 py-0.5 rounded bg-white/10 text-[var(--text-muted-color)]">
                background
              </span>
            </div>
            <p className="text-sm text-[var(--text-muted-color)] mt-1">
              Quietly reads the pages you visit and writes a short, factual summary into your
              knowledge graph. It is <strong className="text-[var(--text-color)]">transparent,
              unbiased, and non-judgemental</strong> — it describes, never editorializes. Skips
              incognito entirely.
            </p>
          </div>
          <Toggle on={settings.curator} onClick={() => onChange({ curator: !settings.curator })} />
        </div>
        <button
          onClick={onOpenMemory}
          className="mt-4 text-sm text-[var(--primary-color)] hover:underline"
        >
          Open EarthMemory →
        </button>
      </div>

      {/* AI Assistant */}
      <div className="bg-theme-card/80 border border-white/10 rounded-2xl p-5 backdrop-blur-sm">
        <div className="flex items-start gap-4">
          <div className="flex-1">
            <div className="flex items-center gap-2">
              <h2 className="text-lg font-semibold text-white">AI Assistant</h2>
            </div>
            <p className="text-sm text-[var(--text-muted-color)] mt-1">
              A general local chat model, auto-selected to fit your hardware (VRAM tier). Safe,
              private, and fully on-device — and it can draw on the pages and media your curator
              has saved.
            </p>
          </div>
          <Toggle on={settings.assistant} onClick={() => onChange({ assistant: !settings.assistant })} />
        </div>
      </div>

      {settings.assistant && <AssistantChat profileId={profileId} journal={settings.curator} />}

      <p className="text-xs text-[var(--text-muted-color)] mt-4">
        The assistant and curator both use a local Ollama model. Install Ollama and run
        <span className="font-mono"> ollama serve</span> to enable them.
      </p>
    </div>
  );
}

export default LocalAIHub;
