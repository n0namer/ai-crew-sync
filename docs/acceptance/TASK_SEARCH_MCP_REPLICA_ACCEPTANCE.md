# Task Search MCP Multi-Replica Recovery Acceptance

**Task key:** `crewsync:mcp-multireplica-recovery-goal-20261009-r1`  
**Project:** `crewsync`  
**Attempt:** `attempt-efa6b708-3d30-406e-a168-66c71ed2a8ca`  
**Session:** `manual-mcp-three-functional-goal-owners-2026100-8f397ca3652dd562`  
**Producer role:** `implementer`  
**Producer thread:** `null`  
**Review status:** `CANDIDATE / UNACCEPTED`  
**Review date:** 2026-10-09  
**Changed files:** `tests/task_search_mcp_multi_replica.rs`, `tests/task_search_mcp_replica_fixture.rs`, this document only

This artifact records the bounded multi-replica MCP test implementation. It is evidence, not task administration, and does not claim `DONE`, `ACCEPTED`, `ACCEPTED_GREEN`, deployment, or production readiness. No source runtime, migration, Ruflo, Native, database volume, task claim, or lease state is modified by this worker.

## Implemented contract

`tests/task_search_mcp_multi_replica.rs` starts two independent Axum/MCP HTTP servers against the same isolated PostgreSQL pool with the same configured signing secret. The test uses real rmcp Streamable HTTP clients and bearer tokens, alternates cursor pages between replica addresses, stops one replica, resumes through the other, and verifies:

- static fixture traversal returns exactly the seeded keys once, with no duplicate or skipped key;
- terminal pagination has `has_more=false` and no `next_cursor`;
- altered status, project prefix, search query/mode/field, tampered cursor, and cross-team cursor replay are rejected;
- a foreign team receives only its own fixture keys;
- a read-only page sequence leaves an unrelated task's state unchanged;
- a live claim remains claimed and is not changed by pagination;
- a new client on the surviving replica resumes after the first replica stops.

`tests/task_search_mcp_replica_fixture.rs` provides the isolated fixture helpers and a deterministic negative assertion that a cursor signed with a nonidentical secret returns `InvalidSignature`. This is the cryptographic component of the wrong-key gate; per-replica cursor secret injection is process/config scoped and is not mutable independently inside one Rust test process.

## Focused execution

Required runtime command:

```text
cargo test --test task_search_mcp_multi_replica -- --ignored
```

Required compile command:

```text
cargo check --tests
```

Both commands require `CREWSYNC_ENABLE_LARGE_FIXTURE=1` for the ignored runtime test and a disposable loopback `TEST_DATABASE_URL` whose database name contains `test` or `fixture` and differs from `DATABASE_URL`. Credentials and secrets must remain outside receipts.

The available worker runner is Node-only and cannot execute Rust/Cargo integration tests. Its result is retained in the worker report as an infrastructure/tooling check, not as Rust acceptance evidence. Cargo/PostgreSQL runtime execution remains an independent coordinator/reviewer gate.

## Broker/outbox classification

This test intentionally sets `nats_url=None`, `nats_credentials=None`, and `publication_worker=false`. The tested read-only task pagination path is therefore PostgreSQL-only and broker publication is **not applicable to this fixture**. This does not prove broker delivery, failover, reconciliation, or no-double-drain behavior; those require a separately provisioned broker runtime and raw outbox receipts.

## Acceptance matrix

| Check | Evidence owner | State |
|---|---|---|
| Two independent replicas alternate authenticated cursor pages | focused Rust E2E | `PENDING_RUNTIME` |
| Static traversal has no duplicate/skip and terminal cursor semantics | focused Rust E2E | `PENDING_RUNTIME` |
| Wrong team, altered filters, tampered cursor rejected | focused Rust E2E | `PENDING_RUNTIME` |
| Nonidentical signing secret rejected | fixture unit test | `PENDING_CARGO` |
| Restart/resume does not reset traversal | focused Rust E2E | `PENDING_RUNTIME` |
| Read-only pagination leaves claim/lease state unchanged | focused Rust E2E | `PENDING_RUNTIME` |
| Cargo compile and ignored runtime test | coordinator/independent QA | `PENDING_INFRA` |
| Broker/outbox behavior | release/reducer | `NOT_APPLICABLE_TO_POSTGRES_ONLY_FIXTURE` |
| Node-only worker runner | this worker | `BLOCKED_TOOLING` |

A successful publication of this document or a completed Codex turn is not independent runtime verification or acceptance.
