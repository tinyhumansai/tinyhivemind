//! What an instance must be found to be before a seat runs on it.
use crate::harness::{Error, Expected, Mismatch, Observed, Started};
use anyhow::{Context as _, Result};
use serde_json::json;

fn expected() -> Expected {
    Expected::new(42, "0.64.15")
        .tools(tools(&["post", "ask"]))
        .setting(
            "provider",
            json!({"url": "http://127.0.0.1:1", "key": "s3cret"}),
        )
        .setting("memory", json!(false))
}

fn observed() -> Observed {
    Observed::new(42, "0.64.15")
        .tools(tools(&["ask", "post"]))
        .setting("memory", json!(false))
        .setting(
            "provider",
            json!({"key": "s3cret", "url": "http://127.0.0.1:1"}),
        )
}

fn tools(names: &[&str]) -> Vec<String> {
    names.iter().map(ToString::to_string).collect()
}

fn refused(observed: &Observed) -> Result<Mismatch> {
    let found: Result<Started, Mismatch> = expected().verify(observed);
    found.err().context("a wrong instance was accepted")
}

#[test]
fn an_exact_instance_is_started() -> Result<()> {
    let started = expected().verify(&observed())?;
    assert_eq!((started.pid, started.version.as_str()), (42, "0.64.15"));
    assert_eq!(started.tools, ["ask".to_owned(), "post".to_owned()].into());
    let settings = ["memory".to_owned(), "provider".to_owned()];
    assert_eq!(started.settings, settings.into());
    Ok(())
}

#[test]
fn an_answer_from_another_process_is_refused() -> Result<()> {
    let mismatch = refused(&Observed::new(43, "0.64.15"))?;
    let expected = Mismatch::Pid {
        expected: 42,
        found: 43,
    };
    assert_eq!(mismatch, expected);
    Ok(())
}

#[test]
fn another_version_is_refused() -> Result<()> {
    let mismatch = refused(&Observed::new(42, "0.64.16"))?;
    assert!(matches!(mismatch, Mismatch::Version { found, .. } if found == "0.64.16"));
    Ok(())
}

#[test]
fn a_missing_or_unexpected_or_repeated_tool_is_refused() -> Result<()> {
    for (names, missing, unexpected) in [
        (&["ask"][..], &["post"][..], &[][..]),
        (&["ask", "post", "shell"], &[], &["shell"]),
        (&["shell"], &["ask", "post"], &["shell"]),
        (&["ask", "post", "post"], &[], &["post"]),
    ] {
        let mismatch = refused(&observed().tools([]).tools(tools(names)))?;
        let expected = Mismatch::Tools {
            missing: tools(missing),
            unexpected: tools(unexpected),
        };
        assert_eq!(mismatch, expected);
        assert!(Error::from(mismatch).to_string().contains("tools"));
    }
    Ok(())
}

#[test]
fn a_config_that_is_not_the_one_written_is_refused() -> Result<()> {
    let settings = |missing: &[&str], different: &[&str], unexpected: &[&str]| Mismatch::Settings {
        missing: tools(missing),
        different: tools(different),
        unexpected: tools(unexpected),
    };
    let bare = Observed::new(42, "0.64.15").tools(tools(&["ask", "post"]));
    assert_eq!(refused(&bare)?, settings(&["memory", "provider"], &[], &[]));
    // As when a harness resets a bad file to its defaults.
    let reset = observed().setting("memory", json!(true));
    assert_eq!(refused(&reset)?, settings(&[], &["memory"], &[]));
    // As when a harness has a route nobody gave it.
    let more = observed().setting("hosted", json!("https://example.invalid"));
    assert_eq!(refused(&more)?, settings(&[], &[], &["hosted"]));
    Ok(())
}

#[test]
fn no_value_of_a_setting_is_ever_shown() -> Result<()> {
    let other = observed().setting("provider", json!("0ther-s3cret"));
    let mismatch = refused(&other)?;
    let started = expected().verify(&observed())?;
    let shown = format!(
        "{:?} {:#?} {:?} {:?} {mismatch:?} {mismatch} {started:?} {}",
        expected(),
        expected(),
        observed(),
        other,
        serde_json::to_string(&started)?
    );
    assert!(shown.contains("provider"));
    assert!(
        !shown.contains("s3cret") && !shown.contains("127.0.0.1"),
        "{shown}"
    );
    Ok(())
}
