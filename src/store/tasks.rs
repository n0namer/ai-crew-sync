use sqlx::{AssertSqlSafe, PgPool};
use uuid::Uuid;

use crate::{
    auth::AuthCtx,
    error::{BusError, BusResult},
    model::{
        ClaimResult, TaskDetail, TaskEventInfo, TaskInfo, TaskList, UpdateTaskResult, ts, ts_opt,
    },
};

#[path = "task_cursor.rs"]
mod task_cursor;
use task_cursor::{
    CursorLifetime, TaskCursorContext, TaskCursorFilters, TaskOrder, decode_cursor_at,
    encode_cursor_with_lifetime,
};

const DEFAULT_LEASE_SECS: i64 = 900; // 15 minutes
const MAX_LEASE_SECS: i64 = 86_400;
// Keep list requests bounded while allowing callers to page through larger inventories.
// This raises only the legacy clamp; the API's default page size remains 50 and
// effective status/lease calculations remain governed by EFFECTIVE_STATUS.
const MAX_LIMIT: i64 = 10_000;

/// A title is a handle a human recognises in a list, not a description.
const MAX_TITLE_BYTES: usize = 512;
/// Descriptions and results carry context, but a task is an index entry, not
/// a document: 64 KiB is generous for both and deliberately smaller than the
/// 1 MiB a message body allows. A payload larger than this belongs in an
/// attachment on the task.
const MAX_DESCRIPTION_BYTES: usize = 64 * 1024;
const MAX_RESULT_BYTES: usize = 64 * 1024;
/// A transition reason is durable audit context, not an unbounded transcript.
const MAX_TRANSITION_REASON_BYTES: usize = 4 * 1024;
/// A pipeline with more upstream tasks than this wants restructuring, and an
/// unbounded list is one INSERT per entry.
const MAX_DEPENDENCIES: usize = 32;

#[derive(sqlx::FromRow)]
struct TaskRow {
    revision: i64,
    key: String,
    title: String,
    description: Option<String>,
    status: String,
    status_rank: i32,
    cancellation_reason: Option<String>,
    superseded_by: Option<String>,
    supersede_reason: Option<String>,
    depends_on: Vec<String>,
    blocked: bool,
    claimed_by: Option<String>,
    claimed_session: Option<String>,
    claimed_at: Option<chrono::DateTime<chrono::Utc>>,
    lease_expires_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Decided by the database clock, the same one the writers use, so a
    /// read never disagrees with the claim that follows it.
    lease_expired: bool,
    result: Option<String>,
    metadata: serde_json::Value,
    attachments: serde_json::Value,
    created_by: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<TaskRow> for TaskInfo {
    fn from(r: TaskRow) -> Self {
        // A lapsed lease is an open task. The writers have always treated it
        // so (anyone may claim it); a read that still said "claimed by marta"
        // sent the reader to wait for a holder that no longer exists, or to
        // trust a list of open tasks that left this one out. Who let it lapse
        // stays visible, in its own field.
        if r.lease_expired {
            return TaskInfo {
                revision: r.revision,
                key: r.key,
                title: r.title,
                description: r.description,
                status: "open".to_owned(),
                cancellation_reason: r.cancellation_reason,
                superseded_by: r.superseded_by,
                supersede_reason: r.supersede_reason,
                depends_on: r.depends_on,
                blocked: r.blocked,
                claimed_by: None,
                claimed_session: None,
                claimed_at: None,
                lease_expires_at: None,
                lease_expired: true,
                lease_seconds_remaining: None,
                lapsed_holder: r.claimed_by,
                result: r.result,
                metadata: r.metadata,
                attachments: serde_json::from_value(r.attachments).unwrap_or_default(),
                created_by: r.created_by,
                created_at: ts(r.created_at),
                updated_at: ts(r.updated_at),
            };
        }
        let now = chrono::Utc::now();
        // Seconds, not a timestamp: an error that says "expires in 240s" tells
        // the caller whether waiting is an option; an RFC 3339 instant makes it
        // do the arithmetic first.
        let lease_seconds_remaining = r
            .lease_expires_at
            .filter(|_| r.status == "claimed")
            .map(|e| (e - now).num_seconds().max(0));
        TaskInfo {
            revision: r.revision,
            key: r.key,
            title: r.title,
            description: r.description,
            status: r.status,
            cancellation_reason: r.cancellation_reason,
            superseded_by: r.superseded_by,
            supersede_reason: r.supersede_reason,
            depends_on: r.depends_on,
            blocked: r.blocked,
            claimed_by: r.claimed_by,
            // '' and NULL both mean the shared session: NULL is a claim taken
            // before sessions existed, by a client that sent no header.
            claimed_session: r.claimed_session.filter(|s| !s.is_empty()),
            claimed_at: ts_opt(r.claimed_at),
            lease_expires_at: ts_opt(r.lease_expires_at),
            lease_expired: false,
            lease_seconds_remaining,
            lapsed_holder: None,
            result: r.result,
            metadata: r.metadata,
            attachments: serde_json::from_value(r.attachments).unwrap_or_default(),
            created_by: r.created_by,
            created_at: ts(r.created_at),
            updated_at: ts(r.updated_at),
        }
    }
}

const TASK_SELECT: &str = r#"
    SELECT t.id,
           t.revision,
           t.key,
           t.title,
           t.description,
           t.status,
           t.cancellation_reason,
           successor.key AS superseded_by,
           t.supersede_reason,
           CASE
               WHEN t.status = 'claimed' AND t.lease_expires_at <= now() THEN 1
               WHEN t.status = 'claimed' THEN 0
               WHEN t.status = 'open' THEN 1
               ELSE 2
           END AS status_rank,
           COALESCE(
               (SELECT array_agg(d.key ORDER BY d.key)
                FROM task_deps td JOIN tasks d ON d.id = td.blocked_by_task_id
                WHERE td.task_id = t.id),
               '{}'
           ) AS depends_on,
           EXISTS (
               SELECT 1
               FROM task_deps td JOIN tasks d ON d.id = td.blocked_by_task_id
               WHERE td.task_id = t.id AND d.status <> 'done'
           ) AS blocked,
           cb.name AS claimed_by,
           t.claimed_session,
           t.claimed_at,
           t.lease_expires_at,
           -- A claim with no expiry (a row from before leases had one) is a
           -- live claim, not a lapsed one: NULL here would not decode.
           COALESCE(t.status = 'claimed' AND t.lease_expires_at <= now(), false) AS lease_expired,
           t.result,
           t.metadata,
           COALESCE(
               (SELECT json_agg(json_build_object(
                           'id', a.id, 'filename', a.filename,
                           'content_type', a.content_type, 'size_bytes', a.size_bytes)
                       ORDER BY a.id)
                FROM attachments a WHERE a.task_id = t.id),
               '[]'::json
           ) AS attachments,
           crb.name AS created_by,
           t.created_at,
           t.updated_at
    FROM tasks t
    LEFT JOIN tasks successor ON successor.id = t.superseded_by_task_id
    LEFT JOIN agents cb  ON cb.id = t.claimed_by
    LEFT JOIN agents crb ON crb.id = t.created_by
"#;

/// The status a reader is told, which is the one the writers act on: a
/// claim whose lease lapsed is open, whatever the row still says.
const EFFECTIVE_STATUS: &str =
    "CASE WHEN t.status = 'claimed' AND t.lease_expires_at <= now() THEN 'open' ELSE t.status END";

/// A task's history entry, always on the transaction that made the change.
/// One that commits separately can outlive a rolled-back mutation, or be
/// lost by one that committed and then answered the caller with an error.
async fn log_event_tx(
    conn: &mut sqlx::PgConnection,
    task_id: Uuid,
    agent_id: Uuid,
    event: &str,
    detail: Option<&str>,
) -> BusResult<()> {
    sqlx::query("INSERT INTO task_events (task_id, agent_id, event, detail) VALUES ($1,$2,$3,$4)")
        .bind(task_id)
        .bind(agent_id)
        .bind(event)
        .bind(detail)
        .execute(conn)
        .await?;
    Ok(())
}

fn normalize_key(key: &str) -> BusResult<String> {
    let key = key.trim();
    if key.is_empty() {
        return Err(BusError::invalid("task key cannot be empty"));
    }
    if key.len() > 128 {
        return Err(BusError::invalid("task key is limited to 128 characters"));
    }
    Ok(key.to_owned())
}

fn normalize_transition_reason(reason: &str) -> BusResult<String> {
    let reason = super::check_text("transition reason", reason, MAX_TRANSITION_REASON_BYTES)?;
    if reason.is_empty() {
        return Err(BusError::invalid("transition reason cannot be empty"));
    }
    Ok(reason)
}

// Read the post-CAS task while its row is still locked, so a later
// concurrent PATCH cannot cause a mismatched revision in this response.
async fn fetch_task_locked(
    conn: &mut sqlx::PgConnection,
    auth: &AuthCtx,
    key: &str,
) -> BusResult<TaskInfo> {
    let row: TaskRow = sqlx::query_as(AssertSqlSafe(format!(
        "{TASK_SELECT} WHERE t.team_id = $1 AND t.key = $2"
    )))
    .bind(auth.team_id)
    .bind(key)
    .fetch_one(conn)
    .await?;
    Ok(row.into())
}

// ---------------------------------------------------------- edit content --
/// None means absent; Some(None) is an explicit JSON null.
pub struct UpdateInput {
    pub expected_revision: i64,
    pub title: Option<Option<String>>,
    pub description: Option<Option<String>>,
    pub metadata: Option<Option<serde_json::Value>>,
}

pub async fn update_task(
    pool: &PgPool,
    auth: &AuthCtx,
    key: &str,
    input: UpdateInput,
) -> BusResult<UpdateTaskResult> {
    let key = normalize_key(key)?;
    if input.expected_revision < 1 {
        return Err(BusError::invalid("expected_revision must be positive"));
    }
    if input.title.is_none() && input.description.is_none() && input.metadata.is_none() {
        return Err(BusError::invalid(
            "update_task needs at least one editable field",
        ));
    }

    let validated_title = match input.title {
        Some(Some(t)) => {
            let t = super::check_text("task title", &t, MAX_TITLE_BYTES)?;
            if t.is_empty() {
                return Err(BusError::invalid("task title cannot be empty"));
            }
            Some(t)
        }
        Some(None) => return Err(BusError::invalid("task title cannot be null")),
        None => None,
    };
    let validated_description = match input.description {
        Some(Some(d)) => Some(Some(super::check_text(
            "task description",
            &d,
            MAX_DESCRIPTION_BYTES,
        )?)),
        Some(None) => Some(None),
        None => None,
    };
    let validated_metadata = match input.metadata {
        Some(optional) => {
            let value = optional.unwrap_or_else(|| serde_json::json!({}));
            if !value.is_object() {
                return Err(BusError::invalid("metadata must be a JSON object or null"));
            }
            super::check_metadata("task", Some(&value))?;
            Some(value)
        }
        None => None,
    };

    let mut tx = pool.begin().await?;
    super::sessions::guard(&mut tx, auth).await?;
    let row: Option<(
        Uuid,
        String,
        Option<String>,
        serde_json::Value,
        String,
        Option<Uuid>,
        Option<String>,
        Option<chrono::DateTime<chrono::Utc>>,
        i64,
    )> = sqlx::query_as(
        "SELECT id, title, description, metadata, status, claimed_by,
                claimed_session, lease_expires_at, revision
         FROM tasks WHERE team_id = $1 AND key = $2 FOR UPDATE",
    )
    .bind(auth.team_id)
    .bind(&key)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((
        task_id,
        old_title,
        old_description,
        old_metadata,
        status,
        claimed_by,
        claimed_session,
        lease_expiry,
        current_revision,
    )) = row
    else {
        return Err(BusError::not_found(format!("task '{key}'")));
    };

    if !matches!(status.as_str(), "open" | "claimed") {
        return Err(BusError::conflict(format!(
            "task '{key}' is terminal ({status}); edit refused"
        )));
    }
    if claimed_by_another_session(
        auth,
        &status,
        claimed_by,
        claimed_session.as_deref(),
        lease_expiry,
    ) {
        return Err(BusError::conflict(format!(
            "task '{key}' has a live claim held by another session"
        )));
    }
    // Strict expected revision even for a no-op to reject stale writers.
    if input.expected_revision != current_revision {
        return Err(BusError::conflict(format!(
            "task '{key}' revision mismatch: expected {}, current {}",
            input.expected_revision, current_revision
        )));
    }

    let new_title = validated_title.unwrap_or_else(|| old_title.clone());
    let new_description = validated_description.unwrap_or_else(|| old_description.clone());
    let new_metadata = validated_metadata.unwrap_or_else(|| old_metadata.clone());
    let mut changed_fields = Vec::new();
    if new_title != old_title {
        changed_fields.push("title".to_owned());
    }
    if new_description != old_description {
        changed_fields.push("description".to_owned());
    }
    if new_metadata != old_metadata {
        changed_fields.push("metadata".to_owned());
    }

    if changed_fields.is_empty() {
        let task = fetch_task_locked(&mut tx, auth, &key).await?;
        tx.commit().await?;
        return Ok(UpdateTaskResult {
            task,
            applied: false,
            previous_revision: current_revision,
            revision: current_revision,
            changed_fields,
        });
    }

    let new_revision: Option<i64> = sqlx::query_scalar(
        "UPDATE tasks SET title=$1, description=$2, metadata=$3,
                revision=revision+1, updated_at=now()
         WHERE id=$4 AND team_id=$5 AND revision=$6
         RETURNING revision",
    )
    .bind(&new_title)
    .bind(&new_description)
    .bind(&new_metadata)
    .bind(task_id)
    .bind(auth.team_id)
    .bind(current_revision)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(new_revision) = new_revision else {
        return Err(BusError::conflict("concurrent task revision changed"));
    };

    // History stores revisions and FIELD NAMES ONLY, never values or secrets.
    let detail = serde_json::json!({
        "previous_revision": current_revision, "revision": new_revision,
        "changed_fields": changed_fields, "actor_session": auth.session,
    })
    .to_string();
    log_event_tx(&mut tx, task_id, auth.agent_id, "updated", Some(&detail)).await?;
    sqlx::query(
        "INSERT INTO task_update_audit
         (task_id, team_id, actor_agent_id, actor_session,
          previous_revision, new_revision, changed_fields)
         VALUES ($1,$2,$3,$4,$5,$6,$7)",
    )
    .bind(task_id)
    .bind(auth.team_id)
    .bind(auth.agent_id)
    .bind(&auth.session)
    .bind(current_revision)
    .bind(new_revision)
    .bind(&changed_fields)
    .execute(&mut *tx)
    .await?;
    let task = fetch_task_locked(&mut tx, auth, &key).await?;
    tx.commit().await?;
    Ok(UpdateTaskResult {
        task,
        applied: true,
        previous_revision: current_revision,
        revision: new_revision,
        changed_fields,
    })
}

// ------------------------------------------------------------------ create --

pub struct CreateInput {
    pub key: String,
    pub title: String,
    pub description: Option<String>,
    pub metadata: Option<serde_json::Value>,
    /// Keys of existing tasks this one depends on. The task cannot be claimed
    /// until every dependency is done or cancelled.
    pub depends_on: Vec<String>,
}

pub async fn create_task(pool: &PgPool, auth: &AuthCtx, input: CreateInput) -> BusResult<TaskInfo> {
    let key = normalize_key(&input.key)?;
    let title = super::check_text("task title", &input.title, MAX_TITLE_BYTES)?;
    if title.is_empty() {
        return Err(BusError::invalid("task title cannot be empty"));
    }
    let description = match input.description.as_deref() {
        Some(d) => Some(super::check_text(
            "task description",
            d,
            MAX_DESCRIPTION_BYTES,
        )?),
        None => None,
    };
    let metadata_in = super::normalize_metadata(input.metadata);
    super::check_metadata("task", metadata_in.as_ref())?;
    let metadata = metadata_in.unwrap_or_else(|| serde_json::Value::Object(Default::default()));

    if input.depends_on.len() > MAX_DEPENDENCIES {
        return Err(BusError::invalid(format!(
            "a task declares at most {MAX_DEPENDENCIES} dependencies; got {}. \
             Group the upstream work into fewer tasks.",
            input.depends_on.len()
        )));
    }
    let mut dep_keys: Vec<String> = Vec::with_capacity(input.depends_on.len());
    for dep_key in &input.depends_on {
        let dep_key = normalize_key(dep_key)?;
        if dep_key == key {
            return Err(BusError::invalid("a task cannot depend on itself"));
        }
        dep_keys.push(dep_key);
    }

    // The task, its dependencies and its `created` event commit together.
    // Written one by one, the task was committed, open and dependency-free
    // until its `task_deps` rows landed, so a claim in between took work
    // whose upstream was unfinished, and a failure in between left it that
    // way for good (#179). The NOTIFY a waiter wakes on fires at this commit
    // too, so nobody is woken into the gap.
    let mut tx = pool.begin().await?;

    // The existence check is the insert itself: a concurrent create of the
    // same key waits here for the other to commit and then gets the same
    // conflict, not a unique violation.
    let inserted: Option<(Uuid,)> = sqlx::query_as(
        r#"
        INSERT INTO tasks (team_id, key, title, description, metadata, created_by)
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (team_id, key) DO NOTHING
        RETURNING id
        "#,
    )
    .bind(auth.team_id)
    .bind(&key)
    .bind(&title)
    .bind(description.as_deref())
    .bind(&metadata)
    .bind(auth.agent_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((id,)) = inserted else {
        return Err(BusError::conflict(format!(
            "task '{key}' already exists; use get_task to inspect it"
        )));
    };

    // Resolved inside the same transaction, so a missing key rolls the task
    // back with it: a typo fails the whole call and leaves nothing behind.
    // Hold each prerequisite row until the dependent edge commits. A concurrent
    // cancellation also takes FOR UPDATE on that row: either this creation
    // commits first and cancellation sees an active dependent, or cancellation
    // commits first and this creation rejects the terminal prerequisite.
    let found: Vec<(String, Uuid, String)> = sqlx::query_as(
        "SELECT key, id, status FROM tasks
         WHERE team_id = $1 AND key = ANY($2)
         ORDER BY id FOR UPDATE",
    )
    .bind(auth.team_id)
    .bind(&dep_keys)
    .fetch_all(&mut *tx)
    .await?;
    let found: std::collections::HashMap<String, (Uuid, String)> = found
        .into_iter()
        .map(|(key, id, status)| (key, (id, status)))
        .collect();
    let mut dep_ids: Vec<Uuid> = Vec::with_capacity(dep_keys.len());
    for dep_key in &dep_keys {
        match found.get(dep_key) {
            Some((id, status)) if matches!(status.as_str(), "open" | "claimed" | "done") => {
                dep_ids.push(*id);
            }
            Some((_, status)) => {
                return Err(BusError::conflict(format!(
                    "dependency '{dep_key}' is {status}; a terminal task cannot be a new prerequisite"
                )));
            }
            None => {
                return Err(BusError::not_found(format!(
                    "dependency '{dep_key}' does not exist; create it first"
                )));
            }
        }
    }
    dep_ids.sort();
    dep_ids.dedup();

    // Dependencies only point at pre-existing tasks and this task is brand
    // new, so no cycle is possible by construction.
    if !dep_ids.is_empty() {
        sqlx::query(
            "INSERT INTO task_deps (task_id, blocked_by_task_id) SELECT $1, unnest($2::uuid[])",
        )
        .bind(id)
        .bind(&dep_ids)
        .execute(&mut *tx)
        .await?;
    }

    log_event_tx(&mut tx, id, auth.agent_id, "created", Some(&title)).await?;
    tx.commit().await?;
    fetch_task(pool, auth, &key).await
}

async fn fetch_task(pool: &PgPool, auth: &AuthCtx, key: &str) -> BusResult<TaskInfo> {
    let row: Option<TaskRow> = sqlx::query_as(AssertSqlSafe(format!(
        "{TASK_SELECT} WHERE t.team_id = $1 AND t.key = $2"
    )))
    .bind(auth.team_id)
    .bind(key)
    .fetch_optional(pool)
    .await?;
    row.map(Into::into)
        .ok_or_else(|| BusError::not_found(format!("task '{key}'")))
}

pub async fn get_task(pool: &PgPool, auth: &AuthCtx, key: &str) -> BusResult<TaskDetail> {
    let task = fetch_task(pool, auth, key).await?;
    let rows: Vec<(
        String,
        Option<String>,
        Option<String>,
        chrono::DateTime<chrono::Utc>,
    )> = sqlx::query_as(
        r#"
            SELECT e.event, a.name, e.detail, e.created_at
            FROM task_events e
            LEFT JOIN agents a ON a.id = e.agent_id
            JOIN tasks t ON t.id = e.task_id
            WHERE t.team_id = $1 AND t.key = $2
            ORDER BY e.id
            "#,
    )
    .bind(auth.team_id)
    .bind(key.trim())
    .fetch_all(pool)
    .await?;

    Ok(TaskDetail {
        task,
        history: rows
            .into_iter()
            .map(|(event, agent, detail, created_at)| TaskEventInfo {
                event,
                agent,
                detail,
                created_at: ts(created_at),
            })
            .collect(),
    })
}

// -------------------------------------------------------------------- list --

const STATUS_RANK: &str = "CASE WHEN t.status = 'claimed' AND t.lease_expires_at <= now() THEN 1 WHEN t.status = 'claimed' THEN 0 WHEN t.status = 'open' THEN 1 ELSE 2 END";
const CURSOR_TTL_SECS: i64 = 900;

#[derive(Debug, Clone)]
pub struct TaskPageQuery {
    pub status: Option<String>,
    pub mine_only: bool,
    pub limit: i64,
    pub project_prefix: Option<String>,
    pub cursor: Option<String>,
}

#[derive(Debug)]
pub struct TaskPage {
    pub tasks: Vec<TaskInfo>,
    pub open: i64,
    pub claimed: i64,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Debug, Clone)]
pub struct TaskSearchQuery {
    pub search: Option<String>,
    pub search_mode: Option<String>,
    pub search_fields: Option<String>,
    pub search_language: Option<String>,
}

#[derive(Debug, Clone)]
struct NormalizedSearch {
    query: String,
    mode: &'static str,
    fields: &'static str,
    language: &'static str,
    parameter: String,
}

const MAX_SEARCH_BYTES: usize = 512;
const REGEX_STATEMENT_TIMEOUT: &str = "250ms";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SearchFailure {
    InvalidRegex,
    Timeout,
    Cancelled,
}

fn classify_search_failure(code: Option<&str>, message: &str) -> Option<SearchFailure> {
    match code {
        Some("2201B") => Some(SearchFailure::InvalidRegex),
        Some("57014") if message.contains("statement timeout") => Some(SearchFailure::Timeout),
        Some("57014") if message.contains("user request") => Some(SearchFailure::Cancelled),
        _ => None,
    }
}

fn search_failure_error(failure: SearchFailure) -> BusError {
    match failure {
        SearchFailure::InvalidRegex => {
            BusError::invalid("search regex is not a valid PostgreSQL regular expression")
        }
        SearchFailure::Timeout => BusError::invalid(
            "regex search exceeded the 250ms execution limit; narrow the pattern and retry",
        ),
        SearchFailure::Cancelled => {
            BusError::conflict("task search was cancelled before completion; retry the request")
        }
    }
}

fn map_search_database_error(error: &sqlx::Error) -> Option<BusError> {
    let database_error = error.as_database_error()?;
    classify_search_failure(database_error.code().as_deref(), database_error.message())
        .map(search_failure_error)
}

fn escape_contains(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn normalize_search(search: TaskSearchQuery) -> BusResult<Option<NormalizedSearch>> {
    let Some(query) = search
        .search
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    if query.len() > MAX_SEARCH_BYTES {
        return Err(BusError::invalid(format!(
            "search must be at most {MAX_SEARCH_BYTES} bytes"
        )));
    }
    let mode = search
        .search_mode
        .unwrap_or_else(|| "keywords".to_owned())
        .trim()
        .to_lowercase();
    let fields = search
        .search_fields
        .unwrap_or_else(|| "both".to_owned())
        .trim()
        .to_lowercase();
    let language = search
        .search_language
        .unwrap_or_else(|| "simple".to_owned())
        .trim()
        .to_lowercase();
    let mode = match mode.as_str() {
        "keywords" => "keywords",
        "contains" => "contains",
        "regex" => "regex",
        _ => {
            return Err(BusError::invalid(
                "search_mode must be one of: keywords, contains, regex",
            ));
        }
    };
    let fields = match fields.as_str() {
        "title" => "title",
        "description" => "description",
        "both" => "both",
        _ => {
            return Err(BusError::invalid(
                "search_fields must be one of: title, description, both",
            ));
        }
    };
    let language = match language.as_str() {
        "simple" => "simple",
        "english" => "english",
        "russian" => "russian",
        _ => {
            return Err(BusError::invalid(
                "search_language must be one of: simple, english, russian",
            ));
        }
    };
    if mode == "regex" && query.as_bytes().contains(&0) {
        return Err(BusError::invalid("regex search cannot contain NUL bytes"));
    }
    let parameter = if mode == "contains" {
        escape_contains(&query)
    } else {
        query.clone()
    };
    Ok(Some(NormalizedSearch {
        query,
        mode,
        fields,
        language,
        parameter,
    }))
}

fn keyword_vector_expression(language: &str) -> &'static str {
    match language {
        "simple" => {
            "(setweight(to_tsvector('simple', coalesce(t.title, '')), 'A') || setweight(to_tsvector('simple', coalesce(t.description, '')), 'B'))"
        }
        "english" => {
            "(setweight(to_tsvector('english', coalesce(t.title, '')), 'A') || setweight(to_tsvector('english', coalesce(t.description, '')), 'B'))"
        }
        "russian" => {
            "(setweight(to_tsvector('russian', coalesce(t.title, '')), 'A') || setweight(to_tsvector('russian', coalesce(t.description, '')), 'B'))"
        }
        _ => unreachable!("search language is normalized before SQL construction"),
    }
}

fn keyword_search_clause(search: &NormalizedSearch) -> String {
    let vector = keyword_vector_expression(search.language);
    let query = format!("websearch_to_tsquery('{}', $7)", search.language);
    let field_clause = match search.fields {
        "title" => format!(
            "({vector} @@ {query} AND to_tsvector('{}', coalesce(t.title, '')) @@ {query})",
            search.language
        ),
        "description" => format!(
            "({vector} @@ {query} AND to_tsvector('{}', coalesce(t.description, '')) @@ {query})",
            search.language
        ),
        "both" => format!("{vector} @@ {query}"),
        _ => unreachable!("search fields are normalized before SQL construction"),
    };
    format!("($8::text IS NOT NULL AND {field_clause})")
}

fn search_clause(search: Option<&NormalizedSearch>) -> String {
    let Some(search) = search else {
        return "($7::text IS NULL AND $8::text IS NULL)".to_owned();
    };
    match (search.mode, search.fields) {
        ("keywords", _) => keyword_search_clause(search),
        ("contains", "title") => "t.title ILIKE ('%' || $7 || '%') ESCAPE chr(92)".to_owned(),
        ("contains", "description") => "coalesce(t.description, '') ILIKE ('%' || $7 || '%') ESCAPE chr(92)".to_owned(),
        ("contains", "both") => "(t.title ILIKE ('%' || $7 || '%') ESCAPE chr(92) OR coalesce(t.description, '') ILIKE ('%' || $7 || '%') ESCAPE chr(92))".to_owned(),
        ("regex", "title") => "t.title ~* $7".to_owned(),
        ("regex", "description") => "coalesce(t.description, '') ~* $7".to_owned(),
        ("regex", "both") => "(t.title ~* $7 OR coalesce(t.description, '') ~* $7)".to_owned(),
        _ => unreachable!("search options are normalized before SQL construction"),
    }
}

fn search_cursor_binding(project_prefix: Option<&str>, search: &NormalizedSearch) -> String {
    serde_json::to_string(&(
        project_prefix,
        search.mode,
        search.fields,
        search.language,
        &search.query,
    ))
    .expect("search cursor binding is serializable")
}

fn cursor_secret() -> BusResult<Vec<u8>> {
    std::env::var("TASK_CURSOR_SECRET")
        .or_else(|_| std::env::var("BUS_DASHBOARD_SECRET"))
        .map(|secret| secret.into_bytes())
        .map_err(|_| {
            BusError::invalid("TASK_CURSOR_SECRET must be configured for paged task scans")
        })
}

fn normalize_project_prefix(prefix: Option<String>) -> Option<String> {
    prefix
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn page_cursor_context(
    auth: &AuthCtx,
    project_prefix: Option<&str>,
    status: Option<&str>,
    mine_only: bool,
    search: Option<&NormalizedSearch>,
) -> TaskCursorContext {
    TaskCursorContext {
        team_id: auth.team_id.to_string(),
        filters: TaskCursorFilters {
            project_prefix: project_prefix.map(str::to_owned),
            status: status.map(str::to_owned),
            mine: mine_only,
            search: search.map(|value| value.query.clone()),
            search_mode: search.map(|value| value.mode.to_owned()),
            search_fields: search.map(|value| value.fields.to_owned()),
            search_language: search.map(|value| value.language.to_owned()),
        },
    }
}

fn invalid_cursor(error: impl std::fmt::Display) -> BusError {
    BusError::invalid(format!("invalid task-list cursor: {error}"))
}

pub async fn list_tasks_page(
    pool: &PgPool,
    auth: &AuthCtx,
    query: TaskPageQuery,
) -> BusResult<TaskPage> {
    let limit = query.limit.clamp(1, MAX_LIMIT);
    let status = query
        .status
        .map(|value| value.trim().to_lowercase())
        .filter(|value| value != "any");
    if let Some(value) = &status
        && !["open", "claimed", "done", "cancelled", "superseded"].contains(&value.as_str())
    {
        return Err(BusError::invalid(
            "status must be one of: open, claimed, done, cancelled, superseded, any",
        ));
    }
    let project_prefix = normalize_project_prefix(query.project_prefix);
    let context = page_cursor_context(
        auth,
        project_prefix.as_deref(),
        status.as_deref(),
        query.mine_only,
        None,
    );
    let secret = cursor_secret()?;
    let now = chrono::Utc::now();
    let decoded = query
        .cursor
        .as_deref()
        .map(|token| decode_cursor_at(&secret, token, &context, now.timestamp()))
        .transpose()
        .map_err(invalid_cursor)?;
    let cursor_order = decoded.as_ref().map(|cursor| &cursor.order);
    let cursor_time = cursor_order
        .map(|order| {
            chrono::DateTime::<chrono::Utc>::from_timestamp_micros(order.updated_at_micros)
                .ok_or_else(|| BusError::invalid("invalid task-list cursor timestamp"))
        })
        .transpose()?;

    let rows: Vec<TaskRow> = sqlx::query_as(AssertSqlSafe(format!(
        r#"{TASK_SELECT}
           WHERE t.team_id = $1
             AND ($2::text IS NULL OR left(t.key, char_length($2::text)) = $2::text)
             AND ($3::text IS NULL OR ({EFFECTIVE_STATUS}) = $3)
             AND (NOT $4::bool
                  OR (t.claimed_by = $5 AND COALESCE(t.claimed_session, '') = $6
                      AND COALESCE(t.lease_expires_at > now(), true)))
             AND ($7::int IS NULL OR (
                    ({STATUS_RANK}) > $7
                    OR (({STATUS_RANK}) = $7 AND t.updated_at < $8)
                    OR (({STATUS_RANK}) = $7 AND t.updated_at = $8 AND t.key > $9)
                 ))
           ORDER BY ({STATUS_RANK}) ASC, t.updated_at DESC, t.key ASC
           LIMIT $10"#
    )))
    .bind(auth.team_id)
    .bind(project_prefix.as_deref())
    .bind(status.as_deref())
    .bind(query.mine_only)
    .bind(auth.agent_id)
    .bind(&auth.session)
    .bind(cursor_order.map(|order| order.status_rank))
    .bind(cursor_time)
    .bind(cursor_order.map(|order| order.task_key.as_str()))
    .bind(limit + 1)
    .fetch_all(pool)
    .await?;

    let (open, claimed): (i64, i64) = sqlx::query_as(AssertSqlSafe(format!(
        r#"SELECT count(*) FILTER (WHERE ({EFFECTIVE_STATUS}) = 'open'),
                  count(*) FILTER (WHERE ({EFFECTIVE_STATUS}) = 'claimed')
           FROM tasks t WHERE t.team_id = $1"#
    )))
    .bind(auth.team_id)
    .fetch_one(pool)
    .await?;

    let mut rows = rows;
    let has_more = rows.len() > limit as usize;
    if has_more {
        rows.truncate(limit as usize);
    }
    let next_cursor = if has_more {
        let last = rows
            .last()
            .ok_or_else(|| BusError::invalid("page sentinel was inconsistent"))?;
        let order = TaskOrder::from_sql(
            last.status_rank,
            last.updated_at.timestamp_micros(),
            last.key.clone(),
        );
        let issued_at = now.timestamp();
        let lifetime =
            CursorLifetime::new(issued_at, issued_at + CURSOR_TTL_SECS).map_err(invalid_cursor)?;
        Some(
            encode_cursor_with_lifetime(&secret, &context, &order, lifetime)
                .map_err(invalid_cursor)?,
        )
    } else {
        None
    };

    Ok(TaskPage {
        tasks: rows.into_iter().map(Into::into).collect(),
        open,
        claimed,
        next_cursor,
        has_more,
    })
}

pub async fn list_tasks_page_with_search(
    pool: &PgPool,
    auth: &AuthCtx,
    query: TaskPageQuery,
    search_query: TaskSearchQuery,
) -> BusResult<TaskPage> {
    let search = normalize_search(search_query)?;
    let limit = query.limit.clamp(1, MAX_LIMIT);
    let status = query
        .status
        .map(|value| value.trim().to_lowercase())
        .filter(|value| value != "any");
    if let Some(value) = &status
        && !["open", "claimed", "done", "cancelled", "superseded"].contains(&value.as_str())
    {
        return Err(BusError::invalid(
            "status must be one of: open, claimed, done, cancelled, superseded, any",
        ));
    }
    let project_prefix = normalize_project_prefix(query.project_prefix);
    let context = page_cursor_context(
        auth,
        project_prefix.as_deref(),
        status.as_deref(),
        query.mine_only,
        search.as_ref(),
    );
    let secret = cursor_secret()?;
    let now = chrono::Utc::now();
    let decoded = query
        .cursor
        .as_deref()
        .map(|token| decode_cursor_at(&secret, token, &context, now.timestamp()))
        .transpose()
        .map_err(invalid_cursor)?;
    let cursor_order = decoded.as_ref().map(|cursor| &cursor.order);
    let cursor_time = cursor_order
        .map(|order| {
            chrono::DateTime::<chrono::Utc>::from_timestamp_micros(order.updated_at_micros)
                .ok_or_else(|| BusError::invalid("invalid task-list cursor timestamp"))
        })
        .transpose()?;
    let search_clause = search_clause(search.as_ref());
    let search_parameter = search.as_ref().map(|value| value.parameter.as_str());
    let search_language = search.as_ref().map(|value| value.language);
    let timeout = match search.as_ref().map(|value| value.mode) {
        Some("regex") => REGEX_STATEMENT_TIMEOUT,
        Some(_) => "5s",
        None => "5s",
    };

    let mut tx = pool.begin().await?;
    sqlx::query("SELECT set_config('statement_timeout', $1, true)")
        .bind(timeout)
        .execute(&mut *tx)
        .await?;
    let rows_result: Result<Vec<TaskRow>, sqlx::Error> = sqlx::query_as(AssertSqlSafe(format!(
        r#"{TASK_SELECT}
           WHERE t.team_id = $1
             AND ($2::text IS NULL OR left(t.key, char_length($2::text)) = $2::text)
             AND ($3::text IS NULL OR ({EFFECTIVE_STATUS}) = $3)
             AND (NOT $4::bool
                  OR (t.claimed_by = $5 AND COALESCE(t.claimed_session, '') = $6
                      AND COALESCE(t.lease_expires_at > now(), true)))
             AND ({search_clause})
             AND ($9::int IS NULL OR (
                    ({STATUS_RANK}) > $9
                    OR (({STATUS_RANK}) = $9 AND t.updated_at < $10)
                    OR (({STATUS_RANK}) = $9 AND t.updated_at = $10 AND t.key > $11)
                 ))
           ORDER BY ({STATUS_RANK}) ASC, t.updated_at DESC, t.key ASC
           LIMIT $12"#
    )))
    .bind(auth.team_id)
    .bind(project_prefix.as_deref())
    .bind(status.as_deref())
    .bind(query.mine_only)
    .bind(auth.agent_id)
    .bind(&auth.session)
    .bind(search_parameter)
    .bind(search_language)
    .bind(cursor_order.map(|order| order.status_rank))
    .bind(cursor_time)
    .bind(cursor_order.map(|order| order.task_key.as_str()))
    .bind(limit + 1)
    .fetch_all(&mut *tx)
    .await;
    let mut rows = match rows_result {
        Err(error) => {
            let mapped = map_search_database_error(&error);
            let fallback = if mapped.is_none() {
                Some(error.into())
            } else {
                None
            };
            let _ = tx.rollback().await;
            return Err(mapped
                .or(fallback)
                .expect("search database error mapping produced no error"));
        }
        Ok(rows) => rows,
    };
    let totals_result: Result<(i64, i64), sqlx::Error> = sqlx::query_as(AssertSqlSafe(format!(
        r#"SELECT count(*) FILTER (WHERE ({EFFECTIVE_STATUS}) = 'open'),
                  count(*) FILTER (WHERE ({EFFECTIVE_STATUS}) = 'claimed')
           FROM tasks t WHERE t.team_id = $1"#
    )))
    .bind(auth.team_id)
    .fetch_one(&mut *tx)
    .await;
    let (open, claimed) = match totals_result {
        Err(error) => {
            let mapped = map_search_database_error(&error);
            let fallback = if mapped.is_none() {
                Some(error.into())
            } else {
                None
            };
            let _ = tx.rollback().await;
            return Err(mapped
                .or(fallback)
                .expect("search totals error mapping produced no error"));
        }
        Ok(totals) => totals,
    };
    tx.commit().await?;

    let has_more = rows.len() > limit as usize;
    if has_more {
        rows.truncate(limit as usize);
    }
    let next_cursor = if has_more {
        let last = rows
            .last()
            .ok_or_else(|| BusError::invalid("page sentinel was inconsistent"))?;
        let order = TaskOrder::from_sql(
            last.status_rank,
            last.updated_at.timestamp_micros(),
            last.key.clone(),
        );
        let issued_at = now.timestamp();
        let lifetime =
            CursorLifetime::new(issued_at, issued_at + CURSOR_TTL_SECS).map_err(invalid_cursor)?;
        Some(
            encode_cursor_with_lifetime(&secret, &context, &order, lifetime)
                .map_err(invalid_cursor)?,
        )
    } else {
        None
    };

    Ok(TaskPage {
        tasks: rows.into_iter().map(Into::into).collect(),
        open,
        claimed,
        next_cursor,
        has_more,
    })
}

pub async fn list_tasks(
    pool: &PgPool,
    auth: &AuthCtx,
    status: Option<String>,
    mine_only: bool,
    limit: i64,
) -> BusResult<TaskList> {
    let limit = limit.clamp(1, MAX_LIMIT);
    let status = status
        .map(|s| s.trim().to_lowercase())
        .filter(|s| s != "any");
    if let Some(s) = &status
        && !["open", "claimed", "done", "cancelled"].contains(&s.as_str())
    {
        return Err(BusError::invalid(
            "status must be one of: open, claimed, done, cancelled, any",
        ));
    }

    let rows: Vec<TaskRow> = sqlx::query_as(AssertSqlSafe(format!(
        r#"{TASK_SELECT}
           WHERE t.team_id = $1
             AND ($2::text IS NULL OR ({EFFECTIVE_STATUS}) = $2)
             -- "mine" means this session's live claim, matching whoami, renew
             -- and release. Matching the agent alone would report a task your
             -- core-manager window is holding as this window's own work,
             -- which is the duplication the session check exists to stop; a
             -- lapsed lease is nobody's.
             AND (NOT $3::bool
                  OR (t.claimed_by = $4 AND COALESCE(t.claimed_session, '') = $6
                      AND COALESCE(t.lease_expires_at > now(), true)))
           ORDER BY
             CASE ({EFFECTIVE_STATUS}) WHEN 'claimed' THEN 0 WHEN 'open' THEN 1 ELSE 2 END,
             t.updated_at DESC,
             t.key ASC
           LIMIT $5"#
    )))
    .bind(auth.team_id)
    .bind(status.as_deref())
    .bind(mine_only)
    .bind(auth.agent_id)
    .bind(limit)
    .bind(&auth.session)
    .fetch_all(pool)
    .await?;

    let (open, claimed): (i64, i64) = sqlx::query_as(AssertSqlSafe(format!(
        r#"
        SELECT count(*) FILTER (WHERE ({EFFECTIVE_STATUS}) = 'open'),
               count(*) FILTER (WHERE ({EFFECTIVE_STATUS}) = 'claimed')
        FROM tasks t WHERE t.team_id = $1
        "#
    )))
    .bind(auth.team_id)
    .fetch_one(pool)
    .await?;

    Ok(TaskList {
        tasks: rows.into_iter().map(Into::into).collect(),
        open,
        claimed,
    })
}

// ------------------------------------------------------------------- claim --

/// Claim a specific task. Succeeds when the task is open, when its lease has
/// already expired, or when the caller already holds it (idempotent re-claim).
pub async fn claim_task(
    pool: &PgPool,
    auth: &AuthCtx,
    key: &str,
    lease_seconds: Option<i64>,
) -> BusResult<ClaimResult> {
    let key = normalize_key(key)?;
    let lease = lease_seconds
        .unwrap_or(DEFAULT_LEASE_SECS)
        .clamp(30, MAX_LEASE_SECS);

    // Serialised with the mutation: a request already queued when its window
    // was resumed must not commit into the session that replaced it.
    let mut tx = pool.begin().await?;
    super::sessions::guard(&mut tx, auth).await?;

    let updated: Option<(Uuid,)> = sqlx::query_as(
        r#"
        UPDATE tasks
        SET status = 'claimed',
            claimed_by = $1,
            claimed_session = $5,
            claimed_at = now(),
            lease_expires_at = now() + make_interval(secs => $2),
            updated_at = now()
        WHERE team_id = $3
          AND key = $4
          AND status IN ('open', 'claimed')
          -- Re-claiming renews the lease, but only from the session that holds
          -- it. Matching on the agent alone made the lease void between two
          -- sessions of one person: both claimed, both were told they had it,
          -- and both did the work. COALESCE so a claim taken before sessions
          -- existed counts as the shared session, which is what it was.
          AND (status = 'open'
               OR (claimed_by = $1 AND COALESCE(claimed_session, '') = $5)
               OR (lease_expires_at IS NOT NULL AND lease_expires_at < now()))
          AND NOT EXISTS (
              SELECT 1
              FROM task_deps td JOIN tasks d ON d.id = td.blocked_by_task_id
              WHERE td.task_id = tasks.id AND d.status <> 'done'
          )
        RETURNING id
        "#,
    )
    .bind(auth.agent_id)
    .bind(lease as f64)
    .bind(auth.team_id)
    .bind(&key)
    .bind(&auth.session)
    .fetch_optional(&mut *tx)
    .await?;
    // The history entry commits with the claim. Written after the commit, a
    // failed write answered "database error" for a claim that stood, so the
    // caller held a lease it had been told it did not get.
    if let Some((id,)) = updated {
        log_event_tx(&mut tx, id, auth.agent_id, "claimed", None).await?;
    }
    tx.commit().await?;

    match updated {
        Some(_) => Ok(ClaimResult {
            claimed: true,
            task: Some(fetch_task(pool, auth, &key).await?),
            reason: None,
        }),
        None => {
            // Distinguish "does not exist" from "someone else holds it".
            let current = fetch_task(pool, auth, &key).await?;
            let reason = if current.blocked {
                format!(
                    "blocked by unfinished dependencies: {}",
                    current.depends_on.join(", ")
                )
            } else {
                match current.status.as_str() {
                    "claimed" => holder_reason(auth, &current),
                    other => format!("task is {other}"),
                }
            };
            Ok(ClaimResult {
                claimed: false,
                task: Some(current),
                reason: Some(reason),
            })
        }
    }
}

/// Why a claim was refused, written for the model that has to act on it.
///
/// The case worth spelling out is your *own* other session: "held by joaquin"
/// reads as a bug when you are joaquin, and the fix — go to that window, or
/// wait for the lease — is not guessable from the name alone.
fn holder_reason(auth: &AuthCtx, current: &TaskInfo) -> String {
    let holder = current.claimed_by.as_deref().unwrap_or("?");
    let until = match current.lease_seconds_remaining {
        Some(secs) => format!("the lease expires in {secs}s"),
        None => "the lease expiry is unknown".to_owned(),
    };
    let mine = current.claimed_by.as_deref() == Some(auth.agent_name.as_str());
    let same_session = current.claimed_session.as_deref().unwrap_or("") == auth.session;

    if mine && !same_session {
        let theirs = current
            .claimed_session
            .as_deref()
            .map(|s| format!("'{s}'"))
            .unwrap_or_else(|| "shared".to_owned());
        format!(
            "claimed by your own {theirs} session, and {until} — continue the \
             work there, or wait for the lease to expire and claim it here"
        )
    } else {
        let where_ = current
            .claimed_session
            .as_deref()
            .map(|s| format!(" (session '{s}')"))
            .unwrap_or_default();
        format!("held by {holder}{where_}, {until}")
    }
}

/// Refusal for renew/release when this session does not hold the claim.
///
/// "You do not hold a claim" is true but useless when your other window does:
/// look up who actually has it and say so.
async fn no_claim_here(pool: &PgPool, auth: &AuthCtx, key: &str) -> BusError {
    match fetch_task(pool, auth, key).await {
        Ok(current) if current.status == "claimed" => BusError::conflict(format!(
            "you do not hold the claim on '{key}': it is {}",
            holder_reason(auth, &current)
        )),
        Ok(current)
            if current.lease_expired
                && current.lapsed_holder.as_deref() == Some(auth.agent_name.as_str()) =>
        {
            BusError::conflict(format!(
                "your lease on '{key}' lapsed, so the task has been open to everyone since. \
                 If you are still working on it, claim it again (and renew before the lease \
                 runs out next time)."
            ))
        }
        Ok(current) => BusError::conflict(format!(
            "you do not hold an active claim on '{key}' (it is {})",
            current.status
        )),
        // The task is gone or not ours to see; the original message is still
        // the honest answer.
        Err(_) => BusError::conflict(format!("you do not hold an active claim on '{key}'")),
    }
}

/// Claim the oldest available task. Uses `SKIP LOCKED` so several agents can
/// call this concurrently without handing the same task to two of them.
pub async fn claim_next_task(
    pool: &PgPool,
    auth: &AuthCtx,
    lease_seconds: Option<i64>,
) -> BusResult<ClaimResult> {
    let lease = lease_seconds
        .unwrap_or(DEFAULT_LEASE_SECS)
        .clamp(30, MAX_LEASE_SECS);

    // Fenced like the named claim: a replaced connection does not pick up
    // work as the window that replaced it.
    let mut tx = pool.begin().await?;
    super::sessions::guard(&mut tx, auth).await?;

    let picked: Option<(Uuid, String)> = sqlx::query_as(
        r#"
        WITH candidate AS (
            SELECT id
            FROM tasks
            WHERE team_id = $1
              AND (status = 'open'
                   OR (status = 'claimed' AND lease_expires_at < now()))
              AND NOT EXISTS (
                  SELECT 1
                  FROM task_deps td JOIN tasks d ON d.id = td.blocked_by_task_id
                  WHERE td.task_id = tasks.id AND d.status <> 'done'
              )
            ORDER BY created_at
            LIMIT 1
            FOR UPDATE SKIP LOCKED
        )
        UPDATE tasks t
        SET status = 'claimed',
            claimed_by = $2,
            claimed_session = $4,
            claimed_at = now(),
            lease_expires_at = now() + make_interval(secs => $3),
            updated_at = now()
        FROM candidate c
        WHERE t.id = c.id
        RETURNING t.id, t.key
        "#,
    )
    .bind(auth.team_id)
    .bind(auth.agent_id)
    .bind(lease as f64)
    .bind(&auth.session)
    .fetch_optional(&mut *tx)
    .await?;

    match picked {
        Some((id, key)) => {
            log_event_tx(
                &mut tx,
                id,
                auth.agent_id,
                "claimed",
                Some("via claim_next_task"),
            )
            .await?;
            tx.commit().await?;
            Ok(ClaimResult {
                claimed: true,
                task: Some(fetch_task(pool, auth, &key).await?),
                reason: None,
            })
        }
        None => {
            tx.rollback().await?;
            Ok(ClaimResult {
                claimed: false,
                task: None,
                reason: Some("no unclaimed task available".into()),
            })
        }
    }
}

pub async fn renew_lease(
    pool: &PgPool,
    auth: &AuthCtx,
    key: &str,
    lease_seconds: Option<i64>,
) -> BusResult<TaskInfo> {
    let key = normalize_key(key)?;
    let lease = lease_seconds
        .unwrap_or(DEFAULT_LEASE_SECS)
        .clamp(30, MAX_LEASE_SECS);

    // Same fence as claiming and completing: a stale connection does not
    // extend the replacement window's lease.
    let mut tx = pool.begin().await?;
    super::sessions::guard(&mut tx, auth).await?;
    let updated: Option<(Uuid,)> = sqlx::query_as(
        r#"
        UPDATE tasks
        SET lease_expires_at = now() + make_interval(secs => $1),
            updated_at = now()
        WHERE team_id = $2 AND key = $3 AND claimed_by = $4 AND status = 'claimed'
          AND COALESCE(claimed_session, '') = $5
          -- A lease that lapsed is not renewed, it is claimed again: the task
          -- has been open to everyone since, and ownership is re-established
          -- through the same door as everyone else's.
          AND (lease_expires_at IS NULL OR lease_expires_at > now())
        RETURNING id
        "#,
    )
    .bind(lease as f64)
    .bind(auth.team_id)
    .bind(&key)
    .bind(auth.agent_id)
    .bind(&auth.session)
    .fetch_optional(&mut *tx)
    .await?;

    if updated.is_none() {
        tx.rollback().await?;
        return Err(no_claim_here(pool, auth, &key).await);
    }
    tx.commit().await?;
    fetch_task(pool, auth, &key).await
}

pub async fn release_task(pool: &PgPool, auth: &AuthCtx, key: &str) -> BusResult<TaskInfo> {
    let key = normalize_key(key)?;
    // Fenced like the rest: a connection that has been replaced does not
    // put back the claim its replacement is holding.
    let mut tx = pool.begin().await?;
    super::sessions::guard(&mut tx, auth).await?;
    let updated: Option<(Uuid,)> = sqlx::query_as(
        r#"
        UPDATE tasks
        SET status = 'open',
            claimed_by = NULL,
            -- Cleared with the holder it belongs to. Leaving it behind made a
            -- released task report claimed_by null next to a session name,
            -- which reads as an active holder that does not exist.
            claimed_session = NULL,
            claimed_at = NULL,
            lease_expires_at = NULL,
            updated_at = now()
        WHERE team_id = $1 AND key = $2 AND claimed_by = $3 AND status = 'claimed'
          AND COALESCE(claimed_session, '') = $4
        RETURNING id
        "#,
    )
    .bind(auth.team_id)
    .bind(&key)
    .bind(auth.agent_id)
    .bind(&auth.session)
    .fetch_optional(&mut *tx)
    .await?;

    match updated {
        Some((id,)) => {
            log_event_tx(&mut tx, id, auth.agent_id, "released", None).await?;
            tx.commit().await?;
            fetch_task(pool, auth, &key).await
        }
        None => {
            tx.rollback().await?;
            Err(no_claim_here(pool, auth, &key).await)
        }
    }
}

pub async fn complete_task(
    pool: &PgPool,
    auth: &AuthCtx,
    key: &str,
    result: Option<String>,
) -> BusResult<TaskInfo> {
    let key = normalize_key(key)?;
    let result = match result.as_deref() {
        Some(r) => Some(super::check_text("task result", r, MAX_RESULT_BYTES)?),
        None => None,
    };
    // Serialised with the mutation, like claim_task: a request already
    // queued when its window was resumed must not finish the work of the
    // session that replaced it. The holder predicate below cannot catch
    // that on its own — a resumed window has the same agent and the same
    // label, and only the epoch tells them apart.
    let mut tx = pool.begin().await?;
    super::sessions::guard(&mut tx, auth).await?;

    // A claim has to mean something on the way out too. Completing is the
    // one write that ended someone else's work: anyone on the team could
    // mark a task done while its holder was still doing it, and the holder
    // found out when its next renew was refused with "it is done".
    //
    // The lease decides, exactly as it does for claiming. An unclaimed task
    // is anyone's to finish; a claim whose lease has expired is fair game,
    // which is what a lease is for; a live claim belongs to its holder.
    let updated: Option<(Uuid,)> = sqlx::query_as(
        r#"
        UPDATE tasks
        SET status = 'done',
            result = $1,
            claimed_by = NULL,
            claimed_session = NULL,
            claimed_at = NULL,
            lease_expires_at = NULL,
            updated_at = now()
        WHERE team_id = $2 AND key = $3 AND status IN ('open', 'claimed')
          AND (
              status = 'open'
              OR (claimed_by = $4 AND COALESCE(claimed_session, '') = $5)
              OR (lease_expires_at IS NOT NULL AND lease_expires_at <= now())
          )
          AND NOT EXISTS (
              SELECT 1 FROM task_deps td
              JOIN tasks prerequisite ON prerequisite.id = td.blocked_by_task_id
              WHERE td.task_id = tasks.id AND prerequisite.status <> 'done'
          )
        RETURNING id
        "#,
    )
    .bind(result.as_deref())
    .bind(auth.team_id)
    .bind(&key)
    .bind(auth.agent_id)
    .bind(&auth.session)
    .fetch_optional(&mut *tx)
    .await?;

    match updated {
        Some((id,)) => {
            log_event_tx(&mut tx, id, auth.agent_id, "completed", result.as_deref()).await?;
            tx.commit().await?;
            fetch_task(pool, auth, &key).await
        }
        None => {
            tx.rollback().await?;
            let current = fetch_task(pool, auth, &key).await?;
            if current.status == "claimed" {
                // Somebody is working on it right now. Say who, and for how
                // much longer, so the caller can do something about it.
                return Err(BusError::conflict(format!(
                    "task '{key}' is {}. Completing it would end work somebody else is \
                     doing; ask them, or wait for the lease to expire.",
                    holder_reason(auth, &current)
                )));
            }
            Err(BusError::conflict(format!(
                "task '{key}' is already {}",
                current.status
            )))
        }
    }
}

/// A session may change an unfinished task only when it owns its live claim.
fn claimed_by_another_session(
    auth: &AuthCtx,
    status: &str,
    claimed_by: Option<Uuid>,
    claimed_session: Option<&str>,
    lease_expires_at: Option<chrono::DateTime<chrono::Utc>>,
) -> bool {
    status == "claimed"
        && lease_expires_at.is_none_or(|expiry| expiry > chrono::Utc::now())
        && (claimed_by != Some(auth.agent_id) || claimed_session.unwrap_or("") != auth.session)
}

/// Cancel an unfinished, safely isolated task.  A cancellation never resolves a
/// dependency: callers must re-plan active dependents instead of silently
/// making them claimable.
pub async fn cancel_task(
    pool: &PgPool,
    auth: &AuthCtx,
    key: &str,
    reason: &str,
) -> BusResult<TaskInfo> {
    let key = normalize_key(key)?;
    let reason = normalize_transition_reason(reason)?;
    let mut tx = pool.begin().await?;
    super::sessions::guard(&mut tx, auth).await?;

    let task: Option<(
        Uuid,
        String,
        Option<Uuid>,
        Option<String>,
        Option<chrono::DateTime<chrono::Utc>>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT id, status, claimed_by, claimed_session, lease_expires_at, cancellation_reason
             FROM tasks WHERE team_id = $1 AND key = $2 FOR UPDATE",
    )
    .bind(auth.team_id)
    .bind(&key)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((id, status, claimed_by, claimed_session, lease_expires_at, old_reason)) = task else {
        return Err(BusError::not_found(format!("task '{key}'")));
    };

    if status == "cancelled" {
        if old_reason.as_deref() == Some(reason.as_str()) {
            tx.commit().await?;
            return fetch_task(pool, auth, &key).await;
        }
        return Err(BusError::conflict(format!(
            "task '{key}' is already cancelled with a different reason"
        )));
    }
    if status == "done" {
        return Err(BusError::conflict(format!(
            "task '{key}' is already done and its accepted result is immutable"
        )));
    }
    if status == "superseded" {
        return Err(BusError::conflict(format!(
            "task '{key}' is already superseded"
        )));
    }
    if claimed_by_another_session(
        auth,
        &status,
        claimed_by,
        claimed_session.as_deref(),
        lease_expires_at,
    ) {
        return Err(BusError::conflict(format!(
            "task '{key}' has a live claim held by another session"
        )));
    }

    let active_dependents: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM task_deps td
            JOIN tasks dependent ON dependent.id = td.task_id
            WHERE td.blocked_by_task_id = $1
              AND dependent.status IN ('open', 'claimed')
        )",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    if active_dependents {
        return Err(BusError::conflict(format!(
            "task '{key}' has active dependents; cancellation would require an explicit safe rewire"
        )));
    }

    sqlx::query(
        "UPDATE tasks
         SET status = 'cancelled', cancellation_reason = $1,
             claimed_by = NULL, claimed_session = NULL, claimed_at = NULL,
             lease_expires_at = NULL, updated_at = now()
         WHERE id = $2",
    )
    .bind(&reason)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    log_event_tx(&mut tx, id, auth.agent_id, "cancelled", Some(&reason)).await?;
    sqlx::query(
        "INSERT INTO task_transition_audit
         (task_id, team_id, actor_agent_id, actor_session, transition, reason)
         VALUES ($1, $2, $3, $4, 'cancelled', $5)",
    )
    .bind(id)
    .bind(auth.team_id)
    .bind(auth.agent_id)
    .bind(&auth.session)
    .bind(&reason)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    fetch_task(pool, auth, &key).await
}

/// Mark a task as superseded without rewriting historical dependency edges.
/// Serial advisory locks make a pair of opposing requests observe each other,
/// so they cannot form a successor cycle.
pub async fn supersede_task(
    pool: &PgPool,
    auth: &AuthCtx,
    key: &str,
    successor_key: &str,
    reason: &str,
) -> BusResult<TaskInfo> {
    let key = normalize_key(key)?;
    let successor_key = normalize_key(successor_key)?;
    let reason = normalize_transition_reason(reason)?;
    if key == successor_key {
        return Err(BusError::invalid("a task cannot supersede itself"));
    }

    let mut tx = pool.begin().await?;
    super::sessions::guard(&mut tx, auth).await?;
    let mut lock_names = [
        format!("task-transition:{}:{key}", auth.team_id),
        format!("task-transition:{}:{successor_key}", auth.team_id),
    ];
    lock_names.sort();
    for lock_name in lock_names {
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(lock_name)
            .execute(&mut *tx)
            .await?;
    }

    let source: Option<(
        Uuid,
        String,
        Option<Uuid>,
        Option<String>,
        Option<chrono::DateTime<chrono::Utc>>,
        Option<Uuid>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT id, status, claimed_by, claimed_session, lease_expires_at,
                    superseded_by_task_id, supersede_reason
             FROM tasks WHERE team_id = $1 AND key = $2 FOR UPDATE",
    )
    .bind(auth.team_id)
    .bind(&key)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((
        source_id,
        source_status,
        source_claimed_by,
        source_session,
        source_lease,
        old_successor,
        old_reason,
    )) = source
    else {
        return Err(BusError::not_found(format!("task '{key}'")));
    };

    let successor: Option<(
        Uuid,
        String,
        Option<Uuid>,
        Option<String>,
        Option<chrono::DateTime<chrono::Utc>>,
    )> = sqlx::query_as(
        "SELECT id, status, claimed_by, claimed_session, lease_expires_at
             FROM tasks WHERE team_id = $1 AND key = $2 FOR UPDATE",
    )
    .bind(auth.team_id)
    .bind(&successor_key)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((
        successor_id,
        successor_status,
        successor_claimed_by,
        successor_session,
        successor_lease,
    )) = successor
    else {
        return Err(BusError::not_found(format!(
            "successor task '{successor_key}' does not exist in this team"
        )));
    };

    if source_status == "superseded" {
        if old_successor == Some(successor_id) && old_reason.as_deref() == Some(reason.as_str()) {
            tx.commit().await?;
            return fetch_task(pool, auth, &key).await;
        }
        return Err(BusError::conflict(format!(
            "task '{key}' is already superseded by a different request"
        )));
    }
    if source_status == "done" {
        return Err(BusError::conflict(format!(
            "task '{key}' is already done and its accepted result is immutable"
        )));
    }
    if source_status == "cancelled" {
        return Err(BusError::conflict(format!(
            "task '{key}' is already cancelled"
        )));
    }
    if !matches!(successor_status.as_str(), "open" | "claimed") {
        return Err(BusError::conflict(format!(
            "successor task '{successor_key}' is {successor_status}, not active"
        )));
    }
    if claimed_by_another_session(
        auth,
        &source_status,
        source_claimed_by,
        source_session.as_deref(),
        source_lease,
    ) || claimed_by_another_session(
        auth,
        &successor_status,
        successor_claimed_by,
        successor_session.as_deref(),
        successor_lease,
    ) {
        return Err(BusError::conflict(
            "supersession requires both tasks to be unclaimed or claimed by this session",
        ));
    }

    // Prevent a permanently blocked successor or downstream DAG. Creation
    // locks prerequisite rows, so a concurrent new dependent cannot escape
    // this check while the source row is FOR UPDATE locked.
    let active_dependents: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM task_deps td
            JOIN tasks dependent ON dependent.id = td.task_id
            WHERE td.blocked_by_task_id = $1
              AND dependent.status IN ('open', 'claimed')
        )",
    )
    .bind(source_id)
    .fetch_one(&mut *tx)
    .await?;
    if active_dependents {
        return Err(BusError::conflict(format!(
            "task '{key}' has active dependents; supersession needs an explicit safe rewire"
        )));
    }

    let cycle: bool = sqlx::query_scalar(
        "WITH RECURSIVE successors(id) AS (
             SELECT $1::uuid
             UNION
             SELECT t.superseded_by_task_id
             FROM tasks t JOIN successors s ON t.id = s.id
             WHERE t.superseded_by_task_id IS NOT NULL
         )
         SELECT EXISTS (SELECT 1 FROM successors WHERE id = $2)",
    )
    .bind(successor_id)
    .bind(source_id)
    .fetch_one(&mut *tx)
    .await?;
    if cycle {
        return Err(BusError::conflict(
            "supersession would create a successor cycle",
        ));
    }

    sqlx::query(
        "UPDATE tasks
         SET status = 'superseded', superseded_by_task_id = $1,
             supersede_reason = $2, claimed_by = NULL, claimed_session = NULL,
             claimed_at = NULL, lease_expires_at = NULL, updated_at = now()
         WHERE id = $3",
    )
    .bind(successor_id)
    .bind(&reason)
    .bind(source_id)
    .execute(&mut *tx)
    .await?;
    log_event_tx(
        &mut tx,
        source_id,
        auth.agent_id,
        "superseded",
        Some(&format!("{successor_key}: {reason}")),
    )
    .await?;
    sqlx::query(
        "INSERT INTO task_transition_audit
         (task_id, team_id, actor_agent_id, actor_session, transition, reason, successor_task_id)
         VALUES ($1, $2, $3, $4, 'superseded', $5, $6)",
    )
    .bind(source_id)
    .bind(auth.team_id)
    .bind(auth.agent_id)
    .bind(&auth.session)
    .bind(&reason)
    .bind(successor_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    fetch_task(pool, auth, &key).await
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_LEASE_SECS, MAX_LIMIT, NormalizedSearch, SearchFailure, TaskSearchQuery,
        classify_search_failure, escape_contains, keyword_search_clause, normalize_search,
        search_clause, search_cursor_binding, search_failure_error,
    };

    #[test]
    fn postgres_search_failures_map_timeout_cancellation_and_invalid_regex() {
        assert_eq!(
            classify_search_failure(Some("2201B"), "invalid regular expression"),
            Some(SearchFailure::InvalidRegex),
        );
        assert_eq!(
            classify_search_failure(
                Some("57014"),
                "canceling statement due to statement timeout"
            ),
            Some(SearchFailure::Timeout),
        );
        assert_eq!(
            classify_search_failure(Some("57014"), "canceling statement due to user request"),
            Some(SearchFailure::Cancelled),
        );
        assert_eq!(
            classify_search_failure(Some("57014"), "canceling statement"),
            None,
        );
        assert_eq!(
            classify_search_failure(Some("23505"), "duplicate key value"),
            None,
        );

        assert!(
            search_failure_error(SearchFailure::InvalidRegex)
                .to_string()
                .contains("not a valid PostgreSQL regular expression")
        );
        assert!(
            search_failure_error(SearchFailure::Timeout)
                .to_string()
                .contains("250ms execution limit")
        );
        assert!(
            search_failure_error(SearchFailure::Cancelled)
                .to_string()
                .contains("cancelled before completion")
        );
    }

    #[test]
    fn search_defaults_and_rejects_unknown_options() {
        let normalized = normalize_search(TaskSearchQuery {
            search: Some("  cargo  ".to_owned()),
            search_mode: None,
            search_fields: None,
            search_language: None,
        })
        .unwrap()
        .unwrap();
        assert_eq!(normalized.query, "cargo");
        assert_eq!(normalized.mode, "keywords");
        assert_eq!(normalized.fields, "both");
        assert_eq!(normalized.language, "simple");
        assert!(
            normalize_search(TaskSearchQuery {
                search: Some("x".to_owned()),
                search_mode: Some("sql".to_owned()),
                search_fields: None,
                search_language: None,
            })
            .is_err()
        );
    }

    #[test]
    fn keyword_search_uses_static_index_matching_language_expressions() {
        for language in ["simple", "english", "russian"] {
            let normalized = normalize_search(TaskSearchQuery {
                search: Some("cargo".to_owned()),
                search_mode: Some("keywords".to_owned()),
                search_fields: Some("both".to_owned()),
                search_language: Some(language.to_owned()),
            })
            .unwrap()
            .unwrap();
            let clause = keyword_search_clause(&normalized);
            assert!(clause.contains(&format!("to_tsvector('{language}'")));
            assert!(clause.contains("setweight(to_tsvector"));
            assert!(clause.contains("$8::text IS NOT NULL"));
            assert!(!clause.contains("$8::regconfig"));
        }
    }

    #[test]
    fn keyword_field_filters_keep_indexed_candidate_expression_and_field_semantics() {
        for fields in ["title", "description"] {
            let normalized = normalize_search(TaskSearchQuery {
                search: Some("cargo".to_owned()),
                search_mode: Some("keywords".to_owned()),
                search_fields: Some(fields.to_owned()),
                search_language: Some("simple".to_owned()),
            })
            .unwrap()
            .unwrap();
            let clause = keyword_search_clause(&normalized);
            assert!(clause.matches("setweight(to_tsvector").count() >= 2);
            assert!(clause.contains(&format!("to_tsvector('simple', coalesce(t.{fields}")));
            assert!(!clause.contains("$8::regconfig"));
        }
    }

    #[test]
    fn empty_search_preserves_no_search_path_and_contains_escapes_literals() {
        assert!(
            normalize_search(TaskSearchQuery {
                search: Some("   ".to_owned()),
                search_mode: Some("regex".to_owned()),
                search_fields: Some("title".to_owned()),
                search_language: Some("russian".to_owned()),
            })
            .unwrap()
            .is_none()
        );
        assert_eq!(escape_contains("100%_\\").as_bytes(), b"100\\%\\_\\\\");
        let normalized = normalize_search(TaskSearchQuery {
            search: Some("needle".to_owned()),
            search_mode: Some("contains".to_owned()),
            search_fields: Some("title".to_owned()),
            search_language: None,
        })
        .unwrap()
        .unwrap();
        let contains_title = search_clause(Some(&normalized));
        assert!(contains_title.contains("ILIKE"));
        assert!(contains_title.contains("ESCAPE chr(92)"));
        let contains_description = search_clause(Some(&NormalizedSearch {
            fields: "description",
            ..normalized.clone()
        }));
        assert!(contains_description.contains("ESCAPE chr(92)"));
        let contains_both = search_clause(Some(&NormalizedSearch {
            fields: "both",
            ..normalized
        }));
        assert_eq!(contains_both.matches("ESCAPE chr(92)").count(), 2);
    }

    #[test]
    fn search_cursor_binding_covers_all_search_options() {
        let normalized = normalize_search(TaskSearchQuery {
            search: Some("Cargo".to_owned()),
            search_mode: Some("regex".to_owned()),
            search_fields: Some("description".to_owned()),
            search_language: Some("russian".to_owned()),
        })
        .unwrap()
        .unwrap();
        let binding = search_cursor_binding(Some("api#"), &normalized);
        assert!(binding.contains("Cargo"));
        assert!(binding.contains("regex"));
        assert!(binding.contains("description"));
        assert!(binding.contains("russian"));
        assert_ne!(
            binding,
            search_cursor_binding(
                Some("api#"),
                &normalize_search(TaskSearchQuery {
                    search: Some("Cargo".to_owned()),
                    search_mode: Some("keywords".to_owned()),
                    search_fields: Some("description".to_owned()),
                    search_language: Some("russian".to_owned()),
                })
                .unwrap()
                .unwrap()
            )
        );
    }

    #[test]
    fn search_rejects_oversized_and_nul_regex_input() {
        assert!(
            normalize_search(TaskSearchQuery {
                search: Some("x".repeat(513)),
                search_mode: Some("regex".to_owned()),
                search_fields: None,
                search_language: None,
            })
            .is_err()
        );
        assert!(
            normalize_search(TaskSearchQuery {
                search: Some(format!("a{}b", '\0')),
                search_mode: Some("regex".to_owned()),
                search_fields: None,
                search_language: None,
            })
            .is_err()
        );
    }

    #[test]
    fn list_limit_allows_a_bounded_high_page_without_changing_small_limits() {
        assert_eq!(DEFAULT_LEASE_SECS, 900);
        assert_eq!(50_i64.clamp(1, MAX_LIMIT), 50);
        assert_eq!(200_i64.clamp(1, MAX_LIMIT), 200);
        assert_eq!(10_000_i64.clamp(1, MAX_LIMIT), 10_000);
        assert_eq!(25_000_i64.clamp(1, MAX_LIMIT), 10_000);
    }

    #[test]
    fn list_limit_never_becomes_zero_or_negative() {
        assert_eq!(0_i64.clamp(1, MAX_LIMIT), 1);
        assert_eq!((-50_i64).clamp(1, MAX_LIMIT), 1);
    }
}
