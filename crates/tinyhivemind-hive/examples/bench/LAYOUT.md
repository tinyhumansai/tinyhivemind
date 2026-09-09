# Layout

Every module that outgrew a single file is a directory: `mod.rs` holds its
module doc, its core types, and whatever re-exports the rest of the crate
actually needs; its siblings hold one cohesive slice of the rest. The `mod
sim;`-style declaration in `main.rs` is unchanged either way, since Rust
resolves it to `sim/mod.rs` transparently.

| file | what it holds |
| --- | --- |
| `main.rs` | the crate doc, the `mod` declarations, and top-level dispatch: `main`, `stats_check`, `run`, `trace`, and the `sweep_*` entry points |
| `cli.rs` | `Options`, `Mode`, and command-line parsing |
| `policy.rs` | `default_policy`, `tuned_policy`, and the delegation-arm policy variants built from it |
| `compare.rs` | the simulated multi-arm comparison engine: `compare`, `Totals`, `run_arms`, `endings`, and the cost table |
| `backend.rs` | seat/backend configuration shared by both live drivers: API keys, HTTP config, seat model and command resolution, usage accounting |
| `live_single.rs` | driving one live desk or episode through a real agent |
| `live_swarm.rs` | driving a live federation of desks through real agents |
| `sim/mod.rs` | the rooms, the private evaluations, the `Expertise` shapes (`--specialists`, `--hidden-profile`) that redistribute those evaluations, the evidence-first opening (`--blind-evidence`), the tuning constants, `Role`, `Expertise`, and the `Room` type and its generation |
| `sim/chain.rs` | a room as one link of a `--stages` chain: renaming its options, inheriting the last stage's window, and being poisoned by a wrong answer |
| `sim/facet.rs` | a room as one facet of a `--facets` task: renaming its options so no facet is scored against another's question, and reading one seat's load rather than the room's mean |
| `sim/generation.rs` | drawing a room's members and their expertise |
| `sim/agent/mod.rs` | `SimAgent`, the participant that holds a private, noisy view of every option: its struct, `Payload`, and `CheckStyle` |
| `sim/agent/{state,turn}.rs` | construction and state mutation (`new`, `import`, `score`, ...), then deciding what to say (`check`, `absorb`, `compose`, ...) and the `Participant` impl |
| `sim/view.rs` | `View`, the window a participant reads the transcript through, and the marker parsers |
| `federation.rs` | several desks, each with a correlated bias of its own |
| `swarm/mod.rs` | one journal per channel, the scheduler, and the referral edge: `SwarmMember`, `SwarmHost`, and driving a swarm episode |
| `swarm/{board,member,format}.rs` | `Board`'s pending queue and seat lookup; `SwarmSim`, which can also field a referral; parsing and restating a `Reading` for the wire |
| `run/mod.rs` | the host: a journal, a roster, and the step loop — `Host`, `Ending`, and the episode entry points |
| `run/{turns,scoring}.rs` | per-turn machinery (audience, appending a turn, one exchange), then `EpisodeReport`, `Tally`, and an episode's accounting |
| `arms.rs` | the `ladder`, `vote`, `merged` and federated controls |
| `scale.rs` | `--scale-sweep`: room size against channel topology, one table per axis |
| `horizon.rs` | the chain ladder: a task with several stages, the arms that decide one, and the soloist controls |
| `sweep.rs` | the policy grid and its ranking |
| `metrics/mod.rs` | aggregation, formatting, and the confidence-interval, bootstrap and rank-correlation statistics: `Aggregate` and the printed/JSON tables |
| `metrics/{format,stats}.rs` | formatting helpers for those tables, then the small numeric statistics helpers (percentile, rank correlation) |
| `live/mod.rs` | the shared prompt state both live backends assemble: `AgentPrompt`, plus the external agent CLI backend and the solo poll |
| `live/{agent,desk}.rs` | `LiveAgent`, driving one seat through a CLI subprocess; `LiveDeskAgent`, driving one seat as a member of a swarm desk |
| `http.rs` | the direct-HTTP backend: the same prompt state over `curl`, and its usage table |
| `scenario.rs` | the scenario file format, the briefs, and the recorded answer |
| `scenarios/` | the scenario files themselves |
| `DELEGATION.md` | the delegation arms, the three questions they answer, and what they scored |
| `LIVE.md` | live rooms: the prompt, the scenario format, and the CLI and HTTP backends |
| `HORIZON.md` | what a task with a history costs, and whether a room or a soloist pays less |
| `DEPTH.md` | the two cost columns — depth beside width — and what the concurrency arms scored |
| `CONTEXT.md` | the context-budget window model, and what a bounded prompt does and does not change about the arms above |
| `rng.rs` | a seeded `SplitMix64`, so every run reproduces |
