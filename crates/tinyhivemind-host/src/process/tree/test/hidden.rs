//! What a process table does not show, and what that must not be taken for.
use super::*;
use crate::process::tree::in_use;
use std::os::unix::fs::PermissionsExt as _;

/// Root may read anything, so nothing can be hidden from it this way.
fn can_hide() -> bool {
    !rustix::process::geteuid().is_root()
}

/// Let this user see that the process is there and nothing else of it.
fn hide(proc: &Path, pid: u32) -> Result<()> {
    let entry = proc.join(pid.to_string());
    Ok(fs::set_permissions(
        entry,
        fs::Permissions::from_mode(0o000),
    )?)
}

#[test]
fn what_cannot_be_read_is_not_gone() -> Result<()> {
    // A directory cannot be read as a file, whoever tries.
    let broken = |proc: &Path, name: &str| -> Result<()> {
        fs::remove_file(proc.join(name))?;
        Ok(fs::create_dir(proc.join(name))?)
    };
    for name in ["20/stat", "20/environ", "30/status"] {
        for says_last in [true, false] {
            let proc = table(&[exited_root(), Entry::new(20, 1, 101), marked(30, 102)])?;
            if says_last {
                last_pid(proc.path(), 30)?;
            }
            let mut tree = tree(&proc)?;
            broken(proc.path(), name)?;
            let found = scan(&mut tree);
            assert!(found.failure.is_some() && !found.settled, "{name}");
            // The rest is found all the same.
            if name.starts_with("20") {
                assert_eq!(found.live.len(), 1, "{name}");
            }
        }
    }
    // A known member that cannot be read counts as running.
    let proc = table(&[Entry::new(10, 1, 100), Entry::new(11, 10, 101)])?;
    children(&proc, 10, "11")?;
    let mut tree = tree(&proc)?;
    tree.sample();
    broken(proc.path(), "11/stat")?;
    tree.sample();
    let found = scan(&mut tree);
    assert!(found.failure.is_some());
    assert_eq!(found.live.len(), 2);
    Ok(())
}

#[test]
fn an_environment_this_user_may_not_read_is_no_evidence() -> Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    // Root may read it.
    if rustix::process::geteuid().is_root() {
        return Ok(());
    }
    let proc = table(&[exited_root(), marked(30, 102)])?;
    let environ = proc.path().join("30/environ");
    fs::set_permissions(environ, fs::Permissions::from_mode(0o000))?;
    let found = scan(&mut tree(&proc)?);
    assert!(found.failure.is_none() && found.settled);
    Ok(())
}

#[test]
fn a_process_that_shows_nothing_does_not_excuse_its_children() -> Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    if rustix::process::geteuid().is_root() {
        return Ok(());
    }
    // A member that cannot be told for one, such as a set-uid program.
    let mut child = marked(30, 103);
    child.parent = 20;
    let proc = table(&[Entry::new(10, 1, 100), Entry::new(20, 1, 102), child])?;
    last_pid(proc.path(), 30)?;
    let environ = proc.path().join("20/environ");
    fs::set_permissions(environ, fs::Permissions::from_mode(0o000))?;
    assert_eq!(pids(&mut tree(&proc)?), [10, 30]);
    Ok(())
}

#[test]
fn another_users_process_that_shows_nothing_does_not_end_the_walk() -> Result<()> {
    if !can_hide() {
        return Ok(());
    }
    for says_last in [true, false] {
        // As with `hidepid=1`.
        let proc = table(&[exited_root(), Entry::new(20, 1, 101), marked(30, 102)])?;
        if says_last {
            last_pid(proc.path(), 30)?;
        }
        hide(proc.path(), 20)?;
        let found = scan(&mut tree(&proc)?);
        assert!(found.failure.is_none() && found.complete);
        assert_eq!(found.live.len(), 1);
        fs::remove_dir_all(proc.path().join("30"))?;
        assert!(scan(&mut tree(&proc)?).settled);
        fs::set_permissions(proc.path().join("20"), fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

#[test]
fn a_known_member_that_shows_nothing_any_more_is_still_a_failure() -> Result<()> {
    if !can_hide() {
        return Ok(());
    }
    let proc = table(&[Entry::new(10, 1, 100), Entry::new(11, 10, 101)])?;
    children(&proc, 10, "11")?;
    let mut tree = tree(&proc)?;
    tree.sample();
    hide(proc.path(), 11)?;
    let found = scan(&mut tree);
    fs::set_permissions(proc.path().join("11"), fs::Permissions::from_mode(0o700))?;
    assert!(found.failure.is_some() && !found.settled);
    assert_eq!(found.live.len(), 2);
    Ok(())
}

/// The host is 2, under 3, under 4, which has adopted a member, 30, as a
/// subreaper does. All but the member are older than the root.
fn adopted() -> Result<tempfile::TempDir> {
    let mut root = exited_root();
    root.parent = 2;
    let mut orphan = marked(30, 102);
    orphan.parent = 4;
    let proc = table(&[
        Entry::new(1, 0, 1),
        Entry::new(2, 3, 4),
        Entry::new(3, 4, 3),
        Entry::new(4, 1, 2),
        root,
        orphan,
    ])?;
    last_pid(proc.path(), 30)?;
    Ok(proc)
}

#[test]
fn an_orphan_a_visible_ancestor_adopted_is_found() -> Result<()> {
    let proc = adopted()?;
    assert_eq!(pids(&mut tree(&proc)?), [30]);
    Ok(())
}

#[test]
fn an_orphan_adopted_beyond_a_hidden_ancestor_is_found() -> Result<()> {
    // As with `hidepid=2`.
    let proc = adopted()?;
    fs::remove_dir_all(proc.path().join("3"))?;
    assert_eq!(pids(&mut tree(&proc)?), [30]);
    // And with `hidepid=1`.
    if can_hide() {
        let proc = adopted()?;
        hide(proc.path(), 3)?;
        let found = scan(&mut tree(&proc)?);
        fs::set_permissions(proc.path().join("3"), fs::Permissions::from_mode(0o700))?;
        assert!(found.failure.is_none() && found.live.len() == 1);
    }
    // And an ancestor that cannot be read at all.
    let proc = adopted()?;
    let path = proc.path().join("3/stat");
    fs::remove_file(&path)?;
    fs::create_dir(&path)?;
    assert_eq!(scan(&mut tree(&proc)?).live.len(), 1);
    Ok(())
}

#[test]
fn an_orphan_adopted_after_an_ancestor_died_during_the_walk_is_found() -> Result<()> {
    // While the entry of 2 is read, its parent dies, 2 goes to 4, and the pid
    // of the dead one goes to somebody who has nothing to do with either.
    let proc = adopted()?;
    let table = proc.path().to_path_buf();
    let path = table.join("2/stat");
    let before = fs::read(&path)?;
    let mut tree = tree(&proc)?;
    let late = interrupt(&path.clone(), move |mut pipe| {
        Entry::new(3, 1, 50).write(&table)?;
        fs::rename(&path, path.with_extension("pipe"))?;
        Entry::new(2, 4, 4).write(&table)?;
        Ok(pipe.write_all(&before)?)
    })?;
    assert_eq!(pids(&mut tree), [30]);
    assert!(late.join().is_ok_and(|died| died.is_ok()));
    Ok(())
}

/// The directory, whether the table says that `10` was started with it as its
/// `TMPDIR`, under the name it is given here.
fn used_as(named: impl Fn(&Path) -> std::path::PathBuf) -> Result<(bool, tempfile::TempDir)> {
    let proc = table(&[Entry::new(10, 1, 100)])?;
    let directory = tempfile::tempdir()?;
    let mut environ = b"A=1\0TMPDIR=".to_vec();
    environ.extend(named(directory.path()).as_os_str().as_encoded_bytes());
    fs::write(proc.path().join("10/environ"), environ)?;
    let used = in_use(proc.path(), directory.path())?;
    Ok((used, proc))
}

#[test]
fn a_directory_is_in_use_under_whatever_name_it_has() -> Result<()> {
    let links = tempfile::tempdir()?;
    let link = links.path().join("link");
    assert!(used_as(Path::to_path_buf)?.0);
    assert!(used_as(|directory| directory.join("."))?.0);
    let linked = used_as(|directory| {
        let _ = std::os::unix::fs::symlink(directory, &link);
        link.clone()
    });
    assert!(linked?.0);
    // Not it, or nothing that could be.
    let long = "x".repeat(5000);
    let loops = links.path().join("loop");
    std::os::unix::fs::symlink(&loops, &loops)?;
    let elsewhere = [
        links.path().to_path_buf(),
        "/etc/passwd/tmp".into(),
        "/none".into(),
    ];
    for other in elsewhere.into_iter().chain([loops, long.into(), "".into()]) {
        assert!(!used_as(|_| other.clone())?.0, "{}", other.display());
    }
    Ok(())
}

#[test]
fn whether_a_directory_is_in_use_is_unknown_if_a_process_cannot_be_read() -> Result<()> {
    let (used, proc) = used_as(|_| "/none".into())?;
    assert!(!used);
    let environ = proc.path().join("10/environ");
    // Gone, or not to be read by this user: no evidence.
    if can_hide() {
        fs::set_permissions(&environ, fs::Permissions::from_mode(0o000))?;
        assert!(!in_use(proc.path(), proc.path())?);
    }
    fs::remove_file(&environ)?;
    assert!(!in_use(proc.path(), proc.path())?);
    // Anything else is not.
    fs::create_dir(&environ)?;
    assert!(in_use(proc.path(), proc.path()).is_err());
    assert!(in_use(&proc.path().join("none"), proc.path()).is_err());
    Ok(())
}
