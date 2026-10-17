# hwpx-agent-engine

AI 에이전트와 사람이 같은 명령으로 HWPX 문서를 조회·편집·검증하는 독립 문서 엔진.

## 현재 상태

2026-10-17 기준, **0.1의 Linux 범위는 완료**했다. 단계 0 선행 시험(S1~S3)을 통과했고, 단계 1 코어·단계 2 JSON-lines·stdio MCP·단계 3 격리 렌더링의 완료 판정은 예이며, 단계 4 Linux 범위는 실제 stdio MCP 94/94와 세트 A·B 실제 픽셀 시각 판독 PASS로 완료했다. **Windows·한컴 검증은 최종 단계로 보류**했으며, 한컴 출력과의 `nativeComparison`·한컴 SDK 분석은 이번 완료 범위 밖이다. 설계 §13에 따라 Windows 결과 회수 전까지 0.1 전체의 최종 완료는 선언하지 않는다. 단계 1~4의 Linux 결과는 **`main`에 병합했다**(`7cfc1fa`). [Linux 범위 완료 판정](docs/decisions/stage234-linux-results.md#linux-범위-완료-판정-2026-10-17)을 따른다.

**한컴 상호운용 추가 검증(2026-10-17).** 한컴오피스 2020 Linux Beta를 디스플레이 없이 실행하고 화면을 읽는 경로(`QT_QPA_PLATFORM=vnc` + 최소 RFB 클라이언트)를 만들어, 엔진 출력을 실제 한컴으로 열어 확인했다. 그 결과 실제 결함 2건을 찾아 고쳤다 — 매니페스트 미디어 항목의 `isEmbeded="1"` 누락(한컴이 내장 대신 외부 연결로 해석)과 `imgClip`/`imgDim`의 단위 오류(픽셀당 75 HWPUNIT). 86개 파일 전수에서 실제 한컴 문서·사용자 작성 문서·엔진 출력은 모두 정상적으로 열린다. 로컬 게이트는 `scripts/hancom_gate.sh`이며, 자세한 근거와 한계는 [한컴 열기 검증 기록](docs/decisions/engine-output-open-verification-2026-10-17.md)에 있다. **이 검증은 Linux 빌드(2020 Beta)에 한정되며 Windows 한컴오피스 2024의 결과가 아니다.**

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
- **추가 인터페이스·프리뷰 crate**: `hae-jsonl`(요청 1 MiB·응답 4 MiB), `hae-mcp`(레지스트리 기반 도구·실제 프로세스 동등성), `hae-render-rhwp`(bubblewrap 격리, 페이지 선택, SVG 검사, PNG 변환, 쪽 대응 검증).
- **대형 프리뷰 전달**: 실제 JSON 및 MCP 이스케이프 예산을 초과하는 쪽은 출력 루트의 PNG/SVG 파일로 제공한다. 원본·기존 파일을 덮어쓰지 않고 해시와 경로를 반환한다.
- **검증 상태**: 최종 Linux 관문은 fmt·Clippy와 Rust 244개, 독립 Python oracle 88개, 사용량 도우미 시험 30개를 통과했다. 실제 stdio MCP 대표 시나리오도 94개 검증을 통과했다. [최종 결과와 재현 명령](docs/decisions/stage234-linux-results.md)에 범위를 기록했다. 이 수치는 임의 문서의 한컴 조판 호환성이나 전체 프로세스 RSS 보장을 의미하지 않는다.
- **사용 안내**: [설치·MCP 연결·편집·프리뷰 안내서](docs/user-guide.md). 별도 `rhwp`·`hae-raster` 실행 파일, `bwrap`, 제한된 폰트 폴더가 필요하다.


## 개발 환경과 검사

`rust-toolchain.toml`이 Rust 1.93.1을 지정한다. 저장소 루트에서 다음 명령을 실행한다.

```sh
bash scripts/check.sh
python3 -m unittest discover -s tests/oracle
```

`scripts/check.sh`는 서식 검사, 경고를 오류로 취급하는 Clippy, PNG 작업자 빌드, 사용량 도우미 시험, workspace 전체 시험을 순서대로 실행한다.

## 설계와 근거

- 설계서: `docs/superpowers/specs/2026-10-16-hwpx-engine-0.1-design.md`
- 구현 계획: `docs/superpowers/plans/2026-10-16-hwpx-engine-0.1-plan.md`
- 단계 0 결과: `docs/decisions/stage0-results.md`
- 단계 1 완료 검증 결과: `docs/decisions/stage1-results.md`
- 단계 2·3·4 Linux 최종 결과: `docs/decisions/stage234-linux-results.md`
- 의존성 및 보안 점검: `docs/decisions/dependencies.md`
- 클린룸·재사용 기록: `docs/decisions/clean-room.md`
- 모델 역할 분담: `docs/decisions/model-roles.md`
- 재개 검토 및 수정 기록: `docs/decisions/resumption-review/`

운영 모델 배정: GPT 6.1 Sol이 오케스트레이터이며, Opus 5.5는 계획 전용이다. [모델 역할과 한도 운영](docs/decisions/model-roles.md)을 따른다.
