//! What both conformance subjects share: the seat, and the processes of an
//! instance directory.
use rustix::process::{Pid, Signal, kill_process};
use serde_json::Value;
use std::fmt::Debug;
use std::fs;
use std::path::Path;
use tinyhivemind_lang::{
    LoweredModel, LoweredSeat, LoweredSeatIntegration, Model, Provider, ProviderKind,
};

/// The result of something the test itself does, which has to work.
pub(crate) fn done<T: Debug, E: Debug>(result: &Result<T, E>) {
    assert!(result.is_ok(), "{result:?}");
}

/// A seat on an `openai` endpoint at `endpoint`.
pub(crate) fn seat(endpoint: &str) -> LoweredSeat {
    let provider = Provider {
        id: "local".into(),
        kind: ProviderKind::Openai,
        endpoint: Some(endpoint.into()),
        credential: None,
    };
    let model = Model {
        id: "fast".into(),
        provider: Some("local".into()),
        name: "mock".into(),
        ..Model::default()
    };
    LoweredSeat {
        id: "scout".into(),
        label: "Scout".into(),
        template: "seat".into(),
        config: Value::Null,
        prompt: "You scout.".into(),
        memory: None,
        integration: LoweredSeatIntegration {
            model: Some(LoweredModel {
                model,
                provider: Some(provider),
            }),
            ..LoweredSeatIntegration::default()
        },
    }
}

/// Whatever runs in `instance`.
pub(crate) fn processes_in(instance: &Path) -> Vec<u32> {
    fs::read_dir("/proc")
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| entry.file_name().to_str()?.parse().ok())
        .filter(|pid: &u32| {
            fs::read_link(format!("/proc/{pid}/cwd")).is_ok_and(|cwd| cwd.starts_with(instance))
        })
        .collect()
}

/// Kill what a faulty run left behind.
pub(crate) fn kill_all(pids: Vec<u32>) {
    for pid in pids {
        if let Some(pid) = Pid::from_raw(pid.cast_signed()) {
            let _ = kill_process(pid, Signal::KILL);
        }
    }
}

