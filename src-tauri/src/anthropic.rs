use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

const ANTHROPIC_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";
pub const MODEL_HAIKU: &str = "claude-haiku-4-5";

pub struct AnthropicClient {
    api_key: String,
    http: reqwest::Client,
    model: String,
}

#[derive(Debug, Clone)]
pub enum ChatStreamEvent {
    Delta(String),
    Done(String),
    Error(String),
}

#[derive(Debug, Serialize)]
struct MessageReq<'a> {
    model: &'a str,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<Vec<SystemBlock<'a>>>,
    messages: Vec<MessageItem<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum SystemBlock<'a> {
    Text {
        text: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
}

#[derive(Debug, Serialize)]
struct CacheControl {
    #[serde(rename = "type")]
    typ: &'static str,
}

#[derive(Debug, Serialize)]
struct MessageItem<'a> {
    role: &'a str,
    content: Vec<ContentBlock<'a>>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ContentBlock<'a> {
    Text {
        text: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
}

#[derive(Debug, Deserialize)]
struct MessageResp {
    #[serde(default)]
    content: Vec<RespBlock>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum RespBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(other)]
    Other,
}

impl AnthropicClient {
    pub fn new(api_key: String) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .pool_idle_timeout(Duration::from_secs(60))
            .build()
            .expect("reqwest client");
        Self {
            api_key,
            http,
            model: MODEL_HAIKU.to_string(),
        }
    }

    fn auth(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        builder
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
    }

    /// Translate Dutch text to English. Non-streaming for simplicity & low overhead.
    pub async fn translate(&self, dutch: &str) -> Result<String> {
        if dutch.trim().is_empty() {
            return Ok(String::new());
        }
        let system = vec![SystemBlock::Text {
            text: TRANSLATE_SYSTEM,
            cache_control: Some(CacheControl { typ: "ephemeral" }),
        }];
        let messages = vec![MessageItem {
            role: "user",
            content: vec![ContentBlock::Text { text: dutch, cache_control: None }],
        }];
        let req = MessageReq {
            model: &self.model,
            max_tokens: 600,
            system: Some(system),
            messages,
            temperature: Some(0.2),
            stream: None,
        };
        let resp: MessageResp = self
            .auth(self.http.post(ANTHROPIC_URL))
            .json(&req)
            .send()
            .await
            .context("translate: send")?
            .error_for_status()
            .context("translate: status")?
            .json()
            .await
            .context("translate: decode")?;
        Ok(extract_text(&resp))
    }

    /// Generate a running summary of the transcript so far.
    pub async fn summarize(&self, transcript: &str) -> Result<String> {
        if transcript.trim().is_empty() {
            return Ok(String::new());
        }
        let system = vec![SystemBlock::Text {
            text: SUMMARY_SYSTEM,
            cache_control: Some(CacheControl { typ: "ephemeral" }),
        }];
        let user_text = format!("Transcript:\n\n{transcript}\n\nProduce the running summary now.");
        let messages = vec![MessageItem {
            role: "user",
            content: vec![ContentBlock::Text { text: &user_text, cache_control: None }],
        }];
        let req = MessageReq {
            model: &self.model,
            max_tokens: 800,
            system: Some(system),
            messages,
            temperature: Some(0.3),
            stream: None,
        };
        let resp: MessageResp = self
            .auth(self.http.post(ANTHROPIC_URL))
            .json(&req)
            .send()
            .await
            .context("summarize: send")?
            .error_for_status()
            .context("summarize: status")?
            .json()
            .await
            .context("summarize: decode")?;
        Ok(extract_text(&resp))
    }

    /// Stream a chat answer. The transcript is sent with cache_control so
    /// follow-up questions reuse the prefix and stay cheap.
    pub async fn chat_stream(
        &self,
        transcript: &str,
        history: &[(String, String)], // (role, content) prior turns, oldest first
        question: &str,
        tx: mpsc::Sender<ChatStreamEvent>,
    ) -> Result<()> {
        let system = vec![SystemBlock::Text {
            text: CHAT_SYSTEM,
            cache_control: Some(CacheControl { typ: "ephemeral" }),
        }];

        // First user message carries the (cached) transcript.
        let transcript_block = format!("Meeting transcript so far:\n\n{transcript}");
        let mut messages: Vec<MessageItem> = Vec::with_capacity(2 + history.len() * 2);
        messages.push(MessageItem {
            role: "user",
            content: vec![ContentBlock::Text {
                text: &transcript_block,
                cache_control: Some(CacheControl { typ: "ephemeral" }),
            }],
        });
        // Acknowledgement turn so prior chat history flows naturally.
        messages.push(MessageItem {
            role: "assistant",
            content: vec![ContentBlock::Text {
                text: "Understood. Ask your questions about the meeting.",
                cache_control: None,
            }],
        });
        for (role, content) in history {
            messages.push(MessageItem {
                role: role.as_str(),
                content: vec![ContentBlock::Text { text: content, cache_control: None }],
            });
        }
        messages.push(MessageItem {
            role: "user",
            content: vec![ContentBlock::Text { text: question, cache_control: None }],
        });

        let req = MessageReq {
            model: &self.model,
            max_tokens: 1024,
            system: Some(system),
            messages,
            temperature: Some(0.3),
            stream: Some(true),
        };

        let resp = self
            .auth(self.http.post(ANTHROPIC_URL))
            .json(&req)
            .send()
            .await
            .context("chat: send")?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("chat http {status}: {body}"));
        }

        let mut stream = resp.bytes_stream().eventsource();
        let mut accumulated = String::new();
        while let Some(event) = stream.next().await {
            let event = match event {
                Ok(e) => e,
                Err(err) => {
                    let _ = tx.send(ChatStreamEvent::Error(format!("{err}"))).await;
                    return Err(err.into());
                }
            };
            match event.event.as_str() {
                "content_block_delta" => {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&event.data) {
                        if let Some(text) = v
                            .get("delta")
                            .and_then(|d| d.get("text"))
                            .and_then(|t| t.as_str())
                        {
                            accumulated.push_str(text);
                            if tx.send(ChatStreamEvent::Delta(text.to_string())).await.is_err() {
                                break;
                            }
                        }
                    }
                }
                "message_stop" => break,
                "error" => {
                    let _ = tx.send(ChatStreamEvent::Error(event.data.clone())).await;
                    return Err(anyhow!("chat error: {}", event.data));
                }
                _ => {}
            }
        }
        let _ = tx.send(ChatStreamEvent::Done(accumulated)).await;
        Ok(())
    }
}

fn extract_text(resp: &MessageResp) -> String {
    let mut out = String::new();
    for block in &resp.content {
        if let RespBlock::Text { text } = block {
            out.push_str(text);
        }
    }
    out.trim().to_string()
}

const TRANSLATE_SYSTEM: &str = "You are a precise Dutch-to-English translator for live meeting \
transcripts. Translate the user's Dutch text into clear, idiomatic English. Output ONLY the \
English translation — no commentary, no quotes, no labels. If a sentence is already in English, \
keep it. If a sentence is mixed, translate the Dutch parts and leave the English parts intact. \
Preserve names, numbers, and acronyms. Translate filler words naturally (e.g. \"uh\", \"hmm\"). \
If input is empty or unintelligible, output an empty string.";

const SUMMARY_SYSTEM: &str = "You produce concise, accurate RUNNING summaries of live meeting \
transcripts. The transcript is in Dutch with English translations alongside. Write the summary \
in English. Structure:\n\
- 2-3 sentence overview at the top.\n\
- ## Decisions  (bullets, or 'None yet')\n\
- ## Action items  (bullets with owner if known, or 'None yet')\n\
- ## Open questions  (bullets, or 'None yet')\n\
Be specific (names, numbers, dates). Keep total under 250 words. Do not invent content not \
supported by the transcript.";

const CHAT_SYSTEM: &str = "You are a meeting assistant. You answer the user's questions about a \
meeting they attended. Use ONLY the provided transcript as ground truth. The transcript contains \
both Dutch (NL) and English (EN) lines with [HH:MM:SS] timestamps. When useful, cite timestamps. \
If the transcript does not contain the answer, say so plainly rather than guessing. Be concise.";
