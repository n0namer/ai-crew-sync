# Independent QA — project ordered queue

## Verdict

**REPAIR_REQUIRED** — do not promote this candidate until every claim path treats a
`superseded` prerequisite as terminal, matching the documented queue contract.

## Scope and method

This independent review inspected only the candidate migration, project-order store,
legacy claim guards, MCP route, specification, acceptance contract, and existing
isolated PostgreSQL tests. It applied adversarial review (attempted bypasses and
transaction races) and path tracing (terminal states, missing dependencies, tenant
boundaries, revision transitions, and lock outcomes). No production database,
CrewSync task, lease, or implementation source was modified.

## Concrete defect

1. **A superseded prerequisite can be omitted during planning but permanently blocks
   execution.** `replace_order` explicitly considers `superseded` terminal when it
   checks a same-project prerequisite outside the approved list
   (`src/store/project_order.rs:145`). The ordered claim update instead accepts only
   `done` and `cancelled` prerequisites (`src/store/project_order.rs:252`). The
   legacy named and global claim paths repeat the same two-state predicate
   (`src/store/tasks.rs:1010`, `src/store/tasks.rs:1134`). This contradicts the
   operator contract that a queue advances once its head becomes
   `done/cancelled/superseded` (`docs/acceptance/PROJECT_ORDERED_QUEUE.md:27`) and
   makes an approved queue unrecoverably blocked after legitimate supersession.

   Required repair: define one shared terminal-prerequisite rule and apply it to
   ordered, named, and global claim predicates. Keep the head-selection rule and
   dependency-omission validation aligned with that same terminal set.

## Focused adversarial probes

| Probe | Evidence | Result |
| --- | --- | --- |
| SQL replan versus ordered claim | Both operations lock the project row before touching queue/task state (`src/store/project_order.rs:70`, `src/store/project_order.rs:218`). | Guard present; concurrent claimant fails closed while the row is locked. |
| Stale revision / partial mutation | Revision comparison happens before entry deletion and transaction commit (`src/store/project_order.rs:80`, `src/store/project_order.rs:157`). | Guard present; rollback protects the prior list. |
| Cross-team queue access | Order, entry, and task queries bind `auth.team_id`; migration composite FKs include `team_id` (`migrations/0022_project_task_order.sql:2`, `src/store/project_order.rs:109`). | Guard present. |
| Old global/named claim bypass | Enrolled tasks are excluded from global claim and initial named claims (`src/store/tasks.rs:998`, `src/store/tasks.rs:1129`). | Guard present for non-expired initial claims. |
| Unauthorized queue actor | Queue replacement and ordered claim require authenticated `coordinator` (`src/store/project_order.rs:25`, `src/store/project_order.rs:212`). | Guard present; existing integration test exercises a non-coordinator name. |
| Same-project dependency omission | Unfinished same-project prerequisite outside the list is rejected (`src/store/project_order.rs:145`). | Guard present. |
| Cancelled/superseded head | Head lookup skips all three terminal task states (`src/store/project_order.rs:235`). | Guard present; independent test covers superseded head. |
| Independent project progress | Lock key includes `(team_id, project_key)` and ordered claim uses that key (`migrations/0022_project_task_order.sql:6`, `src/store/project_order.rs:220`). | Guard present, subject to coordinator E2E execution. |
| Absent/empty queue | Ordered claim has no legacy fallback (`src/store/project_order.rs:225`, `src/store/project_order.rs:240`). | Guard present. |
| Claimed head barrier | Claim update operates only on the first unfinished entry and returns no later work on failure (`src/store/project_order.rs:245`, `src/store/project_order.rs:263`). | Guard present. |

## Added isolated tests

`tests/project_task_order_adversarial.rs` adds two integration scenarios:

1. `superseded_prerequisite_is_terminal_for_ordered_claims` — expected to **fail on
   the current candidate**, demonstrating the concrete defect above.
2. `superseded_head_does_not_hold_back_later_ordered_work` — expected to pass,
   confirming that head selection itself correctly skips a superseded entry.

## Validation receipt

- Requested command: `cargo test --locked --offline --test project_task_order_adversarial`
- Agent execution: **NOT RUN**. The scoped GO runner permits only exact Node/Vitest/tsx
  files and cannot invoke the Rust/PostgreSQL integration engine. Per task authority,
  no shell, Docker, or production database was used.
- Coordinator action: run the requested command against an isolated ephemeral
  PostgreSQL database. The first test should fail before repair and pass after all
  prerequisite claim predicates include `superseded`.

## Acceptance recommendation

**REPAIR_REQUIRED.** The candidate has a concrete semantic contradiction in an
explicitly supported terminal state. Re-run the focused adversarial test and the
existing `project_task_order` suite after repairing the shared terminal-state rule;
independent acceptance remains coordinator-owned.
