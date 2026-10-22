# Public Task Search API Verification

**Task key:** `crewsync:task-search-public-api-safety-r1`  
**Project:** `crewsync`  
**Attempt:** `attempt-d08d46dd-68fa-481e-8f79-57396f1c985e`  
**Session:** `manual-public-api-safety-r1-task-search-public-api-safety-r1`  
**Review date:** 2026-10-09  
**Producer role:** `implementer`  
**Producer thread:** `null`  
**Status:** `CANDIDATE / UNACCEPTED`  
**Changed files:** `tests/task_search_public_api.rs`, `docs/acceptance/TASK_SEARCH_PUBLIC_API_VERIFICATION.md`

This artifact records bounded implementation evidence only. It does not claim `DONE`, `ACCEPTED`, `ACCEPTED_GREEN`, deployment, release readiness, or a CrewSync state transition. No product source, migration, existing test, Ruflo state, Native state, claim, or lock was modified.

## Implementation delta

Added `tests/task_search_public_api.rs`, an ignored disposable-PostgreSQL integration test using the existing safety-gated fixture. The test exercises the public `list_tasks_page_with_search` store API rather than private SQL helpers and keeps cleanup outside the assertion body so fixture rows are removed after both pass and failure paths.

The matrix covers:

- Title-only, description-only, both-field contains/keywords/regex searches, English and Russian language selection, quoted/literal content, case-insensitive regex, and escaped `%`/`_` literals.
- Invalid regex input, unknown mode/field/language, NUL regex input, and a 513-byte query; all must be returned as safe `invalid input` errors.
- A long pathological regex against a disposable sentinel row with an outer 750ms guard; a store timeout must be classified with the repaired `250ms` message, while a fast successful bounded page is also accepted.
- Team ACL isolation through a task in a separate team, plus status, `mine_only`, and project-prefix filtering after a real claim.
- Signed cursor replay rejection when query, mode, field, or language changes; page-plus-one continuation with duplicate detection; and insertion between pages to document live-inventory/non-snapshot cursor semantics.
- Empty/whitespace search equivalence with `list_tasks_page`, including task order and team-wide open/claimed totals.

The fixture uses `LARGE_FIXTURE_SIZE` (2,000 rows), with deterministic timestamps and keys where continuation behavior is asserted. Assertions avoid planner-dependent EXPLAIN details and do not require a relevance ranking order.

## BMad checks applied

- **R2 / adversarial review:** malformed values, NULs, oversized input, invalid regex, cross-team leakage, cursor filter confusion, and cleanup-on-error paths are explicit negative cases.
- **R4 / runtime and rollback:** the regex request is bounded externally; repaired timeout text is checked at the public store error boundary; a subsequent healthy query is part of the same fixture run.
- **R8 / public boundary:** cases are sent to the exported task-list search API with real PostgreSQL rows rather than testing only normalization or SQL-string construction.
- **Quick Dev:** the test is additive, isolated, ignored by default, and reuses the existing disposable fixture contract without source or migration changes.
- **Edge Case Hunter:** equal timestamps, key tie-breaking, live insertion after a cursor, wildcard escaping, team ACL, and no-search compatibility are represented.

## Focused validation

- `run_node_tests("tests/task_search_public_api.rs")` returned `GO_WAVE_NODE_ERROR:test_file_not_allowed`. The repository is Rust-only and the scoped runner only accepts Node test files; this is a runner limitation, not a Rust pass/fail result.
- Rust compilation, formatting, and disposable PostgreSQL execution were not available through the permitted scoped MCP tools. The test remains `#[ignore]` and requires `CREWSYNC_ENABLE_LARGE_FIXTURE=1` plus a safe loopback `TEST_DATABASE_URL`, matching the existing fixture gate.
- No source repair was made in this scope. The test specifically verifies the existing repaired timeout mapping (`SQLSTATE 57014` when surfaced as a statement timeout) and malformed regex mapping (`SQLSTATE 2201B`) through the public store result.

## Acceptance matrix

| Required evidence | Test coverage | Independent status |
|---|---|---|
| Real isolated PostgreSQL positive/negative public API scenarios | `public_task_search_api_safety_matrix` with 2,000-row disposable fixture | **POSTGRES RUN PENDING** |
| Regex malformed input and safe `2201B` classification | Invalid `[` regex requires `invalid input` | **POSTGRES RUN PENDING** |
| Pathological regex bounded at 250ms or bounded completion | Long sentinel plus 750ms outer guard; repaired timeout text assertion | **POSTGRES RUN PENDING** |
| Team ACL and status/mine/project filters | Foreign team row plus claimed primary row | **POSTGRES RUN PENDING** |
| Invalid mode/fields/language, NUL, 512-byte limit | Six negative cases including 513 bytes | **POSTGRES RUN PENDING** |
| Cursor query/mode/field/language binding | Four altered cursor contexts rejected | **POSTGRES RUN PENDING** |
| Non-snapshot cursor semantics and no duplicates | Insert between pages and two-page duplicate check | **POSTGRES RUN PENDING** |
| Old no-search contract | Searched API with whitespace compared to legacy `list_tasks_page` | **POSTGRES RUN PENDING** |
| Rust-focused compile/test evidence | New ignored integration test | **TOOLING PENDING** |
| Independent QA and coordinator readback | Outside worker scope | **PENDING** |

## Candidate decision

**CANDIDATE / UNACCEPTED.** The implementation is complete within the exact write scope and is ready for independent Rust/PostgreSQL execution. Acceptance remains pending because the scoped environment supplied no Rust test runner and the mandated Node runner rejects Rust test files. The coordinator and independent reviewer must preserve the raw Rust/PostgreSQL receipt, verify the timeout/error classifications, and perform the separate CrewSync readback.
