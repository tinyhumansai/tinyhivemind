//! Framing, bounds and cancellation of the piped streams.
use super::{JsonLines, LineError, Limits, Tail, drain};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream, duplex};

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
    pipe.write_all(bytes).await?;
    Ok(())
}

#[tokio::test]
async fn each_value_is_one_line_in_each_direction() -> Result<()> {
    let (mut lines, mut child_in, child_out) = connect(LIMITS);
    lines.sender.send(&json!({"a": "x\ny"})).await?;
    lines.sender.send(&json!(2)).await?;
    let reply = finish(child_out, b"{\"b\":1}\n\n[3]\n\"unterminated\"");
    let (received, ()) = tokio::join!(
        async {
            let mut received = Vec::new();
            while let Some(value) = lines.receiver.recv().await {
                received.push(value?);
            }
            received
        },
        reply
    );
    assert_eq!(received, [json!({"b": 1}), json!([3]), json!("unterminated")]);
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
    let error = lines.receiver.recv().await.context("missing")?.err().context("expected a failure")?;
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
    let error = lines.receiver.recv().await.context("missing")?.err().context("expected a failure")?;
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
    drop(child_out);
    let mut values = 0;
    loop {
        match lines.receiver.recv().await {
            Some(Ok(_)) => values += 1,
            Some(Err(error)) => {
                assert!(matches!(error, LineError::Overflow { limit: 256 }));
                break;
            }
            None => panic!("the loss was not reported"),
        }
    }
    assert!((1..1000).contains(&values));
    assert!(lines.receiver.recv().await.is_none());
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
            .await
            ?;
        assert_eq!(lines.receiver.recv().await.context("missing")??, json!(number));
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
    let error = tokio::time::timeout(Duration::from_secs(10), closed)
        .await
        ?;
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
