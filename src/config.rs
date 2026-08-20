use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::PathBuf;

const OPENAI_DEFAULT_MODEL: &str = "gpt-5.4-mini";
const OPENAI_DEFAULT_INSTRUCTIONS: &str =
    "You are Codex, based on GPT-5. You are running as a coding \
                                agent in the Codex CLI on a user's computer.";
const XAI_DEFAULT_MODEL: &str = "grok-4.6";
const XAI_DEFAULT_INSTRUCTIONS: &str =
    "You are Grok. You translate the user's request into a single shell command.";

#[derive(Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// LLM provider id. Supported: "openai-chatgpt", "xai-grok".
    pub provider: String,
    pub model: String,
    /// Reasoning effort for reasoning models, e.g. none | low | medium |
    /// high | xhigh — the accepted set varies by model.
    pub reasoning_effort: String,
    /// Provider-specific base instructions. The task-specific prompt is
    /// sent as a developer message instead.
    pub base_instructions: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            provider: crate::auth::OPENAI_CHATGPT.into(),
            model: OPENAI_DEFAULT_MODEL.into(),
            reasoning_effort: "low".into(),
            base_instructions: OPENAI_DEFAULT_INSTRUCTIONS.into(),
        }
    }
}

impl Config {
    pub fn path() -> Result<PathBuf> {
        // ~/.config (XDG style) on every platform, as documented in the
        // README; dirs::config_dir() would resolve to
        // ~/Library/Application Support on macOS.
        let dir = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| dirs::home_dir().map(|h| h.join(".config")))
            .context("cannot locate config directory")?;
        Ok(dir.join("shpell").join("config.toml"))
    }

    pub fn load() -> Result<Self> {
        let path = Self::path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let mut cfg: Self =
            toml::from_str(&raw).with_context(|| format!("parsing {}", path.display()))?;
        cfg.normalize();
        Ok(cfg)
    }

    /// Fill provider-specific defaults when the user only switches `provider`
    /// and leaves the ChatGPT model / Codex instructions in place.
    fn normalize(&mut self) {
        if self.provider != crate::auth::XAI_GROK {
            return;
        }
        if self.model == OPENAI_DEFAULT_MODEL {
            self.model = XAI_DEFAULT_MODEL.into();
        }
        if self.base_instructions == OPENAI_DEFAULT_INSTRUCTIONS {
            self.base_instructions = XAI_DEFAULT_INSTRUCTIONS.into();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xai_provider_replaces_openai_defaults() {
        let mut cfg = Config {
            provider: crate::auth::XAI_GROK.into(),
            ..Config::default()
        };
        cfg.normalize();
        assert_eq!(cfg.model, XAI_DEFAULT_MODEL);
        assert_eq!(cfg.base_instructions, XAI_DEFAULT_INSTRUCTIONS);
    }

    #[test]
    fn xai_provider_keeps_explicit_model() {
        let mut cfg = Config {
            provider: crate::auth::XAI_GROK.into(),
            model: "grok-4.5".into(),
            ..Config::default()
        };
        cfg.normalize();
        assert_eq!(cfg.model, "grok-4.5");
    }
}
