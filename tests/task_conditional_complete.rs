use ai_crew_sync::store::tasks::{
    CheckedCompletionOutcome, CheckedCompletionRejection, CheckedProofKind,
    CompleteTaskCheckedInput, validate_checked_completion,
};
use sha2::{Digest, Sha256};
use uuid::Uuid;

fn input() -> CompleteTaskCheckedInput {
    CompleteTaskCheckedInput {
        key: "original-task".to_owned(),
        attempt_id: "original-attempt".to_owned(),
        session_id: "original-crew-session".to_owned(),
        owner_agent_id: Uuid::new_v4(),
        owner_session: "implementer".to_owned(),
        owner_session_id: Uuid::new_v4(),
        owner_epoch: 1,
        owner_generation: 1,
        source_sha256: "a".repeat(64),
        result: " exact result\n".to_owned(),
        result_sha256: hex::encode(Sha256::digest(b" exact result\n")),
        reviewer_session: "independent-reviewer".to_owned(),
        reviewer_session_id: Uuid::new_v4(),
        reviewer_evidence_note_key: "review-note".to_owned(),
        reviewer_evidence_sha256: "b".repeat(64),
        reducer_actor_id: Uuid::new_v4(),
    }
}

#[test]
fn complete_dto_round_trips_without_rewriting_original_binding_or_result() {
    let request = input();
    validate_checked_completion(&request).expect("well-formed request, not authoritative proof");
    let decoded: CompleteTaskCheckedInput =
        serde_json::from_value(serde_json::to_value(&request).unwrap()).unwrap();
    assert_eq!(decoded.key, request.key);
    assert_eq!(decoded.attempt_id, request.attempt_id);
    assert_eq!(decoded.session_id, request.session_id);
    assert_eq!(decoded.result, request.result);
    assert_eq!(decoded.owner_generation, request.owner_generation);
}

#[test]
fn caller_declared_qa_and_unknown_fields_are_not_a_proof_channel() {
    let mut serialized = serde_json::to_value(input()).unwrap();
    serialized["qa"] = serde_json::json!(true);
    assert!(serde_json::from_value::<CompleteTaskCheckedInput>(serialized).is_err());
    let mut serialized = serde_json::to_value(input()).unwrap();
    serialized["description"] = serde_json::json!("independently accepted");
    assert!(serde_json::from_value::<CompleteTaskCheckedInput>(serialized).is_err());
}

#[test]
fn legacy_payload_is_not_implicitly_upgraded_to_checked_completion() {
    let serialized = serde_json::json!({"key": "original-task", "result": "finished"});
    assert!(serde_json::from_value::<CompleteTaskCheckedInput>(serialized).is_err());
}

#[test]
fn hashes_are_canonical_and_result_hash_covers_exact_utf8_bytes() {
    for bad_hash in ["a".repeat(63), "A".repeat(64), "z".repeat(64)] {
        let mut request = input();
        request.source_sha256 = bad_hash;
        assert!(validate_checked_completion(&request).is_err());
    }
    let mut request = input();
    request.result_sha256 = hex::encode(Sha256::digest(request.result.trim().as_bytes()));
    assert!(validate_checked_completion(&request).is_err());
    let mut request = input();
    request.result = "готово\n".to_owned();
    request.result_sha256 = hex::encode(Sha256::digest(request.result.as_bytes()));
    assert!(validate_checked_completion(&request).is_ok());
}

#[test]
fn malformed_or_rewritten_binding_and_nonpositive_fences_are_rejected() {
    let mut request = input();
    request.key = " original-task ".to_owned();
    assert!(validate_checked_completion(&request).is_err());
    let mut request = input();
    request.attempt_id.clear();
    assert!(validate_checked_completion(&request).is_err());
    let mut request = input();
    request.session_id = "x".repeat(257);
    assert!(validate_checked_completion(&request).is_err());
    for generation in [0, -1] {
        let mut request = input();
        request.owner_generation = generation;
        assert!(validate_checked_completion(&request).is_err());
    }
    let mut request = input();
    request.owner_epoch = 0;
    assert!(validate_checked_completion(&request).is_err());
}

#[test]
fn self_review_and_unbounded_payloads_are_rejected() {
    let mut request = input();
    request.reviewer_session_id = request.owner_session_id;
    assert!(validate_checked_completion(&request).is_err());
    let mut request = input();
    request.reviewer_session = request.owner_session.clone();
    assert!(validate_checked_completion(&request).is_err());
    let mut request = input();
    request.result = "x".repeat(65_537);
    request.result_sha256 = hex::encode(Sha256::digest(request.result.as_bytes()));
    assert!(validate_checked_completion(&request).is_err());
}

#[test]
fn unsupported_proof_is_machine_readable_and_never_serializes_as_done() {
    let outcome = CheckedCompletionOutcome::UnsupportedProof {
        missing: vec![CheckedProofKind::IndependentReview, CheckedProofKind::ReducerAuthority],
    };
    let serialized = serde_json::to_value(outcome).unwrap();
    assert_eq!(serialized["status"], "unsupported_proof");
    assert_eq!(serialized["missing"][0], "independent_review");
    assert!(serialized.get("task").is_none());
}

#[tokio::test]
#[ignore = "requires CREWSYNC_ENABLE_CONDITIONAL_FIXTURE=1 and a disposable TEST_DATABASE_URL"]
async fn postgres_preflight_is_fenced_and_cannot_complete_without_trusted_proof() {
    use ai_crew_sync::{auth::AuthCtx, store::tasks};
    use sqlx::{AssertSqlSafe, postgres::PgPoolOptions};

    assert_eq!(std::env::var("CREWSYNC_ENABLE_CONDITIONAL_FIXTURE").as_deref(), Ok("1"));
    let database_url = std::env::var("TEST_DATABASE_URL").expect("disposable database required");
    let database_name = database_url.rsplit('/').next().unwrap_or("")
        .split('?').next().unwrap_or("");
    assert!(matches!(database_name, "crew_fixture_test" | "crewsync_conditional_test"),
        "refusing a database not explicitly named for this disposable fixture");
    let schema = format!("checked_fixture_{}", Uuid::new_v4().simple());
    let bootstrap = PgPoolOptions::new().max_connections(1).connect(&database_url)
        .await.expect("disposable PostgreSQL connection");
    sqlx::query(AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
        .execute(&bootstrap).await.unwrap();
    let search_path = format!("SET search_path TO {schema}");
    let pool = PgPoolOptions::new().max_connections(2).after_connect(move |connection, _| {
        let statement = search_path.clone();
        Box::pin(async move {
            sqlx::query(AssertSqlSafe(statement)).execute(connection).await?;
            Ok(())
        })
    }).connect(&database_url).await.unwrap();

    let test_result: Result<(), Box<dyn std::error::Error>> = async {
        sqlx::raw_sql("CREATE TABLE teams (id UUID PRIMARY KEY);
            CREATE TABLE agents (id UUID PRIMARY KEY);
            CREATE TABLE tasks (id UUID PRIMARY KEY, team_id UUID NOT NULL,
                key TEXT NOT NULL, status TEXT NOT NULL, claimed_by UUID,
                claimed_session TEXT, claimed_at TIMESTAMPTZ, lease_expires_at TIMESTAMPTZ,
                result TEXT, UNIQUE(team_id, id), UNIQUE(team_id, key));
            CREATE TABLE task_deps (task_id UUID, blocked_by_task_id UUID);")
            .execute(&pool).await?;
        let mut request = input();
        let team_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        let auth = AuthCtx {
            agent_id: request.reducer_actor_id, agent_name: "reducer".to_owned(), team_id,
            team_slug: "disposable".to_owned(), session: "reducer".to_owned(),
            session_id: None, session_epoch: None, token_id: None,
        };
        sqlx::query("INSERT INTO teams VALUES ($1)").bind(team_id).execute(&pool).await?;
        sqlx::query("INSERT INTO agents VALUES ($1)").bind(auth.agent_id).execute(&pool).await?;
        sqlx::query("INSERT INTO tasks (id, team_id, key, status, claimed_by, claimed_session,
            claimed_at, lease_expires_at) VALUES ($1,$2,$3,'claimed',$4,$5,clock_timestamp(),
            clock_timestamp() + interval '10 minutes')")
            .bind(task_id).bind(team_id).bind(&request.key).bind(request.owner_agent_id)
            .bind(&request.owner_session).execute(&pool).await?;
        let unsupported = tasks::complete_task_checked(&pool, &auth, &request).await?;
        assert!(matches!(unsupported, CheckedCompletionOutcome::UnsupportedProof { .. }));
        sqlx::raw_sql(include_str!("../migrations/0024_checked_completion_audit.sql"))
            .execute(&pool).await?;
        sqlx::query("UPDATE tasks SET claimed_at = claimed_at + interval '1 microsecond'")
            .execute(&pool).await?;
        let generation: i64 = sqlx::query_scalar("SELECT completion_generation FROM tasks")
            .fetch_one(&pool).await?;
        request.owner_generation = generation;
        let (first, retry) = tokio::join!(
            tasks::complete_task_checked(&pool, &auth, &request),
            tasks::complete_task_checked(&pool, &auth, &request),
        );
        for response in [first?, retry?] {
            assert!(matches!(response, CheckedCompletionOutcome::UnsupportedProof { .. }));
        }
        sqlx::query("UPDATE tasks SET lease_expires_at = clock_timestamp() + interval '20 minutes'")
            .execute(&pool).await?;
        let renewed: i64 = sqlx::query_scalar("SELECT completion_generation FROM tasks")
            .fetch_one(&pool).await?;
        assert_eq!(generation, renewed, "renewal must preserve the acquisition fence");
        request.owner_generation += 1;
        assert!(matches!(tasks::complete_task_checked(&pool, &auth, &request).await?,
            CheckedCompletionOutcome::Rejected { reason: CheckedCompletionRejection::StaleGeneration }));
        request.owner_generation = generation;
        let mut wrong_owner = request.clone();
        wrong_owner.owner_agent_id = Uuid::new_v4();
        assert!(matches!(tasks::complete_task_checked(&pool, &auth, &wrong_owner).await?,
            CheckedCompletionOutcome::Rejected { reason: CheckedCompletionRejection::WrongOwner }));
        let mut foreign_auth = auth.clone();
        foreign_auth.team_id = Uuid::new_v4();
        assert!(tasks::complete_task_checked(&pool, &foreign_auth, &request).await.is_err());
        let mut wrong_session = request.clone();
        wrong_session.owner_session = "another-owner-window".to_owned();
        assert!(matches!(tasks::complete_task_checked(&pool, &auth, &wrong_session).await?,
            CheckedCompletionOutcome::Rejected { reason: CheckedCompletionRejection::WrongOwner }));
        let mut wrong_reducer = request.clone();
        wrong_reducer.reducer_actor_id = Uuid::new_v4();
        assert!(matches!(tasks::complete_task_checked(&pool, &auth, &wrong_reducer).await?,
            CheckedCompletionOutcome::Rejected { reason: CheckedCompletionRejection::WrongReducerActor }));
        let prerequisite_id = Uuid::new_v4();
        sqlx::query("INSERT INTO tasks (id, team_id, key, status) VALUES ($1,$2,'upstream','cancelled')")
            .bind(prerequisite_id).bind(team_id).execute(&pool).await?;
        sqlx::query("INSERT INTO task_deps VALUES ($1,$2)")
            .bind(task_id).bind(prerequisite_id).execute(&pool).await?;
        assert!(matches!(tasks::complete_task_checked(&pool, &auth, &request).await?,
            CheckedCompletionOutcome::Rejected { reason: CheckedCompletionRejection::UnfinishedDependencies }));
        sqlx::query("UPDATE tasks SET status = 'done' WHERE id = $1")
            .bind(prerequisite_id).execute(&pool).await?;
        assert!(matches!(tasks::complete_task_checked(&pool, &auth, &request).await?,
            CheckedCompletionOutcome::UnsupportedProof { .. }));
        sqlx::query("UPDATE tasks SET lease_expires_at = NULL WHERE id = $1")
            .bind(task_id).execute(&pool).await?;
        assert!(matches!(tasks::complete_task_checked(&pool, &auth, &request).await?,
            CheckedCompletionOutcome::Rejected { reason: CheckedCompletionRejection::NoLiveClaim }));
        sqlx::query("UPDATE tasks SET lease_expires_at = clock_timestamp() - interval '1 second' WHERE id = $1")
            .bind(task_id).execute(&pool).await?;
        assert!(matches!(tasks::complete_task_checked(&pool, &auth, &request).await?,
            CheckedCompletionOutcome::Rejected { reason: CheckedCompletionRejection::NoLiveClaim }));
        let state: (String, Option<String>) = sqlx::query_as("SELECT status, result FROM tasks WHERE id = $1")
            .bind(task_id).fetch_one(&pool).await?;
        assert_eq!(state, ("claimed".to_owned(), None));
        let audit_count: i64 = sqlx::query_scalar("SELECT count(*) FROM task_checked_completion_audit")
            .fetch_one(&pool).await?;
        assert_eq!(audit_count, 0, "unsupported proof is not an accepted completion audit");
        Ok(())
    }.await;
    pool.close().await;
    sqlx::query(AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
        .execute(&bootstrap).await.unwrap();
    bootstrap.close().await;
    test_result.expect("conditional completion fixture");
}
