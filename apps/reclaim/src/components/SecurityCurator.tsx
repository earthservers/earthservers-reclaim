// AI curator overlay for the Security panel (Phase 7) — STRICTLY ADVISORY.
//
// This is an optional layer ON TOP of the deterministic panel. It can summarize,
// translate, and triage — it can NEVER authorize, unblock, suppress, or whitelist
// anything (the backend has no such code path; see security/curator.rs). Every AI
// surface here is visually marked "AI · advisory" so a hallucination is never
// mistaken for a kernel-reported fact. If the curator is disabled or Ollama is
// down, these sections degrade to nothing and the panel keeps working fully.

import { useState } from 'react';
import { invoke, isTauri } from '../lib/tauri';
import type { SecurityEvent } from './SecurityMonitor';

interface CuratorResult {
  available: boolean;
  text: string;
}

/// One-shot availability probe (enabled + Ollama running). Cheap.
export async function curatorAvailable(): Promise<boolean> {
  if (!isTauri()) return false;
  try {
    return await invoke<boolean>('security_curator_available');
  } catch {
    return false;
  }
}

/// Small visual marker so AI output is never confused with deterministic facts.
function AiBadge() {
  return (
    <span className="text-[9px] font-semibold uppercase tracking-wide px-1.5 py-0.5 rounded bg-fuchsia-500/15 text-fuchsia-300 border border-fuchsia-500/30">
      AI · advisory
    </span>
  );
}

/// Digest block + query box. Shown only when the curator is available.
export function SecurityCuratorSections({ available }: { available: boolean }) {
  const [digest, setDigest] = useState<string>('');
  const [digestLoading, setDigestLoading] = useState(false);
  const [question, setQuestion] = useState('');
  const [answer, setAnswer] = useState<string>('');
  const [answerLoading, setAnswerLoading] = useState(false);

  if (!available) return null;

  const runDigest = async () => {
    setDigestLoading(true);
    try {
      const r = await invoke<CuratorResult>('security_curator_digest');
      setDigest(r.text);
    } catch (e) {
      setDigest(`Could not generate digest: ${e}`);
    } finally {
      setDigestLoading(false);
    }
  };

  const ask = async () => {
    if (!question.trim()) return;
    setAnswerLoading(true);
    setAnswer('');
    try {
      const r = await invoke<CuratorResult>('security_curator_query', { question });
      setAnswer(r.text);
    } catch (e) {
      setAnswer(`Could not answer: ${e}`);
    } finally {
      setAnswerLoading(false);
    }
  };

  return (
    <div className="mb-3 rounded-lg border border-fuchsia-500/20 bg-fuchsia-500/[0.04] p-2.5">
      <div className="flex items-center gap-2 mb-1.5">
        <span className="text-sm font-medium text-white">Assistant</span>
        <AiBadge />
        <button
          onClick={runDigest}
          disabled={digestLoading}
          className="ml-auto text-[11px] text-fuchsia-300 hover:text-fuchsia-200 disabled:opacity-50"
        >
          {digestLoading ? 'Summarizing…' : 'Summarize recent activity'}
        </button>
      </div>

      {digest && (
        <p className="text-[12px] text-gray-300 whitespace-pre-wrap mb-2 leading-relaxed">{digest}</p>
      )}

      {/* Query box (bottom of the AI block): ask about the isolated security log. */}
      <div className="flex items-center gap-1.5">
        <input
          value={question}
          onChange={e => setQuestion(e.target.value)}
          onKeyDown={e => { if (e.key === 'Enter') ask(); }}
          placeholder="Ask about your security log… e.g. did anything try to read my vault?"
          className="flex-1 px-2 py-1.5 text-[12px] bg-black/30 border border-white/10 rounded focus:outline-none focus:border-fuchsia-500/40 text-white placeholder:text-gray-600"
        />
        <button
          onClick={ask}
          disabled={answerLoading || !question.trim()}
          className="px-2.5 py-1.5 text-[11px] rounded bg-fuchsia-500/20 text-fuchsia-200 hover:bg-fuchsia-500/30 disabled:opacity-50"
        >
          {answerLoading ? '…' : 'Ask'}
        </button>
      </div>
      {answer && (
        <div className="mt-1.5 flex items-start gap-1.5">
          <AiBadge />
          <p className="text-[12px] text-gray-300 whitespace-pre-wrap flex-1 leading-relaxed">{answer}</p>
        </div>
      )}
    </div>
  );
}

/// Per-event "Explain" — user-triggered, single event. Renders nothing when the
/// curator is unavailable so the deterministic feed item is unaffected.
export function ExplainButton({ event, available }: { event: SecurityEvent; available: boolean }) {
  const [text, setText] = useState<string>('');
  const [loading, setLoading] = useState(false);
  if (!available) return null;

  const explain = async () => {
    setLoading(true);
    try {
      const r = await invoke<CuratorResult>('security_curator_explain', { event });
      setText(r.text);
    } catch (e) {
      setText(`Could not explain: ${e}`);
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="mt-1">
      {!text ? (
        <button
          onClick={explain}
          disabled={loading}
          className="text-[10px] text-fuchsia-300/80 hover:text-fuchsia-200 disabled:opacity-50"
        >
          {loading ? 'Explaining…' : 'Explain ·AI'}
        </button>
      ) : (
        <div className="flex items-start gap-1.5 rounded bg-fuchsia-500/[0.06] border border-fuchsia-500/15 px-1.5 py-1">
          <AiBadge />
          <p className="text-[11px] text-gray-300 flex-1 leading-relaxed whitespace-pre-wrap">{text}</p>
        </div>
      )}
    </div>
  );
}
