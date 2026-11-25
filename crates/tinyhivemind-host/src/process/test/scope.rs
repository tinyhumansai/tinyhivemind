//! The child's own lifetime, apart from its supervisor.
use super::support::{Fixture, PATIENCE, running};
use crate::process::scope::Root;
use crate::process::{Containment, Instance, StopPolicy};
use std::os::unix::process::CommandExt;
use std::process::Command;
use std::time::Duration;

#[test]
fn a_root_nobody_released_is_killed_and_reaped_when_dropped() -> Result<()> {
    let mut command = Command::new("/bin/sleep");
    let child = command.arg("300").process_group(0).spawn()?;
    let root = Root::new(child);
    let pid = root.id();
    drop(root);
    // Reaped: not even a zombie is left.
    let found = Command::new("ps").args(["-p", &pid.to_string()]).output();
    assert!(!found?.status.success());
    Ok(())
}

#[tokio::test]
async fn asking_who_is_running_in_a_group_changes_nothing() -> Result<()> {
    let fixture = Fixture::new();
    let launch = fixture.shell("sh -c 'echo $$ > $OUT/pid; exec sleep 300' & exit 0");
    let instance = Instance::contained(launch, Containment::ProcessGroup)
        .await
        ?;
    let left = fixture.pid("pid").await;
    instance.exited().await;
    // A group is one entry: its id.
    assert_eq!(instance.members().await?, [instance.pid()]);
    assert!(running(left));
    instance
        .stop(async {}, super::support::SHORT)
        .await
        ?;
    assert!(!running(left));
    Ok(())
}

#[tokio::test]
async fn a_group_whose_leader_is_gone_is_not_given_the_rest_of_the_grace_period() -> Result<()> {
    let fixture = Fixture::new();
    let launch = fixture.shell("exec sleep 300");
    let instance = Instance::contained(launch, Containment::ProcessGroup)
        .await
        ?;
    let policy = StopPolicy {
        cancel: Duration::ZERO,
        terminate: Duration::MAX,
        kill: Duration::MAX,
    };
    let stopped = instance.stop(async {}, policy);
    let exit = tokio::time::timeout(PATIENCE, stopped).await??;
    assert_eq!(exit.signal, Some(15));
    Ok(())
}
