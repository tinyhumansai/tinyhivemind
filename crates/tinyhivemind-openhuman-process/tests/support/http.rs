//! A small HTTP/1.1 server for the fakes: one request per connection, a JSON
//! answer or a close-delimited stream, and notice when the client hangs up.
//!
//! Hand-written over Tokio so that the fake child needs no web framework. It
//! reads a request line, headers and a `content-length` body; nothing else of
//! HTTP is needed by the clients it serves.
use serde_json::Value;
use std::future::Future;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

/// Largest request the fakes read.
const LIMIT: usize = 4 * 1024 * 1024;

/// One request.
#[derive(Clone, Debug, Default)]
pub(crate) struct Request {
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) query: String,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: Vec<u8>,
}

impl Request {
    /// The header `name`, case-insensitively.
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        let found = self
            .headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name));
        found.map(|(_, value)| value.as_str())
    }

    /// The query parameter `name`, percent-decoded.
    pub(crate) fn param(&self, name: &str) -> Option<String> {
        self.query.split('&').find_map(|pair| {
            let (key, value) = pair.split_once('=')?;
            (key == name).then(|| decode(value))
        })
    }

    /// The body as JSON, or null.
    pub(crate) fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or(Value::Null)
    }
}

fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let hex = bytes.get(index + 1..index + 3).and_then(|hex| {
            let hex = std::str::from_utf8(hex).ok()?;
            u8::from_str_radix(hex, 16).ok()
        });
        match (bytes[index], hex) {
            (b'%', Some(byte)) => {
                out.push(byte);
                index += 3;
            }
            (b'+', _) => {
                out.push(b' ');
                index += 1;
            }
            (byte, _) => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// An answer.
#[derive(Debug)]
pub(crate) enum Reply {
    /// A status and a JSON body.
    Json(u16, Value),
    /// `text/event-stream`, one chunk per message, until the sender is
    /// dropped. Once the client hangs up the receiver is dropped, which the
    /// sender sees as `closed()`.
    Stream(mpsc::Receiver<String>),
}

/// Serve `handler` on `listener` until the task is dropped.
pub(crate) async fn serve<H, F>(listener: TcpListener, handler: H)
where
    H: Fn(Request) -> F + Send + Sync + 'static,
    F: Future<Output = Reply> + Send + 'static,
{
    let handler = Arc::new(handler);
    while let Ok((stream, _)) = listener.accept().await {
        let handler = handler.clone();
        tokio::spawn(async move {
            let _ = connection(stream, handler.as_ref()).await;
        });
    }
}

async fn connection<H, F>(mut stream: TcpStream, handler: &H) -> std::io::Result<()>
where
    H: Fn(Request) -> F,
    F: Future<Output = Reply>,
{
    let Some(request) = read(&mut stream).await? else {
        return Ok(());
    };
    match handler(request).await {
        Reply::Json(status, body) => {
            let body = body.to_string();
            let head = format!(
                "HTTP/1.1 {status} {}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                reason(status),
                body.len()
            );
            stream.write_all(head.as_bytes()).await?;
            stream.write_all(body.as_bytes()).await?;
            stream.shutdown().await
        }
        Reply::Stream(mut chunks) => {
            let head = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\nconnection: close\r\n\r\n";
            stream.write_all(head.as_bytes()).await?;
            stream.flush().await?;
            let (mut reader, mut writer) = stream.into_split();
            let mut scratch = [0_u8; 256];
            loop {
                tokio::select! {
                    chunk = chunks.recv() => match chunk {
                        Some(chunk) => {
                            writer.write_all(chunk.as_bytes()).await?;
                            writer.flush().await?;
                        }
                        None => return writer.shutdown().await,
                    },
                    read = reader.read(&mut scratch) => {
                        if matches!(read, Ok(0) | Err(_)) {
                            // Hung up: dropping `chunks` tells the sender.
                            return Ok(());
                        }
                    }
                }
            }
        }
    }
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        401 => "Unauthorized",
        404 => "Not Found",
        _ => "Status",
    }
}

async fn read(stream: &mut TcpStream) -> std::io::Result<Option<Request>> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 8192];
    let head_end = loop {
        if let Some(end) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            break end;
        }
        let read = stream.read(&mut chunk).await?;
        if read == 0 || buffer.len() > LIMIT {
            return Ok(None);
        }
        buffer.extend_from_slice(&chunk[..read]);
    };
    let head = String::from_utf8_lossy(&buffer[..head_end]).into_owned();
    let mut lines = head.split("\r\n");
    let mut first = lines.next().unwrap_or_default().split(' ');
    let method = first.next().unwrap_or_default().to_owned();
    let target = first.next().unwrap_or_default();
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let headers: Vec<(String, String)> = lines
        .filter_map(|line| {
            let (key, value) = line.split_once(':')?;
            Some((key.trim().to_owned(), value.trim().to_owned()))
        })
        .collect();
    let mut request = Request {
        method,
        path: path.to_owned(),
        query: query.to_owned(),
        headers,
        body: buffer[head_end + 4..].to_vec(),
    };
    let length: usize = request
        .header("content-length")
        .and_then(|length| length.parse().ok())
        .unwrap_or(0);
    while request.body.len() < length.min(LIMIT) {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            return Ok(None);
        }
        request.body.extend_from_slice(&chunk[..read]);
    }
    Ok(Some(request))
}

/// One SSE message.
pub(crate) fn event(name: &str, data: &Value) -> String {
    format!("event: {name}\ndata: {data}\n\n")
}
