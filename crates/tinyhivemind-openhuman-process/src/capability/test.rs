//! The `openhuman` column of the capability table, row by row.
use super::{ITERATION_CAP, Refusal, SeatModel, check};
use tinyhivemind_lang::{
    GroupMode, LoweredModel, LoweredSeat, LoweredSeatIntegration, Model, PermissionSet, Provider,
    ProviderKind, Sandbox, SecretRef,
};

/// A seat on `openai` at an explicit endpoint, with nothing else set.
pub(crate) fn seat() -> LoweredSeat {
    let provider = Provider {
        id: "local".into(),
        kind: ProviderKind::Openai,
        endpoint: Some("http://127.0.0.1:9/v1".into()),
        credential: None,
    };
    let model = Model {
        id: "fast".into(),
        provider: Some("local".into()),
        name: "mock".into(),
        ..Model::default()
    };
    LoweredSeat {
        id: "scout".into(),
        label: "Scout".into(),
        template: "seat".into(),
        config: serde_json::Value::Null,
        prompt: "You scout.".into(),
        memory: None,
        integration: LoweredSeatIntegration {
            model: Some(LoweredModel {
                model,
                provider: Some(provider),
            }),
            ..LoweredSeatIntegration::default()
        },
    }
}

fn refused(change: impl FnOnce(&mut LoweredSeat)) -> Refusal {
    let mut seat = seat();
    change(&mut seat);
    match check(&seat) {
        Err(refusal) => refusal,
        Ok(model) => unreachable!("accepted {model:?}"),
    }
}

fn provider(seat: &mut LoweredSeat) -> &mut Provider {
    let model = seat
        .integration
        .model
        .as_mut()
        .map(|model| &mut model.provider);
    let provider = model.and_then(Option::as_mut);
    provider.unwrap_or_else(|| unreachable!("the seat has a provider"))
}

fn model(seat: &mut LoweredSeat) -> &mut Model {
    let model = seat
        .integration
        .model
        .as_mut()
        .map(|model| &mut model.model);
    model.unwrap_or_else(|| unreachable!("the seat has a model"))
}

#[test]
fn openai_at_an_explicit_endpoint_without_a_credential_is_accepted() {
    let accepted = check(&seat());
    let expected = SeatModel {
        provider: "local".into(),
        endpoint: "http://127.0.0.1:9/v1".into(),
        model: "mock".into(),
        iterations: ITERATION_CAP,
    };
    assert_eq!(accepted, Ok(expected));
}

#[test]
fn every_other_provider_row_is_rejected() {
    assert_eq!(
        refused(|seat| seat.integration.model = None),
        Refusal::NoModel
    );
    let inherits = refused(|seat| {
        if let Some(model) = &mut seat.integration.model {
            model.provider = None;
        }
    });
    assert_eq!(
        inherits,
        Refusal::NoProvider {
            model: "fast".into()
        }
    );
    for (kind, name) in [
        (ProviderKind::Inherit, "inherit"),
        (ProviderKind::Openrouter, "openrouter"),
        (ProviderKind::Anthropic, "anthropic"),
    ] {
        let refusal = refused(|seat| provider(seat).kind = kind);
        let expected = Refusal::ProviderKind {
            provider: "local".into(),
            kind: name,
        };
        assert_eq!(refusal, expected);
    }
    for endpoint in [None, Some("  ".to_owned())] {
        let refusal = refused(|seat| provider(seat).endpoint = endpoint);
        let expected = Refusal::NoEndpoint {
            provider: "local".into(),
        };
        assert_eq!(refusal, expected);
    }
    let credential = refused(|seat| provider(seat).credential = Some(SecretRef::Env("K".into())));
    let expected = Refusal::Credential {
        provider: "local".into(),
    };
    assert_eq!(credential, expected);
}

#[test]
fn model_settings_the_config_cannot_carry_are_rejected() {
    let temperature = refused(|seat| model(seat).temperature = Some(0.2));
    assert!(matches!(
        temperature,
        Refusal::ModelSetting {
            setting: "temperature",
            ..
        }
    ));
    let tokens = refused(|seat| model(seat).max_tokens = Some(10));
    assert!(matches!(
        tokens,
        Refusal::ModelSetting {
            setting: "max_tokens",
            ..
        }
    ));
}

#[test]
fn more_iterations_than_the_cap_are_rejected_and_fewer_are_kept() {
    let refusal = refused(|seat| seat.integration.limits.iterations = Some(9));
    assert_eq!(refusal, Refusal::Iterations { asked: 9, cap: 8 });
    let mut fewer = seat();
    fewer.integration.limits.iterations = Some(3);
    assert_eq!(check(&fewer).map(|model| model.iterations), Ok(3));
    let budget = refused(|seat| seat.integration.limits.budget_usd = Some(1.0));
    assert_eq!(budget, Refusal::Budget);
}

#[test]
fn a_sandbox_and_built_in_tools_are_rejected() {
    for sandbox in [Sandbox::ReadOnly, Sandbox::Sandboxed] {
        let refusal = refused(|seat| {
            let set = PermissionSet {
                id: "tight".into(),
                sandbox,
                ..PermissionSet::default()
            };
            seat.integration.permissions.push(set);
        });
        let expected = Refusal::Sandbox {
            permission: "tight".into(),
        };
        assert_eq!(refusal, expected);
    }
    let mut none = seat();
    none.integration.permissions.push(PermissionSet::default());
    none.integration
        .groups
        .insert("shell".into(), GroupMode::Off);
    assert!(check(&none).is_ok());
    for mode in [GroupMode::Advertised, GroupMode::Withheld] {
        let refusal = refused(|seat| {
            seat.integration.groups.insert("shell".into(), mode);
        });
        let expected = Refusal::ToolGroup {
            group: "shell".into(),
        };
        assert_eq!(refusal, expected);
    }
}

#[test]
fn fields_this_harness_does_not_take_are_named() {
    type Change = fn(&mut LoweredSeat);
    let cases: [(&str, Change); 6] = [
        ("agent_id", |seat| {
            seat.integration.agent_id = Some("a".into());
        }),
        ("definition_base", |seat| {
            seat.integration.definition_base = Some("b".into());
        }),
        ("subagents", |seat| {
            seat.integration.subagents.push("helper".into());
        }),
        ("context", |seat| {
            let document = ("notes.md".to_owned(), "text".to_owned());
            seat.integration.context.push(document);
        }),
        ("bare_prompt", |seat| seat.integration.bare_prompt = true),
        ("skills", |seat| {
            let skill = serde_json::json!({"id": "s"});
            let skill = serde_json::from_value(skill)
                .unwrap_or_else(|error| unreachable!("a skill of an id and a path: {error}"));
            seat.integration.skills.push(skill);
        }),
    ];
    for (field, change) in cases {
        assert_eq!(refused(change), Refusal::Unsupported(field), "{field}");
    }
    let memory = serde_json::from_value(serde_json::json!({
        "agent_id": "scout", "namespace": "agent:scout", "kind": "agent", "root": "",
        "identity": {"id": "m", "namespace": "agent:scout", "kind": "agent", "lifecycle": "run"},
        "settings": {"seat": "scout", "identity": "m", "budget_chars": 100}
    }))
    .unwrap_or_else(|error| unreachable!("a memory binding: {error}"));
    assert_eq!(
        refused(|seat| seat.memory = Some(memory)),
        Refusal::Unsupported("memory")
    );
}

#[test]
fn refusals_read_as_lowercase_sentences_naming_the_field() {
    let refusal = Refusal::Iterations { asked: 9, cap: 8 };
    assert_eq!(
        refusal.to_string(),
        "the seat asks for 9 iterations, more than the 8 this harness runs"
    );
    let unsupported = Refusal::Unsupported("memory").to_string();
    assert_eq!(
        unsupported,
        "the seat sets memory, which this harness does not take"
    );
}
