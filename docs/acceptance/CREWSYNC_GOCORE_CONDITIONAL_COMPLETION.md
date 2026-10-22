# GO Core conditional completion — fail-closed implementation and handoff

Date: 2026-10-14. Status: **BLOCKED / UNACCEPTED**, not production-ready.
Original task: `crewsync:p0-checked-complete-go-core-20261014-r3`.
Attempt: `attempt-f5ab6006-4bef-4a87-8193-4beb3dc9f688`.
Crew session: `manual-20261014-r44-crewsync-fenced-eight-v2-r1-c96ce18aa382bc89`.
Target: `D:/Users/NIKITA/Documents/DEV/worktrees/ai-crew-sync-cancel-supersede-20261012`.
Launcher revision: `34a2c3ea2e3eabfed22fe842863f5d3d43590fa3` (supplied, not independently read back).
Manifest SHA-256: `b0429565c10d6e382e67c06f2776f448cf7f3eadd20070c05b13246d43914cfa`.

## Source of truth and current gate

The governing existing evidence is `CREWSYNC_LIFECYCLE_RUNTIME_R6.md` in this directory. Its lifecycle results are historical evidence for a different candidate, not tests of this change. Its decision boundary requires independent reviewer/reducer binding to current sources. This document is the explicitly authorized conditional-completion extension, not a replacement tracker or a new sprint plan.

FACT: Authorized physical source shows legacy `complete_task` accepts OPEN or expired claims, and checks neither authoritative source/result SHA nor independent QA. `AuthCtx.session` can be a caller-controlled header; authenticated session UUID and epoch are separate optional fields. Legacy completion remains unchanged.

FACT: The authorized schema/source evidence provides no trusted independent QA issuer, immutable attempt/source manifest, or reducer capability contract suitable for authorizing DONE. The caller's note key/hash, original attempt/session strings and owner UUID/epoch are assertions, not proof. No available evidence justifies enabling a successful checked completion.

HYPOTHESIS: Future activation can reuse the existing session guard, task row lock, generation fence and DONE-only dependency predicate after independently issued persisted proof and actor authority are implemented and reviewed. This is not verified runtime behavior.

BMad route: freeform implementation, Quick Dev, under the existing technical-candidate/independent-QA gate. Canonical BMAD-MNNZ is the workflow source, not the target Git root. Scoped runtime exposes no graph/Graft tools, shell/git inspection, configuration resolver, Rust compiler runner or independent agents. Those method steps are BLOCKED, not claimed executed. Quick Dev's extra spec files, interactive checkpoints, sprint writes and review-agent launch are superseded by the exact autonomous launch contract and write scope. Systematic-debugging, edge-case-hunter and verification-before-completion instructions were loaded. Tests were written before the store change, but the red/green TDD cycle could not run. No standalone TDD skill is in the reviewed available catalog.

Planned useful effort: I=5, C=12, T=8, R=4, V=4, E=4 minutes; W=37 minutes, coordination H approximately 2 minutes. This is an estimate, not elapsed-time evidence or a waiting requirement.

## Implemented bounded behavior

- `CompleteTaskCheckedInput` is a required-field, unknown-field-denying DTO shared by MCP and store. There is no `qa=true` or free-text acceptance channel.
- Request validation preserves the exact original key and result bytes; bounds identifiers/results; rejects malformed SHA-256, nil identities, nonpositive epoch/generation and declared self-review. Result SHA-256 is recomputed over exact UTF-8 bytes, with no trimming or JSON canonicalization.
- `complete_task_checked` is additive to the existing MCP tasks router. Its response is tagged `rejected` or `unsupported_proof`; **there is deliberately no successful DONE variant or mutation branch**.
- Store preflight binds the reducer actor assertion to authenticated `AuthCtx.agent_id`, runs the existing transactional session guard, locks only the original same-team task row, and checks persisted owner agent/session, status, non-NULL unexpired lease and positive generation when available. Lease time is read with `clock_timestamp()` after row-lock acquisition, not transaction-start time. Only DONE satisfies dependencies.
- Unsupported preflight rolls back and names missing `attempt_binding`, `source_manifest`, `owner_session_generation`, `independent_review`, and `reducer_authority`; missing authenticated reducer session/epoch and missing/zero claim generation are also reported. Caller-supplied hashes and descriptions cannot remove these missing-proof reasons.
- Migration absence is supported: `to_jsonb(t)->>'completion_generation'` yields NULL without referencing a missing SQL column. No automatic legacy fallback occurs. Missing/zero generation is unsupported proof, never a usable ownership token.
- Client mapping adds an internal `checked_completion_call` helper that validates and serializes the full DTO without compaction or fallback. The existing CLI enum is outside write scope; **no new CLI subcommand or public re-export is claimed**.

## C4 ownership and security boundary

Context: GO Core implementer produces a candidate; an independent reviewer and authorized reducer, not the implementer, own acceptance. CrewSync persists coordination state. None of these actors may derive acceptance from process exit, a tool response or candidate prose.

Containers: MCP/client Rust process -> existing authenticated tool router -> transactional PostgreSQL store. There is no new daemon, scheduler, queue, claim issuer or external service.

Components/signatures:

| Owner/file | Additive contract |
|---|---|
| `src/store/tasks.rs` | `validate_checked_completion(&CompleteTaskCheckedInput) -> BusResult<()>` |
| `src/store/tasks.rs` | `complete_task_checked(&PgPool, &AuthCtx, &CompleteTaskCheckedInput) -> BusResult<CheckedCompletionOutcome>` |
| `src/tools/tasks.rs` | `CompleteTaskCheckedArgs` alias and `complete_task_checked` MCP handler |
| `src/client/mapping.rs` | `checked_completion_call(&CompleteTaskCheckedInput) -> anyhow::Result<(&'static str, Value)>` |
| `migrations/0024_checked_completion_audit.sql` | Monotonic claim-generation groundwork and reserved immutable acceptance audit |

The DTO's Crew session ID is an exact original-task binding, not an authentication credential or the owner's <=64-byte session label. Owner session UUID/epoch must eventually be proven from an authoritative persisted claim binding. Matching a label alone is explicitly insufficient. Matching `reducer_actor_id` to the bearer identity proves who calls, not permission to reduce.

Threat model: caller forges QA, source SHA, attempt/session, note key, owner UUID, reviewer label or reducer role; stale/replaced session; expired/NULL lease; foreign tenant; stale generation; concurrent completion/retry/replay. Guards either reject the preflight or return explicit unsupported proof. All paths leave task/result untouched. Database administrators, migration-role compromise and disabled triggers are outside this application's immutable-audit trust boundary. No credentials, production mutations, live Native task changes or outbound actions occur in this implementation.

## Migration 0024 receipt and limits

Migration 0024 is written, **not applied or tested here**. It does not modify migration 0022 or reserve/use 0023.

- Adds `tasks.completion_generation`, initially zero. Existing claims are intentionally untrusted until a fresh acquisition changes claim identity. A trigger increments on status/owner/session/acquisition-time change and preserves the generation for lease-only renewal. Direct assignment cannot reset the token. BIGINT overflow fails the transaction rather than recycling a token.
- Creates `task_checked_completion_audit` with original task/team/key composite FK, unique task acceptance, owner/reviewer/reducer identities, original attempt/Crew session and source/result/reviewer hashes.
- Rejects audit UPDATE, DELETE and TRUNCATE through triggers; PUBLIC privileges are revoked. This is not a substitute for configuring a least-privilege application role. The migration-owning database role remains privileged.
- The audit table is **not authoritative QA proof**, is never read to authorize checked completion, and this implementation never inserts an accepted audit row. It remains empty on all checked calls. An audit-shaped row manually inserted by a database owner would not unlock completion.
- Application authorization, same-team actor/session binding and independent issuance must precede future accepted audit insertion. The task FK alone does not prove an actor belongs to that team.

## Precise minimum activation change — intentionally not enabled

1. Establish immutable server-owned task-attempt/source binding: `(team_id, task_id, exact_key, attempt_id, crew_session_id, source_sha256, owner_agent_id, authenticated_owner_session_id, owner_epoch, owner_generation)`. It must be issued through authenticated claim/coordinator authority, not task metadata or request parameters. Define how authorized callers obtain their current generation without changing legacy DTOs.
2. Add an independently issued append-only reviewer evidence record: exact binding above plus reviewer agent/session UUID/epoch, immutable note key, exact note bytes/hash, reviewed result bytes/hash and review decision. Enforce a different reviewer agent and session, same tenant, current authenticated reviewer credentials, and immutable evidence. A caller-written note or plain description is not sufficient.
3. Persist an explicit reducer capability tied to team, authenticated actor/session UUID/epoch and the exact task-attempt scope. Do not derive it from a caller label, heartbeat role or equality with `reducer_actor_id`.
4. Define privileged proof issuer and runtime database roles with INSERT/UPDATE/DELETE/TRUNCATE separation. Review the issuance code and real grants before any completion path consumes this proof. The trusted actor/capability contract is currently missing; arbitrary new role names or fake proof producers are not introduced here.
5. In one transaction: existing session guard, row lock, lock/read immutable binding and reviewer evidence, validate all original IDs/hashes/capabilities and authenticated ownership, then perform a same-row conditional UPDATE with `team_id`, `id`, exact key, `status='claimed'`, owner/session, generation, **`lease_expires_at > clock_timestamp()` at mutation time**, and DONE-only prerequisites. Insert the immutable acceptance audit and lifecycle event in that same transaction. Reject zero updated rows. Never call legacy completion.
6. Define lost-response retry behavior from the immutable acceptance receipt: identical task-attempt/fence/source/result/QA/reducer binding may read back that receipt; changed binding must conflict. No fresh acceptance based on status DONE alone. Test crash before/after commit and concurrent reduction with real PostgreSQL.

The successful same-row DONE update, independent proof ingestion, reducer capability enforcement and accepted audit insertion are **remaining work**, not hidden behind a boolean flag or presented as implemented.

## Verification receipts

| Actual authorized invocation | Actual output | Classification |
|---|---|---|
| `go_wave.run_project_tests({"file":"tests/task_conditional_complete.rs"})` | `GO_WAVE_NODE_ERROR:test_file_not_allowed` | INFRA BLOCKED; Rust engine did not execute |
| `go_wave.run_project_tests({"file":"tests/task_cancel_supersede.rs"})` | `GO_WAVE_NODE_ERROR:test_file_not_allowed` | INFRA BLOCKED; regression engine did not execute |

No identical denied call was retried. No shell fallback, installation or alternate runner was attempted. **0 executed Rust tests; no PASS/FAIL test counts are available.** Compiler, PostgreSQL, formatting, lint, git diff/status and restart/replay runtime evidence remain unavailable. Source edit receipts are evidence of scoped writes, not evidence of compilation.

Prepared tests: seven non-ignored request/serialization tests and one gated PostgreSQL integration test in `tests/task_conditional_complete.rs`; one client mapping unit test and one legacy DTO unit test in their respective source modules. PostgreSQL fixture uses a random isolated schema, requires explicit enablement, accepts only database names `crew_fixture_test` or `crewsync_conditional_test`, and does not use `DATABASE_URL`. It checks pre/post-migration fail-closed responses, concurrent retries, exact generation, lease-only renewal, wrong owner/session/reducer, tenant isolation, NULL/expired leases and cancelled-versus-DONE dependencies. These tests have not run; the fixture is not a claim of independent QA or full-production-schema coverage.

Bounded self-review: traced the newly added validation/store response paths and migration guards. Repaired the migration-zero fence interpretation and narrowed the fixture state query to the exact original task after adding a prerequisite. Legacy store completion and legacy client command mapping blocks were not replaced. This is implementer self-review, not an independent reviewer verdict. Audit immutability, actual authenticated session renewal and successful/replayed accepted completion still require independent runtime tests.

## Independent host QA and next action

Coordinator must authorize a host Rust runner on this exact attempt/worktree; do not repeat the denied scoped call or substitute a new task/session. Required checks (not executed here):

```text
cargo test --locked --offline --test task_conditional_complete
cargo test --locked --offline --test task_cancel_supersede
cargo test --locked --offline --lib checked_completion
cargo test --locked --offline --lib legacy_completion_dto_remains_key_and_optional_result
cargo fmt --all -- --check
cargo check --locked --offline --all-targets
```

To execute the new database test, set `CREWSYNC_ENABLE_CONDITIONAL_FIXTURE=1` and `TEST_DATABASE_URL` to an explicitly disposable permitted-name database, then run:

```text
cargo test --locked --offline --test task_conditional_complete -- --ignored
```

The existing lifecycle regression's ignored PostgreSQL tests additionally require their documented `CREWSYNC_ENABLE_LARGE_FIXTURE=1` disposable fixture setup and `-- --ignored`; merely running its default test command is not PostgreSQL evidence.

Independent QA must bind final source digests, migration receipt and actual MCP schema/tool discovery to the original task/attempt. Verify legacy OPEN/expired completion remains compatible; checked completion never reaches DONE even with forged metadata/note/hash; generation cannot reset on renewal; authenticated stale-epoch requests cannot bypass the existing session guard. Audit immutability and same-tenant actor/session constraints require dedicated real-database checks before activation. Future successful completion needs separate positive, concurrent, crash/lost-response/replay and full-schema compatibility evidence after the trusted proof mechanism exists.

Publication is a single typed original-task BLOCKED/UNACCEPTED report because required Rust/PostgreSQL execution is denied. Even VERIFIED publication transport is not acceptance, DONE or production readiness. Coordinator/reviewer alone own those gates.
