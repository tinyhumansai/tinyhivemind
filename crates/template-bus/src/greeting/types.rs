//! Request and response types for the `Greet` member.

use serde::{Deserialize, Serialize};

/// The argument to [`crate::names::methods::GREET`].
///
/// The module trims surrounding whitespace from [`GreetRequest::name`] and
/// rejects a name that is empty once trimmed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct GreetRequest {
    /// The name to greet.
    pub name: String,
}

impl GreetRequest {
    /// Builds a request greeting `name`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use template_bus::GreetRequest;
    /// assert_eq!(GreetRequest::new("Ferris").name, "Ferris");
    /// ```
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

/// The reply from [`crate::names::methods::GREET`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct GreetResponse {
    /// The rendered greeting.
    pub greeting: String,
}

impl GreetResponse {
    /// Builds a reply carrying `greeting`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use template_bus::GreetResponse;
    /// assert_eq!(GreetResponse::new("Hello, Ferris!").greeting, "Hello, Ferris!");
    /// ```
    #[must_use]
    pub fn new(greeting: impl Into<String>) -> Self {
        Self {
            greeting: greeting.into(),
        }
    }
}
