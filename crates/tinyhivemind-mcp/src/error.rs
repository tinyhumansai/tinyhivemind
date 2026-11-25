//! Why a server could not start, or a seat could not be served.

/// Server result.
pub type Result<T> = std::result::Result<T, Error>;

/// Failures starting the server and serving a seat. A refused tool call is
/// not one of these: the model reads it as the tool's error result.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The loopback listener could not be opened.
    #[error("cannot listen on loopback: {0}")]
    Listen(#[source] std::io::Error),
    /// The operating system gave no randomness for a bearer.
    #[error("no randomness for a bearer: {0}")]
    Random(String),
    /// The seat declares a sandbox, which nothing confines here yet.
    #[error("seat {0} declares a sandbox, which this server cannot enforce")]
    UnsupportedSandbox(String),
    /// The seat declares package MCP servers, which are not proxied yet.
    #[error("seat {0} declares package mcp servers, which this server does not proxy")]
    UnsupportedMcpServers(String),
    /// An executable tool declares an argument the server adds itself.
    #[error("tool {tool} declares the reserved argument {name}")]
    ReservedArgument {
        /// The tool.
        tool: String,
        /// `turn` or `call`.
        name: String,
    },
    /// Two of a seat's tools have one name.
    #[error("tool {0} is declared twice")]
    DuplicateTool(String),
    /// The seat is already served.
    #[error("seat {0} is already served")]
    DuplicateSeat(String),
    /// The server's task ended other than by returning.
    #[error("the server task failed: {0}")]
    Stopped(String),
}
