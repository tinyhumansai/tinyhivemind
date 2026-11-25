//! Private instance directories.
use super::support::{Fixture, SHORT, exists, until};
use crate::process::{Error, InstanceDir};
use anyhow::{Context as _, Result};
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};

fn mode(path: &std::path::Path) -> Result<u32> {
    Ok(fs::metadata(path)?.permissions().mode() & 0o777)
}

#[test]
fn both_directories_are_private_and_survive_being_reopened() -> Result<()> {
    let fixture = Fixture::new()?;
    let dir = fixture.dir()?;
    assert_eq!(mode(dir.root())?, 0o700);
    assert_eq!(mode(dir.state())?, 0o700);
    fs::write(dir.state().join("session"), "s1")?;
    let again = fixture.dir()?;
    assert_eq!(fs::read_to_string(again.state().join("session"))?, "s1");
    Ok(())
}

#[test]
fn an_existing_open_directory_is_made_private() -> Result<()> {
    let fixture = Fixture::new()?;
    let root = fixture.path("instance");
    fs::create_dir(&root)?;
    fs::set_permissions(&root, fs::Permissions::from_mode(0o755))?;
    fixture.dir()?;
    assert_eq!(mode(&root)?, 0o700);
    Ok(())
}

#[test]
fn a_symlink_or_a_file_is_not_an_instance_directory() -> Result<()> {
    let fixture = Fixture::new()?;
    fs::create_dir(fixture.path("elsewhere"))?;
    symlink(fixture.path("elsewhere"), fixture.path("link"))?;
    fs::write(fixture.path("file"), "")?;
    for name in ["link", "file"] {
        let refused = InstanceDir::create(fixture.path(name))
            .err()
            .context("expected a failure")?;
        assert!(matches!(refused, Error::NotPrivate { .. }), "{refused}");
    }
    Ok(())
}

#[test]
fn another_users_directory_is_refused() -> Result<()> {
    if rustix::process::geteuid().is_root() {
        return Ok(());
    }
    let refused = InstanceDir::create("/usr")
        .err()
        .context("expected a failure")?;
    assert!(matches!(refused, Error::NotPrivate { .. }));
    Ok(())
}

#[test]
fn a_relative_path_or_a_missing_parent_is_refused() -> Result<()> {
    let refused = InstanceDir::create("relative")
        .err()
        .context("expected a failure")?;
    assert!(matches!(refused, Error::Launch(_)));
    let fixture = Fixture::new()?;
    let refused = InstanceDir::create(fixture.path("none/instance"))
        .err()
        .context("expected a failure")?;
    assert!(matches!(refused, Error::Directory { .. }));
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn what_a_crashed_host_left_is_removed_when_the_instance_is_opened_again() -> Result<()> {
    let fixture = Fixture::new()?;
    let root = fixture.dir()?.root().to_path_buf();
    fs::create_dir_all(root.join("tmp-0badf00d/deep"))?;
    fs::write(root.join("tmp-0badf00d/deep/file"), "x")?;
    // None of these is one of ours.
    fs::create_dir(root.join("tmp-mine"))?;
    fs::create_dir(root.join("tmp-0badf00d0"))?;
    fs::write(root.join("tmp-12345678"), "a file")?;
    fs::create_dir(fixture.path("elsewhere"))?;
    symlink(fixture.path("elsewhere"), root.join("tmp-87654321"))?;
    fs::write(fixture.dir()?.state().join("session"), "s")?;
    fixture.dir()?;
    assert!(!exists(&root.join("tmp-0badf00d")));
    for kept in [
        "tmp-mine",
        "tmp-0badf00d0",
        "tmp-12345678",
        "tmp-87654321",
        "state/session",
    ] {
        assert!(exists(&root.join(kept)), "{kept}");
    }
    assert!(exists(&fixture.path("elsewhere")));
    Ok(())
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn nothing_is_removed_under_a_running_instance_of_the_same_directory() -> Result<()> {
    let fixture = Fixture::new()?;
    let instance = fixture
        .start("echo $TMPDIR > $OUT/tmp; exec sleep 300")
        .await?;
    until(|| fixture.read("tmp").ends_with('\n')).await;
    let temporary = fixture.read("tmp").trim().to_owned();
    // Not even what nobody uses, as that cannot be told for certain.
    let root = fixture.dir()?.root().to_path_buf();
    fs::create_dir(root.join("tmp-0badf00d"))?;
    fixture.dir()?;
    assert!(exists(temporary.as_ref()) && exists(&root.join("tmp-0badf00d")));
    instance.stop(async {}, SHORT).await?;
    fixture.dir()?;
    assert!(!exists(temporary.as_ref()) && !exists(&root.join("tmp-0badf00d")));
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn nothing_is_removed_that_a_process_of_a_crashed_host_still_uses() -> Result<()> {
    let fixture = Fixture::new()?;
    let temporary = fixture.dir()?.root().join("tmp-0badf00d");
    fs::create_dir(&temporary)?;
    let mut left = std::process::Command::new("/bin/sleep")
        .arg("300")
        .env("TMPDIR", &temporary)
        .spawn()?;
    fixture.dir()?;
    assert!(exists(&temporary));
    left.kill()?;
    left.wait()?;
    fixture.dir()?;
    assert!(!exists(&temporary));
    Ok(())
}
