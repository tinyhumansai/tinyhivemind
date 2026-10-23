//! One call of an executable tool under the calling convention of
//! `docs/specs/executable-tools.md`.
//!
//! [`run()`] resolves the program, substitutes the `argv` placeholders, builds
//! the scrubbed environment, starts the program in a process group of its
//! own, writes the argument object to stdin, and turns the exit status and
//! stdout into an [`Output`] or an [`Error`]. Nothing is cached: a `tools/…`
//! script is read from disk by the operating system on every call, so an edit
//! takes effect on the next one.
mod command;
mod process;

use crate::{Error, ExecContext, Result, Secrets};
use process::Launch;
use serde_json::Value;
use tinyhivemind_lang::{ExecTool, ToolOutput, ToolStdin};

/// The outcome of one successful call.
#[derive(Clone, Debug, PartialEq)]
pub struct Output {
    /// Stdout, decoded as UTF-8 with invalid sequences replaced.
    pub text: String,
    /// Stdout parsed, for an `output: json` tool; `None` otherwise.
    pub json: Option<Value>,
    /// Stdout passed 1 MiB and `text` holds only the first 1 MiB.
    pub truncated: bool,
}

/// Run one call of `tool` with `args`, exporting `call_id` as `THM_CALL_ID`.
///
/// # Errors
///
/// - [`Error::EmptyCommand`] when the declaration names no program;
/// - [`Error::MissingArgument`] when a required `{name}` placeholder has no
///   argument;
/// - [`Error::UnresolvedSecret`] when a declared `env` reference has no value;
/// - [`Error::Spawn`] when the program cannot be started;
/// - [`Error::Io`] when its pipes fail;
/// - [`Error::Timeout`] when it outlives `timeout_ms`;
/// - [`Error::Exit`] when it exits with a status other than 0;
/// - [`Error::InvalidOutput`] when an `output: json` tool prints non-JSON.
pub async fn run(
    tool: &ExecTool,
    context: &ExecContext,
    secrets: &dyn Secrets,
    args: &Value,
    call_id: &str,
) -> Result<Output> {
    let launch = Launch {
        program: command::program(tool, context)?,
        arguments: command::arguments(tool, args)?,
        environment: command::environment(tool, context, secrets, call_id)?,
        directory: command::working_directory(tool, context),
        input: (tool.stdin == ToolStdin::Json).then(|| args.to_string().into_bytes()),
    };
    let finished = process::execute(tool, launch).await?;
    if !finished.status.success() {
        return Err(Error::Exit {
            tool: tool.id.clone(),
            code: finished.status.code(),
            stderr: String::from_utf8_lossy(&finished.stderr).into_owned(),
        });
    }
    let text = String::from_utf8_lossy(&finished.stdout).into_owned();
    let json = match tool.output {
        ToolOutput::Text => None,
        ToolOutput::Json => {
            Some(
                serde_json::from_str(&text).map_err(|source| Error::InvalidOutput {
                    tool: tool.id.clone(),
                    source,
                })?,
            )
        }
    };
    Ok(Output {
        text,
        json,
        truncated: finished.truncated,
    })
}

#[cfg(test)]
mod test;
