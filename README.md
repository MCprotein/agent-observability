# Agent Observability

AI coding agent의 토큰 사용량, latency, tool call, error, context compaction,
handoff, approval, sandbox 상태를 관측하기 위한 내부 설계 정리.

이 문서는 사내 적용을 목표로 정리한 독자 아키텍처 초안이다.

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
Internal collector
        |
        v
Storage, dashboard, cost tracking, alerts
```

핵심 원칙은 다음과 같다.

- agent별 공식 또는 사실상 공식 surface를 우선 사용한다.
- prompt, output, tool output은 기본적으로 redaction과 opt-in 정책을 거친다.
- 로컬 adapter는 원본 로그를 읽고, 내부 collector에는 정규화된 event/span만 보낸다.
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
│ - retry queue                                │
└──────────────────────┬──────────────────────┘
                       │
                       │ OTLP-compatible HTTP/gRPC or internal REST
                       v
┌─────────────────────────────────────────────┐
│ Internal collector                           │
│ - auth                                       │
│ - validation                                 │
│ - tenant/project routing                     │
│ - sampling                                   │
│ - enrichment                                 │
└──────────────────────┬──────────────────────┘
                       │
                       v
┌─────────────────────────────────────────────┐
│ Storage and UI                               │
│ - trace store                                │
│ - metrics store                              │
│ - dashboard                                  │
│ - alerting                                   │
│ - audit/export                               │
└─────────────────────────────────────────────┘
```

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
  "collector_endpoint": "https://collector.internal.example/v1/traces",
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
- 중앙 전송 실패 시 local queue에 저장하고 재시도한다.
- content logging 정책과 redaction 정책을 전송 전에 적용한다.

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
6. LLM span과 tool spans를 내부 collector로 전송한다.

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

backend는 다음 논리 컴포넌트로 나눈다.

- trace store: turn/tool/span 원본 구조 저장
- metrics store: token, latency, error count 집계
- audit store: permission, policy, redaction event 저장
- dashboard: session, repo, team, model별 조회
- alerting: error spike, cost spike, timeout, repeated denied permission 알림

처음에는 단일 내부 collector와 단일 저장소로 시작하고, traffic이 늘면 trace와 metrics 저장소를
분리한다.

## PoC 순서

1. Codex local adapter
   - notify payload 수집
   - session JSONL parsing
   - turn/tool span 생성
   - token/latency 표시

2. Claude Code adapter
   - hook registration
   - transcript parsing
   - tool duration 계산
   - permission/compaction event 기록

3. Cursor adapter
   - generation id 기반 correlation
   - shell/MCP/tool span 생성
   - workspace/file edit metadata 수집

4. Internal collector
   - auth
   - schema validation
   - redaction count 검증
   - retry-safe ingest

5. Dashboard
   - repo/session/turn별 trace viewer
   - token/cost/latency chart
   - error and permission timeline

6. Optional gateway PoC
   - OpenAI-compatible request routing
   - Anthropic-compatible request routing
   - Desktop App 설정 상속 검증

## 성공 기준

- agent별 turn이 같은 trace schema로 조회된다.
- LLM span과 tool span의 parent/child 관계가 유지된다.
- token usage와 latency가 dashboard에 표시된다.
- content logging off 상태에서 원문 prompt/output이 중앙 저장소에 남지 않는다.
- redaction 테스트 fixture가 모두 통과한다.
- adapter가 collector 장애 시 local queue로 유실 없이 재시도한다.
- Desktop App/CLI/IDE extension별 설정 차이가 문서화된다.

## 남은 검증 항목

- 각 agent의 최신 hook/transcript format 안정성
- token usage 누락 시 fallback 계산 방식
- long-running tool call과 interrupted turn 처리
- compaction 전후 context size 추정 방식
- local queue retention과 disk budget
- 사내 인증 체계와 collector 연동 방식
- 민감 프로젝트 opt-out 정책

## 요약

AI coding agent observability는 network interception보다 agent-native surface를
조합하는 방식이 더 정확하고 안전하다. 우선 local adapter와 internal collector를 만들고,
필요한 경우 별도 LLM gateway를 추가한다. 핵심은 원문 수집을 최소화하고, turn/tool/token
관계를 안정적으로 복원하며, 모든 backend를 내부 통제 가능한 컴포넌트로 유지하는 것이다.
