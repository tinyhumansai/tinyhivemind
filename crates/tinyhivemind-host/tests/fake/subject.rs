//! The fake harness as a subject of the conformance suite.
use super::client::{FakeHarness, Fault};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tinyhivemind_host::harness::conformance::{
    CallState, EscapeFuture, ModelCall, ModelStep, Subject, ToolFault,
};
use tinyhivemind_host::harness::{Harness, SeatPlan, ToolEndpoint};
use tinyhivemind_lang::{LoweredSeat, LoweredSeatIntegration};

/// Fresh durable state and a scripted model.
pub struct FakeSubject {
    root: tempfile::TempDir,
    fault: Fault,
    steps: AtomicUsize,
    current: Mutex<Option<Arc<FakeHarness>>>,
}

impl FakeSubject {
    pub fn new(fault: Fault) -> Self {
        let root = tempfile::tempdir()?;
        fs::create_dir_all(root.path().join("control/steps"))?;
        Self {
            root,
            fault,
            steps: AtomicUsize::new(0),
            current: Mutex::default(),
        }
    }
    fn control(&self) -> PathBuf {
        self.root.path().join("control")
    }
    fn entries(&self) -> Vec<Value> {
        fs::read_to_string(self.control().join("calls.jsonl"))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }
}

impl Subject for FakeSubject {
    fn harness(&self) -> Arc<dyn Harness> {
        let program = PathBuf::from(env!("CARGO_BIN_EXE_tinyhivemind-fake-harness"));
        let harness = Arc::new(FakeHarness::new(program, self.control(), self.fault));
        *self.current.lock()? = Some(harness.clone());
        harness
    }

    fn seat(&self) -> SeatPlan {
        let seat = LoweredSeat {
            id: "scout".into(),
            label: "Scout".into(),
            template: "seat".into(),
            config: Value::Null,
            prompt: "You scout.".into(),
            memory: None,
            integration: LoweredSeatIntegration::default(),
        };
        SeatPlan::new(seat, self.root.path().join("instance"))
    }

    fn endpoint(&self) -> ToolEndpoint {
        let tools = ["hivemind_post".to_owned(), "hivemind_ask".to_owned()];
        ToolEndpoint::new("hivemind", "http://127.0.0.1:1/mcp", "bearer", tools)
    }

    fn script(&self, step: ModelStep) {
        let number = self.steps.fetch_add(1, Ordering::Relaxed);
        let step = match step {
            ModelStep::Reply { text, usage } => {
                json!({"reply": text, "tokens": usage.input_tokens})
            }
            ModelStep::Hang => Value::Null,
        };
        let steps = self.control().join("steps");
        fs::write(steps.join(format!("{number:04}")), step.to_string())?;
    }

    fn model_calls(&self) -> Vec<ModelCall> {
        let mut calls = BTreeMap::new();
        for entry in self.entries() {
            let number = entry["call"].as_u64().context("missing")?;
            let state = match entry["state"].as_str().context("missing")? {
                "answered" => CallState::Answered,
                "aborted" => CallState::Aborted,
                // A model call ends with the process that makes it.
                _ if !Path::new(&format!("/proc/{}/cwd", entry["pid"])).exists() => {
                    CallState::Aborted
                }
                _ => CallState::Open,
            };
            let seen = entry["seen"].as_u64();
            let call = calls.entry(number).or_insert(ModelCall {
                seen: usize::try_from(seen.unwrap_or(0))?,
                state,
            });
            call.state = state;
        }
        calls.into_values().collect()
    }

    fn lose_sessions(&self) {
        let state = self.root.path().join("instance/state");
        fs::remove_file(state.join("session.json"))?;
    }

    fn offer(&self, fault: ToolFault) {
        let text = match fault {
            ToolFault::None => "",
            ToolFault::Extra => "extra",
            ToolFault::Missing => "missing",
        };
        fs::write(self.control().join("tools"), text)?;
    }

    /// Whatever runs in this subject's instance directory.
    fn processes(&self) -> Vec<u32> {
        let instance = self.root.path().join("instance");
        fs::read_dir("/proc")?
            .flatten()
            .filter_map(|entry| entry.file_name().to_str()?.parse().ok())
            .filter(|pid: &u32| {
                fs::read_link(format!("/proc/{pid}/cwd"))
                    .is_ok_and(|cwd| cwd.starts_with(&instance))
            })
            .collect()
    }

    fn patience(&self) -> Duration {
        // A faulty harness is meant to be waited for in vain.
        match self.fault {
            Fault::None => Duration::from_secs(60),
            _ => Duration::from_secs(3),
        }
    }

    fn escapee(&self) -> EscapeFuture {
        let harness = self.current.lock()?.clone();
        let report = self.control().join("escapee");
        Box::pin(async move {
            harness?.escape().await;
            for _ in 0..3000 {
                if let Ok(pid) = fs::read_to_string(&report) {
                    return pid.parse().ok();
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            panic!("the escapee never reported");
        })
    }
}

/// A faulty harness leaves processes behind.
impl Drop for FakeSubject {
    fn drop(&mut self) {
        for pid in self.processes() {
            let pid = rustix::process::Pid::from_raw(pid.cast_signed()).context("missing")?;
            let _ = rustix::process::kill_process(pid, rustix::process::Signal::KILL);
        }
    }
}
