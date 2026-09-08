import {
  Activity,
  Archive,
  Cable,
  Check,
  Database,
  ExternalLink,
  Gauge,
  HeartPulse,
  MonitorUp,
  Power,
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
import { validateLocalRuntimeConfig } from "./config-validation.js";
import { validateCodexIntegrationStatus } from "./integration-status-validation.js";
import validateIntegrationError from "./generated/validate-codex-integration-error-v1.js";
import type {
  CodexIntegrationStatusV1,
  CollectorDegradationReasonV1,
} from "./generated/codex-integration-status-v1.js";
import type { LocalRuntimeConfigV5 } from "./generated/local-runtime-config-v5.js";

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
  | "retention.max_archive_bytes"
  | "lifecycle.hot_days"
  | "lifecycle.warm_days"
  | "lifecycle.delete_after_days"
  | "lifecycle.private_raw_days"
  | "lifecycle.maintenance_interval_seconds"
  | "lifecycle.max_traces_per_pass";

type Envelope = {
  config: LocalRuntimeConfigV5;
  defaults: LocalRuntimeConfigV5;
  revision: string;
  collection_mode: "automatic_codex" | "manual_import";
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
    label: "수동 정리 기준일",
    description: "자동 삭제 기준과 별개로, 이 기간보다 오래된 trace를 수동 정리 대상으로 선택",
    min: 1,
    max: 3_650,
    step: 1,
    unit: "days",
    format: (value) => `${formatNumber(value)}일`,
  },
  "retention.max_archive_records": {
    path: "retention.max_archive_records",
    label: "정리 레코드 상한",
    description: "한 번의 수동 정리 작업에서 archive로 옮길 수 있는 전체 레코드 상한",
    min: 1,
    max: 100_000,
    step: 1,
    unit: "records",
    format: (value) => `${formatNumber(value)}개`,
  },
  "retention.max_archive_bytes": {
    path: "retention.max_archive_bytes",
    label: "정리 크기 상한",
    description: "한 번의 수동 정리 작업에서 생성하는 archive의 전체 크기 상한",
    min: 65_536,
    max: 268_435_456,
    step: 1,
    unit: "bytes",
    format: formatBytes,
  },
  "lifecycle.hot_days": {
    path: "lifecycle.hot_days",
    label: "Hot(최근) 기준일",
    description: "원본 관측 이력은 제거하고 리포트용 기록은 유지하는 Warm(이력 축소) 단계로 이동",
    min: 1,
    max: 3_650,
    step: 1,
    unit: "days",
    format: (value) => `${formatNumber(value)}일`,
  },
  "lifecycle.warm_days": {
    path: "lifecycle.warm_days",
    label: "Warm(이력 축소) 기준일",
    description: "일반 리포트에서 제외하고 압축하지 않은 trace별 JSON 묶음으로 보관하는 Cold(장기 보관) 단계로 이동",
    min: 1,
    max: 3_650,
    step: 1,
    unit: "days",
    format: (value) => `${formatNumber(value)}일`,
  },
  "lifecycle.delete_after_days": {
    path: "lifecycle.delete_after_days",
    label: "완전 삭제 기준일",
    description: "최신 관측 이후 관리 대상 trace가 영구 삭제되는 시점",
    min: 1,
    max: 3_650,
    step: 1,
    unit: "days",
    format: (value) => `${formatNumber(value)}일`,
  },
  "lifecycle.private_raw_days": {
    path: "lifecycle.private_raw_days",
    label: "원문 상세 보관",
    description: "데이터 보관 정책을 켰을 때 private 요청·응답 원문 보관 기간",
    min: 1,
    max: 3_650,
    step: 1,
    unit: "days",
    format: (value) => `${formatNumber(value)}일`,
  },
  "lifecycle.maintenance_interval_seconds": {
    path: "lifecycle.maintenance_interval_seconds",
    label: "유지관리 주기",
    description: "로컬 수집기가 다음 정리 작업을 확인하는 간격",
    min: 60,
    max: 86_400,
    step: 1,
    unit: "seconds",
    format: formatDurationSeconds,
  },
  "lifecycle.max_traces_per_pass": {
    path: "lifecycle.max_traces_per_pass",
    label: "정리 작업당 trace",
    description: "한 번의 정리 작업에서 처리할 최대 trace 수",
    min: 1,
    max: 128,
    step: 1,
    unit: "traces",
    format: (value) => `${formatNumber(value)}개`,
  },
};

const rootElement = document.querySelector("#app");
if (!(rootElement instanceof HTMLDivElement)) throw new Error("settings root is missing");
const app = rootElement;

const SESSION_TOKEN_KEY = "agent-observability.settings.session.v1";
const INITIAL_INTEGRATION_RETRY_MS = 1_500;
const fragmentToken = new URLSearchParams(location.hash.slice(1)).get("session") ?? "";
let token = fragmentToken || readSessionToken();
if (fragmentToken) writeSessionToken(fragmentToken);
history.replaceState(null, "", `${location.pathname}${location.search}`);
let persisted: LocalRuntimeConfigV5 | null = null;
let draft: LocalRuntimeConfigV5 | null = null;
let defaults: LocalRuntimeConfigV5 | null = null;
let revision = "";
let integration: CodexIntegrationStatusV1 | null = null;
let integrationUnavailable = false;
let integrationRequestGeneration = 0;
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

window.addEventListener("focus", () => {
  void refreshIntegrationStatus();
});

void bootstrap();

async function bootstrap(): Promise<void> {
  renderLoading();
  if (!token) {
    renderExpired();
    return;
  }
  try {
    const envelope = await api<Envelope>("/api/config");
    applyEnvelope(envelope);
    let shouldRenderSettings = false;
    try {
      shouldRenderSettings = await loadInitialIntegrationStatus();
    } catch (error) {
      const apiError = error as Error & { code?: string };
      if (apiError.code === "invalid_session") throw error;
      integration = null;
      integrationUnavailable = true;
      shouldRenderSettings = true;
    }
    if (!token) return;
    if (shouldRenderSettings) renderSettings();
    heartbeatTimer ??= window.setInterval(() => void heartbeat(), 20_000);
  } catch (error) {
    const apiError = error as Error & { code?: string };
    if (apiError.code === "invalid_session" || apiError.code === "network_failure") {
      expireSession();
    } else {
      renderUnavailable(messageOf(error));
    }
  }
}

async function loadInitialIntegrationStatus(): Promise<boolean> {
  const generation = ++integrationRequestGeneration;
  try {
    const initial = await integrationApi("/api/integrations/codex");
    const next = initial.config === "connected" && initial.collector === "unavailable"
      ? await new Promise<void>((resolve) => window.setTimeout(resolve, INITIAL_INTEGRATION_RETRY_MS))
        .then(() => integrationApi("/api/integrations/codex"))
      : initial;
    if (generation !== integrationRequestGeneration || !token) return false;
    integration = next;
    integrationUnavailable = false;
    return true;
  } catch (error) {
    if (generation !== integrationRequestGeneration) return false;
    throw error;
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
    <p>터미널에서 <code>agentobs settings</code>를 실행해 새 세션을 여세요.</p>
  </main>`;
  mountIcons();
}

function renderSettings(focusTarget?: string): void {
  if (!draft) return;
  app.innerHTML = `<div class="app-shell">
    <header class="topbar">
      <div class="brand"><span class="brand-mark"><i data-lucide="settings-2"></i></span><span>Agent Observability</span></div>
      <div class="topbar-actions">
        <button class="button monitor-button" id="open-dashboard" type="button"><i data-lucide="monitor-up"></i>모니터링</button>
        <span class="session-badge"><i data-lucide="shield-check"></i>로컬 전용 · 세션 활성</span>
        <button class="icon-button" id="close-session" type="button" title="설정 세션 닫기" aria-label="설정 세션 닫기"><i data-lucide="x"></i></button>
      </div>
    </header>
    <div class="workspace">
      <nav class="section-nav" aria-label="설정 영역">
        <p class="nav-label">설정</p>
        <a href="#overview" class="active" aria-current="page"><i data-lucide="gauge"></i>개요</a>
        <a href="#collection"><i data-lucide="activity"></i>수집</a>
        <a href="#privacy"><i data-lucide="shield-check"></i>개인정보</a>
        <a href="#storage"><i data-lucide="database"></i>저장소</a>
        <a href="#lifecycle"><i data-lucide="heart-pulse"></i>데이터 보관</a>
        <a href="#retention"><i data-lucide="archive"></i>수동 정리</a>
        <div class="nav-note"><strong>Codex</strong><span>${configNavigationStatus()}</span><span>${collectorNavigationStatus()}</span></div>
      </nav>
      <main class="settings-main">
        <form id="settings-form" novalidate>
          ${overviewSection(draft)}
          ${collectionSection(draft)}
          ${privacySection(draft)}
          ${storageSection(draft)}
          ${lifecycleSection(draft)}
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

function overviewSection(config: LocalRuntimeConfigV5): string {
  const storage = fields["collection.local_storage_budget_bytes"].format(
    config.collection.local_storage_budget_bytes,
  );
  return `<section class="settings-section overview" id="overview" aria-labelledby="overview-title">
    <div class="section-heading"><div><p class="eyebrow">Standalone</p><h1 id="overview-title">로컬 수집 정책</h1><p>정적 리포트와 독립적으로 저장·보관 한도를 관리합니다.</p></div>
      <label class="collection-toggle"><span><strong>수집 허용</strong><small id="enabled-copy">${config.enabled ? "private handoff를 처리합니다" : "설정값을 유지한 채 처리를 중지합니다"}</small></span><input type="checkbox" id="enabled" ${config.enabled ? "checked" : ""}><span class="toggle-track" aria-hidden="true"><span></span></span></label>
    </div>
    ${integrationPanel()}
    <div class="policy-strip" aria-label="정책 요약">
      ${summaryItem("activity", "확인 주기", formatDuration(config.collection.file_reconcile_interval_ms))}
      ${summaryItem("sliders-horizontal", "배치 상한", `${formatNumber(config.collection.max_batch_records)}개`)}
      ${summaryItem("database", "저장 한도", storage)}
      ${summaryItem("archive", "보관 기간", `${formatNumber(config.retention.max_record_age_days)}일`)}
    </div>
    <div class="policy-notice"><i data-lucide="shield-check"></i><div><strong>이 화면은 로컬 정책만 변경합니다.</strong><span>외부 전송 없이 Rust가 검증한 뒤 private config에 원자적으로 저장합니다.</span></div></div>
  </section>`;
}

function integrationPanel(): string {
  const connected = integration?.config === "connected";
  const ready = integration?.collector === "ready";
  const degraded = integration?.collector === "degraded";
  const conflicted = integration?.config === "conflict";
  const degradedCopy = integrationDegradedCopy(
    integration?.collector_degradation_reasons ?? [],
  );
  const state = integrationUnavailable
    ? "상태 확인 불가"
    : conflicted
    ? "설정 충돌"
    : connected && degraded
      ? degradedCopy.state
      : connected && ready
        ? "수집 중"
        : connected
          ? "수집기 응답 없음"
          : "연결 안 됨";
  const detail = integrationUnavailable
    ? "Codex 상태를 확인할 때까지 연결 변경을 잠갔습니다. 다시 확인을 눌러 상태를 조회해 주세요."
    : conflicted
    ? "Codex 설정이 연결 후 변경되어 자동 복원을 중단했습니다."
    : connected && degraded
      ? degradedCopy.detail
      : connected && ready
        ? "Codex 이벤트를 private local runtime에 반영합니다."
        : connected
          ? "Codex 연결은 유지되지만 로컬 수집기에 연결할 수 없습니다."
          : "Codex 자동 수집을 연결하면 다음 작업부터 기록합니다.";
  const action = integrationUnavailable
    ? `<button class="button secondary" id="refresh-integration" type="button"><i data-lucide="refresh-cw"></i>다시 확인</button>`
    : connected
    ? `<button class="button secondary" id="toggle-integration" type="button"><i data-lucide="power"></i>연결 해제</button>`
    : `<button class="button primary" id="toggle-integration" type="button"><i data-lucide="cable"></i>Codex 연결</button>`;
  const panelState = integrationUnavailable ? "unavailable" : conflicted ? "conflict" : degraded ? "degraded" : ready ? "ready" : "idle";
  const collectorLabel = integrationUnavailable ? "확인 불가" : degraded ? "상태 저하" : ready ? "정상" : "중지";
  return `<div class="integration-panel" data-state="${panelState}" data-config-state="${integration?.config ?? "disconnected"}" data-collector-state="${integration?.collector ?? "unavailable"}">
    <div class="integration-identity"><span class="integration-icon"><i data-lucide="activity"></i></span><div><span>Codex</span><strong>${state}</strong><small>${detail}</small></div></div>
    <div class="integration-meta"><span><b>수집기</b>${collectorLabel}</span><span><b>저장</b>로컬 전용</span>${integration?.endpoint ? `<span class="endpoint"><b>Endpoint</b>${escapeHtml(integration.endpoint)}</span>` : ""}</div>
    <div class="integration-actions">${action}<button class="button monitor-button" id="overview-dashboard" type="button"><i data-lucide="external-link"></i>리포트 열기</button></div>
  </div>`;
}

function integrationDegradedCopy(
  reasons: CollectorDegradationReasonV1[],
): { state: string; detail: string } {
  if (reasons.length === 0) {
    return {
      state: "수집기 상태 저하",
      detail: "리포트 반영 또는 데이터 보관 정리가 지연될 수 있습니다.",
    };
  }
  const labels: Record<CollectorDegradationReasonV1, string> = {
    lifecycle_failure: "데이터 보관 정리 미완료",
    storage_pressure: "정리용 임시 저장 공간 부족",
    expired_trace: "만료된 세션 데이터 제외",
  };
  const details: Record<CollectorDegradationReasonV1, string> = {
    lifecycle_failure: "일부 데이터 또는 오류로 데이터 보관 정리 작업을 완료하지 못했습니다.",
    storage_pressure: "정리 작업에 필요한 임시 저장 공간이 부족해 데이터 보관 정리가 지연됩니다.",
    expired_trace: "완전히 만료된 세션의 후속 데이터가 제외되었습니다. 해당 작업을 계속 기록하려면 에이전트에서 새 세션을 시작해야 합니다.",
  };
  const reasonOrder: CollectorDegradationReasonV1[] = [
    "lifecycle_failure",
    "storage_pressure",
    "expired_trace",
  ];
  const orderedReasons = reasonOrder.filter((reason) => reasons.includes(reason));
  return {
    state: orderedReasons.map((reason) => labels[reason]).join(" · "),
    detail: orderedReasons.map((reason) => details[reason]).join(" "),
  };
}

function configNavigationStatus(): string {
  if (integrationUnavailable) return "자동 수집 상태 확인 불가";
  if (integration?.config === "connected") return "자동 수집 연결됨";
  if (integration?.config === "conflict") return "Codex 설정 충돌";
  return "자동 수집 연결 안 됨";
}

function collectorNavigationStatus(): string {
  if (integrationUnavailable) return "collector 상태 확인 불가";
  if (integration?.collector === "ready") return "collector 실행 중";
  if (integration?.collector === "degraded") return "collector 실행 중 · 상태 저하";
  return "collector 중지됨";
}

function collectionSection(config: LocalRuntimeConfigV5): string {
  return `<section class="settings-section" id="collection" aria-labelledby="collection-title">
    ${sectionTitle("collection", "activity", "수집", "파일 확인과 durable 기록 반영 간격")}
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

function storageSection(config: LocalRuntimeConfigV5): string {
  return `<section class="settings-section" id="storage" aria-labelledby="storage-title">
    ${sectionTitle("storage", "database", "저장소", "로컬 데이터가 넘지 못하는 디스크 예산")}
    <div class="section-grid">
      <div class="field-grid single">${fieldControl(fields["collection.local_storage_budget_bytes"], config)}</div>
      ${singleRuler("storage-visual", "설정 저장 한도", fields["collection.local_storage_budget_bytes"], "256 MiB", "20 GiB", true, "현재 사용량이 아닌 허용 한도")}
    </div>
  </section>`;
}

function privacySection(config: LocalRuntimeConfigV5): string {
  const enabled = config.capture_private_codex_turn_details ?? false;
  return `<section class="settings-section" id="privacy" aria-labelledby="privacy-title">
    <div class="section-title"><span class="section-icon"><i data-lucide="shield-check"></i></span><div><h2 id="privacy-title">개인정보</h2><p>Codex 작업 경로와 대화 내용을 별도 로컬 상세 저장소에 보관할지 선택합니다.</p></div></div>
    <label class="collection-toggle privacy-toggle" data-boolean-field="capture_private_codex_turn_details">
      <span><strong>요청·응답 상세 저장</strong><small id="private-details-copy">${enabled ? "새 Codex turn의 경로와 요청·응답을 로컬에 저장합니다" : "꺼짐 · 일반 지표와 해시 식별자만 저장합니다"}</small></span>
      <input type="checkbox" id="capture-private-codex-turn-details" ${enabled ? "checked" : ""}>
      <span class="toggle-track" aria-hidden="true"><span></span></span>
    </label>
    <div class="privacy-warning"><i data-lucide="shield-check"></i><div><strong>명시적으로 켠 이후의 새 turn부터 적용됩니다.</strong><span>원문은 team 전송·일반 리포트·export에 포함되지 않으며, 이 Mac의 private localhost 상세 화면에서만 요청할 때 읽습니다. 민감정보가 포함될 수 있습니다.</span></div></div>
  </section>`;
}

function lifecycleSection(config: LocalRuntimeConfigV5): string {
  const enabled = config.lifecycle.enabled;
  return `<section class="settings-section" id="lifecycle" aria-labelledby="lifecycle-title">
    <div class="section-title"><span class="section-icon"><i data-lucide="heart-pulse"></i></span><div><h2 id="lifecycle-title">데이터 보관 정책</h2><p>최신 trace 관측 시점부터 누적된 경과 기간으로 Hot(최근) → Warm(이력 축소) → Cold(장기 보관) → Delete(삭제)를 적용합니다.</p></div></div>
    <label class="collection-toggle lifecycle-toggle" data-boolean-field="lifecycle.enabled">
      <span><strong>자동 정리</strong><small id="lifecycle-enabled-copy">${enabled ? "켜짐 · 다음 정리 작업부터 보관 기준을 지난 기존 데이터에도 적용됩니다" : "꺼짐 · 기존 수동 보관 설정과 원문 보관 동작을 유지합니다"}</small></span>
      <input type="checkbox" id="lifecycle-enabled" ${enabled ? "checked" : ""}>
      <span class="toggle-track" aria-hidden="true"><span></span></span>
    </label>
    <div class="section-grid lifecycle-grid">
      <div class="field-grid">${fieldControl(fields["lifecycle.hot_days"], config)}${fieldControl(fields["lifecycle.warm_days"], config)}${fieldControl(fields["lifecycle.delete_after_days"], config)}${fieldControl(fields["lifecycle.private_raw_days"], config)}${fieldControl(fields["lifecycle.maintenance_interval_seconds"], config)}${fieldControl(fields["lifecycle.max_traces_per_pass"], config)}</div>
      ${lifecycleTimeline(config)}
    </div>
    <div class="retention-note lifecycle-warning" role="note"><i data-lucide="archive"></i><span><strong>삭제는 되돌릴 수 없습니다.</strong> 자동 정리를 켜거나 기준일을 줄이면 보관 기준을 지난 기존 데이터가 다음 정리 작업에서 이동하거나 영구 삭제될 수 있습니다. 완전히 삭제된 세션의 새 활동을 수집하려면 에이전트에서 새 세션을 시작해야 합니다. 설정 저장 완료는 정리 실행이나 디스크 공간 회수를 의미하지 않습니다.</span></div>
  </section>`;
}

function lifecycleTimeline(config: LocalRuntimeConfigV5): string {
  return `<figure class="policy-visual timeline lifecycle-timeline" data-min="1" data-max="3650" data-log="true">
    <figcaption><span>누적 경과 기간</span><strong data-lifecycle-value>Hot(최근) → Warm(이력 축소) → Cold(장기 보관) → Delete(삭제)</strong></figcaption>
    <div class="timeline-track" aria-hidden="true">
      <span class="timeline-marker first" data-marker data-path="lifecycle.hot_days"><b>Warm ${config.lifecycle.hot_days}일</b></span>
      <span class="timeline-marker second" data-marker data-path="lifecycle.warm_days"><b>Cold ${config.lifecycle.warm_days}일</b></span>
      <span class="timeline-marker third" data-marker data-path="lifecycle.delete_after_days"><b>Delete ${config.lifecycle.delete_after_days}일</b></span>
    </div>
    <div class="ruler-labels"><span>최신 관측</span><span>10년</span></div>
    <p>각 값은 단계별 추가 기간이 아니라 최신 trace 관측 이후의 누적 경과 기간입니다.</p>
  </figure>`;
}

function retentionSection(config: LocalRuntimeConfigV5): string {
  return `<section class="settings-section" id="retention" aria-labelledby="retention-title">
    ${sectionTitle("retention", "archive", "수동 정리", "자동 삭제와 별개인 수동 대상 기준 및 작업별 archive 상한")}
    <div class="section-grid">
      <div class="field-grid">${fieldControl(fields["retention.max_record_age_days"], config)}${fieldControl(fields["retention.max_archive_records"], config)}${fieldControl(fields["retention.max_archive_bytes"], config)}</div>
      <div class="visual-stack">
        ${singleRuler("retention-visual", "수동 정리 기준일", fields["retention.max_record_age_days"], "1일", "10년", true, "기준일보다 오래된 trace는 수동 정리 대상")}
        ${singleRuler("archive-records-visual", "작업별 Archive 레코드 상한", fields["retention.max_archive_records"], "1", "100k", true)}
        ${singleRuler("archive-bytes-visual", "작업별 Archive 크기 상한", fields["retention.max_archive_bytes"], "64 KiB", "256 MiB", true)}
      </div>
    </div>
    <div class="retention-note"><i data-lucide="archive"></i><span>수동 정리 기준 ${config.retention.max_record_age_days}일은 자동 완전 삭제 기준 ${config.lifecycle.delete_after_days}일과 별개입니다. 레코드·크기 상한은 한 번의 수동 정리 작업에 함께 적용됩니다. 기준일을 줄여도 즉시 삭제하지 않으며 별도의 계획 생성·적용 절차를 따릅니다.</span></div>
  </section>`;
}

function sectionTitle(id: "collection" | "storage" | "retention", icon: string, title: string, description: string): string {
  return `<div class="section-title"><span class="section-icon"><i data-lucide="${icon}"></i></span><div><h2 id="${id}-title">${title}</h2><p>${description}</p></div></div>`;
}

function summaryItem(icon: string, label: string, value: string): string {
  return `<div class="summary-item"><i data-lucide="${icon}"></i><span>${label}</span><strong>${value}</strong></div>`;
}

function fieldControl(field: Field, config: LocalRuntimeConfigV5): string {
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
  document.querySelector("#capture-private-codex-turn-details")?.addEventListener("change", handlePrivateDetails);
  document.querySelector("#lifecycle-enabled")?.addEventListener("change", handleLifecycleEnabled);
  document.querySelector("#discard")?.addEventListener("click", discardChanges);
  document.querySelector("#reset")?.addEventListener("click", openResetDialog);
  document.querySelector("#cancel-reset")?.addEventListener("click", closeResetDialog);
  document.querySelector("#confirm-reset")?.addEventListener("click", resetDefaults);
  document.querySelector("#close-session")?.addEventListener("click", requestCloseSession);
  document.querySelector("#cancel-close")?.addEventListener("click", closeCloseDialog);
  document.querySelector("#confirm-close")?.addEventListener("click", () => void closeSession());
  document.querySelector("#toggle-integration")?.addEventListener("click", () => void toggleIntegration());
  document.querySelector("#refresh-integration")?.addEventListener("click", () => void refreshIntegration());
  document.querySelector("#open-dashboard")?.addEventListener("click", () => void openDashboard());
  document.querySelector("#overview-dashboard")?.addEventListener("click", () => void openDashboard());
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

async function toggleIntegration(): Promise<void> {
  if (busy || !integration || integrationUnavailable) return;
  const lifecycleToken = token;
  const generation = ++integrationRequestGeneration;
  busy = true;
  setBusy(true);
  try {
    const method = integration.config === "connected" ? "DELETE" : "POST";
    const nextIntegration = await integrationApi("/api/integrations/codex", { method });
    if (token !== lifecycleToken || generation !== integrationRequestGeneration) return;
    integration = nextIntegration;
    integrationUnavailable = false;
    busy = false;
    renderSettings("toggle-integration");
    showToast(
      integration.config === "connected" ? "Codex 자동 수집을 연결했습니다." : "Codex 자동 수집을 해제했습니다.",
      "success",
    );
  } catch (error) {
    if (token !== lifecycleToken || generation !== integrationRequestGeneration) return;
    integration = null;
    integrationUnavailable = true;
    // A failed response can follow a committed write. Keep mutations locked until GET settles.
    try {
      const next = await integrationApi("/api/integrations/codex");
      if (token !== lifecycleToken || generation !== integrationRequestGeneration) return;
      integration = next;
      integrationUnavailable = false;
    } catch (statusError) {
      if (token !== lifecycleToken || generation !== integrationRequestGeneration) return;
      if ((statusError as Error & { code?: string }).code === "invalid_session") {
        busy = false;
        expireSession();
        return;
      }
    }
    busy = false;
    renderSettings(integrationUnavailable ? "refresh-integration" : "toggle-integration");
    showToast(`${messageOf(error)} ${integrationUnavailable
      ? "상태를 확인할 수 없어 변경을 잠갔습니다. 다시 확인을 눌러 주세요."
      : "현재 상태를 다시 확인했습니다."}`, "error");
  }
}

async function refreshIntegration(): Promise<void> {
  if (busy) return;
  const generation = ++integrationRequestGeneration;
  busy = true;
  setBusy(true);
  try {
    const next = await integrationApi("/api/integrations/codex");
    if (generation !== integrationRequestGeneration || !token) return;
    integration = next;
    integrationUnavailable = false;
    busy = false;
    renderSettings("toggle-integration");
    showToast("Codex 자동 수집 상태를 확인했습니다.", "success");
  } catch (error) {
    if (generation !== integrationRequestGeneration || !token) return;
    busy = false;
    const apiError = error as Error & { code?: string };
    if (apiError.code === "invalid_session") {
      expireSession();
      return;
    }
    integration = null;
    integrationUnavailable = true;
    renderSettings("refresh-integration");
    showToast(messageOf(error), "error");
  }
}

async function refreshIntegrationStatus(): Promise<void> {
  if (busy || !persisted || !token) return;
  const generation = ++integrationRequestGeneration;
  const previous = integration;
  const wasUnavailable = integrationUnavailable;
  try {
    const next = await integrationApi("/api/integrations/codex");
    if (!token || generation !== integrationRequestGeneration) return;
    integration = next;
    integrationUnavailable = false;
    if (wasUnavailable || !sameIntegrationStatus(previous, next)) {
      renderSettings();
    }
  } catch (error) {
    if (generation !== integrationRequestGeneration) return;
    const apiError = error as Error & { code?: string };
    if (apiError.code === "invalid_session") {
      expireSession();
      return;
    }
    integration = null;
    integrationUnavailable = true;
    if (!wasUnavailable || previous !== null) renderSettings();
  }
}

function sameIntegrationStatus(
  left: CodexIntegrationStatusV1 | null,
  right: CodexIntegrationStatusV1,
): boolean {
  const rightReasons = right.collector_degradation_reasons as readonly CollectorDegradationReasonV1[];
  return left !== null
    && left.config === right.config
    && left.collector === right.collector
    && left.endpoint === right.endpoint
    && left.service === right.service
    && left.data_retained === right.data_retained
    && left.collector_degradation_reasons.length === right.collector_degradation_reasons.length
    && left.collector_degradation_reasons.every((reason) => rightReasons.includes(reason));
}

async function openDashboard(): Promise<void> {
  if (busy) return;
  try {
    await api<void>("/api/dashboard/open", { method: "POST" });
    showToast("모니터링 리포트를 열었습니다.", "success");
  } catch (error) {
    showToast(messageOf(error), "error");
  }
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

function handlePrivateDetails(event: Event): void {
  const input = event.target;
  if (!(input instanceof HTMLInputElement) || !draft) return;
  draft.capture_private_codex_turn_details = input.checked;
  setText(
    "private-details-copy",
    input.checked
      ? "새 Codex turn의 경로와 요청·응답을 로컬에 저장합니다"
      : "꺼짐 · 일반 지표와 해시 식별자만 저장합니다",
  );
  updateDirtyState();
}

function handleLifecycleEnabled(event: Event): void {
  const input = event.target;
  if (!(input instanceof HTMLInputElement) || !draft) return;
  draft.lifecycle.enabled = input.checked;
  setText(
    "lifecycle-enabled-copy",
    input.checked
      ? "켜짐 · 다음 정리 작업부터 보관 기준을 지난 기존 데이터에도 적용됩니다"
      : "꺼짐 · 기존 수동 보관 설정과 원문 보관 동작을 유지합니다",
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
    const label = marker.querySelector("b");
    if (label && path.startsWith("lifecycle.")) {
      const stage = path === "lifecycle.hot_days" ? "Warm" : path === "lifecycle.warm_days" ? "Cold" : "Delete";
      label.textContent = `${stage} ${getValue(draft!, path)}일`;
    }
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
  const booleanChanges = booleanChangeCount(draft, persisted);
  const dirty = booleanChanges > 0 || changed.length > 0;
  document.querySelector<HTMLElement>("#save-band")?.classList.toggle("dirty", dirty);
  setText("save-title", conflicted ? "외부 변경 감지" : dirty ? `${changed.length + booleanChanges}개 변경` : "저장됨");
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
  const validation = validateLocalRuntimeConfig(draft);
  if (!validation.valid) {
    for (const error of validation.errors) {
      const path = error.path as FieldPath;
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
  const privateDetailsChanged = (localDraft.capture_private_codex_turn_details ?? false)
    !== (localBase.capture_private_codex_turn_details ?? false);
  const lifecycleEnabledChanged = localDraft.lifecycle.enabled !== localBase.lifecycle.enabled;
  const latest = await api<Envelope>("/api/config");
  applyEnvelope(latest);
  if (!draft) return;
  for (const path of changed) setValue(draft, path, getValue(localDraft, path));
  if (enabledChanged) draft.enabled = localDraft.enabled;
  if (privateDetailsChanged) {
    draft.capture_private_codex_turn_details = localDraft.capture_private_codex_turn_details ?? false;
  }
  if (lifecycleEnabledChanged) draft.lifecycle.enabled = localDraft.lifecycle.enabled;
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
  if (!defaults || !draft) return;
  // P1 does not expose budget-mode controls: reset only the visible settings.
  draft = { ...structuredClone(defaults), storage_budget: structuredClone(draft.storage_budget) };
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
    await refreshIntegrationStatus();
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
      (booleanChangeCount(draft, persisted) > 0 || changedPaths(draft, persisted).length > 0),
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

async function integrationApi(
  path: string,
  init: RequestInit = {},
): Promise<CodexIntegrationStatusV1> {
  let value: unknown;
  try {
    value = await api<unknown>(path, { ...init, signal: AbortSignal.timeout(5000) });
  } catch (error) {
    const failure = error as Error & { code?: string };
    if (failure.code?.startsWith("integration_") &&
        !validateIntegrationError({ code: failure.code, message: failure.message })) {
      throw new Error("Codex 변경 결과 응답을 확인할 수 없습니다. 상태를 다시 확인해야 합니다.");
    }
    throw error;
  }
  if (!validateCodexIntegrationStatus(value)) {
    throw new Error("Codex 자동 수집 상태 응답이 올바르지 않습니다.");
  }
  return value;
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
      Cable,
      Check,
      Database,
      ExternalLink,
      Gauge,
      HeartPulse,
      MonitorUp,
      Power,
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

function getValue(config: LocalRuntimeConfigV5, path: FieldPath): number {
  const [group, key] = path.split(".") as ["collection" | "retention" | "lifecycle", string];
  return Number((config[group] as unknown as Record<string, number>)[key]);
}

function setValue(config: LocalRuntimeConfigV5, path: FieldPath, value: number): void {
  const [group, key] = path.split(".") as ["collection" | "retention" | "lifecycle", string];
  (config[group] as unknown as Record<string, number>)[key] = value;
}

function changedPaths(left: LocalRuntimeConfigV5, right: LocalRuntimeConfigV5): FieldPath[] {
  return (Object.keys(fields) as FieldPath[]).filter(
    (path) => getValue(left, path) !== getValue(right, path),
  );
}

function booleanChangeCount(left: LocalRuntimeConfigV5, right: LocalRuntimeConfigV5): number {
  return Number(left.enabled !== right.enabled)
    + Number(
      (left.capture_private_codex_turn_details ?? false)
        !== (right.capture_private_codex_turn_details ?? false),
    )
    + Number(left.lifecycle.enabled !== right.lifecycle.enabled);
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

function formatDurationSeconds(value: number): string {
  if (value >= 3_600 && value % 3_600 === 0) return `${formatNumber(value / 3_600)}시간`;
  if (value >= 60 && value % 60 === 0) return `${formatNumber(value / 60)}분`;
  return `${formatNumber(value)}초`;
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

function escapeHtml(value: string): string {
  const entities: Record<string, string> = {
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  };
  return value.replace(/[&<>"']/g, (character) => entities[character] ?? character);
}

function setDisabled(id: string, value: boolean): void {
  const button = document.querySelector<HTMLButtonElement>(`#${id}`);
  if (button) button.disabled = value;
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : "알 수 없는 오류가 발생했습니다.";
}
