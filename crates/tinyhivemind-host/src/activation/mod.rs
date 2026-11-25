//! Durable registration barrier for attached tools and concurrently claimed turns.
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Notify;
#[derive(Default)]
pub(crate) struct Activation {
    ready: AtomicBool,
    changed: Notify,
}
impl Activation {
    pub(crate) fn activate(&self) {
        self.ready.store(true, Ordering::Release);
        self.changed.notify_waiters();
    }
    pub(crate) fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }
    pub(super) async fn wait(&self) {
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
#[path = "activation_test.rs"]
mod test;
