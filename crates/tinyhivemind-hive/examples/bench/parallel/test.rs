//! Tests for the scoped-thread fan-out.
//!
//! The property under test is not "it is faster" — it is that the results come
//! back in *input* order whatever order the threads finish in, because every
//! paired statistic this harness prints is index-aligned across arms.

use super::{default_jobs, map_in_order};

#[test]
fn returns_results_in_input_order_at_every_job_count() {
    let items: Vec<usize> = (0..1000).collect();
    let expected: Vec<usize> = items.iter().map(|value| value * 3).collect();
    for jobs in [1, 2, 3, 7, 64, 4096] {
        let got =
            map_in_order(&items, jobs, |value| Ok(value * 3)).expect("multiplying cannot fail");
        assert_eq!(got, expected, "order must not depend on --jobs {jobs}");
    }
}

#[test]
fn out_of_order_completion_does_not_reorder_results() {
    // The first item sleeps longest, so completion order is close to the
    // reverse of input order on any machine with more than one core. If the
    // implementation ever returned completion order this is the test that
    // notices.
    let items: Vec<u64> = (0..16).collect();
    let got = map_in_order(&items, 16, |value| {
        std::thread::sleep(std::time::Duration::from_millis(16 - *value));
        Ok(*value)
    })
    .expect("sleeping cannot fail");
    assert_eq!(got, items);
}

#[test]
fn reports_the_first_error_in_input_order() {
    // Two failures, in different chunks. The one to report is the earlier
    // *room*, not whichever thread lost the race, so a rerun blames the same
    // room every time.
    let items: Vec<usize> = (0..100).collect();
    let error = map_in_order(&items, 8, |value| {
        if *value == 12 || *value == 87 {
            return Err(format!("room {value} failed"));
        }
        Ok(*value)
    })
    .expect_err("two rooms fail");
    assert_eq!(error, "room 12 failed");
}

#[test]
fn handles_slices_shorter_than_the_job_count() {
    let items = [7_usize];
    let got = map_in_order(&items, 32, |value| Ok(*value)).expect("one item cannot fail");
    assert_eq!(got, vec![7]);

    let empty: [usize; 0] = [];
    let got = map_in_order(&empty, 32, |value: &usize| Ok(*value)).expect("no items cannot fail");
    assert!(got.is_empty());
}

#[test]
fn a_panicking_closure_is_reported_rather_than_resumed() {
    let items: Vec<usize> = (0..64).collect();
    let error = map_in_order(&items, 8, |value| {
        assert!(*value != 30, "deliberate");
        Ok(*value)
    })
    .expect_err("the closure panics");
    assert!(error.contains("panicked"), "got {error}");
}

#[test]
fn default_jobs_is_at_least_one() {
    assert!(default_jobs() >= 1);
}
