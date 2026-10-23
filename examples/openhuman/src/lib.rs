//! Package-backed example construction, keeping native ports outside checked-in JSON.
//! Each checked-in hive is a hive language package under `hives/`. A binary
//! reads it with [`manifest`], applies its runtime-only edits (paths, ids,
//! MCP arguments, model names) to the decoded package, and hands it to
//! [`deploy`], which checks, lowers and deploys it. The deployment owns the
//! runtime and persistent seats; [`ExperimentEffects`] classifies the
//! experiments' application-owned MCP tools.
use std::{path::PathBuf, sync::Arc};
use tinyhivemind_core::approval::Effect;
use tinyhivemind_lang::Package;
use tinyhivemind_openhuman::{
    deploy::{BuildOptions, EffectClassifier, EnvironmentSecrets, HiveDeployment},
    language::PackageSource,
};

/// The directory of a checked-in package, such as `basic_hive` or
/// `multi_hive/hive-a`.
#[must_use]
pub fn root(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("hives")
        .join(name)
}

/// Read a checked-in package for editing, without checking it, resolving
/// credentials or starting a runtime.
/// # Errors
/// Returns filesystem failures and canonical files that do not decode.
pub fn manifest(name: &str) -> anyhow::Result<Package> {
    Ok(PackageSource::read(root(name))?)
}

/// Classify the exact application MCP verbs served by these examples.
///
/// Every seat connects to its own hive tool server, `tinyhive-<seat>`, since a
/// package declares one server per argument list and each seat's arguments
/// name it. Unknown servers and tools remain refused.
pub struct ExperimentEffects;
impl EffectClassifier for ExperimentEffects {
    fn classify(&self, context: &openhuman_embed::seams::ToolHookContext) -> Effect {
        if context.tool_name != "mcp_call_tool" {
            return Effect::Unclassified;
        }
        let hive = context.arguments["server"]
            .as_str()
            .and_then(|server| server.strip_prefix("tinyhive-"))
            .is_some_and(|seat| !seat.is_empty());
        match (
            context.arguments["server"].as_str(),
            context.arguments["tool"].as_str(),
        ) {
            (Some(_), Some("broadcast" | "complete_episode" | "hive_memory_recall")) if hive => {
                Effect::ReadOnly
            }
            (Some(_), Some("hive_memory_note" | "hive_memory_forget")) if hive => Effect::Mutating,
            (Some("deepswe"), Some("file_read")) => Effect::ReadOnly,
            (Some("deepswe"), Some("file_write" | "file_edit" | "shell" | "test")) => {
                Effect::Mutating
            }
            _ => Effect::Unclassified,
        }
    }
}

/// Check, lower and deploy edited packages on one runtime, one hive each.
///
/// Each entry names the checked-in package directory its relative files
/// resolve against, as [`root`] takes it, and the package as edited.
/// # Errors
/// Returns package findings or native deployment failures.
pub async fn deploy(
    packages: Vec<(&str, Package)>,
    mut options: BuildOptions,
) -> anyhow::Result<HiveDeployment> {
    options.classifier = Some(Arc::new(ExperimentEffects));
    let sources = packages
        .iter()
        .map(|(name, package)| PackageSource::new(root(name), package))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Box::pin(HiveDeployment::from_packages(
        sources,
        Arc::new(EnvironmentSecrets),
        options,
    ))
    .await?)
}

#[cfg(test)]
mod test;
