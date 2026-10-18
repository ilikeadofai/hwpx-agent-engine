# hwpx-agent-engine (0.3.0)

AI 에이전트와 사람이 같은 명령으로 기존 `.hwpx` 문서를 안전하게 조회·편집·검증하는 독립 문서 엔진입니다.

## 1. 핵심 차별점

새 문서를 처음부터 생성하는 대신, **기존 문서를 깨뜨리지 않고 지정한 부분만 고치는 것**에 집중합니다([경쟁 분석 §8](docs/research/competitive-gap-analysis.md#8-경쟁-프로젝트-대비-우위열위)).

- **원본 바이트 보존**:
  수정하지 않은 ZIP 항목은 재압축하지 않고 원시 압축 데이터와 로컬 헤더를 그대로 유지합니다.
  무편집 내보내기와 전체 Undo 후 내보내기는 원본 파일과 바이트 단위로 동일합니다(SHA-256 일치).
- **독립 저장 검증 (`hae-verify`)**:
  편집 계층(`hae-edit`·`hae-model`·`hae-exec`)에 의존하지 않는 별도 검증기가 원본 패키지와 선언적 요청만으로 기대 변경을 다시 계산합니다.
  허용되지 않은 XML·자원·미디어·캐시 변경이 있으면 저장 전에 거절합니다.
- **원자적 편집과 Undo/Redo**:
  `command.batch` 중 한 단계라도 실패하면 전체를 원복합니다.
  `dryRun` 사전 검증, `expectedVersion` 버전 충돌 방지, `.hwpwork` 프로젝트 파일 잠금(`flock`), 최대 100단위 스냅샷 Undo/Redo를 지원합니다.
- **한컴오피스 없이 동작**:
  한컴오피스·SDK·COM 설치 없이 전 경로가 독립 실행됩니다.

## 2. 할 수 있는 것 (0.4 명령 61개)

단일 레지스트리에 등록된 61개 명령을 모든 인터페이스가 공유합니다([0.3 설계서 §2·§3](docs/superpowers/specs/2026-10-18-hwpx-engine-0.3-design.md)).

- **조회 (12개)**:
  `document.inspect`(구조 개요·편집 가능 여부), `document.stats`(요소·글자 수 통계), `document.validate`(공개 OWPML 사전 대조 — 알 수 없는 요소·속성, 미해결 참조, 카운트 불일치를 오류·경고·정보로 보고), `element.query`, `element.get`, `section.list`, `style.list`(`header.xml` 스타일·글자/문단 모양·테두리 조회), `note.list`(각주·미주), `link.list`(하이퍼링크), `trackchange.list`(변경 추적 표식·작성자), `bookmark.list`(책갈피), `memo.list`(메모), `document.validate`(OWPML 사전 대조 문서 검사)
- **텍스트 편집 (2개)**:
  `text.replace`(자소 범위 교체, 길이 0 위치 삽입, `controls:"preserve"` 개체 보존 교체), `text.replaceAll`(문서·범위 전체 정확 일치 일괄 교체)
- **서식 (3개)**:
  `text.format`(자소 범위 굵게·기울임·밑줄·취소선·크기·글자색, `charPr` 재사용·단일 추가), `paragraph.format`(정렬·백분율 줄간격), `paragraph.setStyle`(기존 `styleIDRef` 변경)
- **표 (10개)**:
  `table.get`(격자·병합·셀 조회), `cell.setText`(단일·다문단 `paragraphs`·병합 기준 셀), `cell.setFormat`(기존 `borderFill`·배경색·여백), `cell.setSize`(비병합 셀 열·행 크기), `table.setColumnWidths`, `table.setRowHeights`, `table.insertRow`, `table.deleteRow`, `table.insertColumn`, `table.deleteColumn`
- **수식 (4개)**:
  `equation.list`, `equation.get`, `equation.setScript`(일반 텍스트 수식 스크립트 교체), `equation.setProperties`(`baseLine`·`textColor`·`font`·`baseUnit`·`lineMode`·`version`)
- **필드·양식 (3개)**:
  `field.list`, `field.setText`(`CLICK_HERE`·`HYPERLINK` 표시 텍스트 채우기), `form.fill`(라벨 셀 오른쪽·아래 빈 셀 일괄 채우기)
- **쪽 설정·머리말 (5개)**:
  `section.getPage`, `section.setPage`(기존 용지 방향·크기·여백), `section.setPageNumbering`(쪽 시작 방식·번호), `section.setVisibility`(첫 쪽 머리말·꼬리말·쪽번호·테두리·채우기 표시), `header.setText`(기존 머리말·꼬리말 직접 문단 텍스트 교체)
- **그림 (3개)**:
  `picture.list`, `image.replace`(기존 프레임 유지 정적 PNG/JPEG 교체), `image.fit`(`contain`·`cover`·`stretch` 비율·자르기 맞춤)
- **생성 (3개)**:
  `document.create`(한컴이 여는 최소 HWPX를 처음부터 만들기), `paragraph.insert`(기준 문단 앞·뒤에 문단 추가 — 문단 모양·글자 모양 복제, 새 nodeId), `paragraph.delete`(표·개체·필드가 든 문단과 구역 속성 문단은 거절)
- **내보내기·저장 (3개)**:
  `document.export`(새 `.hwpx` 저장), `document.exportText`(새 Markdown `.md`·HTML `.html` 파일로 읽기 전용 변환), `project.save`(nodeId·상태를 유지하는 `.hwpwork` 저장)
- **미리보기 (1개)**:
  `document.preview`(격리된 외부 `rhwp`로 SVG·PNG 렌더링)
- **HWP5 읽기 (1개)**:
  `hwp5.inspect`(구형 `.hwp` 5.0 바이너리의 파일 헤더·구역·문단 텍스트를 열린 문서 없이 읽기 전용 조회)
- **세션·이력·배치·스키마 제어 (11개)**:
  `document.open`, `document.close`, `command.batch`, `history.undo`, `history.redo`, `history.clear`, `document.snapshot`, `snapshot.release`, `document.diff`, `command.list`, `command.describe`

## 3. 할 수 없는 것과 알려진 한계

- **표·문단 구조 생성은 일부만 가능**:
  새 문서(`document.create`), 본문 문단 추가·삭제(`paragraph.insert`·`paragraph.delete`), 기존 표 안의 빈 행·열 삽입(`table.insertRow`·`insertColumn`)까지 지원합니다.
  다만 표·그림·수식·구역·머리말·꼬리말을 **새로 만들지는** 못하고, 문단·셀을 합치거나 나누는 작업과 도형·차트·OLE 편집도 지원하지 않습니다.
- **읽기 전용 문서와 HWP5 편집 불가**:
  암호화 문서는 열기를 거절합니다. 전자서명·변경 추적 문서는 조회와 무편집 내보내기만 허용합니다.
  구형 `.hwp`(HWP5)는 `hwp5.inspect` 조회만 지원하며 편집·HWPX 변환 저장은 하지 않습니다.
- **Windows 한컴오피스 2024 실기 미검증**:
  한컴 열기·화면 확인은 **Linux 한컴오피스 2020 Beta(11.20.0.989)**에서만 수행했습니다.
  Windows 한컴 2024에서의 열기·저장·재조판은 아직 검증하지 않았으며 `nativeComparison`은 항상 `not_checked`입니다.
- **`rhwp` 미리보기와 한컴 조판의 불일치 가능성**:
  `document.preview`가 만드는 SVG/PNG와 쪽 번호는 외부 렌더러 `rhwp`의 추정치(`renderer-estimated`)입니다.
  실제 한컴오피스의 줄바꿈·쪽 나눔·쪽수와 일치한다고 보장하지 않습니다.
- **미리보기 외부 실행 파일 및 글꼴 폴더 제약**:
  엔진에 렌더러가 내장되어 있지 않습니다.
  미리보기에는 별도의 `rhwp` 실행 파일, `bubblewrap`(`bwrap`), `hae-raster`, 그리고 **작은 글꼴 폴더(`HAE_FONT_DIR`)**가 필요합니다.
  글꼴 스캔 상한은 **파일 4,096개·총 64 MiB·파일당 32 MiB**이므로 `/usr/share/fonts` 전체를 지정하면 `RENDERER_UNAVAILABLE`(`memory_limit`)로 거절됩니다.

## 4. 빠른 시작

저장소 루트(`rust-toolchain.toml`, Rust 1.93.1)에서 전체 실행 파일(`hae`, `hae-jsonl`, `hae-mcp`, `hae-raster`)을 빌드하거나 사용자 폴더에 설치한 뒤 `hae-mcp doctor`로 점검합니다([사용 안내서 §2·§3](docs/user-guide.md)).

```bash
cargo build --release
```

별도 폴더 설치 및 `doctor` 점검 예시:

```bash
cargo build --release --locked -p hae-mcp -p hae-render-rhwp
install -Dm755 target/release/hae-mcp "$HOME/.local/bin/hae-mcp"
install -Dm755 target/release/hae-raster "$HOME/.local/bin/hae-raster"
hae-mcp --version
HAE_RHWP=/opt/rhwp/bin/rhwp hae-mcp doctor \
  --input-root /home/user/Documents/hwpx-inputs \
  --output-root /home/user/Documents/hwpx-outputs
```

미리보기 없이 조회·편집·내보내기만 쓸 때는 `HAE_RHWP`·`HAE_FONT_DIR` 없이도 동작합니다.
입출력 폴더는 미리 만들어 둔 실제 경로로 바꾸어 지정합니다.

**Claude Code 연결 예시**:

```bash
claude mcp add --scope project --transport stdio hwpx-engine \
  -e HAE_RHWP=/opt/rhwp/bin/rhwp -e HAE_FONT_DIR=/home/user/hwpx-fonts \
  -- /home/user/.local/bin/hae-mcp \
  --input-root /home/user/Documents/hwpx-inputs \
  --output-root /home/user/Documents/hwpx-outputs
```

**Codex CLI 연결 예시**:

```bash
codex mcp add hwpx-engine \
  --env HAE_RHWP=/opt/rhwp/bin/rhwp --env HAE_FONT_DIR=/home/user/hwpx-fonts \
  -- /home/user/.local/bin/hae-mcp \
  --input-root /home/user/Documents/hwpx-inputs \
  --output-root /home/user/Documents/hwpx-outputs
```

## 5. 인터페이스

세 실행 파일 모두 동일한 `--input-root`·`--output-root` 경계와 명령 레지스트리를 사용합니다.

- **CLI (`hae`)**:
  `hae run`(단발 명령 실행, 일회성 Undo/Redo는 `SESSION_REQUIRED` 거절)과 `hae script`(단일 세션에서 JSON 단계 배열 실행)를 제공합니다.
- **JSON-lines (`hae-jsonl`)**:
  표준 입출력 한 줄 JSON 요청·응답 서버입니다.
  단일 요청 1 MiB·응답 4 MiB 상한과 `writerId`·`requestSequence` 순서·재전송 검사를 적용합니다.
- **MCP (`hae-mcp`)**:
  stdio JSON-RPC 2.0 서버입니다. 61개 명령을 개별 도구(`text_replace` 등)와 `outputSchema`·4종 annotations로 노출합니다.
  세션형 리비전(`2025-11-25`, `2025-06-18`, `2025-03-26`, `2024-11-05`)과 요청별 `_meta`·`server/discover`를 사용하는 `2026-07-28` 리비전을 지원합니다.

## 6. 검증 상태

2026-10-18 기준 최신 게이트 수치는 [0.3 계약 검토 후속 검증](docs/decisions/0.3-contract-followup.md)(Rust 리비전 `8c2fcd4`, Python 오라클 `2ae4d7d`)에 기록되어 있습니다.

| 게이트 | 결과 |
|---|---|
| `bash scripts/check.sh` (렌더러 없음) | 종료 0 — fmt·Clippy 통과, Rust **766 passed / 0 failed / 13 ignored(SKIP)**, `scripts/tests` **33/33** |
| `bash scripts/check.sh` (렌더러 있음) | 종료 0 — Rust 총 **779 passed / 0 failed**(실제 렌더러 **13/13** 포함), `scripts/tests` **33/33** |
| 독립 Python 오라클 (`tests/oracle`) | 종료 0 — **111/111 passed / 0 failed** |
| 실제 stdio MCP 평가 (`scripts/evaluate_mcp.py`) | 종료 0 — **146/146 passed / 0 failed** (`nativeComparison=not_checked`) |

- Python 오라클의 독립 구현 범위는 `text.replace`·`text.replaceAll`·`cell.setText`·`paragraph.format`·`image.replace`와 0.3의 `equation.setScript`·`equation.setProperties` 7개 명령입니다. 나머지 편집은 `hae-verify` 독립 검증 게이트로 검사합니다.
- 실제 문서 읽기·편집·Undo 원본 바이트 일치와 Linux 한컴 2020 Beta 게이트·화면 관찰은 [0.3 최종 통합 검증](docs/decisions/0.3-final-verification.md)에 기록했습니다.
- 이 수치는 모든 문서·인자 조합의 성공이나 Windows 한컴 조판 호환성을 보장하지 않습니다.

## 7. 문서 링크

- 사용 안내서: [docs/user-guide.md](docs/user-guide.md)
- 변경 이력: [CHANGELOG.md](CHANGELOG.md)
- 설계서: [0.3 설계서](docs/superpowers/specs/2026-10-18-hwpx-engine-0.3-design.md) · [0.2 설계서](docs/superpowers/specs/2026-10-18-hwpx-engine-0.2-design.md) · [0.1 설계서](docs/superpowers/specs/2026-10-16-hwpx-engine-0.1-design.md)
- 검증 기록: [0.3 계약 검토 후속 검증](docs/decisions/0.3-contract-followup.md) · [0.3 최종 통합 검증](docs/decisions/0.3-final-verification.md)
- 경쟁 분석: [경쟁 분석·기능 격차 보고서](docs/research/competitive-gap-analysis.md)
- 클린룸·의존성: [클린룸·재사용 기록](docs/decisions/clean-room.md) · [의존성 및 보안 점검](docs/decisions/dependencies.md)

## 8. 라이선스와 클린룸 원칙

- **클린룸 원칙**:
  코어 엔진(`hae-zip`·`hae-xml`·`hae-package`·`hae-model`·`hae-edit`·`hae-verify`·`hae-project`·`hae-exec`·`hae-hwp5`)은 비공개 한컴 SDK·바이너리를 참조하지 않고, 공개 사양(KS X 6101, 한컴 공개 `hwpx-owpml-model` Apache-2.0 등)과 자작 시험 문서 관찰만으로 구현했습니다([0.1 설계서 §12](docs/superpowers/specs/2026-10-16-hwpx-engine-0.1-design.md#12-클린룸-자료-경계), [클린룸 기록](docs/decisions/clean-room.md)).
- **외부 의존성 및 라이선스 경계**:
  직접 의존 크레이트 15개(`quick-xml`, `flate2`, `resvg` 등)는 범용 오픈소스 라이브러리이며 정확 버전(`=`)으로 고정했습니다.
  GPL 계열 코드는 복사하지 않습니다. 외부 렌더러 `rhwp`는 코어에 링크하지 않고 격리된 별도 프로세스로만 실행합니다.
  로컬 글꼴 파일은 렌더링 시 읽기 전용으로 마운트할 뿐 저장소에 포함하거나 재배포하지 않으며, 각 크레이트는 현재 `publish = false`로 설정되어 있습니다.
