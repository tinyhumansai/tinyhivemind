//! The `euler-hive` gallery package parses, lowers, and keeps the tool and
//! memory gating a live host relies on.
//!
//! The package is read from disk, the way a host loads it, so a file added to
//! the directory is part of what is validated.
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use tinyhivemind_lang::{
    EntryKind, LoweredHive, MemoryKind, MemoryLifecycle, Package, RecallMoment, lower, parse,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const ROOT: &str = "team:euler-hive";
/// Tool names that reach the web.
const WEB_TOOLS: [&str; 4] = ["web_search", "web_fetch", "browser", "http"];
/// The hive's own speaking and memory tools, which every seat holds.
const HIVE_TOOLS: [&str; 5] = [
    "hive_broadcast",
    "hive_refer",
    "hive_complete",
    "hive_remember",
    "hive_recall",
];
/// Harness tools no seat may hold: the sandbox is the only place code runs.
const HOST_TOOLS: [&str; 5] = ["shell", "file_read", "file_write", "file_edit", "web_fetch"];

fn package_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/hives/euler-hive")
}

fn walk(root: &Path, dir: &Path, into: &mut BTreeMap<String, String>) -> Result<()> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<_>>()?;
    entries.sort();
    for path in entries {
        if path.is_dir() {
            walk(root, &path, into)?;
            continue;
        }
        let relative = path.strip_prefix(root)?;
        let key: Vec<String> = relative
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect();
        into.insert(key.join("/"), std::fs::read_to_string(&path)?);
    }
    Ok(())
}

fn euler_hive() -> Result<(Package, LoweredHive)> {
    let mut documents = BTreeMap::new();
    let dir = package_dir();
    walk(&dir, &dir, &mut documents)?;
    let package = parse(&documents)?;
    let lowered = lower(&package)?;
    Ok((package, lowered))
}

fn desk_of<'a>(hive: &'a LoweredHive, seat: &str) -> &'a str {
    hive.desks
        .iter()
        .find(|desk| desk.members.iter().any(|member| member == seat))
        .map_or("", |desk| desk.id.as_str())
}

#[test]
fn euler_hive_is_five_linked_desks_with_referral_enabled() -> Result<()> {
    let (package, hive) = euler_hive()?;
    let desks: Vec<_> = hive.desks.iter().map(|desk| desk.id.as_str()).collect();
    assert_eq!(
        desks,
        ["research", "theory", "compute", "verify", "synthesis"]
    );
    assert!(
        (12..=20).contains(&hive.seats.len()),
        "{}",
        hive.seats.len()
    );
    let chain: Vec<_> = hive
        .topology
        .iter()
        .map(|link| (link.from.as_str(), link.to.as_str()))
        .collect();
    assert_eq!(
        chain,
        [
            ("research", "theory"),
            ("theory", "compute"),
            ("compute", "verify"),
            ("verify", "synthesis")
        ]
    );
    let referrals: BTreeSet<_> = hive
        .referrals
        .iter()
        .map(|link| (link.from.as_str(), link.to.as_str()))
        .collect();
    for link in &chain {
        assert!(referrals.contains(link), "{link:?} is not referable");
    }
    for back in [("verify", "compute"), ("synthesis", "verify")] {
        assert!(referrals.contains(&back), "{back:?} has no way back");
    }
    let referral = hive.policies.referral;
    assert!(referral.enabled && referral.returns);
    assert!(referral.max_hops >= 4, "the pipeline is four hops deep");
    assert!(u64::from(hive.policies.episode.turn_budget) <= package.constitution.max_turns);
    assert_eq!(hive.context.len(), 1);
    Ok(())
}

#[test]
fn only_research_seats_reach_the_web_and_only_compute_and_verify_run_code() -> Result<()> {
    let (_, hive) = euler_hive()?;
    for seat in &hive.seats {
        let desk = desk_of(&hive, &seat.id);
        let allowed: BTreeSet<_> = seat.integration.tools.allow.iter().map(String::as_str).collect();
        let mut expected: BTreeSet<_> = HIVE_TOOLS.into_iter().collect();
        match desk {
            "research" => {
                expected.insert("web_search");
            }
            "compute" | "verify" => {
                expected.insert("run_python");
            }
            "theory" | "synthesis" => {}
            other => return Err(format!("{} sits on unexpected desk {other:?}", seat.id).into()),
        }
        assert_eq!(allowed, expected, "{} allows the wrong tools", seat.id);
        for tool in HOST_TOOLS {
            assert!(
                seat.integration.tools.deny.iter().any(|denied| denied == tool),
                "{} does not deny {tool}",
                seat.id
            );
        }
        if desk != "research" {
            for tool in WEB_TOOLS {
                assert!(!allowed.contains(tool), "{} allows {tool}", seat.id);
                assert!(
                    seat.integration.tools.deny.iter().any(|denied| denied == tool),
                    "{} does not deny {tool}",
                    seat.id
                );
            }
        }
        if !matches!(desk, "compute" | "verify") {
            assert!(
                seat.integration.tools.deny.iter().any(|denied| denied == "run_python"),
                "{} does not deny run_python",
                seat.id
            );
        }
    }
    Ok(())
}

/// The persona a seat prompt opens with: `You are a "<persona>".`
fn persona(prompt: &str) -> Option<&str> {
    prompt
        .lines()
        .next()?
        .strip_prefix("You are a \"")?
        .strip_suffix("\".")
        .filter(|persona| !persona.trim().is_empty())
}

#[test]
fn every_euler_seat_opens_with_a_distinct_persona() -> Result<()> {
    let (_, hive) = euler_hive()?;
    let mut seen = BTreeSet::new();
    for seat in &hive.seats {
        let persona = persona(&seat.prompt).ok_or(format!("{} has no persona line", seat.id))?;
        assert!(
            seen.insert(persona.to_lowercase()),
            "{} repeats {persona:?}",
            seat.id
        );
        assert!(seat.prompt.lines().count() >= 3, "{} is too thin", seat.id);
    }
    Ok(())
}

#[test]
fn every_seat_binds_its_own_agent_memory_and_reads_the_verified_pool() -> Result<()> {
    let (package, hive) = euler_hive()?;
    let pool = format!("{ROOT}/project:verified");
    assert!(
        package
            .memory
            .identities
            .iter()
            .any(|identity| identity.namespace == pool && identity.kind == MemoryKind::Pool)
    );
    for seat in &hive.seats {
        let binding = seat
            .memory
            .as_ref()
            .ok_or(format!("{} has no memory", seat.id))?;
        assert_eq!(binding.kind, MemoryKind::Agent);
        assert_eq!(binding.root, ROOT);
        assert_eq!(binding.agent_id, seat.id);
        assert_eq!(binding.namespace, format!("{ROOT}/agent:{}", seat.id));
        assert_eq!(binding.identity.lifecycle, MemoryLifecycle::Run);
        assert!(!binding.identity.read_only);
        assert_eq!(
            binding.settings.recall_at,
            [
                RecallMoment::SessionStart,
                RecallMoment::Rejoin,
                RecallMoment::Compaction
            ]
        );
        for kind in [
            EntryKind::Observation,
            EntryKind::FailedAttempt,
            EntryKind::Outcome,
        ] {
            assert!(binding.settings.remember.contains(&kind), "{}", seat.id);
        }
        let reads: Vec<_> = binding
            .settings
            .reads
            .iter()
            .map(|r| r.at.as_str())
            .collect();
        assert!(reads.contains(&binding.namespace.as_str()));
        assert!(reads.contains(&pool.as_str()));
    }
    Ok(())
}

#[test]
fn the_committed_problem_is_a_placeholder_naming_no_problem() -> Result<()> {
    let (_, hive) = euler_hive()?;
    let problem = hive
        .context
        .get("context/problem.md")
        .ok_or("no problem context")?;
    assert!(problem.contains("<!-- item:statement -->"));
    assert!(problem.contains("placeholder"));
    let lower = problem.to_lowercase();
    for named in ["project euler", "projecteuler", "problem 1", "club"] {
        assert!(!lower.contains(named), "placeholder names {named:?}");
    }
    Ok(())
}
