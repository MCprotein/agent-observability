# 저장 예산 P0 결정안

상태: **독립 설계 검토 APPROVE — config P1 검증 완료, 실제 정책 활성화 전.** [전체 계획](STORAGE_BUDGET_POLICY.md)의 P0를
구체화한다. 아래 숫자는 제품의 초기 운영 선택이지 물리 쓰기량의 검증된 상한이 아니다.

## 기본값과 호환성

| 항목 | 초기 선택 | 허용 범위와 이유 |
| --- | --- | --- |
| 신규 설치·기존 설정의 기본 모드 | `legacy` | 릴리즈 중 묵시적인 정책 완화 없음 |
| 명시적 분리 모드의 보관 목표 T | 1 GiB | 256 MiB–20 GiB; 기존 설정 범위와 동일한 관리 규모부터 시작 |
| 명시적 분리 모드의 작업 예산 W | 1 GiB | 256 MiB–20 GiB; T와 독립적으로 설정, 비율 자동 증액 없음 |
| 기기 여유 F | 1 GiB | 256 MiB–20 GiB; 작은 디스크에서도 0으로 비활성화하지 않음 |

모든 값은 정수 bytes이며 범위의 양 끝을 포함한다. GiB는 2^30 bytes다. W/F의 범위와
기본값은 제한된 첫 릴리즈의 운영 선택이다. 모든 기기나 장기 사용에 충분하다는 권장이 아니다.
실제 규모 회귀의 store 할당량 689,463,296 bytes와 기존 ingest 예약 689,987,584 bytes는
각각 1 GiB 미만이다. 이 근거로 두 양을 분리하는 초기 조합을 선택했으며, 전체 설치 파일과
동시 보고서 예약을 포함하는 P5 실측으로 적합성을 다시 검증한다.
근거: [원래 페이지 크기 검사](STAGED_INGEST.md#original-page-size-admission-check--september-8).

후속 config는 v5로 하고 `storage_budget`에 `mode`, `retained_target_bytes`,
`workspace_budget_bytes`, `minimum_free_bytes`를 필수로 둔다. mode는 `legacy|separated`만
허용한다. 기존 `collection.local_storage_budget_bytes`는 보존한다. legacy에서는 새 세 값이
비활성이고 separated에서는 기존 값이 비활성이다. 비활성 값도 유효성 검사와 재저장으로 보존한다.
이는 새 config 계약의 결정안이며 현재 사용 가능한 옵션이 아니다.

- v1–v4 입력은 기존 migration 결과에 legacy 모드와 새 필드 초기값만 붙인다. 삭제 설정 보존.
- 전환은 영향 설명과 revision 비교 후 명시적으로 저장한다. 자동 전환·데이터 재작성 없음.
- `storage-bytes`는 legacy 값만 변경한다. separated에서 실행하면 모드 불일치 오류를 내며
  T로 해석하지 않는다. 복귀 시 저장했던 legacy 값을 그대로 사용한다.
- 구버전 실행 파일은 v5를 거부하는 것이 정상이다. “모드 복귀”는 구버전 바이너리로의
  downgrade와 다르며 자동 v4 역변환이나 저장 schema downgrade를 제공하지 않는다.
- 저장할 때 부족한 공간을 이유로 값을 몰래 올리지 않는다. 적용 후 유예될 수 있음을 표시한다.

P1 strict 계약 근거: `crates/local-runtime/src/config.rs:33`,
`contracts/local-runtime-config-v5.schema.json:7`; 기존 범위는 `storage.rs:23`.

## 파일 분류와 공간 계산

| 분류 | 포함 대상 | 처리 |
| --- | --- | --- |
| A: 보관 할당량 | authority DB와 내부 모든 tier/index, JSONL, current/retired snapshot, catalog, private detail, archive, 로그, config/lock/예약 metadata | T와 비교; freelist도 할당량에 포함 |
| X: 작업 할당량 | 검증된 소유 작업의 journal, report staging, 원자 교체 temp, migration scratch | W에 포함; 보관량과 중복 합산하지 않음 |
| U: 미분류 | 소유권·수명·분류를 검증하지 못한 관리 파일 | 실제 총량에 포함하고 새 쓰기 거부; 확장자만으로 삭제하거나 X로 할인하지 않음 |

분류는 runtime root 내 기존 경로·소유권 검사에 따른 allowlist다. 이름이 staging처럼 보여도
소유권 확인이 안 되면 U다. 불일치·symlink·순회 한도 초과는 기존 fail-closed 동작을 유지한다.
현재 순회 근거: `crates/local-runtime/src/storage.rs:88`; snapshot 소유/경로 검사는
`crates/local-store/src/report_view_catalog.rs:238`.

추가 변수: R은 다른 작업의 **전체 미해제 예약**, E는 시작할 작업의 추가 공간 allowance,
D는 현재 filesystem free bytes다. 최초 구현은 staging이 커졌다는 이유로 R에서 그 크기를
빼지 않는다. 따라서 X와 R의 보수적 중첩 심사는 허용하지만 실제 사용량 표시는 A+X+U로
한 번만 센다. 이렇게 남는 과예약은 안전한 최적화의 후속 대상이지 이중 대여 근거가 아니다.

신규 작업의 시작 조건은 다음을 모두 만족하는 것이다.

1. 설정 revision·mutation guard·파일 분류가 유효하고 U=0.
2. `X + R + E <= W`.
3. `F + R + E <= D`. X는 이미 free에서 빠졌으므로 여기서 다시 빼지 않는다.
4. 신규 ingest/import/private capture는 `A < T`. T는 목표이므로 commit 후 T를 넘을 수 있고,
   그 다음 작업을 유예한다. T를 물리 상한으로 광고하지 않는다.

모든 덧셈은 checked arithmetic이며 overflow는 거부다. W/F 조건에서 같음은 허용한다.
cleanup/recovery/report는 A>=T만으로 차단하지 않지만 1–3과 각 기존 권한·보존 조건을 따른다.
자기 예약의 finalization에서는 검증된 그 예약만 R에서 제외하고 기존 ceiling과 최신 설정을
함께 적용한다. 축소 설정을 이유로 기존 예약을 다른 작업에 빌려주지 않는다.
현재 소유자 검사 근거: `crates/local-runtime/src/control.rs:174`.

### P2/P3 연결 시 확인할 경계

순수 수치 판정의 허용 결과는 파일 소유권, 잠금 또는 설정 revision 검증을 대신하지 않는다.
미분류 파일은 할당 bytes가 0이어도 거부하므로 U bytes와 미분류 entry 수를 함께 검사한다.

| 경계 | 현재 근거 | 연결 전 조건 |
| --- | --- | --- |
| 보고서 예약 | `crates/local-runtime/src/reservation.rs:209`, `:287` | active/stale 모두 전체 ceiling 유지; 자기 예약 제외는 실제 handle·동일 root guard·metadata 검증 후만 허용 |
| 보고서 작업 파일 | `crates/local-store/src/report_view.rs:162`, `crates/local-collector/src/lib.rs:3763` | builder가 mutation guard 밖에서 쓰므로 단순 경로 순회는 원자 snapshot이 아님; staging handle/publication guard와 경로 identity를 연결 |
| 임시 파일 분류 | `crates/local-runtime/src/reservation.rs:29`, `crates/local-store/src/report_view_catalog.rs:915` | 예약 nonce/ceiling만으로 특정 staging 파일 소유를 증명하지 않음; cleanup용 prefix 인식도 일반 admission 소유권 증명이 아님 |
| 수동 import | `crates/cli/src/main.rs:1313`, `:1332` | 항목별 write 및 최종 projection의 비용 포함; parsing만 확인하고 전체 파일을 원자 batch로 표시하지 않음 |

이 경계가 검증되기 전에는 숫자 계산만 통과했다고 분리 모드 실행 차단을 제거하지 않는다.

## 작업별 allowance와 책임

숫자 축소 최적화는 이번 정책 분리와 분리한다. 기존 산정치도 검증 범위를 벗어나면 hard bound가 아니다.

| 작업 | 첫 구현의 E | 소유 책임·근거 |
| --- | --- | --- |
| 자동 ingest 및 수동 canonical import | 기존 `allocated(state/store) + max_batch_bytes`를 하한으로 유지; 해당 경로의 기존 allowance가 더 크면 큰 값 사용 | runtime의 파일 총량 계산, local-store의 SQLite 검증; `control.rs:57`, collector `lib.rs:2157` |
| 보고서 | 가용 작업 공간에서 publication/metadata allowance를 먼저 제외한 기존 bounded build ceiling + publication + metadata | collector 조합, local-store builder; collector `lib.rs:3746`, `:3913` |
| DB lifecycle | 기존 `page_count * page_size + max_archive_bytes + 2 MiB` | local-store `lifecycle.rs:249`; collector `lib.rs:2402`에서 별도 admission |
| store migration | 현재 schema별 `migration_required_workspace`와 preflight를 그대로 사용 | local-store `lib.rs:3668`; 미지원 schema는 거부 |
| private detail·archive·파일 정리 | 기존 경로의 bounded output/atomic replacement allowance 유지; 전체 작업에 SQL이 있으면 SQL allowance도 합산 | runtime/collector의 해당 파일 작업 주체; collector `lib.rs:3022` |

경로에 산정치가 없으면 0이나 batch 크기로 대체하지 않고 `estimate_unavailable`로 유예한다.
manual import는 parsing 제한과 observation/disposition별 원자 기록 계약을 유지하며 추가
output/projection도 합산한다. 현재 CLI는 항목별 transaction 뒤 projection을 재생성하므로
import 파일 전체를 하나의 원자 batch로 취급하지 않는다 (`crates/cli/src/main.rs:1332`).
P3의 경로별 테스트가 없는 작업은 연결 완료로 취급하지 않는다. 큰 correlation, rehydration,
topology, pruning은 작은 입력과 무관하게 비용이 커질 수 있어 기존 전체-store allowance를
제거하지 않는다. 기존 SQLite journal/cache/sync 설정도 변경하지 않는다.

W 초과 관측은 후속 신규 작업 중단과 degraded 상태의 근거이지 강제 중단 명령이 아니다.
동일 config revision에서 자동 증액·무한 재시도하지 않는다. guarded recovery 및 재계산으로
현재 파일/예약이 정합적이고 모든 시작 조건을 다시 만족하면 재개 가능하며, 원인·마지막
초과 상태는 진단에 남긴다. 실제 ENOSPC/EIO와 추정 초과는 서로 다른 이유로 표시한다.

## 결정적 검증 벡터

이 표는 MiB 단위 계산 fixture이며 실제 SQLite 쓰기량 시험이 아니다. 별도 표기가 없으면
T=W=F=1024, U=0이고 신규 ingest를 심사한다.

| A | X | R | E | D | 결과 |
| ---: | ---: | ---: | ---: | ---: | --- |
| 700 | 0 | 0 | 660 | 2048 | 허용: 보관량을 W에서 다시 빼지 않음 |
| 700 | 64 | 128 | 832 | 1984 | 허용: W/F 두 경계에서 동일 |
| 700 | 64 | 128 | 833 | 4096 | workspace 부족 |
| 700 | 0 | 128 | 660 | 1811 | filesystem floor 부족 |
| 1024 | 0 | 0 | 64 | 2048 | retention 유예, 삭제 없음 |
| 1200 | 0 | 0 | 64 | 2048 | 승인된 cleanup이면 허용, ingest는 유예 |

P1은 config migration/invalid enum/unknown field/범위 경계/revision fixture를 제공한다.
P2는 위 표와 overflow, U, stale reservation, 자기 예약 finalization을 Rust 단위 테스트로 옮긴다.
P3–P5가 통과하기 전에는 실제 설치 수집 복구나 기본값 적합성을 주장하지 않는다.

## 독립 검토 범위

2026-09-08 architect 검토는 config P1 착수에 한해 CLEAR다. 구체적 파일 분류
manifest와 report E의 실제 연결은 P2/P3 WATCH 항목으로 남는다. 해당 경로의 근거가
없을 때 `estimate_unavailable`을 반환하는 규칙을 유지하며, 이 승인을 P2/P3 구현 완료,
실제 설치 변경, PR 병합 또는 릴리즈 승인으로 확대하지 않는다.
