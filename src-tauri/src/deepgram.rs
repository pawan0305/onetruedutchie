use std::time::Duration;

use anyhow::{Context, Result};
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_util::sync::CancellationToken;

const DEEPGRAM_WS_URL: &str = "wss://api.deepgram.com/v1/listen";

pub struct DeepgramConfig {
    pub api_key: String,
    pub model: String,    // "nova-2"
    pub language: String, // "nl"
    pub sample_rate: u32, // 16000
    pub channels: u16,    // 1
    pub interim: bool,
}

impl Default for DeepgramConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: "nova-2".to_string(),
            language: "nl".to_string(),
            sample_rate: 16_000,
            channels: 1,
            interim: true,
        }
    }
}

#[derive(Debug, Clone)]
pub enum DeepgramEvent {
    Interim {
        text: String,
        start: f64,
    },
    Final {
        text: String,
        start: f64,
        duration: f64,
        speech_final: bool,
    },
    UtteranceEnd,
    Error(String),
    Closed,
}

#[derive(Debug, Deserialize)]
struct DgMessage {
    #[serde(default, rename = "type")]
    typ: String,
    #[serde(default)]
    is_final: bool,
    #[serde(default)]
    speech_final: bool,
    #[serde(default)]
    start: f64,
    #[serde(default)]
    duration: f64,
    #[serde(default)]
    channel: Option<DgChannel>,
}

#[derive(Debug, Deserialize)]
struct DgChannel {
    #[serde(default)]
    alternatives: Vec<DgAlternative>,
}

#[derive(Debug, Deserialize)]
struct DgAlternative {
    #[serde(default)]
    transcript: String,
}

pub async fn run(
    cfg: DeepgramConfig,
    mut audio_rx: mpsc::Receiver<Bytes>,
    out: mpsc::Sender<DeepgramEvent>,
    cancel: CancellationToken,
) -> Result<()> {
    let url = build_url(&cfg);
    let mut req = url.into_client_request().context("build request")?;
    req.headers_mut().insert(
        "Authorization",
        HeaderValue::from_str(&format!("Token {}", cfg.api_key))
            .context("invalid api key")?,
    );

    let (ws, _) = tokio_tungstenite::connect_async(req)
        .await
        .context("connecting to Deepgram")?;
    tracing::info!("deepgram connected");
    let (mut sink, mut stream) = ws.split();

    // Sender: forward audio frames to Deepgram, plus periodic KeepAlive.
    let send_cancel = cancel.clone();
    let sender = tokio::spawn(async move {
        let mut keepalive = tokio::time::interval(Duration::from_secs(5));
        keepalive.tick().await; // skip immediate
        loop {
            tokio::select! {
                _ = send_cancel.cancelled() => break,
                _ = keepalive.tick() => {
                    let msg = Message::Text(r#"{"type":"KeepAlive"}"#.into());
                    if sink.send(msg).await.is_err() { break; }
                }
                chunk = audio_rx.recv() => match chunk {
                    Some(bytes) => {
                        if sink.send(Message::Binary(bytes.to_vec())).await.is_err() {
                            break;
                        }
                    }
                    None => {
                        // Audio source closed; tell Deepgram to flush.
                        let _ = sink.send(Message::Text(r#"{"type":"CloseStream"}"#.into())).await;
                        break;
                    }
                }
            }
        }
        let _ = sink.close().await;
    });

    // Receiver: parse JSON messages and emit DeepgramEvents.
    let recv_cancel = cancel.clone();
    let recv_out = out.clone();
    let receiver = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = recv_cancel.cancelled() => break,
                msg = stream.next() => match msg {
                    Some(Ok(Message::Text(txt))) => {
                        match serde_json::from_str::<DgMessage>(&txt) {
                            Ok(m) => {
                                if let Some(evt) = into_event(m) {
                                    if recv_out.send(evt).await.is_err() { break; }
                                }
                            }
                            Err(err) => {
                                tracing::warn!(?err, txt = %txt, "deepgram parse error");
                            }
                        }
                    }
                    Some(Ok(Message::Binary(_))) => { /* ignore */ }
                    Some(Ok(Message::Close(frame))) => {
                        tracing::info!(?frame, "deepgram closed");
                        break;
                    }
                    Some(Ok(_)) => { /* Ping/Pong/Frame handled by tungstenite */ }
                    Some(Err(err)) => {
                        let _ = recv_out
                            .send(DeepgramEvent::Error(format!("ws error: {err}")))
                            .await;
                        break;
                    }
                    None => break,
                }
            }
        }
        let _ = recv_out.send(DeepgramEvent::Closed).await;
    });

    let _ = tokio::join!(sender, receiver);
    Ok(())
}

fn into_event(m: DgMessage) -> Option<DeepgramEvent> {
    match m.typ.as_str() {
        "Results" => {
            let text = m
                .channel
                .as_ref()
                .and_then(|c| c.alternatives.first())
                .map(|a| a.transcript.clone())
                .unwrap_or_default();
            if text.trim().is_empty() && !m.speech_final {
                return None;
            }
            if m.is_final {
                Some(DeepgramEvent::Final {
                    text,
                    start: m.start,
                    duration: m.duration,
                    speech_final: m.speech_final,
                })
            } else {
                Some(DeepgramEvent::Interim {
                    text,
                    start: m.start,
                })
            }
        }
        "UtteranceEnd" => Some(DeepgramEvent::UtteranceEnd),
        "Metadata" | "SpeechStarted" => None,
        other => {
            tracing::debug!(typ = %other, "ignored deepgram message");
            None
        }
    }
}

fn build_url(cfg: &DeepgramConfig) -> String {
    let mut u = url::Url::parse(DEEPGRAM_WS_URL).expect("static url");
    {
        let mut q = u.query_pairs_mut();
        q.append_pair("model", &cfg.model);
        q.append_pair("language", &cfg.language);
        q.append_pair("encoding", "linear16");
        q.append_pair("sample_rate", &cfg.sample_rate.to_string());
        q.append_pair("channels", &cfg.channels.to_string());
        q.append_pair("smart_format", "true");
        q.append_pair("punctuate", "true");
        q.append_pair("interim_results", if cfg.interim { "true" } else { "false" });
        q.append_pair("endpointing", "300");
        q.append_pair("utterance_end_ms", "1000");
        q.append_pair("vad_events", "true");
    }
    u.to_string()
}
