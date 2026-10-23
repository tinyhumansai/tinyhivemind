//! An executable tool as a native `tinytools::Tool`.
//!
//! [`ProgramTool`] presents one [`ExecTool`] to a harness: the model sees its
//! `id`, `description` and `args` schema, the permission gate classifies it
//! by `side_effects`, and each call goes through [`crate::run`]. A failing
//! call is reported to the model as an error result it can read and adapt
//! to, never raised to the harness.
use crate::{ExecContext, Secrets, run};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tinyhivemind_lang::{ExecTool, SideEffects};
use tinytools::{PermissionLevel, Tool, ToolPolicy, ToolResult, ToolSideEffects};

/// The note appended to a result whose stdout passed 1 MiB.
const TRUNCATED: &str = "\n[output truncated at 1 MiB]";

/// A `tinytools::Tool` over one executable tool declaration.
#[derive(Debug)]
pub struct ProgramTool {
    tool: ExecTool,
    context: Arc<ExecContext>,
    secrets: Arc<dyn Secrets>,
    calls: AtomicU64,
}

impl ProgramTool {
    /// A tool running `tool` for the seat `context` describes, resolving its
    /// `env` references through `secrets`.
    #[must_use]
    pub fn new(tool: ExecTool, context: Arc<ExecContext>, secrets: Arc<dyn Secrets>) -> Self {
        Self {
            tool,
            context,
            secrets,
            calls: AtomicU64::new(0),
        }
    }

    /// The next call's `THM_CALL_ID`: the tool id and a per-tool sequence.
    fn next_call_id(&self) -> String {
        let call = self.calls.fetch_add(1, Ordering::Relaxed) + 1;
        format!("{}-{call}", self.tool.id)
    }
}

#[async_trait]
impl Tool for ProgramTool {
    fn name(&self) -> &str {
        &self.tool.id
    }

    fn description(&self) -> &str {
        &self.tool.description
    }

    fn parameters_schema(&self) -> Value {
        self.tool.args.clone()
    }

    fn permission_level(&self) -> PermissionLevel {
        match self.tool.side_effects {
            SideEffects::Read => PermissionLevel::ReadOnly,
            SideEffects::Write | SideEffects::Network => PermissionLevel::Write,
        }
    }

    fn policy(&self) -> ToolPolicy {
        match self.tool.side_effects {
            SideEffects::Read => ToolPolicy::classified().with_side_effects(ToolSideEffects {
                read_only: true,
                ..ToolSideEffects::default()
            }),
            SideEffects::Write => ToolPolicy::default(),
            SideEffects::Network => ToolPolicy::classified().with_side_effects(ToolSideEffects {
                network: true,
                ..ToolSideEffects::default()
            }),
        }
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        let call_id = self.next_call_id();
        let outcome = run(
            &self.tool,
            &self.context,
            self.secrets.as_ref(),
            &args,
            &call_id,
        )
        .await;
        Ok(match outcome {
            Ok(output) if output.truncated => ToolResult::success(output.text + TRUNCATED),
            Ok(output) => ToolResult::success(output.text),
            Err(error) => ToolResult::error(error.to_string()),
        })
    }
}

#[cfg(test)]
mod test;
