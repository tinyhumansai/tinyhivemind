//! Every shipped package keeps the content id it had before the language
//! learnt about harnesses.
//!
//! [`content_id`] hashes the package's whole serialization, so a new field
//! that serialized when absent would change every id a host has recorded in
//! its lineage. These literals were computed from `main` before the
//! `harnesses` section and the `harness` fields existed; a package that
//! declares neither must hash to exactly them.
use std::{collections::BTreeMap, fs, path::Path};
use tinyhivemind_lang::{MemoryLifecycle, lineage::content_id, parse};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Every file under `dir`, keyed by package-relative path.
fn documents(dir: &Path) -> Result<BTreeMap<String, String>> {
    let mut files = BTreeMap::new();
    let mut pending = vec![(dir.to_path_buf(), String::new())];
    while let Some((dir, prefix)) = pending.pop() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let key = format!("{prefix}{}", entry.file_name().to_string_lossy());
            if entry.file_type()?.is_dir() {
                pending.push((entry.path(), format!("{key}/")));
            } else {
                files.insert(key, fs::read_to_string(entry.path())?);
            }
        }
    }
    Ok(files)
}

/// The content id of the package in `dir`, every persistent memory at
/// watermark zero.
fn id_of(dir: &str) -> Result<String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(dir);
    let package = parse(&documents(&root)?)?;
    let watermarks = package
        .memory
        .identities
        .iter()
        .filter(|identity| identity.lifecycle == MemoryLifecycle::Hive)
        .map(|identity| (identity.id.clone(), 0))
        .collect();
    Ok(content_id(&package, &watermarks)?)
}

#[test]
fn packages_without_a_harness_keep_their_content_ids() -> Result<()> {
    for (dir, expected) in [
        ("examples/hives/review-board", "sha256:x"),
        ("examples/hives/research-debate", "sha256:x"),
        ("examples/hives/incident-response", "sha256:x"),
        ("examples/hives/memory-guild", "sha256:x"),
        ("examples/hives/self-improving", "sha256:x"),
        ("examples/hives/math-panel", "sha256:x"),
        ("examples/hives/math-swarm", "sha256:x"),
        ("examples/hives/integrations", "sha256:x"),
        ("examples/hives/euler-hive", "sha256:x"),
        ("tests/fixtures/deepswe", "sha256:x"),
        ("tests/fixtures/pe1006", "sha256:x"),
    ] {
        assert_eq!(id_of(dir)?, expected, "{dir}");
    }
    Ok(())
}
