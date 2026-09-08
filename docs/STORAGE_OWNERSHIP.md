# 저장 파일 소유권과 분류 — P2 구현 기준

상태: **개발 중인 설계 기준. 분류기·동시 측정·분리 모드 실행은 아직 구현되지 않았다.**
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
