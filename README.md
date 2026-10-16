# hwpx-agent-engine

AI 에이전트와 사람이 같은 명령으로 HWPX 문서를 조회·편집·검증하는 독립 문서 엔진.

## 현재 상태

2026-10-16 기준, Linux 단계 0 선행 시험(S1~S3)과 **단계 1 코어(구현 계획 1-1~1-20) 전체 구현·검증을 완료**했다. Windows 한컴 재열기 시험(S4)과 단계 2~4(JSON-lines/MCP 어댑터, `rhwp` 격리 프리뷰 어댑터, Windows 한컴 비교 평가)는 후속 단계다.

- **단계 1 코어(10개 crate)**:
  - `hae-diag`: 진단·오류 코드(31종), 패키지 거절 사유(18종), `Limits` 상수, 자원 예산 타입
  - `hae-zip`: 방어적 ZIP 파싱·스트리밍 해제 한도·무편집 항목 원시 압축 데이터/로컬 헤더 바이트 보존 쓰기
  - `hae-xml`: 원문 토큰 범위·BOM/선언/줄바꿈 보존, XML 1.0 NCName·namespace 해석, 문자 참조·CDATA 대응표, 범위 교체
  - `hae-package`: `mimetype`, `content.hpf` manifest/spine, `META-INF`, 미디어 참조 무결성
  - `hae-model`: 정규 상태, 안정 `documentId`·`nodeId` 대응표, 편집 허용/잠금 판정, `inspect`/`query`/`get` 조회 뷰
  - `hae-edit`: 4가지 부분 편집(`text.replace`, `paragraph.format`, `cell.setText`, `image.replace`), `LinesegPolicy`, `PrvText` 갱신
  - `hae-verify`: 편집 계층에 의존하지 않는 독립 저장 관문 검사기(30종 변조 사례 전수 거절)
  - `hae-project`: Linux 디렉터리 핸들(`openat`/`O_NOFOLLOW`) 기반 루트 I/O, 세션 `.lock`(`flock`), 원자적 저장(`RENAME_NOREPLACE`·`fsync`), `.hwpwork` 프로젝트 및 무편집 바이트 통과 내보내기
  - `hae-exec`: 단일 출처 명령 레지스트리(21개 명령·JSON Schema), `writerId`·`requestSequence` 순서/재전송/충돌 처리, `command.batch`, `dryRun`, 100단위 항목 공유 Undo/Redo 이력·스냅샷·세션 메모리 예산
  - `hae-cli`: `hae script`(단일 세션 스크립트 실행) 및 `hae run`(단발 실행, 일회성 Undo/Redo는 `SESSION_REQUIRED` 거절)
- **검증 상태**: Linux에서 서식 검사(`cargo fmt`), `cargo clippy --workspace --all-targets -- -D warnings`, Rust 시험 100건, 독립 Python oracle 단위 시험 88건을 모두 통과했다(`docs/decisions/stage1-results.md`). 단, Windows 한컴 기준 조판 호환성(`S4`, `nativeComparison`)은 아직 `not_checked`이며 전체 0.1 완료는 단계 3·4 검증 뒤에 판정한다.

## 개발 환경과 검사

`rust-toolchain.toml`이 Rust 1.93.1을 지정한다. 저장소 루트에서 다음 명령을 실행한다.

```sh
bash scripts/check.sh
python3 -m unittest discover -s tests/oracle
```

`scripts/check.sh`는 서식 검사, 경고를 오류로 취급하는 Clippy, workspace 전체 시험을 순서대로 실행한다.

## 설계와 근거

- 설계서: `docs/superpowers/specs/2026-10-16-hwpx-engine-0.1-design.md`
- 구현 계획: `docs/superpowers/plans/2026-10-16-hwpx-engine-0.1-plan.md`
- 단계 0 결과: `docs/decisions/stage0-results.md`
- 단계 1 완료 검증 결과: `docs/decisions/stage1-results.md`
- 의존성 및 보안 점검: `docs/decisions/dependencies.md`
- 클린룸·재사용 기록: `docs/decisions/clean-room.md`
- 모델 역할 분담: `docs/decisions/model-roles.md`
- 재개 검토 및 수정 기록: `docs/decisions/resumption-review/`
