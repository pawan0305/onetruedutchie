use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeys {
    pub deepgram: Option<String>,
    pub anthropic: Option<String>,
    /// Per-chunk Claude translation. When false, segments stay in the source
    /// language (no Claude calls per chunk). User can flip this from the
    /// top bar — for English meetings where translation is unnecessary.
    #[serde(default = "default_translate")]
    pub translate: bool,
    /// Subtitle overlay mode: "off" | "dual" (NL+EN) | "en" (EN only).
    #[serde(default = "default_overlay")]
    pub overlay_mode: String,
    /// Subtitle font size in px.
    #[serde(default = "default_overlay_size")]
    pub overlay_font_size: u32,
    /// When true the overlay is click-through (locked). When false the user
    /// can grab and drag/resize it.
    #[serde(default = "default_overlay_locked")]
    pub overlay_locked: bool,
    /// Custom vocabulary fed to Deepgram (`keyterm` parameter on Nova-3).
    /// One word/phrase per entry — colleague names, jargon, etc. Boosts
    /// transcription accuracy for those terms specifically.
    #[serde(default)]
    pub keywords: Vec<String>,
    /// Saved overlay window geometry (None = let Tauri center).
    #[serde(default)]
    pub overlay_x: Option<i32>,
    #[serde(default)]
    pub overlay_y: Option<i32>,
    #[serde(default)]
    pub overlay_w: Option<u32>,
    #[serde(default)]
    pub overlay_h: Option<u32>,
    /// Target language for Claude (translation, summary, chat). The source
    /// language is auto-detected by Deepgram. Stored as a human-readable
    /// language name ("English", "Spanish", "Japanese"…) so it drops
    /// straight into the prompts. Default "English".
    #[serde(default = "default_target_language")]
    pub target_language: String,
}

impl Default for ApiKeys {
    fn default() -> Self {
        Self {
            deepgram: None,
            anthropic: None,
            translate: default_translate(),
            overlay_mode: default_overlay(),
            overlay_font_size: default_overlay_size(),
            overlay_locked: default_overlay_locked(),
            keywords: vec![],
            overlay_x: None,
            overlay_y: None,
            overlay_w: None,
            overlay_h: None,
            target_language: default_target_language(),
        }
    }
}

fn default_translate() -> bool { true }
fn default_overlay() -> String { "off".to_string() }
fn default_overlay_size() -> u32 { 24 }
fn default_overlay_locked() -> bool { true }
fn default_target_language() -> String { "English".to_string() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsView {
    pub deepgram_set: bool,
    pub anthropic_set: bool,
    pub translate: bool,
    pub overlay_mode: String,
    pub overlay_font_size: u32,
    pub overlay_locked: bool,
    pub keywords: Vec<String>,
    pub target_language: String,
}

/// ~/Library/Application Support/com.onetruedutchie.app/keys.json
fn keys_path() -> PathBuf {
    let base = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(base)
        .join("Library/Application Support/com.onetruedutchie.app")
        .join("keys.json")
}

pub fn read_keys() -> Result<ApiKeys> {
    let path = keys_path();
    if !path.exists() {
        return Ok(ApiKeys::default());
    }
    let data = fs::read_to_string(&path).context("read keys file")?;
    serde_json::from_str(&data).context("parse keys file")
}

pub fn settings_view() -> Result<SettingsView> {
    let keys = read_keys()?;
    Ok(SettingsView {
        deepgram_set: keys.deepgram.as_deref().map(|s| !s.is_empty()).unwrap_or(false),
        anthropic_set: keys.anthropic.as_deref().map(|s| !s.is_empty()).unwrap_or(false),
        translate: keys.translate,
        overlay_mode: keys.overlay_mode.clone(),
        overlay_font_size: keys.overlay_font_size,
        overlay_locked: keys.overlay_locked,
        keywords: keys.keywords.clone(),
        target_language: keys.target_language.clone(),
    })
}

pub fn read_target_language() -> String {
    read_keys()
        .map(|k| k.target_language)
        .unwrap_or_else(|_| "English".into())
}

pub fn set_target_language(lang: &str) -> Result<()> {
    let mut keys = read_keys().unwrap_or_default();
    let trimmed = lang.trim();
    keys.target_language = if trimmed.is_empty() {
        "English".into()
    } else {
        trimmed.to_string()
    };
    write_keys_back(&keys)
}

pub fn read_keywords() -> Vec<String> {
    read_keys().map(|k| k.keywords).unwrap_or_default()
}

pub fn set_keywords(words: Vec<String>) -> Result<()> {
    let mut keys = read_keys().unwrap_or_default();
    keys.keywords = words
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    write_keys_back(&keys)
}

pub fn read_overlay_geometry() -> (Option<i32>, Option<i32>, Option<u32>, Option<u32>) {
    let k = read_keys().unwrap_or_default();
    (k.overlay_x, k.overlay_y, k.overlay_w, k.overlay_h)
}

pub fn set_overlay_geometry(x: i32, y: i32, w: u32, h: u32) -> Result<()> {
    let mut keys = read_keys().unwrap_or_default();
    keys.overlay_x = Some(x);
    keys.overlay_y = Some(y);
    keys.overlay_w = Some(w);
    keys.overlay_h = Some(h);
    write_keys_back(&keys)
}

pub fn read_overlay_mode() -> String {
    read_keys().map(|k| k.overlay_mode).unwrap_or_else(|_| "off".into())
}

pub fn read_overlay_locked() -> bool {
    read_keys().map(|k| k.overlay_locked).unwrap_or(true)
}

fn write_keys_back(keys: &ApiKeys) -> Result<()> {
    let path = keys_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("create config dir")?;
    }
    let data = serde_json::to_string_pretty(keys).context("serialize keys")?;
    fs::write(&path, &data).context("write keys file")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

pub fn set_overlay_mode(mode: &str) -> Result<()> {
    let mut keys = read_keys().unwrap_or_default();
    keys.overlay_mode = mode.to_string();
    write_keys_back(&keys)
}

pub fn set_overlay_font_size(size: u32) -> Result<()> {
    let mut keys = read_keys().unwrap_or_default();
    // Clamp to a sensible range so a typo can't make the overlay unusable.
    keys.overlay_font_size = size.clamp(12, 64);
    write_keys_back(&keys)
}

pub fn set_overlay_locked(locked: bool) -> Result<()> {
    let mut keys = read_keys().unwrap_or_default();
    keys.overlay_locked = locked;
    write_keys_back(&keys)
}

pub fn read_translate_enabled() -> bool {
    read_keys().map(|k| k.translate).unwrap_or(true)
}

pub fn set_translate_enabled(enabled: bool) -> Result<()> {
    let mut keys = read_keys().unwrap_or_default();
    keys.translate = enabled;
    let path = keys_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("create config dir")?;
    }
    let data = serde_json::to_string_pretty(&keys).context("serialize keys")?;
    fs::write(&path, &data).context("write keys file")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

pub fn write_keys(deepgram: Option<&str>, anthropic: Option<&str>) -> Result<()> {
    let mut keys = read_keys().unwrap_or_default();
    if let Some(v) = deepgram {
        keys.deepgram = if v.is_empty() { None } else { Some(v.to_string()) };
    }
    if let Some(v) = anthropic {
        keys.anthropic = if v.is_empty() { None } else { Some(v.to_string()) };
    }
    let path = keys_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("create config dir")?;
    }
    let data = serde_json::to_string_pretty(&keys).context("serialize keys")?;
    fs::write(&path, &data).context("write keys file")?;
    // owner read/write only — equivalent to chmod 600
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

pub fn require_deepgram() -> Result<String> {
    read_keys()?
        .deepgram
        .filter(|s| !s.is_empty())
        .context("Deepgram API key not set. Open Settings and add it.")
}

pub fn require_anthropic() -> Result<String> {
    read_keys()?
        .anthropic
        .filter(|s| !s.is_empty())
        .context("Anthropic API key not set. Open Settings and add it.")
}
