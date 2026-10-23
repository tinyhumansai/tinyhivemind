//! Human views of a lowered package: a text tree and a mermaid flowchart.

use std::fmt::Write as _;

use tinyhivemind_lang::{LoweredHive, LoweredSeat};

use crate::load::{Loaded, roles};

/// The text tree: roles, seats, desks, goals, missions, links and policies.
pub(crate) fn tree(loaded: &Loaded) -> String {
    let hive = &loaded.lowered;
    let mut out = String::new();
    let _ = writeln!(out, "{} ({})", hive.name, hive.id);
    if !loaded.summary.is_empty() {
        let _ = writeln!(out, "  {}", loaded.summary);
    }
    let _ = writeln!(out, "roles");
    for role in &loaded.package.manifest.roles {
        let charter = loaded
            .package
            .documents
            .get(&role.charter)
            .map(|text| first_line(crate::load::body(text)))
            .unwrap_or_default();
        let _ = writeln!(out, "  {}: {charter}", role.id);
    }
    let _ = writeln!(out, "seats");
    let role_of = roles(hive);
    for seat in &hive.seats {
        let _ = writeln!(
            out,
            "  {}",
            seat_line(seat, role_of.get(seat.id.as_str()).copied().unwrap_or(""))
        );
    }
    let _ = writeln!(out, "desks");
    for desk in &hive.desks {
        let _ = writeln!(
            out,
            "  {} \"{}\" [{:?}]: {}",
            desk.id,
            desk.name,
            desk.responder_mode,
            desk.members.join(", ")
        );
    }
    links(&mut out, hive);
    policies(&mut out, hive);
    out
}

fn seat_line(seat: &LoweredSeat, role: &str) -> String {
    let mut line = format!("{} ({role}) tools={}", seat.id, seat.integration.tools.allow.join(","));
    if !seat.mcp.is_empty() {
        let _ = write!(line, " mcp={}", seat.mcp.join(","));
    }
    if let Some(memory) = &seat.memory {
        let _ = write!(
            line,
            " memory={} [{:?}/{:?}{}] reads={} recall={:?} remember={:?}",
            memory.namespace,
            memory.kind,
            memory.identity.lifecycle,
            if memory.identity.read_only {
                ", read-only"
            } else {
                ""
            },
            memory
                .settings
                .reads
                .iter()
                .map(|reach| reach.at.as_str())
                .collect::<Vec<_>>()
                .join(","),
            memory.settings.recall_at,
            memory.settings.remember,
        );
    }
    line
}

fn links(out: &mut String, hive: &LoweredHive) {
    if !hive.goals.is_empty() {
        let _ = writeln!(out, "goals");
        for goal in &hive.goals {
            let children = if goal.subgoals.is_empty() {
                String::new()
            } else {
                format!(" -> {}", goal.subgoals.join(", "))
            };
            let _ = writeln!(out, "  {}: {}{children}", goal.id, goal.description);
        }
    }
    if !hive.missions.is_empty() {
        let _ = writeln!(out, "missions");
        for mission in &hive.missions {
            let _ = writeln!(
                out,
                "  {}: {} owns {}",
                mission.id,
                mission.role,
                mission.goals.join(", ")
            );
        }
    }
    for (name, links) in [("topology", &hive.topology), ("referrals", &hive.referrals)] {
        if links.is_empty() {
            continue;
        }
        let _ = writeln!(out, "{name}");
        for link in links {
            let _ = writeln!(out, "  {} -> {}", link.from, link.to);
        }
    }
    if !hive.context.is_empty() {
        let _ = writeln!(out, "context");
        for path in hive.context.keys() {
            let _ = writeln!(out, "  {path}");
        }
    }
}

fn policies(out: &mut String, hive: &LoweredHive) {
    let episode = &hive.policies.episode;
    let _ = writeln!(out, "policies");
    let _ = writeln!(
        out,
        "  episode: budget {} turns, rounds {} blind / {} revealed, blind opening {}, quorum {} grounded={} window {}",
        episode.turn_budget,
        episode.round_width,
        episode.revealed_width,
        episode.blind_round,
        episode.quorum.threshold,
        episode.quorum.require_grounded,
        episode.quorum.window,
    );
    let referral = &hive.policies.referral;
    if referral.enabled {
        let _ = writeln!(
            out,
            "  referral: max {} hops, reach {:?}, returns {}",
            referral.max_hops, referral.reach, referral.returns
        );
    }
    let approval = &hive.policies.approval;
    if approval.enabled {
        let _ = writeln!(
            out,
            "  approval: default {:?}, {} rule(s), approver {:?}",
            approval.default,
            approval.rules.len(),
            approval.approver
        );
    }
    let aside = &hive.policies.aside;
    if aside.enabled {
        let _ = writeln!(
            out,
            "  aside: {} members, {} messages, must surface {}",
            aside.max_members, aside.max_messages, aside.must_surface
        );
    }
}

fn first_line(text: &str) -> String {
    text.lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim()
        .to_owned()
}

/// A mermaid identifier: ASCII alphanumerics and underscores only.
pub(crate) fn node(id: &str) -> String {
    id.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

/// A mermaid flowchart: a subgraph per desk holding its seats, solid edges
/// for topology and dashed, labelled edges for referrals.
pub(crate) fn mermaid(hive: &LoweredHive) -> String {
    let mut out = String::from("flowchart LR\n");
    let role_of = roles(hive);
    for desk in &hive.desks {
        let _ = writeln!(out, "  subgraph {}[\"{}\"]", node(&desk.id), desk.name);
        for member in &desk.members {
            let role = role_of.get(member.as_str()).copied().unwrap_or("");
            let _ = writeln!(
                out,
                "    {}__{}[\"{member}<br/><i>{role}</i>\"]",
                node(&desk.id),
                node(member)
            );
        }
        let _ = writeln!(out, "  end");
    }
    for link in &hive.topology {
        let _ = writeln!(out, "  {} --> {}", node(&link.from), node(&link.to));
    }
    for link in &hive.referrals {
        let _ = writeln!(
            out,
            "  {} -. refer .-> {}",
            node(&link.from),
            node(&link.to)
        );
    }
    out
}
