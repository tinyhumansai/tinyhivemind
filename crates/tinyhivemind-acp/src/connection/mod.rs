//! JSON-RPC 2.0 with one agent process, over its supervised JSON lines.
//!
//! A reader task owns the receiving half. A response settles the call that
//! waits for it; a notification or a request from the agent goes to the
//! current route, which a running turn holds, and a request that arrives
//! with no turn to take it is refused at once, so the agent never waits on
//! an answer nobody will give.
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use tinyhivemind_host::process::{JsonLines, LineError, LineSender};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

/// The JSON-RPC code for a method this client does not offer.
const NOT_OFFERED: i64 = -32601;

/// Why a call failed.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    /// The agent stopped reading or writing.
    #[error("the agent closed the connection")]
    Closed,
    /// The agent answered with an error.
    #[error("the agent refused {method}: {message} ({code})")]
    Refused {
        method: String,
        code: i64,
        message: String,
    },
    /// A line could not be sent.
    #[error(transparent)]
    Line(#[from] LineError),
}

/// What the agent sends unasked.
#[derive(Debug)]
pub(crate) enum Incoming {
    /// A notification.
    Notification { method: String, params: Value },
    /// A request, to be answered with [`Connection::respond`].
    Request {
        id: Value,
        method: String,
        params: Value,
    },
}

type Waiting = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, Error>>>>>;
type Route = Arc<Mutex<Option<mpsc::UnboundedSender<Incoming>>>>;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// One agent's connection.
#[derive(Debug)]
pub(crate) struct Connection {
    sender: LineSender,
    waiting: Waiting,
    route: Route,
    /// Set once the reader ended, before it fails what is waiting.
    closed: Arc<AtomicBool>,
    next: AtomicU64,
    reader: JoinHandle<()>,
}

impl Connection {
    /// Speak over `lines`, reading on a task of the current runtime.
    pub(crate) fn new(lines: JsonLines) -> Self {
        let JsonLines {
            sender,
            mut receiver,
        } = lines;
        let waiting = Waiting::default();
        let route = Route::default();
        let closed = Arc::new(AtomicBool::new(false));
        let reader = tokio::spawn({
            let (sender, waiting, route) = (sender.clone(), waiting.clone(), route.clone());
            let closed = closed.clone();
            async move {
                while let Some(line) = receiver.recv().await {
                    // A malformed line is skipped; the stream goes on.
                    if let Ok(message) = line {
                        dispatch(message, &sender, &waiting, &route).await;
                    }
                }
                // Set first: a call that finds it unset is still drained here.
                closed.store(true, Ordering::SeqCst);
                for (_, call) in lock(&waiting).drain() {
                    let _ = call.send(Err(Error::Closed));
                }
                lock(&route).take();
            }
        });
        Self {
            sender,
            waiting,
            route,
            closed,
            next: AtomicU64::new(1),
            reader,
        }
    }

    /// Call `method` and wait for its result.
    pub(crate) async fn call(&self, method: &str, params: Value) -> Result<Value, Error> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (answer, answered) = oneshot::channel();
        lock(&self.waiting).insert(id, answer);
        if self.closed.load(Ordering::SeqCst) && lock(&self.waiting).remove(&id).is_some() {
            return Err(Error::Closed);
        }
        let request = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        if let Err(error) = self.sender.send(&request).await {
            lock(&self.waiting).remove(&id);
            return Err(error.into());
        }
        match answered.await {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(Error::Refused { code, message, .. })) => Err(Error::Refused {
                method: method.to_owned(),
                code,
                message,
            }),
            Ok(Err(error)) => Err(error),
            Err(_) => Err(Error::Closed),
        }
    }

    /// Send the notification `method`.
    pub(crate) async fn notify(&self, method: &str, params: Value) -> Result<(), Error> {
        let notification = json!({"jsonrpc": "2.0", "method": method, "params": params});
        Ok(self.sender.send(&notification).await?)
    }

    /// Answer the agent's request `id` with `result`.
    pub(crate) async fn respond(&self, id: Value, result: Value) -> Result<(), Error> {
        let response = json!({"jsonrpc": "2.0", "id": id, "result": result});
        Ok(self.sender.send(&response).await?)
    }

    /// Refuse the agent's request `id` with `message`.
    pub(crate) async fn refuse(&self, id: Value, message: &str) -> Result<(), Error> {
        let error = json!({"code": NOT_OFFERED, "message": message});
        let response = json!({"jsonrpc": "2.0", "id": id, "error": error});
        Ok(self.sender.send(&response).await?)
    }

    /// Answer the agent's request `id` from a task of the current runtime,
    /// with a result or a refusal, for a caller that cannot wait, such as a
    /// `Drop`. Without a runtime nothing is sent; the connection is then
    /// ending anyway.
    pub(crate) fn answer_detached(&self, id: &Value, answer: Result<Value, String>) {
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let response = match answer {
            Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
            Err(message) => {
                let error = json!({"code": NOT_OFFERED, "message": message});
                json!({"jsonrpc": "2.0", "id": id, "error": error})
            }
        };
        let sender = self.sender.clone();
        runtime.spawn(async move {
            let _ = sender.send(&response).await;
        });
    }

    /// Take what the agent sends unasked, until [`Self::unroute`] or another
    /// route replaces this one.
    pub(crate) fn route(&self) -> mpsc::UnboundedReceiver<Incoming> {
        let (sender, receiver) = mpsc::unbounded_channel();
        *lock(&self.route) = Some(sender);
        receiver
    }

    /// Stop routing; requests are refused again. Once this returns, nothing
    /// more reaches the route's receiver: the reader sends to it only while
    /// holding the route.
    pub(crate) fn unroute(&self) {
        lock(&self.route).take();
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.reader.abort();
    }
}

async fn dispatch(message: Value, sender: &LineSender, waiting: &Waiting, route: &Route) {
    let method = message["method"].as_str().map(str::to_owned);
    let id = message.get("id").cloned().filter(|id| !id.is_null());
    match (method, id) {
        (None, Some(id)) => {
            let Some(call) = id.as_u64().and_then(|id| lock(waiting).remove(&id)) else {
                return;
            };
            let outcome = match message.get("error") {
                Some(error) => Err(Error::Refused {
                    method: String::new(),
                    code: error["code"].as_i64().unwrap_or_default(),
                    message: error["message"].as_str().unwrap_or_default().to_owned(),
                }),
                None => Ok(message.get("result").cloned().unwrap_or(Value::Null)),
            };
            let _ = call.send(outcome);
        }
        (Some(method), id) => {
            let params = message.get("params").cloned().unwrap_or(Value::Null);
            let incoming = match id.clone() {
                Some(id) => Incoming::Request { id, method, params },
                None => Incoming::Notification { method, params },
            };
            let refused = match lock(route).as_ref() {
                Some(route) => route.send(incoming).err().map(|unsent| unsent.0),
                None => Some(incoming),
            };
            if let (Some(Incoming::Request { id, method, .. }), true) = (refused, id.is_some()) {
                let error =
                    json!({"code": NOT_OFFERED, "message": format!("{method} is not offered now")});
                let _ = sender
                    .send(&json!({"jsonrpc": "2.0", "id": id, "error": error}))
                    .await;
            }
        }
        (None, None) => {}
    }
}

#[cfg(test)]
mod test;
