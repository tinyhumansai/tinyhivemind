//! The `harnesses` section and the `harness` fields: identities, references
//! and isolation, then the capability table once a package names a harness.
//!
//! A package that declares no harness and names none is deployed by the
//! linked `OpenHuman` adapter, which keeps its own limits, so the capability
//! table does not judge it. One that does is judged whole, its implicit
//! `openhuman` seats included: those run as a process too.
use super::{capability, identities, reference};
use crate::{
    Error, HarnessKind, Isolation, Package,
    diagnostic::{Findings, HIVE},
};

/// Push every harness finding for `package`.
pub(super) fn harnesses(package: &Package, found: &mut Findings) {
    let m = &package.manifest;
    let declared = identities(
        found,
        HIVE,
        "harness",
        m.harnesses.iter().map(|h| &h.id),
        |i| format!("/harnesses/{i}/id"),
    );
    let mut named = vec![(m.runtime.harness.as_ref(), "/runtime/harness".to_owned())];
    named.extend(
        m.profiles
            .iter()
            .enumerate()
            .map(|(k, p)| (p.harness.as_ref(), format!("/profiles/{k}/harness"))),
    );
    named.extend(
        m.seats
            .iter()
            .enumerate()
            .map(|(i, s)| (s.harness.as_ref(), format!("/seats/{i}/harness"))),
    );
    for (id, at) in named {
        if let Some(id) = id {
            found.check(reference("harness", id, &declared), HIVE, at);
        }
    }
    for (i, harness) in m.harnesses.iter().enumerate() {
        if harness.isolation == Some(Isolation::Deployment) && harness.kind != HarnessKind::OpenClaw
        {
            found.push(
                Error::Integration {
                    field: format!("harnesses.{}.isolation", harness.id),
                    reason: format!(
                        "only an openclaw harness shares one instance across a deployment, not {}",
                        harness.kind
                    ),
                },
                HIVE,
                format!("/harnesses/{i}/isolation"),
            );
        }
    }
    if crate::harness::resolve::judged(m) {
        capability::capabilities(package, found);
    }
}
