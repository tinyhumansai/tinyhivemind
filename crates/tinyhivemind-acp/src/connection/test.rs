//! The connection over a real child: `cat` echoes every line, so a call
//! comes back as a request from the "agent", and an answer to that comes
//! back as the call's response.
#![allow(clippy::unwrap_used, clippy::panic)] // Tests fail loudly on unexpected results.
use super::*;
use tinyhivemind_host::process::{Instance, InstanceDir, Launch, StopPolicy};

async fn echo() -> (Instance, Connection, tempfile::TempDir) {
    let root = tempfile::tempdir().unwrap();
    let dir = InstanceDir::create(root.path().join("cat")).unwrap();
    let mut instance = Instance::start(Launch::new("/bin/cat", dir).json_lines())
        .await
        .unwrap();
    let connection = Connection::new(instance.take_lines().unwrap());
    (instance, connection, root)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_request_nobody_routes_is_refused_at_once() {
    let (instance, connection, _root) = echo().await;
    let refused = connection.call("ping", json!({})).await;
    match refused {
        Err(Error::Refused {
            method,
            code,
            message,
        }) => {
            assert_eq!((method.as_str(), code), ("ping", NOT_OFFERED));
            assert!(message.contains("ping"));
        }
        other => panic!("{other:?}"),
    }
    instance
        .stop(async {}, StopPolicy::default())
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_routed_request_is_answered_by_its_taker() {
    let (instance, connection, _root) = echo().await;
    let mut incoming = connection.route();
    let call = connection.call("ask", json!({"q": 1}));
    tokio::pin!(call);
    let request = tokio::select! {
        message = incoming.recv() => message.unwrap(),
        _ = &mut call => panic!("answered before it was taken"),
    };
    let Incoming::Request { id, method, params } = request else {
        panic!("not a request")
    };
    assert_eq!((method.as_str(), &params), ("ask", &json!({"q": 1})));
    connection.respond(id, json!({"ok": true})).await.unwrap();
    assert_eq!(call.await.unwrap(), json!({"ok": true}));
    connection.notify("note", json!({"a": 1})).await.unwrap();
    let note = incoming.recv().await.unwrap();
    assert!(matches!(note, Incoming::Notification { ref method, .. } if method == "note"));
    connection.unroute();
    assert!(connection.call("again", json!(null)).await.is_err());
    instance
        .stop(async {}, StopPolicy::default())
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_refusal_from_the_client_reaches_the_caller() {
    let (instance, connection, _root) = echo().await;
    let mut incoming = connection.route();
    let call = connection.call("ask", json!({}));
    tokio::pin!(call);
    let request = tokio::select! {
        message = incoming.recv() => message.unwrap(),
        _ = &mut call => panic!("answered before it was taken"),
    };
    let Incoming::Request { id, .. } = request else {
        panic!("not a request")
    };
    connection.refuse(id, "no").await.unwrap();
    assert!(matches!(call.await, Err(Error::Refused { ref message, .. }) if message == "no"));
    instance
        .stop(async {}, StopPolicy::default())
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_call_fails_once_the_agent_is_gone() {
    let (instance, connection, _root) = echo().await;
    instance
        .stop(async {}, StopPolicy::default())
        .await
        .unwrap();
    assert!(connection.call("late", json!({})).await.is_err());
    assert!(Error::Closed.to_string().contains("closed"));
}
