//! Attribution over a fabricated process table, and signalling real ones.
use super::{Identity, Stat, Tree, signal};
use rustix::process::Signal;
use std::fs;
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
        }
    }
    fn write(&self, proc: &Path) {
        let directory = proc.join(self.pid.to_string());
        let task = directory.join("task").join(self.pid.to_string());
        fs::create_dir_all(&task).unwrap();
        // A name holding spaces and parentheses must not shift the fields.
        let stat = format!(
            "{} (a) b (c) {} {} {} {} 0 -1 0 0 0 0 0 0 0 0 0 20 0 {} 0 {} 0 0\n",
            self.pid, self.state, self.parent, self.group, self.group, self.threads, self.start
        );
        fs::write(directory.join("stat"), stat).unwrap();
        fs::write(directory.join("environ"), self.environ).unwrap();
    }
}

fn table(entries: &[Entry]) -> tempfile::TempDir {
    let proc = tempfile::tempdir().unwrap();
    for entry in entries {
        entry.write(proc.path());
    }
    // Entries that are not processes are skipped.
    fs::create_dir(proc.path().join("sys")).unwrap();
    proc
}

fn tree(proc: &tempfile::TempDir) -> Tree {
    Tree::at(proc.path(), MARKER, 10).unwrap()
}

fn pids(tree: &mut Tree) -> Vec<u32> {
    tree.scan()
        .unwrap()
        .into_iter()
        .map(|member| member.pid)
        .collect()
}

fn children(proc: &tempfile::TempDir, pid: u32, list: &str) {
    let task = format!("{pid}/task/{pid}/children");
    fs::write(proc.path().join(task), list).unwrap();
}

#[test]
fn a_stat_line_is_read_past_a_hostile_command_name() {
    let proc = table(&[Entry::new(10, 1, 100)]);
    let stat = Stat::read(proc.path(), 10).unwrap();
    assert_eq!((stat.parent, stat.group, stat.start), (1, 10, 100));
    assert!(!stat.dead);
    assert!(Stat::read(proc.path(), 11).is_none());
    fs::write(proc.path().join("10/stat"), "10 (x) S 1").unwrap();
    assert!(Stat::read(proc.path(), 10).is_none());
}

#[test]
fn a_root_that_is_not_running_has_no_tree() {
    let proc = table(&[]);
    assert!(Tree::at(proc.path(), MARKER, 10).is_err());
}

#[test]
fn descendants_are_members_however_deep() {
    let proc = table(&[
        Entry::new(13, 12, 103),
        Entry::new(12, 11, 102),
        Entry::new(11, 10, 101),
        Entry::new(10, 1, 100),
        Entry::new(20, 1, 104),
    ]);
    assert_eq!(pids(&mut tree(&proc)), [10, 11, 12, 13]);
}

#[test]
fn a_reparented_process_is_known_by_its_marker_alone() {
    let mut orphan = Entry::new(30, 1, 105);
    orphan.environ = "A=1\0TINYHIVEMIND_INSTANCE=abc\0";
    let mut other = Entry::new(31, 1, 105);
    other.environ = "TINYHIVEMIND_INSTANCE=abcd\0";
    let mut embedded = Entry::new(32, 1, 105);
    embedded.environ = "X=TINYHIVEMIND_INSTANCE=abc\0";
    let proc = table(&[Entry::new(10, 1, 100), orphan, other, embedded]);
    assert_eq!(pids(&mut tree(&proc)), [10, 30]);
}

#[test]
fn a_process_older_than_the_root_is_never_a_member() {
    let mut older = Entry::new(5, 1, 99);
    older.environ = "TINYHIVEMIND_INSTANCE=abc\0";
    let proc = table(&[Entry::new(10, 1, 100), older]);
    assert_eq!(pids(&mut tree(&proc)), [10]);
}

#[test]
fn the_roots_process_group_is_the_instances() {
    let mut grouped = Entry::new(40, 1, 105);
    grouped.group = 10;
    let proc = table(&[Entry::new(10, 1, 100), grouped]);
    assert_eq!(pids(&mut tree(&proc)), [10, 40]);
}

#[test]
fn the_dead_are_not_survivors_but_a_threaded_zombie_leader_is() {
    let mut zombie = Entry::new(11, 10, 101);
    zombie.state = 'Z';
    let mut leader = Entry::new(12, 10, 101);
    leader.state = 'Z';
    leader.threads = 3;
    let proc = table(&[Entry::new(10, 1, 100), zombie, leader]);
    assert_eq!(pids(&mut tree(&proc)), [10, 12]);
}

#[test]
fn a_sampled_descendant_stays_a_member_after_it_is_reparented() {
    let proc = table(&[Entry::new(10, 1, 100), Entry::new(11, 10, 101)]);
    Entry::new(12, 11, 102).write(proc.path());
    children(&proc, 10, "11 ");
    children(&proc, 11, "12 ");
    let mut tree = tree(&proc);
    tree.sample();
    fs::remove_dir_all(proc.path().join("11")).unwrap();
    Entry::new(12, 1, 102).write(proc.path());
    assert_eq!(pids(&mut tree), [10, 12]);
}

#[test]
fn a_reused_pid_is_not_the_process_that_had_it() {
    let proc = table(&[Entry::new(10, 1, 100), Entry::new(11, 10, 101)]);
    children(&proc, 10, "11");
    let mut tree = tree(&proc);
    tree.sample();
    Entry::new(11, 1, 900).write(proc.path());
    // A child of the new owner of the pid is not ours either.
    Entry::new(12, 11, 901).write(proc.path());
    assert_eq!(pids(&mut tree), [10]);
}

#[test]
fn a_stale_children_list_does_not_adopt_a_stranger() {
    let proc = table(&[Entry::new(10, 1, 100), Entry::new(11, 1, 101)]);
    children(&proc, 10, "11 999 junk");
    let mut tree = tree(&proc);
    tree.sample();
    assert_eq!(pids(&mut tree), [10]);
}

#[test]
fn an_unreadable_table_is_an_error_and_not_an_empty_tree() {
    let proc = table(&[Entry::new(10, 1, 100)]);
    let mut tree = tree(&proc);
    drop(proc);
    assert!(tree.scan().is_err());
}

/// Whether `member` is running.
fn alive(proc: &Path, member: Identity) -> bool {
    Stat::read(proc, member.pid).is_some_and(|stat| !stat.dead && stat.identity() == member)
}

/// The identity of a real process.
fn identity(pid: u32) -> Identity {
    Stat::read(Path::new("/proc"), pid).unwrap().identity()
}

#[test]
fn a_signal_reaches_the_process_it_names_and_no_later_owner_of_its_pid() {
    let proc = Path::new("/proc");
    let mut child = Command::new("/bin/sleep")
        .arg("300")
        .stdin(Stdio::null())
        .spawn()
        .unwrap();
    let real = identity(child.id());
    let stale = Identity {
        start: real.start + 1,
        ..real
    };
    signal(proc, stale, Signal::KILL).unwrap();
    assert!(alive(proc, real));
    assert!(!alive(proc, stale));
    assert!(child.try_wait().unwrap().is_none());
    signal(proc, real, Signal::KILL).unwrap();
    assert!(!child.wait().unwrap().success());
    assert!(!alive(proc, real));
    // Gone is not an error.
    signal(proc, real, Signal::KILL).unwrap();
}

#[test]
fn a_signal_the_kernel_refuses_is_an_error() {
    // Nothing unprivileged may signal init; root may, so it is skipped there.
    if rustix::process::geteuid().is_root() {
        return;
    }
    let refused = signal(Path::new("/proc"), identity(1), Signal::TERM);
    assert_eq!(refused.unwrap_err(), rustix::io::Errno::PERM);
}
