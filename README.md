# Agent Observability

Coding agent의 token, latency, tool lifecycle, error, permission, compaction을 하나의
privacy-safe trace 모델로 정규화하고, 로컬 SQLite와 self-contained HTML report로 확인하는
local-first CLI다.

> **v1.3.2 범위:** macOS standalone과 private canonical handoff import를 지원한다.
> Codex, Claude Code, Cursor의 hook/log를 자동 감시하는 producer와 receiver, daemon, team
> collector는 아직 포함하지 않는다.

## 설치

### GitHub Release (권장)

Apple Silicon과 Intel Mac에서 모두 동작하는 universal binary를 설치한다.

```bash
gh release download v1.3.2 \
  --repo MCprotein/agent-observability \
  --pattern 'agent-observability-1.3.2-darwin-universal2.tar.gz'
tar -xzf agent-observability-1.3.2-darwin-universal2.tar.gz
mkdir -p ~/.local/bin
install -m 0755 \
  agent-observability-1.3.2-darwin-universal2/agent-observability \
  ~/.local/bin/agent-observability
export PATH="$HOME/.local/bin:$PATH"
agent-observability --version
```

새 shell에서도 쓰려면 위 `export`를 shell profile에 추가한다. Release에는 arm64/x64 개별 archive,
`SHA256SUMS`, build provenance도 함께 게시된다.

### GitHub Packages

GitHub Packages는 인증이 필요하다. `read:packages` 권한이 있는 personal access token
(classic)을 npm password로 사용한다.

```bash
npm login \
  --scope=@mcprotein \
  --auth-type=legacy \
  --registry=https://npm.pkg.github.com
npm install --global @mcprotein/agent-observability \
  --registry=https://npm.pkg.github.com
agent-observability --version
```

이 npm package는 JavaScript launcher가 아니라 macOS universal Rust binary를 직접 설치한다.

### 소스에서 실행

개발에는 Rust `1.97`, Node.js `20+`가 필요하다.

```bash
cargo run -p agent-observability-cli -- --version
```

## 5분 시작

먼저 private runtime을 만든다.

```bash
agent-observability init ~/.agent-observability
agent-observability config-check ~/.agent-observability/config.json
agent-observability runtime-check ~/.agent-observability
agent-observability storage-check ~/.agent-observability
```

현재 ingest 입력은 별도 producer가 만든 **private canonical handoff JSONL**이어야 한다.
각 agent의 원본 transcript나 hook payload를 직접 넘기는 인터페이스가 아니다.

설치와 report 경로를 끝까지 확인하려면 release에 포함된 content-free Codex example을
사용할 수 있다. 실제 관측값이 아니며 producer 연동을 대신하지 않는다.

```bash
cp /path/to/extracted-release/examples/codex-handoff.v1.jsonl /tmp/codex-handoff.v1.jsonl
chmod 0600 /tmp/codex-handoff.v1.jsonl
agent-observability codex-ingest \
  ~/.agent-observability /tmp/codex-handoff.v1.jsonl
agent-observability report ~/.agent-observability
open ~/.agent-observability/logs/agent-observability-report.html
```

GitHub Package로 설치했다면 example은 tag에서 받을 수 있다.

```bash
curl -fsSLo /tmp/codex-handoff.v1.jsonl \
  https://raw.githubusercontent.com/MCprotein/agent-observability/v1.3.2/examples/codex-handoff.v1.jsonl
chmod 0600 /tmp/codex-handoff.v1.jsonl
```

실제 private canonical handoff는 agent별 `*.v1` schema와
[Adapter Compatibility](docs/ADAPTER_COMPATIBILITY.md)의 verified surface에 맞춰 별도 producer가
생성해야 한다.

```bash
agent-observability codex-ingest \
  ~/.agent-observability /path/to/private-codex-handoff.jsonl
agent-observability claude-code-ingest \
  ~/.agent-observability /path/to/private-claude-handoff.jsonl
agent-observability cursor-ingest \
  ~/.agent-observability /path/to/private-cursor-handoff.jsonl
```

실제 handoff를 가져온 뒤 report를 만들고 브라우저에서 연다.

```bash
agent-observability report ~/.agent-observability
open ~/.agent-observability/logs/agent-observability-report.html
```

HTML은 mode `0600`으로 원자 기록된다. 별도 web server 없이 `file://`로 열리며 외부
network request를 만들지 않는다.

## 무엇을 볼 수 있나

- repo/session/agent/model filter와 saved view
- bounded timeline, trace pagination, token/cost/error KPI
- versioned rate table 기반 예상 비용과 incomplete/unknown 상태
- Codex, Claude Code, Cursor canonical adapter parity
- bounded retention plan/apply와 private JSONL archive

예상 비용은 실제 청구액이 아니다. 단가표가 없거나 불완전하면 `unknown` 또는
`incomplete`로 표시한다. 자세한 계산 계약은 [Cost Estimation](docs/COST_ESTIMATION.md)에 있다.

현재 검증된 adapter 경계:

| Agent | Verified version | Boundary | Known gap |
| --- | --- | --- | --- |
| Codex | `0.150.1` | macOS standalone private handoff | receiver와 producer 미포함 |
| Claude Code | `2.1.248` | macOS standalone private handoff | user interrupt signal 미확인 |
| Cursor | `3.17.21` | macOS standalone private handoff | specific shell/MCP/file hook은 diagnostic-only |

정확한 source surface는 [Adapter Compatibility](docs/ADAPTER_COMPATIBILITY.md)를 따른다.

## 동작 구조

GitHub의 rich display 지원 여부와 관계없이 보이도록 plain-text diagram을 사용한다.

```text
Private canonical handoff JSONL
             |
             v
  bounded Rust agent adapter
             |
             v
  domain + application rules
             |
        +----+--------------------+
        |                         |
        v                         v
SQLite local_state.v4      privacy + cost projector
(local authority)                  |
        |                           v
        v                    validated ReportDtoV1
rebuildable JSONL                   |
        |                           v
        +--> retention       TypeScript UI asset
             plan/apply             |
                                    v
                         self-contained private HTML
                                    |
                                    v
                           browser file://, no network
```

핵심 경계:

1. Adapter는 agent별 입력만 번역하고 storage나 UI를 직접 다루지 않는다.
2. Domain과 application은 agent payload, filesystem, SQLite, CLI, UI에 의존하지 않는다.
3. SQLite가 권위 저장소이며 JSONL과 HTML은 재생성 가능한 projection이다.
4. TypeScript UI는 원본 payload가 아니라 Rust가 검증한 `ReportDtoV1`만 받는다.
5. 현재 standalone 제품 경로에는 network 전송, login, collector가 없다.

상세 흐름은 [Collection Flow](docs/COLLECTION_FLOW.md), 설계 원칙은
[Architecture](docs/ARCHITECTURE.md)를 참고한다.

## Retention

Retention은 ingest 중 암묵적으로 데이터를 지우지 않는다. read-only plan을 먼저 만들고,
managed runtime 밖의 새 private archive 경로를 지정해 적용한다.

```bash
agent-observability retention-plan ~/.agent-observability
agent-observability retention-apply \
  ~/.agent-observability PLAN_ID /path/to/private-retention-archive.jsonl
```

- cutoff와 같거나 이후인 관측이 하나라도 있는 trace는 전체가 유지된다.
- 만료 대상은 trace 전체 단위로 archive한 뒤 제거한다.
- bounded plan이 `truncated=true`이면 apply 전체를 거부한다.
- source cursor와 bounded replay guard는 남아 오래된 재전송을 거부한다.

Crash/retry/reclaim 계약은 [Local Runtime](docs/LOCAL_RUNTIME.md#retention-and-private-archive)에
정리되어 있다.

## Privacy

- prompt, assistant/tool output, cwd, command, path, raw email은 durable contract에 넣지 않는다.
- secret과 민감 field는 durable write 전에 allowlist와 redaction을 통과해야 한다.
- runtime directory는 `0700`, managed files는 `0600`을 요구한다.
- symlink, broad permission, unknown field, unsupported schema version은 fail closed한다.
- transient `SourceObservation` 전체를 SQLite, JSONL, HTML, export에 저장하지 않는다.
- standalone은 login, endpoint, outbox, network client 없이 동작한다.

## 명령

| Command | Purpose |
| --- | --- |
| `init <root>` | private local runtime 생성 |
| `config-check <config-json>` | strict config 검증 |
| `runtime-check <root>` | singleton/resource boundary 확인 |
| `storage-check <root>` | local authority 상태 확인 |
| `<agent>-ingest <root> <handoff>` | canonical handoff ingest |
| `report <root> [rate-table]` | self-contained HTML 생성 |
| `retention-plan <root>` | read-only expiry plan 생성 |
| `retention-apply <root> <plan-id> <archive>` | private archive 후 expiry 적용 |
| `contracts` | active schema/profile boundary 출력 |
| `version` | CLI version 출력 |

## 저장소 안내

| Path | Responsibility |
| --- | --- |
| `crates/domain` | identifier, lifecycle, topology, token 의미 |
| `crates/contracts` | transient/durable/report contract와 manifest |
| `crates/adapter-*` | agent별 canonical handoff translation |
| `crates/application` | pricing과 privacy-safe report projection |
| `crates/local-store` | SQLite authority, recovery, retention |
| `crates/local-runtime` | config, lock, admission, resource policy |
| `crates/static-report` | DTO와 TypeScript asset의 HTML assembly |
| `crates/cli` | one-shot composition root |
| `ui/report` | strict TypeScript report UI |
| `contracts` | shared closed JSON Schema |
| `distribution/npm` | GitHub Packages용 native binary metadata |

## 개발 검증

```bash
cargo fmt --all -- --check
cargo test --workspace --no-fail-fast
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p agent-observability-cli -- contracts
npm test
cargo run -p xtask -- perf local --profile smoke --check
```

Release 판정에는 별도의 uninterrupted release profile과 sanitized manifest review가 필요하다.

## 문서

| 목적 | 문서 |
| --- | --- |
| 수집·저장·report·retention 흐름 | [Collection Flow](docs/COLLECTION_FLOW.md) |
| 기술 스택과 책임 경계 | [Architecture](docs/ARCHITECTURE.md) |
| runtime bounds와 성능 | [Local Runtime](docs/LOCAL_RUNTIME.md) |
| agent별 지원 범위 | [Adapter Compatibility](docs/ADAPTER_COMPATIBILITY.md) |
| UI 원칙 | [Design](DESIGN.md) |
| 버전과 release gate | [Roadmap](ROADMAP.md) |
| branch, PR, review | [Contributing](CONTRIBUTING.md) |
| Future TODO team profile | [Team Architecture](docs/TEAM_ARCHITECTURE.md) / [Team Contracts](docs/TEAM_CONTRACTS.md) |

## 기여와 라이선스

버전 하나는 하나의 release branch와 draft PR로 관리한다. 독립 review와 CI를 통과한 뒤
`main`에 병합하고, 병합 SHA에 immutable version tag를 붙여 Release와 Package를 게시한다.
[CONTRIBUTING.md](CONTRIBUTING.md)에 전체 절차가 있다.

Apache License 2.0. 자세한 내용은 [LICENSE](LICENSE)를 확인한다.
