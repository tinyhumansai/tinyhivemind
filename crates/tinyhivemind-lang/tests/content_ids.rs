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
    let pinned = [
        (
            "examples/hives/review-board",
            "sha256:9ce4a1dd15d31acf69e8d6eddfd050a4eceb8fa083a2b3679cd74fa7a60a8488",
        ),
        (
            "examples/hives/research-debate",
            "sha256:00f5a62cc6ba43bd30a08e64eb7a486222d075c3570006e98b8f5bf0282763f5",
        ),
        (
            "examples/hives/incident-response",
            "sha256:d67a63ce6992b418dc40412e8e17f2c35591dffd1fae5ed70813d68d740b4ffa",
        ),
        (
            "examples/hives/memory-guild",
            "sha256:71ed8edf6e03d6d4f39643c1c1e077b2fa5680e2444687af9793823eb048e867",
        ),
        (
            "examples/hives/self-improving",
            "sha256:9bc13bc144458208cf05a3b9fb7712eb7246da311f5e1b85355eaf17d19c9b37",
        ),
        (
            "examples/hives/math-panel",
            "sha256:f819b3f021d11395c5229c23c29b3b9fb5b6c54423bbf6013d8ad21bde127661",
        ),
        (
            "examples/hives/math-swarm",
            "sha256:d2bbe36d39974e0d26390b8325acc2d35d4e7f0769bfef975371ba4b2211cacc",
        ),
        (
            "examples/hives/integrations",
            "sha256:7cd70afd74951dcb2d174da71f7229b735c2025fa99b089bbb6a90bcfb498268",
        ),
        (
            "examples/hives/euler-hive",
            "sha256:eca2fd31927bd9132b3cc748b03fcab206718cb7c265ed0e1bf26a0a031489ff",
        ),
        (
            "tests/fixtures/deepswe",
            "sha256:aace3549f9fa4870dd862c34717a875b863e40cc9fc00586b94e478bc963d62b",
        ),
        (
            "tests/fixtures/pe1006",
            "sha256:17973e67b5fb89a67183a71d28d4ba4ffbca05c9c83b9a14b7698cb823852767",
        ),
    ];
    let actual = pinned
        .iter()
        .map(|(dir, _)| Ok((*dir, id_of(dir)?)))
        .collect::<Result<Vec<_>>>()?;
    let expected: Vec<_> = pinned
        .iter()
        .map(|(dir, id)| (*dir, (*id).to_owned()))
        .collect();
    assert_eq!(actual, expected);
    Ok(())
}
