import type { AgentObservabilityReportV1, Span, Trace } from "./generated/report-dto-v1.js";
import validateReportDtoV1 from "./generated/validate-report-dto-v1.js";
import {
  buildFilteredView,
  buildTimeline,
  paginate,
  parseSavedFilters,
  isPersistableDimensions,
  sameDimensions,
  serializeSavedFilters,
  type DimensionFilters,
  type FilterKey,
  type FilterState,
  type Page,
} from "./view-state.js";
import { summarizeVisible } from "./view-summary.js";

const ALL_OPTION = "-1";
const UNKNOWN = "unknown";
const TRACE_PAGE_SIZE = 100;
const SPAN_PAGE_SIZE = 200;
const TIMELINE_LIMIT = 120;
const FILTER_OPTION_LIMIT = 500;
const SAVED_FILTER_LIMIT = 20;
const SAVED_FILTER_KEY = "agent-observability.report.v1.saved-filters";
const reportData = document.getElementById("report-data");

if (!reportData) throw new Error("Missing report data");

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
  let tracePageIndex = 0;
  let spanPageIndex = 0;
  let savedFilters: DimensionFilters[] = [];
  const selects = {
    repo: element<HTMLSelectElement>("repo-filter"),
    session: element<HTMLSelectElement>("session-filter"),
    agent: element<HTMLSelectElement>("agent-filter"),
    model: element<HTMLSelectElement>("model-filter"),
  };
  const textFilter = element<HTMLInputElement>("text-filter");
  const clearFilters = element<HTMLButtonElement>("clear-filters");
  const savedFilterSelect = element<HTMLSelectElement>("saved-filter");
  const saveFilter = element<HTMLButtonElement>("save-filter");
  const deleteFilter = element<HTMLButtonElement>("delete-filter");
  const tracesElement = element<HTMLElement>("trace-list");
  const tableElement = element<HTMLTableSectionElement>("span-table");
  const timelineElement = element<HTMLElement>("timeline-list");
  const traceCount = element<HTMLElement>("trace-count");
  const spanCount = element<HTMLElement>("span-count");
  const filterStatus = element<HTMLElement>("filter-status");
  const tracePrevious = element<HTMLButtonElement>("trace-previous");
  const traceNext = element<HTMLButtonElement>("trace-next");
  const spanPrevious = element<HTMLButtonElement>("span-previous");
  const spanNext = element<HTMLButtonElement>("span-next");

  const allFilterValues: Record<FilterKey, string[]> = {
    repo: data.filters.repos,
    session: data.filters.sessions,
    agent: data.filters.agents ?? uniqueSorted(data.spans.map((span) => span.agent.name ?? UNKNOWN)),
    model: data.filters.models ?? uniqueSorted(data.spans.map((span) => span.agent.model ?? UNKNOWN)),
  };
  const filterValues = Object.fromEntries(
    (Object.keys(allFilterValues) as FilterKey[]).map((key) => [key, allFilterValues[key].slice(0, FILTER_OPTION_LIMIT)]),
  ) as Record<FilterKey, string[]>;
  savedFilters = currentSavedFilters(loadSavedFilters(), filterValues);
  fillSelect(selects.repo, filterValues.repo, "All repos", allFilterValues.repo.length);
  fillSelect(selects.session, filterValues.session, "All sessions", allFilterValues.session.length);
  fillSelect(selects.agent, filterValues.agent, "All agents", allFilterValues.agent.length);
  fillSelect(selects.model, filterValues.model, "All models", allFilterValues.model.length);
  renderSavedFilterOptions();

  for (const key of Object.keys(selects) as FilterKey[]) {
    selects[key].addEventListener("change", () => {
      const index = Number(selects[key].value);
      state[key] = index >= 0 ? filterValues[key][index] : undefined;
      resetSelection();
      render();
    });
  }
  textFilter.addEventListener("input", () => {
    state.text = textFilter.value.trim().toLowerCase();
    resetSelection();
    render();
  });
  clearFilters.addEventListener("click", () => {
    applyDimensions(emptyDimensions());
    state.text = "";
    textFilter.value = "";
    resetSelection();
    render();
    selects.repo.focus();
  });
  saveFilter.addEventListener("click", () => {
    const dimensions = currentDimensions();
    if (!isPersistableDimensions(dimensions)) return;
    const existing = savedFilters.findIndex((saved) => sameDimensions(saved, dimensions));
    if (existing >= 0) {
      savedFilterSelect.value = String(existing);
    } else {
      const nextFilters = [dimensions, ...savedFilters].slice(0, SAVED_FILTER_LIMIT);
      if (!persistSavedFilters(nextFilters)) {
        filterStatus.textContent = "Saved views are unavailable in this browser context.";
        return;
      }
      savedFilters = nextFilters;
      renderSavedFilterOptions();
      savedFilterSelect.value = "0";
    }
    deleteFilter.disabled = false;
  });
  savedFilterSelect.addEventListener("change", () => {
    const selected = savedFilters[Number(savedFilterSelect.value)];
    deleteFilter.disabled = selected === undefined;
    if (!selected) return;
    applyDimensions(selected);
    state.text = "";
    textFilter.value = "";
    state.trace = undefined;
    tracePageIndex = 0;
    spanPageIndex = 0;
    render();
  });
  deleteFilter.addEventListener("click", () => {
    const index = Number(savedFilterSelect.value);
    if (!Number.isInteger(index) || index < 0 || index >= savedFilters.length) return;
    const nextFilters = savedFilters.filter((_, savedIndex) => savedIndex !== index);
    if (!persistSavedFilters(nextFilters)) {
      filterStatus.textContent = "The saved view could not be deleted in this browser context.";
      return;
    }
    savedFilters = nextFilters;
    renderSavedFilterOptions();
  });
  tracePrevious.addEventListener("click", () => {
    tracePageIndex -= 1;
    render();
  });
  traceNext.addEventListener("click", () => {
    tracePageIndex += 1;
    render();
  });
  spanPrevious.addEventListener("click", () => {
    spanPageIndex -= 1;
    render();
  });
  spanNext.addEventListener("click", () => {
    spanPageIndex += 1;
    render();
  });

  render();

  function render(): void {
    const view = buildFilteredView(data.spans, data.traces, state);
    if (state.trace !== undefined && !view.spansByTrace.has(state.trace)) state.trace = undefined;
    const visibleSpans = state.trace === undefined
      ? view.spans
      : (view.spansByTrace.get(state.trace) ?? []);
    const tracePage = paginate(view.traces, tracePageIndex, TRACE_PAGE_SIZE);
    const spanPage = paginate(visibleSpans, spanPageIndex, SPAN_PAGE_SIZE);
    tracePageIndex = tracePage.index;
    spanPageIndex = spanPage.index;
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
    traceCount.textContent = String(view.traces.length);
    spanCount.textContent = String(visibleSpans.length);
    clearFilters.disabled = !hasActiveFilters();
    saveFilter.disabled = !isPersistableDimensions(currentDimensions());
    filterStatus.textContent = hasActiveFilters()
      ? `${visibleSpans.length} spans match the active filters.`
      : `${visibleSpans.length} spans in this local report.`;
    renderTraces(tracePage, view.spansByTrace);
    renderTimeline(visibleSpans, state.trace !== undefined);
    renderSpans(spanPage);
    renderPager("trace", tracePage, tracePrevious, traceNext, TRACE_PAGE_SIZE);
    renderPager("span", spanPage, spanPrevious, spanNext, SPAN_PAGE_SIZE);
  }

  function renderTraces(page: Page<Trace>, spansByTrace: Map<string, Span[]>): void {
    if (page.total === 0) {
      tracesElement.innerHTML = '<div class="empty">No traces match the active filters.</div>';
      return;
    }
    tracesElement.replaceChildren(...page.items.map((trace) => {
      const traceSpans = spansByTrace.get(trace.traceId) ?? [];
      const traceSummary = summarizeVisible(traceSpans);
      const button = document.createElement("button");
      button.type = "button";
      button.className = `trace-row${state.trace === trace.traceId ? " active" : ""}`;
      button.setAttribute("aria-pressed", String(state.trace === trace.traceId));
      button.addEventListener("click", () => {
        state.trace = state.trace === trace.traceId ? undefined : trace.traceId;
        spanPageIndex = 0;
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

  function renderTimeline(spans: Span[], traceSelected: boolean): void {
    if (!traceSelected) {
      timelineElement.innerHTML = '<div class="empty">Select a trace to inspect its timeline.</div>';
      element("timeline-status").textContent = "No trace selected.";
      return;
    }
    const items = buildTimeline(spans, TIMELINE_LIMIT);
    if (items.length === 0) {
      timelineElement.innerHTML = '<div class="empty">No timeline data for the active filters.</div>';
      element("timeline-status").textContent = "0 spans.";
      return;
    }
    timelineElement.replaceChildren(...items.map((item) => {
      const row = document.createElement("div");
      row.className = "timeline-row";
      row.innerHTML =
        `<div class="timeline-label"><span>${escapeHtml(item.span.name)}</span>` +
        `<span class="mono">${escapeHtml(formatDuration(item.span.metrics.latencyMs ?? item.span.metrics.durationMs))}</span></div>` +
        `<div class="timeline-track"><span class="timeline-bar ${statusClass(item.span.status)}" ` +
        `style="left:${item.leftPercent.toFixed(3)}%;width:${item.widthPercent.toFixed(3)}%"></span></div>`;
      return row;
    }));
    element("timeline-status").textContent = spans.length > TIMELINE_LIMIT
      ? `Showing first ${TIMELINE_LIMIT} of ${spans.length} spans.`
      : `${spans.length} spans.`;
  }

  function renderSpans(page: Page<Span>): void {
    if (page.total === 0) {
      tableElement.innerHTML = '<tr><td class="empty" colspan="9">No spans match the active filters.</td></tr>';
      return;
    }
    tableElement.replaceChildren(...page.items.map((span) => {
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

  function renderPager(
    prefix: "trace" | "span",
    page: Page<unknown>,
    previous: HTMLButtonElement,
    next: HTMLButtonElement,
    pageSize: number,
  ): void {
    previous.disabled = page.index === 0;
    next.disabled = page.index >= page.count - 1;
    element(`${prefix}-page-status`).textContent = page.total === 0
      ? "0 of 0"
      : `${page.index * pageSize + 1}-${Math.min(page.total, (page.index + 1) * pageSize)} of ${page.total}`;
  }

  function renderSavedFilterOptions(): void {
    savedFilterSelect.replaceChildren(
      option(ALL_OPTION, "Saved views"),
      ...savedFilters.map((saved, index) => option(String(index), savedFilterLabel(saved))),
    );
    deleteFilter.disabled = true;
  }

  function applyDimensions(dimensions: DimensionFilters): void {
    for (const key of Object.keys(selects) as FilterKey[]) {
      const value = dimensions[key];
      const index = value === undefined ? -1 : filterValues[key].indexOf(value);
      state[key] = index >= 0 ? value : undefined;
      selects[key].value = index >= 0 ? String(index) : ALL_OPTION;
    }
  }

  function currentDimensions(): DimensionFilters {
    return { repo: state.repo, session: state.session, agent: state.agent, model: state.model };
  }

  function resetSelection(): void {
    state.trace = undefined;
    tracePageIndex = 0;
    spanPageIndex = 0;
    savedFilterSelect.value = ALL_OPTION;
    deleteFilter.disabled = true;
  }

  function hasActiveFilters(): boolean {
    return state.text.length > 0 || (Object.keys(selects) as FilterKey[]).some((key) => state[key] !== undefined);
  }
}

function emptyDimensions(): DimensionFilters {
  return { repo: undefined, session: undefined, agent: undefined, model: undefined };
}

function loadSavedFilters(): DimensionFilters[] {
  try {
    return parseSavedFilters(globalThis.localStorage?.getItem(SAVED_FILTER_KEY) ?? null, SAVED_FILTER_LIMIT);
  } catch {
    return [];
  }
}

function persistSavedFilters(filters: DimensionFilters[]): boolean {
  try {
    if (!globalThis.localStorage) return false;
    globalThis.localStorage.setItem(SAVED_FILTER_KEY, serializeSavedFilters(filters));
    return true;
  } catch {
    return false;
  }
}

function currentSavedFilters(
  savedFilters: DimensionFilters[],
  values: Record<FilterKey, string[]>,
): DimensionFilters[] {
  const result: DimensionFilters[] = [];
  for (const saved of savedFilters) {
    const isCurrent = (Object.keys(values) as FilterKey[]).every((key) =>
      saved[key] === undefined || values[key].includes(saved[key]),
    );
    if (isCurrent && hasDimensions(saved) && !result.some((candidate) => sameDimensions(candidate, saved))) {
      result.push(saved);
    }
  }
  return result;
}

function hasDimensions(filters: DimensionFilters): boolean {
  return filters.repo !== undefined
    || filters.session !== undefined
    || filters.agent !== undefined
    || filters.model !== undefined;
}

function savedFilterLabel(filters: DimensionFilters): string {
  const parts = [filters.repo, filters.session, filters.agent, filters.model]
    .filter((value): value is string => value !== undefined)
    .map(shortId);
  return parts.length > 0 ? parts.join(" / ") : "All dimensions";
}

function element<T extends HTMLElement>(id: string): T {
  const value = document.getElementById(id);
  if (!value) throw new Error(`Missing report element: ${id}`);
  return value as T;
}

function fillSelect(select: HTMLSelectElement, values: string[], allLabel: string, total: number): void {
  const more = option("", `${values.length} shown; use text search for more`);
  more.disabled = true;
  select.replaceChildren(
    option(ALL_OPTION, allLabel),
    ...values.map((value, index) => option(String(index), value === UNKNOWN ? "Unknown" : value)),
    ...(total > values.length ? [more] : []),
  );
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
  const result = document.createElement("main");
  result.className = "report-error";
  result.textContent = message;
  return result;
}
