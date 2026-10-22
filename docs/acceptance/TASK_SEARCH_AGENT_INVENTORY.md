# Agent Task Inventory Contract

Status: candidate implementation evidence, not production acceptance.

## Machine-readable contract

The agent-facing helper is `plugin/scripts/crew-task-inventory.mjs`.

- `createTaskInventoryClient({ endpoint, token })` performs MCP initialization and exposes `inventory(options)`.
- `collectTaskInventory({ callTool, discover, filters, pageSize })` is transport-independent for other LLM agents and tests.
- Supported filters are `status`, `mine_only`, `project_prefix`, `search`, `search_mode`, `search_fields`, and `search_language`.
- `search_mode` is `keywords`, `contains`, or `regex`; fields are `title`, `description`, or `both`.
- The result declares `consistency: "paged_best_effort"`; no stable snapshot is implied by pagination.
- `capabilities` reports MCP protocol version, server identity, cursor support, and the list-task capability; top-level `tools`/`serverInfo` are preserved even when initialize also contains an empty nested `capabilities` object.
- The transport accepts JSON or SSE responses, forwards bearer authorization and the negotiated `Mcp-Session-Id`, and reads `tools/call` structured content.

## Safety rules

A complete inventory requires every page to provide boolean `has_more` and, when true, a non-empty `next_cursor`. A server that omits cursor fields (including legacy 0.7.3 behavior) fails with `INCOMPLETE_INVENTORY`; the helper never treats the first 50 rows as complete. Replayed cursors, filter-bound cursor rejection (including typed `CURSOR_CONTEXT_MISMATCH`), malformed pages, duplicate task keys, and configured page/task bounds fail explicitly.

The Rust CLI follows the same rule for `client tasks --all-pages`: an old server or malformed continuation returns an `INCOMPLETE_INVENTORY`/`INVALID_CURSOR` error rather than a successful partial result. Non-`--all-pages` calls remain page-oriented and expose `incomplete`, `exhausted`, `has_more`, and `next_cursor` metadata.

## Evidence

- `node --test plugin/scripts/crew-task-inventory.test.mjs` covers old-server incomplete behavior, a 4,053-task traversal with no duplicates, filter preservation, cursor replay, duplicate rejection, and capability/version discovery.
- `node --check plugin/scripts/crew-task-inventory.mjs` validates the helper syntax.
- `cargo test --test task_search_cli_transport -- --ignored` remains the real authenticated MCP transport check and uses the existing disposable database gate.

This document records the agent contract only. It does not claim deployment, production mutation, or independent acceptance.
