//! The names a package gives harnesses.
use super::HarnessKind;
use serde_json::json;

#[test]
fn each_kind_has_the_name_a_package_uses() -> Result<(), serde_json::Error> {
    for (kind, name) in [
        (HarnessKind::OpenHuman, "openhuman"),
        (HarnessKind::OpenClaw, "openclaw"),
        (HarnessKind::Codex, "codex"),
    ] {
        assert_eq!(serde_json::to_value(kind)?, json!(name));
        assert_eq!(serde_json::from_value::<HarnessKind>(json!(name))?, kind);
        assert_eq!(kind.to_string(), name);
    }
    for unknown in ["open_human", "OpenClaw", "claude", ""] {
        assert!(serde_json::from_value::<HarnessKind>(json!(unknown)).is_err());
    }
    Ok(())
}
