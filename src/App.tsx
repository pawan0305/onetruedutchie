import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api, on } from "./lib/tauri";
import type { Meeting, MeetingSummaryRow, Segment, SettingsView } from "./lib/types";
import { TopBar } from "./components/TopBar";
import { TranscriptPane } from "./components/TranscriptPane";
import { SummaryPane } from "./components/SummaryPane";
import { ChatPane } from "./components/ChatPane";
import { SettingsModal } from "./components/SettingsModal";
import { HistoryDrawer } from "./components/HistoryDrawer";

export function App() {
  const [settings, setSettings] = useState<SettingsView | null>(null);
  const [meeting, setMeeting] = useState<Meeting | null>(null);
  const [pending, setPending] = useState<Segment | null>(null);
  const [running, setRunning] = useState(false);
  const [streamingChatId, setStreamingChatId] = useState<string | null>(null);
  const [streamingChatText, setStreamingChatText] = useState("");
  const [showSettings, setShowSettings] = useState(false);
  const [showHistory, setShowHistory] = useState(false);
  const [history, setHistory] = useState<MeetingSummaryRow[]>([]);
  const [errors, setErrors] = useState<string[]>([]);

  const meetingRef = useRef<Meeting | null>(null);
  meetingRef.current = meeting;

  // Load initial state.
  useEffect(() => {
    (async () => {
      try {
        const s = await api.getSettings();
        setSettings(s);
        if (!s.deepgram_set || !s.anthropic_set) setShowSettings(true);
      } catch (err) {
        pushError(`load settings: ${err}`);
      }
      try {
        const m = await api.currentMeeting();
        if (m) {
          setMeeting(m);
          setRunning(true);
        }
      } catch (err) {
        pushError(`load current: ${err}`);
      }
      try {
        setHistory(await api.listMeetings());
      } catch (err) {
        pushError(`list meetings: ${err}`);
      }
    })();
  }, []);

  const pushError = useCallback((msg: string) => {
    setErrors((prev) => [...prev.slice(-4), msg]);
  }, []);

  // Subscribe to backend events.
  useEffect(() => {
    const unlisteners: Promise<() => void>[] = [];
    unlisteners.push(
      on("meeting:started", (m) => {
        setMeeting(m);
        setRunning(true);
        setPending(null);
        setStreamingChatId(null);
        setStreamingChatText("");
      }),
      on("meeting:stopped", (m) => {
        setMeeting(m);
        setRunning(false);
        setPending(null);
        api.listMeetings().then(setHistory).catch(() => {});
      }),
      on("meeting:update", (m) => setMeeting(m)),
      on("segment:pending", (seg) => setPending(seg)),
      on("segment:upsert", (seg) => {
        setPending(null);
        setMeeting((prev) => {
          if (!prev) return prev;
          const exists = prev.segments.some((s) => s.id === seg.id);
          const segments = exists
            ? prev.segments.map((s) => (s.id === seg.id ? seg : s))
            : [...prev.segments, seg];
          return { ...prev, segments };
        });
      }),
      on("segment:translated", ({ id, english, error }) => {
        if (error) pushError(error);
        setMeeting((prev) =>
          prev
            ? {
                ...prev,
                segments: prev.segments.map((s) =>
                  s.id === id ? { ...s, english } : s,
                ),
              }
            : prev,
        );
      }),
      on("summary:update", ({ summary, updated_at }) =>
        setMeeting((prev) =>
          prev
            ? { ...prev, summary, summary_updated_at: updated_at }
            : prev,
        ),
      ),
      on("chat:user", ({ stream_id, question }) => {
        setStreamingChatId(stream_id);
        setStreamingChatText("");
        setMeeting((prev) =>
          prev
            ? {
                ...prev,
                chat: [
                  ...prev.chat,
                  { role: "user", content: question, at: new Date().toISOString() },
                ],
              }
            : prev,
        );
      }),
      on("chat:delta", ({ delta }) => {
        setStreamingChatText((prev) => prev + delta);
      }),
      on("chat:done", ({ answer }) => {
        setMeeting((prev) =>
          prev
            ? {
                ...prev,
                chat: [
                  ...prev.chat,
                  {
                    role: "assistant",
                    content: answer,
                    at: new Date().toISOString(),
                  },
                ],
              }
            : prev,
        );
        setStreamingChatId(null);
        setStreamingChatText("");
      }),
      on("chat:error", ({ error }) => {
        pushError(`chat: ${error}`);
        setStreamingChatId(null);
        setStreamingChatText("");
      }),
      on("error", ({ message }) => pushError(message)),
    );

    return () => {
      unlisteners.forEach((p) => p.then((fn) => fn()).catch(() => {}));
    };
  }, [pushError]);

  const start = useCallback(async () => {
    try {
      const m = await api.startMeeting();
      setMeeting(m);
      setRunning(true);
    } catch (err) {
      pushError(`start: ${err}`);
    }
  }, [pushError]);

  const stop = useCallback(async () => {
    try {
      await api.stopMeeting();
    } catch (err) {
      pushError(`stop: ${err}`);
    }
  }, [pushError]);

  const ask = useCallback(
    async (q: string) => {
      try {
        await api.askQuestion(q, meetingRef.current?.id);
      } catch (err) {
        pushError(`ask: ${err}`);
      }
    },
    [pushError],
  );

  const renameMeeting = useCallback(
    async (title: string) => {
      try {
        await api.setMeetingTitle(title);
      } catch (err) {
        pushError(`rename: ${err}`);
      }
    },
    [pushError],
  );

  const regenerateSummary = useCallback(async () => {
    try {
      await api.regenerateSummary();
    } catch (err) {
      pushError(`summary: ${err}`);
    }
  }, [pushError]);

  const openMeeting = useCallback(
    async (id: string) => {
      try {
        const m = await api.loadMeeting(id);
        setMeeting(m);
        setRunning(false);
        setShowHistory(false);
      } catch (err) {
        pushError(`load: ${err}`);
      }
    },
    [pushError],
  );

  const onSaveKeys = useCallback(
    async (dg: string, an: string) => {
      try {
        const s = await api.setApiKeys(
          dg.trim() || undefined,
          an.trim() || undefined,
        );
        setSettings(s);
        if (s.deepgram_set && s.anthropic_set) setShowSettings(false);
      } catch (err) {
        pushError(`save settings: ${err}`);
      }
    },
    [pushError],
  );

  const liveSegments = useMemo(() => {
    if (!meeting) return [];
    if (pending && running) return [...meeting.segments, pending];
    return meeting.segments;
  }, [meeting, pending, running]);

  return (
    <div className="app">
      <TopBar
        meeting={meeting}
        running={running}
        onStart={start}
        onStop={stop}
        onOpenSettings={() => setShowSettings(true)}
        onOpenHistory={() => setShowHistory(true)}
        onRenameMeeting={renameMeeting}
        settings={settings}
      />
      <div className="main">
        <TranscriptPane segments={liveSegments} pendingId={pending?.id} />
        <SummaryPane
          summary={meeting?.summary ?? null}
          updatedAt={meeting?.summary_updated_at ?? null}
          onRegenerate={running ? regenerateSummary : undefined}
        />
        <ChatPane
          history={meeting?.chat ?? []}
          streamingId={streamingChatId}
          streamingText={streamingChatText}
          disabled={!meeting}
          onAsk={ask}
        />
      </div>
      {errors.length > 0 && (
        <div className="errors">
          {errors.map((e, i) => (
            <div key={i} className="error">
              {e}
            </div>
          ))}
          <button onClick={() => setErrors([])}>dismiss</button>
        </div>
      )}
      {showSettings && (
        <SettingsModal
          settings={settings}
          onSave={onSaveKeys}
          onClose={() => setShowSettings(false)}
        />
      )}
      {showHistory && (
        <HistoryDrawer
          rows={history}
          onOpen={openMeeting}
          onClose={() => setShowHistory(false)}
          onRefresh={async () => {
            try {
              setHistory(await api.listMeetings());
            } catch (err) {
              pushError(`history: ${err}`);
            }
          }}
          onDelete={async (id) => {
            try {
              await api.deleteMeeting(id);
              setHistory(await api.listMeetings());
            } catch (err) {
              pushError(`delete: ${err}`);
            }
          }}
        />
      )}
    </div>
  );
}
