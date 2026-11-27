//! The `OpenClaw` descriptor, its Gateway's config, the readiness probe,
//! and a Gateway that never listens.
#![allow(clippy::unwrap_used, clippy::panic)] // Tests fail loudly on unexpected results.
use super::*;

fn seat(id: &str) -> GatewaySeat {
    let tools = ToolEndpoint::new(
        "hive",
        "http://127.0.0.1:7000/mcp",
        format!("bearer-{id}"),
        ["hivemind_post".to_owned()],
    );
    GatewaySeat {
        id: id.into(),
        tools,
    }
}

fn plan(directory: PathBuf, seats: &[&str]) -> GatewayPlan {
    GatewayPlan {
        node: "/opt/node/bin/node".into(),
        openclaw: "/opt/openclaw/openclaw.mjs".into(),
        directory,
        port: 19_803,
        token: "gateway-token".into(),
        endpoint: "http://127.0.0.1:9000/v1".into(),
        model: "mock".into(),
        seats: seats.iter().map(|id| seat(id)).collect(),
    }
}

#[test]
fn each_seat_is_an_agent_allowed_only_its_own_server_and_bearer() {
    let plan = plan("/g".into(), &["a", "b"]);
    let config = plan.config();
    for id in ["a", "b"] {
        let agent = &config["agents"]["entries"][id];
        assert_eq!(agent["tools"]["allow"], json!([format!("hive_{id}__*")]));
        assert_eq!(agent["tools"]["deny"], json!(DENY));
        assert_eq!(agent["workspace"], format!("/g/state/workspaces/{id}"));
        let server = &config["mcp"]["servers"][format!("hive_{id}")];
        assert_eq!(server["url"], "http://127.0.0.1:7000/mcp");
        assert_eq!(
            server["headers"]["Authorization"],
            format!("Bearer bearer-{id}")
        );
    }
    assert_eq!(config["agents"]["ownership"], "explicit");
    assert_eq!(config["agents"]["defaults"]["model"]["primary"], "thm/mock");
    let provider = &config["models"]["providers"]["thm"];
    assert_eq!(provider["baseUrl"], "http://127.0.0.1:9000/v1");
    assert_eq!(provider["apiKey"], NO_KEY);
    assert_eq!(config["gateway"]["port"], 19_803);
    assert_eq!(config["gateway"]["auth"]["token"], "gateway-token");
    assert_eq!(config["tools"]["toolSearch"], false);
    assert_eq!(config["tools"]["codeMode"], false);
}

#[test]
fn a_single_seat_gateway_leaves_ownership_unset() {
    let config = plan("/g".into(), &["a"]).config();
    assert!(config["agents"].get("ownership").is_none());
}

#[test]
fn the_plan_hides_its_token_and_names_where_bridges_find_it() {
    let plan = plan("/g".into(), &["a"]);
    assert_eq!(plan.url(), "ws://127.0.0.1:19803");
    assert_eq!(plan.token_file(), Path::new("/g/state/token"));
    let shown = format!("{plan:?}");
    assert!(!shown.contains("gateway-token") && !shown.contains("bearer-a"));
}

#[test]
fn the_bridge_opens_its_seats_gateway_session_with_no_servers_of_its_own() {
    let plan = plan("/g".into(), &["a"]);
    let agent = Agent::openclaw(
        "/opt/node/bin/node",
        "/opt/openclaw/openclaw.mjs",
        "2026.10.33",
        &plan,
    );
    assert_eq!(agent.kind, HarnessKind::OpenClaw);
    assert!(!agent.session_mcp);
    assert_eq!(
        agent.args,
        [
            "/opt/openclaw/openclaw.mjs",
            "acp",
            "--url",
            "ws://127.0.0.1:19803",
            "--token-file",
            "/g/state/token",
            "--session",
            "agent:{seat}:acp",
            "--no-prefix-cwd",
        ]
    );
    assert!(
        !agent
            .args
            .iter()
            .any(|argument| argument.contains("gateway-token"))
    );
    let env: BTreeMap<_, _> = agent.env.iter().cloned().collect();
    assert_eq!(env["OPENCLAW_STATE_DIR"], "{state}/openclaw");
    assert_eq!(env["OPENCLAW_SKIP_CHANNELS"], "1");
    assert_eq!(agent.tool_name, "{server}_{seat}__{tool}");
}

#[tokio::test]
async fn a_gateway_that_exits_before_it_listens_fails_to_start() {
    let root = tempfile::tempdir().unwrap();
    let mut plan = plan(root.path().join("gateway"), &["a"]);
    plan.node = "/bin/true".into();
    let error = Gateway::start(&plan, Duration::from_secs(10))
        .await
        .unwrap_err();
    assert!(matches!(error, harness::Error::Process(_)), "{error}");
    // The config and token were written before the start.
    let state = root.path().join("gateway/state");
    let written: Value =
        serde_json::from_str(&std::fs::read_to_string(state.join("openclaw.json")).unwrap())
            .unwrap();
    assert_eq!(written, plan.config());
    assert_eq!(
        std::fs::read_to_string(plan.token_file()).unwrap(),
        "gateway-token"
    );
    assert!(state.join("workspaces/a").is_dir());
}

#[tokio::test]
async fn a_gateway_in_a_relative_directory_is_refused() {
    let plan = plan("relative".into(), &["a"]);
    let error = Gateway::start(&plan, Duration::from_secs(1))
        .await
        .unwrap_err();
    assert!(matches!(error, harness::Error::Process(_)), "{error}");
}

/// A server on loopback that answers one request with `status`, and its port.
async fn answering(status: &'static str) -> u16 {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0_u8; 256];
        let _ = stream.read(&mut request).await;
        let response = format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\n\r\n");
        stream.write_all(response.as_bytes()).await.unwrap();
    });
    port
}

#[tokio::test]
async fn a_gateway_is_ready_only_when_readyz_answers_200() {
    assert!(is_ready(answering("200 OK").await).await);
    assert!(!is_ready(answering("503 Service Unavailable").await).await);
    let closed = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = closed.local_addr().unwrap().port();
    drop(closed);
    assert!(!is_ready(port).await);
}
