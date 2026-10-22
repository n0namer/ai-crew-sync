# Task Search E2E Verification — Independent QA Evidence

**Task key:** `crewsync:task-search-verify-e2e-r1`  
**Project:** `crewsync`  
**Attempt:** `attempt-8bc495be-bef6-448d-9f74-442f1917be0e`  
**Session:** `manual-search-verify-e2e-r1-task-search-verify-e2e-r1`  
**Producer role:** `implementer`  
**Producer thread:** `null`  
**Review status:** `CANDIDATE / UNACCEPTED`  
**Review date:** 2026-10-09  
**Changed files:** `docs/acceptance/TASK_SEARCH_E2E_VERIFICATION.md` only

This is independent evidence and reducer preparation, not task administration. It does not claim `DONE`, `ACCEPTED`, `ACCEPTED_GREEN`, deployment, or release readiness. No source, test, migration, task state, lock, Ruflo state, or Native state was modified.

## Verdict

**REPAIR_REQUIRED + BLOCKED_INFRA; overall CANDIDATE / UNACCEPTED.**

The scoped implementation contains credible source-level coverage for search normalization, ACL predicates, signed cursor context, legacy pagination compatibility, literal escaping, page-plus-one traversal, and bounded regex execution. Independent acceptance is not justified because the disposable PostgreSQL 2,000-task run has no accessible raw receipt in the permitted project evidence, the mandated Node runner cannot execute this Rust test, and the scoped source does not show handling for SQLSTATE `22025` requested by the repair checkpoint. A BMad/spec sentence says the latest disposable run was `2/2 PASS`, but that statement is not an independent runtime receipt and is not promoted to acceptance evidence here.

## Evidence read directly

- `_bmad-output/implementation-artifacts/spec-task-search.md`: frozen V1 contract, mandatory E2E matrix, compatibility/cursor barriers, and the checkpoint statement that a latest disposable search run was reported `2/2 PASS` while the original E2E task remained claimed.
- `Cargo.toml`: Rust-only package; no JavaScript test target is declared.
- `src/store/tasks.rs`: search normalization, query construction, cursor context, old pagination path, searched pagination path, transaction-local timeout, regex error mapping, totals, and focused Rust tests.
- `src/tools/tasks.rs`: additive MCP arguments, defaults, validation, and legacy request mapping tests.
- `tests/task_search.rs`: ignored disposable PostgreSQL E2E with 2,000-task fixture, all search modes, ACL, filters, cursor replay, duplicate detection, old/new client calls, and timeout coverage.

No `go-core/runtime/waves/20261009-crewsync-task-search` receipt was accessible through the scoped reader. No coordinator status, claim, lock, or external runtime readback was used as a substitute.

## Acceptance matrix

| Area | Direct source evidence | Independent status | Exact finding / affected-only repair |
|---|---|---|---|
| Disposable PostgreSQL 2,000-task run | `tests/task_search.rs` is `#[ignore]` and requires `CREWSYNC_ENABLE_LARGE_FIXTURE=1` plus disposable `TEST_DATABASE_URL` | **BLOCKED_INFRA** | No raw run output, database/version, timing, page counts, unique count, terminal cursor, or client termination metadata is accessible. E2E owner/reducer must rerun only the disposable fixture and attach the complete receipt; do not edit product source for this evidence gap. |
| Old pagination compatibility | `list_tasks_page` retains the legacy no-search query, status/project/mine filters, stable rank/time/key order, `LIMIT limit + 1`, and team-wide totals. `list_tasks_page_with_search` is additive and uses the same continuation ordering. | **SOURCE-VERIFIED; RUNTIME-PENDING** | The old path is not visibly replaced by search. Run legacy MCP/CLI requests and compare ordering, totals, and cursor continuation against pre-search behavior. Repair only the owning compatibility test if a regression is reproduced. |
| Search before arbitrary page truncation | Searched SQL places `AND ({search_clause})` before `ORDER BY` and `LIMIT $12`; rows are fetched at `limit + 1`, then truncated. | **SOURCE-VERIFIED; RUNTIME-PENDING** | The 2,000-row traversal must prove no omissions or duplicates and record ten pages at limit 200 for the fixture's `task` contains query. |
| ACL and totals | Both page and totals queries bind `auth.team_id`; searched totals intentionally count the whole team, not search hits. The E2E creates a foreign team. | **SOURCE-VERIFIED; RUNTIME-PENDING** | Attach an explicit zero-leak assertion and exact open/claimed totals from the disposable run. |
| Modes and fields | Normalization supports `keywords`, `contains`, `regex`; fields `title`, `description`, `both`; languages `simple`, `english`, `russian`. MCP rejects unknown nonempty options. | **SOURCE-VERIFIED; RUNTIME-PENDING** | Record title-only/body-only, phrase, English, Cyrillic, literal `%`/`_`/backslash, and case-insensitive regex results from PostgreSQL. |
| Cursor binding and replay | Cursor context binds team, project prefix, status, mine flag, query, mode, fields, and language; E2E alters each search dimension. | **SOURCE-VERIFIED; RUNTIME-PENDING** | Verify altered-query/mode/field/language rejection through the real API boundary, not only unit/source review. |
| Regex safety and SQLSTATE handling | Regex uses transaction-local `statement_timeout` (`250ms` in the source path), a two-second outer test guard, NUL/size checks, and maps PostgreSQL SQLSTATE `2201B` to invalid input. | **REPAIR_REQUIRED + BLOCKED_INFRA** | The requested checkpoint names an SQLSTATE `22025` repair, but no `22025` handling or receipt is visible in the scoped files; the only explicit mapping is `2201B` (invalid regular expression). The owning SQL/E2E Goal Owner must reproduce the repaired failure, identify the actual PostgreSQL code, and add affected-only coverage or evidence for the correct code. Do not broaden error handling speculatively. |
| Pathological/non-indexable regex | E2E exercises `^(a+)+$`; the test bounds it with two seconds. | **BLOCKED_INFRA** | No PostgreSQL receipt proves timeout/cancellation behavior or safe completion. Run the disposable case and preserve error classification plus elapsed time. |
| FTS/index behavior | The visible search SQL uses runtime `$8::regconfig` and field-specific vector expressions; the scoped evidence has no EXPLAIN output. | **REPAIR_REQUIRED** | The owning SQL/index Goal Owner must reconcile fixed GIN expressions with runtime language/field branches or document a bounded accepted fallback, then attach `EXPLAIN (ANALYZE, BUFFERS)` for supported keyword languages and fields. This QA task does not change migrations or source. |
| Read-only/concurrency/lease safety | The E2E claims one task and checks `status=claimed,mine=true`; the frozen contract also requires read-only canary and concurrency/lease evidence. | **BLOCKED_INFRA** | No visible receipt covers read-only DB behavior or concurrent lease operations. Release/reducer owner must run those focused checks in disposable infrastructure. |
| Node validation | `run_node_tests` rejected `tests/task_search.rs` as `test_file_not_allowed`; `Cargo.toml` identifies a Rust project and no JS test target is present. | **BLOCKED_INFRA** | Node-only validation cannot execute this Rust test. Coordinator must use the appropriate Rust runner outside this scoped worker; do not reinterpret the unavailable Node run as a product failure or success. |

## Focused validation performed

- **Node test attempt:** `run_node_tests` on `tests/task_search.rs` returned `GO_WAVE_NODE_ERROR:test_file_not_allowed`. No test process ran.
- **Runtime receipt lookup:** no accessible file under `go-core/runtime/waves/20261009-crewsync-task-search`; no 2/2 output was independently observed.
- **Static review:** confirmed the old pagination path, searched page-plus-one path, team predicate, totals query, cursor context, regex timeout branch, and explicit `2201B` mapping from the scoped source.

The failed Node invocation is an infrastructure/tooling limitation, not evidence that the Rust test fails. Conversely, the spec's textual `2/2 PASS` checkpoint is not treated as proof without its raw receipt.

## Exact remaining defects

1. **Missing independent runtime evidence:** no accessible disposable PostgreSQL receipt for the 2,000-task search E2E, old client path, ACL, cursor replay, regex timeout, page traversal, or totals.
2. **SQLSTATE repair ambiguity:** scoped code explicitly handles `2201B`; the requested `22025` repair is neither visible nor independently proven. The actual failing SQLSTATE must be reproduced before changing behavior.
3. **Index-readiness evidence gap:** no EXPLAIN/timing receipt proves the runtime language and field branches use the intended indexes; source review indicates a likely mismatch risk for dynamic regconfig/field-specific expressions.
4. **Missing canary/concurrency evidence:** read-only and lease/concurrency requirements remain unverified.
5. **Rust validation unavailable in this worker:** the mandated Node tool cannot run the Rust test file; a suitable coordinator-level Rust check remains required.

## Proposed affected-only Goal Owner repair

- **E2E owner:** rerun the ignored test only against a disposable loopback PostgreSQL database, record fixture size exactly 2,000, database version, command, elapsed time, all mode/filter results, foreign-team exclusion, cursor mismatch errors, unique keys/page count, `has_more`, terminal cursor, and old/new client termination metadata.
- **SQL/index owner:** reproduce the `22025` checkpoint failure and reconcile it with PostgreSQL's actual SQLSTATE; update only the owning source/migration/test scope if a defect is confirmed. Capture EXPLAIN plans and timings for all required keyword fields/languages, contains, regex, and non-indexable regex.
- **Compatibility owner:** run old pagination/MCP/CLI requests and lease/concurrency checks; repair only the affected compatibility tests or implementation files if runtime evidence identifies a regression.
- **Reducer/coordinator:** preserve the raw runtime and Rust receipts, independently reconcile the prior `2/2 PASS` claim, and keep this task `UNACCEPTED` until blockers are cleared. No task/lock/claim administration is performed by this worker.

## Candidate decision

**CANDIDATE / UNACCEPTED.** Source inspection supports the intended architecture and several safety properties, but the requested independent E2E verification is not complete. The coordinator must not convert this artifact to `ACCEPTED` or `ACCEPTED_GREEN` from the spec checkpoint alone, from `turn.completed`, or from a non-receipted status. The only changed file is this document, within the exact owned write scope.
