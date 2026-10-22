# VP49 CrewSync fork — independent local QA (2026-10-22)

## Release identity

- Canonical source: `n0namer/ai-crew-sync`, `main` at `7b3f627`.
- Live container: `docker-bus-1`, image `ai-crew-sync:n0namer-ed97381`, exposed on loopback `http://127.0.0.1:18787/mcp`.
- The `7b3f627` commit changes only CI/Makefile/fixtures and tests; no runtime `src/` files have changed since `ed97381`. Thus the deployed image uses the final tested runtime code.
- PostgreSQL: original `docker-db-1` remains running with migration version 21; no volume replacement.
- Rollback images: `ai-crew-sync:n0namer-45e33ed` and upstream `ghcr.io/joaquinbejar/ai-crew-sync:0.7.3`.

## Source tests

- Rust 1.98.1 `cargo fmt --check`: PASS at `7b3f627`.
- `cargo clippy --offline --all-targets -- -D warnings`: PASS at `7b3f627` (Docker runner `crew-fork-lint-final`, exit 0).
- Normal full `cargo test` against disposable PostgreSQL + shared 8 GiB JetStream: **320 PASS / 0 FAIL**, 23 intentionally ignored E2Es; Docker runner `crew-fork-full-ci-final`, exit 0.
- Explicit `--ignored` real MCP/2000-task/search/security/fixture tests: **21 PASS / 0 FAIL**; `CrewSyncRecovery/ci-optin-20261022.log`, runner `crew-fork-ci-optin-verification`, exit 0.
- Two quota regressions on isolated 512 MiB JetStream: **2 PASS / 0 FAIL** (`crew-fork-ci-quotas-audit`, `crew-fork-quota-verified`); the final CI routes them to their own broker, so they are not silently waived.
- Combined **343 passing test cases** across these complementary suites; do not present them as a single unpartitioned run.
- Earlier local failures were diagnosed as runner dependencies (python3/git/curl/jq) and a fixture mismatch (512 MiB quota assertions against 8 GiB broker). These were reproduced and repaired via runner setup and dedicated quota fixture, not by disabling safety checks.

## Physical live operator E2E (local VP49 server)

- `/health` returned HTTP 200 before and after container restart.
- All four roles (`coordinator`, `worker-1/2/3`) authenticated using local protected client credentials; `whoami` successful.
- Original task claims are session-bound. Dependency B rejected before A completed (`claimed:false`); second worker denied duplicate A claim (`claimed:false`); after authoritative server DONE for A, worker-2 claimed and completed B. `tasks --all-pages` returned task records.
- Notes: worker-1 created and revised a scoped note, worker-2 observed both versions; note removed.
- Locks: worker-1 acquired, worker-2 denied while held, worker-1 released, worker-2 acquired and released. No lock left held.
- Direct message: worker-1 sent to worker-2; worker-2 read it, worker-3 could not see it.
- After upgrading to the new image and after Docker restart, existing QA A/B tasks remained server `done`, confirming PostgreSQL persistence.
- Task contains search and cursor-paginated inventory are functional on deployed fork.

## Limitations and operations

- Previous A55 task database is intentionally **not** restored. This QA verifies source code and current VP49 functional behavior, not historical task restoration.
- Production `NATS_REPLICAS=0`; JetStream functionality was exercised with separate real NATS containers, not a routed production team.
- The CLI often returns process exit 0 on a legitimate claim refusal. Always check the returned `claimed` boolean; do not infer acceptance from exit code alone.
- Secrets are stored only under local Git-ignored `.env` and protected `~/.mnnz/crew-sync-local.env`; do not publish backups or bearer tokens.
- See `docs/operations/VP49_CREWSYNC_FORK_DEPLOYMENT_20261022.md` for Compose commands and rollback.
