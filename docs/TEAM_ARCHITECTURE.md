# Team Architecture

Status: Draft; G0 blocked pending named business/legal/security approvals
Last updated: 2026-08-28

이 문서는 agent-observability `team` profile의 제품·보안·데이터·운영 계약 정본이다.
공통 domain과 dependency rule은 `docs/ARCHITECTURE.md`, UI 계약은 `DESIGN.md`, 구현 순서는
`ROADMAP.md`를 따른다. 이 문서는 아직 구현 완료를 의미하지 않으며 team 기능은 버전 미확정
`Future TODO`다.
Wire/API/DTO/state/evidence 상세 계약은 [TEAM_CONTRACTS.md](TEAM_CONTRACTS.md)를 따른다.

## 1. Product contract

Team profile은 여러 사용자의 안전하게 투영된 관측 metadata를 workspace 단위로 모아 조회,
비용 추정, 운영 분석, 감사와 정책 관리를 제공한다.

반드시 지킬 조건:

- standalone은 network, login, collector 없이 동일한 local 기능을 계속 제공한다.
- team 장애, 인증 만료, quota 초과는 local write와 static report를 막지 않는다.
- 중앙 시스템은 prompt, assistant output, tool input/output, 파일 내용, 전체
  `SourceObservation` 또는 `DurableRecordVx`를 받지 않는다.
- 여기서 중앙 시스템은 request URL/header/body, collector, queue/journal telemetry, service
  log/metric, diagnostic, crash artifact, authoritative/derived store, cache, backup, query와 export를
  모두 포함한다.
- commercial usage metering과 모델 비용 추정은 별개다. 예상 모델 비용을 청구액으로 취급하지
  않는다.
- G0 승인 대상인 첫 commercial deployment proposal은 hosted-only 논리적 multi-tenant service다.
  승인되면 각 tenant는 하나의 configured data region에 귀속되고 authoritative store는 그
  region의 여러 failure zone에 동기 복제한다. 물리적 tenant 전용 배포와 self-hosted 배포는
  초기 범위가 아니다.
- 첫 team GA의 usage ledger와 quota는 운영·용량 통제용이다. Customer billing과 invoice 생성은
  별도 commercial contract가 승인되기 전까지 비목표다.

초기 비목표:

- agent 요청을 중계하거나 차단하는 gateway/control plane
- 중앙에서 standalone local artifact를 원격 삭제하는 managed-device 기능
- raw content 검색, prompt 평가, source code 저장
- 처음부터 여러 database나 message broker를 필수로 두는 분산 architecture
- 사용자 정의 role language와 임의 query language

## 2. System context

```text
Local machine

Agent adapters -> domain/application state
                       |-> DurableRecordVx -> private local log/report
                       `-> strict team projector
                              -> TeamIngestEnvelopeV1
                              -> encrypted bounded retry queue
                              -> authenticated batch transport
                                           |
                                           v
Team service

Ingest API -> principal/scope resolver -> schema/privacy/quota validation
           -> authoritative record + dedupe + usage transaction
           -> transactional outbox -> aggregate worker -> report projection

Identity/control API -> tenant/workspace/membership/policy/audit
Query API -> scoped repository -> ReportDtoVx -> TypeScript hosted UI/export
```

Control plane은 identity, membership, policy, quota, key metadata와 audit를 소유한다. Data plane은
ingest, immutable observation, aggregate와 query를 소유한다. 첫 구현은 같은 Rust workspace와
database를 사용할 수 있지만 module, API, credential과 deployment mode를 분리한다.

## 3. Tenancy and ownership

### Resource hierarchy

```text
Tenant
  Workspace
    Project
      Repository reference
      Source instance
        Adapter installation
```

- `tenant`: 계약, 데이터 소유, 상업 과금, encryption key와 삭제의 최상위 경계다.
- `workspace`: membership, authorization, privacy policy, retention, quota와 query의 운영 경계다.
- `project`: owner, cost center, policy override와 report grouping 단위다.
- `repository reference`: 실제 local path가 아닌 server-issued ID와 redacted display metadata다.
- `source instance`: 등록된 machine/forwarder identity다. 사람 principal과 분리한다.
- `adapter installation`: 하나의 source에서 동작하는 Codex, Claude 또는 Cursor adapter identity다.

승인된 record의 tenant/workspace 귀속은 immutable하다. workspace 간 이동은 record update가
아니라 권한이 확인된 export/import 작업으로 취급한다. 모든 primary key, foreign key, cache key,
dedupe key와 query cursor에 resolved tenant/workspace scope를 포함한다.

Tenant scope는 client payload에서 가져오지 않는다. 인증된 principal에서 tenant를 결정하고,
payload의 requested workspace가 membership과 credential scope 안에 있는지만 검증한다.

## 4. Identity and authorization

### Principal types

- `human`: 짧은 수명의 표준 federation session을 사용하는 사용자
- `source_instance`: local forwarder용 device credential
- `operator`: tenant data plane과 분리된 platform operation principal

사람 identity, membership, source instance와 관측된 actor hint를 하나의 ID로 합치지 않는다.
관측된 actor는 server mapping이 확인될 때만 user attribution으로 승격하고, 그 외에는
pseudonymous source attribution으로 남긴다.

### Multi-email hybrid attribution

한 사람은 하나의 stable `human_principal_id`와 1개 이상의 `email_identity`를 가질 수 있다.
`email_identity`는 로그인 principal 자체가 아니라 verified email, verification issuer/state,
purpose category(`work`, `personal`, `client`, `project`, `other`)와 bounded display label을 가진
별도 PII record다. Workspace membership은 principal과 해당 tenant에서 사용할 verified
email identity의 결합에 귀속된다.

`identity_binding`은 `email_identity`, workspace/project scope, source instance, adapter installation과
선택적 purpose label을 연결한다. 같은 사람이 회사 이메일, 개인 이메일, 고객사 이메일을 서로 다른
binding으로 등록하고 이메일별 token/cost/latency를 조회할 수 있다. Tenant UI는 현재 tenant에서
사용된 identity만 보여 주며 principal의 다른 tenant identity나 전체 이메일 목록을 노출하지 않는다.
Identity 소유자만 자신의 verified email로 binding을 요청할 수 있다. Workspace admin은 허용
domain/purpose, project scope와 approval policy를 적용하고 binding을 승인하거나 거부할 수 있지만
다른 사람의 이메일을 대신 소유하거나 임의 귀속할 수 없다.

세션 attribution 선택 우선순위는 다음과 같다.

1. 실행 시 명시한 identity profile
2. 승인된 repository/project policy binding
3. 해당 adapter installation의 account binding
4. source의 default binding
5. 어느 것도 유일하지 않으면 human으로 추측하지 않고 source-only attribution

로컬 adapter는 도구의 credential file, OS username 또는 브라우저 session을 읽어 이메일을 추측하지
않는다. 사용자가 interactive enrollment에서 verified identity를 선택하면 로컬에는 opaque
`identity_binding_ref`만 저장한다. First GA에서 human-attributable source credential은 정확히 하나의
`source_principal_id`에 귀속되며 그 principal이 소유한 email binding만 사용할 수 있다. Ingest
collector는 binding owner가 credential principal과 같은지, 현재 membership과 project scope가
유효한지 다시 검증한다. Shared/unbound source는 signed per-session attribution grant가 별도 승인될
때까지 source-only이며 `identity_binding_ref`를 보낼 수 없다. Raw email은 ingest envelope, local retry
queue, ACK journal, event/audit payload와 idempotency key에 들어가지 않는다. 이메일 문자열은 중앙
identity directory에서 tenant-scoped encryption, access audit, retention/deletion 정책 아래 관리하며
authorized report projection 시 `identity_binding_ref`에 join한다.

공용 machine은 OS account별 principal-bound source daemon/credential을 사용한다. 하나의 credential을
여러 사람이 공유하는 source, 무인 자동화와 식별이 모호한 session은 사람 이메일에 강제 귀속하지
않고 source-only로 남긴다.

### Required adapter coverage

Team alpha의 필수 adapter family는 `codex`, `claude`, `cursor`다. 세 adapter는 같은 Rust domain과
projector를 사용하고 tool-specific parsing만 inbound boundary에 둔다. 구현 시점에 각 제품이
지원하는 hook, session/transcript 또는 native telemetry surface만 사용하며 credential store나
비공개 파일 형식을 계정 탐지 목적으로 scraping하지 않는다.

G1의 versioned adapter capability matrix는 adapter family, supported product/version range, source
surface와 아래 canonical scenario 결과를 고정한다.

공식 surface 우선순위, 중복 방지 규칙과 제품별 검증 근거는
[`ADAPTER_COMPATIBILITY.md`](ADAPTER_COMPATIBILITY.md)가 source of truth다. Capability manifest는
그 문서의 field별 primary/supplement source를 기계 검증 가능한 형태로 고정한다.

| Mandatory scenario | Minimum evidence per Codex/Claude/Cursor adapter |
| --- | --- |
| session and turn lifecycle | stable session/turn boundary, timestamp ordering and restart fixture |
| LLM usage | request lifecycle, model availability state, input/output/cache token semantics |
| tool lifecycle | parent relation, start/end or bounded duration, status/reason category |
| privacy | raw prompt/output/path/email/credential sentinel absent from every sink |
| resilience | duplicate input, truncated source, source upgrade, offline queue and isolated failure |
| attribution | explicit profile, default profile, ambiguous source-only and cross-principal denial |

필수 scenario를 source가 제공하지 못하면 `unknown_source`로 gate를 통과시키지 않고 해당
product/version의 G2 지원을 blocked로 둔다. 부가 field만 `unknown_source`를 허용한다. G2 alpha는 세
family 모두에 대해 capability matrix와 `docs/evidence/team/G2/adapters/manifest.yaml`을 생성하고
설치, identity profile 선택, local-only 수집, team sync, heartbeat, offline recovery fixture를
통과해야 한다. 제품 업데이트로 surface가 바뀌면 해당 adapter만 격리 실패하고 다른 adapter와
authoritative local capture와 report는 계속 동작해야 한다.

### Local capture scheduling and performance envelope

Event-driven hook과 native telemetry 수신에는 polling 주기를 두지 않는다. 설정 가능한 주기는
file/transcript reconciliation, background flush와 heartbeat에만 적용한다. Server policy는 아래 범위를
더 좁힐 수 있지만 client가 protocol bound를 넘게 만들 수 없다.

| Setting | Default | Allowed range | Behavior |
| --- | --- | --- | --- |
| `file_reconcile_interval_ms` | 5000 | 1000..60000 | unchanged source는 cursor 이후만 확인; idle 시 최대값까지 adaptive backoff |
| `flush_interval_ms` | 5000 | 1000..60000 | jittered background flush; queue pressure 시 interval보다 batch bound가 우선 |
| `max_batch_records` | 100 | 1..500 | server batch guardrail 이내 |
| `max_batch_bytes` | 524288 | 16384..2097152 | decoded payload 기준 |
| `active_heartbeat_interval_ms` | 60000 | 30000..300000 | server가 receipt에서 다음 최소 시점을 늘릴 수 있음 |
| `idle_heartbeat_interval_ms` | 300000 | 120000..900000 | activity가 없을 때 사용 |

모든 주기에는 bounded jitter를 적용해 여러 adapter의 동시 wake-up을 피한다. 한 OS user당 daemon은
하나만 동작하며 source별 worker 수, local receiver connection, flush concurrency는 bounded다. Hook
handler는 bounded payload를 local IPC/spool에 넘기는 일만 하고 network, transcript scan, report render,
queue drain을 기다리지 않는다.

Daemon singleton은 private runtime directory의 OS-held exclusive lock과 random boot nonce로 증명한다.
PID만으로 owner를 신뢰하지 않는다. 두 번째 process는 lock owner health를 확인하고 즉시 종료하며,
stale file은 OS lock이 실제 해제된 경우에만 교체한다. Concurrent launch, crash, PID reuse, corrupt lock
metadata와 sleep/wake를 process fixture로 검증한다.

Hook input은 최대 1 MiB까지만 읽고 allowlisted handoff는 64 KiB 이하로 제한한다. Raw payload를 임시
파일에 spill하지 않는다. Oversized input, full local channel 또는 unavailable daemon은 bounded local
diagnostic/drop counter를 남길 수 있는 경우 남기고 hook을 즉시 반환한다. Local channel capacity와
normalization worker 수는 fixed bound를 가지며 report는 명시적 요청 또는 debounced checkpoint에서만
생성하고 idle timer로 반복 렌더링하지 않는다.

Observational hook은 host가 지원하면 asynchronous mode로 설치한다. Capture를 위해 blocking decision
hook을 추가하지 않는다. Host가 synchronous command만 제공하는 surface에서는 IPC enqueue deadline을
10 ms, 전체 handler deadline을 50 ms로 두고 timeout/full/unavailable에서 success exit하여 coding agent를
계속 실행시킨다. Host별 event, async/fail-open mode, timeout과 fallback은 capability manifest에 고정하고
slow/absent/crashed daemon 및 full channel integration fixture를 통과해야 한다.

아래 값은 구현 완료 주장이 아니라 release 전에 실측해야 하는 planning budget이다. 기준 workload,
운영체제, hardware와 측정 명령을 evidence manifest에 함께 기록한다.

| Resource | Release target |
| --- | --- |
| foreground hook overhead | added wall time p95 <= 20 ms, p99 <= 50 ms |
| idle CPU | 15-minute average <= 0.5% of one logical core |
| active CPU | 15-minute average <= 2%; any 1-minute window <= 5% of one logical core |
| resident memory | p95 RSS <= 96 MiB with three active adapters |
| local disk | default total 1 GiB hard budget; no unbounded state, projection, crash or temp artifact |
| network | at most one ingest request in flight per daemon; heartbeat independent and non-queued |

성능 evidence는 같은 machine/workload에서 collection disabled baseline과 enabled run을 각각 최소 5회
측정하고 median run과 p95/p99 sample을 보존한다. Protocol은 60초 warm-up, idle 15분, active 15분,
10,000-event burst와 세 adapter 동시 schedule을 고정한다. 1초 CPU/RSS/network/disk sample, logical-core
normalization, filesystem type, OS/build, CPU/memory, power mode, source versions와 cold/warm cache를 manifest에
기록한다. Hook command startup, source-side telemetry exporter, local receiver, normalization과 durable commit
overhead를 포함하며 daemon process만 따로 재서 통과시키지 않는다. Large transcript, offline queue,
disk-low와 sleep/wake scenario는 별도 run으로 측정한다. Enabled burst의 attempted/enqueued/rejected/durable
count를 모두 기록하고, 명시적 fail-open rejection은 1% 이하여야 한다. Graceful fixture shutdown 뒤에는
enqueued count와 durable observation count가 같아야 하지만 foreground enqueue 자체는 durability를
보장하지 않는다.

Default total local storage budget은 1 GiB이고 user config range는 256 MiB..20 GiB다. Budget `B`에서 먼저
`max(32 MiB, floor(B / 8))`를 atomic-write/WAL headroom으로 예약한다. 나머지 `R`은 authoritative
record/state 40%, encrypted team outbox/ACK 50%, JSONL/report/export projection 8%, diagnostic/crash/temp 2%로
MiB block 단위 내림 배정하고 residual block은 headroom에 둔다. 각 minimum은 80/96/16/4 MiB이며 schema
validation이 이를 만족하지 못하는 값을 거부한다. Team disabled에서는 team partition을 state/projection이
빌릴 수 있지만 headroom은 빌릴 수 없고 total hard budget은 항상 유지한다.

Accounting은 logical file size가 아니라 allocated filesystem block을 사용하며 embedded-store WAL,
sidecar/index, queue journal, report/export, crash file, temp와 atomic replacement의 old/new copy를 모두
포함한다. 새 write의 worst-case block reservation이 headroom 안에 없으면 write 전에 admission을 거부한다.
Temp artifact는 atomic rename 후 제거하고 startup에서 orphan temp를 bounded scan/삭제한다. Projection은
먼저 재생성 대상으로 삭제하고, team outbox는 기존 queue policy를 따른다. Authoritative state가 retention
compaction 후에도 cap에 도달하면 새 observation을 저장하지 않고 `local_storage_blocked` counter를 남긴 뒤
hook을 성공 반환한다. 기존 record를 silent overwrite하거나 coding agent를 block하지 않는다.

Pressure 감지는 CPU/RSS, queue watermark, disk free space, repeated source parse time과 flush latency의 bounded
bucket만 사용하며 raw path/content를 diagnostic label로 쓰지 않는다. Budget 초과 시 report refresh와
enrichment를 먼저 늦추고, reconciliation backoff와 batch coalescing을 늘리고, team projection/flush를
중단한다. 최소 local observation transaction은 가능한 동안 유지하되 hard disk limit에서는 새 team
envelope를 명시적으로 거부하고 `degraded` 상태와 bounded drop counter를 남긴다. 어떤 단계도 coding
agent의 hook 완료를 network availability나 queue drain에 묶지 않는다.

Load shedding state는 `normal -> pressured -> protected -> probe`다. 두 연속 10초 window에서 하나의
resource budget을 넘으면 `pressured`, 60초 지속 또는 disk/queue 90%면 `protected`로 전환한다.
`pressured`는 report debounce와 reconciliation interval을 2배로 늘리고, `protected`는 team projection과
flush를 최대 60초 pause하고 source별 round-robin으로 최소 한 reconcile slot을 보장한다. 이후 5초 probe를
수행하며 세 연속 10초 window가 70% 미만이면 한 단계씩 복구한다. Probe 실패는 exponential backoff하되
최대 60초마다 한 번은 recovery probe를 수행한다. Oscillation, sustained pressure, fairness와 recovery를
deterministic clock fixture로 검증한다.

### Local transactional state and outbox

Target Rust runtime의 authoritative local boundary는 embedded transactional store다. 각 source 입력은
하나의 transaction에서 다음 순서로 처리한다.

1. `(adapter family, source generation fingerprint, source cursor)`와 existing observation key를 읽는다.
2. deterministic observation key에 대응하는 `event_id`를 get-or-create한다.
3. allowlisted canonical durable record와 team profile delivery outcome을 기록한다. Team-enabled record는
   정확히 하나의 `pending`, `acknowledged`, `permanent_reject`, `dropped` outcome을 가지며 admission이
   허용되면 `pending` outbox envelope를 함께 기록한다. Queue hard limit이면 bounded reason/time range를
   가진 `dropped` outcome을 기록하되 local record는 보존한다.
4. 같은 transaction에서 source cursor를 advance한다.

프로세스가 어느 단계에서 종료되어도 cursor만 앞서가거나 outbox만 사라질 수 없다. 재시작 후 같은
source observation은 같은 key와 `event_id`로 수렴한다. Source generation fingerprint와 raw path/cursor는
local state에만 있고 team envelope나 telemetry label로 전송하지 않는다. JSONL, snapshot과 static HTML은
이 state에서 재생성 가능한 append-only export/projection이며 source cursor나 pending delivery의 authority가
아니다. Team ACK는 outbox row의 accepted/duplicate 상태와 recovery journal을 원자 갱신한 뒤 반영한다.

### Credential lifecycle

- Human access token TTL은 최대 15분이며 refresh, logout, global revoke를 지원한다.
- Source instance는 one-time enrollment로 등록하며 secret은 최초 한 번만 표시한다.
- Human-attributable source는 one principal에 귀속한다. Shared/unbound source credential은 email
  identity attribution capability를 갖지 않는다.
- 각 adapter installation은 source 아래 별도 credential을 받고 credential claim에 server-issued
  `adapter_installation_id`, fixed `agent_kind`와 current `source_epoch`를 포함한다.
- Device credential은 workspace와 ingest capability에만 묶고 query/admin 권한을 주지 않는다.
- Rotation은 이전 credential과 새 credential의 짧은 overlap window를 허용한다.
- Membership과 source credential은 versioned `authorization_epoch`를 가진다. API는 token claim과
  current epoch를 비교하고 authorization cache TTL을 최대 30초로 제한한다. 탈퇴, role 변경,
  workspace disable, tenant suspend와 source revoke는 30초 안에 반영되어야 한다.
- Control plane에서 current epoch를 확인할 수 없고 cache TTL이 지났으면 team operation은
  fail closed한다. Standalone local operation은 계속한다.
- Local secret은 OS credential store를 우선 사용하고 config/log/diagnostic에 출력하지 않는다.
- General service account는 first GA에서 지원하지 않는다. Automation identity는 별도 capability와
  credential lifecycle이 승인된 후 추가하며 human/source credential을 재사용하지 않는다.

### Roles

| Capability | Owner | Admin | Analyst | Contributor | Auditor | Billing | Ingest source |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Tenant lifecycle | allow | deny | deny | deny | deny | deny | deny |
| Membership and policy | allow | allow | deny | deny | read | deny | deny |
| Reports and traces | allow | allow | allow | scoped | read | cost only | deny |
| Export | allow | allow | allow | scoped | deny | cost only | deny |
| Audit | allow | allow | deny | deny | read | deny | deny |
| Retention and quota | allow | allow | read | deny | read | read | deny |
| Usage and billing | allow | read | read | own scope | read | allow | deny |
| Identity email PII | allow | allow | deny | own identity only | deny | deny | deny |
| Attribution correction | allow | allow | deny | own scope | read | deny | deny |
| Observation retraction | allow | allow | deny | own scope | read | deny | deny |
| Ingest | deny | deny | deny | deny | deny | deny | allow |

Authorization은 deny-by-default다. UI visibility는 편의 기능일 뿐이며 모든 API와 repository가
server-side authorization을 다시 수행한다. Platform operator의 break-glass 접근은 일반
tenant session과 분리하고 이유, 승인, 시간 제한과 모든 접근을 audit에 남긴다.

`scoped`는 explicit project membership의 교집합, `own scope`는 server-mapped human actor의
project membership 교집합, `cost only`는 span detail을 제외한 aggregate cost DTO만 뜻한다.
Raw email report/export field는 별도 `identity:read_pii` capability가 있어야 하며 Owner/Admin에만
기본 부여한다. 다른 role은 opaque identity ref, purpose와 pseudonymous label만 받고 모든 사람은
별도 own-identity API에서 자신의 email만 볼 수 있다.
Explicit deny, tenant/workspace suspension, legal hold restriction이 allow보다 우선한다. Scope는
token에 완전한 권한 목록으로 고정하지 않고 API authorization 시점에 current policy version으로
결정한다.

### API authorization matrix

| API family | Required capability | Allowed principals | Resource rule |
| --- | --- | --- | --- |
| ingest batch | `ingest:write` | source instance | credential-bound workspace only |
| report/trace query | `report:read` | owner/admin/analyst/auditor/contributor | workspace plus explicit project scope |
| cost aggregate/export | `cost:read` | owner/admin/analyst/billing; scoped contributor | billing gets aggregate-only DTO |
| identity email field/query/export | `identity:read_pii` | owner/admin; human self endpoint | field-level projection after report/export authorization |
| observation attribution correction | `observation:correct` | owner/admin; scoped contributor for own binding | same tenant/workspace, current binding and expected revision required |
| observation retraction | `observation:retract` | owner/admin; scoped contributor for own observation | active report exclusion only; privacy deletion is separate |
| member/policy mutation | `workspace:admin` | owner/admin | cannot grant capability caller lacks |
| source enroll/rotate/revoke | `source:admin` | owner/admin | workspace-bound source only |
| audit query | `audit:read` | owner/admin/auditor | workspace scope; platform audit excluded |
| retention/quota mutation | `data:admin` | owner/admin | legal hold and plan bounds override request |
| legal hold create/release | `legal_hold:admin` | owner plus approved legal/security delegate | tenant lock and dual audit required |
| tenant deletion | `tenant:delete` | owner | recent re-authentication plus second confirmation |
| break-glass tenant access | `platform:break_glass` | operator | approved incident, tenant, reason and expiry required |

Credential rotation preserves `source_instance_id`, so queued envelopes retain the same dedupe scope.
Compromise quarantine pauses replay; an admin must explicitly rotate or replace the source before queued data
can resume. Cross-scope request and resource-existence probes both return the same bounded denial shape.

## 5. Ingest contract

### Envelope

`TeamIngestEnvelopeV1`은 immutable, strict, bounded allowlist다. Unknown field와 arbitrary map을
거부한다. 문자열은 enum 또는 길이가 제한된 opaque identifier만 허용한다.

```json
{
  "schema_version": "team_ingest.v1",
  "event_id": "evt_opaque",
  "requested_workspace_id": "ws_opaque",
  "identity_binding_ref": "identity_binding_opaque",
  "client_time_unix_ms": 1783296000000,
  "observation": {
    "kind": "llm.request",
    "trace_id": "trace_opaque",
    "span_id": "span_opaque",
    "parent_span_id": "turn_opaque",
    "start_time_unix_ms": 1783296000000,
    "end_time_unix_ms": 1783296012000,
    "status": "ok",
    "agent_kind": "codex",
    "model_ref": "model_pseudonym",
    "project_ref": "project_opaque",
    "repository_ref": "repo_opaque",
    "metrics": {
      "input_tokens": 1200,
      "output_tokens": 480,
      "cached_input_tokens": 300,
      "duration_ms": 12000
    },
    "redaction": {
      "applied": true,
      "count": 2,
      "policy_version": "privacy.v3"
    }
  }
}
```

`content`, free-form attributes, path, command, prompt, output, error message, raw email과 local username은
schema에 존재하지 않는다. Model/project/repository 값은 bounded pseudonym 또는 server-issued
reference다. Collector는 source credential에서 source instance를 결정하고
`identity_binding_ref`를 server-side allowlist와 membership으로 검증한다.

V1 field contract:

| Field | Presence | Type/bound | Classification |
| --- | --- | --- | --- |
| `schema_version` | required | exact `team_ingest.v1` | public protocol |
| `event_id` | required | opaque ASCII, 1-96 chars | pseudonymous identifier |
| `requested_workspace_id` | required | opaque ASCII, 1-64 chars | server-validated reference |
| `identity_binding_ref` | optional, not null | server-issued opaque ASCII, 1-96 chars | server-validated email identity binding |
| `client_time_unix_ms` | required | non-negative integer | observation metadata |
| `observation.kind` | required | V1 enum: `session`, `turn`, `llm.request`, `tool.execution`, `permission`, `compaction`, `redaction` | bounded metadata |
| trace/span IDs | required; parent nullable | opaque ASCII, 1-96 chars | pseudonymous identifier |
| start/end time | required | non-negative integer; end >= start | observation metadata |
| `status` | required | `ok`, `error`, `cancelled`, `unknown` | bounded metadata |
| `agent_kind` | required | supported adapter enum, max 32 chars | bounded metadata |
| model/project/repository refs | optional, not null | opaque ASCII, 1-96 chars | pseudonymous reference |
| token/duration metrics | optional, not null | integer 0..2^53-1; max 16 named V1 fields | aggregate input |
| redaction object | required | boolean, count 0..2^31-1, policy ID 1-64 chars | privacy evidence |

Optional field absence means unavailable at source or in this profile. JSON `null` is allowed only for
`parent_span_id`; unknown field, duplicate object key, invalid UTF-8, non-integer number and non-finite value
are rejected before typed decoding. Client time more than 24 hours in the future is permanent reject;
historical replay up to the queue retention window is accepted and marked with server-observed skew.

### Batch API

- Endpoint contract: `POST /api/team/v1/ingest/batches`
- 한 batch는 최대 500 records, decoded 2 MiB, record당 16 KiB를 기본 guardrail로 한다.
- Request body is `{ "records": TeamIngestEnvelopeV1[] }`; the top-level object is strict.
- Syntactically valid batch returns HTTP 200 with record-level results. A malformed top-level request returns
  400, failed authentication 401, scope denial 403, decoded request overflow 413, and service-wide overload
  429/503. Record-level validation does not use 207.
- Response is `{ "request_id", "server_time_unix_ms", "results": [{ "event_id", "status",
  "reason_code", "retryable", "retry_after_ms", "commit_seq" }] }`. Only accepted/duplicate results have
  a `commit_seq`; only retryable results may have `retry_after_ms`.
- ACK는 record, dedupe ledger, accepted usage meter와 aggregate outbox job이 하나의 transaction으로 durable해진 뒤
  반환한다. Aggregate projection은 비동기다.
- Partial success를 허용한다. Client는 accepted/duplicate record를 active queue에서 encrypted
  ACK recovery journal로 원자 이동하고 retryable reject만 active queue에서 재시도한다.

### Source and adapter heartbeat

Heartbeat는 usage span이나 billable observation이 아닌 별도 latest-state contract다. Source
adapter-scoped credential로 `PUT /api/team/v1/sources/self/heartbeat`를 호출하며 collector는 source,
tenant, `adapter_installation_id`, `agent_kind`와 current `source_epoch`를 credential에서 결정한다.

Heartbeat allowlist는 schema version, credential과 일치해야 하는 `agent_kind`, monotonic
`source_epoch`/`heartbeat_seq`, bounded agent/adapter version, observed time,
activity state(`active`, `idle`), sync state(`healthy`, `degraded`, `blocked`), queue depth/age bucket과
capability flags만 허용한다. Raw email, path, repository name, prompt, command, 오류 문자열과 임의
attribute는 금지한다.

- active cadence는 60초, idle cadence는 5분이며 bounded jitter를 적용한다.
- online 상태는 server receipt 기준으로 active는 3분, idle은 15분 후 offline으로 전환한다.
- 실패 heartbeat는 durable queue나 ACK journal에 넣거나 나중에 replay하지 않는다.
- server는 credential epoch가 current adapter epoch와 다르면 거부하고, 같은 epoch에서는
  `heartbeat_seq`가 저장값보다 큰 경우에만 상태를 갱신한다. stale, replay, cross-adapter와
  agent-kind mismatch는 bounded reject이며 receipt time보다 client time을 freshness authority로 쓰지 않는다.
- authoritative store는 adapter별 `last_seen_at`, accepted epoch/sequence와 현재 상태를 upsert하고 장기 분석은 bounded
  hourly availability aggregate만 사용한다.
- heartbeat quota는 ingest usage quota와 분리하고 customer usage/billing에 포함하지 않는다.
- heartbeat 장애나 identity ambiguity는 local collection과 static report를 막지 않는다.

### Idempotency

Unique scope는 `(resolved tenant, resolved workspace, source instance, event_id)`다.

Validation order is fixed: authenticate and resolve the current source credential, enforce transport bounds and
typed canonicalization, then query dedupe by unique scope. An identical committed hash returns its stored receipt
before current identity-binding, policy or quota validation; a different hash returns
`idempotency_conflict`. Mutable binding/policy/quota checks run only for first-seen events. Revoked/invalid source
credentials never reach dedupe. This preserves ACK-lost retry while preventing a revoked identity from
attributing new data.

- 같은 key + 같은 canonical payload hash: `duplicate` 성공
- 같은 key + 다른 hash: permanent `idempotency_conflict`
- accepted unique record만 usage meter에 반영
- V1 hash는 duplicate key가 거부된 typed envelope 전체를 canonical JSON으로 encode한 bytes의
  SHA-256이다. `schema_version`과 `policy_version`을 포함하며 transport compression, HTTP headers,
  server receipt fields는 제외한다. Canonicalization fixture는 key order, Unicode escape와 integer
  representation 차이가 같은 hash가 됨을 증명한다.
- Accepted event는 새 policy/schema로 재투영하지 않는다. Queue는 enqueue 당시 serialized envelope를
  보존한다. 의미가 다른 새 observation은 새 event ID를 사용한다.
- Dedupe retention default는 120일이며 maximum offline queue age와 supported schema replay window보다
  항상 길어야 한다.

### Correction and retraction

Accepted observation은 직접 수정하거나 삭제하지 않는다. 잘못된 email/profile 귀속은
`AttributionCorrectionV1`, 잘못 수집된 observation의 분석 제외는 `ObservationRetractionV1` append-only
revision으로 표현한다. UI의 일반적인 undo/redo가 아니라 감사 가능한 domain operation이다.

- 모든 operation은 unique `operation_id`, target `event_id`, `expected_revision`, bounded reason code와
  요청 actor를 가진다. 같은 operation/payload는 idempotent success, 다른 payload는 conflict다.
- Correction은 원본 usage 수치를 바꾸지 않고 current report의 identity attribution만 이전
  binding에서 빼고 검증된 새 binding에 반영한다. Cross-tenant/workspace 이동은 금지한다.
- `ObservationRetractionV1.action`은 `retract` 또는 `reinstate`다. Retract는 current aggregate/report/export에서 observation을 제외하지만 immutable accepted record와
  audit history는 보존한다. 개인정보 삭제와 key destruction은 별도 deletion workflow를 따른다.
- 동시 수정은 compare-and-set으로 하나만 성공한다. 되돌리기는 이전 revision을 삭제하는 것이 아니라
  반대 의미의 새 revision을 추가한다.
- Effective state는 identity attribution과 visibility(`included`/`retracted`)를 독립적으로 가진다.
  Attribution correction은 visible observation의 old/new identity delta만 만들고, retract는 current
  attribution contribution을 빼며, reinstate는 current attribution contribution을 다시 더한다. Aggregate
  worker는 revision과 observation을 같은 commit sequence 기준으로 처리한다. Replay 결과는 순서와 retry
  횟수에 무관하게 동일해야 한다.
- Contributor 권한은 accepted record의 original principal이 caller와 같고 destination binding도 같은
  principal 소유인 경우에만 허용한다. 이전 correction으로 current binding이 바뀌어도 권한이 이동하지
  않는다. Source-only/ambiguous observation과 다른 project scope는 Owner/Admin만 수정할 수 있다.
- Correction/retraction 접수는 원본 observation retention 기간 안에서만 가능하다. Revision과 aggregate
  effect는 aggregate retention 종료까지 보존하며 identity deletion 시 display PII와 binding join만 제거한다.

### Error taxonomy

| Condition | Retry | Client action |
| --- | --- | --- |
| malformed batch, unknown field | no | terminal diagnostic |
| expired session/credential | after refresh | refresh or re-enroll |
| revoked or unauthorized scope | no | pause source and require admin action |
| idempotency conflict | no | terminal diagnostic and security signal |
| record too large/privacy violation | no | terminal diagnostic |
| rate/admission quota | yes when instructed | honor `Retry-After`; keep bounded queue |
| server unavailable/timeout | yes | exponential backoff with jitter |

Clock skew는 client time을 수정하지 않고 server receipt time과 skew diagnostic으로 기록한다.
Ordering은 event ID와 explicit parent relation으로 복원하며 arrival order를 신뢰하지 않는다.

Contract source and planned evidence locations:

- `crates/contracts/schemas/team-ingest-v1.schema.json`: generated V1 schema artifact
- `crates/contracts/schemas/team-ingest-response-v1.schema.json`: batch response/error contract
- `crates/contracts/tests/fixtures/team-ingest/v1/`: positive, boundary, negative and canonical hash fixtures
- `crates/contracts/tests/compat/team-ingest/`: current/previous supported-version replay fixtures
- `cargo test -p contracts team_ingest_v1`: schema, canonicalization and compatibility gate

이 경로는 G1에서 생성되어야 하는 artifact 계약이며 현재 JavaScript baseline에 존재한다고
주장하지 않는다.

### End-to-end privacy evidence

Privacy sentinel suite는 prompt/output/command/path/token-like marker를 source fixture에 심고 다음
capture point 모두에서 byte/string absence를 검사한다: projector output, serialized envelope,
active queue, ACK journal, HTTP method/path/header/body capture, collector reject diagnostic, authoritative
record, aggregate, audit, service log/metric label, crash report, backup restore, query DTO와 export.
Control plane suite는 session/refresh/logout response, one-time enrollment/rotation response, persisted
credential verifier, management DTO, deletion/legal-hold receipt, quota reservation/ledger receipt와 해당
diagnostic/log를 별도로 capture한다. Token/secret은 지정된 one-time credential response 또는 secure
session cookie 외에는 어디에도 나타날 수 없고, observation payload marker는 모든 control output에서
금지되며 workspace/project display metadata도 endpoint allowlist 밖으로 확산할 수 없다.
Unknown field와 nested smuggling fuzz input은 projector 또는 collector에서 fail closed하며 accepted
record를 만들지 않는다. Evidence는 `docs/evidence/team/G1/privacy/`에 test command, commit, seed와
sanitized result로 보존한다.

## 6. Local retry queue

- Queue에는 `TeamIngestEnvelopeV1`과 retry metadata만 저장한다.
- Collector credential과 분리된 device-bound queue key로 암호화하고 private file permission을
  사용한다.
- Default active queue bound는 384 MiB 또는 7일 중 먼저 도달한 값이며 local total budget이 다르면 위
  deterministic partition formula를 사용한다.
- High watermark부터 oldest-first drain을 우선하고 ingest concurrency를 낮춘다.
- Hard limit에서는 새 team envelope의 queue admission을 거부하고 team sync를 `degraded`로
  전환한다. 기존 queued record를 조용히 덮어쓰지 않으며 dropped count/time range와 안전한
  diagnostic을 남긴다.
- Backoff, jitter, circuit breaker, server `Retry-After`를 적용한다.
- Permanent reject와 poison record는 재시도하지 않는다.
- Queue 삭제나 손상은 authoritative local record와 standalone report에 영향을 주지 않는다.
- ACK recovery journal은 accepted envelope, `commit_seq`와 receipt hash를 24시간 보존한다. Active
  retry queue와 별도 64 MiB default bound를 가지며 같은 queue key로 암호화한다. Active queue와 ACK
  journal 합계는 기본 448 MiB team partition을 넘지 않는다.
- Region restore는 새로운 `recovery_epoch`를 발행한다. Forwarder가 epoch 변화를 보면 unexpired ACK
  journal 전체를 replay한다. `commit_seq`는 receipt correlation에만 쓰며 replay filtering에 쓰지
  않는다. Authoritative record unique key가 dedupe ledger 손실과 무관하게 중복 저장을 막는다.
- ACK journal이 비어 있으면 poll하지 않는다. 비어 있지 않으면 source-authenticated recovery-state
  API를 5분 이하 간격에 jitter를 더해 조회해 새 ingest가 없어도 epoch 변화를 발견한다.
- Journal expiry 전에 service와 source device가 동시에 영구 손실되는 복합 재해는 region backup
  RPO의 적용을 받는다. UI와 incident receipt는 이 gap 가능성을 숨기지 않는다.

## 7. Data architecture

초기 authoritative store는 transaction, composite constraint와 migration을 지원하는 relational
database다. Ingest와 query를 하나의 generic repository로 합치지 않고 use case별 scoped
repository를 사용한다.

핵심 entities:

- `tenants`, `workspaces`, `projects`
- `principals`, `memberships`, `source_instances`, `credentials`
- `ingest_records`, `dedupe_entries`, `observation_revisions`
- `aggregate_buckets`, `aggregate_contribution_journal`, `projection_checkpoints`, `outbox_jobs`
- `privacy_policies`, `retention_policies`, `quota_policies`
- `usage_ledger`, `audit_events`
- `export_jobs`, `deletion_jobs`, `legal_holds`

Rules:

- Tenant data repository와 query function은 non-optional `TenantScope`를 요구한다.
- Platform migration/reconciliation은 tenant payload를 반환할 수 없는 `PlatformScope`, approved
  incident access는 tenant/reason/approver/expiry가 고정된 `BreakGlassTenantScope`를 사용한다. 이
  scope type은 서로 대입할 수 없고 별도 port를 가진다.
- Composite key/FK는 tenant/workspace mismatch를 database level에서도 거부한다.
- Database row isolation은 defense in depth이며 application authorization을 대체하지 않는다.
- Cache key, cursor와 export manifest에 tenant/workspace scope를 포함한다.
- User query는 bounded time range, page size와 cardinality를 강제한다.
- Transactional outbox로 aggregate worker를 구동한다. 별도 broker는 scale evidence가 생길 때만
  도입한다.
- Outbox, checkpoint, retention/deletion/export job은 생성 시 resolved tenant/workspace와 policy
  version을 immutable payload column에 고정한다. Worker가 global current tenant를 추론하지 않는다.
- `ReportDtoVx`는 aggregate와 scoped detail query에서 server-side로 생성한다.
- Aggregate rebuild는 deterministic하고 checkpoint부터 replay 가능해야 한다.
- Privacy-safe `aggregate_contribution_journal`은 allowlisted dimensions/metrics와 correction/visibility delta만
  aggregate retention 동안 보존한다. Raw observation이 만료된 뒤 aggregate rebuild의 authority이며
  versioned checkpoint 이후 journal replay로 동일 bucket/hash를 만들어야 한다.
- Observation revision은 accepted record와 별도 append-only table에 저장하고 tenant/workspace/event FK,
  operation uniqueness와 monotonic revision constraint를 database level에서 강제한다.

Team `ReportDtoV1`은 shared `ReportDtoVx` family의 strict profile projection이다.

| Information | Standalone | Team |
| --- | --- | --- |
| token, duration, status, topology | available when source provides | allowlisted observation only |
| project/repository | local display value after local policy | server-issued ref/display pseudonym |
| model | local normalized ID | bounded model ref/pseudonym |
| cwd/path/command/arguments/output | local policy may omit or summarize | always `unavailable_in_profile` |
| raw error message | local fail-closed reason/omission | bounded reason code only |
| content | local opt-in contract only | always `unavailable_in_profile` |

Field availability is explicit: `available`, `omitted_by_policy`, `unavailable_in_profile`, `unknown_source`.
UI never converts absent team fields to empty string or zero and aggregate completeness includes profile/source
availability counts.

## 8. Retention, deletion, and export

Observation, aggregate, audit, dedupe, queue, export와 backup은 서로 다른 retention class다.

The following values are internal planning targets, not customer or legal commitments until G0 legal/security
approval records their jurisdiction, start event, exclusions and contract wording:

| Data | Default | Rule |
| --- | --- | --- |
| observations | 30 days | workspace configurable within plan |
| observation revisions | aggregate retention 종료까지 | correction 접수는 observation retention 안에서만; orphan effect 금지 |
| aggregates | 13 months | raw observation보다 길게 유지 가능 |
| aggregate contribution journal/checkpoints | 13 months | raw content 없이 aggregate/revision rebuild authority |
| audit | 13 months | tenant admin이 단축할 수 없는 최소 기간은 계약에서 결정 |
| dedupe | 120 days | queue and supported schema replay window보다 길게 유지 |
| generated exports | 24 hours | signed access and one-time revocation support |
| encrypted backups | 35 days | expiry 후 restore 대상에서 제거 |

Deletion target inventory는 primary observation, aggregate/index, cache, generated export, source
credential, tenant data key와 backup generation이다. Service/audit record는 deleted payload를
복제하지 않는 최소 operation evidence만 별도 audit key domain으로 유지한다.

State machine:

```text
requested -> validating -> hold_blocked
                       `-> access_revoked -> primary_purging -> primary_purged
                                             -> backup_expiring -> complete
Any active state -> failed_retryable | failed_terminal
```

- Active legal hold가 있으면 `hold_blocked`가 되고 purge를 시작하지 않는다. Hold release는 별도
  authorized/audited action이며 deletion을 `validating`부터 재개한다.
- Deletion validation과 legal-hold create/release는 tenant-level lock과 monotonic
  `legal_hold_epoch`로 직렬화한다. `primary_purging` 진입 직전 purge fence가 epoch를 다시 확인한다.
  Fence 이전에 생성된 hold는 `hold_blocked`; fence 이후 hold request는 `too_late`로 거부하고 audit와
  legal escalation을 남긴다.
- `access_revoked`부터 tenant session, source credential, export URL과 query를 deny한다.
- `primary_purged`는 primary, aggregate/index, cache와 export purge 및 tenant data key destruction이
  끝났음을 뜻한다. 이 시점부터 active system에서 payload를 복호화할 수 없다.
- Backup generation은 35일 planning target까지 encrypted 상태로 남을 수 있으며 normal restore가
  deleted tenant를 활성화하지 않도록 deletion tombstone을 restore prerequisite로 적용한다.
- `complete`는 backup expiry와 restore-denial verification까지 끝난 상태다.
- Receipt는 `access_revoked_at`, target별 purge state, `primary_purged_at`, key destruction evidence,
  `backup_expires_at`, hold/exception, failure와 final completion을 구분한다. 법적 삭제 증명서라는
  표현은 G0 legal approval 전에는 사용하지 않는다.

Audit retention은 tenant payload retention과 분리한다. Tenant 삭제 후 actor/target은 irreversible
random tombstone references와 bounded action/outcome만 남기며 email, display name, source payload와
tenant data key로 암호화된 값을 남기지 않는다. Team service는 standalone local copy를 원격
삭제한다고 약속하지 않는다.

Export는 항상 server-resolved scope와 별도 fail-closed projector를 사용한다. Manifest에 schema,
field classification, policy version, requester, scope, 생성/만료 시각과 hash를 넣는다.

## 9. Security and privacy

- TLS로 모든 network transport를 보호한다.
- Tenant별 versioned data encryption key로 sensitive metadata를 envelope-encrypt하고 root key와
  data store를 분리한다.
- Key rotation은 새 write부터 새 version을 쓰고 background rewrap으로 완료한다.
- Backup은 generation별 referenced key-version inventory를 가진다. 참조 backup이 만료되거나 새 key로
  재암호화되고 restore 검증되기 전에는 이전 key를 `recovery_only`보다 강하게 retire하지 않는다.
- Credential, key material과 token은 application log, audit payload, panic dump에 남기지 않는다.
- Internal diagnostics도 strict bounded schema를 사용한다.
- Dependency와 build artifact는 provenance, vulnerability scan과 signed release evidence를 가진다.
- Security-sensitive migration은 backward-compatible expand/contract 순서와 rollback evidence가
  필요하다.

Threat model과 필수 방어:

| Threat | Prevent | Detect/recover |
| --- | --- | --- |
| forged tenant/workspace | server scope resolution, composite constraints | denial audit, cross-tenant fixture |
| stolen device credential | narrow ingest scope, rotation/revoke | anomaly signal, immediate revoke |
| replay/conflicting payload | idempotency key + canonical hash | conflict audit, source quarantine |
| schema/content smuggling | strict schema, no arbitrary map/string | privacy sentinel and fuzz suite |
| high-cardinality/resource DoS | bounds, quota, rate limit, bounded query | per-tenant saturation metrics |
| cache/export scope leak | scoped keys, signed manifest, short expiry | negative isolation tests, revoke |
| operator misuse | separated operator identity, break-glass workflow | tamper-evident audit and review |
| backup/key compromise | encrypted backup, separated keys, rotation | restore/key revoke drill |

## 10. Audit, metering, and quotas

Audit events are append-only and contain actor, action, target scope, outcome, request ID, policy version,
server time and bounded reason code. Login, membership, role, policy, retention, quota, export, deletion,
key rotation, source revoke, observation correction/retraction and operator access are audited. Raw source data
and raw error strings are not.

Each tenant audit stream is hash chained over canonical event bytes and sequence number. A daily stream root is
signed with an audit signing key unavailable to the normal application database writer and copied to a
separately authorized checkpoint store. Verification runs daily and after restore; gap, reordered event,
invalid signature or missing checkpoint raises an integrity incident. Repair never rewrites history: it appends
a recovery event and preserves the failed segment for investigation. Platform operator audit uses a separate
stream and key.

Usage ledger is server-derived and reconciled independently from aggregate dashboards. Initial GA uses it for
capacity/quota control, not customer invoicing.

- accepted records: one unit per first committed unique event
- accepted bytes: canonical uncompressed envelope bytes committed before transport compression
- retained storage: measured physical tenant bytes at hourly reconciliation, soft warning only in first GA;
  storage capacity incident는 quota reject가 아니라 `temporarily_unavailable`
- query/export: one idempotent operation plus actual result bytes; concurrency is a separate limiter
- active source instances and members: server-side active rows at policy evaluation time

Hard admission limits in first GA apply to accepted record/byte window, active source/member count, query
concurrency and export bytes. Each mutation creates an idempotent operation ledger entry and atomically
`reserve -> commit` or `reserve -> release` with the affected resource transaction. Duplicate ingest reuses the
original operation and is not metered twice. Window reset uses server UTC and versioned quota policy; adjustment
is a new compensating ledger entry, never an update. Concurrent requests lock or compare-and-swap the same
tenant quota bucket, so hard limits cannot be exceeded by race. 429 response includes bounded reason and
retry-after/reset time; local sender retains retryable envelopes subject to queue bounds. Estimated model cost
remains a separate report metric.

## 11. API and schema evolution

- Rust contract crate is the source for JSON schema and generated/validated TypeScript types.
- `TeamIngestEnvelopeV1`, management DTO and `ReportDtoVx` are separate contracts.
- Unknown field is rejected at external write boundaries.
- Additive optional fields require fixture evidence; changed meaning requires a new contract version.
- Service supports current and previous ingest major version for at least the greater of 90 days or maximum
  supported offline queue age. Hosted API supports current and previous `ReportDto` and management DTO major
  versions through rolling deploy plus 90 days; UI/API compatibility fixtures cover N/N, N/N-1 and N-1/N.
- Deprecation is observable by tenant/source and cannot silently drop queued data.
- Database migration uses expand, dual-read/write only when required, backfill verification, contract, then
  cleanup. Every destructive step has rollback or restore evidence.

## 12. Reliability and commercial readiness

Initial objectives are internal engineering targets, not customer commitments until G0 approval and G4 load,
restore and incident evidence exist.

Reference planning workload for architecture tests is 100 tenants, 1,000 workspaces, 10,000 active source
instances, 1,000 records/sec sustained, 5,000 records/sec for 15-minute bursts and 100 million retained
observations. G0 must replace or approve this forecast before implementation sizing.

| Signal | Definition | Internal target |
| --- | --- | --- |
| ingest availability | syntactically eligible batch requests returning non-5xx / all eligible requests before auth outcome, rolling calendar month; invalid credentials remain expected 401, identity dependency failures count unavailable | 99.9% |
| query availability | authorized bounded queries returning non-5xx / eligible queries; identity dependency failures count unavailable | 99.9% |
| identity/control availability | eligible login/session refresh and authorized control requests returning expected non-5xx outcome / all eligible requests | 99.9% |
| admitted batch ACK latency | server receive to multi-zone durable commit response for batches <=500 records/2 MiB | p95 <= 500 ms, p99 <= 2 s |
| standard dashboard query | one workspace, 30-day range, <=25 projects, <=1,000 detail rows, warm aggregate | p95 <= 2 s |
| aggregate freshness | accepted server receipt time to visible aggregate checkpoint | p95 <= 60 s, p99 <= 5 min |
| accepted-data RPO | process/node/zone loss after ACK; synchronous authoritative quorum | 0 |
| catastrophic region RPO | service restore when no source ACK journal can replay | <= 5 min |
| service RTO | process/node/zone incident | <= 60 min |
| region recovery RTO | total configured-region loss | <= 4 h |
| primary purge target | `access_revoked` to `primary_purged`, excluding active legal hold | <= 24 h planning target |

Required operational evidence:

- load test at 2x approved forecast peak with one tenant consuming 50% of traffic; other tenants remain within
  latency/error targets
- 429, timeout, process restart, database failover, disk full and poison-record fault injection
- encrypted backup restore plus ACK-journal replay; accepted record IDs, authoritative unique index, dedupe,
  usage ledger, policy, key version and audit checkpoint reconcile with pre-failure manifest
- rolling deploy, schema migration and rollback rehearsal
- SLO dashboards, burn alerts and user-visible status/degraded state
- runbooks for credential compromise, tenant isolation incident, queue saturation, projection lag,
  deletion failure and key rotation failure
- audit integrity and usage ledger reconciliation jobs

Evidence is stored under `docs/evidence/team/G2|G3|G4/<test-name>/` with commit SHA, sanitized configuration,
seed/workload, start/end time, command or runbook revision, raw numeric result, pass/fail threshold and reviewer.
An evidence directory is created only when the corresponding implementation test runs; this design document is
not itself evidence.

Service logs and metrics must be sufficient to operate the system without copying tenant observation payloads.

## 13. Delivery gates

### G0 - Decision freeze

- contracting customer organization is assumed to own tenant data and designated Owner authorizes lifecycle;
  legal/security must approve or replace this assumption
- hosted-only, configured-region, logical multi-tenant deployment and operator/key custody approved
- tenant/workspace lifecycle and role matrix approved
- retention, legal hold, deletion wording and responsible owner approved
- operational quota only for first GA; customer billing explicitly deferred
- scale forecast and internal SLO targets approved
- decision record contains approver role, date, chosen value and superseded alternatives
- decision record contains named approver identities plus separate business, legal and security attestations

### G1 - Contract

- `TeamIngestEnvelopeV1`, error taxonomy and field classification complete
- multi-email identity, profile resolution, binding revocation and adapter heartbeat contracts complete
- local transactional state/outbox, bounded collection policy and append-only correction/retraction contracts complete
- official-surface adapter capability matrix records primary/supplement sources, supported versions and fixture digests
- strict schema, privacy sentinel, size/cardinality and N/N-1 fixtures pass
- local queue contract and credential lifecycle defined
- `docs/evidence/team/G1/contracts/manifest.yaml` exists and records every artifact/hash/test result required by
  `TEAM_CONTRACTS.md` section 2.
- ingest/response/report/management/local-state/collection-policy/queue/credential/identity/heartbeat/revision/deletion/quota/operations schema, crypto policy,
  generated TypeScript and hash manifest all exist; every per-artifact Rust test, compatibility/privacy suite
  and `cargo run -p xtask -- contracts generate --check` pass with no drift.

### G2 - Secure alpha

- principal-bound ingest, membership/RBAC, scoped query and export implemented
- Codex, Claude and Cursor adapters pass local/team parity, explicit identity selection, ambiguity fallback,
  heartbeat and isolated adapter-failure tests
- email ownership verification, admin approval separation, allowed-domain policy, cross-tenant non-correlation,
  idempotent identity deletion/receipt, raw-email purge, hosted-export revocation and retained-reference
  pseudonymous-data, identity-key destruction, tombstone restore and legal-hold race tests pass
- same-source cross-principal substitution, shared-source attribution denial, heartbeat stale/replay/concurrent/
  cross-adapter writes and epoch reset tests pass
- record/dedupe/meter atomicity and cross-tenant/cache isolation tests pass
- local crash-point replay proves atomic cursor/record/outbox state and stable event identity
- Codex, Claude and Cursor hook latency plus idle/active CPU, RSS and disk evidence meets the local budget
- endpoint matrix, 30-second revoke/epoch behavior, resource non-disclosure, audit integrity, quota race and admin
  negative tests pass

### G3 - Resilient beta

- bounded encrypted queue, backpressure and degraded UX complete
- adaptive reconciliation, schedule jitter and pressure load-shedding preserve foreground agent responsiveness
- correction/retraction conflict, authorization, aggregate reversal and deterministic replay tests pass
- deletion/legal-hold/key/audit state machine, ACK-journal replay, key rotation and restore drills pass
- load, concurrency, fault injection and projection rebuild pass

### G4 - General availability

- SLO evidence, on-call runbooks and incident exercises complete
- independent security and architecture review has no blocking finding
- no critical finding is open or waived; any time-bounded high-risk exception has named security and business
  approval, compensating controls, expiry and remediation owner and is explicitly non-blocking in both reports
- usage reconciliation, migration/rollback and deletion receipts verified
- clean machine standalone smoke still passes without network, login or collector

No team release receives a semantic version until G0 is approved. No release is called commercially ready
until G4 evidence exists.

## 14. Decisions and open questions

Proposed G0 defaults:

- logical multi-tenancy with tenant-scoped keys and constraints
- single authoritative relational store plus transactional outbox and async aggregates
- at-least-once transport with durable idempotency instead of exactly-once claims
- OIDC-compatible federation for the first human identity path; enterprise federation and lifecycle
  provisioning are compatibility requirements, not first-alpha blockers
- hosted-only first commercial deployment, one configured region per tenant, multi-zone authoritative store
- first GA metering is operational quota only; customer billing is deferred

G0 blockers:

- named business, legal and security approvers for data ownership, deletion/hold wording and operator/key custody
- first supported data region and residency statement
- approved forecast for tenant count, peak records/sec, cardinality and largest offline fleet
- approval or replacement of the internal SLO, retention and deletion planning targets

Deferred, not G0 blockers:

- self-hosted or dedicated tenant deployment
- customer billing/invoice unit
- enterprise federation beyond the first OIDC-compatible path and automated user lifecycle provisioning
