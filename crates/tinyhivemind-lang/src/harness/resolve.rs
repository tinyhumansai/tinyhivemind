//! Which harness a seat resolves to, and which field named it.
//!
//! `harness` resolves like `model`: the seat's own, else its profile's, else
//! the runtime's. Lowering, validation and the edit rules all resolve through
//! here, so they cannot disagree about a seat's harness.
use crate::{Harness, Manifest, Seat};

/// The field a seat's harness came from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Origin {
    /// The seat's own `harness`.
    Seat,
    /// `harness` on the profile at this index of `profiles`.
    Profile(usize),
    /// `runtime.harness`.
    Runtime,
}

impl Origin {
    /// The JSON Pointer of the naming field, for the seat at `seat`.
    pub(crate) fn pointer(self, seat: usize) -> String {
        match self {
            Self::Seat => format!("/seats/{seat}/harness"),
            Self::Profile(k) => format!("/profiles/{k}/harness"),
            Self::Runtime => "/runtime/harness".into(),
        }
    }
}

/// The harness id `seat` names, most specific first, and where it was
/// named; `None` when nothing names one and the seat is an `openhuman` seat.
pub(crate) fn named<'a>(manifest: &'a Manifest, seat: &'a Seat) -> Option<(&'a str, Origin)> {
    if let Some(id) = &seat.harness {
        return Some((id, Origin::Seat));
    }
    let profile = seat.profile.as_ref().and_then(|id| {
        manifest
            .profiles
            .iter()
            .position(|profile| &profile.id == id)
    });
    if let Some(k) = profile
        && let Some(id) = &manifest.profiles[k].harness
    {
        return Some((id, Origin::Profile(k)));
    }
    manifest
        .runtime
        .harness
        .as_deref()
        .map(|id| (id, Origin::Runtime))
}

/// The declared harness `id`, if the `harnesses` section has it.
pub(crate) fn declared<'a>(manifest: &'a Manifest, id: &str) -> Option<&'a Harness> {
    manifest.harnesses.iter().find(|harness| harness.id == id)
}

/// The declared harness `seat` resolves to; `None` for an `openhuman` seat
/// that names none, or a reference validation reports as unresolved.
pub(crate) fn resolve<'a>(manifest: &'a Manifest, seat: &'a Seat) -> Option<&'a Harness> {
    named(manifest, seat).and_then(|(id, _)| declared(manifest, id))
}
