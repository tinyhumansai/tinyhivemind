//! Offline checks of the command line: arguments, loading a gallery package,
//! and the rendered report for a refused and an accepted edit.
#![allow(clippy::expect_used)] // Tests fail loudly on unexpected results.
use super::*;

fn gallery() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/hives/self-improving")
}

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|arg| (*arg).to_owned()).collect()
}

#[test]
fn parses_one_or_two_directories_and_the_json_flag() {
    assert_eq!(
        args(&strings(&["a", "--json", "b"])).expect("valid arguments"),
        Args {
            dirs: vec!["a".into(), "b".into()],
            json: true
        }
    );
    for bad in [&[][..], &["a", "b", "c"], &["a", "--verbose"]] {
        assert!(args(&strings(bad)).is_err(), "{bad:?}");
    }
}

#[test]
fn loads_a_package_directory_with_nested_paths() {
    let files = load(&gallery()).expect("the gallery package loads");
    assert!(files.contains_key("hive.json"));
    assert!(files.contains_key("seats/solver.md"));
    assert!(files.contains_key("context/playbook.md"));
    assert!(load(&gallery().join("missing")).is_err());
}

#[test]
fn a_clean_package_and_an_unchanged_edit_exit_clean() {
    let dir = gallery().to_string_lossy().into_owned();
    let mut out = Vec::new();
    assert!(run(&strings(&[&dir]), &mut out).expect("runs"));
    assert_eq!(String::from_utf8(out).expect("utf-8"), "ok: no findings\n");
    let mut out = Vec::new();
    assert!(run(&strings(&[&dir, &dir, "--json"]), &mut out).expect("runs"));
    assert_eq!(String::from_utf8(out).expect("utf-8"), "[]\n");
    assert!(run(&strings(&["--json"]), &mut Vec::new()).is_err());
}

#[test]
fn a_refused_edit_prints_one_line_per_finding() {
    let original = load(&gallery()).expect("loads");
    let mut edited = original.clone();
    let hive = edited["hive.json"]
        .replace("\"turn_budget\": 9", "\"turn_budget\": 12")
        .replace(
            "\"role\": \"solver\",\n      \"goals\"",
            "\"role\": \"ghost\",\n      \"goals\"",
        );
    edited.insert("hive.json".into(), hive);
    let (diagnostics, classes) = findings(&original, Some(&edited));
    assert_eq!(classes.len(), 0, "{classes:?}");
    let text = render(&diagnostics, &classes, false).expect("renders");
    assert_eq!(
        text,
        "hive.json:/missions/0/role: reference: unresolved role reference ghost\n\
         hive.json:/policies/episode/turn_budget: forbidden_class: the constitution does not \
         permit policy edits\n\
         2 findings\n"
    );
    let json = render(&diagnostics[..1], &classes, true).expect("renders");
    let back: Vec<Diagnostic> = serde_json::from_str(&json).expect("json array");
    assert_eq!(back, diagnostics[..1]);
    assert_eq!(
        render(&diagnostics[..1], &[], false).expect("renders"),
        format!("{}\n1 finding\n", diagnostics[0])
    );
}

#[test]
fn an_accepted_edit_names_the_classes_it_changed() {
    let original = load(&gallery()).expect("loads");
    let mut edited = original.clone();
    edited.insert(
        "seats/solver.md".into(),
        format!(
            "{}Include canceled invoices.\n",
            original["seats/solver.md"]
        ),
    );
    let (diagnostics, classes) = findings(&original, Some(&edited));
    assert_eq!(diagnostics.len(), 0, "{diagnostics:?}");
    assert_eq!(
        render(&diagnostics, &classes, false).expect("renders"),
        "ok: changes prompt\n"
    );
    let (diagnostics, _) = findings(&edited, None);
    assert_eq!(diagnostics.len(), 0, "{diagnostics:?}");
}

#[test]
fn the_broken_integrations_package_reports_each_rule_once() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/broken/integrations");
    let (found, _) = findings(&load(&dir).expect("the broken package loads"), None);
    let located: Vec<(String, &str)> = found
        .iter()
        .map(|d| (d.pointer.clone().unwrap_or_default(), d.rule.name()))
        .collect();
    for expected in [
        ("/seats/1/model", "reference"),
        ("/tools/1/command/1", "missing_document"),
        ("/runtime/session_store", "integration"),
        ("/tools/0/argv/1", "integration"),
        ("/profiles/2/model", "integration"),
    ] {
        assert!(
            located.contains(&(expected.0.to_owned(), expected.1)),
            "{expected:?} in {located:?}"
        );
    }
    assert_eq!(found.len(), 5, "{located:?}");
}
