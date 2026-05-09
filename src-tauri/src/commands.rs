use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use chrono::Utc;
use parking_lot::RwLock;
use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Manager, State};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::anthropic::{AnthropicClient, ChatStreamEvent};
use crate::audio;
use crate::deepgram::{self, DeepgramConfig, DeepgramEvent};
use crate::settings::{self, SettingsView};
use crate::state::{AppState, ChatMessage, Meeting, MeetingHandle, Segment};
use crate::storage::{self, MeetingSummaryRow};

#[derive(Serialize)]
pub struct AskHandle {
    pub stream_id: Uuid,
}

// ------- settings -------

#[tauri::command]
pub async fn get_settings() -> Result<SettingsView, String> {
    settings::settings_view().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_api_keys(
    deepgram: Option<String>,
    anthropic: Option<String>,
) -> Result<SettingsView, String> {
    settings::write_keys(deepgram.as_deref(), anthropic.as_deref())
        .map_err(|e| e.to_string())?;
    settings::settings_view().map_err(|e| e.to_string())
}

/// Toggle per-chunk translation on/off. Persists to keys.json so it survives
/// restarts. Reads on every Final segment in handle_dg_event so the toggle
/// takes effect mid-meeting without restarting capture.
#[tauri::command]
pub async fn set_translate_enabled(enabled: bool) -> Result<SettingsView, String> {
    settings::set_translate_enabled(enabled).map_err(|e| e.to_string())?;
    settings::settings_view().map_err(|e| e.to_string())
}

/// Show / hide / change the subtitle overlay window. Modes: "off", "dual",
/// "en". Persists across restarts. The overlay webview listens to the
/// `overlay:mode` event for live mode switching while it's already visible.
#[tauri::command]
pub async fn set_overlay_mode(mode: String, app: AppHandle) -> Result<SettingsView, String> {
    settings::set_overlay_mode(&mode).map_err(|e| e.to_string())?;
    if let Some(win) = app.get_webview_window("overlay") {
        if mode == "off" {
            let _ = win.hide();
        } else {
            let _ = win.show();
            let _ = win.set_always_on_top(true);
            // Visible on every macOS Space, so the subtitles follow you when
            // you switch desktops. No-op on platforms that don't support it.
            #[cfg(target_os = "macos")]
            {
                let _ = win.set_visible_on_all_workspaces(true);
            }
            // Re-apply the persisted lock state every time we show, in case
            // it was toggled while hidden.
            let locked = settings::read_overlay_locked();
            let _ = win.set_ignore_cursor_events(locked);
        }
    }
    use tauri::Emitter;
    let _ = app.emit("overlay:mode", json!({ "mode": mode }));
    settings::settings_view().map_err(|e| e.to_string())
}

/// Adjust the subtitle font size in pixels.
#[tauri::command]
pub async fn set_overlay_font_size(
    size: u32,
    app: AppHandle,
) -> Result<SettingsView, String> {
    settings::set_overlay_font_size(size).map_err(|e| e.to_string())?;
    let view = settings::settings_view().map_err(|e| e.to_string())?;
    use tauri::Emitter;
    let _ = app.emit(
        "overlay:settings",
        json!({ "font_size": view.overlay_font_size, "locked": view.overlay_locked }),
    );
    Ok(view)
}

/// Lock / unlock the overlay. When locked the overlay is click-through —
/// every click goes to whatever is behind it (Teams, browser, etc). When
/// unlocked the user can grab and drag/resize it.
#[tauri::command]
pub async fn set_overlay_locked(
    locked: bool,
    app: AppHandle,
) -> Result<SettingsView, String> {
    settings::set_overlay_locked(locked).map_err(|e| e.to_string())?;
    if let Some(win) = app.get_webview_window("overlay") {
        let _ = win.set_ignore_cursor_events(locked);
    }
    let view = settings::settings_view().map_err(|e| e.to_string())?;
    use tauri::Emitter;
    let _ = app.emit(
        "overlay:settings",
        json!({ "font_size": view.overlay_font_size, "locked": view.overlay_locked }),
    );
    Ok(view)
}

// ------- meetings -------

#[tauri::command]
pub async fn start_meeting(
    title: Option<String>,
    state: State<'_, Arc<AppState>>,
) -> Result<Meeting, String> {
    if state.current().is_some() {
        return Err("a meeting is already running".into());
    }
    let dg_key = settings::require_deepgram().map_err(|e| e.to_string())?;
    let an_key = settings::require_anthropic().map_err(|e| e.to_string())?;

    let title = title.unwrap_or_else(|| default_title());
    let meeting = Meeting::new(title);
    let cancel = CancellationToken::new();
    let handle = Arc::new(MeetingHandle {
        meeting: Arc::new(RwLock::new(meeting.clone())),
        cancel: cancel.clone(),
    });
    state.set_current(handle.clone());

    let app_state = state.inner().clone();
    tokio::spawn(async move {
        if let Err(err) = run_meeting(app_state.clone(), handle.clone(), dg_key, an_key).await {
            tracing::error!(?err, "meeting loop failed");
            app_state.emit(
                "error",
                json!({ "message": format!("meeting failed: {err}") }),
            );
            // make sure we clean up even on failure
            handle.cancel.cancel();
            let final_meeting = handle.meeting.read().clone();
            app_state.emit("meeting:stopped", final_meeting);
            app_state.clear_current();
        }
    });

    state.emit("meeting:started", meeting.clone());
    Ok(meeting)
}

#[tauri::command]
pub async fn stop_meeting(state: State<'_, Arc<AppState>>) -> Result<Meeting, String> {
    let Some(handle) = state.current() else {
        return Err("no meeting in progress".into());
    };
    handle.cancel.cancel();
    // Wait briefly so the loop can flush + save.
    tokio::time::sleep(Duration::from_millis(400)).await;
    let m = handle.meeting.read().clone();
    Ok(m)
}

#[tauri::command]
pub async fn current_meeting(state: State<'_, Arc<AppState>>) -> Result<Option<Meeting>, String> {
    Ok(state.current().map(|h| h.meeting.read().clone()))
}

#[tauri::command]
pub async fn set_meeting_title(
    title: String,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let Some(handle) = state.current() else {
        return Err("no meeting in progress".into());
    };
    {
        let mut m = handle.meeting.write();
        m.title = title;
    }
    let snapshot = handle.meeting.read().clone();
    state.emit("meeting:update", snapshot);
    Ok(())
}

#[tauri::command]
pub async fn list_meetings(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<MeetingSummaryRow>, String> {
    let dir = state.meetings_dir();
    tokio::task::spawn_blocking(move || storage::list_meetings(&dir))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn load_meeting(
    id: Uuid,
    state: State<'_, Arc<AppState>>,
) -> Result<Meeting, String> {
    let dir = state.meetings_dir();
    tokio::task::spawn_blocking(move || storage::load_meeting(&dir, id))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_meeting(
    id: Uuid,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let dir = state.meetings_dir();
    tokio::task::spawn_blocking(move || storage::delete_meeting(&dir, id))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Translate the full transcript of either the running meeting (id=None) or
/// a historical meeting in one shot — for the "Copy EN" button. Producing
/// one cohesive translation reads much better than concatenating the live
/// per-chunk translations.
#[tauri::command]
pub async fn export_english_transcript(
    id: Option<Uuid>,
    state: State<'_, Arc<AppState>>,
) -> Result<String, String> {
    let an_key = settings::require_anthropic().map_err(|e| e.to_string())?;
    let claude = AnthropicClient::new(an_key);

    let transcript = if let Some(meeting_id) = id {
        // Prefer the live meeting if the id matches; otherwise load from disk.
        if let Some(handle) = state.current() {
            if handle.meeting.read().id == meeting_id {
                handle.meeting.read().source_text()
            } else {
                let dir = state.meetings_dir();
                let m = tokio::task::spawn_blocking(move || storage::load_meeting(&dir, meeting_id))
                    .await
                    .map_err(|e| e.to_string())?
                    .map_err(|e| e.to_string())?;
                m.source_text()
            }
        } else {
            let dir = state.meetings_dir();
            let m = tokio::task::spawn_blocking(move || storage::load_meeting(&dir, meeting_id))
                .await
                .map_err(|e| e.to_string())?
                .map_err(|e| e.to_string())?;
            m.source_text()
        }
    } else {
        let Some(handle) = state.current() else {
            return Err("no meeting".into());
        };
        let s = handle.meeting.read().source_text();
        s
    };

    if transcript.trim().is_empty() {
        return Ok(String::new());
    }
    claude
        .translate_full(&transcript)
        .await
        .map_err(|e| e.to_string())
}

/// Rename a historical meeting on disk. The active meeting (if any) is
/// renamed via `set_meeting_title` instead — that path also updates in-memory
/// state and emits an event.
#[tauri::command]
pub async fn rename_meeting(
    id: Uuid,
    title: String,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    // If the renamed meeting is the one currently in progress, route through
    // the live path so listeners get the update event.
    if let Some(handle) = state.current() {
        if handle.meeting.read().id == id {
            handle.meeting.write().title = title.clone();
            let snap = handle.meeting.read().clone();
            state.emit("meeting:update", snap);
            return Ok(());
        }
    }
    let dir = state.meetings_dir();
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let mut m = storage::load_meeting(&dir, id)?;
        m.title = title;
        storage::save_meeting(&dir, &m)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn regenerate_summary(
    id: Option<Uuid>,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let an_key = settings::require_anthropic().map_err(|e| e.to_string())?;
    let claude = AnthropicClient::new(an_key);
    let app_state = state.inner().clone();

    // Decide which meeting we're summarizing: the live one if `id` matches
    // (or is None and a meeting is running), otherwise a historical meeting
    // loaded from disk.
    let live_handle = state.current().filter(|h| {
        match id {
            None => true,
            Some(want) => h.meeting.read().id == want,
        }
    });

    if let Some(handle) = live_handle {
        let transcript = handle.meeting.read().source_text();
        if transcript.trim().is_empty() {
            return Ok(());
        }
        let meeting = handle.meeting.clone();
        tokio::spawn(async move {
            match claude.summarize(&transcript).await {
                Ok(s) => {
                    let now = Utc::now();
                    {
                        let mut m = meeting.write();
                        m.summary = Some(s.clone());
                        m.summary_updated_at = Some(now);
                    }
                    app_state.emit(
                        "summary:update",
                        json!({ "summary": s, "updated_at": now }),
                    );
                }
                Err(err) => {
                    app_state.emit(
                        "error",
                        json!({ "message": format!("summary failed: {err}") }),
                    );
                }
            }
        });
        return Ok(());
    }

    // Historical meeting path: load → summarize → save → emit update so the
    // pane refreshes.
    let Some(meeting_id) = id else {
        return Err("no meeting".into());
    };
    let dir = state.meetings_dir();
    tokio::spawn(async move {
        let load_dir = dir.clone();
        let m = match tokio::task::spawn_blocking(move || storage::load_meeting(&load_dir, meeting_id))
            .await
        {
            Ok(Ok(m)) => m,
            other => {
                app_state.emit(
                    "error",
                    json!({ "message": format!("load failed: {other:?}") }),
                );
                return;
            }
        };
        let transcript = m.source_text();
        if transcript.trim().is_empty() {
            return;
        }
        match claude.summarize(&transcript).await {
            Ok(s) => {
                let now = Utc::now();
                let mut updated = m.clone();
                updated.summary = Some(s.clone());
                updated.summary_updated_at = Some(now);
                let save_dir = dir.clone();
                let to_save = updated.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    storage::save_meeting(&save_dir, &to_save)
                })
                .await;
                app_state.emit(
                    "summary:update",
                    json!({ "summary": s, "updated_at": now }),
                );
                // Rebroadcast the meeting so the UI refreshes the pane.
                app_state.emit("meeting:update", updated);
            }
            Err(err) => {
                app_state.emit(
                    "error",
                    json!({ "message": format!("summary failed: {err}") }),
                );
            }
        }
    });
    Ok(())
}

// ------- chat -------

#[tauri::command]
pub async fn ask_question(
    question: String,
    meeting_id: Option<Uuid>,
    state: State<'_, Arc<AppState>>,
) -> Result<AskHandle, String> {
    if question.trim().is_empty() {
        return Err("empty question".into());
    }
    let an_key = settings::require_anthropic().map_err(|e| e.to_string())?;

    // Pick the meeting: an explicitly-supplied id wins (so the user can ask
    // questions of a saved meeting while a different one is being recorded),
    // otherwise fall back to the live meeting.
    let meeting_arc: Arc<RwLock<Meeting>> = if let Some(id) = meeting_id {
        if let Some(handle) = state.current().filter(|h| h.meeting.read().id == id) {
            handle.meeting.clone()
        } else {
            let dir = state.meetings_dir();
            let m = tokio::task::spawn_blocking(move || storage::load_meeting(&dir, id))
                .await
                .map_err(|e| e.to_string())?
                .map_err(|e| e.to_string())?;
            Arc::new(RwLock::new(m))
        }
    } else if let Some(handle) = state.current() {
        handle.meeting.clone()
    } else {
        return Err("no current meeting and no meeting_id given".into());
    };

    let stream_id = Uuid::new_v4();
    let app_state = state.inner().clone();
    let claude = AnthropicClient::new(an_key);

    // Snapshot transcript & history for the request. Source-language only —
    // Claude reads Dutch fine, and feeding the choppy per-chunk translations
    // produces worse answers than the original.
    let (transcript, history): (String, Vec<(String, String)>) = {
        let m = meeting_arc.read();
        (
            m.source_text(),
            m.chat
                .iter()
                .map(|c| (c.role.clone(), c.content.clone()))
                .collect(),
        )
    };

    // Persist the user message immediately.
    {
        let mut m = meeting_arc.write();
        m.chat.push(ChatMessage {
            role: "user".into(),
            content: question.clone(),
            at: Utc::now(),
        });
    }
    app_state.emit(
        "chat:user",
        json!({ "stream_id": stream_id, "question": question }),
    );

    let q = question.clone();
    let meeting_for_save = meeting_arc.clone();
    let dir_for_save = state.meetings_dir();
    let app_for_task = app_state.clone();
    tokio::spawn(async move {
        let (tx, mut rx) = mpsc::channel::<ChatStreamEvent>(32);

        let claude_task = tokio::spawn(async move {
            claude.chat_stream(&transcript, &history, &q, tx).await
        });

        let mut full = String::new();
        while let Some(evt) = rx.recv().await {
            match evt {
                ChatStreamEvent::Delta(d) => {
                    full.push_str(&d);
                    app_for_task
                        .emit("chat:delta", json!({ "stream_id": stream_id, "delta": d }));
                }
                ChatStreamEvent::Done(text) => {
                    full = text;
                }
                ChatStreamEvent::Error(err) => {
                    app_for_task.emit(
                        "chat:error",
                        json!({ "stream_id": stream_id, "error": err }),
                    );
                }
            }
        }
        let _ = claude_task.await;

        if !full.is_empty() {
            {
                let mut m = meeting_for_save.write();
                m.chat.push(ChatMessage {
                    role: "assistant".into(),
                    content: full.clone(),
                    at: Utc::now(),
                });
            }
            // best-effort save
            let snap = meeting_for_save.read().clone();
            let _ = tokio::task::spawn_blocking(move || storage::save_meeting(&dir_for_save, &snap))
                .await;
            app_for_task.emit(
                "chat:done",
                json!({ "stream_id": stream_id, "answer": full }),
            );
        }
    });

    Ok(AskHandle { stream_id })
}

// ------- meeting orchestrator -------

async fn run_meeting(
    state: Arc<AppState>,
    handle: Arc<MeetingHandle>,
    dg_key: String,
    an_key: String,
) -> Result<()> {
    let cancel = handle.cancel.clone();
    let meeting = handle.meeting.clone();

    // 1. Audio sidecar. Capture mic + system audio. The Swift sidecar mixes
    //    them sample-aligned so when both pick up the same speech (e.g. mic
    //    sidetone bleeding into system loopback) we get one phrase, not two.
    let audio_rx = audio::start_capture(&state.app_handle, cancel.clone(), true).await?;

    // 2. Deepgram session.
    let (dg_tx, mut dg_rx) = mpsc::channel::<DeepgramEvent>(128);
    {
        let cfg = DeepgramConfig {
            api_key: dg_key,
            ..Default::default()
        };
        let cancel_dg = cancel.clone();
        tokio::spawn(async move {
            if let Err(err) = deepgram::run(cfg, audio_rx, dg_tx, cancel_dg).await {
                tracing::warn!(?err, "deepgram failed");
            }
        });
    }

    let claude = Arc::new(AnthropicClient::new(an_key));

    let mut pending: Option<PendingSeg> = None;
    // Summaries are user-triggered only (regenerate_summary command). No
    // periodic auto-refresh — that just burned tokens and surprised users.
    let mut save_timer = tokio::time::interval(Duration::from_secs(15));
    save_timer.tick().await;

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            evt = dg_rx.recv() => {
                let Some(evt) = evt else { break };
                handle_dg_event(evt, &mut pending, &state, &meeting, &claude).await;
            }
            _ = save_timer.tick() => {
                let snap = meeting.read().clone();
                let dir = state.meetings_dir();
                tokio::task::spawn_blocking(move || {
                    if let Err(err) = storage::save_meeting(&dir, &snap) {
                        tracing::warn!(?err, "save failed");
                    }
                });
            }
        }
    }

    // Flush + final save.
    {
        let mut m = meeting.write();
        m.ended_at = Some(Utc::now());
    }
    let snap = meeting.read().clone();
    let dir = state.meetings_dir();
    let _ = tokio::task::spawn_blocking(move || storage::save_meeting(&dir, &snap)).await;
    state.emit("meeting:stopped", meeting.read().clone());
    state.clear_current();
    Ok(())
}

/// Holds the running interim for the current chunk. Each `is_final=true`
/// from Deepgram closes a chunk and commits it as its own segment, so we
/// don't accumulate stable text here — interim only.
struct PendingSeg {
    id: Uuid,
    started_at: chrono::DateTime<Utc>,
    interim: String,
}

impl PendingSeg {
    fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            started_at: Utc::now(),
            interim: String::new(),
        }
    }

    fn to_segment(&self, dutch: String, is_final: bool) -> Segment {
        Segment {
            id: self.id,
            started_at: self.started_at,
            dutch,
            english: None,
            speaker: None,
            is_final,
        }
    }
}

/// Treat anything starting with `en` (en, en-US, en-GB, …) as English.
fn is_english(lang: &Option<String>) -> bool {
    lang.as_deref()
        .map(|l| l.to_ascii_lowercase().starts_with("en"))
        .unwrap_or(false)
}

async fn handle_dg_event(
    evt: DeepgramEvent,
    pending: &mut Option<PendingSeg>,
    state: &Arc<AppState>,
    meeting: &Arc<RwLock<Meeting>>,
    claude: &Arc<AnthropicClient>,
) {
    match evt {
        DeepgramEvent::Interim { text, .. } => {
            let seg = pending.get_or_insert_with(PendingSeg::new);
            seg.interim = text.clone();
            state.emit("segment:pending", seg.to_segment(text, false));
        }
        DeepgramEvent::Final { text, language, .. } => {
            // Each is_final=true closes a chunk → commit as its own segment.
            // This is what makes translation appear live — chunks fire every
            // ~500ms of detected pause rather than waiting for speech_final.
            let chunk = text.trim().to_string();
            if chunk.is_empty() {
                if let Some(p) = pending.as_mut() {
                    p.interim.clear();
                }
                return;
            }
            // Take the existing pending (so its id/started_at carry into
            // this finalized segment), or mint a new one.
            let p = pending.take().unwrap_or_else(PendingSeg::new);
            let mut done = p.to_segment(chunk, true);

            let translate_on = settings::read_translate_enabled();
            if !translate_on || is_english(&language) {
                // Translation disabled, or segment is already English —
                // skip Claude. Mirror text into english so downstream
                // (Copy EN, summary, chat) still has something coherent.
                done.english = Some(done.dutch.clone());
                {
                    let mut m = meeting.write();
                    m.segments.push(done.clone());
                }
                state.emit("segment:upsert", done);
            } else {
                {
                    let mut m = meeting.write();
                    m.segments.push(done.clone());
                }
                state.emit("segment:upsert", done.clone());
                spawn_translate(state.clone(), meeting.clone(), claude.clone(), done);
            }
            // pending stays None; next Interim/Final mints a fresh segment.
        }
        DeepgramEvent::UtteranceEnd => {
            // is_final commits everything, so usually there's nothing left.
            // Safety net only.
            if let Some(p) = pending.take() {
                let dutch = p.interim.trim().to_string();
                if !dutch.is_empty() {
                    let done = p.to_segment(dutch, true);
                    {
                        let mut m = meeting.write();
                        m.segments.push(done.clone());
                    }
                    state.emit("segment:upsert", done.clone());
                    spawn_translate(state.clone(), meeting.clone(), claude.clone(), done);
                }
            }
        }
        DeepgramEvent::Error(msg) => {
            state.emit("error", json!({ "message": msg }));
        }
        DeepgramEvent::Closed => {
            tracing::info!("deepgram session closed");
        }
    }
}

fn spawn_translate(
    state: Arc<AppState>,
    meeting: Arc<RwLock<Meeting>>,
    claude: Arc<AnthropicClient>,
    seg: Segment,
) {
    tokio::spawn(async move {
        match claude.translate(&seg.dutch).await {
            Ok(en) => {
                let en = en.trim().to_string();
                {
                    let mut m = meeting.write();
                    if let Some(s) = m.segments.iter_mut().find(|s| s.id == seg.id) {
                        s.english = Some(en.clone());
                    }
                }
                state.emit(
                    "segment:translated",
                    json!({ "id": seg.id, "english": en }),
                );
            }
            Err(err) => {
                tracing::warn!(?err, "translate failed");
                state.emit(
                    "segment:translated",
                    json!({
                        "id": seg.id,
                        "english": null,
                        "error": format!("{err}"),
                    }),
                );
            }
        }
    });
}

fn default_title() -> String {
    Utc::now().format("Meeting · %Y-%m-%d %H:%M").to_string()
}
