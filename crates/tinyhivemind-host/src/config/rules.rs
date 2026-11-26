//! What only the harness can say about a deployment plan.

/// The harness-owned rules [`HiveConfig::validate_for`] applies.
///
/// A harness schedules workflows and names its own built-in agents, so it alone
/// knows which cron expressions it accepts and which ids are taken.
///
/// [`HiveConfig::validate_for`]: super::HiveConfig::validate_for
pub trait HarnessRules {
    /// Whether the harness accepts this five-field cron expression.
    fn accepts_schedule(&self, expression: &str) -> bool;
    /// Whether the harness already uses this agent id for a built-in.
    fn reserves_agent_id(&self, agent_id: &str) -> bool;
}
