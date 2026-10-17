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
| M1 | 프로토콜·견고성 구현(1·2) | GPT 6.1 Sol(한도 시 Sonnet 5.5) | 대기 |
| M2 | 배포·doctor·버전(4), 안내서 문구 | Sol 또는 Sonnet / 문구 Gemini 4 Argon | 대기 |
| M3 | 실제 클라이언트 연결 시험(3) | Haiku 5.5 또는 Luna(max) | 대기 |
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
