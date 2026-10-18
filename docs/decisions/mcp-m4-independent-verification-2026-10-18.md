# M4 독립 검증 — 2026-10-18

검증자: Claude Sonnet 5.5. 구현자(GPT 6.1 Sol)와 다른 모델이다.
대상: 브랜치 `wip/stage1-model-edit`, HEAD `363e50f`. MCP 작업 범위는 `e10b0b1..363e50f`다.
엔진 코드와 사용자 전역 설정은 수정하지 않았다. 사용자 문서(`spikes/work/stage4-e2e/inputs/`)와 `spikes/work/hancom-sdk-*/`는 쓰지 않았다. `editable_documents` 시험이 읽는 로컬 입력은 시험 자체의 동작이다.

이 문서는 이전 오케스트레이터(DeepSeek V4.1 Flash)의 미완성 초안을 대체한다. 초안의 §2 "M3-F1은 엔진 결함이 아니다(환경 문제)"는 **틀렸다**. 정정 내용은 §3, 초안 처리 내역은 §11에 있다.

원칙: 기록된 수치를 믿지 않고 이번 세션에서 다시 실행해 얻은 값만 쓴다. 임시 산출물은 `/tmp/m4f/`, 빌드 산출물은 디스크 여유 때문에 `~/.cache/hae-m4f/target`(`/tmp/m4f/target`이 가리킴)에 두었다. 로그와 시험 스크립트는 커밋하지 않는다.

## 1. 결론

- 수용 권고: **조건부 예**. 게이트 수치는 모두 기록과 일치했고 실제 클라이언트 두 곳이 22개 도구로 동작한다. 조건은 낮음 등급 결함 한 건(M4-D1)을 고치거나 문서에 남기는 것이다. 차단 결함은 없다.
- **M3-F1은 엔진 결함(높음)이었고 363e50f에서 해결됐다.** 원인은 현대식(2026-07-28) `tools/list` 결과의 필수 필드 `ttlMs`·`cacheScope` 누락이다. §3의 대조 실험으로 인과를 확인했다.
- 범위 밖이며 이번에 검증하지 못한 것: 실제 `rhwp` 미리보기 렌더링, Windows 한컴 조판, 요청 전체의 강제 시간 상한(Sol이 보류하고 사유를 기록함).

## 2. 게이트 재실행

| 게이트 | 기록 | 이번 재실행 | 일치 |
|---|---|---|---|
| `bash scripts/check.sh` | 종료 0, Rust 315·Python 33 | 종료 0, Rust **315**(실패 0·무시 0), Python **33** | 예 |
| oracle (`python3 -m unittest discover -s tests/oracle`) | 98 | **98** OK | 예 |
| `scripts/evaluate_mcp.py` | 146/146 | **146/146**(passed, failed 0) | 예 |
| stdio 적합성 (`cargo test -p hae-mcp --test protocol_conformance`) | 13/13 | **13/13** | 예 |
| `cargo test -p hae-exec --test editable_documents` | 3/3 | **3/3**(SKIP 없음, 211초) | 예 |
| Inspector strict `tools/list` (release, 2.11.0) | 도구 22·오류 0·경고 0 | 종료 0, 도구 **22**, stderr 오류·경고 **0** | 예 |
| Inspector `tools/call` (`document_open`, `command_describe`) | 오류 0·경고 0 | 종료 0, `isError:false`, `structuredContent.ok:true`, text JSON과 동일, `unknown format` 경고 **0** | 예 |
| `hae-mcp doctor` | 6/6 | rhwp 미설정: 5/6, `ok:false`, 종료 1. `HAE_RHWP`에 실행 파일을 지정: **6/6**, `ok:true`, 종료 0. 없는 입력 루트: 종료 1 | 예 |
| release 빌드 | 종료 0 | `cargo build --release --locked -p hae-mcp -p hae-render-rhwp` 종료 0, `hae-mcp --version`은 `hae-mcp 0.2.0` | 예 |

주의:
- doctor의 6/6은 경로·권한 점검이다. 이번에는 `HAE_RHWP`에 실제 렌더러가 아닌 셸 스크립트 스텁을 지정했다. 렌더러 실행이나 미리보기는 검증하지 않았다.
- Inspector `--strict`는 Inspector CLI가 `tools/list`에만 허용한다. `tools/call`은 strict 없이 실행했다.
- 게이트는 고정 HEAD `363e50f`의 작업 트리(미추적 파일은 이 문서뿐)에서 실행했다. `git status`는 실행 후에도 같았다.

## 3. M3-F1 정정 — 엔진 결함(높음), 해결됨

초안은 M3-F1을 "Claude Code 환경 제약"으로 정정했다. 이것이 틀렸다.

### 원인

Claude Code 2.1.296은 `initialize` 없이 `server/discover` → `tools/list`로 MCP 2026-07-28을 사용한다. 2026-07-28에서 `tools/list` 결과(`ListToolsResult`)는 `CacheableResult`를 상속하므로 `ttlMs`(number, >= 0)와 `cacheScope`(`"public"`|`"private"`)가 필수다. f013a23 이전 서버는 이 두 필드를 내지 않았다. 클라이언트는 결과 검증에 실패하면 목록 전체를 버리고, 서버는 `connected`로만 표시된다. 그래서 "서버는 connected인데 도구가 0개"였다.

### 이번에 직접 얻은 증거

1. **명세 원문 대조.** 공식 `schema.ts`(2026-07-28)를 받아 SHA-256을 계산했다. `742750af0bb8c716e7030c4977c992b55d1adc4407e9e66997db5846baedc2cd`로 Sol 기록과 일치한다. `CacheableResult`를 상속하는 타입은 `DiscoverResult`, `ListResourcesResult`, `ListResourceTemplatesResult`, `ReadResourceResult`, `ListPromptsResult`, `ListToolsResult` 여섯 개다. `PaginatedResult`는 `nextCursor`만 선택으로 더한다. 공식 캐싱 페이지는 "Servers MUST provide a ttlMs value that is >= 0"이라고 쓴다.
2. **인과 대조 실험(이번에 새로 수행).** 고친 release 바이너리를 같은 상태로 두고, 그 앞에 응답 필터(`/tmp/m4f/strip.py`)만 바꿔 Claude Code 2.1.296(Haiku, `--strict-mcp-config`, `--allowedTools "mcp__hae__*"`)을 실행했다. 필터가 하는 일은 지정한 응답에서 `ttlMs`·`cacheScope`를 지우는 것뿐이다.

| 필터 | init 이벤트의 `mcp__hae__*` 도구 | 서버 상태 |
|---|---|---|
| 없음(대조군) | **22** | connected |
| `tools/list` 응답에서 두 필드 제거 | **0** | connected |
| `server/discover` 응답에서만 두 필드 제거 | **22** | connected |

   `tools/list`의 두 필드 누락 하나만으로 증상이 재현된다. `server/discover` 응답의 누락만으로는 이 클라이언트가 도구를 버리지 않는다(그래도 명세 위반이므로 둘 다 고친 것이 맞다).
3. **초안의 "표준 스키마 최소 서버도 안 뜬다"는 같은 원인으로 설명된다.** 그 프로브 서버 `/tmp/m3/ab/server.py`에는 `ttlMs`가 한 번도 나오지 않는다(grep 0건). 즉 최소 서버도 같은 필드를 빼서 실패한 것이다. 초안은 이 공통점을 보지 못했다.
4. **고친 바이너리의 실제 세션(§8).** 응답을 바꾸는 프록시 없이 Claude Code가 도구 22개를 등록하고 `document_open → text_replace → document_export`를 완료했다.

### 판정

M3-F1은 **높음, 엔진 결함, 363e50f에서 해결**. Sol의 정정(`ttlMs:300000`, `cacheScope:"public"`)을 지지한다. "로컬 stdio 서버는 이 환경에서 도구를 노출하지 않는다"는 초안 결론과 "Claude Code 쪽 확인이 필요하다"는 권고는 폐기한다. 안내서에 우회 경로를 넣으라는 초안 제안도 필요 없다.

## 4. 캐시 관련 메서드 전수 대조 (2026-07-28)

캐시 필수 대상 여섯 메서드를 실제 프로세스에서 모두 호출했다.

| 메서드 | 현대식 요청 결과 | 판정 |
|---|---|---|
| `server/discover` | `resultType:"complete"`, `supportedVersions`(5개), `capabilities`, `instructions`, `_meta.serverInfo`, **`ttlMs:300000`, `cacheScope:"public"`** | 명세 필수 충족 |
| `tools/list` | `tools`(22), `resultType`, `_meta`, **`ttlMs:300000`, `cacheScope:"public"`** | 충족 |
| `prompts/list` | -32601 | 미구현, capability 미선언이라 적절 |
| `resources/list` | -32601 | 상동 |
| `resources/templates/list` | -32601 | 상동 |
| `resources/read` | -32601 | 상동 |

그 밖의 미구현 메서드(`prompts/get`, `completion/complete`, `subscriptions/listen`, `logging/setLevel`)도 -32601이다. 빈 성공 결과로 필수 필드를 빠뜨리는 경로는 없다.

**해당 리비전에서만 나오는지.** 다음을 실제로 확인했다.
- 2024-11-05, 2025-03-26, 2025-06-18, 2025-11-25로 `initialize`한 뒤 `tools/list`: 결과 키는 `["tools"]`뿐이다. `ttlMs`·`cacheScope`·`resultType`이 없다.
- 현대식 `tools/call`: 키가 `_meta, content, isError, resultType, structuredContent`다. 캐시 필드는 없다. 현대식 `ping`에도 캐시 필드를 붙이지 않는다.
- 같은 프로세스에서 legacy 초기화 후 현대식 요청과 legacy 요청을 섞어도 상태가 섞이지 않는다(현대식에만 캐시 필드, legacy에는 없음).

값 선택(public, 300000ms)은 고정 레지스트리와 서버 신원만 담는 응답이므로 타당하다. 문서·경로·호출자 데이터가 없고 `tools.listChanged:false`와 양립한다.

## 5. M3-F2와 M4 §5.4(documentId 안내) — 해결 확인

| 항목 | 이번 재현 |
|---|---|
| M3-F2 비표준 `uint64` format 경고 | release Inspector `document_open` 호출 stderr에 `unknown format` **0건**(f013a23 이전에는 2건). 22개 도구의 inputSchema·outputSchema를 재귀 순회해 `format` 키를 찾았고(`default`·`examples`·`const`·`enum` 값은 제외) **0건**이다. 현대식·legacy 목록 모두 같다. `command_describe` 결과 안의 스키마도 적합성 시험이 검사한다 |
| 정수 범위 보존 | `minimum`/`maximum`이 대신 들어간다. 예: `expectedVersion`은 0..18446744073709551615. 이 최댓값은 부동소수점 JSON 파서에서 정밀도를 잃는 크기라는 점만 메모한다(경고는 나오지 않음) |
| M4 §5.4 `instructions`의 documentId 서술 | `server/discover`와 `initialize`의 `instructions`가 동일하며, "2026-07-28 무상태 요청은 모든 문서 도구 호출에 `arguments.documentId` 필수, initialize 기반 legacy는 선택·기본값은 열린 문서"로 구분한다. 현대식 `tools/list`의 문서 도구는 `documentId`가 `required`에 들어 있고, 누락 호출은 `document_open` 안내가 담긴 `INVALID_REQUEST`다 |

## 6. M0 감사 항목(F-01~F-14) 판정

| 항목 | 판정 | 이번 확인 |
|---|---|---|
| F-01 큰 정수 id 변형 | 해결 | id `123456789012345678901234567890`, `-1234…`이 원문 그대로 반환 |
| F-02 객체·배열 id 에코 | 해결 | 객체·배열·bool·소수(`1.5`, `1e2`)·`null` id는 `id:null`, -32600. 문자열 `"s-1"`, 정수 `0`은 그대로 |
| F-03 id null 수락 | 해결 | -32600 |
| F-04 최신 판·2025-11-25 | 해결 | 5개 리비전 협상(§7). 2026 `initialize`나 미지원 버전은 2025-11-25로 응답 |
| F-05 initialize 이전 요청 | 해결 | `tools/list`·`tools/call`은 -32600, `ping`만 허용. `notifications/initialized` 뒤에 `tools/list` 성공 |
| F-06 protocolVersion 누락 | 해결 | -32602 |
| F-07 outputSchema·structuredContent | 해결 | 22개 전부 검증(§8) |
| F-08 annotations | 대체로 해결 | 4종 힌트가 모두 있다. 동작 대조에서 M4-D1(낮음) 한 건 |
| F-09 오류 분류 | 해결 | 알 수 없는 도구·`arguments` 비객체는 -32602. 알려진 도구의 인자·경로·세션 오류는 `isError:true` |
| F-10 panic 방어 | 부분 확인 | `catch_unwind`와 격리는 코드에서 읽었고 단위 시험을 통과했다. 운영 바이너리에 panic을 주입할 방법이 없어 실제 프로세스 시험은 하지 못했다 |
| F-11 시간 상한 | 판단 기록으로 갈음 | 강제 상한은 도입하지 않았다. 동기 executor의 commit·publish 안전성 때문이라는 사유가 M1·M2 기록에 있다. 이번 검증에서 새 반례는 없었다 |
| F-12 doctor | 해결 | §2 |
| F-13 스키마 이식성 | 해결 | Inspector strict 오류 0·경고 0, 22개 스키마 모두 Draft 2020-12 `check_schema` 통과 |
| F-14 stdio 규칙 | 충족 유지 | 모든 시험에서 stdout 전 줄이 유효 JSON, 로그는 stderr |

## 7. 프로토콜 반증 (실제 프로세스)

debug 바이너리(동일 소스)와 release 바이너리로 JSON-RPC 줄을 직접 보냈다. 반증을 시도했으나 결함은 나오지 않았다. 관찰 사항은 §10에 있다.

| 시험 | 결과 |
|---|---|
| id: 큰 정수, 음의 큰 정수, 문자열, 0 | 원문 그대로 반환 |
| id: 객체, 배열, null, 소수, 지수 표기, bool | `id:null`, -32600 |
| 5개 리비전 협상 | 2024-11-05, 2025-03-26, 2025-06-18, 2025-11-25는 같은 버전으로 응답. `2026-07-28`(initialize), `1999-01-01`은 2025-11-25로 응답. 각 초기화 뒤 `tools/list` 22개, `ping` 성공 |
| 2026-07-28 무상태 | `server/discover`(initialize 없음) → `tools/list` → `tools/call`(`command_describe`, `document_open`) 모두 성공 |
| 현대식 미지원 버전 | `tools/list`, `server/discover` 모두 -32022, `data.requested`·`data.supported` |
| 현대식 `_meta` 누락·clientCapabilities 누락 | -32602 |
| `initialize`에 현대식 `_meta` 혼입 | -32601(현대식에서 initialize 없음) |
| initialize 이전 | `tools/list`·`tools/call` -32600, `ping` 성공, 이후 `initialize`→`initialized`→`tools/list` 성공, 두 번째 `initialize`는 -32600 `already initialized` |
| 알림 | `notifications/initialized`·`cancelled`·알 수 없는 알림은 무응답이고 연결 유지 |
| 알 수 없는 메서드 | -32601. id 없는 `foo/bar`는 무응답. 알림 이름을 id가 있는 요청으로 보내면 -32601 |
| 잘못된 JSON, 닫히지 않은 객체, 문자열·숫자·배열 최상위, jsonrpc `"1.0"`, method 없음, params 배열·null, 잘못된 UTF-8 | 각각 -32700 또는 -32600/-32602이고 이후 요청 정상 |
| 크기 | 정확히 1,048,576바이트 줄은 처리, 1,048,636바이트와 9 MiB 줄은 -32600 `id:null` 후 다음 줄 정상 |
| EOF | 빈 입력·공백 줄만·개행 없는 마지막 줄·잘린 JSON 모두 종료 코드 0. 잘린 JSON은 -32700 응답 |
| 파이프라인 | `initialize` 뒤 `ping` 300건 연속 전송: 응답 301개, id 순서 보존 |
| 배치 | 2025-03-26으로 초기화한 뒤에만 수신(알림은 개별 응답 없음, 내부 `initialize`·빈·중첩 배치·현대식 요청 혼입은 거절). 2025-06-18, 2025-11-25와 초기화 전은 -32600 `batches are not supported` |
| 숫자 인자 | `expectedVersion` 18446744073709551615는 `VERSION_CONFLICT`, 18446744073709551616·1.0·-1은 `INVALID_REQUEST` |
| 오류 분류 | `initialize {}` -32602, 없는 도구 -32602, `arguments:null`이나 배열 -32602, `document_open` 인자 누락 `isError` INVALID_REQUEST, `/etc/passwd` PATH_NOT_ALLOWED |

## 8. 스키마, annotations, 실제 클라이언트

### 스키마
- 22개 도구 전부에 대해 대표 호출 후 `structuredContent`를 **별도 jsonschema(4.26.0) Draft202012Validator**로 해당 `outputSchema`에 검증했다. 성공 호출과 오류 호출(UNKNOWN_COMMAND, UNSUPPORTED, TARGET_EXISTS, RENDERER_UNAVAILABLE, SESSION_REQUIRED)을 포함해 legacy 세션과 현대식 세션에서 각각 26건을 호출했다. **스키마 위반 0건**, `content[0].text`의 JSON과 `structuredContent` 동일, 입력 인자도 `inputSchema` 위반 0건(현대식은 `documentId` 필수)이다. 입력·출력 스키마 모두 `check_schema` 통과.
- 한계: `image_replace`는 이미지 개체 없는 합성 문서에서 `UNSUPPORTED` 오류 경로만 확인했다(성공 응답은 `evaluate_mcp`의 146건이 다룬다). `document_preview`는 `RENDERER_UNAVAILABLE` 경로다.

### annotations 동작 대조
- readOnly=true 다섯 도구(`command_list`, `command_describe`, `document_inspect`, `element_query`, `element_get`): 호출 전후 revision 동일, 출력 폴더에 파일 변화 없음, 같은 인자로 두 번 호출한 결과 동일. 맞다. `dryRun` 편집도 revision 불변이다.
- 입력 파일은 모든 시험 후 SHA-256이 동일했다.
- 나머지 도구는 `readOnlyHint:false`(보수적), `document_open`·`document_close`·편집·이력은 `destructiveHint:true`다. 보수적 쪽이므로 문제없다. `document_open`은 문서가 열려 있어도 성공하고 새 `documentId`를 준다.
- **불일치 1건(M4-D1, 낮음)**: `project_save`와 `document_export`는 `destructiveHint:false`다. `document_export`는 기존 파일에 `TARGET_EXISTS`로 거절하므로 맞다. 그러나 `project_save`는 같은 경로에 두 번 성공하고, 편집 후 경로 없이 다시 저장하면 기존 `.hwpwork` 내용이 바뀌었다(크기 변화로 확인). 기존 파일을 덮어쓰는 도구에 비파괴 힌트가 붙은 셈이다.

### 실제 클라이언트 (고친 release 바이너리, 응답 변조 없음)

| 클라이언트 | 결과 |
|---|---|
| Claude Code 2.1.296 / `claude-haiku-5-5` (`-p`, 임시 `--mcp-config`, `--strict-mcp-config`, `--allowedTools "mcp__hae__*"`, `--no-session-persistence`) | init 이벤트에서 `hae` connected, `mcp__hae__*` 도구 **22**. `document_open → element_query → text_replace → document_export` 완료, 최종 `tools=22 revision=1 export=ok`, 종료 0, 비용 약 $0.006 |
| Codex CLI 0.163.0 / `gpt-6-luna`, `low` (`-c mcp_servers.hae.…`, `--ignore-user-config --ephemeral`) | 도구 **22**, `document_open`·`document_inspect(limit 5)` 호출 2회 모두 completed, 최종 `tools=22 opened=true` |

Codex를 처음 실행했을 때는 stdin을 닫지 않아 400초 제한에서 멈췄고(출력 없음), `< /dev/null`을 붙이자 정상이었다. 서버와 무관한 CLI 사용 조건이다.

산출물 무결성:
- Claude Code export `cc-m4.hwpx`: `ZipFile.testzip()` 결과 손상 없음, 항목 5개, 수정 텍스트 `가나` 존재·`셀A` 없음. 원본 입력 SHA-256은 실행 전후 동일.
- 비교용으로 M3 산출물도 다시 계산했다: 입력 fixture `0b34ae66…ac1f60`(저장소 원본과 동일), Codex export `c694eb32…e17eb1`, OpenCode export `1218e1e8…b316b318`, Sol의 Claude Code 재실행 export `9dd4b442…86e2e55`(ZIP 손상 없음, 항목 46). 모두 기록과 일치한다.

## 9. 0.2 수용 수정 회귀 — D3 `edits.requires.controls`

광고와 기본 인자 성공이 일치하는지 표본으로 확인했다. 공개 corpus와 합성 editing fixture 27개 문서를 열고, 문서·명령·광고 유형별로 최대 25개의 `cell.setText`·`text.replace` 후보에 `dryRun`을 실행했다.

| 유형 | 표본 | 결과 |
|---|---|---|
| `cell.setText`, requires 없음 | 110 | 기본 인자 성공 **110** |
| `cell.setText`, `requires.controls:"preserve"` | 6 | 기본 인자 거절 **6**, `controls:"preserve"` 성공 **6** |
| `text.replace`, requires 없음 | 191 | 기본 인자 성공 **191** |
| `text.replace`, `requires.controls:"preserve"` | 69 | 문단 전체 범위의 기본 인자 거절 **69**, preserve 성공 **69** |

불일치 **0건**(총 376건).

주의: 첫 시도에서 다문단 셀 15개가 불일치로 나왔다. 원인은 시험이 `text`를 썼기 때문이다. 이 엔진은 다문단 셀에 `paragraphs`를 요구한다(`UNSUPPORTED: use paragraphs for a multi-paragraph cell`). 자식 문단 수만큼 `paragraphs`를 주도록 시험을 고치자 15개 모두 일치했다. 다만 `edits`의 광고가 이 요건을 알려 주지는 않는다(M4-I3).

## 10. 결함 및 관찰 목록

심각도: 높음(수용 차단), 중간, 낮음(수정 권고), 정보(조치 불필요하거나 선택).

| ID | 심각도 | 내용 | 재현 | 위치 |
|---|---|---|---|---|
| M4-D1 | 낮음 | `project_save`가 기존 `.hwpwork`를 덮어쓰는데 `destructiveHint:false`. 힌트일 뿐이고 기본값 쪽이 안전하지만 사실과 다르다. 고치려면 `project_save`를 destructive로 올리거나, 덮어쓰기 없는 동작을 문서화한다 | 문서 열기 → `project_save {path:P}` 두 번(둘 다 성공) → 편집 → `project_save {}`로 P의 크기 변화 확인 | `crates/hae-exec/src/registry.rs`의 annotations 생성 매크로(`destructive_hint: $mutates \|\| History \|\| Lifecycle`) |
| M4-I1 | 정보 | `-32022` 오류의 `data.supported`는 `["2026-07-28"]`만 담고, `server/discover`의 `supportedVersions`는 5개다. 명세 예시의 이중 시대 서버는 legacy 버전도 나열한다. 현대식 요청에서 legacy는 고를 수 없으므로 현재 동작도 일관되지만, 클라이언트 구현에 따라 혼동할 수 있다 | `tools/list` 요청의 `_meta` 버전을 `1900-01-01`로 보냄 | `crates/hae-mcp/src/lib.rs`의 `dispatch` |
| M4-I2 | 정보 | 실행 중 요청의 선점 취소가 없다(`notifications/cancelled`는 이미 완료된 요청으로 간주해 무시). 명세는 SHOULD다. Sol의 보류 사유(commit·publish 안전성)는 타당하다 | — | M1·M2 기록 |
| M4-I3 | 정보 | 다문단 셀의 `cell.setText` 광고가 `paragraphs` 필요성을 알리지 않는다. 오류 메시지는 이를 안내하므로 에이전트가 복구할 수 있다 | `research-school-plan-reference.hwpx`의 다문단 셀에 `text` 전달 | `crates/hae-model/src/query.rs`의 edits 광고 |
| M4-I4 | 정보 | legacy 리비전에서 `notifications/initialized` 전의 `tools/*`를 -32600으로 거절한다. 명세는 클라이언트가 보내야 한다고 정하므로 문제는 없으나 엄격하다 | initialize 직후 `tools/list` | `crates/hae-mcp/src/lib.rs` |

높음·중간 결함은 발견하지 못했다.

## 11. 초안(DeepSeek)에서 정정·삭제한 내용

| 초안 주장 | 처리 |
|---|---|
| §2 M3-F1은 엔진 결함이 아니며 Claude Code 환경 제약 | **오류, 삭제하고 §3으로 정정.** 원인은 `tools/list`의 `ttlMs`·`cacheScope` 누락(엔진 결함, 높음) |
| "format 가설 기각", "최소 서버도 노출되지 않음", `enableAllProjectMcpServers`·`--allowedTools` 무효 | 최소 서버 실험 자체는 이번에 재실행하지 않았다(**미재현**). 결과는 §3 증거 3으로 설명되고, 결론("환경 때문")은 틀렸다 |
| "Claude Code가 보내는 tools/list 응답이 정상(22개)이므로 서버 동작에서 원인을 찾을 수 없다" | **오류.** 목록은 22개였으나 필수 캐시 필드가 빠져 있었다 |
| §3 M3-F2 `uint64` 경고 2회 재현 | 당시 소스(f013a23)에서의 관찰이다. 363e50f에서는 0건이므로 현재 사실이 아니다. §5로 대체 |
| §4-1 doctor 6항목 중 rhwp 실패 `ok:false` | 방향은 맞다. 이번에 5/6과 6/6 두 경우를 다시 측정해 §2에 넣었다 |
| §5.1 무상태 경로, §5.3 견고성 표 | 이번에 다시 실행했다. 대부분 같은 결과이며 §7로 대체. 초안의 "지원하지 않는 버전은 최신 지원 버전 2025-11-25로 응답"은 같은 값으로 재확인 |
| §5.2 "내 요청 형식 실수" 표 | 초안 작성자 개인 시행착오이며 재검증할 서버 동작이 아니라서 삭제 |
| §5.4 `instructions`의 documentId 서술이 부정확 | f013a23 시점의 사실이었고 363e50f에서 고쳐졌다. §5로 대체 |
| §1 M3 해시 대조 | 이번에 다시 계산해 일치(§8). 유지 |
| §4 게이트 재실행 (빈칸) | §2로 채움 |
| "엔진 코드 변경은 하지 않았다" | 이 문서의 검증 범위에서는 맞다(HEAD 363e50f 그대로) |

## 12. 한계

- 렌더러(`rhwp`)가 없어 `document_preview` 성공 경로, 실제 렌더링, 한컴 조판은 검증하지 않았다. doctor 6/6은 경로 점검이다.
- OpenCode는 이번에 다시 실행하지 않았다(M3 기록의 세션만 있고 수정 후 재실행 근거는 없다). Claude Code와 Codex는 사용량을 아끼기 위해 각각 대표 시나리오 1회만 실행했다. Windows 확인은 최종 단계 원칙을 유지한다.
- D3 표본은 문서·명령·유형별 25건 상한이다. 전수가 아니다.
- panic 방어는 단위 시험과 코드 읽기까지만 확인했다.
- 게이트는 Linux 한 환경(rustc 1.93.1 툴체인)에서 한 번 돌렸다.
