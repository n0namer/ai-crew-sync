# VP49 — n0namer CrewSync fork deployment (2026-10-22)

## Source and authority
- Canonical source: https://github.com/n0namer/ai-crew-sync (branch `main`).
- Recovered feature commit: `45e33edb15f232fdb004d7a5f3b0c3c69548a4f0`; fast-forwarded the fork's `main` from upstream `8897d7a`.
- Source checkout on VP49: `C:\Users\Рафик\Documents\Dev\ai-crew-sync-recovery-20261022`; local branch `main` tracks `origin/main`.
- Legacy upstream checkout `...\Documents\Dev\ai-crew-sync` is retained separately for rollback.

## Running service
- Docker Compose project: `docker`, service `bus`, container `docker-bus-1`.
- Runtime image: `ai-crew-sync:n0namer-45e33ed`, built on VP49 from this fork's `Docker/Dockerfile`.
- Local server: `http://127.0.0.1:18787/mcp`; `/health` returned HTTP 200 after a controlled restart.
- Existing `docker-db-1` was NOT recreated; migration count increased from 19 to 21 (additive task-search migrations 0020/0021).
- Private runtime configuration: this checkout's Git-ignored `.env`, copied from the original deployment with only `BUS_IMAGE` and `BUS_VERSION` changed.
- The task cursor signing code requires `TASK_CURSOR_SECRET`, falling back to an existing nonempty `BUS_DASHBOARD_SECRET`. Never commit either value.
- Local client wrapper: `~/.mnnz/crew-sync-client.mjs`; private tokens: `~/.mnnz/crew-sync-local.env` (coordinator and worker-1/2/3). Never commit, print or distribute them.

## Verified tests
- Disposable PostgreSQL 18 canary: migration 21/21 and health HTTP 200.
- Authenticated real MCP CLI: `tools` exposes `list_tasks`; legacy --all-pages pagination, cursor with page size 1, contains/keywords search, and malformed regex rejection passed.
- Live VP49 post-deploy: `coordinator`, `worker-1`, `worker-2`, `worker-3` `whoami` passed. `coordinator tasks --all-pages` and `worker-1 tasks --all-pages` passed after a Docker restart; `coordinator tasks --search ... --search-mode contains` passed.
- Release Rust binary compiled successfully through Docker Linux; Windows cargo toolchain lacked `link.exe`/rustfmt.
- Formal full Rust acceptance matrix and independent QA are **not** being claimed complete here. Production CrewSync deployment is working for the tested feature subset.
- A55 task data/notes are not recovered; the local queue was empty before this deployment, by user direction data restoration is out of scope.

## Backup and rollback
- Retained old image: `ghcr.io/joaquinbejar/ai-crew-sync:0.7.3`.
- Recovered Docker image tar: `...\Documents\Dev\CrewSyncRecovery\ai-crew-sync-n0namer-45e33ed-image.tar`, SHA256 `35BA9F46A81EA7955FDA59AD443DAAB12126ACAF898B0DEC2C0542738BDD928B`.
- Pre-deployment PostgreSQL dump: `...\Documents\Dev\CrewSyncRecovery\vp49-bus-20261022.dump`. Treat as private.
- To redeploy fork (PowerShell on VP49):
  ```powershell
  $root = Join-Path $env:USERPROFILE 'Documents\Dev\ai-crew-sync-recovery-20261022'
  docker compose --project-directory $root -f (Join-Path $root 'Docker\docker-compose.yml') up -d --no-deps --no-build bus
  ```
- To roll back only bus to upstream 0.7.3, use the original checkout's Compose and unchanged `.env`:
  ```powershell
  $old = Join-Path $env:USERPROFILE 'Documents\Dev\ai-crew-sync'
  docker compose --project-directory $old -f (Join-Path $old 'Docker\docker-compose.yml') up -d --no-deps --no-build bus
  ```
- Neither command should run `down --volumes`, rebuild PostgreSQL, or remove its named volume.
