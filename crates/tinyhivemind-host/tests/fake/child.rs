//! The scripted fake harness: a real process that speaks JSON lines and keeps
//! one session in its working directory.
//!
//! `CONTROL` names a directory shared with the test. The child takes the model
//! steps it plays from `steps/`, in name order, and appends every model call
//! and its end to `calls.jsonl`. `FAULT` names one thing to do wrong.
use serde_json::{Value, json};
use std::fs;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

const VERSION: &str = "1.0.0";

fn main() -> std::io::Result<()> {
    let mut arguments = std::env::args().skip(1);
    if let Some(report) = arguments.next() {
        return escapee(Path::new(&report));
    }
    let mut child = Child::new();
    for line in std::io::stdin().lock().lines() {
        let request: Value = serde_json::from_str(&line?)?;
        child.handle(&request)?;
    }
    Ok(())
}

/// Leave the process group, report, and stay.
fn escapee(report: &Path) -> std::io::Result<()> {
    rustix::process::setsid()?;
    write_whole(report, &std::process::id().to_string())?;
    loop {
        std::thread::park();
    }
}

/// So that a reader never sees half of it.
fn write_whole(path: &Path, text: &str) -> std::io::Result<()> {
    let partial = path.with_extension("partial");
    fs::write(&partial, text)?;
    fs::rename(partial, path)
}

struct Child {
    control: PathBuf,
    fault: String,
    /// The turn whose model call is open, and the call's number.
    running: Option<(String, u64)>,
    total: u64,
}

impl Child {
    fn new() -> Self {
        Self {
            control: std::env::var_os("CONTROL").unwrap_or_default().into(),
            fault: std::env::var("FAULT").unwrap_or_default(),
            running: None,
            total: 0,
        }
    }

    fn say(value: &Value) -> std::io::Result<()> {
        let mut out = std::io::stdout().lock();
        writeln!(out, "{value}")?;
        out.flush()
    }

    fn handle(&mut self, request: &Value) -> std::io::Result<()> {
        match request["op"].as_str().unwrap_or_default() {
            "hello" => self.hello(&request["tools"]),
            "turn" => self.turn(request),
            "cancel" => self.cancel(),
            "escape" => {
                std::process::Command::new(std::env::current_exe()?)
                    .arg(self.control.join("escapee"))
                    .spawn()?;
                Ok(())
            }
            _ => Self::say(&json!({"event": "error", "reason": "unknown op"})),
        }
    }

    fn hello(&self, tools: &Value) -> std::io::Result<()> {
        let mut tools = tools.as_array().cloned().unwrap_or_default();
        match fs::read_to_string(self.control.join("tools")).as_deref() {
            Ok("extra") => tools.push(json!("shell")),
            Ok("missing") => drop(tools.pop()),
            _ => {}
        }
        Self::say(&json!({
            "event": "hello", "pid": std::process::id(), "version": VERSION, "tools": tools,
        }))
    }

    /// The session's id and the user messages in it.
    fn session() -> Option<(String, u64)> {
        let text = fs::read_to_string("session.json").ok()?;
        let session: Value = serde_json::from_str(&text).ok()?;
        Some((
            session["id"].as_str()?.to_owned(),
            session["turns"].as_u64()?,
        ))
    }

    fn turn(&mut self, request: &Value) -> std::io::Result<()> {
        let known = Self::session();
        let (id, turns) = match (request["session"].as_str(), known) {
            (Some(wanted), Some((id, turns))) if wanted == id => (id, turns),
            (Some(_), _) if self.fault == "new_session" => (Self::new_session()?, 0),
            (Some(wanted), _) => {
                let reason = format!("no session {wanted}");
                return Self::say(&json!({"event": "no_session", "reason": reason}));
            }
            (None, _) => (Self::new_session()?, 0),
        };
        let turns = if self.fault == "forgetful" {
            1
        } else {
            turns + 1
        };
        write_whole(
            Path::new("session.json"),
            &json!({"id": id, "turns": turns}).to_string(),
        )?;
        let turn = request["turn"].as_str().unwrap_or_default();
        let call = self.open(turn, turns)?;
        let step = self.step()?;
        if step.is_null() {
            self.running = Some((turn.to_owned(), call));
            return Ok(());
        }
        self.close(call, "answered")?;
        // Like Codex, the child reports a running total.
        self.total += step["tokens"].as_u64().unwrap_or(0);
        Self::say(&json!({
            "event": "done", "session": id, "reply": step["reply"], "total": self.total,
        }))
    }

    fn new_session() -> std::io::Result<String> {
        let mut number = 0_u64;
        loop {
            number += 1;
            let marker = format!("session-{number}");
            match fs::File::create_new(&marker) {
                Ok(_) => return Ok(marker),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
        }
    }

    /// The next scripted step; null hangs.
    fn step(&self) -> std::io::Result<Value> {
        let mut steps: Vec<PathBuf> = fs::read_dir(self.control.join("steps"))?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<std::io::Result<_>>()?;
        steps.sort();
        let Some(next) = steps.first() else {
            return Ok(Value::Null);
        };
        let step = serde_json::from_str(&fs::read_to_string(next)?)?;
        fs::remove_file(next)?;
        Ok(step)
    }

    fn cancel(&mut self) -> std::io::Result<()> {
        if self.fault == "ignores_cancel" {
            return Self::say(&json!({"event": "cancelled"}));
        }
        if let Some((_, call)) = self.running.take() {
            self.close(call, "aborted")?;
        }
        Self::say(&json!({"event": "cancelled"}))
    }

    fn open(&self, turn: &str, seen: u64) -> std::io::Result<u64> {
        let count = fs::read_to_string(self.control.join("calls.jsonl"))
            .unwrap_or_default()
            .lines()
            .filter(|line| line.contains("\"open\""))
            .count();
        let call = u64::try_from(count).unwrap_or(u64::MAX);
        self.log(&json!({
            "call": call, "state": "open", "turn": turn, "seen": seen,
            "pid": std::process::id(),
        }))?;
        Ok(call)
    }

    fn close(&self, call: u64, state: &str) -> std::io::Result<()> {
        self.log(&json!({"call": call, "state": state}))
    }

    fn log(&self, entry: &Value) -> std::io::Result<()> {
        let mut log = fs::File::options()
            .create(true)
            .append(true)
            .open(self.control.join("calls.jsonl"))?;
        log.write_all(format!("{entry}\n").as_bytes())
    }
}
