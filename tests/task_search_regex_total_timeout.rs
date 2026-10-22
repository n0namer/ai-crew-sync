#[test]
fn regex_total_timeout_uses_safe_mapping_and_rollback() {
    let source = include_str!("../src/store/tasks.rs");
    let totals_branch = source
        .split_once("let totals_result: Result<(i64, i64), sqlx::Error>")
        .map(|(_, remainder)| remainder)
        .expect("search totals query has an explicit result boundary");
    let totals_branch = totals_branch
        .split_once("tx.commit().await?;")
        .map(|(branch, _)| branch)
        .expect("search totals error handling precedes commit");

    assert!(
        totals_branch.contains("map_search_database_error(&error)"),
        "totals SQL errors must use the bounded search error mapper"
    );
    assert!(
        totals_branch.contains("let _ = tx.rollback().await;"),
        "totals failures must explicitly rollback the search transaction"
    );
    assert!(
        source.contains("Some(\"57014\") if message.contains(\"statement timeout\")"),
        "statement-timeout SQLSTATE 57014 must remain mapped to the stable resource-limit error"
    );
    assert!(
        source.contains("regex search exceeded the 250ms execution limit; narrow the pattern and retry"),
        "the public timeout message must remain sanitized and stable"
    );
}
