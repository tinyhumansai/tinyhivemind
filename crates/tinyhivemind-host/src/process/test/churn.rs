//! Stopping while processes come and go faster than the table can be read.
use super::support::{Fixture, PATIENCE, running, until};
use crate::process::scope::{Root, Scope};
use crate::process::supervisor::{Request, Supervisor};
use crate::process::tree::Tree;
use crate::process::{Error, Instance, StopPolicy};
use anyhow::Result;
use std::fs;
use std::os::unix::process::CommandExt as _;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};
use tokio::sync::{oneshot, watch};

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
    let deadline = Instant::now() + PATIENCE;
    while !instance.members().await?.contains(&orphan) {
        assert!(Instant::now() < deadline);
    }
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

#[tokio::test]
async fn a_table_that_cannot_be_read_is_an_error_after_the_group_was_killed() -> Result<()> {
    let mut command = Command::new("/bin/sleep");
    let root = Arc::new(Root::new(command.arg("300").process_group(0).spawn()?));
    let pid = root.id();
    // A table with this child in it, and an entry that is not a file.
    let fixture = Fixture::new()?;
    let entry = fixture.path(&pid.to_string());
    fs::create_dir_all(&entry)?;
    let line = format!("{pid} (sleep) S 1 0 0 0 -1 0 0 0 0 0 0 0 0 0 20 0 1 0 7 0 0\n");
    fs::write(entry.join("stat"), line)?;
    fs::create_dir_all(fixture.path("20/stat"))?;
    let (requests, received) = mpsc::channel();
    Supervisor {
        scope: Scope::Tree(Tree::at(&fixture.path(""), "MARKER=x", pid)?),
        root,
        requests: received,
        exit: watch::channel(None).0,
        hurry: Arc::default(),
        temporary: None,
    }
    .start()?;
    let (reply, members) = oneshot::channel();
    requests.send(Request::Members(reply))?;
    assert!(matches!(members.await?, Err(Error::Table(_))));
    let (reply, stopped) = oneshot::channel();
    let policy = StopPolicy {
        kill: Duration::from_millis(300),
        ..BOUNDS
    };
    requests.send(Request::Stop(policy, reply))?;
    let error = stopped.await?.err();
    assert!(
        matches!(error, Some(Error::Table(_) | Error::Survivors { .. })),
        "{error:?}"
    );
    assert!(!running(pid));
    Ok(())
}

#[tokio::test]
async fn a_child_the_host_reaped_is_reported_lost_and_its_tree_is_still_stopped() -> Result<()> {
    let fixture = Fixture::new()?;
    let mut command = Command::new("/bin/sh");
    command.args([
        "-c",
        "setsid sh -c 'echo $$ > $OUT/pid; exec sleep 300' & exec sleep 300",
    ]);
    command
        .env("OUT", fixture.path(""))
        .env("MARKED", fixture.path(""));
    let root = Arc::new(Root::new(command.process_group(0).spawn()?));
    let (pid, process) = (root.id(), root.pid());
    let orphan = fixture.pid("pid").await?;
    let marker = format!("MARKED={}", fixture.path("").display());
    let (requests, received) = mpsc::channel();
    let supervisor = Supervisor {
        scope: Scope::Tree(Tree::new(&marker, pid)?),
        root,
        requests: received,
        exit: watch::channel(None).0,
        hurry: Arc::default(),
        temporary: None,
    };
    // The host kills and reaps the child before its supervisor looks at all.
    rustix::process::kill_process(process, rustix::process::Signal::KILL)?;
    let exited = rustix::process::WaitIdOptions::EXITED;
    rustix::process::waitid(rustix::process::WaitId::Pid(process), exited)?;
    supervisor.start()?;
    let (reply, stopped) = oneshot::channel();
    requests.send(Request::Stop(BOUNDS, reply))?;
    let exit = stopped.await??;
    assert_eq!((exit.code, exit.signal), (None, None));
    assert!(!running(orphan));
    Ok(())
}

#[tokio::test]
async fn a_supervisor_that_is_stuck_does_not_hold_up_a_stop_past_its_bounds() -> Result<()> {
    // As reading the entry of a process that hangs in the kernel would.
    let fixture = Fixture::new()?;
    let mut instance = fixture.start("trap '' TERM; exec sleep 300").await?;
    let root = instance.pid();
    let (unheard, _kept) = mpsc::channel();
    let supervisor = std::mem::replace(&mut instance.requests, unheard);
    let policy = StopPolicy {
        terminate: Duration::from_millis(10),
        kill: Duration::from_millis(10),
        ..BOUNDS
    };
    let stopping = instance.stop(async {}, policy);
    let error = tokio::time::timeout(PATIENCE, stopping).await?.err();
    assert!(matches!(error, Some(Error::Unproven)), "{error:?}");
    // What can be done without it is.
    until(|| !running(root)).await;
    let members = tokio::time::timeout(PATIENCE, instance.members()).await?;
    assert!(matches!(members, Err(Error::Unproven)));
    drop(supervisor);
    Ok(())
}

#[test]
fn a_scan_that_is_stuck_does_not_hold_up_the_others() -> Result<()> {
    let stuck = std::thread::spawn(|| {
        let turn = crate::process::tree::turn(&|| false);
        std::thread::sleep(Duration::from_secs(6));
        drop(turn);
    });
    let child = Command::new("/bin/sleep")
        .arg("300")
        .process_group(0)
        .spawn()?;
    let root = Root::new(child);
    let mut tree = Tree::new("MARKER=x", root.id())?;
    let mut found = tree.scan(None, &|| stuck.is_finished());
    while !found.complete && !stuck.is_finished() {
        found = tree.scan(None, &|| stuck.is_finished());
    }
    assert!(
        found.complete,
        "nothing was found before the other was done"
    );
    assert_eq!(found.live.len(), 1);
    assert!(stuck.join().is_ok());
    Ok(())
}
