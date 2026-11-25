//! The host's deployment plan, checked under `OpenHuman`'s own rules.
//!
//! The plan and its validation live in `tinyhivemind-host`; this module answers
//! what only the linked `OpenHuman` knows: which cron expressions its scheduler
//! accepts and which agent ids its built-in definitions take.
pub(crate) use tinyhivemind_host::config::*;

/// The pinned `OpenHuman`'s schedule parser and built-in registry.
struct OpenHumanRules;
impl HarnessRules for OpenHumanRules {
    fn accepts_schedule(&self, expression: &str) -> bool {
        tinyflows_schedule::validate_schedule(
            &openhuman_core::cron::Schedule::Cron {
                expr: expression.to_owned(),
                tz: None,
                active_hours: None,
            },
            chrono::DateTime::from_timestamp(0, 0).unwrap_or_default(),
        )
        .is_ok()
    }
    fn reserves_agent_id(&self, agent_id: &str) -> bool {
        openhuman_core::agent_registry::host_definition::AgentDefinitionRegistry::builtins_only()
            .get(agent_id)
            .is_some()
    }
}

/// Validation of a plan for `OpenHuman`.
pub(crate) trait Validate {
    /// Validate references and permissions without constructing any runtime.
    ///
    /// # Errors
    /// Returns the first typed identity, reference, limit or authority failure.
    fn validate(self) -> Result<ValidatedHiveConfig>;
}
impl Validate for HiveConfig {
    fn validate(self) -> Result<ValidatedHiveConfig> {
        self.validate_for(&OpenHumanRules)
    }
}

#[cfg(test)]
mod test;
