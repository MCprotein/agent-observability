# Agent Observability

AI coding agent의 토큰 사용량, latency, tool call, error, context compaction,
handoff, approval, sandbox 상태를 관측하기 위한 내부 설계 정리.

이 문서는 사내 적용을 목표로 정리한 독자 아키텍처 초안이다.

## 현재 구현 상태

현재 `v0.3.0`은 local event log foundation과 Codex local adapter 위에 static HTML
report를 구현한다.

- `agent_observability.v1` span record schema
- parent/child span 관계 검증
- append-only JSONL writer
- durable write 전 content logging / secret / sensitive path redaction
- Codex session JSONL / notify payload 정규화
- Codex session, turn, LLM request, tool execution span 생성
- Codex token / latency metric capture
- self-contained static HTML report renderer
- session / repo / turn trace viewer
- token / latency / error summary
- Node test fixture

검증:

```bash
npm test
```

`v0.4.0`은 rate table 기반 예상 비용 필드를 추가한다.

## 결론

AI coding agent 관측은 OS/network proxy로 모델 요청을 몰래 가로채는 방식보다,
각 도구가 제공하는 hook, local transcript, session log, native telemetry, custom
endpoint 설정을 조합하는 방식이 안전하다.

```text
Codex / Claude Code / Cursor / other coding agents
        |
        | hooks, notify, local transcript, native telemetry
        v
Local adapter
        |
        | normalized events and spans
        v
Local event log (JSONL)
        |\
        | \ optional central sync
        |  v
        |  Internal collector, storage, cost tracking, alerts
        |
        | report renderer
        v
Static HTML report
```

핵심 원칙은 다음과 같다.

- agent별 공식 또는 사실상 공식 surface를 우선 사용한다.
- prompt, output, tool output은 기본적으로 redaction과 opt-in 정책을 거친다.
- 로컬 adapter는 원본 로그를 읽고, 우선 로컬 JSONL에는 정규화된 event/span만 남긴다.
- 여러 agent를 동시에 쓰더라도 뒤에서는 하나의 trace/span schema로 합친다.
- 중앙 collector는 1차 PoC의 필수 구성요소가 아니라 팀 단위 집계가 필요할 때 붙이는 선택 경로다.
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

## 전체 아키텍처

```text
┌─────────────────────────────────────────────┐
│ Coding agent                                 │
│ - Codex                                      │
│ - Claude Code                                │
│ - Cursor                                     │
│ - 기타 CLI/IDE 기반 agent                    │
└──────────────────────┬──────────────────────┘
                       │
                       │ hook / notify / transcript / telemetry
                       v
┌─────────────────────────────────────────────┐
│ Local adapter                                │
│ - raw log reader                             │
│ - event correlator                           │
│ - redaction policy                           │
│ - span/event normalizer                      │
│ - local append-only writer                   │
└──────────────────────┬──────────────────────┘
                       │
                       │ normalized JSONL
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

Optional central path:

```text
Local event log
        |
        | OTLP-compatible HTTP/gRPC or internal REST
        v
Internal collector
        |
        v
Central trace/metrics/audit storage, alerts, exports
```

## 통합 수집 컨셉

사용자가 Codex, Claude Code, Cursor, 다른 CLI agent를 동시에 쓰더라도 관측 시스템은
agent별 화면을 따로 만드는 방식으로 시작하지 않는다. 각 agent 옆에 local adapter를 두고,
adapter가 서로 다른 hook/transcript/native telemetry를 같은 내부 event/span schema로
정규화한다. 정적 report renderer와 선택적 central collector는 agent별 세부 파싱을 하지 않고,
이미 정규화된 데이터를 같은 query model로 다룬다.

핵심 흐름:

```text
Agent A adapter ─┐
Agent B adapter ─┼─> local events.jsonl ─> static HTML report
Agent C adapter ─┘             │
       same schema             └─> optional central collector
```

이렇게 하면 agent가 몇 개로 늘어나도 뒤쪽 시스템은 다음 질문을 같은 방식으로 답할 수 있다.

- 특정 repo에서 오늘 어떤 agent가 token을 많이 썼는가
- 한 세션 안에서 LLM 호출과 tool 실행 시간이 어디에 몰렸는가
- permission denied, timeout, retry, compaction이 어떤 turn에서 발생했는가
- 같은 작업을 여러 agent가 병렬로 처리했을 때 비용과 실패율이 어떻게 달라졌는가
- content logging off 상태에서도 원문 없이 비용, latency, error를 볼 수 있는가

## OpenTelemetry-compatible 전송

adapter가 생성하는 내부 event/span schema는 OpenTelemetry의 trace/span/event/metric 모델과
호환되게 잡는다. 1차 PoC에서는 이 schema를 로컬 JSONL에 저장하고 정적 HTML report로 렌더링한다.
중앙 collector를 붙일 때는 같은 schema를 OTel-shaped internal JSON이나 실제 OTLP export
payload로 매핑해서 adapter와 backend 사이의 결합을 낮춘다.

선택적 중앙 전송 방식:

- 1차: OTel-shaped internal HTTP JSON endpoint
- 2차: OTLP/gRPC endpoint
- fallback: 같은 필드를 담는 internal REST endpoint

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

예시 envelope:

```json
{
  "schema_version": "agent_observability.v1",
  "trace_id": "trace_...",
  "span_id": "span_...",
  "parent_span_id": "span_...",
  "span_name": "agent.turn",
  "start_time": "2026-07-06T00:00:00.000Z",
  "end_time": "2026-07-06T00:00:12.345Z",
  "resource": {
    "service.name": "agent-observability-adapter",
    "agent.name": "codex",
    "agent.instance.id": "local-user-host-session",
    "repo.name": "agent-observability",
    "project.name": "agent-observability"
  },
  "attributes": {
    "workstream.id": "workstream_...",
    "session.id": "session_...",
    "turn.id": "turn_...",
    "model.name": "model-id",
    "sandbox.mode": "workspace-write",
    "approval.mode": "on-request",
    "token.input": 1200,
    "token.output": 480,
    "duration.ms": 12345
  },
  "events": [
    {
      "name": "redaction.applied",
      "attributes": {
        "redaction.count": 3,
        "content.prompts.stored": false,
        "content.outputs.stored": false
      }
    }
  ]
}
```

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
estimated_cost =
  token.input * rate.input
+ token.output * rate.output
+ token.cached_input * rate.cached_input
+ token.reasoning_output * rate.reasoning_output
```

주의할 점:

- provider billing API 또는 내부 gateway 없이 최종 청구액과 100% 일치한다고 주장하지 않는다.
- 실패 요청 과금 여부, retry, cache discount, 구독형/번들형 과금은 별도 보정값으로 다룬다.
- report에는 `estimated_cost`, `rate_table.version`, `cost.assumption`을 같이 남긴다.

## Local Adapter

로컬 adapter는 agent별 차이를 흡수하는 얇은 프로세스다.

권장 위치:

```text
~/.agent-observability/
  config.json
  logs/
  queue/
  state/
```

설정 예시:

```json
{
  "enabled": true,
  "project_name": "example-project",
  "user_id": "user@example.com",
  "local_event_log": "~/.agent-observability/events.jsonl",
  "collector_endpoint": null,
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

adapter 책임:

- hook payload와 transcript/session log를 turn 단위로 결합한다.
- token usage와 latency를 가능한 원천에서 읽는다.
- tool call은 parent LLM turn 아래 child span으로 표현한다.
- content logging 정책과 redaction 정책을 durable write 전에 적용한다.
- 로컬 event log에 append-only로 기록한다.
- 중앙 collector를 사용하는 경우 전송 실패 시 local queue에 저장하고 재시도한다.

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

권장 report 생성 방식:

```text
agent-observability report \
  --input ~/.agent-observability/events.jsonl \
  --output ./agent-observability-report.html
```

`report.html`은 외부 network 요청 없이 동작한다. 브라우저의 `file://` 제약을 피하기 위해
JSONL을 따로 fetch하지 않고, 생성 시점에 필요한 데이터를 HTML 안에 주입한다.

report에서 보여줄 1차 화면:

- session 목록과 각 session의 총 token, latency/duration
- turn별 LLM span과 tool span tree
- model별 input/output/cached/reasoning token 집계
- repo/session/turn별 trace 조회
- error, timeout, permission denied, compaction timeline
- redaction count와 content logging 상태

예상 비용과 cost aggregation은 rate table이 들어가는 v0.4.0 범위다.

## 공통 데이터 모델

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

content logging이 꺼져 있으면 `input.value`, `output.value`, `tool.output` 같은 원문
필드는 저장하지 않는다. 대신 size, hash, MIME type, redaction count 같은 메타데이터만
남긴다.

## Codex Adapter

Codex는 notify hook과 session JSONL을 결합하는 방식이 적합하다.

예상 흐름:

1. Codex turn completion 알림을 받는다.
2. payload에서 session/thread/turn 식별자를 얻는다.
3. 로컬 session JSONL에서 해당 turn 범위를 찾는다.
4. user prompt, assistant response, model, token usage, duration을 추출한다.
5. shell/function/apply_patch/web 같은 tool call을 child span으로 분리한다.
6. approval mode, sandbox mode, cwd, workspace 정보를 span attribute로 붙인다.

Codex에서 JSONL 기반 보강이 필요한 이유:

- notify payload만으로는 tool call과 token usage가 부족할 수 있다.
- session JSONL이 turn 재구성의 기준 원천 역할을 한다.
- lifecycle hook만으로는 일부 event가 누락될 수 있다.

## Claude Code Adapter

Claude Code는 hook 이벤트와 transcript를 결합하는 방식이 적합하다.

주요 이벤트:

- session start
- user prompt submit
- pre tool use
- post tool use
- stop
- permission request
- permission denied
- compaction
- session end

예상 흐름:

1. session start에서 session state를 초기화한다.
2. user prompt submit에서 trace id와 parent turn span을 만든다.
3. pre tool use에서 tool start time을 저장한다.
4. post tool use에서 tool input/output, duration, exit status를 기록한다.
5. stop에서 transcript를 읽어 assistant output, model, token usage를 보강한다.
6. LLM span과 tool spans를 로컬 event log에 기록하고, 설정된 경우 내부 collector로 동기화한다.

동시 hook 실행이 있을 수 있으므로 session별 state file과 lock이 필요하다.

## Cursor Adapter

Cursor는 hook과 generation id를 중심으로 trace를 구성한다.

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

## Native Telemetry Receiver

일부 agent는 native telemetry export를 지원할 수 있다. 이 경우 별도 local receiver가
agent에서 내보내는 telemetry를 받아 내부 trace schema로 매핑한다.

receiver 책임:

- incoming telemetry endpoint 제공
- resource/service attribute 정규화
- token, model, tool call 관련 attribute 보강
- 내부 collector endpoint로 재전송

native telemetry가 있는 경우에도 hook/transcript adapter는 보완 수단으로 남긴다.
제품별 telemetry가 모든 tool detail과 content policy를 충분히 담지 못할 수 있기 때문이다.

## Internal LLM Gateway

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

기본 정책:

- prompt/output 원문 저장은 opt-in이다.
- secret, token, key, password, cookie, Authorization header는 전송 전에 redaction한다.
- content logging과 redaction 정책은 local event log, local queue, static report, export,
  collector 전송 같은 모든 durable write 전에 적용한다.
- `.env`, private key, credential file, Terraform state 같은 파일 내용은 수집하지 않는다.
- shell output은 기본적으로 길이 제한과 redaction을 적용한다.
- 민감 repo는 project-level opt-out을 지원한다.
- collector는 mTLS 또는 사내 인증을 요구한다.

redaction 단계:

1. path 기반 차단
2. key name 기반 차단
3. regex 기반 secret pattern 차단
4. 길이 제한
5. hash/size metadata만 남기는 fallback

## 저장과 조회

1차 local-only PoC는 다음 논리 컴포넌트로 나눈다.

- local event log: turn/tool/span 구조 저장
- report renderer: token, latency, error count 집계
- static HTML report: session, repo, team, model별 조회
- export artifact: 정적 HTML과 필요 시 redacted JSON snapshot

중앙화가 필요해지면 선택적으로 internal collector를 추가한다.

- central trace store: turn/tool/span 구조 저장
- central metrics store: token, latency, error count 집계
- audit store: permission, policy, redaction event 저장
- alerting: error spike, cost spike, timeout, repeated denied permission 알림

## PoC 순서

버전별 릴리즈 범위와 완료 기준은 [ROADMAP.md](ROADMAP.md)를 기준으로 한다. 아래는
구현 순서의 개념 요약이다.

1. Codex local adapter
   - notify payload 수집
   - session JSONL parsing
   - turn/tool span 생성
   - token/latency 표시

2. Static HTML report
   - self-contained HTML template
   - token/latency/error summary
   - repo/session/turn별 trace viewer
   - error and permission timeline

3. Claude Code adapter
   - hook registration
   - transcript parsing
   - tool duration 계산
   - permission/compaction event 기록

4. Cursor adapter
   - generation id 기반 correlation
   - shell/MCP/tool span 생성
   - workspace/file edit metadata 수집

Future TODO (버전 미확정):

- Optional internal collector
  - auth
  - schema validation
  - redaction count 검증
  - retry-safe ingest
- Optional gateway PoC
  - provider-compatible request routing
  - Desktop App 설정 상속 검증

## 성공 기준

- agent별 turn이 같은 trace schema로 조회된다.
- LLM span과 tool span의 parent/child 관계가 유지된다.
- token usage와 latency가 static HTML report에 표시된다.
- 예상 비용은 `estimated_cost`, `rate_table.version`, `cost.assumption`과 함께 표시되고,
  단가가 없거나 불완전하면 unknown/incomplete 상태로 표시된다.
- content logging off 상태에서 원문 prompt/output이 report나 중앙 저장소에 남지 않는다.
- content logging off 상태에서 원문 prompt/output이 local event log, queue, export에도 남지 않는다.
- redaction 테스트 fixture가 모두 통과한다.
- adapter가 local event log에 유실 없이 기록한다.
- 중앙 collector를 사용하는 경우 장애 시 local queue로 유실 없이 재시도한다.
- Desktop App/CLI/IDE extension별 설정 차이가 문서화된다.

## 남은 검증 항목

- 각 agent의 최신 hook/transcript format 안정성
- token usage 누락 시 fallback 계산 방식
- long-running tool call과 interrupted turn 처리
- compaction 전후 context size 추정 방식
- local queue retention과 disk budget
- 사내 인증 체계와 optional collector 연동 방식
- 민감 프로젝트 opt-out 정책

## 요약

AI coding agent observability는 network interception보다 agent-native surface를
조합하는 방식이 더 정확하고 안전하다. 우선 local adapter와 정적 HTML report를 만들고,
팀 단위 집계가 필요하면 internal collector를 추가한다. 필요한 경우 별도 LLM gateway를
붙인다. 핵심은 원문 수집을 최소화하고, turn/tool/token 관계를 안정적으로 복원하며,
모든 backend를 내부 통제 가능한 컴포넌트로 유지하는 것이다.
