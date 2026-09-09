//! Unit tests for CLI flag parsing.
//!
//! Promoted out of [`super`] to keep implementation and test module
//! files separate, matching the house convention (see `variety/`).

use super::*;

/// Compare two fidelity values, which arrive through floating-point
/// parsing and clamping rather than as bit-exact literals.
fn close(left: f64, right: f64) -> bool {
    (left - right).abs() < 1e-9
}

/// `f64::parse` accepts `"nan"`, `"inf"` and `"-inf"`, and `f64::clamp`
/// leaves a `NaN` unchanged rather than bounding it -- so an unguarded
/// `--fidelity nan` would have stored a `NaN` in `options.fidelity`, and
/// every downstream read of it (`ContextBudget::worth`, in turn
/// `windowed_score`'s fallback to the agent's own score) would have
/// silently discarded folded context instead of erroring or defaulting.
#[test]
fn a_non_finite_fidelity_is_rejected_rather_than_stored() {
    let mut options = Options::defaults();
    let before = options.fidelity;

    for raw in ["nan", "inf", "-inf"] {
        let mut args = [raw.to_owned()].into_iter();
        apply_expertise_flag(&mut options, "--fidelity", &mut args);
        assert!(
            close(options.fidelity, before),
            "a non-finite --fidelity value of {raw:?} must be rejected, \
             leaving the prior value in place",
        );
    }
}

#[test]
fn a_finite_fidelity_is_still_parsed_and_clamped() {
    let mut options = Options::defaults();

    let mut args = ["1.5".to_owned()].into_iter();
    apply_expertise_flag(&mut options, "--fidelity", &mut args);
    assert!(
        close(options.fidelity, 1.0),
        "a finite value is still clamped",
    );

    let mut args = ["0.4".to_owned()].into_iter();
    apply_expertise_flag(&mut options, "--fidelity", &mut args);
    assert!(close(options.fidelity, 0.4));
}
