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

/// `--fact-noise` plants wrong facts and is read strictly: a missing,
/// nonnumeric, or out-of-range value is an error rather than a silent `0`
/// (silently truthful evidence), and the parser never mistakes the *next*
/// flag for its own value.
#[test]
fn a_missing_fact_noise_value_is_an_error() {
    let mut options = Options::defaults();
    let mut args = std::iter::empty();
    let result = apply_scale_flag(&mut options, "--fact-noise", &mut args);
    assert!(result.is_err(), "a missing --fact-noise value must error");
}

#[test]
fn a_nonnumeric_fact_noise_value_is_an_error_and_is_not_mistaken_for_a_flag() {
    let mut options = Options::defaults();
    // The token that follows is itself a flag. Before this fix
    // `next_number(args).unwrap_or(0)` still consumed it and read `0`,
    // silently dropping `--swarm` from the argument list; now it is read as
    // the (invalid) value and rejected outright, so nothing downstream can
    // mistake a swallowed flag for one that was never given.
    let mut args = ["--swarm".to_owned()].into_iter();
    let result = apply_scale_flag(&mut options, "--fact-noise", &mut args);
    assert!(
        result.is_err(),
        "a nonnumeric --fact-noise value must error rather than default to 0",
    );
    assert!(
        !options.evidence,
        "a rejected --fact-noise must not have side-effected evidence mode on",
    );
}

#[test]
fn a_fact_noise_value_above_the_per_mille_scale_is_rejected() {
    let mut options = Options::defaults();
    let mut args = ["1001".to_owned()].into_iter();
    let result = apply_scale_flag(&mut options, "--fact-noise", &mut args);
    assert!(
        result.is_err(),
        "1001 is above the 0..=1000 per-mille scale and must be rejected",
    );
}

#[test]
fn a_valid_fact_noise_value_is_parsed_and_enables_evidence() {
    let mut options = Options::defaults();
    let mut args = ["250".to_owned()].into_iter();
    let result = apply_scale_flag(&mut options, "--fact-noise", &mut args);
    assert_eq!(result, Ok(true));
    assert_eq!(options.fact_noise, 250);
    assert!(options.evidence, "--fact-noise implies --evidence");
}

/// `--calibrate --api-base <url>` is the documented argument order, and
/// `--api-base`'s handler used to promote every non-swarm mode straight to
/// `Mode::Live`, which ran a live episode instead of calibrating. It must
/// leave an explicitly chosen `Calibrate` alone.
#[test]
fn api_base_does_not_override_an_explicit_calibrate_mode() {
    let mut options = Options::defaults();
    apply_mode_flag(&mut options, "--calibrate");
    let mut args = ["http://localhost:8080".to_owned()].into_iter();
    apply_live_flag(&mut options, "--api-base", &mut args);
    assert!(
        matches!(options.mode, Mode::Calibrate),
        "--api-base must not overwrite an explicitly chosen --calibrate mode",
    );
}

/// The reverse order is the common case and must keep working: a bare
/// `--api-base` still promotes the parser's own default to `Mode::Live`.
#[test]
fn api_base_still_promotes_the_default_mode_to_live() {
    let mut options = Options::defaults();
    let mut args = ["http://localhost:8080".to_owned()].into_iter();
    apply_live_flag(&mut options, "--api-base", &mut args);
    assert!(matches!(options.mode, Mode::Live));
}

/// Naming a grid axis used to set `options.mode` to `Mode::Grid`
/// unconditionally, so `--swarm --topic hidden` silently ran a grid instead
/// of the federation the operator asked for. An axis flag may only promote
/// the parser's own default mode, never overwrite an explicit one.
#[test]
fn a_grid_axis_flag_does_not_override_an_explicit_swarm_mode() {
    let mut options = Options::parse(vec![
        "--swarm".to_owned(),
        "--topic".to_owned(),
        "hidden".to_owned(),
    ])
    .expect("a valid --swarm --topic combination must parse");
    assert!(
        matches!(options.mode, Mode::Swarm),
        "naming an axis after --swarm must not overwrite the explicit swarm mode",
    );
}

/// The common case, order-independent: naming an axis alone still selects
/// the grid from the parser's default mode.
#[test]
fn a_grid_axis_flag_still_selects_the_grid_from_the_default_mode() {
    let options = Options::parse(vec!["--topic".to_owned(), "hidden".to_owned()])
        .expect("a valid --topic value must parse");
    assert!(matches!(options.mode, Mode::Grid));
}
