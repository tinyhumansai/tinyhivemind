//! Problem roles, semantic candidates, and completion-tool compatibility.

use tinyhivemind_core::embed::RouteCandidate;
use tinyhivemind_core::runtime::speech::{CallArguments, ToolCall, Utterance, interpret};

use super::RESEARCH_START;

pub(super) fn completion_assignment(problem: &str, id: &str) -> String {
    if problem == "1008" {
        return completion_assignment_1008(id);
    }
    let role = match id {
        "researcher" => format!(
            "Audit the provenance of the staged EulerSolve and cirosantilli sources. Broadcast the exact public method and file paths to the best implementation or verification specialist. {RESEARCH_START}"
        ),
        "theory" => "Do not invent another recurrence. Audit `research_sources/eulersolve_explanation.html`, `research_sources/eulersolve_solution.py`, and `research_sources/cirosantilli_1006.md`. Broadcast a self-contained account of the compressed-word proof and exact file to solver; if a checker sign-off is already on the desk, complete your assignment.".into(),
        "solver" => "Do not invent another recurrence. Execute `python3 research_sources/eulersolve_solution.py`, inspect its compressed-word implementation, and independently compare it with brute force beyond the supplied samples. Broadcast the exact observed command output and verification request to checker; if checker already signed it, complete your assignment.".into(),
        "checker" => "Execute `python3 research_sources/eulersolve_solution.py` yourself and inspect its built-in brute-force comparisons for k=1..50. Independently reproduce at least the supplied samples. Broadcast a concrete repair if anything fails; otherwise call complete_episode with command-backed sign-off and the canonical residue.".into(),
        "lead" => "Reconcile only executable evidence. Broadcast the single most useful next verification while work remains; after a checker sign-off, call complete_episode with the evidence chain.".into(),
        _ => "Advance the assigned PE1006 work and report through a TinyHiveMind tool.".into(),
    };
    format!("{role}\nDo not repeat already-settled work from the desk delta.")
}

fn completion_assignment_1008(id: &str) -> String {
    let role = match id {
        "theory" => {
            "Read FAILED_RUN.md. If CANDIDATE_DERIVATION.md exists, audit every sign, factorial ratio, and the degree-9 reciprocal-square complement identity against the full Lagrange formula, then broadcast a proof verdict to checker. Otherwise derive the full-node formula independently."
        }
        "solver" => {
            "Read FAILED_RUN.md and CANDIDATE_DERIVATION.md when present. Compile and run solver_pe1008.rs, then independently implement complete direct interpolation for several small N>10 and compare. Broadcast the exact logs and candidate to checker; complete only after checker evidence arrives."
        }
        "checker" => {
            "Audit CANDIDATE_DERIVATION.md and solver_pe1008.rs. Independently compute the full interpolation polynomial for N=10..20, compile and run the optimized solver, and inspect the degree-9 complement argument. Complete only with command-backed sign-off and the target coefficient, or broadcast a concrete counterexample."
        }
        "lead" => {
            "Reconcile only proved formulas and executable checks. Broadcast the most useful unresolved proof or verification task; complete only after independent checker sign-off."
        }
        "researcher" => {
            "Find general references on interpolation at square nodes, inverse Vandermonde coefficients, and symmetric-polynomial formulas. Do not search for PE1008 answers or published solution code. Broadcast cited mathematical identities to the best specialist."
        }
        _ => {
            "Advance the sealed PE1008 derivation and use a TinyHiveMind tool to hand off or complete."
        }
    };
    format!("{role}\nThis is a sealed run: do not search for or use PE1008 answers.")
}

pub(super) fn role_prompt(problem: &str, id: &str) -> String {
    if problem == "1008" {
        return match id {
            "theory" => "You are the algebra and interpolation specialist. Derive exact coefficient identities and prove every reduction.".into(),
            "solver" => "You are the modular implementation specialist. Turn proved formulas into efficient code and validate against direct interpolation for small N.".into(),
            "checker" => "You are the adversarial verifier. Independently derive small cases, attack sign/index errors, and sign only command-backed exact results.".into(),
            "lead" => "You coordinate the sealed PE1008 desk and accept only independently checked mathematical evidence.".into(),
            "researcher" => "You may research general interpolation mathematics, but must not search for PE1008 answers or solution implementations.".into(),
            _ => "You are a PE1008 specialist.".into(),
        };
    }
    match id {
        "theory" => "You are the Fibonacci-word combinatorics specialist. Derive exact structure and logarithmic formulas; test every claimed identity on small k.".into(),
        "solver" => "You are the implementation specialist. Turn proven formulas into exact modular code, run it, and report reproducible commands and residues.".into(),
        "checker" => "You are the adversarial verifier. Independently reproduce samples, attack extrapolations, and sign only an exact candidate supported by code.".into(),
        "lead" => "You coordinate the desk. Reconcile disagreements, demand missing evidence, and state a final residue only after checker sign-off.".into(),
        "researcher" => "You are the web researcher. Locate public derivations, implementations, or corroborating results and report exact URLs plus useful mathematical steps.".into(),
        _ => "You are a PE1006 specialist.".into(),
    }
}

pub(super) fn route_candidates(problem: &str, exclude: Option<&str>) -> Vec<RouteCandidate> {
    let topic = format!("Project Euler {problem}");
    [
        ("theory", "algebraic structure and exact proofs"),
        (
            "solver",
            "exact modular implementation and executable checks",
        ),
        ("checker", "adversarial independent verification"),
        ("lead", "coordination and evidence synthesis"),
        ("researcher", "public-source research and provenance"),
    ]
    .into_iter()
    .filter(|(id, _)| Some(*id) != exclude)
    .map(|(id, description)| RouteCandidate {
        id: id.into(),
        label: id.into(),
        role: Some(description.into()),
        description: Some(description.into()),
        capabilities: vec![description.into()],
        learned_topics: vec![topic.clone()],
        available: true,
    })
    .collect()
}

/// Whether `server` names a seat's hive tool server, `tinyhive-<seat>`.
fn hive_server(server: &str) -> bool {
    server
        .strip_prefix("tinyhive-")
        .is_some_and(|seat| !seat.is_empty())
}

pub(super) fn recover_tool_call(text: &str) -> Option<Utterance> {
    let trimmed = text.trim();
    for (prefix, name) in [
        ("broadcast:", "broadcast"),
        ("complete_episode:", "complete_episode"),
    ] {
        if let Some(message) = trimmed.strip_prefix(prefix) {
            let message = message.trim().trim_matches('"');
            let call = interpret(
                name,
                &CallArguments {
                    message: Some(message),
                    ..Default::default()
                },
            )
            .ok()?;
            if let ToolCall::Speak(utterance) = call {
                return Some(utterance);
            }
        }
    }
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    let value: serde_json::Value = serde_json::from_str(&text[start..=end]).ok()?;
    let tool = value.get("tool")?.as_str()?;
    let (name, arguments) = if tool == "mcp_call_tool" {
        let outer = value.get("arguments")?;
        if !hive_server(outer.get("server")?.as_str()?) {
            return None;
        }
        (
            outer
                .get("tool")
                .or_else(|| outer.get("method"))?
                .as_str()?,
            outer.get("arguments").or_else(|| outer.get("args"))?,
        )
    } else {
        if !matches!(tool, "broadcast" | "complete_episode")
            || value
                .get("server")
                .is_some_and(|server| !server.as_str().is_some_and(hive_server))
        {
            return None;
        }
        (tool, value.get("arguments")?)
    };
    let message = arguments.get("message")?.as_str()?;
    let call = interpret(
        name,
        &CallArguments {
            message: Some(message),
            ..Default::default()
        },
    )
    .ok()?;
    match call {
        ToolCall::Speak(utterance)
            if matches!(
                utterance,
                Utterance::Broadcast { .. } | Utterance::CompleteEpisode { .. }
            ) =>
        {
            Some(utterance)
        }
        ToolCall::Speak(_) | ToolCall::Read { .. } => None,
    }
}
