import type {
  DashboardFacetDimensionV1,
  DashboardFiltersV1,
  DashboardKpisV1,
  DashboardSpanRowV1,
  DashboardTraceRowV1,
  FieldAvailability,
} from "./generated/dashboard-query-v1.js";
import validateDashboardQueryResponseV1 from "./generated/validate-dashboard-query-v1.js";
import validatePrivateCodexTurnDetailV1 from "./generated/validate-private-codex-turn-detail-v1.js";
import {
  createDashboardTransport,
  createPrivateDetailTransport,
  PagedDashboardClient,
  type DashboardWireValidator,
  type PagedDashboardState,
} from "./paged-client.js";
import {
  isPersistableDimensions,
  parseSavedFilters,
  sameDimensions,
  serializeSavedFilters,
  type DimensionFilters,
} from "./view-state.js";

const ALL = "__all__";
const SAVED_FILTER_KEY = "agent-observability.report.v1.saved-filters";
const SAVED_FILTER_LIMIT = 20;
const filterIds = {
  repo: "repo-filter",
  session: "session-filter",
  agent: "agent-filter",
  model: "model-filter",
} as const;

export function startPagedDashboard(options: {
  readonly validate?: DashboardWireValidator;
  readonly endpoint?: string;
  readonly fetch?: typeof fetch;
}): PagedDashboardClient {
  const client = new PagedDashboardClient({
    transport: createDashboardTransport({
      ...options,
      validate: options.validate ?? (validateDashboardQueryResponseV1 as DashboardWireValidator),
    }),
    privateDetailTransport: createPrivateDetailTransport({ validate: validatePrivateCodexTurnDetailV1 }),
    isHidden: () => document.hidden,
    onState: render,
  });
  activeClient = client;
  bindControls(client);
  document.addEventListener("visibilitychange", () => {
    if (!document.hidden) client.resume();
  });
  client.start();
  return client;
}

function bindControls(client: PagedDashboardClient): void {
  const refresh = document.createElement("button");
  refresh.id = "refresh-dashboard";
  refresh.type = "button";
  refresh.textContent = "Refresh snapshot";
  refresh.addEventListener("click", () => client.refresh());
  required<HTMLButtonElement>("clear-filters").insertAdjacentElement("afterend", refresh);

  const savedFilterSelect = required<HTMLSelectElement>("saved-filter");
  const saveFilter = required<HTMLButtonElement>("save-filter");
  const deleteFilter = required<HTMLButtonElement>("delete-filter");
  let savedFilters = loadSavedFilters();
  const renderSavedFilterOptions = (): void => {
    savedFilterSelect.replaceChildren(
      option(ALL, "Saved views"),
      ...savedFilters.map((saved, index) => option(String(index), savedFilterLabel(saved))),
    );
    deleteFilter.disabled = true;
  };
  const resetSavedFilterSelection = (): void => {
    savedFilterSelect.value = ALL;
    deleteFilter.disabled = true;
  };
  renderSavedFilterOptions();

  required<HTMLInputElement>("text-filter").maxLength = 512;
  for (const [key, id] of Object.entries(filterIds) as Array<[keyof typeof filterIds, string]>) {
    required<HTMLSelectElement>(id).addEventListener("change", () => {
      resetSavedFilterSelection();
      client.setFilters(readFilters(key));
    });
  }
  required<HTMLInputElement>("text-filter").addEventListener("input", () => {
    resetSavedFilterSelection();
    client.setFilters(readFilters());
  });
  required<HTMLButtonElement>("clear-filters").addEventListener("click", () => {
    for (const id of Object.values(filterIds)) required<HTMLSelectElement>(id).value = ALL;
    required<HTMLInputElement>("text-filter").value = "";
    resetSavedFilterSelection();
    client.setFilters({});
  });
  required<HTMLButtonElement>("trace-previous").addEventListener("click", () => client.previousTraces());
  required<HTMLButtonElement>("trace-next").addEventListener("click", () => client.nextTraces());
  required<HTMLButtonElement>("span-previous").addEventListener("click", () => client.previousSpans());
  required<HTMLButtonElement>("span-next").addEventListener("click", () => client.nextSpans());
  required<HTMLButtonElement>("details-close").addEventListener("click", closeDetails);
  saveFilter.addEventListener("click", () => {
    const dimensions = savedDimensionsFromDashboardFilters(client.state.filters);
    if (!isPersistableDimensions(dimensions)) return;
    const existing = savedFilters.findIndex((saved) => sameDimensions(saved, dimensions));
    if (existing >= 0) {
      savedFilterSelect.value = String(existing);
    } else {
      const next = [dimensions, ...savedFilters].slice(0, SAVED_FILTER_LIMIT);
      if (!persistSavedFilters(next)) {
        setText("filter-status", "Saved views are unavailable in this browser context.");
        return;
      }
      savedFilters = next;
      renderSavedFilterOptions();
      savedFilterSelect.value = "0";
    }
    deleteFilter.disabled = false;
  });
  savedFilterSelect.addEventListener("change", () => {
    const selected = savedFilters[Number(savedFilterSelect.value)];
    deleteFilter.disabled = selected === undefined;
    if (!selected) return;
    required<HTMLInputElement>("text-filter").value = "";
    client.setFilters(dashboardFiltersFromSavedDimensions(selected));
  });
  deleteFilter.addEventListener("click", () => {
    const index = Number(savedFilterSelect.value);
    if (!Number.isInteger(index) || index < 0 || index >= savedFilters.length) return;
    const next = savedFilters.filter((_, savedIndex) => savedIndex !== index);
    if (!persistSavedFilters(next)) {
      setText("filter-status", "The saved view could not be deleted in this browser context.");
      return;
    }
    savedFilters = next;
    renderSavedFilterOptions();
  });
}

export function savedDimensionsFromDashboardFilters(filters: DashboardFiltersV1): DimensionFilters {
  const single = (values: DashboardFiltersV1["repo"]): string | undefined =>
    values?.length === 1 ? values[0] : undefined;
  return {
    repo: single(filters.repo),
    session: single(filters.session),
    agent: single(filters.agent),
    model: single(filters.model),
  };
}

export function dashboardFiltersFromSavedDimensions(filters: DimensionFilters): DashboardFiltersV1 {
  return {
    ...(filters.repo === undefined ? {} : { repo: [filters.repo] }),
    ...(filters.session === undefined ? {} : { session: [filters.session] }),
    ...(filters.agent === undefined ? {} : { agent: [filters.agent] }),
    ...(filters.model === undefined ? {} : { model: [filters.model] }),
  };
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

function savedFilterLabel(filters: DimensionFilters): string {
  const parts = [filters.repo, filters.session, filters.agent, filters.model]
    .filter((value): value is string => value !== undefined)
    .map(shortId);
  return parts.length > 0 ? parts.join(" / ") : "All dimensions";
}

function readFilters(changed?: keyof typeof filterIds): DashboardFiltersV1 {
  const result: Record<string, unknown> = {};
  for (const [key, id] of Object.entries(filterIds) as Array<[keyof typeof filterIds, string]>) {
    const select = required<HTMLSelectElement>(id);
    if (select.value !== ALL && select.value !== "") result[key] = [select.value];
    if (key === changed && select.selectedOptions[0]) result[key] = [select.selectedOptions[0].value];
  }
  const text = required<HTMLInputElement>("text-filter").value.trim();
  if (text) result.text = text;
  return result as DashboardFiltersV1;
}

function render(state: PagedDashboardState): void {
  renderAvailability(state);
  renderFacets(state);
  renderKpis(state.summary.state === "complete" ? state.summary.kpis : undefined);
  renderTraces(state);
  renderSpans(state);
  renderDetails(state);
}

function renderAvailability(state: PagedDashboardState): void {
  const labels: Record<PagedDashboardState["availability"], string> = {
    idle: "Dashboard idle",
    loading: "Loading dashboard snapshot",
    current: "Current snapshot",
    stale: "Stale snapshot — newer observations are available",
    building: "Dashboard snapshot is building; retry scheduled",
    refresh_pending: "Visibility changed; a safe refresh is pending",
    expired: "Snapshot expired — refresh to continue",
    capacity: "Dashboard capacity limit reached",
    error: "Dashboard query error",
  };
  const suffix = state.scope?.coldExcluded ? " · Cold archive excluded" : "";
  const reason = state.reason && state.reason !== "cold_excluded" ? ` · ${state.reason.replaceAll("_", " ")}` : "";
  setText("filter-status", `${labels[state.availability]}${suffix}${reason}`);
  setText("quality-summary", state.scope?.coldExcluded ? "Hot/warm data; cold excluded" : labels[state.availability]);
  const facetDisclosure = facetLimitDisclosure(state.facetsLimited);
  const snapshotDetail = state.snapshot
    ? `Snapshot ${shortId(state.snapshot.id)} · generated ${formatTime(state.snapshot.generatedAt)}`
    : "No query snapshot is currently bound.";
  setText(
    "quality-detail",
    facetDisclosure ? `${snapshotDetail} · ${facetDisclosure}` : snapshotDetail,
  );
  required<HTMLButtonElement>("clear-filters").disabled = Object.keys(state.filters).length === 0;
  required<HTMLButtonElement>("refresh-dashboard").disabled =
    state.availability === "loading" || state.availability === "building";
  required<HTMLButtonElement>("save-filter").disabled =
    !isPersistableDimensions(savedDimensionsFromDashboardFilters(state.filters));
}

export function facetLimitDisclosure(facetsLimited: boolean): string | undefined {
  if (!facetsLimited) return undefined;
  return "Filter suggestions show up to 125 values per dimension. Totals and trace pages still cover all matching data.";
}

function renderFacets(state: PagedDashboardState): void {
  const values: Record<DashboardFacetDimensionV1, string[]> = { repo: [], session: [], agent: [], model: [] };
  for (const row of state.facets) values[row.dimension].push(row.value);
  for (const [key, id] of Object.entries(filterIds) as Array<[DashboardFacetDimensionV1, string]>) {
    const select = required<HTMLSelectElement>(id);
    const selected = state.filters[key]?.[0] ?? ALL;
    const options = [option(ALL, `All ${key}s`), ...values[key].map((value) => option(value, value))];
    if (selected !== ALL && !values[key].includes(selected)) options.push(option(selected, selected));
    select.replaceChildren(...options);
    select.value = selected;
  }
}

function renderKpis(kpis: DashboardKpisV1 | undefined): void {
  if (!kpis) {
    for (const id of ["kpi-sessions", "kpi-turns", "kpi-llm", "kpi-tools", "kpi-tokens", "kpi-cost", "kpi-errors"]) {
      setText(id, "Pending");
    }
    return;
  }
  setText("kpi-sessions", exact(kpis.sessions));
  setText("kpi-turns", exact(kpis.turns));
  setText("kpi-llm", exact(kpis.llm));
  setText("kpi-tools", exact(kpis.tools));
  setText("kpi-errors", exact(kpis.errors));
  setText("kpi-tokens", kpis.tokenStatus === "complete" && kpis.totalTokens !== null ? exact(kpis.totalTokens) : title(kpis.tokenStatus));
  setText("kpi-cost", kpis.costStatus === "estimated" && kpis.estimatedCost !== null
    ? `Exact estimated ${kpis.currency ?? "USD"} ${Number(kpis.estimatedCost.toPrecision(12))}`
    : title(kpis.costStatus));
}

function renderTraces(state: PagedDashboardState): void {
  const list = required<HTMLElement>("trace-list");
  list.replaceChildren(...state.traces.rows.map((trace) => traceButton(trace)));
  if (state.traces.rows.length === 0) list.replaceChildren(empty("No traces in this page."));
  setText("trace-count", state.traces.total ?? state.traces.rows.length);
  setText("trace-page-status", pageStatus(state.traces.page, state.traces.rows.length, state.traces.total, state.traces.nextCursor !== null));
  required<HTMLButtonElement>("trace-previous").disabled = !state.traces.canGoBack;
  required<HTMLButtonElement>("trace-next").disabled = state.traces.nextCursor === null;

  function traceButton(trace: DashboardTraceRowV1): HTMLElement {
    const button = document.createElement("button");
    button.type = "button";
    button.className = `trace-row${state.selectedTraceId === trace.traceId ? " active" : ""}`;
    button.setAttribute("aria-pressed", String(state.selectedTraceId === trace.traceId));
    const main = document.createElement("div");
    main.className = "trace-main";
    appendText(main, "span", shortId(trace.traceId), "mono");
    appendText(main, "span", trace.errorCount === 0 ? "ok" : `${trace.errorCount} error`, `badge ${trace.errorCount === 0 ? "ok" : "error"}`);
    const metadata = document.createElement("div");
    metadata.className = "trace-meta";
    appendText(metadata, "span", trace.repo);
    appendText(metadata, "span", `${trace.spanCount.toLocaleString()} spans`);
    appendText(metadata, "span", formatTime(trace.startTimeUnixMs));
    button.append(main, metadata);
    button.addEventListener("click", () => activeClient?.selectTrace(trace.traceId));
    return button;
  }
}

let activeClient: PagedDashboardClient | undefined;
type SpanOpenerSurface = "table" | "timeline";
interface SpanDetailsOpener {
  readonly element: HTMLButtonElement;
  readonly spanId: string;
  readonly surface: SpanOpenerSurface;
}
let detailsOpener: SpanDetailsOpener | undefined;
let pendingDetailsFocusSpanId: string | undefined;

function renderSpans(state: PagedDashboardState): void {
  const body = required<HTMLTableSectionElement>("span-table");
  body.replaceChildren(...state.spans.rows.map((span) => spanRow(span, state)));
  if (state.spans.rows.length === 0) {
    const cell = document.createElement("td");
    cell.colSpan = 9;
    cell.textContent = state.selectedTraceId ? "No spans in this page; continuation may still be available." : "Select a trace.";
    const row = document.createElement("tr");
    row.append(cell);
    body.replaceChildren(row);
  }
  setText("span-count", state.spans.total ?? state.spans.rows.length);
  setText("span-page-status", pageStatus(state.spans.page, state.spans.rows.length, state.spans.total, state.spans.nextCursor !== null));
  required<HTMLButtonElement>("span-previous").disabled = !state.spans.canGoBack;
  required<HTMLButtonElement>("span-next").disabled = state.spans.nextCursor === null;
  renderTimeline(state.spans.rows.slice(0, state.limits?.timelineRows ?? 120), state);
}

function spanRow(span: DashboardSpanRowV1, state: PagedDashboardState): HTMLTableRowElement {
  const row = document.createElement("tr");
  row.append(cellWithText(span.kind));
  const nameCell = document.createElement("td");
  const button = document.createElement("button");
  button.type = "button";
  button.className = "span-open";
  button.textContent = span.name;
  button.setAttribute("aria-controls", "span-details");
  button.setAttribute("aria-expanded", String(state.selectedSpan?.spanId === span.spanId));
  button.dataset.spanId = span.spanId;
  button.dataset.openerSurface = "table";
  button.addEventListener("click", () => openSpanDetails(span.spanId, "table", button));
  nameCell.append(button);
  if (span.toolName) appendText(nameCell, "div", span.toolName, "mono");
  row.append(nameCell);
  row.append(cellWithText(span.status), cellWithText(span.repo), cellWithText(span.turnId ?? "", "mono"));
  row.append(cellWithText(availabilityReason(span, "tokens")), cellWithText("Server-priced in detail"));
  row.append(cellWithText(duration(span.startTimeUnixMs, span.endTimeUnixMs)), cellWithText(shortId(span.parentSpanId ?? ""), "mono"));
  return row;
}

function renderTimeline(spans: readonly DashboardSpanRowV1[], state: PagedDashboardState): void {
  const list = required<HTMLElement>("timeline-list");
  const earliest = Math.min(...spans.map((span) => span.startTimeUnixMs));
  const latest = Math.max(...spans.map((span) => span.endTimeUnixMs ?? span.startTimeUnixMs));
  const extent = Math.max(1, latest - earliest);
  list.replaceChildren(...spans.map((span) => {
    const row = document.createElement("div");
    row.className = "timeline-row";
    const label = document.createElement("div");
    label.className = "timeline-label";
    const button = document.createElement("button");
    button.type = "button";
    button.className = "span-open";
    button.textContent = span.name;
    button.setAttribute("aria-controls", "span-details");
    button.setAttribute("aria-expanded", String(state.selectedSpan?.spanId === span.spanId));
    button.dataset.spanId = span.spanId;
    button.dataset.openerSurface = "timeline";
    button.addEventListener("click", () => openSpanDetails(span.spanId, "timeline", button));
    const details = document.createElement("span");
    details.className = "timeline-details";
    appendText(details, "span", span.status, `badge timeline-span-status ${statusClass(span.status)}`);
    appendText(details, "span", duration(span.startTimeUnixMs, span.endTimeUnixMs), "mono");
    label.append(button, details);
    const track = document.createElement("div");
    track.className = "timeline-track";
    const bar = document.createElement("span");
    bar.className = `timeline-bar ${statusClass(span.status)}`;
    bar.style.left = `${Math.max(0, Math.min(100, ((span.startTimeUnixMs - earliest) / extent) * 100)).toFixed(3)}%`;
    const end = span.endTimeUnixMs ?? span.startTimeUnixMs;
    bar.style.width = `${Math.max(0.8, Math.min(100, ((end - span.startTimeUnixMs) / extent) * 100)).toFixed(3)}%`;
    track.append(bar);
    row.append(label, track);
    return row;
  }));
  if (spans.length === 0) list.replaceChildren(empty(state.selectedTraceId ? "No timeline rows in this page." : "Select a trace."));
  setText("timeline-status", `${spans.length.toLocaleString()} spans in current bounded page.`);
}

function renderDetails(state: PagedDashboardState): void {
  const span = state.selectedSpan;
  const panel = required<HTMLElement>("span-details");
  const body = required<HTMLElement>("details-body");
  if (!span) {
    panel.classList.remove("open");
    body.replaceChildren(empty("Select a span to inspect safe projected details."));
    if (pendingDetailsFocusSpanId === undefined) detailsOpener = undefined;
    return;
  }
  panel.classList.add("open");
  const section = document.createElement("section");
  section.className = "detail-section";
  appendText(section, "h3", "Safe projected detail");
  const list = document.createElement("dl");
  list.className = "detail-grid";
  const rows: Array<[string, string]> = [
    ["Name", span.name], ["Kind", span.kind], ["Status", span.status], ["Repository", span.repo],
    ["Agent", span.agent.name ?? availabilityText(span.availability.model)],
    ["Model", span.agent.model ?? availabilityText(span.availability.model)],
    ["Session", span.sessionId ?? "Unavailable"], ["Turn", span.turnId ?? availabilityText(span.availability.turn)],
    ["Span", span.spanId], ["Parent", span.parentSpanId ?? "Root"],
  ];
  for (const [label, value] of rows) {
    appendText(list, "dt", label);
    appendText(list, "dd", value);
  }
  section.append(list);
  body.replaceChildren(section, privateDetailSection(state));
  if (pendingDetailsFocusSpanId === span.spanId) {
    pendingDetailsFocusSpanId = undefined;
    required<HTMLElement>("details-heading").focus();
  }
}

function privateDetailSection(state: PagedDashboardState): HTMLElement {
  const section = document.createElement("section");
  section.className = "detail-section";
  section.id = "private-detail";
  appendText(section, "h3", "Private local detail");
  const privateDetail = state.privateDetail;
  if (privateDetail.state === "eligible") {
    appendText(section, "p", "Sensitive content remains local and is fetched only when requested.", "detail-empty");
    const button = document.createElement("button");
    button.type = "button";
    button.textContent = "Load private detail";
    button.addEventListener("click", () => void activeClient?.loadPrivateDetail());
    section.append(button);
  } else if (privateDetail.state === "loading") {
    appendText(section, "p", "Loading private detail…", "detail-empty");
  } else if (privateDetail.state === "loaded") {
    appendText(section, "p", "Sensitive content stored only in this private local runtime. Review before sharing screenshots.", "private-banner");
    const location = document.createElement("dl");
    location.className = "detail-grid";
    appendText(location, "dt", "Opened from");
    appendText(location, "dd", privateDetail.detail.cwd);
    section.append(location);
    appendText(section, "h3", "Request");
    appendText(section, "pre", privateDetail.detail.inputMessages.join("\n\n"), "private-content").tabIndex = 0;
    appendText(section, "h3", "Response");
    appendText(
      section,
      "pre",
      privateDetail.detail.lastAssistantMessage ?? "Unavailable: Codex did not provide an assistant response in this notification.",
      "private-content",
    ).tabIndex = 0;
  } else if (privateDetail.state === "not_collected" || privateDetail.state === "unavailable" || privateDetail.state === "error") {
    appendText(section, "p", privateDetail.reason, "detail-empty");
    const retry = document.createElement("button");
    retry.type = "button";
    retry.textContent = "Retry private detail";
    retry.addEventListener("click", () => void activeClient?.loadPrivateDetail());
    section.append(retry);
  } else {
    appendText(
      section,
      "p",
      privateDetail.state === "not_applicable" ? privateDetail.reason : "Private detail is not loaded.",
      "detail-empty",
    );
  }
  return section;
}

function openSpanDetails(spanId: string, surface: SpanOpenerSurface, element: HTMLButtonElement): void {
  detailsOpener = { element, spanId, surface };
  pendingDetailsFocusSpanId = spanId;
  activeClient?.selectSpan(spanId);
}

function closeDetails(): void {
  const opener = detailsOpener;
  detailsOpener = undefined;
  pendingDetailsFocusSpanId = undefined;
  activeClient?.clearSelectedDetail();
  visibleSpanOpener(opener)?.focus();
}

function visibleSpanOpener(opener?: SpanDetailsOpener): HTMLButtonElement | undefined {
  if (opener && visible(opener.element)) return opener.element;
  const candidates = [...document.querySelectorAll<HTMLButtonElement>(".span-open")].filter(visible);
  if (!opener) return candidates[0];
  const sameSpan = candidates.filter((candidate) => candidate.dataset.spanId === opener.spanId);
  return sameSpan.find((candidate) => candidate.dataset.openerSurface === opener.surface)
    ?? sameSpan[0]
    ?? candidates[0];
}

function visible(element: HTMLButtonElement): boolean {
  return element.isConnected && !element.disabled && element.getClientRects().length > 0;
}

function availabilityReason(span: DashboardSpanRowV1, field: string): string {
  const reason = span.availabilityReasons.find((item) => item.field === field);
  return reason ? title(reason.reason) : "See detail";
}

function availabilityText(value: FieldAvailability): string {
  return `${title(value.state)} — ${title(value.reason)}`;
}

function pageStatus(page: number, rows: number, total: number | undefined, hasNext: boolean): string {
  const totalText = total === undefined ? "total pending" : `${total.toLocaleString()} total`;
  const continuation = rows === 0 && hasNext ? " · empty slice, continuation available" : hasNext ? " · more available" : " · end";
  return `Page ${page + 1} · ${rows.toLocaleString()} rows · ${totalText}${continuation}`;
}

function exact(value: number): string {
  return `Exact ${value.toLocaleString()}`;
}

function duration(start: number, end: number | null): string {
  return end === null ? "Unavailable" : `${Math.max(0, end - start).toLocaleString()} ms`;
}

function formatTime(value: string | number): string {
  const date = new Date(value);
  return Number.isNaN(date.valueOf()) ? String(value) : date.toLocaleString();
}

function shortId(value: string): string {
  return value.length > 22 ? `${value.slice(0, 10)}…${value.slice(-8)}` : value;
}

function title(value: string): string {
  return value.replaceAll("_", " ").replace(/^./, (character) => character.toUpperCase());
}

function statusClass(status: string): "ok" | "error" | "warning" {
  if (status === "ok") return "ok";
  if (status === "error") return "error";
  return "warning";
}

function option(value: string, label: string): HTMLOptionElement {
  const result = document.createElement("option");
  result.value = value;
  result.textContent = label;
  return result;
}

function empty(message: string): HTMLElement {
  const result = document.createElement("p");
  result.className = "detail-empty";
  result.textContent = message;
  return result;
}

function cellWithText(value: string, className?: string): HTMLTableCellElement {
  const result = document.createElement("td");
  result.textContent = value;
  if (className) result.className = className;
  return result;
}

function appendText(parent: HTMLElement, tag: string, value: string, className?: string): HTMLElement {
  const child = document.createElement(tag);
  child.textContent = value;
  if (className) child.className = className;
  parent.append(child);
  return child;
}

function setText(id: string, value: string | number): void {
  required(id).textContent = typeof value === "number" ? value.toLocaleString() : value;
}

function required<T extends HTMLElement = HTMLElement>(id: string): T {
  const element = document.getElementById(id);
  if (!element) throw new Error(`Missing paged dashboard shell element: ${id}`);
  return element as T;
}
