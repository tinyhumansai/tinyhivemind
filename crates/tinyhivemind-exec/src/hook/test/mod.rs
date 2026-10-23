//! Hooks against fixture scripts: verdicts, matchers, order, failures, and
//! the event objects each event passes.
#![allow(clippy::expect_used, clippy::panic)] // Tests fail loudly on unexpected results.
mod events;
mod matcher;
mod pre_tool_use;
mod support;
