//! Opt-in live end-to-end coverage for multi-agent `OpenRouter` coordination.

#![cfg(feature = "e2e")]

#[path = "support/harness.rs"]
mod harness;

use harness::{AgentEngine, ChatHarness, Message};
use serde_json::{Value, json};
use std::io::Write;
use std::process::{Command, Stdio};

const ENABLE_ENV: &str = "TINYHIVEMIND_LIVE_OPENROUTER";
const KEY_ENV: &str = "OPENROUTER_API_KEY";
const MODEL_ENV: &str = "OPENROUTER_MODEL";

struct OpenRouterAgent {
    name: &'static str,
    instruction: &'static str,
    api_key: String,
    model: String,
}

impl OpenRouterAgent {
    fn new(name: &'static str, instruction: &'static str, api_key: &str, model: &str) -> Self {
        Self {
            name,
            instruction,
            api_key: api_key.to_owned(),
            model: model.to_owned(),
        }
    }
}

impl AgentEngine for OpenRouterAgent {
    fn name(&self) -> &str {
        self.name
    }

    fn respond(&mut self, transcript: &[Message]) -> Result<String, String> {
        let attributed_transcript = transcript
            .iter()
            .map(|message| format!("{}: {}", message.author, message.content))
            .collect::<Vec<_>>()
            .join("\n");
        let body = json!({
            "model": self.model,
            "temperature": 0,
            "max_tokens": 120,
            "messages": [
                {
                    "role": "system",
                    "content": format!(
                        "You are the {} in a small agent team. {} Reply in at most two sentences.",
                        self.name, self.instruction,
                    ),
                },
                {
                    "role": "user",
                    "content": format!("Shared attributed transcript:\n{attributed_transcript}"),
                },
            ],
        });

        let request_body = body.to_string();
        let mut child = Command::new("curl")
            .args(["--config", "-", "--data-binary", &request_body])
            .env_remove(KEY_ENV)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| "live OpenRouter tests require curl".to_owned())?;
        let curl_config = format!(
            "url = \"https://openrouter.ai/api/v1/chat/completions\"\n\
             request = \"POST\"\n\
             header = \"Authorization: Bearer {}\"\n\
             header = \"Content-Type: application/json\"\n\
             silent\n\
             show-error\n\
             fail-with-body\n",
            self.api_key,
        );
        child
            .stdin
            .take()
            .ok_or_else(|| "failed to open curl input".to_owned())?
            .write_all(curl_config.as_bytes())
            .map_err(|_| "failed to configure OpenRouter request".to_owned())?;
        let output = child
            .wait_with_output()
            .map_err(|_| "OpenRouter request failed".to_owned())?;
        if !output.status.success() {
            return Err("OpenRouter request failed".to_owned());
        }
        let payload: Value = serde_json::from_slice(&output.stdout)
            .map_err(|_| "OpenRouter returned invalid JSON".to_owned())?;

        payload["choices"][0]["message"]["content"]
            .as_str()
            .filter(|content| !content.trim().is_empty())
            .map(str::to_owned)
            .ok_or_else(|| "OpenRouter returned no assistant content".to_owned())
    }
}

#[test]
fn live_agents_plan_critique_and_synthesize_over_one_shared_transcript() -> Result<(), String> {
    if !live_tests_enabled() {
        return Ok(());
    }

    let api_key =
        std::env::var(KEY_ENV).map_err(|_| format!("{KEY_ENV} must be set when {ENABLE_ENV}=1"))?;
    let model = std::env::var(MODEL_ENV)
        .map_err(|_| format!("{MODEL_ENV} must be set when {ENABLE_ENV}=1"))?;
    let mut planner = OpenRouterAgent::new(
        "planner",
        "Propose a concrete plan that another agent can challenge.",
        &api_key,
        &model,
    );
    let mut critic = OpenRouterAgent::new(
        "critic",
        "Identify one concrete weakness in the planner's proposal.",
        &api_key,
        &model,
    );
    let mut synthesizer = OpenRouterAgent::new(
        "synthesizer",
        "Reconcile the plan and critique into one final decision.",
        &api_key,
        &model,
    );
    let mut harness = ChatHarness::default();
    harness.send(
        None,
        "operator",
        "Design a reliable process for verifying a factual answer.",
    );

    harness.dispatch(Some("main"), &mut planner)?;
    harness.dispatch(Some("General"), &mut critic)?;
    harness.dispatch(None, &mut synthesizer)?;

    let transcript = harness.transcript(Some("GENERAL"));
    assert_eq!(transcript.len(), 4);
    assert_eq!(harness.journal().len(), 4);
    assert_eq!(transcript[1].author, "planner");
    assert_eq!(transcript[2].author, "critic");
    assert_eq!(transcript[3].author, "synthesizer");
    assert!(
        transcript[1..]
            .iter()
            .all(|message| !message.content.trim().is_empty())
    );
    Ok(())
}

fn live_tests_enabled() -> bool {
    std::env::var(ENABLE_ENV).is_ok_and(|value| value == "1" || value.eq_ignore_ascii_case("true"))
}
