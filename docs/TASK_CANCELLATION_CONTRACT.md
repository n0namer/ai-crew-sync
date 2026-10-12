# CrewSync task cancellation contract (candidate)

## Purpose

Preserve every original task ID, history event, dependency, and acceptance receipt. Never use `complete_task` to suppress a duplicate, and never physically delete historical work.

## MCP operation

`cancel_task({ key, reason, successor_key?, evidence_refs })`

Reasons: `DUPLICATE`, `SUPERSEDED`, `OBSOLETE`, `INVALID`.

`DUPLICATE` and `SUPERSEDED` require an existing, non-cancelled canonical `successor_key`. Every call requires 1–16 bounded evidence references.

## Transactional safety

- Check team-scoped original key, active claim and lease, and successor identity.
- Refuse cancellation with any nonterminal downstream dependent. Existing claim logic considers cancelled dependencies fulfilled, so this is a required fail-closed gate.
- Serialize with claim/complete through a PostgreSQL row lock and session guard.
- Record `status=cancelled`, `metadata.lifecycle_resolution` and a `task_events.cancelled` event in the same transaction.
- Leave original task ID, description and all links intact. A duplicate transport replay should read back the existing resolution; it must not overwrite it.

## Backlog workflow

1. Read complete paginated inventory; deduplicate semantically against Git, tests, QA, receipts, and dependencies.
2. Four independent read-only domain reviewers issue proposed decisions keyed by exact task IDs.
3. One reducer reconciles conflicts, verifies evidence and requires human/operator admission for writes.
4. Update canonical Goal Owner requirements separately; tasks with unfinished dependents stay BLOCKED until dependencies are reconciled.
5. Cancel selected tasks one by one with exact readback; never `complete_task` duplicates.

## Release gate

This is candidate source only. Required before production: Rust compile and unit tests, independent review, PostgreSQL integration tests for CAS/lease/dependency races, verified API tool schema, live harmless canary with exact readback, then guarded rollout. No mass cancellation before acceptance.
