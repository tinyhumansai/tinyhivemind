//! Shared fixtures: hooks over the fixture package and a runner around them.
use crate::{EnvSecrets, ExecContext, HookRunner};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tinyhivemind_lang::LoweredHook;

/// A hook `id` on `event` running `tools/<script>.sh`, with extra hook
/// fields from `hook`.
pub(super) fn hook(id: &str, event: &str, script: &str, hook: Value) -> LoweredHook {
    let mut declaration = json!({"id": id, "event": event, "run": script});
    if let (Some(base), Value::Object(fields)) = (declaration.as_object_mut(), hook) {
        base.extend(fields);
    }
    LoweredHook {
        hook: serde_json::from_value(declaration).expect("a valid hook"),
        tool: serde_json::from_value(json!({
            "id": script,
            "description": "hook",
            "command": [format!("tools/{script}.sh")],
            "argv": [id],
        }))
        .expect("a valid tool"),
    }
}

/// A runner for seat `critic` of hive `acme`, whose workspace is `workspace`.
pub(super) fn runner(hooks: Vec<LoweredHook>, workspace: &Path) -> HookRunner {
    let context = ExecContext {
        package_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/hook/test/fixtures"),
        workspace: Some(workspace.to_path_buf()),
        hive: "acme".into(),
        seat: "critic".into(),
        ..ExecContext::default()
    };
    HookRunner::new(hooks, Arc::new(context), Arc::new(EnvSecrets))
}

/// Every event object `record.sh` appended in `workspace`, in order.
pub(super) fn recorded(workspace: &Path) -> Vec<Value> {
    std::fs::read_to_string(workspace.join("events.log"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).expect("an event object"))
        .collect()
}
