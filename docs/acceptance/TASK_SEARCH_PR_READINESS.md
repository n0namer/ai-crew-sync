# Task Search — PR Readiness Review

**Review date:** 2026-10-09  
**Task:** `crewsync:task-search-pr-readiness-r1`  
**Project:** `crewsync`  
**Attempt:** `attempt-30234490-afe3-419f-ae3b-d280766d2bd0`  
**Session:** `manual-search-pr-readiness-r1-task-search-pr-readiness-r1`  
**Review status:** CANDIDATE / UNACCEPTED  
**Scope:** BMad Help + Quick Dev + adversarial review/edge-case audit of the visible task-search implementation. Only this report is owned and changed; no product source, migration, task state, lock, Ruflo state, Native state, or Git state was modified.

## Review basis

The visible contract in `_bmad-output/implementation-artifacts/spec-task-search.md` requires additive title/description search, three explicit modes, fixed language choices, escaped literal contains, PostgreSQL regex safety, team/filter-bound signed cursors, stable rank/time/key paging, additive indexes, 2,000-row traversal, and deterministic acceptance evidence. The scoped source files were read directly:

- `src/store/tasks.rs`
- `src/store/task_cursor.rs`
- `src/tools/tasks.rs`
- `tests/task_search.rs`
- `migrations/0020_task_search.sql`
- `Cargo.toml`

GitHub draft PR 202 for `joaquinbejar/ai-crew-sync` could not be independently read through the available public page lookup during this attempt; no local Git command or Git operation was used. Therefore no claim is made here about the complete PR diff, review discussion, branch head, or draft status. Findings below are based on the scoped working-tree files and the visible contract only.

## BMad acceptance audit

**Implemented surface observed.** The MCP `list_tasks` request has optional `search`, `search_mode`, `search_fields`, and `search_language` fields with legacy defaults. It routes through `list_tasks_page_with_search`. Store normalization trims input, rejects unknown values and oversized queries, escapes `%`, `_`, and backslash for contains, rejects NUL regex input, maps SQLSTATE `2201B` to an invalid-input error, and applies a local `250ms` statement timeout for regex mode. Cursor context includes team, project prefix, status, mine flag, query, mode, fields, and language. Pagination fetches `limit + 1` and preserves status-rank/updated-at/key ordering. These are strong compatibility and injection-resistance indicators.

**Legacy compatibility observed.** `src/tools/tasks.rs` retains defaults for old list requests, and its unit test deserializes a request without new fields. Empty/whitespace search is normalized to the legacy no-search path. The new response is additive through `TaskPageList` mapping. This is source-level evidence, not a compiled or live MCP compatibility receipt.

**Security observations.** Values are bound rather than interpolated into search predicates. The regex path has a fixed local timeout and handles invalid PostgreSQL regex input. Team authorization remains an explicit `t.team_id = $1` predicate in both page and totals queries. Cursor replay is signed and context-bound in the visible cursor module tests. No credentials or secrets appear in the reviewed report or source excerpts.

## Findings

### F1 — RELEASE BLOCKER: FTS index readiness is not demonstrated and likely does not match the SQL expressions

`migrations/0020_task_search.sql` creates three fixed-language weighted expression indexes for the combined title-plus-description expression. `src/store/tasks.rs:501`–`503` constructs keyword predicates with `to_tsvector($8::regconfig, ...)`, where the language is a bound runtime parameter. Title-only and description-only paths use independent unweighted vectors at `src/store/tasks.rs:501`–`503`, while the migration has no corresponding single-field indexes. PostgreSQL functional-index matching is expression-specific; a parameterized regconfig and field-specific expressions are not the same expressions as the fixed combined indexes.

This does not prove incorrect search results, but it does mean the contract requirement for indexable keyword search is unverified and likely fails for at least title-only/description-only modes. The migration comments instruct reviewers to look for a GIN index scan, but no `EXPLAIN` receipt is included in the scoped evidence. Repair or explicitly narrow the contract: use exact fixed-language SQL branches matching the indexes, add matching field-specific indexes, or document and prove an accepted bounded fallback. Capture `EXPLAIN (ANALYZE, BUFFERS)` on a disposable 2,000+ row fixture for all supported keyword languages and fields, plus contains/regex cases with and without trigrams.

### F2 — RELEASE BLOCKER: mandatory large E2E is ignored and has no executable receipt

`tests/task_search.rs` contains the required broad scenarios: title/body modes, wildcard escaping, quoted and multilingual data, invalid regex, pathological regex timeout, ACL, status/mine/project filters, cursor mismatch, duplicate detection, 2,000-row traversal, and real client calls. However, the only integration test is declared with `#[ignore = "requires CREWSYNC_ENABLE_LARGE_FIXTURE=1 and disposable TEST_DATABASE_URL"]`. No database fixture, run log, timing, EXPLAIN output, or independent pass receipt is present in the scoped files.

The source test is useful acceptance intent, but it cannot support a green PR-readiness claim by itself. Run it in an isolated disposable PostgreSQL environment, preserve the exact command/result and explain plans, and independently verify the client/MCP compatibility cases. Do not describe the ignored test as passed.

### F3 — EVIDENCE GAP: focused tests were not executable through the mandated Node runner

The available project test file is Rust (`tests/task_search.rs`), and the allowed validation tool is `run_node_tests`, which invokes Node directly. This attempt therefore has no successful Node test result for the Rust implementation. The scoped Rust unit tests visible in `src/store/tasks.rs`, `src/store/task_cursor.rs`, and `src/tools/tasks.rs` cover normalization, escaping, cursor binding, malformed options, and legacy deserialization, but they were read rather than executed because the task validation contract supplied no Rust runner. This is an evidence limitation, not a claimed product failure. A coordinator-level Rust validation gate remains necessary.

### F4 — CONTRACT CLARIFICATION: mutable inventory consistency is best-effort only

The cursor binds filters and the rank/time/key boundary, but there is no snapshot identifier. Inserts, status changes, lease expiry, or `updated_at` changes between pages can move rows across the boundary. The BMad contract mentions explicit best-effort consistency for a mutable inventory, but the visible task-search report does not state this behavior to callers. Add the consistency statement to the acceptance/release evidence and ensure mutation coverage does not promise point-in-time semantics.

### F5 — REVIEW LIMITATION: PR 202 and current commit parity are unverified

The requested GitHub draft PR 202 review could not be independently retrieved through the available web lookup, and Git operations are prohibited for this task. The reviewed source may therefore differ from the draft PR head or current commit. The coordinator must supply a trusted commit/diff receipt or independently compare the local working tree with PR 202 before treating this report as a complete PR review. This finding is intentionally not repaired here.

## Edge-case checklist

- Empty/missing search: source path preserves legacy behavior; unit coverage is visible.
- Unknown mode/field/language: rejected for non-empty search; dangling options are rejected by cursor normalization.
- Contains `%`, `_`, and backslash: escaped before bound `ILIKE`; source unit coverage is visible.
- Regex invalid syntax: SQLSTATE `2201B` is mapped to invalid input.
- Regex denial-of-service: fixed local `250ms` statement timeout and bounded page limit are present; live timeout receipt is absent.
- Search after arbitrary page limit: query applies search before `LIMIT limit + 1`.
- ACL/totals: team predicate is present; totals are team-wide status totals, not hit counts.
- Cursor replay: query/mode/fields/language/team/filter context is signed and tested in the cursor module.
- Tie ordering: status rank, descending `updated_at`, and ascending key are carried into the cursor.
- Migration safety: indexes are additive and `IF NOT EXISTS`; no historical migration is modified. Reversibility/down migration evidence is absent.
- Scope exclusions: reviewed search is limited to title/description; no notes, results, attachments, metadata, events, vector engine, or external search dependency was observed.

## Current gate decision

**CANDIDATE / UNACCEPTED.** The visible implementation is substantial and has credible source-level defenses for compatibility, parameter binding, cursor binding, ACL predicates, regex error handling, and page-plus-one traversal. It is not independently ready for `ACCEPTED_GREEN` because FTS index usage is not proven and likely mismatched for dynamic language/field branches, the mandatory 2,000-row E2E is ignored without a receipt, Rust tests were not executable through the mandated Node-only runner, and PR-202/local-commit parity is unverified.

No unverified `ACCEPTED_GREEN`, DONE, release, deployment, or production claim is made.

## Affected-only repair requests

- Search implementation/migration owner: reconcile fixed GIN expressions with runtime language and field branches; add disposable EXPLAIN evidence and bounded regex/contains performance evidence.
- E2E owner/reducer: run the ignored PostgreSQL test with the required fixture, preserve logs and timing, and independently verify old MCP/client requests.
- Release/reviewer: state best-effort mutable-inventory semantics and verify local commit parity with draft PR 202 using trusted GitHub/Git evidence.
- Coordinator: run Rust-focused tests/build checks in a suitable environment; do not treat the unavailable Node runner as product evidence.

## Evidence index

**Directly read through scoped project MCP:**

- `_bmad-output/implementation-artifacts/spec-task-search.md`: frozen contract, execution gates, mandatory acceptance, and E2E requirements.
- `src/store/tasks.rs`: normalization, SQL branches, statement timeout, cursor binding, pagination, totals, and unit tests.
- `src/store/task_cursor.rs`: signed cursor normalization and replay tests.
- `src/tools/tasks.rs`: additive MCP arguments, routing, validation, and legacy tests.
- `tests/task_search.rs`: ignored large-fixture E2E scenarios and client coverage.
- `migrations/0020_task_search.sql`: additive pg_trgm/FTS indexes and EXPLAIN guidance.
- `Cargo.toml`: Rust-only package/test context.

**Not independently verified:**

- GitHub draft PR 202 diff, branch head, review discussion, and current commit parity.
- PostgreSQL execution, EXPLAIN plans, timings, migration application, Rust test output, and live MCP/CLI behavior.
- Any coordinator or external acceptance receipt not present in the scoped files.

## Reviewer signatures

- **BMad Help / Quick Dev contract review:** additive API, defaults, filtering, search modes, cursor binding, and compatibility audited; source-level evidence only; status `CANDIDATE / UNACCEPTED`.
- **Adversarial Review / Edge Case Hunter:** FTS-index mismatch risk, ignored E2E, mutation semantics, unavailable Node validation, and PR parity gap recorded; status `CANDIDATE / UNACCEPTED`.

This document is evidence only. It does not administer tasks, alter claims or locks, accept work, or authorize release.
