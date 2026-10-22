# CrewSync Lifecycle Independent QA Verdict

- **Task:** `crewsync:v2-lifecycle-independent-review-20261012-r1`
- **Project:** `crewsync`
- **Review date:** 2026-10-12
- **Review posture:** Independent adversarial review of the actual worktree source; no implementer history or handoff assertions used as acceptance evidence.
- **Verdict:** **UNACCEPTED — FAIL CLOSED**
- **Reducer decision:** Do not accept this candidate until the findings below are repaired and independently rerun against a disposable PostgreSQL fixture.

## Scope and evidence

Reviewed the declared lifecycle surface:

- `src/store/tasks.rs`
- `src/model.rs`
- `src/tools/tasks.rs`
- `src/client/mod.rs`
- `src/client/mapping.rs`
- `tests/task_cancel_supersede.rs`
- `migrations/0022_task_cancel_supersede_audit.sql`
- `Cargo.toml`

The requested `docker-qa-r3-evidence.json` was not present at the worktree root or under `docs/acceptance/`; no independent container evidence was available to review. The requested `runtime` directory was also absent.

The focused test invocation was attempted through the authorized project runner for `tests/task_cancel_supersede.rs`, but the runner returned `GO_WAVE_NODE_ERROR:test_file_not_allowed`. Therefore no Rust integration test result is asserted here. The integration test itself is marked `#[ignore]` and requires `CREWSYNC_ENABLE_LARGE_FIXTURE=1` plus a disposable `TEST_DATABASE_URL` (`tests/task_cancel_supersede.rs:44`).

## Findings

### F-01 — HIGH: completion leaves stale ownership and makes completed tasks appear mine

**Location:** `src/store/tasks.rs:1349-1370`

`complete_task` updates `status`, `result`, `lease_expires_at`, and `updated_at`, but does not clear `claimed_by`, `claimed_session`, or `claimed_at`. This violates the `TaskInfo` ownership contract: a completed task can still serialize the former holder as its current claimant. It also affects `mine_only`: the list predicate accepts a matching `claimed_by` and treats `NULL` expiry as live through `COALESCE(t.lease_expires_at > now(), true)`, so a completed task can remain in a caller's claimed inventory.

**Failure case:** agent A claims task T; A completes T; `get_task`/list still expose A's claim fields, and `list_tasks(mine_only=true)` can return T even though T is done.

**Required regression:** assert all claim fields are cleared after completion and that a completed task is absent from `mine_only` results for every session.

### F-02 — HIGH: blocked dependents can be completed directly

**Location:** `src/store/tasks.rs:1356-1363`; acknowledged by the test comment at `tests/task_cancel_supersede.rs:57-63`

`complete_task` has no `NOT EXISTS` dependency guard. It can mark a task `done` while any dependency is still `open`, `claimed`, `cancelled`, or `superseded`. Because the dependency predicate elsewhere treats only `done` as resolving, this permits a dependent to bypass the lifecycle gate and records an accepted result for work whose prerequisite was never accepted.

The existing integration test explicitly calls this a “separate pre-existing invariant,” but that is a known gap, not evidence that the lifecycle is safe. The requested review acceptance includes dependency contract review, so this remains blocking.

**Failure case:** create A; create B with `depends_on=[A]`; call `complete_task(B)` before A is done; B becomes done and any later tasks depending on B become claimable despite A never resolving.

**Required regression:** reject completion of B while any dependency is not `done`; verify no result or completion event is committed.

### F-03 — HIGH: superseding a prerequisite with its own dependent creates a permanent deadlock

**Locations:** `src/store/tasks.rs:205-240`, `src/store/tasks.rs:1488-1585`

The API permits task B to be selected as the successor of task A even when B depends on A. Supersession changes A to `superseded`, while dependency readiness remains `d.status <> 'done'`; therefore B remains blocked forever by A. The successor cycle check only follows `superseded_by_task_id`, not dependency edges, and the supersede operation does not reject this dependency relationship.

**Failure case:** create A; create B with `depends_on=[A]`; call `supersede_task(A, B, reason)`. A is terminal `superseded`, B is the recorded successor but cannot be claimed because A is not `done`; the replacement cannot replace the work it depends on.

**Required regression:** reject a successor that depends directly or transitively on the source, or define and implement an explicit dependency rewrite that preserves readiness without creating a deadlock. Test both direct and concurrent attempts.

### F-04 — HIGH: legacy `NULL` lease semantics are inconsistent and permit foreign takeover

**Locations:** `src/store/tasks.rs:143-166`, `src/store/tasks.rs:1041-1052`, `src/store/tasks.rs:1356-1363`

The read path documents a claimed task with `lease_expires_at IS NULL` as a live legacy claim: `TASK_SELECT` says it is “a live claim,” and `TaskInfo` retains the holder. However, `claim_task` explicitly allows any caller to take it through `OR lease_expires_at IS NULL`, and `complete_task` explicitly allows any caller to complete it through the same condition. `claim_next_task` does not select such rows, and `renew_lease` cannot renew them because it requires `lease_expires_at > now()`.

**Failure case:** a pre-lease claimed task with a foreign holder and `NULL` expiry can be stolen by an unrelated session or completed by that session, while reads report the original holder as active.

**Required regression:** choose one backward-compatible policy and apply it consistently. If `NULL` means a live shared/legacy claim, only the holder may renew/release/complete and no foreign claim may steal it; if it means expired, reads and all mutation predicates must expose it as open.

### F-05 — MEDIUM: database constraints do not enforce same-team successor references

**Location:** `migrations/0022_task_cancel_supersede_audit.sql:11-28`

The migration adds UUID foreign keys for `superseded_by_task_id` and `task_transition_audit.successor_task_id`, but neither constraint includes `team_id`. The Rust API checks the source and successor using the caller's team, so normal MCP calls reject cross-team successors, but a direct database writer, compromised internal service, or future code path can create a cross-team successor reference and audit row. The relationship is then exposed through `TASK_SELECT` without a team predicate on the successor join.

**Required regression/repair:** enforce the team relationship at the schema boundary (for example, a composite unique key plus composite foreign key, or a trigger) and add a cross-team insertion test. At minimum, document this as an explicit trusted-writer assumption before acceptance.

### F-06 — MEDIUM: transition audit is not durable across task deletion

**Location:** `migrations/0022_task_cancel_supersede_audit.sql:15-27`

`task_transition_audit.task_id` uses `ON DELETE CASCADE`. If task deletion exists now or is added later, the supposedly durable cancellation/supersession history is deleted with the task. The successor reference uses `ON DELETE RESTRICT`, which shows the migration intentionally protects some history relationships but not the primary audit record.

**Required regression/repair:** either prohibit task deletion for transitioned tasks, retain audit rows with a nullable/task snapshot reference, or explicitly downgrade the documentation from durable audit to task-lifetime audit and cover the chosen policy.

## Positive observations

- `create_task` resolves dependency keys and inserts dependency edges plus the created event in one transaction (`src/store/tasks.rs:225-320`), avoiding the previously exposed dependency-free visibility gap.
- The create path locks existing prerequisite rows before validating status, and the cancellation path locks the source row before checking active dependents; the intended create-versus-cancel race is conservatively serialized.
- Same-team lookup is enforced by `team_id` in the lifecycle API, and foreign-claimed live tasks are rejected for cancel/supersede through the session-aware holder predicate.
- Supersession uses deterministic transaction advisory locks for source/successor key pairs and checks successor-chain cycles.
- Cancellation and supersession write the lifecycle event and transition audit row in the same transaction as the state change.
- MCP and CLI mappings for `cancel_task` and `supersede_task` are present and tested structurally in `src/client/mapping.rs:270-295`.

## Checks and limitations

| Check | Status | Evidence |
|---|---|---|
| Independent source review | PASS WITH FINDINGS | Actual scoped source and migration reads listed above; six blocking/conditional findings remain. |
| Positive dependency transaction review | PARTIAL PASS | `create_task` transaction and row-lock structure inspected at `src/store/tasks.rs:225-320`; no runtime race execution available. |
| Negative dependency contract review | FAIL | Direct completion of blocked tasks is possible; F-02. |
| Supersession cycle review | PARTIAL PASS | Successor-chain cycle query and deterministic advisory locks inspected; dependency deadlock remains, F-03. |
| Foreign-team claim/reference review | FAIL | API team predicates are present, but legacy NULL leases permit foreign mutation, F-04; schema lacks composite team enforcement, F-05. |
| Restart/legacy compatibility review | FAIL | NULL lease behavior is inconsistent across read, claim, renew, complete, and claim-next paths, F-04. |
| Focused integration test | BLOCKED | `go_wave.run_project_tests({file:"tests/task_cancel_supersede.rs"})` returned `GO_WAVE_NODE_ERROR:test_file_not_allowed`. The test is also ignored and requires an external disposable PostgreSQL fixture. |
| `git diff --check` | BLOCKED | No authorized scoped command runner was available for this repository-level check. |
| Docker QA evidence review | BLOCKED | `docker-qa-r3-evidence.json` was absent at the requested candidate paths. |

## Acceptance conditions

The reducer may reconsider only after an independent verifier demonstrates:

1. completion clears ownership fields and excludes done tasks from `mine_only`;
2. blocked completion is rejected and leaves state/history unchanged;
3. superseding a source with a dependent successor is rejected or safely rewires dependencies without deadlock;
4. legacy `NULL` lease behavior is made internally consistent and foreign mutation is covered by negative tests;
5. same-team successor integrity is enforced or the trusted-writer boundary is explicitly accepted by the project authority;
6. the ignored PostgreSQL integration test is run with disposable fixtures, plus focused new regressions for every finding;
7. fresh `git diff --check` and affected Rust tests are captured by an authorized runner.

**Final QA verdict:** **UNACCEPTED / FAIL CLOSED**. The review found material correctness, concurrency-contract, restart-compatibility, and integrity gaps. No production, migration, task record, or Rust source file was modified by this review.
