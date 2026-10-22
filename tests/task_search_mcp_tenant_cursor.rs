#![allow(clippy::too_many_lines)]

#[path = "task_pagination_fixture.rs"]
mod fixture;

use ai_crew_sync::{
    auth,
    serve::{self, ServeOptions},
};
use fixture::{cleanup, setup_fixture, Fixture};
use rmcp::{
    model::{CallToolRequestParams, ClientConfig},
    transport::{
        streamable_http_client::StreamableHttpClientTransportConfig,
        StreamableHttpClientTransport,
    },
    ServiceExt,
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
    url: String,
}

async fn start_server(
    pool: &PgPool,
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
    .execute(pool)
    .await?;

    let cancellation = CancellationToken::new();
    let router = serve::build_router(
        pool.clone(),
        &ServeOptions {
            bind: "127.0.0.1:0".to_owned(),
            allowed_hosts: Vec::new(),
            allowed_origins: Vec::new(),
            max_request_bytes: serve::DEFAULT_MAX_REQUEST_BYTES,
            rate_limit_per_minute: 0,
            dashboard_secret: b"task-search-mcp-tenant-cursor-test-secret".to_vec(),
            nats_url: None,
            nats_credentials: None,
            publication_worker: false,
            event_ping_secs: 3_600,
        },
        cancellation.child_token(),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let url = format!("http://{address}/mcp");
    let server = tokio::spawn(async move { axum::serve(listener, router).await });

    let mut config = StreamableHttpClientTransportConfig::with_uri(url.clone());
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
        url,
    })
}

async fn call_list(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ClientConfig>,
    arguments: Value,
) -> Result<Value, Box<dyn std::error::Error>> {
    let arguments = arguments
        .as_object()
        .cloned()
        .ok_or("list_tasks arguments must be an object")?;
    let result = client
        .call_tool(CallToolRequestParams::new("list_tasks".to_owned()).with_arguments(arguments))
        .await?;
    if result.is_error == Some(true) {
        return Err(format!("list_tasks returned an MCP error: {:?}", result.content).into());
    }
    Ok(result
        .structured_content
        .clone()
        .ok_or("list_tasks did not return structured JSON")?)
}

fn task_keys(page: &Value) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    Ok(page["tasks"]
        .as_array()
        .ok_or("list_tasks response omitted tasks")?
        .iter()
        .map(|task| {
            task["key"]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| "task response omitted key".into())
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?)
}

fn search_arguments(prefix: &str, cursor: Option<&str>, mode: &str, fields: &str, language: &str) -> Value {
    let mut arguments = Map::from_iter([
        ("project_prefix".to_owned(), json!(prefix)),
        ("search".to_owned(), json!("tenant cursor")),
        ("search_mode".to_owned(), json!(mode)),
        ("search_fields".to_owned(), json!(fields)),
        ("search_language".to_owned(), json!(language)),
        ("limit".to_owned(), json!(1)),
    ]);
    if let Some(cursor) = cursor {
        arguments.insert("cursor".to_owned(), json!(cursor));
    }
    Value::Object(arguments)
}

async fn seed_primary(fixture: &Fixture) -> Result<(), sqlx::Error> {
    let mut tx = fixture.pool.begin().await?;
    for ordinal in 0..3 {
        sqlx::query(
            "INSERT INTO tasks (team_id, key, title, description, created_by, updated_at) VALUES ($1, $2, $3, $4, $5, TIMESTAMPTZ '2026-01-01 00:00:00+00')",
        )
        .bind(fixture.team_id)
        .bind(format!("{}:{ordinal:04}", fixture.task_prefix))
        .bind(format!("Primary tenant cursor {ordinal}"))
        .bind("tenant cursor searchable description")
        .bind(fixture.agent_id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await
}

async fn create_foreign_tenant(
    fixture: &Fixture,
) -> Result<(Uuid, Uuid), Box<dyn std::error::Error>> {
    let mut tx = fixture.pool.begin().await?;
    let team_id: Uuid = sqlx::query_scalar(
        "INSERT INTO teams (slug, name) VALUES ($1, $2) RETURNING id",
    )
    .bind(format!("tenant-cursor-foreign-{}", Uuid::new_v4().simple()))
    .bind("tenant cursor foreign team")
    .fetch_one(&mut *tx)
    .await?;
    let agent_id: Uuid = sqlx::query_scalar(
        "INSERT INTO agents (team_id, name) VALUES ($1, $2) RETURNING id",
    )
    .bind(team_id)
    .bind("tenant-cursor-foreign-agent")
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO tasks (team_id, key, title, description, created_by) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(team_id)
    .bind("tenant-cursor:foreign-secret")
    .bind("Foreign tenant cursor secret")
    .bind("must never cross the tenant boundary")
    .bind(agent_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((team_id, agent_id))
}

async fn assert_unauthenticated_is_401(url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let response = reqwest::Client::new()
        .post(url)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        }))
        .send()
        .await?;
    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
    Ok(())
}

async fn stop(harness: McpHarness) {
    harness.cancellation.cancel();
    let _ = harness.client.cancel().await;
    harness.server.abort();
    let _ = harness.server.await;
}

#[tokio::test]
#[ignore = "requires CREWSYNC_ENABLE_LARGE_FIXTURE=1 and disposable TEST_DATABASE_URL"]
async fn mcp_search_cursor_is_tenant_bound_and_search_binding_is_fail_closed() {
    let fixture = setup_fixture().await.expect("safe disposable PostgreSQL");
    let result = async {
        seed_primary(&fixture).await?;
        let (foreign_team_id, foreign_agent_id) = create_foreign_tenant(&fixture).await?;
        let primary = start_server(&fixture.pool, fixture.agent_id, "tenant-primary").await?;
        let foreign = start_server(&fixture.pool, foreign_agent_id, "tenant-foreign").await?;

        assert_unauthenticated_is_401(&primary.url).await?;

        let first = call_list(
            &primary.client,
            search_arguments(&fixture.task_prefix, None, "contains", "both", "simple"),
        )
        .await?;
        assert_eq!(task_keys(&first)?.len(), 1);
        let cursor = first["next_cursor"]
            .as_str()
            .ok_or("search page did not return a cursor")?
            .to_owned();

        let second = call_list(
            &primary.client,
            search_arguments(&fixture.task_prefix, Some(&cursor), "contains", "both", "simple"),
        )
        .await?;
        assert_eq!(task_keys(&second)?.len(), 1);

        let mut tampered = cursor.clone();
        let replacement = if tampered.ends_with('A') { 'B' } else { 'A' };
        tampered.pop();
        tampered.push(replacement);
        assert!(
            call_list(
                &primary.client,
                search_arguments(
                    &fixture.task_prefix,
                    Some(&tampered),
                    "contains",
                    "both",
                    "simple",
                ),
            )
            .await
            .is_err(),
            "tampered cursor must be rejected over MCP"
        );

        for (mode, fields, language) in [
            ("keywords", "both", "simple"),
            ("contains", "title", "simple"),
            ("contains", "both", "english"),
        ] {
            assert!(
                call_list(
                    &primary.client,
                    search_arguments(
                        &fixture.task_prefix,
                        Some(&cursor),
                        mode,
                        fields,
                        language,
                    ),
                )
                .await
                .is_err(),
                "cursor replay with changed {mode}/{fields}/{language} must fail"
            );
        }

        assert!(
            call_list(
                &foreign.client,
                search_arguments(
                    &fixture.task_prefix,
                    Some(&cursor),
                    "contains",
                    "both",
                    "simple",
                ),
            )
            .await
            .is_err(),
            "cursor must not cross tenant boundaries"
        );
        let foreign_search = call_list(
            &foreign.client,
            json!({
                "search": "Primary tenant cursor",
                "search_mode": "contains",
                "search_fields": "both",
                "search_language": "simple",
                "limit": 10
            }),
        )
        .await?;
        assert!(
            task_keys(&foreign_search)?.is_empty(),
            "foreign tenant must not observe primary task data"
        );

        stop(primary).await;
        stop(foreign).await;
        sqlx::query("DELETE FROM teams WHERE id = $1")
            .bind(foreign_team_id)
            .execute(&fixture.pool)
            .await?;
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    cleanup(fixture).await.expect("fixture cleanup");
    result.expect("tenant and cursor MCP E2E");
}
