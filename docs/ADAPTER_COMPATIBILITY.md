# Adapter Compatibility Contract

Status: proposed target contract
Last verified: 2026-08-28

이 문서는 Codex, Claude Code, Cursor adapter가 어떤 공식 surface를 어떤 우선순위로 사용하고,
어떤 evidence가 있어야 특정 제품/version을 지원한다고 표시할 수 있는지 정의한다. 제품 업데이트로
surface가 바뀌면 이 문서와 versioned capability manifest를 함께 갱신한다.

## Source precedence

하나의 canonical field에는 하나의 primary source만 둔다. Supplement source는 primary가 제공하지
않는 field만 보완하며 같은 usage observation을 별도 event로 중복 생성하지 않는다. Correlator는
adapter family, source generation, 공식 session/conversation/generation identifier와 source cursor로
동일 observation을 합친다.

| Adapter | Primary source | Supplement / fallback | Verified official reference |
| --- | --- | --- | --- |
| Codex | native telemetry for model/API/token/tool signals | lifecycle hooks for local handoff and lifecycle gaps; documented local session output only as a version-gated reconciliation fallback | [Advanced configuration](https://learn.chatgpt.com/docs/config-file/config-advanced), [Configuration reference](https://learn.chatgpt.com/docs/config-file/config-reference) |
| Claude Code | native telemetry for usage, cost and tool metrics | hooks for lifecycle events; documented transcript fields only as a version-gated reconciliation fallback | [Hooks reference](https://code.claude.com/docs/en/hooks), [Monitoring usage](https://code.claude.com/docs/en/monitoring-usage) |
| Cursor | official hooks for conversation, generation, model, user-email and transcript references | project/team hook coverage where local and cloud execution differ; transcript parsing only for documented fields and validated fixtures | [Hooks reference](https://cursor.com/docs/hooks) |

Raw email supplied by a product surface is used only for local profile matching. It is immediately resolved to
an opaque `identity_binding_ref`; raw email is not written to observation records, local outbox, retry queue,
diagnostics or team ingest.

## Runtime rules

- Hook handlers perform bounded validation and a constant-size local IPC/spool handoff only. They do not call
  team REST endpoints, render reports, scan transcripts or wait for background flush completion.
- Observational command hooks use host asynchronous mode where supported and never opt into fail-closed behavior
  for capture. A synchronous-only host handler has a 10 ms enqueue deadline and 50 ms total deadline, then exits
  successfully on timeout, unavailable daemon or full channel. Exact event/mode support is versioned evidence,
  not a cross-product assumption.
- Native telemetry is received by a local endpoint and normalized asynchronously. Exporter batch/flush behavior
  is not treated as proof that the observation reached this product; only the local durable transaction is.
- File/transcript fallback uses a persisted generation fingerprint and cursor, filesystem notification where
  reliable, and adaptive reconciliation. It never rescans an unchanged file from byte zero.
- Undocumented credential stores, browser sessions and private account APIs are never scraped. An undocumented
  file format may be used only as an explicitly experimental, local-only parser with fixture evidence and a
  blocked-by-default team capability.
- Source drift, missing mandatory fields or an unsupported product version isolates that adapter as `degraded`
  or `blocked`. Other adapters, local durable capture and the static report continue to work.

## Required capability evidence

Each supported product/version range must publish a versioned capability entry containing the source surface,
primary/supplement field map, product range, verification date, fixture digest, known gaps and fallback status.
Support requires all mandatory scenarios below.

Each entry in `crates/contracts/capabilities/adapter-capability-v1.yaml` contains:

- adapter family, oldest/newest supported product version, OS/profile coverage and verification date
- official reference URL and source surface version
- event name, execution context (`local`/`cloud`), command mode (`async`/`sync_bounded`), failure behavior and
  handler deadline
- canonical field -> primary source map plus allowed supplement and correlation key
- captured input fixture hash, expected observation fixture hash, privacy sentinel result and known gaps
- duplicate/truncation/restart/source-rotation/slow-daemon/full-channel fixture IDs

`cargo test -p contracts adapter_capability_v1` validates schema and field ownership. The adapter-specific
replay command tests every declared fixture; `cargo run -p xtask -- adapters verify --oldest-newest --check`
fails on a missing product bound, stale fixture digest, duplicate primary field, unsupported mandatory signal or
privacy/performance failure. Evidence is written only when the actual supported product versions are exercised.

| Scenario | Required evidence |
| --- | --- |
| session and turn | stable boundaries, restart, interrupted turn and timestamp ordering fixtures |
| LLM usage | model availability state and input/output/cache token semantics without double counting |
| tool lifecycle | parent relation, start/end or bounded duration, status and bounded reason category |
| identity | explicit/default profile, purpose-specific email match, ambiguous source-only and cross-principal denial |
| privacy | raw prompt/output/path/email/credential sentinels absent from every durable and transmitted sink |
| recovery | duplicate delivery, truncation, source rotation, crash between each local transaction step and offline replay |
| performance | hook latency, idle/active CPU, RSS, disk growth, queue pressure and load-shedding evidence |

Mandatory signal absence is `unknown_source` and blocks support for that product/version. Optional fields may
remain `unknown_source`. Capability verification must run against the oldest and newest declared supported
version and rerun when an official surface or schema changes.

## Current planning assumptions

This matrix records source availability, not implementation completion. The current JavaScript baseline has
partial Codex and Claude Code file/hook adapters. The Rust adapters, native telemetry receivers, Cursor support,
capability manifest and performance evidence are roadmap work and must not be reported as released before their
version gates pass.
