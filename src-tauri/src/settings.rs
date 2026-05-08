use anyhow::{Context, Result};
use keyring::Entry;
use serde::{Deserialize, Serialize};

const SERVICE: &str = "com.onetruedutchie.app";
const DEEPGRAM_KEY: &str = "deepgram_api_key";
const ANTHROPIC_KEY: &str = "anthropic_api_key";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeys {
    pub deepgram: Option<String>,
    pub anthropic: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsView {
    pub deepgram_set: bool,
    pub anthropic_set: bool,
}

fn entry(name: &str) -> Result<Entry> {
    Entry::new(SERVICE, name).context("keyring entry")
}

pub fn read_keys() -> Result<ApiKeys> {
    let deepgram = match entry(DEEPGRAM_KEY)?.get_password() {
        Ok(v) => Some(v),
        Err(keyring::Error::NoEntry) => None,
        Err(e) => return Err(e.into()),
    };
    let anthropic = match entry(ANTHROPIC_KEY)?.get_password() {
        Ok(v) => Some(v),
        Err(keyring::Error::NoEntry) => None,
        Err(e) => return Err(e.into()),
    };
    Ok(ApiKeys { deepgram, anthropic })
}

pub fn settings_view() -> Result<SettingsView> {
    let keys = read_keys()?;
    Ok(SettingsView {
        deepgram_set: keys.deepgram.as_deref().map(str::is_empty).map(|e| !e).unwrap_or(false),
        anthropic_set: keys.anthropic.as_deref().map(str::is_empty).map(|e| !e).unwrap_or(false),
    })
}

pub fn write_keys(deepgram: Option<&str>, anthropic: Option<&str>) -> Result<()> {
    if let Some(v) = deepgram {
        if v.is_empty() {
            let _ = entry(DEEPGRAM_KEY)?.delete_credential();
        } else {
            entry(DEEPGRAM_KEY)?.set_password(v).context("set deepgram")?;
        }
    }
    if let Some(v) = anthropic {
        if v.is_empty() {
            let _ = entry(ANTHROPIC_KEY)?.delete_credential();
        } else {
            entry(ANTHROPIC_KEY)?.set_password(v).context("set anthropic")?;
        }
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
