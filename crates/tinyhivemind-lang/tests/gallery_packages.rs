//! The example gallery (`examples/hives/`) parses and lowers, and keeps the
//! facts each package exists to demonstrate.
//!
//! Every document is read with `include_str!`, exactly as the fixtures are, so
//! the gallery cannot drift from the schema without failing this suite. The
//! web-access check alone reads `examples/hives/` from disk, so a package
//! added later is held to it without being listed here.
use std::collections::BTreeMap;
use tinyhivemind_core::{approval::RuleVerdict, desk::ResponderMode};
use tinyhivemind_lang::{
    LoweredHive, MemoryKind, MemoryLifecycle, Package, lower, parse,
    patch::{self, Operation, Patch, PatchClass},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// A package's documents, keyed by package-relative path.
macro_rules! package {
    ($dir:literal, [$($path:literal),* $(,)?]) => {
        BTreeMap::from([$((
            $path.to_owned(),
            include_str!(concat!("../examples/hives/", $dir, "/", $path)).to_owned(),
        )),*])
    };
}

fn lowered(documents: &BTreeMap<String, String>) -> Result<(Package, LoweredHive)> {
    let package = parse(documents)?;
    let lowered = lower(&package)?;
    Ok((package, lowered))
}

fn seat_ids(hive: &LoweredHive) -> Vec<&str> {
    hive.seats.iter().map(|seat| seat.id.as_str()).collect()
}

fn links(links: &[tinyhivemind_lang::Link]) -> Vec<(&str, &str)> {
    links
        .iter()
        .map(|link| (link.from.as_str(), link.to.as_str()))
        .collect()
}

#[test]
fn review_board_is_one_desk_of_three_with_grounded_quorum() -> Result<()> {
    let (_, hive) = review_board()?;
    assert_eq!(hive.desks.len(), 1);
    assert_eq!(hive.desks[0].id, "board");
    assert_eq!(seat_ids(&hive), ["author", "reviewer-a", "reviewer-b"]);
    assert_eq!(
        hive.desks[0].members,
        ["author", "reviewer-a", "reviewer-b"]
    );
    let episode = hive.policies.episode;
    assert!(episode.blind_round);
    assert_eq!(episode.quorum.threshold, 2);
    assert!(episode.quorum.require_grounded);
    assert!(hive.topology.is_empty() && hive.referrals.is_empty());
    Ok(())
}

#[test]
fn research_debate_carries_goals_missions_and_itemized_context() -> Result<()> {
    let (_, hive) = research_debate()?;
    assert_eq!(hive.desks[0].responder_mode, ResponderMode::Auto);
    let verdict = hive
        .goals
        .iter()
        .find(|goal| goal.id == "verdict")
        .ok_or("verdict goal")?;
    assert_eq!(verdict.subgoals, ["survey", "critique"]);
    let missions: Vec<(&str, &str)> = hive
        .missions
        .iter()
        .map(|mission| (mission.id.as_str(), mission.role.as_str()))
        .collect();
    assert_eq!(
        missions,
        [
            ("decide", "moderator"),
            ("survey-prior-work", "librarian"),
            ("stress-test", "skeptic")
        ]
    );
    let brief = hive.context.get("context/brief.md").ok_or("brief")?;
    for item in ["latency", "staleness", "prior-work"] {
        assert!(brief.contains(&format!("<!-- item:{item} -->")), "{item}");
    }
    Ok(())
}

#[test]
fn incident_response_links_two_desks_with_referral_approval_and_asides() -> Result<()> {
    let (_, hive) = incident_response()?;
    let desks: Vec<(&str, usize)> = hive
        .desks
        .iter()
        .map(|desk| (desk.id.as_str(), desk.members.len()))
        .collect();
    assert_eq!(desks, [("triage", 3), ("fixers", 3)]);
    assert_eq!(links(&hive.topology), [("triage", "fixers")]);
    assert_eq!(
        links(&hive.referrals),
        [("triage", "fixers"), ("fixers", "triage")]
    );
    assert!(hive.policies.referral.enabled);
    assert_eq!(hive.policies.referral.max_hops, 2);
    let approval = &hive.policies.approval;
    assert!(approval.enabled);
    assert_eq!(approval.rules.len(), 2);
    assert_eq!(approval.rules[1].verdict, RuleVerdict::Ask);
    assert!(hive.policies.aside.enabled && hive.policies.aside.must_surface);
    Ok(())
}

#[test]
fn memory_guild_binds_agent_pool_forked_and_pinned_memories() -> Result<()> {
    let (package, hive) = memory_guild()?;
    assert_eq!(hive.memory.root, "team:guild");
    assert_eq!(package.constitution.read_only_memories, ["charter"]);
    let memory = |id: &str| {
        hive.seats
            .iter()
            .find(|seat| seat.id == id)
            .and_then(|seat| seat.memory.as_ref())
            .ok_or(format!("{id} has memory"))
    };
    let archivist = memory("archivist")?;
    assert_eq!(archivist.agent_id, "archivist");
    assert_eq!(archivist.root, "team:guild");
    assert_eq!(archivist.kind, MemoryKind::Agent);
    assert_eq!(archivist.identity.lifecycle, MemoryLifecycle::Hive);
    assert_eq!(archivist.settings.reads.len(), 3);
    let scout_a = memory("scout-a")?;
    assert_eq!(scout_a.identity.fork_parent.as_deref(), Some("lore"));
    assert_eq!(scout_a.root, "team:guild/project:lore");
    let scout_b = memory("scout-b")?;
    assert_eq!(scout_b.kind, MemoryKind::Pool);
    let charter = hive
        .memory
        .identities
        .iter()
        .find(|identity| identity.id == "charter")
        .ok_or("charter")?;
    assert!(charter.read_only);
    // Frontmatter is validated against the manifest and stripped on lowering.
    let prompt = &hive
        .seats
        .iter()
        .find(|seat| seat.id == "archivist")
        .ok_or("archivist")?
        .prompt;
    assert!(package.documents["seats/archivist.md"].starts_with("---\n"));
    assert!(prompt.starts_with("You are the archivist."));
    Ok(())
}

#[test]
fn self_improving_constitution_bounds_what_an_editor_may_change() -> Result<()> {
    let (package, hive) = self_improving()?;
    let constitution = &package.constitution;
    assert_eq!(
        constitution.permitted_edits,
        [PatchClass::Prompt, PatchClass::Context, PatchClass::Goal]
    );
    assert_eq!(
        constitution.protected_namespaces,
        ["team:self/project:heldout"]
    );
    assert_eq!(constitution.contamination_canaries, ["HELDOUT-7F3A"]);
    assert_eq!(constitution.read_only_memories, ["eval-ledger"]);
    assert!(constitution.evaluation.is_object());
    assert!(hive.context.contains_key("context/playbook.md"));

    // A prompt edit is permitted; a policy edit is not.
    let before = package.documents["seats/solver.md"].clone();
    let prompt = Patch {
        class: PatchClass::Prompt,
        operations: vec![Operation::Prompt {
            path: "seats/solver.md".into(),
            before: before.clone(),
            after: format!("{before}Include canceled invoices.\n"),
        }],
    };
    assert!(patch::apply(&package, &prompt).is_ok());
    let policy = Patch {
        class: PatchClass::Policy,
        operations: vec![Operation::Json {
            pointer: "/manifest/policies/episode/turn_budget".into(),
            before: Some(serde_json::json!(9)),
            after: Some(serde_json::json!(12)),
        }],
    };
    assert!(matches!(
        patch::apply(&package, &policy),
        Err(patch::Error::ForbiddenClass)
    ));
    Ok(())
}

#[test]
fn math_panel_is_five_seats_on_one_desk() -> Result<()> {
    let (_, hive) = math_panel()?;
    assert_eq!(hive.desks.len(), 1);
    assert_eq!(
        seat_ids(&hive),
        ["conjecturer", "solver-a", "solver-b", "checker", "lead"]
    );
    let roles: Vec<Option<&str>> = hive
        .candidates
        .iter()
        .map(|candidate| candidate.role.as_deref())
        .collect();
    assert_eq!(
        roles,
        [
            Some("conjecturer"),
            Some("solver"),
            Some("solver"),
            Some("checker"),
            Some("lead")
        ]
    );
    assert_eq!(hive.policies.episode.quorum.threshold, 3);
    Ok(())
}

#[test]
fn math_swarm_is_five_linked_desks_of_ten_with_wide_blind_rounds() -> Result<()> {
    let (package, hive) = math_swarm()?;
    assert_eq!(hive.seats.len(), 50);
    let desks = ["conjecture", "compute", "prove", "verify", "synthesis"];
    assert_eq!(
        hive.desks
            .iter()
            .map(|desk| desk.id.as_str())
            .collect::<Vec<_>>(),
        desks
    );
    for desk in &hive.desks {
        let expected: Vec<String> = (1..=10).map(|n| format!("{}-{n:02}", desk.id)).collect();
        assert_eq!(desk.members, expected);
    }
    assert_eq!(
        links(&hive.topology),
        [
            ("conjecture", "compute"),
            ("compute", "prove"),
            ("prove", "verify"),
            ("verify", "synthesis"),
            ("synthesis", "conjecture")
        ]
    );
    assert_eq!(hive.referrals.len(), 5);
    assert!(
        hive.referrals
            .iter()
            .filter(|link| link.to == "verify")
            .count()
            >= 3
    );
    let episode = hive.policies.episode;
    assert_eq!((episode.round_width, episode.revealed_width), (10, 10));
    assert_eq!(episode.quorum.threshold, 26);
    assert!(episode.quorum.require_grounded);
    assert_eq!(package.constitution.max_width, 10);
    assert!(u64::from(episode.turn_budget) <= package.constitution.max_turns);
    Ok(())
}

/// Tool names that reach the web; no gallery seat may be allowed one.
const WEB_TOOLS: [&str; 4] = ["web_search", "web_fetch", "browser", "http"];

/// The one gallery package that opts in to web research: `euler-hive`'s
/// research desk searches the public literature (through the host's search
/// tool) for the theory behind a problem. Every other package, and every
/// other euler-hive seat, stays off the web.
const WEB_RESEARCH_PACKAGE: &str = "euler-hive";
/// The euler-hive desk whose seats may hold `web_search`.
const WEB_RESEARCH_DESK: &str = "research";

/// Every package directory under `examples/hives/`, read from disk the way a
/// host loads it, so a package added later is checked without being listed.
fn gallery_from_disk() -> Result<Vec<(String, LoweredHive)>> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/hives");
    let mut dirs: Vec<_> = std::fs::read_dir(&root)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<_>>()?;
    dirs.retain(|path| path.join("hive.json").is_file());
    dirs.sort();
    let mut packages = Vec::new();
    for dir in dirs {
        let mut documents = BTreeMap::new();
        read_package(&dir, &dir, &mut documents)?;
        let name = dir
            .file_name()
            .ok_or("package directory has a name")?
            .to_string_lossy()
            .into_owned();
        let (_, hive) = lowered(&documents).map_err(|error| format!("{name}: {error}"))?;
        packages.push((name, hive));
    }
    Ok(packages)
}

fn read_package(
    root: &std::path::Path,
    dir: &std::path::Path,
    into: &mut BTreeMap<String, String>,
) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            read_package(root, &path, into)?;
            continue;
        }
        let key: Vec<String> = path
            .strip_prefix(root)?
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect();
        into.insert(key.join("/"), std::fs::read_to_string(&path)?);
    }
    Ok(())
}

#[test]
fn no_gallery_seat_may_reach_the_web_outside_euler_hives_research_desk() -> Result<()> {
    let packages = gallery_from_disk()?;
    for listed in [
        "incident-response",
        "math-panel",
        "math-swarm",
        "memory-guild",
        "research-debate",
        "review-board",
        "self-improving",
    ] {
        assert!(
            packages.iter().any(|(name, _)| name == listed),
            "{listed} is read"
        );
    }
    for (name, hive) in &packages {
        assert!(!hive.seats.is_empty(), "{name}");
        let researchers: Vec<&str> = if name == WEB_RESEARCH_PACKAGE {
            hive.desks
                .iter()
                .filter(|desk| desk.id == WEB_RESEARCH_DESK)
                .flat_map(|desk| desk.members.iter().map(String::as_str))
                .collect()
        } else {
            Vec::new()
        };
        for seat in &hive.seats {
            let allowed: Vec<&str> = WEB_TOOLS
                .into_iter()
                .filter(|tool| seat.integration.tools.allow.iter().any(|allowed| allowed == tool))
                .collect();
            if researchers.contains(&seat.id.as_str()) {
                assert!(
                    allowed.iter().all(|tool| *tool == "web_search"),
                    "{name}/{} may search but allows {allowed:?}",
                    seat.id
                );
            } else {
                assert!(allowed.is_empty(), "{name}/{} allows {allowed:?}", seat.id);
            }
            for tool in WEB_TOOLS {
                if allowed.contains(&tool) {
                    continue;
                }
                assert!(
                    seat.integration.tools.deny.iter().any(|denied| denied == tool),
                    "{name}/{} does not deny {tool}",
                    seat.id
                );
            }
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
fn every_math_seat_opens_with_a_distinct_persona() -> Result<()> {
    let (_, swarm) = math_swarm()?;
    let mut seen = std::collections::BTreeSet::new();
    for seat in &swarm.seats {
        let persona = persona(&seat.prompt).ok_or(format!("{} has no persona line", seat.id))?;
        assert!(
            seen.insert(persona.to_lowercase()),
            "{} repeats the persona {persona:?}",
            seat.id
        );
        assert!(
            seat.prompt.lines().count() >= 3,
            "{} explains how it thinks",
            seat.id
        );
    }
    assert_eq!(seen.len(), 50);
    let (_, panel) = math_panel()?;
    for seat in &panel.seats {
        let persona = persona(&seat.prompt).ok_or(format!("{} has no persona line", seat.id))?;
        assert!(
            seen.insert(persona.to_lowercase()),
            "{} repeats a swarm persona",
            seat.id
        );
    }
    assert_eq!(seen.len(), 55);
    Ok(())
}

fn incident_response() -> Result<(Package, LoweredHive)> {
    lowered(&package!(
        "incident-response",
        [
            "hive.json",
            "constitution.json",
            "memory.json",
            "roles/fixer.md",
            "roles/incident-lead.md",
            "roles/release-reviewer.md",
            "roles/triager.md",
            "seats/fixer-db.md",
            "seats/fixer-net.md",
            "seats/incident-lead.md",
            "seats/release-reviewer.md",
            "seats/triager-logs.md",
            "seats/triager-metrics.md",
        ]
    ))
}

fn math_panel() -> Result<(Package, LoweredHive)> {
    lowered(&package!(
        "math-panel",
        [
            "hive.json",
            "constitution.json",
            "memory.json",
            "roles/checker.md",
            "roles/conjecturer.md",
            "roles/lead.md",
            "roles/solver.md",
            "seats/checker.md",
            "seats/conjecturer.md",
            "seats/lead.md",
            "seats/solver-a.md",
            "seats/solver-b.md",
        ]
    ))
}

fn math_swarm() -> Result<(Package, LoweredHive)> {
    lowered(&package!(
        "math-swarm",
        [
            "hive.json",
            "constitution.json",
            "memory.json",
            "roles/computer.md",
            "roles/conjecturer.md",
            "roles/prover.md",
            "roles/synthesizer.md",
            "roles/verifier.md",
            "seats/compute-01.md",
            "seats/compute-02.md",
            "seats/compute-03.md",
            "seats/compute-04.md",
            "seats/compute-05.md",
            "seats/compute-06.md",
            "seats/compute-07.md",
            "seats/compute-08.md",
            "seats/compute-09.md",
            "seats/compute-10.md",
            "seats/conjecture-01.md",
            "seats/conjecture-02.md",
            "seats/conjecture-03.md",
            "seats/conjecture-04.md",
            "seats/conjecture-05.md",
            "seats/conjecture-06.md",
            "seats/conjecture-07.md",
            "seats/conjecture-08.md",
            "seats/conjecture-09.md",
            "seats/conjecture-10.md",
            "seats/prove-01.md",
            "seats/prove-02.md",
            "seats/prove-03.md",
            "seats/prove-04.md",
            "seats/prove-05.md",
            "seats/prove-06.md",
            "seats/prove-07.md",
            "seats/prove-08.md",
            "seats/prove-09.md",
            "seats/prove-10.md",
            "seats/synthesis-01.md",
            "seats/synthesis-02.md",
            "seats/synthesis-03.md",
            "seats/synthesis-04.md",
            "seats/synthesis-05.md",
            "seats/synthesis-06.md",
            "seats/synthesis-07.md",
            "seats/synthesis-08.md",
            "seats/synthesis-09.md",
            "seats/synthesis-10.md",
            "seats/verify-01.md",
            "seats/verify-02.md",
            "seats/verify-03.md",
            "seats/verify-04.md",
            "seats/verify-05.md",
            "seats/verify-06.md",
            "seats/verify-07.md",
            "seats/verify-08.md",
            "seats/verify-09.md",
            "seats/verify-10.md",
        ]
    ))
}

fn memory_guild() -> Result<(Package, LoweredHive)> {
    lowered(&package!(
        "memory-guild",
        [
            "hive.json",
            "constitution.json",
            "memory.json",
            "roles/archivist.md",
            "roles/scout.md",
            "roles/scribe.md",
            "seats/archivist.md",
            "seats/scout-a.md",
            "seats/scout-b.md",
            "seats/scribe.md",
        ]
    ))
}

fn research_debate() -> Result<(Package, LoweredHive)> {
    lowered(&package!(
        "research-debate",
        [
            "hive.json",
            "constitution.json",
            "memory.json",
            "roles/advocate.md",
            "roles/librarian.md",
            "roles/moderator.md",
            "roles/skeptic.md",
            "seats/advocate.md",
            "seats/librarian.md",
            "seats/moderator.md",
            "seats/skeptic.md",
            "context/brief.md",
        ]
    ))
}

fn review_board() -> Result<(Package, LoweredHive)> {
    lowered(&package!(
        "review-board",
        [
            "hive.json",
            "constitution.json",
            "memory.json",
            "roles/author.md",
            "roles/reviewer.md",
            "seats/author.md",
            "seats/reviewer-a.md",
            "seats/reviewer-b.md",
        ]
    ))
}

fn self_improving() -> Result<(Package, LoweredHive)> {
    lowered(&package!(
        "self-improving",
        [
            "hive.json",
            "constitution.json",
            "memory.json",
            "roles/critic.md",
            "roles/editor.md",
            "roles/solver.md",
            "seats/critic.md",
            "seats/editor.md",
            "seats/solver.md",
            "context/playbook.md",
        ]
    ))
}
