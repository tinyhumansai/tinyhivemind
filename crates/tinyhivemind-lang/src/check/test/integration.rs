//! Editing what a hive runs with: tool sources, skill bundles and the
//! integration sections are one class a constitution may withhold.
use crate::{patch::PatchClass, *};
use std::collections::BTreeMap;

fn original(permitted: &[PatchClass]) -> BTreeMap<String, String> {
    let mut docs = crate::validation::test_documents();
    let mut constitution: Constitution =
        serde_json::from_str(&docs["constitution.json"]).expect("constitution decodes");
    constitution.permitted_edits = permitted.to_vec();
    docs.insert(
        "constitution.json".into(),
        serde_json::to_string(&constitution).expect("constitution encodes"),
    );
    docs
}

fn rules(found: &[Diagnostic]) -> Vec<(Rule, &str)> {
    found.iter().map(|d| (d.rule, d.file.as_str())).collect()
}

#[test]
fn a_tool_script_edit_needs_the_integration_class() {
    let base = original(&[PatchClass::Prompt]);
    let mut edited = base.clone();
    edited.insert(
        "tools/grep_repo.sh".into(),
        "#!/bin/sh\necho changed\n".into(),
    );
    let refused = check(&base, &edited).expect_err("integration is not permitted");
    assert_eq!(
        rules(&refused),
        vec![(Rule::ForbiddenClass, "tools/grep_repo.sh")]
    );
    let base = original(&[PatchClass::Integration]);
    let mut edited = base.clone();
    edited.insert(
        "tools/grep_repo.sh".into(),
        "#!/bin/sh\necho changed\n".into(),
    );
    let review = check(&base, &edited).expect("integration is permitted");
    assert_eq!(review.changes.len(), 1);
    assert_eq!(review.changes[0].class, PatchClass::Integration);
    let applied = patch::apply(
        &crate::parse(&base).expect("original parses"),
        &review.changes[0],
    )
    .expect("the derived patch applies");
    assert_eq!(
        applied.candidate.documents["tools/grep_repo.sh"],
        "#!/bin/sh\necho changed\n"
    );
}

#[test]
fn a_skill_bundle_is_edited_whole_including_its_frontmatter() {
    let base = original(&[PatchClass::Integration]);
    let mut edited = base.clone();
    let skill = "skills/release-notes/SKILL.md";
    let changed = edited[skill].replace("house style", "team style");
    edited.insert(skill.into(), changed.clone());
    edited.insert(
        "skills/release-notes/template.txt".into(),
        "Added:\n".into(),
    );
    let review = check(&base, &edited).expect("skill edits are integration edits");
    assert!(
        review
            .changes
            .iter()
            .all(|patch| patch.class == PatchClass::Integration)
    );
    assert_eq!(review.candidate.documents[skill], changed);
    assert!(
        review
            .candidate
            .documents
            .contains_key("skills/release-notes/template.txt")
    );
}

#[test]
fn integration_sections_belong_to_the_integration_class() {
    let base = original(&[PatchClass::Roster]);
    let mut edited = base.clone();
    let mut manifest: serde_json::Value =
        serde_json::from_str(&edited["hive.json"]).expect("manifest decodes");
    manifest["jev"]["top_k"] = 9.into();
    manifest["features"]["verbose_tools"] = true.into();
    edited.insert("hive.json".into(), manifest.to_string());
    let refused = check(&base, &edited).expect_err("integration is not permitted");
    let pointers: Vec<_> = refused
        .iter()
        .map(|d| (d.rule, d.pointer.as_deref()))
        .collect();
    assert!(
        pointers.contains(&(Rule::ForbiddenClass, Some("/jev/top_k"))),
        "{refused:?}"
    );
    assert!(pointers.contains(&(Rule::ForbiddenClass, Some("/features/verbose_tools"))));
}

#[test]
fn a_file_outside_tools_and_skills_is_still_not_part_of_the_package() {
    let base = original(&[PatchClass::Integration]);
    let mut edited = base.clone();
    edited.insert("scripts/run.sh".into(), "echo\n".into());
    let refused = check(&base, &edited).expect_err("scripts/ is not read");
    assert_eq!(rules(&refused), vec![(Rule::Unowned, "scripts/run.sh")]);
}
