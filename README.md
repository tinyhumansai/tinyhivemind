# tinyhiveminds

> [!NOTE]
>
> A note from [@senamakel](https://github.com/senamakel/).
>
> This is one of my best works so far and one of the most important libraries that I have worked on: tinyhivemind takes inspiration and learnings from my experience building harnesses, coordinating with agents, and building agents that can solve large, complex problems.
> 
> This concept was initially built inside of OpenCompany but had to be later on moved into it's own standalone repo as it was too important to be left inside of OpenCompany and it had to be well-defined, researched, tested, and simulated thoroughly.
>
> I'm excited to share this with you all as an open-source contribution and if you like my work, give me a follow over at https://github.com/senamakel/ 🙌


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

## Simulate it

`tinyhivemind-hive` ships a benchmark that runs whole deliberation episodes
against reproducible synthetic rooms and scores them against two controls: the
responder ladder as it behaves today, and a matched-budget independent vote.

```sh
cargo run --release -p tinyhivemind-hive --example bench
```

```text
arm       turns/ep   decided %   correct %       ns/step    episodes/s
ladder        1.00       100.0        57.6          1109        901660
vote         15.00       100.0        78.5             0           inf
hive          6.16        89.7        73.3          2231         62637
hive+         6.75        99.4        82.1          2278         56641
```

The deliberation decides correctly more often than the matched-budget control
while spending half its turns, and the state machine costs about 2.3 µs per
step — six orders of magnitude below a model turn. `-- --sweep` tunes the
episode policy over a grid; `-- --agent-cmd "opencode run"` drives one episode
through a real agent CLI instead of simulated participants.

[`docs/benchmark.md`](docs/benchmark.md) is the full report: the two bounds on
the quorum threshold, why the budget has to scale with the desk, what happens
to accuracy without the blind round, and what the numbers do and do not claim.
[`crates/tinyhivemind-hive/examples/bench/README.md`](crates/tinyhivemind-hive/examples/bench/README.md)
documents the harness itself.

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
