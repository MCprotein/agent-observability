import type { AgentObservabilityReportV1, Span, Trace } from "./generated/report-dto-v1.js";
import validateReportDtoV1 from "./generated/validate-report-dto-v1.js";
import { summarizeVisible } from "./view-summary.js";

type FilterKey = "repo" | "session" | "agent" | "model";
type FilterState = Record<FilterKey, string | undefined> & { text: string; trace: string | undefined };

const ALL_OPTION = "-1";
const UNKNOWN = "unknown";
const reportData = document.getElementById("report-data");

if (!reportData) {
  throw new Error("Missing report data");
}

const candidate = parseReportData(reportData.textContent);
if (candidate === undefined || !validateReportDtoV1(candidate)) {
  document.body.replaceChildren(errorState("Report data does not match agent_observability.report.v1."));
} else {
  mount(candidate as unknown as AgentObservabilityReportV1);
}

function parseReportData(value: string | null): unknown | undefined {
  try {
    return JSON.parse(value ?? "null") as unknown;
  } catch {
    return undefined;
  }
}

function mount(data: AgentObservabilityReportV1): void {
  const state: FilterState = {
    repo: undefined,
    session: undefined,
    agent: undefined,
    model: undefined,
    text: "",
    trace: undefined,
  };
  const selects = {
    repo: element<HTMLSelectElement>("repo-filter"),
    session: element<HTMLSelectElement>("session-filter"),
    agent: element<HTMLSelectElement>("agent-filter"),
    model: element<HTMLSelectElement>("model-filter"),
  };
  const textFilter = element<HTMLInputElement>("text-filter");
  const clearFilters = element<HTMLButtonElement>("clear-filters");
  const tracesElement = element<HTMLElement>("trace-list");
  const tableElement = element<HTMLTableSectionElement>("span-table");
  const traceCount = element<HTMLElement>("trace-count");
  const spanCount = element<HTMLElement>("span-count");
  const filterStatus = element<HTMLElement>("filter-status");

  const filterValues: Record<FilterKey, string[]> = {
    repo: data.filters.repos,
    session: data.filters.sessions,
    agent: data.filters.agents ?? uniqueSorted(data.spans.map((span) => span.agent.name ?? UNKNOWN)),
    model: data.filters.models ?? uniqueSorted(data.spans.map((span) => span.agent.model ?? UNKNOWN)),
  };
  fillSelect(selects.repo, filterValues.repo, "All repos");
  fillSelect(selects.session, filterValues.session, "All sessions");
  fillSelect(selects.agent, filterValues.agent, "All agents");
  fillSelect(selects.model, filterValues.model, "All models");

  for (const key of Object.keys(selects) as FilterKey[]) {
    selects[key].addEventListener("change", () => {
      const index = Number(selects[key].value);
      state[key] = index >= 0 ? filterValues[key][index] : undefined;
      state.trace = undefined;
      render();
    });
  }
  textFilter.addEventListener("input", () => {
    state.text = textFilter.value.trim().toLowerCase();
    state.trace = undefined;
    render();
  });
  clearFilters.addEventListener("click", () => {
    for (const key of Object.keys(selects) as FilterKey[]) {
      state[key] = undefined;
      selects[key].value = ALL_OPTION;
    }
    state.text = "";
    state.trace = undefined;
    textFilter.value = "";
    render();
    selects.repo.focus();
  });

  render();

  function render(): void {
    const spans = data.spans.filter(matchesFilters);
    const traces = data.traces.filter((trace) => spans.some((span) => span.traceId === trace.traceId));
    if (state.trace !== undefined && !traces.some((trace) => trace.traceId === state.trace)) {
      state.trace = undefined;
    }
    const visibleSpans = state.trace === undefined ? spans : spans.filter((span) => span.traceId === state.trace);
    const summary = summarizeVisible(visibleSpans);

    setText("kpi-sessions", summary.sessions);
    setText("kpi-turns", summary.turns);
    setText("kpi-llm", summary.llmRequests);
    setText("kpi-tools", summary.toolExecutions);
    setText("kpi-tokens", formatNumber(summary.inputTokens + summary.outputTokens));
    setText("kpi-cost", formatCost(summary.estimatedCost, {
      status: summary.costStatus,
      currency: data.cost.currency,
    }));
    setText("kpi-errors", summary.errors);
    traceCount.textContent = String(traces.length);
    spanCount.textContent = String(visibleSpans.length);
    clearFilters.disabled = !hasActiveFilters();
    filterStatus.textContent = hasActiveFilters()
      ? `${visibleSpans.length} spans match the active filters.`
      : `${visibleSpans.length} spans in this local report.`;
    renderTraces(traces, spans);
    renderSpans(visibleSpans);
  }

  function matchesFilters(span: Span): boolean {
    if (state.repo !== undefined && span.repo !== state.repo) return false;
    if (state.session !== undefined && span.sessionId !== state.session) return false;
    if (state.agent !== undefined && (span.agent.name ?? UNKNOWN) !== state.agent) return false;
    if (state.model !== undefined && (span.agent.model ?? UNKNOWN) !== state.model) return false;
    if (!state.text) return true;
    return [
      span.name,
      span.kind,
      span.status,
      span.toolName,
      span.traceId,
      span.spanId,
      span.agent.name,
      span.agent.model,
    ].join(" ").toLowerCase().includes(state.text);
  }

  function renderTraces(traces: Trace[], filteredSpans: Span[]): void {
    if (traces.length === 0) {
      tracesElement.innerHTML = '<div class="empty">No traces match the active filters.</div>';
      return;
    }
    tracesElement.replaceChildren(...traces.map((trace) => {
      const traceSpans = spansForTrace(trace, filteredSpans);
      const traceSummary = summarizeVisible(traceSpans);
      const button = document.createElement("button");
      button.type = "button";
      button.className = `trace-row${state.trace === trace.traceId ? " active" : ""}`;
      button.setAttribute("aria-pressed", String(state.trace === trace.traceId));
      button.addEventListener("click", () => {
        state.trace = state.trace === trace.traceId ? undefined : trace.traceId;
        render();
      });
      button.innerHTML =
        `<div class="trace-main"><span class="mono">${escapeHtml(shortId(trace.traceId))}</span>` +
        `<span class="badge ${traceSummary.errors ? "error" : "ok"}">${traceSummary.errors ? `${traceSummary.errors} error` : "ok"}</span></div>` +
        `<div class="trace-meta"><span>${escapeHtml(trace.repo)}</span><span>${traceSpans.length} spans</span>` +
        `<span>${formatNumber(traceSummary.inputTokens + traceSummary.outputTokens)} tokens</span></div>`;
      return button;
    }));
  }

  function renderSpans(spans: Span[]): void {
    if (spans.length === 0) {
      tableElement.innerHTML = '<tr><td class="empty" colspan="9">No spans match the active filters.</td></tr>';
      return;
    }
    tableElement.replaceChildren(...spans.map((span) => {
      const row = document.createElement("tr");
      row.innerHTML =
        `<td><span class="badge">${escapeHtml(span.kind)}</span></td>` +
        `<td>${escapeHtml(span.name)}${span.toolName ? `<div class="mono">${escapeHtml(span.toolName)}</div>` : ""}</td>` +
        `<td><span class="badge ${statusClass(span.status)}">${escapeHtml(span.status)}</span></td>` +
        `<td>${escapeHtml(span.repo)}</td>` +
        `<td class="mono">${escapeHtml(span.turnId ?? "")}</td>` +
        `<td>${formatNumber((span.metrics.inputTokens ?? 0) + (span.metrics.outputTokens ?? 0))}</td>` +
        `<td>${escapeHtml(formatCost(span.estimatedCost, span.cost))}</td>` +
        `<td>${formatDuration(span.metrics.latencyMs ?? span.metrics.durationMs)}</td>` +
        `<td class="mono">${escapeHtml(shortId(span.parentSpanId ?? ""))}</td>`;
      return row;
    }));
  }

  function hasActiveFilters(): boolean {
    return state.text.length > 0 || (Object.keys(selects) as FilterKey[]).some((key) => state[key] !== undefined);
  }
}

function spansForTrace(trace: Trace, spans: Span[]): Span[] {
  return spans.filter((span) => span.traceId === trace.traceId);
}

function element<T extends HTMLElement>(id: string): T {
  const value = document.getElementById(id);
  if (!value) throw new Error(`Missing report element: ${id}`);
  return value as T;
}

function fillSelect(select: HTMLSelectElement, values: string[], allLabel: string): void {
  const options = [
    option(ALL_OPTION, allLabel),
    ...values.map((value, index) => option(String(index), value === UNKNOWN ? "Unknown" : value)),
  ];
  select.replaceChildren(...options);
}

function option(value: string, label: string): HTMLOptionElement {
  const result = document.createElement("option");
  result.value = value;
  result.textContent = label;
  return result;
}

function uniqueSorted(values: string[]): string[] {
  return [...new Set(values)].sort();
}

function setText(id: string, value: string | number): void {
  element(id).textContent = typeof value === "number" ? formatNumber(value) : value;
}

function formatNumber(value: number): string {
  return Number(value || 0).toLocaleString();
}

function formatDuration(value: number | undefined): string {
  return Number.isFinite(value) ? `${Number(value).toLocaleString()} ms` : "";
}

function formatCost(
  value: number | undefined,
  cost: { status: string; currency?: string | undefined },
): string {
  if (cost.status === "unknown" && (!Number.isFinite(value) || value === 0)) return "unknown";
  if (!Number.isFinite(value)) return cost.status;
  const amount = `${cost.currency ?? "USD"} ${Number(Number(value).toPrecision(12)).toString()}`;
  return cost.status === "incomplete" ? `${amount} incomplete` : amount;
}

function statusClass(status: string): string {
  if (status === "error") return "error";
  if (status === "ok") return "ok";
  return "warning";
}

function shortId(value: string): string {
  return value.length > 18 ? `${value.slice(0, 8)}...${value.slice(-6)}` : value;
}

function escapeHtml(value: unknown): string {
  return String(value ?? "").replace(/[&<>"']/g, (char) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  }[char] ?? char));
}

function errorState(message: string): HTMLElement {
  const element = document.createElement("main");
  element.className = "report-error";
  element.textContent = message;
  return element;
}
