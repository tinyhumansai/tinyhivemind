//! The room's tool surface, stated once, as data.
//!
//! A host renders these into whatever its agents call — a JSON schema over
//! MCP, an implementation of a tool trait, a function registry. Nothing here
//! is JSON, because JSON handling is not in this crate's dependency graph and
//! a schema is not the only shape a host needs.
//!
//! The descriptions are contract text rather than documentation. They are the
//! only place a seat is told that what it writes outside a tool call reaches
//! nobody, and the only place it is told that one `close` is different from
//! one more `post`. A host renders them verbatim.
//!
//! The names here are bare. A host that namespaces its tools — an MCP server
//! called `desk` serving `post` presents it as `desk_post` — prefixes them,
//! and the descriptions are written to read correctly either way.

use super::types::ParameterKind;

/// One argument a tool takes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ToolParameter {
    /// The argument name, as the seat writes it.
    pub name: &'static str,
    /// What the seat should put in it, or `None` where the tool's own
    /// description already says.
    pub description: Option<&'static str>,
    /// What shape the value takes.
    pub kind: ParameterKind,
    /// Whether a call without it is refused.
    pub required: bool,
}

/// One tool a seat may call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ToolSpec {
    /// The bare tool name, before any host namespacing.
    pub name: &'static str,
    /// What the tool does, written for the seat that will call it.
    pub description: &'static str,
    /// The arguments it takes, in the order they should be presented.
    pub parameters: &'static [ToolParameter],
}

/// Every tool the room serves, in the order a seat should meet them.
///
/// `post` is first because it is the one a seat must call; `read` is last
/// because it is the one a seat should rarely need.
#[must_use]
pub const fn tool_specs() -> &'static [ToolSpec] {
    SPECS
}

const SPECS: &[ToolSpec] = &[
    ToolSpec {
        name: "post",
        description: "Say one thing to the whole desk. This is the only way to speak: text you \
                      write outside a tool call is your own thinking and reaches nobody. Mention \
                      a teammate with @id to hand them the next turn — only the first mention \
                      does that. Call this exactly once, at the end of your turn.",
        parameters: &[ToolParameter {
            name: "message",
            description: Some(
                "What you established, what you did not finish, and the one seat you need next \
                 — that seat named first.",
            ),
            kind: ParameterKind::Text,
            required: true,
        }],
    },
    ToolSpec {
        name: "dm",
        description: "Say one thing to named peers instead of the whole desk. Use it to settle a \
                      disagreement without spending the room's attention; the room is told the \
                      exchange happened and not what it said. It still costs your one message \
                      for the turn.",
        parameters: &[
            ToolParameter {
                name: "to",
                description: Some("Seat ids, without the @."),
                kind: ParameterKind::TextList,
                required: true,
            },
            ToolParameter {
                name: "message",
                description: None,
                kind: ParameterKind::Text,
                required: true,
            },
        ],
    },
    ToolSpec {
        name: "close",
        description: "Say one last thing and report that the desk's work is finished. Call this \
                      instead of `post` only when the task is genuinely done and no seat has an \
                      open step — a result someone still has to verify is not done. If you are \
                      being asked again about work you have already delivered, this is the call \
                      that says so.",
        parameters: &[ToolParameter {
            name: "message",
            description: Some("The result, and why nothing is left open."),
            kind: ParameterKind::Text,
            required: true,
        }],
    },
    ToolSpec {
        name: "read",
        description: "Read the desk's recent messages. You are handed a bounded window at the \
                      top of your turn; call this when you need more of it than you were given.",
        parameters: &[ToolParameter {
            name: "limit",
            description: Some("How many recent messages to return. Default 20, max 100."),
            kind: ParameterKind::Count {
                default: READ_DEFAULT as u64,
                min: 1,
                max: READ_MAX as u64,
            },
            required: false,
        }],
    },
];

/// How many messages `read` returns when the seat does not say.
pub const READ_DEFAULT: usize = 20;

/// The most messages one `read` returns, however many it asked for.
///
/// The clamp is here rather than in each host so that two hosts cannot
/// disagree about it, and so that a seat that asks for the whole transcript
/// gets a bounded answer rather than its own context window back.
pub const READ_MAX: usize = 100;
