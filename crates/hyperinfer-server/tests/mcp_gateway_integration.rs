//! Integration tests for the Multiplexing MCP Gateway (HYP-66).

use std::collections::HashMap;
use std::sync::Arc;

use axum::{extract::State, http::HeaderMap, middleware, routing::post, Json, Router};
use axum_test::TestServer;
use hyperinfer_server::mcp::{
    create_jwt_with_team, jwt_auth_middleware, mcp_http_handler, McpState,
};
use hyperinfer_server::mcp_gateway::{McpTeamPolicy, UpstreamMcpServer, UpstreamTransport};
use serde_json::{json, Value};
use tokio::sync::RwLock;

#[derive(Clone)]
struct MockUpstream {
    name: String,
    tools: Vec<Value>,
    seen_headers: Arc<RwLock<Vec<HashMap<String, String>>>>,
}

async fn mock_mcp_upstream(
    State(state): State<MockUpstream>,
    headers: HeaderMap,
    Json(req): Json<Value>,
) -> Json<Value> {
    let mut map = HashMap::new();
    for (k, v) in headers.iter() {
        if let Ok(val) = v.to_str() {
            map.insert(k.as_str().to_lowercase(), val.to_string());
        }
    }
    state.seen_headers.write().await.push(map);

    let id = req.get("id").cloned().unwrap_or(json!("1"));
    match req.get("method").and_then(|m| m.as_str()) {
        Some("tools/list") => Json(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": { "tools": state.tools }
        })),
        Some("tools/call") => {
            let tool_name = req
                .pointer("/params/name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            Json(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "content": [{ "type": "text", "text": format!("{}:{}", state.name, tool_name) }],
                    "isError": false
                }
            }))
        }
        _ => Json(json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32601, "message": "Method not found" }
        })),
    }
}

async fn spawn_mock_upstream(
    name: &str,
    tools: Vec<Value>,
) -> (String, Arc<RwLock<Vec<HashMap<String, String>>>>) {
    let seen = Arc::new(RwLock::new(Vec::new()));
    let state = MockUpstream {
        name: name.to_string(),
        tools,
        seen_headers: seen.clone(),
    };
    let app = Router::new()
        .route("/", post(mock_mcp_upstream))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{addr}"), seen)
}

fn build_mcp_server(mcp_state: McpState) -> TestServer {
    let app = Router::new()
        .route("/mcp", post(mcp_http_handler))
        .layer(middleware::from_fn_with_state(
            mcp_state.clone(),
            jwt_auth_middleware,
        ))
        .with_state(mcp_state);
    TestServer::new(app)
}

async fn setup_gateway() -> (McpState, Arc<RwLock<Vec<HashMap<String, String>>>>) {
    let (gh_url, gh_headers) = spawn_mock_upstream(
        "github",
        vec![
            json!({"name": "list_issues", "description": "List repo issues", "inputSchema": {"type": "object"}}),
            json!({"name": "delete_repo", "description": "Delete a repo", "inputSchema": {"type": "object"}}),
        ],
    )
    .await;
    let (lin_url, _) = spawn_mock_upstream(
        "linear",
        vec![json!({"name": "get_issue", "description": "Fetch a Linear issue", "inputSchema": {"type": "object"}})],
    )
    .await;

    let state = McpState::new("test-secret");
    state
        .gateway
        .register_upstream(UpstreamMcpServer {
            name: "github".to_string(),
            transport: UpstreamTransport::StreamableHttp { url: gh_url },
            headers: HashMap::from([(
                "x-upstream-secret".to_string(),
                "gh-secret-token".to_string(),
            )]),
            enabled: true,
        })
        .await;
    state
        .gateway
        .register_upstream(UpstreamMcpServer {
            name: "linear".to_string(),
            transport: UpstreamTransport::StreamableHttp { url: lin_url },
            headers: HashMap::new(),
            enabled: true,
        })
        .await;
    (state, gh_headers)
}

#[tokio::test]
async fn test_mcp_multiplexing_and_secret_injection() {
    let (state, gh_headers) = setup_gateway().await;
    let server = build_mcp_server(state);
    let token = create_jwt_with_team("agent-1", "team-open", "test-secret");

    let list_res: Value = server
        .post("/mcp")
        .add_header("Authorization", format!("Bearer {token}"))
        .json(&json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}}))
        .await
        .json();
    let names: Vec<&str> = list_res["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        vec![
            "github__delete_repo",
            "github__list_issues",
            "linear__get_issue"
        ]
    );

    let call_res: Value = server
        .post("/mcp")
        .add_header("Authorization", format!("Bearer {token}"))
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": { "name": "github__list_issues", "arguments": {} }
        }))
        .await
        .json();
    assert_eq!(
        call_res["result"]["content"][0]["text"],
        "github:list_issues"
    );

    let seen = gh_headers.read().await;
    assert!(seen
        .iter()
        .all(|h| h.get("x-upstream-secret").map(String::as_str) == Some("gh-secret-token")));
}

#[tokio::test]
async fn test_mcp_team_policy_filtering_and_rbac() {
    let (state, _) = setup_gateway().await;
    state
        .gateway
        .set_team_policy(
            "team-restricted",
            McpTeamPolicy {
                allow_patterns: vec!["^github__.*".to_string()],
                deny_patterns: vec![".*delete.*".to_string()],
                catalog_mode: false,
            },
        )
        .await;
    let server = build_mcp_server(state);
    let token = create_jwt_with_team("agent-2", "team-restricted", "test-secret");

    let list_res: Value = server
        .post("/mcp")
        .add_header("Authorization", format!("Bearer {token}"))
        .json(&json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}}))
        .await
        .json();
    let names: Vec<&str> = list_res["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["github__list_issues"]);

    let denied_delete: Value = server
        .post("/mcp")
        .add_header("Authorization", format!("Bearer {token}"))
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": { "name": "github__delete_repo", "arguments": {} }
        }))
        .await
        .json();
    assert_eq!(denied_delete["error"]["code"], -32603);

    let denied_linear: Value = server
        .post("/mcp")
        .add_header("Authorization", format!("Bearer {token}"))
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": { "name": "linear__get_issue", "arguments": {} }
        }))
        .await
        .json();
    assert_eq!(denied_linear["error"]["code"], -32603);
}

#[tokio::test]
async fn test_mcp_catalog_mode_search_and_call() {
    let (state, _) = setup_gateway().await;
    state
        .gateway
        .set_team_policy(
            "team-catalog",
            McpTeamPolicy {
                allow_patterns: vec![],
                deny_patterns: vec![".*delete.*".to_string()],
                catalog_mode: true,
            },
        )
        .await;
    let server = build_mcp_server(state);
    let token = create_jwt_with_team("agent-3", "team-catalog", "test-secret");

    let list_res: Value = server
        .post("/mcp")
        .add_header("Authorization", format!("Bearer {token}"))
        .json(&json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}}))
        .await
        .json();
    let names: Vec<&str> = list_res["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["mcp_search_tools", "mcp_call_tool"]);

    let search_res: Value = server
        .post("/mcp")
        .add_header("Authorization", format!("Bearer {token}"))
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": { "name": "mcp_search_tools", "arguments": { "query": "issue" } }
        }))
        .await
        .json();
    let search_text = search_res["result"]["content"][0]["text"].as_str().unwrap();
    assert!(search_text.contains("github__list_issues"));
    assert!(search_text.contains("linear__get_issue"));
    assert!(!search_text.contains("delete_repo"));

    let call_ok: Value = server
        .post("/mcp")
        .add_header("Authorization", format!("Bearer {token}"))
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": { "name": "mcp_call_tool", "arguments": { "name": "github__list_issues", "arguments": {} } }
        }))
        .await
        .json();
    assert_eq!(
        call_ok["result"]["content"][0]["text"],
        "github:list_issues"
    );

    let call_denied: Value = server
        .post("/mcp")
        .add_header("Authorization", format!("Bearer {token}"))
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": { "name": "mcp_call_tool", "arguments": { "name": "github__delete_repo", "arguments": {} } }
        }))
        .await
        .json();
    assert_eq!(call_denied["error"]["code"], -32603);
}

#[tokio::test]
async fn test_mcp_double_underscore_name_disabled_and_sse_error() {
    let (prod_url, _) = spawn_mock_upstream(
        "my_org__prod",
        vec![
            json!({"name": "ping", "description": "Ping prod", "inputSchema": {"type": "object"}}),
        ],
    )
    .await;
    let (disabled_url, _) = spawn_mock_upstream(
        "disabled_srv",
        vec![json!({"name": "noop", "description": "Noop", "inputSchema": {"type": "object"}})],
    )
    .await;

    let state = McpState::new("test-secret");
    state
        .gateway
        .register_upstream(UpstreamMcpServer {
            name: "my_org__prod".to_string(),
            transport: UpstreamTransport::StreamableHttp { url: prod_url },
            headers: HashMap::new(),
            enabled: true,
        })
        .await;
    state
        .gateway
        .register_upstream(UpstreamMcpServer {
            name: "disabled_srv".to_string(),
            transport: UpstreamTransport::StreamableHttp { url: disabled_url },
            headers: HashMap::new(),
            enabled: false,
        })
        .await;
    state
        .gateway
        .register_upstream(UpstreamMcpServer {
            name: "sse_srv".to_string(),
            transport: UpstreamTransport::Sse {
                url: "http://127.0.0.1:1/sse".to_string(),
            },
            headers: HashMap::new(),
            enabled: true,
        })
        .await;

    let server = build_mcp_server(state);
    let token = create_jwt_with_team("agent-4", "team-edge", "test-secret");

    // tools/list includes my_org__prod__ping, excludes disabled_srv and sse_srv
    let list_res: Value = server
        .post("/mcp")
        .add_header("Authorization", format!("Bearer {token}"))
        .json(&json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}}))
        .await
        .json();
    let names: Vec<&str> = list_res["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["my_org__prod__ping"]);

    // Calling my_org__prod__ping resolves longest prefix cleanly
    let call_ok: Value = server
        .post("/mcp")
        .add_header("Authorization", format!("Bearer {token}"))
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": { "name": "my_org__prod__ping", "arguments": {} }
        }))
        .await
        .json();
    assert_eq!(call_ok["result"]["content"][0]["text"], "my_org__prod:ping");

    // Calling disabled_srv__noop returns an error
    let call_disabled: Value = server
        .post("/mcp")
        .add_header("Authorization", format!("Bearer {token}"))
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": { "name": "disabled_srv__noop", "arguments": {} }
        }))
        .await
        .json();
    assert!(call_disabled["error"]["message"]
        .as_str()
        .unwrap()
        .contains("disabled"));

    // Calling sse_srv__anything returns explicit unsupported SSE error
    let call_sse: Value = server
        .post("/mcp")
        .add_header("Authorization", format!("Bearer {token}"))
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": { "name": "sse_srv__anything", "arguments": {} }
        }))
        .await
        .json();
    assert!(call_sse["error"]["message"]
        .as_str()
        .unwrap()
        .contains("SSE upstream transport"));
}
