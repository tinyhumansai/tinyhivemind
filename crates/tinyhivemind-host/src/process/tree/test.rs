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

fn pids(tree: &mut Tree) -> Vec<u32> {
    let scan = scan(tree);
    assert!(scan.failure.is_none() && scan.complete);
    scan.live.into_iter().map(|member| member.pid).collect()
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
    // A task the kernel is letting go of has no group left.
    fs::write(
        proc.path().join("10/stat"),
        "10 (x) X 0 -1 -1 0 -1 0 0 0 0 0 0 0 0 0 20 0 0 0 100 0\n",
    )?;
    assert!(Stat::read(proc.path(), 10)?.is_none());
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
    assert_eq!(pids(&mut tree(&proc)?), [10, 11, 12, 13]);
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
    assert_eq!(pids(&mut tree(&proc)?), [10, 30]);
    Ok(())
}

#[test]
fn a_process_older_than_the_root_is_never_a_member() -> Result<()> {
    let mut older = Entry::new(5, 1, 99);
    older.environ = "TINYHIVEMIND_INSTANCE=abc\0";
    let proc = table(&[Entry::new(10, 1, 100), older])?;
    assert_eq!(pids(&mut tree(&proc)?), [10]);
    Ok(())
}

#[test]
fn the_roots_process_group_is_the_instances() -> Result<()> {
    let mut grouped = Entry::new(40, 1, 105);
    grouped.group = 10;
    let proc = table(&[Entry::new(10, 1, 100), grouped])?;
    assert_eq!(pids(&mut tree(&proc)?), [10, 40]);
    let mut released = tree(&proc)?;
    released.release();
    assert_eq!(pids(&mut released), [10]);
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
    assert_eq!(pids(&mut tree(&proc)?), [10, 12]);
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
    assert_eq!(pids(&mut tree), [10, 12]);
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
    assert_eq!(pids(&mut tree), [10]);
    Ok(())
}

#[test]
fn a_stale_children_list_does_not_adopt_a_stranger() -> Result<()> {
    let proc = table(&[Entry::new(10, 1, 100), Entry::new(11, 1, 101)])?;
    children(&proc, 10, "11 999 junk")?;
    let mut tree = tree(&proc)?;
    tree.sample();
    assert_eq!(pids(&mut tree), [10]);
    Ok(())
}

/// Have reading the entry at `path` wait for `then`, which gets to write what
/// is read.
fn interrupt(
    path: &Path,
    then: impl FnOnce(fs::File) -> Result<()> + Send + 'static,
) -> Result<std::thread::JoinHandle<Result<()>>> {
    fs::remove_file(path)?;
    assert!(Command::new("mkfifo").arg(path).status()?.success());
    let path = path.to_path_buf();
    Ok(std::thread::spawn(move || {
        then(fs::File::options().write(true).open(path)?)
    }))
}

/// Answer whoever reads the entry at `path` with `line`, and later readers too.
fn answer(path: &Path, mut pipe: fs::File, line: &[u8]) -> Result<()> {
    // Replaced before it is answered: a later reader must not find the pipe.
    let plain = path.with_extension("plain");
    fs::write(&plain, line)?;
    fs::rename(plain, path)?;
    Ok(pipe.write_all(line)?)
}

fn marked(pid: u32, start: u64) -> Entry {
    let mut member = Entry::new(pid, 1, start);
    member.environ = "TINYHIVEMIND_INSTANCE=abc\0";
    member
}

/// A member appears while the scan reads the entry of a bystander. One pass
/// over a listing made before that misses it, as it would miss a process that
/// forks and exits faster than the table is read.
fn appears_during_a_scan(says_last: bool) -> Result<()> {
    let proc = table(&[Entry::new(10, 1, 100), Entry::new(20, 1, 101)])?;
    if says_last {
        last_pid(proc.path(), 20)?;
    }
    let line = fs::read(proc.path().join("20/stat"))?;
    let table = proc.path().to_path_buf();
    let late = interrupt(&proc.path().join("20/stat"), move |pipe| {
        marked(30, 102).write(&table)?;
        if says_last {
            last_pid(&table, 30)?;
        }
        answer(&table.join("20/stat"), pipe, &line)
    })?;
    assert_eq!(pids(&mut tree(&proc)?), [10, 30]);
    assert!(late.join().is_ok_and(|added| added.is_ok()));
    Ok(())
}

#[test]
fn a_process_that_appears_during_a_scan_is_found_by_it() -> Result<()> {
    appears_during_a_scan(true)
}

#[test]
fn so_it_is_where_the_kernel_does_not_say_which_pid_was_last() -> Result<()> {
    appears_during_a_scan(false)
}

#[test]
fn a_scan_in_which_the_pids_wrapped_around_is_not_complete() -> Result<()> {
    let proc = table(&[Entry::new(10, 1, 100)])?;
    last_pid(proc.path(), 2000)?;
    let table = proc.path().to_path_buf();
    let late = add_interrupted(proc.path(), (20, 0), move || {
        marked(5, 102).write(&table)?;
        last_pid(&table, 5)
    })?;
    let mut tree = tree(&proc)?;
    assert!(!scan(&mut tree).complete);
    assert!(late.join().is_ok_and(|added| added.is_ok()));
    assert_eq!(pids(&mut tree), [5, 10]);
    Ok(())
}

#[test]
fn a_process_still_being_forked_during_the_walk_is_found() -> Result<()> {
    // It has its pid already, and an entry only once the table was listed.
    let proc = table(&[Entry::new(10, 1, 100)])?;
    let table = proc.path().to_path_buf();
    let late = add_interrupted(proc.path(), (20, 20), move || marked(15, 102).write(&table))?;
    assert_eq!(pids(&mut tree(&proc)?), [10, 15]);
    assert!(late.join().is_ok_and(|added| added.is_ok()));
    Ok(())
}

#[test]
fn a_child_of_the_hosts_line_is_judged_for_itself() -> Result<()> {
    // 2 is the host: members' orphans may come to it or to those above it.
    let mut root = exited_root();
    root.parent = 2;
    let mut orphans = [marked(30, 102), marked(31, 102)];
    orphans[0].parent = 2;
    orphans[1].parent = 1;
    let [first, second] = orphans;
    let proc = table(&[
        Entry::new(1, 0, 1),
        Entry::new(2, 1, 2),
        root,
        first,
        second,
    ])?;
    last_pid(proc.path(), 31)?;
    assert_eq!(pids(&mut tree(&proc)?), [30, 31]);
    Ok(())
}

/// The root has exited, so that nobody may be left.
fn exited_root() -> Entry {
    let mut root = Entry::new(10, 1, 100);
    root.state = 'Z';
    root
}

#[test]
fn only_an_empty_scan_that_missed_nobody_is_settled() -> Result<()> {
    let proc = table(&[exited_root(), Entry::new(20, 1, 101)])?;
    last_pid(proc.path(), 20)?;
    assert!(scan(&mut tree(&proc)?).settled);
    marked(30, 102).write(proc.path())?;
    let found = scan(&mut tree(&proc)?);
    assert!(!found.settled && found.live.len() == 1);
    // Out of time before it began.
    let hasty = tree(&proc)?.scan(None, &|| true);
    assert!(!hasty.settled && hasty.failure.is_none());
    Ok(())
}

#[test]
fn a_scan_out_of_time_still_reports_the_members_known_before() -> Result<()> {
    let proc = table(&[Entry::new(10, 1, 100), Entry::new(11, 10, 101)])?;
    children(&proc, 10, "11")?;
    let mut tree = tree(&proc)?;
    tree.sample();
    let hasty = tree.scan(None, &|| true);
    assert!(!hasty.settled);
    assert_eq!(hasty.live.len(), 2);
    Ok(())
}

#[test]
fn a_pid_that_was_gone_too_soon_is_followed_up_and_does_not_stay_in_the_way() -> Result<()> {
    // Handed out after the walk began, and never seen.
    let proc = table(&[exited_root(), Entry::new(20, 1, 101)])?;
    last_pid(proc.path(), 20)?;
    let line = fs::read(proc.path().join("20/stat"))?;
    let table = proc.path().to_path_buf();
    let late = interrupt(&proc.path().join("20/stat"), move |pipe| {
        last_pid(&table, 40)?;
        answer(&table.join("20/stat"), pipe, &line)
    })?;
    let found = scan(&mut tree(&proc)?);
    assert!(found.settled && found.live.is_empty());
    assert!(late.join().is_ok_and(|added| added.is_ok()));
    Ok(())
}

/// Add `pid`, a bystander whose entry is being read when `then` happens, and
/// the pids up to `last`, none of which is there.
fn add_interrupted(
    table: &Path,
    (pid, last): (u32, u32),
    then: impl FnOnce() -> Result<()> + Send + 'static,
) -> Result<std::thread::JoinHandle<Result<()>>> {
    Entry::new(pid, 1, 101).write(table)?;
    let path = table.join(format!("{pid}/stat"));
    let line = fs::read(&path)?;
    let late = interrupt(&path.clone(), move |pipe| {
        then()?;
        answer(&path, pipe, &line)
    })?;
    if last > 0 {
        last_pid(table, last)?;
    }
    Ok(late)
}

#[test]
fn a_round_in_which_a_pid_was_gone_is_not_the_last() -> Result<()> {
    let proc = table(&[exited_root()])?;
    let table = proc.path().to_path_buf();
    last_pid(&table, 10)?;
    // While the walk reads 20, 40 is handed out; while the round after it
    // reads 40, 60 and 61 are, and 61 is gone at once. Its child, 80, is a
    // member, and is handed out while the next round reads 60.
    let (second, third) = (table.clone(), table.clone());
    let late = add_interrupted(&table, (20, 0), move || {
        add_interrupted(&second, (40, 40), move || {
            let fourth = third.clone();
            add_interrupted(&third, (60, 61), move || {
                marked(80, 102).write(&fourth)?;
                last_pid(&fourth, 80)
            })?;
            Ok(())
        })?;
        Ok(())
    })?;
    let found = scan(&mut tree(&proc)?);
    assert!(late.join().is_ok_and(|added| added.is_ok()));
    assert!(!found.settled);
    let pids: Vec<u32> = found.live.iter().map(|member| member.pid).collect();
    assert_eq!(pids, [80]);
    Ok(())
}

#[test]
fn a_zombie_of_another_process_is_nobodys_orphan() -> Result<()> {
    // 20 is neither the host, 2, nor above it.
    let mut root = exited_root();
    root.parent = 2;
    let mut zombie = Entry::new(21, 20, 103);
    zombie.state = 'Z';
    let mut own = Entry::new(22, 2, 103);
    own.state = 'Z';
    let proc = table(&[
        Entry::new(1, 0, 1),
        Entry::new(2, 1, 2),
        root,
        Entry::new(20, 1, 102),
        zombie,
        own,
    ])?;
    last_pid(proc.path(), 22)?;
    assert!(scan(&mut tree(&proc)?).settled);
    Ok(())
}

#[test]
fn a_process_whose_environment_is_not_there_yet_is_looked_at_again() -> Result<()> {
    // As it is inside `exec`.
    for says_last in [true, false] {
        let mut member = marked(30, 102);
        member.environ = "";
        let proc = table(&[Entry::new(10, 1, 100), member])?;
        if says_last {
            last_pid(proc.path(), 30)?;
        }
        let path = proc.path().join("30/environ");
        let late = interrupt(&path.clone(), move |empty| {
            // What it was started with is there before the first look ends,
            // which sees nothing.
            let shown = path.with_extension("shown");
            fs::write(&shown, "TINYHIVEMIND_INSTANCE=abc\0")?;
            fs::rename(shown, path)?;
            drop(empty);
            Ok(())
        })?;
        assert_eq!(pids(&mut tree(&proc)?), [10, 30]);
        assert!(late.join().is_ok_and(|shown| shown.is_ok()));
    }
    Ok(())
}

#[test]
fn a_process_without_an_environment_is_waited_for_once_unless_it_is_anothers() -> Result<()> {
    let mut bare = Entry::new(30, 1, 102);
    bare.environ = "";
    let proc = table(&[exited_root(), bare])?;
    last_pid(proc.path(), 30)?;
    let mut tree = tree(&proc)?;
    let started = std::time::Instant::now();
    assert!(scan(&mut tree).settled);
    assert!(started.elapsed() >= super::scan::BARE);
    // Of another process, of the kernel, or older than the root: no wait.
    let mut child = Entry::new(31, 20, 103);
    child.environ = "";
    let mut kernel = Entry::new(32, 1, 103);
    kernel.environ = "";
    kernel.flags = 0x0020_8040;
    let mut older = Entry::new(33, 1, 9);
    older.environ = "";
    for entry in [&Entry::new(20, 1, 102), &child, &kernel, &older] {
        entry.write(proc.path())?;
    }
    last_pid(proc.path(), 33)?;
    assert!(scan(&mut tree).settled);
    assert_eq!(tree.bare.len(), 1, "only the first was ever in doubt");
    tree.sample();
    fs::remove_dir_all(proc.path().join("30"))?;
    tree.sample();
    assert!(tree.bare.is_empty());
    Ok(())
}

#[test]
fn a_thread_of_a_member_is_not_another_one() -> Result<()> {
    let mut thread = marked(31, 102);
    thread.leader = 30;
    let proc = table(&[Entry::new(10, 1, 100), marked(30, 102), thread])?;
    assert_eq!(pids(&mut tree(&proc)?), [10, 30]);
    Ok(())
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
fn a_member_is_signalled_as_soon_as_it_is_found_and_before_the_scan_ends() -> Result<()> {
    // Every pid here is this test's own child, as signals are real.
    let spawn = || Command::new("/bin/sleep").arg("300").spawn();
    let (mut root, mut found) = (spawn()?, spawn()?);
    let proc = table(&[Entry::new(root.id(), 1, 100)])?;
    let real = |child: &std::process::Child| -> Result<Entry> {
        let mut entry = marked(child.id(), identity(child.id())?.start);
        entry.group = 0;
        Ok(entry)
    };
    let mut first = real(&root)?;
    first.environ = "";
    first.write(proc.path())?;
    real(&found)?.write(proc.path())?;
    let mut tree = Tree::at(proc.path(), MARKER, root.id())?;
    // Time runs out once the member has been sent its signal.
    let pid = identity(found.id())?;
    let scan = tree.scan(Some(Signal::KILL), &|| !alive(Path::new("/proc"), pid));
    assert!(!scan.settled);
    assert!(!found.wait()?.success() && !root.wait()?.success());
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
