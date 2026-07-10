import { mkdir, writeFile } from "node:fs/promises";
import { dirname } from "node:path";
import { estimateCostForRecords, estimateSpanCost, normalizeRateTable } from "../cost.js";

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

export function reportDataFromRecords(records, options = {}) {
  const rateTable = normalizeRateTable(options.rate_table ?? options.rateTable);
  const spans = records
    .filter((record) => record?.record_type === "span")
    .map((record) => safeSpan(record, rateTable))
    .sort((left, right) => left.startTimeUnixMs - right.startTimeUnixMs);

  return {
    generatedAt: options.generated_at ?? new Date().toISOString(),
    title: options.title ?? "Agent Observability Report",
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
      grid-template-columns: repeat(4, minmax(0, 1fr));
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
    input {
      width: 100%;
      min-height: 34px;
      border: 1px solid var(--line);
      border-radius: 6px;
      background: #fff;
      color: var(--text);
      padding: 6px 8px;
      font: inherit;
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

    <section class="controls" aria-label="filters">
      <label>Repo<select id="repo-filter"></select></label>
      <label>Session<select id="session-filter"></select></label>
      <label>Turn<select id="turn-filter"></select></label>
      <label>Text<input id="text-filter" type="search" autocomplete="off"></label>
    </section>

    <section class="layout">
      <aside class="panel">
        <div class="panel-title">
          <span>Traces</span>
          <span class="badge" id="trace-count">0</span>
        </div>
        <div class="trace-list" id="trace-list"></div>
      </aside>
      <section class="panel">
        <div class="panel-title">
          <span>Spans</span>
          <span class="badge" id="span-count">0</span>
        </div>
        <div class="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Kind</th>
                <th>Name</th>
                <th>Status</th>
                <th>Repo</th>
                <th>Turn</th>
                <th>Tokens</th>
                <th>Cost</th>
                <th>Latency</th>
                <th>Parent</th>
              </tr>
            </thead>
            <tbody id="span-table"></tbody>
          </table>
        </div>
      </section>
    </section>
  </main>
  <script id="report-data" type="application/json">${jsonForHtml(data)}</script>
  <script>
    (() => {
      const data = JSON.parse(document.getElementById("report-data").textContent);
      const state = { repo: "all", session: "all", turn: "all", text: "", trace: "all" };
      const els = {
        repo: document.getElementById("repo-filter"),
        session: document.getElementById("session-filter"),
        turn: document.getElementById("turn-filter"),
        text: document.getElementById("text-filter"),
        traces: document.getElementById("trace-list"),
        table: document.getElementById("span-table"),
        traceCount: document.getElementById("trace-count"),
        spanCount: document.getElementById("span-count"),
      };

      fillSelect(els.repo, ["all", ...data.filters.repos]);
      fillSelect(els.session, ["all", ...data.filters.sessions]);
      fillSelect(els.turn, ["all", ...data.filters.turns]);

      els.repo.addEventListener("change", () => { state.repo = els.repo.value; render(); });
      els.session.addEventListener("change", () => { state.session = els.session.value; render(); });
      els.turn.addEventListener("change", () => { state.turn = els.turn.value; render(); });
      els.text.addEventListener("input", () => { state.text = els.text.value.trim().toLowerCase(); render(); });

      render();

      function render() {
        const spans = filteredSpans();
        const traces = data.traces.filter((trace) => spans.some((span) => span.traceId === trace.traceId));
        if (state.trace !== "all" && !traces.some((trace) => trace.traceId === state.trace)) {
          state.trace = "all";
        }
        const visibleSpans = state.trace === "all" ? spans : spans.filter((span) => span.traceId === state.trace);
        const summary = summarizeVisible(visibleSpans);

        setText("kpi-sessions", summary.sessions);
        setText("kpi-turns", summary.turns);
        setText("kpi-llm", summary.llmRequests);
        setText("kpi-tools", summary.toolExecutions);
        setText("kpi-tokens", formatNumber(summary.inputTokens + summary.outputTokens));
        document.getElementById("kpi-cost").textContent = formatCost(summary.estimatedCost, data.cost);
        setText("kpi-errors", summary.errors);
        els.traceCount.textContent = traces.length;
        els.spanCount.textContent = visibleSpans.length;
        renderTraces(traces);
        renderSpans(visibleSpans);
      }

      function filteredSpans() {
        return data.spans.filter((span) => {
          if (state.repo !== "all" && span.repo !== state.repo) return false;
          if (state.session !== "all" && span.sessionId !== state.session) return false;
          if (state.turn !== "all" && span.turnId !== state.turn) return false;
          if (state.text) {
            const haystack = [span.name, span.kind, span.status, span.toolName, span.traceId, span.spanId].join(" ").toLowerCase();
            if (!haystack.includes(state.text)) return false;
          }
          return true;
        });
      }

      function renderTraces(traces) {
        if (traces.length === 0) {
          els.traces.innerHTML = '<div class="empty">No traces</div>';
          return;
        }
        els.traces.replaceChildren(...traces.map((trace) => {
          const button = document.createElement("button");
          button.type = "button";
          button.className = "trace-row" + (state.trace === trace.traceId ? " active" : "");
          button.addEventListener("click", () => {
            state.trace = state.trace === trace.traceId ? "all" : trace.traceId;
            render();
          });
          button.innerHTML =
            '<div class="trace-main"><span class="mono">' + escapeHtml(shortId(trace.traceId)) + '</span>' +
            '<span class="badge ' + (trace.errors ? "error" : "ok") + '">' + (trace.errors ? trace.errors + " error" : "ok") + '</span></div>' +
            '<div class="trace-meta"><span>' + escapeHtml(trace.repo) + '</span><span>' + trace.spans + ' spans</span><span>' +
            formatNumber(trace.inputTokens + trace.outputTokens) + ' tokens</span></div>';
          return button;
        }));
      }

      function renderSpans(spans) {
        if (spans.length === 0) {
          els.table.innerHTML = '<tr><td class="empty" colspan="8">No spans</td></tr>';
          return;
        }
        els.table.replaceChildren(...spans.map((span) => {
          const row = document.createElement("tr");
          row.innerHTML =
            '<td><span class="badge">' + escapeHtml(span.kind) + '</span></td>' +
            '<td>' + escapeHtml(span.name) + (span.toolName ? '<div class="mono">' + escapeHtml(span.toolName) + '</div>' : '') + '</td>' +
            '<td><span class="badge ' + statusClass(span.status) + '">' + escapeHtml(span.status) + '</span></td>' +
            '<td>' + escapeHtml(span.repo) + '</td>' +
            '<td class="mono">' + escapeHtml(span.turnId || "") + '</td>' +
            '<td>' + formatNumber((span.metrics.inputTokens || 0) + (span.metrics.outputTokens || 0)) + '</td>' +
            '<td>' + formatCost(span.estimatedCost, span.cost) + '</td>' +
            '<td>' + formatDuration(span.metrics.latencyMs || span.metrics.durationMs) + '</td>' +
            '<td class="mono">' + escapeHtml(shortId(span.parentSpanId || "")) + '</td>';
          return row;
        }));
      }

      function summarizeVisible(spans) {
        const sessions = new Set();
        const turns = new Set();
        const summary = {
          sessions: 0,
          turns: 0,
          llmRequests: 0,
          toolExecutions: 0,
          errors: 0,
          inputTokens: 0,
          outputTokens: 0,
          estimatedCost: 0,
        };
        for (const span of spans) {
          if (span.sessionId) sessions.add(span.sessionId);
          if (span.turnId) turns.add(span.turnId);
          if (span.kind === "llm.request") summary.llmRequests += 1;
          if (span.kind === "tool.execution") summary.toolExecutions += 1;
          if (span.status === "error") summary.errors += 1;
          summary.inputTokens += span.metrics.inputTokens || 0;
          summary.outputTokens += span.metrics.outputTokens || 0;
          summary.estimatedCost += span.estimatedCost || 0;
        }
        summary.sessions = sessions.size;
        summary.turns = turns.size;
        return summary;
      }

      function fillSelect(select, values) {
        select.replaceChildren(...values.map((value) => {
          const option = document.createElement("option");
          option.value = value;
          option.textContent = value;
          return option;
        }));
      }

      function setText(id, value) {
        document.getElementById(id).textContent = formatNumber(value);
      }

      function statusClass(status) {
        if (status === "error") return "error";
        if (status === "ok") return "ok";
        return "warning";
      }

      function formatNumber(value) {
        return Number(value || 0).toLocaleString();
      }

      function formatDuration(value) {
        if (!Number.isFinite(value)) return "";
        return value.toLocaleString() + " ms";
      }

      function formatCost(value, cost) {
        if (cost?.status === "unknown" && (!Number.isFinite(value) || value === 0)) return "unknown";
        if (!Number.isFinite(value)) return cost?.status || "unknown";
        const currency = cost?.currency || data.cost?.currency || "USD";
        const amount = currency + " " + Number(value.toPrecision(12)).toString();
        return cost?.status === "incomplete" ? amount + " incomplete" : amount;
      }

      function shortId(value) {
        if (!value) return "";
        return value.length > 18 ? value.slice(0, 8) + "..." + value.slice(-6) : value;
      }

      function escapeHtml(value) {
        return String(value ?? "").replace(/[&<>"']/g, (char) => ({
          "&": "&amp;",
          "<": "&lt;",
          ">": "&gt;",
          '"': "&quot;",
          "'": "&#39;",
        }[char]));
      }
    })();
  </script>
</body>
</html>`;
}

export async function writeStaticHtmlReport(filePath, records, options = {}) {
  const html = renderStaticHtmlReport(records, options);
  await mkdir(dirname(filePath), { recursive: true });
  await writeFile(filePath, html, "utf8");
  return {
    filePath,
    bytes: Buffer.byteLength(html, "utf8"),
  };
}

function safeSpan(record, rateTable) {
  const attributes = safeAttributes(record.attributes ?? {});
  const sessionId = attributes.session_id ?? sessionIdFromSpan(record);
  const turnId = attributes.turn_id ?? turnIdFromSpan(record);
  const estimatedCost = estimateSpanCost(record, rateTable);

  return {
    schemaVersion: record.schema_version,
    traceId: record.trace_id,
    spanId: record.span_id,
    parentSpanId: record.parent_span_id,
    kind: record.span_kind,
    name: String(record.name),
    status: record.status?.code ?? "unset",
    startTimeUnixMs: record.start_time_unix_ms,
    endTimeUnixMs: record.end_time_unix_ms,
    repo: repoName(record),
    agent: safeAgent(record.agent ?? {}),
    sessionId,
    turnId,
    toolName: attributes.tool_name,
    attributes,
    metrics: safeMetrics(record.metrics ?? {}),
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
    name: scalarString(agent.name),
    model: scalarString(agent.model),
    version: scalarString(agent.version),
  });
}

function safeAttributes(attributes) {
  const safe = {};
  for (const key of SAFE_ATTRIBUTE_KEYS) {
    const value = attributes[key];
    if (value === undefined || value === null) {
      continue;
    }
    if (typeof value === "string" || typeof value === "number" || typeof value === "boolean") {
      safe[key] = value;
    }
  }
  return safe;
}

function safeMetrics(metrics) {
  return compactObject({
    inputTokens: metricNumber(metrics.input_tokens),
    outputTokens: metricNumber(metrics.output_tokens),
    cachedInputTokens: metricNumber(metrics.cached_input_tokens),
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
  const name = scalarString(record.project?.name);
  if (name) {
    return name;
  }

  const repoPath = scalarString(record.project?.repo_path);
  if (!repoPath || repoPath.includes("[redacted")) {
    return "unknown";
  }

  const parts = repoPath.split(/[\\/]+/).filter(Boolean);
  return parts.at(-1) ?? "unknown";
}

function sessionIdFromSpan(record) {
  if (record.span_kind === "agent.session") {
    return record.span_id;
  }
  const match = /^codex-[^:]+:([^:]+)/.exec(record.span_id);
  return match?.[1];
}

function turnIdFromSpan(record) {
  if (record.span_kind === "turn") {
    const prefix = /^codex-turn:[^:]+:(.+)$/.exec(record.span_id);
    return prefix?.[1] ?? record.span_id;
  }
  const match = /^codex-(?:llm|tool|permission):[^:]+:([^:]+)/.exec(record.span_id);
  return match?.[1];
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
