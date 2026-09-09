//! A live desk: several real agents sharing one transcript, one turn at a time.
//!
//! This is the host side of `tinyhivemind` written out in full — the parts the
//! library deliberately refuses to own. It opens the storage
//! ([`log::JsonlLog`]), it holds the turn queue ([`queue::DeskQueue`]), it
//! runs the model ([`agent::AgentRunner`]), and it keeps the memory
//! ([`memory::Memory`]). The library decides *who speaks next* and *what that
//! speaker can see*, and nothing else.
//!
//! ```sh
//! cargo run --release -p tinyhivemind --example desk -- \
//!   --desk crates/tinyhivemind/examples/desk/desks/pe1006.txt \
//!   --task /tmp/pe1006/TASK.md --workspace /tmp/pe1006 --rounds 8
//! ```
//!
//! See `README.md` beside this file for the flags and the failure modes.
//!
//! # File layout
//!
//! | file | holds |
//! | --- | --- |
//! | `main.rs` | the entry point: parse options, serve the tools or run the loop |
//! | `cli.rs` | [`cli::Options`] and its parsing |
//! | `run.rs` | [`run::run`], the desk loop itself: one function on purpose |
//! | `turn.rs` | one turn's process, and the ladder of recoveries under it |
//! | `queue.rs` | [`queue::DeskQueue`], the host's `MentionTurnQueue` |
//! | `aside.rs` | this desk's aside policy and its host-side bookkeeping |
//! | `prompt.rs` | [`prompt::compose_prompt`], turning a turn into text |
//! | `mcp.rs` | the room as a tool: `desk_post`, `desk_dm`, `desk_read` |
//! | `digest.rs` | the host side of the `Digester` port: the room's account |
//! | `notebook.rs` | the notebook a seat carries between turns |
//! | `agent.rs` | one `opencode run` per turn, and its output |
//! | `chat.rs` | the tool-less wrap-up channel |
//! | `deskfile.rs` | parsing the plain-text desk file |
//! | `log.rs` | the JSONL-backed `SessionLog` |
//! | `room.rs` | the roster and desk snapshots both processes fold over |
//! | `memory.rs` | CortexDB recall and capture |

mod agent;
mod aside;
mod chat;
mod cli;
mod deskfile;
mod digest;
mod log;
mod mcp;
mod memory;
mod notebook;
mod prompt;
mod queue;
mod room;
mod run;
mod turn;

use std::error::Error as StdError;

/// The error every host-side call in this example returns.
type BoxError = Box<dyn StdError + Send + Sync + 'static>;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), BoxError> {
    let options = cli::Options::parse()?;
    if options.serve_mcp {
        // This same binary is the MCP server the agent CLI spawns: re-execing
        // it keeps one artifact and one version of the tool schema. Serving
        // takes no turn. It reads the desk file only to price a `desk_dm`
        // against the aside policy while the seat can still act on the answer.
        let outbox = options.outbox.ok_or("--mcp-server needs --outbox")?;
        return mcp::serve(&mcp::Serving {
            outbox,
            transcript: options.transcript,
            desk: options.turn.as_ref().map(|_| options.desk),
            turn: options.turn,
        });
    }
    run::run(options).await
}
