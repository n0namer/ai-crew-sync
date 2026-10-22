#![allow(clippy::too_many_lines)]

#[path = "task_pagination_fixture.rs"]
mod fixture;

use ai_crew_sync::serve::{self, ServeOptions};
use fixture::{Fixture, cleanup, setup_fixture};
use reqwest::{Client, Response};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

const MCP_PROTOCOL_VERSION: &str = "2025-06-18";

#[derive(Debug)]
struct WireExchange {
    request_body: String,
    status: u16,
    content_type: Option<String>,
    response_body: String,
}

impl WireExchange {
    fn receipt(&self) -> Value {
        json!({
            "request": {
                "method": "POST",
                "path": "/mcp",
                "headers": [
                    "accept: application/json, text/event-stream",
                    "content-type: application/json",
                    "mcp-protocol-version: 2025-06-18",
                    "authorization: Bearer <redacted>",
                    "session-header: <redacted>"
                ],
                "body": self.request_body,
            },
            "response": {
                "status": self.status,
                "content_type": self.content_type,
                "body": self.response_body,
            },
        })
    }
}

async fn post_json(
    client: &Client,
    url: &str,
    token: &str,
    body: &Value,
) -> Result<WireExchange, Box<dyn std::error::Error>> {
    let request_body = serde_json::to_string(body)?;
    let response: Response = client
        .post(url)
        .header(
            reqwest::header::ACCEPT,
            "application/json, text/event-stream",
        )
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header("MCP-Protocol-Version", MCP_PROTOCOL_VERSION)
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {token}"),
        )
        .header(ai_crew_sync::auth::SESSION_HEADER, "task-search-mcp-wire")
        .body(request_body.clone())
        .send()
        .await?;
    let status = response.status().as_u16();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let response_body = response.text().await?;
    assert!(
        !response_body.contains(token),
        "raw MCP response must not echo the bearer token"
    );
    Ok(WireExchange {
        request_body,
        status,
        content_type,
        response_body,
    })
}

fn response_json(exchange: &WireExchange) -> Result<Value, Box<dyn std::error::Error>> {
    Ok(serde_json::from_str(&exchange.response_body)?)
}

fn assert_json_rpc_result(response: &Value, id: i64) {
    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], id);
    assert!(response.get("error").is_none(), "unexpected JSON-RPC error: {response}");
    assert!(response.get("result").is_some(), "missing JSON-RPC result: {response}");
}

async fn seed_wire_fixture(fixture: &Fixture) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO tasks (team_id, key, title, description, created_by) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(fixture.team_id)
    .bind(format!("{}:wire", fixture.task_prefix))
    .bind("wire protocol deployment")
    .bind("raw MCP receipt task")
    .bind(fixture.agent_id)
    .execute(&fixture.pool)
    .await?;
    Ok(())
}

async fn run_wire_protocol_check() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = setup_fixture().await?;
    let result = async {
        seed_wire_fixture(&fixture).await?;

        let token = ai_crew_sync::auth::generate_token();
        sqlx::query(
            "INSERT INTO api_tokens (agent_id, token_hash, prefix, label) VALUES ($1, $2, $3, $4)",
        )
        .bind(fixture.agent_id)
        .bind(ai_crew_sync::auth::hash_token(&token))
        .bind(ai_crew_sync::auth::token_prefix(&token))
        .bind("task search raw wire protocol test")
        .execute(&fixture.pool)
        .await?;

        let cancellation = CancellationToken::new();
        let router = serve::build_router(
            fixture.pool.clone(),
            &ServeOptions {
                bind: "127.0.0.1:0".to_owned(),
                allowed_hosts: Vec::new(),
                allowed_origins: Vec::new(),
                max_request_bytes: serve::DEFAULT_MAX_REQUEST_BYTES,
                rate_limit_per_minute: 0,
                dashboard_secret: b"task-search-wire-protocol-test-secret".to_vec(),
                nats_url: None,
                nats_credentials: None,
                publication_worker: false,
                event_ping_secs: 3_600,
            },
            cancellation.child_token(),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        let url = format!("http://{address}/mcp");
        let client = Client::builder().build()?;

        let initialize = post_json(
            &client,
            &url,
            &token,
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": MCP_PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": {"name": "crewsync-wire-test", "version": "1.0"}
                }
            }),
        )
        .await?;
        assert_eq!(initialize.status, 200);
        assert_json_rpc_result(&response_json(&initialize)?, 1);

        let tools_list = post_json(
            &client,
            &url,
            &token,
            &json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}),
        )
        .await?;
        assert_eq!(tools_list.status, 200);
        assert!(
            tools_list
                .content_type
                .as_deref()
                .is_some_and(|value| value.starts_with("application/json")),
            "tools/list must return a JSON response"
        );
        let tools_list_json = response_json(&tools_list)?;
        assert_json_rpc_result(&tools_list_json, 2);
        let tools = tools_list_json["result"]["tools"]
            .as_array()
            .expect("tools/list result must contain tools");
        let list_tasks = tools
            .iter()
            .find(|tool| tool["name"] == "list_tasks")
            .expect("tools/list must expose list_tasks");
        for field in ["search", "search_mode", "search_fields", "search_language"] {
            assert!(
                list_tasks["inputSchema"]["properties"].get(field).is_some(),
                "tools/list schema is missing {field}"
            );
        }

        let tools_call = post_json(
            &client,
            &url,
            &token,
            &json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": {
                    "name": "list_tasks",
                    "arguments": {
                        "project_prefix": fixture.task_prefix,
                        "search": "deployment",
                        "search_mode": "contains",
                        "search_fields": "title",
                        "search_language": "simple",
                        "limit": 10
                    }
                }
            }),
        )
        .await?;
        assert_eq!(tools_call.status, 200);
        let tools_call_json = response_json(&tools_call)?;
        assert_json_rpc_result(&tools_call_json, 3);
        let structured = tools_call_json["result"]["structuredContent"]
            .as_object()
            .expect("tools/call result must contain structuredContent");
        assert_eq!(structured["tasks"].as_array().map(Vec::len), Some(1));
        assert_eq!(structured["tasks"][0]["title"], "wire protocol deployment");

        let invalid_call = post_json(
            &client,
            &url,
            &token,
            &json!({
                "jsonrpc": "2.0",
                "id": 4,
                "method": "tools/call",
                "params": {
                    "name": "list_tasks",
                    "arguments": {
                        "search": "deployment",
                        "search_mode": "not-a-mode"
                    }
                }
            }),
        )
        .await?;
        assert_eq!(invalid_call.status, 200);
        let invalid_json = response_json(&invalid_call)?;
        assert_eq!(invalid_json["error"]["code"], -32602);
        assert_eq!(invalid_json["id"], 4);
        assert!(invalid_json["error"]["message"].is_string());

        let receipt = json!({
            "protocol_version": MCP_PROTOCOL_VERSION,
            "initialize": initialize.receipt(),
            "tools_list": tools_list.receipt(),
            "tools_call": tools_call.receipt(),
            "invalid_tools_call": invalid_call.receipt(),
            "redaction": {
                "authorization": "redacted",
                "session": "redacted",
                "database_url": "not captured"
            }
        });
        let receipt_text = serde_json::to_string(&receipt)?;
        assert!(!receipt_text.contains(&token));
        assert!(!receipt_text.contains("TEST_DATABASE_URL"));
        println!("MCP_WIRE_RECEIPT {receipt_text}");

        cancellation.cancel();
        server.abort();
        let _ = server.await;
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    cleanup(fixture).await?;
    result
}

#[tokio::test]
#[ignore = "requires CREWSYNC_ENABLE_LARGE_FIXTURE=1 and disposable TEST_DATABASE_URL"]
async fn mcp_wire_protocol_captures_redacted_tools_list_and_call_receipts() {
    run_wire_protocol_check()
        .await
        .expect("raw MCP wire protocol E2E");
}
