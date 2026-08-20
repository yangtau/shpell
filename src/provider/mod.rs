mod openai_chatgpt;
mod xai_grok;

use crate::config::Config;
use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader};

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
        crate::auth::OPENAI_CHATGPT => {
            Ok(Box::new(openai_chatgpt::OpenAiChatGpt::new(cfg.clone())))
        }
        crate::auth::XAI_GROK => Ok(Box::new(xai_grok::XaiGrok::new(cfg.clone()))),
        other => bail!(
            "unknown provider {other:?} (supported: {}, {})",
            crate::auth::OPENAI_CHATGPT,
            crate::auth::XAI_GROK
        ),
    }
}

pub(crate) fn developer_prompt(req: &GenRequest) -> String {
    format!(
        "Translate the user's request into a single line for a {shell} prompt.\n\
         Environment: os={os}, shell={shell}, cwd={cwd}\n\
         Rules:\n\
         - If the request asks for a shell command, reply with ONLY that command on one line. No markdown, no code fences, no explanation.\n\
         - If the request is conversational, a question, or otherwise not a command (e.g. a greeting or \"how are you\"), reply with a single-line comment answering it: `# <short answer>`. Do NOT wrap the answer in echo.\n\
         - The reply is sent straight to the shell, so keep it to ONE line and avoid constructs that need escaping; in particular avoid an unescaped '!' (zsh history expansion).\n\
         - Prefer simple, idiomatic commands available on this OS.\n\
         - Never make the command destructive (rm -rf, force flags, overwrites) unless explicitly requested.",
        shell = req.shell,
        os = req.os,
        cwd = req.cwd,
    )
}

pub(crate) fn message(role: &str, text: &str) -> Value {
    json!({
        "type": "message",
        "role": role,
        "content": [{ "type": "input_text", "text": text }],
    })
}

/// Stream a Responses API SSE body, calling `on_progress` as the cleaned
/// command grows. Shared by ChatGPT and xAI.
pub(crate) fn read_responses_sse(
    resp: reqwest::blocking::Response,
    on_progress: &mut dyn FnMut(&str),
) -> Result<String> {
    let mut out = String::new();
    for line in BufReader::new(resp).lines() {
        let line = line?;
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data == "[DONE]" {
            break;
        }
        let Ok(event): Result<Value, _> = serde_json::from_str(data) else {
            continue;
        };
        match event["type"].as_str().unwrap_or("") {
            "response.output_text.delta" => {
                if let Some(d) = event["delta"].as_str() {
                    out.push_str(d);
                    let snapshot = postprocess(&out);
                    if !snapshot.is_empty() {
                        on_progress(&snapshot);
                    }
                }
            }
            "response.failed" | "error" => {
                bail!("generation failed: {}", event)
            }
            "response.completed" => {
                if out.is_empty() {
                    out = extract_output_text(&event["response"]);
                }
                break;
            }
            _ => {}
        }
    }
    let command = postprocess(&out);
    if command.is_empty() {
        bail!("model returned no command");
    }
    Ok(command)
}

/// Fallback when no deltas were received: pull text out of the final
/// response object.
fn extract_output_text(response: &Value) -> String {
    let mut out = String::new();
    if let Some(items) = response["output"].as_array() {
        for item in items {
            if item["type"] == "message" {
                if let Some(parts) = item["content"].as_array() {
                    for part in parts {
                        if let Some(t) = part["text"].as_str() {
                            out.push_str(t);
                        }
                    }
                }
            }
        }
    }
    out
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
