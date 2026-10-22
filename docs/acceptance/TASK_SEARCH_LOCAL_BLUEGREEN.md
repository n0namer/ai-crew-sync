# Local Search MCP Blue/Green Operator

**Outcome:** `CANDIDATE`
**Acceptance:** `UNACCEPTED`
**Release decision:** `DO NOT CUT OVER`
**Review date:** 2026-10-09
**Project:** `crewsync`

This document records the bounded local operator contract. It does not authorize deployment, task administration, claim/lease changes, Ruflo/Native changes, or acceptance.

## Safety and execution contract

`Docker/local-mcp-bluegreen.mjs` is dry-run by default. Execute mode requires a real injected shell-free Node argv runner. Authorized cutover additionally requires structured readbacks, immutable digest-pinned images, accepted candidate source parity, independently proven rollback compatibility, a controller lock, and explicit release authority.

The only production mutation in the cutover plan is the existing Compose project's `bus` service using `--project-directory .`, `--project-name docker`, the exact `Docker/docker-compose.yml`, `--no-deps`, `--no-build`, and `--scale bus=1`. It preserves loopback `127.0.0.1:18787`, `docker_bus-data`, dashboard/cursor secret continuity, and all claims/leases. It never selects `0.7.3` and never changes `db` or `nats`.

## Gates and phases

1. **Before-schema preflight:** verify the existing Compose project/service, loopback port, volume identity, health, database readiness, existing production migration readback, secret-preservation booleans, one-stack invariant, and candidate/rollback image digest identity. Existing production v19 is accepted as the before state; it is not confused with the candidate schema.
2. **Candidate/rollback schema gate:** require candidate migration v21, rollback migration at least v21, accepted candidate source revision, and an independent `rollbackCompatible=true` readback. A fallback source such as `8897d7a` is allowed only with that compatibility receipt; it is not silently treated as the candidate source.
3. **Backup/catalog:** create a custom `pg_dump` in the DB container, copy it to a durable host path, hash it with Windows-compatible `certutil -hashfile`, copy the durable artifact back into the DB container, run container-side `pg_restore --list`, capture task count, and re-read volume identity. No host `sha256sum` or host `pg_restore` dependency is used.
4. **Canary:** require an explicitly supplied isolated migrated-v21 clone database capability, attach the clone to a private internal network, run the candidate detached with no host port and `BUS_PUBLICATION_WORKER=false`, and use env-file references without exposing credential values.
5. **Proof:** invoke the actual `/usr/local/bin/ai-crew-sync` executable with positional syntax `client --url http://127.0.0.1:8787/mcp --token <runner-resolved-ephemeral-token> --json tasks --all-pages`, plus the 4,053 search traversal. The runner receives only a token environment reference; token values never enter argv or evidence. Structured proof readback must establish MCP authentication/tools/schema cursor, all-pages uniqueness/exhaustion, expected unauthorized behavior without a token, and ephemeral-token revocation. Unsupported bare `client`, invented `ai-crew-sync task list`, and curl proof commands are not used.
6. **Rollback/readback:** use the exact Compose project and immutable migration-compatible rollback digest, bus-only `--no-deps --no-build --scale bus=1`, health readback, and the same authenticated `/usr/local/bin/ai-crew-sync client ... tasks --all-pages` MCP readback.
7. **Cutover/readback:** only after preflight, proof, rollback readiness, controller lock, and release authority pass, execute the bus-only candidate replacement and perform health plus authenticated client task readback. Cleanup removes only the disposable canary and private network.

## Deterministic tests

`Docker/local-mcp-bluegreen.test.mjs` covers:

- safe dry-run with zero runner calls;
- immutable digest, source parity, before/after schema, rollback compatibility, loopback, volume, health, auth, and duplicate-stack failures;
- Windows-safe durable backup/copy/hash/container restore-catalog/task-count commands;
- normalized default private-network expectation, detached clone-connected canary, no host port, no duplicate publication worker, and no credential values;
- actual `client tasks --all-pages` proof shape with cursor/schema, 4,053, negative-auth, uniqueness, exhaustion, and revocation evidence;
- missing clone capability before any execute side effect;
- execute-mode backup/canary/proof/cleanup without authority;
- authorized bus-only Compose cutover and post-cutover MCP readback;
- exact project, one-replica rollback, shell-free argv, and rejection of `0.7.3`.

Required validation:

```text
node --test Docker/local-mcp-bluegreen.test.mjs
node --check Docker/local-mcp-bluegreen.mjs
git diff --check
```

## Validation evidence

The final scoped `go_wave.run_node_tests` invocations for `Docker/local-mcp-bluegreen.test.mjs` and `Docker/local-mcp-bluegreen.mjs` both returned `GO_WAVE_NODE_ERROR:test_file_not_allowed`; no local PASS/GREEN claim is made. `git diff --check` is unavailable through the granted scoped interfaces. Static readback confirms the canary expectation uses `DEFAULTS.canaryNetwork`, proof commands use `client tasks --all-pages`, production/candidate schema gates are separate, backup verification is Windows-compatible, and all changes remain in the exact write scope.

## Remaining blockers

- Independent QA must execute the full suite and syntax check with an allowed Node/git runner and replay the dry-run/backup/canary/rollback gates against disposable migrated-v21 clone infrastructure.
- Coordinator/release authority must provide real structured preflight/proof readbacks, accepted candidate source revision, immutable image identity, durable backup receipt, rollback compatibility receipt, controller-lock evidence, and release authorization before any live cutover.
- This worker has not executed production mutation and claims neither acceptance nor release readiness.
