use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use chrono::Utc;
use parking_lot::RwLock;
use serde::Serialize;
use serde_json::json;
use tauri::State;
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

#[tauri::command]
pub async fn regenerate_summary(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let Some(handle) = state.current() else {
        return Err("no meeting in progress".into());
    };
    let an_key = settings::require_anthropic().map_err(|e| e.to_string())?;
    let claude = AnthropicClient::new(an_key);
    let transcript = handle.meeting.read().finalized_text(true);
    if transcript.is_empty() {
        return Ok(());
    }
    let app_state = state.inner().clone();
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

    // Snapshot transcript & history for the request.
    let (transcript, history): (String, Vec<(String, String)>) = {
        let m = meeting_arc.read();
        (
            m.finalized_text(true),
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

    // 1. Audio sidecar.
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

    let mut pending: Option<Segment> = None;
    let mut summary_timer = tokio::time::interval(Duration::from_secs(120));
    summary_timer.tick().await; // skip immediate fire
    let mut save_timer = tokio::time::interval(Duration::from_secs(15));
    save_timer.tick().await;

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            evt = dg_rx.recv() => {
                let Some(evt) = evt else { break };
                handle_dg_event(evt, &mut pending, &state, &meeting, &claude).await;
            }
            _ = summary_timer.tick() => {
                let transcript = meeting.read().finalized_text(true);
                if transcript.is_empty() { continue; }
                let claude2 = claude.clone();
                let state2 = state.clone();
                let meeting2 = meeting.clone();
                tokio::spawn(async move {
                    match claude2.summarize(&transcript).await {
                        Ok(s) => {
                            let now = Utc::now();
                            {
                                let mut m = meeting2.write();
                                m.summary = Some(s.clone());
                                m.summary_updated_at = Some(now);
                            }
                            state2.emit(
                                "summary:update",
                                json!({ "summary": s, "updated_at": now }),
                            );
                        }
                        Err(err) => tracing::warn!(?err, "summary refresh failed"),
                    }
                });
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

async fn handle_dg_event(
    evt: DeepgramEvent,
    pending: &mut Option<Segment>,
    state: &Arc<AppState>,
    meeting: &Arc<RwLock<Meeting>>,
    claude: &Arc<AnthropicClient>,
) {
    match evt {
        DeepgramEvent::Interim { text, .. } => {
            let seg = pending.get_or_insert_with(|| Segment {
                id: Uuid::new_v4(),
                started_at: Utc::now(),
                dutch: String::new(),
                english: None,
                speaker: None,
                is_final: false,
            });
            seg.dutch = text;
            state.emit("segment:pending", seg.clone());
        }
        DeepgramEvent::Final { text, speech_final, .. } => {
            let seg = pending.get_or_insert_with(|| Segment {
                id: Uuid::new_v4(),
                started_at: Utc::now(),
                dutch: String::new(),
                english: None,
                speaker: None,
                is_final: false,
            });
            // Each "is_final=true" event delivers a stable chunk of the utterance.
            // Concatenate, then commit when speech_final arrives.
            if seg.dutch.trim().is_empty() {
                seg.dutch = text;
            } else if !text.trim().is_empty() {
                seg.dutch.push(' ');
                seg.dutch.push_str(&text);
            }
            if speech_final {
                seg.is_final = true;
                let final_seg = pending.take().unwrap();
                {
                    let mut m = meeting.write();
                    m.segments.push(final_seg.clone());
                }
                state.emit("segment:upsert", final_seg.clone());
                spawn_translate(state.clone(), meeting.clone(), claude.clone(), final_seg);
            } else {
                state.emit("segment:pending", seg.clone());
            }
        }
        DeepgramEvent::UtteranceEnd => {
            // Some Deepgram configs deliver UtteranceEnd separately. Use it as a
            // safety-net commit when we have a pending segment that wasn't
            // closed by speech_final yet.
            if let Some(mut seg) = pending.take() {
                if !seg.dutch.trim().is_empty() {
                    seg.is_final = true;
                    {
                        let mut m = meeting.write();
                        m.segments.push(seg.clone());
                    }
                    state.emit("segment:upsert", seg.clone());
                    spawn_translate(state.clone(), meeting.clone(), claude.clone(), seg);
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
