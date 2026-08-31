//! The bus identity of the template module: interface name, object path, and
//! one constant per member.
//!
//! Nothing here is a string literal at a call site. A host names a member
//! through [`methods`] and the object through [`OBJECT_PATH`], so a rename is a
//! compile error in every consumer rather than a runtime "unknown method".
//!
//! When generating a project from this template, rename all three together —
//! the interface, the path, and the member constants — and keep
//! [`METHODS`] in the same order as the interface's dispatch table.

/// The well-known interface name the module claims on the bus.
pub const INTERFACE: &str = "ai.tinyhumans.template.Greeting";

/// The object path the module serves its interface at.
pub const OBJECT_PATH: &str = "/ai/tinyhumans/template/Greeting";

/// One constant per member of [`INTERFACE`].
pub mod methods {
    /// Builds a greeting for a name.
    ///
    /// Takes a [`crate::GreetRequest`] and returns a [`crate::GreetResponse`].
    pub const GREET: &str = "Greet";
}

/// Every member of [`INTERFACE`], in the order the interface dispatches them.
///
/// `crates/template` asserts its declared manifest methods against this list,
/// so the two cannot drift.
pub const METHODS: &[&str] = &[methods::GREET];

#[cfg(test)]
mod test;
