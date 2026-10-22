# CrewSync lifecycle — independent reducer runtime evidence R6

Date: 2026-10-12. Worktree: `goal/cancel-supersede-20261012` at initial HEAD `8b03a74`, with uncommitted candidate changes. All verification runs used Docker and disposable PostgreSQL (`crewsync-qa-pg-r6` / `crew_fixture_test`), not production.

## Executed results

| Check | Observed result |
|---|---|
| `cargo build --locked --offline --bin ai-crew-sync` | PASS, latest source candidate |
| `cargo test --locked --offline --test task_cancel_supersede -- --ignored` | 2/2 PASS on disposable PostgreSQL R5 |
| `cargo test --locked --offline --lib -- --skip recipes::tests::every_slash_command_is_its_recipe_rendered` | 105/105 PASS |
| Full `cargo test --lib` | 105 PASS / 1 FAIL, unrelated baseline Windows CRLF/LF recipe rendering comparison |
| `cargo test --test task_pagination -- --include-ignored` | 10/10 PASS on disposable PostgreSQL R6, including 2000-task concurrent pagination |
| `cargo test --test task_search -- --include-ignored` | 10/10 PASS on disposable PostgreSQL R6, including 2000-task search |
| `git diff --check` | PASS (exit 0) |
| MCP + CLI lifecycle smoke test | `QA_E2E_GREEN` from `qa-e2e-r3.sh` on isolated server |
| MCP restart persistence test | `QA_RESTART_GREEN` from `qa-restart-r3.sh` on isolated server |
| NULL-lease process restart R6 | `QA_STAGE1_GREEN`, then PostgreSQL `UPDATE 1` to set legacy NULL lease, then `QA_RESTART_GREEN` |
| Independent source reviewer R2 | F01–F06 PASS by source/contract, with pending runtime evidence now supplied here |

## R6 restart gates

1. Started CrewSync binary against disposable database, authenticated through MCP, created and claimed `legacy-null`.
2. Stopped CrewSync server and set `lease_expires_at = NULL` in PostgreSQL; verified one changed row.
3. Started a fresh CrewSync server process on the **same** database.
4. Verified preserved claimed state, rejection of foreign claim/complete/renew, successful original owner renew/release, refusal to complete an unresolved dependent, refusal of deadlocking supersede, and persisted cancellation readback.
5. The assertion-only scripts print `QA_STAGE1_GREEN` and `QA_RESTART_GREEN` only after all respective checks succeed.

Exact QA scripts and search transcript are in:
`C:/Users/NIKITA/.mnnz/go-core-waves/crewsync/20261012-crewsync-safe-cancel-supersede-r1/`:
`qa-legacy-first-r6.sh`, `qa-legacy-restart-r6.sh`, `qa-search-r6.log`, `qa-run-search-r6.mjs`.

## Decision boundaries

- This is **technical candidate evidence**. Independent reviewer R2 reviewed code, not these external QA files. A reducer must bind evidence to actual changed-file readback before acceptance.
- The Windows CRLF/LF recipe rendering mismatch remains outside the lifecycle write scope; do not present the full Rust library run as 106/106.
- No migration was applied to production, no prod container was replaced, no task was marked `done` merely from worker exit.
- The current PROD image `local/ai-crew-sync:auth-hotrow-candidate-20261012` is not byte-for-byte proven compatible with this candidate. Do not replace it without an explicit differential build/backup/rollback gate.
