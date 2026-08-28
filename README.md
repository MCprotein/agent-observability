# Agent Observability

AI coding agent의 토큰 사용량, latency, tool call, error, context compaction,
handoff, approval, sandbox 상태를 관측하기 위한 내부 설계 정리.

이 문서는 사내 적용을 목표로 정리한 독자 아키텍처 초안이다.

## 현재 구현 상태

현재 `v0.10.0`은 JavaScript migration baseline 위에 experimental Rust Codex와 Claude Code
adapter를 제공한다. Rust 경로는 bounded canonical handoff JSONL에서 각 제품의 OTel log와
lifecycle supplement를 정규화해 private local state에 저장한다. Native OTel endpoint와 foreground
spool writer, Cursor Rust adapter, Rust static HTML 생성은 아직 구현하지 않았다.

현재 구현 기술은 Node.js 20+ ESM JavaScript 기준선과 Rust 1.97 Cargo workspace다.
Rust workspace는 다음 책임으로 분리된다.

- `crates/domain`: opaque ID, span/status/lifecycle, token usage 의미
- `crates/contracts`: complete transient `SourceObservation`, wire-compatible `DurableRecordV1`와
  `ReportDtoV1`, adapter capability와 disposition checkpoint contract
- `crates/adapter-codex`: bounded Codex handoff parser, canonical correlation,
  primary/supplement dedupe, content-free diagnostic
- `crates/adapter-claude-code`: Claude Code OTel/hook precedence, permission/compaction,
  interrupted lifecycle와 out-of-order timestamp normalization
- `crates/application`: pricing policy와 privacy-safe report DTO projection
- `crates/local-store`: private SQLite authority, atomic cursor/event/current-record/disposition
  commit, replayable JSONL projection, `local_state.v1` -> `local_state.v2` migration
- `crates/cli`: Rust command path의 composition root
- `contracts/*.schema.json`: JavaScript/Rust/향후 TypeScript가 공유하는 closed JSON Schema
- `contracts/contract-manifest.v1`: schema path와 version/boundary index

목표 기술 스택은 다음과 같다.

현재 JavaScript adapter는 전체 JSONL parsing과 순차 append를 포함하므로 foreground hook 성능
sign-off 대상이 아니다. `v0.13.0`의 Rust end-to-end performance harness가 통과하기 전에는 사용자
기기 성능 보호가 구현 완료되었다고 표시하지 않는다.

- web UI: TypeScript
- domain, application, agent adapters, storage, export, CLI, optional collector/query API: Rust
- Rust와 TypeScript 사이: versioned JSON schema와 generated/validated types
- report 실행 방식: 별도 server가 필요 없는 self-contained static HTML

책임 경계, SOLID/OOP/FP 적용 원칙, 허용 패턴과 모델 호환성 규칙은
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)를 기준으로 한다.

- `agent_observability.v1` span record schema
- parent/child span field와 fixture
- append-only JSONL writer
- sequential replay no-op과 stable identity conflict 거부
- private local artifact permission과 strict metadata allowlist
- durable write 전 content logging / secret / sensitive path redaction
- Codex session JSONL / notify payload 정규화
- Codex session, turn, LLM request, tool execution span 생성
- Codex token / latency metric capture
- self-contained static HTML report renderer
- session / repo / turn trace viewer
- token / latency / error summary
- rate table 기반 `estimated_cost`, `rate_table.version`, `cost.assumption`
- unknown / incomplete pricing 상태
- span display name 안전화
- redacted JSON snapshot export
- local log / report / export raw content leak fixture
- Claude Code hook / transcript JSONL 정규화
- Claude Code session, turn, LLM request, tool execution, permission, compaction span 생성
- Claude Code token / tool duration / permission / compaction metric capture
- Node test fixture

검증:

```bash
npm test
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p agent-observability-cli -- contracts
cargo run -p agent-observability-cli -- codex-ingest <private-store-dir> <private-handoff.jsonl>
cargo run -p agent-observability-cli -- claude-code-ingest <private-store-dir> <private-handoff.jsonl>
```

`v0.10.0` Rust Claude Code adapter는 구현과 privacy/replay fixture 검증을 완료했으며 독립 리뷰를
release gate로 실행한다. 다음 release train은 `v0.11.0` Rust Cursor adapter다.

## 아키텍처 요약

AI coding agent 관측은 OS/network proxy로 모델 요청을 몰래 가로채는 방식보다,
각 도구가 제공하는 hook, local transcript, session log, native telemetry, custom
endpoint 설정을 조합하는 방식이 안전하다.

```text
MIGRATION BASELINE v0.6 - IMPLEMENTED (Node.js ESM JavaScript)

Codex / Claude Code
        -> JS adapters and correlation
        -> agent_observability.v1 records
             |-> local JSONL -> JS report projection -> self-contained HTML
             `-> redacted JSON snapshot

CURRENT v0.10 - IMPLEMENTED (experimental Rust Codex + Claude Code adapters)

Closed schemas + manifest -> deterministic domain reducer + fail-closed projectors
Codex OTel/notify or Claude Code OTel/hook canonical handoff -> bounded Rust adapters
        -> SourceObservation or fixed-code diagnostic/suppression
        -> private SQLite authority (cursor + stable event + current record + disposition)
             `-> replayable JSONL current-record projection
Pricing policy + aggregation -> ReportDtoV1
Rust CLI -> contracts inspection / local storage health check / Codex and Claude Code handoff ingest
No Node.js <-> Rust FFI or subprocess production path

TARGET - PLANNED (Rust + TypeScript)

Agent logs/hooks, including planned Cursor support
        -> bounded local handoff + Rust inbound adapters
        -> SourceObservation (transient, never durable)
        -> domain lifecycle state and application use cases
             |-> atomic local state: source cursor + record
             |       `-> fail-closed DurableRecordVx -> JSONL / snapshot projection
             |-> strict TeamIngestEnvelopeV1 -> optional collector (Future TODO only)
             |-> pricing + aggregation -> fail-closed projector -> ReportDtoVx
             |                                      -> TypeScript strict UI assets
             |                                      -> self-contained HTML
             `-> fail-closed diagnostic contract -> diagnostics
```

TypeScript UI는 원본 agent payload나 JSONL을 직접 읽지 않는다. Rust가 생성한 versioned
`ReportDtoVx`와 빌드된 UI asset을 Rust outbound infrastructure가 하나의 HTML artifact로
조립한다. export나 선택적 collector도 각 sink 전용 fail-closed contract 뒤에만 추가한다.

### 사용 프로필

| Profile | 혼자 사용 | 팀 사용 |
| --- | --- | --- |
| Runtime | Rust CLI만 실행 | 각 사용자 Rust CLI + 선택적 collector |
| Storage | private embedded state + JSONL/snapshot projections | same local state/outbox + tenant/workspace-scoped central store |
| UI | 서버 없는 self-contained HTML | 같은 TypeScript UI의 hosted report 또는 정적 export |
| Network/login | 불필요 | collector 사용 시에만 필요 |

혼자 쓸 때도 모든 핵심 기능이 동작하며 팀 기능은 선택 사항이다. 팀 모드에서는
전용 fail-closed projector가 허용된 관측 필드만 담은 `TeamIngestEnvelopeV1`을 만든다. local
content logging opt-in 여부와 무관하게 전체 `DurableRecordVx`, 원본 `SourceObservation`,
prompt/output/tool content는 로컬 밖으로 전송하지 않는다. 두 모드는 같은 domain 의미,
pricing, privacy, report DTO와 UI component를 공유한다.

collector는 client가 주장한 tenant/workspace/actor를 신뢰하지 않는다. 인증 주체의 membership과
role로 scope를 결정하고 ingest, storage, query에 같은 tenant/workspace predicate를 적용한다.
한 사람은 여러 verified email identity를 등록할 수 있고 회사/개인/고객/프로젝트 용도별
`identity_binding_ref`를 Codex, Claude, Cursor adapter profile에 연결한다. Team envelope에는 raw
email 대신 이 opaque binding만 들어가며 collector가 source, membership과 project scope를 검증한
뒤 중앙 identity directory와 join한다. 따라서 이메일별 사용량을 조회하면서도 raw email을
event, queue와 retry journal에 반복 저장하지 않는다. First GA의 사람 귀속 source는 한 principal에
묶이고 그 사람의 여러 email identity만 선택할 수 있다. 공용/unbound source와 identity가 모호한
session은 source-only로 남긴다. Raw email 조회·export는 별도 PII capability가 필요하다.
상용 team profile의 tenancy, credential, RBAC, ingest, storage, retention/deletion, encryption,
audit, quota, SLO/DR와 delivery gate는 [team architecture](docs/TEAM_ARCHITECTURE.md)를 따른다.
Wire/API/DTO/state와 evidence 형식은 [team contracts](docs/TEAM_CONTRACTS.md)를 따른다.
공통 분석 UI와 team 관리 UI는 [DESIGN.md](DESIGN.md)를 따른다.
제품별 공식 수집 surface, source precedence와 지원 evidence는
[adapter compatibility contract](docs/ADAPTER_COMPATIBILITY.md)를 따른다.

핵심 원칙은 다음과 같다.

- agent별 공식 또는 사실상 공식 surface를 우선 사용한다.
- prompt, output, tool output은 기본적으로 redaction과 opt-in 정책을 거친다.
- 로컬 adapter는 hook foreground에서 bounded local handoff만 수행하며 network나 전체 transcript parse를
  기다리지 않는다.
- source cursor, stable event ID, local record와 optional team outbox는 한 transaction으로 기록하고,
  JSONL/HTML은 재생성 가능한 projection으로 취급한다.
- file reconciliation, flush와 heartbeat 주기는 bounded config로 조정할 수 있고 jitter/adaptive backoff를
  적용한다. CPU/RSS/disk budget을 넘으면 team sync와 report refresh를 먼저 늦추며 agent 실행을 막지 않는다.
- 여러 agent를 동시에 쓰더라도 뒤에서는 하나의 trace/span schema로 합친다.
- 중앙 collector는 1차 PoC의 필수 구성요소가 아니라 팀 단위 집계가 필요할 때 붙이는 선택 경로다.
- standalone과 team은 별도 제품이 아니라 같은 core의 deployment profile이다.
- backend는 교체 가능해야 하며, 특정 벤더나 구현체에 종속하지 않는다.
- Desktop App과 CLI/IDE extension은 설정 상속 방식이 다르므로 제품별 PoC가 필요하다.

## 목표

수집하고 싶은 정보는 다음이다.

- 세션, turn, trace 단위 식별자
- 사용자 prompt와 assistant output의 정책 기반 기록 여부
- 모델명, provider, 요청 endpoint 분류
- input/output/cached/reasoning token
- latency, duration, retry, timeout
- tool call 이름, 인자, 결과 요약, 실패 사유
- shell command, exit code, sandbox, approval mode
- context compaction 발생 시점과 전후 token 변화
- permission request/denied 이벤트
- user, team, repo, project, cost center attribution

## 비목표

다음은 1차 설계에서 제외한다.

- TLS interception, system proxy, packet capture 같은 네트워크 가로채기
- 민감한 원문 prompt/output을 무조건 중앙 저장하는 방식
- agent 제품의 비공개 내부 API에 강하게 의존하는 방식
- 특정 외부 observability backend에 종속되는 구조
- 모델 gateway와 observability collector를 처음부터 하나의 시스템으로 묶는 구조

## v0.6 현재 아키텍처

아래 그림은 migration baseline인 현재 JavaScript 구현의 논리 구조다. 목표 책임 분리와
Rust-TypeScript 경계는 [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)를 기준으로 한다.

```text
┌─────────────────────────────────────────────┐
│ Coding agent                                 │
│ - Codex                                      │
│ - Claude Code                                │
└──────────────────────┬──────────────────────┘
                       │
                       │ hook / notify / transcript
                       v
┌─────────────────────────────────────────────┐
│ Local adapter                                │
│ - raw log reader                             │
│ - event correlator                           │
│ - span/event normalizer                      │
└──────────────────────┬──────────────────────┘
                       │
                       │ agent_observability.v1 records
                       v
┌─────────────────────────────────────────────┐
│ Shared event-log boundary                    │
│ - schema validation                          │
│ - pattern/content redaction                  │
│ - append-only writer                         │
└──────────────────────┬──────────────────────┘
                       │
                       │ sanitized JSONL
                       v
┌─────────────────────────────────────────────┐
│ Local event log                              │
│ - sessions                                   │
│ - turns                                      │
│ - spans                                      │
│ - metrics                                    │
│ - redaction metadata                         │
└──────────────────────┬──────────────────────┘
                       │
                       │ static report renderer
                       v
┌─────────────────────────────────────────────┐
│ Local report artifact                        │
│ - static HTML report                         │
│ - embedded summary data                      │
│ - no runtime server                          │
└─────────────────────────────────────────────┘
```

Future TODO - optional central path:

```text
Domain/application state
        |
        v
Strict team projector
        |
        v
TeamIngestEnvelopeV1 -> bounded retry queue -> Internal collector
                                                |
                                                v
                              Central trace/metrics/audit storage, alerts, exports
```

## 목표 통합 수집 컨셉

사용자가 Codex, Claude Code, Cursor, 다른 CLI agent를 동시에 쓰더라도 관측 시스템은
agent별 화면을 따로 만드는 방식으로 시작하지 않는다. 각 agent 옆에 local adapter를 두고,
adapter가 서로 다른 hook/transcript/native telemetry를 같은 내부 event/span schema로
정규화한다. 정적 report renderer와 선택적 central collector는 agent별 세부 파싱을 하지 않고,
이미 정규화된 데이터를 같은 query model로 다룬다.

Team alpha의 필수 adapter family는 Codex, Claude와 Cursor다. 세 adapter는 같은 Rust core,
privacy projector, queue와 report contract를 공유하고 제품별 hook/session/transcript/native
telemetry 차이만 inbound adapter에서 처리한다. 각 adapter installation은 별도 heartbeat를 보내
online/idle, last seen, sync/queue 상태와 제공 가능한 capability를 표시한다. Heartbeat는 usage
event가 아니며 실패분을 queue에 쌓아 재생하지 않는다.
세 adapter 모두 session/turn, LLM token usage, tool lifecycle, timestamp/status, privacy sentinel과
offline recovery canonical fixture를 통과해야 하며 필수 signal이 없는 제품/version은 지원 완료로
표시하지 않는다.

핵심 흐름:

```text
Agent A adapter ─┐
Agent B adapter ─┼─> domain/application state
Agent C adapter ─┘              `-> atomic local state + outbox
                                     |-> DurableRecordVx -> local events.jsonl -> static HTML report
                                     `-> TeamIngestEnvelopeV1 -> retry queue -> optional collector
```

이렇게 하면 agent가 몇 개로 늘어나도 뒤쪽 시스템은 다음 질문을 같은 방식으로 답할 수 있다.

- 특정 repo에서 오늘 어떤 agent가 token을 많이 썼는가
- 한 세션 안에서 LLM 호출과 tool 실행 시간이 어디에 몰렸는가
- permission denied, timeout, retry, compaction이 어떤 turn에서 발생했는가
- 같은 작업을 여러 agent가 병렬로 처리했을 때 비용과 실패율이 어떻게 달라졌는가
- content logging off 상태에서도 원문 없이 비용, latency, error를 볼 수 있는가

## OpenTelemetry-compatible 전송 (Future TODO)

adapter가 생성하는 내부 event/span 의미는 OpenTelemetry의 trace/span/event/metric 모델과
호환되게 잡는다. 1차 PoC에서는 local durable schema를 JSONL에 저장하고 정적 HTML report로
렌더링한다. 중앙 전송은 local event schema를 직접 보내지 않는다. strict team projector가 만든
`TeamIngestEnvelopeV1`의 allowlisted observation field만 OTel-shaped internal JSON, 실제 OTLP
export payload 또는 internal REST payload로 매핑해 adapter와 collector 사이의 결합을 낮춘다.

G0 승인을 위한 V1 proposal은 하나의 HTTPS JSON batch contract
`POST /api/team/v1/ingest/batches`다. G0 decision record가 승인된 뒤에만 normative contract가
된다. OpenTelemetry compatibility는 내부 의미와 후속
export adapter의 기준이며 V1 forwarder가 여러 transport를 선택하게 만들지 않는다. 다른
transport는 실제 interoperability 요구와 별도 contract fixture가 생긴 뒤 추가한다.

권장 span 계층:

```text
Workstream span
  Agent session span
    Turn span
      LLM request span
      Tool execution span
      Tool execution span
      Approval event
      Compaction event
      Redaction event
```

`Workstream span`은 같은 사용자, repo, task label, 시간 범위로 묶이는 논리 그룹이다.
서로 독립적인 agent 실행을 무리하게 하나의 trace로 합치지는 않는다. 대신
`workstream.id`, `repo.name`, `task.label`, `user.id` 같은 correlation key로 report에서
같이 조회할 수 있게 한다.

현재 v0.6.1 durable record의 축약 예시:

```json
{
  "schema_version": "agent_observability.v1",
  "record_type": "span",
  "trace_id": "trace_...",
  "span_id": "span_...",
  "parent_span_id": null,
  "span_kind": "agent.session",
  "name": "Codex session",
  "start_time_unix_ms": 1783296000000,
  "end_time_unix_ms": 1783296012345,
  "status": { "code": "ok" },
  "agent": {
    "name": "codex",
    "model": "model-id"
  },
  "project": { "name": "agent-observability" },
  "attributes": {
    "session_id": "session_..."
  },
  "metrics": {
    "input_tokens": 1200,
    "output_tokens": 480,
    "duration_ms": 12345
  },
  "content": {},
  "redaction": { "applied": false, "count": 0, "fields": [] }
}
```

이 schema의 현재 validator는 required field, enum, JSON value/plain-object 형태, 시간 범위와
top-level/nested metadata allowlist를 검사한다. parent/child topology 검증은 Rust reducer 단계의
blocker다. 목표 Rust 계약에서는
`SourceObservation`/domain state/`DurableRecordVx`/`ReportDtoVx`로 역할을 분리한다.

metrics는 span에서 파생하거나 adapter가 별도 전송한다.

- `agent.tokens.input`
- `agent.tokens.output`
- `agent.tokens.cached_input`
- `agent.tokens.reasoning_output`
- `agent.turn.duration_ms`
- `agent.tool.duration_ms`
- `agent.error.count`
- `agent.permission.denied.count`
- `agent.cost.estimated`

## 비용 추정

비용은 실제 청구액이 아니라 모델별 단가표를 적용한 예상치로 기록한다. 단가표는 report 생성
시점에 주입하거나 로컬 설정 파일에 둔다.

```text
estimated_cost = sum(exclusive_billable_unit[kind] * rate[kind])
```

이 식은 목표 pricing contract다. `exclusive_billable_unit`은 source별 token 의미를 해석한
뒤 서로 겹치지 않게 만든 과금 단위다. cached input이나 reasoning output이 total에 이미
포함된 source에서는 다시 더하지 않는다. 포함 관계를 판별할 수 없으면 비용을 완전한
예상치로 만들지 않는다. v0.6.1 rate table은 breakdown별 `token_semantics`를 요구하며 의미가
없으면 `ambiguous_token_semantics` incomplete 결과를 낸다.

주의할 점:

- provider billing API 또는 내부 gateway 없이 최종 청구액과 100% 일치한다고 주장하지 않는다.
- 실패 요청 과금 여부, retry, cache discount, 구독형/번들형 과금은 별도 보정값으로 다룬다.
- report에는 `estimated_cost`, `rate_table.version`, `cost.assumption`을 같이 남긴다.

rate table 예시:

```json
{
  "version": "local-rates-2026-07",
  "currency": "USD",
  "unit": "per_1m_tokens",
  "assumption": "Local static rates; not a billing statement.",
  "models": {
    "gpt-test": {
      "input_tokens": 2,
      "output_tokens": 8,
      "cached_input_tokens": 0.5,
      "reasoning_output_tokens": 10,
      "token_semantics": {
        "cached_input_tokens": "included_in_total",
        "reasoning_output_tokens": "included_in_total"
      }
    }
  }
}
```

## Local Adapter

로컬 adapter는 agent별 차이를 흡수하는 얇은 프로세스다.

목표 local artifact layout:

```text
~/.agent-observability/
  config.json
  logs/
  queue/
  state/
```

목표 설정 예시:

```json
{
  "enabled": true,
  "project_name": "example-project",
  "local_profile_label": "personal",
  "local_event_log": "~/.agent-observability/events.jsonl",
  "content_logging": {
    "prompts": false,
    "outputs": false,
    "tool_inputs": false,
    "tool_outputs": false
  },
  "redaction": {
    "enabled": true,
    "patterns": ["env", "token", "secret", "key", "password"]
  }
}
```

Standalone local label은 이메일이나 계정 식별자가 아니다. Future TODO team profile은 별도 설정에서
server-issued `identity_binding_ref`와 collector enrollment를 사용하며 raw email이나
`collector_endpoint`를 standalone 설정 shape에 섞지 않는다.

v0.6.1 legacy adapter 책임:

- hook payload와 transcript/session log를 turn 단위로 결합한다.
- token usage와 latency를 가능한 원천에서 읽는다.
- tool call은 parent turn 아래 child span으로 표현한다.
- content logging 정책과 redaction 정책을 durable write 전에 적용한다.
- 로컬 event log에 append-only로 기록한다.
- ordered source stream 안에서 stable `span_id` replay는 no-op으로 처리하고, 같은 ID의 다른
  payload는 conflict로 거부한다.

동일 파일에 대한 concurrent writer와 crash 원자성, out-of-order event 재정렬은 보장하지 않는다.
timestamp가 없는 동일 입력은 deterministic replay를 위해 `0`으로 정규화한다. 이 한계와 local
queue 재시도, 중앙 collector 전송은 현재 v0.6.1 adapter 기능이 아니며 transaction/reducer 또는
Future TODO 범위다.
중단되거나 잘린 source에서 완료 event가 없으면 synthetic 완료를 만들지 않고 open span의
`end_time_unix_ms: null`, `status.code: unset`을 유지한다. 입력 JSONL의 손상된 record는 조용히
건너뛰지 않고 read/parse error로 거부한다.

목표 구조에서는 위 책임을 inbound adapter, application reducer, privacy projector, outbound
writer로 분리한다. 새 Rust adapter는 source payload를 `SourceObservation`으로 번역하는
책임만 가지며 durable write를 직접 수행하지 않는다.

## Static HTML Report

1차 PoC의 조회 화면은 상시 실행형 웹 UI가 아니라 정적 HTML report로 시작한다. 미리 만든
템플릿에 수집 데이터를 주입해 self-contained `report.html`을 만들고, 사용자는 브라우저로
그 파일을 열어본다.

```text
Local adapter
        |
        v
~/.agent-observability/events.jsonl
        |
        | report renderer
        v
agent-observability-report.html
        |
        v
Browser file open
```

이 방식의 장점:

- 별도 web server, database server, background UI process가 필요 없다.
- 파일 하나로 공유하거나 archive할 수 있다.
- content logging off 정책과 redaction이 적용된 결과만 HTML에 들어간다.
- local-only PoC와 중앙 collector PoC를 분리할 수 있다.

목표 CLI command shape (현재 package에는 CLI entry point가 없음):

```text
agent-observability report \
  --input ~/.agent-observability/events.jsonl \
  --output ./agent-observability-report.html
```

`report.html`은 외부 network 요청 없이 동작한다. 브라우저의 `file://` 제약을 피하기 위해
JSONL을 따로 fetch하지 않고, 생성 시점에 필요한 데이터를 HTML 안에 주입한다.

현재 v0.6.1 report에서 확인 가능한 화면:

- repo/session/turn/text filter
- trace 목록과 parent ID를 포함한 평면 span table
- 현재 filter 결과의 token/cost/error KPI와 span별 latency/duration
- permission/compaction span의 평면 table 표시

목표 TypeScript UI에서 추가할 화면:

- model별 input/output/cached/reasoning token 집계와 model filter
- error, timeout, permission denied, compaction timeline
- redaction count와 content logging 상태

예상 비용과 cost aggregation은 rate table이 제공될 때 표시하고, 단가표나 모델 단가가
없으면 unknown/incomplete 상태로 표시한다.

## 목표 공통 데이터 모델

최소 trace 구조:

```text
Trace
  Session span
    Turn span
      LLM span
      Tool span
      Tool span
      Compaction event
      Error event
```

공통 attribute:

```text
agent.name
agent.version
session.id
turn.id
trace.id
user.id
team.id
repo.name
project.name
cwd
model.name
model.provider
sandbox.mode
approval.mode
duration.ms
token.input
token.output
token.cached_input
token.reasoning_output
tool.name
tool.arguments.redacted
tool.output.redacted
error.type
error.message
```

이 목록은 domain에서 구분할 수 있는 의미의 후보이며 모든 profile의 durable/transport/UI field
목록이 아니다. Team에서는 `cwd`, path, command/arguments/output, raw `error.message`와 content를
항상 제외하고 bounded reason code와 pseudonymous reference만 사용한다. Field별
`available`/`omitted_by_policy`/`unavailable_in_profile`/`unknown_source` 상태는
[team architecture](docs/TEAM_ARCHITECTURE.md)의 `ReportDtoV1` projection을 따른다.

현재 v0.6.1은 content logging이 꺼져 있으면 prompt/output/tool content를 omission marker로
대체하고 redaction count를 남긴다. size, hash, MIME type만 남기는 fallback은 목표
`DurableRecordVx` projector에서 검증할 정책이며 아직 구현되지 않았다.

## Codex Adapter

JavaScript v0.6.1 baseline은 notify hook과 session JSONL을 결합한다. Rust v0.9은
native telemetry의 model/API/token/tool signal을 primary로, `agent-turn-complete` notify의 turn
lifecycle만 supplement로 사용한다. API attempt와 completed response는 같은 request ID로 correlate하되
서로 다른 span으로 유지하고, usage는 completed response에만 둔다. 동일 canonical span의 재전달과
unsupported/content event는 private disposition ledger에서 cursor와 함께 원자 commit한다.

현재 Rust 흐름:

1. foreground receiver/shim이 OTel 또는 lifecycle supplement payload를 최대 1 MiB, 4096 record,
   record당 64 KiB인 private `codex_handoff.v1` 또는 `claude_handoff.v1` JSONL로 넘긴다.
2. adapter가 공식 session/prompt/request/tool tuple을 길이 구분 digest span ID로 만든다.
3. allowlisted model/token/duration/status/tool/decision만 `SourceObservation`으로 옮긴다.
4. prompt, assistant message, tool output, cwd와 임의 오류 문자열은 복사하지 않는다.
5. background `codex-ingest` 또는 `claude-code-ingest`가 observation과 fixed-code disposition을
   순서대로 commit하고 JSONL projection을 batch 마지막에 한 번 재생성한다.

현재 제한:

- 공식 문서는 대표 OTel event 의미를 설명하지만 실제 OTLP attribute key를 고정하지 않는다.
  따라서 handoff producer는 별도 versioned canonical mapping이 필요하며 미확인 key를 추측하지 않는다.
- native OTel HTTP/gRPC receiver와 foreground notify spool writer는 이 release에 포함되지 않는다.
- capability는 로컬 실행으로 검증한 Codex CLI `0.150.1`과 Claude Code `2.1.248`만
  experimental로 선언한다. 공식 문서의 이전 버전 필드 도입 시점은 실행 호환성 증거로 간주하지 않는다.

## Claude Code Adapter

JavaScript v0.6.1 baseline은 hook 이벤트와 transcript를 결합한다. Rust adapter는 공식 native
telemetry를 usage/tool/permission/compaction의 primary로 사용하고 hook을 session/turn lifecycle
supplement로만 사용한다. 내부 transcript 형식은 Rust contract dependency가 아니다.

주요 이벤트:

- session start
- user prompt submit
- API request
- tool result
- tool decision
- compaction
- stop / stop failure

예상 흐름:

1. `SessionStart`에서 session state와 allowlisted model을 초기화한다.
2. `claude_code.user_prompt`의 `prompt.id`를 turn correlation key로 사용한다.
3. `claude_code.api_request`에서 model, duration, input/output/cache token을 한 번만 기록한다.
4. `tool_result`와 `tool_decision`은 `tool_use_id`로 tool/permission observation을 구분한다.
5. compaction은 trigger, duration, pre/post token만 저장하고 error/summary는 버린다.
6. `Stop`은 completed, `StopFailure`는 failed lifecycle만 보완하며 usage를 다시 만들지 않는다.

현재 공식 hook은 사용자 interrupt를 별도 lifecycle event로 제공하지 않으므로 interrupted 상태를
`StopFailure`에서 추측하지 않는다. `prompt_id`가 없는 lifecycle 입력과 미등록 model은 고정 코드
diagnostic으로 격리한다.

동시 hook 입력은 daemon의 local transactional state에서 serialize한다. Hook마다 별도 state file을
만들거나 team network 완료를 기다리지 않는다.

## Cursor Adapter (Planned v0.11.0)

Cursor는 공식 hook surface의 conversation/generation identifier를 중심으로 trace를 구성한다. Local과
cloud execution에서 제공되는 hook coverage 차이는 capability manifest에 version별로 기록한다.

주요 이벤트:

- session start/end
- before submit prompt
- after agent response
- before shell execution
- after shell execution
- before MCP execution
- after MCP execution
- file edit/write

예상 흐름:

1. generation id를 trace/turn correlation key로 사용한다.
2. prompt 제출 전후와 agent response 이후 이벤트를 묶는다.
3. shell/MCP/tool 실행은 child span으로 분리한다.
4. IDE extension 특성상 workspace, file path, edit summary를 함께 기록한다.

## Native Telemetry Receiver (Future TODO)

일부 agent는 native telemetry export를 지원할 수 있다. 이 경우 별도 local receiver가
agent에서 내보내는 telemetry를 받아 내부 trace schema로 매핑한다.

receiver 책임:

- incoming telemetry endpoint 제공
- resource/service attribute 정규화
- token, model, tool call 관련 attribute 보강
- 내부 collector endpoint로 재전송

native telemetry가 있는 경우에도 hook/transcript adapter는 보완 수단으로 남긴다.
제품별 telemetry가 모든 tool detail과 content policy를 충분히 담지 못할 수 있기 때문이다.
같은 canonical field에는 primary source 하나만 지정해 supplement와 중복 집계하지 않는다. 상세
우선순위와 공식 근거는 [adapter compatibility contract](docs/ADAPTER_COMPATIBILITY.md)를 따른다.

## Internal LLM Gateway (Future TODO)

관측만으로 부족하고 모델 요청/응답 자체의 통제가 필요하면 내부 LLM gateway를 별도 PoC한다.

gateway가 담당할 수 있는 것:

- provider별 endpoint routing
- API key 중앙 관리
- request/response metadata 기록
- token/cost 계산
- policy enforcement
- fallback/retry

주의할 점:

- gateway는 agent 관측 adapter를 대체하지 않는다.
- tool call, local shell, permission, compaction 같은 정보는 gateway에서 보이지 않는다.
- Desktop App은 환경 변수나 config 상속 방식이 제품마다 달라 별도 검증이 필요하다.

## 보안과 개인정보

목표 정책:

- prompt/output 원문 저장은 opt-in이다.
- secret, token, key, password, cookie, Authorization header는 전송 전에 redaction한다.
- content logging과 fail-closed allowlist 정책은 local event log, local queue, static report,
  export, diagnostic, collector 전송 같은 모든 durable/external output 전에 적용한다.
- `.env`, private key, credential file, Terraform state 같은 파일 내용은 수집하지 않는다.
- shell output은 기본적으로 길이 제한과 redaction을 적용한다.
- 민감 repo는 project-level opt-out을 지원한다.
- collector ingest는 회전·폐기 가능한 source credential만 사용한다. Hosted query/control API는
  짧은 수명의 사용자 session과 workspace membership/role 기반 authorization을 요구한다.

redaction 단계:

1. path 기반 차단
2. key name 기반 차단
3. regex 기반 secret pattern 차단
4. 길이 제한
5. hash/size metadata만 남기는 fallback

현재 v0.6.1은 content omission, 민감 key/path, secret pattern 치환과 unknown metadata 거부까지
구현한다. 길이 제한, hash/size fallback, project opt-out, collector 인증은 이후 local release 또는
Future TODO의 검증 대상이다.

## 저장과 조회

1차 local-only PoC는 다음 논리 컴포넌트로 나눈다.

- local event log: turn/tool/span 구조 저장
- report renderer: token, latency, error count 집계
- static HTML report: 현재 repo/session/turn 조회, 목표 team/model 조회
- export artifact: 정적 HTML과 redacted JSON snapshot

중앙화가 필요해지면 선택적으로 internal collector를 추가한다.

- central trace store: turn/tool/span 구조 저장
- central metrics store: token, latency, error count 집계
- audit store: permission, policy, redaction event 저장
- alerting: error spike, cost spike, timeout, repeated denied permission 알림

## PoC 순서

버전별 릴리즈 범위와 완료 기준은 [ROADMAP.md](ROADMAP.md)를 기준으로 한다. 아래 1~3은
완료된 JavaScript baseline이고, 이후에는 contract freeze, Rust 수직 경로, TypeScript UI
순서로 진행한다.

1. Codex local adapter
   - notify payload 수집
   - session JSONL parsing
   - turn/tool span 생성
   - token/latency 표시

2. Static HTML report
   - self-contained HTML template
   - token/cost/latency/error summary
   - repo/session/turn별 trace viewer
   - trace tree와 token/latency/error summary

3. Claude Code adapter
   - hook registration
   - transcript parsing
   - tool duration 계산
   - permission/compaction event 기록

4. Contract freeze와 Rust 전환
   - source/durable/report golden fixture 고정
   - Rust core, durable I/O, agent adapter를 독립 CLI 경로로 구현
   - adapter별 end-to-end parity 후 release boundary에서 기본 경로 전환

5. TypeScript static report UI
   - versioned `ReportDtoVx` type 생성 또는 검증
   - self-contained HTML output과 브라우저 file-open smoke

Future TODO (버전 미확정):

- Commercial team profile ([architecture](docs/TEAM_ARCHITECTURE.md), [contracts](docs/TEAM_CONTRACTS.md))
  - principal-bound authentication, membership/RBAC, tenant isolation
  - multi-email identity/profile binding, Codex/Claude/Cursor adapter parity and heartbeat
  - configurable bounded cadence, local performance budgets and transactional source cursor/outbox
  - strict batch ingest, atomic dedupe/metering, encrypted bounded retry queue
  - append-only attribution correction and observation retraction
  - scoped query/report, members, policy, retention, quota, audit and export UI
  - deletion/key rotation/restore/load/fault-injection/SLO evidence before GA
- Optional gateway PoC
  - provider-compatible request routing
  - Desktop App 설정 상속 검증

## 성공 기준

- agent별 turn이 같은 trace schema로 조회된다.
- LLM span과 tool span의 parent/child 관계가 유지된다.
- token usage와 latency가 static HTML report에 표시된다.
- 예상 비용은 `estimated_cost`, `rate_table.version`, `cost.assumption`과 함께 표시되고,
  단가가 없거나 불완전하면 unknown/incomplete 상태로 표시된다.
- content logging off 상태에서 원문 prompt/output이 local event log, report, export에 남지 않는다.
- team retry queue와 collector에는 content logging 설정과 무관하게 원문 prompt/output이 들어가지 않는다.
- redaction 테스트 fixture가 모두 통과한다.
- target Rust adapter가 source cursor, stable event ID, local record와 optional outbox를 한 transaction으로
  기록하고 crash-point replay에서 유실과 중복 집계를 만들지 않는다.
- hook overhead와 idle/active CPU, RSS, disk growth가 declared local budget을 통과하고 pressure에서 team
  sync/report refresh가 foreground agent 작업보다 먼저 강등된다.
- Future TODO collector를 구현할 경우 accepted queue record는 at-least-once로 재시도하고,
  bounded queue 용량을 넘는 장기 장애는 silent loss가 아닌 degraded/drop 상태로 표시한다.
- Future TODO gateway/receiver를 구현할 경우 Desktop App/CLI/IDE extension별 설정 차이를
  문서화한다.

## 남은 검증 항목

- 각 agent의 최신 hook/transcript format 안정성
- token usage 누락 시 fallback 계산 방식
- long-running tool call과 interrupted turn 처리
- compaction 전후 context size 추정 방식
- team queue retention, disk budget과 source credential lifecycle
- 첫 hosted data region, legal deletion wording과 enterprise identity 후속 우선순위
- 민감 프로젝트 opt-out 정책

## 요약

AI coding agent observability는 network interception보다 agent-native surface를
조합하는 방식이 더 정확하고 안전하다. 우선 local adapter와 정적 HTML report를 만들고,
팀 단위 집계가 필요하면 internal collector를 추가한다. 필요한 경우 별도 LLM gateway를
붙인다. 핵심은 원문 수집을 최소화하고, turn/tool/token 관계를 안정적으로 복원하며,
모든 backend를 내부 통제 가능한 컴포넌트로 유지하는 것이다.
