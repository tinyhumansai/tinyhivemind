//! Opt-in live coverage for a deliberation episode driven by real models.
//!
//! This asserts **structure, not quality**: that real models emit parseable
//! traces, that the episode terminates inside its budget by one of its four
//! terminal steps, that exactly one agent spoke per turn, and that attribution
//! survives. Nothing here claims the room reached a good answer, and nothing
//! here could show that it did.

#![cfg(feature = "e2e")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "support/hive_harness.rs"]
mod hive_harness;

use std::io::Write;
use std::process::{Command, Stdio};

use hive_harness::{HiveAgent, HiveHarness, Outcome};
use serde_json::{Value, json};
use tinyteams_hive::{
    EpisodePolicy, EpisodeState, HiveTurn, QuorumPolicy, SessionMessage, Visibility,
};

const ENABLE_ENV: &str = "TINYTEAMS_LIVE_OPENROUTER";
const KEY_ENV: &str = "OPENROUTER_API_KEY";
const MODEL_ENV: &str = "OPENROUTER_MODEL";

const PROTOCOL: &str = "\
You are deliberating with peers in a shared room. Reply with ONE line only, \
beginning with exactly one of these markers:
!propose #topic <one sentence>   put a new option on the floor
!support #topic ^N <why>         back an option, citing message N as grounds
!object >N ^M <why>              object to message N, citing message M
!evidence ^N <fact>              add grounds without taking a side
!question <what you need>        ask for something not yet established
Topics are short kebab-case words. A support with no ^citation does not count. \
Do not write anything before or after the single marker line.";

struct OpenRouterAgent {
    id: String,
    role: &'static str,
    api_key: String,
    model: String,
}

impl OpenRouterAgent {
    fn new(id: &'static str, role: &'static str, api_key: &str, model: &str) -> Self {
        Self {
            id: id.to_owned(),
            role,
            api_key: api_key.to_owned(),
            model: model.to_owned(),
        }
    }
}

impl HiveAgent for OpenRouterAgent {
    fn id(&self) -> &str {
        &self.id
    }

    fn speak(&mut self, turn: &HiveTurn, visible: &[&SessionMessage]) -> Result<String, String> {
        // The model sees exactly what the library authorized it to see. Under a
        // blind turn that is deliberately less than the whole room.
        let transcript = visible
            .iter()
            .map(|message| {
                let author = match &message.author {
                    tinyteams_hive::SessionAuthor::Agent { label, .. }
                    | tinyteams_hive::SessionAuthor::Person { label, .. }
                    | tinyteams_hive::SessionAuthor::System { label, .. } => label.as_str(),
                    tinyteams_hive::SessionAuthor::Operator => "operator",
                };
                format!("[{}] {author}: {}", message.sequence, message.content)
            })
            .collect::<Vec<_>>()
            .join("\n");
        let sight = match turn.visibility {
            Visibility::Blind => {
                "You cannot yet see your peers' positions. Form your own first."
            }
            Visibility::Full => "You can see the whole room.",
        };

        let body = json!({
            "model": self.model,
            "temperature": 0,
            "max_tokens": 120,
            "messages": [
                {
                    "role": "system",
                    "content": format!(
                        "You are @{}, the {} on a small team. {sight}\n\n{PROTOCOL}",
                        self.id, self.role,
                    ),
                },
                {
                    "role": "user",
                    "content": format!("Shared attributed transcript:\n{transcript}"),
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
        // The key travels over stdin so it never appears in the argument list.
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
            .map(|content| content.trim().to_owned())
            .filter(|content| !content.is_empty())
            .ok_or_else(|| "OpenRouter returned no assistant content".to_owned())
    }
}

#[test]
fn live_agents_deliberate_and_the_episode_terminates_within_its_budget() -> Result<(), String> {
    if !live_tests_enabled() {
        return Ok(());
    }

    let api_key =
        std::env::var(KEY_ENV).map_err(|_| format!("{KEY_ENV} must be set when {ENABLE_ENV}=1"))?;
    let model = std::env::var(MODEL_ENV)
        .map_err(|_| format!("{MODEL_ENV} must be set when {ENABLE_ENV}=1"))?;

    let mut planner = OpenRouterAgent::new(
        "planner",
        "planner, who proposes concrete options",
        &api_key,
        &model,
    );
    let mut critic = OpenRouterAgent::new(
        "critic",
        "critic, who looks for the weakness in a proposal",
        &api_key,
        &model,
    );
    let mut archivist = OpenRouterAgent::new(
        "archivist",
        "archivist, who supplies precedent and evidence",
        &api_key,
        &model,
    );

    let mut harness =
        HiveHarness::new("engineering", "Engineering", &["planner", "critic", "archivist"]);
    harness.operator(
        "We must choose one rollout strategy for a risky migration. Decide together.",
    );
    let state = EpisodeState::opened(harness.conversation(), harness.watermark());
    let policy = EpisodePolicy {
        turn_budget: 6,
        quorum: QuorumPolicy {
            threshold: 2,
            window: 100,
            require_grounded: true,
        },
        ..EpisodePolicy::DEFAULT
    };

    let (outcome, steps) = harness.run(
        state,
        &policy,
        &mut [&mut planner, &mut critic, &mut archivist],
    )?;

    // The episode terminated for a nameable reason inside its budget.
    assert!(
        matches!(
            outcome,
            Outcome::Converged { .. }
                | Outcome::Deadlocked { .. }
                | Outcome::Exhausted { .. }
                | Outcome::Idle
        ),
        "unexpected outcome {outcome:?}",
    );
    assert!(
        steps.len() <= policy.turn_budget as usize,
        "an episode must never exceed its budget: {steps:?}",
    );

    // Exactly one agent spoke per turn, each was a real desk member, and the
    // journal grew by exactly the number of authorized turns.
    assert_eq!(harness.journal().len(), steps.len() + 1);
    for step in &steps {
        assert!(
            ["planner", "critic", "archivist"].contains(&step.agent_id.as_str()),
            "an unknown agent took a turn: {step:?}",
        );
        assert!(!step.content.trim().is_empty());
    }

    // Attribution survives the round trip: every appended row names its author.
    for message in harness.journal().iter().skip(1) {
        assert!(
            matches!(message.author, tinyteams_hive::SessionAuthor::Agent { .. }),
            "an agent turn lost its attribution: {message:?}",
        );
    }

    // At least one turn produced a trace the grammar could actually read. This
    // is the only claim made about what the models said.
    let traces = tinyteams_hive::read(harness.journal());
    assert!(
        !traces.is_empty(),
        "no live turn emitted a parseable trace; transcript was {:?}",
        harness.journal(),
    );

    for step in &steps {
        println!(
            "{:>10}  {:?}  {:?}  {}",
            step.agent_id, step.reason, step.visibility, step.content,
        );
    }
    println!("outcome: {outcome:?}");
    Ok(())
}

fn live_tests_enabled() -> bool {
    std::env::var(ENABLE_ENV).is_ok_and(|value| value == "1" || value.eq_ignore_ascii_case("true"))
}
