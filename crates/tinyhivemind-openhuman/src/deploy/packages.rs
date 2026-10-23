//! Deploy hive packages: lower them to the native plan, then attach what the
//! plan does not carry — executable tools, script hooks, skill bundles, and
//! the package's session and Jev settings.
use super::{DeployError, DeployResult, SecretResolver};
use crate::{
    config::{ConfigError, HiveConfig, SecretRef},
    language::{
        PackageSource,
        plan::{Plan, SeatPlan, seat_profile},
    },
};
use async_trait::async_trait;
use openhuman_embed::seams::{
    LearningSink, OpenHumanRunContext, ToolHook, ToolHookContext, ToolHookDecision, TurnSummary,
};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};
use tinyhivemind_exec::{ExecContext, HookRunner, ProgramTool, Secrets, Verdict};
use tinyhivemind_lang as lang;

/// The deployment's resolver, as the executable-tool runner asks for it.
pub(super) struct ExecSecrets(pub(super) Arc<dyn SecretResolver>);

impl std::fmt::Debug for ExecSecrets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ExecSecrets")
    }
}

impl Secrets for ExecSecrets {
    fn resolve(&self, reference: &lang::SecretRef) -> tinyhivemind_exec::Result<String> {
        let native = match reference {
            lang::SecretRef::Env(name) => SecretRef::Env(name.clone()),
            lang::SecretRef::Store(key) => SecretRef::Store(key.clone()),
        };
        self.0
            .resolve(&native)
            .map_err(|_| tinyhivemind_exec::Error::UnresolvedSecret)
    }
}

/// What one seat runs beyond its native definition.
#[derive(Clone)]
pub(super) struct SeatExec {
    pub(super) context: Arc<ExecContext>,
    pub(super) programs: Vec<lang::ExecTool>,
    pub(super) hooks: Arc<HookRunner>,
}

/// Executable tools and hooks, by seat id.
pub(super) type Extras = BTreeMap<String, SeatExec>;

/// The package plan made native: skill directories staged, the session and
/// Jev settings applied to the runtime config, and each seat's executables.
pub(super) fn prepare(
    packages: &[PackageSource],
    secrets: &Arc<dyn SecretResolver>,
    native: &mut openhuman_embed::RuntimeConfig,
) -> DeployResult<(HiveConfig, Extras)> {
    let Plan {
        mut config,
        seats,
        session,
        jev,
    } = crate::language::plan::plan(packages)?;
    apply_session(native, &session);
    apply_jev(native, &jev);
    let exec_secrets: Arc<dyn Secrets> = Arc::new(ExecSecrets(secrets.clone()));
    let mut extras = Extras::new();
    for (seat, plan) in &seats {
        if let Some(dir) = skills_dir(&config.runtime.deployment_id, seat, plan)? {
            let id = seat_profile(seat);
            if let Some(profile) = config.profiles.iter_mut().find(|p| p.id == id) {
                profile.skills = vec![dir.display().to_string()];
            }
        }
        let context = Arc::new(ExecContext {
            package_root: plan.root.clone(),
            workspace: config.runtime.workspace.as_ref().map(PathBuf::from),
            hive: plan.hive.clone(),
            seat: seat.clone(),
            features: plan.features.clone(),
        });
        let hooks = Arc::new(HookRunner::new(
            plan.hooks.clone(),
            context.clone(),
            exec_secrets.clone(),
        ));
        extras.insert(
            seat.clone(),
            SeatExec {
                context,
                programs: plan.programs.clone(),
                hooks,
            },
        );
    }
    Ok((config, extras))
}

fn apply_session(native: &mut openhuman_embed::RuntimeConfig, session: &lang::SessionSettings) {
    let agent = &mut native.agent;
    if let Some(value) = session.compact_context {
        agent.compact_context = value;
    }
    if let Some(value) = session.max_history_messages {
        agent.max_history_messages = value;
    }
    if let Some(value) = session.tool_result_budget_bytes {
        agent.tool_result_budget_bytes = value;
    }
    if let Some(value) = session.max_tool_iterations {
        agent.max_tool_iterations = value;
    }
    if let Some(value) = session.parallel_tools {
        agent.parallel_tools = value;
    }
    if let Some(value) = session.agents_md {
        agent.agents_md_enabled = value;
    }
}

fn apply_jev(native: &mut openhuman_embed::RuntimeConfig, jev: &lang::JevSettings) {
    let search = &mut native.agent.tool_search;
    search.ranker = match jev.ranker {
        lang::JevRanker::Jev => "jev",
        lang::JevRanker::Auto => "auto",
        lang::JevRanker::Bm25 => "bm25",
        lang::JevRanker::Compare => "compare",
    }
    .into();
    search.jev_route = match jev.route {
        lang::JevRoute::Auto => "auto",
        lang::JevRoute::Tinyhumans => "tinyhumans",
        lang::JevRoute::Typesafe => "typesafe",
        lang::JevRoute::Openrouter => "openrouter",
    }
    .into();
    search.jev_base_url.clone_from(&jev.base_url);
    search.top_k = jev.top_k;
}

/// The skill directory a seat's agent copies its bundles from.
///
/// A seat with every bundle the package ships uses `<root>/skills`
/// directly. One with a subset gets a staged copy holding only those, since
/// the harness installs a whole directory.
fn skills_dir(deployment: &str, seat: &str, plan: &SeatPlan) -> DeployResult<Option<PathBuf>> {
    let (selected, shipped) = &plan.skills;
    if selected.is_empty() {
        return Ok(None);
    }
    let source = plan.root.join("skills");
    if selected.len() == shipped.len() {
        return Ok(Some(source));
    }
    let staged = std::env::temp_dir()
        .join("tinyhivemind-skills")
        .join(deployment)
        .join(seat);
    let io = |source| {
        DeployError::Config(ConfigError::Io {
            path: staged.display().to_string(),
            source,
        })
    };
    if staged.exists() {
        std::fs::remove_dir_all(&staged).map_err(io)?;
    }
    for id in selected {
        copy_dir(&source.join(id), &staged.join(id)).map_err(io)?;
    }
    Ok(Some(staged))
}

fn copy_dir(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// A seat's executable tools, built fresh for one turn.
pub(super) fn programs(
    seat: &SeatExec,
    secrets: &Arc<dyn Secrets>,
) -> Vec<Box<dyn tinytools::Tool>> {
    seat.programs
        .iter()
        .map(|tool| {
            Box::new(ProgramTool::new(
                tool.clone(),
                seat.context.clone(),
                secrets.clone(),
            )) as Box<dyn tinytools::Tool>
        })
        .collect()
}

/// A seat's `pre_tool_use` and `post_tool_use` script hooks.
pub(super) struct ScriptHooks(pub(super) Arc<HookRunner>);

#[async_trait]
impl ToolHook<OpenHumanRunContext> for ScriptHooks {
    fn name(&self) -> &str {
        "tinyhivemind-script-hooks"
    }

    async fn before_tool_decision(
        &self,
        _host: &OpenHumanRunContext,
        context: &ToolHookContext,
    ) -> ToolHookDecision {
        match self
            .0
            .pre_tool_use(&context.tool_name, &context.arguments)
            .await
        {
            Verdict::Allow => ToolHookDecision::default(),
            Verdict::Deny(message) => ToolHookDecision::deny(message),
        }
    }

    async fn after_tool(
        &self,
        _host: &OpenHumanRunContext,
        context: &ToolHookContext,
    ) -> anyhow::Result<()> {
        let result = context
            .output
            .as_deref()
            .or(context.error.as_deref())
            .unwrap_or_default();
        self.0
            .post_tool_use(&context.tool_name, &context.arguments, result)
            .await;
        Ok(())
    }
}

/// A seat's `stop` script hooks, observing each finished reply.
pub(super) struct StopHooks(pub(super) Arc<HookRunner>);

#[async_trait]
impl LearningSink for StopHooks {
    async fn on_turn_complete(&self, summary: &TurnSummary) -> anyhow::Result<()> {
        // A stop hook observes; any follow-up it asks for is not a turn the
        // coordinator authorized, so it is not run.
        let _ = self.0.stop(&summary.output).await;
        Ok(())
    }
}
