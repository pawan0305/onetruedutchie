import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  Meeting,
  MeetingSummaryRow,
  SettingsView,
} from "./types";

export const api = {
  getSettings: () => invoke<SettingsView>("get_settings"),
  setApiKeys: (deepgram?: string, anthropic?: string) =>
    invoke<SettingsView>("set_api_keys", { deepgram, anthropic }),
  setTranslateEnabled: (enabled: boolean) =>
    invoke<SettingsView>("set_translate_enabled", { enabled }),
  startMeeting: (title?: string) =>
    invoke<Meeting>("start_meeting", { title }),
  stopMeeting: () => invoke<Meeting>("stop_meeting"),
  currentMeeting: () => invoke<Meeting | null>("current_meeting"),
  setMeetingTitle: (title: string) =>
    invoke<void>("set_meeting_title", { title }),
  listMeetings: () => invoke<MeetingSummaryRow[]>("list_meetings"),
  loadMeeting: (id: string) => invoke<Meeting>("load_meeting", { id }),
  deleteMeeting: (id: string) => invoke<void>("delete_meeting", { id }),
  renameMeeting: (id: string, title: string) =>
    invoke<void>("rename_meeting", { id, title }),
  exportEnglishTranscript: (id?: string) =>
    invoke<string>("export_english_transcript", { id }),
  regenerateSummary: (id?: string) => invoke<void>("regenerate_summary", { id }),
  askQuestion: (question: string, meetingId?: string) =>
    invoke<{ stream_id: string }>("ask_question", {
      question,
      meetingId,
    }),
};

export type EventHandlers = {
  "meeting:started": Meeting;
  "meeting:stopped": Meeting;
  "meeting:update": Meeting;
  "segment:pending": import("./types").Segment;
  "segment:upsert": import("./types").Segment;
  "segment:translated": { id: string; english: string | null; error?: string };
  "summary:update": { summary: string; updated_at: string };
  "chat:user": { stream_id: string; question: string };
  "chat:delta": { stream_id: string; delta: string };
  "chat:done": { stream_id: string; answer: string };
  "chat:error": { stream_id: string; error: string };
  error: { message: string };
};

export function on<K extends keyof EventHandlers>(
  event: K,
  handler: (payload: EventHandlers[K]) => void,
): Promise<UnlistenFn> {
  return listen<EventHandlers[K]>(event, (e) => handler(e.payload));
}
