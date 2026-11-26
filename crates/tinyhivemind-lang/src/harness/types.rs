//! Wire types of the `harnesses` section.
use super::HarnessKind;
use serde::{Deserialize, Serialize};

/// Whether a harness instance serves one seat or a whole deployment.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Isolation {
    /// One supervised instance per seat.
    Seat,
    /// One instance every seat of the deployment on this harness shares.
    /// Only `openclaw` accepts it, and it is that kind's default.
    Deployment,
}

/// One entry of the `harnesses` section: a harness seats may name.
///
/// A package names only the kind and how instances are shared. It never
/// names an executable, port, state path or harness configuration: the host
/// maps a kind to a program, so a package edit cannot reach the process
/// that enforces its permissions.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Harness {
    /// Identity runtime, profiles and seats refer to.
    pub id: String,
    /// The harness kind.
    pub kind: HarnessKind,
    /// Instance sharing; `None` takes the kind's default
    /// ([`Harness::resolved_isolation`]). Omitted when `None`; an explicit
    /// `null` is a decode error.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::document::present"
    )]
    pub isolation: Option<Isolation>,
}

impl Harness {
    /// The isolation in force: the declared one, else the kind's default.
    #[must_use]
    pub fn resolved_isolation(&self) -> Isolation {
        self.isolation
            .unwrap_or_else(|| self.kind.default_isolation())
    }
}
