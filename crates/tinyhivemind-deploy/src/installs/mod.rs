//! Where each harness kind's pinned install is, and the harness a seat of
//! that kind starts on.
//!
//! The host maps a kind to a program: a package names only the kind
//! (`docs/specs/harnesses.md`), so a package edit cannot reach the process
//! that enforces its permissions. A kind left uninstalled cannot run a seat,
//! and a seat of that kind is refused before anything starts.
use crate::{Error, Result};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tinyhivemind_acp::openclaw::GatewayPlan;
use tinyhivemind_acp::{AcpHarness, Agent, agent::write_codex_catalog};
use tinyhivemind_host::harness::Harness;
use tinyhivemind_lang::{HarnessKind, LoweredSeat};
use tinyhivemind_openhuman_process::{OpenHumanHarness, Program};

/// A pinned install: what to run, and the version it must report.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pinned {
    /// The program or script, an absolute path.
    pub path: PathBuf,
    /// The version the running agent must report.
    pub version: String,
}

impl Pinned {
    /// `path`, which must report `version`.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>, version: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            version: version.into(),
        }
    }
}

/// Codex's ACP adapter, and the Codex binary its model catalog comes from.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodexInstall {
    /// `codex-acp`'s `dist/index.js`, and its version.
    pub adapter: Pinned,
    /// The Codex binary the adapter runs.
    pub binary: PathBuf,
}

/// The installs a host has. Every field is optional; a seat whose kind needs
/// one that is `None` is refused.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Installs {
    /// `node`, which runs the ACP adapters and `OpenClaw`.
    pub node: Option<PathBuf>,
    /// `openhuman-core`.
    pub openhuman: Option<Pinned>,
    /// `claude-agent-acp`'s `dist/index.js`.
    pub claude_code: Option<Pinned>,
    /// Codex.
    pub codex: Option<CodexInstall>,
    /// `OpenClaw`'s `openclaw.mjs`.
    pub openclaw: Option<Pinned>,
}

/// The model name `seat` runs, or why it has none.
pub(crate) fn model_name(seat: &LoweredSeat) -> Result<String> {
    seat.integration
        .model
        .as_ref()
        .map(|model| model.model.name.clone())
        .ok_or_else(|| Error::NoModel {
            seat: seat.id.clone(),
        })
}

impl Installs {
    /// The install `kind` needs for `seat`, or why it is missing.
    pub(crate) fn need<'a, T>(
        seat: &LoweredSeat,
        kind: HarnessKind,
        install: Option<&'a T>,
        missing: &'static str,
    ) -> Result<&'a T> {
        install.ok_or_else(|| Error::MissingInstall {
            seat: seat.id.clone(),
            kind,
            missing,
        })
    }

    /// Check that `seat`'s kind can run here, before anything starts.
    pub(crate) fn check(&self, seat: &LoweredSeat) -> Result<()> {
        let kind = seat.integration.harness_kind();
        match kind {
            HarnessKind::OpenHuman => {
                Self::need(seat, kind, self.openhuman.as_ref(), "openhuman-core")?;
            }
            HarnessKind::ClaudeCode => {
                Self::need(seat, kind, self.node.as_ref(), "node")?;
                Self::need(seat, kind, self.claude_code.as_ref(), "claude-agent-acp")?;
            }
            HarnessKind::Codex => {
                Self::need(seat, kind, self.node.as_ref(), "node")?;
                Self::need(seat, kind, self.codex.as_ref(), "codex-acp")?;
                model_name(seat)?;
            }
            HarnessKind::OpenClaw => {
                Self::need(seat, kind, self.node.as_ref(), "node")?;
                Self::need(seat, kind, self.openclaw.as_ref(), "openclaw")?;
                model_name(seat)?;
            }
        }
        Ok(())
    }

    /// The harness `seat` starts on. A Codex seat's model catalog is written
    /// to `catalog`; an `OpenClaw` seat's bridge connects to `gateway`.
    pub(crate) fn harness(
        &self,
        seat: &LoweredSeat,
        options: &crate::Deployment,
        catalog: &Path,
        gateway: Option<&GatewayPlan>,
    ) -> Result<Arc<dyn Harness>> {
        let kind = seat.integration.harness_kind();
        let harness: Arc<dyn Harness> = match kind {
            HarnessKind::OpenHuman => {
                let pinned = Self::need(seat, kind, self.openhuman.as_ref(), "openhuman-core")?;
                let program = Program::new(&pinned.path, &pinned.version);
                Arc::new(OpenHumanHarness::new(program).with_options(options.openhuman))
            }
            HarnessKind::ClaudeCode => {
                let node = Self::need(seat, kind, self.node.as_ref(), "node")?;
                let pinned = Self::need(seat, kind, self.claude_code.as_ref(), "claude-agent-acp")?;
                let agent = Agent::claude_code(node, &pinned.path, &pinned.version);
                Arc::new(AcpHarness::new(agent).with_options(options.acp))
            }
            HarnessKind::Codex => {
                let node = Self::need(seat, kind, self.node.as_ref(), "node")?;
                let codex = Self::need(seat, kind, self.codex.as_ref(), "codex-acp")?;
                write_codex_catalog(&codex.binary, catalog, &model_name(seat)?).map_err(
                    |source| Error::Catalog {
                        seat: seat.id.clone(),
                        source,
                    },
                )?;
                let agent =
                    Agent::codex(node, &codex.adapter.path, &codex.adapter.version, catalog);
                Arc::new(AcpHarness::new(agent).with_options(options.acp))
            }
            HarnessKind::OpenClaw => {
                let node = Self::need(seat, kind, self.node.as_ref(), "node")?;
                let pinned = Self::need(seat, kind, self.openclaw.as_ref(), "openclaw")?;
                let gateway = gateway.ok_or(Error::MissingInstall {
                    seat: seat.id.clone(),
                    kind,
                    missing: "gateway",
                })?;
                let agent = Agent::openclaw(node, &pinned.path, &pinned.version, gateway);
                Arc::new(AcpHarness::new(agent).with_options(options.acp))
            }
        };
        Ok(harness)
    }
}

#[cfg(test)]
mod test;
