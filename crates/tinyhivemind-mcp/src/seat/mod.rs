//! What one seat is served: its executable tools, hooks and permissions.
//!
//! [`SeatTools`] is built by hand or from a package's lowered seat with
//! [`SeatTools::from_lowered`], which refuses what this server cannot
//! enforce rather than accepting it unenforced.
mod bearer;

use crate::{Error, Result};
pub(crate) use bearer::Bearer;
use tinyhivemind_exec::ExecContext;
use tinyhivemind_lang::{
    Access, ExecTool, LoweredHook, LoweredSeat, PermissionSet, RuntimeSettings, Sandbox,
};

/// The arguments the server adds to a state-changing tool's schema.
pub(crate) const RESERVED: [&str; 2] = ["turn", "call"];

/// One seat's tools beyond the hive vocabulary, and the policy over all of
/// them.
#[derive(Clone, Debug)]
pub struct SeatTools {
    seat: String,
    programs: Vec<ExecTool>,
    hooks: Vec<LoweredHook>,
    context: ExecContext,
    access: Access,
    permissions: Vec<PermissionSet>,
    denied: Vec<String>,
    allowed: Vec<String>,
}

impl SeatTools {
    /// Seat `seat` with the hive tools only, under `supervised` access, the
    /// language's default.
    #[must_use]
    pub fn new(seat: impl Into<String>) -> Self {
        let seat = seat.into();
        Self {
            context: ExecContext {
                seat: seat.clone(),
                ..ExecContext::default()
            },
            seat,
            programs: Vec::new(),
            hooks: Vec::new(),
            access: Access::Supervised,
            permissions: Vec::new(),
            denied: Vec::new(),
            allowed: Vec::new(),
        }
    }

    /// The seat a package lowered, on `runtime`, its tools running in
    /// `context`.
    ///
    /// A non-empty `tools.allow` serves only the tools it names and the
    /// seat's executable tools, as on the linked path. Access is the
    /// strictest of the runtime's `autonomy` and every
    /// permission layer's `access`.
    ///
    /// # Errors
    /// [`Error::UnsupportedMcpServers`] for a seat with package MCP servers,
    /// [`Error::UnsupportedSandbox`] for any sandbox but `none`, and
    /// [`Error::ReservedArgument`] for a tool declaring `turn` or `call`, and
    /// [`Error::UnsupportedSchema`] for an argument schema keyword the server
    /// cannot check.
    pub fn from_lowered(
        seat: &LoweredSeat,
        runtime: &RuntimeSettings,
        context: ExecContext,
    ) -> Result<Self> {
        let integration = &seat.integration;
        if !integration.mcp.is_empty() {
            return Err(Error::UnsupportedMcpServers(seat.id.clone()));
        }
        let sandboxed = runtime.sandbox != Sandbox::None
            || integration
                .permissions
                .iter()
                .any(|set| set.sandbox != Sandbox::None);
        if sandboxed {
            return Err(Error::UnsupportedSandbox(seat.id.clone()));
        }
        let access = integration
            .permissions
            .iter()
            .map(|set| set.access)
            .min()
            .unwrap_or(runtime.autonomy)
            .min(runtime.autonomy);
        let tools = Self {
            seat: seat.id.clone(),
            programs: integration.programs.clone(),
            hooks: integration.hooks.clone(),
            context,
            access,
            permissions: integration.permissions.clone(),
            denied: integration.tools.deny.clone(),
            allowed: integration.tools.allow.clone(),
        };
        tools.check()?;
        Ok(tools)
    }

    /// Also serve `programs`.
    #[must_use]
    pub fn with_programs(mut self, programs: Vec<ExecTool>) -> Self {
        self.programs = programs;
        self
    }

    /// Run `hooks` around the seat's tool calls.
    #[must_use]
    pub fn with_hooks(mut self, hooks: Vec<LoweredHook>) -> Self {
        self.hooks = hooks;
        self
    }

    /// Run programs and hooks in `context`.
    #[must_use]
    pub fn with_context(mut self, context: ExecContext) -> Self {
        self.context = context;
        self
    }

    /// Serve only the tools `allowed` names, and the seat's executable tools;
    /// empty serves every tool.
    #[must_use]
    pub fn with_allowed(mut self, allowed: Vec<String>) -> Self {
        self.allowed = allowed;
        self
    }

    /// Judge effectful programs under `access`.
    #[must_use]
    pub fn with_access(mut self, access: Access) -> Self {
        self.access = access;
        self
    }

    /// Serve only what every one of `permissions` allows by name.
    #[must_use]
    pub fn with_permissions(mut self, permissions: Vec<PermissionSet>) -> Self {
        self.permissions = permissions;
        self
    }

    /// The seat's coordinator id.
    #[must_use]
    pub fn seat(&self) -> &str {
        &self.seat
    }

    /// The access effectful programs are judged under.
    #[must_use]
    pub fn access(&self) -> Access {
        self.access
    }

    /// The executable tools' names, in declaration order.
    #[must_use]
    pub fn program_names(&self) -> Vec<&str> {
        self.programs.iter().map(|tool| tool.id.as_str()).collect()
    }

    pub(crate) fn programs(&self) -> &[ExecTool] {
        &self.programs
    }

    pub(crate) fn hooks(&self) -> &[LoweredHook] {
        &self.hooks
    }

    pub(crate) fn context(&self) -> &ExecContext {
        &self.context
    }

    /// Whether the seat's tool rules let `name` be served.
    pub(crate) fn permits(&self, name: &str) -> bool {
        let allowed = self.allowed.is_empty()
            || self.allowed.iter().any(|tool| tool == name)
            || self.programs.iter().any(|tool| tool.id == name);
        allowed
            && !self.denied.iter().any(|denied| denied == name)
            && self.permissions.iter().all(|set| {
                !set.deny_tools.iter().any(|denied| denied == name)
                    && set
                        .allow_tools
                        .as_ref()
                        .is_none_or(|allowed| allowed.iter().any(|tool| tool == name))
            })
    }

    /// Refuse a tool declaring an argument the server adds, or a schema
    /// keyword it cannot check.
    pub(crate) fn check(&self) -> Result<()> {
        for tool in &self.programs {
            if let Some(keyword) = crate::tools::unsupported(&tool.args) {
                return Err(Error::UnsupportedSchema {
                    tool: tool.id.clone(),
                    keyword,
                });
            }
            for name in RESERVED {
                if tool.args["properties"].get(name).is_some() {
                    return Err(Error::ReservedArgument {
                        tool: tool.id.clone(),
                        name: name.into(),
                    });
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod test;
