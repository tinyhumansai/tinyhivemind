//! Server-Sent Events, as `/events` frames them: `event:` and `data:` lines,
//! a blank line ending each message, and `:` comments for keep-alive.
use super::Error;
use serde_json::Value;

/// Bytes in, JSON payloads out, whatever the chunking.
#[derive(Debug, Default)]
pub(crate) struct Parser {
    line: Vec<u8>,
    data: Vec<String>,
}

impl Parser {
    /// Feed `bytes`; the payloads of every message they complete.
    pub(crate) fn push(&mut self, bytes: &[u8]) -> Vec<Result<Value, Error>> {
        let mut frames = Vec::new();
        for &byte in bytes {
            if byte != b'\n' {
                self.line.push(byte);
                continue;
            }
            let mut line = std::mem::take(&mut self.line);
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            if line.is_empty() {
                if !self.data.is_empty() {
                    let data = std::mem::take(&mut self.data).join("\n");
                    let parsed = serde_json::from_str(&data);
                    frames.push(parsed.map_err(|error| Error::Decode(error.to_string())));
                }
                continue;
            }
            let line = String::from_utf8_lossy(&line);
            if let Some(data) = line.strip_prefix("data:") {
                self.data
                    .push(data.strip_prefix(' ').unwrap_or(data).to_owned());
            }
        }
        frames
    }
}
