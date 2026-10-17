# M3 실제 MCP 클라이언트 연결 시험 — 2026-10-18

대상: 브랜치 `wip/stage1-model-edit`, 시험 기준 HEAD `13a2bdc`.

엔진 코드는 수정하지 않았다. 입력은 `/tmp/m3/in`, 출력은 `/tmp/m3/out`만 사용했다. 최종 커밋에는 이 결과 문서와 [MCP 서버 완성 계획](../superpowers/plans/2026-10-18-mcp-server-completion.md)만 넣고 push하지 않는다.

## 준비 및 환경

이미 존재하던 release 바이너리를 사용했다: `hae-mcp 0.2.0`, `hae-raster`. 별도 `cargo build --release`는 생략했다. 독립 실행 파일 `target/release/hae-verify`는 없었다. 대신 `document_export`의 저장 전 독립 검증과 별도 새 `hae-mcp` 프로세스의 재열기·구조 조회를 사용했다.

복사한 fixture:

| 파일 | 분류 및 사용 | 원본/복사본 SHA-256 |
|---|---|---|
| `tests/fixtures/corpus/research-school-plan-reference.hwpx` | 공개 MIT 템플릿. 표, 그림, 텍스트가 포함된 그림 문단, 그룹 도형(`hp:container`)이 있어 세 에이전트 세션의 입력으로 사용 | `0b34ae667d371f0183cb6942e9b5c0cde4719f0b5dabe5a4736dbfbb09ac1f60` |
| `tests/fixtures/editing/tabs_control.hwpx` | 합성 fixture. `/tmp/m3/in`에 복사했으나 주 fixture에서 개체 포함 문단을 찾을 수 있어 열지는 않음 | `f2748507467daa99e5938db24f437e001f3cad290d02b2fb6a30a2b5919476b5` |

두 입력의 저장소 원본과 `/tmp` 복사본 해시는 준비 시점과 종료 시점에 모두 일치했다.

`hae-mcp doctor --input-root /tmp/m3/in --output-root /tmp/m3/out` 결과:

| 항목 | 결과 |
|---|---|
| input/output roots | 읽기 가능 / 쓰기 가능 |
| `bwrap`, 글꼴 폴더, `hae-raster` | 사용 가능 |
| `rhwp` | 설정·실행 파일 없음 |
| doctor 전체 | `ok:false` — 미리보기 렌더러 항목만 실패 |

따라서 미리보기는 환경상 사용할 수 없었다. Codex는 도구 호출 오류도 확인했고 OpenCode는 doctor 결과에 따라 건너뛰었다.

## 클라이언트별 결과

| 클라이언트 | 버전 / 모델 | `hae` 도구 | 세션 결과 | export / 재열기 | 사용량 |
|---|---|---:|---|---|---|
| Claude Code | 2.1.296 / Haiku (`claude-haiku-5-5`) | **0/22** | 서버 상태는 `connected`였으나 도구 검색 3회 모두 0개. 시나리오는 시작하지 못함 | 없음 | `$0.00538891`; input 8, cache creation 16,750, cache read 90,761, output 1,773 |
| Codex CLI | 0.163.0 / `gpt-6-luna`, reasoning `low` | **22/22** | 열기·조회·텍스트 편집·`controls:"preserve"`·셀 편집·replace-all 16건·Undo 1회·export 완료. `structuredContent` 응답을 처리했으며 거절 오류 없음 | 성공, version 5. 새 프로세스 열기 성공 | input 857,074, cached input 793,600, output 1,792. CLI가 비용은 표시하지 않음 |
| OpenCode | 2.0.30 / `opencode-go/deepseek-v4.1-flash` | **22/22** | 열기·조회·텍스트 편집·`controls:"preserve"`·셀 편집·replace-all 17건·Undo 1회·export 완료. `outputSchema` 조회 성공 | 성공, version 5. 새 프로세스 열기 성공 | input 85,092, cache read 588,800, output 3,877, reasoning 6,763; `$0.0209142` |
| MCP Inspector CLI | 2.11.0 | **22/22** | `tools/list --strict` 종료 코드 0. `command_describe`와 `document_open` 호출 성공. 두 응답 모두 `structuredContent`가 있었음 | 대표 `document_open` 성공 | 모델 호출 없음 |

세 도구 목록에는 모두 `outputSchema`가 있었고 Inspector strict 검사에서 22개 전부 통과했다. OpenCode의 `command_describe("text.replace")`는 `inputSchema`, `outputSchema`, annotations를 모두 반환했다. Codex JSON 이벤트는 응답 필드를 `structured_content` 이름으로 노출했지만 도구 응답을 거절하지 않았다. Claude는 도구를 받지 못해 스키마 수용 여부를 확인할 수 없었다.

### 실제 세션 호출과 결과

세 에이전트에게 동일한 핵심 한국어 시나리오를 한 번씩 전달했다. 각 prompt는 마지막 argv로 전달했고, OpenCode는 검사 범위를 30개로 제한했다. prompt 파일과 SHA-256은 `/tmp/m3/{claude,codex,opencode}-prompt.txt` 및 같은 이름의 `.prompt.sha256`에 남아 있다.

| prompt | SHA-256 |
|---|---|
| Claude | `8706dd1f872c11650aa10894729fcb0bc88a033aae52e999456493e07e0c0fa2` |
| Codex | `c12a5c0097ba05ea08d751a02be6254e212897ec0ef98e08248f04967733ccd7` |
| OpenCode | `9ccaee9b16c08a8f34aa0459529864bfded52352694d85555125e9b58283b2d0` |

Claude는 실제 사용 전에 `--verbose` 필수 오류가 나서 수정했다. Codex는 임시 cwd가 Git 저장소가 아니라는 사전 확인에서 멈춰 `--skip-git-repo-check`를 추가했다. 이 두 사전 호출은 모델 세션이나 MCP 도구 호출을 시작하지 않았다.

**Claude Code** — cwd `/tmp/m3/claude`; 임시 설정 `/tmp/m3/claude-mcp.json`은 `mcpServers.hae.command`를 release 바이너리로, args를 `/tmp/m3/in` 및 `/tmp/m3/out`으로 설정했다. 실행 argv:

```json
{"mcpServers":{"hae":{"command":"/home/mattc/Documents/Codex/hwpx-agent-engine/target/release/hae-mcp","args":["--input-root","/tmp/m3/in","--output-root","/tmp/m3/out"]}}}
```

```text
claude -p --mcp-config /tmp/m3/claude-mcp.json --strict-mcp-config --allowedTools "mcp__hae__*" --model haiku --no-session-persistence --output-format stream-json --verbose <prompt from /tmp/m3/claude-prompt.txt>
```

초기 시도는 `--verbose`가 없어 `stream-json requires --verbose`로 거절되었다. 수정된 실제 세션에서는 초기화 이벤트에 `mcp_servers:[{name:"hae",status:"connected",source:"dynamic"}]`가 있었지만, `ToolSearch`의 `hae document`, `mcp__hae__document_open`/도구명, `mcp__hae` 검색이 모두 `No matching deferred tools found`를 반환했다. 해당 세션의 도구 목록에도 `mcp__hae`가 없었다. 파일 열기나 변경은 전혀 발생하지 않았다.

**Codex CLI** — cwd `/tmp/m3/codex`; `--ignore-user-config`와 `--ephemeral`을 사용했고 전역 설정을 쓰지 않았다. 실행 argv:

```text
codex exec --model gpt-6-luna -c 'model_reasoning_effort="low"' -c 'mcp_servers.hae.command="/home/mattc/Documents/Codex/hwpx-agent-engine/target/release/hae-mcp"' -c 'mcp_servers.hae.args=["--input-root","/tmp/m3/in","--output-root","/tmp/m3/out"]' --ignore-user-config --ephemeral --skip-git-repo-check --json --sandbox danger-full-access <prompt from /tmp/m3/codex-prompt.txt>
```

한 문단 조회에서 모델이 지원되지 않는 `limit` 필드를 한 번 보냈다. `INVALID_REQUEST`에 허용 필드(`kind`, `textContains`, `ancestor`, `table`, `row`, `col`)가 명시되어 재조회 후 계속했다. 셀 편집을 문단 편집보다 먼저 수행했지만 revision은 순차적으로 0→1→2→3→4→5였고 작업은 완료되었다. stale-version probe는 `VERSION_CONFLICT`로 거절되었다. preview 호출은 `RENDERER_UNAVAILABLE`, `no rhwp preview adapter is configured`로 끝났다. stderr에는 사용하지 않은 `mcp.getrunpod.io` 리소스에 대한 nonfatal `AuthRequired` worker 메시지가 있었으며, 기록된 도구 호출은 모두 `hae` 서버 대상이었다.

**OpenCode** — cwd `/tmp/m3/opencode`; 그 폴더의 `opencode.json`에 프로젝트 로컬 `mcp.hae` 서버를 지정했다. 설정은 안내서 JSON 형태와 같고 command는 release `hae-mcp`, roots는 `/tmp/m3/in`, `/tmp/m3/out`이다. 실행 argv:

```json
{"$schema":"https://opencode.ai/config.json","mcp":{"hae":{"type":"local","command":["/home/mattc/Documents/Codex/hwpx-agent-engine/target/release/hae-mcp","--input-root","/tmp/m3/in","--output-root","/tmp/m3/out"],"enabled":true}}}
```

```text
opencode run --model opencode-go/deepseek-v4.1-flash --format json <prompt from /tmp/m3/opencode-prompt.txt>
```

시나리오 순서대로 revision 0→1→2→3→4→5를 완료했다. 셀 검색 첫 호출에 row/col을 빼 `INVALID_REQUEST`를 받았고, 안내에 따라 row/col을 지정해 성공했다. stale-version 오류는 current/expected revision이 같이 반환되어 이해할 수 있었다. 문단 조회는 30개 이내로 제한하고 필요한 대상은 `element_query`/`element_get`으로 좁혔다.

### MCP Inspector CLI

안내서 예시의 서버 실행 경로와 roots만 현재 환경 값으로 바꾸어 실행했다.

```bash
npx -y @modelcontextprotocol/inspector --cli /home/mattc/Documents/Codex/hwpx-agent-engine/target/release/hae-mcp --input-root /tmp/m3/in --output-root /tmp/m3/out --method tools/list --strict
npx -y @modelcontextprotocol/inspector --cli /home/mattc/Documents/Codex/hwpx-agent-engine/target/release/hae-mcp --input-root /tmp/m3/in --output-root /tmp/m3/out --method tools/call --tool-name command_describe --tool-arg command=text.replace
npx -y @modelcontextprotocol/inspector --cli /home/mattc/Documents/Codex/hwpx-agent-engine/target/release/hae-mcp --input-root /tmp/m3/in --output-root /tmp/m3/out --method tools/call --tool-name document_open --tool-arg path=/tmp/m3/in/research-school-plan-reference.hwpx
```

Claude/Codex 안내서에는 각각 persistent project/global 등록 예시가 있어 사용하지 않았다. 이를 요청에 지정된 one-shot Claude 설정과 Codex `-c` 호출 범위로 대체했다. OpenCode는 임시 cwd의 프로젝트 로컬 `opencode.json`을 그대로 시험했다.

## 산출물과 독립 검증

Codex 및 OpenCode export 모두 새 파일이었다. `ZipFile.testzip()`에서 손상 항목이 없었고, 각 파일의 46개 ZIP 항목과 8개 BinData 항목을 확인했다. 별도 `hae-mcp` 프로세스로 두 export를 다시 열어 모두 `ok:true`, `nodeCount:1374`를 받았다.

| export | SHA-256 | 재열기/개체 확인 |
|---|---|---|
| `/tmp/m3/out/codex-m3.hwpx` | `c694eb32f500151cd8441f2d27e96e4b23f8e58763cb5ee0dc1ca5f030e17eb1` | 성공. `hp:container` 그룹 도형이 원래 문단에 남았고 텍스트 편집이 반영됨. `hp:pic` 전체 18개 유지 |
| `/tmp/m3/out/opencode-m3.hwpx` | `1218e1e8537b897167bfac7851edc197072c24d3d711828c3837f27df316b318` | 성공. `hp:pic` 문단의 텍스트 변경과 그림 개체 유지 확인. `hp:pic` 전체 18개 유지 |

두 세션 모두 `controls:"preserve"`를 개체가 포함된 문단에 적용했다. 실제 범위는 leading object marker 뒤의 편집 가능 text span에서 시작했으므로 개체 경계를 가로지르는 range의 별도 동작까지 검증한 것은 아니다. Export 후 object와 수정 텍스트가 함께 남은 것은 확인했다.

Undo 뒤 Codex export에는 원래 남아 있던 16개의 `AI-SPARK`와 이전 preserve 편집으로 바뀐 1개의 `M3-SPARK`가 있었다. OpenCode export에는 `AI-SPARK` 17개, `M3-SPARK` 0개가 있어 replace-all 17건만 Undo되고 앞선 변경은 유지된 것을 확인했다.

## 결함 및 재현

### M3-F1 — 높음: Claude Code one-shot 세션에서 연결된 서버의 도구가 노출되지 않음

서버는 `connected`로 표시되지만 tool discovery에서 `hae` 도구가 0개여서 Claude Code 자연어 시나리오를 진행할 수 없었다. 현재 증거로는 Claude Code MCP 도구 노출 단계의 연결 차단이며, 엔진 코드 원인은 확인하지 못했다.

재현(현재 폴더의 임시 설정과 prompt 사용):

```bash
cd /tmp/m3/claude
claude -p --mcp-config /tmp/m3/claude-mcp.json --strict-mcp-config --allowedTools "mcp__hae__*" --model haiku --no-session-persistence --output-format stream-json --verbose "List the available hae MCP tools and report their count."
```

관찰: init의 server status는 `connected`; `ToolSearch` 결과는 `No matching deferred tools found`; HAE 도구 호출과 출력 파일은 0개. 위 한 줄은 최소 재현 prompt이고 실제 전체 시나리오 prompt의 SHA-256은 `/tmp/m3/claude.prompt.sha256`에 기록했다.

### M3-F2 — 낮음: Inspector가 `uint64` outputSchema 형식을 알 수 없어 경고

`document_open` 응답을 거절하지는 않았지만 stderr에 아래 경고가 두 번 나왔다: `unknown format "uint64" ignored in schema at path "#/properties/result/properties/revision"`. 이는 Inspector validator의 schema-format 경고이며 tool result는 `isError:false`, `structuredContent` 포함으로 성공했다.

재현:

```bash
npx -y @modelcontextprotocol/inspector --cli /home/mattc/Documents/Codex/hwpx-agent-engine/target/release/hae-mcp --input-root /tmp/m3/in --output-root /tmp/m3/out --method tools/call --tool-name document_open --tool-arg path=/tmp/m3/in/research-school-plan-reference.hwpx
```

두 오류 모두 작업 대상 결함은 아니었다: Codex의 `element.query` 미지원 `limit` 필드는 명확한 허용 필드 안내로 수정했고, OpenCode의 셀 조회 누락 인자는 row/col 안내로 수정했다. Stale-version 오류는 `VERSION_CONFLICT`와 두 revision 값을 반환해 원인과 재조회 방법을 알아보기 쉬웠다.

## 비용 절감 범위 및 남은 일

- 전체 22개 도구를 모두 호출하지 않고 사용자 지정 시나리오와 Inspector 대표 호출만 수행했다.
- Codex는 `document_inspect` 250개 항목을 요청해 입력 token 수가 컸다. OpenCode는 30개로 제한하고 표적 조회를 사용했다.
- `rhwp`가 없어 preview 렌더링은 생략/실패했다. Windows 한컴 조판 확인, 전체 `check.sh`·oracle·evaluate 재실행은 이 M3 연결 세션의 범위에서 수행하지 않았다.
- Claude 연결 장애는 재시도하지 않아 추가 Haiku 사용량을 발생시키지 않았다.
- M3 결과 문서와 계획표만 기록 대상으로 삼았다. 엔진 코드 변경과 push는 없다. M4 독립 검증은 별도 단계로 남는다.
