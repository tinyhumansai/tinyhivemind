//! Runs the executable tools and hooks a hive package declares.
//!
//! A hive package declares a tool as a program plus an argument schema, in
//! any language: a shell script under `tools/`, a Python file, a binary on
//! `PATH`. This crate runs one call of such a tool under the calling
//! convention of `docs/specs/executable-tools.md`:
//!
//! 1. a `tools/…` program, and every fixed `tools/…` argument of `command`
//!    (the script handed to an interpreter), resolves against the package
//!    root; any other program is found on `PATH`;
//! 2. it runs as `command ++ argv`, each whole-element `{name}` replaced by
//!    that argument (raw when a string, compact JSON otherwise) and each
//!    `{name?}` dropped when the argument is absent;
//! 3. the environment is cleared, then holds `PATH`, `HOME`, `LANG`, the
//!    declared `env` resolved through [`Secrets`], and `THM_HIVE`,
//!    `THM_SEAT`, `THM_TOOL`, `THM_CALL_ID`, `THM_PACKAGE_ROOT` and
//!    `THM_FEATURE_<NAME>`;
//! 4. with `stdin: json` the argument object is written to stdin;
//! 5. exit status 0 makes stdout the result; any other status fails with the
//!    code and the last 4 KiB of stderr; a timeout kills the process group;
//! 6. stdout past 1 MiB is truncated, and the result says so.
//!
//! The entry points:
//!
//! - [`run()`] — one call, returning an [`Output`] or a typed [`Error`];
//! - [`ProgramTool`] — a declaration as a native `tinytools::Tool`, whose
//!   failures reach the model as error results;
//! - [`HookRunner`] — a seat's `pre_tool_use`, `post_tool_use` and `stop`
//!   hooks, which are executable tools too.
//!
//! The crate holds no cache and no process between calls: a script is read
//! from disk on every call, so an edit takes effect on the next one. It
//! holds no sandbox either — a program runs under whatever confinement the
//! host already applies to tools, and `side_effects` is what that host's
//! permission gate classifies a call by. It names no harness type, so any
//! host that speaks `tinytools` can attach its tools.
//!
//! # Example
//!
//! ```no_run
//! use std::path::PathBuf;
//! use tinyhivemind_exec::{EnvSecrets, ExecContext, run};
//! use tinyhivemind_lang::ExecTool;
//!
//! # async fn example() -> tinyhivemind_exec::Result<()> {
//! let tool: ExecTool = serde_json::from_value(serde_json::json!({
//!     "id": "grep_repo",
//!     "description": "Search the repository for a pattern",
//!     "command": ["tools/grep_repo.sh"],
//!     "argv": ["--pattern", "{pattern}", "{path?}"],
//!     "side_effects": "read",
//! }))
//! .expect("a valid declaration");
//! let context = ExecContext {
//!     package_root: PathBuf::from("hives/research"),
//!     workspace: Some(PathBuf::from("/srv/workspaces/critic")),
//!     hive: "research".into(),
//!     seat: "critic".into(),
//!     ..ExecContext::default()
//! };
//! let args = serde_json::json!({"pattern": "fn main"});
//! let output = run(&tool, &context, &EnvSecrets, &args, "call-1").await?;
//! println!("{}", output.text);
//! # Ok(())
//! # }
//! ```
pub mod context;
pub mod error;
pub mod hook;
pub mod program;
mod run;

pub use context::{EnvSecrets, ExecContext, Secrets};
pub use error::{Error, Result};
pub use hook::{HookRunner, Verdict};
pub use program::ProgramTool;
pub use run::{Output, run};
