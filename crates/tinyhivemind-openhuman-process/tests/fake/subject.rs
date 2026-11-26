//! The fake `openhuman-core` as a subject of the host's conformance suite.
//!
//! The runner launches a wrapper script that sets `CONTROL` and execs the
//! fake child, so the runner itself is exactly the one a host uses.
use crate::seat_support::{done, kill_all, processes_in, seat};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tinyhivemind_host::harness::conformance::{
    CallState, EscapeFuture, ModelCall, ModelStep, Subject, ToolFault,
};
use tinyhivemind_host::harness::{Harness, SeatPlan, ToolEndpoint};
use tinyhivemind_host::process::StopPolicy;
use tinyhivemind_openhuman_process::{OpenHumanHarness, Options, Program};

/// Fresh durable state and the scripted fake.
pub(crate) struct FakeSubject {
    root: tempfile::TempDir,
    steps: AtomicUsize,
}

pub(crate) const TOOLS: [&str; 2] = ["hivemind_post", "hivemind_ask"];

impl FakeSubject {
    pub(crate) fn new() -> std::io::Result<Self> {
        let root = tempfile::tempdir()?;
        let control = root.path().join("control");
        fs::create_dir_all(control.join("steps"))?;
        fs::write(control.join("served"), json!(TOOLS).to_string())?;
        let program = env!("CARGO_BIN_EXE_tinyhivemind-fake-openhuman");
        let wrapper = root.path().join("openhuman-core");
        let script = format!(
            "#!/bin/sh\nCONTROL='{}' exec '{program}' \"$@\"\n",
            control.display()
        );
        fs::write(&wrapper, script)?;
        fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o755))?;
        Ok(Self {
            root,
            steps: AtomicUsize::new(0),
        })
    }

    pub(crate) fn control(&self) -> PathBuf {
        self.root.path().join("control")
    }

    pub(crate) fn root_path(&self) -> &Path {
        self.root.path()
    }

    fn instance(&self) -> PathBuf {
        self.root.path().join("instance")
    }

    fn entries(&self) -> Vec<Value> {
        fs::read_to_string(self.control().join("calls.jsonl"))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }

    pub(crate) fn options() -> Options {
        Options {
            ready: Duration::from_secs(20),
            warm_up: Duration::from_secs(3),
            stop: StopPolicy {
                cancel: Duration::from_secs(2),
                terminate: Duration::from_secs(2),
                kill: Duration::from_secs(10),
            },
        }
    }
}

impl FakeSubject {
    /// The harness itself, rather than behind the port.
    pub(crate) fn openhuman(&self) -> OpenHumanHarness {
        let program = Program::new(self.root.path().join("openhuman-core"), "0.64.15");
        OpenHumanHarness::new(program).with_options(Self::options())
    }
}

impl Subject for FakeSubject {
    fn harness(&self) -> Arc<dyn Harness> {
        Arc::new(self.openhuman())
    }

    fn seat(&self) -> SeatPlan {
        SeatPlan::new(seat("http://127.0.0.1:9/v1"), self.instance())
    }

    fn endpoint(&self) -> ToolEndpoint {
        let tools = TOOLS.map(String::from);
        ToolEndpoint::new("hivemind", "http://127.0.0.1:1/mcp", "seat-bearer", tools)
    }

    fn script(&self, steps: &[ModelStep]) {
        let directory = self.control().join("steps");
        for old in fs::read_dir(&directory).into_iter().flatten().flatten() {
            let _ = fs::remove_file(old.path());
        }
        for step in steps {
            let number = self.steps.fetch_add(1, Ordering::Relaxed);
            let step = match step {
                ModelStep::Reply { text, usage } => {
                    json!({"reply": text, "tokens": usage.input_tokens})
                }
                ModelStep::Hang => Value::Null,
            };
            let path = directory.join(format!("{number:06}"));
            done(&fs::write(path, step.to_string()));
        }
    }

    fn model_calls(&self) -> Vec<ModelCall> {
        let mut calls = BTreeMap::new();
        for entry in self.entries() {
            let state = match entry["state"].as_str() {
                Some("answered") => CallState::Answered,
                Some("aborted") => CallState::Aborted,
                // A model call ends with the process that makes it.
                _ if !Path::new(&format!("/proc/{}/cwd", entry["pid"])).exists() => {
                    CallState::Aborted
                }
                _ => CallState::Open,
            };
            let seen = entry["seen"].as_u64().unwrap_or_default();
            let call = calls.entry(entry["call"].as_u64()).or_insert(ModelCall {
                seen: usize::try_from(seen).unwrap_or(usize::MAX),
                state,
            });
            call.state = state;
        }
        calls.into_values().collect()
    }

    fn lose_sessions(&self) {
        let transcripts = self.instance().join("state/ws/workspace/session_raw");
        done(&fs::remove_dir_all(transcripts));
    }

    fn offer(&self, fault: ToolFault) {
        let text = match fault {
            ToolFault::None => "",
            ToolFault::Extra => "extra",
            ToolFault::Missing => "missing",
        };
        done(&fs::write(self.control().join("tools"), text));
    }

    fn misconfigure(&self) {
        done(&fs::write(self.control().join("misconfigured"), ""));
    }

    fn escapee(&self) -> EscapeFuture {
        let control = self.control();
        Box::pin(async move {
            let report = control.join("escapee");
            let _ = fs::remove_file(&report);
            done(&fs::write(control.join("escape"), ""));
            loop {
                if let Ok(pid) = fs::read_to_string(&report) {
                    return pid.parse().ok();
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
    }

    fn processes(&self) -> Vec<u32> {
        processes_in(&self.instance())
    }

    fn turn_limit(&self) -> Duration {
        Duration::from_secs(5)
    }

    fn patience(&self) -> Duration {
        Duration::from_secs(60)
    }
}

impl Drop for FakeSubject {
    fn drop(&mut self) {
        kill_all(self.processes());
    }
}
