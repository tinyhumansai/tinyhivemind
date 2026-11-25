//! The loopback client against the fake surface, and the SSE parser alone.
use super::sse::Parser;
use super::{Client, Error};
use crate::fixture::{Fixture, PID};
use serde_json::json;

#[tokio::test]
async fn health_reports_the_process_that_answers() {
    let fixture = Fixture::new().await;
    let health = fixture.client.health().await;
    assert_eq!(health.ok().map(|health| health["pid"].clone()), Some(json!(PID)));
}

#[tokio::test]
async fn a_call_returns_its_result() {
    let fixture = Fixture::new().await;
    let version = fixture.client.call("core.version", json!({})).await;
    assert_eq!(version.ok(), Some(json!({"version": "0.64.15"})));
}

#[tokio::test]
async fn a_wrong_token_is_refused_by_status() {
    let fixture = Fixture::new().await;
    let client = Client::new(fixture.fake.port, "wrong");
    let client = client.unwrap_or_else(|error| unreachable!("{error}"));
    let refused = client.call("core.version", json!({})).await;
    assert!(
        matches!(refused, Err(Error::Status { status: 401, .. })),
        "{refused:?}"
    );
    let events = client.events("c").await.err();
    assert!(matches!(events, Some(Error::Status { status: 401, .. })));
}

#[tokio::test]
async fn an_rpc_error_carries_its_method_and_message() {
    let fixture = Fixture::new().await;
    let refused = fixture.client.call("openhuman.nope", json!({})).await;
    let Err(Error::Rpc { method, message }) = refused else {
        unreachable!("refused as an rpc error: {refused:?}")
    };
    assert_eq!(method, "openhuman.nope");
    assert_eq!(message, "unknown method: openhuman.nope");
}

#[tokio::test]
async fn nobody_listening_is_a_transport_error() {
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0");
        let listener = listener.unwrap_or_else(|error| unreachable!("{error}"));
        listener.local_addr().map(|address| address.port()).unwrap_or(1)
    };
    let client = Client::new(port, "t").unwrap_or_else(|error| unreachable!("{error}"));
    let refused = client.health().await;
    assert!(matches!(refused, Err(Error::Transport(_))), "{refused:?}");
}

#[tokio::test]
async fn events_are_those_of_the_subscribed_client_in_order() {
    let fixture = Fixture::new().await;
    fixture.script(&[json!({"reply": "hi", "tokens": 5})]);
    let mut mine = fixture.client.events("mine").await.unwrap_or_else(|error| unreachable!("{error}"));
    let params = json!({"client_id": "mine", "thread_id": "t", "message": "go"});
    let ack = fixture.client.call("openhuman.channel_web_chat", params).await;
    assert_eq!(ack.ok().map(|ack| ack["result"]["accepted"].clone()), Some(json!(true)));
    let mut names = Vec::new();
    while let Ok(Some(event)) = mine.next().await {
        let name = event["event"].as_str().unwrap_or_default().to_owned();
        assert_eq!(event["client_id"], "mine");
        names.push(name.clone());
        if name == "chat_done" {
            break;
        }
    }
    assert_eq!(names, ["iteration_start", "text_delta", "turn_cost", "chat_done"]);
}

#[tokio::test]
async fn a_stream_the_server_drops_ends() {
    let fixture = Fixture::new().await;
    let mut events = fixture.client.events("c").await.unwrap_or_else(|error| unreachable!("{error}"));
    drop(fixture);
    let ended = events.next().await;
    assert!(matches!(ended, Ok(None) | Err(Error::Transport(_))), "{ended:?}");
}

#[test]
fn sse_frames_split_anywhere_are_put_back_together() {
    let text = ": keep-alive\n\nevent: a\ndata: {\"n\":\n data: 1}\n\nevent: b\r\ndata: {\"n\":2}\r\n\r\n";
    for split in 0..text.len() {
        let mut parser = Parser::default();
        let mut found = parser.push(&text.as_bytes()[..split]);
        found.extend(parser.push(&text.as_bytes()[split..]));
        let found: Vec<_> = found.into_iter().map(|frame| frame.map_err(|error| error.to_string())).collect();
        assert_eq!(found, [Ok(json!({"n": 1})), Ok(json!({"n": 2}))], "split at {split}");
    }
}

#[test]
fn an_sse_frame_that_is_not_json_is_an_error() {
    let mut parser = Parser::default();
    let found = parser.push(b"data: nope\n\n");
    assert!(matches!(found.as_slice(), [Err(Error::Decode(_))]), "{found:?}");
}
