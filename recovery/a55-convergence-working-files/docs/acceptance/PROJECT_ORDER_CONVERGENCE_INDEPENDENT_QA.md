# Project Order Convergence — Independent QA

## Review status

**REPAIR_REQUIRED** — this is an independent candidate review only. It does not accept the implementation, complete a CrewSync task, deploy a migration, or approve production use.

## Scope and method

- Reviewed the merged ordered-queue implementation and migrations named by the task: baseline `34a2c3e`, `migrations/0022_task_cancel_supersede_audit.sql`, and `migrations/0023_project_task_order.sql`.
- Read the BMad Help, adversarial-review, and edge-case-review workflows before review.
- Traced project-order admission, project-scoped claim, legacy named/global claims, lifecycle transitions, SQL foreign keys, and existing PostgreSQL integration tests.
- Added `tests/project_order_convergence_independent_qa.rs` as an independent terminal-state admission test. The scoped runner declined that Rust test; see Verification.

## Blocking finding

1. **A new revision can explicitly admit a terminal task.** `replace_order` locks and reads each proposed task but rejects only a live `claimed` row (`src/store/project_order.rs:107` through `src/store/project_order.rs:123`). It does not reject `cancelled` or `superseded` rows. The mutation then deletes and recreates entries and advances the revision (`src/store/project_order.rs:151` through `src/store/project_order.rs:185`).
   - Trigger: a coordinator submits a fresh order whose first entry has already become `cancelled` or `superseded`.
   - Consequence: the invalid revision and immutable history snapshot commit. `claim_next_ordered` subsequently selects that terminal item because it selects every status other than `done` (`src/store/project_order.rs:225` through `src/store/project_order.rs:239`) and its claim update only accepts `open` or expired `claimed` work (`src/store/project_order.rs:250` through `src/store/project_order.rs:260`). The queue is therefore blocked until another replan.
   - Required repair: reject terminal entries during `replace_order` before any delete/insert/revision update; keep the queue/revision unchanged on rejection. Update existing adversarial tests whose current assertions intentionally admit terminal entries before observing a blocked claim.
   - Independent regression: `tests/project_order_convergence_independent_qa.rs` expects both terminal states to be rejected and revision 0 to remain empty.

## Additional contract finding

2. **The public task-creation dependency comment is stale against the DONE-only implementation.** `CreateTaskArgs.depends_on` says dependencies may be `done or cancelled` (`src/tools/tasks.rs:78` through `src/tools/tasks.rs:80`), while actual claim/completion conditions use `prerequisite.status <> 'done'` as the blocker (`src/store/tasks.rs:1050` through `src/store/tasks.rs:1054`; `src/store/tasks.rs:1378` through `src/store/tasks.rs:1382`). This can cause a client to infer that cancelling a prerequisite unlocks its successor, contrary to migration 0022 and queue behavior.
   - Required repair: update the public comment/tool description to say only `done` resolves dependencies.

## Verified static controls

- **Tenant fence:** migration 0023 uses `(team_id, task_id)` foreign keys for entries and a `(team_id, project_key)` parent key (`migrations/0023_project_task_order.sql:15` through `migrations/0023_project_task_order.sql:24`); migration 0022 adds the prerequisite composite task uniqueness constraint (`migrations/0022_task_cancel_supersede_audit.sql:8` through `migrations/0022_task_cancel_supersede_audit.sql:15`).
- **Revision CAS and replan serialization:** `replace_order` creates/locks the project row, compares `expected_revision`, and performs all queue/history writes in one transaction (`src/store/project_order.rs:64` through `src/store/project_order.rs:88`; `src/store/project_order.rs:151` through `src/store/project_order.rs:192`).
- **Ordered-head barrier:** ordered claims lock the project row and never select a later task when the first unfinished entry is blocked or claimed (`src/store/project_order.rs:206` through `src/store/project_order.rs:278`). CANCELLED and SUPERSEDED are not treated as DONE by that path.
- **Lease-expiry behavior:** the ordered claim update explicitly permits an expired `claimed` head to be taken, while legacy entry points exclude enrolled entries from first admission (`src/store/project_order.rs:250` through `src/store/project_order.rs:260`; `src/store/tasks.rs:1020` through `src/store/tasks.rs:1054`; `src/store/tasks.rs:1147` through `src/store/tasks.rs:1179`).
- **Existing test coverage:** `tests/project_task_order.rs` covers tenant/authority fences, stale revisions, claims, and legacy bypasses; `tests/project_task_order_adversarial.rs` covers cancelled/superseded heads and prerequisites. Those tests do not reject a terminal task at replan admission; they currently encode the delayed-claim-block behavior identified above.

## Edge-case review output

[
  {
    "location": "src/store/project_order.rs:107-185",
    "trigger_condition": "Fresh revision contains cancelled or superseded proposed task",
    "guard_snippet": "if matches!(status.as_str(), \"cancelled\" | \"superseded\") { return Err(BusError::invalid(\"terminal tasks require removal from the approved replan\")); }",
    "potential_consequence": "Blocked revision and extra coordinator replan"
  },
  {
    "location": "src/tools/tasks.rs:78-80",
    "trigger_condition": "Client relies on cancelled prerequisite documentation",
    "guard_snippet": "Change documentation to: every dependency must be done before claiming.",
    "potential_consequence": "Client schedules successor behind terminal blocker"
  }
]

## Verification

- **Static review:** completed against the files in the task read scope.
- **Independent negative test authored:** `tests/project_order_convergence_independent_qa.rs`.
- **Focused test command requested:** `cargo test --locked --offline --test project_order_convergence_independent_qa`.
- **Actual scoped runner receipt:** `go_wave.run_project_tests` returned `GO_WAVE_NODE_ERROR:test_file_not_allowed` for the Rust test file. No retry was attempted because this runner only authorizes Node/Vitest/tsx tests.
- **Required coordinator verification:** run the requested Cargo test in an isolated disposable PostgreSQL environment, then run the affected project-order suites after the repair. This worker did not run Cargo, Docker, or any production database command.

## Required next action

Repair the two source-contract issues outside this QA task's write scope, starting with terminal-state rejection in `src/store/project_order.rs`; then use this independent test plus the existing project-order and adversarial integration tests for PostgreSQL verification. Re-review is required after the repair.
