//! The program, argument vector and environment of one call.
use crate::{Error, ExecContext, Result, Secrets};
use serde_json::Value;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use tinyhivemind_lang::{ExecTool, ToolDirectory, placeholder};

/// Everything after the program: the fixed leading arguments of `command`,
/// then `argv` with each whole-element placeholder substituted.
///
/// A string argument is substituted raw and any other as its compact JSON.
/// `{name?}` is dropped when the argument is absent or null; `{name}` then
/// fails the call.
pub(super) fn arguments(tool: &ExecTool, args: &Value) -> Result<Vec<String>> {
    let fixed = tool.command.iter().skip(1).cloned().map(Ok);
    let templated = tool.argv.iter().filter_map(|part| match placeholder(part) {
        Some((name, optional)) if !name.is_empty() => match args.get(name) {
            None | Some(Value::Null) if optional => None,
            None | Some(Value::Null) => Some(Err(Error::MissingArgument {
                tool: tool.id.clone(),
                name: name.to_owned(),
            })),
            Some(Value::String(text)) => Some(Ok(text.clone())),
            Some(other) => Some(Ok(other.to_string())),
        },
        _ => Some(Ok(part.clone())),
    });
    fixed.chain(templated).collect()
}

/// `arguments` with each fixed leading argument of `command` that names a
/// package file (`tools/…`) made absolute under the package root, so
/// `["python3", "tools/count.py"]` finds its script from any working
/// directory. Substituted arguments are left as the model sent them.
pub(super) fn package_paths(
    mut arguments: Vec<String>,
    tool: &ExecTool,
    context: &ExecContext,
) -> Vec<String> {
    let fixed = tool.command.len().saturating_sub(1);
    let root = absolute(&context.package_root);
    for argument in arguments.iter_mut().take(fixed) {
        if argument.starts_with("tools/") {
            *argument = root.join(argument.as_str()).display().to_string();
        }
    }
    arguments
}

/// The program to start: a `tools/…` path under the package root, or the
/// declared name for the operating system to find on `PATH`.
pub(super) fn program(tool: &ExecTool, context: &ExecContext) -> Result<PathBuf> {
    let first = tool.command.first().ok_or_else(|| Error::EmptyCommand {
        tool: tool.id.clone(),
    })?;
    Ok(match tool.package_program() {
        Some(path) => absolute(&context.package_root).join(path),
        None => PathBuf::from(first),
    })
}

/// The whole environment of the call: `PATH`, `HOME` and `LANG` from this
/// process when set, the declared `env` resolved, and the `THM_*` variables.
pub(super) fn environment(
    tool: &ExecTool,
    context: &ExecContext,
    secrets: &dyn Secrets,
    call_id: &str,
) -> Result<Vec<(String, OsString)>> {
    let mut env: Vec<(String, OsString)> = INHERITED
        .iter()
        .filter_map(|name| Some(((*name).to_owned(), std::env::var_os(name)?)))
        .collect();
    for (name, reference) in &tool.env {
        env.push((name.clone(), secrets.resolve(reference)?.into()));
    }
    let root = absolute(&context.package_root).into_os_string();
    env.extend([
        ("THM_HIVE".to_owned(), context.hive.clone().into()),
        ("THM_SEAT".to_owned(), context.seat.clone().into()),
        ("THM_TOOL".to_owned(), tool.id.clone().into()),
        ("THM_CALL_ID".to_owned(), call_id.into()),
        ("THM_PACKAGE_ROOT".to_owned(), root),
    ]);
    env.extend(context.features.iter().map(|(name, on)| {
        let value = if *on { "1" } else { "0" };
        (format!("THM_FEATURE_{}", name.to_uppercase()), value.into())
    }));
    Ok(env)
}

/// The directory the call runs in.
pub(super) fn working_directory(tool: &ExecTool, context: &ExecContext) -> PathBuf {
    let directory = match (tool.cwd, &context.workspace) {
        (ToolDirectory::Workspace, Some(workspace)) => workspace,
        _ => &context.package_root,
    };
    absolute(directory)
}

/// The variables a program inherits from this process.
const INHERITED: [&str; 3] = ["PATH", "HOME", "LANG"];

/// `path` made absolute against this process's directory, so it means the
/// same file from the program's own working directory.
fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}
