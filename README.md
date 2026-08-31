# tinyteams

Group chats for agents to chat with each other.

`tinyteams` is the shared session substrate: a transcript that several agents
read and write, and the mechanism by which a message reaches the right agent.
It answers four questions and holds no state doing it.

- **Who is here?** A roster of teammates, and the people signed in alongside
  them.
- **What is a desk, and who is on it?** A blueprint-declared group chat merged
  with the operator's runtime additions, retirements and ordering.
- **Who does `@this` mean?** The mention grammar, and resolution of a name
  against the roster and the desks.
- **What does one participant see?** The projection of a multi-speaker
  transcript into one viewer's turn history.

It is being built by moving that layer out of
[`opencompany`](https://github.com/tinyhumansai/opencompany) and fixing two
defects it has there. See [`ROADMAP.md`](ROADMAP.md) for the phase plan.

## Layout

A two-crate cargo workspace, split by whether an answer has to wait on
something.

```text
crates/
├── tinyteams-core/   # the pure algebra: no async, no IO, no host types
└── tinyteams/        # the session runtime: ports a host implements
                      # (lands in P4 — see ROADMAP.md)
```

`tinyteams-core` is linked into the hot path of every agent turn and must
compile in a host's default build with no feature flags. It may not depend on an
async runtime, a transport, an HTTP client, or a web framework;
`.github/scripts/assert-pure.sh` asserts it in CI.

## What it deliberately does not do

- **It owns no storage.** No database, no file, no socket. The host owns the
  append-only log and lends it through a port. There is no second journal:
  messages are addressed by sequence number across surfaces the host owns —
  reactions, board cards, run rows — so a second log could not be made
  consistent with the first.
- **It serves no HTTP.** Routes, authorization and rendering stay with the host.
- **It does not fan out.** One message triggers exactly one turn. `@everyone`
  resolves to a list named in that turn's context, not to a turn each.

## Use it

Pin the repository as a submodule and take the crate as a path dependency:

```sh
git submodule add https://github.com/tinyhumansai/tinyteams.git vendor/tinyteams
```

```toml
[dependencies]
tinyteams-core = { path = "vendor/tinyteams/crates/tinyteams-core" }
```

Nothing here is published to crates.io, and there are no releases: the pinned
commit is the version.

## Develop

Run every command from the repository root. These four are the contract; CI runs
exactly them.

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo build --all-targets --all-features
cargo test --all-features
```

Plus the guards CI also runs:

```sh
.github/scripts/assert-pure.sh
.github/scripts/check-file-coverage.sh 90 coverage.json
```

Every source file must hold at least 90% line coverage.
[`AGENTS.md`](AGENTS.md) is the full working agreement for humans and coding
agents alike; `CLAUDE.md` is a symlink to it.

## License

GPL-3.0-only. See [`LICENSE`](LICENSE).
