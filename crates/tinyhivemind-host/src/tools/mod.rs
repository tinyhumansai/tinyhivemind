//! The hive tools and the one belt that executes them.
mod types;
use crate::{Error, Result, policy::{Management, SendAuthorizer}};
use tinyhivemind_hives::Coordinator;
pub use types::Kind;
#[derive(Clone, Copy)]
pub struct Belt<'a>(&'a Coordinator);
impl<'a> Belt<'a> {
    pub fn new(c: &'a Coordinator) -> Self { Self(c) }
    pub fn with_send_policy(self, _: &'a dyn SendAuthorizer) -> Self { self }
    pub fn with_management(self, _: Management<'a>) -> Self { self }
    pub async fn call(&self, _: &ToolCall) -> Result<serde_json::Value> { let _ = self.0; Ok(serde_json::Value::Null) }
}
pub struct ToolCall;
impl ToolCall {
    pub fn new(_: impl Into<String>, _: Option<String>, _: Kind, _: serde_json::Value) -> Result<Self> { let _ = Error::ManagementDisabled; Ok(Self) }
}

#[cfg(test)]
mod test;
