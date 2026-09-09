# Repository Guidelines

This file is the single source of truth for how humans and coding agents work
in this repository. `CLAUDE.md` is a symlink to this file, so every agent reads
the same instructions.

## Charter

`tinyhivemind` is **hive mind mechanics for agents**: a shared session
transcript that several agents read and write, the mechanism by which a message
triggers the right agent to run a turn, and the swarm mechanisms by which a
room of them reaches a decision. Stigmergy, decaying salience, quorum sensing,
cross-inhibition and response thresholds, all as pure folds.

It answers four questions and holds no state doing it — who is here, what a desk
is and who is on it, who an authored mention addresses, and what one participant
sees of the shared transcript.

Three rules decide what belongs here:

1. **The host owns storage.** This repository never opens a database, a file, or
   a socket. `crates/tinyhivemind-core` is a pure algebra; `crates/tinyhivemind` owns
   *ports* a host implements, and nothing more. In particular there is no second
   append-only journal: messages are addressed by sequence number across
   surfaces the host owns, so a second log could not be made consistent with the
   first.
2. **No host types, ever.** Nothing here may name a type from a consuming
   application. A snapshot or a borrowed view crosses the boundary, never a
   callback into the host — a callback seam is how the layering violation this
   crate exists to fix grew in the first place.
3. **One message, one round, of bounded width.** A step may authorize several
   turns to run concurrently — seats are async sessions and the algebra says so
   — but never more than `round_width`, and never without an approval in sight.
   The bound is the invariant; the serialization never was. See
   [ADR 0014](docs/adr/0014-a-round-authorizes-concurrent-turns.md), which
   supersedes ADR 0002 on the terms ADR 0002 itself set.

`crates/tinyhivemind-core` additionally may not depend on an async runtime, a
transport, an HTTP client, a web framework, a SQL database client, a git
implementation, or `anyhow` (non-exhaustive — see the enumerated list in
`.github/scripts/assert-pure.sh`, the source of truth): it is linked into the
hot path of every agent turn and must compile in a host's default build with no
feature flags behind it. `.github/scripts/assert-pure.sh` asserts this, and it
is not advisory — do not add an exception to it to land a change.

`ROADMAP.md` holds the phase plan and the two defects this work exists to fix.

## Project Structure

This is a Rust 2024 cargo workspace rooted at a virtual `Cargo.toml`. Every
crate lives under `crates/`, one directory per package, each directory named for
the package it holds. There is no root package.

```text
Cargo.toml              # virtual workspace: members, [workspace.package],
                        # [workspace.dependencies], [workspace.lints]
crates/
├── tinyhivemind-core/     # the pure algebra: no async, no IO, no host types
│   └── src/
│       ├── lib.rs      # crate docs + the entire public re-export surface
│       ├── error/mod.rs      # crate-wide `Error` and `Result<T>`
│       └── <feature>/        # one directory per feature area
│           ├── README.md     # the module's design, surface, and constraints
│           ├── mod.rs        # module docs, wiring, smallest useful public API
│           ├── types.rs      # substantial type definitions
│           └── test.rs       # module-local unit tests, or a test/ directory
│                             # of behavior-grouped submodules once it grows
├── tinyhivemind/          # the session runtime: ports, the paging walk, the
│                       # responder ladder. Lands in P4; see ROADMAP.md.
└── tinyhivemind-hive/     # bounded group deliberation: traces, salience, quorum
                        # with cross-inhibition, the attention market, and the
                        # episode state machine. Pure, opt-in; lands in P8.
docs/
├── specs/              # behavior and architecture specifications
├── plans/              # test-first implementation plans
├── adr/                # immutable architecture decision records
├── research/           # the reading behind a mechanism, with its equations
└── experiments/        # what happened when it was actually run
wiki/                   # the GitHub wiki, checked out as a submodule
```

### The two-crate split

`crates/tinyhivemind-core` holds the algebra: desks and membership, the roster, the
mention grammar and its resolution, and the fold that projects a shared
transcript into one viewer's turn history. Every function there is a fold over
data the caller already holds. P4 (see `ROADMAP.md`) replaces the
`(role, content)` pair this fold produces with an attributed `SessionMessage`
in `crates/tinyhivemind` — the projection *algorithm* stays here in core, but the
richer, attributed shape it produces is assembled by the crate that also owns
the paging walk over a live session log.

`crates/tinyhivemind` holds the parts that must wait on something — the paging walk
over a session log, the responder ladder, the mention-dispatch edge — expressed
against ports a host implements. It depends on the core crate and re-exports it,
so a host takes one dependency rather than two and the types are the *same*
types rather than structural twins.

The rule for deciding where something goes: if it can be answered from arguments
alone it belongs in the core crate; if it has to await a read, a write, or a
model call, it belongs behind a port in the runtime crate. When in doubt, put
the decision in the core crate and the waiting in the runtime crate — that split
is what keeps the interesting logic testable without a fixture.

### The hive crate

`crates/tinyhivemind-hive` is opt-in and answers a different question from the
other two: not *who responds to this message* but *how does a room of agents
reach a decision*. It is **pure and defines no port** — an episode is
`step(state, transcript, roster, desks, policy) -> HiveStep`, a fold over
arguments the caller already holds, and the host does its waiting through the
`SessionLog`, `Selector` and `MentionTurnQueue` ports it already implements. It
is in the `pure_crates` list in `.github/scripts/assert-pure.sh` for that
reason.

`HiveStep::Speak` carries a **round** — the turns authorized to run
concurrently, at most `round_width` of them, plus the single state the episode
takes once all of them are appended. Independence is still a visibility filter
rather than a hope: members writing simultaneously cannot read each other, so a
concurrent round *is* a blind round. `round_width: 1` is the sequential episode
and reproduces every recorded number bit-for-bit. See
[`docs/specs/concurrent-rounds.md`](docs/specs/concurrent-rounds.md) and
[`docs/adr/0014-a-round-authorizes-concurrent-turns.md`](docs/adr/0014-a-round-authorizes-concurrent-turns.md).

All arithmetic in it is fixed-point integer, so every payload derives `Eq` and
every fold is reproducible.

Add a crate by creating `crates/<name>/` — `members = ["crates/*"]` picks it up
by existing. Inherit `version`, `edition`, `rust-version`, `license`, and
`repository` from `[workspace.package]`, take shared dependencies from
`[workspace.dependencies]`, and opt into the shared lint set with:

```toml
[lints]
workspace = true
```

Each feature area belongs in a focused module directory under a crate's `src/`.
A module root explains the module, wires its pieces together, and exposes the
smallest useful API. Move substantial type definitions into `types.rs` and put
module-local unit tests in a dedicated `test.rs`, wired from the bottom of the
module root with:

```rust
#[cfg(test)]
mod test;
```

When a `test.rs` outgrows the 700-line file cap, promote it to a `test/`
directory: a `test/mod.rs` carrying the module doc and the `mod` declarations,
one submodule per behavior area named for the behavior it covers, and shared
fixtures in `test/support.rs`. The `#[cfg(test)] mod test;` line in the module
root is unchanged by that promotion.

Do not accumulate inline `mod tests` blocks in implementation files, and do not
let a general-purpose `utils.rs` or `helpers.rs` grow — those are a symptom of a
missing module. Prefer many small modules that each do one thing well over few
broad ones.

No source file exceeds **700 lines**. Split along a real seam — a cohesive group
of functions or types — never at an arbitrary line count.

Keep public exports centralized in each crate's `src/lib.rs` so downstream users
have one predictable surface. Put shared error variants in
`crates/tinyhivemind-core/src/error/mod.rs` and return the crate-wide `Result<T>` from
fallible public APIs.

## Build And Test

Run every command from the repository root. These four are the contract; CI
runs exactly them, so a green local run should mean a green CI run.

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo build --all-targets --all-features
cargo test --all-features
```

Supporting commands:

- `cargo fmt --all` — format before committing.
- `cargo test <filter>` — run a focused subset while iterating.
- `cargo test -p tinyhivemind-core` — run one crate's suite.
- `cargo run -p tinyhivemind-core --example basic` — run the bundled example.
- `cargo run -p tinyhivemind --example crosstalk -- --api-base <url> --model <id>`
  — drive one desk of real agents through the responder ladder and the
  mention-dispatch edge, and print what each turn saw. `-- --aside` adds
  private asides and prints what each reader was handed. Needs a live endpoint
  or `--agent-cmd`; documented in
  `crates/tinyhivemind/examples/crosstalk/README.md`.
- `cargo run -p tinyhivemind-hive --example hive` — print one deliberation episode.
- `cargo run --release -p tinyhivemind-hive --example bench` — simulate and
  benchmark deliberation against the responder ladder and a matched-budget
  vote; `-- --sweep` tunes the episode policy, `-- --trace` prints one episode,
  `-- --swarm` runs a federation of desks that can only reach each other by a
  referral, and `-- --agent-cmd "opencode run"` drives one through a real agent
  CLI. The harness is documented in
  `crates/tinyhivemind-hive/examples/bench/README.md` and its findings on the
  wiki's `Benchmarks` page (`wiki/Benchmarks.md`).
- `.github/scripts/assert-pure.sh` — assert the pure crates took on no
  runtime, transport, or web-framework dependency.
- `cargo doc --no-deps --all-features` — build the rustdoc CI also builds with
  `RUSTDOCFLAGS="-D warnings"`.
- `cargo test --doc` — run doctests alone when editing documentation examples.

Never skip, ignore, or delete a failing test to make a command pass. Fix the
root cause, or stop and report the blocker.

## Coding Style

Use standard `rustfmt` output and Rust 2024 idioms. Do not hand-format around
`rustfmt`, and do not add `#[rustfmt::skip]` without a comment explaining why.

- `snake_case` for modules, files, functions, methods, fields, and locals.
- `PascalCase` for types, traits, and enum variants; `SCREAMING_SNAKE_CASE` for
  constants and statics.
- Name things for what they are, not for their layer: `RetryPolicy`, not
  `RetryHelper`.
- Prefer small, typed APIs over stringly-typed ones. Accept `&str` and generic
  `impl Into<String>` at boundaries; return owned, concrete types.
- Keep the public surface minimal: default to private, and export deliberately
  from `src/lib.rs`.
- `unsafe` is forbidden workspace-wide by `[workspace.lints]` in the root
  `Cargo.toml`. If a project genuinely needs it, relax the lint in its own
  commit and document every invariant with a `// SAFETY:` comment.

### Errors

- One crate-wide `Error` enum per crate, in `src/error/mod.rs`, built with
  `thiserror`.
- Fallible public functions return `Result<T>`, the crate alias.
- Add a specific variant instead of stuffing context into a string; error
  messages are lowercase, without trailing punctuation.
- Do not `unwrap()`, `expect()`, or `panic!` in library code paths. They are
  fine in tests, examples, and genuinely unreachable states — where `expect`
  must carry a message explaining the invariant.
- Document a `# Errors` section on every public fallible function and a
  `# Panics` section on anything that can panic.

### Dependencies

Adding a dependency is a design decision. Before adding one, check whether the
standard library or an existing dependency already covers the need. When you do
add one:

- pin a caret range (`serde = "1"`), not an exact version;
- enable only the features you need, with `default-features = false` when that
  meaningfully trims the tree;
- gate anything optional behind a Cargo feature, documented in `Cargo.toml`;
- declare it once in the root `[workspace.dependencies]` when more than one
  crate needs it, and take it with `{ workspace = true }`;
- never add one to `crates/tinyhivemind-core` that pulls in a transport, an async
  runtime, an HTTP client, a web framework, a SQL database client, a git
  implementation, or `anyhow` (non-exhaustive — see
  `.github/scripts/assert-pure.sh`) — CI fails the build if you do;
- leave a comment above the entry explaining *why* the crate is needed and what
  uses it — see the existing entries for the expected tone;
- prefer well-maintained crates with a compatible license.

Keep `Cargo.lock` committed; this workspace ships a single lockfile so CI and
releases are reproducible.

### Vendored dependencies

No library crate has one, and that is deliberate. This repository is itself
vendored — a consumer pins it as a submodule and takes it as a path dependency —
so anything a library crate vendored in turn would become a nested submodule in
every consumer.

An **example** may take one, as a `[dev-dependencies]` git dependency pinned by
revision. A consumer builds `crates/*` and never the examples, so it never
resolves them, and `assert-pure.sh` reads `cargo tree -e normal,build` and so
guards the same boundary unchanged. `tinytools` and `tinyinference` back the
`desk` example on those terms; see
[ADR 0013](docs/adr/0013-a-vendored-crate-is-an-example-dependency.md). Neither
could be a library dependency: `tinytools` pulls `anyhow` and `tinyinference`
pulls `reqwest`, and both are forbidden in every crate here.

The one submodule here is `wiki/`, the GitHub wiki repository
(`tinyhumansai/tinyhivemind.wiki`). It carries no code and nothing builds
against it, so a clone that skips it still compiles. Run
`git submodule update --init` when you need to edit documentation.

## Testing

- Module-local unit tests live in `crates/<crate>/src/<feature>/test.rs`, or in
  a `test/` directory of behavior-grouped submodules once that file would pass
  700 lines, and may touch private items.
- Integration tests live in `crates/<crate>/tests/` and exercise only the public
  API — they are the regression suite for the crate's contract.
- Payload types pin their serde representation in a unit test. That
  representation is the wire form: a host and a module that disagree about a
  field name fail at runtime with a decode error.
- Use descriptive, behavioral test names: `rejects_an_empty_name`, not
  `test_greet_2`.
- Cover the failure paths, not just the happy path. Every new error variant
  needs a test that produces it.
- For async behavior, standardize on one runtime (`tokio` as a dev-dependency
  for tests) rather than mixing runtimes.
- Tests must be deterministic and independent of network, wall-clock time, and
  execution order. Gate any live/network test behind a feature or an env var and
  name it `live_*` so it is easy to exclude.
- Maintain at least 90% line coverage in every source file. Add or update tests
  with every behavior change, and note any deliberately untested edge case in
  the pull request description.

Write the test first when fixing a bug: a failing test that reproduces the
report, then the fix that turns it green.

## Documentation

Write documentation for the reader who has never seen the code.

- Every public item gets a rustdoc comment. `missing_docs` is a warning that CI
  treats as an error.
- Start every `mod.rs` and `test.rs` with a concise module-level `//!`
  description.
- Each crate's `src/lib.rs` carries its crate-level overview: what the crate
  does, the primary entry points, and a short runnable example. It should also
  say what the crate deliberately does *not* hold, and why.
- Prefer concrete examples over vague description. Doc examples are compiled and
  run by `cargo test`, so they cannot drift.
- Every directory holding source carries a `README.md` saying what the directory
  is for and what each file in it does. For a feature module that means its
  design, public surface, and important operational constraints; for a `test/`
  or example subdirectory a short file-by-file table is enough. A crate root
  `README.md` and a `src/README.md` index sit above them, and neither duplicates
  the repository root `README.md` — they link it.
- Keep `README.md`, `docs/`, `wiki/`, and module docs aligned with code changes
  in the same commit that changes behavior.
- `README.md` is marketing. It says what the library is, why it is worth using,
  and where to read more. It does not carry reference material, build
  instructions, or a benchmark report.
- `wiki/` carries the narrative documentation: architecture, the concepts, the
  deliberation protocol, the benchmark report, host integration, and the
  development guide. It is a separate git repository, so a change there is
  pushed on its own and the pointer bump is committed here.
- `docs/` keeps the working record: specs, plans, ADRs, and testing notes.
- Write accepted behavior and constraints in `docs/specs/` before creating a
  linked, implementation-ordered plan in `docs/plans/`. Specs define what and
  why; plans define how and in what sequence.
- Keep every Markdown file, including this one, at 500 lines or fewer. When a
  topic outgrows that, split it into focused files and link them from the
  nearest `README.md`.

## Git Workflow

- Never commit directly to `main`. Branch first, one branch per logical change.
- Do feature work in a git worktree so the main checkout stays clean.
- Commit subjects are concise and imperative: `Add retry policy to the client`.
  Keep the subject specific to the change and under ~72 characters.
- Make small, focused commits. Each commit should cover one logical change,
  build independently, and avoid mixing formatting, refactors, and behavior
  changes unless they are inseparable.
- Never commit secrets. `.env` is git-ignored; document new variables in
  `.env.example` with placeholder values.
- Never force-push a shared branch, rewrite published history, or bypass hooks
  with `--no-verify`.

## Pull Requests

Open pull requests ready for review, not as drafts, unless the work genuinely
must not merge yet. A pull request should:

- summarize what changed and why, in a few sentences;
- call out public API or behavior changes explicitly, or state "None";
- list the validation commands actually run, with their outcome;
- link the related issue;
- include updated tests, docs, and examples in the same change.

The template in `.github/PULL_REQUEST_TEMPLATE.md` encodes this checklist.
Address review feedback by fixing it, and reply on each thread describing what
changed. Do not resolve a thread whose feedback you have not addressed or
explicitly declined with a reason.

## Releases

There are none. Every crate here is `publish = false`, and a consumer pins this
repository as a git submodule and takes the crates as path dependencies — so the
pinned commit *is* the version, and a tag would be a second, weaker name for it.

Consequently:

- Land work on `main` and let the consumer bump its submodule pointer. The
  pointer bump is the release.
- The paired pull request in the consuming repository comes **second**. A
  submodule cannot point at an unmerged commit, which is what makes the
  dependency direction self-enforcing.
- `main` should always be green, because a consumer may pin any commit on it.
- Do not hand-edit `version` in the root `[workspace.package]`. It is inherited
  by every member and moves as one; nothing consumes it today.

## Agent Working Agreement

For automated contributors specifically:

1. **Read before writing.** Inspect the surrounding module and match its
   conventions, comment density, and idiom rather than importing a house style.
2. **Verify, do not assume.** Run the four contract commands and read their
   output before reporting a task complete. Report failures with the output;
   never claim a check passed that you did not run.
3. **Stay in scope.** Implement what was asked. Do not opportunistically
   refactor, reformat, upgrade dependencies, or "fix" unrelated code — raise it
   instead.
4. **No placeholders in delivered code.** No `todo!()`, no stubbed functions, no
   commented-out alternatives left behind. If something cannot be finished, say
   so explicitly.
5. **Do not weaken the guardrails.** Never add blanket `#[allow(...)]`, relax a
   lint, mark a test `#[ignore]`, or loosen CI to get a green run. Fix the
   cause.
6. **Secrets stay out.** Never read, echo, or commit `.env` contents, tokens, or
   credentials, and never paste them into a pull request or issue.
7. **Ask only when blocked.** Make routine judgment calls yourself; escalate
   only irreversible decisions or genuine forks with no clear default.
