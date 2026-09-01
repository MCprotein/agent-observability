# Configuration

기본 설정 인터페이스는 local-only web UI다. `config.json`을 직접 편집하지 않고도 모든
runtime option을 조회하고 변경할 수 있다. 기본 runtime은 `~/.agent-observability`다.

## 사용법

```bash
agent-observability ui
```

별도 runtime은 root를 지정한다. 브라우저를 자동으로 열 수 없는 환경에서는 출력된 URL을 직접 연다.

```bash
agent-observability ui /private/runtime
agent-observability ui /private/runtime --no-open
```

UI는 `127.0.0.1`의 임의 port에만 bind하고 URL fragment의 session capability를 private header로
옮긴다. API는 정확한 Host, Origin과 session을 확인하며 CORS를 허용하지 않는다. 설정 화면은
외부 request를 만들지 않는다. 사용자가 1분 이상 화면을 조작하지 않으면 browser heartbeat를
멈추며, 화면 연결이 10분 동안 끊기거나 process 실행 후 1시간이 지나면 종료된다. 정적 report에는
영향을 주지 않는다.

자동화와 headless 환경에서는 CLI를 사용한다.

```bash
agent-observability config show
agent-observability config set retention-days 90
agent-observability config set storage-bytes 2147483648
```

별도 runtime은 root를 option 앞에 둔다.

```bash
agent-observability config show /private/runtime
agent-observability config set /private/runtime retention-days 90
```

변경값은 먼저 전체 config bounds로 검증된다. 성공한 config만 mode `0600` 임시 파일에 기록한
뒤 원자적으로 교체되며 다음 command부터 적용된다. 잘못된 값, unknown option, broad permission,
symlink는 기존 config를 바꾸지 않고 거부한다.

UI process는 UI 중복 실행만 막는 전용 lock을 유지한다. 실제 저장은 전역 runtime lock을 짧게
획득한 뒤 browser가 읽은 revision을 다시 확인한다. 따라서 UI를 열어 둔 동안에도 ingest, report,
CLI 설정 명령을 실행할 수 있고, 동시에 바뀐 설정을 UI가 조용히 덮어쓰지 않는다. 충돌이 나면
최신 설정 위에 browser에서 바꾼 필드만 다시 적용하고 사용자가 재검토 후 저장한다.

## Options

| Option | Default | Allowed | Purpose |
| --- | ---: | ---: | --- |
| `enabled` | `true` | `true`, `false` | local import 허용 여부 |
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

현재 one-shot/manual import 제품 경로에서 직접 체감되는 option은 `enabled`, `storage-bytes`,
retention과 archive 범위다. reconcile, flush, batch, heartbeat option은 동일한 bounded runtime
contract를 사용하는 producer embedding을 위한 값이며, 자동 producer가 없는 v1.5.0 CLI가 background
schedule을 시작한다는 뜻이 아니다.

## 자주 쓰는 변경

```bash
# 수집 일시 중지
agent-observability config set enabled false

# 7일 보관
agent-observability config set retention-days 7

# 4 GiB storage budget
agent-observability config set storage-bytes 4294967296
```

낮은 flush/reconcile 간격과 큰 batch는 처리량뿐 아니라 foreground I/O와 memory 사용량에도
영향을 준다. 특별한 측정 근거가 없다면 기본값을 유지한다.
