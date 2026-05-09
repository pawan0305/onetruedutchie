import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

interface Segment {
  id: string;
  dutch: string;
  english: string | null;
  is_final: boolean;
}

interface Settings {
  overlay_mode: string;
}

const MAX_LINES = 3;

export function Overlay() {
  const [segments, setSegments] = useState<Segment[]>([]);
  const [pending, setPending] = useState<Segment | null>(null);
  const [mode, setMode] = useState<string>("dual");

  useEffect(() => {
    invoke<Settings>("get_settings").then((s) => setMode(s.overlay_mode || "dual")).catch(() => {});

    const promises: Promise<UnlistenFn>[] = [
      listen<Segment>("segment:upsert", (e) => {
        const seg = e.payload;
        setPending(null);
        setSegments((prev) => {
          // Replace if same id (translation update), else append.
          const idx = prev.findIndex((s) => s.id === seg.id);
          let next = idx >= 0
            ? prev.map((s, i) => (i === idx ? seg : s))
            : [...prev, seg];
          if (next.length > MAX_LINES) next = next.slice(next.length - MAX_LINES);
          return next;
        });
      }),
      listen<{ id: string; english: string | null }>("segment:translated", (e) => {
        setSegments((prev) =>
          prev.map((s) =>
            s.id === e.payload.id ? { ...s, english: e.payload.english } : s,
          ),
        );
      }),
      listen<Segment>("segment:pending", (e) => setPending(e.payload)),
      listen<{ mode: string }>("overlay:mode", (e) => setMode(e.payload.mode)),
      listen<unknown>("meeting:started", () => {
        // Fresh meeting → clear stale subtitles.
        setSegments([]);
        setPending(null);
      }),
    ];
    let off: UnlistenFn[] = [];
    Promise.all(promises).then((arr) => {
      off = arr;
    });
    return () => {
      off.forEach((fn) => fn());
    };
  }, []);

  const showNL = mode === "dual";
  const showEN = mode === "dual" || mode === "en";

  return (
    <div className="overlay-shell" data-tauri-drag-region>
      <div className="overlay-lines" data-tauri-drag-region>
        {segments.length === 0 && !pending && (
          <div className="overlay-line muted" data-tauri-drag-region>
            …waiting for speech…
          </div>
        )}
        {segments.map((s) => (
          <div key={s.id} className="overlay-line" data-tauri-drag-region>
            {showNL && (
              <div className="row nl" data-tauri-drag-region>
                {s.dutch}
              </div>
            )}
            {showEN && (
              <div className="row en" data-tauri-drag-region>
                {s.english ?? <span className="muted">translating…</span>}
              </div>
            )}
          </div>
        ))}
        {pending && (
          <div className="overlay-line pending" data-tauri-drag-region>
            {showNL && (
              <div className="row nl" data-tauri-drag-region>
                {pending.dutch}
              </div>
            )}
          </div>
        )}
      </div>
      <div
        className="resize-handle"
        title="drag to resize"
        onMouseDown={(e) => {
          e.preventDefault();
          e.stopPropagation();
          // Tauri 2: ResizeDirection is a string; "SouthEast" expands from BR.
          getCurrentWebviewWindow().startResizeDragging("SouthEast" as any);
        }}
      />
    </div>
  );
}
