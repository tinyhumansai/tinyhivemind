//! Unit tests for the usage handle a live seat reports its spend through.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

#[test]
fn a_poisoned_handle_still_reports_the_spend_it_holds() {
    // A cost column that silently drops a seat is worse than one that reports
    // a stale figure: the first understates the run and says nothing, the
    // second is off by at most the call that panicked.
    let handle: UsageHandle = std::sync::Arc::new(std::sync::Mutex::new(Usage::default()));
    with_usage(&handle, |usage| {
        usage.input = 100;
        usage.output = 40;
        usage.calls = 1;
    });

    // Poison it the only way a mutex is poisoned: panic while holding it.
    let poisoned = std::sync::Arc::clone(&handle);
    let panicked = std::thread::spawn(move || {
        let _guard = poisoned.lock().expect("first lock succeeds");
        panic!("a seat's worker died mid-update");
    })
    .join();
    assert!(panicked.is_err(), "the helper thread must have panicked");
    assert!(handle.lock().is_err(), "which poisons the handle");

    // Reading it still yields what it held...
    let usage = usage_of(&handle);
    assert_eq!(usage.input, 100);
    assert_eq!(usage.output, 40);
    assert_eq!(usage.calls, 1);

    // ...and writing through it still accumulates, so a run that survives a
    // panicked seat keeps counting rather than freezing that seat's total.
    with_usage(&handle, |usage| {
        usage.input = usage.input.saturating_add(10);
    });
    assert_eq!(usage_of(&handle).input, 110);
}
