#[path = "task_pagination_fixture.rs"]
mod fixture;

use ai_crew_sync::{
    auth::AuthCtx,
    store::{
        project_order::{claim_next_ordered, get_order, replace_order},
        tasks::{self, CreateInput},
    },
};
use fixture::{Fixture, cleanup, setup_fixture};
use sqlx::PgPool;
use uuid::Uuid;

async fn auth(fixture: &Fixture, session: &str) -> Result<AuthCtx, sqlx::Error> {
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

async fn create(pool: &PgPool, auth: &AuthCtx, key: &str, deps: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    tasks::create_task(pool, auth, CreateInput {
        key: key.to_owned(),
        title: format!("test task {key}"),
        description: None,
        metadata: None,
        depends_on: deps.iter().map(|value| (*value).to_owned()).collect(),
    })
    .await?;
    Ok(())
}

#[tokio::test]
async fn picks_approved_order_not_creation_order_and_preserves_other_project() -> Result<(), Box<dyn std::error::Error>> {
    let f = setup_fixture().await?;
    let owner = auth(&f, "order-owner-a").await?;
    let a = format!("{}:a", f.task_prefix);
    let b = format!("{}:b", f.task_prefix);
    let c = format!("{}:c", f.task_prefix);
    let other_project = format!("other-{}", Uuid::new_v4().simple());
    let q = format!("{other_project}:q");
    for key in [&a, &b, &c, &q] { create(&f.pool, &owner, key, &[]).await?; }

    let (rev, initial) = get_order(&f.pool, &owner, &f.task_prefix).await?;
    assert_eq!(rev, 0);
    assert!(initial.is_empty());
    assert!(!claim_next_ordered(&f.pool, &owner, &f.task_prefix, None).await?.claimed);

    assert_eq!(replace_order(&f.pool, &owner, &f.task_prefix, 0, &[c.clone(), a.clone(), b.clone()]).await?, 1);
    assert_eq!(replace_order(&f.pool, &owner, &other_project, 0, std::slice::from_ref(&q)).await?, 1);
    assert_eq!(get_order(&f.pool, &owner, &f.task_prefix).await?, (1, vec![c.clone(), a.clone(), b.clone()]));

    let first = claim_next_ordered(&f.pool, &owner, &f.task_prefix, Some(60)).await?;
    assert_eq!(first.task.as_ref().map(|t| t.key.as_str()), Some(c.as_str()));
    assert!(!claim_next_ordered(&f.pool, &owner, &f.task_prefix, Some(60)).await?.claimed);

    // Q may proceed while the exact P head is still running.
    let q_claim = claim_next_ordered(&f.pool, &owner, &other_project, Some(60)).await?;
    assert_eq!(q_claim.task.as_ref().map(|t| t.key.as_str()), Some(q.as_str()));

    tasks::complete_task(&f.pool, &owner, &c, Some("accepted C".to_owned())).await?;
    let second = claim_next_ordered(&f.pool, &owner, &f.task_prefix, None).await?;
    assert_eq!(second.task.as_ref().map(|t| t.key.as_str()), Some(a.as_str()));
    tasks::complete_task(&f.pool, &owner, &a, Some("accepted A".to_owned())).await?;
    let third = claim_next_ordered(&f.pool, &owner, &f.task_prefix, None).await?;
    assert_eq!(third.task.as_ref().map(|t| t.key.as_str()), Some(b.as_str()));
    assert!(tasks::get_task(&f.pool, &owner, &b).await?.history.iter().any(|e| e.event == "claimed" && e.detail.as_deref().is_some_and(|v| v.contains("order-revision:1"))));
    cleanup(f).await?;
    Ok(())
}

#[tokio::test]
async fn stale_revisions_and_live_claims_fail_without_partial_replan() -> Result<(), Box<dyn std::error::Error>> {
    let f = setup_fixture().await?;
    let owner = auth(&f, "order-replanner-a").await?;
    let a = format!("{}:a", f.task_prefix);
    let b = format!("{}:b", f.task_prefix);
    create(&f.pool, &owner, &a, &[]).await?;
    create(&f.pool, &owner, &b, &[]).await?;
    assert_eq!(replace_order(&f.pool, &owner, &f.task_prefix, 0, &[a.clone(), b.clone()]).await?, 1);
    assert!(replace_order(&f.pool, &owner, &f.task_prefix, 0, &[b.clone(), a.clone()]).await.is_err());
    claim_next_ordered(&f.pool, &owner, &f.task_prefix, Some(120)).await?;
    assert!(replace_order(&f.pool, &owner, &f.task_prefix, 1, &[b.clone(), a.clone()]).await.is_err());
    assert_eq!(get_order(&f.pool, &owner, &f.task_prefix).await?, (1, vec![a.clone(), b.clone()]));
    tasks::complete_task(&f.pool, &owner, &a, Some("accepted A".to_owned())).await?;
    assert_eq!(replace_order(&f.pool, &owner, &f.task_prefix, 1, &[b.clone(), a.clone()]).await?, 2);
    assert_eq!(get_order(&f.pool, &owner, &f.task_prefix).await?, (2, vec![b, a]));
    let revisions: (i64,) = sqlx::query_as(
        "SELECT count(*) FROM project_task_order_revisions WHERE team_id=$1 AND project_key=$2",
    )
    .bind(f.team_id)
    .bind(&f.task_prefix)
    .fetch_one(&f.pool)
    .await?;
    assert_eq!(revisions.0, 2);
    cleanup(f).await?;
    Ok(())
}

#[tokio::test]
async fn dependency_inversion_duplicate_key_and_foreign_task_do_not_change_order() -> Result<(), Box<dyn std::error::Error>> {
    let f = setup_fixture().await?;
    let owner = auth(&f, "order-dependencies-a").await?;
    let a = format!("{}:a", f.task_prefix);
    let b = format!("{}:b", f.task_prefix);
    create(&f.pool, &owner, &a, &[]).await?;
    create(&f.pool, &owner, &b, &[&a]).await?;
    assert!(replace_order(&f.pool, &owner, &f.task_prefix, 0, &[b.clone(), a.clone()]).await.is_err());
    assert!(replace_order(&f.pool, &owner, &f.task_prefix, 0, &[a.clone(), a.clone()]).await.is_err());
    assert!(replace_order(&f.pool, &owner, &f.task_prefix, 0, &[a.clone(), "foreign:x".to_owned()]).await.is_err());
    assert_eq!(get_order(&f.pool, &owner, &f.task_prefix).await?, (0, Vec::new()));
    assert_eq!(replace_order(&f.pool, &owner, &f.task_prefix, 0, &[a.clone(), b.clone()]).await?, 1);
    let picked = claim_next_ordered(&f.pool, &owner, &f.task_prefix, None).await?;
    assert_eq!(picked.task.as_ref().map(|t| t.key.as_str()), Some(a.as_str()));
    cleanup(f).await?;
    Ok(())
}

#[tokio::test]
async fn empty_plan_pauses_open_backlog_and_new_claimed_task_cannot_be_planned() -> Result<(), Box<dyn std::error::Error>> {
    let f = setup_fixture().await?;
    let owner = auth(&f, "order-empty-a").await?;
    let a = format!("{}:a", f.task_prefix);
    let b = format!("{}:b", f.task_prefix);
    create(&f.pool, &owner, &a, &[]).await?;
    create(&f.pool, &owner, &b, &[]).await?;
    assert_eq!(replace_order(&f.pool, &owner, &f.task_prefix, 0, &[]).await?, 1);
    assert_eq!(get_order(&f.pool, &owner, &f.task_prefix).await?, (1, vec![]));
    assert!(!claim_next_ordered(&f.pool, &owner, &f.task_prefix, None).await?.claimed);
    tasks::claim_task(&f.pool, &owner, &a, Some(120)).await?;
    assert!(replace_order(&f.pool, &owner, &f.task_prefix, 1, &[a.clone()]).await.is_err());
    assert_eq!(get_order(&f.pool, &owner, &f.task_prefix).await?, (1, vec![]));
    assert_eq!(replace_order(&f.pool, &owner, &f.task_prefix, 1, &[b.clone()]).await?, 2);
    cleanup(f).await?;
    Ok(())
}

#[tokio::test]
async fn team_boundary_and_coordinator_authority_are_fail_closed() -> Result<(), Box<dyn std::error::Error>> {
    let f = setup_fixture().await?;
    let owner = auth(&f, "order-team-a").await?;
    let a = format!("{}:a", f.task_prefix);
    create(&f.pool, &owner, &a, &[]).await?;
    let foreign = setup_fixture().await?;
    let foreign_owner = auth(&foreign, "order-team-b").await?;
    assert!(replace_order(&foreign.pool, &foreign_owner, &f.task_prefix, 0, std::slice::from_ref(&a)).await.is_err());
    assert_eq!(get_order(&foreign.pool, &foreign_owner, &f.task_prefix).await?, (0, vec![]));
    let mut worker = owner.clone();
    worker.agent_name = "worker".to_owned();
    assert!(replace_order(&f.pool, &worker, &f.task_prefix, 0, std::slice::from_ref(&a)).await.is_err());
    assert!(claim_next_ordered(&f.pool, &worker, &f.task_prefix, None).await.is_err());
    assert_eq!(replace_order(&f.pool, &owner, &f.task_prefix, 0, std::slice::from_ref(&a)).await?, 1);
    cleanup(foreign).await?;
    cleanup(f).await?;
    Ok(())
}

#[tokio::test]
async fn concurrent_same_project_claims_never_double_allocate() -> Result<(), Box<dyn std::error::Error>> {
    let f = setup_fixture().await?;
    let owner = auth(&f, "order-race-a").await?;
    let a = format!("{}:a", f.task_prefix);
    let b = format!("{}:b", f.task_prefix);
    create(&f.pool, &owner, &a, &[]).await?;
    create(&f.pool, &owner, &b, &[]).await?;
    replace_order(&f.pool, &owner, &f.task_prefix, 0, &[a.clone(), b]).await?;
    let mut other_session = owner.clone();
    other_session.session = "order-race-b".to_owned();
    let (one, two) = tokio::join!(
        claim_next_ordered(&f.pool, &owner, &f.task_prefix, Some(120)),
        claim_next_ordered(&f.pool, &other_session, &f.task_prefix, Some(120))
    );
    let claimed = [one?, two?].into_iter().filter(|x| x.claimed).collect::<Vec<_>>();
    assert_eq!(claimed.len(), 1);
    assert_eq!(claimed[0].task.as_ref().map(|x| x.key.as_str()), Some(a.as_str()));
    cleanup(f).await?;
    Ok(())
}

#[tokio::test]
async fn legacy_claim_entrypoints_cannot_bypass_approved_project_order() -> Result<(), Box<dyn std::error::Error>> {
    let f = setup_fixture().await?;
    let owner = auth(&f, "order-legacy-a").await?;
    let first = format!("{}:first", f.task_prefix);
    let second = format!("{}:second", f.task_prefix);
    let unplanned = format!("{}:unplanned", f.task_prefix);
    create(&f.pool, &owner, &first, &[]).await?;
    create(&f.pool, &owner, &second, &[]).await?;
    create(&f.pool, &owner, &unplanned, &[]).await?;
    replace_order(&f.pool, &owner, &f.task_prefix, 0, &[second.clone(), first.clone()]).await?;

    // Old named claims may not initially claim *any* enrolled task.
    assert!(!tasks::claim_task(&f.pool, &owner, &first, Some(60)).await?.claimed);
    assert!(!tasks::claim_task(&f.pool, &owner, &second, Some(60)).await?.claimed);

    // Old unnamed claim skips approved tasks, serving only the unplanned pool.
    let global = tasks::claim_next_task(&f.pool, &owner, Some(60)).await?;
    assert_eq!(global.task.as_ref().map(|t| t.key.as_str()), Some(unplanned.as_str()));
    assert!(claim_next_ordered(&f.pool, &owner, &f.task_prefix, Some(60)).await?.claimed);
    // Existing approved holder may renew its lease through the backwards-compatible named path.
    assert!(tasks::claim_task(&f.pool, &owner, &second, Some(60)).await?.claimed);
    assert!(!tasks::claim_task(&f.pool, &owner, &first, Some(60)).await?.claimed);
    cleanup(f).await?;
    Ok(())
}

#[tokio::test]
async fn unfinished_same_project_dependency_cannot_be_left_out_of_order() -> Result<(), Box<dyn std::error::Error>> {
    let f = setup_fixture().await?;
    let owner = auth(&f, "order-dangling-dependency").await?;
    let predecessor = format!("{}:prerequisite", f.task_prefix);
    let dependent = format!("{}:dependent", f.task_prefix);
    create(&f.pool, &owner, &predecessor, &[]).await?;
    create(&f.pool, &owner, &dependent, &[&predecessor]).await?;

    // An explicitly ordered queue must not deadlock itself by omitting
    // a same-project prerequisite that the daemon can no longer discover.
    let err = replace_order(&f.pool, &owner, &f.task_prefix, 0, std::slice::from_ref(&dependent)).await;
    assert!(err.is_err());
    assert_eq!(get_order(&f.pool, &owner, &f.task_prefix).await?, (0, vec![]));

    // A prerequisite already terminal via the ordinary CrewSync workflow
    // may be omitted without blocking the new approved plan.
    assert!(tasks::claim_task(&f.pool, &owner, &predecessor, Some(120)).await?.claimed);
    tasks::complete_task(&f.pool, &owner, &predecessor, Some("accepted predecessor".to_owned())).await?;
    assert_eq!(replace_order(&f.pool, &owner, &f.task_prefix, 0, std::slice::from_ref(&dependent)).await?, 1);
    let granted = claim_next_ordered(&f.pool, &owner, &f.task_prefix, None).await?;
    assert_eq!(granted.task.as_ref().map(|t| t.key.as_str()), Some(dependent.as_str()));
    cleanup(f).await?;
    Ok(())
}

#[tokio::test]
async fn blocked_head_stays_barrier_until_prerequisite_recovers() -> Result<(), Box<dyn std::error::Error>> {
    let f = setup_fixture().await?;
    let owner = auth(&f, "order-blocked-head").await?;
    let prerequisite_project = format!("recovery-{}", Uuid::new_v4().simple());
    let prerequisite = format!("{prerequisite_project}:prerequisite");
    let dependent = format!("{}:dependent", f.task_prefix);
    create(&f.pool, &owner, &prerequisite, &[]).await?;
    create(&f.pool, &owner, &dependent, &[&prerequisite]).await?;

    assert_eq!(replace_order(&f.pool, &owner, &f.task_prefix, 0, std::slice::from_ref(&dependent)).await?, 1);
    let blocked = claim_next_ordered(&f.pool, &owner, &f.task_prefix, None).await?;
    assert!(!blocked.claimed);
    assert!(blocked.reason.as_deref().is_some_and(|reason| reason.contains("blocked")));

    assert!(tasks::claim_task(&f.pool, &owner, &prerequisite, Some(120)).await?.claimed);
    tasks::complete_task(&f.pool, &owner, &prerequisite, Some("recovered prerequisite".to_owned())).await?;
    let recovered = claim_next_ordered(&f.pool, &owner, &f.task_prefix, None).await?;
    assert_eq!(recovered.task.as_ref().map(|task| task.key.as_str()), Some(dependent.as_str()));
    cleanup(f).await?;
    Ok(())
}
