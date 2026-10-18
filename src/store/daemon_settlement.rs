//! Strict, separately gated Daemon settlement. NOT the legacy complete_task.
 //! This API is deliberately store-internal until independent QA proof
 //! registration and server-side cryptographic verification are implemented.
use sqlx::PgPool;
use uuid::Uuid;
use crate::{auth::AuthCtx, error::{BusError, BusResult}};

pub struct StrictSettlement {
    pub key: String,
    pub expected_revision: i64,
    pub source_sha: String,
    pub attempt_id: String,
    pub owner_generation: i64,
    pub fencing_token: String,
    pub qa_digest: String,
    pub result: String,
}

fn valid_hex(value: &str, len: usize) -> bool {
    value.len() == len && value.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}
pub fn validate(input: &StrictSettlement) -> BusResult<()> {
    if input.key.trim().is_empty() || input.key.len() > 256 ||
        input.expected_revision < 1 || input.owner_generation < 1 ||
        !valid_hex(&input.source_sha, 40) || !valid_hex(&input.qa_digest, 64) ||
        input.attempt_id.trim().is_empty() || input.attempt_id.len() > 256 ||
        input.fencing_token.trim().is_empty() || input.fencing_token.len() > 256 ||
        input.result.trim().is_empty() || input.result.len() > 65536 {
        return Err(BusError::invalid("strict Daemon settlement binding invalid"));
    }
    Ok(())
}

/// Not exposed as MCP. QA acceptance rows must be written exclusively through
/// an independently authenticated reviewer API (not implemented here).
/// Without such a row the SQL must fail closed. No caller-supplied boolean
/// can ever substitute for the DB proof.
pub async fn settle_strict(
    pool: &PgPool,
    auth: &AuthCtx,
    input: &StrictSettlement,
) -> BusResult<Uuid> {
    validate(input)?;
    let mut tx = pool.begin().await?;
    super::sessions::guard(&mut tx, auth).await?;

    // Lock order for every future issuer and settler: task -> fence -> QA.
    // This protects the proof bindings from changing between UPDATE and
    // consumption; issuer/reviewer ports MUST adopt the same lock order.
    let task_row: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM tasks WHERE team_id=$1 AND key=$2 FOR UPDATE"
    )
    .bind(auth.team_id)
    .bind(&input.key)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((locked_task_id,)) = task_row else {
        tx.rollback().await?;
        return Err(BusError::conflict("strict Daemon task not found in team"));
    };
    let fence_row: Option<(Uuid,)> = sqlx::query_as(
        "SELECT task_id FROM daemon_execution_fences WHERE team_id=$1 AND task_id=$2 FOR UPDATE"
    )
    .bind(auth.team_id)
    .bind(locked_task_id)
    .fetch_optional(&mut *tx)
    .await?;
    let qa_row: Option<(Uuid,)> = sqlx::query_as(
        "SELECT task_id FROM daemon_qa_acceptances WHERE team_id=$1 AND task_id=$2 AND attempt_id=$3 FOR UPDATE"
    )
    .bind(auth.team_id)
    .bind(locked_task_id)
    .bind(&input.attempt_id)
    .fetch_optional(&mut *tx)
    .await?;
    if fence_row.is_none() || qa_row.is_none() {
        tx.rollback().await?;
        return Err(BusError::conflict("strict Daemon native fence or QA proof missing"));
    }

    // The actual task write and QA proof consumption share one transaction.
    // Only a fresh holder, exact revision/fence and independent QA may pass.
    let id: Option<(Uuid,)> = sqlx::query_as(
        r#"
        UPDATE tasks t
        SET status = 'done', result = $1,
            lease_expires_at = NULL, updated_at = now()
        FROM daemon_execution_fences f, daemon_qa_acceptances q
        WHERE t.team_id = $2 AND t.key = $3 AND t.status = 'claimed'
          AND t.claimed_by = $4 AND COALESCE(t.claimed_session, '') = $5
          AND t.lease_expires_at > clock_timestamp()
          AND t.revision = $6
          AND t.description IS NOT NULL
          AND t.description::jsonb->>'git_head' = $7
          AND f.team_id = t.team_id AND f.task_id = t.id
          AND f.source_revision = t.revision AND f.source_sha = $7
          AND f.attempt_id = $8 AND f.owner_generation = $9
          AND f.fencing_token = $10 AND f.owner_agent_id = $4
          AND f.owner_session = $5
          AND q.team_id = t.team_id AND q.task_id = t.id
          AND q.source_revision = f.source_revision AND q.source_sha = f.source_sha
          AND q.attempt_id = f.attempt_id
          AND q.owner_generation = f.owner_generation
          AND q.fencing_token = f.fencing_token
          AND q.accepted = true AND q.consumed_at IS NULL
          AND q.expires_at > clock_timestamp()
          AND q.review_digest = $11
          AND q.reviewer_agent_id <> $4
          AND q.reviewer_session <> $5
        RETURNING t.id
        "#,
    )
    .bind(&input.result)
    .bind(auth.team_id)
    .bind(&input.key)
    .bind(auth.agent_id)
    .bind(&auth.session)
    .bind(input.expected_revision)
    .bind(&input.source_sha)
    .bind(&input.attempt_id)
    .bind(input.owner_generation)
    .bind(&input.fencing_token)
    .bind(&input.qa_digest)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((task_id,)) = id else {
        tx.rollback().await?;
        return Err(BusError::conflict("strict Daemon owner/source/fence/QA CAS rejected"));
    };
    let consumed = sqlx::query(
        r#"UPDATE daemon_qa_acceptances
        SET consumed_at = now()
        WHERE team_id = $1 AND task_id = $2 AND attempt_id = $3
          AND source_revision = $4 AND source_sha = $5
          AND owner_generation = $6 AND fencing_token = $7
          AND review_digest = $8 AND accepted = true AND consumed_at IS NULL"#,
    )
    .bind(auth.team_id)
    .bind(task_id)
    .bind(&input.attempt_id)
    .bind(input.expected_revision)
    .bind(&input.source_sha)
    .bind(input.owner_generation)
    .bind(&input.fencing_token)
    .bind(&input.qa_digest)
    .execute(&mut *tx)
    .await?;
    if consumed.rows_affected() != 1 {
        tx.rollback().await?;
        return Err(BusError::conflict("strict Daemon QA receipt already consumed"));
    }
    sqlx::query(
        "INSERT INTO task_events (task_id, agent_id, event, detail) VALUES ($1,$2,'daemon_strict_completed',$3)"
    )
    .bind(task_id)
    .bind(auth.agent_id)
    .bind(&input.attempt_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(task_id)
}

#[cfg(test)]
mod tests {
    use super::{StrictSettlement,validate};
    fn sample() -> StrictSettlement {
        StrictSettlement {key:"gocore:test".into(),expected_revision:2,
            source_sha:"a".repeat(40),attempt_id:"attempt-1".into(),
            owner_generation:1,fencing_token:"fence-1".into(),
            qa_digest:"b".repeat(64),result:"candidate independently accepted".into()}
    }
    #[test]
    fn input_validation_rejects_missing_fences_and_sha() {
        let mut v=sample();assert!(validate(&v).is_ok());
        v.fencing_token.clear();assert!(validate(&v).is_err());
        v=sample();v.source_sha="WRONG".into();assert!(validate(&v).is_err());
        v=sample();v.qa_digest="a".repeat(63);assert!(validate(&v).is_err());
        v=sample();v.owner_generation=0;assert!(validate(&v).is_err());
    }
}
