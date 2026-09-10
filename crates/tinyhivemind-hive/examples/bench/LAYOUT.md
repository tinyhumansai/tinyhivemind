# Layout

Every module that outgrew a single file is a directory: `mod.rs` holds its
module doc, its core types, and whatever re-exports the rest of the crate
actually needs; its siblings hold one cohesive slice of the rest. The `mod
sim;`-style declaration in `main.rs` is unchanged either way, since Rust
resolves it to `sim/mod.rs` transparently.

| file | what it holds |
| --- | --- |
| `main.rs` | the crate doc, the `mod` declarations, and top-level dispatch: `main`, `stats_check`, `run`, `trace`, and the `sweep_*` entry points |
| `cost.rs` | the token and wall-clock model five of the six headline columns are computed from: `CostModel`, its four flags, `RoundShape`, and the `Cost` a priced episode folds into |
| `cost/test.rs` | that a wider round costs more tokens and no more wall clock, that a sample's cost folds the same however the threads split it, and that an unparsable constant stops the run |
| `grid/mod.rs` | the cross product: the five systems a cell runs, the per-cell rooms and their seeding, the one table shape, and the across-cells summary |
| `grid/axes.rs` | the four axes and what a point on each one means: `Topic`, `Complexity`, `Cell`, `Axes` and their parsing |
| `grid/test.rs` | what the axes parse to, what they refuse, and that a cell is reproducible from its own label |
| `calibrate.rs` | fitting `CostModel`'s four constants to a live endpoint, as two two-point fits |
| `calibrate/test.rs` | the fit arithmetic, and that the probes differ in one thing each |
| `cli/mod.rs` | `Options`, `Mode`, and command-line parsing |
| `cli/flags.rs` | the per-family flag handlers `Options::parse` delegates to, including `apply_mode_flag` and the precedence between the flags that select what a run does |
| `cli/test.rs` | unit tests for CLI flag parsing |
| `policy.rs` | `default_policy`, `tuned_policy`, and the delegation-arm policy variants built from it |
| `compare/mod.rs` | the simulated multi-arm comparison engine: `compare`, `run_arms`, the check-arm runners, `endings`, and the cost table |
| `compare/totals.rs` | `Totals` — every arm's running aggregate, built at a chosen cost model and merged in room order |
| `compare/totals/test.rs` | that the selected cost model reaches every arm, `ladder_directed` included, which sits outside `arms_mut`'s array |
| `backend.rs` | seat/backend configuration shared by both live drivers: API keys, HTTP config, seat model and command resolution, usage accounting |
| `live_single.rs` | driving one live desk or episode through a real agent |
| `live_swarm/mod.rs` | driving a live federation of desks through real agents |
| `live_swarm/totals.rs` | what a federated comparison counts, and the table it prints |
| `sim/mod.rs` | the rooms, the private evaluations, the `Expertise` shapes (`--specialists`, `--hidden-profile`) that redistribute those evaluations, the evidence-first opening (`--blind-evidence`), the tuning constants, `Expertise`, and the `Room` type and its generation |
| `sim/naming.rs` | what a room's members and options are called and how many of each this harness builds: the two eight-entry name tables, the generators past them, `Role`, and the `MAX_MEMBERS` / `MAX_TOPICS` ceilings |
| `sim/chain.rs` | a room as one link of a `--stages` chain: renaming its options, inheriting the last stage's window, and being poisoned by a wrong answer |
| `sim/facet.rs` | a room as one facet of a `--facets` task: renaming its options so no facet is scored against another's question, and reading one seat's load rather than the room's mean |
| `sim/generation.rs` | drawing a room's members and their expertise |
| `sim/agent/mod.rs` | `SimAgent`, the participant that holds a private, noisy view of every option: its struct, `Payload`, and `CheckStyle` |
| `sim/agent/{state,turn}.rs` | construction and state mutation (`new`, `import`, `score`, ...), then deciding what to say (`check`, `absorb`, `compose`, ...) and the `Participant` impl |
| `sim/view.rs` | `View`, the window a participant reads the transcript through, and the marker parsers |
| `federation.rs` | several desks, each with a correlated bias of its own |
| `swarm/mod.rs` | one journal per channel, the scheduler, and the referral edge: `SwarmMember`, `SwarmHost`, and driving a swarm episode |
| `swarm/{board,member,format}.rs` | `Board`'s pending queue, seat lookup and `Exchange` — whether a desk pays for a cross-channel question with an authorized turn or off the floor, and whether it publishes a reading to every channel; `SwarmSim`, which can also field a referral and publish; parsing and restating a `Reading` for the wire |
| `swarm/work.rs` | one unit of work a seat fills — a planned round or a planned answer — and the filling of it, with no access to the board |
| `swarm/format/test.rs` | the two wire clauses — a reading and a disqualification — and the guarantee that adding one does not disturb the other |
| `swarm/schedule.rs` | the two passes over a federation's desks — one desk at a time, or every desk's round in flight at once — and why they are not equivalent |
| `swarm/test.rs` | the digest's cost, its bound, and the property that keeps a published reading out of a foreign desk's standings |
| `run/mod.rs` | the host: a journal, a roster, and the step loop — `Host`, `Ending`, and the episode entry points |
| `run/turns.rs` | per-turn machinery: audience, appending a turn, one exchange round and the per-caller row counts it records |
| `run/scoring.rs` | `EpisodeReport`, `Tally`, `Recorded`, and `finished` — what an episode came to, folded out of the journal once nobody speaks again |
| `arms.rs` | the `ladder`, `vote`, `merged` and federated controls, and what each is charged — including the router's own call on the `Select` rung |
| `arms/test.rs` | the controls' pricing: that a routed ladder pays for the asking and that its router's prompt grows with the room |
| `scale.rs` | `--scale-sweep`: room size against channel topology, one table per axis |
| `horizon/mod.rs` | the chain ladder: a task with several stages, the arms that decide one, and the soloist controls |
| `horizon/test.rs` | its unit tests, promoted out to match the house convention |
| `variety/mod.rs` | the variety ladder: a task with several facets at once, one owner each, and whether splitting them across seats beats holding them all. `hive+fold` asks `tinyhivemind_hive::division` who owns what rather than deciding for itself |
| `variety/test.rs` | its unit tests, promoted out when the sweep passed the file cap |
| `sweep.rs` | the policy grid and its ranking |
| `parallel.rs` | spreading the per-room loops across cores: `map_in_order`, which returns results in *input* order whatever order the threads finish in, and `default_jobs` |
| `parallel/test.rs` | that ordering guarantee, at every `--jobs` value and under deliberately reversed completion order |
| `metrics/test.rs` | that merging two samples equals folding one, which is what lets the loops above run in parallel without moving a number |
| `metrics/mod.rs` | aggregation and the confidence-interval, bootstrap and rank-correlation statistics: `Aggregate` and the fold that builds one |
| `metrics/tables.rs` | the printed and `--json` tables themselves: the six-column headline row, the library-cost row beside it, the detail and cost tables, and the paired-difference lines |
| `metrics/{format,stats}.rs` | formatting helpers for those tables, then the small numeric statistics helpers (percentile, rank correlation) |
| `live/mod.rs` | the shared prompt state both live backends assemble: `AgentPrompt`, plus the external agent CLI backend and the solo poll |
| `live/{agent,desk}.rs` | `LiveAgent`, driving one seat through a CLI subprocess; `LiveDeskAgent`, driving one seat as a member of a swarm desk |
| `http.rs` | the direct-HTTP backend: the same prompt state over `curl`, the two wire formats, `ask` (which retries) and `ask_once` (which does not, for calibration probes) |
| `http/usage.rs` | what a run spent and how a seat's total reaches the table: `Usage`, `UsageHandle`, and `usage_of` |
| `scenario.rs` | the scenario file format, the briefs, and the recorded answer |
| `scenarios/` | the scenario files themselves: seven hidden profiles across incident triage, logistics, payments fraud and laboratory measurement |
| `scenario/test.rs` | that every shipped scenario parses, records a truth that is on offer, and gives every member something of its own |
| `DELEGATION.md` | the delegation arms, the three questions they answer, and what they scored |
| `LIVE.md` | live rooms: the prompt, the scenario format, and the CLI and HTTP backends |
| `HORIZON.md` | what a task with a history costs, and whether a room or a soloist pays less |
| `DEPTH.md` | the two cost columns — depth beside width — and what the concurrency arms scored |
| `SCALE.md` | running the harness at a thousand agents: `--jobs`, the size ceilings, `--ask-cap`, `--digest` and `--distance` |
| `CONTEXT.md` | the context-budget window model, and what a bounded prompt does and does not change about the arms above |
| `rng.rs` | a seeded `SplitMix64`, so every run reproduces |
| `COST.md` | the cost model: what it assumes, what it refuses to claim, and how to calibrate it |
| `SWARM.md` | several channels: what a crossing costs and how a member decides to make one |
| `EVIDENCE.md` | the evidence-first opening, and why a deposit is not a position |
