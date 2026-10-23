//! The program, argument vector and environment of one call.
use crate::{Error, Result};
use serde_json::Value;
use tinyhivemind_lang::{ExecTool, placeholder};

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
