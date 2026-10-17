# MCP 서버(hae-mcp) 완성 계획과 진행 기록

작성: 2026-10-18, 총괄(Opus 5.5). 사용자 목표: "mcp서버를 완성시켜라." 기준: 현재 hae-mcp(stdio JSON-RPC, initialize·ping·tools/list·tools/call, 도구 22개, evaluate_mcp 146/146), 0.2 설계서.

## 완성의 정의

1. **프로토콜 준수**
   - MCP 최신 안정판과 2025-06-18, 2024-11-05 버전 협상을 지원한다.
   - `notifications/initialized`·`notifications/cancelled`를 처리하고, 알 수 없는 알림은 무시하며 알 수 없는 메서드는 -32601로 응답한다.
   - 도구마다 `outputSchema`와 `structuredContent`를 둔다. 도구 실행 오류는 `isError:true`로, 프로토콜 오류는 JSON-RPC error로 구분한다.
   - 도구 annotations(readOnlyHint·destructiveHint·idempotentHint)를 붙인다.
2. **견고성**
   - 잘못된 JSON·초과 크기·EOF·panic에서 프로세스가 죽지 않고 정해진 오류를 낸다.
   - stdout에는 프로토콜만, stderr에는 로그만 쓴다.
   - 요청별 처리 시간 상한은 0.1에서 범위 밖이었다. 여기서 다시 판단하고 결정을 기록한다.
3. **실제 클라이언트 연결**
   - MCP Inspector CLI, Claude Code, Codex CLI 중 설치된 것으로 실제 연결해 대표 시나리오를 실행한다: 열기 → inspect → 편집 → preview → export.
   - 각 클라이언트의 설정 예시를 안내서에 둔다.
4. **배포**
   - `hae-mcp --version/--help`를 제공한다.
   - release 빌드와 설치 절차, 의존 실행 파일(rhwp·hae-raster·bwrap) 확인 명령(`hae-mcp doctor` 또는 같은 기능)을 둔다.
   - serverInfo 버전은 엔진 버전(0.2.0)과 일치시킨다.
5. **선택(감사 결과로 판단)**: 열린 문서·프리뷰를 resources로 노출, prompts. 근거가 약하면 하지 않고 이유만 기록한다.
6. **게이트**
   - check.sh, oracle, evaluate_mcp 전부 통과.
   - 새 MCP 적합성 시험(실제 stdio 프로세스)과 실제 클라이언트 실행 기록을 남긴다.

## 단계와 담당

| 단계 | 내용 | 담당 | 상태 |
|---|---|---|---|
| M0 | 현황 감사: MCP 명세 대비 빠진 것 목록, Inspector CLI 실제 실행 결과 | Haiku 5.5 또는 GPT 6 Luna(max) | 완료 — [감사 결과](../../decisions/mcp-audit-m0-2026-10-18.md) |
| M1 | 프로토콜·견고성 구현(1·2) | GPT 6.1 Sol(한도 시 Sonnet 5.5) | 완료 — Rust 310·stdio 회귀 9/9·Inspector strict 0/0 |
| M2 | 배포·doctor·버전(4), 안내서 문구 | Sol 또는 Sonnet / 문구 Gemini 4 Argon | 완료 — release 빌드·doctor 6/6·연결 예시 |
| M3 | 실제 클라이언트 연결 시험(3) | Haiku 5.5 또는 Luna(max) | 완료 — [실행 결과](../../decisions/mcp-m3-client-runs-2026-10-18.md) |
| M4 | 독립 검증(게이트 재실행·반례) | 구현자와 다른 모델 | 대기 |

진행 중 작업과 결과는 이 표의 상태 칸과 아래 기록에 적는다. 오케스트레이터가 바뀌어도 이 문서를 보고 이어 간다.

## 오케스트레이터 전환 규칙 (사용자 지시)

- Claude 5시간 사용률이 **85% 이상**이면 이 스레드의 오케스트레이터를 DeepSeek V4.1 Flash(opencode, variant max)로 바꾼다. 확인은 `python3 scripts/check_provider_usage.py`로 한다.
- DeepSeek가 오케스트레이터일 때 Claude 5시간 창이 초기화되어 사용률이 낮아지면(50% 미만, `resetsAt`이 갱신됨) Opus 5.5(effort high)로 되돌린다.
- 전환에 실패하더라도 15분 간격의 예약 작업이 이 문서를 읽고 작업을 재개한다.
- DeepSeek가 오케스트레이터인 동안 지킬 것:
  - 사용자 문서(spikes/work/stage4-e2e/inputs)와 한컴 SDK 자료(spikes/work/hancom-sdk-*)는 열지 않는다.
  - 구현은 역할표(docs/decisions/model-roles.md)대로 위임한다.
  - main 반영과 범위 확대는 하지 않는다.

## M1 작업 지시(위임용)

감사 결과(docs/decisions/mcp-audit-m0-2026-10-18.md)의 다음 항목을 고친다.
- **F-01·F-02·F-03 id 처리**
  - 큰 정수 id를 원문 그대로 되돌려 준다. serde_json의 arbitrary_precision이나 RawValue를 쓴다.
  - 객체·배열 id는 id:null로 -32600을 돌려준다. null id는 명세 리비전 규칙에 따른다.
- **F-04 버전 협상**: 2026-07-28(최신 안정판), 2025-11-25, 2025-06-18, 2025-03-26, 2024-11-05를 지원한다. 리비전별 차이를 명세에서 확인해 반영하고 근거 URL을 남긴다.
- **F-05 초기화 전 요청**: initialize 전 요청은 ping 외에 거절한다.
- **F-09 오류 분류**: 입력 검증 오류를 isError로 할지 -32602로 할지 최신 리비전 기준으로 통일하고 문서화한다.
- **outputSchema·structuredContent**: 모든 도구에 outputSchema를 두고 결과에 structuredContent를 함께 준다. 기존 텍스트 content는 하위 호환으로 유지한다.
- **annotations**: readOnlyHint·destructiveHint·idempotentHint·openWorldHint를 레지스트리 메타데이터에서 생성한다.
- **Inspector strict 스키마**: 오류 1건(command_batch의 bare `true`)과 경고 17건을 해소한다.
- **panic 방어와 시간 상한**: 요청 처리를 catch_unwind로 감싸 내부 오류로 응답한다. 요청별 시간 상한은 실행기 구조에서 가능한지 판단해 구현하거나, 범위 밖 사유를 기록한다.
- **M2 함께 처리**: `hae-mcp doctor`(rhwp·hae-raster·bwrap·폰트 폴더·입출력 루트 점검)와 docs/user-guide.md에 Claude Code·Codex·opencode·Inspector 연결 예시를 둔다.

시험과 게이트:
- 위 항목마다 실제 stdio 프로세스 시험을 넣는다.
- check.sh, oracle, evaluate_mcp가 모두 통과해야 한다.
- Inspector CLI 검사에서 strict 오류가 0이어야 한다.

## 기록

- 2026-10-18 04:2x: 계획 작성. 0.2 독립 검증(Sonnet)은 진행 중이다. Codex는 한도 상태(04:53경 해제)이고, Claude 5시간 사용률은 9%.

- 2026-10-18, Sol 구현: 작업 A를 `32407a4`로 커밋했다. 총괄의 M0 감사·이 계획 문서를 첫 커밋에 포함했다. A 고정 소스 게이트는 check.sh 종료 0/Rust 298, oracle 98, MCP 146/146, editable_documents 3/3이다. 사용자 문서 속성은 시도 98·성공/Undo 92·필드 경계 거절 6(개체 문단 거절 5·셀 거절 1)으로 정정했고, 단계 4 성공 18 중 셀 9건은 paragraphs 적응 요청으로 분리했다. 첫 문단만 새 text로 바꾸고 나머지 텍스트를 유지하는 재시도도 같은 18/6/제외 1 결과였다. Linux 한컴에서 구역 첫 위치 삽입을 화면 확인했다.
- 2026-10-18, Sol 구현: M1·M2 소스 구현 완료. 최신 리비전은 initialize 목록 추가로 끝내지 않고 현대식 요청 메타데이터·server/discover로 구현했다. 2025-03-26만의 batch 수신, id/알림/응답 예산 회귀까지 실제 stdio 시험 9개가 통과했다. Inspector release strict는 22개 도구 오류 0·경고 0, 별도 JSON Schema 검증은 전체 22개 응답 및 E2E 응답 97개 통과다. 요청 전체 강제 timeout과 선점 취소는 동기 executor의 commit/publish 안전성 때문에 보류하는 판단을 [M1·M2 기록](../../decisions/mcp-m1-m2-2026-10-18.md)에 남겼다. M3 실제 에이전트 세션·M4 독립 검증은 대기이며 Windows 확인은 최종 단계 원칙을 유지한다. push·main 반영·전역 클라이언트 설정 변경은 하지 않았다.
- 중간 재개 지점(아래 최종 기록으로 대체): A 마지막 커밋은 `32407a4`. B 코드는 staged 상태로 고정했고 `/tmp/hae-task-b-frozen-check.log`의 전체 게이트가 진행 중이다. oracle 98/MCP 146/146/Inspector 0·0/doctor 6·6/release는 이미 통과했다. 전체 게이트 종료 코드와 Rust 합계를 확인한 뒤 이 기록과 M1 상태를 완료로 갱신하고 B를 커밋하면 된다. 코드는 최종 검증 중 변경하지 않는다.

- 최종 기록(2026-10-18): 고정 소스 `bash scripts/check.sh` 종료 0, Rust **310개** 통과. oracle **98개**, MCP E2E **146/146**, editable_documents **3/3**, 실제 stdio 적합성 **9/9**, release Inspector strict **오류 0·경고 0**, doctor **6/6**, release build 종료 0이다. M1·M2를 완료로 갱신했다. 이 기록을 포함한 B 커밋 제목은 `feat: complete MCP protocol support and deployment diagnostics`이며 A 커밋은 `32407a4`다. 상세 근거·프로토콜 리비전 URL·panic/timeout/취소 판단은 [M1·M2 구현 기록](../../decisions/mcp-m1-m2-2026-10-18.md)에 있다. 남은 일은 M3 실제 에이전트 연결과 M4 독립 검증이며 담당 총괄이 이어 간다. push·main 반영 없음.
- 2026-10-18 06:14: Claude 5시간 사용률 80%(증가 속도 약 0.5%/분, 다음 점검 전 85% 초과 예상)로 오케스트레이터를 DeepSeek V4.1 Flash(max)로 전환. 진행 중: M3(Luna, codex), M4(Sonnet, Claude 하위 에이전트 — Claude 한도에 걸리면 초기화 뒤 재실행 필요). Claude 5시간 창 초기화 09:20 KST.

- 2026-10-18, M3 실제 클라이언트 연결 완료: Codex CLI 0.163.0(GPT 6 Luna, low)와 OpenCode 2.0.30(opencode-go/deepseek-v4.1-flash)은 각각 22개 도구를 노출하고 편집·Undo·export를 완료했다. Claude Code 2.1.296(Haiku)은 서버가 `connected`로 표시됐지만 도구 0개가 노출되어 시나리오를 진행하지 못했다(M3-F1). Inspector CLI 2.11.0 strict는 종료 0, 도구 22개. Export 두 건은 ZIP 무결성 검사와 새 hae-mcp 프로세스 재열기를 통과했고 입력 fixture SHA-256은 유지됐다. Preview는 rhwp 미설정으로 불가; Inspector는 `uint64` outputSchema 형식 경고를 출력했다(M3-F2). 엔진 코드 변경 없음, push 없음. 세부 명령·사용량·재현은 [M3 실행 결과](../../decisions/mcp-m3-client-runs-2026-10-18.md)에 기록했다. M4 독립 검증은 대기.
