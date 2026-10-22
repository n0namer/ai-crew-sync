const DEFAULT_PAGE_SIZE = 50;
const DEFAULT_MAX_PAGES = 10_000;
const DEFAULT_MAX_TASKS = 1_000_000;

export class InventoryError extends Error {
  constructor(code, message, details = {}) {
    super(`${code}: ${message}`);
    this.name = "InventoryError";
    this.code = code;
    this.details = details;
  }
}

function nonEmptyString(value) {
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

function unwrapToolResult(result) {
  if (result && typeof result === "object" && result.structuredContent !== undefined) {
    return result.structuredContent;
  }
  if (result && typeof result === "object" && result.structured_content !== undefined) {
    return result.structured_content;
  }
  if (result && typeof result === "object" && result.result && typeof result.result === "object") {
    return unwrapToolResult(result.result);
  }
  return result;
}

function pageShape(page) {
  const value = unwrapToolResult(page);
  if (!value || typeof value !== "object" || !Array.isArray(value.tasks)) {
    throw new InventoryError("INVALID_PAGE", "list_tasks did not return a task page");
  }
  if (typeof value.has_more !== "boolean") {
    throw new InventoryError(
      "INCOMPLETE_INVENTORY",
      "server did not expose cursor pagination; refusing a partial inventory",
    );
  }
  return value;
}

export function buildTaskFilters(options = {}) {
  const filters = {};
  for (const name of [
    "status",
    "project_prefix",
    "search",
    "search_mode",
    "search_fields",
    "search_language",
  ]) {
    if (options[name] !== undefined && options[name] !== null) filters[name] = options[name];
  }
  if (options.mine_only !== undefined) filters.mine_only = Boolean(options.mine_only);
  return filters;
}

export function capabilitySummary(capabilities = {}) {
  const tools = Array.isArray(capabilities.tools) ? capabilities.tools : [];
  const listTasks = tools.find((tool) => tool?.name === "list_tasks");
  const properties = listTasks?.inputSchema?.properties ?? listTasks?.input_schema?.properties ?? {};
  const paged = Boolean(properties.cursor) || capabilities.taskPagination === true;
  return {
    protocolVersion: nonEmptyString(capabilities.protocolVersion ?? capabilities.protocol_version),
    serverInfo: capabilities.serverInfo ?? capabilities.server_info ?? null,
    taskInventory: {
      tool: "list_tasks",
      pagination: paged,
      consistency: "paged_best_effort",
      filters: ["status", "mine_only", "project_prefix", "search", "search_mode", "search_fields", "search_language"],
    },
  };
}

function normalizeCapabilityShape(value = {}) {
  const nested = value.capabilities && typeof value.capabilities === "object" ? value.capabilities : {};
  const tools = Array.isArray(value.tools) ? value.tools : nested.tools;
  return {
    ...nested,
    ...value,
    protocolVersion: value.protocolVersion ?? value.protocol_version ?? nested.protocolVersion ?? nested.protocol_version,
    serverInfo: value.serverInfo ?? value.server_info ?? nested.serverInfo ?? nested.server_info,
    tools,
  };
}

export async function discoverCapabilities({ discover, callTool } = {}) {
  let raw;
  if (discover) raw = await discover();
  else if (callTool) raw = await callTool("__capabilities__", {});
  else throw new TypeError("discover or callTool is required");
  return capabilitySummary(normalizeCapabilityShape(unwrapToolResult(await raw) ?? {}));
}

export async function collectTaskInventory({
  callTool,
  discover,
  filters = {},
  pageSize = DEFAULT_PAGE_SIZE,
  cursor,
  maxPages = DEFAULT_MAX_PAGES,
  maxTasks = DEFAULT_MAX_TASKS,
  requireComplete = true,
} = {}) {
  if (typeof callTool !== "function") throw new TypeError("callTool is required");
  if (!Number.isInteger(pageSize) || pageSize < 1) throw new RangeError("pageSize must be positive");
  if (!Number.isInteger(maxPages) || maxPages < 1) throw new RangeError("maxPages must be positive");
  if (!Number.isInteger(maxTasks) || maxTasks < 1) throw new RangeError("maxTasks must be positive");

  const capabilities = discover ? await discoverCapabilities({ discover, callTool }) : null;
  if (requireComplete && capabilities && !capabilities.taskInventory.pagination) {
    throw new InventoryError("INCOMPLETE_INVENTORY", "server does not advertise cursor pagination", { capabilities });
  }

  const baseArgs = { ...buildTaskFilters(filters), limit: pageSize };
  if (cursor !== undefined) baseArgs.cursor = cursor;
  const seenCursors = new Set(cursor === undefined ? [] : [cursor]);
  const seenKeys = new Set();
  const tasks = [];
  let pageCount = 0;
  let pageArgs = baseArgs;
  let totals = {};

  while (true) {
    if (++pageCount > maxPages) throw new InventoryError("INVENTORY_BOUNDS", "page limit exceeded", { maxPages });
    let page;
    try {
      page = pageShape(await callTool("list_tasks", pageArgs));
    } catch (error) {
      if (error?.code === "INVALID_CURSOR" || (error?.code === undefined && /cursor/i.test(String(error?.message)))) {
        throw new InventoryError("INVALID_CURSOR", "server rejected the cursor or filter binding", { cause: error });
      }
      throw error;
    }
    totals = { open: page.open, claimed: page.claimed };
    for (const task of page.tasks) {
      const key = task?.key;
      if (!nonEmptyString(key)) throw new InventoryError("INVALID_PAGE", "task page contains a task without a key");
      if (seenKeys.has(key)) throw new InventoryError("DUPLICATE_TASK", `task key repeated: ${key}`, { key });
      seenKeys.add(key);
      tasks.push(task);
      if (tasks.length > maxTasks) throw new InventoryError("INVENTORY_BOUNDS", "task limit exceeded", { maxTasks });
    }
    if (!page.has_more) {
      return {
        tasks,
        ...totals,
        next_cursor: null,
        has_more: false,
        exhausted: true,
        incomplete: false,
        consistency: "paged_best_effort",
        capabilities,
        pages: pageCount,
      };
    }
    const nextCursor = nonEmptyString(page.next_cursor);
    if (!nextCursor) throw new InventoryError("INCOMPLETE_INVENTORY", "server announced another page without a cursor");
    if (seenCursors.has(nextCursor)) throw new InventoryError("INVALID_CURSOR", "server replayed a cursor", { cursor: nextCursor });
    seenCursors.add(nextCursor);
    pageArgs = { ...baseArgs, cursor: nextCursor };
  }
}

export function createTaskInventoryClient({ endpoint, token, fetchImpl = globalThis.fetch, headers = {} } = {}) {
  if (!endpoint || typeof fetchImpl !== "function") throw new TypeError("endpoint and fetchImpl are required");
  let requestId = 0;
  let initialized = false;
  let capabilities = null;
  let sessionId;
  const baseHeaders = { accept: "application/json, text/event-stream", "content-type": "application/json", ...headers };
  if (token) baseHeaders.authorization = `Bearer ${token}`;

  function requestHeaders() {
    return sessionId ? { ...baseHeaders, "Mcp-Session-Id": sessionId } : { ...baseHeaders };
  }

  async function rpc(method, params = {}) {
    const response = await fetchImpl(endpoint, {
      method: "POST",
      headers: requestHeaders(),
      body: JSON.stringify({ jsonrpc: "2.0", id: ++requestId, method, params }),
    });
    const responseSessionId = response.headers?.get?.("mcp-session-id") ?? response.headers?.get?.("Mcp-Session-Id");
    if (responseSessionId) sessionId = responseSessionId;
    const text = await response.text();
    if (!response.ok) throw new InventoryError("TRANSPORT", `MCP HTTP ${response.status}`, { body: text });
    const jsonLine = text.split("\n").find((line) => line.startsWith("data:"));
    const payload = jsonLine ? JSON.parse(jsonLine.slice(5).trim()) : JSON.parse(text);
    if (payload.error) throw new InventoryError(payload.error.data?.code ?? "MCP_ERROR", payload.error.message, payload.error.data);
    return payload.result ?? payload;
  }

  return {
    async discover() {
      if (!initialized) {
        const init = await rpc("initialize", { protocolVersion: "2025-06-18", capabilities: {}, clientInfo: { name: "crew-task-inventory", version: "1.0.0" } });
        initialized = true;
        await fetchImpl(endpoint, { method: "POST", headers: requestHeaders(), body: JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized", params: {} }) });
        let tools = [];
        try {
          const listed = await rpc("tools/list", {});
          tools = listed.tools ?? [];
        } catch {
          tools = [];
        }
        capabilities = { ...init, tools };
      }
      return capabilities;
    },
    async callTool(name, arguments_) {
      if (!initialized) await this.discover();
      const result = await rpc("tools/call", { name, arguments: arguments_ });
      if (result?.isError) throw new InventoryError(result.error?.code ?? "MCP_ERROR", result.error?.message ?? "MCP tool failed");
      return result?.structuredContent ?? result;
    },
    async inventory(options = {}) {
      return collectTaskInventory({ ...options, discover: () => this.discover(), callTool: (name, args) => this.callTool(name, args) });
    },
  };
}
