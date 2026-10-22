use std::collections::{HashMap, HashSet};

use sqlx::{PgPool, Row};
use uuid::Uuid;

use super::tasks;
use crate::{
    auth::AuthCtx,
    error::{BusError, BusResult},
    model::ClaimResult,
};

const MAX_ITEMS: usize = 10_000;

fn validate_project(project: &str) -> BusResult<()> {
    let bytes = project.as_bytes();
    if !(2..=80).contains(&bytes.len())
        || !bytes[0].is_ascii_lowercase()
        || !bytes.iter().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
    {
        return Err(BusError::invalid("project must be a canonical lowercase ID"));
    }
    Ok(())
}

fn require_coordinator(auth: &AuthCtx) -> BusResult<()> {
    if auth.agent_name != "coordinator" {
        return Err(BusError::forbidden("only the authenticated coordinator may approve or claim a project queue"));
    }
    Ok(())
}

fn validate_keys(project: &str, keys: &[String]) -> BusResult<()> {
    if keys.len() > MAX_ITEMS {
        return Err(BusError::invalid("ordered project queue has too many tasks"));
    }
    let prefix = format!("{project}:");
    let mut seen = HashSet::new();
    for key in keys {
        if key.len() > 128 || !key.starts_with(&prefix) || key.len() <= prefix.len() || !seen.insert(key) {
            return Err(BusError::invalid("queue keys must be unique existing tasks in the named project"));
        }
    }
    Ok(())
}

/// Commit a full project order with optimistic revision fencing.
/// Empty lists are valid and pause a project without touching its OPEN backlog.
/// All mutations (and the history entry) are atomic in one PostgreSQL transaction.
pub async fn replace_order(
    pool: &PgPool,
    auth: &AuthCtx,
    project: &str,
    expected_revision: i64,
    keys: &[String],
) -> BusResult<i64> {
    validate_project(project)?;
    require_coordinator(auth)?;
    validate_keys(project, keys)?;
    if expected_revision < 0 {
        return Err(BusError::invalid("expected revision must be nonnegative"));
    }

    let mut tx = pool.begin().await?;
    super::sessions::guard(&mut tx, auth).await?;
    sqlx::query("INSERT INTO project_task_orders(team_id,project_key) VALUES($1,$2) ON CONFLICT DO NOTHING")
        .bind(auth.team_id)
        .bind(project)
        .execute(&mut *tx)
        .await?;

    let current: i64 = sqlx::query_scalar(
        "SELECT revision FROM project_task_orders WHERE team_id=$1 AND project_key=$2 FOR UPDATE",
    )
    .bind(auth.team_id)
    .bind(project)
    .fetch_one(&mut *tx)
    .await?;

    if current != expected_revision {
        return Err(BusError::conflict(format!(
            "project queue revision changed; expected {expected_revision}, current {current}; refresh before replanning"
        )));
    }
    if current == i64::MAX {
        return Err(BusError::conflict("project queue revision is exhausted"));
    }

    // Existing running tasks may not be invalidated by a replan.
    let active_old: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM project_task_order_entries e
         JOIN tasks t ON t.id=e.task_id AND t.team_id=e.team_id
         WHERE e.team_id=$1 AND e.project_key=$2
           AND t.status='claimed' AND (t.lease_expires_at IS NULL OR t.lease_expires_at>now())",
    )
    .bind(auth.team_id)
    .bind(project)
    .fetch_one(&mut *tx)
    .await?;
    if active_old > 0 {
        return Err(BusError::conflict("project has live claims; complete or release them before changing order"));
    }

    let mut ids = HashMap::<String, Uuid>::new();
    if !keys.is_empty() {
        // Lock new task rows too, so an in-flight claim cannot slip through validation.
        let rows: Vec<(Uuid, String, String, Option<chrono::DateTime<chrono::Utc>>)> = sqlx::query_as(
            "SELECT id,key,status,lease_expires_at FROM tasks
             WHERE team_id=$1 AND key=ANY($2) FOR UPDATE",
        )
        .bind(auth.team_id)
        .bind(keys)
        .fetch_all(&mut *tx)
        .await?;
        if rows.len() != keys.len() {
            return Err(BusError::invalid("all ordered tasks must already exist in this team"));
        }
        for (id, key, status, expires_at) in rows {
            if matches!(status.as_str(), "cancelled" | "superseded") {
                return Err(BusError::invalid(
                    "proposed order contains a cancelled or superseded task; remove terminal work and submit an explicit replan",
                ));
            }
            if status == "claimed" && expires_at.is_none_or(|expiry| expiry > chrono::Utc::now()) {
                return Err(BusError::conflict("proposed order contains a task with an active claim"));
            }
            ids.insert(key, id);
        }

        let positions: HashMap<&str, usize> = keys.iter().enumerate().map(|(i, key)| (key.as_str(), i)).collect();
        let dependencies: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT current_task.key, predecessor.key, predecessor.status
             FROM task_deps dep
             JOIN tasks current_task ON current_task.id=dep.task_id
             JOIN tasks predecessor ON predecessor.id=dep.blocked_by_task_id
             WHERE current_task.team_id=$1 AND predecessor.team_id=$1
               AND current_task.key=ANY($2)",
        )
        .bind(auth.team_id)
        .bind(keys)
        .fetch_all(&mut *tx)
        .await?;
        let prefix = format!("{project}:");
        for (current_key, predecessor_key, predecessor_status) in dependencies {
            let Some(&current_pos) = positions.get(current_key.as_str()) else { continue; };
            match positions.get(predecessor_key.as_str()) {
                Some(&predecessor_pos) if predecessor_pos >= current_pos => {
                    return Err(BusError::invalid(format!(
                        "{current_key} depends on {predecessor_key}, which is not earlier in the approved queue"
                    )));
                }
                None if predecessor_key.starts_with(&prefix)
                    && predecessor_status != "done" => {
                    return Err(BusError::invalid(format!(
                        "{current_key} has unfinished same-project prerequisite {predecessor_key} outside the approved queue"
                    )));
                }
                _ => {}
            }
        }
    }

    sqlx::query("DELETE FROM project_task_order_entries WHERE team_id=$1 AND project_key=$2")
        .bind(auth.team_id)
        .bind(project)
        .execute(&mut *tx)
        .await?;
    for (i, key) in keys.iter().enumerate() {
        sqlx::query(
            "INSERT INTO project_task_order_entries(team_id,project_key,task_id,position)
             VALUES($1,$2,$3,$4)",
        )
        .bind(auth.team_id)
        .bind(project)
        .bind(ids[key])
        .bind(i as i32 + 1)
        .execute(&mut *tx)
        .await?;
    }

    let next_revision = current + 1;
    sqlx::query(
        "UPDATE project_task_orders SET revision=$3,updated_at=now()
         WHERE team_id=$1 AND project_key=$2",
    )
    .bind(auth.team_id)
    .bind(project)
    .bind(next_revision)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO project_task_order_revisions
         (team_id,project_key,revision,task_keys,approved_by,approved_agent,approved_session)
         VALUES($1,$2,$3,$4,$5,$6,$7)",
    )
    .bind(auth.team_id)
    .bind(project)
    .bind(next_revision)
    .bind(keys)
    .bind(auth.agent_id)
    .bind(&auth.agent_name)
    .bind(&auth.session)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(next_revision)
}

/// Fail-closed strict queue head. No fallback to the global FIFO and no
/// skipping blocked/claimed tasks to consume unrelated backlog.
pub async fn claim_next_ordered(
    pool: &PgPool,
    auth: &AuthCtx,
    project: &str,
    lease_seconds: Option<i64>,
) -> BusResult<ClaimResult> {
    validate_project(project)?;
    require_coordinator(auth)?;
    let lease = lease_seconds.unwrap_or(900).clamp(30, 86_400);
    let mut tx = pool.begin().await?;
    super::sessions::guard(&mut tx, auth).await?;

    // A row lock serializes both project claims and ordered-plan revisions.
    let revision: Option<i64> = sqlx::query_scalar(
        "SELECT revision FROM project_task_orders
         WHERE team_id=$1 AND project_key=$2 FOR UPDATE SKIP LOCKED",
    )
    .bind(auth.team_id)
    .bind(project)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(revision) = revision else {
        return Ok(ClaimResult { claimed: false, task: None,
            reason: Some("project queue is absent or currently locked".into()) });
    };

    let head: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT t.id,t.key FROM project_task_order_entries e
         JOIN tasks t ON t.id=e.task_id AND t.team_id=e.team_id
         WHERE e.team_id=$1 AND e.project_key=$2
           AND t.status <> 'done'
         ORDER BY e.position LIMIT 1",
    )
    .bind(auth.team_id)
    .bind(project)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((id, key)) = head else {
        return Ok(ClaimResult { claimed: false, task: None,
            reason: Some("approved project queue has no unfinished tasks".into()) });
    };

    let updated: Option<Uuid> = sqlx::query_scalar(
        "UPDATE tasks SET status='claimed',claimed_by=$2,claimed_session=$4,claimed_at=now(),
         lease_expires_at=now()+make_interval(secs=>$3),updated_at=now()
         WHERE id=$1 AND team_id=$5
           AND (status='open' OR (status='claimed' AND lease_expires_at<now()))
           AND NOT EXISTS (
             SELECT 1 FROM task_deps dep
             JOIN tasks prerequisite ON prerequisite.id=dep.blocked_by_task_id
             WHERE dep.task_id=$1 AND prerequisite.status <> 'done'
           )
         RETURNING id",
    )
    .bind(id)
    .bind(auth.agent_id)
    .bind(lease as f64)
    .bind(&auth.session)
    .bind(auth.team_id)
    .fetch_optional(&mut *tx)
    .await?;
    if updated.is_none() {
        return Ok(ClaimResult { claimed: false, task: None,
            reason: Some("first unfinished ordered task is claimed or blocked; no bypass".into()) });
    }
    sqlx::query(
        "INSERT INTO task_events(task_id,agent_id,event,detail) VALUES($1,$2,'claimed',$3)",
    )
    .bind(id)
    .bind(auth.agent_id)
    .bind(format!("project:{project} order-revision:{revision}"))
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    let detail = tasks::get_task(pool, auth, &key).await?;
    Ok(ClaimResult { claimed: true, task: Some(detail.task), reason: None })
}

/// Return one consistent snapshot; a concurrent reorder cannot pair an old
/// revision with a new list.
pub async fn get_order(
    pool: &PgPool,
    auth: &AuthCtx,
    project: &str,
) -> BusResult<(i64, Vec<String>)> {
    validate_project(project)?;
    let mut tx = pool.begin().await?;
    super::sessions::guard(&mut tx, auth).await?;
    let current: Option<i64> = sqlx::query_scalar(
        "SELECT revision FROM project_task_orders
         WHERE team_id=$1 AND project_key=$2 FOR SHARE",
    )
    .bind(auth.team_id)
    .bind(project)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(revision) = current else { return Ok((0, Vec::new())); };
    let keys: Vec<String> = sqlx::query(
        "SELECT t.key FROM project_task_order_entries e
         JOIN tasks t ON t.id=e.task_id AND t.team_id=e.team_id
         WHERE e.team_id=$1 AND e.project_key=$2 ORDER BY e.position",
    )
    .bind(auth.team_id)
    .bind(project)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .map(|row| row.get::<String, _>("key"))
    .collect();
    tx.commit().await?;
    Ok((revision, keys))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn project_id_rejects_ambiguous_and_unsafe_names() {
        for project in ["", "A", "Gocore", "-gocore", "gocore/other", "go_core", "g", "🔥"] {
            assert!(validate_project(project).is_err(), "{project}");
        }
        for project in ["gocore", "ai-crew-sync", "swe-af", "ultra"] {
            assert!(validate_project(project).is_ok(), "{project}");
        }
    }
    #[test]
    fn queue_rejects_mixed_or_duplicate_task_keys() {
        assert!(validate_keys("gocore", &["gocore:A".into(), "gocore:A".into()]).is_err());
        assert!(validate_keys("gocore", &["ultra:A".into()]).is_err());
        assert!(validate_keys("gocore", &[]).is_ok());
    }
}
