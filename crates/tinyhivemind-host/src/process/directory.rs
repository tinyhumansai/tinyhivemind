//! The private directories of one instance.
use super::{Error, Result};
use std::fs::{self, DirBuilder, Permissions};
use std::io::{ErrorKind, Read};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

/// A directory only this user can enter, holding one instance's state.
///
/// The state directory outlives the process, so a restarted instance finds its
/// sessions there. Each start also gets an empty temporary directory, removed
/// once that start is proven stopped.
#[derive(Clone, Debug)]
pub struct InstanceDir {
    root: PathBuf,
    state: PathBuf,
}

impl InstanceDir {
    /// Make or reopen the instance at `root`, an absolute path whose parent
    /// exists, with mode 0700 on it and on its state directory.
    ///
    /// # Errors
    /// [`Error::Launch`] for a relative path, [`Error::NotPrivate`] if
    /// something other than this user's own directory is there (a symlink is
    /// not followed), and [`Error::Directory`] when the system refuses.
    pub fn create(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        if !root.is_absolute() {
            return Err(Error::Launch(
                "the instance directory is not an absolute path",
            ));
        }
        let state = root.join("state");
        private(&root)?;
        private(&state)?;
        Ok(Self { root, state })
    }

    /// The instance directory.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The directory that survives a restart.
    #[must_use]
    pub fn state(&self) -> &Path {
        &self.state
    }

    /// An empty private directory named after `token`.
    pub(super) fn temporary(&self, token: &str) -> Result<PathBuf> {
        // Short, because a Unix socket path has about 100 bytes in all.
        let path = self.root.join(format!("tmp-{}", &token[..8]));
        private(&path)?;
        Ok(path)
    }
}

/// Make `path` a directory of mode 0700 that this user owns.
fn private(path: &Path) -> Result<()> {
    let failed = |source| Error::Directory {
        path: path.to_path_buf(),
        source,
    };
    match DirBuilder::new().mode(0o700).create(path) {
        Err(error) if error.kind() != ErrorKind::AlreadyExists => return Err(failed(error)),
        _ => {}
    }
    let found = fs::symlink_metadata(path).map_err(failed)?;
    if !found.is_dir() || found.uid() != rustix::process::geteuid().as_raw() {
        return Err(Error::NotPrivate {
            path: path.to_path_buf(),
        });
    }
    fs::set_permissions(path, Permissions::from_mode(0o700)).map_err(failed)
}

/// 128 random bits, as hex.
pub(super) fn token() -> Result<String> {
    let path = Path::new("/dev/urandom");
    let mut bytes = [0; 16];
    fs::File::open(path)
        .and_then(|mut source| source.read_exact(&mut bytes))
        .map_err(|source| Error::Directory {
            path: path.to_path_buf(),
            source,
        })?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}
