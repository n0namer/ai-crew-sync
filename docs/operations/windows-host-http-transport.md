# Windows A55 CrewSync host-native transport (2026-10-09)

## Purpose and authority

The supported CrewSync Rust MCP server remains the existing instance, database and transactional ownership authority. This runbook changes **only how the Windows command-line client reaches that existing server**. It does not start another server, migrate PostgreSQL, alter task/lease/lock records, or create a second queue.

- Production Windows loopback mapping: `http://127.0.0.1:18787/mcp`.
- Container-local listener: `http://127.0.0.1:8787/mcp` **inside the bus container only**.
- The Windows `127.0.0.1:8787` listener was not the CrewSync production endpoint during this incident (HTTP.sys/PID 4 returned 401); never infer a successful CrewSync call from its response.
- Canonical Windows caller: `C:\Users\NIKITA\.mnnz\crew-sync-client.mjs`.
- Host-native MCP transport: `C:\Users\NIKITA\.mnnz\crew-sync-http-client.mjs` (Node, no runtime Docker CLI).
- Docker CLI mode is **explicit opt-in diagnostic only**: `CREWSYNC_TRANSPORT=docker`. Never automatically replay an uncertain mutation over another transport.

The Windows wrapper continues to use the existing identity-scoped local credentials and `CREWSYNC_SESSION` / `--session` to set `X-Crew-Session`. The child client receives credentials via environment, not Docker command-line argv. Do not log credentials or the environment.

## Quick smoke

1. `curl.exe --max-time 4 http://127.0.0.1:18787/health`: must return HTTP 200 and `database:up`.
2. `node C:\Users\NIKITA\.mnnz\crew-sync-client.mjs coordinator task show ultra:go:masha-core-identity-r1 --json`: returns an exact task object.
3. `node C:\Users\NIKITA\.mnnz\crew-sync-client.mjs coordinator tasks --all-pages --limit 100 --json`: cursor paging must reach `exhausted:true`, `incomplete:false`, with unique keys. Avoid printing the full inventory in interactive diagnostics; parse and report counts.
4. `node C:\Users\NIKITA\.mnnz\crew-sync-client.mjs coordinator lock list --json`: check live holders before mutation.
5. If any mutation times out, treat it as **UNKNOWN/PENDING_SYNC**; read back exact task, note and lock under the original session before any retry. Do not steal another owner, force complete, or release foreign locks.

Timeout bounds: default parent HTTP child 20s for one CLI operation and 90s for `--all-pages`; each HTTP RPC 9s. `CREWSYNC_HTTP_TIMEOUT_MS` allows bounded parent adjustment (1000–120000 ms). No PowerShell scripts or CMD windows are required.

## 2026-10-09 local acceptance

- Host MCP health: HTTP 200, database up, event listener live.
- Six parallel read-only CLI checks completed in 0.47–0.69s: whoami, 3 Ultra exact tasks, locks, paginated list.
- Ultra task inventory: **665 unique** with `exhausted=true`, `incomplete=false`.
- Full CrewSync inventory: **4103 unique**, `exhausted=true`, `incomplete=false`, in 3.2s (open/claimed counts are time-varying).
- Disposable transport-canary lock: acquire -> exact holder/session readback -> release -> confirmed absent. No user task or foreign lock was changed.
- GO Core CrewSync regression: 23/23 PASS; CrewSync inventory regression: 7/7 PASS.
- Docker Engine named-pipe commands timed out earlier, and subsequently recovered without a restart by this operator; this does not invalidate the HTTP transport improvement.
- The server remains **containerized**. Achieving a completely Docker-free server would require a separate versioned deployment and safe Postgres state migration; it is not part of this fast CLI transport repair.

## Recovery and rollout discipline

Use direct HTTP as the default on the existing Windows wrapper. Do not switch the server to a fresh database, delete volumes, or restart unrelated containers as an attempt to fix a CLI timeout. Preserve original Native/CrewSync task IDs and leases. If the existing container stops entirely, restore **that** service with its original volume and config only after backup and ownership verification. A healthy HTTP endpoint and authenticated task readback are the startup criteria, not `docker info` alone.
