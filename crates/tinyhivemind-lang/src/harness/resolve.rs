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

/// Whether the capability table judges `manifest`: it declares a harness,
/// or names one anywhere. One that does neither is deployed by the linked
/// adapter under its own limits.
pub(crate) fn judged(manifest: &Manifest) -> bool {
    !manifest.harnesses.is_empty()
        || manifest.runtime.harness.is_some()
        || manifest.profiles.iter().any(|p| p.harness.is_some())
        || manifest.seats.iter().any(|s| s.harness.is_some())
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

/// Where `edit` moves a seat of `base` to another harness, as JSON Pointers
/// into `edit`'s manifest.
///
/// A seat moves when its own `harness` field changes (a new seat counts from
/// no field), or when the harness it resolves to changes: an existing seat
/// against what it resolved to in `base`, a new one against `runtime.harness`.
/// A change of `runtime.harness` or a profile's `harness` is itself an
/// integration edit; one through a seat's `profile` is located there.
pub(crate) fn moves(base: &Manifest, edit: &Manifest) -> Vec<String> {
    let mut found = Vec::new();
    for (i, seat) in edit.seats.iter().enumerate() {
        let before = base.seats.iter().find(|s| s.id == seat.id);
        let was = before.and_then(|s| s.harness.as_ref());
        let resolved = named(edit, seat);
        let baseline = match before {
            Some(before) => named(base, before).map(|(id, _)| id),
            None => edit.runtime.harness.as_deref(),
        };
        let at = if seat.harness.as_ref() != was {
            Origin::Seat.pointer(i)
        } else if resolved.map(|(id, _)| id) != baseline {
            let profile_changed = before.is_none_or(|before| before.profile != seat.profile);
            match resolved {
                Some((_, origin)) if !profile_changed => origin.pointer(i),
                _ => format!("/seats/{i}/profile"),
            }
        } else {
            continue;
        };
        // Every seat that inherits a changed `runtime.harness` moves at one
        // field, and is one finding.
        if !found.contains(&at) {
            found.push(at);
        }
    }
    found
}
