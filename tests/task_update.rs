#![allow(clippy::too_many_lines)]
#[path = "task_pagination_fixture.rs"]
mod fixture;

use ai_crew_sync::{
    auth::AuthCtx,
    store::tasks::{self, CreateInput, UpdateInput},
};
use fixture::setup_fixture;
use serde_json::json;
use uuid::Uuid;

fn auth(f: &fixture::Fixture, agent_id: Uuid, session: &str) -> AuthCtx {
    AuthCtx {
        agent_id,
        agent_name: session.into(),
        team_id: f.team_id,
        team_slug: format!("task-update-{}", f.team_id.simple()),
        session: session.into(),
        session_id: None,
        session_epoch: None,
        token_id: None,
    }
}
fn update(
    rev: i64,
    title: Option<&str>,
    description: Option<Option<&str>>,
    metadata: Option<Option<serde_json::Value>>,
) -> UpdateInput {
    UpdateInput {
        expected_revision: rev,
        title: title.map(|v| Some(v.to_owned())),
        description: description.map(|v| v.map(str::to_owned)),
        metadata,
    }
}

#[tokio::test]
#[ignore = "requires CREWSYNC_ENABLE_LARGE_FIXTURE=1 and disposable TEST_DATABASE_URL"]
async fn task_patch_revision_audit_lease_tenant_terminal_and_race() {
    let f = setup_fixture().await.expect("disposable test database");
    let foreign_fixture = setup_fixture().await.expect("foreign disposable team");
    let primary = auth(&f, f.agent_id, "edit-owner");
    let other = auth(&f, f.other_agent_id, "edit-foreign");
    let foreign = auth(&foreign_fixture, foreign_fixture.agent_id, "foreign-team");
    let p = &f.pool;

    tasks::create_task(
        p,
        &primary,
        CreateInput {
            key: "editable".into(),
            title: "Original".into(),
            description: Some("Detail".into()),
            metadata: Some(json!({"old":true})),
            depends_on: vec![],
        },
    )
    .await
    .unwrap();
    tasks::create_task(
        p,
        &primary,
        CreateInput {
            key: "dependent".into(),
            title: "Child".into(),
            description: None,
            metadata: None,
            depends_on: vec!["editable".into()],
        },
    )
    .await
    .unwrap();
    let original_id: Uuid =
        sqlx::query_scalar("SELECT id FROM tasks WHERE team_id=$1 AND key='editable'")
            .bind(f.team_id)
            .fetch_one(p)
            .await
            .unwrap();

    let start = tasks::get_task(p, &primary, "editable").await.unwrap();
    assert_eq!(start.task.revision, 1);
    assert!(
        tasks::update_task(
            p,
            &foreign,
            "editable",
            update(1, Some("spoof"), None, None)
        )
        .await
        .is_err()
    );
    assert!(
        tasks::update_task(
            p,
            &primary,
            "missing",
            update(1, Some("absent"), None, None)
        )
        .await
        .is_err()
    );
    assert!(
        tasks::update_task(p, &primary, "editable", update(1, None, None, None))
            .await
            .is_err()
    );
    assert!(
        tasks::update_task(p, &primary, "editable", update(1, Some("   "), None, None))
            .await
            .is_err()
    );

    let first = tasks::update_task(
        p,
        &primary,
        "editable",
        update(1, Some("Changed"), None, None),
    )
    .await
    .unwrap();
    assert!(first.applied);
    assert_eq!(first.revision, 2);
    assert_eq!(first.changed_fields, ["title"]);
    assert_eq!(first.task.description.as_deref(), Some("Detail"));
    assert_eq!(first.task.metadata, json!({"old":true}));
    let noop = tasks::update_task(
        p,
        &primary,
        "editable",
        update(2, Some("Changed"), None, None),
    )
    .await
    .unwrap();
    assert!(!noop.applied);
    assert_eq!(noop.revision, 2);
    assert!(
        tasks::update_task(
            p,
            &primary,
            "editable",
            update(1, Some("Stale"), None, None)
        )
        .await
        .is_err()
    );

    let cleared = tasks::update_task(
        p,
        &primary,
        "editable",
        update(2, None, Some(None), Some(None)),
    )
    .await
    .unwrap();
    assert_eq!(cleared.revision, 3);
    assert_eq!(cleared.task.description, None);
    assert_eq!(cleared.task.metadata, json!({}));
    let replaced = tasks::update_task(
        p,
        &primary,
        "editable",
        update(
            3,
            None,
            Some(Some("new description")),
            Some(Some(
                json!({"priority":"HIGH","secret":"should-not-appear-in-audit"}),
            )),
        ),
    )
    .await
    .unwrap();
    assert_eq!(replaced.revision, 4);
    assert_eq!(
        replaced.task.description.as_deref(),
        Some("new description")
    );
    assert!(
        tasks::update_task(
            p,
            &primary,
            "editable",
            update(4, None, None, Some(Some(json!(12))))
        )
        .await
        .is_err()
    );

    let claimed = tasks::claim_task(p, &other, "editable", Some(60))
        .await
        .unwrap();
    assert!(claimed.claimed);
    assert!(
        tasks::update_task(
            p,
            &primary,
            "editable",
            update(4, Some("foreign edit"), None, None)
        )
        .await
        .is_err()
    );
    let owned = tasks::update_task(p, &other, "editable", update(4, Some("Owned"), None, None))
        .await
        .unwrap();
    assert_eq!(owned.revision, 5);
    tasks::release_task(p, &other, "editable").await.unwrap();

    // Exactly one update succeeds under a shared expected_revision.
    let (left, right) = tokio::join!(
        tasks::update_task(p, &primary, "editable", update(5, Some("Left"), None, None)),
        tasks::update_task(
            p,
            &primary,
            "editable",
            update(5, Some("Right"), None, None)
        )
    );
    assert_eq!(u8::from(left.is_ok()) + u8::from(right.is_ok()), 1);
    let detail = tasks::get_task(p, &primary, "editable").await.unwrap();
    assert_eq!(detail.task.revision, 6);
    assert_eq!(
        detail
            .history
            .iter()
            .filter(|e| e.event == "updated")
            .count(),
        5
    );
    assert!(
        detail
            .history
            .iter()
            .filter(|e| e.event == "updated")
            .all(|e| !e
                .detail
                .as_deref()
                .unwrap_or("")
                .contains("should-not-appear-in-audit"))
    );
    assert_eq!(detail.task.depends_on.len(), 0);
    let dep = tasks::get_task(p, &primary, "dependent").await.unwrap();
    assert_eq!(dep.task.depends_on, vec!["editable"]);
    let after_id: Uuid =
        sqlx::query_scalar("SELECT id FROM tasks WHERE team_id=$1 AND key='editable'")
            .bind(f.team_id)
            .fetch_one(p)
            .await
            .unwrap();
    assert_eq!(after_id, original_id);
    let audit_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM task_update_audit WHERE task_id=$1")
            .bind(original_id)
            .fetch_one(p)
            .await
            .unwrap();
    assert_eq!(audit_count, 5);

    tasks::create_task(
        p,
        &primary,
        CreateInput {
            key: "finished".into(),
            title: "Finish".into(),
            description: None,
            metadata: None,
            depends_on: vec![],
        },
    )
    .await
    .unwrap();
    tasks::complete_task(p, &primary, "finished", None)
        .await
        .unwrap();
    assert!(
        tasks::update_task(
            p,
            &primary,
            "finished",
            update(1, Some("forbidden"), None, None)
        )
        .await
        .is_err()
    );

    // Cleanup is explicit on disposable fixtures: the audit FK is intentionally
    // RESTRICT to prevent silently erasing immutable production history.
    sqlx::query("DELETE FROM task_update_audit WHERE team_id=$1")
        .bind(f.team_id)
        .execute(p)
        .await
        .unwrap();
    fixture::cleanup(foreign_fixture).await.unwrap();
    fixture::cleanup(f).await.unwrap();
}
