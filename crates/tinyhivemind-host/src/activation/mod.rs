//! Durable registration barrier for attached tools and concurrently claimed turns.
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Notify;

/// Opens once a seat's durable registration has completed.
///
/// A harness attaches a seat's tools before the coordinator has recorded the
/// registration, so a tool call or a claimed turn can arrive first. Tools ask
/// [`Activation::is_ready`]; a claimed turn awaits [`Activation::wait`].
#[derive(Debug, Default)]
pub struct Activation {
    ready: AtomicBool,
    changed: Notify,
}
impl Activation {
    /// Open the barrier and wake every waiting claim.
    pub fn activate(&self) {
        self.ready.store(true, Ordering::Release);
        self.changed.notify_waiters();
    }
    /// Whether registration has completed.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }
    /// Wait until registration has completed.
    pub async fn wait(&self) {
        loop {
            let changed = self.changed.notified();
            if self.is_ready() {
                return;
            }
            changed.await;
        }
    }
}
#[cfg(test)]
mod test;
