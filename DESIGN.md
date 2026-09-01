# Design

## Source of truth

Status: Active
Date: 2026-09-01
Product surfaces: standalone static report, standalone loopback settings console, team hosted operations console, team administration

Evidence reviewed:

- `README.md`: current report behavior, trace model, privacy and cost semantics
- `docs/ARCHITECTURE.md`: Rust/TypeScript boundary and shared `ReportDtoVx`
- `docs/TEAM_ARCHITECTURE.md`: tenancy, role, ingest, retention, quota and audit contracts
- `docs/ADAPTER_COMPATIBILITY.md`: official adapter surfaces, precedence and support evidence
- v1.1 strict TypeScript static report renderer, bounded timeline/pagination, saved views and desktop/mobile baseline

This file is the product and UI source of truth. Backend and security decisions remain owned by the two
architecture documents. Team screens described here are planned, not implemented.

## Brand

Personality: quiet, precise, operational and trustworthy. The product should feel like a daily engineering
console, not a marketing dashboard.

Trust signals:

- every number shows scope, time range and freshness
- estimated cost is visibly distinguished from billed cost
- incomplete and unknown data remain explicit
- privacy policy, redaction and authorization evidence are reachable from affected data
- destructive operations show scope, consequence and audit behavior before confirmation

Avoid:

- oversized hero content, decorative cards and one-note color palettes
- gradients, ornamental blobs and illustration-led empty states
- hiding missing data behind zero values
- presenting client-side role visibility as authorization
- showing local paths, raw prompts, outputs or source payloads

## Product goals

Goals:

- move from team-level KPI to the responsible trace, span and safe diagnostic in a few interactions
- compare token, latency, failures and estimated cost across workspace, project, repository, member and model
- make ingestion freshness, partial data and privacy state visible beside the affected report
- let authorized users manage membership, policies, retention, quota, sources, exports and audit
- share analysis components and `ReportDtoVx` semantics across standalone and team
- configure standalone collection, storage, retention and cadence through a visual local-only surface

Non-goals:

- landing page, sales site or in-product feature tour
- raw prompt/output viewer
- general-purpose log search or arbitrary query language
- model request gateway administration
- remote control of standalone local files; the same-user loopback settings console is explicitly local-only

Success signals:

- users can identify a cost/error spike and reach contributing traces without changing tools
- no UI path can request data outside server-resolved scope
- unknown, incomplete, stale, offline and quota states are distinguishable in usability tests
- keyboard-only workflows cover filter, trace inspection, member management and export
- standalone HTML still opens without server or network

## Personas and jobs

| Persona | Jobs |
| --- | --- |
| Engineering lead | find reliability and latency regressions, compare projects and agents, inspect traces |
| Cost owner | explain estimated spend by project/identity label/agent/model, inspect assumptions and quota; raw email requires separate PII access |
| Workspace admin | enroll sources, manage members/roles, privacy, retention and quota |
| Contributor | verify personal/project collection, inspect allowed traces and sync health |
| Auditor | review access, policy, export, deletion and operator events without changing data |
| Platform operator | operate service health through a separately authorized surface, not tenant UI |

Primary contexts are desktop repeated use and incident investigation. Tablet supports review and light admin.
Mobile supports status, search and trace inspection, not dense bulk administration.

## Information architecture

The first authenticated screen is the last selected workspace overview. There is no marketing landing page.

Standalone file-open uses hash navigation or internal view state so it works from one HTML file:

- `#/overview`, `#/activity`, `#/traces`, `#/traces/:traceId`
- `#/projects`, `#/costs`, `#/privacy`, `#/exports`

Standalone settings use an ephemeral loopback route opened by `agent-observability ui`:

- `/` renders overview, collection, storage and retention controls in one responsive workspace
- `/api/config` reads and atomically replaces the Rust-validated local configuration
- the browser receives a one-time session capability through the URL fragment, removes it from the visible URL,
  and sends it only in a private request header

The settings process binds an operating-system-selected port on `127.0.0.1`, rejects non-loopback host and
origin values, sends no CORS permission, makes no external request and expires after inactivity. Closing it
does not affect the static report or collection runtime. Rust remains authoritative for defaults, validation,
atomic persistence and file permissions.

Hosted team uses server routes:

- `/w/:workspaceId/overview`, `/activity`, `/traces`, `/traces/:traceId`
- `/w/:workspaceId/projects`, `/costs`, `/privacy`, `/exports`
- `/w/:workspaceId/sources`
- `/w/:workspaceId/members`
- `/w/:workspaceId/identities`
- `/w/:workspaceId/data`
- `/w/:workspaceId/audit`
- `/w/:workspaceId/settings`

Navigation groups:

- Analyze: Overview, Activity, Traces, Projects, Costs
- Govern: Privacy, Sources, Identities, Members, Data, Audit
- Output: Exports

Workspace switcher is always visible on desktop and preserves no filter that is invalid in the new scope.
Project and repository are filter dimensions, not navigation trees.

### Delivery scope and route contract

| Gate | Route/surface | Capability | Input contract/API | Required states |
| --- | --- | --- | --- | --- |
| G2 alpha | overview, traces, costs | `report:read`, `cost:read` | paginated `ReportDtoV1`; report/query API | loading, empty, partial, stale, unauthorized |
| G2 alpha | sources | `source:admin` | source/enrollment DTO; source control API | none, pending, active, revoked, degraded |
| G2 alpha | members | `workspace:admin` | member/role DTO; membership API | invite pending/expired, active, revoked, conflict |
| G2 alpha | identities | own/admin scoped; `identity:read_pii` for others' raw email | email identity/binding DTO; identity APIs | pending verification, verified, current PII, current pseudonymous, deleted pseudonymous, ambiguous, revoked |
| G2 alpha | privacy | read/admin split | effective-policy DTO; policy API | inherited, overridden, invalid, pending propagation |
| G3 beta | data | `data:admin` | retention/quota/deletion DTO; data control API | warning, throttled, hold blocked, purging, failed |
| G3 beta | audit | `audit:read` | cursor-paginated audit DTO; audit query API | empty, integrity warning, unavailable |
| G3 beta | exports | scoped `report:read`/`cost:read` | export job/manifest DTO; export API | queued, running, ready, expired, revoked, failed |
| G3 beta | trace revision history | `observation:correct`/`observation:retract` | append-only revision DTO/API | current, corrected, retracted, conflict, unauthorized |
| G4 | all above | gate-specific | current/previous compatible DTOs | offline/degraded, migration, incident banner |

List APIs use opaque cursor pagination, server allowlisted sort keys and bounded filters. Client never builds
raw query expressions. Mutations use operation idempotency keys and map bounded server reason codes to the
interaction states below. A screen is not gate-complete until every listed state has a fixture and role-denial
test under `docs/evidence/team/<gate>/ui/`.

### Onboarding

Standalone starts directly from a generated report. Team onboarding is an operational checklist:

1. Create workspace and set timezone/currency.
2. Confirm privacy baseline and retention.
3. Enroll a source instance with a one-time credential.
4. Verify one or more email identities and bind Codex, Claude and Cursor profiles.
5. Verify first accepted event, heartbeat and report freshness.
6. Invite members and assign roles.
7. Review quota and notification thresholds.

Each item shows `not started`, `blocked`, `ready` or `verified`; percent-complete gamification is not used.

## Design principles

1. Scope before data: workspace, filters, time range and freshness precede every metric.
2. Evidence over decoration: tables, timelines, distributions and links to traces carry the interface.
3. Unknown is a state: zero, unknown, incomplete, stale and unauthorized are never conflated.
4. Metric to trace: every actionable aggregate can open the bounded record set that explains it.
5. Privacy in context: show effective policy and redaction evidence where data absence occurs.
6. Server authority: UI never infers tenant permission or filters already received cross-scope data.
7. Local independence: team sync state never prevents standalone analysis or export.

## Visual language

- Color: neutral white/gray surfaces and charcoal text; teal for healthy/fresh, amber for degraded/incomplete,
  red for errors/destructive actions, blue for focus/selection. Status always includes text or icon.
- Typography: system sans for interface and tables, monospace only for opaque IDs and numeric details.
- Spacing: 4px base; compact 8/12px controls, 16/24px page rhythm.
- Shape: 4-6px radius for controls and repeated items; no nested cards or floating page sections.
- Elevation: borders and sticky bands first; shadow only for modal, menu and drawer separation.
- Motion: 120-180ms state transitions; no decorative motion. Reduced-motion removes nonessential movement.
- Icons: use the selected TypeScript icon library consistently; familiar actions use icons with tooltips.
- Charts: restrained categorical colors, direct labels where possible, accessible patterns for comparison.

## Components

Shared analysis components consume only `ReportDtoVx` or dedicated sanitized management DTOs:

- `AppShell`, `WorkspaceSwitcher`, `ScopeBar`, `FilterBar`
- `KpiStrip`, `FreshnessIndicator`, `DataQualitySummary`
- `ActivityTable`, `TraceList`, `TraceTree`, `TraceTimeline`, `SpanDetails`
- `CostBreakdown`, `IdentityBreakdown`, `AgentBreakdown`, `CostAssumption`, `PrivacyStatus`
- `EmptyState`, `LoadingState`, `PartialState`, `OfflineState`, `QuotaState`
- `ExportDialog`, `FieldManifest`

Standalone settings components consume only a versioned sanitized configuration DTO:

- `LocalSettingsShell`, `SettingsNavigation`, `SessionStatus`
- `CollectionSwitch`, `CadenceTimeline`, `BatchCapacityPlot`
- `StorageBudgetGauge`, `RetentionWindow`, `ArchiveLimitEditor`
- `StickySaveBar`, `ValidationSummary`, `ResetDefaultsDialog`

Visualizations explain policy relationships; they do not invent runtime telemetry. Gauges and timelines label
configured limits as policy, not current usage. Every numeric control keeps a direct text input and unit label,
so the visualization never becomes the only editing mechanism.

Team management components:

- `SourceTable`, `EnrollmentDialog`, `CredentialRotationDialog`
- `AdapterStatusTable`, `CollectionPolicyEditor`, `LocalResourceStatus`
- `EmailIdentityTable`, `IdentityBindingDialog`, `ProfileResolutionPreview`
- `AttributionCorrectionDialog`, `RetractionDialog`, `ObservationRevisionHistory`
- `MemberTable`, `InviteDialog`, `RoleMatrix`, `ScopePicker`
- `PolicyEditor`, `EffectivePolicyPreview`, `RetentionEditor`, `QuotaMeter`
- `AuditTable`, `AuditDetail`, `DeletionDialog`, `DeletionReceipt`

Stable dimensions are required for KPI rows, filters, trace trees, icon buttons and status cells so loading,
unknown values and long identifiers do not shift the layout. Tables define column priority and minimum widths.

TypeScript components do not calculate pricing, privacy, authoritative report aggregates or tenant scope. Rust
application/projector code owns those decisions. The static UI may perform presentation-only filter reduction
over sanitized spans and Rust-priced scalar/status fields; its completeness rule is locked to
`contracts/report-view-reduction-v1.fixture.json` in Rust and TypeScript tests. Profile adapters own
file-embedded DTO versus authenticated paginated query.

The standalone v1.1 report renders at most 100 traces, 200 span rows, 120 timeline rows, and 500 values per
filter dimension while retaining full DTO counts and aggregates. Saved views retain at most 20 sanitized
repo/session/agent/model combinations whose values pass the key-specific safe scalar grammar in browser-local
storage; free-text searches, trace selections, email-like values, and path-like values are never persisted.

## Accessibility

Target: WCAG 2.2 AA.

- semantic landmarks, headings, tables, captions and row/column headers
- full keyboard operation for navigation, filter, tree, pagination, dialogs and drawers
- visible focus and predictable focus return; modal and drawer focus trap
- text/icon in addition to color for status and chart series
- body contrast at least 4.5:1 and large text at least 3:1
- `aria-live` for sync, partial, export and deletion progress without excessive announcements
- accessible names and tooltips for icon-only actions
- reduced-motion support and no information encoded only by animation
- long opaque IDs can wrap or truncate with an accessible full-value action

Accessibility checks require automated rules plus manual keyboard, screen-reader and zoom review.

## Responsive behavior

- Desktop >= 1200px: 224-240px navigation, sticky scope/filter bar, trace list + timeline + detail panes.
- Tablet 768-1199px: collapsible navigation, two-column KPI layout, detail drawer replaces third pane.
- Mobile < 768px: navigation drawer, filter sheet, one-column KPI list, priority table columns, full-screen
  trace/detail views.
- Touch targets are at least 44px on touch layouts; dense desktop rows may remain smaller with keyboard focus.
- Bulk role/policy operations are desktop-first and remain readable, not compressed into unsafe mobile forms.

## Interaction states

- Local settings clean/dirty: changed fields and the persistent save bar are visible without moving layout.
- Local settings saving/saved: disable duplicate submission, announce completion and render the canonical
  configuration returned by Rust.
- Local settings invalid: preserve edits, focus the first invalid control and show bounded field errors.
- Local settings conflict: reload the latest file, reapply only locally changed fields and require another explicit
  save; never overwrite an external edit silently.
- Local settings expired: disable mutation controls and provide the exact CLI command to start a fresh session.
- Loading: preserve component dimensions and display which scope is loading.
- Empty: distinguish no source enrolled, no data in range and filter with no match.
- Partial: list missing source/time range and affected metrics; never silently total incomplete data.
- Stale: show last aggregate and ingest receipt times.
- Offline: show local queue count/age and last successful sync; local report remains available.
- Local pressure: show bounded CPU/RSS/disk buckets, delayed reconciliation, paused team sync and last healthy
  time. Controls expose only contract-bounded collection/flush/heartbeat values and a reset-to-default command.
- Identity ambiguity: show source-only attribution and require an explicit profile choice; never guess an email.
- Identity PII denied: keep identity/purpose grouping with a pseudonymous label and do not render or export raw email.
- Quota: distinguish warning, throttled and hard-rejected; show affected team path only.
- Unauthorized: do not reveal resource existence; show current scope and access request route.
- Unknown/incomplete cost: show no invented total, rate table version and missing dimensions.
- Error: show bounded reason code and correlation ID, not raw backend/source error.
- Destructive action: require typed or explicit confirmation for tenant/workspace deletion, credential revoke,
  email identity deletion, retention reduction and export revoke; show audit consequence.
- Long-running export/deletion: asynchronous job state with refresh/resume and completion receipt.
- Observation correction: require reason, current revision and authorized destination binding. Retraction excludes
  the observation from current reports but preserves revision/audit history. Reinstate is an explicit new
  visibility revision, not a generic undo/redo action.

## Content voice

Use concise operational language. Prefer `수집 지연`, `부분 데이터`, `권한 없음`, `재시도 예정` and
`예상 비용` over vague status labels. Explain impact and next action in one or two lines.

Never call estimated model cost a bill. Never imply raw content exists when policy excluded it. Avoid
implementation tutorials and keyboard-shortcut copy inside the main product surface.

## Implementation constraints

- Web UI is TypeScript in `strict` mode. Framework selection is deferred until implementation evidence.
- Rust owns domain, application, API, query, aggregate, privacy, cost and DTO projection.
- Schema is generated or runtime-validated from a versioned source; Rust and TypeScript do not hand-copy it.
- Standalone output remains one self-contained HTML file with no runtime network request.
- Standalone scope is fixed local state. A team/workspace selector is forbidden until hosted query returns a
  server-resolved authorized scope; agent and model remain report filter dimensions.
- Team uses authenticated pagination and server-resolved scope; `TeamIngestEnvelopeV1` is never a UI DTO.
- No analytics or font network dependency is required for the static artifact.
- Tables and traces require virtualization or bounded pagination only after measured dataset thresholds.
- Browser support target is the latest two stable major versions of major evergreen desktop browsers; exact
  support becomes a release contract at the TypeScript UI milestone.
- Verification includes typecheck, unit/contract tests, file-open smoke, browser screenshots at desktop/mobile,
  keyboard/a11y checks, long-text overflow and empty/partial/offline/quota fixtures.
- v0.12 provisions the Chromium revision paired with lockfile-pinned `playwright-core` in `npm test` pretest,
  then runs file-open browser checks without a local server. It asserts desktop/mobile overflow, 44px mobile
  controls, headings/landmarks, first-tab focus, filter/trace state, console errors and external network requests.

## Open questions

- [ ] Security/legal owner: approve retention, legal hold, region and deletion wording; impacts Data and Audit.
- [ ] Business/security owner: approve the hosted-only first deployment, first data region and operator/key
  custody model; blocks G0.
- [ ] Product owner: prioritize enterprise federation and automated lifecycle provisioning after first GA.
- [ ] Engineering owner: validate trace volume and responsive performance thresholds before framework choice.
- [ ] Accessibility owner: confirm supported browser/screen-reader matrix before UI release candidate.
