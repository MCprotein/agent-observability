# Roadmap

이 문서는 agent-observability의 버전별 로드맵 기준이다. README는 설계 요약과
PoC 순서를 설명하고, 실제 릴리즈 범위와 완료 기준은 이 문서를 우선한다.

## Versioning Rules

참조한 로컬 선례는 다음 원칙으로 요약된다.

- 확정 로드맵 작업은 구체적인 버전 행에 들어간다. 아직 제품화 여부나 순서가 정해지지
  않은 큰 방향은 `Future TODO`에 둔다.
- 활성 release train에서는 버전을 건너뛰지 않는다. 불가피하게 중단된 버전은
  `Superseded` 또는 `Blocked`로 표시하고 근거를 남긴다.
- `Released`는 구현 완료만 뜻하지 않는다. 테스트, privacy/redaction 검증, 문서 갱신,
  독립 리뷰 또는 동등한 검증 evidence가 있어야 한다.
- patch version은 회귀 수정, 문서 정합성 보정, release 복구처럼 기존 범위를 고치는
  데만 쓴다.
- minor version은 작고 검증 가능한 기능 단위다. adapter, report panel, cost field,
  redaction fixture 같은 항목은 minor로 올린다.
- major version은 제품의 운영 경계가 바뀌는 큰 단계에만 쓴다. 중앙 collector나
  gateway/control-plane처럼 저장 위치, 운영 모드, 책임 경계가 달라지는 경우는 TODO에서
  충분히 검증한 뒤 major line으로 승격한다.

Semver는 v1.0.0부터 안정 계약으로 본다. v0.x 단계에서는 minor version도 schema나
구현 경계를 바꿀 수 있지만, 변경 이유와 migration 필요 여부를 로드맵 또는 릴리즈
노트에 남긴다.

## Product North Star

여러 coding agent를 쓰더라도 token, latency, tool call, permission, compaction,
error, 예상 비용을 하나의 trace/span schema로 볼 수 있게 만든다. 1차 제품은 서버를
띄우지 않는 local-only observability다. 중앙 collector와 gateway는 local-only 경로가
검증된 뒤 선택적으로 붙인다.

## Major Lines

| Major | Status | Scope |
| --- | --- | --- |
| v0.x | Planned | Local-only PoC를 작은 minor release로 쪼개 검증한다. |
| v1.x | Planned | Local-only stable: Codex, Claude Code, Cursor adapter와 static HTML report를 안정화한다. |

## Active Train: v0.1.0-v1.0.0

| Version | Status | Scope | Exit Evidence |
| --- | --- | --- | --- |
| v0.1.0 | Released | Trace schema and local event log foundation | `agent_observability.v1` event/span schema, append-only JSONL writer, parent/child span fixture, redaction-before-write fixture |
| v0.2.0 | Released | Codex local adapter | Codex notify/session source parsing, turn/tool span generation, token/latency capture, local event log smoke |
| v0.3.0 | Released | Static HTML report | Self-contained HTML renderer, session/repo/turn trace viewer, token/latency/error summary, browser file-open smoke |
| v0.4.0 | Planned | Cost estimate fields | rate table format, `estimated_cost`, `rate_table.version`, `cost.assumption`, unknown/incomplete pricing behavior |
| v0.5.0 | Planned | Privacy and redaction hardening | content logging off fixture, secret/path/key redaction fixture, no raw prompt/output in log/report/export |
| v0.6.0 | Planned | Claude Code adapter | hook/transcript parsing, tool duration, permission event, compaction event, shared schema parity |
| v0.7.0 | Planned | Cursor adapter | generation correlation, shell/tool span capture, workspace/file edit metadata, shared schema parity |
| v0.8.0 | Planned | Cross-agent local report polish | repo/session/team/model filters, redacted JSON snapshot export, local disk retention note |
| v0.9.0 | Planned | Local-only release candidate | install/config path, CLI command shape, docs, fixtures, independent review |
| v1.0.0 | Planned | Local-only stable | Codex/Claude Code/Cursor adapters, static report, cost estimate, privacy fixtures, docs and smoke checks all pass |

## Later Lines

| Version | Status | Scope | Exit Evidence |
| --- | --- | --- | --- |
| v1.1.0 | Planned | Report usability improvements | richer timelines, saved filters, regression fixture for large local event logs |
| v1.2.0 | Planned | Local retention and archive policy | disk budget, retention config, archive/export smoke |

## Branch Strategy

- `main` is the stable line. It should only receive verified version work.
- Each planned version starts from current `main` on `release/vX.Y.Z`.
- Use focused `feat/vX.Y.Z/<topic>` branches only when a version is too large to
  keep reviewable on one release branch.
- Do not skip the active train. Finish or explicitly mark the current version
  `Blocked` / `Superseded` before starting the next one.
- Merge a release branch to `main` only after the version scope, tests, docs,
  privacy checks, and review evidence are complete.
- Tagging/publishing rules can be added when the project has an actual package
  distribution path; until then, the merge commit plus roadmap status is the
  release record.

## Future TODO

아래 항목은 의도와 방향만 남긴다. 아직 확정 버전으로 약속하지 않는다.
local-only v1.x가 실제로 쓸 만하다는 evidence가 생긴 뒤, 필요성이 분명한 항목만 major
line으로 승격한다.

| Item | Scope | Promotion Gate |
| --- | --- | --- |
| Optional internal collector | 팀/프로젝트 단위 집계, auth, schema validation, central trace/metrics/audit stores, retry-safe ingest | local-only report로는 해결되지 않는 팀 단위 운영 요구가 확인될 것 |
| Team aggregation and alerting | team/repo dashboards, cost/error spike alerts, collector failure/retry tests | 여러 사용자의 실제 event log를 중앙 집계해야 하는 요구가 확인될 것 |
| Optional gateway/control plane | provider-compatible routing, request attribution, billing reconciliation assumptions, Desktop App setting inheritance checks | 관측만으로 부족하고 요청 통제/과금 보정이 필요하다는 evidence가 있을 것 |

## Version Cycle

각 버전은 다음 순서로 닫는다.

1. 해당 버전의 scope와 exit evidence를 확인한다.
2. 가장 작은 완성 범위로 구현한다.
3. 변경된 동작을 fixture나 smoke로 검증한다.
4. privacy/redaction boundary가 약해지지 않았는지 확인한다.
5. README와 ROADMAP의 상태를 같이 갱신한다.
6. 독립 리뷰 또는 동등한 검증을 받고 blocking finding을 해결한다.
7. 커밋 전에 금지된 외부 backend/vendor 참조가 들어오지 않았는지 검색한다.
8. 완료 evidence가 모이면 상태를 `Released`로 바꾼다.

## Non-Skippable Gates

- 원문 prompt/output은 opt-in 없이는 local event log, queue, report, export, collector에
  남지 않아야 한다.
- 비용은 실제 청구액으로 단정하지 않는다. 단가표 기반 예상치와 assumption을 함께
  표시한다.
- central collector나 gateway는 local-only 경로가 깨끗하게 동작하기 전까지 필수
  경로가 아니다.
- agent별 adapter가 달라도 뒤쪽 schema는 하나로 유지한다.
- unsupported 또는 불안정한 agent log/hook format은 추측으로 안정 계약처럼 쓰지
  않는다.
