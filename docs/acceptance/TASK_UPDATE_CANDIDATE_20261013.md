# Native update_task — candidate evidence (2026-10-13)

Status: **CANDIDATE / UNACCEPTED**, **NOT DEPLOYED**. Production retains release `1d0ff74`, image `local/ai-crew-sync:git-docker-parity-07b0aa4`, schema v22.

Branch: `feat/native-task-update-20261013` (isolated from release `1d0ff74`).
Canonical CrewSync task: `crewsync:update-task-native-20261013-r1`.

## Public operation
`update_task({key, expected_revision, title?, description?, metadata?})`.
`expected_revision` is required and positive. Missing fields remain unchanged. Explicit `null` clears description or resets metadata to `{}`. Title cannot be null/empty. Metadata accepts only object/null; unknown fields are rejected. Empty PATCH is rejected.

Return: `{task, applied, previous_revision, revision, changed_fields}`. TaskInfo now includes monotonic `revision`, starting at 1. Strict stale revision => conflict, including identical stale retries; identical content with current revision => `applied:false` and no audit event.

Store locks same team task row `FOR UPDATE`, calls existing session guard, rejects live foreign claims and all terminal statuses, performs revision CAS, writes `task_events` and `task_update_audit` in the same transaction, and reads the return snapshot before commit. Task key, UUID, dependencies and initial creation metadata remain unchanged. Update history stores field NAMES and revisions, not changed values or secrets. See migration `0023_task_update_revision.sql`.

CLI: `ai-crew-sync client task update KEY --expected-revision N --title "..." --description "..." --clear-description --metadata-json '{"priority":"high"}' --clear-metadata`; mutually-exclusive clear flags. Host-native clients have Git-managed source under `plugin/scripts/crew-sync-{http-}client.mjs` but are **not installed to .mnnz until release**.

## Verification
- Docker Rust toolchain built library + `tests/task_update.rs` binary successfully.
- Rust library unit regression: **107 PASS, 0 FAIL**, with known preexisting CRLF/LF recipe test excluded. Without exclusion: **107 PASS, 1 known baseline FAIL**.
- Node client syntax and Git diff whitespace checks: pass.
- New disposable PostgreSQL fixture container was created but remote integration invocation was blocked by the security gate. **No real PostgreSQL test run, MCP round-trip, independent QA/reducer, image build, PROD cutover or drift verification is claimed.**

## Release gates not yet satisfied
1. Run `tests/task_update.rs` against disposable PostgreSQL, plus negative race/lease/team/terminal checks; repeat cancel/supersede/search/pagination.
2. Independent QA and reducer; investigate/repair findings.
3. Commit reproducible Docker release image built from exact Git SHA; verify on private DB clone and perform backup.
4. Controlled bus-only cutover; install Git-managed coordinator clients to `.mnnz`, refresh `CREWSYNC_PROD_LOCK.json` and `verify-git-docker-parity.mjs` without weakening safeguards.
5. Check live MCP task update / history, schema v23, preserved task count and health; only then mark task DONE.

Do not deploy current candidate as if it were an accepted release.

## Follow-up verification (2026-10-13)

- Production candidate branch HEAD after audit cascade fix: `94dc78a`.
- Disposable PostgreSQL 18.4 `crewsync_fixture_v23`: schema version **23** applied; `tests/task_update.rs` **2 PASS / 0 FAIL**, including optimistic concurrency, foreign claims, tenant, terminal and audit assertions.
- `tests/task_cancel_supersede.rs`: **2 PASS / 0 FAIL** on disposable database.
- `tests/task_pagination.rs` and `tests/task_search.rs`: **2 PASS / 0 FAIL each** after setting test-only `TASK_CURSOR_SECRET`. An initial pagination failure with missing test secret was a fixture-configuration error, resolved without product changes.
- Docker production-mode image built successfully from clean commit `94dc78a`, tag `local/ai-crew-sync:update-candidate-94dc78a`, image ID `sha256:51ace27c2704997b5d2787cd5157d33609f3dfbbcd7839d3afcaaabc9c569db5`.
- Candidate CLI runs. Real authenticated MCP round-trip **NOT PROVEN**: remote test-token provisioning call was blocked by security controls. Independent reviewer/reducer and clone-production cutover test **NOT PROVEN**. No production deployment or schema migration has been performed.
- Production remains on existing `local/ai-crew-sync:git-docker-parity-07b0aa4` and PostgreSQL v22 as last verified.
