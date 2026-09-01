import {
  Activity,
  Archive,
  Check,
  Database,
  Gauge,
  HeartPulse,
  RefreshCw,
  RotateCcw,
  Save,
  Settings2,
  ShieldCheck,
  SlidersHorizontal,
  X,
  XCircle,
  createIcons,
} from "lucide";
import validateConfig from "./generated/validate-local-runtime-config-v2.js";
import type { LocalRuntimeConfigV2 } from "./generated/local-runtime-config-v2.js";

type FieldPath =
  | "collection.file_reconcile_interval_ms"
  | "collection.flush_interval_ms"
  | "collection.max_batch_records"
  | "collection.max_batch_bytes"
  | "collection.active_heartbeat_interval_ms"
  | "collection.idle_heartbeat_interval_ms"
  | "collection.local_storage_budget_bytes"
  | "retention.max_record_age_days"
  | "retention.max_archive_records"
  | "retention.max_archive_bytes";

type Envelope = {
  config: LocalRuntimeConfigV2;
  defaults: LocalRuntimeConfigV2;
  revision: string;
  collection_mode: "manual_import";
};

type ApiError = { code?: string; message?: string };

type Field = {
  path: FieldPath;
  label: string;
  description: string;
  min: number;
  max: number;
  step: number;
  unit: string;
  format: (value: number) => string;
};

const fields: Record<FieldPath, Field> = {
  "collection.file_reconcile_interval_ms": {
    path: "collection.file_reconcile_interval_ms",
    label: "파일 확인 주기",
    description: "새 handoff 파일을 다시 확인하는 간격",
    min: 1_000,
    max: 60_000,
    step: 1,
    unit: "ms",
    format: formatDuration,
  },
  "collection.flush_interval_ms": {
    path: "collection.flush_interval_ms",
    label: "기록 반영 주기",
    description: "허용된 배치를 durable storage에 반영하는 간격",
    min: 1_000,
    max: 60_000,
    step: 1,
    unit: "ms",
    format: formatDuration,
  },
  "collection.max_batch_records": {
    path: "collection.max_batch_records",
    label: "배치 레코드",
    description: "한 번에 처리할 최대 레코드 수",
    min: 1,
    max: 500,
    step: 1,
    unit: "records",
    format: (value) => `${formatNumber(value)}개`,
  },
  "collection.max_batch_bytes": {
    path: "collection.max_batch_bytes",
    label: "배치 크기",
    description: "한 번에 처리할 최대 byte 크기",
    min: 16_384,
    max: 2_097_152,
    step: 1,
    unit: "bytes",
    format: formatBytes,
  },
  "collection.active_heartbeat_interval_ms": {
    path: "collection.active_heartbeat_interval_ms",
    label: "활성 heartbeat",
    description: "작업 중 source 상태를 확인하는 간격",
    min: 30_000,
    max: 300_000,
    step: 1,
    unit: "ms",
    format: formatDuration,
  },
  "collection.idle_heartbeat_interval_ms": {
    path: "collection.idle_heartbeat_interval_ms",
    label: "유휴 heartbeat",
    description: "작업이 없을 때 source 상태를 확인하는 간격",
    min: 120_000,
    max: 900_000,
    step: 1,
    unit: "ms",
    format: formatDuration,
  },
  "collection.local_storage_budget_bytes": {
    path: "collection.local_storage_budget_bytes",
    label: "로컬 저장 한도",
    description: "수집 데이터가 사용할 수 있는 최대 디스크 예산",
    min: 268_435_456,
    max: 21_474_836_480,
    step: 1,
    unit: "bytes",
    format: formatBytes,
  },
  "retention.max_record_age_days": {
    path: "retention.max_record_age_days",
    label: "보관 기간",
    description: "이 기간보다 오래된 trace는 만료 대상",
    min: 1,
    max: 3_650,
    step: 1,
    unit: "days",
    format: (value) => `${formatNumber(value)}일`,
  },
  "retention.max_archive_records": {
    path: "retention.max_archive_records",
    label: "archive 레코드",
    description: "하나의 private archive에 담을 최대 레코드 수",
    min: 1,
    max: 100_000,
    step: 1,
    unit: "records",
    format: (value) => `${formatNumber(value)}개`,
  },
  "retention.max_archive_bytes": {
    path: "retention.max_archive_bytes",
    label: "archive 크기",
    description: "하나의 private archive에 담을 최대 크기",
    min: 65_536,
    max: 268_435_456,
    step: 1,
    unit: "bytes",
    format: formatBytes,
  },
};

const rootElement = document.querySelector("#app");
if (!(rootElement instanceof HTMLDivElement)) throw new Error("settings root is missing");
const app = rootElement;

const SESSION_TOKEN_KEY = "agent-observability.settings.session.v1";
const fragmentToken = new URLSearchParams(location.hash.slice(1)).get("session") ?? "";
let token = fragmentToken || readSessionToken();
if (fragmentToken) writeSessionToken(fragmentToken);
history.replaceState(null, "", `${location.pathname}${location.search}`);
let persisted: LocalRuntimeConfigV2 | null = null;
let draft: LocalRuntimeConfigV2 | null = null;
let defaults: LocalRuntimeConfigV2 | null = null;
let revision = "";
let busy = false;
let conflicted = false;
let heartbeatTimer: number | undefined;
let navigationObserver: IntersectionObserver | undefined;
let lastUserActivity = Date.now();

for (const eventName of ["pointerdown", "keydown", "input", "scroll"]) {
  document.addEventListener(eventName, () => {
    lastUserActivity = Date.now();
  }, { passive: true });
}

window.addEventListener("beforeunload", (event) => {
  if (!isDirty()) return;
  event.preventDefault();
  event.returnValue = "";
});

void bootstrap();

async function bootstrap(): Promise<void> {
  renderLoading();
  if (!token) {
    renderExpired();
    return;
  }
  try {
    applyEnvelope(await api<Envelope>("/api/config"));
    renderSettings();
    heartbeatTimer = window.setInterval(() => void heartbeat(), 20_000);
  } catch (error) {
    const apiError = error as Error & { code?: string };
    if (apiError.code === "invalid_session" || apiError.code === "network_failure") {
      expireSession();
    } else {
      renderUnavailable(messageOf(error));
    }
  }
}

function renderLoading(): void {
  app.innerHTML = `<main class="center-state" aria-busy="true">
    <i data-lucide="settings-2" aria-hidden="true"></i>
    <h1>로컬 설정을 불러오는 중</h1>
    <p>Rust runtime의 현재 정책을 확인하고 있습니다.</p>
  </main>`;
  mountIcons();
}

function renderUnavailable(message: string): void {
  app.innerHTML = `<main class="center-state" role="alert">
    <i data-lucide="x-circle" aria-hidden="true"></i>
    <h1>설정을 불러오지 못했습니다</h1>
    <p id="fatal-message"></p>
    <button class="button primary" id="retry"><i data-lucide="refresh-cw"></i>다시 시도</button>
  </main>`;
  setText("fatal-message", message);
  document.querySelector("#retry")?.addEventListener("click", () => void bootstrap());
  mountIcons();
}

function renderExpired(): void {
  window.clearInterval(heartbeatTimer);
  app.innerHTML = `<main class="center-state" role="alert">
    <i data-lucide="shield-check" aria-hidden="true"></i>
    <h1>설정 세션이 종료되었습니다</h1>
    <p>터미널에서 <code>agent-observability ui</code>를 실행해 새 세션을 여세요.</p>
  </main>`;
  mountIcons();
}

function renderSettings(focusTarget?: string): void {
  if (!draft) return;
  app.innerHTML = `<div class="app-shell">
    <header class="topbar">
      <div class="brand"><span class="brand-mark"><i data-lucide="settings-2"></i></span><span>Agent Observability</span></div>
      <div class="topbar-actions">
        <span class="session-badge"><i data-lucide="shield-check"></i>로컬 전용 · 세션 활성</span>
        <button class="icon-button" id="close-session" type="button" title="설정 세션 닫기" aria-label="설정 세션 닫기"><i data-lucide="x"></i></button>
      </div>
    </header>
    <div class="workspace">
      <nav class="section-nav" aria-label="설정 영역">
        <p class="nav-label">설정</p>
        <a href="#overview" class="active" aria-current="page"><i data-lucide="gauge"></i>개요</a>
        <a href="#collection"><i data-lucide="activity"></i>수집</a>
        <a href="#storage"><i data-lucide="database"></i>저장소</a>
        <a href="#retention"><i data-lucide="archive"></i>보관</a>
        <div class="nav-note"><strong>입력 방식</strong><span>수동 private handoff</span><span>자동 producer 미포함</span></div>
      </nav>
      <main class="settings-main">
        <form id="settings-form" novalidate>
          ${overviewSection(draft)}
          ${collectionSection(draft)}
          ${storageSection(draft)}
          ${retentionSection(draft)}
        </form>
      </main>
    </div>
    <div class="save-band" id="save-band">
      <div class="save-state"><span class="state-dot"></span><strong id="save-title" tabindex="-1">저장됨</strong><span id="save-detail">현재 설정과 같습니다.</span></div>
      <div class="save-actions">
        <button class="button ghost" id="discard" type="button" disabled>변경 취소</button>
        <button class="button secondary" id="reset" type="button"><i data-lucide="rotate-ccw"></i>기본값</button>
        <button class="button primary" id="save" type="submit" form="settings-form" disabled><i data-lucide="save"></i>설정 저장</button>
      </div>
    </div>
    <div class="toast" id="toast" role="status" aria-live="polite"></div>
    <dialog id="reset-dialog" aria-labelledby="reset-title">
      <div class="dialog-heading"><i data-lucide="rotate-ccw"></i><div><h2 id="reset-title">기본값으로 복원</h2><p>수집, 저장소, 보관 정책의 편집값을 초기값으로 바꿉니다.</p></div></div>
      <div class="dialog-actions"><button class="button ghost" id="cancel-reset" type="button">취소</button><button class="button primary" id="confirm-reset" type="button">편집값 복원</button></div>
    </dialog>
    <dialog id="close-dialog" aria-labelledby="close-title">
      <div class="dialog-heading"><i data-lucide="x-circle"></i><div><h2 id="close-title">저장하지 않은 변경 닫기</h2><p>현재 편집값은 저장되지 않았습니다. 설정 세션을 종료하면 변경을 잃습니다.</p></div></div>
      <p class="dialog-error" id="close-error" role="alert"></p>
      <div class="dialog-actions"><button class="button ghost" id="cancel-close" type="button">계속 편집</button><button class="button danger" id="confirm-close" type="button">변경 버리고 닫기</button></div>
    </dialog>
  </div>`;
  bindEvents();
  updateAllVisuals();
  updateDirtyState();
  mountIcons();
  if (focusTarget) {
    requestAnimationFrame(() => document.querySelector<HTMLElement>(`#${focusTarget}`)?.focus());
  }
}

function overviewSection(config: LocalRuntimeConfigV2): string {
  const storage = fields["collection.local_storage_budget_bytes"].format(
    config.collection.local_storage_budget_bytes,
  );
  return `<section class="settings-section overview" id="overview" aria-labelledby="overview-title">
    <div class="section-heading"><div><p class="eyebrow">Standalone</p><h1 id="overview-title">로컬 수집 정책</h1><p>정적 리포트와 독립적으로 저장·보관 한도를 관리합니다.</p></div>
      <label class="collection-toggle"><span><strong>수집 허용</strong><small id="enabled-copy">${config.enabled ? "private handoff를 처리합니다" : "설정값을 유지한 채 처리를 중지합니다"}</small></span><input type="checkbox" id="enabled" ${config.enabled ? "checked" : ""}><span class="toggle-track" aria-hidden="true"><span></span></span></label>
    </div>
    <div class="policy-strip" aria-label="정책 요약">
      ${summaryItem("activity", "확인 주기", formatDuration(config.collection.file_reconcile_interval_ms))}
      ${summaryItem("sliders-horizontal", "배치 상한", `${formatNumber(config.collection.max_batch_records)}개`)}
      ${summaryItem("database", "저장 한도", storage)}
      ${summaryItem("archive", "보관 기간", `${formatNumber(config.retention.max_record_age_days)}일`)}
    </div>
    <div class="policy-notice"><i data-lucide="shield-check"></i><div><strong>이 화면은 로컬 정책만 변경합니다.</strong><span>외부 전송 없이 Rust가 검증한 뒤 private config에 원자적으로 저장합니다.</span></div></div>
  </section>`;
}

function collectionSection(config: LocalRuntimeConfigV2): string {
  return `<section class="settings-section" id="collection" aria-labelledby="collection-title">
    ${sectionTitle("activity", "수집", "파일 확인과 durable 기록 반영 간격")}
    <div class="section-grid">
      <div class="field-grid">${fieldControl(fields["collection.file_reconcile_interval_ms"], config)}${fieldControl(fields["collection.flush_interval_ms"], config)}</div>
      ${dualTimeline(
        "cadence-visual",
        "수집 cadence",
        "확인",
        fields["collection.file_reconcile_interval_ms"],
        "반영",
        fields["collection.flush_interval_ms"],
        "1초",
        "60초",
      )}
    </div>
    <div class="subsection">
      <div class="subsection-heading"><h3>배치 및 상태 확인</h3><p>처리량과 source 상태 확인 간격을 bounded policy로 제한합니다.</p></div>
      <div class="section-grid">
        <div class="field-grid">${fieldControl(fields["collection.max_batch_records"], config)}${fieldControl(fields["collection.max_batch_bytes"], config)}${fieldControl(fields["collection.active_heartbeat_interval_ms"], config)}${fieldControl(fields["collection.idle_heartbeat_interval_ms"], config)}</div>
        <div class="visual-stack">
          ${singleRuler("batch-records-visual", "배치 레코드 상한", fields["collection.max_batch_records"], "1", "500")}
          ${singleRuler("batch-bytes-visual", "배치 크기 상한", fields["collection.max_batch_bytes"], "16 KiB", "2 MiB")}
          ${dualTimeline("heartbeat-visual", "Heartbeat 간격", "활성", fields["collection.active_heartbeat_interval_ms"], "유휴", fields["collection.idle_heartbeat_interval_ms"], "30초", "15분", true, 30000, 900000)}
        </div>
      </div>
    </div>
  </section>`;
}

function storageSection(config: LocalRuntimeConfigV2): string {
  return `<section class="settings-section" id="storage" aria-labelledby="storage-title">
    ${sectionTitle("database", "저장소", "로컬 데이터가 넘지 못하는 디스크 예산")}
    <div class="section-grid">
      <div class="field-grid single">${fieldControl(fields["collection.local_storage_budget_bytes"], config)}</div>
      ${singleRuler("storage-visual", "설정 저장 한도", fields["collection.local_storage_budget_bytes"], "256 MiB", "20 GiB", true, "현재 사용량이 아닌 허용 한도")}
    </div>
  </section>`;
}

function retentionSection(config: LocalRuntimeConfigV2): string {
  return `<section class="settings-section" id="retention" aria-labelledby="retention-title">
    ${sectionTitle("archive", "보관", "만료 대상과 private archive 크기 정책")}
    <div class="section-grid">
      <div class="field-grid">${fieldControl(fields["retention.max_record_age_days"], config)}${fieldControl(fields["retention.max_archive_records"], config)}${fieldControl(fields["retention.max_archive_bytes"], config)}</div>
      <div class="visual-stack">
        ${singleRuler("retention-visual", "보관 기간", fields["retention.max_record_age_days"], "1일", "10년", true, "cutoff보다 오래된 trace는 만료 대상")}
        ${singleRuler("archive-records-visual", "Archive 레코드 상한", fields["retention.max_archive_records"], "1", "100k", true)}
        ${singleRuler("archive-bytes-visual", "Archive 크기 상한", fields["retention.max_archive_bytes"], "64 KiB", "256 MiB", true)}
      </div>
    </div>
    <div class="retention-note"><i data-lucide="archive"></i><span>보관 기간을 줄여도 즉시 삭제하지 않습니다. cleanup은 별도의 retention plan/apply 경계를 따릅니다.</span></div>
  </section>`;
}

function sectionTitle(icon: string, title: string, description: string): string {
  return `<div class="section-title"><span class="section-icon"><i data-lucide="${icon}"></i></span><div><h2 id="${title === "수집" ? "collection" : title === "저장소" ? "storage" : "retention"}-title">${title}</h2><p>${description}</p></div></div>`;
}

function summaryItem(icon: string, label: string, value: string): string {
  return `<div class="summary-item"><i data-lucide="${icon}"></i><span>${label}</span><strong>${value}</strong></div>`;
}

function fieldControl(field: Field, config: LocalRuntimeConfigV2): string {
  const value = getValue(config, field.path);
  const id = field.path.replaceAll(".", "-");
  return `<div class="field" data-field="${field.path}">
    <label for="${id}">${field.label}<span class="changed-label" aria-hidden="true">변경됨</span></label>
    <p id="${id}-help">${field.description}</p>
    <div class="number-control"><input id="${id}" name="${field.path}" data-path="${field.path}" type="number" value="${value}" min="${field.min}" max="${field.max}" step="${field.step}" inputmode="numeric" required aria-describedby="${id}-help ${id}-readout"><span>${field.unit}</span></div>
    <output id="${id}-readout" for="${id}">${field.format(value)}</output>
    <span class="field-error" id="${id}-error"></span>
  </div>`;
}

function singleRuler(id: string, title: string, field: Field, min: string, max: string, logarithmic = false, caption = "설정된 정책 상한"): string {
  return `<figure class="policy-visual" id="${id}" data-path="${field.path}" data-log="${logarithmic}">
    <figcaption><span>${title}</span><strong data-visual-value></strong></figcaption>
    <div class="ruler" aria-hidden="true"><span class="ruler-marker" data-marker></span></div>
    <div class="ruler-labels"><span>${min}</span><span>${max}</span></div>
    <p>${caption}</p>
  </figure>`;
}

function dualTimeline(id: string, title: string, firstLabel: string, first: Field, secondLabel: string, second: Field, min: string, max: string, logarithmic = false, sharedMin?: number, sharedMax?: number): string {
  const sharedScale = sharedMin === undefined || sharedMax === undefined ? "" : ` data-min="${sharedMin}" data-max="${sharedMax}"`;
  return `<figure class="policy-visual timeline" id="${id}" data-log="${logarithmic}" data-first-label="${firstLabel}" data-second-label="${secondLabel}"${sharedScale}>
    <figcaption><span>${title}</span><strong data-dual-value></strong></figcaption>
    <div class="timeline-track" aria-hidden="true">
      <span class="timeline-marker first" data-marker data-path="${first.path}"><b>${firstLabel}</b></span>
      <span class="timeline-marker second" data-marker data-path="${second.path}"><b>${secondLabel}</b></span>
    </div>
    <div class="ruler-labels"><span>${min}</span><span>${max}</span></div>
    <p>각 marker는 설정 간격이며 실시간 처리량이 아닙니다.</p>
  </figure>`;
}

function bindEvents(): void {
  const form = document.querySelector<HTMLFormElement>("#settings-form");
  form?.addEventListener("submit", (event) => {
    event.preventDefault();
    void saveDraft();
  });
  form?.addEventListener("input", handleInput);
  document.querySelector("#enabled")?.addEventListener("change", handleEnabled);
  document.querySelector("#discard")?.addEventListener("click", discardChanges);
  document.querySelector("#reset")?.addEventListener("click", openResetDialog);
  document.querySelector("#cancel-reset")?.addEventListener("click", closeResetDialog);
  document.querySelector("#confirm-reset")?.addEventListener("click", resetDefaults);
  document.querySelector("#close-session")?.addEventListener("click", requestCloseSession);
  document.querySelector("#cancel-close")?.addEventListener("click", closeCloseDialog);
  document.querySelector("#confirm-close")?.addEventListener("click", () => void closeSession());
  document.querySelectorAll<HTMLDialogElement>("dialog").forEach((dialog) => {
    dialog.addEventListener("keydown", trapDialogFocus);
  });
  document.querySelectorAll<HTMLAnchorElement>(".section-nav a").forEach((link) => {
    link.addEventListener("click", () => {
      setActiveNavigation(link.hash);
    });
  });
  navigationObserver?.disconnect();
  navigationObserver = new IntersectionObserver(
    (entries) => {
      const visible = entries.find((entry) => entry.isIntersecting);
      if (visible) setActiveNavigation(`#${visible.target.id}`);
    },
    { rootMargin: "-32% 0px -60% 0px", threshold: 0 },
  );
  document
    .querySelectorAll(".settings-section")
    .forEach((section) => navigationObserver?.observe(section));
}

function trapDialogFocus(event: KeyboardEvent): void {
  if (event.key !== "Tab") return;
  const dialog = event.currentTarget;
  if (!(dialog instanceof HTMLDialogElement) || !dialog.open) return;
  const controls = Array.from(
    dialog.querySelectorAll<HTMLElement>(
      "button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex='-1'])",
    ),
  );
  if (controls.length === 0) return;
  const first = controls[0]!;
  const last = controls.at(-1)!;
  const active = document.activeElement;
  if (event.shiftKey && (active === first || !dialog.contains(active))) {
    event.preventDefault();
    last.focus();
  } else if (!event.shiftKey && (active === last || !dialog.contains(active))) {
    event.preventDefault();
    first.focus();
  }
}

function handleInput(event: Event): void {
  const input = event.target;
  if (!(input instanceof HTMLInputElement) || !draft) return;
  const path = input.dataset.path as FieldPath | undefined;
  if (!path) return;
  const value = Number(input.value);
  if (Number.isFinite(value)) setValue(draft, path, value);
  clearFieldError(path);
  updateAllVisuals();
  updateDirtyState();
}

function handleEnabled(event: Event): void {
  const input = event.target;
  if (!(input instanceof HTMLInputElement) || !draft) return;
  draft.enabled = input.checked;
  setText(
    "enabled-copy",
    input.checked ? "private handoff를 처리합니다" : "설정값을 유지한 채 처리를 중지합니다",
  );
  updateDirtyState();
}

function updateAllVisuals(): void {
  if (!draft) return;
  document.querySelectorAll<HTMLElement>("[data-visual-value]").forEach((output) => {
    const visual = output.closest<HTMLElement>("[data-path]");
    const path = visual?.dataset.path as FieldPath | undefined;
    if (path) output.textContent = fields[path].format(getValue(draft!, path));
  });
  document.querySelectorAll<HTMLElement>("[data-marker]").forEach((marker) => {
    const owner = marker.closest<HTMLElement>(".policy-visual");
    const path = (marker.dataset.path ?? owner?.dataset.path) as FieldPath | undefined;
    if (!path) return;
    const field = fields[path];
    const minimum = Number(owner?.dataset.min ?? field.min);
    const maximum = Number(owner?.dataset.max ?? field.max);
    marker.style.left = `${position(getValue(draft!, path), minimum, maximum, owner?.dataset.log === "true")}%`;
  });
  document.querySelectorAll<HTMLElement>("[data-dual-value]").forEach((output) => {
    const visual = output.closest<HTMLElement>(".policy-visual");
    const paths = Array.from(visual?.querySelectorAll<HTMLElement>("[data-path]") ?? []).map(
      (item) => item.dataset.path as FieldPath,
    );
    const labels = [visual?.dataset.firstLabel ?? "첫 번째", visual?.dataset.secondLabel ?? "두 번째"];
    output.textContent = paths
      .map((path, index) => `${labels[index]} ${fields[path].format(getValue(draft!, path))}`)
      .join(" · ");
  });
  (Object.keys(fields) as FieldPath[]).forEach((path) => {
    const id = path.replaceAll(".", "-");
    const output = document.querySelector<HTMLOutputElement>(`#${id}-readout`);
    if (output) output.value = fields[path].format(getValue(draft!, path));
  });
  updateOverviewSummary();
}

function updateOverviewSummary(): void {
  if (!draft) return;
  const items = document.querySelectorAll<HTMLElement>(".summary-item strong");
  const values = [
    formatDuration(draft.collection.file_reconcile_interval_ms),
    `${formatNumber(draft.collection.max_batch_records)}개`,
    formatBytes(draft.collection.local_storage_budget_bytes),
    `${formatNumber(draft.retention.max_record_age_days)}일`,
  ];
  items.forEach((item, index) => {
    item.textContent = values[index] ?? "";
  });
}

function updateDirtyState(): void {
  if (!draft || !persisted) return;
  const changed = changedPaths(draft, persisted);
  const dirty = draft.enabled !== persisted.enabled || changed.length > 0;
  document.querySelector<HTMLElement>("#save-band")?.classList.toggle("dirty", dirty);
  setText("save-title", conflicted ? "외부 변경 감지" : dirty ? `${changed.length + Number(draft.enabled !== persisted.enabled)}개 변경` : "저장됨");
  setText("save-detail", conflicted ? "최신 설정을 다시 불러온 뒤 편집하세요." : dirty ? "저장 전까지 이 브라우저에만 유지됩니다." : "현재 설정과 같습니다.");
  setDisabled("save", !dirty || busy || conflicted);
  setDisabled("discard", !dirty || busy);
  setDisabled("reset", busy);
  document.querySelectorAll<HTMLElement>("[data-field]").forEach((row) => {
    row.classList.toggle("changed", changed.includes(row.dataset.field as FieldPath));
  });
}

async function saveDraft(): Promise<void> {
  if (!draft || busy || conflicted) return;
  clearErrors();
  const form = document.querySelector<HTMLFormElement>("#settings-form");
  if (form && !form.checkValidity()) {
    form.reportValidity();
    showToast("비어 있거나 허용 범위를 벗어난 값을 확인하세요.", "error");
    return;
  }
  if (!validateConfig(draft)) {
    const errors = validateConfig.errors ?? [];
    for (const error of errors) {
      const path = error.instancePath?.replace(/^\//, "").replaceAll("/", ".") as FieldPath;
      if (path in fields) showFieldError(path, error.message ?? "허용 범위를 확인하세요.");
    }
    focusFirstInvalid();
    showToast("허용 범위를 벗어난 값을 확인하세요.", "error");
    return;
  }
  busy = true;
  setBusy(true);
  try {
    const envelope = await api<Envelope>("/api/config", {
      method: "PUT",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ config: draft, revision }),
    });
    applyEnvelope(envelope);
    renderSettings("save-title");
    showToast("설정을 저장했습니다.", "success");
  } catch (error) {
    const apiError = error as Error & { code?: string };
    if (apiError.code === "config_conflict") {
      try {
        await rebaseDraftOnLatest();
        showToast("최신 설정을 불러와 내 변경만 다시 적용했습니다. 검토 후 저장하세요.", "error");
      } catch (rebaseError) {
        const rebaseApiError = rebaseError as Error & { code?: string };
        if (
          rebaseApiError.code === "invalid_session" ||
          rebaseApiError.code === "network_failure"
        ) {
          expireSession();
          return;
        }
        showToast("최신 설정을 불러오지 못했습니다. 편집값은 유지됩니다. 다시 저장해 재시도하세요.", "error");
      }
    } else if (apiError.code === "invalid_session" || apiError.code === "network_failure") {
      expireSession();
      return;
    } else {
      showToast("설정을 저장하지 못했습니다. reason=" + (apiError.code ?? "request_failed"), "error");
    }
  } finally {
    busy = false;
    setBusy(false);
    updateDirtyState();
  }
}

async function rebaseDraftOnLatest(): Promise<void> {
  if (!draft || !persisted) return;
  const localDraft = structuredClone(draft);
  const localBase = structuredClone(persisted);
  const changed = changedPaths(localDraft, localBase);
  const enabledChanged = localDraft.enabled !== localBase.enabled;
  const latest = await api<Envelope>("/api/config");
  applyEnvelope(latest);
  if (!draft) return;
  for (const path of changed) setValue(draft, path, getValue(localDraft, path));
  if (enabledChanged) draft.enabled = localDraft.enabled;
  conflicted = false;
  renderSettings("save-title");
}

function discardChanges(): void {
  if (!persisted) return;
  draft = structuredClone(persisted);
  conflicted = false;
  renderSettings("save-title");
  showToast("저장하지 않은 변경을 취소했습니다.", "neutral");
}

function openResetDialog(): void {
  document.querySelector<HTMLDialogElement>("#reset-dialog")?.showModal();
}

function closeResetDialog(): void {
  document.querySelector<HTMLDialogElement>("#reset-dialog")?.close();
  document.querySelector<HTMLButtonElement>("#reset")?.focus();
}

function resetDefaults(): void {
  if (!defaults) return;
  draft = structuredClone(defaults);
  closeResetDialog();
  renderSettings("reset");
  showToast("기본값을 편집값에 적용했습니다. 저장해야 반영됩니다.", "neutral");
}

async function closeSession(): Promise<void> {
  if (busy) return;
  busy = true;
  setBusy(true);
  setText("close-error", "");
  try {
    await api<void>("/api/shutdown", { method: "POST" });
    if (persisted) draft = structuredClone(persisted);
    conflicted = false;
    expireSession();
  } catch (error) {
    const apiError = error as Error & { code?: string };
    if (apiError.code === "invalid_session") {
      if (persisted) draft = structuredClone(persisted);
      conflicted = false;
      expireSession();
      return;
    }
    setText(
      "close-error",
      "세션을 닫지 못했습니다. 로컬 process 연결을 확인하고 다시 시도하세요.",
    );
    document.querySelector<HTMLButtonElement>("#confirm-close")?.focus();
  } finally {
    busy = false;
    if (token) {
      setBusy(false);
      updateDirtyState();
    }
  }
}

function requestCloseSession(): void {
  if (isDirty()) {
    document.querySelector<HTMLDialogElement>("#close-dialog")?.showModal();
  } else {
    void closeSession();
  }
}

function closeCloseDialog(): void {
  document.querySelector<HTMLDialogElement>("#close-dialog")?.close();
  document.querySelector<HTMLButtonElement>("#close-session")?.focus();
}

async function heartbeat(): Promise<void> {
  if (Date.now() - lastUserActivity >= 60_000) return;
  try {
    await api<void>("/api/heartbeat", { method: "POST" });
  } catch {
    expireSession();
  }
}

function expireSession(): void {
  window.clearInterval(heartbeatTimer);
  token = "";
  clearSessionToken();
  renderExpired();
}

function readSessionToken(): string {
  try {
    return sessionStorage.getItem(SESSION_TOKEN_KEY) ?? "";
  } catch {
    return "";
  }
}

function writeSessionToken(value: string): void {
  try {
    sessionStorage.setItem(SESSION_TOKEN_KEY, value);
  } catch {
    // In-memory use remains available when browser storage is disabled.
  }
}

function clearSessionToken(): void {
  try {
    sessionStorage.removeItem(SESSION_TOKEN_KEY);
  } catch {
    // The in-memory token is already cleared.
  }
}

function setActiveNavigation(hash: string): void {
  document.querySelectorAll<HTMLAnchorElement>(".section-nav a").forEach((item) => {
    const active = item.hash === hash;
    item.classList.toggle("active", active);
    if (active) item.setAttribute("aria-current", "page");
    else item.removeAttribute("aria-current");
  });
}

function isDirty(): boolean {
  return Boolean(
    draft && persisted &&
      (draft.enabled !== persisted.enabled || changedPaths(draft, persisted).length > 0),
  );
}

async function api<T>(path: string, init: RequestInit = {}): Promise<T> {
  const headers = new Headers(init.headers);
  headers.set("x-agent-observability-session", token);
  let response: Response;
  try {
    response = await fetch(path, { ...init, headers, cache: "no-store" });
  } catch {
    const error = new Error("로컬 설정 process에 연결할 수 없습니다.") as Error & { code?: string };
    error.code = "network_failure";
    throw error;
  }
  if (!response.ok) {
    const body = (await response.json().catch(() => ({}))) as ApiError;
    const error = new Error(body.message ?? `요청이 실패했습니다 (${response.status}).`) as Error & {
      code?: string;
    };
    if (body.code) error.code = body.code;
    throw error;
  }
  if (response.status === 204) return undefined as T;
  return (await response.json()) as T;
}

function applyEnvelope(envelope: Envelope): void {
  persisted = structuredClone(envelope.config);
  draft = structuredClone(envelope.config);
  defaults = structuredClone(envelope.defaults);
  revision = envelope.revision;
  conflicted = false;
}

function setBusy(value: boolean): void {
  document.querySelector("#settings-form")?.setAttribute("aria-busy", String(value));
  setText("save-title", value ? "저장 중" : "저장됨");
  document.querySelectorAll<HTMLButtonElement>("button").forEach((button) => {
    if (button.id !== "close-session") button.disabled = value;
  });
}

function showFieldError(path: FieldPath, message: string): void {
  const id = path.replaceAll(".", "-");
  const input = document.querySelector<HTMLInputElement>(`#${id}`);
  input?.setAttribute("aria-invalid", "true");
  input?.setAttribute("aria-describedby", `${id}-help ${id}-readout ${id}-error`);
  setText(`${id}-error`, message);
}

function clearFieldError(path: FieldPath): void {
  const id = path.replaceAll(".", "-");
  document.querySelector<HTMLInputElement>(`#${id}`)?.removeAttribute("aria-invalid");
  setText(`${id}-error`, "");
}

function clearErrors(): void {
  (Object.keys(fields) as FieldPath[]).forEach(clearFieldError);
}

function focusFirstInvalid(): void {
  document.querySelector<HTMLInputElement>("[aria-invalid=true]")?.focus();
}

function showToast(message: string, kind: "success" | "error" | "neutral"): void {
  const toast = document.querySelector<HTMLElement>("#toast");
  if (!toast) return;
  toast.textContent = message;
  toast.dataset.kind = kind;
  toast.classList.add("visible");
  if (kind !== "error") {
    window.setTimeout(() => toast.classList.remove("visible"), 4_000);
  }
}

function mountIcons(): void {
  createIcons({
    icons: {
      Activity,
      Archive,
      Check,
      Database,
      Gauge,
      HeartPulse,
      RefreshCw,
      RotateCcw,
      Save,
      Settings2,
      ShieldCheck,
      SlidersHorizontal,
      X,
      XCircle,
    },
    attrs: { "stroke-width": 1.8 },
  });
}

function getValue(config: LocalRuntimeConfigV2, path: FieldPath): number {
  const [group, key] = path.split(".") as ["collection" | "retention", string];
  return Number((config[group] as unknown as Record<string, number>)[key]);
}

function setValue(config: LocalRuntimeConfigV2, path: FieldPath, value: number): void {
  const [group, key] = path.split(".") as ["collection" | "retention", string];
  (config[group] as unknown as Record<string, number>)[key] = value;
}

function changedPaths(left: LocalRuntimeConfigV2, right: LocalRuntimeConfigV2): FieldPath[] {
  return (Object.keys(fields) as FieldPath[]).filter(
    (path) => getValue(left, path) !== getValue(right, path),
  );
}

function position(value: number, min: number, max: number, logarithmic: boolean): number {
  const bounded = Math.min(max, Math.max(min, value));
  const ratio = logarithmic
    ? (Math.log(bounded) - Math.log(min)) / (Math.log(max) - Math.log(min))
    : (bounded - min) / (max - min);
  return 4 + ratio * 92;
}

function formatDuration(value: number): string {
  if (value >= 60_000 && value % 60_000 === 0) return `${formatNumber(value / 60_000)}분`;
  if (value >= 1_000) return `${formatNumber(value / 1_000)}초`;
  return `${formatNumber(value)}ms`;
}

function formatBytes(value: number): string {
  if (value >= 1_073_741_824) return `${formatDecimal(value / 1_073_741_824)} GiB`;
  if (value >= 1_048_576) return `${formatDecimal(value / 1_048_576)} MiB`;
  return `${formatDecimal(value / 1_024)} KiB`;
}

function formatNumber(value: number): string {
  return new Intl.NumberFormat("ko-KR", { maximumFractionDigits: 0 }).format(value);
}

function formatDecimal(value: number): string {
  return new Intl.NumberFormat("ko-KR", { maximumFractionDigits: 2 }).format(value);
}

function setText(id: string, value: string): void {
  const element = document.querySelector<HTMLElement>(`#${id}`);
  if (element) element.textContent = value;
}

function setDisabled(id: string, value: boolean): void {
  const button = document.querySelector<HTMLButtonElement>(`#${id}`);
  if (button) button.disabled = value;
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : "알 수 없는 오류가 발생했습니다.";
}
