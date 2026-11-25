//! A seat's random bearer, compared in constant time and never printed.
use crate::{Error, Result};
use std::fmt::{self, Write as _};
use subtle::ConstantTimeEq;

/// 32 random bytes as 64 lowercase hex characters.
pub(crate) struct Bearer(String);

impl Bearer {
    /// A fresh bearer from the operating system's randomness.
    pub(crate) fn generate() -> Result<Self> {
        let mut bytes = [0_u8; 32];
        getrandom::fill(&mut bytes).map_err(|error| Error::Random(error.to_string()))?;
        let mut text = String::with_capacity(64);
        for byte in bytes {
            let _ = write!(text, "{byte:02x}");
        }
        Ok(Self(text))
    }

    /// Whether `presented` is this bearer, in time that does not depend on
    /// where they differ.
    pub(crate) fn matches(&self, presented: &[u8]) -> bool {
        self.0.as_bytes().ct_eq(presented).into()
    }

    /// The secret itself, for the seat's endpoint and nothing else.
    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Bearer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Bearer(redacted)")
    }
}
