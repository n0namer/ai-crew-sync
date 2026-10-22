import test from "node:test";
import assert from "node:assert/strict";
import { createTaskInventoryClient, InventoryError, collectTaskInventory, discoverCapabilities } from "./crew-task-inventory.mjs";

function page(tasks, has_more, next_cursor = null) {
  return { tasks, open: tasks.length, claimed: 0, has_more, next_cursor };
}

const task = (key) => ({ key, title: key, description: null });

test("old MCP without cursor fields fails explicitly instead of returning 50 rows", async () => {
  await assert.rejects(
    () => collectTaskInventory({ callTool: async () => ({ tasks: [task("old-1")] }) }),
    (error) => error instanceof InventoryError && error.code === "INCOMPLETE_INVENTORY",
  );
});

test("new MCP traverses all 4053 tasks once without duplicates", async () => {
  const all = Array.from({ length: 4053 }, (_, index) => task(`task-${index}`));
  const calls = [];
  const result = await collectTaskInventory({
    pageSize: 4053,
    callTool: async (name, args) => {
      calls.push({ name, args });
      const offset = args.cursor ? Number(args.cursor) : 0;
      const tasks = all.slice(offset, offset + 4053);
      const next = offset + tasks.length < all.length ? String(offset + tasks.length) : null;
      return page(tasks, next !== null, next);
    },
  });
  assert.equal(result.tasks.length, 4053);
  assert.equal(new Set(result.tasks.map(({ key }) => key)).size, 4053);
  assert.equal(result.exhausted, true);
  assert.equal(result.incomplete, false);
  assert.equal(calls.length, 1);
});

test("multi-page traversal preserves filters and rejects a replayed cursor", async () => {
  const calls = [];
  await assert.rejects(
    () => collectTaskInventory({
      pageSize: 2,
      filters: { status: "open", mine_only: true, project_prefix: "api#", search: "lease", search_mode: "contains" },
      callTool: async (name, args) => {
        calls.push({ name, args });
        return calls.length === 1 ? page([task("one")], true, "same") : page([task("two")], true, "same");
      },
    }),
    (error) => error.code === "INVALID_CURSOR",
  );
  assert.deepEqual(calls[0].args, {
    status: "open", mine_only: true, project_prefix: "api#", search: "lease", search_mode: "contains", limit: 2,
  });
  assert.deepEqual(calls[1].args, { ...calls[0].args, cursor: "same" });
});

test("signed cursor context mismatch is typed and never restarts at page one", async () => {
  const calls = [];
  await assert.rejects(
    () => collectTaskInventory({
      cursor: "signed-for-other-filter",
      filters: { status: "open", search: "lease", search_mode: "contains" },
      callTool: async (name, args) => {
        calls.push({ name, args });
        throw new InventoryError("CURSOR_CONTEXT_MISMATCH", "cursor is bound to different filters");
      },
    }),
    (error) => error.code === "CURSOR_CONTEXT_MISMATCH",
  );
  assert.equal(calls.length, 1);
  assert.equal(calls[0].args.cursor, "signed-for-other-filter");
  assert.equal(calls[0].args.search_mode, "contains");
});

test("duplicate keys are rejected rather than allowing partial conclusions", async () => {
  await assert.rejects(
    () => collectTaskInventory({
      callTool: async (_name, args) => args.cursor ? page([task("one")], false) : page([task("one")], true, "next"),
    }),
    (error) => error.code === "DUPLICATE_TASK",
  );
});

test("real MCP response shape preserves top-level tools and session transport", async () => {
  const requests = [];
  const response = (body, sessionId = null) => ({
    ok: true,
    headers: { get: (name) => name.toLowerCase() === "mcp-session-id" ? sessionId : null },
    text: async () => body,
  });
  const fetchImpl = async (_endpoint, init) => {
    requests.push(init);
    const request = JSON.parse(init.body);
    if (request.method === "initialize") {
      return response(JSON.stringify({ result: {
        protocolVersion: "2025-06-18",
        capabilities: {},
        serverInfo: { name: "crewsync", version: "0.8.0" },
      } }), "session-1");
    }
    if (request.method === "notifications/initialized") return response("{}");
    if (request.method === "tools/list") {
      const tools = { result: {
        tools: [{ name: "list_tasks", inputSchema: { properties: { cursor: {} } } }],
      } };
      const newline = String.fromCharCode(10);
      return response(["event: message", `data: ${JSON.stringify(tools)}`, "", ""].join(newline));
    }
    assert.equal(request.method, "tools/call");
    return response(JSON.stringify({ result: { structuredContent: page([task("real-1")], false) } }));
  };
  const client = createTaskInventoryClient({ endpoint: "https://bus.example/mcp", token: "token", fetchImpl });
  const result = await client.inventory({ pageSize: 1 });
  assert.equal(result.tasks[0].key, "real-1");
  assert.equal(result.capabilities.taskInventory.pagination, true);
  assert.deepEqual(requests.map((init) => JSON.parse(init.body).method), ["initialize", "notifications/initialized", "tools/list", "tools/call"]);
  assert.equal(requests[0].headers.authorization, "Bearer token");
  assert.equal(requests[1].headers["Mcp-Session-Id"], "session-1");
  assert.equal(requests.at(-1).headers["Mcp-Session-Id"], "session-1");
});

test("capability summary exposes version and paged best-effort inventory", async () => {
  const result = await discoverCapabilities({
    discover: async () => ({
      protocolVersion: "2025-06-18",
      capabilities: {},
      serverInfo: { name: "crewsync", version: "0.8.0" },
      tools: [{ name: "list_tasks", inputSchema: { properties: { cursor: {}, has_more: {} } } }],
    }),
  });
  assert.equal(result.protocolVersion, "2025-06-18");
  assert.equal(result.serverInfo.version, "0.8.0");
  assert.equal(result.taskInventory.pagination, true);
  assert.equal(result.taskInventory.consistency, "paged_best_effort");
});
