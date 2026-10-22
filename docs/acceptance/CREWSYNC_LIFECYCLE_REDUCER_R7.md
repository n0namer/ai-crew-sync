# CrewSync lifecycle independent reducer R7

- **Task key:** `crewsync:v2-lifecycle-independent-reducer-20261012-r1`
- **Project:** `crewsync`
- **Review date:** 2026-10-12
- **Review posture:** Independent risk-based reducer of the actual scoped worktree, prior QA findings, repair verification, and R6 runtime receipts. No CrewSync task, claim, lease, lock, database, production image, or deployment state was mutated.
- **Decision:** **ACCEPTED_CODE_CANDIDATE**
- **Boundary:** Technical candidate acceptance only. This is **not** production rollout approval.

## Source and evidence reconciliation

**FACT:** The target is the `crewsync` worktree declared by the task, not canonical BMAD-MNNZ. The only authorized write was this artifact.

**FACT:** Current source readback matches the repair-verification claims:

- `src/store/tasks.rs:1349-1377` clears all ownership and lease columns on completion and rejects completion while any prerequisite is not `done`.
- `src/store/tasks.rs:1000-1049` allows a claim only when open, held by the same agent/session, or expired with a non-NULL lease. A foreign caller cannot take a claimed legacy NULL-lease row.
- `src/store/tasks.rs:1235-1269` permits renewal only for the matching agent/session, with NULL lease treated as a live compatibility claim.
- `src/store/tasks.rs:1530-1669` rejects self-successors, active dependents, and successor-chain cycles while serializing source/successor transitions with deterministic advisory locks.
- `migrations/0022_task_cancel_supersede_audit.sql:11-43` enforces same-team task and audit references with composite foreign keys and protects transition history with `ON DELETE RESTRICT`.
- `tests/task_cancel_supersede.rs:105-171` covers the two-node supersede cycle, replay/idempotency, and self-successor rejection.
- `tests/task_cancel_supersede.rs:174-238` covers owned completion, `mine_only` exclusion, legacy NULL-lease foreign-mutation denial, original-owner renewal/release, active-dependent supersede denial, and the concurrent competing-successor race.
- `src/model.rs:277-331`, `src/tools/tasks.rs:80-220`, `src/client/mod.rs:270-315`, and `src/client/mapping.rs:250-304` preserve the lifecycle wire, MCP, and CLI contracts.

**FACT:** The two prior independent reviews were reconciled:

- `docs/acceptance/CREWSYNC_LIFECYCLE_INDEPENDENT_QA_20261012.md` reported F01–F06 as blocking before repair and recorded missing runtime evidence at that time.
- `docs/acceptance/CREWSYNC_LIFECYCLE_REPAIR_VERIFICATION_R2.md` found all six repair targets represented in source/migration and left acceptance pending only runtime, restart, and fresh diff/test receipts.
- `docs/acceptance/CREWSYNC_LIFECYCLE_RUNTIME_R6.md` supplies those later receipts and explicitly states that the runs used disposable Docker/PostgreSQL fixtures, not production.

## F01–F06 disposition

| Finding | Reducer result | Evidence reconciliation |
|---|---|---|
| F01 stale ownership after completion | **RESOLVED** | Completion clears `claimed_by`, `claimed_session`, `claimed_at`, and `lease_expires_at`; the focused scenario asserts cleared fields and absence from `mine_only`. |
| F02 blocked dependent can complete | **RESOLVED** | Completion `UPDATE` has a `NOT EXISTS` prerequisite guard requiring every dependency to be `done`; test expects blocked completion to fail. |
| F03 supersede deadlock / two-node cycle | **RESOLVED** | Active dependents reject a successor that depends on the source; deterministic advisory locks serialize races; recursive successor check rejects reverse cycles. |
| F04 foreign takeover of legacy NULL lease | **RESOLVED** | Claim/complete/renew predicates consistently require the owning agent/session for NULL-lease claims; R6 restart evidence verifies the policy after process restart. |
| F05 cross-team successor/audit references | **RESOLVED** | Composite `(team_id, id)` uniqueness and composite foreign keys enforce same-team identity at the database boundary; focused test for forged cross-team rows expects failure. |
| F06 audit deleted with task | **RESOLVED** | Both audit task references use `ON DELETE RESTRICT`; focused test expects deletion of a transitioned task to fail. |

## Runtime and regression receipts

**FACT:** The R6 receipt reports the following executed results on disposable PostgreSQL:

- `cargo build --locked --offline --bin ai-crew-sync`: PASS.
- `cargo test --locked --offline --test task_cancel_supersede -- --ignored`: 2/2 PASS.
- Rust library tests excluding the known unrelated CRLF/LF recipe test: 105/105 PASS.
- Full Rust library run: 105 PASS / 1 unrelated baseline CRLF/LF recipe rendering failure; not represented as 106/106.
- Pagination integration: 10/10 PASS, including 2000-task concurrent pagination.
- Search integration: 10/10 PASS, including 2000-task search.
- `git diff --check`: PASS, exit 0.
- Real MCP/CLI lifecycle smoke test: `QA_E2E_GREEN`.
- MCP restart persistence: `QA_RESTART_GREEN`.
- Legacy NULL-lease process restart: `QA_STAGE1_GREEN`, one-row PostgreSQL NULL-lease mutation, then `QA_RESTART_GREEN`.

The R6 restart gate specifically reports preserved claimed state after a fresh server process, denial of foreign claim/complete/renew, successful original-owner renew/release, blocked completion of unresolved dependents, refusal of deadlocking supersede, and persisted cancellation readback.

**FACT:** This worker independently attempted the authorized focused runner once:

- `go_wave.run_project_tests({file:"tests/task_cancel_supersede.rs"})`
- Result: `GO_WAVE_NODE_ERROR:test_file_not_allowed`

This is an infrastructure limitation of the scoped runner, not a product failure. No identical retry was made. The direct cargo/R6 results above are recorded as supplied runtime evidence, not as a test executed by this worker.

## Risk boundaries

- **Technical candidate:** Accepted based on current source readback, test-contract coverage, the two prior independent reviews, and the R6 disposable-fixture receipts.
- **Production:** Not approved. R6 states the current production image is not byte-for-byte proven compatible with this candidate; differential build, backup, rollback, migration, and deployment authority remain external gates.
- **Known unrelated test issue:** The full Rust library run retains the Windows CRLF/LF recipe-rendering mismatch. It is outside this lifecycle write scope and does not invalidate the reported 105/105 affected-library result, but it remains a repository test-suite risk.
- **Evidence limitation:** The external R6 scripts/transcript were not available through this scoped project reader and were not re-executed here. Acceptance relies on the R6 receipt plus source/test reconciliation; an independent verifier still owns final acceptance.

## Changed files

- `docs/acceptance/CREWSYNC_LIFECYCLE_REDUCER_R7.md`

No Rust, SQL, CLI, task-status, claim, lease, lock, runtime, production, or deployment files were changed by this reducer.

## Reducer conclusion

**ACCEPTED_CODE_CANDIDATE** is the independent technical disposition. The six prior QA defects are resolved in the inspected source and migration, and the supplied R6 evidence covers build, lifecycle integration, library regression, pagination, search, MCP/CLI E2E, two-server restart, legacy NULL-lease restart, and whitespace validation. Independent verifier/reviewer and coordinator retain authority for final acceptance and any production rollout decision.
