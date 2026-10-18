# 변경 이력

이 파일은 [Keep a Changelog 1.1.0](https://keepachangelog.com/ko/1.1.0/) 형식을 따른다.
버전 구분(엔진 버전, 명령 레지스트리 schemaVersion, 봉투·프로젝트 정수 schemaVersion)은
[docs/decisions/versioning.md](docs/decisions/versioning.md)를 본다.
근거는 git 기록, `docs/superpowers/specs/`의 설계서, `docs/decisions/`의 결정·검증 기록이다.
세 릴리스 모두 같은 브랜치에서 하루 이틀 간격으로 이어진 개발이어서, 경계는 설계서의 명령 수
(0.1은 21개, 0.2는 22개, 0.3은 57개)와 커밋 시점으로 잡았다.

## [Unreleased]

### 추가

- `document.validate`(카탈로그 MT-13): 열린 문서를 한컴의 공개 OWPML 모델
  (`hancom-io/hwpx-owpml-model`, Apache-2.0)의 요소·속성 이름과 대조하고, 참조 무결성
  (`charPrIDRef`·`paraPrIDRef`·`styleIDRef`·`nextStyleIDRef`·`borderFillIDRef`), 패키지 필수 항목,
  `itemCnt`/`fontCnt`/`secCnt` 불일치, 중복 자원 id, 안 쓰이는 자원을 `code`·`severity`·`reason`·
  `count`·`examples`로 보고한다. 읽기 전용(`category: Query`, `mutatesContent:false`)이며
  `expectedVersion`·요청 순번·이력이 없다. `scope`·`cursor`·`maxExamples` 지원.
  사전은 `crates/hae-model/data/owpml-dictionary.json`(생성: `scripts/owpml_dictionary.py`),
  근거·실측·한컴 게이트는 `docs/decisions/document-validate-2026-10-19.md`와
  `docs/decisions/document-validate-hancom-open-2026-10-19.md`에 있다.
- `Limits::max_validate_elements`(기본 1,000,000): `document.validate`가 훑는 XML 요소 방문 한도.
  넘으면 `RESOURCE_LIMIT`(`reason: validate_scan_limit`)으로 거절하고 부분 결과를 돌려주지 않는다.
- `crates/hae-exec/tests/documentation.rs`: `docs/user-guide.md` 부록 색인(명령 수·이름·MCP 도구
  이름·분류·문서 변경 여부)을 레지스트리와 대조하는 시험.

- 0.4 기능 1호 — 문서·문단 생성: `paragraph.insert`(기준 문단 앞/뒤에 `count`개 문단,
  문단 모양·첫 run 글자 모양 복제, 새 nodeId), `paragraph.delete`(같은 컨테이너의 문단
  연속 삭제, 표·개체·필드·각주·구역 속성 문단 거절), `document.create`(제목·문단 목록으로
  최소 HWPX 패키지를 처음부터 만들어 출력 루트에 새 파일로 공개, `document.export`와 같은
  경로 격리·덮어쓰기 금지, 독립 검증 통과 후 공개).
- 노드 생성 계약 확장: `NodeRemap`(`TableRemap` | `ParagraphRemap`)과
  `Document::rebuild_node_structure`, 편집기/검증기 공용 문단 능력 술어(광고·실행 일치).
- 독립 검증기 `hae-verify/src/paragraph_structure.rs`·`created.rs`(원본/요청만으로 기대
  바이트·기대 패키지 전체를 재도출, 편집기 산출물 미사용)와 변조 거절 시험.
- `document.open`이 HWPX를 입력 루트 다음 **출력 루트**에서도 찾는다(만든 문서 바로 열기).
- 합성 픽스처 `tests/fixtures/paragraph/`(+생성기·결정론 확인)와 `document.create`
  구조 검사기 `tests/fixtures/created/check_created.py`.
- 설계·게이트 기록: `docs/decisions/document-paragraph-creation-design.md`,
  `document-paragraph-creation-hancom-gate-2026-10-19.md`.
- 시험 비의존성 수정: `annotations`·`readonly_queries`가 저장소 밖(`spikes/work/`) 로컬
  입력이 없으면 SKIP한다(`tables.rs`와 같은 규칙). 레지스트리 시험이 명령 수를 상수 대신
  `COMMAND_NAMES`와 비교한다.
- **미리보기 도구 자동 탐색**: `HAE_RHWP`가 없어도 실행 파일 옆, `../bin`, 개발 트리
  (`spikes/vendor/rhwp-src/target/{release,debug}`), `PATH` 순서로 `rhwp`를 찾는다.
  `HAE_BWRAP`·`HAE_FONT_DIR`·`HAE_RASTER`도 같은 원칙으로 기본값을 찾는다. 명시한 환경 변수는
  항상 최우선이며 값이 그대로 쓰인다. 탐색은 `HAE_RENDERER_DISCOVERY=0`으로 끌 수 있다.
- **진단 개선**: `doctor`가 도구별 `path`·`source`(고른 규칙)·`searched`(살펴본 후보와 판정)·
  `guidance`와 `checks.discovery`·`checks.platformIsolation`을 보고한다.
  `RENDERER_UNAVAILABLE`의 주요 사유 메시지에 해결 방법 한 줄이 들어가고,
  `renderer_not_configured`의 `detail`에 살펴본 후보 목록이 실린다.
- **Windows opt-in 미리보기**: `HAE_ALLOW_UNSANDBOXED_PREVIEW=1`(기본값은 거절)로만 격리 없이
  실행하며, 결과에 `SANDBOX_UNVERIFIED` 경고와 `metadata.sandbox`(강제/미강제 목록,
  `jobObject.applied:false`)를 남긴다. PNG는 제공하지 않는다(`PNG_UNAVAILABLE`).
- 문서: [플랫폼별 격리 보장](docs/decisions/renderer-isolation-platforms.md).

### 변경

- 미리보기 실패 메시지의 해결 안내(`renderer_missing`, `sandbox_unverified`,
  `raster_worker_missing`, `raster_worker_not_configured`).
- 기존 "렌더러 미설정" 시험은 `HAE_RENDERER_DISCOVERY=0`을 명시해 결정적으로 만들었다.
- `crates/hae-exec/tests/preview.rs`, `crates/hae-mcp/tests/{renderer_env,protocol_conformance}.rs`,
  `crates/hae-cli|hae-jsonl/tests/renderer_env.rs`를 Unix 전용으로 표시해 Windows 크로스
  `cargo check --tests`가 통과한다(동작 변화 없음).

### 하지 않은 것

- Linux 격리 계약(bwrap·`prlimit`·자기 점검)은 바꾸지 않았다. `HAE_ALLOW_UNSANDBOXED_PREVIEW`는
  Linux에서 무시된다.
- `metadata.coverage`는 `unknown`을 유지한다. 렌더러가 그리지 못한 요소를 보고하지 않으므로
  실제 값을 계산할 근거가 없다(사유는 [어댑터 메모](docs/decisions/stage3-adapter-notes.md)).
- Windows Job Object 상한(메모리·프로세스 수·CPU 시간)은 적용하지 않았다. `unsafe_code = "forbid"`
  아래에서 FFI가 필요하기 때문이며, 값만 선언하고 `applied:false`로 보고한다.

## [0.3.0] - 2026-10-18

기능 계약은 [0.3 설계서](docs/superpowers/specs/2026-10-18-hwpx-engine-0.3-design.md)다.
명령은 22개에서 57개로 늘었다. 이 목록은 등록과 시험의 범위이며, 모든 문서·인자에서
성공한다는 뜻이 아니다(§9 현재 한계).

### 추가

- 조회 명령: `document.stats`, `section.list`, `section.getPage`, `note.list`, `style.list`,
  `table.get`, `picture.list`, `equation.list`, `equation.get`, `field.list`, `link.list`,
  `trackchange.list`, `bookmark.list`, `memo.list`. 페이지 조회는 revision에 묶인 cursor를 쓴다.
- `document.exportText`: 현재 문서를 Markdown 또는 HTML 새 파일로 내보낸다(원본·이력 불변).
- `hwp5.inspect`와 새 crate `hae-hwp5`: HWP 5.0 파일 헤더와 문단 텍스트를 읽기 전용으로 조회한다.
- 서식·구조 편집: `text.format`(글자 서식), `paragraph.setStyle`, `header.setText`, `image.fit`,
  `section.setPage`, `section.setPageNumbering`, `section.setVisibility`.
- 표 편집: `table.setColumnWidths`, `table.setRowHeights`, `cell.setSize`, `cell.setFormat`,
  `table.insertRow`, `table.deleteRow`, `table.insertColumn`, `table.deleteColumn`(기존 표 안의
  셀·문단만 새로 만든다. nodeId 사상 TableRemap과 Undo/Redo ID 복원 포함).
- 수식·필드: `equation.setScript`, `equation.setProperties`(PrvText 유일 일치 치환, 아니면 보존+stale),
  `field.setText`(CLICK_HERE·HYPERLINK 결과 채우기), `form.fill`(라벨 기반 양식 채우기).
- 명령 descriptor의 MCP `annotations` 힌트와 레지스트리 상수 `REGISTRY_SCHEMA_VERSION`.
- `docs/decisions/versioning.md`(버전 축과 migration 정책), 한컴 공식 문서 기능 카탈로그,
  경쟁 분석(`docs/research/competitive-gap-analysis.md`).
- Windows 이식성: `hae-project`의 루트 I/O를 `io/{unix,windows}`로 분리하고
  `x86_64-pc-windows-gnu` 대상의 `cargo check`를 통과시켰다.
- `scripts/check.sh`가 렌더러 유무를 출력하고, 렌더러가 필요한 실제 시험 13건
  (executor preview 4, adapter 9)을 없으면 명시적으로 SKIP, 있으면 실행한다.
- GitHub Actions CI(`.github/workflows/ci.yml`).

### 변경

- 엔진 패키지 버전과 `command.list`/`command.describe`의 descriptor `schemaVersion`이 모두 `0.3.0`이다.
  descriptor 버전은 이제 별도 상수이며 엔진 버전과 독립적으로 관리한다. JSON-lines/MCP 봉투와
  `.hwpwork`의 정수 `schemaVersion`은 1을 유지한다.
- 표 치수 광고와 검증은 수정하는 축 단위로 판정한다. 수정하지 않는 축의 합 불일치나 상대 크기는
  편집을 막지 않는다.
- Windows에서는 렌더러 격리를 보장할 수 없으므로 미리보기를 `RENDERER_UNAVAILABLE(sandbox_unverified)`로
  거절한다(fail closed).
- 이미 실제 렌더러를 쓰는 시험을 모두 `#[ignore]`로 분리해 렌더러 없는 검사와 섞이지 않게 했다.

### 수정

- 표 치수 광고가 요청 축 모순을 true로 내보내던 문제와 반대 축 불일치로 정상 편집이 거절되던 문제.
- 조회 pagination이 끝에서 되돌아 감기던 문제와 게이트 로그를 최신 것만 보관하도록 한 문제.
- Windows에서 프로젝트 I/O가 이식되지 않던 문제.
- 수식 속성 oracle의 nullable 공개 필드 불일치.

### 알려진 한계

- Windows 한컴오피스 2024에서 0.3 산출물의 열기·저장·재열기는 실기 검증하지 않았다. 한컴 판정은
  Linux 한컴 2020 Beta 한 빌드에서만 얻었고, Windows 실기는 Stage 0 S4(원본·캐시 변형 12개) 1차 기록뿐이다.
  Windows에서는 빌드와 편집·내보내기·검증만 되고 미리보기(SVG/PNG)와 렌더러 기반 `page_info`는 쓸 수 없다.
- rhwp가 계산한 쪽수와 한컴의 쪽수가 다를 수 있다. 문단 줄 배치 캐시를 지운 문서를 rhwp가 자체 규칙으로
  나누기 때문이다. `nativeComparison`은 별도 Windows 근거가 생기기 전까지 `not_checked`다.
- 보도자료 편집본에서 한컴이 "- 3 -"만 있는 3쪽 빈 쪽을 만든다. 편집 때문인지 한컴의 재조판 때문인지 확정하지
  못했다.
- 새 문서와 새 문단을 만들 수 없다. 새로 만드는 노드는 기존 표 안의 셀·문단으로 한정한다.
- 표 구조 편집은 중첩 표·cellzoneList·병합과 겹치는 삭제·지원 밖 템플릿을 거절한다. 노드 outline에는
  새 명령 34개 전부의 capability가 실려 있지 않다.
- HWP5는 읽기만 되며 편집·저장은 하지 않는다. Python oracle은 설계서 §8의 범위만 판정한다.
- 이번 시험은 공개·합성 문서와 일부 로컬 문서 범위이며 사용자 문서 전반의 성공을 보증하지 않는다.

## [0.2.0] - 2026-10-18

설계는 [0.2 설계서](docs/superpowers/specs/2026-10-18-hwpx-engine-0.2-design.md)다. 명령은 21개에서
22개가 되었다. 워크스페이스 버전 문자열은 `ec87953`에서 0.2.0이 되었고 MCP 완성 작업까지 같은 문자열로 이어졌다.

### 추가

- `text.replace`: grapheme 위치 삽입(길이 0 범위), 빈 문단 삽입,
  `controls: "preserve"`로 개체·탭·줄바꿈을 남기는 범위 교체.
- `cell.setText`: 병합 셀 편집, 다문단 셀의 `paragraphs` 배열(문단 수는 유지).
- `text.replaceAll`: 정확한 일치를 겹치지 않게 골라 원자적으로 교체한다. 건너뜀은 `MATCHES_SKIPPED`
  경고로 사유와 위치를 알린다.
- 편집 가능 여부 광고: `textSpans`, `modes`, `requires` 등 대상별 capability와 사유.
- 필드 경계 거절과 기존 개체 옆 삽입.
- MCP 서버 완성: 프로토콜 4개 버전(2025-03-26부터 2026-07-28) 협상, 도구 annotations, 배포 진단
  `hae-mcp doctor`.
- 독립 검증: 별도 Python oracle과 버전이 붙은 명령 계약, MCP 평가 스크립트(`scripts/evaluate_mcp.py`).
- 한컴 열기 확인 기록(Linux): v0.2 출력 4건, MCP 출력, 폼 채우기 등.

### 변경

- 레지스트리 descriptor `schemaVersion`이 `0.2.0`이 되었다. 봉투와 프로젝트 정수 버전은 1이다.
- `docs/user-guide.md`를 0.2 편집 동작에 맞췄다.
- 미리보기 폰트 사전 점검을 `doctor`와 공유한다.

### 수정

- 광고한 편집 범위가 verifier가 지원하는 범위보다 넓던 문제(`2874c19`).
- 세션 메모리 예산이 보관 중인 검증 로그를 세지 않던 문제.
- MCP 최신 프로토콜의 캐시 메타데이터와 schema 계약, 문서 선택·탐색 후속 문제(M3·M4 검증 후속).
- 이전 fixture에 header.xml과 해석되는 shape 참조가 없던 문제.

### 알려진 한계

- Windows 한컴 2024에서 0.2 산출물의 열기·저장·재열기는 실기 검증하지 않았다. 한컴 열기 판정은 Linux에서만 했다.
- rhwp 쪽수와 한컴 쪽수가 다를 수 있다. 캐시를 지운 계획서는 rhwp가 19쪽을 20쪽으로 배치했다.
- 보도자료의 3쪽 빈 쪽 원인은 미확정이다(한컴 PDF에서 "- 3 -"만 있는 쪽).
- 새 문서·새 문단 생성과 문단 수 변경은 할 수 없다.
- 공개 합성 범위 밖의 일부 사용자 문서에서 dangling 스타일 참조처럼 원본 결함 때문에 편집이 막힌다.

## [0.1.0] - 2026-10-17

설계는 [0.1 설계서](docs/superpowers/specs/2026-10-16-hwpx-engine-0.1-design.md)다. 명령 21개.

### 추가

- 기존 HWPX 문서의 구조를 보존하는 편집 코어. crate: `hae-diag`, `hae-zip`, `hae-xml`,
  `hae-package`, `hae-model`, `hae-edit`, `hae-verify`, `hae-project`, `hae-exec`.
- 원문 바이트 보존, 무편집 내보내기 바이트 동일, 안정 ID, `.hwpwork` 프로젝트 저장·재열기.
- 명령: `text.replace`, `paragraph.format`, `cell.setText`, `image.replace`, 조회
  (`document.inspect`, `element.query/get`), `command.batch`, `history.undo/redo/clear`, `document.snapshot`,
  `snapshot.release`, `document.diff`, `document.export`, `project.save`, `document.open/close`,
  `command.list/describe`, `document.preview`.
- 원자적 commit, `dryRun`, 정확한 Undo/Redo, 독립 검증기 `hae-verify`.
- 인터페이스: CLI(`hae script`, `hae run`), JSON-lines 서버(`hae-jsonl`), MCP stdio 서버(`hae-mcp`).
- 격리된 rhwp 미리보기 어댑터(`hae-render-rhwp`): bubblewrap 샌드박스, SVG 안전 검사, PNG 변환 worker.
- 스파이크 S1~S4 결과와 한컴 상호 운용 확인 기록(Linux).

### 변경

- 이미지 추가 때 미디어를 embedded로 표시하고 그림 기하를 HWPUNIT으로 쓴다.
- 미리보기 격리와 쿼터 인지 오케스트레이션을 강화했다.
- MCP 문서 선택과 흐름 검토 결정을 반영했다.

### 알려진 한계

- Windows 한컴 2024 실기 검증은 보류했다. Stage 0 S4의 Windows 1차 기록(12 입력의 열기·재저장·재열기)만 있다.
- rhwp 쪽수와 한컴 쪽수가 다를 수 있다. 줄 배치 캐시를 지운 문서에서 rhwp가 쪽을 더 만든 사례가 있다.
- 보도자료 편집본의 한컴 3쪽 빈 쪽("- 3 -"만 있는 쪽)은 원인 미확정이다.
- 새 문서와 새 문단을 만들 수 없고 기존 구조 안에서만 바꾼다. 미리보기는 Linux(bubblewrap)에서만 된다.
