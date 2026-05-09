import { useEffect, useState } from "react";
import type { Meeting, SettingsView } from "../lib/types";

interface Props {
  meeting: Meeting | null;
  running: boolean;
  settings: SettingsView | null;
  onStart: () => void;
  onStop: () => void;
  onOpenSettings: () => void;
  onOpenHistory: () => void;
  onRenameMeeting: (title: string) => void;
  onToggleTranslate: (enabled: boolean) => void;
  onCycleOverlay: () => void;
}

export function TopBar({
  meeting,
  running,
  settings,
  onStart,
  onStop,
  onOpenSettings,
  onOpenHistory,
  onRenameMeeting,
  onToggleTranslate,
  onCycleOverlay,
}: Props) {
  const [editingTitle, setEditingTitle] = useState<string | null>(null);

  useEffect(() => {
    if (!running) setEditingTitle(null);
  }, [running, meeting?.id]);

  const keysOk = !!settings?.deepgram_set && !!settings?.anthropic_set;

  return (
    <header className="topbar">
      <div className="topbar-left">
        <span className="brand">OneTrueDutchie</span>
        {meeting ? (
          editingTitle !== null ? (
            <input
              className="title-input"
              value={editingTitle}
              autoFocus
              onChange={(e) => setEditingTitle(e.target.value)}
              onBlur={() => {
                if (editingTitle.trim()) onRenameMeeting(editingTitle.trim());
                setEditingTitle(null);
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter") (e.target as HTMLInputElement).blur();
                if (e.key === "Escape") setEditingTitle(null);
              }}
            />
          ) : (
            <span
              className="title"
              onClick={() => running && setEditingTitle(meeting.title)}
              title={running ? "click to rename" : ""}
            >
              {meeting.title}
              {running && <span className="rec-dot" />}
            </span>
          )
        ) : (
          <span className="title muted">No meeting</span>
        )}
      </div>
      <div className="topbar-right">
        {!keysOk && (
          <span className="warn" onClick={onOpenSettings}>
            ⚠ keys not set
          </span>
        )}
        <button
          className={`ghost ${settings?.translate ? "" : "muted"}`}
          onClick={() => onToggleTranslate(!(settings?.translate ?? true))}
          title={settings?.translate
            ? "Translation on — click to turn off"
            : "Translation off — click to turn on"}
        >
          Translate: {settings?.translate ? "on" : "off"}
        </button>
        <button
          className={`ghost ${settings?.overlay_mode === "off" ? "muted" : ""}`}
          onClick={onCycleOverlay}
          title="Subtitles overlay: click to cycle off → dual → EN-only"
        >
          Subtitles: {
            settings?.overlay_mode === "dual" ? "NL+EN"
              : settings?.overlay_mode === "en" ? "EN"
              : "off"
          }
        </button>
        <button onClick={onOpenHistory}>History</button>
        {running ? (
          <button className="primary danger" onClick={onStop}>
            ◼ Stop
          </button>
        ) : (
          <button
            className="primary"
            onClick={onStart}
            disabled={!keysOk}
            title={keysOk ? "" : "set API keys first"}
          >
            ● Start meeting
          </button>
        )}
        <button onClick={onOpenSettings} title="Settings">
          ⚙
        </button>
      </div>
    </header>
  );
}
