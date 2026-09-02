# Configuration

기본 설정 인터페이스는 local-only web UI다. `config.json`을 직접 편집하지 않고도 모든
runtime option을 조회하고 변경할 수 있다. 기본 runtime은 `~/.agent-observability`다.

v1.8.0은 **Released**다. 이 문서의 `config.json` option은 standalone runtime policy다.
Optional Codex automatic connection은 별도의 private collector settings와 exact Codex config ownership을
사용하며 `connect`, `status`, `disconnect` 명령 또는 같은 local UI로 관리한다. Manual imports는 이
연결 없이도 모두 동작한다.

## 사용법

```bash
agentobs ui
```

별도 runtime은 root를 지정한다. 브라우저를 자동으로 열 수 없는 환경에서는 출력된 URL을 직접 연다.

```bash
agentobs ui /private/runtime
agentobs ui /private/runtime --no-open
```

UI는 `127.0.0.1`의 임의 port에만 bind하고 URL fragment의 session capability를 private header로
옮긴다. 같은 tab의 새로고침을 위해서만 session storage에 보존하며 확인된 명시적 종료, invalid session,
bootstrap/heartbeat/config mutation network failure 확인 시 삭제한다. 종료 요청 자체가 실패하면 다시
시도할 수 있도록 현재 tab의 capability를 유지한다.
cookie, local storage, 외부 전송에는 저장하지 않는다. API는 정확한 Host, Origin과 session을
확인하며 CORS를 허용하지 않는다. 설정 화면은 외부 request를 만들지 않는다. 사용자가 1분 이상
화면을 조작하지 않으면 browser heartbeat를
멈추며, 화면 연결이 10분 동안 끊기거나 설정 server 시작 후 1시간이 지나면 종료를 요청한다. 이
deadline은 로컬 executor와 filesystem이 응답하는 동안 적용된다. 정적 report에는
영향을 주지 않는다.

자동화와 headless 환경에서는 CLI를 사용한다.

```bash
agentobs config show
agentobs config set retention-days 90
agentobs config set storage-bytes 2147483648
```

별도 runtime은 root를 option 앞에 둔다.

```bash
agentobs config show /private/runtime
agentobs config set /private/runtime retention-days 90
```

변경값은 먼저 전체 config bounds로 검증된다. `config.json` 직접 편집은 지원 인터페이스가 아니다.
성공한 config만 mode `0600` 임시 파일에 기록한
뒤 원자적으로 교체되며 다음 command부터 적용된다. 잘못된 값, unknown option, broad permission,
symlink는 기존 config를 바꾸지 않고 거부한다.

UI process는 UI 중복 실행만 막는 전용 lock을 유지한다. 실제 저장은 전역 runtime lock을 짧게
획득한 뒤 browser가 읽은 revision을 다시 확인한다. 모든 지원 CLI/UI writer는 타입으로 강제된
동일 mutation guard를 사용한다. 따라서 UI를 열어 둔 동안에도 ingest, report, CLI 설정 명령을
실행할 수 있고, 지원 writer끼리 동시에 바뀐 설정을 조용히 덮어쓰지 않는다. 충돌이 나면 최신
설정 위에 browser에서 바꾼 필드만 다시 적용하고 사용자가 재검토 후 저장한다.

같은 UI에서 Codex integration status를 확인하고 연결/해제하며 기존 dashboard를 열 수 있다. 이
integration mutation도 private UI session, exact Host와 Origin을 확인하고 Rust blocking executor에서
실행한다. UI를 닫아도 이미 연결된 LaunchAgent는 계속 실행되며, 연결 해제는 명시적으로 수행해야 한다.

## Options

| Option | Default | Allowed | Purpose |
| --- | ---: | ---: | --- |
| `enabled` | `true` | `true`, `false` | manual import와 automatic collector ingest 허용 여부 |
| `file-reconcile-ms` | `5000` | `1000..60000` | file source 재확인 간격 |
| `flush-ms` | `5000` | `1000..60000` | bounded flush 간격 |
| `batch-records` | `100` | `1..500` | batch당 최대 record 수 |
| `batch-bytes` | `524288` | `16384..2097152` | batch당 최대 bytes |
| `active-heartbeat-ms` | `60000` | `30000..300000` | active source heartbeat 간격 |
| `idle-heartbeat-ms` | `300000` | `120000..900000` | idle source heartbeat 간격 |
| `storage-bytes` | `1073741824` | `268435456..21474836480` | local runtime 전체 storage budget |
| `retention-days` | `30` | `1..3650` | retention cutoff age |
| `archive-records` | `10000` | `1..100000` | 한 archive의 최대 record 수 |
| `archive-bytes` | `16777216` | `65536..268435456` | 한 archive의 최대 bytes |

시간 option은 milliseconds, 용량 option은 bytes 단위다. 설정 변경은 자동 cleanup을 실행하지
않는다. `retention-days` 변경 후 실제 만료 대상은 `retention-plan`으로 확인하고
`retention-apply`로 명시적으로 적용한다.

`enabled`, `batch-records`, `batch-bytes`, `storage-bytes`는 v1.8.0 Codex automatic collector가
요청마다 다시 읽고 적용하므로 UI나 CLI에서 바꾼 뒤 collector를 재시작할 필요가 없다. Codex
receiver는 설정값과 별개로 요청당 1 MiB, 4096 log record의 절대 parser 상한을 유지하며 실제
허용량은 설정 상한과 절대 상한 중 더 작은 값이다. `enabled=false`이면 인증된 요청을 수신하되
parse하거나 저장하지 않는다.

`file-reconcile-ms`, `flush-ms`, heartbeat option은 동일한 bounded runtime contract를 사용하는
producer embedding을 위한 값이다. retention과 archive 설정은 manual/automatic 경로가 공유하는
durable store에 적용된다. v1.8.0 Codex automatic collector의 connection, port, authentication과
LaunchAgent lifecycle을 이 option으로 직접 편집하지 않는다.

## Codex automatic connection

```bash
agentobs connect codex
agentobs status codex
agentobs disconnect codex
```

별도 runtime은 마지막 argument로 지정한다.

```bash
agentobs connect codex /private/runtime
agentobs status codex /private/runtime
agentobs disconnect codex /private/runtime
```

Collector settings는 `<root>/runtime/collector.json`에 mode `0600`으로 기록한다. 현재 schema는
`local_collector.v3`이며 loopback port, transport, private random 256-bit request-header value와 bounded
credential generation metadata를 소유한다. PEM 본문은 settings에 넣지 않고
`<root>/runtime/integrations/codex/tls` 아래의 `0700` directory와 `0600` regular file로 분리한다.
이 파일들은 supported user-editable config가 아니다. Receiver는 configured IPv4 loopback port에만
bind한다. Client는 private CA가 서명한 server certificate와 loopback IP SAN을 검증하고, 모든
request에 exact `x-agent-observability-token` header를 제공한다. Client certificate/private-key field는
구성하지 않으며 이 transport는 mTLS가 아니다. 정확히 인식한 `local_collector.v2` mTLS settings는
exact prior bytes/mode와 credential generation을 durable migration journal에 보존한 뒤 v3 후보를
게시한다. LaunchAgent와 Codex config commit이 모두 끝난 뒤에만 obsolete client identity artifact를
제거하며, 실패하면 prior settings와 credentials를 복원한다. External endpoint 설정은 없다.

Codex config는 `$CODEX_HOME/config.toml` 또는 기본 `~/.codex/config.toml`이다. Connection manager가
항상 소유하는 값은 아래 세 OTEL 항목이다. top-level `notify`는 기존 값이 없을 때만 선택적으로 소유한다.

| Managed value | Required value |
| --- | --- |
| top-level `notify` (optional) | 기존 값이 없을 때만 canonical absolute installed executable path, `codex-notify`, absolute runtime root의 3-element command를 추가; 기존 명령은 그대로 보존 |
| `otel.exporter` | local HTTPS `/v1/logs` endpoint, JSON protocol, private CA path와 exact `x-agent-observability-token` header만 포함한 OTLP/HTTP exporter; client identity field 없음 |
| `otel.log_user_prompt` | `false` |
| `otel.environment` | `local` |

인자 없는 `setup`은 real Codex home 또는 PATH의 executable을 읽기 전용으로 감지한 경우에만 이
connection lifecycle을 시작한다. 감지되지 않으면 `codex=not_detected`를 출력하며 Codex home, config,
collector service를 생성하지 않는다. 명시적 `connect codex`는 사용자가 integration 생성을 요청한
경로이므로 기존 생성 semantics를 유지한다.

Connect는 기존 OTEL managed value가 없거나 정확히 같은 경우에만 진행한다. 기존의 non-empty
string-array notify command는 충돌로 취급하지 않고 `external_preserved`로 보고한다. string, table,
empty array 또는 string 이외의 원소가 있는 notify는 config를 수정하지 않고 conflict로 중단한다. 연결 전과 연결 직후 config의
존재 여부, exact bytes, SHA-256와 permission mode를
`<root>/runtime/integrations/codex/codex-config-ownership-v1.json`에 private snapshot으로 보존한 뒤
serialized compare/replace transaction으로 교체한다. 중간 crash는 snapshot phase와 현재 file을 비교해
다음 lifecycle command에서 결정적으로 복구한다. 기존 다른 exporter/privacy/environment 값을
병합하거나 덮어쓰지 않고 conflict로 중단하며, 기존 notify는 agentobs 소유권 밖에 둔다.

Disconnect는 commit 전 반복 검사에서 현재 config의 전체 bytes와 mode가 snapshot의 exact connected
state와 같을 때만 prior bytes와 mode를 복원한다. 연결 전 config가 없었다면 생성한 file을 제거한다.
검사에서 관측된 user/tool 편집은 managed value 여부와 무관하게 conflict로 중단하고 보존한다.
agentobs writer는 lock으로 직렬화하지만, lock을 무시하고 이미 열린 descriptor에 최종 검사와 rename
사이 쓰는 임의 프로세스를 배제하는 portable filesystem CAS는 아니다. Successful
disconnect는 연결 전 LaunchAgent plist와 loaded 상태를 먼저 복원한다. Connect가 새로 만든 service만
종료·제거하며, 그 뒤 config를 복원한다. Local observation, SQLite와 dashboard는 보존한다.

Private CA는 trusted client가 올바른 receiver를 검증하게 하고 exact private header는 credential이 없는
request를 거부한다. 다만 같은 OS user로 실행되어 `0600` header secret이나 server private key를
읽을 수 있는 process는 이 경계만으로 구분할 수 없다.

## 자주 쓰는 변경

```bash
# 수집 일시 중지
agentobs config set enabled false

# 7일 보관
agentobs config set retention-days 7

# 4 GiB storage budget
agentobs config set storage-bytes 4294967296
```

낮은 flush/reconcile 간격과 큰 batch는 처리량뿐 아니라 foreground I/O와 memory 사용량에도
영향을 준다. 특별한 측정 근거가 없다면 기본값을 유지한다.
