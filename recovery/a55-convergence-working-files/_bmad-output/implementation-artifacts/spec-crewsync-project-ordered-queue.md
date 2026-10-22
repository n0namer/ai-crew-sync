---
title: 'CrewSync explicit per-project task order for one portfolio Mini Daemon'
type: 'feature'
created: '2026-10-13'
status: 'in-progress'
baseline_commit: '34a2c3e'
review_loop_iteration: 0
context: []
---

<frozen-after-approval reason="User-authorized continued execution: implement existing project-scoped queue without introducing another scheduler">

## Intent

**Problem:** The current CrewSync claim_next_task selects the oldest available task across all projects. The global Mini Daemon would therefore execute unrelated historical OPEN tasks instead of an explicitly approved order within the selected project.

**Approach:** Add the smallest additive **server-owned ordered queue per project**. An authorized coordinator can replace the exact ordered list of existing CrewSync task IDs using a revision fence; the global daemon selects a project and asks CrewSync to atomically claim its next ordered task. Existing task results, dependencies, leases and claim semantics stay canonical in CrewSync.

## Boundaries & Constraints

**Always:** Use existing CrewSync task keys and tenant boundaries. No task auto-enrollment; approved order is independent of created_at. Fixed next task is a barrier while claimed or dependency-blocked. Two different projects can claim concurrently; workers within one project cannot bypass the barrier. Revisions are CAS-style; reordering cannot invalidate a live writer. No hidden fallback to global claim_next_task. Long results stay in existing result/notes.

**Ask First:** Changes to operational service or live database; silent migration of historical backlog; non-FIFO parallel groups or any reinterpretation of approved order.

**Never:** New scheduling database outside CrewSync, duplicate task DAG, re-creating CrewSync tasks, Ruflo Mission M2 dependency, edits in the owner's dirty source checkout, production deploy/redeploy before GREEN.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|---|---|---|---|
| Approved order | Project P: tasks A,B,C created chronologically, order C,A,B approved | P claims C; B cannot jump past C/A | Exact project, tenant and revision checked |
| Other project | Projects P and Q have independent queues | Q progresses while P is blocked | No cross-project task access |
| No order | OPEN backlog exists but project queue is absent or explicitly empty | No task claimed | Fail closed, no global FIFO fallback |
| Stale revision | Two coordinators change order from revision N | First commits N+1; second is refused | Conflict with current revision |
| Active claim | Task in queue is actively claimed | Existing owner unaffected; no reordering | Reject revision change |
| Dependency | Earlier task depends on unfinished predecessor | Later queue tasks do not jump ahead | No claim until prerequisite terminal |
| Omitted unfinished dependency | Task A depends on unlisted same-project task B, still OPEN | Reject plan as deadlock-prone | Actionable dependency error |
| Out-of-order dependency | A depends on B but order is [A,B] | Reject impossible strict ordering | Actionable validation error |
| Completion | Earlier accepted task becomes done | Next queue member can be claimed | Atomic claim preserves lease/session |
| Corrupt input | duplicate/wrong-team/wrong-project task keys | No partial queue change | Full transaction rollback |

</frozen-after-approval>

## Code Map

- migrations/0001_init.sql — baseline tasks, team ACL, task events.
- migrations/0022_task_cancel_supersede_audit.sql + migrations/0023_project_task_order.sql — lifecycle audit/status schema remains the prerequisite; ordered queue is additive and uses its team/task uniqueness constraint.
- src/store/tasks.rs — existing global FIFO and named claim; preserve compatibility for unplanned tasks while refusing initial claims of enrolled keys.
- src/store/project_order.rs — scoped transactional read/replace/claim with DONE-only prerequisite/head completion, explicit replan barriers, and atomic rejection of newly proposed cancelled/superseded rows.
- src/store/mod.rs — existing module registry.
- src/tools/tasks.rs — existing MCP router, additive ordered-queue tools while preserving cancellation and supersession tools.
- tests/project_task_order.rs — isolated database E2E and validation.
- docs/acceptance/PROJECT_ORDERED_QUEUE.md — operator boundary, not a parallel PLAN.md.
- Original main untracked BMad pagination/search specs are other concurrent work; do not modify.

## Tasks & Acceptance

**Execution:**
- [x] migrations/0022_task_cancel_supersede_audit.sql + migrations/0023_project_task_order.sql — lifecycle audit/status schema and additive ordered queue coexist; strict team/project/position uniqueness and FK.
- [x] src/store/project_order.rs — validate input and dependencies; transactional CAS replacement rejects newly proposed cancelled/superseded rows before mutation, preserves order/revision on rejection, and keeps atomic project-scoped claims.
- [x] src/store/mod.rs + src/tools/tasks.rs — export get/replace/claim MCP tools, typed request and view.
- [x] src/store/tasks.rs — legacy claim_next_task excludes enrolled tasks, named claim_task only permits renewal by exact existing holder, and lifecycle dependency guards remain DONE-only.
- [x] tests/project_task_order.rs + tests/project_task_order_adversarial.rs — prove ordering, independent projects, stale revisions, claim concurrency, blocked/recovery behavior, tenant isolation, invalid order, explicit empty queue, and cancelled/superseded fail-closed paths.
- [x] docs/acceptance/PROJECT_ORDERED_QUEUE.md — small operator contract for Mini Daemon (project selection remains outside server).

**Acceptance Criteria:**
- Given P contains task keys [P:A,P:B,P:C] created in any order, when order [P:C,P:A,P:B] is committed at expected revision 0, then claim_next_project_ordered_task(P) issues C first.
- Given P has a claimed or blocked head, when a second worker requests P, then it receives no claim rather than A/B or another project's task.
- Given Q has an independently approved order, when P is blocked, then Q can claim its own head.
- Given an absent or empty queue, when claim is requested, then nothing from the legacy backlog is claimed.
- Given a task in an approved order, when a legacy named/global claim attempts to bypass ordered admission, then it is refused while a genuine holder may renew.
- Given concurrent stale planners or invalid dependency order, when they attempt replace, then no partial mutation is visible.
- Given a cancelled or superseded prerequisite/head, when ordered claim or replacement is attempted, then the queue fails closed and requires explicit replan; only DONE satisfies readiness.
- Given a newly proposed cancelled or superseded task, when replace_order runs at the current or stale revision, then it rejects without changing the existing order or revision.
- Given migration and code, when containerized cargo check/tests run, then they complete GREEN without changing the deployed service.

## Spec Change Log

- 2026-10-13: Resumed already-started isolated per-project queue implementation. User specifically chose CrewSync as queue authority for one global multi-project Mini Daemon.
- 2026-10-13: Added focused blocked-head recovery coverage; implementation remains a candidate pending coordinator Docker Rust/PostgreSQL verification and independent QA.
- 2026-10-13: Resolved lifecycle/project-order merge markers, preserved cancellation and supersession APIs, and made queue prerequisites and heads DONE-only; cancelled or superseded work now requires explicit replan.
- 2026-10-13: Independent QA required repair: reject newly proposed cancelled/superseded rows before an ordered replan mutates entries or revision; adversarial regression verifies both terminal statuses and stale revision preservation. No deploy or acceptance action was performed.
- 2026-10-13: Lifecycle/order convergence pass restored `superseded` support in the legacy task-list status filter and added public cancellation-head coverage proving later ordered work remains blocked until explicit replan. The scoped runner again refused Rust integration tests with `test_file_not_allowed`; host PostgreSQL verification remains required.

## Design Notes

Only replace explicit per-project position lists. Selecting which project should be served is already portfolio-level responsibility of GO Core; do not add a competing dispatcher to CrewSync. Strict per-project serial barriers deliberately prioritize predictable approved order; parallel branch semantics can be added only with explicit approval.

## Verification

**Commands:**
- Docker rust:1.98.1-slim cargo check --locked --offline — compile without touching live service.
- Cargo tests for project_task_order against isolated ephemeral PostgreSQL.
- git diff --check and independent adversarial/edge-case review before commit.

**Current gate:** Candidate implementation prepared; focused SQL E2E is coordinator-owned and pending because the scoped runner denied the Rust integration test. Independent QA and production gates remain pending.
