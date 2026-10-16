# hwpx-agent-engine

AI 에이전트와 사람이 같은 명령으로 HWPX 문서를 조회·편집·검증하는 독립 문서 엔진.

## 현재 상태

2026-10-16 기준, Linux 단계 0 선행 시험(S1~S3)을 마쳤고 단계 1 코어를 구현 중이다. Windows 한컴 재열기 시험(S4)은 결과 대기 중이다.

- `main`: Rust workspace와 진단·ZIP·XML·패키지 관계 계층(계획 1-1~1-6)의 구현 및 시험 자료를 포함한다. 나머지 crate는 아직 골격이다.
- `wip/stage1-model-edit`: 중단 시점의 문서 모델·조회·편집 계층 작업을 보존하는 브랜치다. 전체 빌드가 완료되지 않은 진행 중 코드이며, 상태는 해당 브랜치의 `docs/decisions/github-upload-2026-10-16.md`에 기록한다.
- CLI·MCP·제품용 미리보기·트랜잭션·Undo/Redo의 전체 연결은 아직 완료되지 않았다.

## 개발 환경과 검사

`rust-toolchain.toml`이 Rust 1.93.1을 지정한다. 저장소 루트에서 다음 명령을 실행한다.

```sh
bash scripts/check.sh
```

이 스크립트는 서식 검사, 경고를 오류로 취급하는 Clippy, workspace 시험을 순서대로 실행한다. `main`의 코드 기준점인 `077447a`를 별도 디렉터리에 추출해 2026-10-16에 세 검사를 모두 통과했다. 이 결과는 해당 기초 계층의 시험 결과이며, 한컴 조판 호환성이나 전체 엔진 완성을 뜻하지 않는다.

## 설계와 근거

- 설계서: docs/superpowers/specs/2026-10-16-hwpx-engine-0.1-design.md (원본은 학교 탐구 폴더의 산출물/AI_native_한글엔진_설계_2026-10-16에 보존)
- 설계 검토 기록: docs/decisions/design-review/
- 모델 역할 분담: docs/decisions/model-roles.md
- 단계 0 시험(버리는 코드): spikes/. 결과 판단은 docs/decisions/stage0-results.md. spikes/의 코드는 제품 crate로 옮기지 않는다.
- 외부 코드 사본(spikes/vendor/)은 Git에 넣지 않는다.

