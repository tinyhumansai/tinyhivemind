//! Stopping while processes come and go faster than the table can be read.
use super::support::{Fixture, running, until};
use crate::process::{Instance, StopPolicy};
use anyhow::Result;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Shells that start one process after another and belong to no instance.
struct Forkers(Vec<Child>);

impl Forkers {
    fn start(count: usize) -> Result<Self> {
        let mut forkers = Self(Vec::new());
        for _ in 0..count {
            let mut shell = Command::new("/bin/sh");
            shell.args(["-c", "while :; do /bin/true; done"]);
            forkers.0.push(shell.stdin(Stdio::null()).spawn()?);
        }
        Ok(forkers)
    }
    fn unharmed(&mut self) -> Result<bool> {
        for shell in &mut self.0 {
            if shell.try_wait()?.is_some() {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

impl Drop for Forkers {
    fn drop(&mut self) {
        for shell in &mut self.0 {
            let _ = shell.kill();
            let _ = shell.wait();
        }
    }
}

const BOUNDS: StopPolicy = StopPolicy {
    cancel: Duration::ZERO,
    terminate: Duration::from_millis(200),
    kill: Duration::from_secs(10),
};

/// An instance with an orphan in its own session that ignores SIGTERM.
async fn stubborn(fixture: &Fixture) -> Result<(Instance, u32)> {
    fixture.script("stay", "echo $$ > $OUT/pid; exec sleep 300")?;
    let instance = fixture
        .start("trap '' TERM; (setsid sh $OUT/stay &); exec sleep 300")
        .await?;
    let orphan = fixture.pid("pid").await?;
    Ok((instance, orphan))
}

#[tokio::test]
async fn an_instance_that_forks_without_pause_is_stopped_and_proven() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.script("chain", "echo $$ >> $OUT/pids; setsid sh $OUT/chain & exit")?;
    let instance = fixture
        .start("trap '' TERM; for i in 1 2 3 4; do setsid sh $OUT/chain & done; exec sleep 300")
        .await?;
    until(|| fixture.read("pids").lines().count() > 200).await;
    instance.stop(async {}, BOUNDS).await?;
    let after = fixture.read("pids");
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(fixture.read("pids") == after, "something is still forking");
    Ok(())
}

#[tokio::test]
async fn stopping_is_proven_while_unrelated_processes_come_and_go() -> Result<()> {
    let mut forkers = Forkers::start(16)?;
    for _ in 0..5 {
        let fixture = Fixture::new()?;
        let (instance, orphan) = stubborn(&fixture).await?;
        instance.stop(async {}, BOUNDS).await?;
        assert!(!running(orphan) && !running(instance.pid()));
    }
    assert!(forkers.unharmed()?);
    Ok(())
}

#[tokio::test]
async fn a_bound_too_short_for_proof_is_kept_and_the_kill_still_happens() -> Result<()> {
    let _forkers = Forkers::start(16)?;
    let fixture = Fixture::new()?;
    let (instance, orphan) = stubborn(&fixture).await?;
    instance.members().await?;
    let policy = StopPolicy {
        terminate: Duration::ZERO,
        kill: Duration::ZERO,
        ..BOUNDS
    };
    let started = Instant::now();
    assert!(instance.stop(async {}, policy).await.is_err());
    assert!(started.elapsed() < Duration::from_secs(5));
    // Already signalled: nothing else is asked of the instance.
    until(|| !running(orphan) && !running(instance.pid())).await;
    Ok(())
}
