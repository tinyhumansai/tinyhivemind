//! Packages read from a directory, edited and refused.
use super::PackageSource;
use crate::config::ConfigError;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn example() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../tinyhivemind-lang/examples/hives/integrations")
}

/// Copy the example to a directory of its own.
fn copy() -> anyhow::Result<tempfile::TempDir> {
    let target = tempfile::tempdir()?;
    let mut pending = vec![(example(), target.path().to_path_buf())];
    while let Some((from, to)) = pending.pop() {
        std::fs::create_dir_all(&to)?;
        for entry in std::fs::read_dir(from)? {
            let entry = entry?;
            let to = to.join(entry.file_name());
            if entry.file_type()?.is_dir() {
                pending.push((entry.path(), to));
            } else {
                std::fs::copy(entry.path(), to)?;
            }
        }
    }
    Ok(target)
}

#[test]
fn a_package_is_loaded_with_the_directory_it_came_from() -> anyhow::Result<()> {
    let root = copy()?;
    // Neither is part of the package, and this one is not even text.
    std::fs::create_dir(root.path().join(".git"))?;
    std::fs::write(root.path().join(".hidden"), [0xff, 0xfe])?;
    let source = PackageSource::load(root.path())?;
    assert_eq!(source.root(), root.path());
    let seats = &source.package().manifest.seats;
    assert_eq!(source.lowered().seats.len(), seats.len());
    let hidden = source
        .package()
        .documents
        .keys()
        .any(|key| key.contains(".hidden"));
    assert!(!hidden);
    Ok(())
}

#[test]
fn a_package_read_unchecked_can_be_edited_and_is_checked_then() -> anyhow::Result<()> {
    let root = copy()?;
    let mut package = PackageSource::read(root.path())?;
    let source = PackageSource::new(root.path(), &package)?;
    assert_eq!(source.package().manifest, package.manifest);
    assert_eq!(source.lowered().id, package.manifest.id);
    package.manifest.seats[0].role = "nobody".into();
    let refused = PackageSource::new(root.path(), &package).err();
    let Some(ConfigError::Package { diagnostics, .. }) = refused else {
        anyhow::bail!("an unknown role was accepted: {refused:?}");
    };
    assert!(diagnostics.iter().any(|found| found.file == "hive.json"));
    Ok(())
}

#[test]
fn a_package_that_does_not_check_is_refused_with_every_finding() -> anyhow::Result<()> {
    let root = copy()?;
    std::fs::write(root.path().join("hive.json"), "{")?;
    let loaded = PackageSource::load(root.path()).err();
    let read = PackageSource::read(root.path()).err();
    for refused in [loaded, read] {
        let Some(ConfigError::Package {
            root: named,
            diagnostics,
        }) = refused
        else {
            anyhow::bail!("a broken manifest was accepted: {refused:?}");
        };
        assert_eq!(named, root.path().display().to_string());
        assert_eq!(diagnostics[0].file, "hive.json");
    }
    let empty = PackageSource::from_documents("/nowhere", &BTreeMap::new());
    assert!(matches!(empty, Err(ConfigError::Package { .. })));
    Ok(())
}

#[test]
fn a_file_that_cannot_be_read_is_named() -> anyhow::Result<()> {
    let root = copy()?;
    let binary = root.path().join("context/binary.md");
    std::fs::create_dir_all(root.path().join("context"))?;
    std::fs::write(&binary, [0xff, 0xfe])?;
    for (source, path) in [
        (root.path().to_path_buf(), binary),
        (root.path().join("none"), root.path().join("none")),
    ] {
        let Some(ConfigError::Io { path: named, .. }) = PackageSource::load(source).err() else {
            anyhow::bail!("an unreadable file was accepted");
        };
        assert_eq!(named, path.display().to_string());
    }
    Ok(())
}
