#![allow(clippy::too_many_lines)]

#[path = "task_pagination_fixture.rs"]
mod fixture;

use std::time::Duration;

use ai_crew_sync::serve::{self, ServeOptions};
use fixture::{Fixture, cleanup, setup_fixture};
use rmcp::{
    ServiceExt,
    model::{CallToolRequestParams, ClientConfig},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde_json::{Value, json};
use tokio::{task::JoinSet, time::timeout};
use tokio_util::sync::CancellationToken;

const SERVER_START_TIMEOUT: Duration = Duration::from_secs(5);
const SERVER_STOP_TIMEOUT: Duration = Duration::from_secs(5);

struct RunningServer {
    address: String,
    cancellation: CancellationToken,
    task: tokio::task::JoinHandle<Result<(), std::io::Error>>,
}

fn server_options() -> ServeOptions {
    ServeOptions {
        bind: "127.0.0.1:0".to_owned(),
        allowed_hosts: Vec::new(),
        allowed_origins: Vec::new(),
        max_request_bytes: serve::DEFAULT_MAX_REQUEST_BYTES,
        rate_limit_per_minute: 0,
        dashboard_secret: b"task-search-mcp-restart-lease-secret".to_vec(),
        nats_url: None,
        nats_credentials: None,
        publication_worker: false,
        event_ping_secs: 3_600,
    }
}

async fn seed_restart_task(fixture: &Fixture) -> Result<String, sqlx::Error> {
    let key = format!("{}:restart-lease", fixture.task_prefix);
    sqlx::query(
        "INSERT INTO tasks (team_id, key, title, description, created_by) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(fixture.team_id)
    .bind(&key)
    .bind("restart lease recovery task")
    .bind("claim must survive MCP server restart")
    .bind(fixture.agent_id)
    .execute(&fixture.pool)
    .await?;
    Ok(key)
}

async fn issue_token(fixture: &Fixture, agent_id: uuid::Uuid) -> Result<String, sqlx::Error> {
    let raw_token = ai_crew_sync::auth::generate_token();
    sqlx::query(
        "INSERT INTO api_tokens (agent_id, token_hash, prefix, label) VALUES ($1, $2, $3, $4)",
    )
    .bind(agent_id)
    .bind(ai_crew_sync::auth::hash_token(&raw_token))
    .bind(ai_crew_sync::auth::token_prefix(&raw_token))
    .bind("task search MCP restart lease E2E")
    .execute(&fixture.pool)
    .await?;
    Ok(raw_token)
}

async fn start_server(fixture: &Fixture) -> Result<RunningServer, Box<dyn std::error::Error>> {
    let cancellation = CancellationToken::new();
    let router = serve::build_router(
        fixture.pool.clone(),
        &server_options(),
        cancellation.child_token(),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?.to_string();
    let shutdown = cancellation.clone();
    let task = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown.cancelled_owned())
            .await
    });
    Ok(RunningServer {
        address,
        cancellation,
        task,
    })
}

async fn wait_until_ready(address: &str) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    timeout(SERVER_START_TIMEOUT, async {
        loop {
            match client.get(format!("http://{address}/health")).send().await {
                Ok(response) if response.status().is_success() => return Ok(()),
                Ok(_) | Err(_) => tokio::time::sleep(Duration::from_millis(25)).await,
            }
        }
    })
    .await
    .map_err(|_| "MCP server did not become ready")?
}

async fn connect_client(
    address: &str,
    raw_token: &str,
    session: &str,
) -> Result<rmcp::service::RunningService<rmcp::RoleClient, ClientConfig>, Box<dyn std::error::Error>>
{
    let mut transport =
        StreamableHttpClientTransportConfig::with_uri(format!("http://{address}/mcp"));
    transport.auth_header = Some(raw_token.to_owned());
    transport.allow_stateless = true;
    transport.custom_headers.insert(
        ai_crew_sync::auth::SESSION_HEADER.parse()?,
        session.parse()?,
    );
    Ok(ClientConfig::default()
        .serve(StreamableHttpClientTransport::from_config(transport))
        .await?)
}

async fn call_tool(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ClientConfig>,
    name: &str,
    arguments: Value,
) -> Result<Value, Box<dyn std::error::Error>> {
    let arguments = arguments
        .as_object()
        .cloned()
        .ok_or("MCP tool arguments must be an object")?;
    let result = client
        .call_tool(CallToolRequestParams::new(name.to_owned()).with_arguments(arguments))
        .await?;
    if result.is_error == Some(true) {
        return Err(format!("MCP tool {name} returned an error").into());
    }
    result
        .structured_content
        .ok_or_else(|| format!("MCP tool {name} returned no structured content").into())
}

async fn stop_server(server: RunningServer) -> Result<(), Box<dyn std::error::Error>> {
    server.cancellation.cancel();
    timeout(SERVER_STOP_TIMEOUT, server.task)
        .await
        .map_err(|_| "MCP server did not shut down")??
        .map_err(|error| format!("MCP server shutdown failed: {error}"))?;
    Ok(())
}

async fn run_restart_lease_e2e() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = setup_fixture().await?;
    let result = async {
        let task_key = seed_restart_task(&fixture).await?;
        let owner_token = issue_token(&fixture, fixture.agent_id).await?;
        let other_token = issue_token(&fixture, fixture.other_agent_id).await?;

        let first_server = start_server(&fixture).await?;
        wait_until_ready(&first_server.address).await?;
        let first_address = first_server.address.clone();
        let owner_first = connect_client(
            &first_address,
            &owner_token,
            "task-search-mcp-restart-lease-owner",
        )
        .await?;
        let other_first = connect_client(
            &first_address,
            &other_token,
            "task-search-mcp-restart-lease-contender",
        )
        .await?;

        let tools = owner_first.list_all_tools().await?;
        assert!(tools.iter().any(|tool| tool.name == "claim_task"));
        assert!(tools.iter().any(|tool| tool.name == "list_tasks"));

        let claimed = call_tool(
            &owner_first,
            "claim_task",
            json!({"key": task_key, "lease_seconds": 120}),
        )
        .await?;
        assert_eq!(claimed["claimed"], true);
        assert_eq!(claimed["task"]["key"], task_key);
        assert_eq!(claimed["task"]["claimed_session"], "task-search-mcp-restart-lease-owner");
        assert!(claimed["task"]["lease_expires_at"].is_string());

        let contender = call_tool(
            &other_first,
            "claim_task",
            json!({"key": task_key, "lease_seconds": 120}),
        )
        .await?;
        assert_eq!(contender["claimed"], false);
        assert!(contender["reason"].is_string());

        let owner_list = call_tool(
            &owner_first,
            "list_tasks",
            json!({
                "project_prefix": fixture.task_prefix,
                "status": "claimed",
                "mine_only": true,
                "limit": 10
            }),
        )
        .await?;
        assert_eq!(owner_list["tasks"].as_array().map(Vec::len), Some(1));
        assert_eq!(owner_list["tasks"][0]["key"], task_key);
        assert_eq!(owner_list["tasks"][0]["claimed_session"], "task-search-mcp-restart-lease-owner");

        let mut concurrent_clients = JoinSet::new();
        for ordinal in 0..4 {
            let address = first_address.clone();
            let token = owner_token.clone();
            concurrent_clients.spawn(async move {
                let session = format!("task-search-mcp-restart-lease-concurrent-{ordinal}");
                let client = connect_client(&address, &token, &session)
                    .await
                    .map_err(|error| error.to_string())?;
                let result = call_tool(
                    &client,
                    "list_tasks",
                    json!({"limit": 10, "mine_only": false}),
                )
                .await
                .map_err(|error| error.to_string())?;
                let task_count = result["tasks"].as_array().map(Vec::len).unwrap_or_default();
                let _ = client.cancel().await;
                if task_count == 0 {
                    return Err("concurrent MCP client saw no tasks".to_owned());
                }
                Ok::<(), String>(())
            });
        }
        while let Some(worker) = concurrent_clients.join_next().await {
            worker??;
        }

        let _ = owner_first.cancel().await;
        let _ = other_first.cancel().await;
        stop_server(first_server).await?;
        let stopped_health = reqwest::Client::new()
            .get(format!("http://{first_address}/health"))
            .send()
            .await;
        assert!(stopped_health.is_err());

        let second_server = start_server(&fixture).await?;
        wait_until_ready(&second_server.address).await?;
        let owner_second = connect_client(
            &second_server.address,
            &owner_token,
            "task-search-mcp-restart-lease-owner",
        )
        .await?;
        let other_second = connect_client(
            &second_server.address,
            &other_token,
            "task-search-mcp-restart-lease-contender",
        )
        .await?;

        let restarted_list = call_tool(
            &owner_second,
            "list_tasks",
            json!({
                "project_prefix": fixture.task_prefix,
                "status": "claimed",
                "mine_only": true,
                "limit": 10
            }),
        )
        .await?;
        assert_eq!(restarted_list["tasks"].as_array().map(Vec::len), Some(1));
        assert_eq!(restarted_list["tasks"][0]["key"], task_key);
        assert_eq!(restarted_list["tasks"][0]["claimed_session"], "task-search-mcp-restart-lease-owner");
        assert_eq!(restarted_list["tasks"][0]["lease_expired"], false);

        let restarted_contender = call_tool(
            &other_second,
            "claim_task",
            json!({"key": task_key, "lease_seconds": 120}),
        )
        .await?;
        assert_eq!(restarted_contender["claimed"], false);

        let renewed = call_tool(
            &owner_second,
            "renew_task_lease",
            json!({"key": task_key, "lease_seconds": 180}),
        )
        .await?;
        assert_eq!(renewed["key"], task_key);
        assert_eq!(renewed["claimed_session"], "task-search-mcp-restart-lease-owner");
        assert!(renewed["lease_expires_at"].is_string());

        let _ = owner_second.cancel().await;
        let _ = other_second.cancel().await;
        stop_server(second_server).await?;
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    cleanup(fixture).await?;
    result
}

#[tokio::test]
#[ignore = "requires CREWSYNC_ENABLE_LARGE_FIXTURE=1 and disposable TEST_DATABASE_URL"]
async fn task_search_mcp_restart_preserves_live_claim_leases_and_concurrent_clients() {
    run_restart_lease_e2e()
        .await
        .expect("MCP restart/lease recovery E2E");
}
