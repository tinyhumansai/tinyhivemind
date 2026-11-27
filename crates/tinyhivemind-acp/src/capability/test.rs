//! Which model routes an agent accepts.
#![allow(clippy::unwrap_used)] // Tests fail loudly on unexpected results.
use super::*;
use tinyhivemind_lang::{LoweredModel, LoweredSeatIntegration, Model, Provider, SecretRef};

fn seat(provider: Option<Provider>) -> LoweredSeat {
    LoweredSeat {
        id: "scout".into(),
        label: "Scout".into(),
        template: "seat".into(),
        config: serde_json::Value::Null,
        prompt: String::new(),
        memory: None,
        integration: LoweredSeatIntegration {
            model: Some(LoweredModel {
                model: Model {
                    id: "fast".into(),
                    provider: Some("p".into()),
                    name: "mock".into(),
                    ..Model::default()
                },
                provider,
            }),
            ..LoweredSeatIntegration::default()
        },
    }
}

fn provider(kind: ProviderKind, endpoint: Option<&str>) -> Provider {
    Provider {
        id: "p".into(),
        kind,
        endpoint: endpoint.map(Into::into),
        credential: None,
    }
}

#[test]
fn each_kind_takes_the_provider_it_speaks() {
    let anthropic = seat(Some(provider(
        ProviderKind::Anthropic,
        Some("http://127.0.0.1:1"),
    )));
    let model = check(&anthropic, HarnessKind::ClaudeCode).unwrap();
    assert_eq!(
        model,
        SeatModel {
            provider: "p".into(),
            endpoint: "http://127.0.0.1:1".into(),
            model: "mock".into()
        }
    );
    let openai = seat(Some(provider(
        ProviderKind::Openai,
        Some("http://127.0.0.1:1/v1"),
    )));
    assert!(check(&openai, HarnessKind::Codex).is_ok());
    assert!(matches!(
        check(&openai, HarnessKind::ClaudeCode),
        Err(Refusal::ProviderKind { .. })
    ));
    assert!(matches!(
        check(&anthropic, HarnessKind::Codex),
        Err(Refusal::ProviderKind { .. })
    ));
}

#[test]
fn a_route_without_an_endpoint_or_with_a_credential_is_refused() {
    let none = seat(Some(provider(ProviderKind::Openai, None)));
    assert_eq!(
        check(&none, HarnessKind::Codex),
        Err(Refusal::NoEndpoint("p".into()))
    );
    let mut keyed = provider(ProviderKind::Openai, Some("http://127.0.0.1:1/v1"));
    keyed.credential = Some(SecretRef::Env("KEY".into()));
    assert_eq!(
        check(&seat(Some(keyed)), HarnessKind::Codex),
        Err(Refusal::Credential("p".into()))
    );
    assert_eq!(
        check(&seat(None), HarnessKind::Codex),
        Err(Refusal::NoProvider("fast".into()))
    );
    let mut bare = seat(None);
    bare.integration.model = None;
    assert_eq!(
        check(&bare, HarnessKind::Codex),
        Err(Refusal::NoModel("scout".into()))
    );
}

#[test]
fn refusals_name_the_field() {
    assert_eq!(
        Refusal::NoEndpoint("p".into()).to_string(),
        "provider p has no endpoint"
    );
    assert_eq!(
        provider_kind(HarnessKind::ClaudeCode),
        ProviderKind::Anthropic
    );
    assert_eq!(provider_kind(HarnessKind::OpenClaw), ProviderKind::Openai);
}
