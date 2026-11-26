//! Who may reach the server, how a request is bounded, and MCP framing.
#![allow(clippy::unwrap_used, clippy::expect_used)]
mod support;

use http_body_util::Full;
use hyper::body::Bytes;
use hyper::{Request, StatusCode};
use serde_json::json;
use support::{Hive, post, send};
use tinyhivemind_mcp::{PROTOCOL_VERSIONS, ServerOptions};

fn body(value: &serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(value).unwrap()
}
const LIST: &str = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#;

async fn hive() -> Hive {
    Hive::start(|services| services, |seat, _| seat).await
}

#[tokio::test]
async fn a_wrong_or_missing_bearer_is_refused() {
    let hive = hive().await;
    let addr = hive.server.local_addr();
    for bearer in [Some("not-a-seat"), None] {
        let request = post(addr, bearer, LIST.as_bytes())
            .body(Full::new(Bytes::from(LIST)))
            .unwrap();
        let reply = send(addr, request).await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
        assert_eq!(reply.headers["www-authenticate"], "Bearer");
    }
}

fn to_host(addr: std::net::SocketAddr, bearer: &str, hosts: &[&str]) -> Request<Full<Bytes>> {
    let mut builder = Request::post("/mcp").header("authorization", format!("Bearer {bearer}"));
    let _ = addr;
    for host in hosts {
        builder = builder.header("host", *host);
    }
    builder.body(Full::new(Bytes::from(LIST))).unwrap()
}

#[tokio::test]
async fn a_bad_host_is_refused() {
    let hive = hive().await;
    let addr = hive.server.local_addr();
    let own = format!("127.0.0.1:{}", addr.port());
    let named = format!("localhost:{}", addr.port());
    for hosts in [
        vec!["evil.example"],
        vec!["127.0.0.1:1"],
        vec!["localhost"],
        vec![own.as_str(), "evil.example"],
    ] {
        let reply = send(addr, to_host(addr, &hive.a.bearer, &hosts)).await;
        assert_eq!(reply.status, StatusCode::FORBIDDEN, "{hosts:?}");
    }
    for host in [own.as_str(), named.as_str()] {
        let reply = send(addr, to_host(addr, &hive.a.bearer, &[host])).await;
        assert_eq!(reply.status, StatusCode::OK, "{host}");
    }
}

#[tokio::test]
async fn any_origin_is_refused() {
    let hive = hive().await;
    let addr = hive.server.local_addr();
    let own = format!("http://127.0.0.1:{}", addr.port());
    for origin in ["https://evil.example", "null", own.as_str()] {
        let request = post(addr, Some(&hive.a.bearer), LIST.as_bytes())
            .header("origin", origin)
            .body(Full::new(Bytes::from(LIST)))
            .unwrap();
        assert_eq!(send(addr, request).await.status, StatusCode::FORBIDDEN);
    }
}

#[tokio::test]
async fn an_oversized_body_is_refused() {
    let options = ServerOptions {
        max_body_bytes: 64,
        ..ServerOptions::default()
    };
    let hive = Hive::start_with(|services| services, |seat, _| seat, options).await;
    let addr = hive.server.local_addr();
    let big = body(
        &json!({"jsonrpc": "2.0", "id": 1, "method": "ping", "params": {"pad": "x".repeat(100)}}),
    );
    let request = post(addr, Some(&hive.a.bearer), &big)
        .body(Full::new(Bytes::from(big.clone())))
        .unwrap();
    assert_eq!(
        send(addr, request).await.status,
        StatusCode::PAYLOAD_TOO_LARGE
    );
}

#[tokio::test]
async fn only_post_to_the_mcp_path_is_served_and_nothing_redirects() {
    let hive = hive().await;
    let addr = hive.server.local_addr();
    let host = format!("127.0.0.1:{}", addr.port());
    let auth = format!("Bearer {}", hive.a.bearer);
    for method in ["GET", "DELETE", "PUT"] {
        let request = Request::builder()
            .method(method)
            .uri("/mcp")
            .header("host", &host)
            .header("authorization", &auth)
            .body(Full::new(Bytes::new()))
            .unwrap();
        let reply = send(addr, request).await;
        assert_eq!(reply.status, StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(reply.headers["allow"], "POST");
    }
    for path in ["/", "/mcp/", "/other"] {
        let request = Request::post(path)
            .header("host", &host)
            .header("authorization", &auth)
            .body(Full::new(Bytes::from(LIST)))
            .unwrap();
        let reply = send(addr, request).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND);
        assert!(reply.headers.get("location").is_none());
    }
}

#[tokio::test]
async fn initialize_negotiates_every_version_the_harnesses_sent() {
    let hive = hive().await;
    // Codex 0.171 sent 2025-06-18; OpenHuman 0.64.15 and OpenClaw 2025-11-25.
    for version in ["2025-06-18", "2025-11-25", "2025-03-26"] {
        let response = hive
            .a
            .rpc(
                "initialize",
                json!({"protocolVersion": version, "capabilities": {}, "clientInfo": {"name": "t", "version": "0"}}),
            )
            .await;
        assert_eq!(response["result"]["protocolVersion"], version);
        assert_eq!(
            response["result"]["capabilities"],
            json!({"tools": {"listChanged": false}})
        );
        assert_eq!(response["result"]["serverInfo"]["name"], "hivemind");
    }
    let response = hive
        .a
        .rpc("initialize", json!({"protocolVersion": "1999-01-01"}))
        .await;
    assert_eq!(response["result"]["protocolVersion"], PROTOCOL_VERSIONS[0]);
}

#[tokio::test]
async fn an_unsupported_protocol_version_header_is_refused() {
    let hive = hive().await;
    let addr = hive.server.local_addr();
    let request = post(addr, Some(&hive.a.bearer), LIST.as_bytes())
        .header("mcp-protocol-version", "1999-01-01")
        .body(Full::new(Bytes::from(LIST)))
        .unwrap();
    assert_eq!(send(addr, request).await.status, StatusCode::BAD_REQUEST);
    let request = post(addr, Some(&hive.a.bearer), LIST.as_bytes())
        .header("mcp-protocol-version", "2025-06-18")
        .body(Full::new(Bytes::from(LIST)))
        .unwrap();
    assert_eq!(send(addr, request).await.status, StatusCode::OK);
}

#[tokio::test]
async fn notifications_and_responses_are_accepted_without_a_body() {
    let hive = hive().await;
    for message in [
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        json!({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": 1}}),
        json!({"jsonrpc": "2.0", "id": 7, "result": {}}),
    ] {
        let reply = hive.a.message(&message).await;
        assert_eq!(reply.status, StatusCode::ACCEPTED);
        assert!(reply.body.is_none());
    }
}

#[tokio::test]
async fn malformed_messages_get_json_rpc_errors() {
    let hive = hive().await;
    let addr = hive.server.local_addr();
    let request = post(addr, Some(&hive.a.bearer), b"{nope")
        .body(Full::new(Bytes::from_static(b"{nope")))
        .unwrap();
    let reply = send(addr, request).await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST);
    assert_eq!(reply.body.unwrap()["error"]["code"], -32700);
    for message in [
        json!([{"jsonrpc": "2.0", "id": 1, "method": "ping"}]),
        json!({"id": 1}),
    ] {
        let reply = hive.a.message(&message).await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST);
        assert_eq!(reply.body.unwrap()["error"]["code"], -32600);
    }
    let response = hive.a.rpc("sampling/createMessage", json!({})).await;
    assert_eq!(response["error"]["code"], -32601);
}

#[tokio::test]
async fn the_surface_is_tools_only() {
    let hive = hive().await;
    assert_eq!(hive.a.rpc("ping", json!({})).await["result"], json!({}));
    assert_eq!(
        hive.a.rpc("resources/list", json!({})).await["result"],
        json!({"resources": []})
    );
    assert_eq!(
        hive.a.rpc("resources/templates/list", json!({})).await["result"],
        json!({"resourceTemplates": []})
    );
    assert_eq!(
        hive.a.rpc("prompts/list", json!({})).await["result"],
        json!({"prompts": []})
    );
}

#[tokio::test]
async fn debug_output_never_carries_a_bearer() {
    let hive = hive().await;
    for printed in [format!("{:?}", hive.server), format!("{:?}", hive.endpoint)] {
        assert!(!printed.contains(&hive.a.bearer), "{printed}");
        assert!(!printed.contains(&hive.b.bearer), "{printed}");
    }
    assert!(hive.endpoint.url.starts_with("http://127.0.0.1:"));
    assert!(hive.endpoint.url.ends_with("/mcp"));
    assert_ne!(hive.a.bearer, hive.b.bearer);
    assert_eq!(hive.a.bearer.len(), 64);
}

#[tokio::test]
async fn idle_unauthenticated_sockets_cannot_block_a_seat() {
    let hive = hive().await;
    let addr = hive.server.local_addr();
    let mut idle = Vec::new();
    for _ in 0..40 {
        idle.push(tokio::net::TcpStream::connect(addr).await.unwrap());
    }
    let ping = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        hive.a.rpc("ping", json!({})),
    )
    .await
    .expect("a seat is served while idle sockets are open");
    assert_eq!(ping["result"], json!({}));
    drop(idle);
}

#[tokio::test]
async fn shutdown_closes_the_listener() {
    let hive = hive().await;
    let addr = hive.server.local_addr();
    assert_eq!(hive.a.rpc("ping", json!({})).await["result"], json!({}));
    hive.server.shutdown().await.unwrap();
    assert!(tokio::net::TcpStream::connect(addr).await.is_err());
}
