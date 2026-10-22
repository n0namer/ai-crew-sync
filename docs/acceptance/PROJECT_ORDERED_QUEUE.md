# CrewSync server-owned ordered task queues

## Status

Candidate implementation in isolated branch `feat/crewsync-project-ordered-queue-20261013`.
Not deployed. The existing CrewSync server and historic global FIFO remain unchanged until an independently accepted promotion.

## Problem and rule

CrewSync `claim_next_task` currently selects by `created_at` across the whole team. A portfolio Mini Daemon must not use it to consume the large unrelated OPEN backlog. Each project instead has one **explicit approved ordered list of existing CrewSync task keys**.

One global GO Core Mini Daemon may distribute resource slots across all projects; this new CrewSync API only answers: given a chosen project, which is its first unfinished approved task? The answer is transactional and respects claims and task dependencies.

## MCP API

These new tool names are additive:

1. `get_project_task_order({ "project": "gocore" })` returns `revision` and ordered `task_keys`. An absent queue returns revision 0, empty list.
2. `replace_project_task_order({ "project": "gocore", "expected_revision": 0, "task_keys": ["gocore:task-B","gocore:task-A"] })` creates revision 1. Tasks **must already exist**. Repeat with fresh revision to change ordering. Empty list is a deliberate project pause.
3. `claim_next_project_ordered_task({ "project": "gocore", "lease_seconds": 900 })` atomically claims the first unfinished listed task. If it is blocked or actively claimed, returns `claimed:false`, never a later task or a different project.

Read revision, submit change with exact expected revision, then re-read. A stale revision, foreign/duplicate task, inconsistent internal dependency, active claim or unauthorized agent is rejected atomically.

Only authenticated agent `coordinator` may submit plan changes or claim from the ordered queue; other agents may read it. Task results still use existing CrewSync `result`, `notes` and immutable evidence references.

## Semantics and replan

- **No implicit enrollment:** old OPEN cards stay outside approved project queues, regardless of age.
- **Sequential per project:** the first unfinished entry is a barrier until it becomes done/cancelled/superseded; when blocked by dependencies or held by another session the API reports no claim. This prevents priority inversion.
- **Parallel across projects:** each project has a distinct row lock; while `gocore` is blocked, `ultra` may claim its independent head.
- **No omitted prerequisites:** if a task depends on another unfinished task of the same project, that prerequisite must appear earlier in the approved order. Already-terminal prerequisites may remain outside the list.
- **CAS replan:** the impact analyzer may propose the next exact order after independent acceptance. The coordinator must wait for own/foreign live claims to terminate, then call replace with the last observed revision. The server stores an immutable snapshot in `project_task_order_revisions`.
- **Historical jobs:** do not mark any OPEN item READY merely because it exists; governance/registry/dirty-worktree checks remain obligations of the Mini Daemon before selecting a project or running a worker.
- **Legacy operations:** global `claim_next_task` skips enrolled project tasks, while named `claim_task` refuses their initial claim. Only the already authorized session may renew an enrolled task with the named claim API. Both original APIs remain available for tasks outside all approved orders. Mini Daemon must use the new ordered API exclusively and treat any legacy fallback as a configuration error.
- **Parallel branches inside one project** are not part of this strict FIFO DoD. A later revision or explicit separate queue lanes may add parallelism, with conflict and dependency proof.

## Validation and deployment gate

Run `cargo check --locked --offline` and `cargo test --locked --offline --test project_task_order` using the project's cached Rust Docker image and a **new ephemeral PostgreSQL** database. No test should use `docker-postgres-1` or the running CrewSync database. Test cases include out-of-creation order, project isolation, stale planners, active claims, forward dependencies, blocked-head recovery, cancellation/pause, and concurrent claims. The scoped GO runner denied this Rust integration test in the worker environment; coordinator Docker verification is required before acceptance.

Before enabling in production: independent adversarial + edge-case review, exact live service binary/version and migration preflight, backup/reversal strategy, QA-approved integration with global Mini Daemon's project selection. This worker result is a CANDIDATE only; coordinator and independent QA own acceptance. Do not restart/redeploy CrewSync before these gates are GREEN.
