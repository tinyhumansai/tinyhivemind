//! A host proposes, guards, evaluates and archives accepted and rejected designs.
use std::collections::BTreeMap;
use tinyhivemind_lang::{
    Package,
    lineage::{self, Direction, ObjectiveScore, Verdict, VersionMetadata},
    patch::{PatchClass, apply, diff},
};

fn evidence(
    parent: Option<String>,
    patch: tinyhivemind_lang::patch::Patch,
    score: f64,
    verdict: Verdict,
) -> VersionMetadata {
    VersionMetadata {
        parent,
        patch,
        rationale: "Require explicit evidence before answering".into(),
        transcript_refs: vec![4, 9],
        scores: BTreeMap::from([(
            "quality".into(),
            ObjectiveScore {
                value: score,
                direction: Direction::Maximize,
            },
        )]),
        verdict,
        memory_watermarks: BTreeMap::new(),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut original = Package::default();
    original.constitution.permitted_edits = vec![PatchClass::Prompt];
    original.constitution.contamination_canaries = vec!["private-evaluation-canary".into()];
    original
        .documents
        .insert("roles/editor.md".into(), "Answer the task.\n".into());
    let initial_patch = tinyhivemind_lang::patch::Patch {
        class: PatchClass::Prompt,
        operations: vec![],
    };
    let initial = lineage::record(
        &original,
        evidence(None, initial_patch, 0.5, Verdict::Accepted),
    )?;
    let mut proposed = original.clone();
    proposed.documents.insert(
        "roles/editor.md".into(),
        "Answer the task and cite concrete evidence.\n".into(),
    );
    let patch = diff(&original, &proposed, PatchClass::Prompt)?;
    let applied = apply(&original, &patch)?;
    // The host supplies held-out evaluation. This deterministic example uses
    // a local rubric; the language neither runs a model nor chooses acceptance.
    let quality = if applied.candidate.documents["roles/editor.md"].contains("evidence") {
        0.8
    } else {
        0.4
    };
    let accepted = lineage::record(
        &applied.candidate,
        evidence(Some(initial.id.clone()), patch, quality, Verdict::Accepted),
    )?;
    let mut rejected_candidate = applied.candidate.clone();
    rejected_candidate
        .documents
        .insert("roles/editor.md".into(), "Guess without checking.\n".into());
    let rejected_patch = diff(&applied.candidate, &rejected_candidate, PatchClass::Prompt)?;
    let rejected_candidate = apply(&applied.candidate, &rejected_patch)?.candidate;
    let rejected = lineage::record(
        &rejected_candidate,
        evidence(
            Some(accepted.id.clone()),
            rejected_patch,
            0.2,
            Verdict::Rejected,
        ),
    )?;
    let archive = vec![initial, accepted, rejected];
    let head = lineage::accepted_head(&archive)?.ok_or("expected an accepted design")?;
    println!("Accepted head: {}", head.id);
    println!(
        "Archived designs: {}; Pareto frontier: {}",
        archive.len(),
        lineage::pareto_frontier(&archive)?.len()
    );
    Ok(())
}
