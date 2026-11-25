//! Hive seats on OpenHuman, each a supervised `openhuman-core` process.
pub mod capability;
mod config;
mod rpc;
mod turn;

#[cfg(test)]
#[path = "../tests/support/http.rs"]
mod http;
#[cfg(test)]
#[path = "../tests/fake/surface.rs"]
mod fake;
#[cfg(test)]
mod fixture;
