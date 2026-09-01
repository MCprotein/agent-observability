# Adapter Compatibility Contract

Status: v1.8.0 In Progress; Codex automatic local E2E passes locally, exact-revision CI and publication pending, exact-version private imports supported
Last verified: 2026-09-02

이 문서는 Codex, Claude Code, Cursor adapter가 어떤 공식 surface를 어떤 우선순위로 사용하고,
어떤 evidence가 있어야 특정 제품/version을 지원한다고 표시할 수 있는지 정의한다. 제품 업데이트로
surface가 바뀌면 이 문서와 versioned capability manifest를 함께 갱신한다.

## Source precedence

하나의 canonical field에는 하나의 primary source만 둔다. Supplement source는 primary가 제공하지
않는 field만 보완하며 같은 usage observation을 별도 event로 중복 생성하지 않는다. Correlator는
adapter family, source generation, 공식 session/conversation/generation identifier와 source cursor로
동일 observation을 합친다.

아래 표는 source 의미, field ownership과 현재 runtime 경계를 정의한다.

| Adapter | Primary source | Supplement / fallback | Current runtime support | Verified official reference |
| --- | --- | --- | --- | --- |
| Codex | native telemetry for model/API/token/tool signals | `agent-turn-complete` notify owns turn completion only; local session output is unused | macOS automatic private-CA HTTPS local OTLP/HTTP JSON with exact private random request header + projected bounded notify, plus manual handoff import; actual Codex e2e is part of the exact-revision release gate | [Advanced configuration](https://developers.openai.com/codex/config-advanced), [Configuration reference](https://developers.openai.com/codex/config-reference) |
| Claude Code | native telemetry for usage, cost and tool metrics | hooks for lifecycle events; no transcript dependency in the Rust adapter | manual handoff import only; automatic producer/receiver TODO | [Hooks reference](https://code.claude.com/docs/en/hooks), [Monitoring usage](https://code.claude.com/docs/en/monitoring-usage) |
| Cursor | generic `preToolUse`/`postToolUse`/`postToolUseFailure` hooks with `tool_use_id` | session/turn lifecycle hooks; specific shell/MCP/file hooks are diagnostic-only because they lack a shared operation ID | manual handoff import only; automatic producer/receiver TODO | [Hooks reference](https://cursor.com/docs/hooks) |

Cursor의 durable `tool.name`은 원문 이름이 아니라 `shell`, `mcp`, `file`, `agent`, `other` 중 하나인
bounded canonical category다. 같은 `tool_use_id`의 start/result/failure source observation은 append-only로
보존하고 current durable record와 report에서는 하나의 operation span으로 reduce한다.

Raw email supplied by a product surface is used only for local profile matching. It is immediately resolved to
an opaque `identity_binding_ref`; raw email is not written to observation records, local outbox, retry queue,
diagnostics or team ingest.

For Codex automatic collection, raw notify JSON is projected by the foreground helper before transport. Bounded
raw OTLP JSON and tool attributes may transiently enter receiver memory during decode. Only explicitly owned
scalar values cross the adapter boundary:

- `conversation.id`, `turn.id`, request/call ID and fixed source cursor/generation
- known model, bounded tool category and bounded permission decision
- duration, input/output/cached/reasoning/total token counts and success state

Prompt, response, tool arguments/output, command, cwd, path, raw account identity and unknown attributes are
discarded before canonical mapping. They are never persisted, logged, placed in diagnostics, projected to JSONL
or HTML, reported, archived or exported.

## Runtime rules

- The Codex notify helper accepts at most 64 KiB of raw input, projects it to a smaller closed content-free
  representation before any I/O, and performs only a bounded local private-CA HTTPS + exact-header handoff. Accepted, rejected and
  unavailable outcomes all exit successfully so observation cannot block Codex work. It does not call team
  endpoints, render reports, scan transcripts or wait for background flush completion.
- The Codex OTLP receiver binds only IPv4 loopback. Clients validate its private-CA server certificate and
  loopback IP SAN; every request must carry the exact `x-agent-observability-token` header containing the
  runtime's private random 256-bit value. The exporter contains no client certificate/private-key fields, so the
  transport is not mTLS. The receiver limits a JSON request to 1 MiB and 4096 log records and assigns monotonic
  local cursors. Exporter success is not treated as durability; only the local transactional commit is authoritative.
- Claude Code and Cursor automatic foreground producers remain future work. Their exact event/mode support must
  be backed by versioned evidence rather than inferred from Codex behavior.
- A future file fallback must use a persisted generation fingerprint and cursor, filesystem notification where
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

- adapter family, support status, platform/profile/import boundary, oldest/newest checked product version and verification date
- official reference URL, source surface ID/role, event names and uniquely owned canonical fields
- correlation keys, closed privacy flags, known gaps, fixture IDs and input/projection fixture hashes

`cargo test -p agent-observability-contracts adapter_capability_v1` validates schema, field ownership and
privacy closure. The Codex, Claude Code and Cursor adapter suites verify declared input/projection fixture hashes,
exact replay output, bounded input, restart/idempotency and privacy behavior. Claude Code additionally locks permission,
compaction, failed lifecycle, interrupt-gap and out-of-order timestamp fixtures. The capability manifest publishes
separate manual `private_canonical_handoff_v1` entries and a macOS standalone `codex_automatic_local.v3` entry
pinned to Codex `0.151.0`. It remains `experimental` on the release branch until native receiver, foreground
notify, privacy, restart, exact-binary performance and publication evidence pass for the final revision. Release
promotion changes that same closed entry to `supported`; cross-version/OS/profile execution remains a future gate.

The automatic-path release gate is
`cargo run -p xtask -- perf automatic --profile release --check`. Its versioned protocol is
`crates/contracts/performance/automatic-local-performance-v1.yaml`; the older `perf local` workload does not
substitute for collector or foreground-notify evidence. The gate first runs actual Codex `0.151.0` against a
content-free loopback Responses fixture, then drives synthetic Codex-shaped OTLP through the product client.

Codex `0.151.0` on macOS loads the previous client-identity config under the strict diagnostic but fails later
while constructing the exporter. The automated v1.8.0 gate now proves the corrected private-CA HTTPS plus
exact-header exporter with actual `codex exec`, native OTLP acceptance, private session and exact 10-input/2-output
token records, and a durable-tree raw-prompt sentinel scan.

| Scenario | Required evidence |
| --- | --- |
| session and turn | stable boundaries, restart, terminal lifecycle, timestamp ordering, and either explicit interrupt support or a verified interrupt gap |
| LLM usage | model availability state and input/output/cache token semantics without double counting |
| tool lifecycle | parent relation, start/end or bounded duration, status and bounded reason category |
| identity | explicit/default profile, purpose-specific email match, ambiguous source-only and cross-principal denial |
| privacy | raw prompt/output/path/email/credential sentinels absent from every durable and transmitted sink |
| recovery | duplicate delivery, truncation, source rotation, crash between each local transaction step and offline replay |
| performance | hook latency, idle/active CPU, RSS, disk growth, queue pressure and load-shedding evidence |

Mandatory signal absence is `unknown_source` and blocks support for that product/version. Optional fields may
remain `unknown_source`. Capability verification must run against the oldest and newest declared supported
version and rerun when an official surface or schema changes.

## Current implementation boundary

Historical v0.6 fixtures retain partial Codex and Claude Code source examples. Rust Codex, Claude Code and
Cursor adapters implement bounded canonical handoff parsers, fixed source precedence, content-free dispositions,
fixture hash validation and CLI-to-private-store replay. Claude Code uses documented OTel events as primary and
hooks only for lifecycle. Cursor uses generic tool hooks as primary and lifecycle hooks as supplement; specific
shell/MCP/file hooks remain diagnostic-only, and raw transcript/content fields are not parsed.

The v1.8.0 code adds a Codex-only OTLP/HTTP JSON receiver, bounded notify helper, exact config ownership and
macOS LaunchAgent. It does not add OTLP/gRPC, Claude Code automatic collection, Cursor automatic collection,
file scraping or a team transport. Manual imports remain the stable shared boundary. The automatic capability is
pinned to Codex `0.151.0`. Local actual-Codex evidence has passed; exact-revision CI and publication evidence
remain release gates, so the source entry stays `experimental` and v1.8.0 stays **In Progress**, not Released.
Other platforms fail closed for automatic setup until equivalent service, no-follow, identity, permission and
execution evidence exists; manual private imports retain their existing supported boundary.
