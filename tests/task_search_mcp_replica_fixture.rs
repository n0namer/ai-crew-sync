//! Fixtures shared by the isolated multi-replica MCP acceptance test.
#[path = "task_pagination_fixture.rs"]
mod pagination;

pub use pagination::{Fixture, cleanup, setup_fixture};
use ai_crew_sync::auth;
use sqlx::PgPool;
use uuid::Uuid;

pub const REPLICA_TASK_COUNT: usize = 7;
pub const REPLICA_SECRET: &[u8] = b"task-search-mcp-multi-replica-secret";

pub async fn issue_token(
    pool: &PgPool,
    agent_id: Uuid,
    label: &str,
) -> Result<String, sqlx::Error> {
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
    Ok(raw_token)
}

pub async fn seed_replica_tasks(fixture: &Fixture) -> Result<Vec<String>, sqlx::Error> {
    let mut tx = fixture.pool.begin().await?;
    let mut keys = Vec::with_capacity(REPLICA_TASK_COUNT);
    for ordinal in 0..REPLICA_TASK_COUNT {
        let key = format!("{}:replica-{ordinal:02}", fixture.task_prefix);
        sqlx::query(
            "INSERT INTO tasks (team_id, key, title, description, metadata, created_by, updated_at) VALUES ($1, $2, $3, $4, jsonb_build_object('replica_ordinal', $5), $6, TIMESTAMPTZ '2026-01-01 00:00:00+00')",
        )
        .bind(fixture.team_id)
        .bind(&key)
        .bind(format!("Replica cursor task {ordinal:02}"))
        .bind(format!("Replica recovery description {ordinal:02}"))
        .bind(ordinal as i32)
        .bind(fixture.agent_id)
        .execute(&mut *tx)
        .await?;
        keys.push(key);
    }
    tx.commit().await?;
    Ok(keys)
}

pub async fn task_state(
    pool: &PgPool,
    team_id: Uuid,
    key: &str,
) -> Result<(String, Option<Uuid>, Option<String>), sqlx::Error> {
    sqlx::query_as(
        "SELECT status::text, claimed_by, claimed_session FROM tasks WHERE team_id = $1 AND key = $2",
    )
    .bind(team_id)
    .bind(key)
    .fetch_one(pool)
    .await
}

#[cfg(test)]
mod tests {
    use super::REPLICA_SECRET;
    use ai_crew_sync::store::task_cursor::{
        TaskCursorContext, TaskCursorFilters, TaskOrder, CursorError, decode_cursor, encode_cursor,
    };

    #[test]
    fn nonidentical_replica_secret_rejects_a_signed_cursor() {
        let context = TaskCursorContext {
            team_id: "replica-team".to_owned(),
            filters: TaskCursorFilters::default(),
        };
        let cursor = encode_cursor(
            REPLICA_SECRET,
            &context,
            &TaskOrder::new(1, "replica-task"),
        )
        .expect("cursor should encode");
        assert_eq!(
            decode_cursor(b"different-replica-secret", &cursor, &context),
            Err(CursorError::InvalidSignature)
        );
    }
}
