# Task Search Regex Verification — Independent QA Evidence

**Task key:** `crewsync:task-search-verify-regex-r1`  
**Project:** `crewsync`  
**Attempt:** `attempt-350ac51e-fe2d-4367-9b2a-ddefd4a30c35`  
**Session:** `manual-search-verify-regex-r1-task-search-verify-regex-r1`  
**Trusted producer thread:** `01a1204c-dffc-7672-8c3c-3c8ecf8543ea`  
**Producer role:** `implementer`  
**Report producer thread:** `null`  
**Review date:** 2026-10-09  
**Review status:** `CANDIDATE / UNACCEPTED`  
**Changed files:** `docs/acceptance/TASK_SEARCH_REGEX_VERIFICATION.md` only

This is independent QA and reducer-preparation evidence, not task administration. It does not claim `VERIFIED`, `ACCEPTED`, `ACCEPTED_GREEN`, completion, deployment, or release readiness. No product source, test source, migration, task state, lock, Ruflo state, or Native state was modified.

## Corrected verdict

**Overall classification: `REPAIR_REQUIRED`; independent acceptance: `NOT ACCEPTED`.**

The earlier report correctly identified that the scoped Node worker cannot execute Rust tests or directly read GO Core external runtime files, but it incorrectly treated that access boundary as proof that the coordinator's runtime tests and receipts did not exist. The receipts listed below are explicitly tagged **`COORDINATOR_VERIFIED_EXTERNAL_RECEIPT`**. They are coordinator evidence, not independently executed by this scoped worker and not a substitute for acceptance.

The external evidence closes the previously pending FTS, positive trigram, MCP E2E, regression, build, and staged-image CLI evidence gaps. The remaining affected-only product gap is regex totals-query cancellation mapping: `SQLSTATE 57014` remains unproven as correctly mapped by the production totals path. The registered repair task is `crewsync:task-search-regex-total-timeout-r1`; its eventual code/test receipt must be independently assessed before closure. Do not claim production release green.

## Evidence boundary and provenance

### Scoped worker evidence

Read from the allowed project scope:

- `_bmad-output/implementation-artifacts/spec-task-search.md`
- `tests/task_search_performance.rs`
- `docs/acceptance/TASK_SEARCH_PERFORMANCE.md`
- `migrations/0020_task_search.sql`
- `Cargo.toml`
- this acceptance document before amendment

The scoped `run_node_tests` interface cannot execute the Rust benchmark. That limitation is reported as an interface boundary only; it is not evidence that the benchmark was never run elsewhere.

### Coordinator-verified external receipts

The following are supplied as coordinator-verified runtime/build receipts. The paths are external to this scoped project reader and are not independently re-executed here:

- **`COORDINATOR_VERIFIED_EXTERNAL_RECEIPT`** — `D:/Users/NIKITA/Documents/DEV/go-core/.local-ops/crewsync-bluegreen-20261009/fts-field-index-qa.json`: isolated PostgreSQL 18.4 migrated clone with 4,053 tasks; 9/9 `EXPLAIN (ANALYZE, BUFFERS)` cases across `simple|english|russian` × `title|description|both`; each plan used `tasks_search_fts_<language>_idx`; observed execution 0.12–19.9 ms.
- **`COORDINATOR_VERIFIED_EXTERNAL_RECEIPT`** — `D:/Users/NIKITA/Documents/DEV/go-core/.local-ops/crewsync-bluegreen-20261009/trgm-regex-index-qa.json`: 6/6 contains/regex × `title|description|both`; plans used exact per-field `tasks_title_search_trgm_idx` / `tasks_description_search_trgm_idx`; observed execution 0.58–5.83 ms. A non-indexable regex EXPLAIN was recorded, but actual 250 ms timeout behavior remains unconfirmed.
- **`COORDINATOR_VERIFIED_EXTERNAL_RECEIPT`** — `D:/Users/NIKITA/Documents/DEV/go-core/go-core/runtime/waves/20261009-crewsync-task-search/full-validation-results.json`: separate MCP Rust E2E 14/14 PASS across seven modules and supporting 20/20 PASS.
- **`COORDINATOR_VERIFIED_EXTERNAL_RECEIPT`** — `D:/Users/NIKITA/Documents/DEV/go-core/go-core/runtime/waves/20261009-crewsync-task-search/full-regression-results.json`: canonical full library regression 105/105 PASS.
- **`COORDINATOR_VERIFIED_EXTERNAL_RECEIPT`** — `D:/Users/NIKITA/Documents/DEV/go-core/go-core/runtime/waves/20261009-crewsync-task-search/canonical-build-results.json`: `cargo check --tests` PASS.
- **`COORDINATOR_VERIFIED_EXTERNAL_RECEIPT`** — `D:/Users/NIKITA/Documents/DEV/go-core/.local-ops/crewsync-bluegreen-20261009/authenticated-image-canary-result.json`: staged candidate Docker image traversed all 4,053 tasks uniquely with zero duplicates, `has_more=false`, and `exhausted=true`; the legacy rollback image returned its 50-task page; search/cursor/project-prefix schema was exposed; temporary token was revoked and production instance identity remained unchanged.

These receipts must remain labeled coordinator-verified external evidence. They do not become scoped-worker test execution, and they do not certify production deployment.

## Focused assessment

### Regex safety, cost, and timeout

**Verified from scoped source:**

- Regex values are bound parameters to PostgreSQL `~*`; the inspected benchmark does not interpolate user patterns into SQL.
- The benchmark sets transaction-local `statement_timeout` to `250ms` and wraps the query future in a two-second outer guard.
- The pathological pattern `^(a+)+b$` is included as a non-indexable case.
- Invalid regex `SQLSTATE 2201B` handling is required by the contract but is not demonstrated by the scoped benchmark source or the supplied receipts.

**Coordinator-verified external evidence:** non-indexable regex planning was recorded, but actual cancellation or successful completion under the 250 ms bound is not present in the supplied receipt set.

**Remaining affected-only gap:** regex totals-query cancellation mapping for `SQLSTATE 57014` is not proven. Route this only to `crewsync:task-search-regex-total-timeout-r1`; do not repair source or migrations from this documentation scope.

### Index correctness and cost

The previous index concern is corrected by the coordinator receipts. The isolated clone evidence demonstrates:

- 9/9 language/field FTS plans used the expected language-specific FTS GIN indexes, with 0.12–19.9 ms observed execution.
- 6/6 positive contains/regex field plans used the expected per-field trigram indexes, with 0.58–5.83 ms observed execution.
- A non-indexable regex plan was captured without claiming universal trigram coverage.

The scoped migration reader alone did not expose the full candidate index set, so the positive index conclusion is based only on the explicitly labeled coordinator receipt, not on inference from `migrations/0020_task_search.sql`. No global latency or production performance claim is made.

### Tenant, pagination, cursor, and CLI safety

The coordinator-verified MCP and image-canary receipts provide supporting evidence for the actual candidate path:

- MCP E2E: 14/14 PASS, plus supporting 20/20 PASS.
- Canonical library regression: 105/105 PASS.
- Staged candidate image: 4,053 unique tasks, zero duplicates, `has_more=false`, `exhausted=true`.
- Search, cursor, and `project_prefix` fields were exposed; rollback preserved the legacy 50-task page.
- Temporary test token was revoked; the production instance identity remained unchanged.

These are coordinator-verified external receipts, not independent acceptance. The remaining regex totals timeout mapping prevents a release-green conclusion.

## Case matrix

| Case | Status | Exact finding |
|---|---|---|
| Case-insensitive regex | `COORDINATOR_VERIFIED_EXTERNAL_RECEIPT` | Positive regex plans passed for title/description/both; no production release claim. |
| Invalid regex / SQLSTATE `2201B` | `RUNTIME-PENDING` | No supplied receipt demonstrates invalid-pattern mapping. |
| Pathological regex bounded time | `REPAIR_REQUIRED / RUNTIME-PENDING` | Non-indexable EXPLAIN exists; actual 250 ms cancellation/success and totals mapping remain unconfirmed. |
| Non-indexable regex | `COORDINATOR_VERIFIED_EXTERNAL_RECEIPT` | Non-indexable plan was recorded without claiming trigram coverage. |
| Positive trigram index use | `COORDINATOR_VERIFIED_EXTERNAL_RECEIPT` | 6/6 plans used exact per-field trigram indexes, 0.58–5.83 ms. |
| FTS index correctness | `COORDINATOR_VERIFIED_EXTERNAL_RECEIPT` | 9/9 language/field plans used expected FTS indexes, 0.12–19.9 ms. |
| MCP E2E | `COORDINATOR_VERIFIED_EXTERNAL_RECEIPT` | 14/14 PASS plus supporting 20/20 PASS. |
| Canonical regression/build | `COORDINATOR_VERIFIED_EXTERNAL_RECEIPT` | 105/105 library PASS and `cargo check --tests` PASS. |
| CLI traversal and rollback | `COORDINATOR_VERIFIED_EXTERNAL_RECEIPT` | Candidate: 4,053 unique, zero duplicates, exhausted; rollback: legacy 50 page. |
| Search cursor/schema | `COORDINATOR_VERIFIED_EXTERNAL_RECEIPT` | Search, cursor, and `project_prefix` exposed by staged candidate image. |
| Regex totals-query `57014` mapping | `REPAIR_REQUIRED` | Affected-only repair task `crewsync:task-search-regex-total-timeout-r1` remains open for independent assessment. |
| Production release | `NOT ACCEPTED` | Do not infer release green from isolated clone, image, or coordinator receipts. |

## Scoped test execution

The focused scoped call was made with `run_node_tests` for `tests/task_search_performance.rs`. It returned `GO_WAVE_NODE_ERROR:test_file_not_allowed` because the file is Rust rather than an allowed Node test file. This is an interface limitation, not a Rust test result and not evidence that coordinator tests were absent.

No source, migration, or test repair was authorized in this attempt. The only changed file is this document.

## Affected-only repair routing

| Finding | Classification | Required action |
|---|---|---|
| Regex totals-query `SQLSTATE 57014` mapping remains unproven | `REPAIR_REQUIRED` | Assess only the registered `crewsync:task-search-regex-total-timeout-r1` code/test receipt; verify valid, invalid, pathological, and totals-query behavior before any closure recommendation. |
| Invalid regex `2201B` receipt absent | `RUNTIME-PENDING` | Coordinator/original regex owner supplies the actual request-path receipt; do not infer from EXPLAIN or harness source. |
| 250 ms timeout path absent | `RUNTIME-PENDING` | Coordinator/original regex owner supplies actual cancellation or bounded-success evidence, including totals query. |
| Scoped Node runner cannot run Rust | `BLOCKED_INFRA` | Preserve the coordinator-verified external receipt label and use a Rust-capable coordinator runner for any new execution. |

No source repair is authorized here. Do not modify `migrations/0020_task_search.sql`, `tests/task_search_performance.rs`, product source, task state, locks, Ruflo state, or Native state. Do not self-accept or self-complete any task.

## Acceptance gate

Independent acceptance remains blocked until the affected regex-total-timeout repair is assessed with an exact code/test receipt covering:

1. Valid case-insensitive regex through the production request path.
2. Invalid regex with safe `SQLSTATE 2201B` handling.
3. Pathological/non-indexable regex bounded by the intended 250 ms statement timeout or an explicitly safe bounded-success path.
4. Totals-query `SQLSTATE 57014` mapping with no leak, panic, or unbounded retry.
5. Existing coordinator-verified FTS, trigram, MCP, regression, build, and image-canary receipts retained as external evidence.

Until that affected-only repair is independently assessed, the correct worker result is **`CANDIDATE` / `UNACCEPTED`**, not production release acceptance.
