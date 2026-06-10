mod openai_chatgpt;

use crate::config::Config;
use anyhow::{bail, Result};

pub struct GenRequest {
    pub query: String,
    pub shell: String,
    pub os: String,
    pub cwd: String,
}

pub trait Provider {
    /// Generate a shell command. `on_progress` is called with the cleaned
    /// command-so-far each time it grows, enabling live streaming; callers
    /// that don't stream pass a no-op. The final command is returned.
    fn generate(&self, req: &GenRequest, on_progress: &mut dyn FnMut(&str)) -> Result<String>;
}

pub fn from_config(cfg: &Config) -> Result<Box<dyn Provider>> {
    match cfg.provider.as_str() {
        "openai-chatgpt" => Ok(Box::new(openai_chatgpt::OpenAiChatGpt::new(cfg.clone()))),
        other => bail!("unknown provider {other:?} (supported: openai-chatgpt)"),
    }
}

/// Models occasionally wrap output in fences or prefix a prompt symbol
/// despite instructions; keep only the first command line.
pub fn postprocess(raw: &str) -> String {
    raw.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("```"))
        .map(|l| l.strip_prefix("$ ").unwrap_or(l))
        .next()
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::postprocess;

    #[test]
    fn strips_fences_and_prompt() {
        assert_eq!(postprocess("```sh\n$ touch test\n```"), "touch test");
        assert_eq!(postprocess("touch test\n"), "touch test");
        assert_eq!(postprocess("\n  ls -la  \nextra"), "ls -la");
    }
}
