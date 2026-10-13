# CrewSync PROD cutover — 2026-10-13

## Released Git ↔ Docker ↔ Postgres state

- Source commit: `07b0aa4`, branch `integration/lifecycle-hotrow-20261012` in the isolated Git worktree. Code lineage: `34a2c3e` → `331118d` → `07b0aa4`.
- Container: `docker-bus-1`, image `local/ai-crew-sync:git-docker-parity-07b0aa4`.
- Immutable image ID: `sha256:c6ebfef6005b48ca6bd77177981fecbab383d2a847911073b5ccabda9882d3d8`.
- Compose project: `docker`, service `bus`, loopback port `127.0.0.1:18787`. The only recreated service was `bus`; no `--remove-orphans`.
- Local `.env` in original `DEV/ai-crew-sync` now pins `BUS_IMAGE=local/ai-crew-sync`, `BUS_VERSION=git-docker-parity-07b0aa4`. Secrets remained unprinted and unchanged.
- Postgres 18.4, existing volume `docker_bus-data`, schema migrated v21 → v22, task count 4251.
- PROD readbacks: bus `healthy`, authorized coordinator `whoami` PASS, Ultra Outreach `healthy`.
- Production-clone acceptance before cutover: 4251/4251 inventory, v21 → v22 migration, authenticated MCP, cancellation/supersession, blocked dependent, persistence: GREEN.
- Rust tests of integrated source: 10/10 lifecycle, 10/10 pagination, 10/10 search, 105/105 library (excluding one pre-existing Windows CRLF recipe mismatch). Blue-green operator unit checks: 11/11.
- Fresh pre-cutover database backup: `D:/Archives/CrewSyncBackup/crewsync-pre-cutover-20261013.dump`, SHA-256 `a119d2d4ee062c88d6b48e4d7266a96a4d1556674daaa9fabfacc29fdcc8462d`.
- Preserved old image `local/ai-crew-sync:auth-hotrow-candidate-20261012`, saved as `D:/Archives/CrewSyncBackup/crewsync-rollback-auth-hotrow-20261012.tar`. **Do not run old image directly against v22 without verifying migration compatibility.**

## Repeatable drift guard

`node Docker/verify-git-docker-parity.mjs` in the integration Git worktree is read-only and checks source ancestry/no runtime changes, image ID, live container, health, Compose, loopback port, Postgres migration, tasks and backup integrity. The release-specific expected values are tracked in `Docker/CREWSYNC_PROD_LOCK.json`. All 12 checks passed after PROD cutover.

## Cleanup (all Docker projects)

- `docker builder prune --all --force --filter until=72h`: freed 898.1 MB old unused build cache.
- `docker image prune --force`: 0 B dangling images.
- Removed only three stopped (since 2026-09-23) bind-mounted edit/canary containers: `lanops-vpn-vm-canary-edit`, `network-observability-analytics-edit`, `network-observability-task13-edit`. Host bind data remains untouched.
- Kept dormant N8N, OpenClaw, Hermes, HH bot, PostgreSQL test containers and all Docker volumes; these may contain important data.
- Docker Desktop Engine was recovered after a hung restart; the previously exited non-autorestarting `ultra-outreach-edit-20260910` was started and verified healthy.

## Source strategy

The original Git checkout remains dirty with other agents' changes; it was **not reset or overwritten**. This integration branch is the canonical deployed source. The alternative cancellation branch `db4453c` is retained but not simultaneously activated. Upstream `origin` points to an external repository and was not pushed. Further changes must be integrated into the canonical release branch and result in a new pinned image, followed by drift guard.
