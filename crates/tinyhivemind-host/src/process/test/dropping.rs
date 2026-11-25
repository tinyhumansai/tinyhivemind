//! What dropping an instance guarantees without awaiting anything.
use super::support::{Fixture, running, until};
use crate::process::Instance;
use anyhow::Result;

#[tokio::test]
async fn dropping_kills_the_process_group() {
    let fixture = Fixture::new()?;
    let instance = fixture
        .start("trap '' TERM; sh -c 'echo $$ > $OUT/pid; exec sleep 300' & while :; do sleep 0.01; done")
        .await?;
    let (root, grandchild) = (instance.pid(), fixture.pid("pid").await?);
    drop(instance);
    until(|| !running(root) && !running(grandchild)).await;
}

#[tokio::test]
async fn dropping_kills_the_process_group_itself_and_not_through_the_supervisor() {
    let fixture = Fixture::new()?;
    let mut instance = fixture.start("trap '' TERM; exec sleep 300").await?;
    let root = instance.pid();
    let (unheard, _) = std::sync::mpsc::channel();
    // The supervisor keeps waiting for requests and is never told.
    let supervisor = std::mem::replace(&mut instance.requests, unheard);
    drop(instance);
    until(|| !running(root)).await;
    drop(supervisor);
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn dropping_also_kills_an_orphan_in_its_own_session_and_cleans_up() {
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
}

#[test]
fn dropping_needs_no_runtime() -> Result<()> {
    let fixture = Fixture::new()?;
    let runtime = tokio::runtime::Runtime::new()?;
    let (instance, orphan): (Instance, u32) = runtime.block_on(async {
        let instance = fixture
            .start("setsid sh -c 'echo $$ > $OUT/pid; exec sleep 300' & exec sleep 300")
            .await?;
        let pid = fixture.pid("pid").await?;
        (instance, pid)
    });
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
async fn dropping_a_start_that_is_never_awaited_to_the_end_leaves_nothing() {
    let fixture = Fixture::new()?;
    let launch = fixture.shell("echo $$ > $OUT/pid; exec sleep 300")?;
    let started = tokio::spawn(Instance::start(launch));
    let root = fixture.pid("pid").await?;
    started.abort();
    drop(started.await);
    until(|| !running(root)).await;
}
