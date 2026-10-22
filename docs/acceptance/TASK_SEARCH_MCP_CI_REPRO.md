# PR #202 Task Search MCP Clean-Checkout Reproduction

**Document status:** `CANDIDATE / UNACCEPTED`
**Project:** `crewsync`
**Purpose:** reproducibly validate Draft PR #202 from a clean checkout without converting partial, indirect, or skipped evidence into `GREEN`.
**Scope:** release-validation instructions and evidence matrix only. This document does not change product code, tests, migrations, recipes, CI, deployment, or CrewSync task state.

## Release boundary

The coordinator must substitute the exact PR #202 head commit in every command below. Do not use a moving branch name as the evidence identity. Every receipt must record:

- PR number and exact source commit (`git rev-parse HEAD`);
- Rust toolchain and PostgreSQL image/version;
- the exact command, exit code, start/end time, and artifact path;
- fixture size and migration state;
- test outcome and any skipped/ignored count.

A worker report, `turn.completed`, a passing direct-store test, a passing CLI-only test, or a green GitHub job is not release acceptance. The coordinator and independent QA own acceptance. No production database, production token, credential, secret, deployment, merge, or push is permitted during this run.

## Clean checkout

Use a disposable directory and a disposable local evidence directory outside the repository. The operator must obtain the PR head SHA from the PR/coordinator, then verify it before running tests:

```text
git clone <repository> crewsync-pr-202
cd crewsync-pr-202
git fetch origin pull/202/head:refs/remotes/origin/pr-202
git checkout --detach <EXACT_PR_202_HEAD_SHA>
git rev-parse HEAD
git status --short
git diff --exit-code
```

Expected result: `HEAD` equals the coordinator-supplied PR #202 SHA and the working tree is clean. If the SHA is unavailable, differs, or the checkout contains unrelated changes, stop with `PENDING`; never silently substitute another revision.

Record the source identity before and after every gate:

```text
git rev-parse HEAD
git show -s --format='%H%n%ad%n%s' --date=iso-strict HEAD
rustc --version
cargo --version
```

Do not copy `.env`, `.env.*` backups, bearer tokens, database URLs containing credentials, NATS credentials, or local plugin changes into the evidence directory. Use a local-only test password and redact it from logs.

## Baseline gates

Run the formatting and compile gates from the clean checkout. These are necessary but are not MCP runtime proof:

```text
cargo fmt --all -- --check
cargo check --tests
cargo test --no-run
```

For CI-equivalent coverage, also run the repository's declared stable/MSRV checks in disposable infrastructure. The workflow currently defines stable format/clippy/tests and an MSRV `1.98.1` build/tests job. Record both toolchains if both are exercised:

```text
cargo clippy --all-targets -- -D warnings
cargo test
```

The repository CI sets `TEST_DATABASE_URL`, `AI_CREW_SYNC_REQUIRE_DB=1`, `AI_CREW_SYNC_REQUIRE_NATS=1`, and `TEST_NATS_URL`. Reproduce those requirements only with local disposable services. A command that reports zero integration tests, skips an ignored test, or cannot reach its dependency is not a pass.

## Disposable PostgreSQL sandbox

Use a fresh, isolated PostgreSQL instance for this run. The sandbox must not point at a shared, staging, or production database. Apply the repository migrations through the built application or the test fixture, then record the PostgreSQL version, migration success, and fixture row count without recording the connection string.

The MCP search tests require the large-fixture opt-in and a disposable `TEST_DATABASE_URL`:

```text
CREWSYNC_ENABLE_LARGE_FIXTURE=1
TEST_DATABASE_URL=postgres://<local-test-user>:<local-test-password>@127.0.0.1:<random-port>/<local-test-db>
AI_CREW_SYNC_REQUIRE_DB=1
```

Use a random loopback port, a unique database/container name, and a cleanup trap or equivalent lifecycle guard. Verify readiness before starting the server. At teardown, stop and remove the database and retain only redacted logs and summary metadata. If readiness, migration, cleanup, or fixture seeding fails, mark every dependent row `BLOCKED` or `FAIL`; do not call it `PASS`.

The release run must prove the fixture is large enough for pagination (at least 2,000 tasks), includes title-only and description-only matches, wildcard literals, multiple keywords, quoted phrases, English and Cyrillic data, identical timestamps/ties, foreign-tenant data, malformed and pathological regex inputs, and a no-trigram regex case. The fixture must be static while the duplicate/omission traversal runs.

## Build and run the actual server

The six existing Rust MCP test files are useful supporting coverage, but their current implementations construct an in-process Axum router and are `#[ignore]`-gated. They do not by themselves prove binary startup, migration/bootstrap, listener binding, health, or clean shutdown. Release evidence must therefore start the binary built from the exact checkout:

```text
cargo build --bin ai-crew-sync
DATABASE_URL=<redacted-local-value> \
BUS_BIND=127.0.0.1:<free-port> \
BUS_ALLOWED_HOSTS=127.0.0.1 \
BUS_AUTO_MIGRATE=true \
BUS_DASHBOARD_SECRET=<ephemeral-local-secret> \
./target/debug/ai-crew-sync serve
```

Use an ephemeral token issued only in the disposable database. Probe `/health` and record the HTTP status and redacted JSON body. Do not treat process existence, a listening socket, or a `200` health response without a database check as readiness. Capture startup logs, binary path/hash, health response, authenticated MCP initialize response, `tools/list`, and at least one `tools/call`; redact authorization values and database URLs.

The production-shaped MCP proof must use the streamable HTTP endpoint `/mcp`, a real MCP client, and the running binary, not a direct Rust function or a router-only shortcut. Verify graceful shutdown, restart, and a second authenticated session. If the harness cannot start the real binary or preserve raw request/response receipts, the runtime rows remain `PENDING`.

## Seven MCP E2E receipts

Run at least these seven independent scenarios against the binary and the same disposable PostgreSQL source revision. Each scenario must retain a redacted wire receipt with HTTP status, JSON-RPC method/id, result or error class, fixture/source identity, and exit code. Do not retain tokens, passwords, connection strings, or credentials.

| ID | MCP scenario | Required proof | Exact pass condition |
|---|---|---|---|
| E2E-01 | Wire search modes | Authenticated `initialize`, `tools/list`, and `tools/call` for `list_tasks` using keywords, literal contains, and case-insensitive regex | Calls reach PostgreSQL through the running binary; title/description hits and response shape match the contract; no second `search_tasks` tool is used |
| E2E-02 | Security and cursor binding | Unauthenticated/invalid-token call, foreign-team call, malformed/oversized/pathological/no-trigram regex, and cursor replay with changed query/mode/fields/language | Auth is rejected safely; no foreign task is observable; invalid input is classified safely and bounded; altered or tampered cursor is rejected |
| E2E-03 | 2,000-row pagination | Seed at least 2,000 tasks, traverse every MCP page, repeat a cursor, and include status/mine/project-prefix filters and tied timestamps | Every expected task appears once, no gaps or duplicates occur, `has_more` and terminal cursor are correct, and numeric totals remain team-wide |
| E2E-04 | Schema and legacy compatibility | Compare `tools/list` schema with the frozen contract and send an old `list_tasks` payload omitting all search fields | Search fields are additive and bounded; unknown/invalid options return the documented invalid-params error; old requests preserve no-search behavior and totals |
| E2E-05 | Existing CLI/client over MCP | Run the existing client/CLI against the same binary for omitted search fields, populated search fields, cursor continuation, and all-pages behavior | Existing client behavior and cursor metadata remain compatible; this is additional evidence and never substitutes for E2E-01 wire proof |
| E2E-06 | Binary lifecycle and recovery | Start, health-check, authenticate, call, gracefully stop, restart, and call again; if two replicas are available, alternate requests between them | Readiness follows dependency state, restart does not leak or duplicate session state, and both replicas accept the same stateless authenticated search behavior |
| E2E-07 | Read-only and dependency safety | Run concurrent authenticated searches while checking that task claims/leases and unrelated broker/outbox state do not change; exercise database readiness failure in a disposable instance | Search remains read-only, concurrent calls complete within the recorded budget, and dependency failure is visible as a failure/blocker rather than a false successful test |

The existing supporting Rust paths are:

```text
tests/task_search_mcp_transport.rs
tests/task_search_mcp_security.rs
tests/task_search_mcp_pagination.rs
tests/task_search_mcp_schema.rs
tests/task_search_cli_transport.rs
tests/task_search_mcp_server_canary.rs
```

Run them explicitly with ignored tests only after the sandbox is ready, and preserve the full output. The command must fail if the test is missing, filtered out, or skipped:

```text
cargo test --test task_search_mcp_transport -- --ignored --nocapture
cargo test --test task_search_mcp_security -- --ignored --nocapture
cargo test --test task_search_mcp_pagination -- --ignored --nocapture
cargo test --test task_search_mcp_schema -- --ignored --nocapture
cargo test --test task_search_cli_transport -- --ignored --nocapture
cargo test --test task_search_mcp_server_canary -- --ignored --nocapture
```

These six commands are supporting receipts unless the harness demonstrably starts the production binary and records the wire boundary. In-process router success must not be relabeled as binary success.

## Recipe drift and CI absence

Recipes are source prose; generated Claude commands must remain synchronized. From the clean checkout, run:

```text
python3 plugin/scripts/recipes.py check
make recipes-check
```

A drift failure is a release blocker. Do not regenerate files as part of this validation, and do not include unrelated local recipe or plugin changes in PR #202.

Inspect `.github/workflows/ci.yml` at the exact PR commit and retain its relevant job excerpt or a checksum. The current workflow runs format, clippy, ordinary `cargo test`, and an MSRV build/test job, but the six MCP tests are ignored and are not explicitly invoked with `--ignored`; the workflow also has no seven-scenario binary/raw-wire MCP harness. Therefore:

- a green CI job does not prove the six MCP E2E paths or E2E-01 through E2E-07;
- CI absence is a documented gap unless PR #202 adds and independently passes an explicit harness;
- any missing job, missing dependency service, filtered test, or ignored test is `PENDING`, never `GREEN`;
- the clean-checkout run must report this gap even when `cargo fmt`, `cargo check`, clippy, and ordinary tests pass.

## Evidence matrix

Store receipts under a coordinator-selected, source-keyed directory such as `artifacts/pr-202/<commit-sha>/`. The paths below are required evidence classes, not claims that artifacts already exist.

| Gate | Required artifact | Status rule |
|---|---|---|
| Source identity | Commit SHA, clean-tree output, toolchain versions | `PASS` only when exact PR #202 SHA is verified |
| Formatting | `cargo fmt --all -- --check` output | `PASS` on zero diff and zero exit code |
| Compile | `cargo check --tests` and `cargo test --no-run` output | `PASS` only when all test targets compile |
| Disposable DB | Redacted PostgreSQL version, readiness, migration, fixture count, cleanup receipt | `PASS` only with isolated DB and complete fixture |
| E2E-01 wire search | Raw/redacted initialize, tools/list, and search tools/call receipts | `PASS` only against the real binary and MCP endpoint |
| E2E-02 security | Auth-negative, ACL, regex-bound, and cursor-binding receipts | `PASS` only when negative results are observed at the MCP boundary |
| E2E-03 pagination | 2,000-row traversal summary plus page/cursor digest | `PASS` only with zero omissions/duplicates and correct terminal state |
| E2E-04 schema | Redacted schema and legacy request/response receipts | `PASS` only when additive schema and old payload behavior are verified |
| E2E-05 CLI | Client command output tied to the same server/source | Supporting `PASS`; never replaces E2E-01 |
| E2E-06 lifecycle | Binary hash/path, health, startup, restart, shutdown, and optional replica receipts | `PASS` only with clean lifecycle; router-only evidence is `PENDING` |
| E2E-07 safety | Concurrent/read-only and dependency-failure receipts | `PASS` only when no unrelated state changes and failures are visible |
| Recipe consistency | `recipes.py check` and `make recipes-check` output | `PASS` only with zero drift |
| CI parity | Exact workflow inspection plus stable/MSRV command receipts | `PASS` only for checks actually run; ignored MCP gaps stay `PENDING` |
| Independent QA | Reviewer readback of source identity, redaction, wire boundary, negatives, and blockers | Required before any coordinator acceptance review |
| Release reducer | Same-source matrix with no missing mandatory rows | Any `FAIL` means repair; any `PENDING` means do not promote |

## No-false-GREEN policy

Use only these dispositions:

- `PASS`: exact command or wire receipt exists, is reproducible, is tied to the same source and disposable environment, and has no skip/filter ambiguity.
- `FAIL`: the required behavior or gate fails. Route only the affected repair; do not waive it in this document.
- `BLOCKED`: the required runner, binary, dependency, or receipt is unavailable. A blocked runner is not a pass.
- `PENDING`: evidence is missing, indirect, stale, coordinator-only, router-only, or awaiting independent QA.

The overall release decision is `DO NOT RELEASE` while any mandatory row is `FAIL`, `BLOCKED`, or `PENDING`. In particular, `run_node_tests` cannot execute Rust `.rs` integration tests; its typed `test_file_not_allowed` result is runner evidence, not a test pass. Likewise, an ignored test that was not explicitly run, a direct-store assertion, a CLI-only assertion, a source review, or a CI job that omits the MCP harness cannot produce `GREEN`.

## Handoff

The coordinator should attach the exact evidence paths and preserve all negative results, then obtain independent QA readback. Any failure belongs to the original affected owner or an explicitly scoped repair owner. This document remains `CANDIDATE / UNACCEPTED`; it grants no release, merge, deployment, task completion, or production-readiness authority.
