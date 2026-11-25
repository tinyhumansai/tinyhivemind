//! Environment, directories, pipes, readiness and exit.
use super::support::{Fixture, PATIENCE, exists, until};
use crate::process::{Containment, Error, Exit, Instance, Launch, StopPolicy, containment};
use anyhow::{Context as _, Result};
use serde_json::json;
use std::time::Duration;

#[test]
fn this_platform_says_what_stopping_covers() {
    let expected = if cfg!(target_os = "linux") {
        Containment::Tree
    } else {
        Containment::ProcessGroup
    };
    assert_eq!(containment(), expected);
}

#[tokio::test]
async fn the_environment_is_the_listed_one_and_nothing_inherited() -> Result<()> {
    let fixture = Fixture::new()?;
    let launch = Launch::new("/usr/bin/env", fixture.dir()?).env("GIVEN", "yes");
    let instance = Instance::start(launch).await?;
    instance.exited().await;
    let mut text = String::new();
    until(|| {
        text = instance.stdout();
        text.lines().count() >= 3
    })
    .await;
    let mut names: Vec<_> = text
        .lines()
        .filter_map(|line| line.split('=').next())
        .collect();
    names.sort_unstable();
    assert_eq!(names, ["GIVEN", "TINYHIVEMIND_INSTANCE", "TMPDIR"]);
    instance.stop(async {}, StopPolicy::default()).await?;
    Ok(())
}

#[tokio::test]
async fn the_child_runs_in_its_private_state_with_a_private_temporary_directory() -> Result<()> {
    let fixture = Fixture::new()?;
    let instance = fixture
        .start("pwd > $OUT/pwd; ls -ld $TMPDIR | cut -c1-10 > $OUT/mode; echo $TMPDIR > $OUT/tmp")
        .await?;
    assert!(instance.exited().await.success());
    let state = fixture.dir()?.state().canonicalize()?;
    assert_eq!(
        fixture.read("pwd").trim(),
        state.to_str().context("missing")?
    );
    assert_eq!(fixture.read("mode").trim(), "drwx------");
    let temporary = fixture.read("tmp").trim().to_owned();
    assert!(temporary.starts_with(fixture.dir()?.root().to_str().context("missing")?));
    assert!(exists(temporary.as_ref()));
    instance.stop(async {}, StopPolicy::default()).await?;
    assert!(!exists(temporary.as_ref()), "removed once the tree is gone");
    Ok(())
}

#[tokio::test]
async fn a_working_directory_can_be_named() -> Result<()> {
    let fixture = Fixture::new()?;
    let launch = fixture.shell("pwd > $OUT/pwd")?.current_dir("/");
    let instance = Instance::start(launch).await?;
    instance.exited().await;
    assert_eq!(fixture.read("pwd").trim(), "/");
    Ok(())
}

#[tokio::test]
async fn a_bad_launch_is_refused_before_anything_starts() -> Result<()> {
    let fixture = Fixture::new()?;
    for launch in [
        Launch::new("sh", fixture.dir()?),
        fixture.shell("")?.env("TMPDIR", "/tmp"),
        fixture.shell("")?.env("TINYHIVEMIND_INSTANCE", "x"),
        fixture.shell("")?.env("", "x"),
        fixture.shell("")?.env("A=B", "x"),
    ] {
        let refused = Instance::start(launch)
            .await
            .err()
            .context("expected a failure")?;
        assert!(matches!(refused, Error::Launch(_)), "{refused}");
    }
    let missing = Launch::new("/nonexistent/program", fixture.dir()?);
    let refused = Instance::start(missing)
        .await
        .err()
        .context("expected a failure")?;
    assert!(matches!(refused, Error::Spawn { .. }), "{refused}");
    // No temporary directory is left by a launch that failed.
    let left = std::fs::read_dir(fixture.dir()?.root())?.count();
    assert_eq!(left, 1, "only the state directory");
    Ok(())
}

#[tokio::test]
async fn debug_output_holds_no_environment_value_or_argument() -> Result<()> {
    let fixture = Fixture::new()?;
    let launch = fixture
        .shell("secret-argument")?
        .env("TOKEN", "secret-value");
    let text = format!("{launch:?}");
    assert!(!text.contains("secret"), "{text}");
    assert!(text.contains("TOKEN"));
    let instance = Instance::start(launch).await?;
    assert!(!format!("{instance:?}").contains("secret"));
    Ok(())
}

#[tokio::test]
async fn json_lines_flow_both_ways_and_stderr_is_kept_apart() -> Result<()> {
    let fixture = Fixture::new()?;
    let launch = fixture
        .shell("echo noise >&2; while read line; do echo \"{\\\"got\\\":$line}\"; done")?
        .json_lines();
    let mut instance = Instance::start(launch).await?;
    let mut lines = instance.take_lines().context("missing")?;
    assert!(instance.take_lines().is_none());
    lines.sender.send(&json!([1, "a"])).await?;
    let value = lines.receiver.recv().await.context("missing")??;
    assert_eq!(value, json!({"got": [1, "a"]}));
    until(|| instance.stderr() == "noise\n").await;
    assert_eq!(instance.stdout(), "");
    // Dropping the sender closes stdin, which ends this child.
    drop(lines);
    assert_eq!(instance.exited().await.code, Some(0));
    Ok(())
}

#[tokio::test]
async fn logs_are_bounded_and_always_drained() -> Result<()> {
    let fixture = Fixture::new()?;
    let launch = fixture
        .shell("i=0; while [ $i -lt 20000 ]; do echo line-$i-padding; i=$((i+1)); done >&2; echo end >&2")?
        .log_limit(1000);
    let instance = Instance::start(launch).await?;
    assert!(instance.exited().await.success());
    until(|| instance.stderr().ends_with("end\n")).await;
    assert_eq!(instance.stderr().len(), 1000);
    Ok(())
}

#[tokio::test]
async fn an_exit_is_observed_with_its_code_or_signal() -> Result<()> {
    let fixture = Fixture::new()?;
    let instance = fixture.start("exit 7").await?;
    let exit = Exit {
        code: Some(7),
        signal: None,
    };
    assert_eq!(instance.exited().await, exit);
    assert_eq!(instance.exit(), Some(exit));
    assert!(!exit.success());
    let instance = fixture.start("kill -9 $$").await?;
    let exit = instance.exited().await;
    assert_eq!((exit.code, exit.signal), (None, Some(9)));
    Ok(())
}

#[tokio::test]
async fn readiness_is_the_probe_unless_the_child_exits_or_the_deadline_passes() -> Result<()> {
    let fixture = Fixture::new()?;
    let instance = fixture.start("exec sleep 300").await?;
    let ready = instance.ready(async { Ok::<_, Error>(5) }, PATIENCE).await;
    assert_eq!(ready?, 5);
    let late = instance
        .ready(
            std::future::pending::<Result<(), Error>>(),
            Duration::from_millis(50),
        )
        .await;
    assert!(matches!(late, Err(Error::ReadyTimeout { .. })));
    let failed = instance
        .ready(async { Err::<(), _>("no handshake") }, PATIENCE)
        .await;
    assert!(matches!(failed, Err(Error::NotReady(reason)) if reason == "no handshake"));
    let dead = fixture.start("exit 3").await?;
    let early = dead
        .ready(std::future::pending::<Result<(), Error>>(), PATIENCE)
        .await;
    assert!(matches!(early, Err(Error::Exited(exit)) if exit.code == Some(3)));
    Ok(())
}
