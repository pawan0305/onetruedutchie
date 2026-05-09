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
}

impl Default for ApiKeys {
    fn default() -> Self {
        Self {
            deepgram: None,
            anthropic: None,
            translate: default_translate(),
            overlay_mode: default_overlay(),
        }
    }
}

fn default_translate() -> bool { true }
fn default_overlay() -> String { "off".to_string() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsView {
    pub deepgram_set: bool,
    pub anthropic_set: bool,
    pub translate: bool,
    pub overlay_mode: String,
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
    })
}

pub fn read_overlay_mode() -> String {
    read_keys().map(|k| k.overlay_mode).unwrap_or_else(|_| "off".into())
}

pub fn set_overlay_mode(mode: &str) -> Result<()> {
    let mut keys = read_keys().unwrap_or_default();
    keys.overlay_mode = mode.to_string();
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
