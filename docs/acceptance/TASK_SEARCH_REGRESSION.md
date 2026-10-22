# Task Search Regression — Acceptance Evidence

**Task key:** `crewsync:task-search-regression-r1`  
**Project:** `crewsync`  
**Attempt:** `attempt-66217e51-60ec-4ffc-a427-85cc8057d372`  
**Session:** `manual-search-regression-r1-task-search-regression-r1`  
**Producer role:** implementer  
**Producer thread:** `null` (trusted binding is pending)  
**Review status:** `CANDIDATE / UNACCEPTED`  
**Review date:** 2026-10-09  
**Changed files:** `docs/acceptance/TASK_SEARCH_REGRESSION.md` only

This artifact is acceptance evidence, not task administration. It does not claim completion, `ACCEPTED`, `ACCEPTED_GREEN`, deployment, or release readiness. No product source, test source, migration, task state, lock, Ruflo state, or Native state was modified.

## Scope and method

The owned deliverable is acceptance documentation for the title/description task-search regression. The review combines three lenses:

- **BMad Help:** trace the frozen contract from specification to store, cursor, MCP, migration, and E2E assertions.
- **Quick Dev:** map focused unit and integration checks to the acceptance cases without changing product code.
- **Adversarial Review / Edge Case Hunter:** look for false-green paths, unsafe regex behavior, cursor-filter confusion, ACL leakage, pagination truncation, and evidence gaps.

The source of truth is the visible project tree. Coordinator or CrewSync receipts not present in the scoped reader are not treated as independently verified.

## Evidence read directly

- `_bmad-output/implementation-artifacts/spec-task-search.md`: frozen V1 contract, execution gates, mandatory acceptance matrix, cursor barrier, compatibility barrier, and release distinctions.
- `src/store/tasks.rs`: search normalization, mode/field/language validation, literal wildcard escaping, search cursor binding, bounded limit, and focused unit tests.
- `src/store/task_cursor.rs`: normalized search filters, signed cursor context, replay mismatch rejection, lifetime, and rank/time/key ordering tests.
- `src/tools/tasks.rs`: additive MCP list arguments, defaults, validation, legacy request compatibility, and store-query mapping tests.
- `tests/task_search.rs`: ignored disposable PostgreSQL E2E covering 2,000-task seeding, title/body search, language modes, phrase search, literal `%`/`_`, regex, invalid input, pathological regex timeout, ACL fixture, cursor replay, status/mine filters, empty-search compatibility, and real client calls.
- `migrations/0020_task_search.sql`: additive `pg_trgm` and weighted simple/English/Russian GIN indexes plus tenant-aware EXPLAIN guidance.
- `Cargo.toml`: Rust package and test-toolchain context.

## Regression matrix

| Case | Visible implementation/evidence | Independent status | Finding / repair recommendation |
|---|---|---|---|
| Absent or whitespace-only search preserves legacy path | `normalize_search` returns no search; E2E compares empty-search page with default page | **SOURCE-VERIFIED; RUNTIME-PENDING** | Preserve the no-search branch and old totals/order. Run the ignored fixture test before acceptance. |
| Title-only, description-only, both | Store query helpers and E2E calls select `title`, `description`, and `both` | **SOURCE-VERIFIED; RUNTIME-PENDING** | Require assertions that every returned row matches the selected field, not merely that the page is nonempty. |
| Keywords, quoted phrase, English/Russian | Fixed option validation; E2E exercises phrase, English stemming, and Cyrillic search | **SOURCE-VERIFIED; RUNTIME-PENDING** | Verify the actual disposable PostgreSQL dictionaries and record the database version/configuration with the receipt. |
| Literal contains `%`, `_`, and backslash | `escape_contains` and `ILIKE ... ESCAPE chr(92)` unit assertions; E2E uses `100%_wildcard` | **SOURCE-VERIFIED; RUNTIME-PENDING** | Keep bind parameters and escaping; do not replace with interpolated SQL. |
| Case-insensitive regex | E2E invokes regex mode with uppercase `DEPLOYMENT`; normalization rejects NUL/oversized input | **SOURCE-VERIFIED; RUNTIME-PENDING** | Confirm database regex errors are surfaced as safe input errors and not generic broadening. |
| Invalid/pathological regex and no-indexable pattern | E2E includes `[` and `^(a+)+$` under a two-second timeout; migration documents trigram limitations | **SOURCE-VERIFIED; RUNTIME-PENDING** | Execute against disposable PostgreSQL and capture elapsed time plus error classification. Do not infer safety from a unit-only SQL-string check. |
| Empty, oversized, and unknown options | Store and MCP tests reject unknown options for nonempty search; 513-byte input is rejected | **SOURCE-VERIFIED** | Retain the distinction that malformed nonempty options reject; whitespace search remains legacy-compatible. |
| Search plus status/mine/project prefix/team ACL | E2E creates a foreign team, claims one task, and queries status/mine/project prefix | **SOURCE-VERIFIED; RUNTIME-PENDING** | Require an explicit assertion that the foreign task is absent and that totals remain team-wide, not hit counts. |
| Signed cursor binds query, mode, fields, language, team, and filters | Cursor normalization and replay tests cover altered query/mode/fields/language, team, status, and signature | **SOURCE-VERIFIED** | Independent API-level replay receipt remains required; source unit coverage alone is not acceptance. |
| Stable rank/time/key continuation and no duplicates | `TaskOrder` tests cover rank, descending timestamp, and key tie-break; E2E traverses 2,000 rows with a set | **SOURCE-VERIFIED; RUNTIME-PENDING** | Record page count, unique key count, `has_more`, and terminal `next_cursor` in the independent receipt. |
| Page limit +1 and numeric totals | Spec requires bounded pages and team-wide open/claimed totals; E2E asserts totals for the title-only page | **CONTRACT-VERIFIED; RUNTIME-PENDING** | Confirm totals do not become search-hit totals and that no arbitrary per-page limit truncates candidate search. |
| Old MCP request and CLI path | MCP legacy deserialization test retains defaults; E2E invokes the real client for old and searched requests | **SOURCE-VERIFIED; RUNTIME-PENDING** | Run the real client path and preserve JSON termination metadata; do not claim CLI compatibility from deserialization alone. |
| Read-only canary and concurrency/lease unaffected | Spec requires both; visible task-search E2E covers a claim but no read-only canary or concurrency stress | **PENDING** | Release/reducer evidence must run a read-only database canary and a focused lease/concurrency regression; no repair is requested in this docs-only scope. |
| Indexes and EXPLAIN | Migration is additive/idempotent and defines trigram plus weighted FTS indexes with benchmark guidance | **SOURCE-VERIFIED; BENCHMARK-PENDING** | Run `ANALYZE` and EXPLAIN on the disposable 2,000-row fixture; capture plans/timings for contains, regex, and each language. |

## Failure matrix

| Failure mode | Detection | Severity | Scoped action |
|---|---|---:|---|
| Search fields/options silently broaden | Normalization and MCP validation tests; altered cursor options | Blocker | Keep rejection behavior; independent API receipt required. |
| `%`, `_`, or `\\` treated as wildcards | `escape_contains` unit test plus literal fixture row | High | Preserve escaping; verify with disposable PostgreSQL. |
| Regex error or pathological pattern escapes bounds | Invalid regex and two-second timeout case | Blocker | Run the ignored E2E; record SQLSTATE/error and elapsed time. |
| Cross-team task leaks into search | Foreign-team fixture and team-bound query | Blocker | Require an explicit zero-leak assertion in independent evidence. |
| Cursor replay under changed search context | Four altered search cursor cases and signed context | Blocker | Verify through the API boundary, not only direct store helpers. |
| Duplicate/omitted rows across pages | 2,000-task `HashSet`, expected ten pages at limit 200 | Blocker | Record exact unique count and terminal cursor state. |
| Totals report hits instead of team inventory | Spec and E2E total assertions | High | Preserve team-wide totals and document them in release evidence. |
| Search applied after arbitrary page truncation | Store contract and traversal requirement | Blocker | Inspect query plan/SQL receipt and complete traversal before acceptance. |
| Missing search indexes or poor plan | Migration plus EXPLAIN guidance | High | Benchmark disposable PostgreSQL; no production benchmark is authorized here. |
| Full Rust/toolchain evidence unavailable | Required cargo/fmt checks are not exposed by the scoped MCP toolset | High | Coordinator must run `cargo test --all-targets`, `cargo check --tests`, and `cargo fmt -- --check` with the available Rust toolchain. |

## Focused checks and limitations

The visible source contains focused Rust unit tests in the store, cursor, and MCP modules, and the 2,000-row integration test is explicitly marked `#[ignore]` with requirements `CREWSYNC_ENABLE_LARGE_FIXTURE=1` and a disposable `TEST_DATABASE_URL`. No test result is claimed here because this worker was restricted to the scoped Go Wave project tools: those tools expose direct Node test execution but do not expose a Cargo runner, and the project tree contains no JavaScript test file applicable to this Rust task. Running an unrelated Rust file through Node would not be meaningful evidence.

Therefore:

- **Directly verified:** source-level contract coverage, normalization/escaping rules, cursor binding logic, MCP compatibility mapping, additive migration shape, and the presence/content of the required E2E assertions.
- **Not independently verified:** PostgreSQL execution, real HTTP/MCP client behavior, EXPLAIN plans/timings, regex timeout behavior against the database, full Rust tests, `cargo check --tests`, and `cargo fmt -- --check`.
- **Not claimed:** `ACCEPTED_GREEN`, task completion, deployment, production canary, or release readiness.

## Affected-only repair recommendations

1. Run the ignored disposable PostgreSQL E2E and attach raw output containing fixture size, database version, all mode assertions, ACL result, cursor rejection result, unique traversal count, page count, and client termination metadata.
2. Run the independent full Rust checks: `cargo test --all-targets`, `cargo check --tests`, and `cargo fmt -- --check`; classify unrelated failures without editing outside this write scope.
3. Capture `EXPLAIN (ANALYZE, BUFFERS)` for contains, regex, and simple/English/Russian keyword paths after `ANALYZE`; retain plans and timings as release evidence.
4. Add or preserve explicit read-only-canary and lease/concurrency evidence in the release reducer; this document does not authorize product-source repairs.
5. Reconcile any failure only in its owning task/files. This task owns documentation only, so no source repair is requested.

## Candidate decision

**CANDIDATE / UNACCEPTED.** The implementation and tests visibly address the frozen contract, but independent runtime and toolchain evidence is pending. The coordinator/reviewer must not convert this report to `ACCEPTED` or `ACCEPTED_GREEN` without the missing PostgreSQL, client, EXPLAIN, full Rust, formatting, and canary evidence.

This document is the sole changed file and remains inside the exact write scope: `docs/acceptance/TASK_SEARCH_REGRESSION.md`.
