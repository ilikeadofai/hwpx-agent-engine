# HWPX 문서 엔진 (0.3) 사용 안내서

이 안내서는 한컴오피스(한글)로 작성된 `.hwpx` 보고서나 양식 문서를 Claude, Codex 같은 AI 에이전트에게 맡겨 안전하게 다듬고자 하는 선생님과 학생을 위해 작성되었습니다.

---

## 1. 이 엔진은 무엇을 하고, 무엇을 하지 않나요?

**HWPX 문서 엔진**은 기존 `.hwpx` 문서에서 **지정한 부분만 정확하게 고치고, 수정하지 않은 나머지 내용·서식·표·그림·수식을 원본 그대로 보존**하는 독립 도구입니다.

### 할 수 있는 것 (0.3 지원 범위)
- **한컴오피스 불필요**: Linux 환경에서 한컴오피스 설치나 라이선스 없이 바로 작동합니다.
- **문서 구조 및 요소 조회**: 구역, 문단, 표, 표 셀, 그림, 수식의 구조와 텍스트를 읽고 검색합니다.
- **문단 텍스트 범위 교체 (`text_replace`)**: 한 문단의 `[start, end)` grapheme 범위를 바꾸거나 지우며, 길이 0 범위에 새 글을 삽입합니다. `controls: "preserve"`로 개체를 보존하면서 범위 안 텍스트만 바꿀 수 있습니다.
- **문서 전체 바꾸기 (`text_replace_all`)**: 문서 전체(또는 `scope`로 고른 표·셀·문단 아래)에서 같은 글자를 정확히 찾아 한 번에 바꿉니다. 개체·탭·줄바꿈을 가로지르거나 자소 경계가 맞지 않는 일치는 바꾸지 않고 건너뛴 위치를 알려 줍니다. 전체가 한 번의 되돌리기 단위입니다.
- **문단 서식 변경 (`paragraph_format`)**: 문단의 정렬 방식(왼쪽·가운데·오른쪽·양쪽·배분·나눔 정렬)과 백분율 줄간격(`1%~1000%`, 정수 `1`~`1000`)을 바꿉니다. 문서 전체 스타일은 건드리지 않고 대상 문단에만 적용합니다.
- **글자 서식 변경 (`text_format`)**: 문단 안의 자소 범위만 굵게·기울임·밑줄·취소선·글자 크기(`100`~`100000` HWPUNIT)·글자색(`#RRGGBB`)으로 바꿉니다. 값이 같은 기존 글자 모양을 재사용하거나 하나를 새로 만들어 붙이고, 범위 밖 글자의 모양은 그대로 둡니다.
- **표 셀 텍스트 교체 (`cell_set_text`)**: 병합 기준 셀을 포함해 기존 셀의 텍스트를 바꿉니다. 문단 하나는 `text`, 여러 문단은 정확한 기존 문단 수의 `paragraphs` 배열을 줍니다.
- **라벨로 양식 채우기 (`form_fill`)**: 표에서 라벨 글자("성명" 등)와 정확히 같은 셀을 찾아, 같은 행 오른쪽(없으면 바로 아래)의 **비어 있는** 셀에 값을 채웁니다. 실패한 라벨은 사유와 함께 보고하고, 기본 `strict`에서는 하나라도 실패하면 아무것도 바꾸지 않습니다.
- **그림 교체 (`image_replace`)**: 기존 그림의 위치·크기·캡션을 그대로 유지한 채, 정적 PNG 또는 JPEG 이미지 파일로 교체합니다.
- **문단 추가·삭제 (0.4, `paragraph_insert`·`paragraph_delete`)**: 기존 문단 앞이나 뒤에 새 문단을 넣거나, 같은 컨테이너의 문단을 이어서 지웁니다. 새 문단은 대상 문단의 문단 모양·첫 run의 글자 모양을 복제하고 글은 새로 넣거나 비웁니다. 표·개체·필드·각주가 든 문단과 구역 속성을 담은 문단은 지우지 않습니다(아래 ④-5).
- **새 문서 만들기 (0.4, `document_create`)**: 제목과 문단 목록으로 한컴이 열 수 있는 최소 HWPX를 처음부터 만들어 출력 폴더에 새 파일로 씁니다. 만든 파일은 바로 `document_open`으로 열어 편집할 수 있습니다(아래 ⑨).
- **표 행·열 추가·삭제 (0.3, `table_insert_row`·`table_delete_row`·`table_insert_column`·`table_delete_column`)**: 기존 표에 빈 행·열을 넣거나 지웁니다. 새 셀은 이웃 행(열)의 속성·테두리를 복제하고 글은 비웁니다. 엔진이 처음으로 **노드를 만들고 지우는** 명령이므로 새 `nodeId`와 되돌리기 규칙이 따로 있습니다(아래 ④-4).
- **구역 용지·여백·쪽 번호·표시 설정 (0.3, `section.*`)**: 기존 구역 하나의 용지 방향·크기·제본 방식·여백, 쪽 시작 방식과 시작 번호, 첫 쪽 머리말·꼬리말·쪽 번호 숨김과 테두리·채우기 표시, 격자를 조회하고(`section_get_page`) 지정한 항목만 바꿉니다(`section_set_page`, `section_set_page_numbering`, `section_set_visibility`). 조회 값은 XML에 명시된 것만 `explicit`으로, 없는 값은 `unspecified`로 구분해 알려 주며 상속·기본값을 추측하지 않습니다. 자세한 내용은 아래 0.3절 「구역 용지·여백·쪽 번호·표시 설정」을 보세요.
- **Markdown·HTML로 내보내기 (`document_export_text`)**: 문서(또는 한 구역)를 읽기 전용으로 변환해 새 `.md`/`.html` 파일로 씁니다. 문서·`revision`·되돌리기 이력은 바뀌지 않습니다(아래 ⑧).
- **누름틀(필드) 조회와 값 채우기 (`field_list`·`field_set_text`, 0.3)**: 공문서 서식의 누름틀(`CLICK_HERE`)·하이퍼링크(`HYPERLINK`) 필드를 찾아 이름·ID·Command 원문·현재 값·채우기 가능 여부를 조회하고, 필드 마커와 서식을 그대로 둔 채 표시 텍스트만 채웁니다.
- **구형 `.hwp`(HWP5) 읽기 전용 조회 (`hwp5_inspect`, 0.3)**: HWPX와 별개 crate(`hae-hwp5`)로 구형 HWP5 바이너리를 직접 열어 버전·압축·암호화 여부, 구역·문단·글자 수와 문단별 텍스트 앞부분을 돌려줍니다. 열린 문서가 필요 없고 파일을 바꾸지 않습니다(아래 0.3절 「구형 .hwp(HWP5) 읽기 전용 조회」).
- **되돌리기·다시 실행 및 미리보기**: 작업 전 가상 적용(`dryRun`), 변경 내역 비교(`document_diff`), 실행 취소(`history_undo`)·다시 실행(`history_redo`), 외부 렌더러 연동을 통한 PNG/SVG 미리보기(`document_preview`)를 지원합니다.

### 하지 않는 것 및 한계 (0.3 기준 제외 범위)
- **전체 HWP/HWPX 기능 지원 아님**: 0.4는 **새 문서 하나와 기존 구역·표 셀 안의 새 문단**을 만들 수 있고, 문단을 이어서 지울 수 있습니다. 그 밖에 문단 나누기·합치기·이동, 다른 구역이나 다른 문서에서 복사, 셀 병합·분할, 병합 셀을 가르는 행·열 삽입과 병합 셀이 걸린 행·열 삭제, 표 자체의 생성·삭제, 수식·그림·도형·필드·스타일 정의 새로 만들기, 각주·미주·글상자 안 문단의 추가·삭제는 지원하지 않습니다. 새로 만들 수 있는 노드는 `paragraph.insert`의 문단과 0.3 표 명령의 빈 셀·문단, 그리고 `document.create`가 만드는 문서 전체뿐입니다. 구형 `.hwp`(HWP5 바이너리)는 읽기 전용 조회(`hwp5.inspect`)만 지원하며 편집·쓰기·HWPX 변환은 지원하지 않습니다.
- **읽기 전용 문서**: 암호화된 문서는 열지 않으며, 전자서명이나 변경 추적(Track Changes)이 들어 있는 문서는 열어서 내용을 조회하거나 무편집 상태로 내보내는 것만 허용하고 편집은 거절합니다. 변경 추적 표시와 작성자는 `trackchange_list`로 확인합니다.
- **외부 렌더러 별도 준비 필요**: 이 엔진에는 미리보기 렌더러가 내장되어 있지 않습니다. `document_preview`를 사용하려면 별도의 `rhwp` 실행 파일, `bubblewrap`(`bwrap`) 샌드박스 도구, 읽을 수 있는 글꼴 폴더를 환경 변수(`HAE_RHWP`, `HAE_BWRAP`, `HAE_FONT_DIR`)로 직접 지정해야 합니다.
- **엔진 구현 완료 범위와 Windows 한컴오피스 실기 호환성(단계 4 예정)의 구분**: 현재 엔진의 파일 편집·보존 검증·격리 미리보기 기능은 구현되어 있으나, 수정된 문서를 실제 Windows 한컴오피스(한글)에서 열었을 때의 실기 호환성 및 조판 대조 검증(단계 4)은 별도 확인 대상입니다. `document_preview`가 만드는 SVG/PNG 이미지와 쪽 번호는 외부 렌더러(`rhwp`)가 어림잡아 계산한 **추정치**이므로 실제 Windows 한컴오피스의 줄바꿈·쪽 나눔·표현 범위와 100% 일치한다고 보장하지 않습니다.

### 할 수 있는 것 (0.3 추가, 읽기 전용 조회)

- **링크 목록 조회 (`link_list`)**: 본문 하이퍼링크 필드의 문단 `nodeId`, `fieldid`/`id`, `hp:parameters` 원문(`Command`·`Path` 등), 표시 텍스트, 참조가 내부 문서인지 외부인지, 커서 페이징을 제공합니다.
- **변경 추적 목록 조회 (`trackchange_list`)**: 삽입·삭제 표시(`insertBegin`/`insertEnd`/`deleteBegin`/`deleteEnd`)의 문단 위치·종류·오프셋과 머리말 `hh:trackChange`/`hh:trackChangeAuthor` 작성자 연결을 보여 줍니다. 변경 추적 문서는 계속 읽기 전용이며 조회만 허용됩니다.
- **책갈피 목록 조회 (`bookmark_list`)**: 책갈피 표식(`hp:ctrl/hp:bookmark`, `hp:fieldBegin type="BOOKMARK"`)의 이름, 담은 문단 `nodeId`, 문단 안 자소 위치, 짝이 맞는 범위 끝, `hp:parameters` 원문을 보여 줍니다. 이름이 없는 책갈피는 `null`로 두고 지어내지 않습니다.
- **메모 목록 조회 (`memo_list`)**: 메모(`hp:fieldBegin type="MEMO"`, `hp:memo`)의 `id`, 작성자, 작성 시각, 모양 참조, 달린 문단과 자소 위치, 본문 문단(`nodeId`·텍스트·미리보기), 답글 관계를 보여 줍니다. XML에 없는 속성은 `null`로 둡니다.

두 명령의 자세한 인자·결과는 아래 §0.3 절을 참고하세요.

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

Usage: hae-mcp [OPTIONS] [COMMAND]

Commands:
  doctor  Check preview executables, font directory and input/output roots; print JSON
  help    Print help

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
3. **`HAE_FONT_DIR`** *(기본값: `/usr/share/fonts`)*: 렌더링과 PNG 변환에 사용할 글꼴(`.ttf`, `.otf`, `.ttc`, `.otc`, 대소문자 구분 없음)이 들어 있는 **읽기 가능한 폴더의 절대 경로**입니다. **한국어 글꼴 몇 개만 모은 작은 폴더**(예: `/home/user/hwpx-fonts`)를 권장합니다. 기본 시스템 폴더 전체는 상한을 넘겨 미리보기에 실패할 수 있습니다. 제한은 **글꼴 파일 4,096개, 총 64 MiB, 파일당 32 MiB**이며 메타데이터 작업 공간에도 상한을 적용합니다. 재귀 깊이는 8, 방문 항목은 8,192개로 제한하고 심볼릭 링크는 건너뜁니다. 세션의 남은 메모리에 따라 더 일찍 거절할 수 있습니다. 문서에 필요한 한국어 글꼴을 포함하고 `hae-mcp doctor`로 먼저 점검하세요.

### MCP 설정 JSON 예시 (`hae-mcp`)

Claude Desktop처럼 `mcpServers` 형식을 쓰는 MCP 클라이언트에 아래와 같이 `command`, `args`, `env`를 등록합니다. 입출력 폴더는 미리 만들어 두고 본인의 환경에 맞는 절대 경로로 바꾸어 주세요.

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

### 설치·환경 점검

별도 설치 폴더를 쓰려면 release 빌드 뒤 `hae-mcp`와 `hae-raster`를 같은 폴더에 복사합니다. 다음은 사용자 폴더에 설치하는 명령 예시입니다.

```bash
cargo build --release --locked -p hae-mcp -p hae-render-rhwp
install -Dm755 target/release/hae-mcp "$HOME/.local/bin/hae-mcp"
install -Dm755 target/release/hae-raster "$HOME/.local/bin/hae-raster"
hae-mcp --version
HAE_RHWP=/opt/rhwp/bin/rhwp hae-mcp doctor \
  --input-root /home/user/Documents/hwpx-inputs \
  --output-root /home/user/Documents/hwpx-outputs
```

`doctor`는 실행 파일·폴더·입출력 권한과 미리보기와 동일한 제한 스캔의 글꼴 파일 수·용량을 JSON으로 보고합니다. 상한 초과는 fonts의 `ok:false`와 해결 안내로 표시하며, `measurementsComplete:false`이면 표시된 수치가 전체 합계의 하한입니다. 한국어 글꼴이 없을 가능성은 `warning`으로 알립니다. 비용이 작은 파일명 휴리스틱(Noto Sans/Serif KR·CJK KR, Nanum, Malgun, Batang, Gulim, Dotum 및 한글 이름)을 사용하며 cmap이나 글꼴 유효성은 검사하지 않습니다. 따라서 경고 유무는 실제 한글 지원을 보장하지 않고, 빈 폴더도 상한 점검은 통과하되 경고가 나옵니다. 모두 정상이면 종료 0, 빠진 항목이 있으면 1입니다. 렌더러가 없어도 조회·편집·내보내기는 쓸 수 있습니다. doctor는 렌더러나 한컴을 실행하지 않으므로 sandbox·한글 글꼴·실제 조판은 미리보기와 화면 확인으로 검증합니다.

### Claude Code·Codex·opencode·Inspector 연결

아래는 사용자가 선택해 적용하는 예시입니다. 이 작업에서는 전역 설정을 변경하지 않았습니다. 실행 파일과 입출력 폴더의 절대 경로를 본인 환경에 맞게 바꾸세요.

Claude Code의 프로젝트 범위 등록([공식 안내](https://code.claude.com/docs/en/mcp)):

```bash
claude mcp add --scope project --transport stdio hwpx-engine \
  -e HAE_RHWP=/opt/rhwp/bin/rhwp -e HAE_FONT_DIR=/home/user/hwpx-fonts \
  -- /home/user/.local/bin/hae-mcp \
  --input-root /home/user/Documents/hwpx-inputs \
  --output-root /home/user/Documents/hwpx-outputs
```

Codex CLI 등록 명령([공식 OpenAI 안내](https://learn.chatgpt.com/docs/extend/mcp?surface=cli)):

```bash
codex mcp add hwpx-engine \
  --env HAE_RHWP=/opt/rhwp/bin/rhwp --env HAE_FONT_DIR=/home/user/hwpx-fonts \
  -- /home/user/.local/bin/hae-mcp \
  --input-root /home/user/Documents/hwpx-inputs \
  --output-root /home/user/Documents/hwpx-outputs
```

Codex에서 프로젝트 설정 파일을 직접 관리할 경우 `.codex/config.toml` 예시는 다음과 같습니다. 위 CLI 등록과 이 파일 예시 중 하나를 선택합니다.

```toml
[mcp_servers.hwpx-engine]
command = "/home/user/.local/bin/hae-mcp"
args = ["--input-root", "/home/user/Documents/hwpx-inputs", "--output-root", "/home/user/Documents/hwpx-outputs"]
startup_timeout_sec = 10
tool_timeout_sec = 120

[mcp_servers.hwpx-engine.env]
HAE_RHWP = "/opt/rhwp/bin/rhwp"
HAE_FONT_DIR = "/home/user/hwpx-fonts"
```

opencode는 프로젝트의 `opencode.json`에 local 서버를 지정합니다([공식 안내](https://opencode.ai/docs/mcp-servers/)).

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "hwpx-engine": {
      "type": "local",
      "command": ["/home/user/.local/bin/hae-mcp", "--input-root", "/home/user/Documents/hwpx-inputs", "--output-root", "/home/user/Documents/hwpx-outputs"],
      "environment": {"HAE_RHWP": "/opt/rhwp/bin/rhwp", "HAE_FONT_DIR": "/home/user/hwpx-fonts"},
      "enabled": true
    }
  }
}
```

Inspector CLI는 전역 설치 없이 실제 서버의 도구 목록과 strict 스키마를 확인할 수 있습니다.

```bash
npx -y @modelcontextprotocol/inspector --cli /home/user/.local/bin/hae-mcp \
  --input-root /home/user/Documents/hwpx-inputs \
  --output-root /home/user/Documents/hwpx-outputs \
  --method tools/list --strict
npx -y @modelcontextprotocol/inspector --cli /home/user/.local/bin/hae-mcp \
  --input-root /home/user/Documents/hwpx-inputs \
  --output-root /home/user/Documents/hwpx-outputs \
  --method tools/call --tool-name command_describe --tool-arg command=text.replace
```

`--strict`는 tools/list에서만 사용합니다. Inspector CLI는 실행마다 새 서버를 띄우므로 문서 열기 후 다른 CLI 호출에서 이어 편집할 수 없습니다. 연속 편집은 한 클라이언트 세션에서 실행합니다. Claude Code·Codex·opencode 예시는 연결 방법이며 해당 에이전트의 실제 세션 시험(M3) 결과와 구분합니다.

### 프로토콜·오류·응답 규칙

기존 클라이언트는 initialize → notifications/initialized 뒤 도구를 호출합니다(2025-11-25·2025-06-18·2025-03-26·2024-11-05). 초기화 전 도구 요청은 거절하고 ping은 허용합니다. 2026-07-28 클라이언트는 요청별 `_meta`에 버전과 clientCapabilities를 넣으며, 선택적으로 server/discover로 지원 버전을 확인합니다. 현대식 문서 도구에는 documentId가 필수입니다. 이 핸들은 문서 닫기 또는 서버 종료까지 유효합니다.

모든 도구는 outputSchema·4종 annotations를 제공하며, structuredContent는 content[0].text의 JSON 봉투와 같습니다. 잘못된 JSON·메서드·tools/call 외형은 JSON-RPC error, 알려진 도구의 입력 값·업무 오류는 isError:true와 diagnostics로 반환합니다. 상세 리비전 근거와 시간 상한 판단은 [M1·M2 기록](decisions/mcp-m1-m2-2026-10-18.md)을 참고하세요.

요청은 순차 처리하며 실행 도중 선점 취소와 전체 요청의 강제 시간 상한은 없습니다. 렌더러 한도는 기본 60초입니다. 클라이언트 대기 시간이 지난 경우 결과가 불명확할 수 있으므로, 프로세스 재시작 후 저장본을 열어 확인합니다. 내부 panic은 -32603을 반환하고 다음 편집은 차단하므로 서버를 재시작해야 합니다.

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
  HAE_FONT_DIR=/home/user/hwpx-fonts \
  /opt/hwpx-agent-engine/target/release/hae script \
    --input-root /home/user/Documents/hwpx-inputs \
    --output-root /home/user/Documents/hwpx-outputs \
    /home/user/Documents/hwpx-inputs/steps.json
  ```
- **JSON-lines 서버 실행 (`hae-jsonl`)**:
  ```bash
  HAE_RHWP=/opt/rhwp/bin/rhwp \
  HAE_BWRAP=/usr/bin/bwrap \
  HAE_FONT_DIR=/home/user/hwpx-fonts \
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
- **응답**: 조건에 맞는 요소의 고유 ID(`nodeId`), 텍스트 미리보기와 명령별 `edits`가 반환됩니다. **노드의 `editable`은 `edits` 중 하나라도 `available: true`라는 뜻**입니다. 텍스트 교체가 된다는 뜻으로 사용하지 마세요. 예를 들어 객체만 있는 문단도 `paragraph.format`이 가능하면 `editable: true`입니다.
- 문단의 `edits["text.replace"].available`을 확인한 뒤, 기본 `controls: "reject"` 교체는 `textSpans` 한 구간 안의 범위를 고릅니다. 삽입은 `modes`에 `insert`가 있을 때 같은 구간의 내부·경계에 길이 0 범위를 줍니다. `preserve`의 개체 포함 범위 규칙은 아래 0.2 설명을 따릅니다. `textSpans`는 객체·제어 문자·해석 불가 텍스트 속성을 건드리지 않는 최대 구간이며, 기존 `range`와 같은 **extended grapheme cluster의 반열린 `[start,end)` 오프셋**을 사용합니다. 여러 run·문자 참조에 걸친 안전한 텍스트는 한 구간이 될 수 있습니다. CDATA 내용과 리터럴 탭·줄바꿈은 제외하며, 주석·PI·빈 CDATA 경계에서는 구간을 나눕니다. 인접한 두 구간의 end/start가 같아도 합쳐서는 안 됩니다.
- 문단당 앞의 64개 구간까지 제공합니다. `textSpansTruncated: true`이면 뒤쪽 구간이 생략된 것이므로 목록 밖 전체를 편집 불가로 단정하지 마세요. 기본 인자에 맞는 구간이 없으면 `no_text_span`이며, 빈 문단은 `{start:0,end:0}` 삽입을 광고합니다(실행할 run이 없으면 `no_run`). `no_text_span` 문단도 일부 범위의 `controls:"preserve"` 교체가 가능할 수 있습니다. 광고는 기본 인자 기준이고 preserve 전용 범위를 전부 나열하지 않습니다. 문서 권한·노드 잠금이 막으면 명령이 모두 비활성화되고 `textSpans`는 빈 목록입니다.
- 미해결 문서 자원 참조·중복 정의 ID는 `verify_invalid_references`, 최상위 문단 없는 중첩 대상은 `verify_no_top_level_paragraph`로 편집 명령이 비활성화됩니다. CDATA·주석/PI 교차·리터럴 제어 문자 때문에 셀 전체 교체가 불가능하면 `verify_unsupported_markup`, 중첩 셀이면 `nested_cell`을 보고합니다. 값이 바뀌지 않는 no-op 성공은 실제 편집 가능성을 뜻하지 않습니다.
- 셀의 `cell.setText`가 `nested_table`·`no_run` 등으로 불가능해도 `childParagraphsEditable: true`일 수 있습니다. 이는 자손 문단 중 적어도 하나가 `text.replace` 가능하다는 뜻입니다. `ancestor: "<셀 nodeId>"`, `kind: "paragraph"`로 자손을 찾아 각 문단의 `edits`와 `textSpans`를 확인합니다.
- 같은 개요 계약을 `document_inspect.items`, `element_query.matches`, `element_get.outline`에 제공합니다. inspect 기본 페이지는 100개(최대 500개)입니다. 조회 최상위 `editable`(inspect/open)은 문서 전체 권한을 뜻하며 개별 노드의 편집 가능성과 구분합니다. `edits[명령].requires`가 있으면 그 인자를 반드시 함께 보냅니다. 예를 들어 `requires:{controls:"preserve"}`인 셀과 preserve 전용 `modes:["replace"]` 문단은 기본 `controls:"reject"` 대신 preserve가 필요합니다. 다문단 셀의 `requires:{paragraphs:3}`은 `paragraphs`에 정확히 문자열 3개의 배열을 보내라는 뜻입니다(숫자 3을 전달하는 것이 아닙니다). 개체도 있으면 `requires:{paragraphs:3,controls:"preserve"}`로 두 요건이 함께 표시됩니다. `requires`가 없으면 광고된 구간·모드는 기본 인자를 기준으로 합니다. `available`은 현재 노드에 지원되는 편집 경로가 있다는 뜻으로, 잘못된 인자·버전·자원 한도까지 면제하지는 않습니다.
- 표의 특정 칸을 찾을 때는 `kind: "cell"`, `table: "<표 nodeId>"`, `row: 0`, `col: 0`(0부터 시작하는 행·열 번호)으로 검색하거나 `element_get`으로 문단 전체 텍스트와 글자 수를 확인합니다.

### ②-2 문서 요약 통계 조회 (`document_stats`, `document.stats`)
문서의 전체 규모와 구조적 요소를 한눈에 파악합니다. 큰 문서를 다루거나 에이전트가 작업 계획을 세울 때 전체 문단·표·셀·그림·수식·각주·미주·메모 수와 전체 글자 수(grapheme 기준)를 빠르게 확인할 수 있습니다.
```json
{
  "documentId": "doc-1"
}
```
특정 구역·표·셀·문단 아래만 집계하려면 `scope`에 해당 요소의 `nodeId`를 지정합니다.
```json
{
  "documentId": "doc-1",
  "scope": "n-table1"
}
```
- **응답**: `revision`, `scope`, `sections`(구역 수), `topLevelParagraphs`(구역 직속 최상위 문단 수), `paragraphs`(전체 문단 수), `tables`(표 수), `cells`(셀 수), `pictures`(그림 수), `equations`(수식 수), `fields`(필드 시작 태그 수), `footnotes`(각주 수), `endnotes`(미주 수), `notes`(각주+미주 총합), `memos`(메모 수), `totalChars`(grapheme 기준 전체 글자 수), `totalNodes`(고유 nodeId를 부여받은 노드 총수)가 반환됩니다.
- **읽기 전용 안전성**: 문서를 전혀 수정하지 않으며, `expectedVersion`이 필요하지 않습니다. 응답의 `revision`으로 조회 기준 시점을 확인합니다.

### ②-3 구역 및 용지·단 목록 조회 (`section_list`, `section.list`)
문서의 구역(Section) 목록과 각 구역의 용지 크기, 방향, 여백, 단(Column) 설정, 시작 쪽 번호를 content.hpf spine 순서대로 돌려줍니다.
```json
{
  "documentId": "doc-1",
  "limit": 10
}
```
- **응답**: `revision`, `total`(전체 구역 수), `nextCursor`(다음 페이지 커서), 그리고 구역 배열 `sections`가 반환됩니다. 각 구역 객체는 `section`(0부터 시작하는 spine 순서), `nodeId`, `paragraphs`(구역 내 문단 수), `topLevelParagraphs`(최상위 문단 수), `page`(용지 설정), `column`(단 설정)을 포함합니다.
- **추측 금지와 출처 명시**: XML에 명시적인 용지 설정(`secPr`/`pagePr`)이나 단 설정(`colPr`)이 없으면 기본값을 임의로 지어내지 않고 `"source": "unspecified"`와 함께 속성값을 `null`로 보고합니다. 명시되어 있는 경우 `"source": "explicit"`와 함께 원본 속성(`rawOrientation: "WIDELY" | "NARROWLY"`, 정규화된 `orientation: "portrait" | "landscape"`, HWPUNIT 단위의 `width`, `height`, 여백 `left`, `right`, `top`, `bottom`, `header`, `footer`, `gutter`, 단 수 `colCount`, 단 종류 `colType`)을 사실 그대로 보고합니다.
- **커서 페이징**: 큰 문서에서 응답 예산을 초과하지 않도록 `limit`(기본 100, 최대 1000)과 `cursor`(`"<revision>:<offset>"`)를 지원합니다.

### ②-4 각주 및 미주 목록 조회 (`note_list`, `note.list`)
문서에 포함된 각주(`footnote`)와 미주(`endnote`) 목록을 번호, 담고 있는 본문 문단 nodeId, 텍스트, 참조 위치와 함께 돌려줍니다.
```json
{
  "documentId": "doc-1",
  "kind": "footnote",
  "limit": 50
}
```
- **응답**: `revision`, `total`, `nextCursor`, 그리고 각주·미주 항목 배열 `notes`가 반환됩니다. 각 항목은 `kind`(`"footnote"` 또는 `"endnote"`), `number`(정수 번호 또는 `null`), `rawNumber`(XML 원문 표기), `paragraphNodeId`(주석 내부 본문 문단의 nodeId), `text`(주석 본문 텍스트), `referenceParagraphNodeId`(해당 주석이 참조된 본문 문단의 nodeId)를 담습니다.
- **추측 금지**: XML 속성에 번호가 없으면 임의의 번호를 생성하지 않고 `number: null`로 둡니다.
- **필터 및 페이징**: `kind`에 `"footnote"` 또는 `"endnote"`를 지정해 원하는 주석만 걸러낼 수 있으며, `limit`와 `cursor`로 페이징을 지원합니다.

### ③ 텍스트 교체 (`text_replace`)
문단의 `edits["text.replace"].available`이 true이고 `[0,5)`가 한 `textSpans` 구간에 포함되는지 확인합니다. 찾아낸 문단(`node-p1`)에서 앞의 5글자(`"탐구 보고서"`, 범위 `[0, 5)`)를 `"최종 연구 보고서"`로 바꿉니다. 편집 명령에는 항상 현재 버전 번호를 `expectedVersion`으로 함께 보냅니다.
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
- 표 셀 전체 글을 바꿀 때는 `cell_set_text`에 셀의 `target`(`nodeId`)과 새 `text`(한 문단) 또는 `paragraphs`(기존 문단 수와 같은 문자열 배열)를 넘깁니다. 개체 보존이 필요하면 `controls: "preserve"`를 줍니다.

### ③-2 문서 전체 바꾸기 (`text_replace_all`)
같은 글자가 여러 곳에 나올 때는 문단마다 범위를 계산하지 않고 한 번에 바꿉니다. 먼저 `dryRun`으로 일치 목록(`matches`: `nodeId`, `range`, 앞뒤 미리보기 `before`/`after`)을 확인한 뒤 같은 요청을 `dryRun` 없이 보냅니다.
```json
{
  "documentId": "doc-1",
  "expectedVersion": 2,
  "find": "탐구",
  "text": "연구",
  "dryRun": true
}
```
- **찾기**: `find`는 비어 있지 않은 글자이며 유니코드 코드포인트가 정확히 같은 곳만 일치합니다. 한 문단 안에서는 앞에서부터 겹치지 않게 고릅니다(`"aaaa"`에서 `"aa"`는 2곳). 표 셀 안 문단도 포함합니다. `scope`에 nodeId를 주면 그 문단과 그 아래 문단(표를 고르면 모든 셀)만 찾습니다.
- **바꾸기**: `text`에는 탭·줄바꿈을 넣을 수 없고, 빈 문자열이면 일치한 글자를 지웁니다. 일치 구간이 문단의 `textSpans` 안에 온전히 들어간 곳만 바뀝니다.
- **건너뜀**: 아래 일치는 바꾸지 않고 `skipped`(사유별 `count`와 위치 `locations`의 `nodeId`·`range`, 사유마다 최대 50곳)와 `MATCHES_SKIPPED` 경고로 알립니다. `crosses_control`(개체·탭·줄바꿈·필드 표식을 가로지름), `grapheme_boundary`(예: 결합 문자 `é`의 `e`만 일치), `unsupported_markup`(CDATA·주석·해석 불가 속성 등), `paragraph_not_editable`(잠긴 문단). 조용히 빠지는 일치는 없고 `replacementCount + skippedCount`가 전체 일치 수입니다.
- **한도**: 바꿀 수 있는 일치가 `maxMatches`(기본 1000, 최대 10000)보다 많으면 아무것도 바꾸지 않고 `RESOURCE_LIMIT`로 거절합니다. 범위를 좁히거나 값을 올려 다시 보내세요.
- **원자성**: 하나라도 실패하면 문서는 그대로입니다. 성공하면 한 번의 `history_undo`로 원본 바이트까지 되돌립니다. 응답의 `matches`는 앞의 200곳만 미리보기로 담습니다(개수는 `replacementCount`가 정확합니다).
- 글자 모양이 다른 run에 걸친 일치는 첫 글자의 모양으로 합쳐지며 `FORMAT_COLLAPSED` 경고가 붙습니다.

### ③-3 라벨로 양식 채우기 (`form_fill`)
공문서 서식은 "성명", "소속" 같은 **라벨 셀 옆에 빈 셀**이 있는 표 구조가 많습니다. `form_fill`은 라벨 글자로 대상 셀을 찾아 값을 채웁니다. 표 셀 하나씩 `cell_set_text`로 찾아다니지 않아도 됩니다.
```json
{
  "documentId": "doc-1",
  "expectedVersion": 3,
  "values": { "성명": "홍길동", "소속": "○○고등학교" }
}
```
- **찾는 규칙**: `values`의 각 라벨과 **글자가 정확히 같은 셀**(표 셀 텍스트 전체)을 찾습니다. 값은 그 셀의 **같은 행 오른쪽 셀**에 넣고, 오른쪽 셀이 없으면 **바로 아래 셀**에 넣습니다. 대상 셀은 비어 있어야 합니다(문단이 여러 개면 첫 문단에만 값이 들어가고 나머지 빈 문단은 그대로입니다). 병합 기준 셀도 `cell_set_text`와 같은 규칙으로 채웁니다.
- **추측하지 않음**: 라벨이 없거나, 같은 라벨 셀이 둘 이상이거나, 오른쪽·아래 후보가 둘 이상이거나, 대상 셀에 이미 글자가 있거나, 대상 셀이 구조적으로 채울 수 없으면(중첩 표·`no_run`·해석 불가 마크업 등) 그 라벨은 실패로 보고합니다. 라벨 문자열은 정확히 비교하므로 공백 하나만 달라도 일치하지 않습니다. 대신 `scope`에 표나 구역 `nodeId`를 주면 찾는 범위를 좁혀 같은 라벨이 여러 표에 있는 서식을 구분할 수 있습니다.
- **strict (기본 true)**: 실패한 라벨이 하나라도 있으면 **아무것도 바꾸지 않고** 거절하며 `detail.failures[]`에 라벨별 `{label, reason, detail?}`를 담습니다. 모든 실패가 없는 라벨이면 `TARGET_NOT_FOUND`, 모두 여러 셀 일치면 `AMBIGUOUS_TARGET`, 그 밖에는 `UNSUPPORTED`(`reason: "form_fill_rejected"`)입니다.
- **strict: false**: 채울 수 있는 라벨만 적용하고 나머지를 결과 `skipped[]`와 `FORM_FILL_SKIPPED` 경고로 보고합니다. 라벨별 `reason`은 `label_not_found`, `ambiguous_label`, `no_cell_geometry`, `no_target_cell`, `ambiguous_target`, `target_not_empty`, `conflicting_target`, `nested_table` 같은 셀 사유입니다.
- **한 번의 되돌리기**: 성공한 호출은 버전을 1 올리고 하나의 `history_undo`로 원본 바이트까지 되돌립니다. `command_batch`의 한 단계로 넣을 수도 있고, 그때는 `formFill` 배열에 단계별 보고가 담깁니다.
- **미리보기**: `dryRun: true`로 결과를 확인해도 문서·버전·이력은 바뀌지 않습니다.
- **주의**: `values`가 비었거나 라벨·값이 비었거나 탭·줄바꿈이 들어 있으면 `INVALID_ARGUMENT`, `scope`가 표·구역이 아니면 `UNSUPPORTED`(`form_scope`), 없는 `scope` nodeId는 `TARGET_NOT_FOUND`입니다.

### ③-4 그림 교체 (`image_replace`)
기존 그림 하나의 이미지 파일만 다른 PNG 또는 JPEG로 바꿉니다. 그림의 위치·크기·캡션·감싸기 설정은 그대로입니다.
```json
{
  "documentId": "doc-1",
  "expectedVersion": 4,
  "target": "node-pic1",
  "imagePath": "/home/user/Documents/hwpx-inputs/new_logo.png"
}
```
- **대상**: `target`은 `picture_list`나 `element_query`로 찾은 그림의 `nodeId`입니다. `imagePath`는 `--input-root` 안의 파일이어야 합니다.
- **받는 형식**: 정적 PNG와 JPEG만 받습니다. 움직이는 PNG(`animated_image`), 깨졌거나 잘린 파일(`invalid_image`), 화소 수 한도(약 4,096만 화소)를 넘는 파일은 거절합니다.
- **거절되는 그림**: 문서 안에 이미지가 내장되어 있지 않거나(`no_embedded_media`) 패키지 목록이 예상한 모양이 아니면 `UNSUPPORTED`입니다.
- **동작**: 새 이미지는 문서에 새 미디어 항목으로 추가되고 기존 그림이 그것을 가리키도록 바뀝니다. 파일 내용은 계획할 때 한 번만 읽어 고정하므로, 이후 `history_redo`는 파일을 다시 읽지 않습니다. `dryRun`, `command_batch`, 되돌리기를 지원합니다.

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

### ④-2 글자 서식 (`text_format`)
문단 안의 글자 범위만 굵게 하고 색을 바꿉니다. `text_replace`와 같은 대상 규약을 쓰므로 `target`(문단 `nodeId`)과 자소 `range`를 주고, `props`에 바꿀 항목만 넣습니다. 문단 전체 서식인 `paragraph_format`과 달리 **글자 자체**의 모양을 다룹니다.

```json
{
  "documentId": "doc-1",
  "expectedVersion": 2,
  "target": "node-p1",
  "range": { "start": 0, "end": 4 },
  "props": { "bold": true, "textColor": "#FF0000" }
}
```

- **`props` 항목**: `bold`, `italic`, `underline`, `strikeout`(참·거짓), `heightHwpunit`(글자 크기, `100`~`100000`), `textColor`(`"#RRGGBB"`). 한 번에 여러 개를 줄 수 있고, 최소 하나는 있어야 합니다.
- **동작**: 값이 완전히 같은 기존 글자 모양이 있으면 **재사용**하고, 없으면 `header.xml`에 `charPr`를 **하나 새로 만들어** 붙입니다. 한 번 호출에 새로 생기는 모양은 최대 하나입니다. 범위가 걸친 run은 쪼개져서 **범위 안 글자만** 새 모양을 갖고, 범위 밖 글자는 원래 모양을 유지합니다.
- **거절되는 경우**: 길이가 0인 범위(`INVALID_RANGE`), 문단 밖을 벗어난 범위, 범위가 CDATA·주석·처리 명령을 가로지르는 경우(`UNSUPPORTED_CONTENT_IN_RANGE`), 범위 안에 필드 시작·끝 표식이 있는 경우(`field_boundary_in_range`), 해석할 수 없는 외부 namespace 요소를 품은 문단(`contains_opaque`).
- **`controls: "preserve"`**: `text_replace`와 같은 규칙으로 개체·탭을 그대로 두고 글자만 바꿉니다. 다만 `preserve`가 문단 잠금을 풀어 주지는 않습니다.
- **응답**: 성공하면 버전이 `afterVersion`으로 올라갑니다. `resourceChanges`에 이번 호출이 글자 모양을 `{"action":"added","kind":"charPr","resourceId":"4"}`로 **새로 만들었는지** `{"action":"reused",...}`로 **재사용했는지**가 담깁니다. 한 호출에 새로 생기는 모양은 최대 하나이고, 같은 `props`를 다른 범위에 다시 주면 같은 `resourceId`를 재사용합니다.

### ④-3 표 치수와 셀 서식 (`table_set_column_widths`, `table_set_row_heights`, `cell_set_format`, `cell_set_size`)
표의 **칸 크기와 테두리·배경**을 맞춥니다. 이 명령들은 노드를 만들거나 지우지 않습니다. 행·열을 넣고 빼는 것은 바로 아래 ④-4입니다.

```json
{ "documentId": "doc-1", "expectedVersion": 3,
  "target": "node-tbl1", "widths": [20000, 30000, 25000] }
```

- **`table.setColumnWidths`**: 열 너비를 HWPUNIT으로 지정합니다. `widths`는 **표의 열 수와 정확히 같아야** 하고, 모든 열을 같은 값으로 하려면 `width`를 씁니다. 병합 셀은 열 합계에 맞춰 함께 조정됩니다. 가로 격자와 바뀌는 너비의 좌우 여백만 검증합니다. 한컴 문서의 배치 높이(`hp:sz height`)가 행 높이 합과 달라도 사용할 수 있고, 높이·상하 여백은 그대로 보존됩니다.
- **`table.setRowHeights`**: 행 높이를 지정합니다. 저장된 값은 **최소 높이**라서, 글자가 많으면 한컴이 행을 더 늘릴 수 있습니다. 행 트랙은 셀의 저장 높이에서 복원하며, 기존 배치 높이가 행 합과 같을 필요는 없습니다. 지정한 행 높이 합을 새 표 높이로 기록하고 너비는 보존합니다. 셀 사이의 높이 제약이 모순되거나 행을 유일하게 복원할 수 없는 경우, 바뀌는 높이가 상하 여백 이하인 경우에는 거절합니다.
- **`cell.setFormat`**: 셀의 **테두리·배경·안 여백**을 바꿉니다. `borderFillId`는 `header.xml`에 **이미 있는** 값만 쓸 수 있고(없으면 `TARGET_NOT_FOUND`), `background`는 `"#RRGGBB"` 또는 `"none"`이며 **테두리가 같은 기존 자원이 있을 때만** 적용됩니다(없으면 `UNSUPPORTED`/`background_resource_unavailable`). `cellMargin`은 `{left,right,top,bottom}` 중 지정한 것만 바뀝니다. 이때는 해당 셀의 크기와 유효 여백을 검증하며 표 전체의 트랙 합 일치를 요구하지 않습니다.
- **`cell.setSize`**: **병합되지 않은** 셀 하나의 너비·높이를 바꿉니다. 너비를 지정하면 그 셀이 속한 **열 트랙**, 높이를 지정하면 **행 트랙** 전체가 바뀌고 교차하는 셀과 해당 축의 표 크기가 함께 조정됩니다. 지정하지 않은 축은 복원하거나 수정하지 않습니다. 병합 셀을 직접 지정하면 `UNSUPPORTED`/`merged_cell_size_unsupported`이고, 대신 표 트랙 명령을 쓰세요.
- **공통**: 네 명령 모두 `expectedVersion`이 필요하고 `dryRun`·`command.batch`·되돌리기를 지원합니다. 치수 명령의 `intents`에는 `tableSize`와 `changedCells`가 담깁니다. `widths`/`heights` 중 요청하지 않은 축은 빈 배열이며, 이 축의 저장값은 보존됩니다. **공유 자원(`header.xml`)을 새로 만들거나 고치지 않습니다.**

### ④-4 표 행·열 추가·삭제 (`table_insert_row`, `table_delete_row`, `table_insert_column`, `table_delete_column`)
기존 표에 **빈 행·열을 넣거나**, 지정한 행·열을 **지웁니다**. 엔진이 노드(셀·문단)를 만들고 지우는 유일한 명령이라 규칙이 조금 다릅니다. 설계와 근거는 `docs/decisions/table-structure-node-creation-design.md`에 있습니다.

```json
{ "documentId": "doc-1", "expectedVersion": 4,
  "target": "node-tbl1", "index": 3, "count": 2 }
```

- **`index`는 0부터 센 새 줄의 주소입니다.** `table_insert_row`의 `index: 3`은 "위에 3개 행을 남기고 그 아래에 넣기"(= 3번째 행 뒤)입니다. 범위는 `0` ~ 행 수(맨 뒤에 붙이려면 행 수)입니다. 삭제는 `index`부터 `count`개를 지우며 `0` ~ (행 수 − 1)입니다. `count`는 기본 1, 최대 1024이고 표는 1024행·1024열을 넘을 수 없습니다.
- **새 셀**: 이웃 행(위 행, 없거나 제목 행이면 아래 행)의 셀 속성·테두리·안 여백을 그대로 복제하고, 글은 비워 문단 하나만 둡니다(그 셀 첫 문단의 문단 모양·글자 모양 유지). 셀 이름(`name`)은 비우고 새 **행**의 제목 표시(`header`)는 끕니다. 열 삽입은 왼쪽 이웃 열(없으면 오른쪽)을 복제하며 제목 셀은 제목 셀로 남습니다.
- **크기**: 행은 복제한 행의 저장 높이 최댓값만큼 기존 배치 높이가 늘고(삭제는 지운 행별 최댓값 합만큼 줄고) 열은 **표 전체 너비가 새 열 너비만큼 늘어납니다**(기존 열은 줄이지 않습니다). 기존 배치 높이와 행 합의 일치를 요구하지 않습니다. 열 명령은 가로 격자만 복원하고 높이를 보존하며, 행 명령은 너비를 보존합니다. 수정할 축은 절대 크기여야 하고, 배치 높이 가감 결과가 양의 HWPUNIT 범위를 벗어나면 거절합니다. 쪽 너비를 넘는지는 검사하지 않으니 필요하면 이어서 `table_set_column_widths`로 맞추세요. `rowCnt`/`colCnt`, 모든 셀의 `cellAddr`, 표 `sz`가 함께 갱신되고 저장 전에 독립 검증기가 바이트 단위로 다시 확인합니다.
- **nodeId**: 새 셀·문단은 **새 `nodeId`**를 받아 결과 `structure[].created`(`nodeId`·`kind`·`row`·`col`)에 나옵니다. 기존 셀은 `nodeId`를 유지하지만 **주소가 밀립니다**(`structure[].shifted`, `changedIds`). 행·열 번호로 대상을 찾았다면 변경 뒤 다시 조회하세요. 삭제된 노드는 `structure[].removed`에 나오며, 그 ID는 다른 요소에 다시 쓰이지 않습니다.
- **병합 셀**: 삽입은 새 줄의 경계가 병합 셀을 가르면 `UNSUPPORTED`/`insert_inside_merged_cell`로, 복제할 단순한 이웃 줄이 없으면 `no_simple_template_line`으로 거절합니다. 삭제는 구간과 겹치는 셀이 **하나라도 병합(행·열 병합 수 ≠ 1)**이면 `merged_cell_blocks_delete`로 거절하고 `detail.cells`에 해당 셀 주소를 담습니다. 병합을 건드리지 않는 한 같은 표의 다른 행·열은 지울 수 있습니다.
- **삭제 제한**: 글과 줄바꿈·탭 정도만 든 셀만 지웁니다. 그림·수식·필드·도형·각주가 든 셀은 `delete_content_unsupported`, 표의 모든 행(열)을 지우려 하면 `INVALID_ARGUMENT`/`would_empty_table`, 범위 밖이면 `index_out_of_range`, 열 삭제로 셀이 하나도 없는 행이 생기면 `row_would_become_empty`입니다. 지운 셀의 글자는 미리보기 텍스트(`PrvText`)에서 지우지 않고 `stale.prvText: true`로만 알립니다.
- **되돌리기**: `history_undo`는 지워진 셀·문단을 **원래 `nodeId`로** 되살리고, 삽입으로 만든 노드는 사라지며 내보낸 파일은 **원본과 바이트가 같습니다**. `history_redo`는 처음 만들었던 **같은 새 `nodeId`**를 복원합니다. `dryRun`은 `structure`를 돌려주지만 후보가 버려지므로 새 노드의 `nodeId`는 `null`이고 `changedIds`에도 새 ID가 들어가지 않습니다(주소가 밀린 기존 셀은 `shifted`에 그대로 나옵니다). `command_batch`에서 앞 단계가 행·열을 바꿔도 뒷 단계의 기존 셀 대상은 따라갑니다. 단 같은 배치에서 앞 단계가 **만든** 노드나 **지운** 노드는 대상이 될 수 없습니다.
- **그 밖에**: 표와 그 뒤 모든 최상위 문단의 줄 배치 캐시를 지웁니다(한컴이 열 때 다시 계산). 중첩 표, 셀 구역(`cellzoneList`), `cellSpacing ≠ 0`, 상대 크기 표는 `UNSUPPORTED`입니다. `table.addRow` 같은 옛 이름은 계속 `structure_change_unsupported`이고 안내 문구가 새 명령을 가리킵니다.

### ④-5 문단 추가·삭제 (`paragraph_insert`, `paragraph_delete`)

기존 문단 앞이나 뒤에 **새 문단을 넣거나**, 같은 컨테이너(구역 본문 또는 표 셀)의 문단을 **이어서 지웁니다**. 표 행·열 명령에 이어 엔진이 문단 노드를 직접 만들고 지우는 두 번째 명령군이며, 규칙과 근거는 `docs/decisions/document-paragraph-creation-design.md`에 있습니다.

```json
{ "documentId": "doc-1", "expectedVersion": 4,
  "target": "node-para2", "position": "after", "text": "새 문단", "count": 2 }
```

- **`position`**: `"before"` 또는 `"after"`. `count`는 기본 1, `1`~`1024`입니다. 같은 요청을 여러 번 하면 `count`개가 한꺼번에 들어갑니다.
- **새 문단의 모양**: 대상 문단의 시작 태그(문단 모양 `paraPrIDRef`·스타일 `styleIDRef`·나눔 속성)를 복제하고, **문단 `id`는 뺍니다**. 첫 `hp:run`의 시작 태그도 그대로 복제해 글자 모양(`charPrIDRef`)을 물려받습니다.
  - `text`가 있으면 `<hp:t>` 하나에 넣고, 없거나 빈 문자열이면 **빈 문단**(글자 조각 없음)이 됩니다.
  - 새 문단에는 줄 배치 캐시(`linesegarray`)를 만들지 않습니다.
- **`nodeId`**: 새 문단은 **새 `nodeId`**를 받아 결과 `paragraph[].created`(`nodeId`·`kind`)에 나옵니다. 대상 문단의 `nodeId`는 그대로이고, 삭제는 `paragraph[].removed`에 나옵니다. `dryRun`은 `created[].nodeId`를 `null`로 돌려줍니다.
- **삽입 경계**: `before` 대상이 **구역 속성(`hp:secPr`)을 담은 문단**이면 `UNSUPPORTED`/`paragraph_before_section_property`로 거절합니다. 구역 속성은 첫 문단에 남아야 하므로, 그 문단 **뒤**에 넣으세요(`position: "after"`).
- **삭제 제한**: 글·탭·줄바꿈만 든 문단만 지웁니다. **표를 담은 문단**은 `paragraph_holds_table`, 그림·수식·필드·도형·각주·구역 속성이 있으면 `paragraph_content_unsupported`(`detail.element`)로 거절합니다. 컨테이너에 문단이 하나도 남지 않으면 `paragraph_would_empty_container`, 삭제 구간이 컨테이너를 넘으면 `paragraph_count_out_of_range`입니다.
- **대상 범위**: 구역 본문 문단과 **표 셀 안 문단**만 대상입니다. 각주·미주·글상자 안 문단은 `paragraph_scope_unsupported`로 거절합니다. 표 셀 문단을 넣고 지우면 그 표의 배치가 바뀌므로 표와 그 뒤 문단의 줄 배치 캐시를 지웁니다(표 행·열 명령과 같은 규칙). 구역 본문 문단이면 그 문단부터 구역 끝까지의 캐시를 지웁니다.
- **미리보기 텍스트**: 새 글은 `PrvText`에 줄을 끼워 넣지 않고, 지운 글도 빼지 않습니다. 대신 `stale.prvText: true`로 낡음을 알립니다.
- **되돌리기**: `history_undo`는 지운 문단을 **원래 `nodeId`로** 되살리고 삽입한 문단을 없애며, 내보낸 파일은 **원본과 바이트가 같습니다**. `history_redo`는 처음 만든 **같은 새 `nodeId`**를 복원합니다. `command_batch`에서 앞 단계가 문단을 밀어도 뒷 단계의 기존 문단 대상은 따라갑니다(새로 만든 문단은 같은 배치의 대상이 될 수 없습니다).

### ⑤ 실행 취소 (`history_undo`)와 다시 실행 (`history_redo`)
방금 바꾼 문단 서식이 마음에 들지 않아 직전 상태로 되돌립니다. 되돌리기 역시 문서 상태를 바꾸는 작업이므로 현재 버전(`2`)을 `expectedVersion`으로 보냅니다.
```json
{
  "documentId": "doc-1",
  "expectedVersion": 2
}
```
- **응답**: 문단 서식이 이전 상태로 복원되고 버전은 `afterVersion: 3`이 됩니다(버전 번호는 줄어들지 않고 항상 앞으로 증가합니다). 다시 적용하려면 `history_redo`(`expectedVersion: 3`)를 호출합니다.

### ⑤-2 명령 발견, 일괄 실행, 스냅샷·비교, 이력 정리, 닫기

문서를 직접 바꾸지 않는 보조 명령들입니다. 명령 이름은 점 표기(`document.diff`), MCP 도구 이름은 밑줄 표기(`document_diff`)입니다.

- **명령 목록 (`command.list` / `command_list`)**: 인자가 없습니다. 등록된 57개 명령 각각의 이름·MCP 도구 이름·분류·요약, `requiresDocument`·`requiresExpectedVersion`·`supportsDryRun`·`recordsHistory` 같은 정책, 입력·출력 JSON Schema, 설정상 사용 가능 여부(`available`)를 돌려줍니다. 각 항목의 `schemaVersion`은 레지스트리 계약 버전(`0.3.0`)이며 엔진 버전과 별개입니다. `available`은 렌더러 설정 같은 실행 환경에서의 가용성이고, 개별 문단·표에서 편집이 되는지는 `element_query`·`element_get`의 `edits[명령].available`이 알려 줍니다. CLI에서는 `hae run --command command.list --params '{}'`로 같은 목록을 얻습니다.
- **명령 하나 설명 (`command.describe` / `command_describe`)**: `{"command":"text.replace"}`처럼 이름을 주면 그 명령의 입력·출력 Schema와 `dryRun`·이력 정책을 돌려줍니다. 인자를 만들기 전에 확인하는 용도입니다.
- **일괄 실행 (`command.batch` / `command_batch`)**: `{"documentId":"doc-1","expectedVersion":3,"steps":[{"command":"text.replace","params":{...}},{"command":"paragraph.format","params":{...}}]}`처럼 편집 단계를 순서대로 줍니다. 전체가 한 번의 `revision`·되돌리기 단위이고, 한 단계라도 실패하면 앞 단계까지 포함해 아무것도 바뀌지 않으며 오류의 `detail.failedStep`이 실패한 단계를 가리킵니다. `dryRun: true`로 먼저 시험할 수 있습니다. 단계 안의 `params`에는 문서 ID나 `expectedVersion`을 넣지 않습니다.
- **스냅샷 만들기 (`document.snapshot` / `document_snapshot`)**: `{"documentId":"doc-1","label":"교체 전"}`(`label`은 선택). 현재 상태를 세션 안에 보관하고 `snapshotId`와 `revision`을 돌려줍니다. 문서와 이력은 바뀌지 않으며, 세션 메모리 한도를 넘으면 `RESOURCE_LIMIT`로 거절합니다. 스냅샷은 세션이 끝나면 사라지고 `.hwpwork`에 저장되지 않습니다.
- **스냅샷 해제 (`snapshot.release` / `snapshot_release`)**: `{"documentId":"doc-1","snapshotId":"snap-...","requestSequence":n}`. 메모리를 돌려주기 위해 지정한 스냅샷을 지웁니다. 이미 해제되었거나 없는 ID는 `SNAPSHOT_UNAVAILABLE`입니다. `expectedVersion`은 받지 않습니다.
- **변경 비교 (`document.diff` / `document_diff`)**: `baseSnapshotId`와 `targetSnapshotId`는 모두 선택입니다. 기준을 생략하면 문서를 처음 열었을 때의 상태, 대상을 생략하면 현재 상태를 씁니다. 응답은 `identical`, 텍스트·미디어가 바뀐 `changedNodes`(`beforeText`·`afterText`), 새로 생긴 `createdNodes`, 사라진 `removedNodes`, 바뀐 패키지 항목 `changedEntries`, `mediaChanges`, 줄 배치가 낡은 구역 `staleLayoutSections`입니다. 문서는 바뀌지 않습니다.
- **이력 비우기 (`history.clear` / `history_clear`)**: `{"documentId":"doc-1","requestSequence":n}`. Undo/Redo 이력만 비워 메모리를 돌려줍니다. 문서 내용과 `revision`은 그대로이지만 이후로는 그 이전 편집을 되돌릴 수 없습니다. 응답의 `controlChanged`는 지울 이력이 있었는지 알려 줍니다.
- **문서 닫기 (`document.close` / `document_close`)**: 인자가 없습니다. 열린 문서를 닫고 `.hwpwork` 프로젝트 잠금을 풉니다. 닫기만으로는 HWPX 파일을 저장하거나 덮어쓰지 않으므로 필요한 내보내기·저장을 먼저 합니다.

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

### ⑧ Markdown·HTML로 내보내기 (`document_export_text`)
문서 내용을 에이전트가 읽거나 웹·메모 도구로 옮기고 싶을 때 씁니다. **조회 명령**이라 문서를 바꾸지 않고, `revision`을 올리지 않으며, `expectedVersion`도 필요 없습니다. 결과 본문은 응답이 아니라 **파일**에 쓰고, 응답에는 요약(경로·크기·SHA-256·개수)만 담습니다. 그래서 큰 문서도 응답 한도(4 MiB)를 넘지 않습니다.

```json
{
  "documentId": "doc-1",
  "path": "/home/user/Documents/hwpx-outputs/report.md",
  "format": "markdown"
}
```

| 인자 | 설명 |
|---|---|
| `path` | `--output-root` 안의 **새 파일** 절대 경로. 이미 있으면 `TARGET_EXISTS`로 거절하고 덮어쓰지 않습니다. 루트 밖·링크·`..` 경로는 `PATH_NOT_ALLOWED`입니다. 확장자는 `markdown`이면 `.md`/`.markdown`, `html`이면 `.html`/`.htm`이어야 하며 아니면 `INVALID_ARGUMENT`(reason `extension_mismatch`)입니다. 텍스트 파일이 `.hwpx` 같은 문서 확장자를 얻지 못하게 하려는 규칙입니다. |
| `format` | `"markdown"` 또는 `"html"` |
| `scope` | (선택) 구역 `nodeId`. 주면 그 구역만, 안 주면 모든 구역. 구역이 아닌 `nodeId`는 `UNSUPPORTED`(reason `wrong_kind`)입니다. |
| `includeNotes` | (선택, 기본 `true`) `false`면 각주·미주를 본문 표시와 목록에서 모두 뺍니다. 머리말·꼬리말은 영향받지 않습니다. |

**변환 규칙** (확인한 구조만 바꾸고, 해석하지 못하는 요소는 숨기지 않고 표시합니다)

| 문서 요소 | Markdown | HTML |
|---|---|---|
| 문단 | 문단. 탭은 `&emsp;`, 줄바꿈은 `\`+줄바꿈(표 셀 안은 `<br>`) | `<p>`, 탭 `&emsp;`, 줄바꿈 `<br>` |
| 문단 정렬 가운데·오른쪽 | **무시**하고 문단 바로 위에 `<!-- hwpx: 문단 정렬(가운데) 무시됨 -->` 주석을 남김. 문서 머리 안내에 곳수 표시 | `style="text-align:center"` / `right` |
| 표 | 파이프 표. 문법상 머리글 행이 필요해 **빈 머리글 행**을 넣음(첫 행을 머리글로 단정하지 않음) | `<table>`·`<tr>`·`<td>` |
| 병합 셀 | 셀을 **반복하지 않고** 덮인 칸을 비움. 표 앞에 주석, 문서 머리 안내에 `병합 셀이 있는 표: 표 N` | `colspan`·`rowspan` |
| 그림 | ``[그림] `파일명` (미디어 항목 `BinData/…`, SHA-256 …)`` + 캡션. **이미지 파일은 만들지 않음**(`![]()` 링크 없음) | `<figure data-media-entry data-sha256>` + `<figcaption>`, `<img>` 없음 |
| 수식 | `hp:script` 원문을 변환 없이 코드(`` `…` ``, 줄바꿈이 있으면 `<code>`)로 감쌈 | `<code class="hwpx-equation">` |
| 각주·미주 | 위치에 `[^fn1]`/`[^en1]`, 문서 끝에 `## 각주`·`## 미주` 목록 | 위치에 위 첨자 링크, 끝에 `<ol>` 목록 |
| 머리말·꼬리말 | 문서 머리의 `## 머리말·꼬리말` 절(구역·적용 쪽 표시) | 같은 이름의 `<section>` |
| 특수문자 | `` \ ` * _ [ ] < > & \| ~ $ ``를 백슬래시로 이스케이프, 줄 첫머리의 `#`·`-`·`+`·`=`·`1.`·들여쓰기는 목록·제목·코드로 바뀌지 않게 처리 | `& < > " '`를 엔티티로 |

문서 머리에는 `> **변환 안내**` 상자가 붙어 변환한 개수, 무시·생략한 것, 변환하지 못한 것을 적습니다. 구역이 둘 이상이면 Markdown은 구역마다 주석(`<!-- hwpx: 구역 N nodeId … -->`), HTML은 `<section data-node-id>`로 나눕니다.

**변환하지 못하는 것**: 글상자·도형·묶음 개체(`rect`, `container` 등), 메모(`hiddenComment`), 해석할 수 없는 표(셀 주소가 겹치거나 병합 칸에 주소가 없는 표), 그리고 Markdown의 중첩 표는 본문에 `⟦미지원: 이름⟧` 표시를 남기고 응답 `counts.unsupported`에 개수를 셉니다. 이런 개체 **안에** 있던 문단·표·그림은 `counts.insideUnsupported`로 따로 셉니다. 제목 스타일·글꼴·색·밑줄 같은 서식과 책갈피·필드 경계·쪽 번호 표지는 옮기지 않습니다(필드 경계와 쪽 번호 표지는 `counts.ignoredMarkers`에 셈). 구조가 어떤 문서에서 나왔는지는 아래 "검증 범위"를 보세요.

**크기 한도**: 결과 파일이 `max_text_export_bytes`(기본 16 MiB)를 넘으면 `RESOURCE_LIMIT`(reason `text_export_too_large`)으로 거절하고 `detail.limitBytes`에 한도를, `detail.producedBytes`에 만들어진 크기를 알려 줍니다. 파일은 만들어지지 않습니다.

**응답 예** (본문은 파일에만 있음)

```json
{
  "path": "/home/user/Documents/hwpx-outputs/report.md",
  "format": "markdown",
  "bytes": 38812,
  "sha256": "6bb33f5c…",
  "revision": 0,
  "scope": null,
  "includeNotes": true,
  "counts": {"paragraphs": 411, "tables": 24, "pictures": 2, "equations": 0, "footnotes": 0, "endnotes": 0,
             "headers": 2, "footers": 0, "mergedTables": 7, "alignmentIgnored": 115, "notesOmitted": 0,
             "unsupported": {"container": 1, "nestedTable": 1}, "insideUnsupported": {"paragraphs": 0, "tables": 0, "pictures": 0, "equations": 0},
             "ignoredMarkers": {"colPr": 1, "newNum": 1, "pageHiding": 2, "pageNum": 1}}
}
```

`counts.paragraphs`는 본문·표 셀·각주·머리말·캡션을 가리지 않고 **내용이 있는** 문단 수입니다. 이 도구는 MCP `readOnlyHint`를 `false`로 광고합니다(조회 명령이지만 파일을 새로 쓰기 때문입니다).

**검증 범위**: 공개 코퍼스 문서(`tests/fixtures/corpus/`)의 표·병합 셀·그림·머리말·탭·줄바꿈 변환은 실제 문서로 실행해 확인했습니다. 코퍼스에는 **각주·미주, 그림·표 캡션, 수식이 없어서** 이 세 가지는 `hp:ctrl > hp:footNote/endNote > hp:subList`, `hp:pic > hp:caption > hp:subList`, `hp:equation > hp:script` 구조를 가정한 합성 문서로만 시험했습니다(머리말 구조와 `hp:script`는 저장소의 기존 편집 시험 자료에 같은 모양이 있습니다). 실제 한컴 문서의 각주·캡션으로 확인한 것이 아닙니다. 각주 번호는 문서 속 번호가 아니라 **문서 순서대로 새로 매긴 값**입니다.

---

## ⑨ 새 문서 만들기 (`document_create`, 0.4)

제목과 문단 목록만으로 **한컴이 열 수 있는 HWPX 파일을 처음부터** 만들어 출력 폴더에 새 파일로 씁니다. 열린 문서가 필요 없고, `documentId`·`expectedVersion`도 쓰지 않습니다. 문서 상태·`revision`·되돌리기 이력은 바뀌지 않습니다.

```json
{ "path": "/home/user/hwpx-outputs/보고서.hwpx",
  "title": "탐구 보고서",
  "paragraphs": ["제목", "", "첫 번째 절의 내용입니다."] }
```

- **응답**: `{path, sha256, bytes, entries, paragraphCount, title}`. `entries`는 만든 패키지 항목 여섯 개입니다.
- **만들어지는 것**: `mimetype`(첫 항목, 무압축) · `version.xml` · `META-INF/container.xml` · `Contents/content.hpf` · `Contents/header.xml`(`fontfaces` 7언어·`borderFills`·`charProps`·`paraProps`·`styles`) · `Contents/section0.xml`. 첫 문단이 A4 세로·20 mm 여백의 구역 속성(`hp:secPr`)을 담고, `paragraphs`가 없거나 비면 빈 문단 하나를 만듭니다.
- **검증**: 파일을 공개하기 전에 `hae-verify`가 요청에서 기대 패키지 전체를 다시 만들어 **항목 순서·이름·바이트를 정확히 대조**합니다. 다르면 파일을 만들지 않습니다(`VERIFY_FAILED`).
- **`version.xml` 주의**: `application="Hancom Office Hangul"`이면 `tagetApplication="WORDPROCESSOR"`, `major`는 5 이하여야 합니다. 이 규칙을 어기면 한컴이 빈 문서를 엽니다(0.1에서 실측).
- **덮어쓰기 금지**: 같은 경로가 이미 있으면 `TARGET_EXISTS`이고 기존 파일은 그대로입니다. 출력 루트 밖 경로·심볼릭 링크·읽기 보호된 입력은 `PATH_NOT_ALLOWED`입니다. 같은 인자로 다시 만들면 **바이트가 같은 파일**이 나옵니다(결정적).
- **바로 편집**: 만든 파일은 `document_open`으로 열 수 있습니다(출력 루트의 파일도 열립니다). 기본 문단·글자 모양이 정의되어 있어 바로 `text_replace`·`paragraph_insert`·`table` 명령을 쓸 수 있습니다.
- **한계**: 새 문서에 그림·표·머리말·여러 구역·사용자 스타일을 함께 만들지는 않습니다. 만든 뒤 기존 명령으로 편집하거나, `paragraphs`로 본문만 넣으세요. Windows 한컴오피스 실기 검증은 아직 없고 Linux 한컴 2020 Beta 게이트까지만 확인했습니다.

## 5. 자주 만나는 오류·경고 코드 풀이

문서 편집 중 조건이 맞지 않으면 엔진은 문서를 망가뜨리는 대신 작업을 안전하게 거절하고 이유(`diagnostics`)를 알려줍니다.

| 코드 | 쉬운 설명 및 해결 방법 |
|---|---|
| `PACKAGE_REJECTED` | **문서 열기 거절**: 암호가 걸려 있는 문서(`encrypted`)이거나 손상된 파일, 비정상 ZIP/XML 구조일 때 발생합니다. 한컴오피스에서 암호를 해제하고 표준 `.hwpx`로 저장한 뒤 다시 열어 주세요. 구형 `.hwp`(HWP5) 조회(`hwp5.inspect`)도 이 코드를 쓰며 reason은 `malformed_compound_file`·`missing_component`·`malformed_record_stream`·`distribution_unsupported`·`encrypted`입니다(아래 「구형 .hwp(HWP5) 읽기 전용 조회」). |
| `RESOURCE_LIMIT` | **용량·크기 한도 초과**: `text_replace_all`의 바꿀 일치 수가 `maxMatches`를 넘을 때도 이 코드로 거절합니다. 파일 크기(기본 HWPX 32 MiB, 프로젝트 96 MiB), 압축 해제 크기(128 MiB), 그림 화소 수(4,096만 화소), 세션 메모리(512 MiB), 단일 요청(1 MiB)·응답(4 MiB) 등의 안전 한도를 넘었을 때 발생합니다. `hwp5.inspect`는 여기에 더해 컨테이너 항목 수(`max_entries`)와 문단 수(`max_hwp5_paragraphs`, 기본 1,000,000) 한도를 씁니다. |
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
| `INVALID_ARGUMENT` | **인자 값이 맞지 않음**: 표 행·열 명령의 `index_out_of_range`(범위 밖)·`count_out_of_range`·`table_size_limit`(1024 초과)·`would_empty_table`(모두 지우려 함), `paragraph.delete`의 `paragraph_count_out_of_range`(삭제 구간이 컨테이너를 넘음), 표 치수 값 오류 등입니다. `reason`을 확인하세요. |
| `AMBIGUOUS_TARGET` | **대상이 모호함**: 조건에 맞는 대상이 여러 개여서 하나로 특정할 수 없을 때 발생합니다. 정확한 `nodeId`를 지정하세요. |
| `UNSUPPORTED` | **지원하지 않는 편집**: 지원하지 않는 대상을 고치려 할 때 발생합니다. 0.4 문단 명령은 `paragraph_scope_unsupported`(각주·미주·글상자 안 문단), `paragraph_template_unsupported`(첫 run이나 `charPrIDRef` 없음), `paragraph_before_section_property`(구역 속성 문단 앞 삽입), `paragraph_holds_table`·`paragraph_content_unsupported`(표·개체·필드 등이 든 문단 삭제), `paragraph_would_empty_container`(컨테이너의 마지막 문단 삭제)를 씁니다. 그 밖에 중첩 표가 있는 셀, 문단이 2개 이상인 셀에 `text` 사용, 전자서명·변경 추적이 있는 읽기 전용 문서, 자르기·효과가 들어간 그림, 애니메이션 PNG 등이 있습니다. |
| `INVALID_RANGE` | **잘못된 글자 범위**: `text_replace`의 `[start, end)` 범위가 문단 grapheme 수를 벗어났거나 역순입니다. 길이 0 범위는 지원되는 textSpans 위치에 비어 있지 않은 text를 넣을 때만 허용합니다. |
| `RANGE_CROSSES_CONTROL` | **제어 문자를 가로지르는 범위**: 바꾸려는 텍스트 범위 안에 탭(Tab), 줄바꿈, 필드, 수식, 표, 그림 같은 제어 요소가 끼어 있습니다. 제어 요소 앞뒤로 범위를 나누어 일반 텍스트 구간만 교체하세요. |
| `UNSUPPORTED_CONTENT_IN_RANGE` | **범위 내 알 수 없는 요소**: 지정한 텍스트 범위 안에 엔진이 해석할 수 없는 특수 속성이나 하위 요소가 들어 있어 안전을 위해 편집을 거절했습니다. |
| `MATCHES_SKIPPED` *(경고)* | **일부 일치를 건너뜀**: `text_replace_all`이 개체를 가로지르거나 자소 경계가 맞지 않는 등 안전하게 바꿀 수 없는 일치를 건드리지 않았습니다. `detail.reasons`의 사유별 개수와 위치(`nodeId`, `range`)를 확인해 필요하면 `text_replace`·`controls: "preserve"`로 따로 고치세요. |
| `FORM_FILL_SKIPPED` *(경고)* | **일부 라벨을 건너뜀**: `form_fill`을 `strict: false`로 실행했을 때 채우지 못한 라벨이 있습니다. 결과 `skipped`의 라벨별 `reason`(`label_not_found`, `ambiguous_label`, `target_not_empty`, `no_target_cell`, `ambiguous_target`, `conflicting_target`, 셀 사유 등)을 확인하세요. |
| `FORMAT_COLLAPSED` *(경고)* | **글자 모양 통합 경고**: 서로 다른 글자 모양(글꼴·크기·색상 등)이 적용된 여러 구간을 한 번에 교체하여, 새 글자가 첫 글자의 모양으로 통일되었음을 알려주는 경고입니다. |
| `RENDERER_UNAVAILABLE` | **미리보기 렌더러 사용 불가**: `document_preview`를 호출했으나 외부 렌더러(`HAE_RHWP`)가 설정되지 않았거나(`renderer_not_configured`), 실행 파일이 없거나(`renderer_missing`), `bubblewrap`(`HAE_BWRAP`) 격리 자가 점검에 실패했거나(`sandbox_unverified`), 글꼴 폴더(`HAE_FONT_DIR`)를 읽을 수 없거나(`font_dir_unreadable`), 렌더링 제한 시간(60초)을 넘겼을 때(`timeout`) 발생합니다. 미리보기만 실패할 뿐 문서 내용은 전혀 바뀌지 않습니다. |
| `PREVIEW_PAGE_LIMIT` *(경고)* | **미리보기 100쪽 제한 경고**: 문서 전체가 100쪽을 넘어 앞의 100쪽까지만 렌더링했습니다(결과 `status`는 `"partial"`이 됩니다). |
| `INVALID_REQUEST` / `UNKNOWN_COMMAND` | **잘못된 요청 형식 / 알 수 없는 명령**: 명령 이름이 틀렸거나, 필수 인자가 빠졌거나, 줄간격 백분율(`1`~`1000`)·미리보기 쪽 번호(`1`~`100`개, 1 이상) 범위를 벗어났거나, 새 텍스트에 줄바꿈·탭 문자를 넣었을 때 발생합니다. |
| `PATH_NOT_ALLOWED` | **허용되지 않은 파일 경로**: `--input-root` 또는 `--output-root` 바깥의 경로에 접근하려 했거나, 심볼릭 링크를 거치거나, 원본 파일 덮어쓰기를 시도했을 때 거부합니다. |
| `TARGET_EXISTS` | **파일이 이미 존재함**: `document_export`·`document_create`나 첫 `project_save`를 할 때 지정한 경로에 이미 파일이 있으면 기존 파일을 보호하기 위해 저장을 거절합니다. 새 파일 이름을 지정하세요. |
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

### 텍스트 범위 거절 후 재시도

`RANGE_CROSSES_CONTROL`의 진단 `detail`에는 해당 문단의 `textSpans`와
`textSpansTruncated`가 포함됩니다. batch 오류에도 `detail.failedStep`과 함께 유지되며,
실패 단계 직전 후보 상태의 오프셋입니다. batch가 롤백되면 현재 revision을 다시 조회한 뒤
새 구간으로 요청합니다. 0.2에서는 `controls: "preserve"`로 개체를 남기는 교체, 빈 문단 삽입, `paragraphs` 배열을 통한 다문단 셀 편집도 가능합니다. 아래 인자 규칙을 확인하세요.


## 0.2 텍스트 삽입과 개체 보존

엔진 버전과 command.list/describe의 레지스트리 schemaVersion은 모두 0.3.0이지만 서로 독립된 값이다(0.2.0에서는 둘 다 0.2.0이었다). JSON-lines/MCP 요청 봉투와 .hwpwork 파일 형식의 정수 schemaVersion=1은 유지한다. 자세한 규칙은 docs/decisions/versioning.md를 본다.

text.replace는 [start,end) grapheme 범위를 받는다. start=end이면 비어 있지 않은 text를 삽입한다. 위치는 textSpans 내부·경계여야 하며, 빈 문단은 [{start:0,end:0}]과 modes:["insert"]를 제공한다. 일반 텍스트 문단은 modes:["replace","insert"]를 제공한다. 바로 뒤 글자의 run, 없으면 바로 앞 글자의 run을 쓴다. 빈 문단은 첫 run을 쓰고 필요한 hp:t 하나만 마지막 자식 뒤에 만든다. run이 없으면 UNSUPPORTED(reason:no_run)다. 빈 문자열 삽입은 INVALID_RANGE다.

```json
{"command":"text.replace","expectedVersion":0,"params":{"target":"문단 nodeId","range":{"start":0,"end":0},"text":"새 글"}}
```

controls는 기본 reject이며 기존처럼 개체를 가로지르면 거절한다. preserve를 주면 선택 범위의 첫 텍스트 조각에 새 문자열을 넣고 나머지 텍스트 조각을 비우며, 개체·탭·줄바꿈·필드 경계의 원문 바이트와 순서를 유지한다. 선행 개체는 문자열 앞에 남는다. 텍스트가 없으면 범위 내 기존 빈 hp:t를 재사용한다. 그것도 없으면 no_text_fragment로 거절한다. 수식 노드 자체 편집, CDATA·주석/PI 교차·리터럴 제어 문자·해석 불가 표기는 계속 거절한다. CONTROLS_PRESERVED 진단의 detail.count와 offsets는 유지된 개체 수와 새 문단 grapheme 위치를 담는다.

preserve 범위는 개체 앞·뒤 grapheme 경계를 사용할 수 있다. textSpans는 기본 reject/삽입 구간이며 preserve의 모든 범위를 나열하지 않는다. preserve만 가능한 문단은 textSpans가 비어 있어도 modes:["replace"], available:true다. editable만 보고 텍스트 교체 가능을 추론하지 말고 명령 능력과 사용할 controls 모드를 함께 확인한다.

```json
{"command":"text.replace","expectedVersion":0,"params":{"target":"문단 nodeId","range":{"start":0,"end":12},"text":"교체 글","controls":"preserve"}}
```

cell.setText는 실제 존재하는 병합 기준 셀도 지원한다. 한 문단이면 text, 여러 문단이면 정확한 기존 문단 수의 paragraphs 배열을 준다. 두 인자를 동시에 주거나 둘 다 생략하면 INVALID_ARGUMENT이며, 배열 길이가 다르면 detail.paragraphCount로 실제 수를 알려준다. 다문단 셀에 text를 주면 paragraphs 안내와 함께 UNSUPPORTED다. controls는 text.replace와 같다. 새 문단·행·개체를 만들지는 않는다.

```json
{"command":"cell.setText","expectedVersion":0,"params":{"target":"셀 nodeId","paragraphs":["첫 문단","둘째 문단"],"controls":"preserve"}}
```

한 명령은 문단 수와 무관하게 Undo 한 단위이며 batch·dryRun·expectedVersion·재전송 규칙은 같다. 변경한 셀 문단과 표를 담은 최상위 문단부터 구역 끝의 줄 배치 캐시를 지운다. PrvText 갱신과 썸네일 staleness 규칙은 0.1과 같다. 독립 저장 관문을 통과한 뒤에만 commit한다.


### 0.2 추가 안전 규칙 (§7-1)

`controls:"preserve"`여도 범위 안 필드 시작·끝(하이퍼링크 포함)이 있으면 `field_boundary_in_range`로 거절합니다. 필드 안 일반 텍스트 구간이 광고되어 있다면 경계를 가로지르지 않는 교체는 가능합니다. 개체만 있는 문단에는 개체 앞·뒤와 사이의 삽입 가능 위치를 `{start:p,end:p}`로 표시합니다. 인접 개체와 같은 기존 run에 hp:t를 만들 수 있는 위치만 제공하며 새 run은 만들지 않습니다. 삽입하면 실제 XML 경로는 바뀔 수 있지만 기존 nodeId·개체 순서와 Undo 복원은 유지합니다.


### 0.2 문서 전체 바꾸기 (`text.replaceAll`, §7-2)

명령 이름은 `text.replaceAll`(MCP 도구 `text_replace_all`, CLI `text-replace-all`)이고 `command.batch`의 단계로도 쓸 수 있습니다. 인자는 `find`, `text`, 선택 `scope`(nodeId), 선택 `maxMatches`(1~10000, 기본 1000)입니다. 편집 명령이므로 `expectedVersion`이 필요하고 `dryRun`·재전송 규칙은 다른 편집과 같습니다.

```json
{"command":"text.replaceAll","expectedVersion":0,"params":{"find":"2025학년도","text":"2026학년도"}}
```

결과에는 `replacementCount`, `paragraphCount`, `skippedCount`, `skipped[]`, `matches[]`(앞 200곳 미리보기)와 `changedIds`가 담깁니다. 일치는 명령 시작 시점의 문단 기준 grapheme `range`로 보고합니다. 바꾼 문단마다 줄 배치 캐시를 지우는 규칙은 `command.batch`와 같아, 구역마다 가장 앞서 바뀐 최상위 문단(셀은 표를 담은 최상위 문단)부터 구역 끝까지 지웁니다. PrvText는 문단 옛 문장이 미리보기에서 한 번만 나올 때만 바뀌고 아니면 `stale`로 알립니다. 잠긴 문단(불투명 요소가 가리키는 문단 등)은 건너뜁니다. 수식·필드의 내용은 어떤 경우에도 바꾸지 않습니다.

## 수식 조회·편집 (0.3)

기존 수식은 `equation.list` / MCP `equation_list`로 찾습니다. `scope`는 문단·구역·표·셀·수식의 nodeId이며 자신과 하위 요소를 포함합니다. `limit`은 1~500(기본 100), `nextCursor`를 다음 호출의 `cursor`에 그대로 줍니다. 커서는 문서·scope·revision에 묶이고 편집·Undo 후에는 다시 조회합니다. 수식이 없는 범위는 빈 `items`와 `nextCursor: null`입니다.

각 항목은 수식 `nodeId`, 담은 문단 `paragraphNodeId`, XML의 수식 `id`, `script`, `scriptRaw`, `shapeComment`, `size`(hp:sz의 모든 속성), `baseLine`, `textColor`, `font`, `baseUnit`, `lineMode`, `version`, `textWrap`, `textFlow`, `treatAsChar`를 반환합니다. `script`는 XML 엔티티를 해석한 수식 문자열이고, **`scriptRaw`는 엔티티 표기를 포함한 hp:script 내부 XML 원문**입니다. 빈 스크립트는 `""`, 누락된/중복된 script는 `null`입니다. CDATA는 조회할 수 있지만 편집은 거절합니다. `equation.get` / `equation_get`의 `target`은 수식 nodeId이며 같은 항목과 문단 앞 60자소의 `paragraphTextPreview`를 `equation` 아래 반환합니다.

`equation.setScript` / `equation_set_script`:

```json
{"target":"수식-nodeId","script":"{x} over {2} + a < b & c"}
```

사용자는 XML로 미리 이스케이프하지 않은 스크립트를 줍니다. 엔진은 `text.replace`와 같은 XML 텍스트 이스케이프 경로를 사용합니다. 수식 속성·크기·위치·바깥 여백·설명은 원본 바이트를 보존합니다. 빈 script 요소(`hp:script/` 포함)는 채울 수 있지만 없는 script를 새로 만들지는 않습니다. 수식 모델 노드도 추가하지 않습니다. 같은 문자열 요청은 원본 엔티티 표기까지 보존하는 noop입니다.

빈/공백 전용 문자열과 XML 금지 문자는 `INVALID_REQUEST`입니다. 원본 script의 CDATA·주석·PI·자식 요소·속성·중복/누락, 새 문자열의 역슬래시·큰따옴표·제어 문자(탭/줄바꿈 포함)·`<!`/`<?` 표기·맞지 않는 중괄호는 `UNSUPPORTED`로 거절합니다. 역슬래시/큰따옴표의 수식 전용 이스케이프 규칙은 확인하지 못했으므로 변환하거나 추측하지 않습니다. HWP 수식 문자열의 전체 문법·명령어 및 LaTeX/MathML 변환은 검증하지 않습니다. 보통 문자, 중괄호 그룹, `over`, `TIMES` 등 입력은 문자열 그대로 보존합니다. 수식의 수학적 의미나 조판이 유효하다는 보장은 별도 네이티브 확인이 필요합니다.

`equation.setProperties` / `equation_set_properties`:

```json
{"target":"수식-nodeId","props":{"baseLine":70,"textColor":"#123456","font":"HancomEQN","baseUnit":1200,"lineMode":"CHAR","version":"Equation Version 60"}}
```

여섯 속성 중 지정한 것만 바꿉니다. 최소 하나가 필요합니다. `baseLine`은 uint32, `baseUnit`은 양의 uint32, 색은 `#RRGGBB`, `lineMode`는 `CHAR` 또는 `LINE`, font/version은 제어 문자 없는 비어 있지 않은 XML 문자열입니다. 글꼴의 설치 여부·version의 네이티브 지원 여부는 검사하지 않습니다. script와 hp:sz는 바이트 그대로 보존하며 새 크기를 계산하지 않습니다. 요청한 속성이 없으면 기존 equation 시작 태그에 그 속성만 추가합니다.

두 편집은 현재 `expectedVersion`, `dryRun`, `command.batch`, Undo/Redo를 지원합니다(MCP에서는 이 필드를 인자에 포함). batch 전체가 한 revision·Undo 단위이며 나중 단계 거절 시 전체를 취소합니다. 기존 linesegarray E 정책(수식이 든 문단부터 구역 끝 + 편집한 중첩 문단)을 적용하고, 문단의 표시 텍스트는 바꾸지 않으므로 PrvText는 그대로 둡니다. 미편집 ZIP 항목의 원시 압축 데이터·로컬 헤더를 보존합니다. 독립 저장 검증기는 원본 + 요청에서 기대 변경을 계산해 다른 수식, 크기, 위치, 설명, 요청 밖 속성 또는 원문 재인코딩을 거절합니다.

속성 근거: 한컴 공개 [EquationType](https://github.com/hancom-io/hwpx-owpml-model/blob/main/OWPML/Class/Para/EquationType.h)와 [lineMode 정의](https://github.com/hancom-io/hwpx-owpml-model/blob/main/OWPML/Class/enumdef.h). [공식 도움말](https://help.hancom.com/hoffice110/ko-KR/Hwp/insert/equation/equation.htm)은 `1 over 2` 스크립트 입력을 설명합니다. 이 자료만으로 수식 전용 이스케이프 문법 전체를 확인했다고 주장하지 않습니다.

## 0.3 좁은 편집 명령

세 명령 모두 현재 `expectedVersion`이 필요하며 `dryRun`, `command.batch`, Undo/Redo를 지원합니다. batch 전체는 하나의 revision·Undo 단위이고, 어느 단계든 실패하면 전체를 취소합니다. 새 문단·그림·머리말·꼬리말·스타일을 만들지 않습니다.

### 기존 문단 스타일 지정 (`paragraph.setStyle`)

`target`(문단 nodeId) 또는 비어 있지 않은 `targets` 배열 중 하나와 `styleId`(0 이상 정수)를 줍니다. `Contents/header.xml`에 이미 정의된 스타일만 참조하며, 없는 스타일은 `TARGET_NOT_FOUND`입니다. `styleIDRef`만 바꾸고 `paraPrIDRef`·글자 모양·스타일 정의는 유지합니다. 문단에 기존 참조 속성이 없으면 `UNSUPPORTED(style_shape)`입니다.

```json
{"command":"paragraph.setStyle","expectedVersion":0,"params":{"targets":["문단1 nodeId","문단2 nodeId"],"styleId":1}}
```

### 그림 프레임 맞춤 (`image.fit`)

`target`은 기존 그림 nodeId, `mode`는 다음 셋 중 하나입니다.

- `contain`: 현재 표시 프레임 안에 원본 이미지 비율을 유지해 맞춥니다. 한쪽 표시 크기가 줄어듭니다. 위치·앵커는 유지합니다.
- `cover`: 표시 프레임 크기를 유지하면서 원본 이미지의 가운데를 잘라 가득 채웁니다.
- `stretch`: 표시 프레임 크기를 유지하고 원본 전체를 늘려 채웁니다.

프레임은 명령 직전 `sz`이며 0인 축은 `curSz`, 다시 0이면 `orgSz`를 사용합니다. 반복 호출은 그 시점의 프레임에 적용됩니다. 크기와 자르기 경계는 가장 가까운 정수로 반올림하고 중앙 자르기의 홀수 나머지는 오른쪽·아래쪽에 둡니다. 반올림 결과가 0인 극단적인 비율은 `UNSUPPORTED(image_geometry)`로 거절합니다. 원본 `orgSz` 좌표계와 위치·회전을 유지하고, `imgRect` 네 꼭짓점을 그 좌표계에 맞춥니다. 표시 크기 `sz == curSz`를 보장하고 기존 `scaMatrix`의 각 행을 표시 크기 변화율로 곱합니다. `transMatrix`·`rotMatrix`와 세 행렬 자체는 보존합니다. 배율 소수는 6자리로 기록합니다.

내장 정적 PNG/JPEG의 실제 픽셀 크기로 `imgDim = px × 75 HWPUNIT`를 계산합니다. `contain`·`stretch`는 `imgClip` 전체 범위를 사용하고, `cover`만 더 작은 중앙 자르기 범위를 사용합니다. `include`는 별도 mode 이름이 아니며 허용하지 않습니다. 미디어·manifest·`isEmbeded`는 바꾸지 않습니다. 필요한 기존 기하 속성이나 행렬이 빠졌거나 숫자가 잘못되면 `UNSUPPORTED(image_geometry)`이며 새 요소를 보충하지 않습니다.

```json
{"command":"image.fit","expectedVersion":1,"params":{"target":"그림 nodeId","mode":"cover"}}
```

### 기존 머리말·꼬리말 글 바꾸기 (`header.setText`)

`scope`는 구역 nodeId이며 생략하면 첫 구역입니다. `kind`는 `header` 또는 `footer`, `paragraphIndex`는 0부터 시작하고 기본 0입니다. 해당 구역에서 같은 종류의 머리말·꼬리말을 문서 순서로 훑고 각 `subList`의 직접 문단만 셉니다. 내부 표·캡션 문단은 포함하지 않습니다. 기존 머리말·꼬리말이나 문단이 없으면 `TARGET_NOT_FOUND`이며 생성하지 않습니다.

```json
{"command":"header.setText","expectedVersion":2,"params":{"kind":"header","paragraphIndex":0,"text":"2026학년도 연구 보고서"}}
```

일반 텍스트와 기존 run·t만 편집합니다. 여백·번호·필드·탭·개체 제어가 든 문단은 `UNSUPPORTED`, 사유 `header_control`로 거절하며 제어 필드를 지우지 않습니다. 기존 텍스트 조각이 없으면 `no_text_fragment`로 거절합니다. 새 텍스트에는 탭·줄바꿈을 넣을 수 없습니다.

줄 배치 캐시는 기존 정책대로 대상 문단(그림은 그림을 담은 문단)과 해당 최상위 문단부터 구역 끝까지 제거합니다. `PrvText`는 머리말·꼬리말의 옛 문장이 한 번만 나올 때 동기화하고 아니면 stale로 알립니다. 스타일·그림 편집은 미리보기 텍스트를 바꾸지 않습니다. 미편집 ZIP 항목의 원시 압축 바이트·로컬 헤더 보존과 Undo 원본 전체 바이트 복원 규칙은 같습니다. 저장 관문은 세 요청을 편집 구현과 독립적으로 재계산합니다. Linux 한컴 게이트의 프로세스 생존은 Windows 호환성이나 화면 조판 대조를 의미하지 않습니다. 산출물 6종의 게이트·화면 관찰은 [좁은 편집 명령 한컴 게이트](decisions/narrow-edits-hancom-gate-2026-10-18.md)에 기록했습니다.


## 0.3 링크와 변경 추적 조회 (`link_list`, `trackchange_list`)

0.3에서 읽기 전용 조회 두 가지가 추가되었습니다. 두 명령 모두 문서를 바꾸지 않으며 `revision`·Undo 이력·세션 상태를 그대로 둡니다. 시험에서 명령 실행 전후 `document_export` 결과가 원본과 바이트 단위로 같음을 확인했습니다. 커서(`cursor`) 규약은 `document_inspect`와 같습니다. `revision`이 바뀐 뒤의 옛 커서는 `VERSION_CONFLICT`로 거절하고, 형식이 틀리면 `INVALID_REQUEST`입니다.

### 링크 목록 (`link_list`)

본문의 하이퍼링크 필드(`hp:fieldBegin type="HYPERLINK"`와 짝이 맞는 `hp:fieldEnd`)를 문서 순서대로 나열합니다. 링크를 담은 **문단 `nodeId`**, `fieldid`/`id`, `hp:parameters`의 **원문**(`Command`·`Path`·`Category`·`TargetType`·`DocOpenType` 등), **표시 텍스트**, 그리고 참조가 **문서 내부인지 외부인지**를 알려 줍니다.

```json
{"command":"link.list","params":{"limit":2}}
```

- **인자**: `cursor`(이전 응답의 `nextCursor`), `limit`(기본 100, 최대 500), `scope`(표·셀·문단 등 nodeId; 그 노드와 하위 문단만 조회).
- **결과**: `links[]`, `total`(scope 적용 후 전체 개수), `nextCursor`, `documentId`, `revision`.
- **`links[]` 항목**: `nodeId`, `section`, `entry`, `fieldType`, `fieldId`(`fieldid`), `beginId`(fieldBegin의 `id`), `name`, `editable`, `parameters[]`(`{kind,name,value}` 원문), `displayText`, `displayTextSource`, `reference`.
- **표시 텍스트**: `hp:script`가 있으면 그 글자, 없으면 fieldBegin과 짝이 맞는 fieldEnd **사이**의 글자입니다(`displayTextSource`: `script`/`between`/`none`). 한컴이 두 표식을 붙여서 저장한 문서는 사이 글자가 비어 `displayText`가 빈 문자열이 됩니다(로컬 기준 문서 `research-school-plan-reference.hwpx`의 링크 3건이 이 경우입니다). 엔진은 그때 주변 글자를 추측하지 않습니다.
- **내부/외부 판정**(`reference.scope`): `Category=HWPHYPERLINK_TYPE_HWP`이거나 `Path`가 `#`로 시작하거나 `Command`가 `?`로 시작하면(책갈피) `internal`입니다. `Category`가 `…_URL`/`…_EMAIL`이거나 `Path`/`Command`가 주소 형태(`https:`, `mailto:`, `file:`, `//`, `\\`)이면 `external`입니다. 어느 쪽도 아니면 `unknown`이며 **추측하지 않습니다**.
- **`reference.target`**: `Path`가 있으면 그 값, 없으면 `\` 이스케이프를 푼 `Command` 값입니다(책갈피는 `?책갈피이름;0;0;0;` 형태 유지). `Command`의 `;1;5;-1;` 꼬리와 이스케이프는 원문 그대로이므로 함께 해석하세요.

```json
{
  "command": "link.list",
  "params": {"scope": "표 또는 문단 nodeId"}
}
```

### 변경 추적 목록 (`trackchange_list`)

삽입·삭제 표시와 머리말의 변경 추적 기록을 보여 줍니다.

```json
{"command":"trackchange.list","params":{"limit":20}}
```

- **인자**: `link_list`와 같은 `cursor`/`limit`/`scope`.
- **결과**: `hasTrackChanges`, `readonlyReasons`, `marks[]`, `markCount`, `nextCursor`, `changes[]`, `authors[]`, `documentId`, `revision`.
- **`marks[]`**: 본문 표시(`hp:insertBegin`/`insertEnd`/`deleteBegin`/`deleteEnd`)마다 `kind`(요소 이름), `nodeId`(표시를 담은 문단, 표 셀 안 문단 포함), `section`, `entry`, `id`, `tcId`, `paraEnd`, `textOffset`(그 표시가 속한 `hp:t` 안에서의 코드포인트 위치)를 담고, 머리말 기록에서 `changeType`(`Insert`/`Delete`/`CharShape`/`ParaShape`), `date`, `authorId`, `authorName`을 연결합니다. 표시의 `TcId`가 `hh:trackChange@id`와 연결됩니다.
- **`changes[]`**는 `hh:trackChange` 기록(`id`, `type`, `date`, `authorId`, `hide`, `charShapeId`, `paraShapeId`), **`authors[]`**는 `hh:trackChangeAuthor` 기록(`id`, `name`, `mark`, `color`)입니다.
- **`hasTrackChanges`**는 본문 표시나 머리말 기록이 있거나 엔진이 변경 추적으로 잠근 문서면 true입니다.
- **잠금 유지**: 변경 추적이 있는 문서는 지금처럼 읽기 전용으로 열립니다(`readonlyReasons: ["track_changes"]`). 조회와 무편집 내보내기만 가능하고 편집 명령은 `UNSUPPORTED`(`document_readonly`)로 거절됩니다. `trackchange_list`는 이 잠금을 풀지 않습니다.
- **검증 범위**: 로컬 입력(픽스처·spikes)에는 변경 추적 문서가 없어 이 명령의 시험은 KS X 6101 표기(`Id`/`TcId`/`paraend`)와 `hh:trackChanges`/`hh:trackChangeAuthors` 스키마를 따른 **합성 문서로만** 수행했습니다. 실제 한컴 변경 추적 문서로는 아직 확인하지 않았습니다.

## 0.3 책갈피·메모 조회 (`bookmark_list`, `memo_list`)

0.3에서 읽기 전용 조회 두 가지가 더 추가되었습니다. 두 명령 모두 문서를 바꾸지 않으며 `revision`·Undo 이력·세션 상태를 그대로 둡니다. 시험에서 명령 실행 전후 `document_export` 결과가 원본과 바이트 단위로 같음을 확인했습니다. `cursor`/`limit`/`scope` 규약은 `link_list`와 같습니다(`revision:offset`, 기본 100·최대 500, 다른 `revision`의 커서는 `VERSION_CONFLICT`, 형식 오류는 `INVALID_REQUEST`, 목록 범위를 넘는 커서는 빈 쪽과 `nextCursor: null`).

### 책갈피 목록 (`bookmark_list`)

문서의 책갈피 표식을 문서 순서대로 나열합니다. 책갈피는 두 모양으로 저장되며 **둘 다** 보고합니다.

```json
{"command":"bookmark.list","params":{"limit":100}}
```

- **모양(`marker`)**: `bookmark`는 `hp:bookmark` 요소(한컴은 `hp:ctrl`의 유일한 자식으로 저장합니다. 한 점을 가리키는 표식), `field`는 `hp:fieldBegin type="BOOKMARK"`와 짝이 맞는 `hp:fieldEnd`(범위 표식)입니다.
- **인자**: `cursor`, `limit`(기본 100, 최대 500), `scope`(구역·표·셀·문단 `nodeId`; 그 노드와 하위 문단만 조회).
- **결과**: `bookmarks[]`, `total`(scope 적용 후 전체 표식 수), `nextCursor`, `documentId`, `revision`.
- **`bookmarks[]` 항목**: `nodeId`(표식을 담은 문단, 표 셀 안 문단 포함), `section`, `entry`, `marker`, `name`, `id`, `fieldId`, `graphemeOffset`, `endNodeId`/`endGraphemeOffset`(field 모양이고 짝이 맞는 `fieldEnd`가 있을 때만), `parameters[]`(field 모양의 `hp:parameters` 원문, `{kind,name,value}`).
- **`graphemeOffset`**: 표식이 그 문단 텍스트 안에서 차지하는 **자소(grapheme) 위치**입니다. `element_get`의 `textSpans`·`text_replace` 범위와 같은 단위이며, 그 위치의 문자는 표식의 자리 표시자(`U+FFFC`)입니다.
- **이름 없는 책갈피**: 한컴은 `name` 속성이 없거나 빈 책갈피도 저장합니다. 엔진은 그때 `name`을 `null`로 두고 `parameters`·주변 글자에서 이름을 **지어내지 않습니다**. 같은 이름이 여러 번 나오면 표식마다 한 번씩 그대로 보고하며 하나를 골라 합치지 않습니다.
- **검증 범위**: 공개 코퍼스 10종과 로컬 입력 `04_complex.hwpx`에는 책갈피가 없습니다(코퍼스 `hp:bookmark` 0건). 이 명령의 시험은 KS X 6101 `hp:bookmark@name`·`fieldBegin type="BOOKMARK"` 표기를 따른 **합성 문서로만** 수행했고, 실제 한컴 책갈피 문서로는 아직 확인하지 않았습니다.

### 메모 목록 (`memo_list`)

검토 메모를 문서 순서대로 나열합니다.

```json
{"command":"memo.list","params":{"limit":20}}
```

- **모양(`source`)**: `field`는 한컴이 실제로 저장하는 `hp:fieldBegin type="MEMO"`(본문은 그 컨트롤의 `hp:subList`), `memo`는 `hp:memo` 요소(본문은 `hp:paraList`/`hp:p`)입니다. 그룹형 `hp:memo`는 그 메모의 `ID`를 가리키는 MEMO 필드가 있으면 그 필드를 `paragraphNodeId`(달린 위치)로 삼고, 그 필드 자체도 별도 항목으로 함께 보고합니다.
- **인자**: `cursor`, `limit`(기본 100, 최대 500), `scope`(메모가 달린 문단이 scope 노드 또는 그 하위일 때만).
- **결과**: `memos[]`, `total`(scope 적용 후 전체 메모 수), `nextCursor`, `documentId`, `revision`.
- **`memos[]` 항목**: `id`(`ID` 파라미터 또는 `hp:memo@id`), `beginId`/`fieldId`, `number`/`rawNumber`, `author`, `createdAt`(`CreateDateTime` 원문), `memoShapeIdRef`, `shape`(머리말 `hh:memoPr@id`와 정확히 일치할 때만 해석, 아니면 `null`), `hidden`, `source`, `paragraphNodeId`(메모가 달린 문단), `graphemeOffset`(그 문단 안에서 표식의 자소 위치), `parentBeginId`(`Contents/memoExtended.xml`의 `he:memo@parentId`, 답글 관계), `paragraphCount`, `paragraphs[]`, `text`, `parameters[]`(field 모양의 원문).
- **본문이 여러 문단일 때**: `paragraphCount`에 문단 수를 담고 `paragraphs[]`에 문단마다 `nodeId`, 전체 `text`, `preview`(앞 120자소), `previewTruncated`, `graphemeCount`를 담습니다. `text`는 문단들을 `\n`으로 이어 붙인 본문입니다. 문단 전체 텍스트가 필요하면 `paragraphs[].nodeId`를 `element_get`에 넘기세요.
- **`hidden`(숨김 여부)**: 표식이 `hidden="1"`/`"true"` 같은 **불리언 속성을 실제로 밝힐 때만** true/false를 보고하고, 그런 표기가 없으면 `null`입니다. 지금까지 확인한 2011 `hp:fieldBegin(MEMO)`·`hp:memo`·`hh:memoPr` 표기에는 숨김 속성이 없어(로컬 `04_complex.hwpx` 메모 2건 모두 `null`) `false`로 단정하지 않습니다.
- **추측 금지**: `ID`·`Author`·`Number`·`CreateDateTime`이 없으면 그대로 `null`입니다. `MemoShapeIDRef`가 `hh:memoPr` 목록에 없으면(예: `04_complex.hwpx`의 `65535`) `shape: null`이며 기본 모양을 가정하지 않습니다.
- **검증 범위**: 로컬 한컴 문서 `spikes/work/stage4-e2e/inputs/04_complex.hwpx`의 메모 2건(작성자 `mattc`, 본문 `메모 테스트용.`·`댓글이 달린다.`, 둘째 메모는 첫째에 대한 답글)으로 실제 동작을 확인했습니다. 그룹형 `hp:memo`는 공개 코퍼스와 로컬 입력에 없어 **합성 문서로만** 확인했습니다.

## 0.3 구역 용지·여백·쪽 번호·표시 설정

기존 구역 하나를 `sectionIndex`(spine 순서, 0부터 시작, 기본 0) 또는 `scope`로 선택합니다.
`scope`는 **그 구역의 `hp:secPr`을 담은 최상위 문단의 nodeId**입니다.
`section.getPage` 응답의 `scope`를 그대로 사용할 수 있습니다. 두 선택자를 함께 주면 거절합니다.
구역·문단·속성 자식 요소를 생성하거나 삭제하지 않습니다. 없는 구역을 지정하면
`UNSUPPORTED(reason:structure_change_unsupported)`입니다.

`section.getPage`는 용지(`landscape`, `width`, `height`, `gutterType`), `margins`,
`textDirection`, `spaceColumns`, `startNum`, `visibility`, `grid`를 조회합니다.
각 항목은 `{"value":59528,"source":"explicit"}` 형태입니다. 없는 속성은
`{"value":null,"source":"unspecified"}`이며, 상속·응용 프로그램 기본값을 추론하지 않습니다.
수치 속성이 수치로 해석되지 않으면 원문 문자열을 명시 값으로 반환합니다.
플래그는 XML의 수치(0/1)를 반환합니다. `spaceColumns`는 단 사이 간격의 **조회**이며,
단 수·단 너비(`colPr`)의 편집은 이번 명령 범위에 포함하지 않습니다.

```json
{"command":"section.getPage","params":{"sectionIndex":0}}
{"command":"section.setPage","expectedVersion":0,"params":{"sectionIndex":0,"width":59528,"height":84186,"margins":{"left":7000,"right":7000}}}
{"command":"section.setPageNumbering","expectedVersion":1,"params":{"sectionIndex":0,"pageStartsOn":"ODD","page":3}}
{"command":"section.setVisibility","expectedVersion":2,"params":{"sectionIndex":0,"hideFirstHeader":true,"hideFirstPageNum":true}}
```

용지·여백 수치는 HWPUNIT이며 `width`·`height`는 1 이상, 여백은 0 이상입니다.
`landscape`는 원문 enum `NARROWLY`/`WIDELY`, `gutterType`은
`LEFT_ONLY`/`LEFT_RIGHT`/`TOP_BOTTOM`을 받습니다. 방향을 바꾸더라도 저장된 width·height를
자동 교환하지 않습니다. 본문 영역 검사에서만 `NARROWLY`의 물리 축을 교환합니다.
본문 폭은 물리 너비에서 좌·우·가로 제본 여백을 뺀 값, 본문 높이는 물리 높이에서
위·아래·머리말·꼬리말·세로 제본 여백을 뺀 값입니다. 하나라도 0 이하가 되면
`INVALID_REQUEST(reason:non_positive_body_area)`로 거절합니다.
검사에 필요한 원본 값이 빠졌거나 지원 범위의 수치/enum이 아니면 추정하지 않고 거절합니다.
누락된 **속성**은 요청에서 명시적으로 주면 추가할 수 있지만, `pagePr`·`margin` 등
누락된 **요소**를 만들지는 않습니다. `scope`/`sectionIndex`만 있고 편집 항목이 없으면 거절합니다.

`section.setPageNumbering`은 `pageStartsOn`(`BOTH`/`EVEN`/`ODD`)과 `page`(0 이상) 중
지정한 것만 바꿉니다. `pic`·`tbl`·`equation`은 그대로 유지합니다. `page:0`의 네이티브
연속 번호 의미를 별도로 재해석하지 않으며, 쪽 번호 표시 컨트롤을 새로 만들지 않습니다.

`section.setVisibility`는 `hideFirstHeader`, `hideFirstFooter`, `hideFirstPageNum`,
`hideFirstEmptyLine`, `showLineNumber`의 boolean과 `border`·`fill`의
`SHOW_ALL`/`HIDE_ALL`/`SHOW_FIRST`/`HIDE_FIRST` 중 지정한 것만 바꿉니다.
`hideFirstMasterPage` 등 요청 범위 밖 속성은 보존합니다.

세 편집 명령 모두 `expectedVersion`·`dryRun`·원자적 batch·Undo/Redo를 지원합니다.
실제 변경 때 해당 구역의 모든 문단(셀 안 문단 포함)의 `linesegarray`를 지우며,
다른 구역은 보존합니다. 본문 텍스트가 바뀌지 않으므로 UTF-8 `PrvText`는 바꾸지 않고,
기존 규칙대로 썸네일과 조판을 stale로 표시합니다. 동일 값 요청은 바이트·revision·이력을
그대로 유지하는 noop입니다. 저장 전 별도 검증기가 원본과 요청으로 기대 바이트를
다시 계산하여 요청 없는 속성 변경, 자식 순서 변경, 다른 구역 변경, 미편집 ZIP 항목의
재인코딩을 거절합니다. 한컴 Linux Beta 게이트의 프로세스 생존만으로 화면·인쇄·Windows
호환성을 확정하지 않습니다.

MCP 도구 설명:
- `section_get_page`: 기존 구역의 명시된 용지·여백·쪽 번호·표시·격자 값과 항목별 출처를 조회합니다.
- `section_set_page`: 요청한 용지·여백 속성만 바꾸고 본문 영역이 0 이하이면 거절합니다.
- `section_set_page_numbering`: 쪽 시작 방식·시작 번호만 바꾸며 그림·표·수식 번호는 보존합니다.
- `section_set_visibility`: 요청한 첫 쪽 표시 플래그와 테두리·채우기 값만 바꿉니다.

## 0.3 자원·표·그림 조회

`style.list` / `table.get` / `picture.list`는 열린 문서의 자원과 개체 구조를 읽습니다.
MCP 이름은 `style_list` / `table_get` / `picture_list`, CLI 별칭은
`style-list` / `table-get` / `picture-list`입니다. `expectedVersion`이나 요청 순번 없이
호출하며 문서·revision·Undo/Redo 이력을 바꾸지 않습니다.

세 명령의 결과는 `documentId`, `revision`, `items`, `total`, `nextCursor`를 갖습니다.
`limit`은 기본 100, 범위 1~500입니다. 응답 직렬화 예산에 도달하면 더 적은 항목과
`nextCursor`를 돌려주므로 null이 될 때까지 같은 필터로 이어서 읽습니다. cursor는 문서,
revision, 명령과 필터에 묶이며 다른 조건으로 재사용하면 `VERSION_CONFLICT`입니다.
응답은 MCP의 텍스트·structuredContent 중복 직렬화 공간까지 예약합니다. 단일 항목이나
필수 메타데이터가 예산을 넘으면 `RESOURCE_LIMIT`으로 거절하며 조용히 잘라내지 않습니다.

### 스타일·문단 모양·글자 모양·테두리

```json
{"command":"style.list","params":{"kind":"charPr","limit":100}}
```

`kind`를 생략하면 `style`, `charPr`, `paraPr`, `borderFill`을 모두 나열합니다.
각 항목의 `attributes`에는 원문 ID, 스타일의 `name`/`engName`/`paraPrIDRef`/`charPrIDRef`,
글자 모양의 `height`(HWPUNIT)·`textColor` 같은 XML 속성이 있습니다. 글꼴은
`fonts[{language,id,face}]`, 볼드/이탤릭은 `bold`/`italic`, 문단 정렬과 줄간격은
`alignments[]`/`lineSpacings[]`, 테두리는 `borders`에 담습니다. switch/case/default의
대체 줄간격도 XML 순서대로 모두 보고하며 한 값을 실제 적용값으로 추정하지 않습니다.
찾을 수 없는 글꼴 이름은 null입니다.

`useCount`는 해당 종류/ID를 가리키는 **실제 XML 참조 속성의 발생 횟수**입니다.
본문 참조는 `bodyReferenceCount`, 헤더 등 나머지 XML 참조는 `otherReferenceCount`이며
둘의 합이 `useCount`입니다. 스타일의 기반 모양과 nextStyle 참조도 포함합니다.
상속을 펼친 글자 수나 화면에 보이는 사용량을 의미하지 않습니다. 알려진 HWPX namespace의
비 namespace 속성만 세며 외부 namespace의 같은 이름 속성을 혼동하지 않습니다.

`unresolvedReferenceCount`, `unresolvedBodyReferenceCount`는 정의가 없는 자원을 참조한
전체/본문 발생 횟수입니다. `unresolvedExamples`는 종류·ID·entry·element·attribute와
가능하면 가까운 `nodeId`를 본문 구역 우선으로 앞 20곳까지 보고하고, 그 이상은
`unresolvedExamplesTruncated:true`로 알립니다. 이 집계는 kind/페이지 필터와 무관하게
문서 전체를 대상으로 합니다. 결함을 알리는 조회이며 문서를 자동으로 고치지 않습니다.
기존 미해결 참조 때문에 저장 검증이 실패하는 문서는 조회 후에도 export가 거절될 수 있습니다.

### 표 구조

```json
{"command":"table.get","params":{"scope":"구역 또는 문단 nodeId"}}
{"command":"table.get","params":{"table":"표 nodeId"}}
```

두 선택자를 생략하면 모든 표를 읽습니다. 둘을 함께 주면 지정 표가 scope 안에 있어야 합니다.
각 표는 `nodeId`, `parent`, `section`, `rows`/`cols`, `cellSpacing`, `outerMargins`,
`innerMargins`, `borderFillIdRef`와 `cells[]`를 보고합니다. 셀에는 0부터 시작하는
`address:{row,col,rowSpan,colSpan}`, `size`, `margins`, `hasMargin`, `borderFillIdRef`,
직접 담긴 `paragraphCount`와 앞 60 grapheme의 `textPreview`가 있습니다. 크기와 여백은
원문 HWPUNIT 문자열입니다. 셀별 여백과 표 기본 여백을 합쳐 유효 여백을 추측하지 않습니다.
중첩 표는 별도 표로 나오며 그 셀·문단을 바깥 표의 직접 셀·문단으로 중복 집계하지 않습니다.

병합 기준 셀은 실제 XML 셀로 `cells`에 남습니다. `coveredCells`는 **완전한 명시적 주소와
span**에서 계산한 가려진 좌표 목록으로, 기준 좌표는 제외합니다. `coveredBy`에 덮는 기준
셀 ID를 모두, `presentCellNodeIds`에는 그 좌표에 실제로 존재하는 셀 ID를 보고합니다.
겹치는 실제 셀도 삭제하거나 어느 셀이 승자인지 추정하지 않습니다. 실제 셀의 `coveredBy`도
같은 사실을 알립니다. 주소·span·크기 누락은 null이고 span을 1로 보정하지 않습니다.
거대 span 확장이나 단일 표가 응답/작업 예산을 넘으면 `RESOURCE_LIMIT`입니다.

### 그림·미디어

```json
{"command":"picture.list","params":{"scope":"구역 또는 문단 nodeId","limit":100}}
```

scope를 생략하면 문서 전체를 읽습니다. 그림마다 `nodeId`, `paragraphNodeId`, `section`,
`position`, `offset`, `size`, `orgSz`, `curSz`를 보고하며 위치의 기준·offset도 원문 속성으로
보존합니다. `captionText`는 캡션 문단의 텍스트이고 캡션이 없으면 null입니다.
`cropped`는 `imgClip`의 좌/상단이 0인지와 우/하단이 `imgDim`과 같은지를 비교합니다.
필수 값이 없어 판단할 수 없으면 null입니다. `hasEffects`, `hasShadow`, `effectProperties`,
`effectElements`, `flip`, `flipped`, 원문 `reverse`도 함께 제공합니다.

`media`에는 `itemId`, 원문 `href`, ZIP `entry`, payload의 `sha256`, 원문 `isEmbeded`,
`external`, `status`, `otherPictureNodeIds`가 있습니다. 동일 item 또는 동일 payload entry를
가리키는 다른 그림을 문서 전체에서 찾으며 scope 밖 중복 참조도 포함합니다. 서로 다른
entry의 바이트가 같다는 이유로 동일 미디어 참조로 간주하지는 않습니다.
명시적인 `isEmbeded="0"`/`"false"` 이미지 manifest 항목은 외부 참조로 읽습니다.
외부 href는 접근하거나 내려받지 않으며 entry/sha256은 null, status는 `external`입니다.
외부 그림은 `image.replace` 대상으로 지원하지 않습니다. 내장 여부 플래그가 없으면 원문 값과
external은 null로 남기되 ZIP payload가 있으면 해시는 제공합니다.


## 누름틀·하이퍼링크 필드 조회와 텍스트 채우기 (0.3)

`field.list` (MCP `field_list`, CLI의 `--command field.list`)는 문서의 필드를 읽기 전용으로 조회합니다. `expectedVersion` 없이 호출하며 선택 `scope`에 문단·표·셀·구역 nodeId를 주면 해당 영역과 교차하는 필드만 나옵니다. 각 항목의 `nodeId`는 **시작 마커를 담은 문단**이며, `paragraphIds`에는 다문단 필드가 걸친 문단이 나옵니다.

큰 문서는 `limit`(기본 100, 최대 500)과 `cursor`로 나눠 읽습니다. 결과의 `nextCursor`가 `null`이 아니면 그 값을 그대로 `cursor`에 넣어 다음 쪽을 이어 받습니다. 커서에는 조회 시점 `revision`이 들어 있어, 그 사이 문서가 바뀌면 쪽을 섞지 않고 `VERSION_CONFLICT`로 거절합니다. 커서는 같은 `scope`로 이어서 호출할 때만 의미가 있으므로, 중간에 `scope`를 바꾸려면 첫 쪽부터 다시 읽습니다. 형식이 틀린 커서는 `INVALID_REQUEST`, 목록 밖 인덱스는 빈 쪽과 `null` 커서입니다.

```json
{"command":"field.list","params":{"limit":100,"cursor":"0:100"}}
```

결과에는 `type`, `name`, `fieldid`, `id`, 현재 `text`, 빈 값 여부(`empty`), 여러 문단 여부(`multiParagraph`), 중첩 여부(`nested`), 셀 내부 여부(`inCell`), `setTextSupported`와 불가 `reason`이 있습니다. `command`는 Command 파라미터의 해석된 문자열이고 `commandRaw`는 엔터티 표기를 포함한 원래 내부 XML입니다. 필드 이름이 빈 문자열이어도 ID로 선택할 수 있습니다. 한컴이 실제로 쓰는 누름틀 표기는 `CLICK_HERE`이며 합성 픽스처에는 `CLICKHERE` 표기도 있습니다. 이 둘과 `HYPERLINK`만 채우기 후보이고 MEMO·교정부호 등은 조회만 가능합니다. 실제 한컴 서식에서는 `fieldid` 속성이 없고 `id`만 있는 경우가 많아, `fieldid` 선택자는 두 속성을 모두 받습니다.

`field.setText` (MCP `field_set_text`, CLI의 `--command field.setText`)에는 조회 결과의 문단 `target`, 새 `text`, 선택 `name` 또는 `fieldid`를 줍니다. `fieldid`는 XML의 `fieldid` 또는 `id` 중 하나와 일치해야 하며, 두 선택자를 모두 주면 모두 일치해야 합니다. 같은 문단에서 선택자와 일치하는 필드가 여러 개면 **`occurrence`를 명시**해야 합니다(`0`부터 시작). 생략하면 `AMBIGUOUS_TARGET`; 필드나 순번이 없으면 `TARGET_NOT_FOUND`입니다. 이름이 유일한 경우 생략된 순번은 0입니다.

```json
{"command":"field.setText","expectedVersion":0,"dryRun":true,
 "params":{"target":"조회한 문단 nodeId","name":"성명","text":"홍길동"}}
```

`text`에는 탭·줄바꿈을 넣을 수 없습니다. 이 명령은 필드 마커와 모든 속성·파라미터를 그대로 두고 **표시 텍스트만** 바꿉니다. 하이퍼링크의 URL/Command는 바뀌지 않습니다. 기존 run·글자 모양을 유지하며 첫 텍스트 조각에 새 글을 넣고 나머지 조각은 비웁니다(`FORMAT_COLLAPSED` 경고 가능). 빈 필드는 기존 run 안의 빈 텍스트 조각을 채우거나 안전한 위치에 텍스트 carrier를 넣습니다. 새 문단·run·모델 노드는 만들지 않습니다.

여러 문단·중첩·짝이 맞지 않는 마커, 개체·탭·불투명 요소, CDATA·주석·처리 명령(PI)이 있는 필드는 안전하게 거절합니다. `reason`은 각각 `multi_paragraph_field`, `nested_field`, `unpaired_field`, `field_contains_control_or_opaque`, `field_lexical_markup`, `unsupported_field_type`, `no_text_fragment`, `paragraph_not_editable` 중 해당 사유입니다. 안전한 기존 run도 없는 빈 필드는 `no_text_fragment`로 거절합니다.

다른 편집과 동일하게 현재 `expectedVersion`이 필요하고, `dryRun`은 실제 적용·이력 없이 독립 검증을 수행합니다. `command.batch`에 넣으면 전체가 한 revision·Undo 단위이며 실패 시 전체를 취소합니다. 재전송도 공통 writer/requestSequence 규칙을 따릅니다. Undo는 원본 HWPX 바이트를 복원하고 Redo는 편집 바이트를 복원합니다. linesegarray·PrvText 갱신 정책과 미편집 ZIP 항목의 압축 바이트·로컬 헤더 보존 계약도 동일합니다.

검증 근거는 두 갈래입니다. 첫째는 결정론적 생성기 `gen_fields.py`가 만드는 합성 픽스처 `tests/fixtures/editing/fields.hwpx`로, 빈 값·다문단·중첩·개체·탭·CDATA·MEMO·표 셀·중복 이름 같은 경계를 시험합니다. 둘째는 저장소에 공개 서식으로 들어 있는 실제 기안문 표본 `tests/fixtures/corpus/gani-gian.hwpx`(누름틀 13개)와 `tests/fixtures/corpus/ilban-gian.hwpx`(누름틀 23개)이며, 두 문서 모두 한컴이 저장한 `type="CLICK_HERE"` 표기를 씁니다. 실제 표본에서는 표 셀 안 누름틀 채우기, `id`로 지목하기, 여러 필드 일괄 채우기와 Undo 뒤 원본 바이트 복원, 한컴 Linux 열기 게이트까지 확인했습니다.

다만 이 표본은 공개 배포된 **서식 파일**이지 사용자가 실제로 쓰는 공문서가 아니며, Windows 한컴오피스 2024에서의 열기·조판 대조는 아직 하지 않았습니다. Linux 한컴 열기 게이트는 프로세스가 살아남았는지만 보고 내용·조판은 대조하지 않습니다.

## 구형 .hwp(HWP5) 읽기 전용 조회 (0.3)

`hwp5.inspect`(MCP `hwp5_inspect`, CLI의 `--command hwp5.inspect` 또는 별칭 `hwp5-inspect`)는 구형 HWP 5.0 바이너리 파일을 **열린 문서 없이** 직접 읽습니다. HWPX 파이프라인과는 별개 crate(`hae-hwp5`)이며, `document.open`을 비롯한 `document.*` 명령은 계속 HWPX 전용입니다. 이 명령은 파일을 바꾸지 않습니다(실행 전후 바이트와 mtime이 같습니다).

```json
{"command":"hwp5.inspect","params":{"path":"/입력루트/문서.hwp","limit":100,"previewChars":120}}
```

- `path`는 입력 루트 안의 `.hwp` 파일이어야 합니다. 다른 확장자는 `INVALID_ARGUMENT`(`extension_mismatch`)로 거절합니다.
- 결과에는 `fileHeader`(`version`은 `5.1.0.1` 형식, `compressed`·`encrypted`·`distribution`·`drm`, 원값 `properties`), `sectionCount`(컨테이너에서 찾은 `BodyText/SectionN` 수), `paragraphCount`, `charCount`, 구역별 `sections`, 그리고 `paragraphs` 한 쪽이 들어 있습니다.
- 각 문단 항목: `paragraphIndex`(문서 전체 순번), `sectionIndex`, `level`(`PARA_HEADER`의 level), `charCount`, `preview`, `previewTruncated`. `previewChars`(기본 120, 1..=1000)는 미리보기 길이를, `limit`(기본 100, 1..=500)은 쪽 크기를 정합니다.
- 페이징: 결과의 `nextCursor`(형식 `<sha256>:<offset>`)를 그대로 `cursor`에 넣어 이어 읽습니다. 커서는 **파일 바이트 해시**에 묶여 있어 같은 경로의 파일 내용이 바뀌면 `VERSION_CONFLICT`로 거절합니다. 형식이 틀린 커서는 `INVALID_REQUEST`, 문서 밖 offset은 빈 쪽과 `null` 커서입니다.
- 텍스트는 `BodyText/SectionN` 레코드 스트림의 `PARA_TEXT`를 UTF-16LE로 디코드합니다. 제어 문자는 HWP 5.0 표 6 분류대로 처리합니다: 줄 끝(0x0A)은 줄바꿈, 탭은 유지, 묶음 빈칸(0x1E)은 U+00A0, 고정폭 빈칸(0x1F)은 U+2007, 소프트 하이픈(0x18)은 U+00AD로 옮기고, 16바이트 컨트롤(0x01–0x09, 0x0B–0x0C, 0x0E–0x17)은 8코드유닛을 통째로 건너뛰며, 문단 끝(0x0D)에서 그 레코드를 끝냅니다. **자동 번호·수식·글자겹침처럼 조판할 때 만들어지는 글자는 PARA_TEXT에 없으므로 지어내지 않습니다.** `charCount`는 이 규칙으로 읽은 Unicode 스칼라 값 수이고, 표 셀 안 문단도 포함합니다(셀 경계는 구분하지 않습니다).
- 압축(`COMPRESSED`) 스트림은 raw DEFLATE로 풉니다. 해제에 실패하면 원시 바이트로 대신 읽지 않고 거절합니다.
- 거절 사유: 암호·DRM 문서는 `PACKAGE_REJECTED(encrypted)`, 배포용 문서는 `PACKAGE_REJECTED(distribution_unsupported)`, 필수 요소(FileHeader/BodyText)가 없으면 `missing_component`, 컨테이너 손상은 `malformed_compound_file`, 압축·레코드 손상은 `malformed_record_stream`입니다. 한도 초과는 `RESOURCE_LIMIT`입니다(해제 누적 `max_inflate_total`, 컨테이너 항목 `max_entries`, 문단 수 `max_hwp5_paragraphs` 기본 1,000,000, 입력 파일 크기는 다른 입력과 같은 `max_hwpx_bytes`).
- 지원하지 않는 것: 쓰기·변환 저장, 암호 문서, 배포용 문서, HWP 3.x, DocInfo의 폰트·스타일 해석과 표·컨트롤 구조 해석(구역 수는 DocInfo가 아니라 컨테이너의 `BodyText/SectionN` 열거로 셉니다), 수식·그림·글자겹침의 내용 추출.

검증 범위: 합성 CFB 컨테이너 시험 23개(`crates/hae-hwp5/tests/hwp5.rs`)와 명령 시험 5개(`crates/hae-exec/tests/hwp5.rs`)가 `scripts/check.sh`에 포함됩니다. 실물은 공개 표본 44개 중 43개를 읽었고 배포용 1개는 위 사유로 거절했으며(입력 44개 모두 실행 전후 SHA-256 동일), 독립 구현 `rhwp` `export-text`와 대조한 42개 중 비교 가능한 39개 가운데 27개가 공백 제거 후 문자 단위로 일치했습니다(나머지 12개는 자동 번호·수식·글자겹침·숨은 설명·캡션 자동 번호·조판 순서 등 조판 시 만들어지는 값 때문이고, 미리보기 1000자 상한을 넘는 3개는 문자 비교 불가). Linux 한컴 2020 Beta로 실물 표본 4개를 열어 모두 로드됐고(PASS) 그중 2개는 화면 텍스트까지 대조했지만, **Windows 한컴오피스에서의 열기·조판 대조는 하지 않았습니다.** 형식 근거와 확인/미확인 구분은 `docs/decisions/hwp5-format-notes.md`에 있습니다.

---

## 부록. 명령 60개 색인

`hae run --command command.list --params '{}'`로 얻은 60개 명령 전체입니다. 「문서를 바꿈」이 「예」인 명령만 현재 상태를 편집합니다(이력 명령 `history.undo`·`history.redo`도 문서 상태를 복원하므로 「예」). 「아니오」인 명령 중 `document.exportText`, `document.export`, `document.create`, `project.save`는 문서가 아니라 새 파일을 만듭니다. 분류는 `command.list`의 `category`를 풀어 쓴 것이며, 같은 이름의 내부 정책과 인자는 `command.describe`로 확인합니다.

| 명령 | MCP 도구 | 분류 | 문서를 바꿈 | 설명 위치 |
|---|---|---|---|---|
| `equation.list` | `equation_list` | 조회 | 아니오 | 수식 조회·편집 |
| `equation.get` | `equation_get` | 조회 | 아니오 | 수식 조회·편집 |
| `equation.setScript` | `equation_set_script` | 편집 | 예 | 수식 조회·편집 |
| `equation.setProperties` | `equation_set_properties` | 편집 | 예 | 수식 조회·편집 |
| `command.list` | `command_list` | 조회(명령) | 아니오 | ⑤-2 |
| `command.describe` | `command_describe` | 조회(명령) | 아니오 | ⑤-2 |
| `document.open` | `document_open` | 문서 열기·닫기 | 아니오 | ① |
| `document.inspect` | `document_inspect` | 조회 | 아니오 | ② |
| `document.stats` | `document_stats` | 조회 | 아니오 | ②-2 |
| `section.list` | `section_list` | 조회 | 아니오 | ②-3 |
| `note.list` | `note_list` | 조회 | 아니오 | ②-4 |
| `document.close` | `document_close` | 문서 열기·닫기 | 아니오 | ⑤-2 |
| `element.query` | `element_query` | 조회 | 아니오 | ② |
| `element.get` | `element_get` | 조회 | 아니오 | ② |
| `table.setColumnWidths` | `table_set_column_widths` | 편집 | 예 | ④-3 |
| `table.setRowHeights` | `table_set_row_heights` | 편집 | 예 | ④-3 |
| `cell.setFormat` | `cell_set_format` | 편집 | 예 | ④-3 |
| `cell.setSize` | `cell_set_size` | 편집 | 예 | ④-3 |
| `table.insertRow` | `table_insert_row` | 편집 | 예 | ④-4 |
| `table.deleteRow` | `table_delete_row` | 편집 | 예 | ④-4 |
| `table.insertColumn` | `table_insert_column` | 편집 | 예 | ④-4 |
| `table.deleteColumn` | `table_delete_column` | 편집 | 예 | ④-4 |
| `paragraph.insert` | `paragraph_insert` | 편집 | 예 | ④-5 |
| `paragraph.delete` | `paragraph_delete` | 편집 | 예 | ④-5 |
| `link.list` | `link_list` | 조회 | 아니오 | 링크와 변경 추적 |
| `trackchange.list` | `trackchange_list` | 조회 | 아니오 | 링크와 변경 추적 |
| `bookmark.list` | `bookmark_list` | 조회 | 아니오 | 책갈피·메모 |
| `memo.list` | `memo_list` | 조회 | 아니오 | 책갈피·메모 |
| `section.getPage` | `section_get_page` | 조회 | 아니오 | 구역 용지·여백 |
| `section.setPage` | `section_set_page` | 편집 | 예 | 구역 용지·여백 |
| `section.setPageNumbering` | `section_set_page_numbering` | 편집 | 예 | 구역 용지·여백 |
| `section.setVisibility` | `section_set_visibility` | 편집 | 예 | 구역 용지·여백 |
| `style.list` | `style_list` | 조회 | 아니오 | 자원·표·그림 조회 |
| `table.get` | `table_get` | 조회 | 아니오 | 자원·표·그림 조회 |
| `picture.list` | `picture_list` | 조회 | 아니오 | 자원·표·그림 조회 |
| `text.format` | `text_format` | 편집 | 예 | ④-2 |
| `text.replace` | `text_replace` | 편집 | 예 | ③ 및 0.2절 |
| `field.list` | `field_list` | 조회 | 아니오 | 누름틀·필드 |
| `hwp5.inspect` | `hwp5_inspect` | 조회 | 아니오 | 구형 .hwp(HWP5) |
| `field.setText` | `field_set_text` | 편집 | 예 | 누름틀·필드 |
| `text.replaceAll` | `text_replace_all` | 편집 | 예 | ③-2 |
| `paragraph.format` | `paragraph_format` | 편집 | 예 | ④ |
| `cell.setText` | `cell_set_text` | 편집 | 예 | ③ 및 0.2절 |
| `form.fill` | `form_fill` | 편집 | 예 | ③-3 |
| `image.replace` | `image_replace` | 편집 | 예 | ③-4 |
| `paragraph.setStyle` | `paragraph_set_style` | 편집 | 예 | 0.3 좁은 편집 명령 |
| `image.fit` | `image_fit` | 편집 | 예 | 0.3 좁은 편집 명령 |
| `header.setText` | `header_set_text` | 편집 | 예 | 0.3 좁은 편집 명령 |
| `command.batch` | `command_batch` | 편집 | 예 | ⑤-2 |
| `history.undo` | `history_undo` | 이력 | 예 | ⑤ |
| `history.redo` | `history_redo` | 이력 | 예 | ⑤ |
| `history.clear` | `history_clear` | 이력 | 아니오 | ⑤-2 |
| `document.snapshot` | `document_snapshot` | 스냅샷·비교 | 아니오 | ⑤-2 |
| `snapshot.release` | `snapshot_release` | 스냅샷·비교 | 아니오 | ⑤-2 |
| `document.diff` | `document_diff` | 스냅샷·비교 | 아니오 | ⑤-2 |
| `document.preview` | `document_preview` | 미리보기 | 아니오 | ⑥ |
| `project.save` | `project_save` | 저장·내보내기 | 아니오 | ⑦ |
| `document.create` | `document_create` | 저장·내보내기 | 아니오 | ⑨ |
| `document.export` | `document_export` | 저장·내보내기 | 아니오 | ⑦ |
| `document.exportText` | `document_export_text` | 조회 | 아니오 | ⑧ |
