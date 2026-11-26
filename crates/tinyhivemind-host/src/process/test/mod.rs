//! Real child processes: launching, observing, stopping and dropping them.
#[cfg(target_os = "linux")]
mod churn;
mod directory;
mod dropping;
mod launching;
mod scope;
#[cfg(target_os = "linux")]
mod stopping;
mod support;
