//! A scripted Chat Completions model on loopback, played by the host's
//! conformance steps: each request takes the next step, and a hanging one
//! notices when the caller hangs up.
#[allow(dead_code, reason = "shared with the fake surface")]
#[path = "../support/http.rs"]
mod http;

use http::{Reply, Request, serve};
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use tinyhivemind_host::harness::conformance::{CallState, ModelCall, ModelStep};
use tokio::net::TcpListener;
use tokio::sync::mpsc;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Default)]
struct State {
    /// Requests to refuse with a 400 before the script plays on.
    failures: Mutex<usize>,
    steps: Mutex<VecDeque<ModelStep>>,
    calls: Mutex<Vec<ModelCall>>,
}

/// The model, serving until dropped.
pub(crate) struct ScriptedModel {
    pub(crate) port: u16,
    state: Arc<State>,
    task: tokio::task::JoinHandle<()>,
}

impl ScriptedModel {
    pub(crate) async fn start() -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        let state = Arc::new(State::default());
        let serving = state.clone();
        let task = tokio::spawn(serve(listener, move |request| {
            let state = serving.clone();
            async move { answer(&state, &request) }
        }));
        Ok(Self { port, state, task })
    }

    pub(crate) fn script(&self, steps: &[ModelStep]) {
        *lock(&self.state.steps) = steps.iter().cloned().collect();
    }

    /// Refuse the next request, as a provider refuses a bad one.
    pub(crate) fn fail_next(&self) {
        *lock(&self.state.failures) += 1;
    }

    pub(crate) fn calls(&self) -> Vec<ModelCall> {
        lock(&self.state.calls).clone()
    }
}

impl Drop for ScriptedModel {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn answer(state: &Arc<State>, request: &Request) -> Reply {
    if request.method == "GET" && request.path.ends_with("/models") {
        return Reply::Json(
            200,
            json!({"object": "list", "data": [{"id": "mock", "object": "model"}]}),
        );
    }
    if !request.path.ends_with("/chat/completions") {
        return Reply::Json(404, json!({}));
    }
    {
        let mut failures = lock(&state.failures);
        if *failures > 0 {
            *failures -= 1;
            let error =
                json!({"error": {"message": "scripted refusal", "type": "invalid_request_error"}});
            return Reply::Json(400, error);
        }
    }
    let body = request.json();
    let messages = body["messages"].as_array().cloned().unwrap_or_default();
    let seen = messages
        .iter()
        .filter(|message| message["role"] == "user")
        .count();
    let index = {
        let mut calls = lock(&state.calls);
        calls.push(ModelCall {
            seen,
            state: CallState::Open,
        });
        calls.len() - 1
    };
    let step = lock(&state.steps).pop_front().unwrap_or(ModelStep::Hang);
    let (sender, receiver) = mpsc::channel(16);
    let state = state.clone();
    let stream = body["stream"] == true;
    match step {
        ModelStep::Hang => {
            tokio::spawn(async move {
                sender.closed().await;
                lock(&state.calls)[index].state = CallState::Aborted;
            });
            Reply::Stream(receiver)
        }
        ModelStep::Reply { text, usage } => {
            lock(&state.calls)[index].state = CallState::Answered;
            let usage = json!({"prompt_tokens": usage.input_tokens, "completion_tokens": 1,
                "total_tokens": usage.input_tokens + 1});
            let base = json!({"id": "c", "created": 1, "model": "mock"});
            if !stream {
                let mut reply = base;
                reply["object"] = json!("chat.completion");
                reply["choices"] = json!([{"index": 0, "finish_reason": "stop",
                    "message": {"role": "assistant", "content": text}}]);
                reply["usage"] = usage;
                return Reply::Json(200, reply);
            }
            let chunk = |choices: Value, usage: Option<Value>| {
                let mut chunk = base.clone();
                chunk["object"] = json!("chat.completion.chunk");
                chunk["choices"] = choices;
                if let Some(usage) = usage {
                    chunk["usage"] = usage;
                }
                format!("data: {chunk}\n\n")
            };
            let frames = [
                chunk(
                    json!([{"index": 0, "delta": {"role": "assistant", "content": text}, "finish_reason": null}]),
                    None,
                ),
                chunk(
                    json!([{"index": 0, "delta": {}, "finish_reason": "stop"}]),
                    None,
                ),
                chunk(json!([]), Some(usage)),
                "data: [DONE]\n\n".to_owned(),
            ];
            tokio::spawn(async move {
                for frame in frames {
                    let _ = sender.send(frame).await;
                }
            });
            Reply::Stream(receiver)
        }
    }
}
