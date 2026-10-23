//! Packages lower to one native plan; shared seats must agree.
use super::*;
use crate::config::ToolScope;

fn integrations() -> PackageSource {
    PackageSource::load(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tinyhivemind-lang/examples/hives/integrations"),
    )
    .unwrap_or_else(|error| unreachable!("the example package checks: {error}"))
}

#[test]
fn a_package_lowers_to_a_native_plan_with_every_integration() -> anyhow::Result<()> {
    let source = integrations();
    let plan = plan(std::slice::from_ref(&source))?;
    let config = &plan.config;
    assert_eq!(config.runtime.deployment_id, "release-desk");
    assert_eq!(config.runtime.provider.kind, "openrouter");
    assert_eq!(
        config.runtime.provider.credential,
        Some(config::SecretRef::Env("OPENROUTER_API_KEY".into()))
    );
    assert_eq!(config.runtime.session_store, "memory");
    assert_eq!(config.runtime.concurrency, 2);
    let lead = config
        .profiles
        .iter()
        .find(|p| p.id == "seat-lead")
        .ok_or_else(|| anyhow::anyhow!("lead profile"))?;
    assert!(lead.system_prompt.starts_with("You lead the release desk"));
    assert!(lead.system_prompt.contains("one change per line"));
    assert_eq!(
        lead.model.name.as_deref(),
        Some("anthropic/claude-sonnet-4.5")
    );
    assert_eq!(lead.mcp[0].id, "issues");
    assert_eq!(lead.mcp[0].transport, "http");
    assert_eq!(lead.groups["web"], lang::GroupMode::Off);
    assert_eq!(lead.subagents, ["summariser"]);
    let scout = config
        .profiles
        .iter()
        .find(|p| p.id == "seat-scout")
        .ok_or_else(|| anyhow::anyhow!("scout profile"))?;
    assert_eq!(scout.tool_scope, ToolScope::Wildcard);
    assert!(scout.deny_tools.contains(&"write_file".to_owned()));
    // The declared profile becomes a native profile, for delegation.
    assert!(config.profiles.iter().any(|p| p.id == "summariser"));
    assert_eq!(config.permission_profiles[0].id, "read_only");
    let hive = &config.hives[0];
    assert_eq!(hive.id, "integrations");
    assert_eq!(
        hive.members[1].permission_profile.as_deref(),
        Some("read_only")
    );
    assert_eq!(config.workflows[0].id, "nightly-digest");
    let lead = &plan.seats["lead"];
    assert_eq!(lead.programs.len(), 2);
    assert_eq!(lead.hooks.len(), 1);
    assert_eq!(lead.skills.0, ["release-notes"]);
    assert_eq!(plan.jev.top_k, 5);
    assert_eq!(plan.session.max_history_messages, Some(80));
    config.clone().validate()?;
    Ok(())
}

#[test]
fn a_seat_two_packages_declare_differently_conflicts() -> anyhow::Result<()> {
    let first = integrations();
    let mut package = first.package().clone();
    package.manifest.id = "second".into();
    package.manifest.workflows.clear();
    package.manifest.seats[1].model = Some("careful".into());
    let second = PackageSource::new(first.root(), package)?;
    let error = plan(&[first, second]).err();
    assert!(
        matches!(error, Some(ConfigError::ConflictingSeat(ref seat)) if seat == "scout"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_shared_seat_may_narrow_per_hive_and_runtimes_must_match() -> anyhow::Result<()> {
    let first = integrations();
    let mut package = first.package().clone();
    package.manifest.id = "second".into();
    package.manifest.workflows.clear();
    package.manifest.seats[0].permissions = Some("read_only".into());
    let second = PackageSource::new(first.root(), package.clone())?;
    let plan = plan(&[first.clone(), second])?;
    assert_eq!(plan.config.hives.len(), 2);
    assert_eq!(plan.config.seats.len(), 2);
    assert_eq!(
        plan.config.hives[1].members[0]
            .permission_profile
            .as_deref(),
        Some("read_only")
    );
    package.manifest.runtime.concurrency = 9;
    let third = PackageSource::new(first.root(), package)?;
    let error = super::plan(&[first, third]).err();
    assert!(matches!(
        error,
        Some(ConfigError::ConflictingDeclaration { ref section, .. }) if section == "runtime"
    ));
    Ok(())
}

#[test]
fn an_empty_deployment_and_undecodable_tool_rules_are_refused() -> anyhow::Result<()> {
    assert!(matches!(plan(&[]), Err(ConfigError::EmptyHive(_))));
    let first = integrations();
    let mut package = first.package().clone();
    package.manifest.permissions[0].tool_rules = serde_json::json!("not rules");
    let source = PackageSource::new(first.root(), package)?;
    assert!(matches!(
        plan(&[source]),
        Err(ConfigError::UnsupportedSetting(ref field)) if field.contains("tool_rules")
    ));
    Ok(())
}

#[test]
fn a_package_that_does_not_check_reports_every_finding() {
    let first = integrations();
    let mut package = first.package().clone();
    package.manifest.seats[0].model = Some("ghost".into());
    package.manifest.tools[0].argv = vec!["{nope}".into()];
    let error = PackageSource::new(first.root(), package).err();
    assert!(
        matches!(error, Some(ConfigError::Package { ref diagnostics, .. }) if diagnostics.len() == 2),
        "{error:?}"
    );
    assert!(PackageSource::load("/nonexistent/tinyhivemind").is_err());
}
