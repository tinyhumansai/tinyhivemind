//! The package runner checks, summarises and calls tools offline.
use super::*;

fn integrations() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../tinyhivemind-lang/examples/hives/integrations")
}

#[tokio::test]
async fn summarises_what_each_seat_runs() -> Result<()> {
    let out = Box::pin(main_with(Command {
        dir: integrations(),
        ..Command::default()
    }))
    .await?;
    assert!(
        out.contains("hive integrations (Release desk): 2 seats"),
        "{out}"
    );
    assert!(
        out.contains(
            "lead model=anthropic/claude-sonnet-4.5 tools=grep_repo,word_count hooks=no_force_push"
        ),
        "{out}"
    );
    assert!(out.contains("scout model=openai/gpt-5-mini"), "{out}");
    Ok(())
}

#[tokio::test]
async fn calls_a_tool_as_a_seat_and_honours_its_hooks() -> Result<()> {
    let out = Box::pin(main_with(parse(
        [
            integrations().display().to_string(),
            "--seat".into(),
            "scout".into(),
            "--call".into(),
            "word_count".into(),
            "--args".into(),
            r#"{"text":"one two three"}"#.into(),
        ]
        .into_iter(),
    )?))
    .await?;
    assert!(
        out.contains("\"words\": 3") || out.contains("\"words\":3"),
        "{out}"
    );
    // The lead's hook guards shell calls only, so its word count runs too.
    let out = Box::pin(main_with(parse(
        [
            integrations().display().to_string(),
            "--call".into(),
            "word_count".into(),
            "--args".into(),
            r#"{"text":"hello"}"#.into(),
        ]
        .into_iter(),
    )?))
    .await?;
    assert!(
        out.contains("\"words\": 1") || out.contains("\"words\":1"),
        "{out}"
    );
    Ok(())
}

#[test]
fn rejects_unknown_flags_and_missing_values() {
    assert!(parse(std::iter::empty()).is_err());
    assert!(parse(["dir".into(), "--nope".into(), "x".into()].into_iter()).is_err());
    assert!(parse(["dir".into(), "--seat".into()].into_iter()).is_err());
}
