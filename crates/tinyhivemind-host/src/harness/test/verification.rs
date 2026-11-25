//! What an instance must be found to be before a seat runs on it.
use crate::harness::{Error, Expected, Mismatch};
use anyhow::{Context as _, Result};

fn expected() -> Expected {
    Expected::new(42, "0.64.15", ["post".into(), "ask".into()])
}

fn tools(names: &[&str]) -> Vec<String> {
    names.iter().map(ToString::to_string).collect()
}

#[test]
fn an_exact_instance_is_started() -> Result<()> {
    let started = expected().verify(42, "0.64.15", tools(&["ask", "post"]))?;
    assert_eq!((started.pid, started.version.as_str()), (42, "0.64.15"));
    assert_eq!(started.tools, ["ask".to_owned(), "post".to_owned()].into());
    Ok(())
}

#[test]
fn an_answer_from_another_process_is_refused() -> Result<()> {
    let mismatch = expected()
        .verify(43, "0.64.15", tools(&["ask", "post"]))
        .err()
        .context("expected a failure")?;
    assert_eq!(
        mismatch,
        Mismatch::Pid {
            expected: 42,
            found: 43
        }
    );
    Ok(())
}

#[test]
fn another_version_is_refused() -> Result<()> {
    let mismatch = expected()
        .verify(42, "0.64.16", tools(&["ask", "post"]))
        .err()
        .context("expected a failure")?;
    assert!(matches!(mismatch, Mismatch::Version { found, .. } if found == "0.64.16"));
    Ok(())
}

#[test]
fn a_missing_or_unexpected_or_repeated_tool_is_refused() -> Result<()> {
    let found = |names: &[&str]| {
        expected()
            .verify(42, "0.64.15", tools(names))
            .err()
            .context("expected a failure")?
    };
    for (names, missing, unexpected) in [
        (&["ask"][..], &["post"][..], &[][..]),
        (&["ask", "post", "shell"], &[], &["shell"]),
        (&["shell"], &["ask", "post"], &["shell"]),
        (&["ask", "post", "post"], &[], &["post"]),
    ] {
        let mismatch = found(names);
        assert_eq!(
            mismatch,
            Mismatch::Tools {
                missing: tools(missing),
                unexpected: tools(unexpected),
            }
        );
    }
    let error: Error = found(&["ask"]).into();
    assert!(error.to_string().contains("post"), "{error}");
    Ok(())
}
