//! Provider backed by a SuperGrok / X Premium subscription via xAI OAuth
//! (`shpell auth login xai-grok`). Requests go to the public Responses API
//! (`api.x.ai/v1/responses`) with the subscription token — no API key.

use super::{GenRequest, Provider};
use crate::auth::{self, XAI_GROK};
use crate::config::Config;
use anyhow::{bail, Context, Result};
use serde_json::json;

const URL: &str = "https://api.x.ai/v1/responses";

pub struct XaiGrok {
    cfg: Config,
}

impl XaiGrok {
    pub fn new(cfg: Config) -> Self {
        Self { cfg }
    }
}

impl Provider for XaiGrok {
    fn generate(&self, req: &GenRequest, on_progress: &mut dyn FnMut(&str)) -> Result<String> {
        let tokens = auth::access(XAI_GROK)?;

        let body = json!({
            "model": self.cfg.model,
            "instructions": self.cfg.base_instructions,
            "input": [
                super::message("developer", &super::developer_prompt(req)),
                super::message("user", &req.query),
            ],
            "stream": true,
            "store": false,
            "reasoning": { "effort": self.cfg.reasoning_effort },
        });

        let resp = reqwest::blocking::Client::new()
            .post(URL)
            .bearer_auth(&tokens.access_token)
            .header("User-Agent", concat!("shpell/", env!("CARGO_PKG_VERSION")))
            .header("Accept", "text/event-stream")
            .json(&body)
            .send()
            .context("request to xAI failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            bail!("xAI returned {status}: {text}");
        }

        super::read_responses_sse(resp, on_progress)
    }
}
