//! Private instance directories.
use super::support::Fixture;
use crate::process::{Error, InstanceDir};
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};

fn mode(path: &std::path::Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[test]
fn both_directories_are_private_and_survive_being_reopened() {
    let fixture = Fixture::new();
    let dir = fixture.dir();
    assert_eq!(mode(dir.root()), 0o700);
    assert_eq!(mode(dir.state()), 0o700);
    fs::write(dir.state().join("session"), "s1").unwrap();
    let again = fixture.dir();
    assert_eq!(
        fs::read_to_string(again.state().join("session")).unwrap(),
        "s1"
    );
}

#[test]
fn an_existing_open_directory_is_made_private() {
    let fixture = Fixture::new();
    let root = fixture.path("instance");
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();
    fixture.dir();
    assert_eq!(mode(&root), 0o700);
}

#[test]
fn a_symlink_or_a_file_is_not_an_instance_directory() {
    let fixture = Fixture::new();
    fs::create_dir(fixture.path("elsewhere")).unwrap();
    symlink(fixture.path("elsewhere"), fixture.path("link")).unwrap();
    fs::write(fixture.path("file"), "").unwrap();
    for name in ["link", "file"] {
        let refused = InstanceDir::create(fixture.path(name)).unwrap_err();
        assert!(matches!(refused, Error::NotPrivate { .. }), "{refused}");
    }
}

#[test]
fn another_users_directory_is_refused() {
    if rustix::process::geteuid().is_root() {
        return;
    }
    let refused = InstanceDir::create("/usr").unwrap_err();
    assert!(matches!(refused, Error::NotPrivate { .. }));
}

#[test]
fn a_relative_path_or_a_missing_parent_is_refused() {
    let refused = InstanceDir::create("relative").unwrap_err();
    assert!(matches!(refused, Error::Launch(_)));
    let fixture = Fixture::new();
    let refused = InstanceDir::create(fixture.path("none/instance")).unwrap_err();
    assert!(matches!(refused, Error::Directory { .. }));
}
