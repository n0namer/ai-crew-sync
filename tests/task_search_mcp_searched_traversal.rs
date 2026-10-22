#![allow(clippy::too_many_lines)]

#[path = "task_pagination_fixture.rs"]
mod fixture;

use std::collections::HashSet;

use ai_crew_sync::{
    auth,
    serve::{self, ServeOptions},
};
use fixture::{LARGE_FIXTURE_SIZE, cleanup, setup_fixture};
use rmcp::{
    ServiceExt,
    model::{CallToolRequestParams, ClientConfig},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde_json::{Map, Value, json};
use sqlx::PgPool;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

struct McpHarness {
    client: rmcp::service::RunningService<rmcp::RoleClient, ClientConfig>,
    cancellation: CancellationToken,
    server: JoinHandle<Result<(), std::io::Error>>,
}

async fn start_mcp(
    pool: PgPool,
    agent_id: Uuid,
    label: &str,
) -> Result<McpHarness, Box<dyn std::error::Error>> {
    let raw_token = auth::generate_token();
    sqlx::query(
        "INSERT INTO api_tokens (agent_id, token_hash, prefix, label) VALUES ($1, $2, $3, $4)",
    )
    .bind(agent_id)
    .bind(auth::hash_token(&raw_token))
    .bind(auth::token_prefix(&raw_token))
    .bind(label)
    .execute(&pool)
    .await?;

    let cancellation = CancellationToken::new();
    let router = serve::build_router(
        pool,
        &ServeOptions {
            bind: "127.0.0.1:0".to_owned(),
            allowed_hosts: Vec::new(),
            allowed_origins: Vec::new(),
            max_request_bytes: serve::DEFAULT_MAX_REQUEST_BYTES,
            rate_limit_per_minute: 0,
            dashboard_secret: b"task-search-mcp-searched-traversal-secret".to_vec(),
            nats_url: None,
            nats_credentials: None,
            publication_worker: false,
            event_ping_secs: 3_600,
        },
        cancellation.child_token(),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let server = tokio::spawn(async move { axum::serve(listener, router).await });

    let mut config = StreamableHttpClientTransportConfig::with_uri(format!("http://{address}/mcp"));
    config.auth_header = Some(raw_token);
    config.allow_stateless = true;
    config.custom_headers.insert(
        auth::SESSION_HEADER.parse()?,
        format!("{label}-session").parse()?,
    );
    let client = ClientConfig::default()
        .serve(StreamableHttpClientTransport::from_config(config))
        .await?;

    Ok(McpHarness {
        client,
        cancellation,
        server,
    })
}

async fn call_list_tasks(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ClientConfig>,
    arguments: Value,
) -> Result<Value, Box<dyn std::error::Error>> {
    let arguments = arguments
        .as_object()
        .cloned()
        .ok_or_else(|| "MCP arguments must be an object".to_owned())?;
    let result = client
        .call_tool(CallToolRequestParams::new("list_tasks".to_owned()).with_arguments(arguments))
        .await?;
    if result.is_error == Some(true) {
        return Err(format!("list_tasks returned an MCP error: {:?}", result.content).into());
    }
    result
        .structured_content
        .clone()
        .ok_or_else(|| "list_tasks did not return structured JSON".into())
}

fn task_keys(page: &Value) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    Ok(page["tasks"]
        .as_array()
        .ok_or_else(|| "MCP page omitted tasks".to_owned())?
        .iter()
        .map(|task| {
            task["key"]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| "MCP task omitted key".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?)
}

async fn seed_matching_and_distractor_tasks(
    pool: &PgPool,
    team_id: Uuid,
    agent_id: Uuid,
    prefix: &str,
) -> Result<HashSet<String>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let mut expected = HashSet::with_capacity(LARGE_FIXTURE_SIZE);
    for ordinal in 0..LARGE_FIXTURE_SIZE {
        let key = format!("{prefix}:match:{ordinal:04}");
        sqlx::query(
            "INSERT INTO tasks (team_id, key, title, description, metadata, created_by) VALUES ($1, $2, $3, $4, jsonb_build_object('fixture_ordinal', $5, 'searched', true), $6)",
        )
        .bind(team_id)
        .bind(&key)
        .bind(format!("needle searched task {ordinal:04}"))
        .bind(format!("fixed traversal match description {ordinal:04}"))
        .bind(ordinal as i32)
        .bind(agent_id)
        .execute(&mut *tx)
        .await?;
        expected.insert(key);
    }
    for ordinal in 0..LARGE_FIXTURE_SIZE {
        sqlx::query(
            "INSERT INTO tasks (team_id, key, title, description, metadata, created_by) VALUES ($1, $2, $3, $4, jsonb_build_object('fixture_ordinal', $5, 'searched', false), $6)",
        )
        .bind(team_id)
        .bind(format!("{prefix}:distractor:{ordinal:04}"))
        .bind(format!("ordinary inventory task {ordinal:04}"))
        .bind(format!("unrelated description {ordinal:04}"))
        .bind(ordinal as i32)
        .bind(agent_id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(expected)
}

async fn stop_mcp(harness: McpHarness) {
    harness.cancellation.cancel();
    let _ = harness.client.cancel().await;
    harness.server.abort();
}

#[tokio::test]
#[ignore = "requires CREWSYNC_ENABLE_LARGE_FIXTURE=1, TASK_CURSOR_SECRET, and disposable TEST_DATABASE_URL"]
async fn traverses_exactly_2000_search_matches_through_real_mcp_signed_cursors() {
    let fixture = setup_fixture().await.expect("safe disposable PostgreSQL");
    let prefix = fixture.task_prefix.clone();
    let result = async {
        let expected = seed_matching_and_distractor_tasks(
            &fixture.pool,
            fixture.team_id,
            fixture.agent_id,
            &prefix,
        )
        .await?;
        assert_eq!(expected.len(), LARGE_FIXTURE_SIZE);

        let harness = start_mcp(
            fixture.pool.clone(),
            fixture.agent_id,
            "task-search-mcp-searched-traversal",
        )
        .await?;
        let fixed = |cursor: Option<String>| {
            let mut arguments = Map::new();
            arguments.insert("project_prefix".to_owned(), json!(prefix));
            arguments.insert("status".to_owned(), json!("open"));
            arguments.insert("search".to_owned(), json!("needle searched"));
            arguments.insert("search_mode".to_owned(), json!("contains"));
            arguments.insert("search_fields".to_owned(), json!("both"));
            arguments.insert("search_language".to_owned(), json!("simple"));
            arguments.insert("limit".to_owned(), json!(113));
            if let Some(cursor) = cursor {
                arguments.insert("cursor".to_owned(), json!(cursor));
            }
            Value::Object(arguments)
        };

        let mut cursor = None;
        let mut seen = HashSet::with_capacity(LARGE_FIXTURE_SIZE);
        let mut pages = 0;
        let mut first_cursor = None;
        loop {
            let page = call_list_tasks(&harness.client, fixed(cursor.take())).await?;
            pages += 1;
            let page_keys = task_keys(&page)?;
            assert!(page_keys.len() <= 113);
            for key in page_keys {
                assert!(
                    seen.insert(key.clone()),
                    "duplicate matching task returned by signed MCP cursor"
                );
                assert!(expected.contains(&key), "distractor escaped search filter: {key}");
            }
            let has_more = page["has_more"]
                .as_bool()
                .ok_or("MCP page omitted has_more")?;
            let next_cursor = page["next_cursor"].as_str().map(str::to_owned);
            if pages == 1 {
                first_cursor = next_cursor.clone();
            }
            if !has_more {
                assert!(next_cursor.is_none(), "terminal page must omit next_cursor");
                break;
            }
            assert!(next_cursor.is_some(), "non-terminal page must return cursor");
            cursor = next_cursor;
        }

        assert!(pages >= 18, "2000 matches must span many pages");
        assert_eq!(seen, expected, "MCP traversal must return exactly the match set");
        assert!(first_cursor.is_some(), "first page must expose a signed cursor");

        let cursor = first_cursor.expect("first page cursor was captured");
        let altered_search = call_list_tasks(
            &harness.client,
            fixed(Some(cursor.clone())).as_object().map(|arguments| {
                let mut changed = arguments.clone();
                changed.insert("search".to_owned(), json!("different search"));
                Value::Object(changed)
            }).expect("fixed arguments are an object"),
        )
        .await;
        assert!(
            altered_search.is_err(),
            "signed cursor must reject changed search filters"
        );

        let mut tampered_arguments = fixed(Some(cursor))
            .as_object()
            .cloned()
            .expect("fixed arguments are an object");
        let tampered_cursor = tampered_arguments["cursor"]
            .as_str()
            .expect("captured cursor is a string");
        let replacement = if tampered_cursor.ends_with('A') { 'B' } else { 'A' };
        let mut tampered = tampered_cursor.to_owned();
        tampered.pop();
        tampered.push(replacement);
        tampered_arguments.insert("cursor".to_owned(), json!(tampered));
        let tampered_result = call_list_tasks(&harness.client, Value::Object(tampered_arguments)).await;
        assert!(tampered_result.is_err(), "signed cursor tampering must be rejected");

        stop_mcp(harness).await;
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    cleanup(fixture).await.expect("fixture cleanup");
    result.expect("real MCP searched traversal E2E");
}
