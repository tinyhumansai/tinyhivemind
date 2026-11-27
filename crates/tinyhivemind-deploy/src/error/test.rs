//! Every variant reads as written: lowercase, without trailing punctuation.
use super::*;

fn io() -> std::io::Error {
    std::io::Error::other("refused")
}

#[test]
fn every_message_is_lowercase_and_unpunctuated() {
    let harness_error = || harness::Error::Harness("broke".into());
    let errors = [
        Error::MissingInstall {
            seat: "a".into(),
            kind: HarnessKind::Codex,
            missing: "codex-acp",
        },
        Error::NoModel { seat: "a".into() },
        Error::AcpRefused {
            seat: "a".into(),
            source: tinyhivemind_acp::capability::Refusal::NoModel("a".into()),
        },
        Error::OpenHumanRefused {
            seat: "a".into(),
            source: tinyhivemind_openhuman_process::capability::Refusal::NoModel,
        },
        Error::SharedGateway {
            first: "a".into(),
            seat: "b".into(),
        },
        Error::Random("no entropy".into()),
        Error::io("the deployment root")(io()),
        Error::Catalog {
            seat: "a".into(),
            source: CatalogError::NoModels,
        },
        Error::Server(tinyhivemind_mcp::Error::Stopped("the tool server".into())),
        Error::Gateway {
            gateway: "gateway".into(),
            source: harness_error(),
        },
        Error::Start {
            seat: "a".into(),
            source: harness_error(),
        },
        Error::Register {
            seat: "a".into(),
            source: tinyhivemind_hives::Error::StaleEpisode("e".into()),
        },
        Error::Stop {
            seat: "a".into(),
            source: harness_error(),
        },
    ];
    for error in errors {
        let message = error.to_string();
        let first = message.chars().next().unwrap_or_default();
        assert!(!first.is_uppercase(), "{message}");
        assert!(!message.ends_with('.'), "{message}");
    }
    let missing = Error::MissingInstall {
        seat: "a".into(),
        kind: HarnessKind::Codex,
        missing: "codex-acp",
    };
    assert_eq!(
        missing.to_string(),
        "seat a runs on codex, but no codex-acp is installed"
    );
}
