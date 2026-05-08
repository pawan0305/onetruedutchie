use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub id: Uuid,
    pub started_at: DateTime<Utc>,
    pub dutch: String,
    #[serde(default)]
    pub english: Option<String>,
    #[serde(default)]
    pub speaker: Option<String>,
    pub is_final: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meeting {
    pub id: Uuid,
    pub title: String,
    pub started_at: DateTime<Utc>,
    #[serde(default)]
    pub ended_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub segments: Vec<Segment>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub summary_updated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub chat: Vec<ChatMessage>,
}

impl Meeting {
    pub fn new(title: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            title,
            started_at: Utc::now(),
            ended_at: None,
            segments: vec![],
            summary: None,
            summary_updated_at: None,
            chat: vec![],
        }
    }

    pub fn finalized_text(&self, include_english: bool) -> String {
        let mut out = String::new();
        for seg in self.segments.iter().filter(|s| s.is_final) {
            let ts = seg.started_at.format("%H:%M:%S");
            out.push_str(&format!("[{ts}] NL: {}\n", seg.dutch.trim()));
            if include_english {
                if let Some(en) = seg.english.as_deref() {
                    out.push_str(&format!("[{ts}] EN: {}\n", en.trim()));
                }
            }
        }
        out
    }
}

pub struct MeetingHandle {
    pub meeting: Arc<RwLock<Meeting>>,
    pub cancel: CancellationToken,
}

pub struct AppState {
    pub app_handle: AppHandle,
    pub data_dir: PathBuf,
    inner: RwLock<Inner>,
}

#[derive(Default)]
struct Inner {
    current: Option<Arc<MeetingHandle>>,
}

impl AppState {
    pub fn new(app_handle: AppHandle, data_dir: PathBuf) -> Self {
        Self {
            app_handle,
            data_dir,
            inner: RwLock::new(Inner::default()),
        }
    }

    pub fn meetings_dir(&self) -> PathBuf {
        self.data_dir.join("meetings")
    }

    pub fn current(&self) -> Option<Arc<MeetingHandle>> {
        self.inner.read().current.clone()
    }

    pub fn set_current(&self, handle: Arc<MeetingHandle>) {
        self.inner.write().current = Some(handle);
    }

    pub fn clear_current(&self) {
        self.inner.write().current = None;
    }

    pub fn emit<S: Serialize + Clone>(&self, event: &str, payload: S) {
        if let Err(err) = self.app_handle.emit(event, payload) {
            tracing::warn!(?err, %event, "emit failed");
        }
    }
}
