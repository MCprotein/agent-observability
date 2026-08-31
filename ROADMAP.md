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
  작성 역할과 분리된 독립 리뷰 evidence가 있어야 한다.
- patch version은 회귀 수정, 보안/정확성 보정, 문서 정합성, 기존 동작을 고정하는 fixture와
  migration contract처럼 사용자 기능 범위를 늘리지 않는 작업에만 쓴다.
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
error, 예상 비용을 하나의 trace/span schema로 볼 수 있게 만든다. 제품 구조는 server 없는
`standalone`과 선택적 collector를 쓰는 `team` profile을 함께 수용한다. 1차 delivery는
standalone이며, team collector와 gateway 구현은 local 경로가 검증된 뒤 Future TODO promotion
gate를 통과해 추가한다.

## Technology and Architecture Policy

- web UI는 TypeScript로 구현한다.
- domain, application, agent adapters, storage, export, CLI는 Rust로 구현한다.
- 현재 JavaScript v0.6 구현은 migration baseline으로 유지한다. Rust는 별도 CLI 경로로
  구현하고, 전체 command path의 contract parity가 확인된 release boundary에서 대체한다.
- architecture와 engineering rule의 정본은 `docs/ARCHITECTURE.md`다.
- standalone은 collector, login, network 없이 완전하게 동작해야 한다. team profile은 같은
  domain 의미와 report contract를 사용하되 별도 strict ingest contract를 가지며 local 경로를
  대체하거나 약화하지 않는다.
- canonical contract와 privacy boundary를 안정화하기 전에 새 agent adapter를 추가하지
  않는다.

## Major Lines

| Major | Status | Scope |
| --- | --- | --- |
| v0.x | Completed | Local-only PoC를 작은 minor release로 쪼개 검증했다. |
| v1.x | Active | Local-only stable: Codex, Claude Code, Cursor adapter와 static HTML report를 안정화한다. |

## Active Train: v0.1.0-v1.3.2

| Version | Status | Scope | Exit Evidence |
| --- | --- | --- | --- |
| v0.1.0 | Released | Trace schema and local event log foundation | `agent_observability.v1` event/span schema, append-only JSONL writer, parent/child span fixture, redaction-before-write fixture |
| v0.2.0 | Released | Codex local adapter | Codex notify/session source parsing, turn/tool span generation, token/latency capture, local event log smoke |
| v0.3.0 | Released | Static HTML report | Self-contained HTML renderer, session/repo/turn trace viewer, token/latency/error summary, browser file-open smoke |
| v0.4.0 | Released | Cost estimate fields | rate table format, `estimated_cost`, `rate_table.version`, `cost.assumption`, unknown/incomplete pricing behavior |
| v0.5.0 | Released | Privacy and redaction hardening | content logging off fixture, secret/path/key redaction fixture, no raw prompt/output in log/report/export |
| v0.6.0 | Released | Claude Code adapter | hook/transcript parsing, tool duration, permission event, compaction event, shared schema parity, raw prompt/output leak fixture |
| v0.6.1 | Released | Baseline correctness and contract freeze | Codex/Claude source-to-durable and cross-agent report golden fixture, strict metadata allowlist, fail-closed privacy regression, private artifact permissions, explicit correlation fields without downstream agent-ID parsing, deterministic sequential replay/no-op plus identity-conflict fixture, overlap-aware token pricing, independent review |
| v0.7.0 | Released | Rust contract foundation | Cargo workspace and CLI composition root, complete `SourceObservation` correlation/event boundary, wire-compatible `DurableRecordV1`/`ReportDtoV1`, closed shared JSON Schemas plus manifest, JavaScript schema conformance and fixture byte lock, typed Rust golden harness, independent architecture and code review; team ingest disabled |
| v0.8.0 | Released | Rust core and durable I/O | deterministic reducer, topology validation, fail-closed projectors, schema-semantic Rust serialization parity against v0.7 closed schemas/golden baseline, embedded transaction for source cursor/stable event/local record/profile-neutral delivery outcome, JSONL projection replay, crash-point idempotency, pricing policy parity, independent architecture/code/test review; team envelope/outbox/network remain disabled until Future TODO G0 promotion |
| v0.9.0 | Released | Rust Codex adapter | official-surface capability entry, primary/supplement source dedupe, bounded local handoff, canonical correlation, unsupported-event diagnostics, end-to-end CLI parity, independent architecture/code/test review |
| v0.10.0 | Released | Rust Claude Code adapter | official-surface capability entry, telemetry/hook precedence, permission and compaction events, failed lifecycle, explicit interrupt gap, out-of-order fixture parity, independent architecture/code/test review |
| v0.11.0 | Released | Rust Cursor adapter | official-hook capability entry, generation correlation, generic tool operation capture, specific shell/MCP/file hook diagnostic isolation, raw workspace/path/edit omission, shared contract parity, independent architecture/code/test review |
| v0.12.0 | Released | TypeScript static report UI | schema-generated and runtime-validated report DTO types, repo/session/agent/model filters, fixed local scope, versioned Rust/TypeScript view-reduction parity, pinned self-contained file-open browser smoke, independent architecture/code/test/dependency review |
| v0.13.0 | Released | Local-only release candidate | install/config path, bounded collection/flush/storage policy, singleton/crash/full-channel fixtures, adaptive load shedding, capacity/large-log bounds, `cargo run -p xtask -- perf local --profile release --check` evidence for foreground local-runtime ingress latency and CPU/RSS/disk/network budgets, docs and independent review; age retention/archive remain v1.2 |
| v1.0.0 | Released | Local-only stable | Exact-version macOS standalone private handoff imports, Rust static report, cost estimate, privacy fixtures, docs and smoke checks all pass; v0.13 normative evidence is carried forward for the unchanged local runtime path only and does not cover agent hook/receiver/producer performance; independent code and documentation review clear |

## Later Lines

| Version | Status | Scope | Exit Evidence |
| --- | --- | --- | --- |
| v1.1.0 | Released | Report usability improvements | bounded timeline, local structured-dimension saved views, 100-trace/200-span pagination, deterministic 4,096-span Node/Chromium regression, reload/delete browser smoke, independent review clear |
| v1.2.0 | Released | Local retention and archive policy | strict retention config and migration, whole-trace plan/apply, private archive contract, replay/crash/path-safety fixtures, physical reclaim, archive CLI smoke, passing normative manifest `1788152070592764000` bound to source `fe8da2e9b2bb9cbc088c4df7f551ca423ad9d097`, independent review clear |
| v1.3.0 | Superseded | Installable open-source distribution implementation | Source, license, package metadata, documentation, tests, and native arm64/x64 builds passed, but tag workflow run `33412746641` stopped before publication because its universal-binary verification used invalid `lipo` argument order; no Release or Package was published |
| v1.3.1 | Superseded | Distribution publication correction | Universal assembly, attestation, and draft Release succeeded, but run `33414142455` stopped before Package/public Release because npm interpreted `dist/*.tgz` as a repository shorthand; immutable tag and draft remain as failure evidence |
| v1.3.2 | Released | Package publication path correction | Run `33416302530` published the GitHub Package and public Release; downloaded checksums, universal `x86_64 arm64` executable version, and four artifact attestations verified independently |

## Branch Strategy

- `main` is the stable line. It should only receive verified version work.
- Each planned version starts from current `main` on `release/vX.Y.Z`.
- Push the release branch to `origin`, then open a draft pull request early and
  keep its scope, completed evidence, and remaining gates current while the
  version is in progress.
- Use focused `feat/vX.Y.Z/<topic>` branches only when a version is too large to
  keep reviewable on one release branch.
- Do not skip the active train. Finish or explicitly mark the current version
  `Blocked` / `Superseded` before starting the next one.
- Merge a release branch to `main` only after the version scope, tests, docs,
  privacy checks, performance gates, and role-separated independent review evidence
  are complete. A role-separated subagent review can provide that independent evidence.
  Review evidence records the reviewer role, reviewed commit SHA, verdict, and resolved
  blocking findings. An incomplete gate keeps the PR in draft.
- Confirm the resulting commit on `main`, switch to and update local `main`, then
  delete the merged release branch locally and remotely. Start the next version
  on a new branch from the updated `main`; preserve a merged branch only when its
  PR records a concrete reason and removal condition.
- After the release PR is merged, create one immutable annotated `vX.Y.Z` tag on
  the resulting `main` commit. The tag-triggered workflow must reject a tag that
  is not contained in `main` or does not match Cargo, root npm, and distribution
  package versions.
- The release workflow publishes checksum-bound, attested native archives to a
  GitHub Release and the same universal Rust executable through GitHub Packages.
  A failed publication keeps the GitHub Release in draft; never move or reuse a
  published version tag.

The contributor-facing procedure is documented in [CONTRIBUTING.md](CONTRIBUTING.md).

## Future TODO

아래 항목은 의도와 방향만 남긴다. 아직 확정 버전으로 약속하지 않는다.
local-only v1.x가 실제로 쓸 만하다는 evidence가 생긴 뒤, 필요성이 분명한 항목만 major
line으로 승격한다.

| Item | Scope | Promotion Gate |
| --- | --- | --- |
| Commercial team profile | [Team architecture](docs/TEAM_ARCHITECTURE.md)와 [contracts](docs/TEAM_CONTRACTS.md)의 G0-G4: Rust collector/query API, strict `TeamIngestEnvelopeV1`, local transactional outbox, principal-bound multi-email profile, Codex/Claude/Cursor mandatory capability matrix, configurable bounded cadence, monotonic heartbeat, append-only correction/retraction, field-level identity PII authorization, identity/RBAC, tenant isolation, atomic dedupe/metering, retention/deletion, encryption, audit, quota, DR/SLO, hosted UI | standalone report로는 해결되지 않는 실제 다중 사용자 운영 요구가 있고 named business/legal/security approvers가 data ownership, deployment, authorization, retention and commercial scope를 G0 artifact로 승인할 것 |
| Advanced team alerting | cost/error/latency spike rules, notification delivery, dedupe/suppression and alert audit | shared dashboard 이후 실제 notification 운영 요구와 incident owner가 확인될 것 |
| Optional gateway/control plane | provider-compatible routing, request attribution, billing reconciliation assumptions, Desktop App setting inheritance checks | 관측만으로 부족하고 요청 통제/과금 보정이 필요하다는 evidence가 있을 것 |

Team 항목은 collector endpoint 하나로 완료되지 않는다. `docs/TEAM_ARCHITECTURE.md`의 G0-G4를
순서대로 통과하며 G0 전에는 버전을 배정하지 않고, G4 evidence 전에는 상용화 완료로 표시하지
않는다.

## Version Cycle

각 버전은 다음 순서로 닫는다.

1. 해당 버전의 scope와 exit evidence를 확인한다.
2. 가장 작은 완성 범위로 구현한다.
3. 변경된 동작을 fixture나 smoke로 검증한다.
4. privacy/redaction boundary가 약해지지 않았는지 확인한다.
5. README와 ROADMAP의 상태를 같이 갱신한다.
6. 작성 역할과 분리된 독립 리뷰를 받고 blocking finding을 해결한다.
7. 커밋 전에 금지된 외부 backend/vendor 참조가 들어오지 않았는지 검색한다.
8. 완료 evidence가 모이고 PR이 mergeable이면 병합 직전에 상태를 `Released`로 바꾼다.
   병합이 완료되지 않으면 상태를 되돌린다.
9. release PR을 `main`에 병합하고 resulting commit SHA와 PR 상태를 확인한다.
10. 병합 SHA에 annotated `vX.Y.Z` tag를 생성하고 Release, Package, checksum과
    provenance 게시 결과를 확인한다.
11. local `main`을 갱신한 뒤 병합된 branch를 local/remote에서 삭제한다.
12. 다음 버전은 갱신된 `main`에서 새 release branch로 시작한다.

## Non-Skippable Gates

- 원문 prompt/output은 local opt-in 없이는 local event log, report, export에 남지 않아야 한다.
  team retry queue와 collector에는 local opt-in 여부와 무관하게 들어갈 수 없다.
- 비용은 실제 청구액으로 단정하지 않는다. 단가표 기반 예상치와 assumption을 함께
  표시한다.
- central collector나 gateway는 local-only 경로가 깨끗하게 동작하기 전까지 필수
  경로가 아니다.
- team profile이 추가되어도 standalone은 network, login, collector 없이 동일한 local 기능을
  유지해야 한다.
- agent별 adapter가 달라도 뒤쪽 schema는 하나로 유지한다.
- hook foreground path는 local bounded handoff만 수행하고 network, full-file scan, report render나 queue
  drain을 기다리지 않는다. Local release는 declared CPU/RSS/disk/latency budget과 pressure fixture를
  통과해야 한다.
- accepted observation을 직접 수정하지 않는다. 귀속 보정과 분석 제외는 idempotent append-only
  revision으로 처리하고 privacy deletion과 구분한다.
- unsupported 또는 불안정한 agent log/hook format은 추측으로 안정 계약처럼 쓰지
  않는다.
- UI 이외의 새 제품 코드는 Rust로 작성하고, web UI는 TypeScript `strict` contract를
  사용한다.
- canonical schema, Rust domain, TypeScript report DTO 사이에 수동으로 중복 정의된 계약을
  만들지 않는다.
- JavaScript와 Rust를 한 command path 안에서 FFI나 subprocess로 혼합하지 않는다. Rust
  수직 기능을 병렬 구현하고 golden parity가 확인된 release boundary에서 기본 경로를 전환한다.
