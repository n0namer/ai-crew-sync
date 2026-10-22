#[path = "task_pagination_fixture.rs"]
mod fixture;

use ai_crew_sync::{
    auth::AuthCtx,
    store::{
        project_order::{claim_next_ordered, get_order, replace_order},
        tasks::{self, CreateInput},
    },
};
use fixture::{cleanup, setup_fixture, Fixture};
use sqlx::PgPool;
use uuid::Uuid;

async fn coordinator_auth(fixture: &Fixture, session: &str) -> Result<AuthCtx, sqlx::Error> {
    let agent_id: Uuid = sqlx::query_scalar(
        "INSERT INTO agents(team_id,name) VALUES($1,$2) RETURNING id",
    )
    .bind(fixture.team_id)
    .bind("coordinator")
    .fetch_one(&fixture.pool)
    .await?;
    Ok(AuthCtx {
        agent_id,
        agent_name: "coordinator".to_owned(),
        team_id: fixture.team_id,
        team_slug: format!("order-{}", fixture.team_id),
        session: session.to_owned(),
        session_id: None,
        session_epoch: None,
        token_id: None,
    })
}

async fn create(
    pool: &PgPool,
    auth: &AuthCtx,
    key: &str,
    dependencies: &[&str],
) -> Result<(), Box<dyn std::error::Error>> {
    tasks::create_task(
        pool,
        auth,
        CreateInput {
            key: key.to_owned(),
            title: format!("adversarial task {key}"),
            description: None,
            metadata: None,
            depends_on: dependencies.iter().map(|key| (*key).to_owned()).collect(),
        },
    )
    .await?;
    Ok(())
}

#[tokio::test]
async fn superseded_prerequisite_requires_explicit_replan(
) -> Result<(), Box<dyn std::error::Error>> {
    let fixture = setup_fixture().await?;
    let owner = coordinator_auth(&fixture, "adversarial-superseded-dependency").await?;
    let prerequisite = format!("{}:prerequisite", fixture.task_prefix);
    let dependent = format!("{}:dependent", fixture.task_prefix);
    create(&fixture.pool, &owner, &prerequisite, &[]).await?;
    create(&fixture.pool, &owner, &dependent, &[&prerequisite]).await?;

    sqlx::query("UPDATE tasks SET status='superseded' WHERE team_id=$1 AND key=$2")
        .bind(fixture.team_id)
        .bind(&prerequisite)
        .execute(&fixture.pool)
        .await?;

    let superseded = tasks::list_tasks(
        &fixture.pool,
        &owner,
        Some("superseded".to_owned()),
        false,
        10,
    )
    .await?;
    assert_eq!(
        superseded.tasks.iter().map(|task| task.key.as_str()).collect::<Vec<_>>(),
        vec![prerequisite.as_str()],
        "legacy task listing must retain the lifecycle superseded status",
    );

    assert!(
        replace_order(
            &fixture.pool,
            &owner,
            &fixture.task_prefix,
            0,
            std::slice::from_ref(&dependent),
        )
        .await
        .is_err(),
        "a superseded prerequisite still requires an explicit replan",
    );

    assert!(
        replace_order(
            &fixture.pool,
            &owner,
            &fixture.task_prefix,
            0,
            &[prerequisite.clone(), dependent.clone()],
        )
        .await
        .is_err(),
        "a superseded task cannot be newly admitted to an approved order",
    );
    assert_eq!(get_order(&fixture.pool, &owner, &fixture.task_prefix).await?, (0, vec![]));

    cleanup(fixture).await?;
    Ok(())
}

#[tokio::test]
async fn cancelled_prerequisite_does_not_unlock_successor_without_replan(
) -> Result<(), Box<dyn std::error::Error>> {
    let fixture = setup_fixture().await?;
    let owner = coordinator_auth(&fixture, "adversarial-cancelled-dependency").await?;
    let prerequisite = format!("{}:prerequisite", fixture.task_prefix);
    let dependent = format!("{}:dependent", fixture.task_prefix);
    create(&fixture.pool, &owner, &prerequisite, &[]).await?;
    create(&fixture.pool, &owner, &dependent, &[&prerequisite]).await?;

    sqlx::query("UPDATE tasks SET status='cancelled' WHERE team_id=$1 AND key=$2")
        .bind(fixture.team_id)
        .bind(&prerequisite)
        .execute(&fixture.pool)
        .await?;

    assert!(replace_order(
        &fixture.pool,
        &owner,
        &fixture.task_prefix,
        0,
        std::slice::from_ref(&dependent),
    )
    .await
    .is_err(), "cancelled prerequisites require explicit replan");

    assert!(replace_order(
        &fixture.pool,
        &owner,
        &fixture.task_prefix,
        0,
        &[prerequisite.clone(), dependent.clone()],
    )
    .await
    .is_err(), "a cancelled task cannot be newly admitted to an approved order");
    assert_eq!(get_order(&fixture.pool, &owner, &fixture.task_prefix).await?, (0, vec![]));

    cleanup(fixture).await?;
    Ok(())
}

#[tokio::test]
async fn terminal_order_rejection_preserves_order_revision_and_stale_fence(
) -> Result<(), Box<dyn std::error::Error>> {
    let fixture = setup_fixture().await?;
    let owner = coordinator_auth(&fixture, "adversarial-terminal-revision").await?;
    let first = format!("{}:first", fixture.task_prefix);
    let cancelled = format!("{}:cancelled", fixture.task_prefix);
    let superseded = format!("{}:superseded", fixture.task_prefix);
    for key in [&first, &cancelled, &superseded] {
        create(&fixture.pool, &owner, key, &[]).await?;
    }
    assert_eq!(
        replace_order(
            &fixture.pool,
            &owner,
            &fixture.task_prefix,
            0,
            std::slice::from_ref(&first),
        )
        .await?,
        1
    );
    for (key, status) in [
        (cancelled.as_str(), "cancelled"),
        (superseded.as_str(), "superseded"),
    ] {
        sqlx::query("UPDATE tasks SET status=$3 WHERE team_id=$1 AND key=$2")
            .bind(fixture.team_id)
            .bind(key)
            .bind(status)
            .execute(&fixture.pool)
            .await?;
        let proposed = vec![first.clone(), key.to_owned()];
        assert!(
            replace_order(
                &fixture.pool,
                &owner,
                &fixture.task_prefix,
                1,
                &proposed,
            )
            .await
            .is_err(),
            "{status} tasks must not enter a new approved order",
        );
        assert_eq!(
            get_order(&fixture.pool, &owner, &fixture.task_prefix).await?,
            (1, vec![first.clone()]),
        );
    }

    assert!(
        replace_order(
            &fixture.pool,
            &owner,
            &fixture.task_prefix,
            0,
            std::slice::from_ref(&first),
        )
        .await
        .is_err(),
        "a stale revision remains rejected after terminal-order failures",
    );
    assert_eq!(
        get_order(&fixture.pool, &owner, &fixture.task_prefix).await?,
        (1, vec![first]),
    );

    cleanup(fixture).await?;
    Ok(())
}

#[tokio::test]
async fn superseded_head_requires_explicit_replan_before_later_work(
) -> Result<(), Box<dyn std::error::Error>> {
    let fixture = setup_fixture().await?;
    let owner = coordinator_auth(&fixture, "adversarial-superseded-head").await?;
    let first = format!("{}:first", fixture.task_prefix);
    let second = format!("{}:second", fixture.task_prefix);
    create(&fixture.pool, &owner, &first, &[]).await?;
    create(&fixture.pool, &owner, &second, &[]).await?;

    assert_eq!(
        replace_order(
            &fixture.pool,
            &owner,
            &fixture.task_prefix,
            0,
            &[first.clone(), second.clone()],
        )
        .await?,
        1
    );
    sqlx::query("UPDATE tasks SET status='superseded' WHERE team_id=$1 AND key=$2")
        .bind(fixture.team_id)
        .bind(&first)
        .execute(&fixture.pool)
        .await?;

    let blocked = claim_next_ordered(&fixture.pool, &owner, &fixture.task_prefix, None).await?;
    assert!(!blocked.claimed, "superseded queue heads require an explicit replan");

    assert_eq!(
        replace_order(
            &fixture.pool,
            &owner,
            &fixture.task_prefix,
            1,
            std::slice::from_ref(&second),
        )
        .await?,
        2
    );
    let claim = claim_next_ordered(&fixture.pool, &owner, &fixture.task_prefix, None).await?;
    assert!(claim.claimed);
    assert_eq!(claim.task.as_ref().map(|task| task.key.as_str()), Some(second.as_str()));

    cleanup(fixture).await?;
    Ok(())
}

#[tokio::test]
async fn cancelled_head_requires_explicit_replan_before_later_work(
) -> Result<(), Box<dyn std::error::Error>> {
    let fixture = setup_fixture().await?;
    let owner = coordinator_auth(&fixture, "adversarial-cancelled-head").await?;
    let first = format!("{}:first", fixture.task_prefix);
    let second = format!("{}:second", fixture.task_prefix);
    create(&fixture.pool, &owner, &first, &[]).await?;
    create(&fixture.pool, &owner, &second, &[]).await?;

    assert_eq!(
        replace_order(
            &fixture.pool,
            &owner,
            &fixture.task_prefix,
            0,
            &[first.clone(), second.clone()],
        )
        .await?,
        1
    );
    let cancelled = tasks::cancel_task(
        &fixture.pool,
        &owner,
        &first,
        "approved work withdrawn; coordinator must replan",
    )
    .await?;
    assert_eq!(cancelled.status, "cancelled");

    let blocked = claim_next_ordered(&fixture.pool, &owner, &fixture.task_prefix, None).await?;
    assert!(
        !blocked.claimed,
        "cancelled queue heads must not silently unlock later work",
    );

    assert_eq!(
        replace_order(
            &fixture.pool,
            &owner,
            &fixture.task_prefix,
            1,
            std::slice::from_ref(&second),
        )
        .await?,
        2
    );
    let claim = claim_next_ordered(&fixture.pool, &owner, &fixture.task_prefix, None).await?;
    assert_eq!(
        claim.task.as_ref().map(|task| task.key.as_str()),
        Some(second.as_str()),
    );

    sqlx::query("DELETE FROM task_transition_audit WHERE team_id=$1")
        .bind(fixture.team_id)
        .execute(&fixture.pool)
        .await?;
    cleanup(fixture).await?;
    Ok(())
}
