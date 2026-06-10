use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// LLM provider id. Supported: "openai-chatgpt".
    pub provider: String,
    pub model: String,
    /// Reasoning effort for reasoning models: minimal | low | medium | high.
    pub reasoning_effort: String,
    /// The ChatGPT codex backend expects Codex-style base instructions; the
    /// task-specific prompt is sent as a developer message instead.
    pub base_instructions: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            provider: "openai-chatgpt".into(),
            model: "gpt-5.2-codex".into(),
            reasoning_effort: "low".into(),
            base_instructions: "You are Codex, based on GPT-5. You are running as a coding \
                                agent in the Codex CLI on a user's computer."
                .into(),
        }
    }
}

impl Config {
    pub fn path() -> Result<PathBuf> {
        let dir = dirs::config_dir().context("cannot locate config directory")?;
        Ok(dir.join("x").join("config.toml"))
    }

    pub fn load() -> Result<Self> {
        let path = Self::path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        toml::from_str(&raw).with_context(|| format!("parsing {}", path.display()))
    }
}
