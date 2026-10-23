//! A hive package together with the directory it was read from.
//!
//! The language crate opens no files; the adapter does, because executable
//! tools and skill bundles are run and installed from the package directory.
use crate::config::ConfigError;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
use tinyhivemind_lang::{LoweredHive, Package};

/// A checked hive package and the root its relative files resolve against.
#[derive(Clone, Debug)]
pub struct PackageSource {
    root: PathBuf,
    package: Package,
    lowered: LoweredHive,
}

impl PackageSource {
    /// Read every file below `root` and check it as a hive package.
    ///
    /// Dot-files and dot-directories are skipped, as `hive_check` skips them.
    /// # Errors
    /// [`ConfigError::Io`] when a file cannot be read, and
    /// [`ConfigError::Package`] with every finding when the package does not
    /// check.
    pub fn load(root: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let root = root.as_ref();
        Self::from_documents(root, read(root)?)
    }

    /// Check a package already read into memory, rooted at `root`.
    /// # Errors
    /// [`ConfigError::Package`] with every finding when it does not check.
    pub fn from_documents(
        root: impl Into<PathBuf>,
        documents: BTreeMap<String, String>,
    ) -> Result<Self, ConfigError> {
        let root = root.into();
        let failed = |diagnostics| ConfigError::Package {
            root: root.display().to_string(),
            diagnostics,
        };
        let (package, diagnostics) = tinyhivemind_lang::diagnose_documents(&documents);
        let Some(package) = package.filter(|_| diagnostics.is_empty()) else {
            return Err(failed(diagnostics));
        };
        let lowered = tinyhivemind_lang::lower(&package).map_err(|error| {
            failed(vec![tinyhivemind_lang::Diagnostic {
                file: "hive.json".into(),
                pointer: None,
                rule: tinyhivemind_lang::Rule::from(&error),
                message: error.to_string(),
            }])
        })?;
        Ok(Self {
            root,
            package,
            lowered,
        })
    }

    /// Check a package a host built or edited in memory, rooted at `root`.
    ///
    /// For a host that adjusts a loaded package before deploying it — a
    /// per-run deployment id, a program path, a server's arguments.
    /// # Errors
    /// [`ConfigError::Package`] with every finding when it does not check.
    pub fn new(root: impl Into<PathBuf>, package: Package) -> Result<Self, ConfigError> {
        let root = root.into();
        let diagnostics = tinyhivemind_lang::diagnose(&package);
        if !diagnostics.is_empty() {
            return Err(ConfigError::Package {
                root: root.display().to_string(),
                diagnostics,
            });
        }
        let mut documents = package.documents.clone();
        for (name, value) in [
            ("hive.json", serde_json::to_string(&package.manifest)),
            (
                "constitution.json",
                serde_json::to_string(&package.constitution),
            ),
            ("memory.json", serde_json::to_string(&package.memory)),
        ] {
            documents.insert(name.into(), value.map_err(|_| ConfigError::Json)?);
        }
        Self::from_documents(root, documents)
    }

    /// Read a package from disk without checking it, for a host to edit and
    /// then hand to [`PackageSource::new`].
    /// # Errors
    /// [`ConfigError::Io`] when a file cannot be read, and
    /// [`ConfigError::Package`] when the canonical files do not decode.
    pub fn read(root: impl AsRef<Path>) -> Result<Package, ConfigError> {
        let root = root.as_ref();
        let (package, diagnostics) = tinyhivemind_lang::diagnose_documents(&read(root)?);
        package.ok_or_else(|| ConfigError::Package {
            root: root.display().to_string(),
            diagnostics,
        })
    }

    /// The directory relative package files resolve against.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The checked package.
    #[must_use]
    pub fn package(&self) -> &Package {
        &self.package
    }

    /// The package lowered to activation values.
    #[must_use]
    pub fn lowered(&self) -> &LoweredHive {
        &self.lowered
    }
}

fn read(root: &Path) -> Result<BTreeMap<String, String>, ConfigError> {
    let io = |path: &Path, source| ConfigError::Io {
        path: path.display().to_string(),
        source,
    };
    let mut files = BTreeMap::new();
    let mut pending = vec![(root.to_path_buf(), String::new())];
    while let Some((dir, prefix)) = pending.pop() {
        for entry in std::fs::read_dir(&dir).map_err(|error| io(&dir, error))? {
            let entry = entry.map_err(|error| io(&dir, error))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let key = format!("{prefix}{name}");
            let path = entry.path();
            if entry
                .file_type()
                .map_err(|error| io(&path, error))?
                .is_dir()
            {
                pending.push((path, format!("{key}/")));
            } else {
                let text = std::fs::read_to_string(&path).map_err(|error| io(&path, error))?;
                files.insert(key, text);
            }
        }
    }
    Ok(files)
}
