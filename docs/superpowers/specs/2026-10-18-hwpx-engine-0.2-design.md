# HWPX 엔진 0.2 설계 (1차안)

작성: 2026-10-18, 총괄(Opus 5.5). 기준: 0.1 설계서(2026-10-16), 0.1 편집 가능 정의(docs/decisions/editable-semantics-0.1.md, 커밋 2874c19), 단계 4 사용자 문서 실측(거절 25건). 0.2 후보 재검토(docs/decisions/0.2-candidates-review.md, 진행 중)에서 범위가 추가되면 §7에 2단계로 덧붙인다.

## 1. 목표

0.1이 확보한 "기존 구조를 바꾸는 능력"을 실제 문서에서 막히지 않는 넓이로 넓힌다. 새 문단·표 행·개체를 만드는 구조 생성은 하지 않는다. 0.1의 성질은 그대로 유지한다. 원본 바이트 보존, 무편집 내보내기 바이트 동일, Undo/Redo 정확 복원, 독립 검증기(hae-verify)와 oracle의 독립 판정이 그 성질이다.

## 2. 위치 삽입 (text.replace 확장)

- 길이 0 범위 [p, p)를 받는다. 이 경우 새 문자열은 비어 있으면 안 된다(INVALID_RANGE).
- p는 textSpans 중 한 구간의 내부 또는 경계여야 한다. 빈 문단이면 p = 0이다. 그 밖의 위치는 0.1과 같은 규칙으로 거절한다.
- 글자 모양은 다음 순서로 정한다. p 바로 뒤 글자가 있으면 그 run, 없으면 p 바로 앞 글자의 run, 빈 문단이면 문단의 첫 run이다.
- 빈 문단에서 대상 run에 hp:t가 없으면, 그 run의 마지막 자식 뒤에 `<hp:t>…</hp:t>`를 새로 만든다. 이것이 0.2에서 허용하는 유일한 요소 생성이다. run이 없는 문단은 UNSUPPORTED(reason: `no_run`)다.
- 구역 첫 문단처럼 run 안에 secPr·ctrl이 있어도 그 뒤에 hp:t를 둔다. 기존 자식의 순서와 바이트는 바꾸지 않는다.
- edits["text.replace"]에 `modes`(`replace`, `insert`)를 둔다. 빈 문단은 `insert`만 가능하다. 삽입 가능 위치는 textSpans로 표현한다. 빈 문단은 {start:0, end:0} 하나를 갖는다.

## 3. 개체를 보존하는 범위 교체 (text.replace의 controls 인자)

- 새 인자는 `controls: "reject" | "preserve"`이고 기본값은 `reject`다. 기본값에서는 0.1 동작이 그대로다.
- `preserve`이면 범위 안의 개체·탭·줄바꿈·필드 경계를 원래 순서대로 남기고, 범위 안의 텍스트만 바꾼다.
  - 새 문자열 전체는 범위 안 첫 텍스트 조각에 들어가고 그 run의 글자 모양을 쓴다.
  - 범위 안 나머지 텍스트 조각은 비운다. 빈 hp:t와 run은 0.1 규칙대로 지우지 않는다.
  - 첫 텍스트 조각보다 앞에 있던 개체는 앞에 남는다. 나머지 보존된 개체는 새 텍스트 뒤에 원래 순서로 놓인다. 비어 있는 기존 hp:t만 있으면 범위 안 첫 빈 hp:t를 재사용하며 새 요소를 만들지 않는다. 텍스트 조각과 빈 hp:t가 모두 없으면 UNSUPPORTED_CONTENT_IN_RANGE(reason: `no_text_fragment`)다. 응답 진단 `CONTROLS_PRESERVED`에 개체 수와 새 grapheme 오프셋을 담는다.
- 기본 reject와 삽입은 textSpans 규칙을 따른다. preserve는 문단의 grapheme 경계에서 개체 바로 앞·뒤를 끝점으로 삼을 수 있다. 개체를 포함한 grapheme의 내부를 끝점으로 삼을 수는 없다. textSpans는 reject 교체·삽입 구간이며 preserve 전체 범위를 나열하지 않는다. preserve만 가능한 문단은 textSpans가 비어 있어도 modes:["replace"]와 available:true를 보고한다.
- 범위 안에 검증기가 지원하지 않는 표기(CDATA, 주석, PI, 리터럴 제어문자)나 해석하지 못한 속성이 있으면 0.1처럼 거절한다.
- 수식·그림 등 개체 자체의 내용은 바꾸지 않는다. 수식 노드를 대상으로 지목하면 계속 거절한다.

## 4. cell.setText 확장

- 병합 셀은 일반 셀과 같게 다룬다. HWPX에는 실제로 존재하는 기준 셀만 있으므로 그 셀의 문단을 바꾼다. 가려진 칸은 노드가 아니다.
- 다문단 셀에는 새 인자 `paragraphs: string[]`를 쓴다. 길이는 셀의 문단 수와 정확히 같아야 한다(다르면 INVALID_ARGUMENT와 실제 문단 수). 각 문단을 전체 교체하고, 빈 문단은 §2 삽입으로 채운다. 문단 수를 바꾸는 일은 0.3에서 한다.
- `text`와 `paragraphs`는 동시에 줄 수 없다. 다문단 셀에 `text`를 주면 0.1처럼 거절하되, details에 `paragraphs` 사용 안내를 담는다.
- `controls` 인자는 §3과 같은 뜻으로 받는다.

## 5. 공통 규칙

- 줄 배치 캐시는 0.1 §11 규칙을 그대로 쓴다. 셀 편집은 표를 담은 최상위 문단부터 구역 끝까지 지운다. PrvText 규칙도 같다.
- 한 명령은 한 Undo 단위다. batch·dryRun·expectedVersion·replay 규칙도 0.1과 같다.
- 독립성:
  - hae-verify는 새 동작의 기대 결과를 hae-edit에 의존하지 않고 따로 계산한다.
  - tests/oracle(Python)에도 삽입·보존 교체·다문단 셀의 독립 구현과 변이 시험을 더한다.
- editable 계약:
  - edits, modes, textSpans, 이유 코드는 새 능력을 정확히 반영한다.
  - 광고한 모든 구간·모드는 executor 경로(편집 + 검증)에서 성공해야 한다(0.1 2차 라운드 원칙).
- 새 진단 코드는 docs/decisions/diagnostic-codes.md에 추가한다(CONTROLS_PRESERVED 등).
- 레지스트리 스키마 버전과 엔진 버전을 0.2.0으로 올린다. MCP 도구 설명과 docs/user-guide.md를 갱신한다. 사용자 문구는 Gemini 4 Argon이 다듬을 수 있다.

## 6. 완료 조건

1. `bash scripts/check.sh` 종료 0, oracle 전체 통과, `scripts/evaluate_mcp.py` 전체 통과. MCP 평가에는 삽입·보존 교체·다문단/병합 셀 시나리오를 추가한다.
2. 사용자 문서 5종(spikes/work/stage4-e2e/inputs, 로컬 전용)에서 다음 속성 시험이 모두 성공한다. 단계 4 거절 25건 중 수식 노드를 지목한 1건을 뺀 24건의 재시도도 성공해야 한다.
   - 개체가 있는 모든 문단: `preserve` 전체 교체
   - 모든 빈 문단: 삽입
   - 모든 다문단·병합 셀: setText
   - 각 편집 뒤 Undo로 원본 바이트 복원
3. Linux 한컴 관찰 환경(docs/decisions/engine-output-open-verification-2026-10-17.md)에서 대표 산출물(삽입, 보존 교체, 다문단 셀, 병합 셀)을 열어 화면으로 확인한다. Windows 확인은 사용자 지시대로 최종본에서 한 번에 한다.
4. 독립 검증: Sol 구현은 Sonnet 또는 Luna가 반례 시도와 게이트 재실행으로 확인한다.

## 7. 2단계 (재검토 결과 반영 자리)

0.2-candidates-review.md(Luna 재검토)를 반영한 총괄 결정이다(2026-10-18).

### 7-1. 1단계 보완 (0.2에 포함)
- **필드 교차 거절**: `controls:"preserve"`여도 범위 안에 필드 시작·끝(하이퍼링크 포함) 표식이 하나라도 있으면 거절한다(reason: `field_boundary_in_range`). 그러지 않으면 필드 결과 텍스트의 의미가 바뀔 수 있다. 수식·필드 payload는 어떤 모드에서도 바꾸지 않는다.
- **개체 전용 문단 삽입 위치**: 실제 표본에 16문단이 있다. 개체와 개체 사이, 첫 개체 앞, 마지막 개체 뒤의 grapheme 경계 p에 길이 0 삽입을 허용한다. 단, 그 위치에 인접한 개체와 **같은 run** 안에 hp:t를 만들 수 있을 때만이다. 새 run은 만들지 않는다. 이런 위치는 textSpans에 길이 0 구간으로 광고하고, 광고한 위치는 executor 경로에서 성공해야 한다.
- **24/25 표기**: 단계 4 원 요청 24건을 새 인자로 정확히 재시도하고 Undo 원본 복원까지 독립 검증한 수만 결과로 쓴다. 이론상 상한으로 쓰지 않는다.

### 7-2. 2단계: text.replaceAll (0.2에 포함)
- 하나 이상의 문단(기본은 문서 전체, `scope` nodeId로 제한 가능)에서 정확한 코드포인트 일치를 모두 찾아 바꾼다. 일치 구간이 textSpans 안에 온전히 들어간 경우만 바꾸고, 개체·필드를 가로지르거나 자소 경계가 맞지 않는 일치는 건너뛰어 진단에 개수와 위치를 남긴다.
- 겹치는 일치는 앞에서부터 겹치지 않게 고른다. `maxMatches`(기본 1,000)를 넘으면 commit 전에 거절한다.
- 전체가 한 Undo 단위이고 원자적이다. 일부 실패 시 아무것도 바꾸지 않는다. dryRun은 일치 목록을 돌려준다.
- 캐시 규칙은 batch와 같아 구역마다 가장 앞 기준 문단부터 지운다. 셀 안 일치도 포함한다.
- hae-verify와 oracle에 독립 구현을 둔다.
- 구현 확정(2026-10-18, 2단계 구현): 명령 `text.replaceAll`(인자 `find`, `text`, 선택 `scope`·`maxMatches`)은 `command.batch` 단계로도 쓴다. `scope`가 문단이면 그 문단과 자손 문단을, 표·셀·구역이면 자손 문단을 대상으로 한다. 일치는 문단마다 앞에서부터 겹치지 않게 고른 뒤 분류하므로 건너뛴 일치도 겹치는 뒤쪽 일치를 막는다. 건너뜀 사유는 `crosses_control`, `grapheme_boundary`, `unsupported_markup`, `paragraph_not_editable`이며 MATCHES_SKIPPED(경고)로 사유별 개수와 위치(최대 50)를 알린다. `maxMatches`는 바꿀 수 있는 일치만 세며 1~10,000이고 넘으면 RESOURCE_LIMIT로 거절한다. `find`와 `text`가 같으면 noop이다. 응답의 `matches` 미리보기는 앞 200곳으로 제한한다(개수는 정확). 자세한 이유는 docs/decisions/v0.2-implementation-notes.md.

### 7-3. 0.3 이후로 미룸
글자 서식(run 분할·charPr 복제), 쪽 설정, 스타일 적용, 표 셀 서식, 머리말·꼬리말·각주·필드·번호 모델, 새 문단·행·개체 생성, HWP5, GUI. 이들은 run·자원 경계나 넓은 재배치를 다뤄 0.2의 "기존 구조 안 텍스트 편집" 원칙 밖이다.
