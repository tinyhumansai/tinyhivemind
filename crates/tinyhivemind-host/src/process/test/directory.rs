//! Private instance directories.
use super::support::Fixture;
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
