//! What a run spent, and the handle a seat's total is read back through.
//!
//! Split out of [`super`] when the file passed the repository's 700-line cap.
//! The seam is a real one rather than a line count: everything here is
//! *accounting* — how many tokens a request was billed and how that total
//! reaches the table — and nothing in it knows what a request looks like or
//! how one is put.

/// Tokens spent and calls made by one seat, or one poll, over an HTTP backend.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Usage {
    /// Prompt/input tokens billed.
    pub(crate) input: u64,
    /// Completion/output tokens billed.
    pub(crate) output: u64,
    /// Successful requests made (a retried request still counts once).
    pub(crate) calls: u64,
}

impl Usage {
    pub(super) fn add(&mut self, input: u64, output: u64) {
        self.input = self.input.saturating_add(input);
        self.output = self.output.saturating_add(output);
        self.calls = self.calls.saturating_add(1);
    }

    /// Tokens a failed attempt was billed for anyway.
    ///
    /// No call is counted: `calls` measures the requests that produced a
    /// turn, and the retry that follows counts itself. The tokens are real
    /// spend whether or not the reply was usable, so dropping them would
    /// under-report what the run cost.
    pub(super) fn spent(&mut self, input: u64, output: u64) {
        self.input = self.input.saturating_add(input);
        self.output = self.output.saturating_add(output);
    }

    /// Total tokens spent, input and output combined.
    #[must_use]
    pub(crate) fn tokens(&self) -> u64 {
        self.input.saturating_add(self.output)
    }

    /// Cost at `cost_per_1k` units per thousand tokens.
    #[must_use]
    pub(crate) fn cost(&self, cost_per_1k: u64) -> u64 {
        self.tokens().saturating_mul(cost_per_1k) / 1000
    }
}

/// A shared handle onto one seat's running usage total.
///
/// [`HttpAgent`] is seated as a `Box<dyn Participant>` so the harness can mix
/// HTTP and CLI seats in one room, and the trait object erases everything but
/// [`Participant`] once it is boxed. This handle is the way around that: the
/// caller clones it out at seat-building time, before boxing, and reads it
/// back after the episode finishes driving.
///
/// `Arc<Mutex<_>>` rather than `Rc<RefCell<_>>`, because a live federation
/// runs its desks at the same time: each desk authorizes exactly one speaker,
/// so N desks in flight is N model calls in flight, and a seat that is not
/// `Send` cannot be handed to a worker. The lock is uncontended in practice —
/// one seat's usage is touched by one thread, right after that seat's own
/// request returns.
///
/// Read it through [`usage_of`] rather than by locking directly. A poisoned
/// handle must not be skipped: skipping one is a cost report that silently
/// undercounts, which is the one thing a spend column may not do.
pub(crate) type UsageHandle = std::sync::Arc<std::sync::Mutex<Usage>>;

/// Read a seat's usage, recovering a poisoned handle rather than dropping it.
///
/// [`Usage`] is four saturating counters with no invariant across them, so a
/// thread that panicked mid-update leaves a total that is *stale*, never
/// inconsistent. Recovering it costs at most one call's tokens; skipping the
/// seat costs its whole run and says nothing about having done so.
pub(crate) fn usage_of(handle: &UsageHandle) -> Usage {
    *handle
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Fold one request's tokens into a seat's usage.
///
/// A poisoned lock means a worker panicked mid-update, which is a bug rather
/// than a benchmark outcome. The usage table is diagnostic, so the honest
/// response is to keep the run going and let the totals be short rather than
/// to take the whole federation down over a spent-token count.
pub(super) fn with_usage(handle: &UsageHandle, update: impl FnOnce(&mut Usage)) {
    // Recovered rather than dropped, for the reason `usage_of` gives: a lost
    // update is an undercounted spend column.
    let mut usage = handle
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    update(&mut usage);
}
