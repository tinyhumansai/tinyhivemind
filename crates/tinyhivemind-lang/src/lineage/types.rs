//! Evaluation archive wire types and typed archive failures.

use crate::patch::{Patch, PatchClass};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Host decision after evaluation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Activate the evaluated design.
    Accepted,
    /// Retain evidence without activating the design.
    Rejected,
}

/// Desired direction for an objective.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// Higher values are better.
    Maximize,
    /// Lower values are better.
    Minimize,
}

/// A finite host-evaluated objective.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectiveScore {
    /// Host measurement; validated as finite before archive use.
    pub value: f64,
    /// Optimization direction, fixed across a comparable archive.
    pub direction: Direction,
}

/// Host supplied evaluation evidence used to construct a record.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VersionMetadata {
    /// Earlier design identity, absent for a founding design.
    pub parent: Option<String>,
    /// Proposal that produced this design.
    pub patch: Patch,
    /// Host or editor explanation.
    pub rationale: String,
    /// Shared transcript sequence references; no transcript payloads.
    pub transcript_refs: Vec<u64>,
    /// Named objectives with explicit comparison directions.
    pub scores: BTreeMap<String, ObjectiveScore>,
    /// Evaluation outcome.
    pub verdict: Verdict,
    /// Every hive-lifecycle identity's host event watermark.
    pub memory_watermarks: BTreeMap<String, u64>,
}

/// An immutable evaluated design record; the host persists its candidate.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    /// Cryptographic design identity, including persistent memory watermarks.
    pub id: String,
    /// Earlier design identity.
    pub parent: Option<String>,
    /// Proposal that produced the candidate.
    pub patch: Patch,
    /// Redundant authority field, checked against the patch.
    pub class: PatchClass,
    /// Explanation of the proposal.
    pub rationale: String,
    /// Supporting transcript sequence references.
    pub transcript_refs: Vec<u64>,
    /// Named finite objective scores.
    pub scores: BTreeMap<String, ObjectiveScore>,
    /// Host evaluation outcome.
    pub verdict: Verdict,
    /// Persistent identity IDs and host event watermarks.
    pub memory_watermarks: BTreeMap<String, u64>,
}

/// A failed archive or identity check.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Identity does not represent the supplied design.
    #[error("invalid content identity")]
    Identity,
    /// Persistent watermarks do not cover exactly the hive identities.
    #[error("persistent memory watermarks do not match identities")]
    Watermarks,
    /// A parent is absent or follows its child.
    #[error("parent must precede child in archive")]
    Parent,
    /// A design is repeated in one archive.
    #[error("duplicate design identity")]
    Duplicate,
    /// No objectives, empty objective names, or nonfinite measurements.
    #[error("objectives must be named, nonempty and finite")]
    Scores,
    /// Objective sets or directions differ.
    #[error("objective scores are incomparable")]
    Incomparable,
    /// Record authority does not match its patch.
    #[error("record class does not match patch")]
    Class,
    /// Package invariants failed.
    #[error("invalid candidate: {0}")]
    Validation(#[from] crate::Error),
    /// Canonical serialization failed.
    #[error("canonical serialization failed: {0}")]
    Wire(#[from] serde_json::Error),
}

/// Result of an archive operation.
pub type Result<T> = std::result::Result<T, Error>;
