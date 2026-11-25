//! Framing, bounds and cancellation of the piped streams.
use super::{JsonLines, Limits, LineError, Tail, drain};
use anyhow::{Context as _, Result, bail};
use serde_json::{Value, json};
use std::io;
use std::pin::Pin;
use std::task::{self, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt, DuplexStream, ReadBuf, duplex};

const LIMITS: Limits = Limits {
    line: 64,
    queued: 256,
};

/// The host's end, and the child's stdin (read) and stdout (write) ends.
fn connect(limits: Limits) -> (JsonLines, DuplexStream, DuplexStream) {
    let (host_in, child_in) = duplex(16);
    let (child_out, host_out) = duplex(16);
    let (lines, writing, reading) = JsonLines::over(host_in, host_out, limits);
    tokio::spawn(writing);
    tokio::spawn(reading);
    (lines, child_in, child_out)
}

async fn finish(mut pipe: DuplexStream, bytes: &[u8]) -> Result<()> {
    Ok(pipe.write_all(bytes).await?)
}

#[tokio::test]
async fn each_value_is_one_line_in_each_direction() -> Result<()> {
    let (mut lines, mut child_in, child_out) = connect(LIMITS);
    lines.sender.send(&json!({"a": "x\ny"})).await?;
    lines.sender.send(&json!(2)).await?;
    let reply = finish(child_out, b"{\"b\":1}\n\n[3]\n\"unterminated\"");
    let receive = async {
        let mut received = Vec::new();
        while let Some(value) = lines.receiver.recv().await {
            received.push(value?);
        }
        Ok(received)
    };
    let (received, ()) = tokio::try_join!(receive, reply)?;
    assert_eq!(
        received,
        [json!({"b": 1}), json!([3]), json!("unterminated")]
    );
    drop(lines);
    let mut written = String::new();
    child_in.read_to_string(&mut written).await?;
    assert_eq!(written, "{\"a\":\"x\\ny\"}\n2\n");
    Ok(())
}

#[tokio::test]
async fn a_malformed_line_is_an_error_and_the_next_is_still_read() -> Result<()> {
    let (mut lines, _child_in, child_out) = connect(LIMITS);
    tokio::spawn(async move { finish(child_out, b"not json\n{}\n").await });
    let error = lines
        .receiver
        .recv()
        .await
        .context("missing")?
        .err()
        .context("expected a failure")?;
    assert!(matches!(error, LineError::Malformed(_)));
    assert_eq!(lines.receiver.recv().await.context("missing")??, json!({}));
    assert!(lines.receiver.recv().await.is_none());
    Ok(())
}

#[tokio::test]
async fn an_overlong_line_is_an_error_and_the_next_is_still_read() -> Result<()> {
    let (mut lines, _child_in, child_out) = connect(LIMITS);
    let mut bytes = vec![b'1'; 100];
    bytes.extend_from_slice(b"\n7\n");
    tokio::spawn(async move { finish(child_out, &bytes).await });
    let error = lines
        .receiver
        .recv()
        .await
        .context("missing")?
        .err()
        .context("expected a failure")?;
    assert!(matches!(error, LineError::TooLong { limit: 64 }));
    assert_eq!(lines.receiver.recv().await.context("missing")??, json!(7));
    Ok(())
}

#[tokio::test]
async fn a_child_that_writes_unread_lines_is_drained_and_the_loss_is_reported() -> Result<()> {
    let (mut lines, _child_in, mut child_out) = connect(LIMITS);
    // Far more than the queue holds. If the pipe were not drained this write
    // would never finish.
    for _ in 0..1000 {
        child_out.write_all(b"\"0123456789\"\n").await?;
    }
    let mut values = 0;
    loop {
        match lines.receiver.recv().await {
            Some(Ok(_)) => values += 1,
            Some(Err(error)) => {
                assert!(matches!(error, LineError::Overflow { limit: 256 }));
                break;
            }
            None => bail!("the loss was not reported"),
        }
    }
    assert!((1..1000).contains(&values));
    // The stream is over although the child is still there and writing.
    child_out.write_all(b"{}\n").await?;
    let ended = tokio::time::timeout(Duration::from_secs(10), lines.receiver.recv());
    assert!(ended.await?.is_none());
    for _ in 0..1000 {
        child_out.write_all(b"\"0123456789\"\n").await?;
    }
    Ok(())
}

/// Fails every read after the first bytes.
struct Broken(Option<&'static [u8]>);

impl AsyncRead for Broken {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _: &mut task::Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Poll::Ready(match self.0.take() {
            Some(bytes) => {
                buffer.put_slice(bytes);
                Ok(())
            }
            None => Err(io::Error::other("device gone")),
        })
    }
}

#[tokio::test]
async fn a_pipe_that_cannot_be_read_is_an_error_and_not_the_end_of_the_output() -> Result<()> {
    let (host_in, _child_in) = duplex(16);
    let broken = Broken(Some(b"1\n2"));
    let (mut lines, _, reading) = JsonLines::over(host_in, broken, LIMITS);
    reading.await;
    assert_eq!(lines.receiver.recv().await.context("missing")??, json!(1));
    // Half a line is not a line.
    let error = lines.receiver.recv().await.context("missing")?.err();
    assert!(matches!(error, Some(LineError::Read(_))), "{error:?}");
    assert!(lines.receiver.recv().await.is_none());
    let tail = Tail::new(8);
    drain(Broken(Some(b"log")), tail.clone()).await;
    assert_eq!(tail.text(), "log");
    assert_eq!(tail.failure(), Some(io::ErrorKind::Other));
    Ok(())
}

#[tokio::test]
async fn a_line_too_long_to_send_is_refused_and_none_of_it_is_sent() -> Result<()> {
    let (lines, mut child_in, _child_out) = connect(LIMITS);
    let error = lines.sender.send(&json!("x".repeat(100))).await.err();
    assert!(matches!(error, Some(LineError::TooLong { limit: 64 })));
    lines.sender.send(&json!("x".repeat(62))).await?;
    drop(lines);
    let mut written = String::new();
    child_in.read_to_string(&mut written).await?;
    assert_eq!(written.len(), 65);
    // More than may wait could wait for ever.
    let narrow = Limits {
        line: 64,
        queued: 16,
    };
    let (lines, _child_in, _child_out) = connect(narrow);
    let error = lines.sender.send(&json!("x".repeat(20))).await.err();
    assert!(matches!(error, Some(LineError::TooLong { limit: 15 })));
    Ok(())
}

#[tokio::test]
async fn what_waits_to_be_written_is_bounded_by_bytes() -> Result<()> {
    let (lines, _child_in, _child_out) = connect(LIMITS);
    let long = json!("x".repeat(60));
    let mut sent = 0;
    let waiting = async {
        while sent < 100 {
            lines.sender.send(&long).await?;
            sent += 1;
        }
        Ok::<_, LineError>(())
    };
    let filled = tokio::time::timeout(Duration::from_millis(200), waiting).await;
    assert!(filled.is_err(), "nobody reads, so a send has to wait");
    // 256 bytes may wait, beside what the pipe and the writer hold.
    assert!((1..=6).contains(&sent), "{sent}");
    Ok(())
}

#[tokio::test]
async fn a_dropped_receiver_does_not_stop_the_drain() -> Result<()> {
    let (lines, _child_in, mut child_out) = connect(LIMITS);
    drop(lines);
    for _ in 0..100 {
        child_out.write_all(b"{}\n").await?;
    }
    Ok(())
}

#[tokio::test]
async fn a_queue_that_is_read_never_overflows() -> Result<()> {
    let (mut lines, _child_in, mut child_out) = connect(LIMITS);
    for number in 0..100 {
        child_out
            .write_all(format!("{number}\n").as_bytes())
            .await?;
        assert_eq!(
            lines.receiver.recv().await.context("missing")??,
            json!(number)
        );
    }
    Ok(())
}

#[tokio::test]
async fn abandoning_a_send_never_splits_a_line() -> Result<()> {
    let (lines, mut child_in, _child_out) = connect(LIMITS);
    let long: Value = json!("x".repeat(40));
    // The child is not reading, so once the pipe and queue are full a send
    // waits; give up on many of them.
    for _ in 0..200 {
        let _ = tokio::time::timeout(Duration::from_millis(1), lines.sender.send(&long)).await;
    }
    drop(lines);
    let mut written = String::new();
    child_in.read_to_string(&mut written).await?;
    for line in written.lines() {
        assert_eq!(serde_json::from_str::<Value>(line)?, long);
    }
    assert!(written.ends_with('\n'));
    Ok(())
}

#[tokio::test]
async fn sending_to_a_child_that_closed_its_input_is_an_error() -> Result<()> {
    let (lines, child_in, _child_out) = connect(LIMITS);
    drop(child_in);
    let closed = async {
        loop {
            if let Err(error) = lines.sender.send(&json!(1)).await {
                return error;
            }
        }
    };
    let error = tokio::time::timeout(Duration::from_secs(10), closed).await?;
    assert!(matches!(error, LineError::Closed));
    Ok(())
}

#[tokio::test]
async fn a_tail_keeps_the_last_bytes_and_drains_the_rest() -> Result<()> {
    let tail = Tail::new(8);
    let (mut writer, reader) = duplex(4);
    let draining = tokio::spawn(drain(reader, tail.clone()));
    writer.write_all(b"0123456789abcdef").await?;
    drop(writer);
    draining.await?;
    assert_eq!(tail.text(), "89abcdef");
    Ok(())
}
