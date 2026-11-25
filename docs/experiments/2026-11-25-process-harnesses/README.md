# Repeating the process harness spike

Companion to [the record](../2026-11-25-process-harnesses.md). It holds what is
needed to repeat the gate and the measurements, and nothing that belongs to a
vendor: no binary, log, database or model catalog.

| Directory | Holds |
| --- | --- |
| [`fixture/`](fixture/README.md) | the scripted model and MCP stub all runs share |
| [`openhuman/`](openhuman/README.md) | config, environment, client and run for `openhuman-core` |
| [`openclaw/`](openclaw/README.md) | config, client and run for the OpenClaw Gateway |
| [`codex/`](codex/README.md) | config, catalog rewrite, client and run for `codex app-server` |
| [`baseline/`](baseline/README.md) | the linked OpenHuman program the table compares with |

## Versions and checksums

Linux x86_64. sha256 unless stated.

| What | Version | Checksum |
| --- | --- | --- |
| `openhuman-core-0.64.15-x86_64-unknown-linux-gnu.tar.gz` | 0.64.15 | `f86ae0157b4ed28767c580c443e04027b62fe14e6f3260786f44cd265a242587` |
| `openhuman-core` inside it | 0.64.15 | `4788efd57d66ee43ae398c99875a2c84e006691e9fbefab60e5a22c0fb7fefce` |
| `openclaw-2026.10.33.tgz` (npm) | 2026.10.33 | integrity `sha512-OMJvs7fSwBPBENfeghTDKo2uxQ2r6zkIH1KcYevSEY6Ca3DTEunbfuPlSE+lmvOZFmyLLefeQgcoVExqEV7IeQ==` |
| `node-v24.22.3-linux-x64.tar.xz` | 24.22.3 | `4f2b175a6d53a5c617a80dba2676238a0688cef28c6da4eba35c7d844e66fcfb` |
| `codex`, standalone musl build | 0.171.0 | `b76a027d6b210f9a8da7aaf321dd235c700f0ef7a794d4a5314b9af6c134f0f0` |
| linked OpenHuman | `5f8b7c235f18b46f90fabbf1e7f1b8d9a957cee6` | git |

Fetch each from its vendor and check it. OpenClaw: `npm install openclaw@2026.10.33`
in an empty directory; the spike's lock resolved that tarball.

## Run

Use absolute paths, and one fresh work directory per harness under the
git-ignored `target/`. Node 24 runs every script.

```sh
W=$PWD/target/repro; mkdir -p "$W"/{oh,oc,cx}; D=docs/experiments/2026-11-25-process-harnesses
node $D/fixture/server.mjs 19501 "$W/cx.log" &
node $D/fixture/server.mjs 19502 "$W/oh.log" &
node $D/fixture/server.mjs 19503 "$W/oc.log" &
node $D/codex/run.mjs     /abs/codex               "$W/cx"       19501 "$W/cx.log"
node $D/openhuman/run.mjs /abs/openhuman-core      "$W/oh" 19602 19502 "$W/oh.log"
node $D/openclaw/run.mjs  /abs/node_modules/openclaw "$W/oc" 19603 19503 "$W/oc.log"
```

Each `run.mjs` prints the gate (tool list, hive call and its seat, usage, cancel,
same session after restart, `kill -9`) and the measurements in the record's table.
The fixture always replies, so `CALLTOOL`, `PLAIN` and `SLOW` in a prompt pick the
scripted behaviour. Tokens such as `seat-a-token` are placeholders, not secrets.
Egress is not blocked by these scripts. Run them under a network namespace or
firewall to repeat the traffic check; the spike used
`strace -f -e trace=connect` on an idle Codex thread for 25 s.

## What was kept

| Script | State |
| --- | --- |
| `fixture/server.mjs`, `openhuman/rpc.mjs`, `openclaw/gw.mjs`, `openclaw/mkcfg.mjs`, `baseline/main.rs` | as used in the spike |
| `openhuman/start.sh`, `openhuman/config.toml.in`, `codex/config.toml.in`, `codex/lib.mjs` | as used, with paths, ports and names turned into parameters; `lib.mjs` trimmed |
| `baseline/manifest.sh` | new; regenerates the spike's `Cargo.toml`, whose patch tables equal the repository's |
| `codex/catalog.mjs` | new; its output was compared with the spike's hand-rewritten catalog and is equal model by model |
| each `run.mjs` | new; condenses the spike's numbered scripts to the gate and the measurements |

Not kept: the spike's numbered scripts and their raw outputs. Those cover the
side findings: hooks and approvals, forced calls across seats, OpenClaw recovery
mitigations, redirects and scopes, Codex code mode, dynamic tools, `exec` and
shell children, the iteration cap, and the traffic traces. The record states what
they found. They are not reproducible from here, and the conformance suite in the
[plan](../../plans/2026-11-25-process-harnesses.md) takes over the ones that
matter.

## Rerun

The three `run.mjs` were run from this directory on 2026-11-25, on the same shared
and loaded machine: OpenHuman and Codex once, OpenClaw twice, the second time after
its usage line was corrected. Every gate line agreed with the record. These are
single runs, so the numbers are indicative:

| | OpenHuman | OpenClaw, two seats | Codex |
| --- | --- | --- | --- |
| Cold start | 0.29 to 1.07 s | 7.1 to 8.1 s | 67 ms median |
| Idle memory | 72 MB RSS, 69 PSS | 1981 MB PSS | 140 MB RSS |
| After turns | 135 MB RSS, 132 PSS | 2045 MB PSS | 194 MB RSS, four threads |
| Plain, tool turn (median) | 16, 46 ms | 225, 249 ms | 19, 36 ms |
| Cancel closes the model call | 1 ms | 16 ms | 2 ms |
| Model calls after `kill -9` | none in 5 s | one, 10.9 s after the restart began | none in 3 s |

OpenClaw's memory is above the record's 1.4 to 1.8 GB, which was measured with one
seat. OpenHuman's tool-turn median was not recorded in the spike.
