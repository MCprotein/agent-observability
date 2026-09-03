# Contributing

agent-observability는 버전 단위의 작은 pull request로 변경을 검토하고
릴리즈 기록을 남긴다. 구현 범위와 완료 조건은 [ROADMAP.md](ROADMAP.md)를
기준으로 한다.

## Version Pull Request Workflow

1. 최신 `main`에서 계획된 다음 버전의 `release/vX.Y.Z` 브랜치를 만든다.
2. `git push --set-upstream origin release/vX.Y.Z`로 branch를 게시한다.
3. 버전 scope와 exit evidence를 확인한 뒤 draft pull request를 일찍 연다.
4. 변경을 작고 논리적인 커밋으로 나누고, PR 본문에 현재 검증 상태와 남은
   gate를 계속 갱신한다.
5. 해당 버전의 테스트, 문서, privacy/redaction, 성능 및 호환성 gate를 실행한다.
6. 작성 역할과 분리된 독립 리뷰를 받고 blocking finding을 해결한다. 역할이 분리된
   서브에이전트 리뷰도 독립 리뷰로 사용할 수 있다. PR에는 reviewer의 사람/에이전트
   역할, 검토한 commit SHA, verdict, finding과 해결 결과를 기록한다. 리뷰는 테스트를
   대체하지 않으며, 실행하지 못한 gate를 통과한 것으로 표시하지 않는다.
7. exit evidence가 모두 모이고 PR이 mergeable인 것을 확인한 뒤에만 draft를 해제한다.
   release review는 `Candidate`, ROADMAP은 `In Progress`로 유지해 아직 게시되지 않은
   버전을 `Released`로 표시하지 않는다.
8. PR을 `main`에 병합한 뒤 resulting commit SHA와 PR 상태를 확인한다.
9. 병합 SHA에 annotated `vX.Y.Z` tag를 만들고 push한다. Release workflow가 GitHub
   Release와 GitHub Package를 모두 게시할 때까지 결과를 확인한다.
10. 공개 Release를 다시 내려받아 checksum, attestation, 설치와 필요한 사용자 경로 QA를
    검증한다.
11. 별도 post-publication docs branch에서 README 설치 버전, ROADMAP `Released`, release review
    evidence와 current-release 문서를 함께 갱신하고 독립 리뷰 후 `main`에 병합한다.
12. 각 PR 병합 뒤 remote와 동기화하고 병합된 release/docs branch를 GitHub와 로컬에서
    삭제한다. 보존해야 할 예외가 있으면 PR에 이유와 제거 조건을 기록한다.
13. 다음 버전은 post-publication 문서가 반영된 최신 `main`에서 새 release branch로 시작한다.

버전 브랜치 하나에는 한 버전만 포함한다. 현재 버전을 `Released`, `Blocked`, 또는
`Superseded`로 정리하기 전에는 다음 버전 구현을 섞지 않는다. 큰 버전을 여러 사람이
병렬 작업해야 할 때만 `feat/vX.Y.Z/<topic>` 브랜치를 사용하고 release PR에 통합한다.

## Version Selection

- patch: 회귀 수정, 보안/정확성 보정, 문서 정합성처럼 사용자 기능 범위를 늘리지
  않는 변경
- minor: 작고 독립적으로 검증 가능한 기능 추가
- major: 저장 위치, 운영 모드, 책임 경계 또는 호환성 계약을 바꾸는 큰 단계

아직 범위와 순서가 확정되지 않은 큰 기능은 버전을 미리 배정하지 않고 ROADMAP의
`Future TODO`에 둔다.

## Required Checks

PR에 적용되는 명령은 버전 scope에 따라 달라질 수 있지만, 최소한 다음 저장소 검증을
실행한다.

```bash
cargo fmt --all -- --check
cargo test --workspace --no-fail-fast
cargo clippy --workspace --all-targets -- -D warnings
npm test
git fetch origin main
git diff --check origin/main...HEAD
git diff --check
```

GitHub의 `CI` workflow는 pull request에서 Rust 검사와 `npm test`를 다시 실행한다.
CI 성공은 merge gate의 일부이며, 로컬에서만 실행할 수 있는 장시간 release performance
검증을 대체하지 않는다.

사용자 동작이나 성능 계약이 바뀌면 ROADMAP에 선언된 fixture, smoke, browser, performance
검증도 추가한다. 생성된 evidence는 실제 실행 결과와 호환되는 protocol/manifest만
보존하며, 실패하거나 오래된 결과로 현재 gate를 충족했다고 주장하지 않는다.

## Release Publication

- tag 형식은 `vX.Y.Z`이며 Cargo workspace, root npm package와
  `distribution/npm/package.json`의 버전이 모두 `X.Y.Z`와 같아야 한다.
- tag는 merge된 `main` commit에만 붙인다. 게시한 tag는 이동, 삭제 후 재사용하거나 같은
  package version으로 다시 만들지 않는다.
- GitHub Release에는 macOS arm64, x64, universal2 archive, npm package tarball,
  `SHA256SUMS`를 게시하고 artifact attestation을 생성한다.
- GitHub Package는 `@mcprotein/agent-observability`이며 universal Rust binary를 직접
  `bin`으로 노출한다. JavaScript launcher나 runtime dependency를 추가하지 않는다.
- workflow는 Release를 draft로 먼저 만들고 Package 게시가 성공한 뒤 공개한다. 일부 단계가
  실패하면 draft와 workflow log를 근거로 복구하며 version이나 tag를 바꾸지 않는다.
- 게시 후 universal archive를 새 directory에 받아 checksum, attestation,
  `agent-observability --version`을 검증하고 PR 또는 review evidence에 결과를 남긴다.

## Pull Request Review

리뷰는 구현 정확성뿐 아니라 다음 항목을 확인한다.

- 버전 scope와 실제 diff가 일치하는가
- schema, Rust domain, TypeScript report contract가 정합적인가
- privacy/redaction 및 local-only 기본 동작이 유지되는가
- 실패, 재시도, crash, pressure 경로가 검증됐는가
- README, ROADMAP, architecture 문서와 코드가 서로 모순되지 않는가
- 외부 backend/vendor에 종속된 표현이나 구현이 들어오지 않았는가

독립 리뷰는 작성자가 아닌 사람의 리뷰 또는 작성 역할과 분리해 실행한 서브에이전트
리뷰다.
리뷰 finding과 해결 결과는 reviewed commit SHA와 함께 PR에 남겨 이후 버전이 같은
결정을 다시 추측하지 않게 한다. 리뷰 뒤 코드가 바뀌면 변경된 범위에 맞춰 다시
검토한다.
