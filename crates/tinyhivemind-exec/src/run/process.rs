//! Starting the program, feeding stdin, collecting stdout and stderr, and
//! enforcing the deadline.
use crate::{Error, Result};
use std::ffi::OsString;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::process::{ExitStatus, Stdio};
use std::time::Duration;
use tinyhivemind_lang::ExecTool;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, Command};

/// Most stdout kept: 1 MiB.
const STDOUT_LIMIT: usize = 1024 * 1024;
/// Stderr kept for a failure: its last 4 KiB.
const STDERR_TAIL: usize = 4 * 1024;

/// Everything a call needs to start its program.
pub(super) struct Launch {
    pub(super) program: PathBuf,
    pub(super) arguments: Vec<String>,
    pub(super) environment: Vec<(String, OsString)>,
    pub(super) directory: PathBuf,
    /// The bytes written to stdin; `None` gives the program a null stdin.
    pub(super) input: Option<Vec<u8>>,
}

/// What a finished program left behind.
pub(super) struct Finished {
    pub(super) status: ExitStatus,
    pub(super) stdout: Vec<u8>,
    pub(super) truncated: bool,
    pub(super) stderr: Vec<u8>,
}

/// Start `launch` in a process group of its own, exchange its pipes and wait
/// for it, killing the whole group if `tool.timeout_ms` passes first.
pub(super) async fn execute(tool: &ExecTool, launch: Launch) -> Result<Finished> {
    let mut command = Command::new(&launch.program);
    command
        .args(&launch.arguments)
        .env_clear()
        .envs(launch.environment)
        .current_dir(&launch.directory)
        .stdin(if launch.input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);
    let mut child = spawn(&mut command).await.map_err(|source| Error::Spawn {
        program: tool.command.first().cloned().unwrap_or_default(),
        source,
    })?;
    let group = child.id();
    let exchange = exchange(&mut child, launch.input);
    let deadline = Duration::from_millis(tool.timeout_ms);
    let Ok(finished) = tokio::time::timeout(deadline, exchange).await else {
        kill(&mut child, group).await;
        return Err(Error::Timeout {
            tool: tool.id.clone(),
            timeout_ms: tool.timeout_ms,
        });
    };
    finished.map_err(|source| Error::Io {
        tool: tool.id.clone(),
        source,
    })
}

/// Start `command`, retrying briefly while its file is open for writing.
///
/// Exec fails with "text file busy" while any process holds the program open
/// for writing. That includes a child this process forked in the instant
/// between another thread writing a script and closing it, so a host that
/// edits package scripts while it runs tools would otherwise see spurious
/// failures. Ten attempts back off for 450 ms in all.
async fn spawn(command: &mut Command) -> std::io::Result<Child> {
    const ATTEMPTS: u64 = 10;
    let mut attempt = 1;
    loop {
        match command.spawn() {
            Err(error) if error.kind() == ErrorKind::ExecutableFileBusy && attempt < ATTEMPTS => {
                tokio::time::sleep(Duration::from_millis(10 * attempt)).await;
                attempt += 1;
            }
            spawned => return spawned,
        }
    }
}

/// Write stdin, read both outputs and wait, all at once so that no pipe
/// fills while another is awaited.
async fn exchange(child: &mut Child, input: Option<Vec<u8>>) -> std::io::Result<Finished> {
    let stdin = child.stdin.take();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let feed = async move {
        let (Some(mut stdin), Some(input)) = (stdin, input) else {
            return Ok(());
        };
        match stdin.write_all(&input).await {
            // A program that exits without reading its stdin is not a failure.
            Err(error) if error.kind() == ErrorKind::BrokenPipe => Ok(()),
            written => written,
        }
    };
    let (written, stdout, stderr, status) = tokio::join!(
        feed,
        drain(stdout, STDOUT_LIMIT, Keep::Head),
        drain(stderr, STDERR_TAIL, Keep::Tail),
        child.wait(),
    );
    written?;
    let (stdout, truncated) = stdout?;
    Ok(Finished {
        status: status?,
        stdout,
        truncated,
        stderr: stderr?.0,
    })
}

/// Which end of an over-long stream to keep.
#[derive(Clone, Copy)]
enum Keep {
    /// The first `limit` bytes.
    Head,
    /// The last `limit` bytes.
    Tail,
}

/// Read `pipe` to its end, keeping `limit` bytes, and say whether any were
/// dropped. Reading continues past the limit so the program never blocks on
/// a full pipe.
async fn drain(
    pipe: Option<impl AsyncRead + Unpin>,
    limit: usize,
    keep: Keep,
) -> std::io::Result<(Vec<u8>, bool)> {
    let mut kept = Vec::new();
    let mut dropped = false;
    let Some(mut pipe) = pipe else {
        return Ok((kept, dropped));
    };
    let mut chunk = vec![0; 64 * 1024];
    loop {
        let read = pipe.read(&mut chunk).await?;
        if read == 0 {
            return Ok((kept, dropped));
        }
        kept.extend_from_slice(&chunk[..read]);
        if kept.len() > limit {
            dropped = true;
            match keep {
                Keep::Head => kept.truncate(limit),
                Keep::Tail => {
                    kept.drain(..kept.len() - limit);
                }
            }
        }
    }
}

/// Kill the program's whole process group, then reap the program.
async fn kill(child: &mut Child, group: Option<u32>) {
    #[cfg(unix)]
    if let Some(group) = group {
        // The group outlives its leader while any member runs, so this
        // reaches grandchildren even when the leader has already exited.
        let _ = Command::new("kill")
            .args(["-s", "KILL", "--", &format!("-{group}")])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
    }
    #[cfg(not(unix))]
    let _ = group;
    let _ = child.start_kill();
    let _ = child.wait().await;
}
