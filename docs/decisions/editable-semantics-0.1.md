# 0.1 editable 의미와 실제 명령 가능성의 정합성

작성: 2026-10-18 (한국 시간). 1차 기준: `b6a1e39`; 2차 수정은 아래에 기록, 브랜치 `wip/stage1-model-edit`.
사용자 시험 문서는 로컬에서만 읽었다. 문서·본문·추출 로그를 저장소나 외부 모델에 보내지 않았다.
`spikes/work/hancom-sdk-*/`는 열지 않았다.

## 확정한 정의와 변경 전후

종전에는 문단이면 권한·잠금 검사 뒤 `editable: true`였고, 셀은 병합·문단 수 같은
일부 구조 조건만 보고했다. 호출자가 이를 텍스트 교체 가능으로 읽으면 객체뿐인 문단,
제어 문자를 포함하는 범위, 셀 전체 교체에서 실패했다.

이제 노드 `editable`은 `edits` 중 하나라도 `available: true`일 때만 true다.
문단에는 `text.replace`와 `paragraph.format`, 셀에는 `cell.setText`, 그림에는
`image.replace`를 보고한다. 나머지는 빈 edits와 editable=false를 보고한다.
기존 `notEditableReason`은 노드의 모든 명령이 불가능할 때 유지한다.
문서 전체 권한을 나타내는 open/inspect 최상위 `editable`은 종전 의미를 유지한다.

문단 `textSpans`는 객체·제어 문자·해석 불가 요소나 hp:t 속성을 덮지 않는 최대 구간이다.
기존 range와 같은 extended grapheme cluster의 `[start,end)` 오프셋을 사용한다.
일반 text·run·문자 참조 사이도 안전하면 합친다. CDATA 내용과 리터럴 탭·LF·CR은
제외하고, hp:t 내부나 run 직하의 주석·PI, 빈 CDATA 경계에서는 구간을 나눈다.
run 직하의 비공백 텍스트도 경계다. 경계나 제외 코드포인트에 걸친 grapheme은
통째로 제외한다. 따라서 주석 경계 앞뒤 구간의 end/start가 같을 수 있지만 합쳐서는 안 된다.
빈 구간은 제공하지 않는다. 빈 문단은
`empty_paragraph_insert_unsupported`, 객체뿐이거나 안전한 텍스트가 없으면
`no_text_span`을 보고한다. 권한·노드 잠금은 명령별 사유보다 우선하고 구간을 비운다.

셀의 `childParagraphsEditable`은 자손 중 적어도 한 문단에 text.replace 경로가 있다는
뜻이며, 셀 자신의 editable을 true로 바꾸지 않는다. 셀 전체 교체의 기존 제한과 실제
본문 경로(자손 잠금, 제어 문자, 빈 셀 run/t)를 조회에 반영했다.

available은 현재 노드에서 유효한 인자로 사용할 수 있는 편집 경로의 존재를 뜻한다.
모든 인자 조합·이미지 입력·버전·자원 예산까지 성공한다고 보장하지는 않는다.
paragraph.format은 header·paraPr 참조와 정렬 또는 줄간격을 변경할 수 있는 모양인지 확인한다.

## 실측 근거와 설계 제안에 대한 판단

합성 문단 `￼￼￼`에서 paragraph.format(center)이 성공하고 객체 텍스트와 ID가 보존됨을
먼저 시험했다. 새 필드를 확인하는 시험은 구현 전 누락 필드 때문에 실패했고 구현 후 통과했다.
따라서 객체뿐인 문단에 editable=false를 강제하지 않았다. 실제 가능한 서식 편집과 불가능한
text.replace를 edits로 구분하는 총괄 제안을 채택했다.

`crates/hae-exec/tests/editable_documents.rs`는 로컬 입력 디렉터리가 있을 때만 실행되며,
다른 환경에서는 `HAE_EDITABLE_INPUT_DIR`로 경로를 지정할 수 있다. 실행 예:

```sh
cargo test -p hae-exec --test editable_documents -- --nocapture
```

2026-10-17~18 구현 검증에서 사용자 문서 5종 전체에 대해 다음을 확인했다.

- 174개 노드, 문단 139개 모두 inspect/query/get 개요가 일치했다.
- 제공한 안전 구간 68개 모두 실제 SessionExecutor text.replace dryRun이 성공했다.
- 목록 밖 grapheme 또는 그 경계를 가로지르는 범위와 빈 삽입 127건은 거절됐다.
- 문단 서식 dryRun 139건이 성공했다. 이 표본에서 cell.setText 가능 셀은 0개였다.
- 모든 dryRun 뒤 revision=0과 원본 바이트 동일성을 확인했다.
- 기본 inspect 페이지의 최대 result 크기는 36,297 bytes였다.

합성 시험은 객체뿐인 문단, 객체/제어 문자 사이 텍스트, 빈 문단, 병합 셀, 다문단 셀,
수식, 문서 읽기 전용·잠금, 해석 불가 hp:t 속성, CDATA·문자 참조·결합 문자·ZWJ,
전체 비어 있는 셀의 기존 삽입 경로를 포함한다. 작은 합성 문단의 모든 비어 있지 않은
grapheme 부분 범위를 열거한 1차 시험은 hae_edit::apply만 실행했다. 독립 저장 관문을
검증하지 못한 한계가 있었으며, 아래의 2차 executor 시험으로 보완했다.

## 출력 예와 재시도

객체만 있는 문단에서 노드 ID 등 나머지 필드를 생략한 예:

```json
{
  "kind": "paragraph",
  "textPreview": "￼￼￼",
  "editable": true,
  "edits": {
    "text.replace": {"available": false, "reason": "no_text_span"},
    "paragraph.format": {"available": true}
  },
  "textSpans": [],
  "textSpansTruncated": false
}
```

RANGE_CROSSES_CONTROL의 기존 `Diagnostic.detail`에 textSpans와
textSpansTruncated를 넣는다. batch의 실패 단계 첨부가 기존 detail을 덮어쓰던 경로를
수정해 failedStep과 함께 보존한다. batch 오류 구간은 실패 단계 직전 후보 상태이므로
롤백 뒤 현재 문서에서 다시 조회해야 한다.

## 응답 크기와 호환성

문단당 처음 64개 구간만 제공하고 더 있으면 textSpansTruncated=true를 보고한다.
구간 65개 이상인 문단 100개를 가진 합성 문서의 기본 inspect 100항목 응답 전체를
직렬화해 4 MiB 미만과 페이지 커서 유지를 시험한다. 잘린 목록의 바깥을 편집 불가로
추론해서는 안 된다. 전체 query와 전체 text를 주는 get에는 종전 응답 한도가 계속 적용된다.

필드 추가는 기존 클라이언트가 알 수 없는 필드를 허용할 때 호환된다. 객체뿐인 문단은
서식 편집이 가능하면 여전히 editable=true다. 서식도 없고 안전한 텍스트도 없는 문단,
전체 본문 교체가 불가능한 셀은 더 정확하게 editable=false가 될 수 있다.
text.replace를 계획하는 클라이언트는 editable 대신 edits["text.replace"]와 textSpans를
확인해야 한다. inspect/query/get 레지스트리 출력 스키마도 GenericResultShape에서 실제
개요·결과 필드로 교체했다. CLI/JSON-lines/MCP는 같은 실행 계층과 개요를 사용한다.

객체를 가로지르는 교체, 빈 문단 text.replace 삽입, 다문단 셀 setText를 추가하지 않았다.
Linux의 계약·보존 검증이며 네이티브 Windows 한컴 재열기·조판 호환 판정과는 별개다.

## 1차 게이트

2026-10-18 한국 시간, 1차 소스 기준:

- `bash scripts/check.sh`: 종료 0. fmt·clippy(-D warnings), 보조 Python 33개, Rust 255개 통과.
  로컬 사용자 문서 5종 속성 시험과 네 인터페이스의 edits/textSpans 동등성 시험을 포함한다.
- `python3 -m unittest discover -s tests/oracle`: 88개 통과, 종료 0.
- `python3 scripts/evaluate_mcp.py --output-dir spikes/work/editable-semantics-0.1/mcp-20261018`:
  실제 stdio MCP 94/94 통과, 종료 0. `summary.json`의 nativeComparison은 not_checked다.

사용자 문서는 저장소에 추가하지 않았다. push와 main 반영은 수행하지 않는다.

## 2차 독립 검증 반영 (2026-10-18)

해결 방식은 **광고 축소 및 구간 분할**이다. hae-verify의 규칙과 의존성은 변경하지 않았다.
hae-model이 저장 관문의 CDATA·주석·PI·리터럴 제어 문자 제한을 별도 계산한다.
실제 편집 planner가 더 넓은 범위를 처리하더라도 저장 검증을 통과하지 못하면 광고하지 않는다.
cell.setText는 CDATA(빈 CDATA 포함), 전체 교체가 경계를 가로지르는 주석·PI,
리터럴 제어 문자를 `verify_unsupported_markup`로 비활성화한다.
전체 editing fixture 시험에서 추가로 발견한 중첩 셀은 `nested_cell`로 비활성화했다.
자손 문단의 안전 범위는 독립적으로 조회할 수 있다. 새 편집 능력은 추가하지 않았다.

결정적 선행 조건은 구현했다. header의 paraPr/charPr/style/borderFill 정의 ID 중복과
section의 미해결 참조는 문서 조립·rebuild 때 한 번 계산하여 캐시하고,
`verify_invalid_references`로 노드의 편집 명령을 비활성화한다. 노드마다 문서 전체를
다시 검색하지 않는다. 최상위 section 자식이 hp:p가 아닌 중첩 대상은 XML 조상 경로로
판정하여 `verify_no_top_level_paragraph`로 비활성화한다. textSpans도 비운다.
문서 권한·잠금 사유는 여전히 우선한다. open/inspect 최상위 editable은 권한 정보다.

`anchor_char_height.hwpx`의 텍스트 있는 문단 19개는 미해결 styleIDRef 때문에 광고하지 않는다.
이 문서의 paragraph.format도 실제 변경(줄간격 163%)은 검증에서 실패함을 확인했다.
기존 값과 같은 요청이 no-op으로 성공하는 것은 저장 관문을 실행하지 않는 executor의 기존
동작이며, 본 결정에서는 **실제 변경 경로**가 있는지로 available을 판단한다.
`irb-sample.hwpx`는 최상위 문단 없는 중첩 문단 18개 중 텍스트 있는 9개가 해당한다.
해당 문단의 서식과 셀 전체 교체도 같은 선행 조건을 적용한다.

교차 시험은 `crates/hae-exec/tests/editable_verified.rs`에서 실행한다.
네 재현(CDATA fixture, 주석 교차, 리터럴 탭, CDATA 셀)과 PI/run 직하 경계·빈 CDATA·
리터럴 LF/CR·결합 문자·ZWJ·문자 참조·객체를 포함한다.
editing fixture 전체 17종 및 합성 24사례에서 **광고된 각 구간의 하위 범위**를
SessionExecutor dryRun(편집 + 독립 검증)으로 실행한다. 길이 12 이하 구간은 모든
비어 있지 않은 하위 범위, 긴 구간은 양 끝·인접 위치·중앙을 조합하며 교체값은 X/빈 문자열/ZZ가다.
추가로 광고된 셀 전체 교체와 네 자원 참조 종류·중복 ID·rebuild 후 판정 갱신을 시험한다.
기존 hae-edit 합성 시험은 planner 성공을 필요 조건으로만 검증하도록 정정했다.

MCP tools/list의 outputSchema는 이번 0.1 수정에 추가하지 않는다. 현재 command.describe의
outputSchema는 result 스키마이고, 실제 MCP 응답은 ResponseEnvelope를 text content의 JSON 문자열로
담는다. tools/list에 result 스키마를 단순 복사하면 다른 객체를 기술하므로, envelope와
오류 응답을 포함한 별도 계약·호환성 시험이 필요하다. 이번 구간 판정 수정과 독립된
범위로 보류하며 결과 스키마는 command.describe에서 계속 제공한다.


### 2차 최종 게이트

- `bash scripts/check.sh`: 종료 0. fmt·clippy(-D warnings), 보조 Python 33개,
  Rust **261개** 통과. 1차 255개에서 executor 회귀 시험 6개가 추가됐다.
- `python3 -m unittest discover -s tests/oracle`: **88개** 통과, 종료 0.
- `python3 scripts/evaluate_mcp.py --output-dir spikes/work/editable-semantics-0.1/round2-mcp-20261018`:
  실제 stdio MCP **94/94**, 종료 0. nativeComparison=not_checked.
- `cargo test -p hae-exec --test editable_documents -- --nocapture`: 사용자 문서 **5종** 통과.
  174노드·139문단·68구간·거절 127건·서식 dryRun 139건·셀 dryRun 0건,
  기본 페이지 최대 36,297 bytes로 수치 변화 없다. revision=0·원본 바이트 보존 확인.
- editing fixture 17종·합성 24사례의 광고 하위 범위 시험: text dryRun **1,410회** 성공.

검증기 소스·의존성은 변경하지 않았고 사용자 문서는 저장소에 추가하지 않았다.
push·main 반영은 수행하지 않는다.
