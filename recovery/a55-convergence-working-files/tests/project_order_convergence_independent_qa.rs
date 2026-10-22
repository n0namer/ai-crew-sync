#[path = "task_pagination_fixture.rs"]
mod fixture;

use ai_crew_sync::{
    auth::AuthCtx,
    store::{
        project_order::{get_order, replace_order},
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
) -> Result<(), Box<dyn std::error::Error>> {
    tasks::create_task(
        pool,
        auth,
        CreateInput {
            key: key.to_owned(),
            title: format!("independent QA task {key}"),
            description: None,
            metadata: None,
            depends_on: Vec::new(),
        },
    )
    .await?;
    Ok(())
}

#[tokio::test]
async fn replan_rejects_terminal_entries_without_advancing_revision(
) -> Result<(), Box<dyn std::error::Error>> {
    let fixture = setup_fixture().await?;
    let coordinator = coordinator_auth(&fixture, "independent-qa-terminal-replan").await?;
    let cancelled = format!("{}:cancelled", fixture.task_prefix);
    let superseded = format!("{}:superseded", fixture.task_prefix);
    let replacement = format!("{}:replacement", fixture.task_prefix);
    create(&fixture.pool, &coordinator, &cancelled).await?;
    create(&fixture.pool, &coordinator, &superseded).await?;
    create(&fixture.pool, &coordinator, &replacement).await?;

    tasks::cancel_task(&fixture.pool, &coordinator, &cancelled, "independent QA").await?;
    tasks::supersede_task(
        &fixture.pool,
        &coordinator,
        &superseded,
        &replacement,
        "independent QA",
    )
    .await?;

    for terminal in [&cancelled, &superseded] {
        assert!(
            replace_order(
                &fixture.pool,
                &coordinator,
                &fixture.task_prefix,
                0,
                std::slice::from_ref(terminal),
            )
            .await
            .is_err(),
            "a new approved order must reject a terminal task and require a replan"
        );
        assert_eq!(
            get_order(&fixture.pool, &coordinator, &fixture.task_prefix).await?,
            (0, Vec::new()),
            "rejected terminal-entry replan must not mutate the queue"
        );
    }

    cleanup(fixture).await?;
    Ok(())
}
