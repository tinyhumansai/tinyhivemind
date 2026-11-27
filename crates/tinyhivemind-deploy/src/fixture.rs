//! Seats for the unit tests: a lowered seat on a given harness kind and
//! isolation, with a model at a given endpoint.
use tinyhivemind_lang::{
    Harness, HarnessKind, Isolation, LoweredModel, LoweredSeat, LoweredSeatIntegration, Model,
    Provider, ProviderKind,
};

/// Seat `id` on `kind`, of `isolation` when given, running model `model` at
/// `endpoint`.
pub(crate) fn seat(
    id: &str,
    kind: HarnessKind,
    isolation: Option<Isolation>,
    endpoint: &str,
    model: &str,
) -> LoweredSeat {
    let provider = Provider {
        id: "local".into(),
        kind: ProviderKind::Openai,
        endpoint: Some(endpoint.into()),
        credential: None,
    };
    let model = Model {
        id: "fast".into(),
        provider: Some("local".into()),
        name: model.into(),
        ..Model::default()
    };
    LoweredSeat {
        id: id.into(),
        label: id.into(),
        template: "seat".into(),
        config: serde_json::Value::Null,
        prompt: format!("You are {id}."),
        memory: None,
        integration: LoweredSeatIntegration {
            model: Some(LoweredModel {
                model,
                provider: Some(provider),
            }),
            harness: Some(Harness {
                id: kind.to_string(),
                kind,
                isolation,
            }),
            ..LoweredSeatIntegration::default()
        },
    }
}

/// The same seat with no model.
pub(crate) fn without_model(mut seat: LoweredSeat) -> LoweredSeat {
    seat.integration.model = None;
    seat
}
