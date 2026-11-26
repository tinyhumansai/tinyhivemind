//! Plans are checked under the pinned `OpenHuman`'s own rules.
use super::*;

/// The plan of the bundled integrations package, which schedules a workflow.
fn plan() -> anyhow::Result<HiveConfig> {
    let package = crate::language::PackageSource::load(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tinyhivemind-lang/examples/hives/integrations"),
    )?;
    Ok(crate::language::plan::plan(&[package])?.config)
}

#[test]
fn a_package_plan_validates() -> anyhow::Result<()> {
    let config = plan()?;
    assert_eq!(config.workflows.len(), 1);
    config.validate()?;
    Ok(())
}

#[test]
fn a_schedule_the_openhuman_parser_refuses_is_an_invalid_workflow() -> anyhow::Result<()> {
    let mut config = plan()?;
    config.workflows[0].schedule = "90 90 * * *".into();
    assert!(matches!(
        config.validate(),
        Err(ConfigError::InvalidWorkflow(_))
    ));
    Ok(())
}

#[test]
fn a_built_in_openhuman_agent_id_is_reserved() -> anyhow::Result<()> {
    let mut config = plan()?;
    config.seats[0].agent_id = Some("orchestrator".into());
    assert!(matches!(
        config.validate(),
        Err(ConfigError::ReservedAgentId { .. })
    ));
    Ok(())
}
