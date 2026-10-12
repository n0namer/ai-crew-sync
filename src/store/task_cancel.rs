use super::*;

/// Cancellation is a distinct terminal outcome, never task completion.
pub struct CancelInput {
    pub key: String,
    pub reason: String,
    pub successor_key: Option<String>,
    pub evidence_refs: Vec<String>,
}

fn validate_cancel(input: &CancelInput) -> BusResult<(String, Option<String>, Vec<String>)> {
    let reason = input.reason.trim().to_ascii_uppercase();
    if !matches!(reason.as_str(), "DUPLICATE" | "SUPERSEDED" | "OBSOLETE" | "INVALID") {
        return Err(BusError::invalid("reason must be DUPLICATE, SUPERSEDED, OBSOLETE or INVALID"));
    }
    let successor = input.successor_key.as_deref().map(normalize_key).transpose()?;
    if matches!(reason.as_str(), "DUPLICATE" | "SUPERSEDED") && successor.is_none() {
        return Err(BusError::invalid("DUPLICATE/SUPERSEDED require successor_key"));
    }
    if successor.as_deref() == Some(input.key.trim()) {
        return Err(BusError::invalid("task cannot supersede itself"));
    }
    if input.evidence_refs.is_empty() || input.evidence_refs.len() > 16
        || input.evidence_refs.iter().any(|e| e.trim().is_empty() || e.len() > 2048) {
        return Err(BusError::invalid("provide 1..16 bounded, nonempty evidence_refs"));
    }
    Ok((reason, successor, input.evidence_refs.iter().map(|s| s.trim().to_owned()).collect()))
}

/// Atomic cancellation with preserved original task ID, history and successor reference.
/// Never cancels a live claim or unlocks unfinished downstream dependencies.
pub async fn cancel_task(pool: &PgPool, auth: &AuthCtx, input: CancelInput) -> BusResult<TaskInfo> {
    let key = normalize_key(&input.key)?;
    let (reason, successor, evidence) = validate_cancel(&input)?;
    let mut tx = pool.begin().await?;
    super::super::sessions::guard(&mut tx, auth).await?;

    // FOR UPDATE serializes with claim, complete and concurrent cancellations.
    let row: Option<(Uuid, String, Option<chrono::DateTime<chrono::Utc>>)> =
        sqlx::query_as("SELECT id, status, lease_expires_at FROM tasks WHERE team_id = $1 AND key = $2 FOR UPDATE")
        .bind(auth.team_id).bind(&key).fetch_optional(&mut *tx).await?;
    let Some((task_id, status, expiry)) = row else {
        return Err(BusError::not_found(format!("task '{key}'")));
    };
    if !matches!(status.as_str(), "open" | "claimed") {
        return Err(BusError::conflict(format!("task '{key}' already {status}")));
    }
    if status == "claimed" && expiry.is_none_or(|v| v > chrono::Utc::now()) {
        return Err(BusError::conflict("live claimant must release this task first"));
    }

    if let Some(ref successor_key) = successor {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT status FROM tasks WHERE team_id = $1 AND key = $2 FOR SHARE"
        ).bind(auth.team_id).bind(successor_key).fetch_optional(&mut *tx).await?;
        let Some((successor_status,)) = row else {
            return Err(BusError::not_found(format!("successor '{successor_key}'")));
        };
        if successor_status == "cancelled" {
            return Err(BusError::conflict("canonical successor is cancelled"));
        }
    }

    // Existing claim_task interprets cancelled dependencies as resolved.
    // Refuse cancellation until every nonterminal dependent is reconciled.
    let downstream: Vec<(String,)> = sqlx::query_as(
        "SELECT t.key FROM task_deps d JOIN tasks t ON t.id = d.task_id
         WHERE d.blocked_by_task_id = $1 AND t.team_id = $2
         AND t.status NOT IN ('done', 'cancelled') ORDER BY t.key LIMIT 8"
    ).bind(task_id).bind(auth.team_id).fetch_all(&mut *tx).await?;
    if !downstream.is_empty() {
        return Err(BusError::conflict(format!(
            "unfinished downstream tasks must be reconciled first: {}",
            downstream.iter().map(|t| t.0.as_str()).collect::<Vec<_>>().join(", ")
        )));
    }
    let detail = serde_json::json!({
        "reason": reason, "successor_key": successor, "evidence_refs": evidence,
        "actor": auth.agent_name, "session": auth.session
    });
    sqlx::query(
        "UPDATE tasks SET status = 'cancelled',
         claimed_by = NULL, claimed_session = NULL, claimed_at = NULL,
         lease_expires_at = NULL,
         metadata = jsonb_set(COALESCE(metadata, '{}'::jsonb), '{lifecycle_resolution}', $1::jsonb, true),
         updated_at = now() WHERE team_id = $2 AND id = $3"
    ).bind(&detail).bind(auth.team_id).bind(task_id).execute(&mut *tx).await?;
    let event_detail = serde_json::to_string(&detail)
        .map_err(|_| BusError::invalid("cancel event cannot be serialized"))?;
    log_event_tx(&mut tx, task_id, auth.agent_id, "cancelled", Some(&event_detail)).await?;
    tx.commit().await?;
    fetch_task(pool, auth, &key).await
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input(reason: &str, successor_key: Option<&str>, evidence_refs: Vec<&str>) -> CancelInput {
        CancelInput {
            key: "us-af:old".into(), reason: reason.into(),
            successor_key: successor_key.map(str::to_owned),
            evidence_refs: evidence_refs.into_iter().map(str::to_owned).collect()
        }
    }
    #[test]
    fn requires_canonical_replacement_for_duplicate_and_superseded() {
        assert!(validate_cancel(&input("DUPLICATE", None, vec!["receipt"])).is_err());
        assert!(validate_cancel(&input("SUPERSEDED", None, vec!["receipt"])).is_err());
        assert!(validate_cancel(&input("DUPLICATE", Some("us-af:old"), vec!["receipt"])).is_err());
        assert!(validate_cancel(&input("DUPLICATE", Some("us-af:new"), vec!["receipt"])).is_ok());
    }
    #[test]
    fn rejects_unproven_or_invalid_cancellation() {
        assert!(validate_cancel(&input("OTHER", None, vec!["receipt"])).is_err());
        assert!(validate_cancel(&input("OBSOLETE", None, vec![])).is_err());
        assert!(validate_cancel(&input("INVALID", None, vec![""])).is_err());
        assert!(validate_cancel(&input("OBSOLETE", None, vec!["git:evidence"])).is_ok());
    }
}
