# 저장 예산 분리 계획

상태: **v1.11.0 개발 — P1, P2 수치 계산·기초 파일 집계·공통 잠금 검증 완료. 전체 writer 연동·분리 모드 운영 적용 전.**

2026-09-08 사용자 결정: 보관 예산, 작업용 공간, 기기 디스크 비상 여유를 분리한다.
`unsafe`/FFI 예외는 허용하지 않는다. 이 문서는 새 정책의 설계 기준이며, 현재 동작을
설명하는 [Configuration](CONFIGURATION.md#저장-공간-예산)을 대체하지 않는다.

## 요구사항과 비목표

- 불필요한 수집 거부를 줄이되 기존 데이터, 원자적 commit, 재시도와 privacy를 보존한다.
- 일시적인 작업 파일을 평상시 보관량과 구분하고, 중단 이유를 CLI와 설정 화면에 표시한다.
- 기존 `storage-bytes` 값을 업그레이드 중 보관 목표로 몰래 재해석하지 않는다.
- 자동 삭제는 별도 opt-in이다. 예산 변경만으로 미만료 데이터를 지우지 않는다.
- Rust/TypeScript, 기존 SQLite transaction과 안전한 API를 유지한다. 새 VFS, 자체 복구 엔진,
  native shim, OS quota 강제 설정, Docker, team 기능, 저장 데이터 schema 변경은 이 계획 밖이다.
- **이번 정책은 순간적인 물리 할당량의 hard quota를 약속하지 않는다.** 실제 quota 강제와
  작업 시작을 결정하는 admission을 구분한다. 이 구분을 UI에서도 숨기지 않는다.

## 현재 근거

| 확인한 사실 | 코드·문서 근거 |
| --- | --- |
| 기존 예산은 여러 파티션과 안전 여유를 포함한다 | `crates/local-runtime/src/storage.rs:13`, `:23`, `:155` |
| 수집 예약은 전체 store 할당량 + 최대 batch의 수치상 사전 검사다 | `crates/local-runtime/src/control.rs:57` |
| 수집은 외부 mutation guard 아래에서 검사부터 commit까지 진행한다 | `crates/local-collector/src/lib.rs:2157`, `:2205` |
| report에는 별도의 활성·중단된 durable 예약이 있다 | `crates/local-runtime/src/control.rs:142`, `:174` |
| observation/disposition/correlation은 하나의 transaction으로 반영된다 | `crates/local-store/src/lib.rs:914`; cursor 갱신은 `crates/local-collector/src/lib.rs:2302` |
| P1 config v5는 strict schema이며 v1–v4 값을 보존한다; UI도 v5를 검증한다 | `contracts/local-runtime-config-v5.schema.json:7`, `crates/local-runtime/src/config.rs:33`, `ui/settings/config-validation.ts:1` |
| 원래 페이지 크기의 복사본에서도 authority-only proxy가 부족했다 | [Staged Ingest](STAGED_INGEST.md#original-page-size-admission-check--september-8) |

마지막 측정은 현재 hard-budget 정책의 실패 근거이지 새 정책의 성공 근거가 아니다.
잠금이 없다는 진단이나 모든 저장 오류가 반드시 rollback됐다는 가정을 사용하지 않는다.

## 정책 모델

아래는 정책 용어다. P1은 대응하는 v5 config 필드를 제공하지만 분리 모드 실행이나
CLI/웹 활성화 옵션은 제공하지 않는다.
P2의 `crates/local-runtime/src/storage_policy.rs`는 수치만 판정한다. 파일 소유권이나
동시 측정의 정합성을 증명하지 않고, 어떤 실제 작업 경로도 아직 이 결과로 쓰기를 허용하지 않는다.
기본값·분류·산정식·전환의 구체적인 결정안은 [P0 결정안](STORAGE_BUDGET_P0.md)에 있다.
결정안은 독립 검토에서 config P1 착수에 한해 승인됐다. P2/P3 파일 분류·report 연결과
실제 규모 수용성은 별도 검증 대상이다.

[파일 소유권 기준](STORAGE_OWNERSHIP.md)은 P2의 경로별 근거와 아직 필요한 연결을 정리한다.
파일 이름 목록 자체를 소유권 검증이나 구현 완료로 취급하지 않는다.
생성 직후 빈 report staging descriptor와 runtime 예약의 연결은 구현·검증됐다.
다음 단계는 소유권 근거에 따른 A/X/U 분류와 전체 writer의 일관된 측정이다.
SQLite 쓰기 전 연결과 게시 전 재검증은 전체 파일 분류·일관된 측정·분리 모드 활성화와 구분한다.

| 항목 | 의미 | 부족하거나 초과했을 때 |
| --- | --- | --- |
| 보관 목표 T | 평상시 유지할 관리 데이터·인덱스·현재/retired 보고서·private detail 등의 목표량 | 허용된 정리 수행; 정리 불가 시 이유 표시 및 신규 ingest 유예 |
| 작업 예산 W | 동시에 시작할 작업의 추가 공간을 심사하는 명시적인 운영 예산 | 신규 작업 시작 거부 또는 유예; 무제한 자동 증액 금지 |
| 기기 최소 여유 F | 앱 외의 작업과 복구를 위해 남겨야 할 파일 시스템 여유의 시작 조건 | 새 작업 유예; 실제 디스크 부족과 앱 정책 부족을 별도 표시 |

W는 파일을 미리 할당하는 기능도, SQLite의 매번 쓰기를 W에서 차단하는 기능도 아니다.
따라서 **T+W를 순간 최대 디스크 사용량이라고 표시하지 않는다.** T는 목표이며 디스크의
물리적 남은 공간은 다른 앱 때문에 검사 직후에도 변할 수 있다. F도 기기 전체의 강제 quota가 아니다.

### accounting과 실행 규칙

1. 파일 분류표를 먼저 확정한다. 보관/작업/미분류의 모든 관리 파일을 한 번씩 실제 할당량에
   포함한다. SQLite 내부 freelist는 파일 시스템에서 반환된 공간으로 계산하지 않는다.
2. 이미 기록된 작업 파일과 아직 쓰지 않은 예약 약속을 구분한다. 활성·중단된 report 예약,
   catalog 교체, 이전 snapshot, migration 및 cleanup 자체의 공간을 누락하거나 이중 대여하지 않는다.
   검증된 release/recovery 전에는 중단된 예약을 만료 시간만 보고 해제하지 않는다.
3. runtime이 같은 guard 아래 설정 revision, 실제 사용량, filesystem free, 예약을 재검사한다.
   local-store가 SQLite별 예상 작업량을 제공하며 UI/domain이 journal 계산을 소유하지 않는다.
4. 작업량 산정은 ingest/report/maintenance/migration/manual import별로 구분한다. 검증된 상한과
   운영상 추정치를 구별한다. 추정치를 사용하는 경로는 그 한계와 초과 처리 규칙을 명시해야 하며,
   입력 byte 수만으로 journal 전체를 예측하거나 실측 p95를 hard bound라고 부르지 않는다.
5. T를 넘었을 때 lifecycle이 켜져 있고 만료 조건을 만족한 데이터만 bounded cleanup한다.
   삭제 off, pinned/oversized trace, 미만료 데이터뿐인 경우 신규 ingest를 유예한다. 복구·허용된
   정리는 T 초과만으로 영구 차단하지 않되 그 작업 자체도 공간 검사를 통과해야 한다.
6. 작업이 W 추정을 넘으면 상태를 degraded로 표시하고 후속 신규 작업을 중단한다. 감시는
   보조 수단이며 이미 발생한 초과를 막았다고 주장하지 않는다. 진행 중 transaction은 기존
   SQLite 오류·rollback 경로를 따르며 journal을 직접 삭제하거나 강제 종료로 공간을 회수하지 않는다.
7. commit 결과가 불명확한 재시작/응답 유실은 durable cursor와 identity로 재조정한다.
   실패 응답만 보고 미반영으로 단정하거나 성공 전에 in-memory cursor를 진행하지 않는다.

## 호환성과 설정

- 새 versioned config에는 기존 의미를 유지하는 모드와 예산 분리 모드를 명시한다.
  schema version/key/범위의 P0 결정안을 검토한 뒤 P1에서 contract fixture로 고정한다.
- 기존 v1–v4 설정 migration은 기존 모드, 값, 삭제 off/on 선택을 보존한다. `storage-bytes`
  명령을 새 보관 목표로 조용히 바꾸지 않는다. 신규 정책 전환은 CLI/UI에서 영향 설명 후 명시적으로 저장한다.
- P0 결정안은 새 설치에도 legacy를 기본으로 유지하고, 명시적 분리 모드에는 T/W/F 각
  1 GiB를 초기 운영값으로 제안한다. 작은 디스크·낮은 예산 fixture 및 독립 검토를 거쳐
  확정하며, 실제 규모 수용성은 별도 P5 검증이다. 숫자 선택을 실측 상한으로 표시하지 않는다.
- 모드 전환은 설정 변경이지 즉시 데이터 migration/삭제 명령이 아니다. 이전 모드 복귀 시
  용량이 부족하면 기존 데이터를 보존하고 수집 유예를 표시한다. 다운그레이드 호환성도 fixture로 검증한다.
- 실행 중 설정 축소는 새 작업에 적용한다. 이미 진행 중인 예약을 무효화해 공간을 다시 빌려주지 않는다.

## 구현 순서

| 단계 | 변경 위치 | 완료 조건 |
| --- | --- | --- |
| P0 정책·수치 확정 | 이 문서, `docs/CONFIGURATION.md`, config fixtures | 기본값/범위, 파일 분류표, 작업별 산정식, 보수 추정의 한계, 모드 전환과 초과 시 상태를 독립 검토. 미확정 결정이 남으면 P1 금지 |
| P1 계약·마이그레이션 | `crates/local-runtime/src/config.rs:33`, `contracts/local-runtime-config-v5.schema.json:7`, `ui/settings/generated/` | v1–v4 보존, strict unknown/bounds 검사, Rust/TS parity, 명시적 전환 fixture |
| P2 예산 계산·상태 | `crates/local-runtime/src/storage.rs:13`, `control.rs:57`, `:142` | legacy 결과 불변; 분리 모드의 T/W/F, active/stale 예약, overrun/저장 부족 상태를 결정적 테스트로 검증 |
| P3 작업 연결 | `crates/local-collector/src/lib.rs:2157`, `:2205`, `:2302`; `crates/local-store/src/lib.rs:914` | 단일 guard·transaction 유지; 수집/보고서/정리/수동 import/migration 각각 accounting 검증; journal/동기화 설정과 unsafe 금지 유지 |
| P4 CLI·웹 설정 | `crates/cli/`, `crates/local-ui/`, `ui/settings/main.ts:129`, `DESIGN.md:210` | 목표/사용량/작업 예상/예약/기기 여유/중단 이유 구분, 동시 수정 충돌, 재시작 후 보존, 실제 설정 revision 일치 |
| P5 수용성·문서 | `crates/local-collector/src/rotation_diagnostic.rs:277`, `:570`, `xtask/`, docs | 아래 검증 표 통과, 독립 리뷰, 정확한 revision 성능·플랫폼 CI, Chrome QA 후에만 release 판단 |

P1의 schema/검증기 parity는 확정된 P0 필드 계약으로 병렬 검증한다. 새 정책을 조작하는
UI/작업 연결은 P1과 P2의 contract가 확정된 후에만 disjoint ownership으로 병렬화한다.
P0는 현재 버그가 자동으로 해결됐다는 선언이 아니다. 실제 규모에서 적절한 T/W/F 조합으로
수집·보고서·정리가 반복되지 못하면 그 원인을 수정하고, 예약 숫자만 낮춰 성공 처리하지 않는다.

## 검증과 완료 기준

| 검사 | 기대 결과 |
| --- | --- |
| 기존 설정 upgrade/reopen | 값과 admission 결과 보존, 자동 삭제나 정책 전환 없음 |
| 새 정책 switch/back, concurrent config save | revision 충돌 거부, 중간 설정 없음, 데이터 보존 |
| T/W/F 각각 경계값 및 overflow | 경계 직전/동일/직후가 명세의 상태와 정확히 일치 |
| T 초과 + 삭제 off 또는 모두 미만료 | 데이터 삭제 없음, 명확한 유예 이유, tight retry loop 없음 |
| T 초과 + 허용된 정리 | 정리 자체 공간을 검사하고 만료 대상만 처리, 부족하면 안전한 유예 |
| report 활성/중단 예약 + ingest | 같은 여유 공간 이중 대여 없음, 검증 전 stale 예약 해제 없음 |
| 추정 초과와 OS ENOSPC/EIO | 초과 상태와 신규 작업 유예, 기존 오류 처리·재시작 복구, cursor/identity 정합성 |
| commit 전 종료 / commit 후 응답 유실 / 중복 재시도 | 재개 후 부분 batch나 중복 durable observation 없음; source cursor 일치 |
| 큰 correlation, rehydration, topology fan-out, ledger pruning | 작업량 산정·성능 범위 포함; 표본을 최대치 보장으로 보고하지 않음 |
| 실제 규모 private copy, current+retired views | 원본 불변, 새 ingest→publish 3세대, 정확한 count, 추가 동시 정리 fixture, cleanup 확인 |
| CLI/Chrome 설정 QA | 저장 후 실제 admission이 같은 revision 사용, 중단 이유/재개 표시, 기존 그룹 탭 재사용 |
| 자원·privacy | 기존 CPU/RSS/foreground latency 기준 유지; 내용·경로·secret이 새 상태/로그에 유출되지 않음 |

관측해야 할 지표는 정상 보관량, 작업 peak(표본임을 표시), 예상/실제 차이, 회수된 물리 bytes,
수집 거부/유예, 재시도, CPU/RSS와 원본 불변 여부다. fault fixture와 독립 리뷰를 함께 사용한다.
장시간 성능 기준을 새 정책 때문에 묵시적으로 완화하지 않는다.

## 위험과 대응

- 정책을 hard quota로 오해: 설정·문서에 시작 조건/목표/실제 강제 한도의 차이를 표시한다.
- 정리를 켜지 않아 목표로 복귀 불가: 명확한 유예와 사용자 선택 제공, 임의 삭제 금지.
- 누적 temp/중단 예약: 기존 guarded recovery와 파일 분류 검증, TTL만으로 해제 금지.
- 거대한 단일 작업: P0에서 fallback/유예 정의, 원자성을 깨는 batch 분할이나 무제한 증액 금지.
- 신뢰성보다 구현 복잡도가 커짐: safe Rust와 기존 SQLite API 안에서 단계별 diff와 독립 리뷰;
  schema 변경·추가 dependency가 필요하면 별도 판단하며 unsafe 예외로 우회하지 않는다.

## 문서와 릴리즈 경계

[Roadmap](../ROADMAP.md#v1110--local-storage-lifecycle-and-paged-dashboard-in-progress)의
v1.11.0 하위 계획으로 추적한다. 새 버전 train이나 major line을 임의로 열지 않는다.
구현 시 `CONFIGURATION`, `LOCAL_RUNTIME`, `ARCHITECTURE`, `STORAGE_LIFECYCLE`, `DESIGN`,
README와 review checkpoint를 같은 실제 상태로 갱신한다. 현재 안정판 안내는 그대로 두고
개발 문서에서 P1 설정 계약과 아직 비활성인 admission 정책을 구분한다.

[Staged Ingest](STAGED_INGEST.md)와 [Isolated Ingest](ISOLATED_INGEST.md)의 과거 strict-quota
연구는 근거로 보존한다. 새 정책은 그 VFS 실험을 재개하지 않으며 기존 모드를 자동 완화하지 않는다.
PR → 독립 리뷰 → 정확한 revision 검증 → merge → release → 병합 branch 정리 순서를 유지한다.
계획 완료, 구현 완료, 실제 설치 수용성, 릴리즈 완료를 별개로 보고한다.
