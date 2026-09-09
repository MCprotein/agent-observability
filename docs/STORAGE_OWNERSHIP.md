# 저장 파일 소유권과 분류 — P2 구현 기준

상태: **개발 중인 설계 기준. 제한된 파일 분류와 예약 identity 검증은 구현·검증됐으며,
전체 writer의 동시 측정·분리 모드 실행은 아직 연결되지 않았다.**
[저장 예산 계획](STORAGE_BUDGET_POLICY.md)과 [P0 계산 계약](STORAGE_BUDGET_P0.md)을
구체화한다. 아래 경로 목록은 이름만 보고 파일을 승인하거나 삭제하는 allowlist가 아니다.

## 분류 원칙

| 분류 | 의미 | 필요한 근거 |
| --- | --- | --- |
| A — 보관 | DB, 게시된 보고서, 설정·복구 기록 등 지속적으로 보존하는 파일 | 해당 경로를 소유한 모듈의 구조·identity·private 경로 검증 |
| X — 작업 | 현재 작업 또는 검증된 복구 작업에 귀속된 일시 파일 | 정확한 파일 identity와 작업 handle/guard의 결합; 이름·PID·mtime만으로 인정하지 않음 |
| U — 미분류 | 위 근거를 확보하지 못한 파일·디렉터리 | 실제 할당량과 entry 수를 기록하고 신규 쓰기 거부; 임의 정리하지 않음 |

이하 모든 경로는 managed runtime root 기준이다. 디렉터리 자체의 할당량도 포함하고,
빈 파일도 entry로 검사한다. 검증된 디렉터리 아래 있다는 이유로 모든 자식을 승인하지 않는다.
symlink, hardlink alias, 경로 교체, 순회 중 사라짐, 권한·형식 불일치와 순회 상한 초과는
구현 시 fail-closed해야 한다. 현재 strict tree scan만으로 이 모든 검사가 완료된 것은 아니다.
근거: `crates/local-runtime/src/storage.rs`의 `allocated_tree_bytes_strict`.

## 지속 파일 후보

아래 A는 **소유 모듈의 검증을 통과했을 때의 목표 분류**다. 검증되지 않은 동일 이름은 U다.
정상 생성 직후부터 게시 전까지의 중간 상태는 다음 절의 작업 소유권 검증을 별도로 따른다.

| 경로 | 소유 경계·확인할 원문 | 검증 시 분류 |
| --- | --- | --- |
| `config.json`; `logs`, `queue`, `state`, `runtime` 및 root 디렉터리 | local-runtime `config.rs`: `install`, layout 검증; 디렉터리 승인과 자식 승인을 분리 | A |
| `runtime/mutation.lock`, `runtime/report-reservation.lock`, `runtime/report-reservation.meta` | local-runtime `lock.rs`, `reservation.rs`; stable lock identity와 별도 versioned 예약 metadata | A; 예약 R은 파일 크기와 별도로 유지 |
| `<singleton-dir>/runtime.lock`, `<singleton-dir>/runtime.meta` | local-runtime `Singleton::acquire`, `read_nonce`; singleton-dir는 CLI의 `runtime`, collector의 `runtime/collector`, UI의 `runtime/settings-ui`, `runtime/dashboard-ui`로 한정 | A; metadata가 제거 가능하다는 이유로 X로 할인하지 않음 |
| `runtime/dashboard-ui/capability` | local-ui `read_dashboard_token`, `load_or_create_dashboard_token`; private file과 token grammar 검증 | A; capability 값은 출력 금지 |
| `state/store/local-store.sqlite3`, `state/store/observations.jsonl`, `state/store/.store-open.lock`, `state/store/.report-render.lock` | local-store `lib.rs`: 파일 상수, private open, projection과 render guard | A; DB 내부 hot/warm/cold/index/freelist도 DB 할당량에 포함 |
| `state/store/report-views.v1/catalog.json` | local-store `report_view_catalog.rs`: `validate_catalog`, `validate_catalog_authority` | A |
| `state/store/report-views.v1/report-view-<view-id>.sqlite3` | 같은 모듈 `validate_snapshot`, `validate_view_id`; view-id는 소문자 hex 64자이며 검증된 current/retired catalog membership도 필요 | A; 이름만 같은 orphan은 U |
| `logs/agent-observability-report.html` | static-report `write_private`; local-ui의 bounded private report reader | A |
| `runtime/report-dirty` | local-collector `report_dirty_path`, `mark_report_dirty`; wakeup hint이며 완료 authority가 아님 | A; 임시 workspace로 할인하지 않음 |
| `runtime/collector.json`, `runtime/collector-settings-migration.json` | local-collector `settings_path`, `settings_migration_path`, settings/migration 검증 | A; token·복구 원문을 accounting 결과에 넣지 않음 |
| `runtime/integrations/codex/tls/<generation>/ca-certificate.pem`, `server-certificate.pem`, `server-private-key.pem` | local-collector `validate_settings_shape`, `validate_owned_credentials`; generation은 settings가 참조하는 ASCII hex 64자, 경로는 exact match | A; 임의 generation은 U |
| 위 TLS generation의 legacy `client-certificate.pem`, `client-private-key.pem` | local-collector legacy/migration 검증과 복구 phase; current v3의 일반 자격 증명으로 승인하지 않음 | 유효 legacy/migration 소유 증거가 있으면 A, 없으면 U |
| `state/private-codex-turn-details/<digest>.json`, `state/private-codex-turn-detail-statuses/<digest>.json` | local-collector `private_turn_detail_path`, detail/status 검증; digest는 소문자 hex 64자 | A; incomplete pair도 임의 X 전환하지 않음 |
| `runtime/integrations/codex/codex-config-ownership-v1.json` | codex-config `snapshot_path`, snapshot 검증; codex-integration이 state directory를 지정 | A; 원문 config snapshot은 accounting 출력에서 제외 |
| `runtime/integrations/codex/launch-agent-ownership-v1.json`, `runtime/integrations/codex/lifecycle/mutation.lock` | codex-integration ownership phase 검증과 lifecycle guard | A |

이 표에 없는 파일을 편의상 A로 승인하지 않는다. `queue/*`에 대한 범용 승인은 없으며,
team outbox는 현재 활성화되지 않은 계약이다.

## 작업 파일 후보와 귀속

`<pid>`와 `<seq>`는 각 writer가 십진수로 생성하는 값이다. 이 표는 생성 형태를 설명하며,
그 문자열을 파싱한 결과가 소유권 증명이라는 뜻은 아니다.

| 경로·생성 형태 | 소유 경계 | X로 인정하기 전에 필요한 연결 |
| --- | --- | --- |
| `.config.json.tmp.<pid>`, `.config.json.update.<pid>.<seq>` | local-runtime `install`, `private_update_file` | install의 no-overwrite publication과 save의 config mutation guard; install의 일시적 hardlink를 일반 alias 승인으로 확대하지 않음 |
| `<singleton-dir>/.runtime.meta.tmp.<pid>` | local-runtime `Singleton::acquire`; 위의 네 singleton directory만 해당 | singleton lock과 실제 metadata temp handle, publication phase |
| `runtime/.report-reservation.meta.tmp` | local-runtime `reservation.rs` | 동일 root mutation guard, stable reservation lock, metadata publication/recovery |
| `state/store/.observations.jsonl.tmp.<pid>.<seq>` | local-store `create_projection_temp`, `remove_stale_projection_temps` | projection writer의 실제 file identity; cleanup prefix만 공유하지 않음 |
| `state/store/local-store.sqlite3-journal` | local-store의 SQLite connection, migration admission | 소유 DB 및 transaction/recovery 경계; 직접 삭제하거나 WAL 동작을 추측하지 않음 |
| `state/store/report-views.v1/.report-view.sqlite3.staging.<pid>.<seq>` 및 해당 SQLite journal | `ReportViewStaging`, `create_staging_file` | 실제 staging identity, publication guard, runtime 예약 owner를 결합 |
| `state/store/report-views.v1/.catalog.json.tmp.<pid>.<seq>` | `create_catalog_temp`, catalog publication | 같은 publication 작업의 handle와 교체 phase |
| `<검증된 collector writer의 parent>/.collector.json.tmp.<pid>.<seq>` | local-collector `settings_temporary_path`, private atomic writers | parent는 호출자가 실제 소유한 settings/detail/status 경계여야 함; root 전체 wildcard 금지 |
| `logs/.agent-observability-report.html.tmp.<pid>.<seq>` | static-report `temporary_path`, atomic writer | 정확한 target, private temp handle와 보고서 작업 수명 |
| `runtime/integrations/codex/.launch-agent-ownership-v1.json.agentobs.<pid>.<seq>` | codex-integration `atomic_write`, `atomic_write_checked` | lifecycle guard와 ownership transaction phase; 같은 형식의 외부 plist temp와 구분 |
| `runtime/integrations/codex/.codex-config-ownership-v1.json.agentobs.new.<pid>.<seq>` | codex-config `atomic_replace_with`, `unique_sibling(path, "new")` | shared Codex config lock, snapshot expectation과 private temp handle; 외부 config 교체 temp는 root 집계에서 제외 |

실패·중단 뒤 temp가 남았다는 이유로 예약을 해제하지 않는다. 기존 소유 모듈의 guarded
recovery로 귀속과 정리 가능성을 검증하고, 정리 및 durable metadata 해제가 끝나야 R을 반환한다.
분류기는 cleanup을 실행하지 않으며, 기존 cleanup API의 prefix 인식을 일반 소유 증명으로 재사용하지 않는다.

## singleton 분류의 별도 완료 조건

생산 singleton의 writer 참여는 파일 분류기 구현과 구분한다. 다음 읽기 전용 증거는
정확한 네 scope의 디렉터리와 `runtime.lock`/`runtime.meta`만 대상으로 한다.
root mutation 소유권과 외부 accounting freeze를 유지한 상태에서 기존 descriptor와
부재를 보존하고, 빈 private lock 및 기존 256바이트 metadata 문법을 검증해야 한다.
PID나 nonce 문법은 파일 의미의 검증이지 실행 중인 process나 삭제 권한의 증명이 아니다.
임의 sibling, metadata temp, 다른 이름의 scope는 승인하지 않는다. 디렉터리·파일의 교체,
권한 변경, hardlink/FIFO, capture 후 출현·소멸과 metadata 변경은 최종 재검증에서 거부한다.
이 제한된 증거와 synthetic collector 연결은 구현·독립 검증됐다. 일반 경로 목록만으로
A를 승인하지 않으며, 전체 production 분류·admission 연결 완료를 뜻하지 않는다.

## runtime 바깥 경계

수동 `retention-apply` archive는 CLI `normalize_archive_path`가 **runtime root 밖**으로
제한한다. 따라서 그 archive와 임시 파일을 root 내부 A/X 총량에 넣지 않는다. 작업 E의
보수적 산정과 실제 archive destination의 여유 검사 책임은 별개이며, 다른 filesystem의
여유를 runtime filesystem의 D로 대신 증명할 수 없다. 새 정책 통합 시 이 경계를 검증해야 한다.
DB 내부 cold blob은 별도 외부 archive 파일이 아니라 authority DB의 A다.

Codex home의 config·공유 config lock, LaunchAgents plist, npm/binary 설치 경로도 root 밖이다.
accounting 때문에 이 경로를 순회하거나 snapshot 내용·PEM·사용자 원문을 읽어 출력하지 않는다.
별도 소유 모듈의 기존 검증 결과를 좁은 증거로 조합해야 한다.

## 구현 순서와 회귀 기준

1. 고정 경로·singleton·config atomic update·integration recovery의 전체 production writer
   수명을 검증한다. 위 후보 목록을 완전한 inventory로 간주하지 않으며,
   누락은 U로 남기고 P2 완료로 표시하지 않는다.
2. 소유 모듈이 정확한 파일 identity와 수명을 검증한다. local-runtime이 local-store를
   의존하거나 SQLite schema/catalog를 직접 해석하지 않는다. collector/CLI가 증거를 조합한다.
3. report staging 생성과 runtime 예약을 실제로 연결한다. 이번 개발 단계는 생성한 빈 파일의
   descriptor를 예약에 묶은 뒤에만 mutation guard를 풀고 SQLite 초기화·projection을 시작한다.
   아래 연결 계약은 전체 분류기나 원자적인 cross-writer snapshot의 증명이 아니다.
4. config revision·root identity·reservation·분류 측정의 일관성을 검증한 뒤 숫자 판정에 전달한다.
   관측 중 변경을 발견하면 유예한다. 새로운 writer 수명 규약 없이 단순 전후 숫자 일치만으로
   모든 중간 변경을 배제했다고 주장하지 않는다.
5. 다음 회귀를 통과한 후에만 P3 경로를 연결한다: 영 bytes unknown, 닮은 temp 이름,
   orphan snapshot, stale owner, 다른 root/nonce, alias/교체/사라짐, 과도한 entry 수,
   동시 render 성장, config 축소, current→retired 전환, 실패 후 예약 보존.

전체 예약 R은 이미 쓴 X만큼 할인하지 않는다. 명시적인 소유자 finalization에서만 그 소유자의
예약을 제외한다. 위 분류표 자체는 검증 체크리스트이며 새로운 파일 schema, 삭제 권한, write permit,
물리 quota 또는 분리 모드 활성화를 제공하지 않는다.

## 보고서 staging 연결 — v1.11 개발

- local-store는 `build_report_view_staging_bound`의 callback에 방금 생성한 빈 파일과
  descriptor를 전달한다. publication guard와 원래 descriptor는 build·publication 동안 유지한다.
- collector는 같은 root의 mutation guard를 쥔 상태로 runtime 예약에 연결하고, 성공한 뒤에만
  mutation guard를 풀어 수집과 projection이 병행되게 한다. callback 거부 시 SQLite를 열지 않는다.
- runtime은 owner nonce·metadata와 root/parent/file identity를 검사한다. 이름만 같은 파일,
  alias, 교체, 재연결을 승인하지 않는다. 게시 직전에도 같은 연결을 검증한다.
- 연결은 같은 예약 metadata의 v2에 원자 기록하며 기존 512바이트 상한을 유지한다. 기존 unbound v1 metadata는
  복구 호환성을 유지하지만 X 분류 근거로 승격하지 않는다. active/stale 예약 R은 줄이지 않는다.
- 실패로 파일이 교체됐다면 store의 Drop도 그 대체 파일을 삭제하지 않는다. stale 복구는
  identity 불일치를 숨기지 않으며 기존 guarded catalog 복구가 끝나야 예약을 해제한다.
- catalog directory 준비·기존 orphan 정리는 callback 전에 수행된다. journal과 다른 writer의
  소유권, 일관된 전체 측정, 실제 T/W/F admission은 별도 P2/P3 작업이며 아직 활성화하지 않는다.

## 파일 집계와 예약 증거 — 개발 중인 연결

- `storage_inventory::classify_storage`는 열린 descriptor를 소유 모듈의 검증 callback에
  전달하고, A/X/U의 실제 할당량을 집계한다. callback 오류는 집계 실패로 전달한다.
  빈 미분류 파일도 U entry로 남기며, callback은 파일명만으로 소유권을 승인하면 안 된다.
  이 원시 함수는 crate 내부 전용이며 결과 `StorageAllocationObservationV1`은 정책 입력과
  다른 타입이다. 정책 입력으로 자동 변환하거나 쓰기 허가로 승격하는 API는 제공하지 않는다.
- `reservation::ReportReservationEvidence`는 같은 mutation guard 아래 root·lock·metadata의
  descriptor와 예약 내용을 유지한다. capture는 lock이나 metadata를 생성하지 않으며,
  경로 identity 또는 예약 내용이 바뀌면 재검증에 실패한다.
- 예약 v2가 결합한 정확한 staging 파일만 X 후보가 된다. unbound v1은 X 소유권을
  증명하지 않는다. 실제 staging 할당량이 늘거나 owner가 종료돼도 전체 예약 R은 유지한다.
  `captured_reserved_bytes`는 캡처 당시 값이며 현재 유효성 확인을 대신하지 않는다.
- 이 증거와 분류기의 연결 회귀는 unbound→bound, 실제 파일 쓰기, owner 종료와 파일
  교체를 검사한다. 다른 파일의 A 소유권을 증명하는 테스트나 운영 admission은 아니다.
- 순회 전후 identity·metadata·directory entry 재검증은 관측된 변경을 거부하지만,
  모든 writer가 참여하는 동기화 없이 중간 변경이 전혀 없었다고 증명하지 않는다.
  따라서 이 API 결과만으로 분리 모드 쓰기를 허용하지 않는다.

## 동시 측정의 공통 잠금 — API 구현, 전체 writer 연결 전

### 현재 TLS generation의 제한된 소유권 증거

개발 중인 `CollectorTlsOwnershipEvidence`는 설정 v3가 정확히 참조하는 generation의
디렉터리와 CA·서버 인증서·서버 키만 대상으로 한다. 설정 및 credential은 기존 byte 상한
안에서 열린 private descriptor로 검증하고, 설정 bytes와 경로 identity를 전후 재검증한다.
credential bytes는 해당 증거의 제한된 수명 동안 메모리에만 유지하며 출력하지 않는다.
이 검사는 TLS 서버 설정 생성에 필요한 기존 parser 검증이며 새로운 인증서 chain 검증을
제공한다고 주장하지 않는다. v1/v2 legacy 및 참조되지 않은 generation은 이 증거로 승인하지 않는다.

호출자는 전체 capture·분류·재검증 동안 공통 freeze를 유지해야 한다. synthetic 통합 검사는
실제 생성한 TLS 파일을 A로 분류하되 미분류 sentinel과 전체 예약 R을 그대로 유지한다.
이 제한된 소유권 연결만으로 전체 P2 완료나 운영 admission을 활성화하지 않는다.

### private detail/status 소유권 연결 기준

다음 연결은 원문을 집계 결과에 포함하거나 전체 원문을 캐시에 복제하지 않는다.
공통 freeze 아래 scanner가 넘긴 descriptor 하나씩 기존 detail parser 및 status 계약으로
검증한다. 정확한 turn ID와 `<digest>.json` 경로의 일치, private 권한·단일 link·동일 named
identity를 함께 확인한다. 정상 파일명만으로는 승인하지 않는다. 내용이 없는 status/detail
짝도 각각 유효한 보관 artifact일 수 있으며, 짝이 없다고 삭제하거나 작업 공간으로 할인하지 않는다.

열린 descriptor 수는 state 및 두 parent와 현재 검사 파일로 제한하고, 파일별 기존 상한
(detail 64 KiB, status 1 KiB)과 디렉터리별 기존 1,024개 상한을 유지한다. scanner의 전체
entry 재검증과 소유 parent의 전후 identity 검증을 함께 사용한다. 이 관측은 새로운 영구
소유 manifest나 admission cache를 만들지 않는다. 전체 용량에서의 읽기·파싱 비용은 P5에서
측정하며, 성능 검증 없이 수집 foreground에 운영 연결하지 않는다.

### 공통 잠금 계약

기존 mutation guard는 보고서 projection, singleton metadata, integration writer를
모두 막지 않는다. 분류기에 report publication guard를 추가하면 보고서가 끝날 때까지
수집이 막히므로, 전체 writer가 참여하는 별도 `runtime/storage-accounting.lock`을 사용한다.
이 잠금은 설치·명시적 bootstrap에서만 생성하는 빈 private stable lock이며 A에 포함한다.
일반 측정은 없는 잠금을 생성하거나 손상된 잠금을 복구하지 않는다.

- 집계·admission: root mutation → exclusive accounting guard → 설정·예약·파일·기기 여유
  검증 → 정책 판정 → 해당 작업 commit/rollback까지 guard 유지.
- 보고서: 전체 작업의 publication guard는 유지하되, 제한된 SQLite 쓰기 transaction마다
  shared accounting guard를 획득하고 commit/rollback·journal 처리가 끝난 뒤 반환한다.
  반환 전 같은 permit의 identity를 다시 검증한다. 초기화와 명시적 폐기도 같은 규칙을
  따르며, 기존 작업 오류와 재검증 오류가 함께 발생하면 기존 작업 오류를 보존한다.
  집계와 수집은 보고서의 쓰기 묶음 사이에 진행한다.
- schema 초기화, repository 보정, 완료 metadata 쓰기도 같은 범위에 포함한다.
  staging 폐기는 permit을 확보한 명시적 경로로 수행하며, 확보하지 못하면 파일과 예약을
  남겨 기존 guarded recovery가 처리한다.
- singleton 생성·종료, 설정과 TLS·ownership snapshot, 정적 HTML, store-open repair와
  recovery 등 독립 writer도 참여하기 전에는 운영 분리 모드를 활성화하지 않는다.
- 집계에서 journal이 보이더라도 suffix만으로 X로 승인하지 않는다. 별도 journal 소유권
  증거가 없으면 U로 남기고 유예한다. 전체 R은 실제 X와 독립적으로 유지한다.

이것은 advisory writer 규약이다. 공통 guard API의 단위 테스트 통과만으로 전체 writer
적용이나 물리 디스크 quota를 증명하지 않는다. busy는 typed 유예로 처리하며, 잠금을 얻지
못했다고 이전 측정값으로 쓰기를 허용하지 않는다.

### 남은 쓰기 경로의 연결 기준

수집기는 다음 public 경계를 각각 확인한다. `_locked` 하위 함수가 다시 같은 잠금을
획득하게 하지 않고, 최상위 작업이 commit·rollback·임시 파일 처리까지 잠금을 유지한다.

| 경계 | 함께 보호할 작업 | 현재 확인할 사항 |
| --- | --- | --- |
| 설정 설치·migration commit/rollback·포트 복구 | collector 설정, TLS generation, migration 기록과 정리 | 외부 integration의 lifecycle 잠금과 root mutation을 구별하고 실제 caller별 중첩 획득 검사 |
| collector 시작 | 예약 복구, private detail 정리, store open/migration, catalog 복구, dirty marker | 기존 root mutation에 accounting freeze 연결 |
| lifecycle pass | 만료 raw detail 정리, DB tier migration, 보고서 무효화·placeholder·dirty marker | 기존 상한·삭제 선택을 유지하고 같은 작업 범위에서 postcheck |
| private detail 요청과 명시적 정리 | detail/status 쌍과 실패 정리 | canonical ingest 완료 후 별도 작업임을 보존하고 새로 guard 확보 |
| report authority watcher | dirty marker 생성·제거 | DB 상태 확인과 파일 변경 사이에 적절한 작업 잠금 확보; 기존 ingest 내부 helper에 중첩 획득 금지 |

완료 후 검증 오류를 저장 실패로 바꾸지 않는다. canonical commit이 확정되었으면 커서와
저장 완료 응답을 보존하고 건강 상태를 degraded로 표시한다. private detail은 별도 receipt로
실제 저장 여부를 알리며, canonical commit을 근거로 원문 저장 성공을 주장하지 않는다.
이 표는 연결·회귀 기준이며 각 경로가 이미 구현되었다는 선언이 아니다.

### Integration writer 연결과 남은 소유권 증거

`codex-integration`의 lifecycle 잠금은 동일 integration 작업을 직렬화하지만,
root accounting 측정을 막는 잠금은 아니다. `f8d2c12`는 다음 작업의 실제 쓰기 구간에
root mutation과 accounting 참여를 연결했다. 독립 코드·아키텍처 검토와 integration 71개,
UI 27개 테스트를 통과했지만, 아래 파일의 의미적 소유권을 집계에 연결하는 작업은 별개다.

- lifecycle 디렉터리와 잠금 파일의 최초 생성.
- `CodexConfigManager`의 connect/disconnect뿐 아니라 `ownership_status`의
  중단된 rotation 복구·snapshot 정리, `notify_ownership`의 상태 디렉터리 준비.
- LaunchAgent transaction의 생성·단계 갱신·복구·삭제.

연결된 구현은 실제 파일 변경 구간을 root mutation → accounting으로 감싸고,
완료·오류 모두에서 identity를 확인한다. collector 시작을 기다리는 health 확인이나
LaunchAgent 프로세스 대기까지 root mutation을 유지하면 시작 경로가 같은 잠금을
필요로 하므로, 외부 프로세스 대기와 파일 쓰기 범위를 분리해야 한다.
실제 사용자 Codex 설정과 LaunchAgent를 바꾸지 않는 fake lifecycle 회귀로 먼저 검증한다.
이 검증은 해당 writer 구간에 한정되며 분리 모드 활성화 선언이 아니다.

Codex config snapshot의 소유권 관측은 `5611cf5`에서 소유 모듈의 기존 snapshot 검증을
재사용하도록 구현·독립 검증됐다. 호출자가 전달한 정확한 root 및 config manager의 외부 config 경로에 결합하고,
private snapshot의 schema·phase·prior/connected/pending hash를 검사한다. 외부 config를
읽거나 복구하지 않으며 snapshot/status 메서드의 부수 효과를 집계에서 호출하지 않는다.
기존 snapshot byte 상한, parent/file descriptor identity와 bounded 재검증을 유지하고,
원문 snapshot bytes나 경로를 집계 결과·Debug로 노출하지 않는다. snapshot이 없으면 생성하지 않는다.

LaunchAgent 쪽 관측도 현재 프로세스나 외부 plist를 조사하지 않는다. 명시적인 root와
home 경계에서 기존 service-label 계산으로 기대 plist 경로를 만들고, 소유 transaction의
기존 schema·phase·file-state 검증을 재사용한다. `id`, `launchctl`, health 요청을 집계 중
실행하지 않으며, 유효한 복구 snapshot의 보관 소유권과 실제 외부 서비스 상태를 구분한다.
정확한 lifecycle 디렉터리 및 빈 private stable lock도 retained descriptor로 확인한다.
없는 경로는 생성하지 않으며, 운영체제가 해석하지 못하는 transaction을 이름만으로 승인하지 않는다.

`storage_coherence::StorageBarrier`의 독립 descriptor 기반 shared/exclusive 잠금과
config·예약 control 파일의 정확한 identity 검증은 독립 코드·아키텍처 리뷰를 통과했다.
`OwnedStorageFreezeGuard`는 root mutation과 accounting 잠금을 함께 소유하므로,
staging 연결 callback에서 두 잠금을 해제하고 게시 전에 새로 획득할 수 있다.
이 guard 자체는 예산 판정이나 파일 소유권 승인을 대신하지 않는다.

분리 모드 활성화 전의 legacy 호환 writer는 `open_if_initialized`로 이미 존재하는
잠금에 참여한다. 이 API는 원래 없던 잠금과 삭제된 잠금을 구분하지 못한다.
따라서 분리 모드의 저장·측정·실행 경로는 설정의 모드를 근거로 `open_existing`을
필수 사용해야 한다. 현재 설정 또는 저장 후보가 분리 모드일 때 잠금이 사라진 경우,
legacy로 우회하거나 자동 재생성하지 않는 회귀 검증을 활성화 조건에 포함한다.

합성 데이터 3세대 통합 테스트는 실제 report builder와 catalog를 사용한다. 매 쓰기 묶음
사이에 root mutation과 exclusive accounting 잠금을 다시 얻고, 쓰기 transaction 안에서는
exclusive accounting 획득이 거부되는 것을 확인한다. 설정·예약·게시 파일과 정확한 staging
외에 아직 소유 모듈 검증이 없는 파일은 U로 남긴다. 이 테스트는 설치된 사용자의 데이터,
동시 source mutation, 전체 writer coverage 또는 분리 모드 활성화 검증이 아니다.

추가 store observer는 생성·열기 시 유지한 directory/DB descriptor와 현재 schema 구조를
검증하고, 정확한 DB·projection·control lock만 분류 후보로 제공한다. SQLite 내부 file
descriptor를 얻었다고 주장하지 않으며, construction부터 observation callback 종료까지
외부 writer 동기화가 필요하다. 연결 회귀에서는 기존 store 후보 다섯 개를 이 observer로
검증하고, 의도적으로 넣은 0-byte 미소유 파일 하나는 U로 남기는지 검사한다.

### 정적 HTML 의미 검증

상태: 활성화 전 의미 검증 단계이며, 파일 소유권 또는 admission 완료를 뜻하지 않는다.

정적 HTML의 의미 검증은 `static-report`의 기존 renderer 계약을 재사용한다.
정확한 갱신 대기 placeholder 또는 한 개의 `ReportDtoV2`를 계약 검증한 뒤 기존
`write_rendered` 출력과 스트리밍 비교한 artifact만 인정한다. 입력은 기존 32 MiB
상한을 유지하며 두 번째 완성 HTML을 만들거나 raw HTML을 evidence에 보관하지 않는다.
현재 renderer와 다른 이전 template은 U로 남기고 활성화 전 재생성 필요 상태를
보고한다. 이를 이유로 admission 이전에 자동 재생성하지 않는다. 파일 descriptor와
동일성 검증은 별도 저장소 관측 경계이며, DTO 검증만으로 파일 소유권을 승인하지 않는다.

파일 관측 연결은 정확한 root와 `logs` 디렉터리, 고정 report 파일의 열린 descriptor 및
부재를 유지한다. 기존 no-follow/nonblocking·identity helper를 재사용하고 owner 경계에서
정확한 0700/0600·단일 link를 확인한다. 입력은 한 번의 bounded 읽기 동안만 유지하며
evidence에 원문을 캐시하지 않는다. 전후 할당·크기·권한·수정 metadata와 named identity를
재검증하고 caller는 공통 freeze를 유지한다. 이는 비협조적인 외부 writer에 대한 원자적
filesystem snapshot 보장이 아니다. 실제 32 MiB 입력의 파싱·메모리 비용은 P5에서 별도 검증한다.

### rollback journal과 복구 전 검사

SQLite rollback journal header에는 page 수·크기·checksum nonce 등이 있지만 DB inode나
파일 identity를 결합하는 필드는 없다. 따라서 header가 그럴듯하거나 DB와 page size가
같다는 사실을 journal 소유권 증명으로 사용하지 않는다.
근거: [SQLite rollback journal 형식](https://www.sqlite.org/fileformat.html#the_rollback_journal).

일반 SQLite 연결의 읽기조차 hot journal rollback을 먼저 수행하면서 DB와 journal을
변경할 수 있다. 그러므로 보통의 store open을 읽기 전용 accounting 검사라고 취급하거나,
U를 없애기 위해 admission 전에 자동 복구를 실행하면 안 된다.
근거: [SQLite hot journal 처리 순서](https://www.sqlite.org/lockingv3.html#dealing_with_hot_journals).
현재 분리 모드는 여전히 비활성이며, P3는 이 비쓰기 사전 검사·복구 필요 상태를 기존
U 거부 계약과 일치시켜 검증해야 한다. journal을 직접 삭제·이름 변경하거나, filename API,
header parser, unsafe/VFS 우회로 A/X를 만들어내는 것은 허용하지 않는다.

P3 사전 검사 준비에서는 기존 journal 파일 열기의 좁은 안전 경계도 검증한다.
검사와 open 사이에 FIFO나 다른 inode로 바뀌어도 무기한 대기하거나 대체 파일을 수용하지
않도록 기존 nonblocking/no-follow flag와 private file identity 검사를 재사용한다.
이 변경은 안전한 파일 열기일 뿐 SQLite 내부 journal descriptor를 얻거나 journal을
A/X로 승인하는 근거가 아니다. 정상 private journal·부재 동작은 유지하고, 바꿔치기와
hardlink에 대한 회귀를 먼저 고정한 뒤 적용한다.

분리 모드 사전 관측은 별도의 store evidence 모델을 만들지 않고 기존
`LocalStore::open_report_reader`와 `with_storage_ownership_observation`을 callback 수명으로
조합한다. 정확한 store 디렉터리가 없을 때만 부재로 관측하고 callback 이후에도 부재를
확인한다. 디렉터리는 있는데 DB나 필수 lock이 없으면 손상으로 거부한다. 디렉터리가 있으면
SQLite를 열기 전에 정확한 journal 경로의 부재를 확인하고, 빈 파일을 포함해 journal이
존재하면 typed journal-present 결과로 유예한다. callback 이후에도 journal 부재를 재확인한다.
복구·생성·정리·migration은 실행하지 않고, caller의 root mutation과 accounting freeze가
전체 관측을 감싸야 한다. composer는 검증된 root에서 도출한 정규화된 정확한 `state/store`
경로를 전달해야 하며, facade 자체는 임의 입력 경로를 runtime root에 결합하지 않는다.
부재 관측 자체는 신규 파일 생성이나 admission 허가가 아니다.

authority observation의 좁은 callback API로 기존 report-view
ownership 관측을 중첩한다. composer에 쓰기 가능한 `LocalStore`를 노출하지 않는다.
정확한 report-view 디렉터리 부재만 `None`으로 표현하고 callback 이후 재검증한다.
디렉터리가 있으면 기존 catalog/current/retired 의미 검증을 그대로 사용하며,
손상·교체·symlink·기타 오류를 부재로 바꾸지 않는다. 이 연결도 동일한 전체 writer
freeze 안에서 수행하며, 별도 캐시·복구·쓰기·정책 활성화를 추가하지 않는다.

게시된 report-view SQLite도 원본 DB와 같은 journal 사전 차단 원칙을 따른다.
정확한 snapshot의 `-journal` 항목이 있으면 내용이 비어 있어도 SQLite 연결을 열기 전에
거부하고 파일을 보존한다. query callback 이후와 ownership 최종 재검증에서도 부재를
확인한다. 정상적인 immutable snapshot은 복구를 요구하지 않아야 하며, 이 검사는
저널의 소유권을 인정하거나 복구·정리 권한을 부여하지 않는다.

수동 integration 작업의 root mutation 대기는 accounting 충돌 처리와 구분한다.
root의 짧은 쓰기와 경합하면 기존 foreground root-wait 경계에서 같은 검증된 lock을
기다릴 수 있지만, accounting exclusion은 한 번만 시도하고 실패 시 operation을 실행하지
않는다. 시작된 operation이나 실패 후 복구 전체를 재시도하지 않는다. 이 경계는 collector
foreground ingest나 hook의 대기 정책을 변경하지 않으며, 실제 `WouldBlock` 관측·한 번 실행·
accounting 거부·identity 교체 회귀로 검증됐다. 기존 root 대기 primitive에는 내부 timeout이
없으며 이를 bounded wait로 표현하지 않는다.

### 전체 소유권 조합의 연결 순서

CLI는 runtime, store, collector, integration, UI, static-report를 모두 의존하는 현재의
조합 위치다. private callback 함수에서 각 소유자의 검증을 수행하고 A/X/U, 전체 예약 R,
설정 revision만 전달한다. 쓰기 가능한 store나 owner handle은 결과로 반환하지 않는다.
동일 경로를 여러 소유자가 검사해도 모든 matcher를 실행한다. 앞선 승인으로 뒤의 오류를
가리는 short-circuit은 금지하며, A와 X가 동시에 참이면 불일치로 거부한다.

첫 연결은 `runtime-check`의 추가 진단이다. 분리 모드나 collector admission 활성화와
구분하고 기존 legacy 예산 결과를 새 숫자로 대체하지 않는다. 초기화된 accounting 경계는
root mutation 다음 exclusive freeze를 한 번씩만 획득한다. 이미 exclusive writer를 가진
상태에서 또 다른 descriptor로 freeze를 중첩하지 않는다. 기존 barrier나 mutation lock이
없으면 새 분류 검사 자체가 생성·복구하지 않는다. barrier 미초기화는 기존 명령과 출력을
그대로 유지하고 새 분류 필드를 내보내지 않는다.
Codex config/home 해석은 integration의 기존 resolver를 재사용하고 외부 config/plist를
읽거나 process·서비스를 조회하지 않는다. 이 조합과 진단 호출 경로는 `35cf297`에서
구현·독립 검증됐다. 실제 process 회귀는 수집된 fixture의 record 수와 config·보고서 bytes,
stable lock identity 보존 및 누락 lock 비생성을 확인한다. 실제 사용자 runtime 검증은 별도다.

`runtime-check` 전체가 읽기 전용인 것은 아니다. 기존 설치·singleton 및 store 준비는
생성·복구·migration 가능 동작을 유지하고, 같은 freeze를 유지한 상태에서 store를 닫은 뒤
새 관측을 수행한다. `accounting_stage=post_store_open`은 이 순서를 명시한다. journal 사전
검사 계약은 관측 함수의 SQLite open 경계에 적용되며, 이 명령 전체를 P3의 쓰기 전
admission 검사로 사용해서는 안 된다. 초기화된 경계의 owner 오류는 명령 실패이며
legacy 결과로 대체하지 않는다. 정상 관측의 U는 진단에 남기되 아직 legacy admission을
새 분리 정책으로 바꾸지는 않는다.

### P3 증분: 수집 commit 직전 검사 경계

collector가 integration이나 CLI를 의존하면 순환 의존성이 생긴다. `1cb4e28`은 collector가
소유하는 좁은 `CollectorIngestPrecommitGuard`와 명시적 실행 진입점으로 이 경계를 연결한다.
기존 `serve`와 CLI 기본 실행은 추가 검사를 사용하지 않아 legacy 동작·비용을 유지한다.
명시적 검사 경로만 이미 초기화된 accounting barrier를 요구하며 요청 중 생성하지 않는다.
같은 owned freeze를 검사부터 commit·실패 후 확인까지 유지하고, 기존 요청·batch·pressure·
예산 검사를 통과한 뒤 첫 durable mutation 직전에 한 번만 검사한다. 원문이나 쓰기 가능한
store를 port에 넘기지 않으며, callback 재시도·비동기 실행·관측 cache를 만들지 않는다.
이 port는 추가 precommit 조건이지 기존 admission의 대체물이 아니다. 수집 외 writer,
정확한 전체 예약·현재 filesystem 여유·작업량 산정 연결과 자원 검증 전에는 분리 모드를
활성화하지 않는다. 이 비활성 연결 경계는 독립 코드 APPROVE / 아키텍처 CLEAR를 받았다.
실제 검사 구현은 collector mutex와 freeze를 유지한 시간까지 자원 검증해야 하며,
trait 자체가 구현체의 I/O·대기·변경을 컴파일 시점에 금지하는 것은 아니다.

예약 수치 연결은 `1008153`에서 실제 확인된 전체 R을 그대로 받는 순수 계산 진입점을 추가했다.
전체 R만 아는 관측에서 `active=R, stale=0` 같은 상태를 만들어내지 않는다. 기존 active/stale
합산 API는 유지하고 한 개의 내부 계산을 공유한다. 기존 오류 우선순위와 합산 overflow를
회귀 테스트로 고정했고, 새 함수도 소유권·동시성·쓰기 권한이나 예약 해제를 증명하지
않는다. 독립 코드 APPROVE / 아키텍처 CLEAR 범위이며 운영 모드 활성화는 별도다.

filesystem 여유 D 관측은 `9c2dbe1`에서 기존 owned freeze가 소유한 정확한 root의 `fs2`
조회만 수행한다. 조회 전후 guard identity를 확인하고 I/O 실패를 0이나 이전 값으로
대체하지 않는다. 0은 실제 관측값으로 유지한다. 별도 잠금·cache·새 dependency를 만들지
않으며, 다른 앱의 디스크 사용을 잠그거나 미래의 여유를 보장하는 API로 표시하지 않는다.
원자적인 전체 기기 snapshot이나 쓰기 허가가 아닌 P3의 관측 경계다. 독립 코드 APPROVE /
아키텍처 CLEAR를 받았고, 조회의 WouldBlock·NotFound도 잠금 경합이나 barrier 누락이 아닌
조회 I/O 오류로 유지한다. 실제 admission 연결과 자원 검증은 남아 있다.

E 증분 `1f5e6f6`은 `control.rs`의 기존 `allocated_tree_bytes_strict(state/store) +
max_batch_bytes` 산정만 공통 함수로 분리했다. 기존 legacy 진단도 같은 함수를 사용해
E 산정 전용 `state/store` 순회를 한 번 수행한다. 이후 store를 포함하는 기존 전체 root
headroom 순회는 그대로 유지하며, 없는 store의 비생성·checked overflow·경로 오류 계약을 보존한다.
모드와 무관한 수치 산정 API이지 config 로딩의 분리 모드 차단을 우회하는 API가 아니다.
실제 collector guard 배선보다 먼저 기존 추정값의 동일성을 회귀로 검증했고 독립 코드
APPROVE / 아키텍처 CLEAR를 받았다. 집행 시 root mutation뿐 아니라 초기화된 exclusive
all-writer freeze를 모든 snapshot 입력·판정·commit/rollback 동안 유지해야 한다.

설정 관측 증분 `7e6c1d2`는 구조 검증과 운영 실행 허용을 구분했다. 기존
`ConfigAccountingEvidence`는 파일 identity·revision·유효한 정책을 읽는 관측이므로
schema가 유효한 분리 설정도 읽으며, 별도 공개 로더나 관측 생성자는 추가하지
않았다. 기존 private decoder를 공유하되 public `load`·save·config service·
`RuntimeControl`의 분리 모드 차단은 그대로 유지한다. 관측은 현재 정책만 노출하고 전체
설정 객체나 쓰기 권한을 전달하지 않는다. 분리 설정 관측 성공과 운영 차단, 설정 변경 후
revision 실패 및 기존 교체·권한·비생성 회귀를 함께 검증했고 독립 코드 APPROVE /
아키텍처 CLEAR를 받았다. 실제 guard 배선은 후속이다.

실제 수집 검사 연결의 선행 보강 `7cc52b5`는 설정 파일 읽기를 제한한다. 입력 상한은 64 KiB로,
기존 settings HTTP 요청 상한과 같은 규모이며 저장 예산 T/W/F와 무관하다. 설정은 고정된
필드와 유한한 숫자·enum으로 구성되며, 과도한 공백을 포함해 상한을 넘는 파일은 수정·절단
없이 명시적으로 거부한다. decoder는 최대 상한+1 byte만 읽어 파일 증가에도 제한을 유지한다.
열기는 기존 no-follow와 nonblocking flag를 함께 사용하고 열린 descriptor가 private
regular file인지 확인한 뒤 읽는다. FIFO를 먼저 blocking open하거나 사전 stat만 믿지 않는다.
정확한 byte 경계, 초과·읽는 중 증가, invalid UTF-8/JSON, FIFO의 유한 종료와 기존 권한·
교체·비생성 및 운영 모드 차단을 회귀 검증했고 독립 코드 APPROVE / 아키텍처 CLEAR를 받았다.
이는 설정 입력 한정의 자원 보강이며, 전체 소유권 관측이나 실제 guard의 지연 검증은 별도다.

실제 계산 조합은 `bac7bdd`에서 CLI의 기존 `storage_accounting.rs` 안에 비활성 private
`CliCollectorIngestPrecommitGuard`로 구현했다. 전달받은 owned freeze를 다시 획득하지 않고
동일 설정 revision·정책, A/X/U, 전체 R, 현재 D와 기존 E를 검증한 뒤 ingest 수치 판정을
수행한다. 수치 거부는 `Denied`, 관측·identity·revision 실패는 `Unavailable`로 구분한다.
callback의 허용 결과도 전체 관측의 마지막 재검증을 통과해야 반환한다. 명시적으로 전달된
batch 상한과 설정의 불일치도 거부하며 원문·경로·secret을 오류에 포함하지 않는다.
이 단계는 아직 `main.rs`나 `serve`에 연결하지 않는다. 자원 보강과 테스트를 먼저 통과하고
다른 작업 경계 및 모드 전환 검증을 마치기 전까지 좁게 설명된 dead-code 허용으로
비활성임을 드러낸다. 별도 로더·범용 service·새 dependency는 추가하지 않는다.
고정본 `554588bc`의 독립 코드 APPROVE / 아키텍처 CLEAR와 소유권·판정 테스트 19개,
CLI strict Clippy 및 Rust 1.97.0 formatting을 확인했다. 이는 실제 수집 연결이나 전체
관측 지연의 적합성을 검증한 결과가 아니며 다른 쓰기 경로와 운영 차단은 그대로다.

보고서 연결의 첫 준비 `7a7eee6`은 기존 headroom에서 publication·metadata 여유를 빼고
기존 build ceiling으로 제한하는 수치 계산만 private 순수 함수로 분리했다. 기존 caller의
오류·포화 뺄셈·상한은 바꾸지 않았고, 경계값 회귀와 독립 코드/아키텍처 검토를 통과했다.
보고서 시작·게시 직전 admission, 검증된 자기 예약 제외, 복구·정리 경로 연결은 아직 남아 있다.

자원 진단 `faca3c4`는 실제 설치 대신 synthetic current store와 private detail/status 각 1,024개를
별도 fixture에 만들고, 최대 64 KiB detail을 포함한 전체 비활성 guard 호출 시간을 측정한다.
fixture 준비는 측정 구간 밖에 두고 inventory 한도 초과의 명시적 거부도 확인한다. 이 진단은
기본 테스트에서 제외된 명시적 실행이며 출력은 숫자·결과 코드로 제한한다. 새 latency SLO나
임의의 통과 임계값을 만들지 않는다. 소수의 로컬 표본은 전체 collector의 CPU/RSS·동시성·
foreground 응답 기준이나 실제 규모 검증을 대체하지 않으며, 결과만으로 정책을 활성화하지 않는다.
후속 `633ac75`는 파일 수·status 크기를 유지하고 detail 내용 크기만 바꾼 비교 fixture를
추가했다. 두 변경은 독립 코드·아키텍처 리뷰와 명시적 릴리즈 테스트를 통과했다. 최적화 빌드의
표본은 [검토 기록](reviews/v1.11.0.md)에 남기며, 내용 파싱이 지배적이라는 가정이나 새 SLO의
근거로 삼지 않는다. 현재 store는 빈 최신 schema이며, 실제 규모·보고서 catalog·동시성은 별도다.

### 동시 연결의 설정 준비 경합 수정 계획

CI `34314891405`의 storage-busy 분기와 같은 오류를, lifecycle 진입 후 root mutation을
다른 thread가 보유한 상태의 `install_settings`에서 재현했다. legacy·initialized 환경 모두
설정/TLS bytes는 보존된다. 새 CI root에는 accounting barrier를 생성하는 운영 경로가 없으므로
이 root 경합을 우선 수정하되, artifact가 정확한 호출 단계를 보존하지 않아 역사적 CI 원인이
확정됐다고 표현하지 않는다.

독립 아키텍처 검토를 통과한 변경 범위는 foreground `connect`의 첫 설정 준비뿐이다.
별도 `install_settings_waiting_for_root`가 기존 설정 본문과 마지막 재검증을 공유하고,
기존 `StorageMutationWriter::acquire_waiting_for_root`를 사용한다. 기본 `install_settings`의
try-only 동작은 유지한다. root를 얻은 뒤 accounting은 한 번만 시도하고, 작업은 한 번만
실행한다. 새 deadline·전체 작업 재시도·예산 예외·자동 barrier 생성은 추가하지 않는다.
두 환경에서 기다린 뒤 한 번 성공하는 회귀, 기존 즉시 거부, accounting 경합의 즉시 거부,
설정/TLS 보존 및 기존 postcheck·primary-error 보존을 검증한 뒤 통합한다.
