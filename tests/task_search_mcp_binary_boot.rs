#![allow(clippy::too_many_lines)]

#[path = "task_pagination_fixture.rs"]
mod fixture;

use std::{process::Stdio, time::Duration};

use ai_crew_sync::auth;
use fixture::{Fixture, cleanup, setup_fixture};
use rmcp::{
    ServiceExt,
    model::{CallToolRequestParams, ClientConfig},
    transport::{
        StreamableHttpClientTransport,
        streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde_json::{Value, json};
use tokio::{
    process::{Child, Command},
    time::{sleep, timeout},
};

const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(150);

async fn seed_binary_fixture(fixture: &Fixture) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO tasks (team_id, key, title, description, created_by) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(fixture.team_id)
    .bind(format!("{}:binary", fixture.task_prefix))
    .bind("Binary boot task")
    .bind("served by the compiled production process")
    .bind(fixture.agent_id)
    .execute(&fixture.pool)
    .await?;
    Ok(())
}

async fn issue_token(fixture: &Fixture) -> Result<String, sqlx::Error> {
    let raw_token = auth::generate_token();
    sqlx::query(
        "INSERT INTO api_tokens (agent_id, token_hash, prefix, label) VALUES ($1, $2, $3, $4)",
    )
    .bind(fixture.agent_id)
    .bind(auth::hash_token(&raw_token))
    .bind(auth::token_prefix(&raw_token))
    .bind("task search MCP binary boot E2E")
    .execute(&fixture.pool)
    .await?;
    Ok(raw_token)
}

fn binary_path() -> Result<std::ffi::OsString, Box<dyn std::error::Error>> {
    std::env::var_os("CARGO_BIN_EXE_ai-crew-sync")
        .ok_or_else(|| "Cargo did not provide CARGO_BIN_EXE_ai-crew-sync".into())
}

async fn launch_binary(database_url: &str) -> Result<(Child, String), Box<dyn std::error::Error>> {
    let probe = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = probe.local_addr()?;
    drop(probe);

    let bind = address.to_string();
    let mut child = Command::new(binary_path()?)
        .arg("serve")
        .arg("--bind")
        .arg(&bind)
        .env("DATABASE_URL", database_url)
        .env("BUS_RATE_LIMIT_PER_MINUTE", "0")
        .env("RUST_LOG", "warn")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;

    let health_url = format!("http://{bind}/health");
    let client = reqwest::Client::new();
    let result = timeout(STARTUP_TIMEOUT, async {
        loop {
            if let Some(status) = child.try_wait()? {
                let stderr = child
                    .stderr
                    .take()
                    .map(|mut stream| async move {
                        use tokio::io::AsyncReadExt;
                        let mut bytes = Vec::new();
                        let _ = stream.read_to_end(&mut bytes).await;
                        String::from_utf8_lossy(&bytes).into_owned()
                    });
                let details = match stderr {
                    Some(read) => read.await,
                    None => String::new(),
                };
                return Err(format!(
                    "ai-crew-sync serve exited during startup with {status}: {details}"
                )
                .into());
            }

            if let Ok(response) = client.get(&health_url).send().await {
                if response.status().is_success() {
                    let body: Value = response.json().await?;
                    if body["status"] == "ok" && body["database"] == "up" {
                        return Ok(());
                    }
                }
            }
            sleep(POLL_INTERVAL).await;
        }
    })
    .await;

    match result {
        Ok(Ok(())) => Ok((child, format!("http://{bind}/mcp"))),
        Ok(Err(error)) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            Err(error)
        }
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            Err("timed out waiting for the compiled ai-crew-sync binary health check".into())
        }
    }
}

async fn call_list_tasks(
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
    assert_ne!(
        result.is_error,
        Some(true),
        "list_tasks returned an MCP error"
    );
    result
        .structured_content
        .clone()
        .ok_or_else(|| "list_tasks did not return structured JSON".into())
}

async fn run_binary_boot_e2e() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = setup_fixture().await?;
    let database_url = std::env::var("TEST_DATABASE_URL")?;
    let result = async {
        seed_binary_fixture(&fixture).await?;
        let token = issue_token(&fixture).await?;
        let (mut process, mcp_url) = launch_binary(&database_url).await?;

        let mut transport = StreamableHttpClientTransportConfig::with_uri(mcp_url);
        transport.auth_header = Some(token);
        transport.allow_stateless = true;
        transport.custom_headers.insert(
            auth::SESSION_HEADER.parse()?,
            "task-search-mcp-binary-boot".parse()?,
        );
        let client = ClientConfig::default()
            .serve(StreamableHttpClientTransport::from_config(transport))
            .await?;

        let tools = client.list_all_tools().await?;
        assert!(
            tools.iter().any(|tool| tool.name == "list_tasks"),
            "compiled binary tools/list does not expose list_tasks"
        );

        let response = call_list_tasks(
            &client,
            json!({
                "project_prefix": fixture.task_prefix,
                "search": "Binary boot",
                "search_mode": "contains",
                "search_fields": "title",
                "search_language": "simple",
                "limit": 10,
                "mine_only": false
            }),
        )
        .await?;
        let tasks = response["tasks"].as_array().ok_or("tasks is not an array")?;
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0]["key"], format!("{}:binary", fixture.task_prefix));
        assert_eq!(response["has_more"], false);

        let _ = client.cancel().await;
        process.kill().await?;
        process.wait().await?;
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    cleanup(fixture).await?;
    result
}

#[tokio::test]
#[ignore = "requires CREWSYNC_ENABLE_LARGE_FIXTURE=1, disposable TEST_DATABASE_URL, and Cargo's binary path"]
async fn compiled_serve_binary_boots_and_serves_authenticated_mcp() {
    run_binary_boot_e2e()
        .await
        .expect("compiled ai-crew-sync serve binary E2E");
}

#[test]
fn binary_boot_contract_uses_the_production_binary_name() {
    assert_eq!(env!("CARGO_PKG_NAME"), "ai-crew-sync");
}
