//! Attribution over a fabricated process table, and signalling real ones.
use super::{Identity, Scan, Stat, Tree, signal};
use anyhow::{Context as _, Result};
use rustix::process::Signal;
use std::fs;
use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};

const MARKER: &str = "TINYHIVEMIND_INSTANCE=abc";

/// One fabricated `/proc/<pid>` entry.
struct Entry {
    pid: u32,
    parent: u32,
    group: u32,
    start: u64,
    state: char,
    threads: u64,
    environ: &'static str,
    /// The process this is a thread of: itself, usually.
    leader: u32,
    flags: u64,
}

impl Entry {
    fn new(pid: u32, parent: u32, start: u64) -> Self {
        Self {
            pid,
            parent,
            group: pid,
            start,
            state: 'S',
            threads: 1,
            environ: "PATH=/bin\0",
            leader: pid,
            flags: 0,
        }
    }
    fn write(&self, proc: &Path) -> Result<()> {
        let directory = proc.join(self.pid.to_string());
        let task = directory.join("task").join(self.pid.to_string());
        fs::create_dir_all(&task)?;
        // A name holding spaces and parentheses must not shift the fields.
        let stat = format!(
            "{} (a) b (c) {} {} {} {} 0 -1 {} 0 0 0 0 0 0 0 0 20 0 {} 0 {} 0 0\n",
            self.pid,
            self.state,
            self.parent,
            self.group,
            self.group,
            self.flags,
            self.threads,
            self.start
        );
        fs::write(directory.join("stat"), stat)?;
        let status = format!("Name:\tx\nTgid:\t{}\n", self.leader);
        fs::write(directory.join("status"), status)?;
        Ok(fs::write(directory.join("environ"), self.environ)?)
    }
}

fn table(entries: &[Entry]) -> Result<tempfile::TempDir> {
    let proc = tempfile::tempdir()?;
    for entry in entries {
        entry.write(proc.path())?;
    }
    // Entries that are not processes are skipped.
    fs::create_dir_all(proc.path().join("sys/kernel"))?;
    Ok(proc)
}

/// Have the kernel say which pid it handed out last.
fn last_pid(proc: &Path, pid: u32) -> Result<()> {
    Ok(fs::write(
        proc.join("sys/kernel/ns_last_pid"),
        format!("{pid}\n"),
    )?)
}

/// Look, without signalling anyone: the pids here are not this test's.
fn scan(tree: &mut Tree) -> Scan {
    let started = std::time::Instant::now();
    tree.scan(None, &|| started.elapsed().as_secs() >= 10)
}

fn tree(proc: &tempfile::TempDir) -> Result<Tree> {
    Ok(Tree::at(proc.path(), MARKER, 10)?)
}

fn pids(tree: &mut Tree) -> Result<Vec<u32>> {
    let scan = scan(tree);
    assert!(scan.failure.is_none() && scan.settled);
    Ok(scan.live.into_iter().map(|member| member.pid).collect())
}

fn children(proc: &tempfile::TempDir, pid: u32, list: &str) -> Result<()> {
    let task = format!("{pid}/task/{pid}/children");
    Ok(fs::write(proc.path().join(task), list)?)
}

#[test]
fn a_stat_line_is_read_past_a_hostile_command_name() -> Result<()> {
    let proc = table(&[Entry::new(10, 1, 100)])?;
    let stat = Stat::read(proc.path(), 10)?.context("missing")?;
    assert_eq!((stat.parent, stat.group, stat.start), (1, 10, 100));
    assert!(!stat.dead);
    assert!(Stat::read(proc.path(), 11)?.is_none());
    fs::write(proc.path().join("10/stat"), "10 (x) S 1")?;
    assert!(Stat::read(proc.path(), 10).is_err());
    Ok(())
}

#[test]
fn a_root_that_is_not_running_has_no_tree() -> Result<()> {
    let proc = table(&[])?;
    assert!(Tree::at(proc.path(), MARKER, 10).is_err());
    Ok(())
}

#[test]
fn descendants_are_members_however_deep() -> Result<()> {
    let proc = table(&[
        Entry::new(13, 12, 103),
        Entry::new(12, 11, 102),
        Entry::new(11, 10, 101),
        Entry::new(10, 1, 100),
        Entry::new(20, 1, 104),
    ])?;
    assert_eq!(pids(&mut tree(&proc)?)?, [10, 11, 12, 13]);
    Ok(())
}

#[test]
fn a_reparented_process_is_known_by_its_marker_alone() -> Result<()> {
    let mut orphan = Entry::new(30, 1, 105);
    orphan.environ = "A=1\0TINYHIVEMIND_INSTANCE=abc\0";
    let mut other = Entry::new(31, 1, 105);
    other.environ = "TINYHIVEMIND_INSTANCE=abcd\0";
    let mut embedded = Entry::new(32, 1, 105);
    embedded.environ = "X=TINYHIVEMIND_INSTANCE=abc\0";
    let proc = table(&[Entry::new(10, 1, 100), orphan, other, embedded])?;
    assert_eq!(pids(&mut tree(&proc)?)?, [10, 30]);
    Ok(())
}

#[test]
fn a_process_older_than_the_root_is_never_a_member() -> Result<()> {
    let mut older = Entry::new(5, 1, 99);
    older.environ = "TINYHIVEMIND_INSTANCE=abc\0";
    let proc = table(&[Entry::new(10, 1, 100), older])?;
    assert_eq!(pids(&mut tree(&proc)?)?, [10]);
    Ok(())
}

#[test]
fn the_roots_process_group_is_the_instances() -> Result<()> {
    let mut grouped = Entry::new(40, 1, 105);
    grouped.group = 10;
    let proc = table(&[Entry::new(10, 1, 100), grouped])?;
    assert_eq!(pids(&mut tree(&proc)?)?, [10, 40]);
    let mut released = tree(&proc)?;
    released.release();
    assert_eq!(pids(&mut released)?, [10]);
    Ok(())
}

#[test]
fn the_dead_are_not_survivors_but_a_threaded_zombie_leader_is() -> Result<()> {
    let mut zombie = Entry::new(11, 10, 101);
    zombie.state = 'Z';
    let mut leader = Entry::new(12, 10, 101);
    leader.state = 'Z';
    leader.threads = 3;
    let proc = table(&[Entry::new(10, 1, 100), zombie, leader])?;
    assert_eq!(pids(&mut tree(&proc)?)?, [10, 12]);
    Ok(())
}

#[test]
fn a_sampled_descendant_stays_a_member_after_it_is_reparented() -> Result<()> {
    let proc = table(&[Entry::new(10, 1, 100), Entry::new(11, 10, 101)])?;
    Entry::new(12, 11, 102).write(proc.path())?;
    children(&proc, 10, "11 ")?;
    children(&proc, 11, "12 ")?;
    let mut tree = tree(&proc)?;
    tree.sample();
    fs::remove_dir_all(proc.path().join("11"))?;
    Entry::new(12, 1, 102).write(proc.path())?;
    assert_eq!(pids(&mut tree)?, [10, 12]);
    Ok(())
}

#[test]
fn a_reused_pid_is_not_the_process_that_had_it() -> Result<()> {
    let proc = table(&[Entry::new(10, 1, 100), Entry::new(11, 10, 101)])?;
    children(&proc, 10, "11")?;
    let mut tree = tree(&proc)?;
    tree.sample();
    Entry::new(11, 1, 900).write(proc.path())?;
    // A child of the new owner of the pid is not ours either.
    Entry::new(12, 11, 901).write(proc.path())?;
    assert_eq!(pids(&mut tree)?, [10]);
    Ok(())
}

#[test]
fn a_stale_children_list_does_not_adopt_a_stranger() -> Result<()> {
    let proc = table(&[Entry::new(10, 1, 100), Entry::new(11, 1, 101)])?;
    children(&proc, 10, "11 999 junk")?;
    let mut tree = tree(&proc)?;
    tree.sample();
    assert_eq!(pids(&mut tree)?, [10]);
    Ok(())
}

#[test]
fn a_process_that_appears_during_a_scan_is_found_by_it() -> Result<()> {
    // Reading the bystander's entry waits for the thread below, which adds a
    // member first. One pass over a listing made before that misses it, as it
    // would miss a process that forks and exits faster than the table is read.
    let proc = table(&[Entry::new(10, 1, 100), Entry::new(20, 1, 101)])?;
    let pipe = proc.path().join("20/stat");
    let line = fs::read(&pipe)?;
    fs::remove_file(&pipe)?;
    assert!(Command::new("mkfifo").arg(&pipe).status()?.success());
    let table = proc.path().to_path_buf();
    let late = std::thread::spawn(move || -> Result<()> {
        let mut pipe = fs::File::options().write(true).open(pipe)?;
        let mut member = Entry::new(30, 1, 102);
        member.environ = "TINYHIVEMIND_INSTANCE=abc\0";
        member.write(&table)?;
        Ok(pipe.write_all(&line)?)
    });
    assert_eq!(pids(&mut tree(&proc)?)?, [10, 30]);
    assert!(late.join().is_ok_and(|added| added.is_ok()));
    Ok(())
}

#[test]
fn a_process_whose_environment_is_not_there_yet_is_looked_at_again() -> Result<()> {
    // As it is inside `exec`.
    let mut member = Entry::new(30, 1, 102);
    member.environ = "";
    let proc = table(&[Entry::new(10, 1, 100), member])?;
    let pipe = proc.path().join("30/environ");
    fs::remove_file(&pipe)?;
    assert!(Command::new("mkfifo").arg(&pipe).status()?.success());
    let late = std::thread::spawn(move || -> Result<()> {
        // What it was started with is there before the first look ends, which
        // sees nothing.
        let empty = fs::File::options().write(true).open(&pipe)?;
        let shown = pipe.with_extension("shown");
        fs::write(&shown, "TINYHIVEMIND_INSTANCE=abc\0")?;
        fs::rename(shown, pipe)?;
        drop(empty);
        Ok(())
    });
    assert_eq!(pids(&mut tree(&proc)?)?, [10, 30]);
    assert!(late.join().is_ok_and(|shown| shown.is_ok()));
    Ok(())
}

#[test]
fn a_process_without_an_environment_is_looked_at_only_so_often() -> Result<()> {
    let mut bare = Entry::new(30, 1, 102);
    bare.environ = "";
    let proc = table(&[Entry::new(10, 1, 100), bare])?;
    assert_eq!(pids(&mut tree(&proc)?)?, [10]);
    Ok(())
}

#[test]
fn an_unreadable_table_is_a_failure_and_not_an_empty_tree() -> Result<()> {
    let proc = table(&[Entry::new(10, 1, 100)])?;
    let mut tree = tree(&proc)?;
    drop(proc);
    let found = scan(&mut tree);
    assert!(found.failure.is_some() && !found.settled);
    Ok(())
}

/// Whether `member` is running.
fn alive(proc: &Path, member: Identity) -> bool {
    let stat = Stat::read(proc, member.pid).ok().flatten();
    stat.is_some_and(|stat| !stat.dead && stat.identity() == member)
}

/// The identity of a real process.
fn identity(pid: u32) -> Result<Identity> {
    let stat = Stat::read(Path::new("/proc"), pid)?;
    Ok(stat.context("no such process")?.identity())
}

#[test]
fn a_signal_reaches_the_process_it_names_and_no_later_owner_of_its_pid() -> Result<()> {
    let proc = Path::new("/proc");
    let mut child = Command::new("/bin/sleep")
        .arg("300")
        .stdin(Stdio::null())
        .spawn()?;
    let real = identity(child.id())?;
    let stale = Identity {
        start: real.start + 1,
        ..real
    };
    signal(proc, stale, Signal::KILL)?;
    assert!(alive(proc, real));
    assert!(!alive(proc, stale));
    assert!(child.try_wait()?.is_none());
    signal(proc, real, Signal::KILL)?;
    assert!(!child.wait()?.success());
    assert!(!alive(proc, real));
    // Gone is not an error.
    signal(proc, real, Signal::KILL)?;
    Ok(())
}

#[test]
fn a_signal_the_kernel_refuses_is_an_error() -> Result<()> {
    // Nothing unprivileged may signal init; root may, so it is skipped there.
    if rustix::process::geteuid().is_root() {
        return Ok(());
    }
    let refused = signal(Path::new("/proc"), identity(1)?, Signal::TERM);
    let refused = refused.err().context("init was signalled")?;
    assert_eq!(refused.kind(), std::io::ErrorKind::PermissionDenied);
    Ok(())
}
