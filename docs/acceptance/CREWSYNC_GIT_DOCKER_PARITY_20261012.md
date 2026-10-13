# CrewSync: Git ↔ Docker parity release

Date: 2026-10-12. Scope: only CrewSync. No second task scheduler or task database.

## Canonical code candidate

- Canonical integration branch: `integration/lifecycle-hotrow-20261012` in
  `D:/Users/NIKITA/Documents/DEV/worktrees/ai-crew-sync-integrated-20261012`.
- Parent lifecycle implementation: `34a2c3e` (independent reducer: `ACCEPTED_CODE_CANDIDATE`).
- Integrated hotrow pagination/search delta: `331118d`.
- Integrated Compose stable cursor signing key: `07b0aa4`.
- Alternative branch `feat/safe-task-cancel-20261012` (`db4453c`) is **not merged**: it implements a competing cancellation engine. Preserve it for reference; never enable both.
- The original checkout `D:/Users/NIKITA/Documents/DEV/ai-crew-sync` is intentionally left dirty and untouched, because it contains non-overlapping operator/review work.

## Required invariants

1. Docker image must come from this exact Git source revision; immutable image ID is recorded after build.
2. Docker Compose project stays `docker`; only `bus` is eligible for cutover.
3. Existing Postgres 18.4 volume `docker_bus-data`, cursor secret, team IDs and task identities stay unchanged.
4. Full database archive plus prior image are preserved before database migration.
5. New image must migrate a private copy of production DB from v21 to v22; current clone snapshot has 4251 tasks.
6. Private canary must show real MCP auth, full pagination, cancel, supersede and readback with publication worker disabled.
7. Failures prevent cutover. Rollback is pinned to the previous image, but schema compatibility after v22 must be proven before allowing it.

## Verified tests (isolated QA PostgreSQL)

- Lifecycle: 10/10 PASS.
- Pagination: 10/10 PASS.
- Search: 10/10 PASS.
- Rust library: 105/105 PASS, one unrelated CRLF/LF recipe-rendering regression excluded.
- Existing local blue-green operator: 11/11 PASS.

Machine-readable transcripts: `C:/Users/NIKITA/.mnnz/go-core-waves/crewsync/git-docker-parity-20261012-r1/qa-results.json`.

## Data-protection receipts

- Production export: `D:/Archives/CrewSyncBackup/crewsync-pre-lifecycle-20261012.dump`.
- PostgreSQL v18 custom-format archive, 334 TOC entries, file SHA-256:
  `8583ff818eac33b0234c30477900f9485a25df3d42f030041984ee5c2a1d3099`.
- Prior production image: `local/ai-crew-sync:auth-hotrow-candidate-20261012`,
  image ID `sha256:318c83983e71ff5439d70027e8592972432997dbce41733ad97a5100ea53aa73`.
- Saved rollback image:
  `D:/Archives/CrewSyncBackup/crewsync-rollback-auth-hotrow-20261012.tar`.
- A private PostgreSQL 18.4 clone was restored successfully and had migration 21 and 4251 tasks.

## Current release gate

The approved source, tests and backups are ready. Official static-image build and isolated production-clone canary are pending. **Do not run plain Compose from the original checkout:** its resolved image is currently `ghcr.io/joaquinbejar/ai-crew-sync:0.7.3`, which could downgrade the running service.

A safe Compose invocation explicitly provides `BUS_IMAGE=local/ai-crew-sync` and `BUS_VERSION=git-docker-parity-07b0aa4`, uses the integrated worktree for `--project-directory`, the original local `.env` through `--env-file`, and project name `docker`. No permanent switch of PROD is implied by documenting that command.
