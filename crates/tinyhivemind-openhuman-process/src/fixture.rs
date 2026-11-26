//! Unit-test fixture: the in-process fake surface, a seat config written for
//! it, and a client.
use crate::capability::SeatModel;
use crate::config::{Input, Written};
use crate::fake::{Fake, Setup, control, hashed};
use crate::rpc::Client;
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;

pub(crate) const TOKEN: &str = "fake-core-token";
pub(crate) const PID: u32 = 4242;
pub(crate) const SERVER: &str = "hivemind";

/// A fake instance with a written config.
pub(crate) struct Fixture {
    pub(crate) _root: tempfile::TempDir,
    pub(crate) control: PathBuf,
    pub(crate) written: Written,
    pub(crate) client: Client,
    pub(crate) fake: Fake,
}

pub(crate) fn served() -> Vec<String> {
    vec!["hivemind_post".into(), "hivemind_ask".into()]
}

pub(crate) fn hashed_tools() -> Vec<String> {
    served().iter().map(|tool| hashed(SERVER, tool)).collect()
}

pub(crate) fn model() -> SeatModel {
    SeatModel {
        provider: "local".into(),
        endpoint: "http://127.0.0.1:9/v1".into(),
        model: "mock".into(),
        iterations: 8,
    }
}

impl Fixture {
    /// A fake whose control directory already holds `files`.
    pub(crate) async fn with(files: &[(&str, &str)]) -> Self {
        let root = tempfile::tempdir().unwrap_or_else(|error| unreachable!("{error}"));
        let control = control(root.path()).unwrap_or_else(|error| unreachable!("{error}"));
        let workspace = root.path().join("ws");
        let tools = hashed_tools();
        let model = model();
        let written = Written::new(&Input {
            model: &model,
            seat: "scout",
            prompt: "You scout.",
            server: SERVER,
            url: "http://127.0.0.1:1/mcp",
            bearer: "bearer",
            tools: &tools,
            turn_secs: 600,
        });
        let wrote = fs::create_dir_all(&workspace)
            .and_then(|()| fs::write(workspace.join("config.toml"), written.toml()))
            .and_then(|()| fs::write(control.join("served"), json!(served()).to_string()));
        for (name, text) in files {
            let _ = fs::write(control.join(name), text);
        }
        assert!(wrote.is_ok(), "{wrote:?}");
        let setup = Setup {
            token: TOKEN.into(),
            workspace,
            control: control.clone(),
            pid: PID,
        };
        let fake = Fake::start(setup).await;
        let fake = fake.unwrap_or_else(|error| unreachable!("the fake starts: {error}"));
        let client = Client::new(fake.port, TOKEN);
        let client = client.unwrap_or_else(|error| unreachable!("a client: {error}"));
        Self {
            _root: root,
            control,
            written,
            client,
            fake,
        }
    }

    /// The config every fixture writes, without a fake.
    pub(crate) fn written_for_test() -> Written {
        let tools = hashed_tools();
        Written::new(&Input {
            model: &model(),
            seat: "scout",
            prompt: "You scout.",
            server: SERVER,
            url: "http://127.0.0.1:1/mcp",
            bearer: "bearer",
            tools: &tools,
            turn_secs: 600,
        })
    }

    pub(crate) async fn new() -> Self {
        Self::with(&[]).await
    }

    /// Have the model play `steps` from its next call on.
    pub(crate) fn script(&self, steps: &[Value]) {
        for (index, step) in steps.iter().enumerate() {
            let path = self.control.join("steps").join(format!("{index:04}"));
            let _ = fs::write(path, step.to_string());
        }
    }

    /// Each line of a control file the fake writes.
    pub(crate) fn log(&self, name: &str) -> Vec<Value> {
        let text = fs::read_to_string(self.control.join(name)).unwrap_or_default();
        text.lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }

    /// The states of the model calls, in order.
    pub(crate) fn calls(&self) -> Vec<String> {
        let mut states: Vec<String> = Vec::new();
        for entry in self.log("calls.jsonl") {
            let index = usize::try_from(entry["call"].as_u64().unwrap_or(0)).unwrap_or(0);
            let state = entry["state"].as_str().unwrap_or_default().to_owned();
            if index < states.len() {
                states[index] = state;
            } else {
                states.push(state);
            }
        }
        states
    }
}
