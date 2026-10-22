# Task Search Review — BMad Adversarial and Edge-Case Evidence

**Task key:** `crewsync:task-search-review-r1`  
**Project:** `crewsync`  
**Attempt:** `attempt-ff1679d3-d433-4d2a-8ffa-c1c0e22f5e9a`  
**Session:** `manual-search-review-r1-task-search-review-r1`  
**Review date:** 2026-10-09  
**Producer role:** `implementer`  
**Producer thread:** `null`  
**Review status:** `CANDIDATE / UNACCEPTED`  
**Changed files:** `docs/acceptance/TASK_SEARCH_REVIEW.md` only

This is an independent acceptance artifact, not task administration. It does not claim `DONE`, `ACCEPTED`, `ACCEPTED_GREEN`, deployment, release readiness, or a CrewSync state transition. No product source, migration, test source, task state, lock, Ruflo state, or Native state was modified.

## Review basis

The source of truth is `_bmad-output/implementation-artifacts/spec-task-search.md`, with pagination compatibility checked against `_bmad-output/implementation-artifacts/spec-task-list-pagination.md`. Directly inspected evidence included:

- `src/store/tasks.rs`: normalization, SQL predicates, keyset continuation, totals, timeout transaction, and regex error mapping.
- `src/store/task_cursor.rs`: signed context, search option canonicalization, expiry, and rank/time/key order.
- `src/tools/tasks.rs`: additive MCP schema, defaults, validation, and store mapping.
- `migrations/0020_task_search.sql`: trigram and language-specific weighted FTS indexes.
- `tests/task_search.rs`: ignored disposable PostgreSQL fixture and client-level scenarios.
- `tests/task_search_performance.rs`: EXPLAIN and timeout benchmark shape.
- `Cargo.toml` and `README.md`: Rust-only project and task-list compatibility context.

The permitted reader did not expose a coordinator-owned CrewSync claim/readback or the raw PostgreSQL receipt. The BMad checkpoint text reporting a latest `2/2 PASS` run is therefore retained as a lead, not treated as independent evidence.

## Verdict

**CANDIDATE / UNACCEPTED; REPAIR_REQUIRED for performance/evidence reconciliation.**

The implementation has credible source-level coverage for ACL binding, literal escaping, signed search-filter cursors, page-plus-one traversal, stable rank/time/key ordering, bounded input length, and invalid-regex SQLSTATE `2201B` mapping. Independent acceptance is not justified because the required real PostgreSQL run, EXPLAIN output, full Rust checks, read-only canary, and concurrency/lease evidence are not available through the scoped tools. In addition, the production contains/regex predicates do not match the concatenated trigram expression used by the migration and benchmark guidance, so the performance claim is not yet demonstrated for the actual query.

## Adversarial findings

| ID | Severity | Finding | Evidence | Affected-only action |
|---|---|---|---|---|
| R1 | **Blocker** | The production contains and regex branches search `t.title` and `coalesce(t.description, '')` independently, including an `OR` for `both`. Migration `tasks_search_trgm_idx` indexes one concatenated expression. An index plan for the concatenated benchmark query does not prove that the production predicates use the index. | `src/store/tasks.rs` search clauses; `migrations/0020_task_search.sql`; `tests/task_search_performance.rs` uses concatenation for `contains_sql` and `regex_sql`. | SQL/index owner must run EXPLAIN against the exact production-shaped SQL for title, description, and both. If the required bound plan is absent, repair only the owning migration/query/test scope. |
| R2 | **Blocker** | Regex timeout is bounded but not classified as a safe input result. The store maps invalid PostgreSQL regex SQLSTATE `2201B`, but a pathological execution timeout (`57014`) falls through to the generic database error path. The frozen contract requires bounded execution and safe handling; no receipt proves the client-facing behavior on timeout. | `src/store/tasks.rs` maps only `2201B`; `tests/task_search_performance.rs` expects `57014` in a lower-level query but does not exercise the public store/API error mapping. | Reproduce the pathological pattern through the public store and MCP/client boundary. Preserve the timeout bound and classify the actual PostgreSQL code before making an affected-only repair; do not add speculative SQLSTATE handling. |
| R3 | **High** | The MCP `list_tasks` tool always calls `list_tasks_page_with_search`, even when `search` is absent or whitespace. The function currently produces the same logical rows through a null-search predicate, but it is not the unchanged legacy path: it opens a transaction, sets a five-second local timeout, and uses the new search SQL branch. | `src/tools/tasks.rs` `list_tasks`; `src/store/tasks.rs` `list_tasks_page` versus `list_tasks_page_with_search`; spec requires absent search to preserve old semantics. | Run old MCP/CLI requests against the real server and compare ordering, totals, cursor behavior, timeout/error behavior, and lease/concurrency side effects. If a regression is reproduced, repair the owning compatibility path only. |
| R4 | **High** | Search and totals execute in one transaction after setting the search timeout. A timeout or other statement error aborts the transaction; the code returns immediately for the row query, while the successful path performs team-wide totals in that same transaction. This is probably safe, but timeout classification and rollback behavior are unverified at the API boundary. | `src/store/tasks.rs` `list_tasks_page_with_search`; benchmark only tests a lower-level SQL statement. | Capture a disposable receipt for normal search, invalid regex, pathological regex, and a subsequent healthy request on the same pool/session. Confirm no poisoned connection or misleading totals. |
| R5 | **High** | The mandatory runtime acceptance matrix is incomplete in available evidence: no raw 2,000-row receipt, database version, timings, exact page count/unique count, terminal cursor, foreign-team zero-leak output, real MCP/CLI termination metadata, read-only canary, or concurrent lease evidence is accessible. | `tests/task_search.rs` is `#[ignore]` and requires `CREWSYNC_ENABLE_LARGE_FIXTURE=1` plus disposable `TEST_DATABASE_URL`; no runtime receipt is in the scoped project files. | E2E/reducer owner must attach raw disposable PostgreSQL and real-client receipts. Do not promote the textual BMad `2/2 PASS` checkpoint without the receipt. |
| R6 | **High** | The benchmark's non-indexable regex assertion is useful only for its concatenated expression, not the production `title OR description` expression. It can therefore produce a false-green performance result while the API scans both columns. | `tests/task_search_performance.rs` `regex_sql`; `src/store/tasks.rs` regex branches. | Add or run exact-query EXPLAIN evidence in the owning benchmark scope, including the pathological pattern and its elapsed/SQLSTATE result. |
| R7 | **Medium** | FTS uses a weighted title+description vector and then adds a second field-specific `to_tsvector` predicate for `title` or `description`. This is semantically conservative, but field-specific index use and title-weight ranking are not shown by the available plans. | `src/store/tasks.rs` `keyword_search_clause`; migration has three weighted expression indexes; no EXPLAIN output is available. | Capture simple, English, and Russian plans for `title`, `description`, and `both`; verify no language silently falls back and that ranking does not alter stable cursor order. |
| R8 | **Medium** | The contract requires malformed/unknown nonempty options to reject without broadening. Source tests cover normalization and MCP validation, but no real API receipt demonstrates invalid mode/field/language, oversized input, invalid regex, and NUL rejection at the public boundary. | `src/tools/tasks.rs` validation; `src/store/tasks.rs` normalization; ignored E2E cases. | Preserve source validation and record public MCP/client error responses in the E2E receipt. |

## False positives explicitly cleared

- **Cursor context omission for empty search is intentional.** `normalize_search` turns missing/whitespace search into the legacy no-search state; `TaskCursorFilters::normalized` rejects dangling search options rather than silently broadening. Nonempty cursors bind query, mode, fields, language, team, project prefix, status, and mine flag.
- **Team-wide totals are not a search bug.** Both searched totals bind `auth.team_id` and intentionally count open/claimed tasks for the whole team, as required by the frozen contract; they must not be compared with hit counts.
- **Search is applied before pagination truncation.** The searched SQL places the predicate before `ORDER BY` and `LIMIT`, fetches `limit + 1`, then truncates and emits a cursor from the final returned row.
- **Literal `%`, `_`, and backslash handling is source-consistent.** `escape_contains` escapes all three and the SQL uses a bound parameter with `ESCAPE chr(92)`; no interpolated user regex or value is visible.
- **Regex language injection is not visible.** The language/mode/field choices are fixed whitelists before SQL construction; the regex itself is bound as `$7`.
- **Stable tie breaking is present.** Continuation compares status rank, descending `updated_at`, then ascending task key, matching the cursor order structure. Mutable-inventory best-effort consistency remains a runtime property, not a snapshot guarantee.
- **No second search inventory was introduced.** The implementation extends task listing and retains the existing MCP/CLI inventory concept.

## Acceptance matrix

| Required case | Source status | Independent status |
|---|---|---|
| Absent/whitespace search and old MCP/CLI request | Legacy path and null-search path both exist; MCP routes through the new function | **RUNTIME PENDING** |
| Title-only, description-only, both | Normalized fields and predicates present; fixture calls are present | **RUNTIME PENDING** |
| Keywords, quoted phrase, English/Russian | Fixed language whitelist, websearch parser, weighted vectors present | **RUNTIME PENDING** |
| Literal `%`, `_`, backslash | Escaper and bound ILIKE are present | **RUNTIME PENDING** |
| Case-insensitive, invalid, pathological regex | `~*`, `2201B` mapping, 250ms local timeout, and tests are present | **RUNTIME/API PENDING** |
| Empty/oversized/unknown input | 512-byte limit and option rejection are present | **API RECEIPT PENDING** |
| Search + status/mine/project prefix/team ACL | All predicates bind team and filters | **RUNTIME PENDING** |
| Cursor replay after query/mode/field/language change | Signed context and source tests are present | **API RECEIPT PENDING** |
| 2,000-row traversal, no duplicates, `has_more`, terminal cursor | HashSet/page loop is present in ignored E2E | **POSTGRES RECEIPT PENDING** |
| Equal timestamp/key continuation | SQL and cursor tie breaker are present | **RUNTIME PENDING** |
| Read-only canary and concurrency/lease unaffected | No direct evidence in scoped search receipt | **PENDING** |
| Exact production EXPLAIN/timing and non-indexable regex | Benchmark shape exists but uses different contains/regex SQL | **REPAIR + BENCHMARK PENDING** |

## Focused validation

- **Node test attempt:** `run_node_tests` on `tests/task_search.rs` returned `GO_WAVE_NODE_ERROR:test_file_not_allowed`. The project is Rust-only (`Cargo.toml` has no JavaScript test target), so this is a runner/tooling limitation, not a Rust test result.
- **Static review:** completed against the listed source, migration, test, spec, and compatibility files. No out-of-scope write was made.
- **Rust/PostgreSQL execution:** not available through the permitted scoped tools; no success or failure is claimed.
- **CrewSync readback:** not performed; worker does not call CrewSync administration or alter claims/locks. Coordinator-owned readback remains required.

## Affected-only repair plan

1. SQL/index owner: reproduce exact production-shaped EXPLAIN plans and timings for all modes, fields, and languages; repair only source/migration/test files in that owner’s manifest if a defect is confirmed.
2. E2E owner: run the ignored disposable PostgreSQL fixture and real client boundary, preserving fixture size, database version, page/unique counts, cursor mismatch errors, ACL result, totals, regex SQLSTATE, timeout, and termination metadata.
3. Compatibility/reducer owner: run the legacy no-search path, read-only canary, and concurrent claim/lease checks; classify any regression before editing.
4. Coordinator: perform independent CrewSync readback and keep this artifact unaccepted until the raw receipts reconcile with the BMad DoD.

## Candidate decision

**CANDIDATE / UNACCEPTED.** The source supports several required safety properties, but the actual production query-plan mismatch and missing independent runtime receipts prevent acceptance. No implementation repair is made in this review scope. The sole changed file is `docs/acceptance/TASK_SEARCH_REVIEW.md`, within the exact manifest write scope.
