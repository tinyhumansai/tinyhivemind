//! Package wiring: the problem placeholder, per-seat tool gate, tool schemas and the federation opening.

use std::collections::BTreeSet;

use super::super::floors::{Assignment, Federation};
use super::super::package::{self, SeatTool};
use super::super::search::Provider;
use super::super::tools::schema;
use super::support::*;

#[test]
fn the_host_injects_the_problem_into_the_placeholder() {
    let (package, hive) = lowered();
    let problem = &hive.context[package::PROBLEM_DOCUMENT];
    assert!(problem.contains("Find the answer to a test problem."));
    assert!(!problem.contains("placeholder"));
    assert_eq!(package.manifest.id, "euler-hive");
}

#[test]
fn a_package_without_the_problem_document_is_refused() {
    let mut documents = package::read_package(&package_dir()).expect("package reads");
    documents.remove(package::PROBLEM_DOCUMENT);
    assert!(package::load(documents, "x").is_err());
}

#[test]
fn problem_markdown_keeps_paragraphs_and_tex_and_drops_tags() {
    let html = "<p>A club has $7$ members&nbsp;named <b>A</b>.</p><p>Find &lt;it&gt;.</p>";
    assert_eq!(
        package::problem_markdown(html),
        "A club has $7$ members named A.\n\nFind <it>."
    );
}

#[test]
fn only_research_seats_get_web_search_and_only_compute_and_verify_get_python() {
    let (_, hive) = lowered();
    for seat in &hive.seats {
        let belt = package::seat_tools(seat).expect("every seat's belt resolves");
        let desk = package::desk_of(&hive, &seat.id).expect("seat has a desk");
        assert_eq!(
            belt.contains(&SeatTool::WebSearch),
            desk.id == "research",
            "{}",
            seat.id
        );
        assert_eq!(
            belt.contains(&SeatTool::RunPython),
            matches!(desk.id.as_str(), "compute" | "verify"),
            "{}",
            seat.id
        );
        for speaking in [SeatTool::Broadcast, SeatTool::Refer, SeatTool::Complete] {
            assert!(belt.contains(&speaking));
        }
        let names: BTreeSet<_> = belt.iter().map(|tool| tool.name()).collect();
        for harness in [
            "shell",
            "file_read",
            "file_write",
            "web_fetch",
            "http_request",
            "curl",
        ] {
            assert!(!names.contains(harness), "{} holds {harness}", seat.id);
        }
    }
}

#[test]
fn a_seat_allowing_an_unserved_or_denied_tool_is_refused() {
    let (_, hive) = lowered();
    let mut seat = hive.seats[0].clone();
    seat.integration.tools.allow.push("shell".into());
    assert!(package::seat_tools(&seat).is_err());
    let mut seat = hive.seats[0].clone();
    seat.integration.tools.deny.push("hive_complete".into());
    assert!(package::seat_tools(&seat).is_err());
    let mut seat = hive.seats[0].clone();
    seat.integration.tools.allow.retain(|tool| tool != "hive_complete");
    assert!(package::seat_tools(&seat).is_err());
}

#[test]
fn tool_schemas_offer_only_colleagues_linked_desks_and_served_providers() {
    let broadcast = schema(SeatTool::Broadcast, &scope("compute"), false);
    assert_eq!(
        broadcast["properties"]["to"]["enum"],
        serde_json::json!(["a", "b"])
    );
    let refer = schema(SeatTool::Refer, &scope("compute"), false);
    assert_eq!(
        refer["properties"]["desk"]["enum"],
        serde_json::json!(["verify"])
    );
    let search = schema(SeatTool::WebSearch, &scope("research"), false);
    assert_eq!(
        search["properties"]["provider"]["enum"],
        serde_json::json!(Provider::SEAT_CHOICES)
    );
    assert!(
        schema(SeatTool::Remember, &scope("theory"), false)["properties"]["verified"].is_null()
    );
    assert!(
        schema(SeatTool::Remember, &scope("verify"), true)["properties"]["verified"].is_object()
    );
    assert!(Provider::parse(Some("exa")).is_err());
    assert!(Provider::parse(None).is_ok());
}

#[test]
fn the_federation_opens_on_the_first_desk_with_every_member_assigned() {
    let (_, hive) = lowered();
    let mut federation = Federation::new(&hive).expect("federation builds");
    assert_eq!(
        federation.linked_from("verify"),
        ["synthesis", "compute", "theory"]
    );
    federation.open("go");
    let pending = federation.ready().expect("ready");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].desk, 0);
    assert!(pending[0].blind);
    assert_eq!(pending[0].seats, hive.desks[0].members);
    let assignment: Assignment = federation.assignment(0, &hive.desks[0].members[0]);
    assert_eq!(assignment.reason, "opening");
}

#[cfg(unix)]
#[test]
fn a_symlinked_package_file_is_refused_not_followed() {
    let dir = tempfile::tempdir().expect("dir");
    let outside = dir.path().join("outside.txt");
    std::fs::write(&outside, "not part of the package").expect("outside file");
    let package = dir.path().join("package");
    std::fs::create_dir_all(&package).expect("package dir");
    std::fs::write(package.join("hive.json"), "{}").expect("hive.json");
    std::os::unix::fs::symlink(&outside, package.join("README.md")).expect("symlink");
    let error = package::read_package(&package).expect_err("the link is refused");
    assert!(error.to_string().contains("symbolic link"), "{error}");
}

#[test]
fn problem_markdown_preserves_superscripts_and_subscripts() {
    assert_eq!(package::problem_markdown("<p>2<sup>10</sup> + a<sub>i</sub>.</p>"),
        "2^(10) + a_(i).");
    assert_eq!(package::problem_markdown("<p>x<SUP class=\"math\"><b>n+1</b></SUP></p>"), "x^(n+1)");
}

#[test]
fn web_enabled_models_are_refused_before_runtime_setup() {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(16 * 1024 * 1024)
        .build().expect("runtime")
        .block_on(async { tokio::spawn(async {
            for model in ["x/y:online", "perplexity/sonar", "x/search-preview", "x/Web"] {
                let args = super::super::Args {
                    package: "does-not-exist".into(), problem: "does-not-exist".into(),
                    problem_id: "test".into(), runs: "does-not-exist".into(),
                    limits: super::super::budget::Limits::SMOKE,
                    model: model.into(), probe: false,
                };
                let error = Box::pin(super::super::run(args)).await.expect_err("refused model");
                assert!(error.to_string().contains("web-enabled model"), "{model}: {error}");
            }
        }).await }).expect("test worker");
}
