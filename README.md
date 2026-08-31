# tinyhiveminds

> [!NOTE]
>
> A note from [@senamakel](https://github.com/senamakel/).
>
> This is one of my best works so far and one of the most important libraries that I have worked on: tinyhivemind is a very crucial component, taking inspiration and learnings from my experience building harnesses, coordinating with agents, and building agents that solve a large, complex amount of problems.
> 
> The reason this repository was built was because such a crucial component, had to be well-defined, researched, coded, tested, and simulated thoroughly before it got shipped into any software module. I'm excited to share this repo as an open-source GNU Rust library, and I hope this contributes towards hivemind agents.
>
> If you like this work, give me a follow over at https://github.com/senamakel/


Group chats for agents to chat with each other.

`tinyhivemind` is the shared session substrate: a transcript that several agents
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
├── tinyhivemind-core/   # the pure algebra: no async, no IO, no host types
└── tinyhivemind/        # the session runtime: ports a host implements
                      # and attributed projection/initialization
```

`tinyhivemind-core` is linked into the hot path of every agent turn and must
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
git submodule add https://github.com/tinyhumansai/tinyhivemind.git vendor/tinyhivemind
```

```toml
[dependencies]
tinyhivemind = { path = "vendor/tinyhivemind/crates/tinyhivemind" }
```

The runtime crate re-exports `tinyhivemind-core`; hosts that only need pure desk,
roster, and mention decisions may depend on `tinyhivemind-core` directly.

Nothing here is published to crates.io, and there are no releases: the pinned
commit is the version.

## Develop

Run every command from the repository root. These four are the contract; CI
includes them alongside default-feature tests, the bundled example, the purity
guard, and the coverage gate (see `.github/workflows/ci.yml`).

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
