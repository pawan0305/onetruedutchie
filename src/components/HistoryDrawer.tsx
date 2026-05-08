import { useEffect, useState } from "react";
import type { MeetingSummaryRow } from "../lib/types";

interface Props {
  rows: MeetingSummaryRow[];
  onOpen: (id: string) => void;
  onDelete: (id: string) => void;
  onRename: (id: string, title: string) => void;
  onClose: () => void;
  onRefresh: () => void;
}

export function HistoryDrawer({
  rows,
  onOpen,
  onDelete,
  onRename,
  onClose,
  onRefresh,
}: Props) {
  // Tracks which row is "armed" for delete (first click → "delete?",
  // second click within 3s → actually deletes). Avoids the silent
  // window.confirm() on Tauri webviews.
  const [armedDeleteId, setArmedDeleteId] = useState<string | null>(null);
  // Tracks which row is being renamed inline; null when nothing is editing.
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [renameDraft, setRenameDraft] = useState("");

  useEffect(() => {
    onRefresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Disarm delete after 3s of inactivity.
  useEffect(() => {
    if (!armedDeleteId) return;
    const t = setTimeout(() => setArmedDeleteId(null), 3000);
    return () => clearTimeout(t);
  }, [armedDeleteId]);

  return (
    <div className="drawer-backdrop" onClick={onClose}>
      <aside className="drawer" onClick={(e) => e.stopPropagation()}>
        <header className="drawer-header">
          <h2>Meeting history</h2>
          <button onClick={onClose}>✕</button>
        </header>
        {rows.length === 0 && (
          <div className="empty">No saved meetings yet.</div>
        )}
        <ul className="history-list">
          {rows.map((r) => {
            const armed = armedDeleteId === r.id;
            const renaming = renamingId === r.id;
            return (
              <li key={r.id}>
                {renaming ? (
                  <input
                    className="title-input history-rename"
                    autoFocus
                    value={renameDraft}
                    onChange={(e) => setRenameDraft(e.target.value)}
                    onClick={(e) => e.stopPropagation()}
                    onBlur={() => {
                      const v = renameDraft.trim();
                      if (v && v !== r.title) onRename(r.id, v);
                      setRenamingId(null);
                    }}
                    onKeyDown={(e) => {
                      if (e.key === "Enter")
                        (e.target as HTMLInputElement).blur();
                      if (e.key === "Escape") setRenamingId(null);
                    }}
                  />
                ) : (
                  <button
                    className="history-row"
                    onClick={() => onOpen(r.id)}
                  >
                    <div className="history-title">{r.title}</div>
                    <div className="history-meta">
                      {new Date(r.started_at).toLocaleString()} ·{" "}
                      {r.segment_count} segments
                    </div>
                  </button>
                )}
                <button
                  className="ghost"
                  title="rename"
                  onClick={() => {
                    setArmedDeleteId(null);
                    setRenameDraft(r.title);
                    setRenamingId(r.id);
                  }}
                >
                  ✏︎
                </button>
                <button
                  className="ghost danger"
                  title={armed ? "click again to confirm" : "delete"}
                  onClick={() => {
                    if (armed) {
                      setArmedDeleteId(null);
                      onDelete(r.id);
                    } else {
                      setArmedDeleteId(r.id);
                    }
                  }}
                >
                  {armed ? "delete?" : "🗑"}
                </button>
              </li>
            );
          })}
        </ul>
      </aside>
    </div>
  );
}
