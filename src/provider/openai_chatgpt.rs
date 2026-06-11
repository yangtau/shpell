//! Provider backed by a ChatGPT subscription via the codex backend
//! (`chatgpt.com/backend-api/codex/responses`), authenticated with the
//! OAuth tokens from `shpell auth login`. No per-token API billing.

use super::{GenRequest, Provider};
use crate::auth;
use crate::config::Config;
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader};

const URL: &str = "https://chatgpt.com/backend-api/codex/responses";

pub struct OpenAiChatGpt {
    cfg: Config,
}

impl OpenAiChatGpt {
    pub fn new(cfg: Config) -> Self {
        Self { cfg }
    }
}

fn developer_prompt(req: &GenRequest) -> String {
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

fn message(role: &str, text: &str) -> Value {
    json!({
        "type": "message",
        "role": role,
        "content": [{ "type": "input_text", "text": text }],
    })
}

impl Provider for OpenAiChatGpt {
    fn generate(&self, req: &GenRequest, on_progress: &mut dyn FnMut(&str)) -> Result<String> {
        let (token, account_id) = auth::access()?;

        let body = json!({
            "model": self.cfg.model,
            "instructions": self.cfg.base_instructions,
            "input": [
                message("developer", &developer_prompt(req)),
                message("user", &req.query),
            ],
            // The codex backend only supports streaming and rejects
            // persisted conversations.
            "stream": true,
            "store": false,
            "reasoning": { "effort": self.cfg.reasoning_effort },
        });

        let resp = reqwest::blocking::Client::new()
            .post(URL)
            .bearer_auth(&token)
            .header("ChatGPT-Account-Id", &account_id)
            .header("OpenAI-Beta", "responses=experimental")
            .header("originator", "codex_cli_rs")
            .header("Accept", "text/event-stream")
            .json(&body)
            .send()
            .context("request to ChatGPT backend failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            bail!("ChatGPT backend returned {status}: {text}");
        }

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
                        let snapshot = super::postprocess(&out);
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

        let command = super::postprocess(&out);
        if command.is_empty() {
            bail!("model returned no command");
        }
        Ok(command)
    }
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
