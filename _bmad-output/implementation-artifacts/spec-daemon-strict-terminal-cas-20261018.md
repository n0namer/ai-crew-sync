---
title: CrewSync strict Daemon terminal CAS
type: feature
created: 2026-10-18
status: in-review
baseline_commit: 11a477ee3b13a80a9047dfa5d3d13faad85eb70d
review_loop_iteration: 0
---

# Goal
Legacy CrewSync complete_task may end an open or expired task and is not suitable for Daemon QA-bound SERVER_DONE. Preserve legacy operation. Add a separate stronger atomic settlement, reusing existing revision-fenced update_task.

# Current candidate
- migrations/0024_daemon_strict_settlement.sql: per-task durable fencing and independent QA acceptance rows.
- src/store/daemon_settlement.rs: internal-only transactional strict settlement with team, task source revision/SHA, active owner/session lease, attempt, generation, token and unconsumed independent QA digest.
- src/store/mod.rs: module registration only; NO public MCP strict settlement operation.

# Acceptance
- [x] Candidate source in isolated CrewSync worktree, existing legacy completion unchanged.
- [x] No self-acceptance flag, no public bypass, no production live settlement.
- [ ] Compile with Rust 1.98.1+ and verify PostgreSQL migration.
- [ ] Physical negative and race tests: open/expired task, foreign owner, stale source, wrong attempt/fence, forged/consumed QA, double completion, restart.
- [ ] Authenticated independent reviewer proof insertion and native fence generation with audited database permissions.
- [ ] Independent SHA-bound BMad adversarial/edge review, QA/reducer.
- [ ] Deploy only after full GREEN and controlled release approval.

# Explicit blockers
Rust toolchain unavailable on A55 PATH. No trusted QA registration or source/fence admission exists. Database-row presence alone does not prove cryptographic reviewer authentication. No PostgreSQL test yet. Current implementation is CANDIDATE / IN REVIEW, cannot issue SERVER_DONE.

## 2026-10-18 BMad Adversarial + Edge Case review (source-only)

Review evidence: %TEMP%/crewsync-strict-settlement-review-result-20261018.txt, verdict QA_REPAIR. Identified missing authenticated QA insertion and team isolation, temporal lease expiry using transaction-start now(), mutable proof between first UPDATE and consumed_at write, and missing fence/QA row locks. Sticky source repair: task->fence->QA row-lock order, clock_timestamp lease read and full receipt-binding predicate when consuming QA. These changes are NOT independently re-reviewed on a frozen repaired SHA. Reviewer identity inequality is not cryptographic independence; server proof-registration and database permission enforcement remain required. Compatibility with production DB migration (v23->v24), real Rust compile and concurrency tests remain unverified. Do not expose settle_strict via MCP or claim QA_ACCEPT.

## Focused QA_REPAIR follow-up
Confirmed baseline migration 0022 already defines tasks(team_id,id) uniqueness, supporting the same-task foreign key in migration 0024. This patch adds agents(team_id,id) uniqueness for same-team reviewer and owner FK, QA expiry, explicit locking order task -> fence -> QA, full bound receipt consumption, and clock_timestamp() freshness. These repair concrete source-only findings but are NOT a replacement for Rust compilation, PostgreSQL transactional/race tests, independent reviewer authentication or issuer monotonicity checks. Current reviewer finding remains QA_REPAIR. No MCP tool registration, no production mutation or deployment.

## Additional reviewer-binding repair
A second source-only BMad/Edge review `%TEMP%/crewsync-strict-postrepair-review-result-20261018.txt` remained BLOCKED, highlighting unbound result data, proof-issuer authority and monotonic native generations. Sticky repair adds SHA-256 of exact settled result to QA row and both settlement/consumption predicates, plus task revision increment on terminal. Still missing: authenticated reviewer writer/role isolation, monotonic fence issuance under the same row-lock protocol, verified privileges, migration/Rust compile/physical PostgreSQL concurrent tests. Explicitly do not use this candidate for production and do not interpret stored QA rows as signed evidence.
