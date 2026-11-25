//! The child's own lifetime, apart from its supervisor.
use super::support::{Fixture, running};
use crate::process::scope::Root;
use crate::process::{Containment, Instance};
use std::os::unix::process::CommandExt;
use std::process::Command;

#[test]
fn a_root_nobody_released_is_killed_and_reaped_when_dropped() {
    let mut command = Command::new("/bin/sleep");
    let child = command.arg("300").process_group(0).spawn().unwrap();
    let root = Root::new(child);
    let pid = root.id();
    drop(root);
    // Reaped: not even a zombie is left.
    let found = Command::new("ps").args(["-p", &pid.to_string()]).output();
    assert!(!found.unwrap().status.success());
}

#[tokio::test]
async fn asking_who_is_running_in_a_group_changes_nothing() {
    let fixture = Fixture::new();
    let launch = fixture.shell("sh -c 'echo $$ > $OUT/pid; exec sleep 300' & exit 0");
    let instance = Instance::contained(launch, Containment::ProcessGroup)
        .await
        .unwrap();
    let left = fixture.pid("pid").await;
    instance.exited().await;
    // A group is one entry: its id.
    assert_eq!(instance.members().await.unwrap(), [instance.pid()]);
    assert!(running(left));
    instance
        .stop(async {}, super::support::SHORT)
        .await
        .unwrap();
    assert!(!running(left));
}
