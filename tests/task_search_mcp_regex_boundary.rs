#![allow(clippy::too_many_lines)]

#[path = "task_pagination_fixture.rs"]
mod fixture;

use std::time::{Duration, Instant};

use ai_crew_sync::{
    auth,
    serve::{self, ServeOptions},
};
use fixture::{Fixture, cleanup, setup_fixture};
use rmcp::{
    ServiceExt,
    model::{CallToolRequestParams, ClientConfig},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

struct McpHarness {
    client: rmcp::service::RunningService<rmcp::RoleClient, ClientConfig>,
    cancellation: CancellationToken,
    server: JoinHandle<Result<(), std::io::Error>>,
}

async fn start_mcp(
    pool: PgPool,
    agent_id: uuid::Uuid,
) -> Result<McpHarness, Box<dyn std::error::Error>> {
    let raw_token = auth::generate_token();
    sqlx::query(
        "INSERT INTO api_tokens (agent_id, token_hash, prefix, label) VALUES ($1, $2, $3, $4)",
    )
    .bind(agent_id)
    .bind(auth::hash_token(&raw_token))
    .bind(auth::token_prefix(&raw_token))
    .bind("task search MCP regex boundary")
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
            dashboard_secret: b"task-search-mcp-regex-boundary-secret".to_vec(),
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
        "task-search-mcp-regex-boundary".parse()?,
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
) -> Result<(bool, String, Option<Value>), Box<dyn std::error::Error>> {
    let arguments = arguments
        .as_object()
        .cloned()
        .ok_or_else(|| "list_tasks arguments must be an object".to_owned())?;
    let result = match client
        .call_tool(CallToolRequestParams::new("list_tasks".to_owned()).with_arguments(arguments))
        .await
    {
        Ok(result) => result,
        Err(error) => {
            let details = format!("{error:?}");
            if details.contains("-32602")
                && (details.contains("search regex is not a valid PostgreSQL regular expression")
                    || details.contains("regex search exceeded the 250ms execution limit"))
            {
                return Ok((true, details, None));
            }
            return Err(error.into());
        }
    };
    Ok((
        result.is_error == Some(true),
        serde_json::to_string(&result.content)?,
        result.structured_content.clone(),
    ))
}

fn open_total(page: &Value) -> i64 {
    page["open"].as_i64().expect("MCP response has numeric open total")
}

fn task_keys(page: &Value) -> Vec<String> {
    page["tasks"]
        .as_array()
        .expect("MCP response has tasks")
        .iter()
        .map(|task| task["key"].as_str().expect("task key is a string").to_owned())
        .collect()
}

async fn seed_fixture(fixture: &Fixture) -> Result<String, sqlx::Error> {
    let prefix = fixture.task_prefix.clone();
    let long_nonmatching = format!("{}b", "a".repeat(120_000));
    let rows = [
        ("regex healthy title", "ordinary searchable description"),
        ("invalid regex sentinel", "safe recovery description"),
        ("timeout regex sentinel", long_nonmatching.as_str()),
        ("non indexable regex sentinel", "a repeated searchable value"),
    ];
    let mut tx = fixture.pool.begin().await?;
    for (ordinal, (title, description)) in rows.into_iter().enumerate() {
        sqlx::query(
            "INSERT INTO tasks (team_id, key, title, description, created_by) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(fixture.team_id)
        .bind(format!("{prefix}:{ordinal:04}"))
        .bind(title)
        .bind(description)
        .bind(fixture.agent_id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(prefix)
}

async fn explain_non_indexable_regex(
    pool: &PgPool,
    team_id: uuid::Uuid,
) -> Result<String, sqlx::Error> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "EXPLAIN (COSTS OFF) SELECT key FROM tasks WHERE team_id = $1 AND coalesce(description, '') ~* $2",
    )
    .bind(team_id)
    .bind("^(a+)+$")
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(line,)| line).collect::<Vec<_>>().join("\n"))
}

async fn stop_mcp(harness: McpHarness) {
    let _ = harness.client.cancel().await;
    harness.cancellation.cancel();
    harness.server.abort();
    let _ = harness.server.await;
}

#[tokio::test]
#[ignore = "requires CREWSYNC_ENABLE_LARGE_FIXTURE=1 and disposable TEST_DATABASE_URL"]
async fn real_mcp_regex_errors_are_classified_bounded_and_recover_with_totals() {
    let fixture = setup_fixture().await.expect("safe disposable PostgreSQL");
    let result = async {
        let prefix = seed_fixture(&fixture).await?;
        let plan = explain_non_indexable_regex(&fixture.pool, fixture.team_id).await?;
        assert!(plan.contains("Filter:"), "EXPLAIN should expose the regex filter: {plan}");
        assert!(plan.contains("~*"), "EXPLAIN should retain the regex operator: {plan}");
        assert!(
            !plan.contains("tasks_description_search_trgm_idx")
                && !plan.contains("tasks_title_search_trgm_idx"),
            "non-indexable regex must not use a trigram index; tenant indexes remain allowed: {plan}"
        );

        let harness = start_mcp(fixture.pool.clone(), fixture.agent_id).await?;
        let invalid_started = Instant::now();
        let (invalid_error, invalid_content, invalid_page) = call_list_tasks(
            &harness.client,
            json!({
                "project_prefix": prefix,
                "search": "[unterminated",
                "search_mode": "regex",
                "search_fields": "description",
                "search_language": "simple",
                "limit": 20
            }),
        )
        .await?;
        assert!(invalid_error, "invalid regex must be an MCP protocol error");
        assert!(invalid_page.is_none());
        assert!(
            invalid_content.contains("-32602")
                && invalid_content.contains(
                    "search regex is not a valid PostgreSQL regular expression"
                ),
            "invalid regex must preserve exact MCP InvalidParams and sanitized message: {invalid_content}"
        );
        assert!(!invalid_content.contains("SELECT"));
        assert!(!invalid_content.contains("postgres://"));
        assert!(!invalid_content.contains("DATABASE_URL"));
        assert!(invalid_started.elapsed() < Duration::from_secs(2));

        let timeout_started = Instant::now();
        let (timeout_error, timeout_content, timeout_page) = call_list_tasks(
            &harness.client,
            json!({
                "project_prefix": prefix,
                "search": "^(a+)+$",
                "search_mode": "regex",
                "search_fields": "description",
                "search_language": "simple",
                "limit": 20
            }),
        )
        .await?;
        let timeout_elapsed = timeout_started.elapsed();
        assert!(
            timeout_elapsed < Duration::from_secs(2),
            "pathological regex exceeded the bounded MCP request budget"
        );
        assert!(timeout_error, "pathological regex must hit the bounded MCP timeout");
        assert!(timeout_page.is_none());
        assert!(
            timeout_content.contains("-32602")
                && timeout_content.contains("regex search exceeded the 250ms execution limit"),
            "statement_timeout SQLSTATE 57014 should preserve the distinct bounded MCP error: {timeout_content}"
        );
        assert!(!timeout_content.contains("SELECT"));
        assert!(!timeout_content.contains("postgres://"));
        assert!(!timeout_content.contains("DATABASE_URL"));

        let (healthy_error, healthy_content, healthy_page) = call_list_tasks(
            &harness.client,
            json!({
                "project_prefix": prefix,
                "search": "safe recovery",
                "search_mode": "contains",
                "search_fields": "description",
                "search_language": "simple",
                "limit": 20
            }),
        )
        .await?;
        assert!(!healthy_error, "a later healthy search must succeed: {healthy_content}");
        let healthy_page = healthy_page.expect("healthy search has structured content");
        assert_eq!(task_keys(&healthy_page).len(), 1);
        assert!(task_keys(&healthy_page)[0].ends_with(":0001"));
        assert_eq!(open_total(&healthy_page), 4);
        assert_eq!(healthy_page["claimed"].as_i64(), Some(0));

        let (regex_error, regex_content, regex_page) = call_list_tasks(
            &harness.client,
            json!({
                "project_prefix": prefix,
                "search": "^regex healthy title$",
                "search_mode": "regex",
                "search_fields": "title",
                "search_language": "simple",
                "limit": 20
            }),
        )
        .await?;
        assert!(!regex_error, "a later regex search must remain healthy: {regex_content}");
        let regex_page = regex_page.expect("healthy regex has structured content");
        assert_eq!(task_keys(&regex_page).len(), 1);
        assert_eq!(open_total(&regex_page), 4);
        assert_eq!(regex_page["claimed"].as_i64(), Some(0));

        stop_mcp(harness).await;
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    cleanup(fixture).await.expect("fixture cleanup");
    result.expect("real MCP regex boundary E2E");
}
