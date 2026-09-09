//! Command-line options for the desk example, and how they are parsed.
//!
//! Every flag has a default that lets the example run against a local
//! `opencode`/ladder setup with nothing supplied but `--desk` and `--task`,
//! so `Options::parse` reads the environment for the rest before falling
//! back to a literal default.

use std::path::PathBuf;
use std::time::Duration;

use tinyhivemind::DigestPolicy;

use crate::BoxError;

/// What one invocation of this binary is for.
///
/// Three, because this one binary is three things: the desk, the MCP server
/// the agent CLI spawns for each seat, and a way to look at the surface those
/// seats are given.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Mode {
    /// Run the desk.
    Desk,
    /// Serve the room's tools over stdio and take no turn.
    ServeTools,
    /// Print the tool surface a seat is given, then exit.
    PrintSurface,
}

/// Command-line options.
pub(crate) struct Options {
    /// Path to the desk file describing the room and its seats.
    pub(crate) desk: PathBuf,
    /// Path to the chair's opening message.
    pub(crate) task: PathBuf,
    /// Directory every seat reads, writes, and runs code in.
    pub(crate) workspace: PathBuf,
    /// Path to the JSONL transcript; an existing one is resumed.
    pub(crate) transcript: PathBuf,
    /// The agent CLI invoked once per turn, with the prompt as its last
    /// argument.
    pub(crate) agent_cmd: String,
    /// How many times the chair may nudge a room that has gone quiet.
    pub(crate) rounds: usize,
    /// Hard ceiling on the number of turns this run may take.
    pub(crate) max_turns: usize,
    /// `MentionDispatchPolicy::max_hops`: how long one hand-off chain may
    /// run before it is refused.
    pub(crate) max_hops: u32,
    /// How many turns pass before the chair speaks again, cadence rather
    /// than only when the room falls silent. Zero disables the cadence.
    pub(crate) chair_every: usize,
    /// Whether a seat's prior CLI session is resumed rather than started
    /// fresh. Off by default; see `run.rs` for why.
    pub(crate) resume_sessions: bool,
    /// How many messages of transcript a seat is projected on one turn.
    pub(crate) window: usize,
    /// Wall-clock budget for one turn before the process is killed.
    pub(crate) timeout: Duration,
    /// `CortexDB` root; `None` disables recall and capture.
    pub(crate) cortex_base: Option<String>,
    /// The key for `cortex_base`, read from `CORTEX_API_KEY`.
    pub(crate) cortex_key: Option<String>,
    /// The durable, cross-run memory scope.
    pub(crate) library_scope: String,
    /// The per-run memory scope.
    pub(crate) session_scope: String,
    /// Passed through to the agent process so a run can pin one model.
    pub(crate) opencode_config: Option<String>,
    /// The router endpoint the wrap-up channel completes against.
    pub(crate) router_base: String,
    /// The router's API key.
    pub(crate) router_key: String,
    /// The model id the wrap-up channel asks for.
    pub(crate) router_model: String,
    /// What this invocation is for.
    pub(crate) mode: Mode,
    /// Where a turn's tool calls to the room are collected.
    pub(crate) outbox: Option<PathBuf>,
    /// Where the host says whose turn is running.
    ///
    /// Only the served side reads it, and only to price a `desk_dm` against
    /// the aside policy before the turn ends. A server without it still
    /// serves; it just cannot tell a seat its aside will be refused.
    pub(crate) turn: Option<PathBuf>,
    /// Whether messages older than the live window are folded into one
    /// standing account of the room.
    pub(crate) fold_account: bool,
    /// How many rows must accumulate past the window before a fold is worth
    /// spending a completion on. Lower it to exercise the account on a short
    /// desk; the default only pays off on a long one.
    pub(crate) fold_after: usize,
    /// Tokens of desk-visible scrollback that trigger a fold on their own.
    ///
    /// The other half of the same question. `fold_after` says the room has
    /// moved; this says its scrollback has grown expensive, and whichever
    /// binds first wins. Converted to a character threshold by
    /// [`DigestPolicy::from_token_budget`](tinyhivemind::DigestPolicy::from_token_budget),
    /// which states the ratio it assumes. Zero leaves only the row trigger.
    pub(crate) fold_tokens: usize,
}

impl Options {
    /// Parse the process's command-line arguments into [`Options`].
    ///
    /// # Errors
    ///
    /// Returns an error if an unknown flag is given, a flag's value fails to
    /// parse, or a required flag (`--desk`, `--task`) is missing.
    pub(crate) fn parse() -> Result<Self, BoxError> {
        let mut options = Self {
            desk: PathBuf::new(),
            task: PathBuf::new(),
            workspace: PathBuf::from("."),
            transcript: PathBuf::from("transcript.jsonl"),
            agent_cmd: "opencode run --auto --format json -m ladder/reasoning".into(),
            rounds: 8,
            max_turns: 40,
            max_hops: 6,
            chair_every: 6,
            resume_sessions: false,
            window: 40,
            timeout: Duration::from_secs(2400),
            cortex_base: std::env::var("CORTEX_BASE").ok(),
            cortex_key: std::env::var("CORTEX_API_KEY").ok(),
            library_scope: "org:math/problem:euler1006/kind:library".into(),
            session_scope: "org:math/problem:euler1006-hive/kind:session".into(),
            opencode_config: std::env::var("OPENCODE_CONFIG_CONTENT").ok(),
            router_base: std::env::var("LADDER_BASE")
                .unwrap_or_else(|_| "http://127.0.0.1:6969".into()),
            router_key: std::env::var("LADDER_API_KEY").unwrap_or_default(),
            router_model: "deepseek-flash".into(),
            mode: Mode::Desk,
            outbox: None,
            turn: None,
            fold_account: true,
            fold_after: DigestPolicy::DEFAULT.fold_after,
            // Low enough that a desk of the shape run 28 had would fold at
            // least once, which no run has yet done.
            fold_tokens: 50_000,
        };
        let mut args = std::env::args().skip(1);
        while let Some(flag) = args.next() {
            let mut value = || args.next().ok_or_else(|| format!("{flag} needs a value"));
            match flag.as_str() {
                "--desk" => options.desk = PathBuf::from(value()?),
                "--task" => options.task = PathBuf::from(value()?),
                "--workspace" => options.workspace = PathBuf::from(value()?),
                "--transcript" => options.transcript = PathBuf::from(value()?),
                "--agent-cmd" => options.agent_cmd = value()?,
                "--rounds" => options.rounds = value()?.parse()?,
                "--max-turns" => options.max_turns = value()?.parse()?,
                "--max-hops" => options.max_hops = value()?.parse()?,
                "--chair-every" => options.chair_every = value()?.parse()?,
                "--resume-sessions" => options.resume_sessions = true,
                "--window" => options.window = value()?.parse()?,
                "--timeout" => options.timeout = Duration::from_secs(value()?.parse()?),
                "--cortex-base" => options.cortex_base = Some(value()?),
                "--library-scope" => options.library_scope = value()?,
                "--session-scope" => options.session_scope = value()?,
                "--router-base" => options.router_base = value()?,
                "--router-model" => options.router_model = value()?,
                "--mcp-server" => options.mode = Mode::ServeTools,
                "--outbox" => options.outbox = Some(PathBuf::from(value()?)),
                "--turn" => options.turn = Some(PathBuf::from(value()?)),
                "--no-digest" => options.fold_account = false,
                "--fold-after" => options.fold_after = value()?.parse()?,
                "--fold-tokens" => options.fold_tokens = value()?.parse()?,
                "--tool-surface" => options.mode = Mode::PrintSurface,
                "--no-memory" => {
                    options.cortex_base = None;
                }
                other => return Err(format!("unknown flag {other}").into()),
            }
        }
        if options.mode == Mode::PrintSurface {
            // Printing the surface needs neither a desk nor a task: it is the
            // library's four tools, rendered.
            return Ok(options);
        }
        if options.mode == Mode::ServeTools {
            // Serving the room as a tool needs a transcript and an outbox.
            // A desk file and a turn file are optional and buy one thing: a
            // `desk_dm` the policy will refuse can be said so to its author.
            return Ok(options);
        }
        if options.desk.as_os_str().is_empty() {
            return Err("--desk is required".into());
        }
        if options.task.as_os_str().is_empty() {
            return Err("--task is required".into());
        }
        Ok(options)
    }
}
