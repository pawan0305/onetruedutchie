import { useEffect } from "react";
import type { MeetingSummaryRow } from "../lib/types";

interface Props {
  rows: MeetingSummaryRow[];
  onOpen: (id: string) => void;
  onDelete: (id: string) => void;
  onClose: () => void;
  onRefresh: () => void;
}

export function HistoryDrawer({
  rows,
  onOpen,
  onDelete,
  onClose,
  onRefresh,
}: Props) {
  useEffect(() => {
    onRefresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

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
          {rows.map((r) => (
            <li key={r.id}>
              <button className="history-row" onClick={() => onOpen(r.id)}>
                <div className="history-title">{r.title}</div>
                <div className="history-meta">
                  {new Date(r.started_at).toLocaleString()} ·{" "}
                  {r.segment_count} segments
                </div>
              </button>
              <button
                className="ghost danger"
                title="delete"
                onClick={() => {
                  if (confirm(`Delete meeting "${r.title}"?`)) onDelete(r.id);
                }}
              >
                🗑
              </button>
            </li>
          ))}
        </ul>
      </aside>
    </div>
  );
}
