//! Every type that crosses the template module's `TinyBus` boundary, and the
//! names of the members that carry them.
//!
//! This crate ships as a loadable `TinyBus` module: `crates/template` is built
//! as a `cdylib` and exports one object. A host that loads that binary can call
//! into it but cannot `use` anything out of it, so the payload vocabulary has
//! to be published as an ordinary library. This is that library.
//!
//! # What is here
//!
//! - [`names`] — the interface name, the object path, and one constant per
//!   member, plus [`names::METHODS`] listing them in dispatch order.
//! - [`greeting`] — the value vocabulary: the request and response payloads the
//!   `Greet` member exchanges.
//! - [`version`] — [`CONTRACT_VERSION`] and the [`is_compatible`] bind rule.
//!
//! # What is deliberately not here
//!
//! **No behavior.** The `greet` implementation lives in `crates/template`,
//! which depends on this crate and re-exports it. A payload type describes what
//! a frame carries, not what the module does with it.
//!
//! **No transport.** This crate does not depend on `tinybus` and holds no
//! connection, client, or codec. A host already owns its connection — its
//! reconnect policy, its timeouts, its tracing — and the useful part is the
//! vocabulary, not another wrapper around it.
//!
//! That is also a structural necessity, not only a preference: `tinybus` is
//! vendored as a submodule whose manifest inherits fields from its own nested
//! `[workspace.package]`. A crate that every workspace member can depend on has
//! to stay transport-free, and staying transport-free is what keeps this crate
//! down to two pure-Rust dependencies.
//!
//! # This crate sits underneath the implementation, not beside it
//!
//! `template` **depends on this crate and re-exports all of it**, so
//! `template::GreetRequest` and `template_bus::greeting::GreetRequest` are the
//! *same type*, not structural twins. Defining a parallel set of payload types
//! for hosts would mean a conversion at every call site that nothing checks.
//! One definition, here, at the bottom.
//!
//! So: a module author depends on `template` and gets behavior and vocabulary.
//! A host depends on `template-bus` and gets vocabulary alone.
//!
//! # Staying in step with the module
//!
//! [`names::METHODS`] lists every member. `crates/template` asserts its served
//! members against that list, in order, so a method added to the interface
//! without an entry here fails that crate's tests rather than surfacing as an
//! unknown method in a host at runtime.
//!
//! # Example
//!
//! ```
//! use template_bus::{names, GreetRequest, GreetResponse};
//!
//! let body = serde_json::to_value([GreetRequest::new("Ferris")])?;
//! assert_eq!(names::methods::GREET, "Greet");
//! assert_eq!(names::OBJECT_PATH, "/ai/tinyhumans/template/Greeting");
//!
//! let reply: GreetResponse = serde_json::from_value(
//!     serde_json::json!({ "greeting": "Hello, Ferris!" }),
//! )?;
//! assert_eq!(reply.greeting, "Hello, Ferris!");
//! # Ok::<(), serde_json::Error>(())
//! ```

pub mod greeting;
pub mod names;
pub mod version;

pub use greeting::{GreetRequest, GreetResponse};
pub use names::{INTERFACE, METHODS, OBJECT_PATH};
pub use version::{CONTRACT_VERSION, is_compatible};
