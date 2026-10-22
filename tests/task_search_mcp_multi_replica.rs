#![allow(clippy::too_many_lines)]

#[path = "task_search_mcp_replica_fixture.rs"]
mod fixture;

use std::collections::HashSet;
use std::time::Duration;

use ai_crew_sync::serve::{self, ServeOptions};
use fixture::{REPLICA_SECRET, REPLICA_TASK_COUNT, cleanup, issue_token, seed_replica_tasks, setup_fixture, task_state};
use rmcp::{
    ServiceExt,
    model::{CallToolRequestParams, ClientConfig},
    transport::{
        StreamableHttpClientTransport,
        streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde_json::{Value, json};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

const SERVER_START_TIMEOUT: Duration = Duration::from_secs(5);
const SERVER_STOP_TIMEOUT: Duration = Duration::from_secs(5);
const PAGE_SIZE: usize = 2;

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
        dashboard_secret: REPLICA_SECRET.to_vec(),
        nats_url: None,
        nats_credentials: None,
        publication_worker: false,
        event_ping_secs: 3_600,
    }
}

async fn start_server(pool: &sqlx::PgPool) -> Result<RunningServer, Box<dyn std::error::Error>> {
    let cancellation = CancellationToken::new();
    let router = serve::build_router(
        pool.clone(),
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
    .map_err(|_| "MCP replica did not become ready")?
}

async fn connect_client(
    address: &str,
    raw_token: &str,
    session: &str,
) -> Result<rmcp::service::RunningService<rmcp::RoleClient, ClientConfig>, Box<dyn std::error::Error>> {
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

async fn call_list(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ClientConfig>,
    arguments: Value,
) -> Result<Value, Box<dyn std::error::Error>> {
    let arguments = arguments
        .as_object()
        .cloned()
        .ok_or("MCP tool arguments must be an object")?;
    let result = client
        .call_tool(CallToolRequestParams::new("list_tasks".to_owned()).with_arguments(arguments))
        .await?;
    if result.is_error == Some(true) {
        return Err(format!("list_tasks rejected request: {:?}", result.content).into());
    }
    result
        .structured_content
        .ok_or_else(|| "list_tasks returned no structured content".into())
}

fn page_keys(page: &Value) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    Ok(page["tasks"]
        .as_array()
        .ok_or("MCP page omitted tasks")?
        .iter()
        .map(|task| {
            task["key"]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| "MCP task omitted key".into())
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?)
}

fn next_cursor(page: &Value) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let has_more = page["has_more"].as_bool().ok_or("MCP page omitted has_more")?;
    let cursor = page["next_cursor"].as_str().map(str::to_owned);
    if has_more != cursor.is_some() {
        return Err("MCP cursor/has_more contract is inconsistent".into());
    }
    Ok(cursor)
}

async fn stop_server(server: RunningServer) -> Result<(), Box<dyn std::error::Error>> {
    server.cancellation.cancel();
    timeout(SERVER_STOP_TIMEOUT, server.task)
        .await
        .map_err(|_| "MCP replica did not shut down")??
        .map_err(|error| format!("MCP replica shutdown failed: {error}"))?;
    Ok(())
}

async fn stop_client(client: rmcp::service::RunningService<rmcp::RoleClient, ClientConfig>) {
    let _ = client.cancel().await;
}

async fn run_multi_replica_e2e() -> Result<(), Box<dyn std::error::Error>> {
    let primary = setup_fixture().await?;
    let foreign = setup_fixture().await?;
    let result = async {
        let primary_prefix = primary.task_prefix.clone();
        let foreign_prefix = foreign.task_prefix.clone();
        let expected_keys = seed_replica_tasks(&primary).await?;
        let foreign_keys = seed_replica_tasks(&foreign).await?;
        let owner_token = issue_token(&primary.pool, primary.agent_id, "multi-replica-owner").await?;
        let contender_token = issue_token(
            &primary.pool,
            primary.other_agent_id,
            "multi-replica-contender",
        )
        .await?;
        let foreign_token = issue_token(&foreign.pool, foreign.agent_id, "multi-replica-foreign").await?;

        let replica_one = start_server(&primary.pool).await?;
        wait_until_ready(&replica_one.address).await?;
        let replica_two = start_server(&primary.pool).await?;
        wait_until_ready(&replica_two.address).await?;

        let first_client = connect_client(
            &replica_one.address,
            &owner_token,
            "multi-replica-owner-one",
        )
        .await?;
        let second_client = connect_client(
            &replica_two.address,
            &owner_token,
            "multi-replica-owner-two",
        )
        .await?;

        let first_page = call_list(
            &first_client,
            json!({
                "project_prefix": primary_prefix,
                "limit": PAGE_SIZE,
                "status": "open"
            }),
        )
        .await?;
        let first_cursor = next_cursor(&first_page)?.ok_or("first page unexpectedly exhausted")?;
        let mut seen = HashSet::new();
        seen.extend(page_keys(&first_page)?);
        assert_eq!(seen.len(), PAGE_SIZE);

        let owner_state_before = task_state(
            &primary.pool,
            primary.team_id,
            &expected_keys[REPLICA_TASK_COUNT - 1],
        )
        .await?;

        stop_client(first_client).await;
        stop_server(replica_one).await?;

        let second_page = call_list(
            &second_client,
            json!({
                "project_prefix": primary_prefix,
                "limit": PAGE_SIZE,
                "status": "open",
                "cursor": first_cursor.clone()
            }),
        )
        .await?;
        let second_cursor = next_cursor(&second_page)?.ok_or("second page unexpectedly exhausted")?;
        seen.extend(page_keys(&second_page)?);
        assert_eq!(seen.len(), PAGE_SIZE * 2);

        let restart_client = connect_client(
            &replica_two.address,
            &owner_token,
            "multi-replica-owner-restart",
        )
        .await?;
        let third_page = call_list(
            &restart_client,
            json!({
                "project_prefix": primary_prefix,
                "limit": PAGE_SIZE,
                "status": "open",
                "cursor": second_cursor
            }),
        )
        .await?;
        let third_cursor = next_cursor(&third_page)?;
        seen.extend(page_keys(&third_page)?);
        assert_eq!(seen.len(), PAGE_SIZE * 3);

        let mut terminal_page = third_page;
        let mut cursor = third_cursor;
        while let Some(cursor_value) = cursor {
            terminal_page = call_list(
                &restart_client,
                json!({
                    "project_prefix": primary_prefix,
                    "limit": PAGE_SIZE,
                    "status": "open",
                    "cursor": cursor_value
                }),
            )
            .await?;
            seen.extend(page_keys(&terminal_page)?);
            cursor = next_cursor(&terminal_page)?;
        }
        assert_eq!(seen, expected_keys.iter().cloned().collect());
        assert_eq!(seen.len(), REPLICA_TASK_COUNT);
        assert_eq!(terminal_page["next_cursor"], Value::Null);

        let altered_filter_attempts = [
            json!({"status": "claimed"}),
            json!({"project_prefix": format!("{}-changed", primary_prefix)}),
            json!({"search": "different", "search_mode": "contains"}),
            json!({"search": "Replica", "search_mode": "regex", "search_fields": "title"}),
        ];
        for altered in altered_filter_attempts {
            let mut arguments = altered.as_object().cloned().unwrap_or_default();
            arguments.insert("project_prefix".to_owned(), json!(primary_prefix));
            arguments.insert("limit".to_owned(), json!(PAGE_SIZE));
            arguments.insert("cursor".to_owned(), json!(first_cursor.clone()));
            assert!(
                call_list(&restart_client, Value::Object(arguments)).await.is_err(),
                "cursor must reject altered task-list filters"
            );
        }

        let mut tampered = first_cursor.clone().into_bytes();
        let last = tampered.len() - 1;
        tampered[last] = if tampered[last] == b'a' { b'b' } else { b'a' };
        assert!(
            call_list(
                &restart_client,
                json!({
                    "project_prefix": primary_prefix,
                    "limit": PAGE_SIZE,
                    "status": "open",
                    "cursor": String::from_utf8(tampered)?
                }),
            )
            .await
            .is_err(),
            "tampered cursor must be rejected"
        );

        let foreign_client = connect_client(
            &replica_two.address,
            &foreign_token,
            "multi-replica-foreign-team",
        )
        .await?;
        assert!(
            call_list(
                &foreign_client,
                json!({
                    "project_prefix": primary_prefix,
                    "limit": PAGE_SIZE,
                    "status": "open",
                    "cursor": first_cursor.clone()
                }),
            )
            .await
            .is_err(),
            "cursor must reject cross-team replay"
        );
        let foreign_page = call_list(
            &foreign_client,
            json!({
                "project_prefix": foreign_prefix,
                "limit": PAGE_SIZE,
                "status": "open"
            }),
        )
        .await?;
        assert_eq!(page_keys(&foreign_page)?, foreign_keys[..PAGE_SIZE].to_vec());

        let contender_client = connect_client(
            &replica_two.address,
            &contender_token,
            "multi-replica-contender",
        )
        .await?;
        let claim_key = &expected_keys[0];
        let claim = contender_client
            .call_tool(
                CallToolRequestParams::new("claim_task".to_owned()).with_arguments(
                    serde_json::Map::from_iter([
                        ("key".to_owned(), json!(claim_key)),
                        ("lease_seconds".to_owned(), json!(120)),
                    ]),
                ),
            )
            .await?;
        assert_ne!(claim.is_error, Some(true));
        let claimed_state = task_state(&primary.pool, primary.team_id, claim_key).await?;
        assert_eq!(claimed_state.0, "claimed");
        let after_read_state = task_state(
            &primary.pool,
            primary.team_id,
            &expected_keys[REPLICA_TASK_COUNT - 1],
        )
        .await?;
        assert_eq!(after_read_state, owner_state_before);

        stop_client(second_client).await;
        stop_client(restart_client).await;
        stop_client(foreign_client).await;
        stop_client(contender_client).await;
        stop_server(replica_two).await?;
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    cleanup(primary).await?;
    cleanup(foreign).await?;
    result
}

#[tokio::test]
#[ignore = "requires CREWSYNC_ENABLE_LARGE_FIXTURE=1 and disposable TEST_DATABASE_URL"]
async fn task_search_mcp_alternates_signed_cursor_pages_across_replicas_and_recovers_after_restart() {
    run_multi_replica_e2e()
        .await
        .expect("multi-replica MCP pagination/recovery E2E");
}
