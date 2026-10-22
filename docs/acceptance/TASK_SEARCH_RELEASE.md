# CrewSync Task Search Release Reconciliation

**Task key:** `crewsync:task-search-release-r1`  
**Project:** `crewsync`  
**Attempt:** `attempt-d23d3c1d-0ef9-49ce-8cd3-c3d5c609c57b`  
**Session:** `manual-search-release-r1-task-search-release-r1`  
**Review date:** 2026-10-09  
**Producer role:** `implementer`  
**Producer thread:** `null`  
**Review status:** `CANDIDATE / UNACCEPTED`  
**Changed files:** `docs/acceptance/TASK_SEARCH_RELEASE.md` only

This document is release-reducer evidence, not task administration. It does not claim `DONE`, `ACCEPTED`, `ACCEPTED_GREEN`, deployment, merge, or production readiness. No product source, migration, test source, task state, claim, lock, Ruflo state, Native state, or Git state was modified.

## Decision

**`BLOCK_INFRA` with `REPAIR_REQUIRED`; overall `CANDIDATE / UNACCEPTED`.**

Release cannot be accepted from the accessible evidence. The BMad source of truth requires full Rust formatting/check/test evidence, a disposable PostgreSQL 2,000-task run, exact production-shaped regex/contains/keyword EXPLAIN evidence, a read-only database canary, unaffected lease/concurrency behavior, backwards-compatible old MCP/CLI behavior, and independently accepted predecessor readbacks. Those receipts are not available through the scoped project reader and cannot be reconstructed from narrative status, `turn.completed`, or candidate documents.

A source-level release blocker is also recorded by the independent review artifacts: the migration indexes a concatenated trigram expression and fixed-language weighted combined vectors, while the production query paths use separate title/description predicates and a runtime-bound regconfig. Correctness may still hold, but index use and bounded cost for the actual production SQL are not proven. This requires affected-only SQL/index repair or an independently accepted bounded fallback with exact EXPLAIN evidence.

No production deployment or release authorization is made.

## Authority and reconciliation boundary

The authority is `_bmad-output/implementation-artifacts/spec-task-search.md`, with pagination compatibility checked against `_bmad-output/implementation-artifacts/spec-task-list-pagination.md`. The BMad checkpoint states that initial implementation and compatibility tasks had per-scope `ACCEPTED_GREEN`, and that a disposable PostgreSQL search run was reported `2/2 PASS`. The accessible project tree does not contain the coordinator-owned CrewSync task readbacks, immutable receipts, or the referenced runtime wave directory. Therefore those statements remain leads and are not promoted to independent acceptance here.

The following visible acceptance artifacts all explicitly remain `CANDIDATE / UNACCEPTED`:

- `docs/acceptance/TASK_SEARCH_REVIEW.md`: adversarial review; repair required for production-shaped index/benchmark alignment and missing runtime evidence.
- `docs/acceptance/TASK_SEARCH_GATE_VERIFICATION.md`: release-gate QA; repair required for FTS/index alignment, with runtime/toolchain/canary/PR evidence blocked.
- `docs/acceptance/TASK_SEARCH_E2E_VERIFICATION.md`: E2E QA; blocked by absent raw PostgreSQL receipt, unavailable Rust runner, and unresolved SQLSTATE repair checkpoint.
- `docs/acceptance/TASK_SEARCH_REGRESSION.md`: regression evidence; runtime, EXPLAIN, Rust, canary, and compatibility receipts pending.
- `docs/acceptance/TASK_SEARCH_PR_READINESS.md`: PR-readiness review; index readiness, ignored E2E, Rust execution, mutable-inventory wording, and PR parity remain unresolved.
- `docs/acceptance/TASK_SEARCH_PERFORMANCE.md`: benchmark procedure only; it expressly says candidate evidence and does not certify acceptance.

No exact CrewSync accepted-result readback was independently accessible in this bounded attempt. This is an evidence boundary, not a claim that the coordinator's records do not exist.

## BMad DoD reconciliation

| Requirement | Visible/source status | Independent release status | Decision / affected-only action |
|---|---|---|---|
| Frozen search contract and additive inventory | Search modes, fields, languages, escaped contains, regex bounds, signed cursor context, stable order, team totals, and title/description scope are visible in source and tests | **SOURCE-VERIFIED; RUNTIME-PENDING** | Preserve the existing list inventory; run public MCP/CLI cases before acceptance. |
| Empty/missing search backwards compatibility | Legacy no-search path exists; old MCP deserialization and empty-search behavior are covered in source tests | **RUNTIME-PENDING** | Compare old request shape, ordering, totals, cursor behavior, and timeout/lease side effects against the pre-search path. Repair only the owning compatibility scope if reproduced. |
| Search before arbitrary page truncation | Search predicates are placed before `ORDER BY` and `LIMIT`; page-plus-one logic is visible | **SOURCE-VERIFIED; RUNTIME-PENDING** | Disposable traversal must record page count, unique keys, `has_more`, and terminal cursor for 2,000 tasks. |
| ACL, filters, and team-wide totals | Team predicate, status/mine/project-prefix filters, and separate whole-team totals are visible | **SOURCE-VERIFIED; RUNTIME-PENDING** | Require explicit foreign-team zero-leak and exact totals in the raw API/E2E receipt. |
| Cursor binding and replay rejection | Team, filters, query, mode, fields, and language are represented in signed cursor context; unit cases are visible | **SOURCE-VERIFIED; API-RECEIPT-PENDING** | Verify altered query/mode/field/language replay through the real MCP/API boundary. |
| Keywords, phrase, English/Russian, title/description fields | Whitelists, websearch parsing, field branches, and fixture cases are visible | **SOURCE-VERIFIED; POSTGRES-PENDING** | Run all required dictionaries/fields on disposable PostgreSQL and preserve database version and output. |
| Literal contains safety | `%`, `_`, and backslash are escaped before bound `ILIKE` with an explicit escape character | **SOURCE-VERIFIED; POSTGRES-PENDING** | Confirm literal wildcard behavior against the real database. |
| Regex safety and SQLSTATE handling | NUL/size validation, `~*`, local `250ms` timeout, and explicit `2201B` mapping are visible; benchmark expects timeout `57014` at lower level | **REPAIR_REQUIRED + RUNTIME-PENDING** | Reproduce the actual public timeout and invalid-regex SQLSTATE. Do not add speculative handling for `22025`; repair only the owning SQL/E2E scope after reproduction. |
| Regex cost and non-indexable patterns | A performance test defines 2,000 rows, `EXPLAIN`, a pathological pattern, `statement_timeout=250ms`, and a two-second outer bound | **BLOCKED_INFRA** | No raw plan/timing receipt is accessible. Run exact production-shaped SQL, including title, description, both, usable and non-indexable patterns. |
| FTS/trigram index alignment | Migration has one concatenated trigram index and fixed-language weighted combined FTS indexes; independent review identifies mismatch risk with runtime production branches | **REPAIR_REQUIRED** | Align query branches with exact indexed expressions, add exact field/language indexes, or prove an accepted bounded fallback with `EXPLAIN (ANALYZE, BUFFERS)`. |
| Full `cargo fmt`, `cargo check`, and `cargo test` | `Cargo.toml` confirms a Rust project and ignored PostgreSQL tests are present | **BLOCKED_INFRA** | No Cargo runner or raw result is available through the scoped tools. Coordinator must run the complete requested commands and preserve exact exit/output receipts. |
| Disposable DB canary | Fixture safety gate and isolated-team setup are visible in the test sources | **BLOCKED_INFRA** | Run only against safe loopback disposable PostgreSQL; record migration, read-only canary, cleanup, database identity/version, and exit status. Never use production. |
| Lease/concurrency unaffected | Source retains existing task claims/leases; search E2E includes only limited claim/filter coverage | **BLOCKED_INFRA** | Run focused concurrent claim/lease regression and a healthy request after regex timeout; preserve raw receipt. |
| Immutable predecessor acceptance | BMad narrative reports accepted predecessors, but no exact coordinator-owned readbacks are accessible here | **BLOCKED_INFRA** | Coordinator must independently read back exact task keys, attempts, sessions, scopes, immutable result status, and terminal acceptance. Do not infer from `done`, `VERIFIED`, or `blocked=false`. |
| No P0 blockers | Visible review artifacts retain release blockers and pending evidence | **BLOCKED_INFRA** | Keep release closed until index alignment and all mandatory receipts reconcile with the BMad DoD. |
| PR/commit parity and English-only diff | BMad references Draft PR #202 and commit `f213f1e`; scoped tools cannot verify GitHub/local parity | **BLOCKED_INFRA** | Coordinator/reviewer must provide trusted head/diff parity and full changed-diff language audit. |
| Mutable inventory semantics | Cursor has no snapshot identifier; BMad permits best-effort mutable consistency | **REPAIR_REQUIRED** | Release wording must state best-effort continuation, not point-in-time snapshot consistency. |

## Exact source-level findings

### R1 — Production search SQL does not match benchmark/index expressions

The visible migration defines `tasks_search_trgm_idx` over one concatenated `coalesce(title, '') || ' ' || coalesce(description, '')` expression and fixed-language weighted combined FTS indexes. The visible production search implementation has separate title/description branches and uses `$8::regconfig` for runtime language selection. The benchmark procedures likewise demonstrate concatenated contains/regex SQL rather than every production-shaped field branch.

This does not establish incorrect results, but it prevents a release claim that the required PostgreSQL regex/contains/keyword cost is bounded by the intended indexes. The affected owner must either make production SQL expressions exactly match the indexes, add exact indexes for supported field/language branches, or provide an explicitly accepted bounded fallback. The repair must include `EXPLAIN (ANALYZE, BUFFERS)` after `ANALYZE` for title, description, and both fields; simple, English, and Russian keyword paths; literal contains; case-insensitive regex; and a deliberately non-indexable regex.

### R2 — Regex error classification is not independently reconciled

The visible source explicitly maps SQLSTATE `2201B` for invalid regular expressions. The benchmark expects PostgreSQL cancellation `57014` for the pathological timeout. One prior QA artifact names a requested `22025` repair checkpoint, but no scoped source or raw receipt proves that `22025` is the actual failure code. The correct action is reproduction through the public store/MCP boundary, then an affected-only repair if the actual code and contract require it. No speculative broad error mapping is authorized by this document.

### R3 — Release evidence is incomplete, not green by narrative

The ignored PostgreSQL tests, performance benchmark, and source-level unit tests describe the required cases, but no accessible receipt proves they ran successfully. Existing acceptance artifacts consistently distinguish source verification from runtime pending. The BMad `2/2 PASS` checkpoint is retained as an unverified lead because its immutable raw output is unavailable in the allowed read scope.

### R4 — Mutable pagination is best effort

The signed cursor binds filters and rank/time/key continuation but not a snapshot. Inserts, status changes, lease expiry, and `updated_at` changes can move rows between pages. This is allowed by the frozen contract only as explicitly documented best-effort consistency and must not be described as snapshot pagination.

## Required affected-only repair and evidence sequence

1. **SQL/index owner:** reproduce exact production-shaped plans and regex behavior; repair only the owning source/migration/test files if a defect is confirmed.
2. **E2E owner:** run the ignored 2,000-task disposable PostgreSQL fixture, including ACL, filters, all modes/languages/fields, cursor replay, old/new MCP/CLI requests, regex invalid/pathological cases, unique traversal, totals, and cleanup; preserve raw logs.
3. **Compatibility owner:** run old requests and focused lease/concurrency checks; repair only the affected compatibility scope if runtime behavior differs.
4. **Regression/release owner:** run `cargo fmt -- --check`, `cargo check --tests`, and `cargo test --all-targets` (plus the disposable ignored suites), preserve exit statuses/output, inspect generated-recipe drift, and reconcile failures without unrelated edits.
5. **Coordinator/reviewer:** independently read back every predecessor's exact task/attempt/session/result and acceptance status, verify immutable receipt integrity, confirm no P0, and only then decide release. This document remains unaccepted and does not self-accept.

## Validation performed in this bounded attempt

- Static reconciliation of the BMad specs and all accessible search acceptance artifacts listed above.
- Direct read of `Cargo.toml`, `migrations/0020_task_search.sql`, and `tests/task_search_performance.rs`.
- No Cargo, PostgreSQL, Git, deployment, CrewSync administration, claim, lock, or task-completion operation was performed.
- Node validation is attempted separately and its exact result is preserved in the final typed worker evidence; Node is not a substitute for Rust or PostgreSQL validation.

## Candidate release outcome

**Outcome: `BLOCK_INFRA`; repair required before any release decision.**

The only changed file is `docs/acceptance/TASK_SEARCH_RELEASE.md`, inside the exact required write scope. This artifact is evidence for coordinator/reviewer handling only; it does not accept the task, mutate task state, authorize deployment, or replace immutable CrewSync readback.
