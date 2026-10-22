# CrewSync Lifecycle Repair Verification R2

- **Task key:** `crewsync:v2-lifecycle-repair-verification-20261012-r2`
- **Project:** `crewsync`
- **Review date:** 2026-10-12
- **Review posture:** Independent source and test-contract review of the isolated worktree after the prior F01-F06 findings. This report is evidence only; it does not accept the candidate.
- **Verdict:** **UNACCEPTED — PENDING RUNTIME/INTEGRATION EVIDENCE**
- **Reducer condition:** Source review finds the six prior repair targets represented in the current code and migration, but the focused PostgreSQL test could not run through the authorized runner and no runtime-restart evidence is present.

## Scope and source of truth

Reviewed the authorized lifecycle surface:

- `src/store/tasks.rs`
- `src/model.rs`
- `src/tools/tasks.rs`
- `src/client/mod.rs`
- `src/client/mapping.rs`
- `tests/task_cancel_supersede.rs`
- `migrations/0022_task_cancel_supersede_audit.sql`
- `Cargo.toml`
- Existing prior review: `docs/acceptance/CREWSYNC_LIFECYCLE_INDEPENDENT_QA_20261012.md`

The requested `runtime` directory is absent from the worktree. No `docker-qa-r3-evidence.json` was present in the reviewed acceptance locations.

## Repair verdicts

### F-01 — PASS: completion clears ownership

**Evidence:** `src/store/tasks.rs:1349-1370`.

`complete_task` now sets `claimed_by = NULL`, `claimed_session = NULL`, `claimed_at = NULL`, and `lease_expires_at = NULL` in the same `UPDATE` that changes the task to `done`. The returned `TaskInfo` therefore cannot expose the former live owner, and `mine_only` cannot retain the completed claim through stale ownership columns.

**Contract check:** The integration scenario `owned-completion` in `tests/task_cancel_supersede.rs` asserts the completed task has no owner/session/timestamp and is absent from a `mine_only` list.

**Status:** PASS by source and test-contract inspection; runtime assertion not executed.

### F-02 — PASS: unresolved dependencies block completion

**Evidence:** `src/store/tasks.rs:1359-1367`.

The completion `UPDATE` includes `NOT EXISTS` over `task_deps` joined to prerequisites and rejects every prerequisite whose status is not `done`. The update therefore does not write the result, completion state, or completion event when a dependent remains blocked.

**Contract check:** `tests/task_cancel_supersede.rs` calls `complete_task` on a dependent before its prerequisite is done and expects an error (`blocked completion must fail closed`).

**Status:** PASS by source and negative-test contract; runtime assertion not executed.

### F-03 — PASS: active dependents and self/cyclic successors are rejected

**Evidence:**

- Self-successor guard: `src/store/tasks.rs:1534-1536` rejects identical source and successor keys.
- Active-dependent guard: `src/store/tasks.rs:1601-1618` checks `task_deps` for open or claimed dependents of the source before supersession.
- Successor-chain cycle guard: `src/store/tasks.rs:1620-1637` follows `superseded_by_task_id` recursively and rejects a cycle.
- Transaction serialization: `src/store/tasks.rs:1542-1552` takes deterministic advisory locks for the source/successor key pair and then locks both task rows.

A successor that directly depends on the source is itself an active dependent when it is eligible as a successor (`open` or `claimed`), so the active-dependent query rejects the deadlock-producing request. The focused integration scenario `active-parent`/`active-child` in `tests/task_cancel_supersede.rs` covers that negative contract, and the same test also covers self and reverse-cycle attempts.

**Status:** PASS by source and test-contract inspection; concurrent runtime race assertion not executed.

### F-04 — PASS: foreign NULL-lease takeover is denied

**Evidence:**

- Named claim: `src/store/tasks.rs:1041-1058` permits takeover only for an open task, the same agent/session, or a non-NULL lease that has expired. A claimed row with a NULL lease cannot be taken by a foreign caller.
- Completion: `src/store/tasks.rs:1359-1365` uses the same-agent/session predicate and only permits expired takeover when `lease_expires_at IS NOT NULL`.
- Renewal: `src/store/tasks.rs:1243-1257` requires matching team/key/agent/session; the `NULL` lease compatibility branch does not permit a foreign holder to renew.
- Read contract: `src/store/tasks.rs:143-166` treats a claimed NULL lease as a live legacy claim rather than as open.

The selected compatibility policy is internally consistent for foreign mutation: a legacy NULL-lease holder may renew/release its own claim, while another agent/session cannot claim, complete, or renew it. The test scenario `legacy-null` in `tests/task_cancel_supersede.rs` explicitly checks foreign claim and completion rejection, then permits the original holder to renew and release.

**Status:** PASS by source and test-contract inspection; restart/fixture execution not available.

### F-05 — PASS: same-team successor and audit references are schema-enforced

**Evidence:** `migrations/0022_task_cancel_supersede_audit.sql:11-35`.

The migration adds `UNIQUE (team_id, id)` on `tasks`, then uses composite `(team_id, superseded_by_task_id) -> (team_id, id)` and composite audit FKs for both `task_id` and `successor_task_id`. A direct database writer cannot create a cross-team successor or cross-team transition-audit reference without violating the schema constraint.

**Contract check:** `tests/task_cancel_supersede.rs` attempts forged cross-team successor and audit rows and expects both database operations to fail.

**Status:** PASS by migration and negative-test contract; database execution not available.

### F-06 — PASS: transition audit survives task deletion attempts

**Evidence:** `migrations/0022_task_cancel_supersede_audit.sql:24-45`.

The transition audit primary task reference uses `ON DELETE RESTRICT`; the successor reference also uses `ON DELETE RESTRICT`. Deleting a transitioned task is rejected rather than cascading away its audit history. The test scenario attempts to delete a transitioned task and expects failure, while cleanup explicitly removes disposable audit rows only after the assertions.

**Status:** PASS by migration and negative-test contract; database execution not available.

## Race and dependency review

- `create_task` inserts the task, resolves and locks prerequisite rows, inserts `task_deps`, and logs `created` in one transaction (`src/store/tasks.rs:260-359`).
- `cancel_task`/`supersede_task` operate under transaction/session guards and lock the source task before evaluating active dependents; the intended create-versus-transition race is serialized through the prerequisite/source row.
- `claim_task`, `complete_task`, and `renew_lease` use SQL predicates rather than a read-then-write ownership check, preventing a stale read from authorizing a foreign NULL-lease mutation.
- Supersession races on the same source use deterministic advisory locks; the integration scenario `race-source` expects exactly one of two competing successors to win.

These are source-backed conclusions only until the disposable PostgreSQL fixture executes.

## Runtime and test evidence

| Check | Status | Evidence |
|---|---|---|
| Independent source review | PASS WITH CONDITIONS | Current source and migration inspected; F01-F06 repair points are present. |
| Positive/negative dependency review | PASS BY CONTRACT | Completion guard, create transaction, active-dependent guard, and test assertions inspected. |
| Lease/session foreign-mutation review | PASS BY CONTRACT | Claim, complete, renew predicates and `legacy-null` assertions inspected. |
| Same-team composite FK review | PASS BY MIGRATION | Composite task and audit FKs inspected. |
| Audit preservation review | PASS BY MIGRATION | `ON DELETE RESTRICT` constraints inspected. |
| Focused integration test | BLOCKED | `go_wave.run_project_tests({file:"tests/task_cancel_supersede.rs"})` returned `GO_WAVE_NODE_ERROR:test_file_not_allowed`. The test is also `#[ignore]` and requires `CREWSYNC_ENABLE_LARGE_FIXTURE=1` plus a disposable `TEST_DATABASE_URL`. |
| `git diff --check` | PENDING | No authorized repository command runner was available in this worker; no claim is made. |
| Runtime restart behavior | PENDING | `runtime/` is absent and no restart/container evidence was present. The legacy NULL-lease test models persisted state by SQL mutation, not an actual process restart. |

## Remaining conditions for reducer acceptance

1. Run `tests/task_cancel_supersede.rs` against a disposable PostgreSQL fixture with the required environment and ignore gate enabled.
2. Capture a real restart/rehydration check for a claimed task with a NULL lease and verify foreign claim, completion, and renewal remain denied after restart.
3. Capture fresh `git diff --check` and affected Rust test output through an authorized runner.
4. Re-review the resulting receipts independently; this report remains `UNACCEPTED` and does not authorize task acceptance.

## Changed files

Only this review artifact was written:

- `docs/acceptance/CREWSYNC_LIFECYCLE_REPAIR_VERIFICATION_R2.md`

No Rust source, migration, task record, runtime state, claim, lease, lock, or production data was modified.
