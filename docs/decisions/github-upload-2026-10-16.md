# GitHub 업로드와 중단 작업 보존 (2026-10-16)

요청: 이 프로젝트의 Git 환경을 설정하고 GitHub에 업로드한다.

## 브랜치와 검증 범위

- `main`: 기초 구현 커밋 `077447abc8b14d9e3e457f55fed279340cfb3ade`와 README·Git 제외 목록 갱신을 포함한다.
- `wip/stage1-model-edit`: 중단된 문서 모델·조회·편집 계층 및 코퍼스 어휘 조사 파일을 원래 내용 그대로 보존한다. 이 브랜치는 전체 빌드가 실패하는 진행 중 작업이다.
- GitHub 대상은 `ilikeadofai/hwpx-agent-engine`, 초기 공개 범위는 private이다.

기초 구현 커밋을 `git archive`로 별도 디렉터리에 추출한 뒤 `bash scripts/check.sh`를 실행했다. 서식 검사, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`가 모두 종료 코드 0이었다. Rust 시험 항목은 총 34개 통과, 실패 0개다. 패키지 통합 시험의 악성·경계 사례 대조와 공개 코퍼스의 무편집 왕복 시험도 포함한다. CLI 등 일부 crate의 시험 수가 0인 것은 골격 상태이기 때문이다.

## 작업 브랜치의 현재 실패

업로드 준비 시 작업 디렉터리에서 `cargo test --workspace --locked`를 실행했다. 종료 코드는 101이며 `hae-edit` 컴파일 단계에서 실패했다.

- `mod image`가 선언돼 있지만 `crates/hae-edit/src/image.rs`는 아직 작성되지 않았다(`E0583`).
- `hae-edit`에서 사용하는 `serde`와 `serde_json`이 해당 crate 의존성에 선언되지 않았다(`E0432`, `E0433` 및 serde 속성 오류).
- 편집 실행부의 오류 처리 클로저에서 타입 추론 오류(`E0282`)도 보고됐다. 앞선 오류를 수정한 뒤 다시 확인해야 한다.

따라서 문서 모델·편집 계층 또는 단계 1 전체를 완료로 기록하지 않는다. 이번 업로드 작업은 해당 제품 코드를 수정하지 않았다.

## Git 설정과 저장 범위

기존 작성자 `Matt Choi <mattchoi10828@gmail.com>`를 저장소 로컬 설정으로 명시했다. `main` 브랜치를 유지하고 `push.default=simple`, `pull.ff=only`, `fetch.prune=true`, `core.quotepath=false`를 적용했다. GitHub HTTPS 인증은 로그인된 GitHub CLI의 자격 증명 도우미를 사용한다.

기존 Git 이력, 설계·검토 기록, 기초 코드와 시험 자료를 함께 업로드한다. `spikes/vendor/`, `spikes/work/`, 빌드 출력은 기존 제외 규칙을 유지한다. 환경 파일, Python 캐시와 코드 색인 산출물도 제외한다. Windows 한컴 시험 결과는 아직 없으며 이번 업로드로 검증 상태가 바뀌지 않는다.
