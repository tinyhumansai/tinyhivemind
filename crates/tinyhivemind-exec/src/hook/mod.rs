//! A seat's hooks: executable tools bound to `pre_tool_use`,
//! `post_tool_use` and `stop`.
//!
//! [`HookRunner`] holds the hooks of one seat and runs those bound to an
//! event, in declaration order, each through [`crate::run()`] with the event
//! object as its argument object:
//!
//! | Event | Argument object | Stdout |
//! | --- | --- | --- |
//! | `pre_tool_use` | `{"event", "tool", "arguments", "seat", "hive"}` | `{"permission": "deny", "message": "…"}` refuses the call |
//! | `post_tool_use` | `{"event", "tool", "arguments", "result", "seat", "hive"}` | ignored |
//! | `stop` | `{"event", "reply", "seat", "hive"}` | ignored |
//!
//! A tool event runs only the hooks whose matcher names the tool. A failing
//! hook is ignored, except that a failing `fail_closed` `pre_tool_use` hook
//! refuses the call. A hook's own `timeout_ms` replaces its tool's.
//!
//! A `stop` hook observes the finished reply and returns nothing: a turn is
//! authorized only by the coordinator's round, so a hook cannot start one
//! (see `docs/specs/executable-tools.md#hooks`).
mod matcher;

use crate::{ExecContext, Result, Secrets, run};
pub(crate) use matcher::matches;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tinyhivemind_lang::{HookEvent, LoweredHook};

/// Whether a tool call may go ahead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Every hook allowed it.
    Allow,
    /// A hook refused it; the message is for the model.
    Deny(String),
}

/// Runs a seat's hooks for one event.
#[derive(Debug)]
pub struct HookRunner {
    hooks: Vec<LoweredHook>,
    context: Arc<ExecContext>,
    secrets: Arc<dyn Secrets>,
    calls: AtomicU64,
}

impl HookRunner {
    /// A runner over `hooks`, in the order they run, for the seat `context`
    /// describes, resolving `env` references through `secrets`.
    #[must_use]
    pub fn new(
        hooks: Vec<LoweredHook>,
        context: Arc<ExecContext>,
        secrets: Arc<dyn Secrets>,
    ) -> Self {
        Self {
            hooks,
            context,
            secrets,
            calls: AtomicU64::new(0),
        }
    }

    /// Whether the seat has no hooks at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.hooks.is_empty()
    }

    /// Ask the `pre_tool_use` hooks whether a call of `tool` with
    /// `arguments` may run. The first refusal wins and later hooks do not run.
    pub async fn pre_tool_use(&self, tool: &str, arguments: &Value) -> Verdict {
        let event = json!({
            "event": "pre_tool_use",
            "tool": tool,
            "arguments": arguments,
            "seat": self.context.seat,
            "hive": self.context.hive,
        });
        for hook in self.bound(HookEvent::PreToolUse, Some(tool)) {
            match self.call(hook, &event).await {
                Ok(Some(reply)) if reply["permission"] == "deny" => {
                    let message = reply["message"].as_str().map_or_else(
                        || format!("refused by hook {}", hook.hook.id),
                        str::to_owned,
                    );
                    return Verdict::Deny(message);
                }
                Err(error) if hook.hook.fail_closed => {
                    return Verdict::Deny(format!("hook {} failed: {error}", hook.hook.id));
                }
                _ => {}
            }
        }
        Verdict::Allow
    }

    /// Tell the `post_tool_use` hooks that `tool` ran with `arguments` and
    /// returned `result`. Their output and failures are ignored.
    pub async fn post_tool_use(&self, tool: &str, arguments: &Value, result: &str) {
        let event = json!({
            "event": "post_tool_use",
            "tool": tool,
            "arguments": arguments,
            "result": result,
            "seat": self.context.seat,
            "hive": self.context.hive,
        });
        for hook in self.bound(HookEvent::PostToolUse, Some(tool)) {
            let _ = self.call(hook, &event).await;
        }
    }

    /// Tell every `stop` hook the turn ended with `reply`. A stop hook only
    /// observes: it cannot start another turn, so its stdout is ignored, as
    /// are its failures.
    pub async fn stop(&self, reply: &str) {
        let event = json!({
            "event": "stop",
            "reply": reply,
            "seat": self.context.seat,
            "hive": self.context.hive,
        });
        for hook in self.bound(HookEvent::Stop, None) {
            let _ = self.call(hook, &event).await;
        }
    }

    /// The hooks bound to `event` whose matcher names `tool`, in order.
    fn bound<'a>(
        &'a self,
        event: HookEvent,
        tool: Option<&'a str>,
    ) -> impl Iterator<Item = &'a LoweredHook> {
        self.hooks.iter().filter(move |hook| {
            hook.hook.event == event
                && tool.is_none_or(|tool| matches(hook.hook.matcher.as_deref(), tool))
        })
    }

    /// Run `hook` with `event` as its argument object and return its stdout
    /// as JSON, or `None` when stdout is not JSON.
    async fn call(&self, hook: &LoweredHook, event: &Value) -> Result<Option<Value>> {
        let mut tool = hook.tool.clone();
        tool.timeout_ms = hook.hook.timeout_ms.unwrap_or(tool.timeout_ms);
        let call = self.calls.fetch_add(1, Ordering::Relaxed) + 1;
        let call_id = format!("hook-{}-{call}", hook.hook.id);
        let output = run(&tool, &self.context, self.secrets.as_ref(), event, &call_id).await?;
        Ok(output
            .json
            .or_else(|| serde_json::from_str(output.text.trim()).ok()))
    }
}

#[cfg(test)]
mod test;
