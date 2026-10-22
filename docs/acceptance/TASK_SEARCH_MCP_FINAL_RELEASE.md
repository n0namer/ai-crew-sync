# Task Search MCP Final Release Reducer

**Outcome:** `CANDIDATE`
**Acceptance:** `UNACCEPTED`
**Reducer decision:** `REPAIR`
**Release decision:** `DO NOT RELEASE`
**Review date:** 2026-10-09
**Project:** `crewsync`
**Task key:** `crewsync:task-search-mcp-final-reducer-r1`
**Attempt:** `attempt-fe30abf5-5c2c-4b47-ab65-784e91349775`
**Session:** `manual-mcp-final-reducer-r1-task-search-mcp-final-reducer-r1`
**Producer role:** `implementer`
**Producer thread:** `null`
**Changed files:** `docs/acceptance/TASK_SEARCH_MCP_FINAL_RELEASE.md` only

This is an evidence reducer, not task administration. It does not claim `DONE`, `ACCEPTED`, `ACCEPTED_GREEN`, merge authorization, deployment, or production readiness. No product code, migration, test, task state, claim, lock, Ruflo state, or Native state was changed.

## Authority and method

The authoritative behavior contract is `_bmad-output/implementation-artifacts/spec-task-search.md`, reconciled with the current Git/source snapshot and the scoped acceptance artifacts. The review applies BMad Help, Adversarial Review, and Edge Case Hunter principles:

- Keep the existing `list_tasks` inventory and legacy no-search semantics.
- Treat real MCP transport, PostgreSQL fixture, source identity, and independent readback as separate evidence requirements.
- Treat missing, stale, indirect, coordinator-only, or runner-incompatible evidence as `PENDING` or `BLOCKED`, never as false green.
- Review all six requested MCP/CLI E2E paths individually.
- Route only affected gaps to affected owners; do not authorize a product or release change from this document.

### Dependency reconciliation

The BMad execution DAG requires the search contract, SQL/store, index, MCP, CLI, E2E, adversarial review, and release reducer gates in order. The scoped readback contains candidate/unaccepted acceptance artifacts and coordinator-verified external receipts, but no worker-side CrewSync administration or terminal acceptance readback. Therefore dependency completion is **not independently accepted by this worker**. The coordinator must reconcile accepted task keys, source revision, and locks from actual CrewSync readback before any final decision.

The existing transport contract and release matrix both explicitly require independent QA and keep runtime rows pending until raw receipts are replayed. Their non-acceptance state is preserved here.

## Evidence ledger

### Coordinator-verified external receipts

These receipts were recorded by the existing adversarial review as independently read by the coordinator, but they were not directly accessible through this worker's scoped MCP read path. They are evidence references, not a substitute for final coordinator QA:

- `D:/Users/NIKITA/Documents/DEV/go-core/runtime/waves/20261009-crewsync-task-search/full-validation-results.json`: seven new Rust MCP modules reported `14/14 PASS` at Git source HEAD `45e33ed`; individual per-test logs were reported as present.
- `full-regression-results.json`: ten supporting MCP/CLI/PostgreSQL tests reported `20/20 PASS`; `cargo check --tests` reported `PASS`.
- `canonical-build-results.json`: canonical LF snapshot reported `cargo test --lib 105/105 PASS`.
- `cli-full-traversal-clone-result.json`: isolated restored clone reported 4,053 rows, 4,053 unique keys, zero duplicates, `exhausted=true`, `incomplete=false`, `has_more=false`, and `next_cursor=null`.
- `fallback-clone-acceptance.json`: fallback binary from Git `8897d7a` booted against the isolated migrated v21 clone, health was OK, and all 4,053 tasks were readable; the published 0.7.3 image failed on migrated v21 with SQLx `VersionMissing`.
- Source identity recorded by the adversarial review: Git HEAD `45e33ed`, snapshot SHA256 `13dd4a66669928e6a88a2f297b66712139bb7a454b5ffeb62c04746eab23dd97`.

These receipts must be replayed and source-bound by independent QA before any matrix row can become `PASS`.

### Worker-scoped validation

`Cargo.toml` identifies a Rust/Cargo project and provides no Node test suite. The mandated Node runner was attempted once for each requested Rust test path; every invocation returned the exact result `GO_WAVE_NODE_ERROR:test_file_not_allowed`:

| Requested path | Node runner result | Evidence classification |
|---|---|---|
| `tests/task_search_mcp_transport.rs` | `GO_WAVE_NODE_ERROR:test_file_not_allowed` | `BLOCKED_INFRA`; no local PASS claimed |
| `tests/task_search_mcp_security.rs` | `GO_WAVE_NODE_ERROR:test_file_not_allowed` | `BLOCKED_INFRA`; no local PASS claimed |
| `tests/task_search_mcp_pagination.rs` | `GO_WAVE_NODE_ERROR:test_file_not_allowed` | `BLOCKED_INFRA`; no local PASS claimed |
| `tests/task_search_mcp_schema.rs` | `GO_WAVE_NODE_ERROR:test_file_not_allowed` | `BLOCKED_INFRA`; no local PASS claimed |
| `tests/task_search_cli_transport.rs` | `GO_WAVE_NODE_ERROR:test_file_not_allowed` | `BLOCKED_INFRA`; no local PASS claimed |
| `tests/task_search_mcp_server_canary.rs` | `GO_WAVE_NODE_ERROR:test_file_not_allowed` | `BLOCKED_INFRA`; no local PASS claimed |

No Cargo or PostgreSQL execution was available in this worker scope. The Rust results above are retained only as coordinator-verified external receipts, not as a new local run.

## Six-path MCP E2E review

### 1. Transport — `tests/task_search_mcp_transport.rs`

The test is an ignored disposable-PostgreSQL Rust integration test. It starts an HTTP MCP router, authenticates a real Streamable HTTP client, checks `tools/list`, preserves an old no-search request, and exercises keyword title search, literal description contains search, regex description search, Russian keyword search, title-only search, and description-only search. The coordinator-verified aggregate receipt supports runtime coverage, but this worker has no raw per-test replay and the Node runner cannot execute Rust.

**Disposition:** `COORDINATOR_VERIFIED_EXTERNAL_RECEIPT`; independent wire readback remains pending.

### 2. Security — `tests/task_search_mcp_security.rs`

The test covers authenticated and foreign-team clients, invalid mode/field/language, invalid regex, oversized input, pathological regex bounded time, signed cursor replay with changed query/mode/field/language, and cross-tenant cursor rejection. The source is strong coverage evidence, but the exact raw error/status and timeout receipts remain coordinator QA responsibilities.

**Disposition:** `COORDINATOR_VERIFIED_EXTERNAL_RECEIPT`; negative-case replay and redaction verification remain pending.

### 3. Pagination — `tests/task_search_mcp_pagination.rs`

The test seeds `LARGE_FIXTURE_SIZE` tasks, traverses through real MCP cursors at a page limit of 200, checks uniqueness and expected keys, exercises concurrent inserts, legacy requests, claim/status filters, and search-bound cursor mismatch. The existing external clone receipt separately reports 4,053/4,053 unique exhausted traversal. The required exact 2,000-row raw page transcript, timings, and terminal cursor must still be independently read back.

**Disposition:** `COORDINATOR_VERIFIED_EXTERNAL_RECEIPT`; exact 2,000-row MCP receipt and plan/timing evidence remain pending.

### 4. Schema compatibility — `tests/task_search_mcp_schema.rs`

The test checks exactly one canonical `list_tasks` tool, additive optional search fields, old request payload compatibility, numeric open/claimed totals, and InvalidParams for unsupported search options. The source and external aggregate support the intended behavior; the raw `tools/list` and old/new `tools/call` transcript must still be replayed against the same source revision.

**Disposition:** `COORDINATOR_VERIFIED_EXTERNAL_RECEIPT`; strict schema and legacy wire readback remain pending.

### 5. CLI transport — `tests/task_search_cli_transport.rs`

The test launches the compiled CLI, routes it through the MCP server, exercises regex search with `--all-pages`, no-search bounded pagination, claimed-plus-mine filtering, and `task show`. This supports end-to-end client coverage and is distinct from direct store testing. It does not prove compatibility of the published old 0.7.3 image with the v21 migration; the external fallback receipt explicitly records that old-image `VersionMissing` failure.

**Disposition:** `COORDINATOR_VERIFIED_EXTERNAL_RECEIPT`; old/new client transcript and migration compatibility policy remain pending.

### 6. Server canary — `tests/task_search_mcp_server_canary.rs`

The test covers health readiness, unauthenticated MCP rejection, authenticated `tools/list` and `list_tasks`, concurrent clients, clean stop, restart, and reconnect. The external aggregate supports restart/lifecycle coverage, but a single in-process server restart is not proof of alternating independent replicas or broker/outbox reconciliation.

**Disposition:** `COORDINATOR_VERIFIED_EXTERNAL_RECEIPT`; true replica canary and broker/outbox evidence remain pending where required by the transport contract.

## Rust, format, and source checks

- `cargo check --tests`: coordinator-verified `PASS` in `full-regression-results.json`.
- Supporting MCP/CLI/PostgreSQL tests: coordinator-verified `20/20 PASS` in `full-regression-results.json`.
- New MCP validation modules: coordinator-verified `14/14 PASS` at source HEAD `45e33ed` in `full-validation-results.json`.
- Canonical library tests: coordinator-verified `105/105 PASS` in `canonical-build-results.json`.
- `cargo fmt --check`: a format-only clone was reported green for the seven new tests, but rustfmt drift remained on the original dirty Wave B tree. The clone result cannot be silently promoted to the original tree.
- No local Cargo run was possible in this worker scope; the six mandated Node attempts are runner-blocked as recorded above.

**Reducer conclusion:** test receipts are materially supportive, but formatting provenance and clean-checkout CI equivalence are not closed on the exact candidate source.

## Findings and affected-only repairs

### F-01 — Immutable candidate and rollback provenance remain open

The fallback binary was independently booted against the isolated v21 clone, while the published 0.7.3 image failed with SQLx `VersionMissing`. This is a release blocker: old-image migration incompatibility cannot be treated as a successful rollback.

**Repair owner/action:** produce immutable candidate and rollback image digests, document migration compatibility policy, and preserve the old-image failure as a blocking negative receipt. Do not deploy from this report.

**Severity:** `HIGH`.

### F-02 — Exact source-bound independent QA is pending

The runtime aggregates are coordinator-verified external receipts, not worker-accessible raw artifacts. Independent QA must replay the six wire paths, verify source identity, confirm redaction, and distinguish actual MCP transport from direct store or in-process substitutes.

**Repair owner/action:** coordinator QA reads the raw per-test artifacts and records exact command, environment, source revision, fixture size, status/error class, and artifact path for each path.

**Severity:** `HIGH`.

### F-03 — FTS and regex plan/timing evidence is incomplete

The frozen spec requires `EXPLAIN (ANALYZE, BUFFERS)` for keyword field/language branches, contains, regex, pathological regex, and no-indexable-trigram regex, plus bounded totals-query behavior. The available summaries do not close this requirement.

**Repair owner/action:** capture redacted plan/timing receipts on the disposable PostgreSQL fixture; keep no-trigram and timeout behavior fail-closed.

**Severity:** `HIGH`.

### F-04 — Clean-checkout formatting and CI recipe provenance are unresolved

The format-only clone passed, but the original dirty tree had rustfmt drift. The candidate must be checked at one clean source revision, with generated recipe and CI-equivalent results reconciled. No plugin, `.env` backup, Ruflo, or Native state may enter the evidence.

**Repair owner/action:** affected CI/repro owner runs clean-checkout `cargo fmt --check` and CI-equivalent checks at the exact source snapshot, then records content hashes and exclusions.

**Severity:** `HIGH`.

### F-05 — Replica and broker/outbox gates remain pending

The server canary supports lifecycle and restart behavior, but it does not by itself prove alternating independent replicas, stateless behavior across replica boundaries, or broker publication reconciliation. Broker reachability must be separated from accepted Postgres persistence and outbox state.

**Repair owner/action:** run the two-replica authenticated canary and broker/outbox failure/recovery receipt against the isolated fixture; do not call broker delivery green without a raw receipt.

**Severity:** `MEDIUM`.

### F-06 — CrewSync dependency acceptance readback is not closed here

This worker did not call CrewSync administration and did not possess an accepted-task readback in its scoped inputs. Existing documents are candidate/unaccepted and cannot be promoted by this report.

**Repair owner/action:** coordinator reads actual accepted dependency task keys, claims/locks, source binding, and terminal status from CrewSync. Preserve the exact coordinator result; do not infer acceptance from publication or `turn.completed`.

**Severity:** `HIGH`.

## Reducer matrix

| Gate | Evidence | Result |
|---|---|---|
| Six MCP/CLI E2E paths reviewed | Six source tests plus coordinator-verified aggregate runtime receipts | `SUPPORTED / QA PENDING` |
| MCP schema and legacy compatibility | Schema test source and aggregate runtime receipt | `SUPPORTED / WIRE READBACK PENDING` |
| Security, ACL, cursor, regex negatives | Security test source and aggregate runtime receipt | `SUPPORTED / NEGATIVE RECEIPT PENDING` |
| 2,000-row MCP pagination | Pagination source and external traversal receipt | `SUPPORTED / EXACT FIXTURE READBACK PENDING` |
| Full Rust regression | `cargo check --tests`, 20/20 supporting, 14/14 new-module receipts | `SUPPORTED / SOURCE QA PENDING` |
| Rust library tests | `canonical-build-results.json`, 105/105 | `SUPPORTED / SOURCE QA PENDING` |
| Formatting | Format-only clone pass; original-tree drift noted | `REPAIR REQUIRED` |
| EXPLAIN/regex timing | No complete receipt in scoped evidence | `PENDING` |
| Replica/broker/outbox | No complete receipt in scoped evidence | `PENDING` |
| Dependency acceptance readback | No worker-side accepted CrewSync readback | `PENDING` |
| Independent QA | Required by contract and not closed here | `PENDING` |
| Release authorization | Coordinator-owned only | `NOT AUTHORIZED` |

## Final disposition

**Final worker disposition: `CANDIDATE / REPAIR / UNACCEPTED`.** The implementation has meaningful coordinator-verified runtime evidence and all six requested paths were reviewed, but exact source-bound QA, clean-tree formatting/CI, EXPLAIN and regex-plan receipts, immutable migration/rollback provenance, replica/broker gates, and accepted CrewSync dependency readback remain unresolved. The correct reducer result is `REPAIR`, not false green and not an unsupported block on the existence of runtime work.

The coordinator must decide final acceptance independently after affected-only repairs and readback. This document grants no merge, deployment, production, task completion, claim, lock, Ruflo, or Native authorization.
