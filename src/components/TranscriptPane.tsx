import { useEffect, useRef } from "react";
import type { Segment } from "../lib/types";

interface Props {
  segments: Segment[];
  pendingId?: string;
}

function fmtTime(iso: string): string {
  const d = new Date(iso);
  const hh = String(d.getHours()).padStart(2, "0");
  const mm = String(d.getMinutes()).padStart(2, "0");
  const ss = String(d.getSeconds()).padStart(2, "0");
  return `${hh}:${mm}:${ss}`;
}

export function TranscriptPane({ segments, pendingId }: Props) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const stickToBottomRef = useRef(true);

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
        <span className="pane-sub">{segments.length} segments</span>
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
            <div className="segment-cols">
              <div className="col nl">
                <div className="lang-label">NL</div>
                <div className="text">{s.dutch || <em>…</em>}</div>
              </div>
              <div className="col en">
                <div className="lang-label">EN</div>
                <div className="text">
                  {s.english ?? (s.is_final ? <em className="muted">translating…</em> : <em className="muted">—</em>)}
                </div>
              </div>
            </div>
          </div>
        ))}
      </div>
    </section>
  );
}
