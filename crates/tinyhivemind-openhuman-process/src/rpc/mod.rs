//! The instance's loopback API: JSON-RPC at `/rpc`, `/health`, and the
//! `/events` stream of one `client_id`.
//!
//! Every request carries the instance's core token as a bearer. The client
//! uses no proxy and no TLS: the URL is always `http://127.0.0.1:<port>`.
//! Neither the token nor a request body is ever put in an error.
mod sse;

use serde_json::{Value, json};
use std::collections::VecDeque;
use std::time::Duration;

/// How long one RPC may take. Turns are not RPCs: `channel_web_chat`
/// answers once the turn is accepted.
const CALL: Duration = Duration::from_secs(30);

/// Result of an API call.
pub(crate) type Result<T> = std::result::Result<T, Error>;

/// API failures.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    /// Nothing answered, or the connection broke.
    #[error("the instance could not be reached: {0}")]
    Transport(String),
    /// An answer other than 200.
    #[error("{what} answered with status {status}")]
    Status {
        /// The endpoint.
        what: &'static str,
        /// Its status.
        status: u16,
    },
    /// A JSON-RPC error.
    #[error("{method} failed: {message}")]
    Rpc {
        /// The method.
        method: String,
        /// The instance's message.
        message: String,
    },
    /// An answer that is not what the API says.
    #[error("the instance answered with something unreadable: {0}")]
    Decode(String),
}

impl From<reqwest::Error> for Error {
    fn from(error: reqwest::Error) -> Self {
        // Without the URL: it is loopback, and says nothing a reader needs.
        Self::Transport(error.without_url().to_string())
    }
}

/// A client of one instance.
#[derive(Clone, Debug)]
pub(crate) struct Client {
    http: reqwest::Client,
    base: String,
    token: String,
}

impl Client {
    /// The instance at `127.0.0.1:port`, which takes `token`.
    pub(crate) fn new(port: u16, token: &str) -> Result<Self> {
        let http = reqwest::Client::builder().no_proxy().build()?;
        Ok(Self {
            http,
            base: format!("http://127.0.0.1:{port}"),
            token: token.to_owned(),
        })
    }

    /// `GET /health`, which only a healthy instance answers with 200.
    pub(crate) async fn health(&self) -> Result<Value> {
        let url = format!("{}/health", self.base);
        let response = self.http.get(url).timeout(CALL).send().await?;
        let status = response.status().as_u16();
        if status != 200 {
            return Err(Error::Status {
                what: "/health",
                status,
            });
        }
        decode(&response.bytes().await?)
    }

    /// Call `method` with `params`; its `result`.
    pub(crate) async fn call(&self, method: &str, params: Value) -> Result<Value> {
        let body = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
        let response = self
            .http
            .post(format!("{}/rpc", self.base))
            .bearer_auth(&self.token)
            .header("content-type", "application/json")
            .body(body.to_string())
            .timeout(CALL)
            .send()
            .await?;
        let status = response.status().as_u16();
        if status != 200 {
            return Err(Error::Status {
                what: "/rpc",
                status,
            });
        }
        let mut answer = decode(&response.bytes().await?)?;
        if let Some(error) = answer.get("error").filter(|error| !error.is_null()) {
            let message = error["message"].as_str().unwrap_or("no message");
            return Err(Error::Rpc {
                method: method.to_owned(),
                message: message.to_owned(),
            });
        }
        Ok(answer["result"].take())
    }

    /// Subscribe to the events of `client`. They are published to whoever
    /// is subscribed when they happen, so this must come first.
    pub(crate) async fn events(&self, client: &str) -> Result<Events> {
        let response = self
            .http
            .get(format!("{}/events", self.base))
            .query(&[("client_id", client)])
            .bearer_auth(&self.token)
            .send()
            .await?;
        let status = response.status().as_u16();
        if status != 200 {
            return Err(Error::Status {
                what: "/events",
                status,
            });
        }
        Ok(Events {
            response,
            parser: sse::Parser::default(),
            ready: VecDeque::new(),
        })
    }
}

fn decode(bytes: &[u8]) -> Result<Value> {
    serde_json::from_slice(bytes).map_err(|error| Error::Decode(error.to_string()))
}

/// One client's event stream. Dropping it closes the connection.
#[derive(Debug)]
pub(crate) struct Events {
    response: reqwest::Response,
    parser: sse::Parser,
    ready: VecDeque<Result<Value>>,
}

impl Events {
    /// The next event's payload, or `None` once the stream has ended.
    pub(crate) async fn next(&mut self) -> Result<Option<Value>> {
        loop {
            if let Some(ready) = self.ready.pop_front() {
                return ready.map(Some);
            }
            match self.response.chunk().await? {
                Some(chunk) => self.ready.extend(self.parser.push(&chunk)),
                None => return Ok(None),
            }
        }
    }
}

#[cfg(test)]
mod test;
