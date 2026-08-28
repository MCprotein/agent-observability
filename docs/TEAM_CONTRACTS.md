# Team Contracts

Status: Proposed; normative only after G0 approval
Last updated: 2026-08-28

이 문서는 `docs/TEAM_ARCHITECTURE.md`의 G1-G4 실행 계약을 구체화한다. G0가 blocked인 동안
schema/fixture scaffold와 threat-model test design만 허용하며 production credential, hosted
deployment, real tenant data ingest와 customer commitment는 금지한다.

## 1. Decision states

| Decision | State | Meaning |
| --- | --- | --- |
| hosted-only first deployment | proposed | G0 approval 후 normative |
| one configured residency region per tenant | proposed | first region/jurisdiction approval required |
| HTTPS JSON batch ingest | proposed | G0 승인 시 V1 normative transport |
| operational quota, no customer billing | proposed | first GA scope |
| self-hosted/dedicated tenant | deferred | no G1-G4 implementation work |
| raw content central ingest | rejected | cannot be promoted by G0 |

G0 decision artifact is `docs/evidence/team/G0/decision-record.yaml` with `decision`, `chosen_value`,
`alternatives`, `owner_identity`, separate named `business_attestation`, `legal_attestation` and
`security_attestation`, each attestation's role/time/scope, `approved_at`, `expires_or_review_at` and
`supersedes`. Schema validation rejects role-only or missing attestations. Until it exists, this file is a
reviewed proposal rather than an approved external contract.

## 2. Version families and artifacts

- `TeamIngestEnvelopeVx` is the conceptual family name; product documents use concrete
  `TeamIngestEnvelopeV1` only.
- `ReportDtoVx` is the conceptual report family. First standalone/team shared UI contract is
  `ReportDtoV1`.
- `ManagementDtoVx` is the management family. First hosted control API contract is `ManagementDtoV1`.

Rust contract types in `crates/contracts/src/` are the single canonical source. JSON schemas, TypeScript types
and artifact hashes are generated outputs and must have no hand-edited semantic fields.

Planned G1 artifacts are Future TODO specifications. Verification cells name future test targets, not
current shell commands. G1 promotion must add a wrapper that fails when a named target matches zero tests.

| Artifact | Path | Verification |
| --- | --- | --- |
| ingest request | `crates/contracts/schemas/team-ingest-v1.schema.json` | Future G1 test target: `team_ingest_v1` |
| ingest response/error | `crates/contracts/schemas/team-ingest-response-v1.schema.json` | same gate |
| report | `crates/contracts/schemas/report-dto-v1.schema.json` | Future G1 test target: `report_dto_v1` |
| management | `crates/contracts/schemas/management-dto-v1.schema.json` | Future G1 test target: `management_dto_v1` |
| local state/outbox | `crates/contracts/schemas/local-state-v1.schema.json` | Future G1 test target: `local_state_v1` |
| collection policy | `crates/contracts/schemas/collection-policy-v1.schema.json` | Future G1 test target: `collection_policy_v1` |
| queue | `crates/contracts/schemas/team-queue-v1.schema.json` | Future G1 test target: `team_queue_v1` |
| credential/enrollment | `crates/contracts/schemas/source-enrollment-v1.schema.json` | Future G1 test target: `source_enrollment_v1` |
| identity/binding | `crates/contracts/schemas/identity-binding-v1.schema.json` | Future G1 test target: `identity_binding_v1` |
| adapter heartbeat | `crates/contracts/schemas/adapter-heartbeat-v1.schema.json` | Future G1 test target: `adapter_heartbeat_v1` |
| adapter capability matrix | `crates/contracts/capabilities/adapter-capability-v1.yaml` | Future G1 test target: `adapter_capability_v1` |
| local performance protocol | `crates/contracts/performance/local-performance-v1.yaml` | `cargo run -p xtask -- perf local --profile release --check` |
| observation revision | `crates/contracts/schemas/observation-revision-v1.schema.json` | Future G1 test target: `observation_revision_v1` |
| deletion/hold receipt | `crates/contracts/schemas/deletion-v1.schema.json` | Future G1 test target: `deletion_v1` |
| quota/reservation ledger | `crates/contracts/schemas/quota-v1.schema.json` | Future G1 test target: `quota_v1` |
| audit/export/recovery DTO | `crates/contracts/schemas/operations-v1.schema.json` | Future G1 test target: `operations_v1` |
| generated TypeScript | `web/src/generated/contracts/*.ts` | Future G1: `cargo run -p xtask -- contracts generate --check` |
| generated hash manifest | `crates/contracts/generated-manifest.json` | same command; repository diff must be empty |
| crypto policy | `crates/contracts/security/team-crypto-v1.yaml` | Future G1 test target: `team_crypto_v1` |

Schemas set `additionalProperties: false` at every object, reject duplicate JSON keys before schema
validation and define required/nullability/string/numeric bounds. Fixture directories contain `valid`,
`boundary`, `invalid`, `privacy`, `compat` and `canonical-hash` cases.

After G1 promotion, `cargo run -p xtask -- contracts generate` will emit JSON schema, TypeScript and SHA-256 hashes from Rust types.
`--check` regenerates in a temporary directory and fails on semantic output or hash drift. G1 evidence is stored
at `docs/evidence/team/G1/contracts/manifest.yaml` using the evidence manifest fields in section 9 plus generator
version, every artifact hash, test command and compatibility/privacy suite result.

Compatibility matrix:

| Client/UI | Service | Required behavior |
| --- | --- | --- |
| N | N | full support |
| N-1 | N | accept and emit deprecation metadata |
| N | N-1 during rolling deploy | UI/API feature gate; no request with unsupported required field |
| older than N-1 | N | bounded `unsupported_version`, no partial decode |

Ingest N-1 support lasts at least 90 days and longer than queue age. Report/management N-1 support lasts
through rolling deploy plus 90 days. Deprecation response includes family, version, last-supported date and
upgrade action without tenant payload.

## 2.1 Local durability and scheduling contract

The target Rust runtime commits `source_cursor`, deterministic observation key and `event_id`, canonical local
record, optional team outbox row and source generation in one embedded-store transaction. Cursor advance without
the record/outbox, or outbox admission without its stable event identity, is invalid state. JSONL, snapshot and
HTML are projections and never own replay position or delivery state.

`LocalStateV1` uses a local-only source key consisting of adapter family, source generation fingerprint and
bounded cursor. Raw path, email and source content are forbidden from this key and from diagnostic labels.
Crash fixtures terminate before and after every transaction write/commit point, reopen the store, replay the
same input and prove one canonical record, one stable `event_id` and exactly one delivery outcome for every
team-enabled record. Delivery state is `pending`, `acknowledged`, `permanent_reject` or `dropped`; `pending` has
exactly one outbox row, terminal states have none, and `dropped` requires bounded reason/time range. Standalone
records have explicit `not_applicable`, not a missing outcome inferred as success.

`CollectionPolicyV1` is strict and contains only:

| Field | Default | Bound |
| --- | --- | --- |
| `file_reconcile_interval_ms` | 5000 | 1000..60000 |
| `flush_interval_ms` | 5000 | 1000..60000 |
| `max_batch_records` | 100 | 1..500 |
| `max_batch_bytes` | 524288 | 16384..2097152 |
| `active_heartbeat_interval_ms` | 60000 | 30000..300000 |
| `idle_heartbeat_interval_ms` | 300000 | 120000..900000 |
| `local_storage_budget_bytes` | 1073741824 | 268435456..21474836480 |

Every schedule is jittered. Event-driven hooks/native telemetry have no poll interval. A hook performs local
bounded handoff only; synchronous network, source scanning, report generation and queue draining are contract
violations. Runtime pressure may increase intervals within bounds and pause team projection/flush, but cannot
silently relax privacy, mutate accepted observations or overwrite queued records. Performance evidence uses the
budgets and load-shedding order in `TEAM_ARCHITECTURE.md`.

Hook ingress reads at most 1 MiB and emits an allowlisted local message of at most 64 KiB. It never persists raw
overflow bytes. Local channel capacity and normalization worker count are implementation constants recorded in
the evidence manifest, not unbounded/user-controlled values. Full-channel and daemon-unavailable outcomes return
without waiting for network or drain and expose only bounded counters/reason codes. The v0.13 gate compares a
deterministic three-source fixture host plus the real bounded ingress and durable drain against a
collection-disabled fixture-host baseline. Daemon-only measurements and hard-coded metrics do not satisfy the
gate. Product-process compatibility remains a separate adapter capability fixture because external process
versions and background activity are not a reproducible performance baseline.

The singleton contract uses an OS-held exclusive lock plus boot nonce; a PID or lock file alone is insufficient.
Observational hooks select host asynchronous mode where supported. A synchronous-only host uses 10 ms enqueue
and 50 ms total handler deadlines and returns success on timeout/full/unavailable. Capability entries declare
event mode and fail-open behavior. A process fixture proves exclusive startup and lock release; deterministic
fixtures cover stale/corrupt metadata, unavailable/disconnected receivers, oversized messages, full channels,
crash repair and restart replay. The in-memory v0.13 channel has no partial-record format. Interrupted outbox
ACK is a Future TODO team fixture and is not a v0.13 standalone release condition.

Storage accounting uses allocated filesystem blocks and covers authoritative state plus WAL/sidecars,
projections/exports, diagnostics/crash/temp files and atomic old/new copies under one hard budget. Standalone
creates no team queue/ACK artifacts; its disabled team partition is lendable under the rule below.
For budget `B`, reserve `max(32 MiB, floor(B/8))` as non-borrowable atomic/WAL headroom; split remaining `R`
40/50/8/2 percent with minimums 80/96/16/4 MiB and assign rounding residual to headroom. Invalid minima fail
schema validation. A disabled team profile may lend its partition to state/projection but never the headroom.
Worst-case block reservation is checked before write. Startup removes orphan temp files by a bounded scan. Capacity behavior and
the `normal/pressured/protected/probe` load-shedding transitions are normative in `TEAM_ARCHITECTURE.md` and use a
deterministic-clock fixture.

`local-performance-v1.yaml` fixes workload size, 60-second warm-up, 15-minute idle/active runs, 10,000-event
burst, one-second sampler, CPU normalization, machine/OS/filesystem/power metadata, cold/warm cache, three-adapter
schedule and threshold calculation. The xtask command emits
`docs/evidence/local/performance/<run>/manifest.yaml` and exits non-zero on any budget breach. No v0.13/v1.0
release can pass with a missing manifest or a daemon-only sample.

## 3. V1 transport contract

Normative candidate transport is TLS-protected JSON over HTTP:

- `POST /api/team/v1/ingest/batches`
- request content type: `application/json`
- response content type: `application/json`
- transport compression may be negotiated, but limits apply after decoding
- authentication: source credential only; no human cookie/session accepted
- tenant and source identity: resolved only from credential
- optional email attribution: opaque `identity_binding_ref`, accepted only after server-side source,
  workspace, membership and project-scope validation; raw email is forbidden

Top-level request:

```json
{
  "records": []
}
```

`records` is required, non-null, 1..500 items. The complete envelope field list and bounds are owned by the
generated ingest schema; no extension field or vendor map exists.

`identity_binding_ref` is optional because some source and automation sessions have no defensible human
attribution. When present it is a server-issued opaque 1..96 character reference. It is part of canonical
request hashing and immutable accepted-record attribution. A human-attributable adapter credential has one
`source_principal_id`; the binding owner must equal it. Shared/unbound credentials are not eligible to submit
this field. A stale, revoked, cross-principal, cross-source or cross-project binding is rejected with bounded
`identity_binding_denied`; the collector never falls back to a different email identity. Raw email and local
account names are invalid fields at every nesting level.

For an authenticated current source, validation order is transport bound -> typed decode/canonical hash ->
dedupe lookup -> first-seen mutable authorization/policy/quota. An identical committed hash returns the original
stored receipt even if its identity binding or current policy was later revoked; a conflicting hash is rejected.
Only a first-seen event evaluates the current binding and consumes quota. Invalid/revoked source credentials are
denied before dedupe and receive no receipt oracle.

Successful batch response:

```json
{
  "request_id": "req_opaque",
  "server_time_unix_ms": 1783296013000,
  "recovery_epoch": "recovery_opaque",
  "results": [
    {
      "event_id": "evt_opaque",
      "status": "accepted",
      "reason_code": "none",
      "retryable": false,
      "retry_after_ms": null,
      "commit_seq": 123
    }
  ]
}
```

Response bounds:

- `request_id`, `recovery_epoch`: required opaque ASCII 1..96 chars
- `server_time_unix_ms`, `commit_seq`: integer 0..2^53-1
- `status`: `accepted`, `duplicate`, `rejected`
- `reason_code`: `none`, `schema_invalid`, `privacy_rejected`, `scope_denied`,
  `idempotency_conflict`, `record_too_large`, `quota_rate`, `source_quarantined`,
  `temporarily_unavailable`
- `retryable`: required boolean
- `retry_after_ms`: nullable integer 0..86,400,000; non-null only when retryable
- `commit_seq`: non-null only for accepted/duplicate; correlation only, never a replay filter

Top-level error uses `{ "request_id", "reason_code", "retryable", "retry_after_ms" }` and never echoes
input. 401 and 403 have the same body size class and do not disclose tenant/resource existence.

## 4. ReportDtoV1

`ReportDtoV1` is generated server-side for team and Rust-side for standalone. UI receives no raw event log.

```json
{
  "schema_version": "report.v1",
  "scope": {
    "profile": "team",
    "workspace_ref": "ws_opaque",
    "from_unix_ms": 1783296000000,
    "to_unix_ms": 1783382400000
  },
  "freshness": {
    "generated_at_unix_ms": 1783382410000,
    "latest_receipt_unix_ms": 1783382405000,
    "projection_lag_ms": 5000,
    "state": "fresh"
  },
  "summary": {
    "input_tokens": {
      "value": 1200,
      "completeness": {
        "total_records": 10,
        "available": 8,
        "omitted_by_policy": 0,
        "unavailable_in_profile": 0,
        "unknown_source": 2
      }
    }
  },
  "dimensions": {
    "email_identities": [
      {
        "email_identity_ref": "email_identity_opaque",
        "display_state": "current_pii",
        "display_email": "developer@example.test",
        "purpose": "work",
        "agent_kind": "codex",
        "input_tokens": 1200,
        "output_tokens": 480,
        "estimated_cost": {
          "currency": "USD",
          "microunits": 42000,
          "state": "estimated"
        }
      }
    ]
  },
  "page": {
    "limit": 100,
    "returned": 1,
    "has_more": false,
    "next_cursor": null
  },
  "traces": [
    {
      "trace_id": "trace_opaque",
      "status": "ok",
      "project_ref": {
        "availability": "available",
        "value": "project_opaque"
      },
      "cwd": {
        "availability": "unavailable_in_profile"
      },
      "error_reason": {
        "availability": "available",
        "value": "none"
      }
    }
  ]
}
```

`FieldValue<T>` has required `availability` and permits `value` only when availability is `available`.
Availability enum is `available`, `omitted_by_policy`, `unavailable_in_profile`, `unknown_source`.
Numeric aggregate completeness must sum to `total_records`; UI displays the value as complete only when all
records are available, partial when at least one value is available, and unknown when none are available.
Zero is a valid available value and never substitutes for unavailable.

`dimensions.email_identities` is present only for an authorized team query and contains at most 200 rows.
Rows group by immutable `email_identity_ref` and `agent_kind`; identity display is a strict discriminated union:

| `display_state` | Required | Forbidden | Authorization |
| --- | --- | --- | --- |
| `current_pii` | current `display_email` | `pseudonymous_label` | report/cost plus `identity:read_pii`, or own-identity projection |
| `current_pseudonymous` | bounded `pseudonymous_label` | `display_email` | report/cost without PII capability |
| `deleted_pseudonymous` | bounded non-identifying `pseudonymous_label` | `display_email` | deleted or no-longer-visible identity; still personal data |

`display_email` is joined from the current tenant-visible identity directory and is never copied from an
observation. Exports apply the same field projector. No row may contain both email and pseudonymous label.
Standalone profile omits this dimension unless a local-only identity profile was explicitly configured.
Schema fixtures cover owner/admin, self, analyst/contributor/billing, export and deleted-identity projections,
including rejection of every mixed or missing discriminant field combination.

`deleted_pseudonymous` is deliberately not called anonymized. Historical observations may retain the stable
`email_identity_ref`, so a party that previously received a PII-authorized export could correlate it. The
reference and historical aggregates remain pseudonymous personal data under observation retention, subject
access and deletion policy. Raw email and verification records are deleted on identity deletion; hosted
exports containing that email are revoked and purged, while already downloaded exports cannot be recalled and
are covered by the customer's export-handling responsibility. Filtering by the retained reference remains
authorization-scoped until its observation retention expires. True unlinkable erasure would require a separate
contract that replaces the reference in every retained record and aggregate and must not reuse this state.

`scope` is a discriminated union. Standalone is `{ "profile": "standalone", "artifact_id",
"from_unix_ms", "to_unix_ms" }`; team is `{ "profile": "team", "workspace_ref",
"from_unix_ms", "to_unix_ms" }`. The opposite profile field is forbidden. `page.limit` is 1..200,
`returned` is 0..limit, `has_more` is boolean, and `next_cursor` is opaque 1..512 chars only when
`has_more` is true. A DTO contains at most 200 trace rows.

Team profile always marks cwd/path/command/arguments/output/content/raw error as
`unavailable_in_profile`. Standalone may use `omitted_by_policy`; neither profile embeds raw content in
`ReportDtoV1`. Fixtures cover every availability state and mixed aggregate rendering.

## 5. Hosted API surface

All mutation requests include `operation_id` (opaque ASCII 1..96 chars) and return the same result for a
repeated ID with the same canonical request hash; different hash is conflict.

Request/response names below are concrete `$defs` in `management-dto-v1.schema.json`, except deletion,
quota, export/recovery and credential definitions owned by their artifact in section 2.

| Method/path | Request/response | Capability | State/result |
| --- | --- | --- | --- |
| `GET /api/team/v1/auth/start` | `AuthStartQueryV1` -> `AuthStartRedirectV1` | public | one-time state, 10-minute expiry |
| `GET /api/team/v1/auth/callback` | `AuthCallbackQueryV1` -> `SessionV1` | federation callback | active/rejected; no token in URL response |
| `GET /api/team/v1/session` | none -> `SessionV1` | authenticated human | active/refresh-required |
| `POST /api/team/v1/session/refresh` | `SessionRefreshV1` -> `SessionV1` | human | active/revoked |
| `DELETE /api/team/v1/session` | `OperationV1` -> `SessionReceiptV1` | human | logged-out |
| `POST /api/team/v1/workspaces` | `WorkspaceCreateV1` -> `WorkspaceV1` | owner | active |
| `GET /api/team/v1/workspaces/{id}` | none -> `WorkspaceV1` | member | active/suspended |
| `PUT /api/team/v1/workspaces/{id}` | `WorkspaceUpdateV1` -> `WorkspaceV1` | owner/admin | active/suspended/conflict |
| `GET /api/team/v1/workspaces/{id}/policy` | none -> `EffectivePolicyV1` | member/auditor | inherited/active/pending |
| `PUT /api/team/v1/workspaces/{id}/policy` | `PolicyUpdateV1` -> `EffectivePolicyV1` | admin | active/pending/conflict |
| `GET /api/team/v1/workspaces/{id}/members` | `MemberListQueryV1` -> `MemberPageV1` | admin/auditor | cursor page |
| `POST /api/team/v1/workspaces/{id}/members` | `MemberInviteV1` -> `MemberV1` | admin | pending/active/expired |
| `PATCH /api/team/v1/workspaces/{id}/members/{member}` | `MemberUpdateV1` -> `MemberV1` | admin | active/revoked/conflict |
| `GET /api/team/v1/identity/email-identities` | none -> `EmailIdentityPageV1` | authenticated human | own verified/pending/revoked identities only |
| `POST /api/team/v1/identity/email-identities` | `EmailIdentityEnrollV1` -> `EmailIdentityV1` | authenticated human | pending verification; no ingest use yet |
| `POST /api/team/v1/identity/email-identities/{identity}/verify` | `EmailIdentityVerifyV1` -> `EmailIdentityV1` | authenticated human | verified/rejected/expired |
| `DELETE /api/team/v1/identity/email-identities/{identity}` | `OperationV1` -> `EmailIdentityDeletionReceiptV1` | identity owner with recent re-authentication | requested/hold-blocked/revoking/pii-purging/complete/failed |
| `GET /api/team/v1/workspaces/{id}/identity-bindings` | `IdentityBindingListQueryV1` -> `IdentityBindingPageV1` | member; admin sees workspace | own or administered workspace scope |
| `POST /api/team/v1/workspaces/{id}/identity-bindings` | `IdentityBindingCreateV1` -> `IdentityBindingV1` | identity owner with enrolled source | active or pending admin approval |
| `POST /api/team/v1/workspaces/{id}/identity-bindings/{binding}/approve` | `IdentityBindingApproveV1` -> `IdentityBindingV1` | admin | active/denied; cannot change identity owner |
| `DELETE /api/team/v1/workspaces/{id}/identity-bindings/{binding}` | `OperationV1` -> `IdentityBindingReceiptV1` | binding owner or admin | revoked; source epoch updated |
| `GET /api/team/v1/workspaces/{id}/sources` | `SourceListQueryV1` -> `SourcePageV1` | admin | cursor page |
| `POST /api/team/v1/workspaces/{id}/sources` | `SourceEnrollmentV1` -> `SourceCredentialV1` | admin | pending/active |
| `POST /api/team/v1/workspaces/{id}/sources/{source}/rotate` | `SourceRotateV1` -> `SourceCredentialV1` | admin | overlap/active |
| `DELETE /api/team/v1/workspaces/{id}/sources/{source}` | `OperationV1` -> `SourceReceiptV1` | admin | revoked |
| `PUT /api/team/v1/sources/self/heartbeat` | `AdapterHeartbeatV1` -> `AdapterHeartbeatReceiptV1` | adapter-scoped source credential | monotonic latest-state upsert; never replayed |
| `GET /api/team/v1/sources/self/recovery-state` | none -> `RecoveryStateV1` | source instance | own source only; poll while ACK journal non-empty |
| `POST /api/team/v1/workspaces/{id}/observations/{event}/attribution-corrections` | `AttributionCorrectionV1` -> `ObservationRevisionReceiptV1` | `observation:correct` | applied/conflict; immutable original |
| `POST /api/team/v1/workspaces/{id}/observations/{event}/retractions` | `ObservationRetractionV1` -> `ObservationRevisionReceiptV1` | `observation:retract` | applied/conflict; report exclusion only |
| `GET /api/team/v1/workspaces/{id}/observations/{event}/revisions` | none -> `ObservationRevisionPageV1` | scoped report reader | append-only authorized history |
| `GET /api/team/v1/workspaces/{id}/reports` | `ReportQueryV1` -> `ReportDtoV1` | report reader | fresh/stale/partial |
| `GET /api/team/v1/workspaces/{id}/audit` | `AuditQueryV1` -> `AuditPageV1` | audit reader | available/integrity-warning |
| `POST /api/team/v1/workspaces/{id}/exports` | `ExportCreateV1` -> `ExportJobV1` | scoped reader | queued/running/ready/failed |
| `GET /api/team/v1/workspaces/{id}/exports/{job}` | none -> `ExportJobV1` | original requester or admin | queued/running/ready/expired/revoked/failed |
| `DELETE /api/team/v1/workspaces/{id}/exports/{job}` | `OperationV1` -> `ExportReceiptV1` | original requester or admin | revoked; repeated delete idempotent |
| `GET /api/team/v1/workspaces/{id}/data-policy` | none -> `DataPolicyV1` | data reader/admin | active/pending |
| `PUT /api/team/v1/workspaces/{id}/data-policy` | `DataPolicyUpdateV1` -> `DataPolicyV1` | data admin | active/pending/invalid |
| `POST /api/team/v1/tenants/{id}/legal-holds/{hold}` | `LegalHoldCreateV1` -> `LegalHoldReceiptV1` | owner + delegate | active/too-late |
| `DELETE /api/team/v1/tenants/{id}/legal-holds/{hold}` | `LegalHoldReleaseV1` -> `LegalHoldReceiptV1` | owner + delegate | released/conflict |
| `POST /api/team/v1/tenants/{id}/deletions` | `DeletionCreateV1` -> `DeletionReceiptV1` | owner | state machine below |
| `GET /api/team/v1/tenants/{id}/deletions/{job}` | none -> `DeletionReceiptV1` | owner/auditor | state machine below |

All list endpoints use opaque cursor, page size 1..200, server allowlisted sort key and bounded time/filter
fields. Authorization runs before existence lookup. State-changing errors are bounded reason codes:
`unauthenticated`, `unauthorized`, `epoch_stale`, `conflict`, `invalid_transition`, `hold_active`, `too_late`,
  `quota_exceeded`, `identity_binding_denied`, `temporarily_unavailable`.

All non-2xx JSON responses use `ApiErrorV1` from `operations-v1.schema.json`: required `request_id`,
`reason_code`, `retryable`; nullable bounded `retry_after_ms`; no echoed input or resource display data.
400 is invalid shape/query, 401 missing/expired authentication, 403 denied or hidden resource, 409 conflict or
invalid transition, 413 size, 422 valid shape with rejected policy, 429 quota/rate, 500 internal and 503
temporary dependency failure. Only 429/503 and explicitly refreshable 401 can be retryable. 403 and hidden
resource use the same status, error definition and response size class.

Heartbeat rejects use bounded `epoch_stale`, `heartbeat_stale`, `agent_kind_mismatch` or `unauthorized` and
never echo the submitted state. A stale heartbeat is a successful no-state-change receipt only when its exact
epoch/sequence and canonical body match the accepted heartbeat; same sequence with different body is conflict.

`AuthStartRedirectV1` is an HTTP 302 with allowlisted location and secure same-site state cookie; it contains no
access token. State is random, single-use, expires in 10 minutes and is bound server-side to the initiating
browser cookie hash, requested tenant hint and exact allowlisted return path. Callback validates state binding,
issuer, audience, nonce and authorization code once; replay, mismatch, expired state and unlisted return path
fail closed through a fixed safe redirect carrying only a bounded reason code and correlation ID.

Cursor is an authenticated opaque value scoped to tenant/workspace, endpoint, normalized filters, sort and
snapshot time. It is 1..512 chars, expires after 15 minutes and returns `cursor_expired` without exposing
resource existence. Report/audit list query parameters are `from_unix_ms`, `to_unix_ms`, `project_ref`,
`repository_ref`, `email_identity_ref`, `identity_binding_ref`, `agent_kind`, `status`, `sort`, `limit`,
`cursor`; unknown parameter is rejected. The schema artifacts own
exact enum, presence and bounds.

## 6. Identifier and key contracts

Human access token claims are `iss`, `aud`, `sub`, `session_id`, `authorization_epoch`, `iat`, `exp` and
tenant reference; role/project capability is resolved server-side. Access TTL is <=15 minutes. Refresh proof
is opaque, single-use and rotated on every refresh; only its verifier/hash is stored. Hosted browser session
uses secure, HTTP-only, same-site cookies and never puts access/refresh token in URL or application log.

Source credential claims are `iss`, ingest `aud`, `source_instance_id`, `adapter_installation_id`, fixed
`agent_kind`, current `source_epoch`, nullable `source_principal_id`, `workspace_id`, scopes limited to
`ingest:write`, `heartbeat:write` and `recovery:read`, `authorization_epoch`, `iat`, `exp`, `credential_id`. Default lifetime is
30 days and rotation overlap is <=10 minutes. Enrollment binds the one-time credential to the server-created
source/workspace and returns it once. Rotation preserves source identity; revoke/epoch change is enforced
within 30 seconds. Server stores only a verifier/hash and metadata. Credential fixtures cover wrong issuer,
audience, scope, workspace, epoch, expiry, replayed refresh, overlap end and revoked source.

- Tenant/workspace/project/repository/source IDs are server-issued random opaque IDs with no encoded owner,
  path or sequence. They are unique only within documented scope and not reversible.
- Project/repository references are configured during enrollment; local paths and names are never hashed or
uploaded to discover them.
- Human principal IDs, email identity IDs and identity binding IDs are independent server-issued opaque IDs.
  One principal may own multiple verified email identities. Raw email is encrypted in the central identity
  directory and appears only in authorized identity/report DTOs, never ingest, queue, audit event payload,
  cursor or identifier derivation.
- Trace/span/event IDs are client-generated random opaque IDs. They must not be derived from content or local
  paths. Cross-tenant equality has no meaning.
- Model ref is a bounded allowlisted canonical model identifier. Unsupported values become `unknown`; raw value
  is not uploaded.
- No shared tenant salt or client-side pseudonym key is required in V1, avoiding dictionary/linkability risk.

## 6.1 Email identity and adapter contracts

`EmailIdentityV1` contains opaque identity ID, email display value, verification state, purpose category,
bounded purpose label and timestamps. Email comparison uses a reviewed canonicalization policy without
rewriting the user-visible address; uniqueness scope and provider-specific alias behavior are fixed in G1
fixtures. Verification proof is single-use, expires, is rate-limited and is never returned after validation.

Email identity deletion is idempotent by `operation_id` and returns a receipt containing only opaque identity/
operation references, state, bounded reason, target counts, timestamps and audit reference. It never echoes
the email. The accepted transition atomically disables the identity for new bindings, revokes every active
binding, bumps affected source authorization epochs and changes report projection to
`deleted_pseudonymous`. New attribution must be denied within 30 seconds.

The asynchronous `pii-purging` phase removes raw email, canonical lookup value, verification proof and
identity-directory caches; revokes and purges hosted exports containing the email; and emits bounded audit
actions without PII. Historical observations retain only the pseudonymous reference until their normal
retention expires. `complete` requires every owned PII target and hosted export target to be verified absent.
Partial failure stays `failed` with retryable bounded target classes and resumes idempotently; it must not
re-enable bindings. An approved legal hold may produce `hold-blocked` only when the G0 legal policy explicitly
requires that PII target, with no silent completion. Previously downloaded exports cannot be recalled and are
reported as an external-copy limitation in the receipt without recording their contents.

Raw identity PII is encrypted with a per-identity PII data key wrapped by the tenant key. Deletion destroys
that PII key after the purge fence, so pre-deletion backups retain only undecryptable ciphertext. A durable
PII-free deletion tombstone records identity reference, deletion generation, destroyed key version, fence and
operation reference. Tombstones replicate to the recovery failure domain before `complete`, outlive the
maximum backup retention plus restore window and are applied before identity rows, caches, projections or
exports during restore. Restore must suppress deleted identity material even when the backup predates deletion.
The receipt reports key-destruction and restore-suppression evidence plus the latest affected backup generation,
without key material or email.

Identity deletion and legal-hold mutation use the same tenant-scoped lock, monotonic `hold_epoch` and purge
fence model as tenant deletion. The operation snapshots the hold epoch before acceptance and revalidates it
immediately before key destruction. A hold committed first yields `hold-blocked`; once the deletion purge fence
is committed for the identity PII target, a new hold for that destroyed target returns `too_late`. Hold release
requeues the same idempotent operation after epoch revalidation. No race may both claim an active hold and
destroy the protected key.

`IdentityBindingV1` connects one verified email identity to one source/adapter and bounded workspace/project
scope. Required adapter enum values are `codex`, `claude`, `cursor`; unsupported tools use a future contract
version rather than arbitrary strings. Local profile resolution is explicit profile, project policy, adapter
account binding, source default, then source-only. Ambiguity must not select the first email silently.
Only the identity owner may create the binding request. Admin approval can restrict or deny requested scope
but cannot replace its principal or email identity. Workspace policy may require verified domain, permitted
purpose category and explicit approval; ownership, approval and revocation produce separate audit actions.
For first GA, a binding is usable only when its principal equals the credential's non-null
`source_principal_id`. A credential with null principal is shared/unbound and always source-only. Signed
per-session attribution grants are deferred behind a new threat model and contract version.

Authorized report APIs accept `identity_binding_ref` or `email_identity_ref` as server-validated filters and
return email breakdown rows only within caller scope. Cross-tenant principal correlation is forbidden even
when the normalized email strings match. Revocation prevents new attribution within 30 seconds; immutable
historical records retain the opaque binding while display email follows retention/deletion policy.

Identity deletion fixtures cover repeated operation IDs, changed-body conflict, concurrent binding creation,
30-second attribution denial, legal hold, partial target failure/resume, cache purge, hosted export revocation,
PII-free audit/receipt, downloaded-export limitation, hold-create/release races, pre-deletion-backup restore,
tombstone-first recovery, destroyed-key denial and final absence verification.

`AdapterHeartbeatV1` permits only schema version, credential-matching `agent_kind`, `source_epoch`, monotonic
`heartbeat_seq`, bounded agent/adapter versions, `observed_at_unix_ms`, `activity_state`, `sync_state`, queue
depth/age bucket and fixed capability flags. The server resolves adapter registration from the credential,
rejects an epoch other than the current server epoch and accepts only a sequence greater than the stored
sequence for that epoch. Client time never overrides server receipt freshness.
Heartbeat receipt contains server time, accepted state and bounded `next_heartbeat_after_ms`. Heartbeats are
not queued, replayed, deduplicated as usage, metered as customer usage or stored as an unbounded event stream.
Fixtures cover stale/replayed sequence, concurrent updates, epoch reset, wrong agent kind, cross-adapter
credential use and credential rotation overlap.

`AttributionCorrectionV1` and `ObservationRetractionV1` require `operation_id`, target `event_id`,
`expected_revision` and a bounded reason enum. Correction additionally requires an active
`identity_binding_ref` in the same tenant/workspace and within the caller's project scope. Retraction additionally
requires action enum `retract` or `reinstate` and has no free-form payload. The server derives actor and scope from
authentication, never from the request body.

Contributor authorization is anchored to the accepted record's immutable original principal and project scope;
it does not move when current attribution changes. The destination binding must belong to that same principal.
Only Owner/Admin can revise source-only or ambiguous observations. New revisions are accepted only while the
target observation is retained; revision rows and their aggregate effect remain through aggregate retention.
Expired/hidden targets use the same non-disclosing response shape.

Revision uniqueness is `(resolved tenant, resolved workspace, operation_id)`. Same canonical request returns
the existing receipt; changed body conflicts. `expected_revision` is compare-and-set, so concurrent operations
cannot both claim the same predecessor. Effective state stores attribution and visibility independently.
Correction moves the contribution only when visible; retract subtracts the current-attribution contribution;
reinstate adds it back exactly once. The aggregate transaction writes the revision effect,
`aggregate_contribution_journal`, bucket delta, checkpoint/outbox progress and one audit entry atomically. A later
explicit correction or reinstate revision is the only supported reversal; physical undo/redo and mutation of the
accepted record are forbidden. Privacy deletion remains a separate key-destruction workflow.

Tenant data key states are `active`, `decrypt_only`, `rewrapping`, `recovery_only`, `retired`, `destroyed`.
Authenticated encryption uses a reviewed standard AEAD implementation; algorithm and key length are fixed in the G1 crypto
artifact rather than hand-implemented. Root wrapping key, audit signing key and tenant data key have separate
custody and access policy. Rotation writes with the new key, rewraps, verifies counts/hashes, then retires the
old key. Failure remains `rewrapping_failed` and keeps old decrypt capability; it never destroys the old key
before verification. A backup key-version inventory prevents transition from `recovery_only` to `retired`
until reference count is zero or every referenced backup is re-encrypted and restore-verified. Deletion
destroys every non-destroyed tenant key version, including active, decrypt-only, recovery-only and retired,
after primary targets purge and
records key-version evidence without retaining key material.

Each email identity PII record has a separately wrapped identity PII data key with states `active` and
`destroyed`; it is never reused across identities. Destroy is one-way, serialized with the identity purge
fence and recorded only by key version/evidence hash. Backup restore loads the deletion tombstone ledger
before unwrap; a tombstoned or destroyed identity key is denied even if an older wrapped-key row is present
in the backup.

`team-crypto-v1.yaml` defines algorithm identifier, key/nonce/tag length, additional-authenticated-data fields,
key owner/custodian, tenant and identity-key state transitions, backup inventory rule and restore-denial checks. The verifier
future target is `team_crypto_v1`; evidence covers cross-tenant unlinkability, rotation
interruption/resume, recovery-only backup restore, retired-key denial and destroyed-key denial.

## 7. Deletion contract

Requirement IDs: `TEAM-DEL-001` through `TEAM-DEL-008`.

Deletion operation key is `(tenant_id, operation_id)`. Same request hash returns the existing job; different
hash conflicts. Only one non-terminal deletion exists per tenant.

| Current | Event/guard | Next |
| --- | --- | --- |
| requested | validation starts | validating |
| validating | hold epoch stable, confirmation valid | access_revoked |
| validating | active/new hold before fence | hold_blocked |
| hold_blocked | hold released, epoch captured | validating |
| access_revoked | purge fence rechecks hold epoch | primary_purging or hold_blocked |
| primary_purging | all primary targets and data keys verified | primary_purged |
| primary_purging | transient target failure | failed_retryable |
| failed_retryable | retry with same job/checkpoint | prior incomplete state |
| any pre-fence state | invalid authority/contract | failed_terminal |
| primary_purged | backup expiry tracking starts | backup_expiring |
| backup_expiring | all generations expired and restore denial verified | complete |

After purge fence, new hold request returns `too_late`; no transition reverses `primary_purged`. Terminal failure
requires incident owner and a new audited remediation operation, not mutation of history.

Receipt DTO includes operation/request hash, legal-hold epoch, purge fence time, target inventory with
`pending/running/verified/failed`, key versions destroyed, primary purge time, backup generation expiry,
restore-denial result, failures and final state. G3 fixtures cover duplicate/concurrent request, hold race,
process crash at every transition, retry checkpoint and restore of a deletion tombstone.

## 8. Quota contract

Requirement IDs: `TEAM-QUOTA-001` through `TEAM-QUOTA-006`.

`QuotaPolicyV1` fields:

- `policy_version`, `effective_at`
- accepted record and canonical-byte limits per UTC minute and UTC calendar month
- active member/source hard limits
- concurrent query/export limits
- export canonical result-byte limit
- retained storage soft warning limit

Window key is `(tenant, workspace, resource, UTC window start, policy_version)`. One operation creates a
reservation set containing a unit vector for every affected resource/window, for example minute records,
minute bytes, month records, month bytes and current concurrency slot. All vector elements reserve in one
transaction; failure of any element rolls back the entire set. Commit/release is atomic across the set and no
partial terminal state is valid. Concurrency slots are reservation elements without permanent metered usage
and are released when the operation ends.

Each reservation set includes operation ID, unit vector, created/expiry time and state
`reserved/committed/released/expired`. Default
reservation TTL is 2 minutes for ingest/query and 15 minutes for export. Crash recovery expires stale
reservations only after checking the authoritative operation result; it commits completed operations and
releases unstarted ones. Window reset never deletes ledger entries.

Concurrency fixture spans minute/month boundaries and launches requests at one unit below multiple hard
limits. It passes only when committed units
never exceed the policy, every operation has exactly one terminal ledger state, duplicate operations add zero
units, partial reservation never persists, concurrency slots return to baseline and client-visible
accepted/rejected counts reconcile with ledger totals.

## 9. DR and evidence contracts

Hosted topology proposal:

- stateless ingest/query instances across at least two failure zones
- synchronous authoritative database quorum across zones in configured region
- encrypted incremental backup/checkpoint at least every 5 minutes to a separate approved recovery failure
  domain within the tenant's G0-approved residency policy
- daily full backup, 35-day planning retention
- source ACK recovery journal replay in a new `recovery_epoch`

Zone drill passes only when ACKed records have RPO 0 and service recovers within 60 minutes. Region drill
restores the latest backup, publishes a new epoch, replays all unexpired journals, then compares pre-failure
manifest counts and hashes for records, unique IDs, revisions, aggregate contribution journal, aggregate buckets,
projection checkpoints/outbox, ledger, policy, key versions and audit roots. Backend-only
RPO must be <=5 minutes and recovery <=4 hours; gaps from sources without journals are enumerated in the
incident receipt.

Evidence manifest `docs/evidence/team/<gate>/<test>/manifest.yaml` contains:

- gate, test ID, requirement IDs, commit and artifact versions
- sanitized topology/workload/seed, tool and runbook revisions
- start/end/server timezone and raw numeric measurements
- threshold, exclusions, pass/fail and failure links
- author identity, independent reviewer identity and role, review timestamp and blocking findings

G4 review passes only when security and architecture reports are separate artifacts and reviewers did not
author the reviewed implementation. Critical findings are non-waivable and must be resolved. A high finding
may be a non-blocking, time-bounded exception only with named security and business approvers, compensating
controls, expiry, remediation owner and matching status in both reports. No unresolved blocking finding may
remain.
