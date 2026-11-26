//! JSON-RPC 2.0 and the MCP methods a tools-only server answers.
//!
//! One message per POST; a batch is refused. A notification or a response
//! from the client is accepted with no body. No session id is issued: the
//! bearer already names the seat, and none of the three harnesses needed one.
use super::{PROTOCOL_VERSIONS, Shared};
use crate::tools::{self, SeatState};
use hyper::StatusCode;
use serde_json::{Value, json};
use std::sync::Arc;

/// What a POST is answered with.
#[derive(Debug)]
pub(super) enum Answer {
    /// 202, no body.
    Accepted,
    /// A JSON-RPC message with this status.
    Reply(StatusCode, Value),
}

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
const INTERNAL_ERROR: i64 = -32603;

/// Answer the JSON-RPC message in `body` as `seat`.
pub(super) async fn answer(shared: &Arc<Shared>, seat: Arc<SeatState>, body: &[u8]) -> Answer {
    let Ok(message) = serde_json::from_slice::<Value>(body) else {
        return Answer::Reply(
            StatusCode::BAD_REQUEST,
            error(&Value::Null, PARSE_ERROR, "parse error"),
        );
    };
    if message.is_array() {
        let refused = error(&Value::Null, INVALID_REQUEST, "batches are not supported");
        return Answer::Reply(StatusCode::BAD_REQUEST, refused);
    }
    let id = message.get("id").cloned();
    let replied = message.get("result").is_some() || message.get("error").is_some();
    let invalid = |id: Option<&Value>| {
        let id = id.unwrap_or(&Value::Null);
        Answer::Reply(
            StatusCode::BAD_REQUEST,
            error(id, INVALID_REQUEST, "invalid request"),
        )
    };
    if message.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return invalid(id.as_ref());
    }
    match (message.get("method").and_then(Value::as_str), id) {
        (Some(method), Some(id)) => {
            let params = message.get("params").cloned().unwrap_or(Value::Null);
            let reply = match request(shared, seat, method, params).await {
                Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
                Err((code, text)) => error(&id, code, &text),
            };
            Answer::Reply(StatusCode::OK, reply)
        }
        // A notification, or the client's answer to a request: nothing to say.
        (Some(_), None) => Answer::Accepted,
        (None, Some(_)) if replied => Answer::Accepted,
        (None, id) => invalid(id.as_ref()),
    }
}

async fn request(
    shared: &Arc<Shared>,
    seat: Arc<SeatState>,
    method: &str,
    params: Value,
) -> Result<Value, (i64, String)> {
    Ok(match method {
        "initialize" => {
            let asked = params["protocolVersion"].as_str();
            let version = PROTOCOL_VERSIONS
                .iter()
                .find(|version| Some(**version) == asked)
                .unwrap_or(&PROTOCOL_VERSIONS[0]);
            json!({
                "protocolVersion": version,
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": shared.options.name, "version": env!("CARGO_PKG_VERSION")},
            })
        }
        "ping" => json!({}),
        "tools/list" => json!({"tools": seat.catalog.listing()}),
        "tools/call" => return call(shared, seat, params).await,
        "resources/list" => json!({"resources": []}),
        "resources/templates/list" => json!({"resourceTemplates": []}),
        "prompts/list" => json!({"prompts": []}),
        other => return Err((METHOD_NOT_FOUND, format!("method {other} is not served"))),
    })
}

/// Run a `tools/call` on its own task, so that a client hanging up does not
/// cancel a call halfway and leave its outcome unknown.
async fn call(
    shared: &Arc<Shared>,
    seat: Arc<SeatState>,
    params: Value,
) -> Result<Value, (i64, String)> {
    let Some(name) = params["name"].as_str().map(str::to_owned) else {
        return Err((INVALID_PARAMS, "tools/call needs a tool name".into()));
    };
    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
    // The seat's own slot first, so a seat at its bound holds no shared one.
    // Both are held by the call's task, so they are released when the call
    // ends, not when its client hangs up.
    let stopping = || (INTERNAL_ERROR, "the server is stopping".to_owned());
    let own = seat
        .calls
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| stopping())?;
    let shared_slot = shared
        .calls
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| stopping())?;
    let shared = shared.clone();
    let task = tokio::spawn(async move {
        let _slots = (own, shared_slot);
        tools::call(&shared.services, &shared.ledger, &seat, &name, arguments).await
    });
    match task.await {
        Ok(Ok(outcome)) => Ok(json!({
            "content": [{"type": "text", "text": outcome.text}],
            "isError": outcome.is_error,
        })),
        Ok(Err(unknown)) => Err((INVALID_PARAMS, format!("unknown tool {}", unknown.0))),
        Err(_) => Err((INTERNAL_ERROR, "the call failed".into())),
    }
}

fn error(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}
