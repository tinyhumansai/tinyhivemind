//! Which Gateway each `OpenClaw` seat joins, and what each Gateway is given.
#![allow(clippy::unwrap_used, clippy::panic)] // Tests fail loudly on unexpected results.
use super::*;
use crate::Pinned;
use crate::fixture::{seat, without_model};
use std::path::PathBuf;

fn installs() -> Installs {
    Installs {
        node: Some("/opt/node".into()),
        openclaw: Some(Pinned::new("/opt/openclaw/openclaw.mjs", "2026.10.33")),
        ..Installs::default()
    }
}

fn tools(id: &str) -> ToolEndpoint {
    ToolEndpoint::new(
        "hive",
        "http://127.0.0.1:7000/mcp",
        format!("bearer-{id}"),
        [],
    )
}

fn claw(id: &str, isolation: Option<Isolation>, model: &str) -> LoweredSeat {
    seat(
        id,
        HarnessKind::OpenClaw,
        isolation,
        "http://127.0.0.1:9/v1",
        model,
    )
}

fn planned(seats: &[LoweredSeat], installs: &Installs) -> Result<BTreeMap<String, GatewayPlan>> {
    let endpoints: Vec<ToolEndpoint> = seats.iter().map(|seat| tools(&seat.id)).collect();
    let pairs: Vec<_> = seats.iter().zip(endpoints.iter()).collect();
    plan(&pairs, installs, &PathBuf::from("/deploy"))
}

fn ids(plan: &GatewayPlan) -> Vec<&str> {
    plan.seats.iter().map(|seat| seat.id.as_str()).collect()
}

#[test]
fn seats_not_on_openclaw_need_no_gateway() {
    let others = [
        seat("a", HarnessKind::Codex, None, "http://x/v1", "m"),
        seat("b", HarnessKind::OpenHuman, None, "http://x/v1", "m"),
    ];
    assert!(planned(&others, &Installs::default()).unwrap().is_empty());
    let mut unnamed = others[1].clone();
    unnamed.integration.harness = None;
    assert_eq!(gateway_of(&unnamed), None);
}

#[test]
fn deployment_seats_share_one_gateway_and_seat_isolation_gets_its_own() {
    let seats = [
        claw("a", None, "mock"),
        claw("b", Some(Isolation::Deployment), "mock"),
        claw("c", Some(Isolation::Seat), "other"),
    ];
    let plans = planned(&seats, &installs()).unwrap();
    assert_eq!(plans.keys().collect::<Vec<_>>(), ["gateway", "gateway-c"]);
    let shared = &plans["gateway"];
    assert_eq!(ids(shared), ["a", "b"]);
    assert_eq!(shared.directory, PathBuf::from("/deploy/gateway"));
    assert_eq!(shared.model, "mock");
    assert_eq!(shared.endpoint, "http://127.0.0.1:9/v1");
    assert_eq!(shared.seats[0].tools.bearer(), "bearer-a");
    let own = &plans["gateway-c"];
    assert_eq!(ids(own), ["c"]);
    assert_eq!(own.model, "other");
    for plan in plans.values() {
        assert_eq!(plan.token.len(), 64);
        assert!(plan.token.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_ne!(plan.port, 0);
        assert_eq!(plan.node, PathBuf::from("/opt/node"));
    }
    assert_ne!(shared.token, own.token);
}

#[test]
fn a_shared_gateway_refuses_a_second_model() {
    let seats = [claw("a", None, "mock"), claw("b", None, "other")];
    match planned(&seats, &installs()) {
        Err(Error::SharedGateway { first, seat }) => {
            assert_eq!((first, seat), ("a".into(), "b".into()));
        }
        other => panic!("{other:?}"),
    }
    let mut moved = claw("b", None, "mock");
    if let Some(model) = moved.integration.model.as_mut()
        && let Some(provider) = model.provider.as_mut()
    {
        provider.endpoint = Some("http://elsewhere/v1".into());
    }
    assert!(matches!(
        planned(&[claw("a", None, "mock"), moved], &installs()),
        Err(Error::SharedGateway { .. })
    ));
}

#[test]
fn an_openclaw_seat_without_its_installs_or_model_is_refused() {
    assert!(matches!(
        planned(&[claw("a", None, "mock")], &Installs::default()),
        Err(Error::MissingInstall { .. })
    ));
    assert!(matches!(
        planned(&[without_model(claw("a", None, "mock"))], &installs()),
        Err(Error::NoModel { .. })
    ));
    let mut no_endpoint = claw("a", None, "mock");
    if let Some(model) = no_endpoint.integration.model.as_mut() {
        model.provider = None;
    }
    assert!(matches!(
        planned(&[no_endpoint], &installs()),
        Err(Error::NoModel { .. })
    ));
}
