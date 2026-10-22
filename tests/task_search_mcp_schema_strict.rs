#![allow(clippy::too_many_lines)]

#[path = "task_pagination_fixture.rs"]
mod fixture;

use fixture::{Fixture, cleanup, setup_fixture};
use rmcp::{
    ServiceExt,
    model::{CallToolRequestParams, ClientConfig},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde_json::{Map, Value, json};
use ai_crew_sync::serve::{self, ServeOptions};

fn arguments(value: Value) -> Map<String, Value> {
    value
        .as_object()
        .cloned()
        .expect("MCP tool arguments must be a JSON object")
}

fn schema_has_only_type(schema: &Value, expected: &str, nullable: bool) -> bool {
    if let Some(types) = schema.get("type").and_then(Value::as_array) {
        let mut found_expected = false;
        for value in types {
            let Some(value) = value.as_str() else {
                return false;
            };
            if value == expected {
                found_expected = true;
            } else if !(nullable && value == "null") {
                return false;
            }
        }
        return found_expected && nullable;
    }
    if let Some(actual) = schema.get("type").and_then(Value::as_str) {
        return actual == expected && !nullable;
    }
    if nullable {
        let Some(variants) = schema.get("anyOf").and_then(Value::as_array) else {
            return false;
        };
        let mut found_expected = false;
        let mut found_null = false;
        for variant in variants {
            match variant.get("type").and_then(Value::as_str) {
                Some(value) if value == expected => found_expected = true,
                Some("null") => found_null = true,
                _ => return false,
            }
        }
        return found_expected && found_null;
    }
    false
}

async fn seed_tasks(fixture: &Fixture) -> Result<(), sqlx::Error> {
    let mut tx = fixture.pool.begin().await?;
    sqlx::query(
        "INSERT INTO tasks (team_id, key, title, description, created_by) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(fixture.team_id)
    .bind(format!("{}:open", fixture.task_prefix))
    .bind("Strict schema open task")
    .bind("legacy response count")
    .bind(fixture.agent_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO tasks (team_id, key, title, description, status, claimed_by, claimed_at, lease_expires_at, created_by) VALUES ($1, $2, $3, $4, 'claimed', $5, now(), now() + interval '1 hour', $5)",
    )
    .bind(fixture.team_id)
    .bind(format!("{}:claimed", fixture.task_prefix))
    .bind("Strict schema claimed task")
    .bind("legacy response cursor")
    .bind(fixture.other_agent_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await
}

async fn run_strict_schema_check() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = setup_fixture().await?;
    let result = async {
        seed_tasks(&fixture).await?;

        let raw_token = ai_crew_sync::auth::generate_token();
        sqlx::query(
            "INSERT INTO api_tokens (agent_id, token_hash, prefix, label) VALUES ($1, $2, $3, $4)",
        )
        .bind(fixture.agent_id)
        .bind(ai_crew_sync::auth::hash_token(&raw_token))
        .bind(ai_crew_sync::auth::token_prefix(&raw_token))
        .bind("strict MCP schema test")
        .execute(&fixture.pool)
        .await?;

        let cancellation = tokio_util::sync::CancellationToken::new();
        let router = serve::build_router(
            fixture.pool.clone(),
            &ServeOptions {
                bind: "127.0.0.1:0".to_owned(),
                allowed_hosts: Vec::new(),
                allowed_origins: Vec::new(),
                max_request_bytes: serve::DEFAULT_MAX_REQUEST_BYTES,
                rate_limit_per_minute: 0,
                dashboard_secret: b"strict-schema-test-secret".to_vec(),
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

        let mut transport_config =
            StreamableHttpClientTransportConfig::with_uri(format!("http://{address}/mcp"));
        transport_config.auth_header = Some(raw_token);
        transport_config.allow_stateless = true;
        let client = ClientConfig::default()
            .serve(StreamableHttpClientTransport::from_config(transport_config))
            .await?;

        let tools = client.list_all_tools().await?;
        let list_tasks = tools
            .iter()
            .find(|tool| tool.name == "list_tasks")
            .expect("tools/list must expose list_tasks");
        assert_eq!(
            tools.iter().filter(|tool| tool.name == "list_tasks").count(),
            1
        );
        assert!(
            tools.iter().all(|tool| tool.name != "search_tasks"),
            "search must remain additive on list_tasks"
        );

        let schema = serde_json::to_value(&list_tasks.input_schema)?;
        let properties = schema
            .get("properties")
            .and_then(Value::as_object)
            .expect("list_tasks input schema must expose properties");
        let mut property_names = properties.keys().cloned().collect::<Vec<_>>();
        property_names.sort();
        assert_eq!(
            property_names,
            [
                "cursor",
                "limit",
                "mine_only",
                "project_prefix",
                "search",
                "search_fields",
                "search_language",
                "search_mode",
                "status",
            ]
        );

        for field in ["status", "project_prefix", "cursor", "search"] {
            assert!(
                schema_has_only_type(&properties[field], "string", true),
                "{field} must be an optional string schema: {}",
                properties[field]
            );
        }
        for field in ["search_mode", "search_fields", "search_language"] {
            assert!(
                schema_has_only_type(&properties[field], "string", false),
                "{field} must be a string schema: {}",
                properties[field]
            );
            assert!(
                !schema["required"]
                    .as_array()
                    .unwrap_or(&Vec::new())
                    .iter()
                    .any(|value| value.as_str() == Some(field)),
                "{field} must remain optional for old clients"
            );
        }
        assert!(schema_has_only_type(&properties["mine_only"], "boolean", false));
        assert!(schema_has_only_type(&properties["limit"], "integer", false));

        let legacy_result = client
            .call_tool(
                CallToolRequestParams::new("list_tasks").with_arguments(arguments(json!({
                    "status": "any",
                    "mine_only": false,
                    "project_prefix": fixture.task_prefix,
                }))),
            )
            .await?;
        assert_ne!(legacy_result.is_error, Some(true));
        let legacy_json = legacy_result
            .structured_content
            .clone()
            .expect("legacy list_tasks must return structured JSON");
        assert!(legacy_json["tasks"].is_array());
        assert!(legacy_json["open"].is_i64() || legacy_json["open"].is_u64());
        assert!(legacy_json["claimed"].is_i64() || legacy_json["claimed"].is_u64());
        assert!(legacy_json["has_more"].is_boolean());
        assert!(legacy_json["next_cursor"].is_null() || legacy_json["next_cursor"].is_string());
        assert_eq!(legacy_json["open"], 1);
        assert_eq!(legacy_json["claimed"], 1);
        assert_eq!(legacy_json["has_more"], false);
        assert!(legacy_json["next_cursor"].is_null());

        let default_search_result = client
            .call_tool(
                CallToolRequestParams::new("list_tasks").with_arguments(arguments(json!({
                    "search": "legacy response",
                    "limit": 10,
                    "project_prefix": fixture.task_prefix,
                }))),
            )
            .await?;
        assert_ne!(default_search_result.is_error, Some(true));
        let default_search_json = default_search_result
            .structured_content
            .clone()
            .expect("default search list_tasks must return structured JSON");
        let mut default_keys = default_search_json["tasks"]
            .as_array()
            .expect("default search response must include tasks")
            .iter()
            .map(|task| {
                task["key"]
                    .as_str()
                    .expect("task key must be a string")
                    .to_owned()
            })
            .collect::<Vec<_>>();
        let mut expected_keys = vec![
            format!("{}:open", fixture.task_prefix),
            format!("{}:claimed", fixture.task_prefix),
        ];
        default_keys.sort_unstable();
        expected_keys.sort_unstable();
        assert_eq!(default_keys, expected_keys);
        assert_eq!(default_search_json["open"], 1);
        assert_eq!(default_search_json["claimed"], 1);
        assert_eq!(default_search_json["has_more"], false);
        assert!(default_search_json["next_cursor"].is_null());

        let search_result = client
            .call_tool(
                CallToolRequestParams::new("list_tasks").with_arguments(arguments(json!({
                    "search": "strict schema",
                    "search_mode": "contains",
                    "search_fields": "title",
                    "search_language": "simple",
                    "limit": 10,
                    "project_prefix": fixture.task_prefix,
                }))),
            )
            .await?;
        assert_ne!(search_result.is_error, Some(true));
        let search_json = search_result
            .structured_content
            .clone()
            .expect("search list_tasks must return structured JSON");
        assert_eq!(search_json["tasks"].as_array().map(Vec::len), Some(2));
        assert_eq!(search_json["open"], 1);
        assert_eq!(search_json["claimed"], 1);
        assert_eq!(search_json["has_more"], false);
        assert!(search_json["next_cursor"].is_null());

        for invalid in [
            json!({ "search": "term", "search_mode": "unsupported" }),
            json!({ "search": "term", "search_fields": "unsupported" }),
            json!({ "search": "term", "search_language": "unsupported" }),
        ] {
            let error = client
                .call_tool(
                    CallToolRequestParams::new("list_tasks").with_arguments(arguments(invalid)),
                )
                .await
                .expect_err("invalid enum parameters must fail at the protocol boundary");
            let error_debug = format!("{error:?}");
            assert!(
                error_debug.contains("-32602"),
                "invalid enum parameters must return MCP InvalidParams (-32602): {error_debug}"
            );
        }

        let typed_result = client
            .call_tool(
                CallToolRequestParams::new("list_tasks").with_arguments(arguments(json!({
                    "search": "term",
                    "search_mode": 7,
                }))),
            )
            .await?;
        assert_eq!(typed_result.is_error, Some(true));
        assert!(typed_result.structured_content.is_none());
        let typed_error = format!("{:?}", typed_result.content);
        assert!(
            typed_error.contains("invalid type") && typed_error.contains("expected a string"),
            "typed parameter errors must be sanitized and explain the expected string type: {typed_error}"
        );
        assert!(!typed_error.contains("tasks"));
        assert!(!typed_error.contains("SELECT"));

        cancellation.cancel();
        server.abort();
        let _ = client.cancel().await;
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    cleanup(fixture).await?;
    result
}

#[tokio::test]
#[ignore = "requires CREWSYNC_ENABLE_LARGE_FIXTURE=1 and disposable TEST_DATABASE_URL"]
async fn mcp_tools_list_exposes_strict_list_tasks_schema_and_wire_errors() {
    run_strict_schema_check()
        .await
        .expect("real MCP service and disposable PostgreSQL fixture");
}
