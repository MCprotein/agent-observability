# Architecture and Engineering Principles

이 문서는 agent-observability의 목표 기술 스택, 책임 경계, 설계 원칙의 정본이다.
현재 릴리즈 상태와 작업 순서는 `ROADMAP.md`가 담당하고, 이 문서는 구현 방식과 의존성
규칙을 담당한다.

## Current and target stack

현재 `v0.11.0`은 Node.js 20+ ESM JavaScript 구현을 migration baseline으로
보존하면서 experimental Rust Codex, Claude Code와 Cursor adapter를 제공한다. Rust 경로는 closed contract,
deterministic lifecycle reduction, topology validation, pricing/report projection, bounded product handoff와
private embedded transaction을 구현한다. SQLite `local_state.v2`가 source cursor,
stable observation, current reduced record, adapter disposition과 profile-neutral delivery outcome의
정본이며 JSONL은 정본에서 재생성하는 projection이다. Team envelope, outbox와 network는 활성
계약이 아니다.

목표 스택은 다음과 같다.

| Area | Target | Responsibility |
| --- | --- | --- |
| Domain, application, adapters, storage, export, CLI, optional collector/query API | Rust | canonical schema, lifecycle reduction, privacy, cost, ingestion, local/team artifacts |
| Static report web UI | TypeScript | sanitized report DTO 조회, filtering, visualization, interaction |
| Rust-TypeScript boundary | Versioned JSON schema | generated or validated types, compatibility fixtures |

새 core/runtime 기능을 기존 JavaScript 구조에 계속 추가하지 않는다. 먼저 현재 동작을
contract fixture로 고정한 뒤 Rust CLI를 별도 실행 경로로 병렬 구현한다. Node.js와 Rust를
모듈 단위 FFI나 subprocess 호출로 섞지 않는다. 동일한 입력 fixture에서 durable record와
report DTO parity가 확인되면 완성된 수직 기능 단위를 release boundary에서 Rust CLI로
전환하고 대응하는 JavaScript 경로를 제거한다.

TypeScript UI는 브라우저에서 직접 원본 event log를 읽지 않고 Rust가 만든 sanitized report
DTO만 사용한다. report는 self-contained static HTML이며 runtime web server를 요구하지 않는다.

## Deployment profiles

제품은 하나의 core를 두 개의 독립된 composition root로 조립한다.

| Profile | Required runtime | Storage | UI |
| --- | --- | --- | --- |
| `standalone` | Rust CLI only | private embedded transactional state + JSONL/snapshot projections | embedded TypeScript asset + `ReportDtoVx`를 담은 self-contained HTML |
| `team` | local Rust CLI/forwarder + optional collector | same local state/outbox + tenant/workspace-scoped central store | 같은 TypeScript UI를 hosted report 또는 self-contained export로 제공 |

`standalone`은 기본이자 완전한 제품 경로다. login, network, collector, central database가 없어도
수집, 비용 추정, report 생성이 모두 동작해야 한다. team profile의 장애나 설정 부재가 local
write와 local report를 막아서는 안 된다.

`team`은 같은 domain 의미를 재사용하되 local durable contract와 전송 계약은 분리한다.

- 전용 team projector가 domain/application state에서 허용된 관측 필드만 골라
  `TeamIngestEnvelopeV1`을 만든다. `DurableRecordVx`를 입력이나 envelope payload로 사용하지
  않는다.
- local content logging opt-in과 무관하게 `SourceObservation`, `content`, 원문
  prompt/output/tool data, 로컬 파일 내용은 envelope에 들어갈 수 없다.
- envelope는 schema version, requested workspace reference, idempotency key, client timestamp와
  allowlisted observation fields만 담는다. authoritative source identity는 credential에서
  server-side로 결정한다.
- local projector가 전송 전에 privacy를 적용하고, collector는 schema/scope/privacy allowlist를
  다시 검증한다.
- collector는 인증 주체에서 tenant를 결정하고 requested workspace membership과 role을
  검증한다. actor attribution은 인증 주체와 server-side mapping으로 정하며 client claim을
  그대로 저장하지 않는다.
- persistence와 query는 항상 server-resolved tenant/workspace predicate를 포함한다. 다른
  tenant/workspace에 대한 ingest와 query는 deny하고 negative isolation fixture로 검증한다.
- collector는 authorization, tenant isolation, deduplication, retention, audit와 query를 소유한다.
- standalone과 team query는 모두 `ReportDtoVx`를 만들며 TypeScript UI가 집계 의미를 다시
  구현하지 않는다.
- collector 전송은 retry-safe outbound port다. 실패하면 bounded local queue에 남기되 local
  observability 경로는 계속 동작한다.
- source cursor, stable event identity, local record와 optional team outbox는 한 local transaction으로
  commit한다. JSONL/snapshot/HTML은 재생성 가능한 projection이며 pending delivery authority가 아니다.
  Local-only train은 profile-neutral delivery outcome port와 `not_applicable` state까지만 구현하며 team
  envelope/outbox/network adapter는 Future TODO G0 promotion 전에는 생성하거나 활성화하지 않는다.
- idempotency uniqueness scope는 server-resolved tenant/workspace + source ID + key다. 같은
  key의 동일 payload hash는 성공으로 재응답하고, 다른 hash는 conflict로 거부한다.
- server receipt time을 authoritative ingest time으로 기록하고 client time은 관측값으로만
  보존한다. tenant별 request/record 크기, ingest rate, storage/retention quota를 적용하며
  poison record는 재시도하지 않는 terminal diagnostic으로 분류한다.

team profile 구현은 roadmap의 Future TODO promotion gate를 통과한 뒤 시작한다. 하지만 domain,
identifier, privacy, schema evolution은 team 확장을 막지 않도록 지금부터 profile-neutral하게
설계한다. Team의 tenancy, identity, API, storage, retention, security, reliability와 commercial
readiness gate는 [TEAM_ARCHITECTURE.md](TEAM_ARCHITECTURE.md)를 정본으로 한다.

## Architectural style

기본 구조는 ports and adapters와 functional core / imperative shell의 조합이다.

v0.11.0의 Rust 경로는 `crates/domain`, `crates/contracts`, `crates/adapter-codex`,
`crates/adapter-claude-code`, `crates/adapter-cursor`,
`crates/application`, `crates/local-store`, `crates/cli`로 나뉜다. domain은 외부 형식을
모르고, contracts는 transient
source와 durable/report DTO 경계를 소유한다. application은 pricing과 report projection을,
inbound adapters는 제품별 source precedence/correlation/dedupe를, local-store는 SQLite transaction과 JSONL
projection을, CLI는 composition root를 소유한다.
`contracts/*.schema.json`은 closed wire contract이고 `contracts/contract-manifest.v1`은 현재
활성 schema path/version과 `team_ingest=disabled` 경계를 runtime 중립적으로 고정한다.

```text
Agent logs, hooks and native telemetry
        |
        v
Bounded local handoff + inbound adapters (Rust)
        |
        v
SourceObservation
        |
        v
Domain lifecycle state + application use cases
        |
        +--> SQLite transaction --> source cursor + stable event + current record
        |                              `--> DurableRecordVx --> JSONL / snapshot
        +--> fixed-code disposition --> source cursor + bounded diagnostic/suppression ledger
        +--> pricing + aggregation --> ReportDto projector --> TypeScript static UI
        +--> topology validation --> diagnostic projector --> diagnostics

Future TODO after promotion gate:
domain/application state --> TeamIngestEnvelopeV1 --> bounded outbox --> optional collector
```

경계 계약은 이름과 소유권을 분리한다.

- `SourceObservation`: inbound adapter가 만드는 비영속 입력. source payload의 의미를
  canonical field로 번역하지만 durable artifact로 직접 쓸 수 없다.
- domain lifecycle state: reducer가 소유하는 유효 상태와 transition. serialization 형식이 아니다.
- `DurableRecordVx`: local event log와 snapshot에 쓰는 versioned allowlist contract.
- `TeamIngestEnvelopeV1`: 첫 team 전송 전용 versioned allowlist contract. local
  `DurableRecordVx` 전체나 `content` field를 포함하지 않고 collector가 검증할 workspace
  reference, idempotency와 관측 필드만 포함한다. Authoritative source identity는 collector가
  credential에서 resolve해 stored record에 추가한다.
- `ReportDtoVx`: 가격과 집계를 적용한 뒤 UI에 전달하는 별도의 versioned allowlist contract.
- local transactional state: source generation/cursor, stable observation identity, canonical record,
  bounded adapter disposition과
  optional outbox를 원자적으로 소유하는 runtime authority. 외부 wire contract가 아니다.

재시작 후 source를 정확히 이어 읽기 위해 raw source cursor는 private SQLite control state에만
가역 보존한다. DB와 상위 directory는 각각 `0600`/`0700`을 강제한다. Source generation과 관측
identifier는 해시하며, raw cursor는 `DurableRecordVx`, JSONL, report, diagnostic 또는 전송 경계에
투영하지 않는다.

`DurableRecordVx`와 `ReportDtoVx`는 각각 명시적인 fail-closed projector를 통과한다. 한쪽의
privacy 검증을 다른 쪽이 암묵적으로 상속한다고 가정하지 않는다. canonical이라는 표현은
agent 간 공통 의미를 뜻하며, 위 네 계약을 하나의 범용 구조체로 합친다는 뜻이 아니다.

### Domain

Domain은 agent나 저장 방식에 종속되지 않는 의미를 소유한다.

- trace, session, turn, model request, tool operation, permission, compaction
- opaque identifiers와 명시적인 correlation fields
- token usage의 total과 breakdown 의미
- lifecycle state와 허용되는 transition
- topology와 canonical contract invariant

Domain은 파일 시스템, JSONL, HTML, CLI, 특정 agent payload, 단가표 파일 위치를 알지 않는다.
유효하지 않은 상태는 가능하면 타입으로 표현할 수 없게 만들고, 외부 입력 오류는 명시적인
`Result`로 반환한다.

### Application

Application은 use case와 순서를 소유한다.

- source event normalization 실행
- deterministic lifecycle reducer와 topology validation 순서
- storage port가 보장해야 하는 replay/idempotency와 atomic commit 계약 정의
- durable record와 report DTO의 독립된 privacy projection 요청
- pricing policy를 통한 예상 비용 계산
- report DTO 생성

Application은 구체적인 파일 writer나 agent parser를 직접 생성하지 않고 좁은 port에
의존한다. 한 use case가 parsing, storage, UI formatting을 동시에 소유하지 않는다.
SQLite adapter는 이 계약을 한 transaction으로 실행하고 JSONL materialization을 직렬화하지만,
lifecycle/topology 의미와 privacy projection 규칙을 자체 구현하지 않는다.

### Inbound adapters

Codex, Claude Code, Cursor adapter는 각 source format을 canonical observation으로 번역하는
anti-corruption layer다.

- source format 차이는 adapter 내부에서 끝난다.
- source format version과 unsupported event는 명시적인 diagnostic으로 남긴다.
- downstream consumer가 agent별 ID prefix를 파싱하지 않도록 `session_id`, `turn_id`,
  `operation_id` 등을 명시적으로 채운다.
- 원문 prompt, output, command, diff, file content를 canonical metadata로 가장하지 않는다.
- 같은 contract fixture suite를 모든 adapter에 적용한다.
- Codex의 `api_request`와 `sse_event(response.completed)`는 같은 request ID로 correlate하되
  transport attempt와 completed response를 별도 span으로 유지한다. usage는 completed response에만
  두며, 동일 canonical span의 재전달만 adapter에서 억제한다.
- unsupported, content-ignored와 duplicate-suppressed 입력도 raw payload 없이 fixed enum만
  `adapter_dispositions`에 기록하며 같은 transaction에서 cursor를 진행한다.
- hook path는 bounded local handoff만 수행하고 network, full transcript parse, report render나 queue drain을
  기다리지 않는다. File fallback은 persisted cursor와 source generation으로 incrementally reconcile한다.
- 제품별 공식 source 우선순위와 지원 evidence는
  [`ADAPTER_COMPATIBILITY.md`](ADAPTER_COMPATIBILITY.md)를 따른다.
- 현재 Rust adapter 입력은 private regular JSONL file로 제한하며 최대 1 MiB, 4096 record,
  record당 64 KiB다. group/other permission이나 symbolic link는 거부한다. 이 parser를 foreground
  hook에서 직접 실행하는 계약은 아니며 native receiver/spool writer는 별도 release gate다.

### Outbound infrastructure

파일 시스템, JSONL, snapshot, report artifact 같은 I/O를 구현한다.

- event log/snapshot write와 report artifact write는 각자의 fail-closed projector를 반드시
  통과한다.
- projector가 허용하지 않은 field는 저장하지 않으며 diagnostic 또는 validation error로
  처리한다.
- diagnostic, queue, export, collector payload도 같은 원칙의 전용 allowlist contract를
  사용하며 source의 원문 오류 문자열이나 unknown metadata를 그대로 전달하지 않는다.
- append/replay는 embedded transaction의 deterministic source observation key, stable event ID와 cursor로
  중복과 crash recovery를 통제한다.
- local artifact path의 상위 directory와 file은 각각 private permission `0700`과 `0600`이어야
  한다. writer는 느슨한 기존 directory를 임의 변경하지 않고 쓰기를 거부하므로 호출자는 전용
  artifact directory를 전달해야 한다.
  기존 v0.6 writer는 parity 대상이 되기 전에 이 조건을 충족해야 한다.
- 손상된 trailing record와 schema migration 정책을 명시적으로 처리한다.

### Web UI

Web UI는 TypeScript `strict` mode를 사용한다.

- UI는 집계 의미, privacy 판단, 가격 계산을 다시 구현하지 않는다.
- Rust가 생성한 sanitized report DTO만 입력으로 받는다.
- DTO type은 versioned schema에서 생성하거나 runtime validation으로 확인한다.
- 브라우저에서 외부 network 요청이나 상시 server 없이 동작한다.
- agent별 예외 처리는 UI가 아니라 canonical contract나 adapter에서 해결한다.
- Rust outbound infrastructure가 빌드된 TypeScript UI asset과 `ReportDtoVx`를 하나의
  self-contained HTML artifact로 조립한다.
- team profile의 hosted UI도 같은 `ReportDtoVx` schema와 UI component를 사용한다. transport와
  authentication/authorization만 profile별 composition root에서 달라진다. hosted query는
  server-resolved tenant/workspace scope 밖의 DTO를 생성할 수 없다.

Framework는 필요성이 확인될 때 선택한다. TypeScript 자체가 목표이며 특정 UI framework는
현재 architecture contract가 아니다. 사용자, route, component, interaction, accessibility와
responsive 계약은 [DESIGN.md](../DESIGN.md)를 따른다.

## Engineering principles

### SOLID

- Single Responsibility: module과 crate는 하나의 변경 이유를 가진다. 예를 들어 source
  parsing, lifecycle reduction, privacy projection, storage를 한 모듈에 섞지 않는다.
- Open/Closed: 새 agent는 canonical contract를 구현하는 adapter 추가로 수용한다. 기존
  report와 storage에 agent별 분기를 추가하지 않는다.
- Liskov Substitution: 같은 port의 구현은 동일한 error, ordering, idempotency 계약을
  지켜야 하며 contract test로 검증한다.
- Interface Segregation: reader, writer, pricing lookup, clock처럼 실제 소비자가 필요한 작은
  port를 사용한다. 모든 기능을 가진 거대한 context trait을 만들지 않는다.
- Dependency Inversion: application과 domain은 infrastructure에 의존하지 않는다. 구체 구현은
  composition root인 CLI에서 조립한다.

### Object-oriented techniques

Rust에서는 상속 중심 OOP를 사용하지 않는다. 상태와 invariant가 함께 움직여야 할 때 struct와
method를 사용하고, 교체 가능한 경계에는 trait을 사용한다. trait object, generic, builder는
실제 polymorphism이나 construction complexity가 있을 때만 도입한다.

### Functional techniques

Parsing 이후의 normalization, reducer, privacy projection, cost calculation, report projection은
입력과 출력을 명확히 가진 순수 함수로 유지하는 것을 기본값으로 한다. mutation은 reducer
내부처럼 범위가 좁고 성능 또는 상태 전이가 명확한 곳에 제한한다. 시간, 파일 시스템, 환경
변수는 주입 가능한 경계로 다룬다.

### Patterns

기본적으로 허용되는 패턴은 다음과 같다.

- ports and adapters for external formats and I/O
- anti-corruption layer for agent-specific payloads
- reducer/state machine for lifecycle correlation
- strategy/policy for provider and model pricing rules
- repository only when persistence substitution or transaction semantics are required
- composition root in the CLI

패턴 이름을 맞추기 위한 클래스나 trait은 만들지 않는다. 두 번째 구현, 독립 테스트 대역,
실제 변경 축 중 하나도 없다면 concrete function이나 struct를 우선한다.

## Model and pricing compatibility

모델별 조건을 domain 곳곳의 분기로 하드코딩하지 않는다. canonical usage는 최소한 다음 의미를
구분할 수 있어야 한다.

- total input tokens
- cached input tokens
- cache-write input tokens
- total output tokens
- reasoning output tokens
- provider-specific billable units or modifiers

breakdown이 total에 포함되는지 여부는 source adapter와 pricing policy 계약에 명시한다.
v1 parity contract의 pricing identity는 globally qualified canonical model ID와 rate-table version을
사용한다. provider나 service tier에 따라 가격이 달라지는 경우에는 해당 차원을 canonical model
ID/rate key에 포함해야 하며, 구분할 source evidence가 없으면 `estimated`를 반환하지 않는다.
향후 schema가 provider와 service tier를 독립 field로 추가하면 migration fixture로 이 의미를
보존한다. alias와 snapshot은 구분하며, 장문 context 배율이나 cache-write 배율은 versioned pricing
policy로 표현한다. 수집된 billable dimension에 대응하는 규칙이 없으면 결과는 `estimated`가 아니라
`incomplete` 또는 `unknown`이어야 한다.

모델 호환성은 모델명을 인식하는 것만 뜻하지 않는다. fixture는 token semantics, inherited model
identity, unknown model, alias, snapshot, cache breakdown, pricing modifier를 검증해야 한다.

## Maintainability and extensibility gates

- canonical schema 변경에는 migration note와 이전 schema fixture가 필요하다.
- adapter 추가에는 공통 parity suite와 unsupported-event fixture가 필요하다.
- privacy 변경에는 raw sentinel이 log, report, snapshot 어디에도 남지 않는 fixture가 필요하다.
- lifecycle 변경에는 out-of-order, duplicate, interrupted, replay fixture가 필요하다.
- pricing 변경에는 total/breakdown 중복 계산 방지와 incomplete 상태 fixture가 필요하다.
- Rust-TypeScript contract 변경에는 schema compatibility와 static report smoke가 필요하다.
- local runtime 변경에는 hook latency, idle/active CPU, RSS, disk growth, crash-point replay와 queue pressure
  fixture가 필요하다. Budget 초과 시 team sync와 projection을 먼저 늦추고 agent foreground path를
  block하지 않아야 한다.
- architecture boundary 변경은 간단한 ADR 또는 이 문서의 decision section으로 근거를 남긴다.

## Migration strategy

1. JavaScript v0.6 fixture를 source input, durable record, report DTO의 golden contract로 고정한다.
2. Rust workspace와 CLI composition root를 만들고 JavaScript 경로와 독립적으로 실행한다.
3. Rust domain/application, durable I/O, adapter를 작은 수직 결과 단위로 구현해 같은 fixture와
   privacy sentinel을 통과시킨다.
4. 한 release의 전체 command path가 parity를 통과하면 기본 CLI를 Rust로 전환한다.
5. 전환된 command path의 JavaScript 구현을 제거하되 fixture는 compatibility evidence로 남긴다.

중간 단계의 사용자 경로는 항상 완전한 JavaScript CLI 또는 완전한 Rust CLI 중 하나다. 두
runtime을 한 요청 안에서 연결하는 임시 production architecture는 만들지 않는다.

## Rejected defaults

- agent별 schema와 agent별 dashboard
- downstream consumer의 span ID 문자열 파싱
- 알려지지 않은 metadata를 그대로 저장하는 fail-open redaction
- 전역 mutable state, singleton registry, service locator
- parsing, business rule, storage, presentation을 한 service에 모으는 구조
- 미래 가능성만을 위한 trait, generic, factory, manager 계층
- 모델별 조건을 source code 곳곳에 직접 분기하는 방식
