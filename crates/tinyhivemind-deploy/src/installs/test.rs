//! Each kind's installs, the check before anything starts, and the harness a
//! seat gets.
#![allow(clippy::unwrap_used, clippy::panic)] // Tests fail loudly on unexpected results.
use super::*;
use crate::Deployment;
use crate::fixture::{seat, without_model};
use tinyhivemind_acp::capability::Refusal as AcpRefusal;
use tinyhivemind_openhuman_process::capability::Refusal as OpenHumanRefusal;

fn on(kind: HarnessKind) -> LoweredSeat {
    seat("a", kind, None, "http://127.0.0.1:9/v1", "mock")
}

fn missing(result: Result<()>) -> &'static str {
    match result {
        Err(Error::MissingInstall { missing, .. }) => missing,
        other => panic!("{other:?}"),
    }
}

fn everything() -> Installs {
    Installs {
        node: Some("/opt/node".into()),
        openhuman: Some(Pinned::new("/opt/openhuman-core", "0.64.15")),
        claude_code: Some(Pinned::new("/opt/claude/index.js", "0.112.0")),
        codex: Some(CodexInstall {
            adapter: Pinned::new("/opt/codex/index.js", "2.12.1"),
            binary: "/nonexistent/codex".into(),
        }),
        openclaw: Some(Pinned::new("/opt/openclaw/openclaw.mjs", "2026.10.33")),
    }
}

#[test]
fn each_kind_names_the_install_it_lacks() {
    let none = Installs::default();
    assert_eq!(
        missing(none.check(&on(HarnessKind::OpenHuman))),
        "openhuman-core"
    );
    assert_eq!(missing(none.check(&on(HarnessKind::ClaudeCode))), "node");
    assert_eq!(missing(none.check(&on(HarnessKind::Codex))), "node");
    assert_eq!(missing(none.check(&on(HarnessKind::OpenClaw))), "node");
    let only_node = Installs {
        node: Some("/opt/node".into()),
        ..Installs::default()
    };
    let lacks = |kind| missing(only_node.check(&on(kind)));
    assert_eq!(lacks(HarnessKind::ClaudeCode), "claude-agent-acp");
    assert_eq!(lacks(HarnessKind::Codex), "codex-acp");
    assert_eq!(lacks(HarnessKind::OpenClaw), "openclaw");
}

#[test]
fn every_kind_passes_with_its_installs_and_codex_and_openclaw_need_a_model() {
    let all = everything();
    for kind in HarnessKind::ALL {
        all.check(&on(kind)).unwrap();
    }
    for kind in [HarnessKind::Codex, HarnessKind::OpenClaw] {
        assert!(matches!(
            all.check(&without_model(on(kind))),
            Err(Error::NoModel { .. })
        ));
    }
    assert_eq!(model_name(&on(HarnessKind::Codex)).unwrap(), "mock");
}

#[test]
fn a_seat_gets_the_harness_its_kind_maps_to() {
    let root = tempfile::tempdir().unwrap();
    let deployment = Deployment::new(root.path());
    let catalog = root.path().join("a-codex-catalog.json");
    let all = everything();
    for kind in [HarnessKind::OpenHuman, HarnessKind::ClaudeCode] {
        all.harness(&on(kind), &deployment, &catalog, None).unwrap();
    }
    // Codex writes its catalog from the Codex binary, which is not there.
    assert!(matches!(
        all.harness(&on(HarnessKind::Codex), &deployment, &catalog, None),
        Err(Error::Catalog { .. })
    ));
    // OpenClaw's bridge needs its seat's Gateway.
    match all.harness(&on(HarnessKind::OpenClaw), &deployment, &catalog, None) {
        Err(Error::MissingInstall { missing, .. }) => assert_eq!(missing, "gateway"),
        other => panic!("{:?}", other.map(|_| ())),
    }
    let none = Installs::default();
    assert!(matches!(
        none.harness(&on(HarnessKind::OpenHuman), &deployment, &catalog, None),
        Err(Error::MissingInstall { .. })
    ));
}

#[test]
fn a_seat_its_harness_refuses_is_refused_before_anything_starts() {
    let all = everything();
    // Claude Code needs a model, like every ACP agent.
    assert!(matches!(
        all.check(&without_model(on(HarnessKind::ClaudeCode))),
        Err(Error::AcpRefused { ref seat, source: AcpRefusal::NoModel(_) }) if seat == "a"
    ));
    // Each agent speaks one provider kind: Claude Code's is Anthropic's.
    let mut openai = on(HarnessKind::ClaudeCode);
    let codex = on(HarnessKind::Codex);
    openai.integration.model = codex.integration.model;
    assert!(matches!(
        all.check(&openai),
        Err(Error::AcpRefused {
            source: AcpRefusal::ProviderKind { .. },
            ..
        })
    ));
    // OpenHuman takes only an OpenAI-compatible endpoint.
    let mut anthropic = on(HarnessKind::OpenHuman);
    anthropic.integration.model = on(HarnessKind::ClaudeCode).integration.model;
    assert!(matches!(
        all.check(&anthropic),
        Err(Error::OpenHumanRefused { ref seat, source: OpenHumanRefusal::ProviderKind { .. } }) if seat == "a"
    ));
}
