//! Finding every member, and knowing when there is none.
//!
//! The process table cannot be read at one instant, so a process that forks
//! and exits could stay ahead of the reader for ever. A scan follows it. Pids
//! are handed out in ascending order, and the kernel says which was last, so
//! after one walk of the whole table a scan looks at just the pids handed out
//! since, over and over. Each such round takes microseconds.
//!
//! **When a scan is settled.** Consider a member alive at the end, and its
//! ancestors back to one that was alive when the scan began. That first one
//! was looked at: if it was alive, the scan found a member. If it was dead, it
//! had forked the next one before it died, so the next one was there when it
//! was looked at in turn, its pid being the higher; and so on, until one is
//! found alive. The argument holds as long as every pid handed out is looked
//! at, in order. It fails only for pids looked at too late, after their owner
//! died and left a child whose pid is not among those looked at yet. So a
//! round is final if none of its pids turned out to be gone, now or when one
//! that was unsure is looked at again. Other processes coming and going do not
//! prevent that, unless one comes and goes within every single round: each is
//! found alive and plainly someone else's, or a zombie whose parent plainly
//! is.
use super::{Evidence, Identity, Stat, Tree, present};
use rustix::process::Signal;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::time::{Duration, Instant};

/// Pids before the last one that are looked at again after the first walk: a
/// process being forked has its pid before it can be listed.
const FORKING: u32 = 1024;
/// More new pids than this are cheaper to list than to try one by one.
const STORM: u32 = 50_000;
/// How long a process may show no environment before that counts as having
/// none. Inside `exec` it lasts microseconds, if nothing else has the CPU.
const BARE: Duration = Duration::from_millis(200);
/// Rounds that wait for what they are unsure of. More are not needed: they wait
/// for at most [`BARE`].
const ROUNDS: usize = 256;
/// The pause when a round has nothing new and only waits.
const IDLE: Duration = Duration::from_millis(1);

/// What one scan found.
pub(in crate::process) struct Scan {
    /// The members alive when they were looked at.
    pub(in crate::process) live: Vec<Identity>,
    /// Whether nobody was alive, for certain.
    pub(in crate::process) settled: bool,
    /// Why it could not settle, if something could not be read.
    pub(in crate::process) failure: Option<io::Error>,
}

/// What one look at a pid came to.
enum Judged {
    /// A member, or not one.
    Known,
    /// Bare: look again.
    Unsure,
    /// Nobody is there any more to judge.
    Gone,
}

/// What a scan has seen so far.
#[derive(Default)]
struct Pass {
    /// Every pid that needs no further look.
    seen: BTreeSet<u32>,
    /// Looked at and not a member. None becomes one.
    others: BTreeMap<u32, u64>,
    /// The host and its ancestors.
    line: BTreeSet<u32>,
    /// Others that excused a child since the last confirmation.
    parents: BTreeMap<u32, u64>,
    /// Members, and everything that may still turn out to descend from one.
    stats: Vec<Stat>,
    members: BTreeMap<u32, Identity>,
    /// Whether a member was alive when it was looked at.
    alive: bool,
    /// Bare, to be looked at again.
    unsure: BTreeSet<u32>,
    /// Of each round that may be final, whom it is still unsure of.
    rounds: Vec<BTreeSet<u32>>,
    failure: Option<io::Error>,
}

impl Pass {
    /// Whether `stat` is the child of a process that is not a member and
    /// cannot have adopted one. Orphans go to an ancestor, so a child of one
    /// of the host's own line may be a member's. To be confirmed.
    fn is_anothers_child(&mut self, stat: &Stat) -> bool {
        if self.line.contains(&stat.parent) {
            return false;
        }
        let parent = self.others.get(&stat.parent);
        self.parents
            .extend(parent.map(|start| (stat.parent, *start)));
        parent.is_some()
    }
}

impl Tree {
    /// Find the members, signalling each with `order` the moment it is found,
    /// until the result is settled or `expired`.
    pub(in crate::process) fn scan(
        &mut self,
        order: Option<Signal>,
        expired: &dyn Fn() -> bool,
    ) -> Scan {
        let mut pass = Pass::default();
        let mut settled = false;
        if order.is_some() {
            // A signalled parent no longer shows whose its children are.
            self.sample();
        }
        self.recall(&mut pass, order);
        match self.walk(&mut pass, order, expired) {
            Ok(done) => settled = done,
            Err(error) => pass.failure = Some(error),
        }
        self.adopt(&mut pass, order);
        self.known.extend(pass.members.values().copied());
        let members = pass.members.values().copied();
        Scan {
            live: members.filter(|member| self.runs(*member)).collect(),
            // One that was alive may have forked unseen before it was killed.
            settled: settled && !pass.alive && pass.failure.is_none(),
            failure: pass.failure,
        }
    }

    /// Whether `member` runs, as far as can be told.
    fn runs(&self, member: Identity) -> bool {
        match Stat::read(&self.proc, member.pid) {
            Ok(Some(stat)) => !stat.dead && stat.identity() == member,
            Ok(None) => false,
            Err(_) => true,
        }
    }

    /// Start with the members known already, whatever comes of the walk.
    fn recall(&mut self, pass: &mut Pass, order: Option<Signal>) {
        for member in self.known.clone() {
            match Stat::read(&self.proc, member.pid) {
                Ok(Some(stat)) if stat.identity() == member => {
                    self.found(pass, stat, order);
                }
                Ok(_) => {
                    self.known.remove(&member);
                }
                Err(error) => pass.failure = Some(error),
            }
        }
    }

    /// Look at the whole table, then at what is new until nothing matters.
    fn walk(
        &mut self,
        pass: &mut Pass,
        order: Option<Signal>,
        expired: &dyn Fn() -> bool,
    ) -> io::Result<bool> {
        pass.line = self.line()?;
        let mut last = self.last_pid()?;
        self.look(pass, self.listed(pass)?, order, expired);
        // These may have been looked at before their owner was there to see.
        let mut window = FORKING;
        while !expired() && (pass.failure.is_none() || window > 0) {
            let now = self.last_pid()?;
            let (fresh, whole) = match (last, now) {
                (Some(last), Some(now)) if last <= now && now - last <= STORM => {
                    let from = last.saturating_sub(window) + 1;
                    let fresh = (from..=now).filter(|pid| !pass.seen.contains(pid));
                    (fresh.collect(), window == 0)
                }
                (None, None) => (self.listed(pass)?, true),
                // Started over from the lowest pid, or too far ahead.
                _ => (self.listed(pass)?, false),
            };
            window = if whole || window > 0 { 0 } else { FORKING };
            last = now;
            let idle = fresh.is_empty();
            let unsure = pass.unsure.iter().copied().collect();
            self.look(pass, unsure, order, expired);
            let gone = self.look(pass, fresh, order, expired) || !self.confirm(pass);
            if whole && !gone && pass.rounds.len() < ROUNDS {
                pass.rounds.push(pass.unsure.clone());
            }
            if pass.rounds.iter().any(BTreeSet::is_empty) {
                return Ok(true);
            }
            if idle {
                std::thread::sleep(IDLE);
            }
        }
        Ok(false)
    }

    /// The last pid handed out, if the kernel says.
    fn last_pid(&self) -> io::Result<Option<u32>> {
        let path = self.proc.join("sys/kernel/ns_last_pid");
        let last = present(fs::read_to_string(path))?;
        Ok(last.and_then(|last| last.trim().parse().ok()))
    }

    /// The listed pids that need a look, in ascending order.
    fn listed(&self, pass: &Pass) -> io::Result<Vec<u32>> {
        let mut pids = Vec::new();
        for entry in fs::read_dir(&self.proc)? {
            let name = entry?.file_name();
            let pid = name.to_str().and_then(|name| name.parse().ok());
            pids.extend(pid.filter(|pid| !pass.seen.contains(pid)));
        }
        pids.sort_unstable();
        Ok(pids)
    }

    /// Look at `pids`, and say whether any was gone too soon to judge.
    fn look(
        &mut self,
        pass: &mut Pass,
        pids: Vec<u32>,
        order: Option<Signal>,
        expired: &dyn Fn() -> bool,
    ) -> bool {
        let mut gone = false;
        for pid in pids {
            if expired() {
                return true;
            }
            match self.judge(pass, pid, order) {
                Ok(Judged::Known) => {
                    pass.unsure.remove(&pid);
                    for round in &mut pass.rounds {
                        round.remove(&pid);
                    }
                }
                Ok(Judged::Unsure) => {
                    pass.unsure.insert(pid);
                }
                Ok(Judged::Gone) => {
                    gone = true;
                    pass.unsure.remove(&pid);
                    pass.rounds.retain(|round| !round.contains(&pid));
                }
                // What cannot be read is not gone.
                Err(error) => pass.failure = Some(error),
            }
        }
        gone
    }

    /// Look at one process.
    fn judge(&mut self, pass: &mut Pass, pid: u32, order: Option<Signal>) -> io::Result<Judged> {
        let Some(stat) = Stat::read(&self.proc, pid)? else {
            return Ok(Judged::Gone);
        };
        if let Some(parent) = pass.members.get(&stat.parent)
            && self.is_child(&stat, *parent)?
        {
            self.found(pass, stat, order);
            return Ok(Judged::Known);
        }
        if !self.is_known(&stat) && pass.is_anothers_child(&stat) {
            pass.seen.insert(pid);
            pass.others.insert(pid, stat.start);
            return Ok(Judged::Known);
        }
        match self.evidence(&stat)? {
            Evidence::Member => {
                self.found(pass, stat, order);
                return Ok(Judged::Known);
            }
            Evidence::Gone => {
                // A zombie stays one. Its children may be anyone's.
                if stat.dead {
                    pass.seen.insert(pid);
                }
                return Ok(Judged::Gone);
            }
            Evidence::Bare => {
                let bare = self.bare.entry(stat.identity());
                let since = *bare.or_insert_with(Instant::now);
                if since.elapsed() < BARE {
                    return Ok(Judged::Unsure);
                }
            }
            Evidence::None => {}
        }
        pass.seen.insert(pid);
        pass.others.insert(pid, stat.start);
        if stat.start >= self.root.start {
            pass.stats.push(stat);
        }
        Ok(Judged::Known)
    }

    /// Whether the parents that excused their children are still the processes
    /// they were taken for. Only what is confirmed afterwards was so then.
    fn confirm(&self, pass: &mut Pass) -> bool {
        std::mem::take(&mut pass.parents)
            .into_iter()
            .all(
                |(pid, start)| match Stat::is_now(&self.proc, Identity { pid, start }) {
                    Ok(same) => same,
                    Err(error) => {
                        pass.failure = Some(error);
                        false
                    }
                },
            )
    }

    /// The host and its ancestors.
    fn line(&self) -> io::Result<BTreeSet<u32>> {
        let mut line = BTreeSet::from([0, 1]);
        let mut next = self.host;
        while line.insert(next) {
            next = match Stat::read(&self.proc, next)? {
                Some(stat) => stat.parent,
                None => 1,
            };
        }
        Ok(line)
    }

    /// Add a member, and signal it.
    fn found(&mut self, pass: &mut Pass, stat: Stat, order: Option<Signal>) {
        let member = stat.identity();
        pass.seen.insert(stat.pid);
        pass.others.remove(&stat.pid);
        pass.members.insert(stat.pid, member);
        pass.alive |= !stat.dead;
        if let Some(order) = order
            && !stat.dead
        {
            if self.known.insert(member) {
                self.sample_under(vec![member]);
            }
            self.signal(member, order);
        }
        pass.stats.push(stat);
    }

    /// Add the descendants of members among those seen.
    fn adopt(&mut self, pass: &mut Pass, order: Option<Signal>) {
        // Until nothing is added: the table is not ordered by descent.
        loop {
            let added: Vec<Identity> = pass
                .stats
                .iter()
                .filter(|stat| !pass.members.contains_key(&stat.pid))
                .filter(|stat| {
                    pass.members.get(&stat.parent).is_some_and(|parent| {
                        let now = Stat::read(&self.proc, stat.pid).ok().flatten();
                        now.is_some_and(|now| {
                            now.start == stat.start && self.is_child(&now, *parent).unwrap_or(false)
                        })
                    })
                })
                .map(Stat::identity)
                .collect();
            if added.is_empty() {
                return;
            }
            for member in added {
                pass.members.insert(member.pid, member);
                pass.alive = true;
                if let Some(order) = order {
                    self.signal(member, order);
                }
            }
        }
    }
}
