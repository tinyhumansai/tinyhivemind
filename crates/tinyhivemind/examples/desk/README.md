# A live desk

A host, written out in full, for a room of real agents that share one
transcript and speak one at a time. The deliberation benchmark next door asks
whether a *protocol* beats a poll on a synthetic room; this asks something
smaller and more literal: can the ports in this crate carry a real problem that
takes hours and code, with real models in the seats.

```sh
cargo run --release -p tinyhivemind --example desk -- \
  --desk crates/tinyhivemind/examples/desk/desks/pe1006.txt \
  --task ./TASK.md --workspace ./ws --rounds 8
```

## What the library does and what this file does

Everything that waits on something is here, and none of it is in the library:

| the host (this example) | the library |
| --- | --- |
| `log.rs` — the transcript, as JSONL on disk | `SessionLog`, the paging port it satisfies |
| `queue.rs` — `DeskQueue`, the turn queue and its idempotency | `MentionTurnQueue`, and `dispatch_mention` deciding *whether* to enqueue |
| `agent.rs` — one `opencode run` per turn, and `turn.rs` — the ladder of recoveries under it | nothing: the library never starts a process |
| `mcp.rs` and `tools.rs` — serving the room's tools over MCP and as `tinytools::Tool` | `speech`: what the tools are, what a call must look like, and what one accepted utterance becomes |
| `digest.rs` — one completion that rewrites the room's account | `digest`: when to fold, what a fold may cover, and whether to accept it |
| `memory.rs` — CortexDB recall and capture | nothing: the library holds no memory |
| the chair's nudge when the room falls quiet, in `run.rs` | `choose_responder`, deciding who answers the chair |
| `prompt.rs` — `compose_prompt` | `TeamBriefing::system_text` and `project_session`, which say what a seat may see |
| `aside.rs` — this desk's aside policy and its bookkeeping | `speech::commit_utterance`, which folds that policy into an audience and a refusal |

### File layout

| file | holds |
| --- | --- |
| `main.rs` | the entry point: parse options, run the desk loop |
| `cli.rs` | `Options` and its parsing |
| `run.rs` | the desk loop itself — open the desk, choose who answers, run a turn, post it, route the reply. One function on purpose; see the module doc |
| `queue.rs` | `DeskQueue`, the host's `MentionTurnQueue` |
| `aside.rs` | this desk's aside policy and the bookkeeping folded from the transcript: what an open aside has spent, and what is unsettled |
| `mcp.rs` | the MCP transport: the JSON-RPC loop, the per-turn outbox, and pricing a `desk_dm` before the turn ends |
| `tools.rs` | the room's surface rendered twice — JSON Schema for MCP, `tinytools::Tool` for a host running its own loop — over one `invoke` |
| `room.rs` | the roster and desk snapshots both processes fold over |
| `prompt.rs` | `compose_prompt`, turning a seat's briefing, history, and trigger into one prompt |
| `notebook.rs` | the notebook a seat carries between turns: reading back its tail within budget, and naming what a turn wrote |
| `agent.rs` | one `opencode run` per turn, and its output |
| `chat.rs` | the tool-less wrap-up channel, on `tinyinference` |
| `deskfile.rs` | parsing the plain-text desk file |
| `log.rs` | the JSONL-backed `SessionLog` |
| `memory.rs` | CortexDB recall and capture |

The rule the whole thing turns on is **one message, one turn**. A reply may
mention four teammates; only the first one runs. That is `mention_dispatch`
refusing to fan out, and it is why a desk cannot quietly start N processes.

## The desk file

Plain text. A header, then one block per seat:

```text
desk: pe1006
name: PE 1006 Desk
person: steven

[agent theory]
name: Theory
role: Fibonacci-word and Sturmian combinatorics specialist
brief: What this seat owns, in as many `brief:` lines as it takes.
```

`role:` is what the responder ladder's selector would see. `brief:` is private
standing context prepended to every turn that seat takes — it is not in the
shared transcript, because a fact every seat can read is not private
information.

## Flags

| flag | meaning |
| --- | --- |
| `--desk PATH` | the desk file (required) |
| `--task PATH` | the chair's opening message (required) |
| `--workspace DIR` | where seats read, write, and run code |
| `--transcript PATH` | the JSONL log; an existing one is resumed |
| `--agent-cmd CMD` | the agent CLI; the prompt is appended as its last argument. Name the workspace here too (`--dir`), not only through the child's working directory — see obligation 4 |
| `--rounds N` | how many times the chair may nudge a room that went quiet |
| `--max-turns N` | hard ceiling on turns |
| `--max-hops N` | `MentionDispatchPolicy::max_hops`: how long one chain may run |
| `--window N` | messages of transcript a seat sees |
| `--timeout SECS` | per-turn wall clock before the process is killed |
| `--cortex-base URL` | CortexDB root; `CORTEX_API_KEY` supplies the key |
| `--library-scope` `--session-scope` | the durable and per-run memory scopes |
| `--no-memory` | run with no recall and no capture |
| `--no-digest` | do not fold older messages into the room's account |
| `--fold-tokens N` | fold once the unfolded scrollback would cost roughly N tokens, whichever binds first with the row count. `0` leaves only the row trigger |
| `--mcp-server --outbox PATH` | serve the desk tools over stdio; the binary re-execs itself into this mode and takes no turn. `--desk` and `--turn` let it price a `desk_dm` before the turn ends |
| `--tool-surface` | print the four tools a seat is given, with their schemas, and exit |

`OPENCODE_CONFIG_CONTENT` is passed through to the agent process, which is how
a run pins one model — for instance a ladder rung that only ever serves
`deepseek-v4-pro`. The host **merges its own MCP block into it** before
handing it over, so whatever the variable already said is kept and the desk
tools are added. Without a provider block in it, the agent CLI falls back to
whatever it is configured with by default, which on this box is a local model
that is not running: a run that answers `Unexpected server error` on turn 1 is
usually a missing `OPENCODE_CONFIG_CONTENT`, not a broken desk.

## Speaking is a tool call

A seat says one thing per turn by **calling a tool**, not by writing a marker:

| tool | what it does |
| --- | --- |
| `desk_post(message)` | say one thing to the whole desk |
| `desk_dm(to[], message)` | say it to named seats instead |
| `desk_read(limit)` | read further back than the window it was handed |
| `desk_close(message)` | say one last thing and report the work finished |

Text a seat produces outside a tool call is its own thinking and reaches
nobody. The reason is a defect: in run 26 a seat closed with `<<<POST>>> …
<<<POST>>>` rather than `<<<POST … POST>>>` and a verified result reached the
room as the three characters `>>>`. A model-authored delimiter is an interface
with a fallible producer, and it has no schema and no way to tell the producer
it got it wrong; a tool call has both, and a malformed one is refused to the
seat while it can still fix it.

**The tools are stated once, in the library.** `speech::tool_specs()` holds
each name, description and argument as data; `tools.rs` renders that into JSON
Schema for the MCP server and into `tinytools::Tool` for a host that runs an
agent loop in its own process, and both go through one `invoke`. A seat sees
the same four tools whichever way it was reached. `--tool-surface` prints them.

`mcp.rs` is that server, and it is this same binary re-executed
(`--mcp-server`). It never writes the transcript. It appends to a per-turn
outbox the host truncates before the turn and drains after it, so sequence
assignment, audience resolution through `aside`, mention resolution and
dispatch all stay exactly where they were — a tool call is a *request* to
speak. `desk_dm` in particular goes through the same aside policy as `!aside`
and can be refused, leaving the row desk-visible. The fence still works
(`speech::fence`), as a documented fallback for an agent CLI that cannot reach
the tools.

**A refusal reaches the seat that caused it.** Given `--desk` and `--turn` —
which the host passes automatically — the server prices a `desk_dm` against
the aside policy *while the turn is still running*, so a seat is told its
message is going to the whole desk instead, and why, in time to say it
differently. A `desk_dm` naming somebody who is not on the desk is refused
outright and nothing is written. Without those two paths the server answers as
it did before: the host still resolves the audience when it drains the outbox,
so the row is never wrong — only the seat is uninformed.

## The room's standing account

A window is not a memory. Everything older than the live window is folded into
one **account** — bounded, rewritten as the room moves, always covering
strictly more than before — which is placed in every prompt under `## The room
before that`, ahead of the messages it does not cover. That is
`tinyhivemind::digest`; `digest.rs` here is the host's side of its `Digester`
port, one tool-less completion through the same router the wrap-up uses.

Three properties are worth knowing while reading a run:

- **It folds only desk-visible rows.** One account serves every seat, including
  one that has never spoken, and no fold can launder an aside into a shared
  summary.
- **It is lossy and says so.** Every message it stands for is still in the
  transcript at the number it cites, and `desk_read` reaches it.
- **A fold that fails costs the compaction and nothing else.** The window is
  already correct without one.

`desk_close` exists because run 28 had no way to end. The answer was signed off
on turn 3; turns 4-12 are the chair nudging `@lead` once per remaining round and
`@lead` replying "the desk is finished, not stalled" nine times — nine of twelve
turns spent restating a delivered result, because the nudge fires on `rounds`
alone and nothing could contradict it. It is still a request, not an act: the
host appends the row and then closes, so one-message-one-turn is untouched.

## Private asides

A seat may address one peer alone by making `!aside @peer` the first line of
its post. The harness never acts on that marker: it hands the line to
`tinyhivemind_core::aside::aside`, which resolves who it may reach and refuses
with a named reason otherwise, and a refusal leaves the row desk-visible —
the safe direction to fail in. The policy here is a pair, six rows, and a
settlement owed before another may be opened, because a desk solving one
problem wants two seats to sort out a disagreement without spending the room's
attention on it, and wants that to end in something the room can read.

The budget is folded out of the journal rather than stored, so this host keeps
no state the transcript does not already carry.

## The notebook a seat carries

A seat is a fresh process every turn, and resuming its CLI session instead was
tried and turned off: the request grows without bound until every call
stalls. What a seat carries instead is a **notebook** — `notebooks/<seat>.md`
under the workspace, written by the seat with its own file tool, read back
verbatim at the top of its next turn under `## Your notebook`. The seat is told
to *rewrite* it, not append, and told the budget (`NOTEBOOK_CHARS`, 6000); an
overrun is truncated on read from the front and reported in the first line,
and the file is never touched by the host. Nobody else's prompt carries it.

Two smaller mechanisms ride with it. When a turn wrote or edited files, the
host appends one `system/workspace` row — `@solver wrote psi_sublinear.py,
NOTES.md` — so the room learns what changed without the seat spending its post
on it; the notebook itself is not announced. And the chair's brief is appended
only to an empty transcript, because a resumed one already opens with it and
the standing brief is in every prompt anyway.

The per-turn line now prints the notebook size and the turn's `read` count.
The count is what the notebook is for: turn 1 of the run-27 solver spent 15 of
19 tool calls re-reading files. The design is
[`docs/specs/seat-continuity.md`](../../../../docs/specs/seat-continuity.md).

## Five host obligations found by running it

Both are the same shape as the ones `../../../tinyhivemind-hive/examples/bench/LIVE.md`
records: things the library cannot impose and a host has to.

1. **A model narrates.** Its answer arrives wrapped in reasoning that every
   other seat would then have to read. The prompt asks for the room message
   inside `<<<POST … POST>>>` and `agent.rs` takes only that.
2. **A model mentions everybody.** Told that four seats exist, it addresses all
   four, and only the first one runs — so the chain continues to whoever
   happened to be named first rather than to whoever is needed. The prompt says
   so in as many words: put the seat you need first.
3. **A turn that is killed has said nothing.** A seat can spend a whole budget
   inside tool calls and never speak: one run cost 90k tokens across 27 tool
   calls and posted nothing at all. The library cannot know a process was
   killed, so the host owes the room a message — `chat.rs` asks for one
   through a plain completion where there is no tool to call. Asking the CLI
   nicely does not work: told in as many words not to run anything, it ran
   eight more commands and hit the deadline again.
4. **Setting the child's working directory is not enough.** The seats wrote
   their notes and their code into the *repository checkout* rather than the
   workspace: an agent CLI resolves a write against its own project root,
   which it finds by walking up for a `.git`, not against the working
   directory its parent handed it. This run's own commit history is the
   evidence — an auto-commit hook filed four of the seats' scratch files into
   the branch before anyone noticed. Pass the workspace on the command line —
   `opencode run --dir <workspace>` — and check where the first file actually
   lands before trusting a room to accumulate anything.
5. **`max_tokens` is spent on reasoning first.** That wrap-up came back empty
   with `finish_reason: length` at a 1200-token cap, and again at 8000 — the
   whole budget went to reasoning tokens and `content` was the empty string,
   on both DeepSeek tiers. Uncapped, the same call answers in 350 tokens. A
   silent seat looked like a model refusing to speak and was a cap set too
   low.
