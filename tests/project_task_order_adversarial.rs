#[path = "task_pagination_fixture.rs"]
mod fixture;

use ai_crew_sync::{
    auth::AuthCtx,
    store::{
        project_order::{claim_next_ordered, replace_order},
        tasks::{self, CreateInput},
    },
};
use fixture::{Fixture, cleanup, setup_fixture};
use sqlx::PgPool;
use uuid::Uuid;

async fn coordinator_auth(fixture: &Fixture, session: &str) -> Result<AuthCtx, sqlx::Error> {
    let agent_id: Uuid =
        sqlx::query_scalar("INSERT INTO agents(team_id,name) VALUES($1,$2) RETURNING id")
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
async fn superseded_prerequisite_is_terminal_for_ordered_claims()
-> Result<(), Box<dyn std::error::Error>> {
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

    assert_eq!(
        replace_order(
            &fixture.pool,
            &owner,
            &fixture.task_prefix,
            0,
            std::slice::from_ref(&dependent),
        )
        .await?,
        1
    );

    let claim = claim_next_ordered(&fixture.pool, &owner, &fixture.task_prefix, None).await?;
    assert!(claim.claimed, "a superseded prerequisite is terminal");
    assert_eq!(
        claim.task.as_ref().map(|task| task.key.as_str()),
        Some(dependent.as_str())
    );

    cleanup(fixture).await?;
    Ok(())
}

#[tokio::test]
async fn superseded_head_does_not_hold_back_later_ordered_work()
-> Result<(), Box<dyn std::error::Error>> {
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

    let claim = claim_next_ordered(&fixture.pool, &owner, &fixture.task_prefix, None).await?;
    assert!(claim.claimed);
    assert_eq!(
        claim.task.as_ref().map(|task| task.key.as_str()),
        Some(second.as_str())
    );

    cleanup(fixture).await?;
    Ok(())
}
