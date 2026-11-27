//! The `OpenClaw` Gateways a deployment's seats need.
//!
//! An `OpenClaw` seat's bridge takes its tools from a Gateway, configured with
//! the seat's tool server before it starts. Its isolation decides which:
//! `deployment`, the kind's default, puts it in the one Gateway every such
//! seat shares, which has one model provider, so those seats must agree on
//! their model endpoint and name; `seat` gives it a Gateway of its own.
use crate::installs::model_name;
use crate::{Error, Installs, Result};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;
use tinyhivemind_acp::openclaw::{GatewayPlan, GatewaySeat};
use tinyhivemind_host::harness::ToolEndpoint;
use tinyhivemind_lang::{HarnessKind, Isolation, LoweredSeat};

/// The name of the Gateway seat `seat` belongs to, under the deployment
/// root; `None` for a seat that is not on `OpenClaw`.
pub(crate) fn gateway_of(seat: &LoweredSeat) -> Option<String> {
    let harness = seat.integration.harness.as_ref()?;
    if harness.kind != HarnessKind::OpenClaw {
        return None;
    }
    Some(match harness.resolved_isolation() {
        Isolation::Deployment => "gateway".to_owned(),
        Isolation::Seat => format!("gateway-{}", seat.id),
    })
}

/// The endpoint `seat`'s model is served at.
fn endpoint(seat: &LoweredSeat) -> Result<String> {
    seat.integration
        .model
        .as_ref()
        .and_then(|model| model.provider.as_ref())
        .and_then(|provider| provider.endpoint.clone())
        .ok_or_else(|| Error::NoModel {
            seat: seat.id.clone(),
        })
}

/// 32 random bytes as 64 lowercase hex characters.
fn token() -> Result<String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(|error| Error::Random(error.to_string()))?;
    let mut text = String::with_capacity(64);
    for byte in bytes {
        let _ = write!(text, "{byte:02x}");
    }
    Ok(text)
}

/// A loopback port nothing listens on now. Another process may take it
/// before the Gateway binds it, which the Gateway then fails to start on.
fn free_port() -> Result<u16> {
    let listener =
        std::net::TcpListener::bind("127.0.0.1:0").map_err(Error::io("a gateway port"))?;
    let port = listener
        .local_addr()
        .map_err(Error::io("a gateway port"))?
        .port();
    Ok(port)
}

/// The Gateways `seats` need, by name, each with its seats and their tools.
/// `root` holds their directories.
pub(crate) fn plan(
    seats: &[(&LoweredSeat, &ToolEndpoint)],
    installs: &Installs,
    root: &Path,
) -> Result<BTreeMap<String, GatewayPlan>> {
    let mut plans: BTreeMap<String, (String, GatewayPlan)> = BTreeMap::new();
    for (seat, tools) in seats {
        let Some(name) = gateway_of(seat) else {
            continue;
        };
        let kind = HarnessKind::OpenClaw;
        let node = Installs::need(seat, kind, installs.node.as_ref(), "node")?;
        let openclaw = Installs::need(seat, kind, installs.openclaw.as_ref(), "openclaw")?;
        let endpoint = endpoint(seat)?;
        let model = model_name(seat)?;
        let gateway_seat = GatewaySeat {
            id: seat.id.clone(),
            tools: (*tools).clone(),
        };
        if let Some((first, plan)) = plans.get_mut(&name) {
            if plan.endpoint != endpoint || plan.model != model {
                return Err(Error::SharedGateway {
                    first: first.clone(),
                    seat: seat.id.clone(),
                });
            }
            plan.seats.push(gateway_seat);
            continue;
        }
        let plan = GatewayPlan {
            node: node.clone(),
            openclaw: openclaw.path.clone(),
            directory: root.join(&name),
            port: free_port()?,
            token: token()?,
            endpoint,
            model,
            seats: vec![gateway_seat],
        };
        plans.insert(name, (seat.id.clone(), plan));
    }
    Ok(plans
        .into_iter()
        .map(|(name, (_, plan))| (name, plan))
        .collect())
}

#[cfg(test)]
mod test;
