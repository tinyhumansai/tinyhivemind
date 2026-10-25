//! Located findings: the compiler-style report every rule writes into.
//!
//! Validation, parsing and [`crate::check()`] push each finding into a
//! private collector together with the file and JSON Pointer it concerns.
//! [`crate::validate`] and [`crate::parse`] return the first finding as an
//! [`crate::Error`]; [`crate::diagnose`], [`crate::diagnose_documents`] and
//! the checker return all of them as [`Diagnostic`]s.

mod types;
use crate::{Error, Result};
pub use types::{Diagnostic, Rule};

/// The canonical manifest file.
pub(crate) const HIVE: &str = "hive.json";
/// The canonical constitution file.
pub(crate) const CONSTITUTION: &str = "constitution.json";
/// The canonical memory file.
pub(crate) const MEMORY: &str = "memory.json";

/// One package error and where it was found.
#[derive(Debug)]
pub(crate) struct Finding {
    error: Error,
    file: String,
    pointer: Option<String>,
}

/// Findings in the order the rules produced them.
#[derive(Debug, Default)]
pub(crate) struct Findings(Vec<Finding>);

impl Findings {
    /// Record `error` against `file` at `pointer`.
    pub(crate) fn push(&mut self, error: Error, file: &str, pointer: impl Into<Option<String>>) {
        self.0.push(Finding {
            error,
            file: file.to_owned(),
            pointer: pointer.into(),
        });
    }

    /// Record the failure of `result`, if it failed; report whether it passed.
    pub(crate) fn check(
        &mut self,
        result: Result<()>,
        file: &str,
        pointer: impl Into<Option<String>>,
    ) -> bool {
        match result {
            Ok(()) => true,
            Err(error) => {
                self.push(error, file, pointer);
                false
            }
        }
    }

    /// Move each finding to the file `home` names for its file and
    /// pointer, if it names one.
    pub(crate) fn relocate(&mut self, home: impl Fn(&str, Option<&str>) -> Option<&'static str>) {
        for finding in &mut self.0 {
            if let Some(file) = home(&finding.file, finding.pointer.as_deref()) {
                file.clone_into(&mut finding.file);
            }
        }
    }

    /// The first finding, as the error fail-fast validation returns.
    pub(crate) fn into_first(self) -> Result<()> {
        self.0.into_iter().next().map_or(Ok(()), |f| Err(f.error))
    }

    /// Every finding as a diagnostic, without repeats, in production order.
    pub(crate) fn into_diagnostics(self) -> Vec<Diagnostic> {
        let mut diagnostics: Vec<Diagnostic> = Vec::with_capacity(self.0.len());
        for finding in self.0 {
            let diagnostic = Diagnostic {
                rule: Rule::from(&finding.error),
                message: finding.error.to_string(),
                file: finding.file,
                pointer: finding.pointer,
            };
            if !diagnostics.contains(&diagnostic) {
                diagnostics.push(diagnostic);
            }
        }
        diagnostics
    }
}

/// Escape one JSON Pointer reference token (RFC 6901).
pub(crate) fn escape(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}

#[cfg(test)]
mod test;
