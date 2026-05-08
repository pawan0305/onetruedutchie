import { useEffect, useRef, useState } from "react";
import { api } from "../lib/tauri";
import type { Segment } from "../lib/types";

interface Props {
  segments: Segment[];
  pendingId?: string;
  meetingId?: string;
  showEnglish?: boolean;
}

function fmtTime(iso: string): string {
  const d = new Date(iso);
  const hh = String(d.getHours()).padStart(2, "0");
  const mm = String(d.getMinutes()).padStart(2, "0");
  const ss = String(d.getSeconds()).padStart(2, "0");
  return `${hh}:${mm}:${ss}`;
}

export function TranscriptPane({ segments, pendingId, meetingId, showEnglish = true }: Props) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const stickToBottomRef = useRef(true);
  const [copyState, setCopyState] = useState<"idle" | "translating" | "copied" | "error">("idle");

  const onCopy = async () => {
    if (segments.filter((s) => s.is_final).length === 0) return;
    setCopyState("translating");
    try {
      const text = await api.exportEnglishTranscript(meetingId);
      if (!text.trim()) {
        setCopyState("idle");
        return;
      }
      await navigator.clipboard.writeText(text);
      setCopyState("copied");
      setTimeout(() => setCopyState("idle"), 1500);
    } catch {
      setCopyState("error");
      setTimeout(() => setCopyState("idle"), 2000);
    }
  };

  const copyLabel =
    copyState === "translating" ? "translating…"
    : copyState === "copied" ? "✓ copied"
    : copyState === "error" ? "× failed"
    : "Copy EN";

  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    if (stickToBottomRef.current) {
      el.scrollTop = el.scrollHeight;
    }
  }, [segments.length, segments.at(-1)?.dutch, segments.at(-1)?.english]);

  const onScroll = () => {
    const el = scrollRef.current;
    if (!el) return;
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 32;
    stickToBottomRef.current = atBottom;
  };

  return (
    <section className="pane transcript-pane">
      <header className="pane-header">
        <h2>Transcript</h2>
        <div className="pane-sub-row">
          <button
            className="ghost"
            onClick={onCopy}
            disabled={segments.length === 0 || copyState === "translating"}
            title="Translate the full transcript and copy to clipboard"
          >
            {copyLabel}
          </button>
          <span className="pane-sub">{segments.length} segments</span>
        </div>
      </header>
      <div className="pane-body scroll" ref={scrollRef} onScroll={onScroll}>
        {segments.length === 0 && (
          <div className="empty">
            Press <strong>Start meeting</strong> to begin live transcription.
          </div>
        )}
        {segments.map((s) => (
          <div
            key={s.id}
            className={`segment${s.id === pendingId ? " pending" : ""}${
              s.is_final ? " final" : ""
            }`}
          >
            <div className="segment-time">{fmtTime(s.started_at)}</div>
            <div className={`segment-cols${showEnglish ? "" : " single"}`}>
              <div className="col nl">
                {showEnglish && <div className="lang-label">NL</div>}
                <div className="text">{s.dutch || <em>…</em>}</div>
              </div>
              {showEnglish && (
                <div className="col en">
                  <div className="lang-label">EN</div>
                  <div className="text">
                    {s.english ?? (s.is_final ? <em className="muted">translating…</em> : <em className="muted">—</em>)}
                  </div>
                </div>
              )}
            </div>
          </div>
        ))}
      </div>
    </section>
  );
}
