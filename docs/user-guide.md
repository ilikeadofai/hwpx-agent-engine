# HWPX 문서 엔진 (0.1) 사용 안내서

이 안내서는 한컴오피스(한글)로 작성된 `.hwpx` 보고서나 양식 문서를 Claude, Codex 같은 AI 에이전트에게 맡겨 안전하게 다듬고자 하는 선생님과 학생을 위해 작성되었습니다.

---

## 1. 이 엔진은 무엇을 하고, 무엇을 하지 않나요?

**HWPX 문서 엔진**은 기존 `.hwpx` 문서에서 **지정한 부분만 정확하게 고치고, 수정하지 않은 나머지 내용·서식·표·그림·수식을 원본 그대로 보존**하는 독립 도구입니다.

### 할 수 있는 것 (0.1 지원 범위)
- **한컴오피스 불필요**: Linux 환경에서 한컴오피스 설치나 라이선스 없이 바로 작동합니다.
- **문서 구조 및 요소 조회**: 구역, 문단, 표, 표 셀, 그림, 수식의 구조와 텍스트를 읽고 검색합니다.
- **문단 텍스트 범위 교체 (`text_replace`)**: 한 문단 안의 연속된 일반 글자 구간(`[start, end)` 글자 단위)을 다른 글자로 바꾸거나 지웁니다.
- **문단 서식 변경 (`paragraph_format`)**: 문단의 정렬 방식(왼쪽·가운데·오른쪽·양쪽·배분·나눔 정렬)과 백분율 줄간격(`1%~1000%`, 정수 `1`~`1000`)을 바꿉니다. 문서 전체 스타일은 건드리지 않고 대상 문단에만 적용합니다.
- **표 셀 텍스트 교체 (`cell_set_text`)**: 병합되지 않고 문단이 하나인 표 셀의 전체 텍스트를 바꿉니다.
- **그림 교체 (`image_replace`)**: 기존 그림의 위치·크기·캡션을 그대로 유지한 채, 정적 PNG 또는 JPEG 이미지 파일로 교체합니다.
- **되돌리기·다시 실행 및 미리보기**: 작업 전 가상 적용(`dryRun`), 변경 내역 비교(`document_diff`), 실행 취소(`history_undo`)·다시 실행(`history_redo`), 외부 렌더러 연동을 통한 PNG/SVG 미리보기(`document_preview`)를 지원합니다.

### 하지 않는 것 및 한계 (0.1 제외 범위)
- **전체 HWP/HWPX 기능 지원 아님**: 문단 추가·삭제·합치기·나누기, 표의 행·열 추가 및 삭제, 병합된 셀 편집, 여러 문단이 들어 있는 셀 편집, 수식 새로 만들기 및 편집, 도형 편집, 구형 `.hwp`(HWP5 바이너리) 파일 읽기·쓰기는 0.1에서 지원하지 않습니다.
- **읽기 전용 문서**: 암호화된 문서는 열지 않으며, 전자서명이나 변경 추적(Track Changes)이 들어 있는 문서는 열어서 내용을 조회하거나 무편집 상태로 내보내는 것만 허용하고 편집은 거절합니다.
- **외부 렌더러 별도 준비 필요**: 이 엔진에는 미리보기 렌더러가 내장되어 있지 않습니다. `document_preview`를 사용하려면 별도의 `rhwp` 실행 파일, `bubblewrap`(`bwrap`) 샌드박스 도구, 읽을 수 있는 글꼴 폴더를 환경 변수(`HAE_RHWP`, `HAE_BWRAP`, `HAE_FONT_DIR`)로 직접 지정해야 합니다.
- **엔진 구현 완료 범위와 Windows 한컴오피스 실기 호환성(단계 4 예정)의 구분**: 현재 엔진의 파일 편집·보존 검증·격리 미리보기 기능은 구현되어 있으나, 수정된 문서를 실제 Windows 한컴오피스(한글)에서 열었을 때의 실기 호환성 및 조판 대조 검증(단계 4)은 별도 확인 대상입니다. `document_preview`가 만드는 SVG/PNG 이미지와 쪽 번호는 외부 렌더러(`rhwp`)가 어림잡아 계산한 **추정치**이므로 실제 Windows 한컴오피스의 줄바꿈·쪽 나눔·표현 범위와 100% 일치한다고 보장하지 않습니다.

---

## 2. 빌드 방법

저장소 루트 폴더에서 Rust 툴체인(`cargo`)으로 릴리스 바이너리를 빌드합니다.

```bash
cargo build --release
```

빌드가 끝나면 `target/release/` 폴더 안에 세 사용자용 실행 파일과 PNG 변환 작업 파일이 만들어집니다.

- `target/release/hae-mcp`: Claude Desktop, Codex 등 MCP 클라이언트와 연결하는 **stdio MCP 서버**
- `target/release/hae-jsonl`: 한 줄 JSON 요청·응답으로 통신하는 **JSON-lines 서버**
- `target/release/hae`: 명령 스크립트 파일을 한 번에 실행하는 **CLI 도구**
- `target/release/hae-raster`: PNG 변환을 격리된 별도 프로세스에서 실행하는 작업 파일. 사용자용 실행 파일과 같은 폴더에 두거나 `HAE_RASTER`로 지정합니다.

---

## 3. 실행 환경 설정 (MCP 클라이언트 및 CLI)

세 실행 파일(`hae-mcp`, `hae-jsonl`, `hae`)은 모두 동일한 입출력 폴더 옵션과 미리보기 환경 변수를 사용합니다. 먼저 `hae-mcp --help` 출력을 확인해 보겠습니다.

```text
HWPX document engine MCP server (stdio, JSON-RPC 2.0, one message per line)

Usage: hae-mcp [OPTIONS]

Options:
      --input-root <INPUT_ROOT>    Root directory for HWPX/image inputs (must already exist) [default: /tmp]
      --output-root <OUTPUT_ROOT>  Root directory for exports and project saves (must already exist) [default: /tmp]
  -h, --help                       Print help
  -V, --version                    Print version

Preview environment (used by document.preview):
  HAE_RHWP      Path to the sandboxed rhwp renderer executable (enables preview)
  HAE_BWRAP     Path to bubblewrap (default /usr/bin/bwrap)
  HAE_FONT_DIR  Font directory mounted for rendering (default /usr/share/fonts)
  HAE_RASTER    PNG worker executable (default hae-raster next to this application)
```

### 명령줄 옵션
- `--input-root <INPUT_ROOT>`: 원본 `.hwpx` 문서와 교체할 이미지 파일(PNG/JPEG)이 들어 있는 **입력 허용 폴더**입니다. (이미 존재하는 폴더여야 하며 기본값은 `/tmp`입니다.)
- `--output-root <OUTPUT_ROOT>`: 수정된 `.hwpx` 내보내기 파일과 `.hwpwork` 프로젝트 파일을 저장할 **출력 허용 폴더**입니다. (이미 존재하는 폴더여야 하며 기본값은 `/tmp`입니다.)
- `-h, --help`: 도움말을 출력합니다.
- `-V, --version`: 버전을 출력합니다.

### 미리보기(`document_preview`)용 환경 변수 준비
엔진 바이너리 안에는 렌더러가 묶여(번들) 있지 않으며, 시스템 `PATH`도 자동으로 검색하지 않습니다. 문서 조회·편집·내보내기는 아래 환경 변수 없이도 모두 작동하지만, **`document_preview`를 쓰려면 다음 세 가지를 준비해 명시적으로 지정**해야 합니다.

1. **`HAE_RHWP`** *(미리보기 사용 시 필수)*: 별도로 준비한 `rhwp` 렌더러 실행 파일의 **절대 경로**입니다. 이 변수를 설정하지 않으면 미리보기 호출 시 `RENDERER_UNAVAILABLE`(`renderer_not_configured`) 오류가 반환됩니다.
2. **`HAE_BWRAP`** *(기본값: `/usr/bin/bwrap`)*: 렌더러를 네트워크·파일시스템으로부터 격리해 실행하는 `bubblewrap`(`bwrap`) 실행 파일의 **절대 경로**입니다. 엔진은 첫 미리보기 실행 전에 격리 자가 점검(self-test)을 수행하며, `bwrap`이 없거나 격리가 확인되지 않으면 렌더러를 실행하지 않고 거절합니다.
3. **`HAE_FONT_DIR`** *(기본값: `/usr/share/fonts`)*: 렌더링과 PNG 변환에 사용할 글꼴(`.ttf`, `.otf`, `.ttc`)이 들어 있는 **읽기 가능한 폴더의 절대 경로**입니다. 문서에서 쓰인 글꼴(예: 나눔고딕 등 한글 글꼴)이 이 폴더에 있어야 글자가 깨지지 않고 그려집니다.

### MCP 설정 JSON 예시 (`hae-mcp`)

사용 중인 MCP 클라이언트(예: Claude Desktop 설정 파일이나 Codex/에디터의 MCP 설정)에 아래와 같이 `command`, `args`, `env`를 등록합니다. 입출력 폴더는 미리 만들어 두고 본인의 환경에 맞는 절대 경로로 바꾸어 주세요.

```json
{
  "mcpServers": {
    "hwpx-engine": {
      "command": "/opt/hwpx-agent-engine/target/release/hae-mcp",
      "args": [
        "--input-root",
        "/home/user/Documents/hwpx-inputs",
        "--output-root",
        "/home/user/Documents/hwpx-outputs"
      ],
      "env": {
        "HAE_RHWP": "/opt/rhwp/bin/rhwp",
        "HAE_RASTER": "/opt/hwpx-agent-engine/target/release/hae-raster",
        "HAE_BWRAP": "/usr/bin/bwrap",
        "HAE_FONT_DIR": "/home/user/hwpx-fonts"
      }
    }
  }
}
```

> **팁**: 프로젝트 파일(`.hwpwork`)을 저장했다가 나중에 다시 열려면 `--input-root`와 `--output-root`를 모두 지정해 두는 것이 편리합니다. 엔진은 이 두 폴더 바깥의 문서 파일에는 절대 접근하지 않습니다.

### CLI(`hae`) 및 JSON-lines(`hae-jsonl`) 실행 예시

터미널이나 자동화 스크립트에서 직접 실행할 때도 동일한 플래그와 환경 변수를 사용합니다.

- **일회성 명령 실행 (`hae run`)**:
  ```bash
  /opt/hwpx-agent-engine/target/release/hae run \
    --input-root /home/user/Documents/hwpx-inputs \
    --output-root /home/user/Documents/hwpx-outputs \
    --command command.list
  ```
- **한 세션 스크립트 실행 (`hae script`, 미리보기 포함)**:
  여러 단계의 열기·편집·되돌리기·미리보기·내보내기를 한 세션으로 묶어 실행할 때는 JSON 스크립트 파일을 만들어 넘깁니다.
  ```bash
  HAE_RHWP=/opt/rhwp/bin/rhwp \
  HAE_BWRAP=/usr/bin/bwrap \
  HAE_FONT_DIR=/usr/share/fonts \
  /opt/hwpx-agent-engine/target/release/hae script \
    --input-root /home/user/Documents/hwpx-inputs \
    --output-root /home/user/Documents/hwpx-outputs \
    /home/user/Documents/hwpx-inputs/steps.json
  ```
- **JSON-lines 서버 실행 (`hae-jsonl`)**:
  ```bash
  HAE_RHWP=/opt/rhwp/bin/rhwp \
  HAE_BWRAP=/usr/bin/bwrap \
  HAE_FONT_DIR=/usr/share/fonts \
  /opt/hwpx-agent-engine/target/release/hae-jsonl \
    --input-root /home/user/Documents/hwpx-inputs \
    --output-root /home/user/Documents/hwpx-outputs
  ```

---

## 4. 대표 작업 시나리오 (열기 → 조회 → 편집 → 되돌리기 → 미리보기 → 저장/내보내기)

AI 에이전트에게 *"보고서 제목을 바꾸고 가운데 정렬한 다음, 첫 쪽 미리보기를 확인하고 새 파일로 저장해 줘"*라고 요청하면 에이전트는 내부적으로 아래 순서대로 MCP 도구를 호출합니다.

### ① 문서 열기 (`document_open`)
에이전트가 `--input-root` 안의 `.hwpx` 파일을 엽니다.
```json
{
  "path": "/home/user/Documents/hwpx-inputs/report_template.hwpx"
}
```
- **응답**: 문서 고유 식별자인 `documentId`(예: `"doc-1"`)와 시작 버전 번호인 `revision: 0`을 돌려받습니다.
- **MCP 문서 선택**: 문서가 필요한 도구의 `documentId`는 선택 사항이며, 생략하면 현재 연결에서 열린 문서를 사용합니다. 지정한 값은 열린 문서 ID와 일치해야 합니다. 불일치는 `TARGET_NOT_FOUND`로 거절하고 열린 문서에 적용하지 않으며, 열린 문서 없이 생략하면 `SESSION_REQUIRED`로 거절합니다. 편집의 `expectedVersion`은 여전히 필수입니다.

### ② 구조 훑어보기 및 요소 검색 (`document_inspect`, `element_query`)
문서에 어떤 문단과 표가 있는지 전체 개요를 보거나(`document_inspect`), 특정 글자가 들어간 문단·표 셀을 찾습니다(`element_query`).
```json
{
  "documentId": "doc-1",
  "kind": "paragraph",
  "textContains": "탐구 보고서"
}
```
- **응답**: 조건에 맞는 요소들의 고유 ID(`nodeId`, 예: `"node-p1"`), 현재 텍스트, 편집 가능 여부(`editable: true`)가 반환됩니다.
- 표의 특정 칸을 찾을 때는 `kind: "cell"`, `table: "<표 nodeId>"`, `row: 0`, `col: 0`(0부터 시작하는 행·열 번호)으로 검색하거나 `element_get`으로 문단 전체 텍스트와 글자 수를 확인합니다.

### ③ 텍스트 교체 (`text_replace`)
찾아낸 문단(`node-p1`)에서 앞의 5글자(`"탐구 보고서"`, 범위 `[0, 5)`)를 `"최종 연구 보고서"`로 바꿉니다. 편집 명령에는 항상 현재 버전 번호를 `expectedVersion`으로 함께 보냅니다.
```json
{
  "documentId": "doc-1",
  "expectedVersion": 0,
  "target": "node-p1",
  "range": { "start": 0, "end": 5 },
  "text": "최종 연구 보고서"
}
```
- **참고**: 실제로 바꾸기 전에 결과만 미리 확인하고 싶다면 `"dryRun": true`를 덧붙입니다.
- **응답**: 적용이 완료되면 `outcome: "applied"`와 함께 새 버전 번호 `afterVersion: 1`이 반환됩니다.
- 표 셀 전체 글을 바꿀 때는 `cell_set_text`에 셀의 `target`(`nodeId`)과 새 `text`를 넘깁니다.

### ④ 문단 정렬 및 줄간격 변경 (`paragraph_format`)
제목 문단(`node-p1`)을 가운데 정렬(`center`)하고 줄간격을 160%로 맞춥니다. 줄간격(`lineSpacing.value`)은 `1`부터 `1000`까지의 정수 백분율(`1%~1000%`)을 지정할 수 있습니다. 직전 편집으로 버전이 `1`이 되었으므로 `expectedVersion: 1`을 넣습니다.
```json
{
  "documentId": "doc-1",
  "expectedVersion": 1,
  "target": "node-p1",
  "alignment": "center",
  "lineSpacing": { "kind": "percent", "value": 160 }
}
```
- **응답**: 성공하면 버전이 `afterVersion: 2`로 올라갑니다.

### ⑤ 실행 취소 (`history_undo`)와 다시 실행 (`history_redo`)
방금 바꾼 문단 서식이 마음에 들지 않아 직전 상태로 되돌립니다. 되돌리기 역시 문서 상태를 바꾸는 작업이므로 현재 버전(`2`)을 `expectedVersion`으로 보냅니다.
```json
{
  "documentId": "doc-1",
  "expectedVersion": 2
}
```
- **응답**: 문단 서식이 이전 상태로 복원되고 버전은 `afterVersion: 3`이 됩니다(버전 번호는 줄어들지 않고 항상 앞으로 증가합니다). 다시 적용하려면 `history_redo`(`expectedVersion: 3`)를 호출합니다.

### ⑥ 문서 미리보기 (`document_preview`)
현재 문서 상태를 격리된 `rhwp` 렌더러로 그려 봅니다. 미리보기는 읽기 전용 작업이므로 문서 내용이나 버전(`revision`), 작업 이력을 전혀 바꾸지 않습니다.
```json
{
  "documentId": "doc-1",
  "pages": [1],
  "includePng": true,
  "includeSvg": false
}
```
- **요청 옵션 설명**:
  - `pages`: 돌려받을 쪽 번호 목록입니다(1부터 시작하는 쪽 번호, 최대 100개). **생략하면 기본값으로 첫 번째 쪽(`[1]`)만 반환**합니다.
  - `includePng`: 각 쪽의 PNG 이미지(Base64 인코딩 문자열 및 가로·세로 픽셀 크기)를 응답에 바로 포함할지 여부입니다. **기본값은 `true`**입니다.
  - `includeSvg`: 안전 검사를 통과한 각 쪽의 SVG 원문 텍스트를 응답에 바로 포함할지 여부입니다. **기본값은 `false`**입니다(끄더라도 각 쪽의 `svgSha256`과 `svgBytes` 크기는 함께 알려줍니다).
- **반환 방식과 4 MiB 응답 크기 상한**:
  - 작은 PNG는 `pages[].png.base64`, 선택한 SVG는 `pages[].svg`로 응답에 담습니다. JSON 이스케이프와 메타데이터까지 포함한 실제 직렬화 크기를 검사합니다.
  - 응답에 담기 어려운 쪽은 설정한 **출력 루트에 별도 파일로 저장**합니다. PNG는 `pages[].png.path`, SVG는 `pages[].svgArtifact.path`를 확인하세요. 파일마다 `sha256`·`bytes`가 있어 받은 파일을 대조할 수 있습니다. 이 경우 `PAGE_CONTENT_ARTIFACT` 안내가 붙으며, 파일 전달 자체는 실패나 부분 성공으로 계산하지 않습니다.
  - 파일 이름은 실행기가 생성하고 기존 파일은 덮어쓰지 않습니다. 출력 루트에 쌓인 프리뷰 파일의 보관·삭제는 사용자가 관리합니다. 공개 전에 쓰기나 검증이 실패하면 오류를 반환합니다. 공개 뒤 디렉터리 동기화만 실패하면 `SAVE_DURABILITY_UNCERTAIN`의 상세 정보에 이미 공개된 파일 경로와 해시가 있습니다.
  - PNG 생성이 실패하면 `pngError` 또는 `omitted: "png_failed"`를 확인하세요. 요청한 내용을 하나도 전달하지 못하면 `status`가 `failed`, 일부만 전달하면 `partial`입니다. SVG는 명시적으로 요청했을 때 제공됩니다.
- **미리보기 결과 필드 쉽게 읽기**:
  - `status` (`"rendered"` / `"partial"` / `"failed"`):
    - `"rendered"`: 요청한 쪽들이 빠짐없이 정상적으로 렌더링되어 응답에 모두 담겼다는 뜻입니다.
    - `"partial"`: 일부 쪽만 담겼거나 제한이 적용된 상태입니다. 문서 전체가 100쪽을 넘어 앞 100쪽까지만 처리된 경우(`PREVIEW_PAGE_LIMIT`), 문서 쪽수를 벗어난 쪽 번호를 함께 요청한 경우(`PAGE_NOT_RENDERED`), 안전하지 않은 SVG가 제외된 경우(`SVG_UNSAFE`), 또는 일부 쪽의 PNG를 생성하지 못한 경우에 `"partial"`이 됩니다. 전달할 쪽이 전혀 없으면 `"failed"`입니다.
    - `"failed"`: 렌더러 실행은 끝났으나 요청한 쪽 가운데 실제로 돌려줄 수 있는 유효한 쪽이 하나도 없는 상태입니다(예: 요청한 쪽 번호가 모두 전체 쪽수를 벗어났거나 안전 검사에서 모두 제외된 경우). 참고로 렌더러나 `bwrap` 자체가 준비되지 않았거나 실행 시간 초과 등이 발생하면 `status` 대신 `RENDERER_UNAVAILABLE` 오류가 반환됩니다.
  - `coverage: "unknown"`: 렌더러가 문서 안의 모든 HWPX 서식·개체를 빠짐없이 지원해 그렸는지 엔진이 자동으로 판별할 수 없다는 뜻입니다(항상 `"unknown"`으로 표시됩니다).
  - `metadata.nativeComparison: "not_checked"`: 이 미리보기 결과를 실제 Windows 한컴오피스(한글) 화면과 대조해 검증하지 않았다는 뜻입니다(항상 `"not_checked"`로 표시됩니다).

### ⑦ 프로젝트 저장 (`project_save`) 및 새 HWPX 내보내기 (`document_export`)
- **작업 이어하기용 프로젝트 저장 (`project_save`)**: 나중에 같은 요소 ID(`nodeId`)를 유지한 채 작업을 이어가고 싶다면 `--output-root` 안에 `.hwpwork` 프로젝트 파일로 저장합니다.
  ```json
  {
    "documentId": "doc-1",
    "path": "/home/user/Documents/hwpx-outputs/report_work.hwpwork"
  }
  ```
- **한컴오피스 제출용 HWPX 내보내기 (`document_export`)**: 수정이 끝난 문서를 일반 `.hwpx` 파일로 내보냅니다. 엔진은 저장 직전에 원본 보존 검사를 자동으로 수행합니다.
  ```json
  {
    "documentId": "doc-1",
    "path": "/home/user/Documents/hwpx-outputs/report_final.hwpx"
  }
  ```

---

## 5. 자주 만나는 오류·경고 코드 풀이

문서 편집 중 조건이 맞지 않으면 엔진은 문서를 망가뜨리는 대신 작업을 안전하게 거절하고 이유(`diagnostics`)를 알려줍니다.

| 코드 | 쉬운 설명 및 해결 방법 |
|---|---|
| `PACKAGE_REJECTED` | **문서 열기 거절**: 암호가 걸려 있는 문서(`encrypted`)이거나 손상된 파일, 비정상 ZIP/XML 구조일 때 발생합니다. 한컴오피스에서 암호를 해제하고 표준 `.hwpx`로 저장한 뒤 다시 열어 주세요. |
| `RESOURCE_LIMIT` | **용량·크기 한도 초과**: 파일 크기(기본 HWPX 32 MiB, 프로젝트 96 MiB), 압축 해제 크기(128 MiB), 그림 화소 수(4,096만 화소), 세션 메모리(512 MiB), 단일 요청(1 MiB)·응답(4 MiB) 등의 안전 한도를 넘었을 때 발생합니다. |
| `STATE_INVARIANT_ERROR` | **내부 상태 검증 오류**: 편집을 확정하기 직전 내부 데이터와 색인의 일관성이 맞지 않아 엔진이 변경을 취소했습니다. 원본 문서는 그대로 보존됩니다. |
| `PROJECT_INTEGRITY_ERROR` | **프로젝트 파일 손상**: `.hwpwork` 프로젝트 파일의 무결성(해시·구조) 검사가 실패했습니다. 임의로 수정되었거나 손상된 프로젝트 파일은 열 수 없습니다. |
| `PROJECT_LOCKED` | **프로젝트 잠김**: 같은 `.hwpwork` 프로젝트를 다른 프로그램이나 세션이 이미 열어 작업 중입니다. 이전 세션을 닫은(`document_close`) 뒤 다시 시도하세요. |
| `EXTERNAL_MODIFICATION` | **외부에서 파일이 변경됨**: 프로젝트를 열어 둔 사이에 디스크의 `.hwpwork` 파일이 다른 프로그램에 의해 바뀌었습니다. 덮어쓰기 사고를 막기 위해 저장을 거절합니다. |
| `SAVE_DURABILITY_UNCERTAIN` | **디스크 동기화 주의**: 새 파일은 만들어졌으나 마지막 폴더 동기화(fsync) 단계에서 운영체제 오류가 났습니다. 생성된 파일이 정상인지 확인해 주세요. |
| `VERSION_CONFLICT` | **버전 번호 불일치**: 편집 명령에 적은 `expectedVersion`이 현재 문서의 최신 `revision`과 다릅니다(예: 같은 명령이 두 번 전송됨). 문서를 다시 조회(`document_inspect` 또는 `element_query`)해 최신 버전 번호를 확인한 뒤 요청하세요. |
| `HISTORY_EMPTY` | **되돌릴 이력 없음**: 더 이상 실행 취소(`history_undo`)하거나 다시 실행(`history_redo`)할 작업 내역이 없습니다. |
| `SNAPSHOT_UNAVAILABLE` | **스냅샷 없음**: 비교(`document_diff`)나 해제(`snapshot_release`)하려는 스냅샷 ID가 존재하지 않거나 이미 해제되었습니다. |
| `SESSION_MISMATCH` | **세션 ID 불일치**: 현재 연결된 세션과 다른 `sessionId`로 요청을 보냈을 때 발생합니다. |
| `SESSION_REQUIRED` | **세션 유지 필요**: 열린 문서가 없거나, 일회성 실행 모드(`hae run`)에서 이전 호출의 실행 취소(Undo/Redo)를 시도할 때 발생합니다. 이력 기능은 한 세션 안에서만 작동합니다. |
| `UNKNOWN_WRITER` / `REQUEST_ID_CONFLICT` / `REPLAY_TOO_OLD` / `REQUEST_OUT_OF_ORDER` | **요청 순서·작성자 번호 오류**: JSON-lines 등에서 작성자 ID(`writerId`)나 요청 일련번호(`requestSequence`)가 어긋났을 때 발생합니다. (MCP 클라이언트나 `hae script`를 쓸 때는 번호를 자동 관리하므로 발생하지 않습니다.) |
| `TARGET_NOT_FOUND` | **대상을 찾을 수 없음**: 지정한 `nodeId`(`target`)가 문서에 없습니다. `document_inspect`나 `element_query`로 정확한 `nodeId`를 다시 찾아 주세요. |
| `AMBIGUOUS_TARGET` | **대상이 모호함**: 조건에 맞는 대상이 여러 개여서 하나로 특정할 수 없을 때 발생합니다. 정확한 `nodeId`를 지정하세요. |
| `UNSUPPORTED` | **지원하지 않는 편집**: 0.1에서 다루지 않는 대상을 고치려 할 때 발생합니다. (예: 병합된 표 셀, 문단이 2개 이상인 셀, 전자서명·변경 추적이 있는 읽기 전용 문서, 자르기·효과가 들어간 그림, 애니메이션 PNG 등) |
| `INVALID_RANGE` | **잘못된 글자 범위**: `text_replace`의 `[start, end)` 범위가 문단 글자 수를 벗어났거나 빈 범위(`start >= end`)입니다. 위치만 지정하는 끼워 넣기(삽입)는 지원하지 않으므로 1글자 이상의 범위를 지정하세요. |
| `RANGE_CROSSES_CONTROL` | **제어 문자를 가로지르는 범위**: 바꾸려는 텍스트 범위 안에 탭(Tab), 줄바꿈, 필드, 수식, 표, 그림 같은 제어 요소가 끼어 있습니다. 제어 요소 앞뒤로 범위를 나누어 일반 텍스트 구간만 교체하세요. |
| `UNSUPPORTED_CONTENT_IN_RANGE` | **범위 내 알 수 없는 요소**: 지정한 텍스트 범위 안에 엔진이 해석할 수 없는 특수 속성이나 하위 요소가 들어 있어 안전을 위해 편집을 거절했습니다. |
| `FORMAT_COLLAPSED` *(경고)* | **글자 모양 통합 경고**: 서로 다른 글자 모양(글꼴·크기·색상 등)이 적용된 여러 구간을 한 번에 교체하여, 새 글자가 첫 글자의 모양으로 통일되었음을 알려주는 경고입니다. |
| `RENDERER_UNAVAILABLE` | **미리보기 렌더러 사용 불가**: `document_preview`를 호출했으나 외부 렌더러(`HAE_RHWP`)가 설정되지 않았거나(`renderer_not_configured`), 실행 파일이 없거나(`renderer_missing`), `bubblewrap`(`HAE_BWRAP`) 격리 자가 점검에 실패했거나(`sandbox_unverified`), 글꼴 폴더(`HAE_FONT_DIR`)를 읽을 수 없거나(`font_dir_unreadable`), 렌더링 제한 시간(60초)을 넘겼을 때(`timeout`) 발생합니다. 미리보기만 실패할 뿐 문서 내용은 전혀 바뀌지 않습니다. |
| `PREVIEW_PAGE_LIMIT` *(경고)* | **미리보기 100쪽 제한 경고**: 문서 전체가 100쪽을 넘어 앞의 100쪽까지만 렌더링했습니다(결과 `status`는 `"partial"`이 됩니다). |
| `INVALID_REQUEST` / `UNKNOWN_COMMAND` | **잘못된 요청 형식 / 알 수 없는 명령**: 명령 이름이 틀렸거나, 필수 인자가 빠졌거나, 줄간격 백분율(`1`~`1000`)·미리보기 쪽 번호(`1`~`100`개, 1 이상) 범위를 벗어났거나, 새 텍스트에 줄바꿈·탭 문자를 넣었을 때 발생합니다. |
| `PATH_NOT_ALLOWED` | **허용되지 않은 파일 경로**: `--input-root` 또는 `--output-root` 바깥의 경로에 접근하려 했거나, 심볼릭 링크를 거치거나, 원본 파일 덮어쓰기를 시도했을 때 거부합니다. |
| `TARGET_EXISTS` | **파일이 이미 존재함**: `document_export`나 첫 `project_save`를 할 때 지정한 경로에 이미 파일이 있으면 기존 파일을 보호하기 위해 저장을 거절합니다. 새 파일 이름을 지정하세요. |
| `VERIFY_FAILED` | **저장 전 보존 검증 실패**: 파일을 디스크에 최종 기록하기 전에 엔진이 다시 읽어 검사하는 과정에서 지정하지 않은 변경이나 구조 이상이 감지되어 파일 생성을 중단했습니다. |
| `IO_ERROR` | **파일 입출력 오류**: 파일이 없거나 디스크 권한·용량 문제로 읽기/쓰기에 실패했습니다. |

---

## 6. 꼭 알아 두어야 할 안전 수칙

1. **원본 파일은 절대 덮어쓰지 않습니다**: `document_open`으로 연 원본 `.hwpx` 파일과 `image_replace`에 사용한 이미지 파일은 읽기 전용으로만 다루며, 어떤 경우에도 원본 파일을 수정하거나 덮어쓰지 않습니다.
2. **내보내기(`document_export`)는 항상 새 파일로만 저장합니다**: 이미 존재하는 파일 경로로 `.hwpx`를 내보내려고 하면 `TARGET_EXISTS` 오류를 내며 거절합니다. 결과물을 저장할 때는 항상 새로운 파일 이름을 지정하세요.
3. **지정된 폴더(`--input-root`, `--output-root`) 안에서만 작동합니다**: AI 에이전트가 실수로 시스템의 다른 폴더나 개인 파일을 읽거나 쓰지 못하도록 입출력 허용 폴더 경계를 엄격하게 검사합니다.
4. **저장 전 독립 검증을 거칩니다**: 문서를 내보내거나 프로젝트를 저장할 때 임시 파일을 먼저 다시 파싱하여 요청한 부분만 바뀌었는지, 나머지 서식과 미디어가 온전히 보존되었는지 확인한 뒤에야 최종 파일을 만듭니다.

### 폰트와 프리뷰 자원 예산

`HAE_FONT_DIR`에는 필요한 글꼴만 모은 디렉터리를 지정하세요. 프리뷰는 폰트 파일 합계 64 MiB, 파일당 32 MiB 이하를 받고, 세션의 남은 메모리가 적으면 더 일찍 거절합니다. 대형 시스템 폰트 폴더 전체를 지정하면 `RENDERER_UNAVAILABLE(memory_limit)`가 발생할 수 있습니다. 로컬에 설치된 글꼴을 선별해 사용하는 것과 글꼴 파일을 배포하는 것은 별개입니다.

렌더러 자식 프로세스는 `prlimit`의 `RLIMIT_AS`를 상속받습니다. 이는 프로세스별 가상 주소 공간 상한이며, 전체 프로세스 트리의 RSS 합계를 보장하는 cgroup 한도는 아닙니다. 엔진의 SVG 안전 검사·폰트 바이트 처리에는 보수적 예산을 적용하고, PNG 변환에는 별도 작업 프로세스의 주소 공간·시간·출력 상한을 적용합니다. 이 회계값을 엔진 프로세스 전체의 OS 메모리 상한으로 해석하지 마세요. `coverage`와 실제 한컴 비교 여부는 이 제한과 별도로 표시됩니다.


### PNG 작업 파일 배치

PNG 변환에는 같은 빌드의 `hae-raster`가 필요합니다. 저장소 루트에서 `cargo build --workspace --release`로 함께 빌드하고 `hae`·`hae-jsonl`·`hae-mcp`와 같은 폴더에 배치하세요. 다른 위치라면 `HAE_RASTER`에 절대 경로를 지정합니다. PATH는 검색하지 않습니다. 작업 파일이 없거나 실패했을 때 엔진 프로세스 안에서 PNG 변환을 대신 실행하지 않습니다.

작업 파일은 읽기 전용 SVG·폰트 스냅샷을 받아 네트워크가 차단된 격리 안에서 실행하며, 프로세스별 주소 공간·시간·출력 상한을 적용합니다. 이 한도와 일반 엔진 메모리 회계값을 전체 시스템 RSS나 디스크 하드 quota로 해석하지 마세요. 실제 지원·실패 범위는 `docs/decisions/resumption-review/raster-worker.md`의 검증 기록을 따릅니다.
