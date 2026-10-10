//! Content identities and pure archive folds; hosts own evaluation and storage.

mod types;
use crate::Package;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
pub use types::{Direction, Error, ObjectiveScore, Record, Result, Verdict, VersionMetadata};

/// Hash canonical package contents and pinned persistent memory watermarks.
///
/// JSON object keys are sorted recursively. Watermarks participate in the
/// identity because unchanged specs can behave differently with changed memory.
///
/// # Errors
/// Rejects wire serialization or missing/extra persistent identity watermarks.
pub fn content_id(package: &Package, watermarks: &BTreeMap<String, u64>) -> Result<String> {
    let memory = serde_json::to_value(&package.memory)?;
    let identities = memory["identities"].as_array().ok_or(Error::Watermarks)?;
    let persistent: std::collections::BTreeSet<_> = identities
        .iter()
        .filter(|identity| identity["lifecycle"] == "hive")
        .filter_map(|identity| identity["id"].as_str())
        .collect();
    if persistent.len() != watermarks.len()
        || persistent.iter().any(|id| !watermarks.contains_key(*id))
    {
        return Err(Error::Watermarks);
    }
    let mut value = serde_json::json!({"package": package, "memory_watermarks": watermarks});
    canonicalize(&mut value);
    let bytes = serde_json::to_vec(&value)?;
    Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
}

fn canonicalize(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for value in map.values_mut() {
                canonicalize(value);
            }
            let entries: BTreeMap<_, _> = std::mem::take(map).into_iter().collect();
            map.extend(entries);
        }
        serde_json::Value::Array(values) => {
            for value in values {
                canonicalize(value);
            }
        }
        _ => {}
    }
}

/// Record a host evaluation with the exact candidate and memory it evaluated.
///
/// # Errors
/// Rejects invalid packages, nonfinite/empty scores, or invalid watermarks.
pub fn record(package: &Package, metadata: VersionMetadata) -> Result<Record> {
    crate::validate(package)?;
    validate_scores(&metadata.scores)?;
    let id = content_id(package, &metadata.memory_watermarks)?;
    Ok(Record {
        id,
        parent: metadata.parent,
        class: metadata.patch.class,
        patch: metadata.patch,
        rationale: metadata.rationale,
        transcript_refs: metadata.transcript_refs,
        scores: metadata.scores,
        verdict: metadata.verdict,
        memory_watermarks: metadata.memory_watermarks,
    })
}

/// Verify a record's content identity against an evaluated package snapshot.
///
/// # Errors
/// Rejects a mismatched identity, class, watermark set or invalid scores/package.
pub fn verify(record: &Record, package: &Package) -> Result<()> {
    crate::validate(package)?;
    validate_scores(&record.scores)?;
    if record.class != record.patch.class {
        return Err(Error::Class);
    }
    if record.id != content_id(package, &record.memory_watermarks)? {
        return Err(Error::Identity);
    }
    Ok(())
}

fn validate_scores(scores: &BTreeMap<String, ObjectiveScore>) -> Result<()> {
    if scores.is_empty()
        || scores
            .iter()
            .any(|(name, score)| name.is_empty() || !score.value.is_finite())
    {
        return Err(Error::Scores);
    }
    Ok(())
}

fn archive(records: &[Record]) -> Result<()> {
    let mut seen = std::collections::BTreeSet::new();
    for record in records {
        validate_scores(&record.scores)?;
        if record.class != record.patch.class {
            return Err(Error::Class);
        }
        if !record.id.starts_with("sha256:")
            || record.id.len() != 71
            || !record.id[7..].bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(Error::Identity);
        }
        if record
            .parent
            .as_ref()
            .is_some_and(|parent| !seen.contains(parent))
        {
            return Err(Error::Parent);
        }
        if !seen.insert(record.id.clone()) {
            return Err(Error::Duplicate);
        }
    }
    Ok(())
}

/// Select the last accepted design from an ordered, validated archive.
///
/// Rejected candidates remain in the archive and never activate themselves.
/// Parent references must point to earlier records, including rejected ancestors.
///
/// # Errors
/// Rejects malformed identities, duplicate designs, missing parents or scores.
pub fn accepted_head(records: &[Record]) -> Result<Option<&Record>> {
    archive(records)?;
    Ok(records
        .iter()
        .rev()
        .find(|record| record.verdict == Verdict::Accepted))
}

/// Select all nondominated evaluated designs, preserving archive order.
///
/// Both accepted and rejected designs can be stepping stones. Every design must
/// score the same named objectives with the same directions; equality does not
/// dominate, so tied designs are retained.
///
/// # Errors
/// Rejects invalid archives or scores that cannot be compared consistently.
pub fn pareto_frontier(records: &[Record]) -> Result<Vec<&Record>> {
    archive(records)?;
    if let Some(first) = records.first()
        && records.iter().any(|record| {
            record.scores.len() != first.scores.len()
                || first.scores.iter().any(|(name, score)| {
                    record
                        .scores
                        .get(name)
                        .is_none_or(|other| other.direction != score.direction)
                })
        })
    {
        return Err(Error::Incomparable);
    }
    Ok(records
        .iter()
        .filter(|candidate| {
            !records.iter().any(|other| {
                let mut better = false;
                for (name, score) in &candidate.scores {
                    let other = &other.scores[name];
                    let (a, b) = match score.direction {
                        Direction::Maximize => (other.value, score.value),
                        Direction::Minimize => (score.value, other.value),
                    };
                    if a < b {
                        return false;
                    }
                    better |= a > b;
                }
                better
            })
        })
        .collect())
}

#[cfg(test)]
mod test;
