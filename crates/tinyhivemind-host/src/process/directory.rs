//! The private directories of one instance.
use super::types::TEMPORARY;
use super::{Error, Result};
use std::fs::{self, DirBuilder, File, Permissions, TryLockError};
use std::io::{self, ErrorKind, Read};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// What the name of a temporary directory starts with, and how many digits of
/// its start's token follow.
const PREFIX: &str = "tmp-";
const TOKEN: usize = 8;
/// Longest a start waits for the instance to be tidied.
const TIDYING: Duration = Duration::from_secs(10);

/// A directory only this user can enter, holding one instance's state.
///
/// The state directory outlives the process, so a restarted instance finds its
/// sessions there. Each start also gets an empty temporary directory, removed
/// once that start is proven stopped. One left by a host that crashed is
/// removed, on Linux, when the instance is opened again and no instance of it
/// is running here or is found to be using it.
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
        let dir = Self { root, state };
        dir.tidy()?;
        Ok(dir)
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

    /// An empty private directory named after `token`, for one start.
    pub(super) async fn temporary(&self, token: &str) -> Result<Temporary> {
        let lease = self.lock()?;
        let started = Instant::now();
        // Held by whoever tidies up.
        while let Err(refused) = lease.try_lock_shared() {
            match refused {
                TryLockError::WouldBlock if started.elapsed() < TIDYING => {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
                refused => return Err(at(&self.root)(refused.into())),
            }
        }
        // Short, because a Unix socket path has about 100 bytes in all.
        let path = self.root.join(format!("{PREFIX}{}", &token[..TOKEN]));
        private(&path)?;
        Ok(Temporary {
            path,
            spent: true,
            lease,
        })
    }

    /// The file every running instance of this directory holds a lock on. The
    /// system lets go of it when a host ends, however it ends.
    fn lock(&self) -> Result<File> {
        let path = self.root.join("lock");
        let mut options = File::options();
        options.create(true).truncate(false).write(true);
        options.open(&path).map_err(at(&path))
    }

    /// Remove the temporary directories nobody uses any more.
    fn tidy(&self) -> Result<()> {
        let lock = self.lock()?;
        // Somebody is running: theirs cannot be told from the others.
        if lock.try_lock().is_ok() {
            for entry in fs::read_dir(&self.root).map_err(at(&self.root))?.flatten() {
                let path = entry.path();
                if is_spent(&path) {
                    // What cannot be removed is private and harmless.
                    let _ = fs::remove_dir_all(path);
                }
            }
            unlock(&lock);
        }
        Ok(())
    }
}

/// Let go of `lock` for good. Closing it would not do: a child forked by some
/// other thread at this moment has it too, until it execs.
pub(super) fn unlock(lock: &File) {
    // It is let go of at the latest then.
    let _ = lock.unlock();
}

/// Whether `path` is a temporary directory of ours that no process uses.
fn is_spent(path: &Path) -> bool {
    let name = path.file_name().and_then(|name| name.to_str());
    let token = name.and_then(|name| name.strip_prefix(PREFIX));
    let is_ours = token.is_some_and(|token| {
        let hex = |byte: u8| matches!(byte, b'0'..=b'9' | b'a'..=b'f');
        token.len() == TOKEN && token.bytes().all(hex)
    }) && fs::symlink_metadata(path)
        .is_ok_and(|found| found.is_dir() && found.uid() == rustix::process::geteuid().as_raw());
    is_ours && is_unused(path)
}

/// Whether no process is found that was started with `path` as its `TMPDIR`.
#[cfg(target_os = "linux")]
fn is_unused(path: &Path) -> bool {
    let mut entry = std::ffi::OsString::from(TEMPORARY);
    entry.push("=");
    entry.push(path);
    super::tree::in_use(entry.as_encoded_bytes()).is_ok_and(|in_use| !in_use)
}

/// Nothing says who uses `path`, so somebody may.
#[cfg(not(target_os = "linux"))]
fn is_unused(_: &Path) -> bool {
    false
}

/// The temporary directory of one start, and its claim on the instance.
#[derive(Debug)]
pub(super) struct Temporary {
    path: PathBuf,
    /// Whether nothing uses it, so that it goes when this does.
    spent: bool,
    lease: File,
}

impl Temporary {
    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    /// Say whether anything may still use it.
    pub(super) fn in_use(&mut self, in_use: bool) {
        self.spent = !in_use;
    }
}

impl Drop for Temporary {
    fn drop(&mut self) {
        if self.spent {
            // A leftover directory is private and harmless.
            let _ = fs::remove_dir_all(&self.path);
        }
        unlock(&self.lease);
    }
}

/// Make `path` a directory of mode 0700 that this user owns.
fn private(path: &Path) -> Result<()> {
    match DirBuilder::new().mode(0o700).create(path) {
        Err(error) if error.kind() != ErrorKind::AlreadyExists => return Err(at(path)(error)),
        _ => {}
    }
    let found = fs::symlink_metadata(path).map_err(at(path))?;
    if !found.is_dir() || found.uid() != rustix::process::geteuid().as_raw() {
        return Err(Error::NotPrivate {
            path: path.to_path_buf(),
        });
    }
    fs::set_permissions(path, Permissions::from_mode(0o700)).map_err(at(path))
}

/// 128 random bits, as hex.
pub(super) fn token() -> Result<String> {
    let source = Path::new("/dev/urandom");
    let mut bytes = [0; 16];
    let mut file = File::open(source).map_err(at(source))?;
    file.read_exact(&mut bytes).map_err(at(source))?;
    Ok(format!("{:032x}", u128::from_ne_bytes(bytes)))
}

/// A failure at `path`.
fn at(path: &Path) -> impl FnOnce(io::Error) -> Error {
    let path = path.to_path_buf();
    move |source| Error::Directory { path, source }
}
