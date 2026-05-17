import { useEffect, useRef, useState } from "react";
import { api } from "../lib/tauri";
import type { Segment } from "../lib/types";

interface Props {
  segments: Segment[];
  pendingId?: string;
  meetingId?: string;
  showEnglish?: boolean;
  speakerNames?: Record<string, string>;
  onError?: (msg: string) => void;
}

function speakerLabel(
  speaker_id: number | null | undefined,
  names: Record<string, string> | undefined,
): string | null {
  if (speaker_id == null) return null;
  const key = String(speaker_id);
  const mapped = names?.[key];
  return mapped && mapped.trim() ? mapped : `Speaker ${speaker_id + 1}`;
}

function fmtTime(iso: string): string {
  const d = new Date(iso);
  const hh = String(d.getHours()).padStart(2, "0");
  const mm = String(d.getMinutes()).padStart(2, "0");
  const ss = String(d.getSeconds()).padStart(2, "0");
  return `${hh}:${mm}:${ss}`;
}

/** Source-language transcript: one line per finalised segment, no timestamps. */
function buildDutchTranscript(segments: Segment[]): string {
  return segments
    .filter((s) => s.is_final)
    .map((s) => s.dutch.trim())
    .filter((line) => line.length > 0)
    .join("\n");
}

/** English transcript built from the per-chunk translations that have
 *  already been computed live. Falls back to the source text for segments
 *  where translation was disabled (or where the segment was already
 *  English so `english === dutch`). Instant — no Claude round-trip. */
function buildEnglishTranscript(segments: Segment[]): string {
  return segments
    .filter((s) => s.is_final)
    .map((s) => (s.english ?? s.dutch).trim())
    .filter((line) => line.length > 0)
    .join("\n");
}

type CopyKind = "nl" | "en";

export function TranscriptPane({
  segments,
  pendingId,
  meetingId,
  showEnglish = true,
  speakerNames,
  onError,
}: Props) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const stickToBottomRef = useRef(true);
  const [copied, setCopied] = useState<CopyKind | null>(null);
  const [editingSpeaker, setEditingSpeaker] = useState<number | null>(null);
  const [draft, setDraft] = useState("");

  const commitSpeakerName = async (sid: number) => {
    setEditingSpeaker(null);
    try {
      await api.setSpeakerName(meetingId, sid, draft.trim());
    } catch (err) {
      onError?.(`speaker: ${err}`);
    }
  };

  const doCopy = async (kind: CopyKind) => {
    const text =
      kind === "nl"
        ? buildDutchTranscript(segments)
        : buildEnglishTranscript(segments);
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
      setCopied(kind);
      setTimeout(() => setCopied((c) => (c === kind ? null : c)), 1500);
    } catch {
      /* clipboard blocked */
    }
  };

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

  const hasFinal = segments.some((s) => s.is_final);

  return (
    <section className="pane transcript-pane">
      <header className="pane-header">
        <h2>Transcript</h2>
        <div className="pane-sub-row">
          <button
            className="ghost"
            onClick={() => doCopy("nl")}
            disabled={!hasFinal}
            title="Copy the raw source-language transcript"
          >
            {copied === "nl" ? "✓ copied" : "Copy NL"}
          </button>
          <button
            className="ghost"
            onClick={() => doCopy("en")}
            disabled={!hasFinal}
            title="Copy the live English transcript (per-chunk translations, instant)"
          >
            {copied === "en" ? "✓ copied" : "Copy EN"}
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
                {(() => {
                  const sid = s.speaker_id ?? null;
                  const label = speakerLabel(sid, speakerNames);
                  if (label == null) return null;
                  if (editingSpeaker === sid) {
                    return (
                      <input
                        className="segment-speaker-input"
                        autoFocus
                        value={draft}
                        onChange={(e) => setDraft(e.target.value)}
                        onClick={(e) => e.stopPropagation()}
                        onBlur={() => sid != null && commitSpeakerName(sid)}
                        onKeyDown={(e) => {
                          if (e.key === "Enter") (e.target as HTMLInputElement).blur();
                          if (e.key === "Escape") setEditingSpeaker(null);
                        }}
                      />
                    );
                  }
                  return (
                    <div
                      className="segment-speaker"
                      title="Click to rename this speaker"
                      onClick={() => {
                        if (sid == null) return;
                        setDraft(speakerNames?.[String(sid)] ?? "");
                        setEditingSpeaker(sid);
                      }}
                    >
                      {label}
                    </div>
                  );
                })()}
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
