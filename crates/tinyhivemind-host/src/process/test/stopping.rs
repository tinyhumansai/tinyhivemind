//! Stopping covers the whole tree, and nothing outside it.
use super::support::{Fixture, PATIENCE, SHORT, running, until};
use crate::process::{Containment, Error, Instance, StopPolicy};
use std::time::Duration;

#[tokio::test]
async fn a_cooperative_child_is_stopped_by_the_cancel_future_alone() -> Result<()> {
    let fixture = Fixture::new();
    let instance = fixture
        .start("trap 'echo term > $OUT/term' TERM; until [ -e $OUT/cancel ]; do sleep 0.01; done")
        .await;
    let cancel = {
        let path = fixture.path("cancel");
        let exited = instance.exited();
        async move {
            std::fs::write(path, "")?;
            exited.await;
        }
    };
    let policy = StopPolicy {
        cancel: PATIENCE,
        ..StopPolicy::default()
    };
    let exit = instance.stop(cancel, policy).await?;
    assert!(exit.success());
    assert_eq!(fixture.read("term"), "");
    Ok(())
}

#[tokio::test]
async fn sigterm_is_tried_before_sigkill_and_lets_the_child_finish() -> Result<()> {
    let fixture = Fixture::new();
    let instance = fixture
        .start("trap 'echo bye > $OUT/bye; exit 4' TERM; echo $$ > $OUT/up; while :; do sleep 0.01; done")
        .await;
    fixture.pid("up").await;
    let policy = StopPolicy {
        terminate: PATIENCE,
        ..StopPolicy::default()
    };
    let exit = instance.stop(async {}, policy).await?;
    assert_eq!(exit.code, Some(4));
    assert_eq!(fixture.read("bye"), "bye\n");
    Ok(())
}

#[tokio::test]
async fn a_child_that_ignores_everything_is_killed_within_the_bounds() -> Result<()> {
    let fixture = Fixture::new();
    let instance = fixture
        .start("trap '' TERM; echo $$ > $OUT/up; while :; do sleep 0.01; done")
        .await;
    fixture.pid("up").await;
    let exit = instance.stop(std::future::pending(), SHORT).await?;
    assert_eq!(exit.signal, Some(9));
    assert!(!running(instance.pid()));
    Ok(())
}

#[tokio::test]
async fn cancelling_a_child_that_already_exited_is_not_waited_for() -> Result<()> {
    let fixture = Fixture::new();
    let instance = fixture.start("exit 0").await;
    instance.exited().await;
    let policy = StopPolicy {
        cancel: Duration::MAX,
        ..SHORT
    };
    let stopped = instance.stop(std::future::pending(), policy);
    tokio::time::timeout(PATIENCE, stopped).await??;
    Ok(())
}

#[tokio::test]
async fn stopping_twice_or_after_exit_gives_the_same_exit() -> Result<()> {
    let fixture = Fixture::new();
    let instance = fixture.start("exit 2").await;
    instance.exited().await;
    for _ in 0..2 {
        let exit = instance.stop(async {}, SHORT).await?;
        assert_eq!(exit.code, Some(2));
    }
    Ok(())
}

/// Reports its pid in `$OUT/pid` and stays.
const STAY: &str = "echo $$ > $OUT/pid; exec sleep 300";

/// Start `script`, in which `$OUT/stay` reports a grandchild, stop, and
/// require both to be gone.
async fn leaves_nothing(script: &str) -> Result<()> {
    let fixture = Fixture::new();
    fixture.script("stay", STAY);
    let instance = fixture.start(script).await;
    let grandchild = fixture.pid("pid").await;
    assert!(running(grandchild));
    instance.stop(async {}, SHORT).await?;
    assert!(!running(grandchild), "{script}");
    assert!(!running(instance.pid()));
    Ok(())
}

#[tokio::test]
async fn a_grandchild_in_its_own_session_is_stopped() {
    leaves_nothing("setsid sh $OUT/stay & exec sleep 300").await;
}

#[tokio::test]
async fn a_grandchild_whose_parent_died_is_stopped() {
    leaves_nothing("(sh $OUT/stay &); exec sleep 300").await;
}

#[tokio::test]
async fn an_orphan_in_its_own_session_is_stopped() {
    leaves_nothing("(setsid sh $OUT/stay &); exec sleep 300").await;
}

#[tokio::test]
async fn an_orphan_that_ignores_sigterm_is_stopped() {
    leaves_nothing("trap '' TERM; (setsid sh $OUT/stay &); exec sleep 300").await;
}

#[tokio::test]
async fn a_descendant_that_cleared_its_environment_is_stopped_while_its_parent_lives() {
    leaves_nothing("setsid env -i OUT=$OUT /bin/sh $OUT/stay & exec sleep 300").await;
}

#[tokio::test]
async fn an_orphan_left_by_a_root_that_already_exited_is_stopped() -> Result<()> {
    let fixture = Fixture::new();
    fixture.script("stay", STAY);
    let instance = fixture.start("setsid sh $OUT/stay & exit 1").await;
    let orphan = fixture.pid("pid").await;
    assert_eq!(instance.exited().await.code, Some(1));
    assert!(running(orphan));
    instance.stop(async {}, SHORT).await?;
    assert!(!running(orphan));
    Ok(())
}

#[tokio::test]
async fn a_descendant_seen_before_it_was_orphaned_is_stopped_though_it_cleared_its_environment() -> Result<()> {
    let fixture = Fixture::new();
    fixture.script("stay", STAY);
    // The middle process lives until the supervisor has seen its child.
    fixture.script(
        "middle",
        "setsid env -i OUT=$OUT /bin/sh $OUT/stay & until [ -e $OUT/go ]; do sleep 0.01; done",
    );
    let instance = fixture
        .start("sh $OUT/middle; echo 1 > $OUT/left; exec sleep 300")
        .await;
    let escapee = fixture.pid("pid").await;
    let deadline = tokio::time::Instant::now() + PATIENCE;
    while !instance.members().await?.contains(&escapee) {
        assert!(tokio::time::Instant::now() < deadline);
    }
    fixture.script("go", "");
    until(|| fixture.read("left") == "1\n").await;
    instance.stop(async {}, SHORT).await?;
    assert!(!running(escapee));
    Ok(())
}

/// Whether a descendant that cleared its environment is stopped, given `wait`
/// for the supervisor to notice it before its parent dies.
async fn noticed_within(wait: Duration) -> bool {
    let fixture = Fixture::new();
    fixture.script("stay", STAY);
    fixture.script(
        "middle",
        "setsid env -i OUT=$OUT /bin/sh $OUT/stay & until [ -e $OUT/go ]; do sleep 0.01; done",
    );
    let instance = fixture
        .start("sh $OUT/middle; echo 1 > $OUT/left; exec sleep 300")
        .await;
    let escapee = fixture.pid("pid").await;
    tokio::time::sleep(wait).await;
    fixture.script("go", "");
    until(|| fixture.read("left") == "1\n").await;
    instance.stop(async {}, SHORT).await?;
    let escaped = running(escapee);
    if escaped {
        let pid = rustix::process::Pid::from_raw(i32::try_from(escapee)?).context("missing")?;
        rustix::process::kill_process(pid, rustix::process::Signal::KILL)?;
    }
    !escaped
}

#[tokio::test]
async fn a_running_instance_is_sampled_without_being_asked() {
    // The wait is doubled rather than trusted: on a loaded machine the
    // supervisor may be late, but without sampling no wait is long enough.
    for tenths in [2, 4, 8, 16, 32, 64] {
        if noticed_within(Duration::from_millis(100 * tenths)).await {
            return;
        }
    }
    panic!("the tree was never sampled");
}

#[tokio::test]
async fn a_tree_that_keeps_forking_is_stopped() -> Result<()> {
    let fixture = Fixture::new();
    fixture.script(
        "chain",
        "echo $$ >> $OUT/pids; sleep 0.02; setsid sh $OUT/chain & exit",
    );
    let instance = fixture
        .start("trap '' TERM; for i in 1 2 3 4; do setsid sh $OUT/chain & done; exec sleep 300")
        .await;
    until(|| fixture.read("pids").lines().count() > 40).await;
    instance.stop(async {}, SHORT).await?;
    let after = fixture.read("pids");
    for pid in after.lines().filter_map(|line| line.parse().ok()).rev().take(10) {
        assert!(!running(pid));
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(fixture.read("pids"), after, "something is still forking");
    Ok(())
}

#[tokio::test]
async fn another_instance_and_the_hosts_own_children_are_left_alone() -> Result<()> {
    let fixture = Fixture::new();
    let other = Fixture::new();
    let script = "setsid sh -c 'echo $$ > $OUT/pid; exec sleep 300' & exec sleep 300";
    let stopped = fixture.start(script).await;
    let kept = other.start(script).await;
    let mut own = std::process::Command::new("/bin/sleep").arg("300").spawn()?;
    let bystander = other.pid("pid").await;
    fixture.pid("pid").await;
    stopped.stop(async {}, SHORT).await?;
    assert!(running(bystander) && running(kept.pid()));
    assert!(own.try_wait()?.is_none());
    own.kill()?;
    own.wait()?;
    kept.stop(async {}, SHORT).await?;
    Ok(())
}

#[tokio::test]
async fn a_tree_that_cannot_be_proven_gone_is_a_typed_error_and_can_be_retried() -> Result<()> {
    let fixture = Fixture::new();
    let instance = fixture
        .start("trap '' TERM; echo $$ > $OUT/up; while :; do sleep 0.01; done")
        .await;
    let root = fixture.pid("up").await;
    // No time at all to see the kill take effect.
    let policy = StopPolicy {
        cancel: Duration::ZERO,
        terminate: Duration::ZERO,
        kill: Duration::ZERO,
    };
    let error = instance.stop(async {}, policy).await.err().context("expected a failure")?;
    assert!(
        matches!(&error, Error::Survivors { pids } if pids.contains(&root)),
        "{error}"
    );
    instance.stop(async {}, SHORT).await?;
    assert!(!running(root));
    Ok(())
}

#[tokio::test]
async fn abandoning_a_stop_does_not_abandon_the_kill() {
    let fixture = Fixture::new();
    let instance = fixture
        .start("trap '' TERM; echo $$ > $OUT/up; while :; do sleep 0.01; done")
        .await;
    let root = fixture.pid("up").await;
    let gave_up = tokio::time::timeout(
        Duration::from_millis(20),
        instance.stop(async {}, SHORT),
    )
    .await;
    assert!(gave_up.is_err());
    until(|| !running(root)).await;
}

#[tokio::test]
async fn without_tree_containment_the_process_group_is_stopped_and_proven() -> Result<()> {
    let fixture = Fixture::new();
    let launch = fixture
        .shell("trap '' TERM; sh -c 'echo $$ > $OUT/pid; exec sleep 300' & while :; do sleep 0.01; done");
    let instance = Instance::contained(launch, Containment::ProcessGroup).await?;
    assert_eq!(instance.containment(), Containment::ProcessGroup);
    let grandchild = fixture.pid("pid").await;
    let exit = instance.stop(async {}, SHORT).await?;
    assert_eq!(exit.signal, Some(9));
    assert!(!running(grandchild));
    Ok(())
}

#[tokio::test]
async fn a_process_group_is_a_survivor_until_it_is_gone() -> Result<()> {
    let fixture = Fixture::new();
    let launch = fixture.shell("exec sleep 300");
    let instance = Instance::contained(launch, Containment::ProcessGroup).await?;
    let policy = StopPolicy {
        cancel: Duration::ZERO,
        terminate: Duration::ZERO,
        kill: Duration::ZERO,
    };
    let error = instance.stop(async {}, policy).await.err().context("expected a failure")?;
    assert!(matches!(error, Error::Survivors { pids } if pids == [instance.pid()]));
    Ok(())
}

#[tokio::test]
async fn a_child_the_host_reaped_is_reported_lost_and_its_tree_is_still_stopped() -> Result<()> {
    let fixture = Fixture::new();
    fixture.script("stay", STAY);
    let instance = fixture.start("setsid sh $OUT/stay & exit 0").await;
    let orphan = fixture.pid("pid").await;
    let root = rustix::process::Pid::from_raw(i32::try_from(instance.pid())?).context("missing")?;
    let options = rustix::process::WaitIdOptions::EXITED;
    rustix::process::waitid(rustix::process::WaitId::Pid(root), options)?;
    instance.exited().await;
    let exit = instance.stop(async {}, SHORT).await?;
    assert!(!exit.success() || exit.code == Some(0));
    assert!(!running(orphan));
    Ok(())
}
