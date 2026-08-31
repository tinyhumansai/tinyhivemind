//! Greeting behavior used to demonstrate the template's module layout.
//!
//! A module root like this one documents the module, wires its pieces
//! together, and exposes the smallest useful API. Substantial type definitions
//! belong in a sibling `types.rs`, and unit tests belong in `test.rs`, wired in
//! at the bottom of this file.
//!
//! Replace this module with the crate's first real feature area.

use crate::{Error, Result};

/// Returns a friendly greeting for `name`.
///
/// Surrounding whitespace is trimmed before the greeting is built.
///
/// # Examples
///
/// ```
/// # use template::greet;
/// assert_eq!(greet("  Ferris  ")?, "Hello, Ferris!");
/// # Ok::<(), template::Error>(())
/// ```
///
/// # Errors
///
/// Returns [`Error::EmptyName`] when `name` is empty or contains only
/// whitespace.
pub fn greet(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::EmptyName);
    }

    Ok(format!("Hello, {name}!"))
}

#[cfg(test)]
mod test;
