//! What dropping an instance guarantees without awaiting anything.
use super::support::{Fixture, running, until};
use crate::process::{Instance, Launch};
use anyhow::Result;
use std::os::unix::fs::PermissionsExt as _;
use std::time::Duration;

#[tokio::test]
async fn dropping_kills_the_process_group() -> Result<()> {
    let fixture = Fixture::new()?;
    let instance = fixture
        .start("trap '' TERM; sh -c 'echo $$ > $OUT/pid; exec sleep 300' & while :; do sleep 0.01; done")
        .await?;
    let (root, grandchild) = (instance.pid(), fixture.pid("pid").await?);
    drop(instance);
    until(|| !running(root) && !running(grandchild)).await;
    Ok(())
}

#[tokio::test]
async fn dropping_kills_the_process_group_itself_and_not_through_the_supervisor() -> Result<()> {
    let fixture = Fixture::new()?;
    let mut instance = fixture.start("trap '' TERM; exec sleep 300").await?;
    let root = instance.pid();
    let (unheard, _) = std::sync::mpsc::channel();
    // The supervisor keeps waiting for requests and is never told.
    let supervisor = std::mem::replace(&mut instance.requests, unheard);
    drop(instance);
    until(|| !running(root)).await;
    drop(supervisor);
    Ok(())
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn dropping_also_kills_an_orphan_in_its_own_session_and_cleans_up() -> Result<()> {
    let fixture = Fixture::new()?;
    let instance = fixture
        .start("(setsid sh -c 'echo $$ > $OUT/pid; exec sleep 300' &); echo $TMPDIR > $OUT/tmp; exec sleep 300")
        .await?;
    let orphan = fixture.pid("pid").await?;
    until(|| fixture.read("tmp").ends_with('\n')).await;
    let temporary = fixture.read("tmp").trim().to_owned();
    drop(instance);
    until(|| !running(orphan)).await;
    until(|| !std::path::Path::new(&temporary).exists()).await;
    Ok(())
}

#[test]
fn dropping_needs_no_runtime() -> Result<()> {
    let fixture = Fixture::new()?;
    let runtime = tokio::runtime::Runtime::new()?;
    let (instance, orphan) = runtime.block_on(async {
        let instance = fixture
            .start("setsid sh -c 'echo $$ > $OUT/pid; exec sleep 300' & exec sleep 300")
            .await?;
        let pid = fixture.pid("pid").await?;
        anyhow::Ok((instance, pid))
    })?;
    drop(runtime);
    let root = instance.pid();
    drop(instance);
    let waiting = tokio::runtime::Runtime::new()?;
    waiting.block_on(async {
        until(|| !running(root)).await;
        if cfg!(target_os = "linux") {
            until(|| !running(orphan)).await;
        }
    });
    Ok(())
}

#[tokio::test]
async fn dropping_a_start_that_waits_for_its_program_leaves_nothing() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.script("program", "#!/bin/sh\necho $$ > $OUT/pid\n")?;
    let program = fixture.path("program");
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700))?;
    // A program open for writing is busy, and the start tries again.
    let busy = std::fs::File::options().append(true).open(&program)?;
    let launch = Launch::new(&program, fixture.dir()?).env("OUT", fixture.path(""));
    let mut starting = Box::pin(Instance::start(launch));
    let waited = tokio::time::timeout(Duration::from_millis(50), &mut starting).await;
    assert!(waited.is_err(), "the start was not under way");
    let temporaries = || -> Result<usize> {
        let entries = std::fs::read_dir(fixture.dir()?.root())?;
        let names = entries.flatten().map(|entry| entry.file_name());
        Ok(names
            .filter(|name| name.to_string_lossy().starts_with("tmp-"))
            .count())
    };
    assert_eq!(temporaries()?, 1);
    drop(starting);
    assert_eq!(temporaries()?, 0);
    drop(busy);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(fixture.read("pid"), "", "the program ran all the same");
    Ok(())
}
