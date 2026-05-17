import { useEffect, useMemo, useState } from "react";
import { api } from "../lib/tauri";
import type { MeetingSummaryRow } from "../lib/types";

interface Props {
  rows: MeetingSummaryRow[];
  onOpen: (id: string) => void;
  onDelete: (id: string) => void;
  onRename: (id: string, title: string) => void;
  onClose: () => void;
  onRefresh: () => void;
  onError?: (msg: string) => void;
}

export function HistoryDrawer({
  rows,
  onOpen,
  onDelete,
  onRename,
  onClose,
  onRefresh,
  onError,
}: Props) {
  const [armedDeleteId, setArmedDeleteId] = useState<string | null>(null);
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [renameDraft, setRenameDraft] = useState("");
  const [editingTagsId, setEditingTagsId] = useState<string | null>(null);
  const [tagsDraft, setTagsDraft] = useState("");
  const [query, setQuery] = useState("");

  useEffect(() => {
    onRefresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (!armedDeleteId) return;
    const t = setTimeout(() => setArmedDeleteId(null), 3000);
    return () => clearTimeout(t);
  }, [armedDeleteId]);

  const visible = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return rows;
    return rows.filter((r) => {
      const inTitle = r.title.toLowerCase().includes(q);
      const inTags = (r.tags ?? []).some((t) => t.toLowerCase().includes(q));
      return inTitle || inTags;
    });
  }, [rows, query]);

  const commitTags = async (id: string) => {
    const tags = tagsDraft
      .split(",")
      .map((s) => s.trim())
      .filter((s) => s.length > 0);
    setEditingTagsId(null);
    try {
      await api.setMeetingTags(id, tags);
      onRefresh();
    } catch (err) {
      onError?.(`tags: ${err}`);
    }
  };

  return (
    <div className="drawer-backdrop" onClick={onClose}>
      <aside className="drawer" onClick={(e) => e.stopPropagation()}>
        <header className="drawer-header">
          <h2>Meeting history</h2>
          <button onClick={onClose}>✕</button>
        </header>
        <div className="drawer-search">
          <input
            type="text"
            placeholder="Search title or tag…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
        {visible.length === 0 && (
          <div className="empty">
            {rows.length === 0 ? "No saved meetings yet." : "No matches."}
          </div>
        )}
        <ul className="history-list">
          {visible.map((r) => {
            const armed = armedDeleteId === r.id;
            const renaming = renamingId === r.id;
            const editingTags = editingTagsId === r.id;
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
                    {editingTags ? (
                      <input
                        className="history-tags-input"
                        autoFocus
                        value={tagsDraft}
                        onChange={(e) => setTagsDraft(e.target.value)}
                        onClick={(e) => e.stopPropagation()}
                        onBlur={() => commitTags(r.id)}
                        onKeyDown={(e) => {
                          e.stopPropagation();
                          if (e.key === "Enter")
                            (e.target as HTMLInputElement).blur();
                          if (e.key === "Escape") setEditingTagsId(null);
                        }}
                        placeholder="tags, comma-separated"
                      />
                    ) : (
                      <div
                        className="history-tags"
                        onClick={(e) => {
                          e.stopPropagation();
                          setTagsDraft((r.tags ?? []).join(", "));
                          setEditingTagsId(r.id);
                        }}
                        title="Click to edit tags"
                      >
                        {(r.tags ?? []).length === 0
                          ? <span className="muted">+ tags</span>
                          : (r.tags ?? []).map((t) => (
                              <span key={t} className="tag-pill">{t}</span>
                            ))}
                      </div>
                    )}
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
