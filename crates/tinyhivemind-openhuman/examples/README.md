# Adapter live examples

| File | Purpose |
| --- | --- |
| `run_package.rs` | Checks a hive package, summarises what each seat runs with, calls one of its executable tools as a seat would (`--call`), or deploys it and sends one message (`--live`) |
| `run_package/` | The example's offline regressions and directory guide |
| `live_language/` | Exact completion and audience evidence, plus offline regressions |
| `live_language.rs` | Loads a live-evaluated language package, creates two model-backed seats through the management adapter with a shared memory binding, and verifies native completion, private delivery and leave access. |

## Running a package

```sh
cargo run -p tinyhivemind-openhuman --example run_package -- \
  crates/tinyhivemind-lang/examples/hives/integrations
cargo run -p tinyhivemind-openhuman --example run_package -- \
  crates/tinyhivemind-lang/examples/hives/integrations --seat scout \
  --call word_count --args '{"text":"one two three"}'
cargo test -p tinyhivemind-openhuman --example run_package
```

With no flag it prints one line per seat. `--call` runs the seat's
`pre_tool_use` hooks, then the tool, then `post_tool_use`, with no model and no
harness: edit a script and call it again. `--live` deploys the package with
`HiveDeployment::from_packages` and needs its provider credential.

## Live language package

Run from the repository root, after the language live example has produced its
accepted package:

```sh
cargo run -p tinyhivemind-openhuman --features offline --example live_language -- \
  --live target/live-self-edit-run-1/accepted-package.json target/live-language-host.json
```

Requires `OPENROUTER_API_KEY`; `OPENROUTER_MODEL` optionally overrides
`openai/gpt-oss-120b:nitro`. Incidental backend calls use a local fixture;
inference uses OpenRouter. This host supports inert, read-only run memory
bindings: no recall or store is attached. It verifies identity installation,
not persistent memory contents or cross-run learning.

The completion check matches the assignment ledger’s exact completion sequence,
episode and author. Privacy is checked in both directions by task receipt
sequence. Package seats named `host` are rejected before registration to keep
model actors distinct from the privileged management principal. Run the offline
regressions with:

```sh
cargo test -p tinyhivemind-openhuman --features offline --example live_language
```
