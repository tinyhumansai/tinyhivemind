//! Draining the pipes of a child: JSON lines, and the bounded tail of a log.
//!
//! Every pipe is read to its end whatever the caller does, so a child never
//! blocks on a full pipe. What is *kept* is bounded: a log keeps its tail, and
//! a line stream that outruns its reader ends in [`LineError::Overflow`]
//! rather than dropping a message silently.
use serde_json::Value;
use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc;

/// Lines accepted for writing before a send waits.
const WRITE_QUEUE: usize = 64;
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
    /// The child wrote a line over the limit. It was skipped.
    #[error("a line exceeds {limit} bytes")]
    TooLong {
        /// The most bytes one line may have.
        limit: usize,
    },
    /// The child wrote more than the caller read. Every later line is lost, so
    /// the stream ends here.
    #[error("over {limit} bytes of lines were left unread")]
    Overflow {
        /// The most bytes that may wait unread.
        limit: usize,
    },
}

/// Bounds on a child's line stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    /// Most bytes in one line.
    pub line: usize,
    /// Most bytes waiting to be received.
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
        let (sender, writes) = mpsc::channel(WRITE_QUEUE);
        let (entries, received) = mpsc::unbounded_channel();
        let waiting = Arc::new(AtomicUsize::new(0));
        let queue = Queue {
            entries,
            waiting: waiting.clone(),
            limit: limits.queued,
            overflowed: false,
        };
        let lines = Self {
            sender: LineSender(sender),
            receiver: LineReceiver {
                entries: received,
                waiting,
            },
        };
        (lines, write(input, writes), read(output, limits.line, queue))
    }
}

/// Sends values to the child. Dropping every clone closes the child's stdin.
#[derive(Clone, Debug)]
pub struct LineSender(mpsc::Sender<Vec<u8>>);

impl LineSender {
    /// Queue `value` as one line, waiting while the queue is full.
    ///
    /// Dropping this future sends the whole line or none of it: the write
    /// itself happens elsewhere, so an abandoned turn cannot leave half a
    /// message in the stream.
    ///
    /// # Errors
    /// [`LineError::Malformed`] if `value` cannot be serialized and
    /// [`LineError::Closed`] once the child no longer reads.
    pub async fn send(&self, value: &Value) -> Result<(), LineError> {
        let mut line = serde_json::to_vec(value)?;
        line.push(b'\n');
        self.0.send(line).await.map_err(|_| LineError::Closed)
    }
}

/// Receives values from the child.
#[derive(Debug)]
pub struct LineReceiver {
    entries: mpsc::UnboundedReceiver<(usize, Result<Value, LineError>)>,
    waiting: Arc<AtomicUsize>,
}

impl LineReceiver {
    /// The next value, or `None` once the child closed its stdout. Safe to
    /// cancel: a dropped call loses no line.
    ///
    /// The errors inside are [`LineError::Malformed`] and
    /// [`LineError::TooLong`], after which the stream continues, and
    /// [`LineError::Overflow`], after which it ends.
    pub async fn recv(&mut self) -> Option<Result<Value, LineError>> {
        let (cost, entry) = self.entries.recv().await?;
        self.waiting.fetch_sub(cost, Ordering::AcqRel);
        Some(entry)
    }
}

/// The unread lines, up to a bound.
struct Queue {
    entries: mpsc::UnboundedSender<(usize, Result<Value, LineError>)>,
    waiting: Arc<AtomicUsize>,
    limit: usize,
    overflowed: bool,
}

impl Queue {
    /// Whether a line of `length` bytes still fits. The first that does not
    /// ends the stream.
    fn admits(&mut self, length: usize) -> bool {
        if self.overflowed || self.entries.is_closed() {
            return false;
        }
        let waiting = self.waiting.load(Ordering::Acquire);
        if waiting + length + ENTRY_COST > self.limit {
            self.overflowed = true;
            let limit = self.limit;
            self.push(0, Err(LineError::Overflow { limit }));
        }
        !self.overflowed
    }

    fn push(&self, cost: usize, entry: Result<Value, LineError>) {
        self.waiting.fetch_add(cost, Ordering::AcqRel);
        // A receiver dropped meanwhile wants none of it.
        let _ = self.entries.send((cost, entry));
    }
}

/// Write each queued line whole, until every sender is gone.
async fn write(mut input: impl AsyncWrite + Unpin, mut lines: mpsc::Receiver<Vec<u8>>) {
    while let Some(line) = lines.recv().await {
        if input.write_all(&line).await.is_err() || input.flush().await.is_err() {
            return;
        }
    }
}

/// Split `output` into lines until its end.
async fn read(mut output: impl AsyncRead + Unpin, limit: usize, mut queue: Queue) {
    let mut chunk = vec![0; CHUNK];
    let mut line = Vec::new();
    let mut too_long = false;
    loop {
        let count = output.read(&mut chunk).await.unwrap_or(0);
        if count == 0 {
            end_line(&mut line, &mut too_long, limit, &mut queue);
            return;
        }
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
    let overlong = std::mem::take(too_long);
    if (overlong || !line.trim_ascii().is_empty()) && queue.admits(line.len()) {
        let entry = if overlong {
            Err(LineError::TooLong { limit })
        } else {
            serde_json::from_slice(line).map_err(LineError::from)
        };
        queue.push(line.len() + ENTRY_COST, entry);
    }
    line.clear();
}

/// The last bytes of a stream. Clones share them.
#[derive(Clone, Debug)]
pub(super) struct Tail {
    kept: Arc<Mutex<Vec<u8>>>,
    limit: usize,
}

impl Tail {
    pub(super) fn new(limit: usize) -> Self {
        Self {
            kept: Arc::default(),
            limit,
        }
    }

    /// The kept bytes as text, with invalid UTF-8 replaced.
    pub(super) fn text(&self) -> String {
        let kept = self.kept.lock().unwrap_or_else(PoisonError::into_inner);
        String::from_utf8_lossy(&kept).into_owned()
    }

    fn push(&self, bytes: &[u8]) {
        let mut kept = self.kept.lock().unwrap_or_else(PoisonError::into_inner);
        kept.extend_from_slice(bytes);
        let excess = kept.len().saturating_sub(self.limit);
        kept.drain(..excess);
    }
}

/// Read `pipe` to its end into `tail`.
pub(super) async fn drain(mut pipe: impl AsyncRead + Unpin, tail: Tail) {
    let mut chunk = vec![0; CHUNK];
    while let Ok(count @ 1..) = pipe.read(&mut chunk).await {
        tail.push(&chunk[..count]);
    }
}

#[cfg(test)]
mod test;
