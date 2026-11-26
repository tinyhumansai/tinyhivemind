//! The `autonomy` and `access` gate, judged before hooks and dispatch.
//!
//! It covers executable tools not declared `side_effects = "read"`. Hive
//! tools stay under send and management policy instead, as on the linked
//! path, and a harness's own approval prompts are never relied on.
use std::sync::Arc;
use tinyhivemind_host::harness::{Approver, ToolApproval, ToolDecision};
use tinyhivemind_lang::Access;

/// Admit `request` under `access`, asking `approver` when supervised.
///
/// # Errors
/// The refusal the model reads: `read_only` refuses outright, `supervised`
/// refuses without an approver or on its denial.
pub(crate) async fn admit(
    access: Access,
    approver: Option<&Arc<dyn Approver>>,
    request: ToolApproval,
) -> Result<(), String> {
    match access {
        Access::Autonomous => Ok(()),
        Access::ReadOnly => Err(format!(
            "access read_only refuses {}, which may change state",
            request.tool
        )),
        Access::Supervised => {
            let Some(approver) = approver else {
                return Err(format!(
                    "supervised seat {} has no approver for {}",
                    request.seat, request.tool
                ));
            };
            match approver.decide(request).await {
                ToolDecision::Allow => Ok(()),
                ToolDecision::Deny { reason } => Err(reason),
            }
        }
    }
}
