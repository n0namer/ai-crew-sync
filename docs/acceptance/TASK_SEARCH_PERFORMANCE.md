# Task Search Performance Acceptance

## Purpose

`tests/task_search_performance.rs` is the reproducible, opt-in benchmark for the frozen task-search contract. It uses only a disposable PostgreSQL target, creates an isolated team, populates exactly 2,000 tasks, and removes the team after the run. It never uses the production connection for fixture work.

The benchmark captures PostgreSQL `EXPLAIN (ANALYZE, BUFFERS)` evidence for production-shaped title/description predicates:

- weighted `simple` keyword search using the same title/description `to_tsvector` expression as the search store;
- literal contains search using separate escaped `ILIKE` predicates for title and description;
- case-insensitive regex search using separate `~*` predicates for title and description;
- a deliberately non-indexable pathological regex pattern using the same regex predicate.

All EXPLAIN statements are fixed controlled SQL literals wrapped with `sqlx::AssertSqlSafe`; search values are bound parameters and are never interpolated into SQL. The non-indexable pattern is expected to avoid `tasks_search_trgm_idx`; that is evidence that the benchmark exercises the fail-closed/non-indexable path rather than silently claiming index coverage for every regex. The benchmark also sets a transaction-local `statement_timeout` of `250ms` and runs the pathological regex under a two-second outer guard. PostgreSQL cancellation (`SQLSTATE 57014`) is accepted as the intended bounded-time outcome; a successful fast completion is also accepted.

## Reproducible command

Run from the project root with a disposable loopback database whose name contains `test` or `fixture`:

```text
CREWSYNC_ENABLE_LARGE_FIXTURE=1 TEST_DATABASE_URL=postgres://tester:password@127.0.0.1:5432/crewsync_test cargo test --test task_search_performance -- --ignored --nocapture
```

The fixture safety gate rejects non-loopback hosts, production/live database names, and a test URL that resolves to the configured `DATABASE_URL` target. Do not substitute a production connection.

## Evidence checklist

A valid run must show four labeled plans (`keywords`, `contains`, `regex`, and `nonindexable regex`) and a timeout line containing `statement_timeout=250ms`. Each plan must retain the `team_id` predicate and execution evidence. The benchmark's assertions fail if the pathological regex unexpectedly uses the trigram index, if tenant filtering disappears, if the fixture is not exactly 2,000 tasks, or if the bounded execution exceeds the outer guard.

Before coordinator acceptance, run the focused Rust compile/test check and retain its exact output. The Node-only scoped validator is not a Rust compiler and is not evidence of Rust compilation. This artifact is candidate evidence only; it does not accept the task, certify production readiness, or replace independent QA/reviewer readback.
