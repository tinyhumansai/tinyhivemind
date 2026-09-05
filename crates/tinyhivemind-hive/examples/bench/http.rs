//! Drive one seat directly over HTTP, against an `OpenAI`- or `Anthropic`-shaped
//! chat endpoint, instead of shelling out to an agent CLI.
//!
//! This is the same experiment [`crate::live`] runs, over a different last
//! mile: [`crate::live::AgentPrompt`] assembles the identical prompt either
//! way, so an HTTP seat and a CLI seat parse identically and a room can mix
//! the two. The intended target is a local ladder router
//! (`http://127.0.0.1:6969`) that serves both `OpenAI`'s
//! `/v1/chat/completions` and `Anthropic`'s `/v1/messages`, but nothing here
//! is specific to it.
//!
//! ```sh
//! cargo run --release -p tinyhivemind-hive --example bench -- \
//!   --scenario crates/tinyhivemind-hive/examples/bench/scenarios/checkout-503.txt \
//!   --api-base http://127.0.0.1:6969 --model flash
//! ```
//!
//! Every request goes through the `curl` binary rather than an HTTP crate —
//! this workspace forbids a transport dependency in a pure crate, and the
//! example is built alongside it — with the whole request, headers included,
//! sent over `curl`'s own stdin via `--config -`. That keeps the API key, and
//! the request body, out of the process argument list entirely, which a
//! machine can read out of `ps` for as long as the process runs.

use std::fmt::Write as _;
use std::io::Write as _;
use std::process::{Command, Stdio};

use serde_json::Value;

use tinyhivemind_hive::referral::{Referral, ReferralKind};
use tinyhivemind_hive::{HiveTurn, SessionMessage};

use crate::live::AgentPrompt;
use crate::run::Participant;
use crate::swarm::SwarmMember;

/// Which wire format the backend speaks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Wire {
    /// `/v1/chat/completions`, `OpenAI`'s request and response shape.
    OpenAi,
    /// `/v1/messages`, `Anthropic`'s request and response shape.
    Anthropic,
}

impl Wire {
    /// Parse a `--wire` flag's value.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text {
            "openai" => Some(Self::OpenAi),
            "anthropic" => Some(Self::Anthropic),
            _ => None,
        }
    }
}

/// Whether the backend is asked to think before it answers.
///
/// The default is [`Thinking::On`], which sends no thinking field at all and
/// lets the endpoint do whatever it does by default. [`Thinking::Off`] asks
/// for it to be switched off, which on the `OpenAI` wire means sending
/// `"thinking": {"type": "disabled"}` alongside the request and on the
/// `Anthropic` wire means sending no thinking block, since that wire only
/// ever thinks when a request asks it to.
///
/// It is worth a flag because the two regimes cost about two orders of
/// magnitude apart on the same prompt: probed against the target ladder
/// router, `flash` spends roughly 300 completion tokens reasoning its way to
/// the one marker line and 38 with thinking disabled. Which of those a room
/// is measured under is a property of the run, not a constant.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Thinking {
    /// Send no thinking directive; the endpoint's own default applies.
    On,
    /// Ask the endpoint not to think before answering.
    Off,
}

impl Thinking {
    /// Parse a `--thinking` flag's value.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text {
            "on" => Some(Self::On),
            "off" => Some(Self::Off),
            _ => None,
        }
    }
}

/// The endpoint and credentials every seat on one backend shares.
#[derive(Clone)]
pub(crate) struct HttpConfig {
    /// The router's base URL, without a trailing slash or version path.
    pub(crate) base: String,
    /// The API key, read once from `--api-key-env` and never logged.
    pub(crate) key: String,
    /// Which wire format the endpoint speaks.
    pub(crate) wire: Wire,
    /// Per-request deadline, passed to curl as `--max-time`.
    pub(crate) timeout_secs: u64,
    /// Whether the endpoint is asked to think before it answers.
    pub(crate) thinking: Thinking,
}

/// Written by hand so the key cannot reach a log through a `{:?}`.
///
/// A derived `Debug` on a struct holding a credential is one interpolation
/// away from printing it, and this one is carried into every seat, every
/// error path and every panic message the harness can produce.
impl std::fmt::Debug for HttpConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HttpConfig")
            .field("base", &self.base)
            .field("key", &"<redacted>")
            .field("wire", &self.wire)
            .field("timeout_secs", &self.timeout_secs)
            .field("thinking", &self.thinking)
            .finish()
    }
}

/// Tokens spent and calls made by one seat, or one poll, over an HTTP backend.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Usage {
    /// Prompt/input tokens billed.
    pub(crate) input: u64,
    /// Completion/output tokens billed.
    pub(crate) output: u64,
    /// Successful requests made (a retried request still counts once).
    pub(crate) calls: u64,
}

impl Usage {
    fn add(&mut self, input: u64, output: u64) {
        self.input = self.input.saturating_add(input);
        self.output = self.output.saturating_add(output);
        self.calls = self.calls.saturating_add(1);
    }

    /// Tokens a failed attempt was billed for anyway.
    ///
    /// No call is counted: `calls` measures the requests that produced a
    /// turn, and the retry that follows counts itself. The tokens are real
    /// spend whether or not the reply was usable, so dropping them would
    /// under-report what the run cost.
    fn spent(&mut self, input: u64, output: u64) {
        self.input = self.input.saturating_add(input);
        self.output = self.output.saturating_add(output);
    }

    /// Total tokens spent, input and output combined.
    #[must_use]
    pub(crate) fn tokens(&self) -> u64 {
        self.input.saturating_add(self.output)
    }

    /// Cost at `cost_per_1k` units per thousand tokens.
    #[must_use]
    pub(crate) fn cost(&self, cost_per_1k: u64) -> u64 {
        self.tokens().saturating_mul(cost_per_1k) / 1000
    }
}

/// A shared handle onto one seat's running usage total.
///
/// [`HttpAgent`] is seated as a `Box<dyn Participant>` so the harness can mix
/// HTTP and CLI seats in one room, and the trait object erases everything but
/// [`Participant`] once it is boxed. This handle is the way around that: the
/// caller clones it out at seat-building time, before boxing, and reads it
/// back after the episode finishes driving.
pub(crate) type UsageHandle = std::rc::Rc<std::cell::RefCell<Usage>>;

/// A participant driven directly over HTTP rather than through a CLI.
pub(crate) struct HttpAgent {
    prompt: AgentPrompt,
    config: HttpConfig,
    model: String,
    usage: UsageHandle,
}

impl HttpAgent {
    /// Seat one HTTP-backed participant.
    pub(crate) fn new(prompt: AgentPrompt, config: HttpConfig, model: String) -> Self {
        Self {
            prompt,
            config,
            model,
            usage: UsageHandle::default(),
        }
    }

    /// A shared handle onto this seat's usage, to read back after it has been
    /// boxed as a `Box<dyn Participant>`.
    pub(crate) fn usage_handle(&self) -> UsageHandle {
        std::rc::Rc::clone(&self.usage)
    }

    /// What only this member knows, as a block for a prompt.
    pub(crate) fn private(&self) -> &str {
        self.prompt.private()
    }

    /// Render exactly what this turn is allowed to see.
    pub(crate) fn prompt_with(
        &self,
        turn: &HiveTurn,
        visible: &[&SessionMessage],
        extra: &str,
    ) -> String {
        self.prompt.prompt_with(turn, visible, extra)
    }

    /// Post one prompt and take its reply, retrying once.
    fn call(&mut self, text: &str) -> Result<String, String> {
        ask(&self.config, &self.model, text, &self.usage)
    }
}

impl Participant for HttpAgent {
    fn id(&self) -> &str {
        self.prompt.id()
    }

    fn speak(&mut self, turn: &HiveTurn, visible: &[&SessionMessage]) -> Result<String, String> {
        let text = self.prompt.prompt(turn, visible);
        self.call(&text)
    }
}

/// A member of a federation driven directly over HTTP.
///
/// Mirrors [`crate::live::LiveDeskAgent`]: an [`HttpAgent`] that also knows
/// which channel it sits on and who the other channels are.
pub(crate) struct HttpDeskAgent {
    agent: HttpAgent,
    /// This member's own desk, by display name.
    here: String,
    /// Every other channel, id and display name.
    peers: Vec<(String, String)>,
}

impl HttpDeskAgent {
    /// Seat one HTTP agent on a channel.
    pub(crate) fn new(agent: HttpAgent, here: String, peers: Vec<(String, String)>) -> Self {
        Self { agent, here, peers }
    }

    /// The sentence naming the channels this member may reach.
    fn directory(&self) -> String {
        let named = self
            .peers
            .iter()
            .map(|(id, name)| format!("@#{id} — the {name} desk"))
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "{}\n\n{}\nYou are on the {} desk. The desks you can address, and how:\n{named}\n",
            crate::live::CROSS_PROTOCOL,
            crate::live::CROSS_RULES,
            self.here,
        )
    }
}

impl SwarmMember for HttpDeskAgent {
    fn id(&self) -> &str {
        self.agent.id()
    }

    fn speak(&mut self, turn: &HiveTurn, visible: &[&SessionMessage]) -> Result<String, String> {
        let prompt = self.agent.prompt_with(turn, visible, &self.directory());
        self.agent.call(&prompt)
    }

    fn answer(
        &mut self,
        incoming: &Referral,
        visible: &[&SessionMessage],
    ) -> Result<String, String> {
        let transcript = AgentPrompt::render(visible);
        let asked = match incoming.kind {
            ReferralKind::Forward => format!(
                "@{} on another desk has asked your desk this, and your answer will be posted \
                 here and carried back to them:\n{}\n\nAnswer it in ONE line, beginning with \
                 !evidence. They cannot see anything on this desk, so state the facts they need \
                 — including the ones above that only you hold — rather than your conclusion \
                 from them. A desk that asks for a number and receives an argument has learned \
                 nothing it can check. Do not ask a question back and do not tell them which \
                 option to pick.",
                incoming.source_id, incoming.content,
            ),
            ReferralKind::Return => format!(
                "You asked another desk a question and this is what came back:\n{}\n\nRelay it \
                 to your own desk in ONE line, beginning with !evidence, stating what they told \
                 you. Do not add anything they did not say.",
                incoming.content,
            ),
        };
        let prompt = format!(
            "You are @{}, on the {} desk.\n\n{}\n{asked}\n\nYour desk's transcript so far:\n\
             {transcript}\n\nYour one line:",
            self.agent.id(),
            self.here,
            self.agent.private(),
        );
        self.agent.call(&prompt)
    }
}

/// Ask the backend once and, where trying again could plausibly help, twice.
///
/// Both attempts' token spend is recorded on `usage` even when the reply was
/// unusable: a 200 with empty content, or a 4xx from a request the endpoint
/// still billed, is real cost, and dropping it would make every cost-per-point
/// figure the harness prints optimistic.
///
/// A second attempt is made only for a failure that could go the other way —
/// a transport error, a 429, or a 5xx. A 400 or a 401 is a property of the
/// request, and repeating it buys a second identical rejection and a second
/// bill.
///
/// # Errors
///
/// Returns curl's own failure, an HTTP status outside 2xx (via
/// `fail-with-body`), a response that is not valid JSON, or a response with
/// no answer in the shape the wire format expects. When a retry was made and
/// also failed, the first attempt's error text is carried in the message
/// rather than discarded — the two are often different, and the first one is
/// usually the informative one.
pub(crate) fn ask(
    config: &HttpConfig,
    model: &str,
    prompt: &str,
    usage: &UsageHandle,
) -> Result<String, String> {
    let first = match attempt(config, model, prompt) {
        Ok(reply) => {
            usage.borrow_mut().add(reply.input, reply.output);
            return Ok(reply.answer);
        }
        Err(failed) => failed,
    };
    usage.borrow_mut().spent(first.spent.0, first.spent.1);
    if !first.retryable {
        return Err(first.message);
    }
    match attempt(config, model, prompt) {
        Ok(reply) => {
            usage.borrow_mut().add(reply.input, reply.output);
            Ok(reply.answer)
        }
        Err(second) => {
            usage.borrow_mut().spent(second.spent.0, second.spent.1);
            Err(format!(
                "{} (first attempt: {})",
                second.message, first.message
            ))
        }
    }
}

/// One usable reply and what it was billed.
struct Reply {
    answer: String,
    input: u64,
    output: u64,
}

/// One failed attempt: why, whether repeating it could help, and what it cost.
struct Failed {
    message: String,
    retryable: bool,
    /// Input and output tokens the response reported, or zeros when it
    /// carried no usage block at all.
    spent: (u64, u64),
}

/// The line `--write-out` puts on stderr so the status code is readable
/// without disturbing the JSON body on stdout.
const STATUS_PREFIX: &str = "http-status: ";

fn attempt(config: &HttpConfig, model: &str, prompt: &str) -> Result<Reply, Failed> {
    let (url, body) = request(config, model, prompt);
    let script = config_script(config, &url, &body);

    let output = match curl(&script) {
        Ok(output) => output,
        Err(message) => {
            return Err(Failed {
                message,
                // Nothing reached the endpoint, so nothing about the request
                // has been shown to be wrong.
                retryable: true,
                spent: (0, 0),
            });
        }
    };
    let stderr = String::from_utf8_lossy(&output.stderr);
    let status = http_status(&stderr);
    // `fail-with-body` keeps the body on a failing status, so an endpoint
    // that bills a rejected request still reports what it billed.
    let payload: Option<Value> = serde_json::from_slice(&output.stdout).ok();
    let spent = payload
        .as_ref()
        .map_or((0, 0), |payload| billed(config, payload));

    if !output.status.success() {
        let head = stderr
            .lines()
            .filter(|line| !line.starts_with(STATUS_PREFIX))
            .take(3)
            .collect::<Vec<_>>()
            .join(" | ");
        return Err(Failed {
            message: format!("curl exited with {}: {head}", output.status),
            retryable: retryable(status),
            spent,
        });
    }
    let payload = match serde_json::from_slice::<Value>(&output.stdout) {
        Ok(payload) => payload,
        Err(error) => {
            return Err(Failed {
                message: format!("{} returned invalid JSON: {error}", config.base),
                // A 2xx that did not decode is a truncated or mangled
                // response rather than a rejected request.
                retryable: true,
                spent,
            });
        }
    };
    match parse(config, &payload) {
        Ok((answer, input, output)) => Ok(Reply {
            answer,
            input,
            output,
        }),
        Err(message) => Err(Failed {
            message,
            retryable: true,
            spent,
        }),
    }
}

/// Whether a status is worth one more attempt.
///
/// `None` is a request that never got a status at all. A 429 is the endpoint
/// asking for exactly this, and a 5xx is the endpoint's own fault; every
/// other 4xx is a property of the request that a repeat would reproduce.
fn retryable(status: Option<u16>) -> bool {
    match status {
        None => true,
        Some(code) => code == 429 || (500..600).contains(&code),
    }
}

/// The status code curl wrote to stderr, if the request got one.
fn http_status(stderr: &str) -> Option<u16> {
    let code: u16 = stderr
        .lines()
        .find_map(|line| line.strip_prefix(STATUS_PREFIX))
        .and_then(|code| code.trim().parse().ok())?;
    // curl reports `000` when no response was received at all.
    (code > 0).then_some(code)
}

/// What one response says it was billed, in this backend's wire format.
fn billed(config: &HttpConfig, payload: &Value) -> (u64, u64) {
    let usage = &payload["usage"];
    match config.wire {
        Wire::OpenAi => (
            usage["prompt_tokens"].as_u64().unwrap_or(0),
            usage["completion_tokens"].as_u64().unwrap_or(0),
        ),
        Wire::Anthropic => (
            usage["input_tokens"].as_u64().unwrap_or(0),
            usage["output_tokens"].as_u64().unwrap_or(0),
        ),
    }
}

/// Run one curl config script and collect its output.
fn curl(script: &str) -> Result<std::process::Output, String> {
    let mut child = Command::new("curl")
        .args(["--config", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("could not run curl: {error}"))?;
    child
        .stdin
        .take()
        .ok_or_else(|| "failed to open curl input".to_owned())?
        .write_all(script.as_bytes())
        .map_err(|error| format!("failed to send curl config: {error}"))?;
    child
        .wait_with_output()
        .map_err(|error| format!("curl failed: {error}"))
}

/// Reply budget for one turn.
///
/// A one-line marker reply costs almost nothing on its own, but the `flash`
/// model behind the target ladder router spends its budget on a hidden
/// reasoning block before it ever writes the line the room reads. Smoke
/// testing the crate's first value of 300 against the real router returned
/// `finish_reason: "length"` with an empty `content` on the very first turn
/// of a simple synthetic room, and 6000 was still not enough for the
/// harness's own hidden-profile scenario: the same model reasoned in circles
/// past an 8000-token budget without ever reaching a line, on more than one
/// seat and more than one run.
///
/// 16000 is where that stops. Probed against the router on the shipped
/// `checkout-503` scenario, every seat finished its reasoning and emitted the
/// marker line, at roughly 300 completion tokens a turn — so the budget is a
/// ceiling the run does not approach rather than a cost it pays. A room that
/// wants the cheap regime instead should ask for `--thinking off`, which on
/// the same prompt costs about 38 completion tokens, rather than starve the
/// reasoning with a smaller ceiling and collect empty turns.
const MAX_TOKENS: u32 = 16_000;

/// The endpoint and JSON body for one request.
///
/// One user message carries the whole assembled prompt, so an HTTP seat reads
/// exactly what a CLI seat would have been given as its final argument — a
/// system message is not needed to make the two parse identically.
fn request(config: &HttpConfig, model: &str, prompt: &str) -> (String, Value) {
    let base = config.base.trim_end_matches('/');
    match config.wire {
        Wire::OpenAi => {
            let mut body = serde_json::json!({
                "model": model,
                "temperature": 0,
                "max_tokens": MAX_TOKENS,
                "messages": [{"role": "user", "content": prompt}],
            });
            // Only the `OpenAI` wire carries a thinking directive here. The
            // `Anthropic` wire thinks only when a request asks it to, so
            // "off" there is the absence of a block rather than a block
            // saying off, and sending one would be sending an unknown field.
            if config.thinking == Thinking::Off
                && let Some(object) = body.as_object_mut()
            {
                object.insert(
                    "thinking".to_owned(),
                    serde_json::json!({"type": "disabled"}),
                );
            }
            (format!("{base}/v1/chat/completions"), body)
        }
        Wire::Anthropic => (
            format!("{base}/v1/messages"),
            serde_json::json!({
                "model": model,
                "temperature": 0,
                "max_tokens": MAX_TOKENS,
                "messages": [{"role": "user", "content": prompt}],
            }),
        ),
    }
}

/// The curl config script for one request, read over stdin via `--config -`.
///
/// Every part of the request — URL, headers, and the JSON body — is written
/// into this one script rather than passed as process arguments, so neither
/// the API key nor the prompt ever appears in the argument list a machine on
/// the same host could read out of `ps`.
fn config_script(config: &HttpConfig, url: &str, body: &Value) -> String {
    let mut script = format!(
        "url = \"{}\"\nrequest = \"POST\"\nheader = \"Content-Type: application/json\"\n",
        escape(url),
    );
    match config.wire {
        Wire::OpenAi => {
            let _ = writeln!(
                script,
                "header = \"Authorization: Bearer {}\"",
                escape(&config.key)
            );
        }
        Wire::Anthropic => {
            let _ = writeln!(script, "header = \"x-api-key: {}\"", escape(&config.key));
            script.push_str("header = \"anthropic-version: 2023-06-01\"\n");
        }
    }
    let _ = writeln!(script, "data-binary = \"{}\"", escape(&body.to_string()));
    let _ = writeln!(script, "max-time = {}", config.timeout_secs);
    // The status goes to stderr rather than stdout, so reading it costs the
    // JSON body nothing.
    let _ = writeln!(
        script,
        "write-out = \"%{{stderr}}{STATUS_PREFIX}%{{http_code}}\\n\""
    );
    script.push_str("silent\nshow-error\nfail-with-body\n");
    script
}

/// Escape a value for a double-quoted curl config entry.
///
/// curl's config-file quoting recognises a backslash escape for the
/// characters that would otherwise end the quoted value early; a compact JSON
/// body only ever needs the two the format actually reserves.
fn escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            _ => escaped.push(character),
        }
    }
    escaped
}

/// Read the reply and usage out of one response, in this backend's wire
/// format.
///
/// The content is taken through [`crate::live::marker_line`], the same
/// function a CLI seat's stdout goes through, so an HTTP seat and a CLI seat
/// contribute the identical line for the identical answer.
fn parse(config: &HttpConfig, payload: &Value) -> Result<(String, u64, u64), String> {
    match config.wire {
        Wire::OpenAi => {
            let content = payload["choices"][0]["message"]["content"]
                .as_str()
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .ok_or_else(|| format!("{} returned no assistant content", config.base))?;
            let (input, output) = billed(config, payload);
            Ok((crate::live::marker_line(content), input, output))
        }
        Wire::Anthropic => {
            let content = payload["content"]
                .as_array()
                .and_then(|blocks| {
                    blocks
                        .iter()
                        .find(|block| block["type"].as_str() == Some("text"))
                })
                .and_then(|block| block["text"].as_str())
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .ok_or_else(|| format!("{} returned no text content", config.base))?;
            let (input, output) = billed(config, payload);
            Ok((crate::live::marker_line(content), input, output))
        }
    }
}
