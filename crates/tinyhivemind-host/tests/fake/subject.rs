//! The fake harness as a subject of the conformance suite.
use super::client::{FakeHarness, Fault};
use rustix::process::{Pid, Signal, kill_process};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;
use tinyhivemind_host::harness::conformance::{
    CallState, EscapeFuture, ModelCall, ModelStep, Subject, ToolFault,
};
use tinyhivemind_host::harness::{Harness, SeatPlan, ToolEndpoint};
use tinyhivemind_lang::{LoweredSeat, LoweredSeatIntegration};

/// The result of something the test itself does, which has to work.
fn done<T: Debug, E: Debug>(result: &Result<T, E>) {
    assert!(result.is_ok(), "{result:?}");
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Fresh durable state and a scripted model.
pub(crate) struct FakeSubject {
    root: tempfile::TempDir,
    fault: Fault,
    steps: AtomicUsize,
    current: Mutex<Option<Arc<FakeHarness>>>,
}

impl FakeSubject {
    pub(crate) fn new(fault: Fault) -> std::io::Result<Self> {
        let root = tempfile::tempdir()?;
        fs::create_dir_all(root.path().join("control/steps"))?;
        Ok(Self {
            root,
            fault,
            steps: AtomicUsize::new(0),
            current: Mutex::default(),
        })
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
        *lock(&self.current) = Some(harness.clone());
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

    fn script(&self, steps: &[ModelStep]) {
        let directory = self.control().join("steps");
        for old in fs::read_dir(&directory).into_iter().flatten().flatten() {
            // The child may have taken it meanwhile.
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
            let path = directory.join(format!("{number:04}"));
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
        let state = self.root.path().join("instance/state");
        done(&fs::remove_file(state.join("session.json")));
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

    /// Whatever runs in this subject's instance directory.
    fn processes(&self) -> Vec<u32> {
        let instance = self.root.path().join("instance");
        fs::read_dir("/proc")
            .into_iter()
            .flatten()
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
            _ => Duration::from_secs(8),
        }
    }

    fn escapee(&self) -> EscapeFuture {
        let harness = lock(&self.current).clone();
        let report = self.control().join("escapee");
        Box::pin(async move {
            done(&harness?.escape().await);
            // The suite bounds the wait.
            loop {
                if let Ok(pid) = fs::read_to_string(&report) {
                    return pid.parse().ok();
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
    }
}

/// A faulty harness leaves processes behind.
impl Drop for FakeSubject {
    fn drop(&mut self) {
        for pid in self.processes() {
            if let Some(pid) = Pid::from_raw(pid.cast_signed()) {
                let _ = kill_process(pid, Signal::KILL);
            }
        }
    }
}
