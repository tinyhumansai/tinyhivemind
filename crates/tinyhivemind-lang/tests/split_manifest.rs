//! The manifest split into `hive.json`, `agents.json`, `tools.json` and
//! `desks.json`: the gallery's `self-improving` package, split, must parse,
//! lower and check exactly like the one-file original, with every finding
//! about a moved section reported in the file that now holds it.
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use tinyhivemind_lang::{Rule, check, diagnose_documents, lower, parse};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

macro_rules! package {
    ($($path:literal),* $(,)?) => {
        BTreeMap::from([$((
            $path.to_owned(),
            include_str!(concat!("../examples/hives/self-improving/", $path)).to_owned(),
        )),*])
    };
}

fn single() -> BTreeMap<String, String> {
    package![
        "hive.json",
        "constitution.json",
        "memory.json",
        "context/playbook.md",
        "roles/critic.md",
        "roles/editor.md",
        "roles/solver.md",
        "seats/critic.md",
        "seats/editor.md",
        "seats/solver.md",
    ]
}

const SPLITS: [(&str, &[&str]); 3] = [
    ("agents.json", &["roles", "seats", "profiles"]),
    (
        "tools.json",
        &["tools", "toolpacks", "mcp", "skills", "hooks"],
    ),
    ("desks.json", &["desks", "topology", "referrals"]),
];

/// The single-file package with every split section moved to its file.
fn split() -> Result<BTreeMap<String, String>> {
    let mut files = single();
    let mut hive: Map<String, Value> = serde_json::from_str(&files["hive.json"])?;
    for (file, sections) in SPLITS {
        let part: Map<String, Value> = sections
            .iter()
            .filter_map(|section| Some(((*section).to_owned(), hive.remove(*section)?)))
            .collect();
        files.insert(file.to_owned(), serde_json::to_string_pretty(&part)?);
    }
    files.insert("hive.json".to_owned(), serde_json::to_string_pretty(&hive)?);
    Ok(files)
}

/// Rewrite one JSON file through its decoded value.
fn edit(
    files: &mut BTreeMap<String, String>,
    path: &str,
    edit: impl FnOnce(&mut Value),
) -> Result<()> {
    let mut value: Value = serde_json::from_str(&files[path])?;
    edit(&mut value);
    files.insert(path.to_owned(), serde_json::to_string_pretty(&value)?);
    Ok(())
}

fn located(files: &BTreeMap<String, String>) -> Vec<(String, Option<String>, Rule)> {
    diagnose_documents(files)
        .1
        .into_iter()
        .map(|d| (d.file, d.pointer, d.rule))
        .collect()
}

#[test]
fn a_split_manifest_parses_and_lowers_like_one_hive_json() -> Result<()> {
    let files = split()?;
    let hive: Map<String, Value> = serde_json::from_str(&files["hive.json"])?;
    assert!(!hive.contains_key("seats") && !hive.contains_key("tools"));
    let (one, many) = (parse(&single())?, parse(&files)?);
    assert_eq!(one, many);
    assert_eq!(lower(&one)?, lower(&many)?);
    Ok(())
}

#[test]
fn every_split_file_is_optional_and_may_hold_some_of_its_sections() -> Result<()> {
    let mut files = single();
    let mut hive: Map<String, Value> = serde_json::from_str(&files["hive.json"])?;
    let desks = hive.remove("desks").ok_or("the gallery declares desks")?;
    files.insert("desks.json".into(), json!({ "desks": desks }).to_string());
    files.insert("hive.json".into(), Value::Object(hive).to_string());
    assert_eq!(parse(&files)?, parse(&single())?);
    Ok(())
}

#[test]
fn a_section_declared_in_two_files_is_reported_in_the_split_file() -> Result<()> {
    let mut files = split()?;
    let seats = serde_json::from_str::<Value>(&files["agents.json"])?["seats"].clone();
    edit(&mut files, "hive.json", |hive| hive["seats"] = seats)?;
    let (package, diagnostics) = diagnose_documents(&files);
    assert!(package.is_none());
    assert_eq!(
        located(&files),
        [("agents.json".into(), Some("/seats".into()), Rule::Json)]
    );
    assert!(
        diagnostics[0].message.contains("also declared in hive.json"),
        "{}",
        diagnostics[0].message
    );
    Ok(())
}

#[test]
fn a_section_a_split_file_does_not_own_is_rejected_in_that_file() -> Result<()> {
    let mut files = split()?;
    edit(&mut files, "tools.json", |tools| tools["goals"] = json!([]))?;
    assert_eq!(
        located(&files),
        [("tools.json".into(), None, Rule::Json)],
        "{:?}",
        diagnose_documents(&files).1
    );
    files.insert("desks.json".into(), "[]".into());
    assert!(
        located(&files).contains(&("desks.json".into(), None, Rule::Json)),
        "{:?}",
        located(&files)
    );
    Ok(())
}

#[test]
fn a_finding_about_a_moved_section_points_into_its_file() -> Result<()> {
    let mut files = split()?;
    edit(&mut files, "agents.json", |agents| {
        agents["seats"][0]["role"] = json!("ghost");
    })?;
    edit(&mut files, "desks.json", |desks| {
        desks["desks"][0]["members"] = json!(["nobody"]);
    })?;
    let found = located(&files);
    assert!(
        found.contains(&("agents.json".into(), Some("/seats/0/role".into()), Rule::Reference)),
        "{found:?}"
    );
    assert!(
        found
            .iter()
            .any(|(file, pointer, _)| file == "desks.json"
                && pointer.as_deref() == Some("/desks/0/members/0")),
        "{found:?}"
    );
    assert!(found.iter().all(|(file, ..)| file != "hive.json"), "{found:?}");
    Ok(())
}

#[test]
fn checking_an_edit_reports_it_in_the_split_file_it_was_made_in() -> Result<()> {
    let original = split()?;
    // The constitution permits prompt, context and goal edits only.
    let mut edited = original.clone();
    edit(&mut edited, "agents.json", |agents| {
        agents["seats"][0]["config"]["max_iterations"] = json!(9);
    })?;
    let diagnostics = check(&original, &edited).err().ok_or("a seat edit is forbidden")?;
    assert_eq!(
        diagnostics
            .iter()
            .map(|d| (d.file.as_str(), d.rule))
            .collect::<Vec<_>>(),
        [("agents.json", Rule::ForbiddenClass)],
        "{diagnostics:?}"
    );

    // A permitted edit through hive.json checks the same split or not.
    let mut goal = original.clone();
    edit(&mut goal, "hive.json", |hive| {
        hive["goals"][0]["title"] = json!("Settle every invoice");
    })?;
    let mut goal_single = single();
    edit(&mut goal_single, "hive.json", |hive| {
        hive["goals"][0]["title"] = json!("Settle every invoice");
    })?;
    let (split_review, single_review) = (
        check(&original, &goal).map_err(|d| format!("{d:?}"))?,
        check(&single(), &goal_single).map_err(|d| format!("{d:?}"))?,
    );
    assert_eq!(split_review, single_review);
    Ok(())
}

#[test]
fn moving_a_section_between_files_is_no_change() -> Result<()> {
    let review = check(&single(), &split()?).map_err(|d| format!("{d:?}"))?;
    assert!(review.changes.is_empty(), "{:?}", review.changes);
    Ok(())
}
