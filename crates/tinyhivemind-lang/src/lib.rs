//! The hive language: a declarative, portable definition of a hive of agents,
//! validated and lowered into core values without opening storage.
//!
//! A *package* is a map from relative path to file contents. Three canonical
//! JSON files carry the typed definition, a large manifest may be split over
//! three optional ones, and Markdown side documents carry the prose:
//!
//! | Path | Decoded into | Holds |
//! | --- | --- | --- |
//! | `hive.json` | [`Manifest`] | roles, seats, desks, goals, missions, topology, referrals, context paths, core policies, coordinator settings |
//! | `agents.json`, `tools.json`, `desks.json` | [`Manifest`] | optional: `hive.json` sections moved out of it (agents: roles, seats, profiles; tools: tools, toolpacks, mcp, skills, hooks; desks: desks, topology, referrals) |
//! | `constitution.json` | [`Constitution`] | host-pinned caps, permitted edit classes, protected namespaces, canaries, opaque evaluator settings |
//! | `memory.json` | [`MemorySpec`] | memory identities and seat bindings, never remembered contents |
//! | `roles/*.md` | [`Package::documents`] | one charter per role |
//! | `seats/*.md` | [`Package::documents`] | one prompt per seat, with optional JSON frontmatter |
//! | `context/*.md` | [`Package::documents`] | shared documents split into stable `<!-- item:id -->` items |
//! | `tools/*` | [`Package::documents`] | executable tool and hook sources, in any language |
//! | `skills/<id>/…` | [`Package::documents`] | a skill bundle; `skills/<id>/SKILL.md` is required |
//!
//! # What the agents run with
//!
//! Besides who is in a hive and how it deliberates, `hive.json` says what its
//! agents run on, in the sections `runtime`, `providers`, `models`, `mcp`,
//! `skills`, `tools`, `toolpacks`, `permissions`, `profiles`, `hooks`,
//! `session`, `jev`, `features` and `workflows` (see
//! [`RuntimeSettings`], [`Provider`], [`Model`], [`McpServer`], [`Toolpack`],
//! [`PermissionSet`], [`Profile`], [`Hook`] and [`Workflow`]). A seat names a
//! `profile`, `model`, `toolpacks`, `hooks`, `permissions`, `limits` and extra
//! `context`; each is resolved by [`lower`] with runtime defaults first, the
//! seat's profile next, and the seat last. Every section is optional and
//! defaults to empty, so a package without them is unchanged. The vocabulary
//! is exactly what the `OpenHuman` harness can run: a setting it cannot honour
//! is a located diagnostic, never silently dropped. Credentials are
//! [`SecretRef`]s (`{"env": "NAME"}` or `{"store": "key"}`), never values.
//!
//! An [`ExecTool`] declares a tool as a program plus an argument schema. The
//! program may be a script shipped under `tools/` or any binary on `PATH`;
//! arguments reach it as JSON on stdin or through an `argv` template, and
//! its stdout is the result. A [`Hook`] binds such a tool to a
//! `pre_tool_use`, `post_tool_use` or `stop` event. The crate only declares
//! and validates these; the `tinyhivemind-exec` crate runs them, and the
//! convention is in `docs/specs/executable-tools.md`.
//!
//! # Editing a hive
//!
//! An orchestrator or editor agent changes a hive by editing its files
//! directly, the way a programmer edits source. The host then calls
//! [`check()`] with the original and edited document maps, and the crate acts
//! as the compiler: it runs every language rule and every constitution rule,
//! and either returns *every* violation as a located [`Diagnostic`]
//! (`hive.json:/policies/episode/turn_budget: forbidden_class: ...`) or a
//! [`Review`] holding the edit split into one [`patch::Patch`] per touched
//! edit class. The `integration` class ([`patch::PatchClass::Integration`])
//! owns the integration sections, a seat's integration fields and the files
//! under `tools/` and `skills/`, which are edited whole; a constitution that
//! does not permit it stops a hive from changing what its agents can run or
//! reach. Those patches are the recorded form the host keeps in
//! [`lineage`]; nobody has to author one. The `hive_check` example is the
//! same check as a command an agent runs after editing.
//!
//! # Entry points
//!
//! - [`check()`] and [`check_packages`] check a direct edit against the
//!   original and the constitution, collecting every finding.
//! - [`diagnose`] and [`diagnose_documents`] collect every validation or
//!   parse finding of one package, located by file and JSON Pointer.
//! - [`parse`] decodes a caller-supplied document map into a [`Package`] and
//!   validates it.
//! - [`validate`] checks references, paths, policy bounds against the
//!   constitution, frontmatter agreement, and memory reaches.
//!   [`validate_path`] and [`validate_namespace`] expose its path and
//!   namespace checks on their own.
//! - [`lower`] turns a valid package into a [`LoweredHive`]: core desks,
//!   routing candidates, per-seat factory inputs the host activates, the
//!   hive-wide [`LoweredIntegrations`], and each seat's resolved
//!   [`LoweredSeatIntegration`] (model, MCP servers, skills, executable tools,
//!   hooks, permissions, limits).
//! - [`patch::apply`], [`patch::guard`] and [`patch::diff`] apply, check and
//!   compute the recorded form of an edit, a classified [`patch::Patch`].
//!   Memory learning operations, which are not files, are proposed this way.
//! - [`lineage::content_id`], [`lineage::record`], [`lineage::accepted_head`]
//!   and [`lineage::pareto_frontier`] address evaluated designs by content and
//!   fold an archive of them.
//!
//! # What this crate does not hold
//!
//! It reads no files: the host collects the document map and hands it to
//! [`parse`]. It runs no processes and resolves no credentials: an executable
//! tool is declared here and run by `tinyhivemind-exec`, a [`SecretRef`] is
//! resolved by the adapter at deployment. It runs no agents and calls no model: [`lower`] produces the
//! inputs a host needs to create seats, and the host decides when to activate
//! them. It owns no evaluation, archive storage, or memory store: those are
//! host-owned, and [`lineage`] only folds records the host supplies. It does
//! not convert to `OpenHuman` types either; that adapter lives in the
//! `tinyhivemind-openhuman` crate's `language` module, so this crate stays
//! host-neutral.
//!
//! # Example
//!
//! ```
//! use std::collections::BTreeMap;
//! use tinyhivemind_lang::{Policies, lower, parse};
//!
//! // Only what has no safe default is written: every omitted section, seat
//! // field and policy takes its default, and the constitution's caps default
//! // to 60 turns and a width of 8.
//! let hive = serde_json::json!({
//!     "$schema": "https://tinyhumans.ai/schemas/hive/v1",
//!     "version": 1,
//!     "id": "review",
//!     "name": "Review board",
//!     "roles": [{ "id": "reviewer", "charter": "roles/reviewer.md" }],
//!     "seats": [{
//!         "id": "reviewer",
//!         "role": "reviewer",
//!         "prompt": "seats/reviewer.md",
//!         "template": "default",
//!         "tools": { "allow": ["file_read"] }
//!     }],
//!     "desks": [{ "id": "board", "name": "Board", "members": ["reviewer"] }],
//!     "policies": { "episode": { "turn_budget": 8 } }
//! });
//! let constitution = serde_json::json!({});
//! let memory = serde_json::json!({ "root": "team:hive" });
//!
//! let documents = BTreeMap::from([
//!     ("hive.json".to_owned(), hive.to_string()),
//!     ("constitution.json".to_owned(), constitution.to_string()),
//!     ("memory.json".to_owned(), memory.to_string()),
//!     ("roles/reviewer.md".to_owned(), "Judge changes on evidence.\n".to_owned()),
//!     (
//!         "seats/reviewer.md".to_owned(),
//!         "---\n{\"id\":\"reviewer\",\"role\":\"reviewer\"}\n---\nCite a line for every finding.\n"
//!             .to_owned(),
//!     ),
//! ]);
//!
//! let package = parse(&documents)?;
//! let hive = lower(&package)?;
//! let seats: Vec<_> = hive.seats.iter().map(|seat| seat.id.as_str()).collect();
//! assert_eq!(seats, ["reviewer"]);
//! // Lowering hands the host the prompt body without its frontmatter.
//! assert_eq!(hive.seats[0].prompt, "Cite a line for every finding.\n");
//! assert_eq!(hive.desks[0].members, ["reviewer"]);
//! // An omitted label is the seat's id; an omitted policy field its default.
//! assert_eq!(package.manifest.seats[0].label, "reviewer");
//! let episode = package.manifest.policies.episode;
//! assert_eq!(episode.turn_budget, 8);
//! assert_eq!(episode.quorum, Policies::default().episode.quorum);
//! # Ok::<(), tinyhivemind_lang::Error>(())
//! ```
//!
//! # Example: declaring an executable tool
//!
//! ```
//! use tinyhivemind_lang::{ExecTool, SideEffects, ToolOutput, ToolStdin};
//!
//! // Only the name, description and command are required; everything else
//! // defaults: JSON on stdin, text out, 60 s, and the permission gate treats
//! // the call as a write.
//! let tool: ExecTool = serde_json::from_value(serde_json::json!({
//!     "id": "word_count",
//!     "description": "Count the words of a text",
//!     "command": ["python3", "tools/word_count.py"],
//!     "args": {
//!         "type": "object",
//!         "properties": { "text": { "type": "string" } },
//!         "required": ["text"]
//!     }
//! }))?;
//! assert_eq!(tool.stdin, ToolStdin::Json);
//! assert_eq!(tool.output, ToolOutput::Text);
//! assert_eq!(tool.timeout_ms, 60_000);
//! assert_eq!(tool.side_effects, SideEffects::Write);
//! # Ok::<(), serde_json::Error>(())
//! ```
//!
//! # Example: checking a direct edit
//!
//! ```
//! use std::collections::BTreeMap;
//! use tinyhivemind_lang::{Package, Role, Rule, Seat, check, patch::PatchClass};
//!
//! let mut package = Package::default();
//! package.constitution.permitted_edits = vec![PatchClass::Prompt];
//! package.manifest.roles.push(Role { id: "writer".into(), charter: "roles/writer.md".into() });
//! package.manifest.seats.push(Seat {
//!     id: "writer".into(),
//!     label: "Writer".into(),
//!     role: "writer".into(),
//!     prompt: "seats/writer.md".into(),
//!     template: "default".into(),
//!     ..Seat::default()
//! });
//! let mut original = BTreeMap::from([
//!     ("roles/writer.md".to_owned(), "Write clearly.\n".to_owned()),
//!     ("seats/writer.md".to_owned(), "Draft the summary.\n".to_owned()),
//! ]);
//! original.insert("hive.json".into(), serde_json::to_string_pretty(&package.manifest)?);
//! original.insert("constitution.json".into(), serde_json::to_string_pretty(&package.constitution)?);
//! original.insert("memory.json".into(), serde_json::to_string_pretty(&package.memory)?);
//!
//! // The editor rewrites the seat prompt file. The check accepts it as one
//! // prompt-class change.
//! let mut edited = original.clone();
//! edited.insert("seats/writer.md".into(), "Draft the summary and cite sources.\n".into());
//! let review = check(&original, &edited).expect("a permitted prompt edit");
//! assert_eq!(review.changes.len(), 1);
//! assert_eq!(review.changes[0].class, PatchClass::Prompt);
//!
//! // Raising its own turn cap in the constitution is refused, located at the
//! // field it changed.
//! let mut tampered = original.clone();
//! tampered.insert(
//!     "constitution.json".into(),
//!     original["constitution.json"].replace("\"max_turns\": 60", "\"max_turns\": 600"),
//! );
//! let diagnostics = check(&original, &tampered).expect_err("the constitution is immutable");
//! assert_eq!(diagnostics[0].rule, Rule::Immutable);
//! assert_eq!(diagnostics[0].file, "constitution.json");
//! assert_eq!(
//!     diagnostics[0].to_string(),
//!     "constitution.json:/max_turns: immutable: the constitution is host-owned and no edit may change it"
//! );
//! # Ok::<(), serde_json::Error>(())
//! ```
pub mod check;
pub mod diagnostic;
pub mod document;
pub mod error;
pub mod lineage;
pub mod lowering;
pub mod memory;
pub mod patch;
pub mod validation;
pub use check::{Review, check, check_packages};
pub use diagnostic::{Diagnostic, Rule};
pub use document::*;
pub use error::{Error, Result};
pub use lowering::{
    LoweredHive, LoweredHook, LoweredIntegrations, LoweredMemoryBinding, LoweredModel, LoweredSeat,
    LoweredSeatIntegration, lower,
};
pub use memory::*;
pub use validation::{diagnose, validate, validate_namespace, validate_path};
