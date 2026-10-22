# Task Search MCP Final Adversarial Review

**Review status:** `CANDIDATE / UNACCEPTED`
**Review date:** 2026-10-09
**Project:** `crewsync`
**Task key:** `crewsync:task-search-mcp-adversarial-review-r1`
**Attempt:** `attempt-85fbd592-7b22-4cdb-ae84-be0381e0e629`
**Session:** `manual-crewsync-existing-release-qa-wave-202610-92535dad60e611ca`
**Producer role:** `implementer`
**Producer thread:** `01a1202e-79ce-7712-9235-4c93909e1cab`
**Changed files:** `docs/acceptance/TASK_SEARCH_MCP_FINAL_REVIEW.md` only

## Scope and corrected decision

This is the continuing independent BMad Help, Adversarial Review, and Edge Case Hunter review of the actual task-search production code, the six requested MCP/CLI E2E paths, and the coordinator-read-back evidence. It does not create a new task or redo discovery. It corrects the prior report's evidence classification and records only residual release gaps.

**Decision: remain `CANDIDATE / UNACCEPTED`; do not promote.** The coordinator independently read GO Core artifacts outside this worker's allowed MCP read path. Those artifacts exist and validate substantial runtime coverage. They must not be described as nonexistent merely because this worker could not open them through `go_wave`. The receipts support F01/F02/F03/F04/F05/F07 runtime coverage, but they do not authorize release or erase the remaining CI, image, cross-source, regex-plan, broker/outbox, and replica-canary gaps.

No source, migration, production, task-state, lock, Ruflo, or Native state was changed. The only write is this review document.

## Coordinator-verified external receipts

The following are recorded as `COORDINATOR_VERIFIED_EXTERNAL_RECEIPT`: independently read by the coordinator, but not directly accessible through this worker's scoped MCP read path. They are evidence references, not model claims and not self-acceptance.

- **Source identity:** Git source HEAD `45e33ed`; snapshot SHA256 `13dd4a66669928e6a88a2f297b66712139bb7a454b5ffeb62c04746eab23dd97`.
- **Seven new Rust MCP modules:** `D:/Users/NIKITA/Documents/DEV/go-core/runtime/waves/20261009-crewsync-task-search/full-validation-results.json`; seven individual per-test logs exist; independent result `14/14 PASS` at the source snapshot.
- **Supporting regression:** `full-regression-results.json`; ten supporting MCP/CLI/PostgreSQL tests `20/20 PASS` and `cargo check --tests PASS`.
- **Canonical library snapshot:** `canonical-build-results.json`; LF canonical snapshot with the plugin fixture compiled and `cargo test --lib 105/105 PASS`. The earlier `104/105` result was the intentionally excluded plugin/commands fixture and must not replace the canonical result.
- **Full restored-clone CLI traversal:** `cli-full-traversal-clone-result.json`; isolated restored database, 4,053 rows, 4,053 unique keys, zero duplicates, `exhausted=true`, `incomplete=false`, `has_more=false`, `next_cursor=null`; filter readbacks include `crewsync:` prefix 57, contains `MCP` 166, and open status 1,904. Ephemeral token was revoked.
- **Database restoration and migration comparison:** PostgreSQL 18.4 production dump was verified as PGDMP with SHA256 and `pg_restore` catalog, then restored to the isolated clone with 4,053 tasks. Canonical LF migration checks matched 19/19 historical migrations; candidate applied 20/21 and continued with 4,053 rows. Production remains on 0.7.3.
- **Rollback compatibility:** `D:/Users/NIKITA/Documents/DEV/go-core/.local-ops/crewsync-bluegreen-20261009/fallback-clone-acceptance.json`; fallback binary from Git `8897d7a`, with 20/21 canonical SQL, independently booted against the isolated v21 clone, health OK, all 4,053 tasks readable. The old published 0.7.3 image fails on migrated v21 with SQLx `VersionMissing`; this is a release blocker, not a hidden success.

These external receipts correct the previous phrase “no raw receipts or same-source independent readbacks are present.” The accurate statement is: **the receipts are coordinator-verified but inaccessible through this worker's scoped MCP read path; raw artifact replay and final acceptance remain coordinator responsibilities.**

## Worker-scoped validation

`Cargo.toml` identifies a Rust/Cargo project and provides no Node test suite. The earlier scoped Node attempts for the six `.rs` integration tests returned `GO_WAVE_NODE_ERROR:test_file_not_allowed`; that runner limitation is retained as worker evidence, not as evidence that the Rust tests did not run.

No Cargo/PostgreSQL execution tool is available in this worker scope. Therefore this repair does not claim a new local Rust PASS. The coordinator-verified external receipts above are preserved verbatim by identifier and status.

## Corrected findings and residual repairs

### F-01 — Runtime coverage supported; production image and release provenance remain open

**Validated evidence:** The coordinator-verified `full-validation-results.json` reports seven new Rust MCP modules `14/14 PASS` at source HEAD `45e33ed`, and the independent fallback-clone receipt shows a compiled compatibility binary from `8897d7a` booted against isolated migrated v21 with health OK and all 4,053 tasks readable. This corrects the prior claim that no binary/runtime evidence existed.

**Residual risk:** The published 0.7.3 image fails on migrated v21 with SQLx `VersionMissing`. The fallback binary is an isolated compatibility receipt, not an immutable production image or deployment authorization. Candidate source, image contents, migration policy, and rollback provenance still require coordinator verification.

**Exact affected-only repair:** The release/image owner must produce immutable candidate and rollback image digests, prove migration compatibility policy on the isolated clone, and retain the old-image `VersionMissing` failure as a blocking rollback result. Do not deploy or mutate production from this review.

**Severity:** `HIGH`; runtime coverage is supported, but release/rollback remains blocked.

### F-02 — Raw wire coverage supported; artifact replay and redaction remain coordinator gates

**Validated evidence:** The coordinator reports `14/14 PASS` across the seven new Rust MCP modules at snapshot SHA256 `13dd4a66669928e6a88a2f297b66712139bb7a454b5ffeb62c04746eab23dd97`; the referenced per-test logs and full-validation receipt are the raw external evidence for initialize, tools/list, tools/call, and transport behavior.

**Residual risk:** This worker cannot open those logs through `go_wave`, so it cannot independently replay headers, negotiated protocol details, or redaction. The prior report's “no raw wire receipt” language is withdrawn; the remaining gap is cross-source reviewer readback, not evidence absence.

**Exact affected-only repair:** The independent reviewer/coordinator must replay the referenced raw wire receipt, confirm source hash and redaction, and retain the result as `COORDINATOR_VERIFIED_EXTERNAL_RECEIPT`. Do not rerun or rewrite the original worker-owned tests solely to compensate for this worker's read-path limitation.

**Severity:** `MEDIUM`; transport result supported, independent final readback pending.

### F-03 — ACL and tenant boundary coverage supported; final raw negative-case readback remains open

**Validated evidence:** The coordinator-verified seven-module result includes the tenant/ACL and signed-cursor module among the `14/14 PASS` source-snapshot results. This corrects the prior report's claim that no direct ACL runtime result existed.

**Residual risk:** The worker cannot inspect the per-test log to confirm exact fresh foreign-team calls, status/error classes, and redaction. The release matrix still requires independent QA of those negative cases.

**Exact affected-only repair:** Coordinator QA must read back the tenant/cursor log and confirm fresh no-cursor foreign-team denial, invalid-token behavior, tamper rejection, and absence of secret/SQL leakage. Keep status `COORDINATOR_VERIFIED_EXTERNAL_RECEIPT` until that readback is attached to the final reducer.

**Severity:** `MEDIUM`; runtime result supported, independent QA pending.

### F-04 — Cursor binding coverage supported; complete matrix readback remains open

**Validated evidence:** The coordinator-verified seven-module result includes signed cursor/tenant coverage and reports `14/14 PASS` at the exact source snapshot. This corrects the prior “incomplete cursor coverage” conclusion as an evidence-absence claim.

**Residual risk:** The worker cannot replay the external log to enumerate every altered query/mode/field/language/status/prefix/tampered-token case. The frozen cursor contract still requires that matrix and exact error classification to be independently read back.

**Exact affected-only repair:** The coordinator/reviewer must read the cursor test log and preserve the exact altered-binding and tamper outcomes. Any genuine failure routes only to the original cursor owner; do not alter cursor semantics from this document.

**Severity:** `MEDIUM`; runtime result supported, matrix readback pending.

### F-05 — Exact 2,000-row MCP traversal coverage supported; full restored CLI inventory is additional evidence

**Validated evidence:** The coordinator-verified seven-module receipt reports the searched 2,000-task traversal passing at source snapshot `45e33ed`. Separately, the restored-clone CLI receipt reports 4,053 rows, 4,053 unique keys, zero duplicates, exhausted terminal state, and no next cursor. These results correct the prior claim that the available traversal was only permissive/no-search evidence.

**Residual risk:** The full CLI inventory is isolated-clone evidence, not production deployment evidence. The worker cannot inspect page-by-page raw receipts, timing, exact searched traversal fields, or the restored database artifact through `go_wave`.

**Exact affected-only repair:** Coordinator QA must reconcile the 2,000-row MCP receipt, the 4,053-row CLI receipt, source snapshot, migration state, and fixture identity. Preserve the distinction between isolated clone and production; do not mutate production or infer deployment.

**Severity:** `MEDIUM`; runtime coverage supported, provenance/reducer readback pending.

### F-06 — Regex plan and totals-query classification remain unresolved

**Validated evidence:** The external result set supports the broader Rust MCP suite, but the coordinator explicitly lists FTS `EXPLAIN` index coverage and total regex query classification as pending. The production search path still sets a transaction-local regex timeout and maps some row-query SQLSTATEs; the totals query remains a separate propagated query in `src/store/tasks.rs`.

**Residual risk:** A no-trigram pattern may regress to an unsafe plan, or a timeout during the team-wide totals query may surface as an unclassified database error. A bounded successful regex is valid, but timing/plan evidence must demonstrate that the bound and error contract are real.

**Exact affected-only repair:** The regex owner must provide redacted `EXPLAIN (ANALYZE, BUFFERS)` and timing evidence for supported and no-trigram patterns, plus a focused totals-timeout classification result. Repair only the affected search error path if a generic timeout is reproduced; do not require a nondeterministic timeout when bounded success is demonstrated.

**Severity:** `HIGH`; remains a release blocker.

### F-07 — Schema/compatibility coverage supported; clean-source and strict-shape readback remain open

**Validated evidence:** The coordinator-verified seven-module result includes the strict schema path and reports `14/14 PASS`; the supporting regression reports `20/20 PASS`. This corrects the prior claim that schema runtime coverage was absent.

**Residual risk:** The worker cannot inspect the external schema receipt or confirm exact property/default/optional shape, absence of a competing tool, and source identity. The old published 0.7.3 image is also known to be incompatible with migrated v21, so compatibility must distinguish old-client request compatibility from old-image migration compatibility.

**Exact affected-only repair:** Coordinator QA must read back the strict schema and legacy-request artifacts, verify one canonical `list_tasks` tool and additive optional fields, and preserve the old-image migration failure as a separate rollback blocker. Do not reorder production semantics to satisfy an incorrect fixture expectation.

**Severity:** `MEDIUM`; runtime coverage supported, final schema/release readback pending.

### F-08 — Lifecycle/restart coverage supported; true replica and broker/outbox gates remain open

**Validated evidence:** The coordinator-verified seven-module result includes restart/lease coverage and reports `14/14 PASS`; supporting MCP/CLI/PostgreSQL tests report `20/20 PASS`. This corrects the prior claim that lifecycle runtime evidence was absent.

**Residual risk:** The coordinator identifies true replica canary and broker/outbox evidence as pending if required. In-process or single-binary restart evidence does not by itself prove alternating independent replicas, publication reconciliation, or broker failure semantics.

**Exact affected-only repair:** The lifecycle/reducer owner must run the required two-replica canary against the isolated clone, alternate authenticated requests without sticky state, and capture broker/outbox disposition separately. Keep broker delivery `PENDING` unless a raw receipt proves it.

**Severity:** `MEDIUM`; restart runtime supported, replica/broker gates pending.

### F-09 — CI, formatting provenance, immutable images, and recipe drift remain unresolved

**Validated evidence:** `full-regression-results.json` reports `cargo check --tests PASS` and 20/20 supporting tests; `canonical-build-results.json` reports `cargo test --lib 105/105 PASS` in the LF canonical snapshot. Independent format-only clone formatting passed `cargo fmt --check` for the seven new tests.

**Residual risk:** Rustfmt was not green on the original untracked Wave B tests in the repository. The format-only clone result must not be silently promoted to the original dirty tree. Clean-checkout CI/recipes, immutable image construction, cross-source review, and task-search-only file provenance remain pending.

**Exact affected-only repair:** The CI/repro owner must provide a clean-checkout receipt at the exact tested snapshot, reconcile original versus format-only clone content hashes, run CI-equivalent checks and recipe validation, and exclude plugin changes, `.env` backups, Ruflo, and Native state. No CI, recipe, or source changes belong in this review document.

**Severity:** `HIGH`; release blocker.

## Six-path disposition

| Path | Corrected evidence disposition | Status |
|---|---|---|
| `tests/task_search_mcp_transport.rs` plus new wire modules | Coordinator-verified external 14/14 source-snapshot result and raw per-test logs; worker cannot open those artifacts via MCP. | `COORDINATOR_VERIFIED_EXTERNAL_RECEIPT` |
| `tests/task_search_mcp_security.rs` plus tenant/cursor modules | Coordinator-verified ACL, signed-cursor, invalid-input runtime coverage; exact raw negative-case replay remains coordinator QA. | `COORDINATOR_VERIFIED_EXTERNAL_RECEIPT` |
| `tests/task_search_mcp_pagination.rs` plus searched traversal | Coordinator-verified 2,000-row MCP traversal; separate restored-clone CLI inventory is 4,053/4,053 unique and exhausted. | `COORDINATOR_VERIFIED_EXTERNAL_RECEIPT` |
| `tests/task_search_mcp_schema.rs` plus strict schema module | Coordinator-verified strict schema and legacy compatibility coverage within 14/14 and 20/20 results. | `COORDINATOR_VERIFIED_EXTERNAL_RECEIPT` |
| `tests/task_search_cli_transport.rs` plus supporting regression | Coordinator-verified real CLI/MCP/PostgreSQL regression and full restored-clone traversal; old 0.7.3 image migration incompatibility remains separate. | `COORDINATOR_VERIFIED_EXTERNAL_RECEIPT` |
| `tests/task_search_mcp_server_canary.rs` plus restart/lease modules | Coordinator-verified restart/lease runtime coverage; true replica canary and broker/outbox evidence remain pending where required. | `COORDINATOR_VERIFIED_EXTERNAL_RECEIPT` |

## Remaining release gaps

- FTS `EXPLAIN (ANALYZE, BUFFERS)` and no-trigram regex plan/timing evidence, including totals-query timeout classification.
- Immutable production candidate/rollback image digests and migration policy; published 0.7.3 fails on migrated v21 with SQLx `VersionMissing`.
- Clean-checkout CI and recipe-generation reproducibility, including reconciliation of original dirty Wave B tests versus the format-only clone.
- Cross-source independent QA/readback of the external raw receipts and exact source snapshot.
- True replica canary and broker/outbox reconciliation if required by the transport contract.
- Coordinator-owned release reducer and acceptance; this report remains neither release authorization nor `ACCEPTED_GREEN`.

## Coordinator disposition

1. Preserve all coordinator-verified receipts above; do not revert to the prior evidence-absence wording.
2. Keep F01/F02/F03/F04/F05/F07 runtime statuses evidence-supported, not `MISSING` and not `CLAIMED_GREEN`.
3. Route only F06/F08/F09 and the listed release gaps to their exact affected owners; do not broaden this document's write scope.
4. Perform independent QA/readback using the external artifact identifiers and source snapshot. A worker result, `VERIFIED`, process exit, or publication receipt is not acceptance.
5. Do not merge, deploy, or authorize release until the remaining mandatory gates are independently green and coordinator-owned acceptance occurs.

**Final review outcome:** `CANDIDATE` only; no self-acceptance, no release authorization, no task administration, no Ruflo/Native changes.
