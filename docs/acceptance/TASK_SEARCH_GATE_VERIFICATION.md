# Task Search Gate Verification — Independent QA

**Task key:** `crewsync:task-search-verify-release-r1`  
**Project:** `crewsync`  
**Attempt:** `attempt-f5b464a2-0251-4d46-9a2b-a46fe5811be5`  
**Session:** `manual-crewsync-existing-release-qa-wave-202610-89932ebdea62b351`  
**Producer role:** `implementer`  
**Producer thread:** `01a1202e-799c-7b61-9503-ac54d6c5f312`  
**Review date:** 2026-10-09  
**Review status:** `CANDIDATE / UNACCEPTED`  
**Changed files:** `docs/acceptance/TASK_SEARCH_GATE_VERIFICATION.md` only

This document is independent QA and reducer preparation. It is evidence, not task administration. It does not claim `DONE`, `ACCEPTED`, `ACCEPTED_GREEN`, deployment, merge, or release authorization. No product source, test source, migration, task state, claim, lock, Ruflo state, Native state, or Git state was modified.

## Verdict

**Overall classification: `CANDIDATE` with residual `REPAIR_REQUIRED` and `BLOCKED_INFRA` release gates.**

Coordinator-supplied external receipts correct the prior evidence-boundary wording and materially clear several runtime barriers. They show independent Rust MCP coverage, supporting MCP/CLI/PostgreSQL coverage, canonical-build results, full restored-database traversal, PostgreSQL dump/restore validation, and a working compatibility fallback binary. Those receipts are not directly readable through this worker's scoped MCP project reader, so they are recorded as `COORDINATOR_VERIFIED_EXTERNAL_RECEIPT`, not as nonexistent or as locally re-executed evidence.

Release acceptance remains open. The residual barriers are immutable production image evidence, cross-source/PR parity review, CI and recipe checks, production-shaped FTS EXPLAIN/index coverage, complete regex cancellation/error classification, and broker/outbox plus true replica-canary evidence if required by the release contract. The original repository's Wave B rustfmt state is also not green; the separate format-only clone result does not alter that source tree.

`CANDIDATE` is the submission outcome for coordinator and independent reviewer handling only. It is not acceptance. `VERIFIED` is a publication/readback status, not `ACCEPTED` or `ACCEPTED_GREEN`.

## Evidence boundary and reconciliation

### Directly checked in this worker

- `_bmad-output/implementation-artifacts/spec-task-search.md`: frozen V1 contract, P0–P4 gates, mandatory acceptance matrix, mutable-inventory semantics, CLI/version-skew notes, rollout holds, and the named Draft PR #202 / commit lead.
- `Cargo.toml`: Rust package context; no JavaScript test target is declared.
- `docs/acceptance/TASK_SEARCH_REGRESSION.md`, `docs/acceptance/TASK_SEARCH_PR_READINESS.md`, `docs/acceptance/TASK_SEARCH_E2E_VERIFICATION.md`, `docs/acceptance/TASK_SEARCH_RELEASE.md`, and `docs/acceptance/TASK_SEARCH_MCP_CI_REPRO.md`: prior source/runtime classifications, release procedure, and no-false-green rules.
- `plugin/commands/ask.md`: command documentation only; no release or task evidence.
- Project inventory: Rust source/tests and acceptance docs are present; no applicable JavaScript test file is present.
- `run_node_tests(tests/task_search.rs)` was attempted in this continuation and returned `GO_WAVE_NODE_ERROR:test_file_not_allowed`; no Node test process ran. This is a tool limitation, not a Rust pass or failure.

### Coordinator-verified external receipts

The following receipts were supplied by the coordinator and are treated as external evidence with their stated source identifiers. They were not erased merely because the worker MCP reader cannot access the external paths.

- **New Rust MCP modules:** 7 modules, `14/14 PASS`, source HEAD `45e33ed`, snapshot SHA256 `13dd4a66669928e6a88a2f297b66712139bb7a454b5ffeb62c04746eab23dd97`; source `D:/Users/NIKITA/Documents/DEV/go-core/runtime/waves/20261009-crewsync-task-search/full-validation-results.json`; individual per-test logs exist.
- **Supporting MCP/CLI/PostgreSQL:** 10 tests, `20/20 PASS`, and `cargo check --tests PASS`; receipt `full-regression-results.json` in the same coordinator evidence set.
- **Canonical Rust library build:** original `cargo test --lib` was `104/105` because the plugin/commands fixture was intentionally excluded; a later LF canonical snapshot compiled the candidate fixture and reached `cargo test --lib 105/105 PASS`; receipt `canonical-build-results.json`. The original untracked Wave B test files remain rustfmt-not-green in the repository; the format-only clone passed `cargo fmt --check` without changing those worker-owned files.
- **Full restored-database CLI traversal:** isolated clone result `cli-full-traversal-clone-result.json`: `4053` rows, `4053` unique keys, `0` duplicates, `exhausted=true`, `incomplete=false`, `has_more=false`, `next_cursor=null`; filter counts include `crewsync:` prefix `57`, contains `MCP` `166`, and open status `1904`. The ephemeral token was revoked.
- **PostgreSQL dump/restore and migration checks:** PGDMP production dump verified with SHA256 and `pg_restore` catalog, restored to an isolated clone containing `4053` tasks. Canonical LF migration checks matched historical database `19/19`; candidate migration reached `20/21` and continued with `4053` rows. Current production remains on `0.7.3`.
- **Compatibility fallback:** the old published `0.7.3` image fails on migrated v21 with SQLx `VersionMissing`. A fallback binary from git `8897d7a`, using canonical `20/21` SQL, booted independently against the isolated v21 clone, passed health, and served all `4053` tasks; receipt `D:/Users/NIKITA/Documents/DEV/go-core/.local-ops/crewsync-bluegreen-20261009/fallback-clone-acceptance.json`.

These external receipts support runtime coverage; they do not by themselves prove production deployment, immutable-image parity, complete PR parity, release acceptance, or coordinator task terminal state.

## Evidence-supported gate status

| Gate | Status | Evidence and boundary |
|---|---|---|
| F01 — Rust MCP search coverage | `COORDINATOR_VERIFIED_EXTERNAL_RECEIPT` | 7 new modules, `14/14 PASS`, source HEAD `45e33ed`, snapshot `13dd4a...3dd97`; raw path and per-test logs supplied externally. |
| F02 — Supporting MCP/CLI/PostgreSQL coverage | `COORDINATOR_VERIFIED_EXTERNAL_RECEIPT` | 10 supporting tests, `20/20 PASS`, plus `cargo check --tests PASS` in `full-regression-results.json`. |
| F03 — Canonical library test coverage | `COORDINATOR_VERIFIED_EXTERNAL_RECEIPT` | LF canonical snapshot reached `cargo test --lib 105/105 PASS`; original `104/105` was the intentional plugin fixture exclusion. |
| F04 — Full restored-database traversal | `COORDINATOR_VERIFIED_EXTERNAL_RECEIPT` | `4053/4053` unique, zero duplicates, exhausted, complete, no next cursor; isolated clone only. |
| F05 — PostgreSQL restore/migration compatibility | `COORDINATOR_VERIFIED_EXTERNAL_RECEIPT` | PGDMP/catalog verified; `19/19` historical migration match; candidate `20/21` continuation; current production remains `0.7.3`. |
| F07 — Old-image compatibility fallback | `REPAIR_REQUIRED / COORDINATOR_VERIFIED_EXTERNAL_RECEIPT` | Published `0.7.3` fails migrated v21 with SQLx `VersionMissing`; fallback `8897d7a` boots and serves clone. Immutable production image and rollback policy remain open. |
| Regex safety | `RUNTIME-PENDING` | General coverage exists, but total regex query classification/cancellation behavior still needs exact raw evidence. |
| FTS/trigram readiness | `REPAIR_REQUIRED` | Production-shaped `EXPLAIN (ANALYZE, BUFFERS)` and index-use evidence remain outstanding for all field/language/mode branches. |
| Rust formatting | `REPAIR_REQUIRED / BLOCKED_INFRA` | Original repository Wave B tests are not rustfmt-green; format-only clone passed without modifying original worker-owned files. |

## Residual findings

### R1 — RELEASE BLOCKER: immutable production images and rollback compatibility remain unresolved

The external fallback receipt demonstrates a valid isolated-clone rollback path: old `0.7.3` fails against migrated v21 with SQLx `VersionMissing`, while the `8897d7a` compatibility binary boots and serves the restored clone. This is valuable affected evidence, not production deployment evidence. The release still needs immutable image digests, source-to-image parity, migration compatibility policy, and a coordinator-approved rollback/readmission path. Current production remains `0.7.3`; no production mutation was performed by this worker.

### R2 — RELEASE BLOCKER: FTS/index readiness lacks production-shaped EXPLAIN coverage

The restored traversal and functional tests support correctness and pagination, but they do not prove bounded query plans. The remaining evidence must cover title, description, and both fields across simple/English/Russian keyword branches, literal contains, case-insensitive regex, and a deliberately non-indexable regex after `ANALYZE`, with `EXPLAIN (ANALYZE, BUFFERS)` and timings. Any mismatch between runtime expressions and migration indexes remains an affected SQL/index-owner repair, not a documentation repair.

### R3 — RELEASE BLOCKER: regex total-query classification is incomplete

The available receipt set supports broad runtime coverage, but the coordinator identifies total regex query classification as still pending. Preserve the actual PostgreSQL SQLSTATE/error category for invalid regex, statement timeout/cancellation, and non-indexable patterns, including elapsed time and whether the public MCP/CLI boundary returns a safe bounded error. Do not infer the disputed SQLSTATE from source claims.

### R4 — RELEASE BLOCKER: cross-source review, CI, recipes, and PR parity remain open

The new results are tied to source HEAD `45e33ed` and canonical snapshots, while the prior release lead names Draft PR #202 and commit `f213f1e`. A complete review still needs exact source-to-PR/commit parity, whole-diff English-comment audit, `recipes.py check`, `make recipes-check`, and exact CI workflow coverage. No green narrative or test subset substitutes for these receipts.

### R5 — RELEASE BLOCKER: original repository formatting is not green

The original untracked Wave B tests are not rustfmt-green. The independent format-only clone passed `cargo fmt --check`, but that clone result did not modify or certify the original worker-owned files. The affected owner must format or otherwise repair only its own source/test scope, then rerun the repository formatter and preserve the exact diff/result.

### R6 — CONDITIONAL BLOCKER: broker/outbox and true replica canary

If the release contract requires broker/outbox behavior and a true replica/read-only canary, those receipts remain pending. The coordinator must state whether these are mandatory for this release wave. If mandatory, run them against isolated infrastructure and preserve raw results; if not applicable, record the explicit waiver/contract basis rather than silently omitting them.

### R7 — RELEASE WORDING: mutable inventory is best effort

Cursor continuation has no snapshot identifier. Inserts, status changes, lease expiry, and `updated_at` changes can move rows between pages. Release wording must say mutable-inventory pagination is best effort and must not promise point-in-time snapshot consistency.

## Corrected release barrier matrix

| Gate | Classification | Exact remaining barrier |
|---|---|---|
| Search correctness, ACL, filters, totals | `COORDINATOR_VERIFIED_EXTERNAL_RECEIPT` | Preserve raw receipts and source identity; no production acceptance inferred. |
| Cursor replay and full traversal | `COORDINATOR_VERIFIED_EXTERNAL_RECEIPT` | Retain `4053/4053`, zero duplicates, exhausted terminal state, and filter counts. |
| Rust MCP/supporting tests | `COORDINATOR_VERIFIED_EXTERNAL_RECEIPT` | Retain raw results and exact source/snapshot identifiers. |
| Canonical Rust library tests | `COORDINATOR_VERIFIED_EXTERNAL_RECEIPT` | Retain `105/105`; explain original `104/105` fixture exclusion. |
| FTS/trigram readiness | `REPAIR_REQUIRED` | Add production-shaped EXPLAIN/timing coverage or affected SQL/index repair. |
| Regex safety/classification | `RUNTIME-PENDING` | Preserve actual invalid-regex and timeout/cancellation categories and timings. |
| Rust formatting | `REPAIR_REQUIRED` | Format original Wave B files or repair affected source/test scope; clone-only green is insufficient. |
| Immutable production image parity | `BLOCKED_INFRA` | Provide immutable image digests, source parity, migration compatibility, and rollback evidence. |
| Draft PR #202 / cross-source parity | `BLOCKED_INFRA` | Verify exact PR head/commit relationship and complete diff. |
| English-only comments | `BLOCKED_INFRA` | Audit the complete verified diff, not only excerpts. |
| CI and recipes | `BLOCKED_INFRA` | Run recipe checks and inspect exact-source CI, including ignored MCP/raw-wire coverage. |
| Broker/outbox and replica canary | `BLOCKED_INFRA / CONDITIONAL` | Run if mandatory; otherwise document explicit contract non-applicability. |
| Mutable inventory semantics | `REPAIR_REQUIRED` | State best-effort, non-snapshot pagination in release wording. |
| Coordinator task readbacks | `BLOCKED_INFRA` | Independently reconcile predecessor/current task status and immutable receipts. |

## Affected-only repair sequence

1. **SQL/index owner:** capture production-shaped EXPLAIN/timing evidence and repair only runtime SQL/migration/test scope if the plans confirm a defect.
2. **Regex/E2E owner:** preserve exact SQLSTATE/error categories, timeout/cancellation behavior, and public-boundary output for total regex cases.
3. **Wave B test owner:** format or repair only the original untracked Wave B files; rerun repository rustfmt and preserve the diff.
4. **Release/CI owner:** verify immutable image digests, PR/source parity, complete English-comment audit, CI workflow, and recipe checks.
5. **Coordinator/reducer:** decide whether broker/outbox and true replica canary are mandatory, then collect raw receipts or explicit contract-based non-applicability.
6. **Coordinator:** read back predecessor/current task statuses and immutable result records; do not treat `CANDIDATE`, `VERIFIED`, or external receipt status as acceptance.

No product-source repair is requested from this documentation-owned task. Any repair belongs to the affected owner’s manifest scope.

## Self-review

- Trusted task key, project, attempt, session, and known producer thread exactly match the supplied binding.
- Coordinator-external receipts are explicitly labeled external and not falsely reported as nonexistent or locally executed.
- F01/F02/F03/F04/F05/F07 statuses retain evidence-supported distinctions.
- `CANDIDATE / UNACCEPTED` is preserved; no acceptance or release claim is made.
- The only changed file is inside the exact write scope.
- The Node result is preserved exactly as `GO_WAVE_NODE_ERROR:test_file_not_allowed`.
- No task, claim, lock, Ruflo, Native, Git, deployment, production, migration, or source state was mutated.
