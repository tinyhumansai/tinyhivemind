//! Direct-edit checking: each edit rule, raw-file fallbacks, and the order
//! of derived per-class changes.
#![allow(clippy::expect_used, clippy::panic)] // Tests fail loudly on unexpected results.

mod files;
mod guards;
mod integration;
mod locations;
mod sequence;
mod support;
