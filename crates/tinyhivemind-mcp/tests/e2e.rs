//! A harness's whole MCP session, as the three harnesses ran it in the spike.
#![allow(clippy::unwrap_used, clippy::expect_used)]
mod support;

use http_body_util::Full;
use hyper::body::Bytes;
use hyper::{Request, StatusCode};
use serde_json::json;
use support::{Hive, send};

#[tokio::test]
async fn a_client_lists_tools_and_posts_as_seat_a_and_is_refused_as_seat_b() {
    let mut hive = Hive::start(|services| services, |seat, _| seat).await;
    let addr = hive.server.local_addr();
    for client in [&hive.a, &hive.b] {
        let init = client
            .rpc("initialize", json!({"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "e2e", "version": "0"}}))
            .await;
        assert_eq!(init["result"]["protocolVersion"], "2025-11-25");
        let note = client
            .message(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
            .await;
        assert_eq!(note.status, StatusCode::ACCEPTED);
        // OpenClaw opens the optional event stream; it is not offered.
        let stream = Request::get("/mcp")
            .header("host", format!("127.0.0.1:{}", addr.port()))
            .header("authorization", format!("Bearer {}", client.bearer))
            .header("accept", "text/event-stream")
            .body(Full::new(Bytes::new()))
            .unwrap();
        assert_eq!(
            send(addr, stream).await.status,
            StatusCode::METHOD_NOT_ALLOWED
        );
        let names: Vec<String> = client
            .tools()
            .await
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        assert!(names.contains(&"hivemind_post".to_owned()));
    }
    let args = json!({"episode_id": hive.episode(), "body": "hello hive", "turn": hive.turn_id(), "call": "post-1"});
    assert_eq!(
        hive.a.call("hivemind_post", args.clone()).await,
        (r#"{"accepted":true}"#.into(), false)
    );
    let (text, refused) = hive.b.call("hivemind_post", args).await;
    assert!(refused, "{text}");
    hive.settle().await;
    assert_eq!(hive.written("a", "hello hive"), 1);
    assert_eq!(hive.written("b", "hello hive"), 0);
}
