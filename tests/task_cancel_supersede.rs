#![allow(clippy::too_many_lines)]

#[path = "task_pagination_fixture.rs"]
mod fixture;

use ai_crew_sync::{
    auth::AuthCtx,
    store::tasks::{self, CreateInput},
};
use fixture::{cleanup, setup_fixture};
use uuid::Uuid;

fn auth(fixture: &fixture::Fixture, agent_id: Uuid, agent_name: &str, session: &str) -> AuthCtx {
    AuthCtx {
        agent_id,
        agent_name: agent_name.to_owned(),
        team_id: fixture.team_id,
        team_slug: format!("cancel-supersede-{}", fixture.team_id.simple()),
        session: session.to_owned(),
        session_id: None,
        session_epoch: None,
        token_id: None,
    }
}

async fn create(
    fixture: &fixture::Fixture,
    actor: &AuthCtx,
    key: &str,
    depends_on: Vec<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    tasks::create_task(
        &fixture.pool,
        actor,
        CreateInput {
            key: key.to_owned(),
            title: key.to_owned(),
            description: None,
            metadata: None,
            depends_on,
        },
    )
    .await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires CREWSYNC_ENABLE_LARGE_FIXTURE=1 and disposable TEST_DATABASE_URL"]
async fn cancellation_and_supersession_are_fenced_audited_and_dependency_safe() {
    let fixture = setup_fixture().await.expect("safe test database");
    let foreign_fixture = setup_fixture().await.expect("isolated second team");
    let foreign = auth(&foreign_fixture, foreign_fixture.agent_id, "foreign", "foreign-session");
    let primary = auth(&fixture, fixture.agent_id, "fixture-primary", "cancel-owner");
    let other = auth(
        &fixture,
        fixture.other_agent_id,
        "fixture-secondary",
        "cancel-other",
    );

    let result = async {
        create(&fixture, &primary, "upstream", vec![]).await?;
        create(&fixture, &primary, "dependent", vec!["upstream".to_owned()]).await?;

        let dependency_error = tasks::cancel_task(
            &fixture.pool,
            &primary,
            "upstream",
            "obsolete upstream",
        )
        .await
        .expect_err("active dependent must prevent unsafe cancellation");
        assert!(dependency_error.to_string().contains("active dependents"));

        assert!(tasks::complete_task(&fixture.pool, &primary, "dependent", None)
            .await.is_err(), "blocked completion must fail closed");
        // Model an old imported DONE child while testing transition history.
        // A completed dependent is no longer active, so cancellation can preserve
        // the edge and history without accidentally making new work claimable.
        sqlx::query("UPDATE tasks SET status = 'done' WHERE team_id = $1 AND key = 'dependent'")
            .bind(fixture.team_id)
            .execute(&fixture.pool)
            .await?;
        let cancelled =
            tasks::cancel_task(&fixture.pool, &primary, "upstream", "obsolete upstream").await?;
        assert_eq!(cancelled.status, "cancelled");
        assert_eq!(cancelled.cancellation_reason.as_deref(), Some("obsolete upstream"));
        let repeated =
            tasks::cancel_task(&fixture.pool, &primary, "upstream", "obsolete upstream").await?;
        assert_eq!(repeated.status, "cancelled");
        assert!(tasks::cancel_task(&fixture.pool, &primary, "upstream", "different")
            .await
            .is_err());

        assert!(create(
            &fixture, &primary, "late-cancel-dependent", vec!["upstream".to_owned()],
        ).await.is_err());

        create(&fixture, &primary, "racing-parent", vec![]).await?;
        let (created, cancelled) = tokio::join!(
            create(&fixture, &primary, "racing-child", vec!["racing-parent".to_owned()]),
            tasks::cancel_task(&fixture.pool, &primary, "racing-parent", "OBSOLETE"),
        );
        assert_ne!(
            created.is_ok(), cancelled.is_ok(),
            "either the dependent wins or the cancellation wins, not both",
        );

        create(&fixture, &primary, "source", vec![]).await?;
        create(&fixture, &primary, "successor", vec![]).await?;
        assert!(tasks::cancel_task(
            &fixture.pool, &foreign, "source", "foreign team attempt",
        ).await.is_err());
        tasks::claim_task(&fixture.pool, &other, "source", Some(60)).await?;
        assert!(tasks::supersede_task(
            &fixture.pool,
            &primary,
            "source",
            "successor",
            "replace source",
        )
        .await
        .is_err());
        tasks::release_task(&fixture.pool, &other, "source").await?;

        let superseded = tasks::supersede_task(
            &fixture.pool,
            &primary,
            "source",
            "successor",
            "replace source",
        )
        .await?;
        assert_eq!(superseded.status, "superseded");
        assert_eq!(superseded.superseded_by.as_deref(), Some("successor"));
        assert_eq!(superseded.supersede_reason.as_deref(), Some("replace source"));
        assert!(tasks::supersede_task(
            &fixture.pool,
            &primary,
            "successor",
            "source",
            "cycle",
        )
        .await
        .is_err());
        assert!(tasks::supersede_task(
            &fixture.pool,
            &primary,
            "successor",
            "successor",
            "self",
        )
        .await
        .is_err());

        let replay = tasks::supersede_task(
            &fixture.pool, &primary, "source", "successor", "replace source",
        ).await?;
        assert_eq!(replay.status, "superseded");
        assert!(tasks::supersede_task(
            &fixture.pool, &primary, "source", "successor", "different reason",
        ).await.is_err());

        create(&fixture, &primary, "foreign-claimed", vec![]).await?;
        tasks::claim_task(&fixture.pool, &other, "foreign-claimed", Some(60)).await?;
        assert!(tasks::cancel_task(
            &fixture.pool, &primary, "foreign-claimed", "DUPLICATE",
        ).await.is_err());
        tasks::release_task(&fixture.pool, &other, "foreign-claimed").await?;

        create(&fixture, &primary, "missing-target", vec![]).await?;
        assert!(tasks::supersede_task(
            &fixture.pool, &primary, "missing-target", "unknown-successor", "SUPERSEDED",
        ).await.is_err());
        assert!(tasks::cancel_task(
            &fixture.pool, &primary, "missing-target", "   ",
        ).await.is_err());
        tasks::complete_task(&fixture.pool, &primary, "missing-target", None).await?;
        assert!(tasks::cancel_task(
            &fixture.pool, &primary, "missing-target", "OBSOLETE",
        ).await.is_err());

        create(&fixture, &primary, "owned-completion", vec![]).await?;
        assert!(tasks::claim_task(
            &fixture.pool, &primary, "owned-completion", Some(60),
        ).await?.claimed);
        let done = tasks::complete_task(
            &fixture.pool, &primary, "owned-completion", None,
        ).await?;
        assert_eq!(done.status, "done");
        assert!(done.claimed_by.is_none() && done.claimed_session.is_none());
        assert!(done.claimed_at.is_none());
        let mine = tasks::list_tasks(&fixture.pool, &primary, None, true, 100).await?;
        assert!(!mine.tasks.iter().any(|task| task.key == "owned-completion"));

        create(&fixture, &primary, "legacy-null", vec![]).await?;
        assert!(tasks::claim_task(
            &fixture.pool, &other, "legacy-null", Some(60),
        ).await?.claimed);
        sqlx::query(
            "UPDATE tasks SET lease_expires_at = NULL
             WHERE team_id = $1 AND key = 'legacy-null'",
        )
        .bind(fixture.team_id)
        .execute(&fixture.pool)
        .await?;
        assert!(!tasks::claim_task(
            &fixture.pool, &primary, "legacy-null", Some(60),
        ).await?.claimed);
        assert!(tasks::complete_task(
            &fixture.pool, &primary, "legacy-null", None,
        ).await.is_err());
        tasks::renew_lease(&fixture.pool, &other, "legacy-null", Some(60)).await?;
        tasks::release_task(&fixture.pool, &other, "legacy-null").await?;

        create(&fixture, &primary, "active-parent", vec![]).await?;
        create(&fixture, &primary, "replacement", vec![]).await?;
        create(&fixture, &primary, "active-child", vec!["active-parent".to_owned()]).await?;
        assert!(!tasks::claim_task(&fixture.pool, &primary, "active-child", Some(60)).await?.claimed);
        assert!(tasks::supersede_task(
            &fixture.pool, &primary, "active-parent", "replacement", "canonical replacement",
        ).await.is_err(), "active dependent requires an explicit rewire");
        assert!(!tasks::claim_task(&fixture.pool, &primary, "active-child", Some(60)).await?.claimed);
        assert!(tasks::supersede_task(
            &fixture.pool, &primary, "active-parent", "active-child", "invalid cycle",
        ).await.is_err());

        create(&fixture, &primary, "race-source", vec![]).await?;
        create(&fixture, &primary, "race-left", vec![]).await?;
        create(&fixture, &primary, "race-right", vec![]).await?;
        let (left, right) = tokio::join!(
            tasks::supersede_task(
                &fixture.pool, &primary, "race-source", "race-left", "race",
            ),
            tasks::supersede_task(
                &fixture.pool, &primary, "race-source", "race-right", "race",
            ),
        );
        assert_eq!(left.is_ok() as usize + right.is_ok() as usize, 1);

        let audit_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM task_transition_audit WHERE team_id = $1",
        )
        .bind(fixture.team_id)
        .fetch_one(&fixture.pool)
        .await?;
        assert_eq!(audit_count, 3);
        create(&foreign_fixture, &foreign, "other-tenant-target", vec![]).await?;
        let forged_link = sqlx::query(
            "UPDATE tasks SET superseded_by_task_id = (
                SELECT id FROM tasks WHERE team_id = $1 AND key = 'other-tenant-target'
             ) WHERE team_id = $2 AND key = 'successor'",
        )
        .bind(foreign_fixture.team_id)
        .bind(fixture.team_id)
        .execute(&fixture.pool)
        .await;
        assert!(forged_link.is_err(), "database must reject a cross-team successor");

        let forged_audit = sqlx::query(
            "INSERT INTO task_transition_audit
             (task_id, team_id, transition, reason)
             SELECT id, $1, 'cancelled', 'forged'
             FROM tasks WHERE team_id = $2 AND key = 'upstream'",
        )
        .bind(foreign_fixture.team_id)
        .bind(fixture.team_id)
        .execute(&fixture.pool)
        .await;
        assert!(forged_audit.is_err(), "database must reject a cross-team audit");

        let deleted_history = sqlx::query(
            "DELETE FROM tasks WHERE team_id = $1 AND key = 'upstream'",
        )
        .bind(fixture.team_id)
        .execute(&fixture.pool)
        .await;
        assert!(deleted_history.is_err(), "transitioned tasks must retain durable audit");

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    // Only the disposable fixture cleanup may explicitly discard audit rows.
    // Real task deletion remains restricted by the production FK.
    sqlx::query("DELETE FROM task_transition_audit WHERE team_id = $1")
        .bind(fixture.team_id)
        .execute(&fixture.pool)
        .await
        .expect("cleanup disposable audit rows");
    cleanup(foreign_fixture).await.expect("cleanup foreign fixture");
    cleanup(fixture).await.expect("cleanup fixture");
    result.expect("cancel/supersede integration");
}
