import { readFileSync } from "node:fs";
import { writeFile } from "node:fs/promises";
import { estimateCostForRecords, estimateSpanCost, normalizeRateTable } from "../cost.js";
import { enforcePrivateFile, preparePrivateArtifact } from "../private-artifact.js";
import { redactRecord, redactText } from "../redaction.js";

const SAFE_ATTRIBUTE_KEYS = new Set([
  "source",
  "event_type",
  "envelope_type",
  "session_id",
  "turn_id",
  "request_id",
  "call_id",
  "tool_name",
  "phase",
  "exit_code",
  "sandbox",
  "approval",
]);
const REPORT_UI_SOURCE = readFileSync(
  new URL("./generated/report-ui.js", import.meta.url),
  "utf8",
);

export function reportDataFromRecords(records, options = {}) {
  const rateTable = normalizeRateTable(options.rate_table ?? options.rateTable);
  const spans = records
    .filter((record) => record?.record_type === "span")
    .map((record) => safeSpan(record, rateTable))
    .sort((left, right) => left.startTimeUnixMs - right.startTimeUnixMs);

  return {
    schemaVersion: "agent_observability.report.v1",
    generatedAt: options.generated_at ?? new Date().toISOString(),
    title: redactText(options.title ?? "Agent Observability Report", "title"),
    summary: summarize(spans),
    cost: estimateCostForRecords(
      records.filter((record) => record?.record_type === "span"),
      rateTable,
    ),
    filters: filterValues(spans),
    traces: traceSummaries(spans),
    spans,
  };
}

export function renderStaticHtmlReport(records, options = {}) {
  const data = reportDataFromRecords(records, options);

  return `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>${escapeHtml(data.title)}</title>
  <style>
    :root {
      color-scheme: light;
      --bg: #f7f8fa;
      --surface: #ffffff;
      --surface-strong: #eef2f5;
      --text: #182026;
      --muted: #66727d;
      --line: #d9e0e6;
      --accent: #0f766e;
      --accent-soft: #d7f2ed;
      --warning: #a16207;
      --error: #b42318;
      --ok: #16774f;
    }

    * { box-sizing: border-box; }

    body {
      margin: 0;
      background: var(--bg);
      color: var(--text);
      font: 14px/1.45 -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    }

    header {
      border-bottom: 1px solid var(--line);
      background: var(--surface);
    }

    .wrap {
      width: min(1280px, calc(100% - 32px));
      margin: 0 auto;
    }

    .topbar {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 16px;
      min-height: 68px;
    }

    h1 {
      margin: 0;
      font-size: 20px;
      font-weight: 700;
      letter-spacing: 0;
    }

    .timestamp {
      color: var(--muted);
      font-size: 12px;
      white-space: nowrap;
    }

    main {
      padding: 20px 0 28px;
    }

    .kpis {
      display: grid;
      grid-template-columns: repeat(7, minmax(0, 1fr));
      gap: 10px;
      margin-bottom: 16px;
    }

    .kpi {
      min-width: 0;
      border: 1px solid var(--line);
      border-radius: 8px;
      background: var(--surface);
      padding: 12px;
    }

    .kpi-label {
      color: var(--muted);
      font-size: 12px;
    }

    .kpi-value {
      margin-top: 6px;
      font-size: 24px;
      font-weight: 700;
      line-height: 1;
      letter-spacing: 0;
    }

    .controls {
      display: grid;
      grid-template-columns: repeat(5, minmax(0, 1fr));
      gap: 10px;
      margin-bottom: 16px;
      padding: 12px;
      border: 1px solid var(--line);
      border-radius: 8px;
      background: var(--surface);
    }

    label {
      display: grid;
      gap: 6px;
      color: var(--muted);
      font-size: 12px;
    }

    select,
    input,
    button {
      width: 100%;
      min-height: 34px;
      border: 1px solid var(--line);
      border-radius: 6px;
      background: #fff;
      color: var(--text);
      padding: 6px 8px;
      font: inherit;
    }

    button { cursor: pointer; }
    button:disabled { cursor: default; opacity: 0.55; }
    select:focus-visible,
    input:focus-visible,
    button:focus-visible { outline: 2px solid #2563eb; outline-offset: 2px; }

    .control-actions {
      display: grid;
      grid-column: 1 / -1;
      grid-template-columns: minmax(120px, 180px) minmax(0, 1fr);
      gap: 12px;
      align-items: center;
    }

    .filter-status { color: var(--muted); font-size: 12px; }
    .sr-only {
      position: absolute;
      width: 1px;
      height: 1px;
      padding: 0;
      margin: -1px;
      overflow: hidden;
      clip: rect(0, 0, 0, 0);
      white-space: nowrap;
      border: 0;
    }

    .layout {
      display: grid;
      grid-template-columns: 360px minmax(0, 1fr);
      gap: 16px;
      align-items: start;
    }

    .panel {
      border: 1px solid var(--line);
      border-radius: 8px;
      background: var(--surface);
      overflow: hidden;
    }

    .panel-title {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 10px;
      min-height: 44px;
      padding: 10px 12px;
      border-bottom: 1px solid var(--line);
      background: var(--surface-strong);
      font-weight: 700;
      margin: 0;
      font-size: 14px;
      letter-spacing: 0;
    }

    .trace-list {
      display: grid;
      max-height: 650px;
      overflow: auto;
    }

    .trace-row {
      border: 0;
      border-bottom: 1px solid var(--line);
      background: transparent;
      color: inherit;
      padding: 12px;
      text-align: left;
      cursor: pointer;
      font: inherit;
    }

    .trace-row:hover,
    .trace-row.active {
      background: var(--accent-soft);
    }

    .trace-main {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      font-weight: 700;
    }

    .trace-meta {
      display: flex;
      flex-wrap: wrap;
      gap: 8px;
      margin-top: 6px;
      color: var(--muted);
      font-size: 12px;
    }

    .badge {
      display: inline-flex;
      align-items: center;
      min-height: 22px;
      border-radius: 999px;
      padding: 2px 8px;
      background: var(--surface-strong);
      color: var(--muted);
      font-size: 12px;
      font-weight: 600;
    }

    .badge.ok { color: var(--ok); }
    .badge.error { color: var(--error); }
    .badge.warning { color: var(--warning); }

    .table-wrap {
      overflow: auto;
      max-height: 650px;
    }

    table {
      width: 100%;
      border-collapse: collapse;
      min-width: 860px;
    }

    th,
    td {
      border-bottom: 1px solid var(--line);
      padding: 9px 10px;
      text-align: left;
      vertical-align: top;
    }

    th {
      position: sticky;
      top: 0;
      z-index: 1;
      background: var(--surface-strong);
      color: var(--muted);
      font-size: 12px;
      font-weight: 700;
    }

    td {
      font-size: 13px;
    }

    .mono {
      font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
      font-size: 12px;
      word-break: break-all;
    }

    .empty {
      padding: 24px;
      color: var(--muted);
    }

    @media (max-width: 980px) {
      .kpis { grid-template-columns: repeat(2, minmax(0, 1fr)); }
      .controls { grid-template-columns: repeat(2, minmax(0, 1fr)); }
      .layout { grid-template-columns: 1fr; }
    }

    @media (max-width: 560px) {
      .wrap { width: min(100% - 20px, 1280px); }
      .topbar { align-items: flex-start; flex-direction: column; padding: 12px 0; }
      .timestamp { white-space: normal; }
      .kpis,
      .controls { grid-template-columns: 1fr; }
      select,
      input,
      button { min-height: 44px; }
    }
  </style>
</head>
<body>
  <header>
    <div class="wrap topbar">
      <h1>${escapeHtml(data.title)}</h1>
      <div class="timestamp">${escapeHtml(data.generatedAt)}</div>
    </div>
  </header>
  <main class="wrap">
    <section class="kpis" aria-label="summary">
      <div class="kpi"><div class="kpi-label">Sessions</div><div class="kpi-value" id="kpi-sessions">0</div></div>
      <div class="kpi"><div class="kpi-label">Turns</div><div class="kpi-value" id="kpi-turns">0</div></div>
      <div class="kpi"><div class="kpi-label">LLM</div><div class="kpi-value" id="kpi-llm">0</div></div>
      <div class="kpi"><div class="kpi-label">Tools</div><div class="kpi-value" id="kpi-tools">0</div></div>
      <div class="kpi"><div class="kpi-label">Tokens</div><div class="kpi-value" id="kpi-tokens">0</div></div>
      <div class="kpi"><div class="kpi-label">Cost</div><div class="kpi-value" id="kpi-cost">unknown</div></div>
      <div class="kpi"><div class="kpi-label">Errors</div><div class="kpi-value" id="kpi-errors">0</div></div>
    </section>

    <fieldset class="controls">
      <legend class="sr-only">Report filters</legend>
      <label>Repo<select id="repo-filter"></select></label>
      <label>Session<select id="session-filter"></select></label>
      <label>Agent<select id="agent-filter"></select></label>
      <label>Model<select id="model-filter"></select></label>
      <label>Text<input id="text-filter" type="search" autocomplete="off"></label>
      <div class="control-actions">
        <button id="clear-filters" type="button" disabled>Clear filters</button>
        <span class="filter-status" id="filter-status" aria-live="polite"></span>
      </div>
    </fieldset>

    <section class="layout">
      <aside class="panel" aria-labelledby="traces-heading">
        <h2 class="panel-title" id="traces-heading">
          <span>Traces</span>
          <span class="badge" id="trace-count">0</span>
        </h2>
        <div class="trace-list" id="trace-list"></div>
      </aside>
      <section class="panel" aria-labelledby="spans-heading">
        <h2 class="panel-title" id="spans-heading">
          <span>Spans</span>
          <span class="badge" id="span-count">0</span>
        </h2>
        <div class="table-wrap">
          <table>
            <caption class="sr-only">Filtered agent observation spans</caption>
            <thead>
              <tr>
                <th scope="col">Kind</th>
                <th scope="col">Name</th>
                <th scope="col">Status</th>
                <th scope="col">Repo</th>
                <th scope="col">Turn</th>
                <th scope="col">Tokens</th>
                <th scope="col">Cost</th>
                <th scope="col">Latency</th>
                <th scope="col">Parent</th>
              </tr>
            </thead>
            <tbody id="span-table"></tbody>
          </table>
        </div>
      </section>
    </section>
  </main>
  <script id="report-data" type="application/json">${jsonForHtml(data)}</script>
  <script>${REPORT_UI_SOURCE}</script>
</body>
</html>`;
}

export async function writeStaticHtmlReport(filePath, records, options = {}) {
  const html = renderStaticHtmlReport(records, options);
  await preparePrivateArtifact(filePath);
  await writeFile(filePath, html, { encoding: "utf8", mode: 0o600 });
  await enforcePrivateFile(filePath);
  return {
    filePath,
    bytes: Buffer.byteLength(html, "utf8"),
  };
}

function safeSpan(record, rateTable) {
  const sanitized = redactRecord(record);
  const attributes = safeAttributes(sanitized.attributes ?? {});
  const sessionId = attributes.session_id;
  const turnId = attributes.turn_id;
  const estimatedCost = estimateSpanCost(sanitized, rateTable);

  return {
    schemaVersion: sanitized.schema_version,
    traceId: sanitized.trace_id,
    spanId: sanitized.span_id,
    parentSpanId: sanitized.parent_span_id,
    kind: sanitized.span_kind,
    name: spanDisplayName(sanitized, attributes),
    status: sanitized.status?.code ?? "unset",
    startTimeUnixMs: sanitized.start_time_unix_ms,
    endTimeUnixMs: sanitized.end_time_unix_ms,
    repo: repoName(sanitized),
    agent: safeAgent(sanitized.agent ?? {}),
    sessionId,
    turnId,
    toolName: attributes.tool_name,
    attributes,
    metrics: safeMetrics(sanitized.metrics ?? {}),
    estimatedCost: estimatedCost.estimated_cost,
    cost: estimatedCost,
  };
}

function summarize(spans) {
  return {
    generatedSpans: spans.length,
    sessions: countKind(spans, "agent.session"),
    turns: countKind(spans, "turn"),
    llmRequests: countKind(spans, "llm.request"),
    toolExecutions: countKind(spans, "tool.execution"),
    errors: spans.filter((span) => span.status === "error").length,
    inputTokens: sumMetric(spans, "inputTokens"),
    outputTokens: sumMetric(spans, "outputTokens"),
    cachedInputTokens: sumMetric(spans, "cachedInputTokens"),
    cacheCreationInputTokens: sumMetric(spans, "cacheCreationInputTokens"),
    reasoningOutputTokens: sumMetric(spans, "reasoningOutputTokens"),
    latencyMs: sumMetric(spans, "latencyMs"),
    durationMs: sumMetric(spans, "durationMs"),
    estimatedCost: sumCost(spans),
  };
}

function filterValues(spans) {
  return {
    repos: uniqueSorted(spans.map((span) => span.repo)),
    sessions: uniqueSorted(spans.map((span) => span.sessionId).filter(Boolean)),
    turns: uniqueSorted(spans.map((span) => span.turnId).filter(Boolean)),
    agents: uniqueSorted(spans.map((span) => span.agent.name ?? "unknown")),
    models: uniqueSorted(spans.map((span) => span.agent.model ?? "unknown")),
  };
}

function traceSummaries(spans) {
  const groups = new Map();
  for (const span of spans) {
    const group = groups.get(span.traceId) ?? {
      traceId: span.traceId,
      repo: span.repo,
      spans: 0,
      errors: 0,
      inputTokens: 0,
      outputTokens: 0,
      estimatedCost: 0,
      startTimeUnixMs: span.startTimeUnixMs,
      endTimeUnixMs: span.endTimeUnixMs,
      sessions: new Set(),
      turns: new Set(),
    };

    group.spans += 1;
    group.errors += span.status === "error" ? 1 : 0;
    group.inputTokens += span.metrics.inputTokens ?? 0;
    group.outputTokens += span.metrics.outputTokens ?? 0;
    group.estimatedCost += span.estimatedCost ?? 0;
    group.startTimeUnixMs = Math.min(group.startTimeUnixMs, span.startTimeUnixMs);
    group.endTimeUnixMs = maxNullable(group.endTimeUnixMs, span.endTimeUnixMs);
    if (span.sessionId) {
      group.sessions.add(span.sessionId);
    }
    if (span.turnId) {
      group.turns.add(span.turnId);
    }
    groups.set(span.traceId, group);
  }

  return [...groups.values()]
    .map((group) => ({
      ...group,
      sessions: [...group.sessions].sort(),
      turns: [...group.turns].sort(),
    }))
    .sort((left, right) => left.startTimeUnixMs - right.startTimeUnixMs);
}

function safeAgent(agent) {
  return compactObject({
    name: safeString(agent.name, "agent.name"),
    model: safeString(agent.model, "agent.model"),
    version: safeString(agent.version, "agent.version"),
  });
}

function safeAttributes(attributes) {
  const safe = {};
  for (const key of SAFE_ATTRIBUTE_KEYS) {
    const value = attributes[key];
    if (value === undefined || value === null) {
      continue;
    }
    if (typeof value === "string") {
      safe[key] = redactText(value, key);
    } else if (typeof value === "number" || typeof value === "boolean") {
      safe[key] = value;
    }
  }
  return safe;
}

function spanDisplayName(record, attributes) {
  if (record.span_kind === "agent.session") {
    return `${safeString(record.agent?.name, "agent.name") ?? "Agent"} session`;
  }
  if (record.span_kind === "turn") {
    return "Turn";
  }
  if (record.span_kind === "llm.request") {
    return safeString(record.agent?.model, "agent.model")
      ? `LLM ${safeString(record.agent.model, "agent.model")}`
      : "LLM request";
  }
  if (record.span_kind === "tool.execution") {
    return attributes.tool_name ?? "Tool execution";
  }
  if (record.span_kind === "permission") {
    return "Permission";
  }
  if (record.span_kind === "compaction") {
    return "Compaction";
  }
  if (record.span_kind === "workstream") {
    return "Workstream";
  }
  return redactText(String(record.name ?? record.span_kind), "name");
}

function safeMetrics(metrics) {
  return compactObject({
    inputTokens: metricNumber(metrics.input_tokens),
    outputTokens: metricNumber(metrics.output_tokens),
    cachedInputTokens: metricNumber(metrics.cached_input_tokens),
    cacheCreationInputTokens: metricNumber(metrics.cache_creation_input_tokens),
    reasoningOutputTokens: metricNumber(metrics.reasoning_output_tokens),
    totalTokens: metricNumber(metrics.total_tokens),
    latencyMs: metricNumber(metrics.latency_ms),
    durationMs: metricNumber(metrics.duration_ms),
    totalInputTokens: metricNumber(metrics.total_input_tokens),
    totalOutputTokens: metricNumber(metrics.total_output_tokens),
    totalCachedInputTokens: metricNumber(metrics.total_cached_input_tokens),
    totalReasoningOutputTokens: metricNumber(metrics.total_reasoning_output_tokens),
    totalAccumulatedTokens: metricNumber(metrics.total_accumulated_tokens),
    contextWindowTokens: metricNumber(metrics.context_window_tokens),
  });
}

function repoName(record) {
  const name = safeString(record.project?.name, "project.name");
  if (name) {
    return name;
  }

  const repoPath = scalarString(record.project?.repo_path);
  if (!repoPath || repoPath.includes("[redacted")) {
    return "unknown";
  }

  const parts = repoPath.split(/[\\/]+/).filter(Boolean);
  return redactText(parts.at(-1) ?? "unknown", "repo.name");
}

function countKind(spans, kind) {
  return spans.filter((span) => span.kind === kind).length;
}

function sumMetric(spans, key) {
  return spans.reduce((sum, span) => sum + (span.metrics[key] ?? 0), 0);
}

function sumCost(spans) {
  return spans.reduce((sum, span) => sum + (span.estimatedCost ?? 0), 0);
}

function uniqueSorted(values) {
  return [...new Set(values)].sort();
}

function maxNullable(left, right) {
  if (left === null || left === undefined) {
    return right ?? null;
  }
  if (right === null || right === undefined) {
    return left;
  }
  return Math.max(left, right);
}

function metricNumber(value) {
  return typeof value === "number" && Number.isFinite(value) ? value : undefined;
}

function scalarString(value) {
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

function safeString(value, key) {
  return typeof value === "string" && value.length > 0 ? redactText(value, key) : undefined;
}

function compactObject(object) {
  return Object.fromEntries(
    Object.entries(object).filter(([, value]) => value !== undefined && value !== null),
  );
}

function jsonForHtml(value) {
  return JSON.stringify(value).replace(/</g, "\\u003c");
}

function escapeHtml(value) {
  return String(value).replace(/[&<>"']/g, (char) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  }[char]));
}
