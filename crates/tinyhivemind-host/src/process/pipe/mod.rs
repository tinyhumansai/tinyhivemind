//! Draining the pipes of a child: JSON lines, and the bounded tail of a log.
//!
//! Every pipe is read to its end whatever the caller does, so a child never
//! blocks on a full pipe. What is *kept* is bounded: a log keeps its tail, and
//! a line stream that outruns its reader ends in [`LineError::Overflow`]
//! rather than dropping a message silently. Waiting lines are kept as the bytes
//! they are and parsed when they are received, as a parsed value may be many
//! times as large as its text.
use serde_json::Value;
use std::future::Future;
use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc};

/// What an entry costs beside its bytes, so that empty lines are bounded too.
const ENTRY_COST: usize = 64;
const CHUNK: usize = 64 * 1024;

/// Why a line could not be sent or received.
#[derive(Debug, thiserror::Error)]
pub enum LineError {
    /// The child closed its input, or exited.
    #[error("the child no longer reads its input")]
    Closed,
    /// The line is not JSON, or the value sent cannot be written as JSON. The
    /// line itself is not repeated: it may hold a secret.
    #[error("malformed line: {0}")]
    Malformed(#[from] serde_json::Error),
    /// A line is over the limit. One the child wrote was skipped, and one to
    /// be sent was not.
    #[error("a line exceeds {limit} bytes")]
    TooLong {
        /// The most bytes one line may have.
        limit: usize,
    },
    /// The child's output could not be read. The stream ends here, which is
    /// not to say that the child closed it.
    #[error("the child's output could not be read: {0}")]
    Read(#[source] io::Error),
    /// The child wrote more than the caller read. Every later line is lost, so
    /// the stream ends here.
    #[error("over {limit} bytes of lines were left unread")]
    Overflow {
        /// The most bytes that may wait unread.
        limit: usize,
    },
}

/// Bounds on a child's line stream, in each direction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    /// Most bytes in one line.
    pub line: usize,
    /// Most bytes waiting to be received, and most waiting to be written.
    /// Besides these, one line each way is in the making.
    pub queued: usize,
}

impl Default for Limits {
    /// 16 MiB lines and 64 MiB waiting.
    fn default() -> Self {
        Self {
            line: 16 * 1024 * 1024,
            queued: 64 * 1024 * 1024,
        }
    }
}

/// Both directions of a child that speaks one JSON value per line.
#[derive(Debug)]
pub struct JsonLines {
    /// Writes to the child's stdin.
    pub sender: LineSender,
    /// Reads from the child's stdout.
    pub receiver: LineReceiver,
}

impl JsonLines {
    /// The streams over `input` and `output`, and the two futures that pump
    /// them. Both pumps end by themselves: the first when every sender is
    /// dropped or the child stops reading, the second at the end of `output`.
    pub(super) fn over(
        input: impl AsyncWrite + Unpin + Send + 'static,
        output: impl AsyncRead + Unpin + Send + 'static,
        limits: Limits,
    ) -> (
        Self,
        impl Future<Output = ()> + Send + 'static,
        impl Future<Output = ()> + Send + 'static,
    ) {
        let (writes, outgoing) = mpsc::unbounded_channel();
        let (entries, received) = mpsc::unbounded_channel();
        let waiting = Arc::new(AtomicUsize::new(0));
        let room = limits.queued.min(Semaphore::MAX_PERMITS);
        let queue = Queue {
            entries: Some(entries),
            waiting: waiting.clone(),
            limit: limits.queued,
        };
        let lines = Self {
            sender: LineSender {
                lines: writes,
                // A line and its end have to fit, or it would wait for ever.
                limit: limits.line.min(room.saturating_sub(1)),
                room: Arc::new(Semaphore::new(room)),
            },
            receiver: LineReceiver {
                entries: received,
                waiting,
            },
        };
        let room = lines.sender.room.clone();
        (
            lines,
            write(input, outgoing, room),
            read(output, limits.line, queue),
        )
    }
}

/// A line to be written, and the room it takes until it is.
type Outgoing = (Vec<u8>, OwnedSemaphorePermit);
/// What a line costs while it waits, and the line, not parsed yet.
type Incoming = (usize, Result<Vec<u8>, LineError>);

/// Sends values to the child. Dropping every clone closes the child's stdin.
#[derive(Clone, Debug)]
pub struct LineSender {
    lines: mpsc::UnboundedSender<Outgoing>,
    /// Bytes that may still wait.
    room: Arc<Semaphore>,
    limit: usize,
}

impl LineSender {
    /// Queue `value` as one line, waiting while there is no room for it.
    ///
    /// Dropping this future sends the whole line or none of it: the write
    /// itself happens elsewhere, so an abandoned turn cannot leave half a
    /// message in the stream.
    ///
    /// # Errors
    /// [`LineError::Malformed`] if `value` cannot be serialized,
    /// [`LineError::TooLong`] if it is over the limit of a line or of what may
    /// wait, and [`LineError::Closed`] once the child no longer reads.
    pub async fn send(&self, value: &Value) -> Result<(), LineError> {
        let mut line = serde_json::to_vec(value)?;
        if line.len() > self.limit {
            return Err(LineError::TooLong { limit: self.limit });
        }
        line.push(b'\n');
        let limit = self.limit;
        let length = u32::try_from(line.len()).map_err(|_| LineError::TooLong { limit })?;
        let room = self.room.clone().acquire_many_owned(length).await;
        let room = room.map_err(|_| LineError::Closed)?;
        let sent = self.lines.send((line, room));
        sent.map_err(|_| LineError::Closed)
    }
}

/// Receives values from the child.
#[derive(Debug)]
pub struct LineReceiver {
    entries: mpsc::UnboundedReceiver<Incoming>,
    waiting: Arc<AtomicUsize>,
}

impl LineReceiver {
    /// The next value, or `None` once the child closed its stdout. Safe to
    /// cancel: a dropped call loses no line.
    ///
    /// The errors inside are [`LineError::Malformed`] and
    /// [`LineError::TooLong`], after which the stream continues, and
    /// [`LineError::Overflow`] and [`LineError::Read`], after which it ends.
    pub async fn recv(&mut self) -> Option<Result<Value, LineError>> {
        let (cost, entry) = self.entries.recv().await?;
        self.waiting.fetch_sub(cost, Ordering::AcqRel);
        Some(entry.and_then(|line| Ok(serde_json::from_slice(&line)?)))
    }
}

/// The unread lines, up to a bound.
struct Queue {
    /// Dropped, so that the receiver ends, after the first that does not fit.
    entries: Option<mpsc::UnboundedSender<Incoming>>,
    waiting: Arc<AtomicUsize>,
    limit: usize,
}

impl Queue {
    /// Keep a line, or what went wrong with it.
    fn push(&mut self, entry: Result<Vec<u8>, LineError>) {
        let Some(entries) = &self.entries else {
            return;
        };
        let cost = ENTRY_COST + entry.as_ref().map_or(0, Vec::len);
        // A receiver dropped meanwhile wants none of it.
        if self.waiting.load(Ordering::Acquire) + cost > self.limit {
            let limit = self.limit;
            let _ = entries.send((0, Err(LineError::Overflow { limit })));
            self.entries = None;
        } else {
            self.waiting.fetch_add(cost, Ordering::AcqRel);
            let _ = entries.send((cost, entry));
        }
    }
}

/// Write each queued line whole, until every sender is gone.
async fn write(
    mut input: impl AsyncWrite + Unpin,
    mut lines: mpsc::UnboundedReceiver<Outgoing>,
    room: Arc<Semaphore>,
) {
    while let Some((line, _room)) = lines.recv().await {
        if input.write_all(&line).await.is_err() || input.flush().await.is_err() {
            break;
        }
    }
    // Whoever waits for room waits in vain.
    room.close();
}

/// Split `output` into lines until its end.
async fn read(mut output: impl AsyncRead + Unpin, limit: usize, mut queue: Queue) {
    let mut chunk = vec![0; CHUNK];
    let mut line = Vec::new();
    let mut too_long = false;
    loop {
        let count = match output.read(&mut chunk).await {
            Ok(0) => return end_line(&mut line, &mut too_long, limit, &mut queue),
            Ok(count) => count,
            Err(error) => return queue.push(Err(LineError::Read(error))),
        };
        let mut rest = &chunk[..count];
        while let Some(end) = rest.iter().position(|byte| *byte == b'\n') {
            extend(&mut line, &rest[..end], &mut too_long, limit);
            end_line(&mut line, &mut too_long, limit, &mut queue);
            rest = &rest[end + 1..];
        }
        extend(&mut line, rest, &mut too_long, limit);
    }
}

fn extend(line: &mut Vec<u8>, bytes: &[u8], too_long: &mut bool, limit: usize) {
    if *too_long || line.len() + bytes.len() > limit {
        *too_long = true;
        line.clear();
    } else {
        line.extend_from_slice(bytes);
    }
}

fn end_line(line: &mut Vec<u8>, too_long: &mut bool, limit: usize, queue: &mut Queue) {
    if std::mem::take(too_long) {
        queue.push(Err(LineError::TooLong { limit }));
    } else if !line.trim_ascii().is_empty() {
        queue.push(Ok(line.as_slice().to_vec()));
    }
    line.clear();
}

/// The last bytes of a stream. Clones share them.
#[derive(Clone, Debug)]
pub(super) struct Tail {
    kept: Arc<Mutex<Kept>>,
    limit: usize,
}

#[derive(Debug, Default)]
struct Kept {
    bytes: Vec<u8>,
    failure: Option<io::ErrorKind>,
}

impl Tail {
    pub(super) fn new(limit: usize) -> Self {
        Self {
            kept: Arc::default(),
            limit,
        }
    }

    fn lock(&self) -> MutexGuard<'_, Kept> {
        self.kept.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The kept bytes as text, with invalid UTF-8 replaced.
    pub(super) fn text(&self) -> String {
        String::from_utf8_lossy(&self.lock().bytes).into_owned()
    }

    /// Why the stream could not be read to its end, if it could not.
    pub(super) fn failure(&self) -> Option<io::ErrorKind> {
        self.lock().failure
    }

    fn push(&self, bytes: &[u8]) {
        let mut kept = self.lock();
        kept.bytes.extend_from_slice(bytes);
        let excess = kept.bytes.len().saturating_sub(self.limit);
        kept.bytes.drain(..excess);
    }
}

/// Read `pipe` to its end into `tail`.
pub(super) async fn drain(mut pipe: impl AsyncRead + Unpin, tail: Tail) {
    let mut chunk = vec![0; CHUNK];
    loop {
        match pipe.read(&mut chunk).await {
            Ok(0) => return,
            Ok(count) => tail.push(&chunk[..count]),
            Err(error) => {
                tail.lock().failure = Some(error.kind());
                return;
            }
        }
    }
}

#[cfg(test)]
mod test;
