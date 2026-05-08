export interface Segment {
  id: string;
  started_at: string;
  dutch: string;
  english?: string | null;
  speaker?: string | null;
  is_final: boolean;
}

export interface ChatMessage {
  role: "user" | "assistant";
  content: string;
  at: string;
}

export interface Meeting {
  id: string;
  title: string;
  started_at: string;
  ended_at?: string | null;
  segments: Segment[];
  summary?: string | null;
  summary_updated_at?: string | null;
  chat: ChatMessage[];
}

export interface SettingsView {
  deepgram_set: boolean;
  anthropic_set: boolean;
}

export interface MeetingSummaryRow {
  id: string;
  title: string;
  started_at: string;
  ended_at?: string | null;
  segment_count: number;
}
