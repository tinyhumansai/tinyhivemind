//! Running one seat's turn as one child process.
//!
//! One message, one turn: the library authorizes exactly one speaker per step,
//! and this module is what a speaker costs — a single `opencode run` in the
//! desk's shared workspace, so the seat can read files, write code, and run it.
//! Nothing here decides *who* speaks.

use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    sync::{Arc, Mutex, PoisonError},
    time::{Duration, Instant},
};

/// How long a turn may produce nothing at all before it is treated as stalled.
///
/// Set high on purpose, and the reason is a mistake worth recording. A
/// reasoning model emits no events at all while it is thinking: the CLI writes
/// a step when a call starts and when it finishes, and nothing in between. So
/// stdout silence does not distinguish a hung stream from a seat mid-thought.
/// At 120s this killed healthy turns three times over while the router was
/// quietly serving twenty calls for them — the detector became the outage it
/// was written to catch.
///
/// What it is still for is the genuine hang, where the CLI waits forever on a
/// stream that opened and stopped and nothing upstream ends it. Those ran the
/// whole turn budget with *zero* router calls.
///
/// The threshold has to clear the router's own worst case, which is the thing
/// that caught this out twice. With `request_timeout` at 300s and two rungs to
/// walk, one call the caller is still waiting on can legitimately be silent for
/// 600s before any answer arrives. Fifteen minutes sits above that and below
/// the turn deadline. Change the router's timeout and this has to move with
/// it — they are one budget, not two.
///
/// Run 27 tried widening both ends and it was the wrong read. The call that
/// exhausted the ladder carried 48k of context and 374 output tokens, while
/// probes answered in 3s throughout; it was a dead call, not a slow one, and
/// doubling the cap only doubled what its death cost the turn.
const STALL_AFTER: Duration = Duration::from_secs(900);

/// What one turn produced.
#[derive(Clone, Debug, Default)]
pub(crate) struct TurnOutput {
    /// The text the seat wants posted to the room.
    pub(crate) message: String,
    /// Total tokens the provider reported across the turn's steps.
    pub(crate) tokens: u64,
    /// Wall-clock cost of the process.
    pub(crate) elapsed: Duration,
    /// Names of the tools the seat actually invoked, in order.
    pub(crate) tools: Vec<String>,
    /// Whether the process was killed at its deadline rather than finishing.
    pub(crate) timed_out: bool,
    /// Whether it was killed for going quiet long before its deadline.
    pub(crate) stalled: bool,
    /// The agent CLI's own session id, so this seat can resume its own work.
    pub(crate) session: Option<String>,
    /// What the seat actually ran and saw, so a wrap-up summarizes evidence
    /// rather than inventing it.
    pub(crate) work_log: String,
    /// A provider or CLI failure reported inside the event stream.
    ///
    /// A 402 from a fallback provider arrives here, not as a nonzero exit: the
    /// process runs its full budget and prints one `error` event, so a turn
    /// that lost its credit is indistinguishable from a model that would not
    /// speak unless the host reads this.
    pub(crate) error: Option<String>,
    /// Whether the seat actually marked a message for the room.
    ///
    /// Unmarked trailing text is narration — "Now I have the full picture, let
    /// me write the solver" — and posting it wastes a turn and tells the room
    /// nothing. A turn that did not mark a post has not spoken.
    pub(crate) posted: bool,
    /// Paths the seat wrote or edited, as the CLI reported them, first write
    /// first, without repeats.
    ///
    /// This is the room's feedthrough: a file changing in the shared workspace
    /// is the one thing a seat does that every peer can observe without being
    /// told, and in run 26 the only reason a result survived a broken post was
    /// that the same turn had also written a file. The host turns this into one
    /// row so that survival is no longer luck.
    pub(crate) files_written: Vec<String>,
    /// How many `read` tool calls the turn made.
    ///
    /// The re-reading cost of a stateless seat, measured: turn 1 of the run-27
    /// solver spent 15 of its 19 calls here. Printed per turn so the notebook's
    /// effect on it is a number rather than an impression.
    pub(crate) reads: usize,
}

/// A configured agent CLI: one process per turn.
pub(crate) struct AgentRunner {
    program: String,
    args: Vec<String>,
    workspace: String,
    config: Option<String>,
    timeout: Duration,
    raw_dir: Option<PathBuf>,
}

impl AgentRunner {
    /// Build a runner from a command line whose final argument is the prompt.
    pub(crate) fn new(
        command: &str,
        workspace: &str,
        config: Option<String>,
        timeout: Duration,
        raw_dir: Option<PathBuf>,
    ) -> Self {
        let mut parts = command.split_whitespace().map(str::to_string);
        let program = parts.next().unwrap_or_else(|| "opencode".to_string());
        Self {
            program,
            args: parts.collect(),
            workspace: workspace.to_string(),
            config,
            timeout,
            raw_dir,
        }
    }

    /// Run one turn.
    ///
    /// # Errors
    ///
    /// Returns a spawn or wait failure. A model that answers nothing is not an
    /// error here; it is an empty message the caller decides what to do with.
    pub(crate) fn run(
        &self,
        prompt: &str,
        label: &str,
        timeout: Duration,
        session: Option<&str>,
    ) -> Result<TurnOutput, Box<dyn std::error::Error + Send + Sync>> {
        let started = Instant::now();
        let mut command = Command::new(&self.program);
        command.args(&self.args);
        // A seat that resumes its own CLI session keeps the working context it
        // built last turn — its scratch reasoning, its file reads — which is
        // what a persistent agent has and a fresh process does not.
        if let Some(session) = session {
            command.args(["--session", session]);
        }
        command
            .arg(prompt)
            .current_dir(&self.workspace)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(config) = &self.config {
            command.env("OPENCODE_CONFIG_CONTENT", config);
        }
        let mut child = command.spawn()?;
        // Keep stderr. A turn whose event stream is zero bytes said nothing and
        // explained nothing; whatever the CLI complained about went here, and
        // discarding it is how a crash reads as a quiet model.
        let stderr = child.stderr.take();
        let errors = std::thread::spawn(move || {
            use std::io::Read as _;
            let mut buffer = String::new();
            if let Some(mut stderr) = stderr {
                let _ = stderr.read_to_string(&mut buffer);
            }
            buffer
        });
        let (output, timed_out, stalled) = wait_with_timeout(child, timeout)?;
        let raw = String::from_utf8_lossy(&output);
        let complaints = errors.join().unwrap_or_default();
        // Keep the whole event stream. A turn that produced nothing postable is
        // exactly the turn whose transcript someone will want to read.
        if let Some(dir) = &self.raw_dir {
            let _ = fs::create_dir_all(dir);
            if let Ok(mut file) = fs::File::create(dir.join(format!("{label}.jsonl"))) {
                let _ = file.write_all(raw.as_bytes());
            }
            if let Ok(mut file) = fs::File::create(dir.join(format!("{label}.prompt.txt"))) {
                let _ = file.write_all(prompt.as_bytes());
            }
            if !complaints.trim().is_empty()
                && let Ok(mut file) = fs::File::create(dir.join(format!("{label}.stderr.txt")))
            {
                let _ = file.write_all(complaints.as_bytes());
            }
        }
        let mut turn = parse_events(&raw);
        turn.elapsed = started.elapsed();
        turn.timed_out = timed_out;
        turn.stalled = stalled;
        if turn.error.is_none() && raw.trim().is_empty() && !complaints.trim().is_empty() {
            turn.error = Some(complaints.trim().lines().last().unwrap_or("").to_string());
        }
        Ok(turn)
    }

    /// The runner's configured per-turn deadline.
    pub(crate) const fn timeout(&self) -> Duration {
        self.timeout
    }
}

fn wait_with_timeout(
    mut child: std::process::Child,
    timeout: Duration,
) -> Result<(Vec<u8>, bool, bool), Box<dyn std::error::Error + Send + Sync>> {
    let stdout = child.stdout.take().ok_or("child has no stdout")?;
    // The buffer and the time it last grew, shared with the reader: a turn is
    // alive because it is still saying something, not because it is still
    // running.
    let seen = Arc::new(Mutex::new((Vec::new(), Instant::now())));
    let writer = Arc::clone(&seen);
    let reader = std::thread::spawn(move || {
        use std::io::Read;
        let mut stdout = stdout;
        let mut chunk = [0_u8; 8192];
        while let Ok(read) = stdout.read(&mut chunk) {
            if read == 0 {
                break;
            }
            let mut held = writer.lock().unwrap_or_else(PoisonError::into_inner);
            held.0.extend_from_slice(&chunk[..read]);
            held.1 = Instant::now();
        }
    });
    let deadline = Instant::now() + timeout;
    let mut timed_out = false;
    let mut stalled = false;
    loop {
        if child.try_wait()?.is_some() {
            break;
        }
        let quiet = seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .1
            .elapsed();
        if Instant::now() >= deadline {
            timed_out = true;
            let _ = child.kill();
            break;
        }
        if quiet >= STALL_AFTER {
            stalled = true;
            let _ = child.kill();
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    let _ = reader.join();
    let buffer = seen
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .0
        .clone();
    Ok((buffer, timed_out, stalled))
}

/// Fold one `--format json` event stream into a turn.
///
/// Text parts are concatenated in arrival order; a `<<<POST ... POST>>>` block
/// wins over everything around it, because a model narrating its own reasoning
/// into the shared transcript is noise every other seat then has to read.
fn parse_events(stdout: &str) -> TurnOutput {
    let mut text = String::new();
    let mut turn = TurnOutput::default();
    for line in stdout.lines() {
        let Ok(event) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if turn.session.is_none()
            && let Some(id) = event.get("sessionID").and_then(serde_json::Value::as_str)
        {
            turn.session = Some(id.to_string());
        }
        match event.get("type").and_then(serde_json::Value::as_str) {
            Some("text") => {
                if let Some(part) = event
                    .pointer("/part/text")
                    .and_then(serde_json::Value::as_str)
                {
                    if !text.is_empty() {
                        text.push('\n');
                    }
                    text.push_str(part);
                }
            }
            Some("tool" | "tool_use") => {
                let tool = event
                    .pointer("/part/tool")
                    .and_then(serde_json::Value::as_str);
                if let Some(name) = tool {
                    turn.tools.push(name.to_string());
                }
                let path = event
                    .pointer("/part/state/input/filePath")
                    .and_then(serde_json::Value::as_str);
                match (tool, path) {
                    (Some("read"), _) => turn.reads += 1,
                    (Some("write" | "edit"), Some(path))
                        if !turn.files_written.iter().any(|seen| seen == path) =>
                    {
                        turn.files_written.push(path.to_string());
                    }
                    _ => {}
                }
                let input = event
                    .pointer("/part/state/input")
                    .map(serde_json::Value::to_string)
                    .unwrap_or_default();
                let result = event
                    .pointer("/part/state/output")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                turn.work_log.push_str("\n$ ");
                turn.work_log.push_str(&truncate(&input, 1200));
                turn.work_log.push('\n');
                turn.work_log.push_str(&truncate(result, 1200));
                turn.work_log.push('\n');
            }
            Some("error") => {
                turn.error = Some(
                    event
                        .pointer("/error/data/message")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("unknown agent error")
                        .to_string(),
                );
            }
            Some("step_finish") => {
                if let Some(total) = event
                    .pointer("/part/tokens/total")
                    .and_then(serde_json::Value::as_u64)
                {
                    turn.tokens = turn.tokens.max(total);
                }
            }
            _ => {}
        }
    }
    turn.posted = text.contains("<<<POST");
    turn.message = extract_post(&text);
    turn
}

/// Keep the head of a long string, marking what was dropped.
fn truncate(text: &str, limit: usize) -> String {
    let mut end = limit.min(text.len());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    if end == text.len() {
        return text.to_string();
    }
    format!("{}… [{} bytes elided]", &text[..end], text.len() - end)
}

/// Pull the room message out of a turn's raw text.
///
/// Two fences are accepted: the asymmetric `<<<POST … POST>>>` the brief asks
/// for, and the symmetric `<<<POST>>> … <<<POST>>>` a seat writes anyway.
/// Taking only the first cost run 26 its whole turn 3 — a verified sublinear
/// recursion reached the transcript as the three characters `>>>`, the room
/// read that as silence, and the chair nudged a seat that had in fact spoken.
pub(crate) fn extract_post(text: &str) -> String {
    let opens: Vec<usize> = text.match_indices("<<<POST").map(|(at, _)| at).collect();
    let Some(&last) = opens.last() else {
        return text.trim().to_string();
    };
    let after = &text[last + "<<<POST".len()..];
    if let Some(end) = after.find("POST>>>") {
        return after[..end].trim().to_string();
    }
    // No terminator after the last marker: it is a symmetric fence, so the
    // marker found is the closer and the one before it opened the block.
    if let Some(&open) = opens.iter().rev().nth(1) {
        let body = &text[open + "<<<POST".len()..last];
        return body.strip_prefix(">>>").unwrap_or(body).trim().to_string();
    }
    after
        .strip_prefix(">>>")
        .unwrap_or(after)
        .trim()
        .to_string()
}

/// Trim a turn's work log to the tail a wrap-up can actually read.
pub(crate) fn truncate_work_log(log: &str) -> String {
    const LIMIT: usize = 12_000;
    if log.len() <= LIMIT {
        return log.to_string();
    }
    let mut start = log.len() - LIMIT;
    while start < log.len() && !log.is_char_boundary(start) {
        start += 1;
    }
    format!("[earlier steps elided]\n{}", &log[start..])
}

#[cfg(test)]
mod test;
