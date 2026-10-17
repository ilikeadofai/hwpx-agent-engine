# 0.1 editable 의미와 실제 명령 가능성의 정합성

작성: 2026-10-18 (한국 시간). 구현 기준: `dbcefb8`, 브랜치 `wip/stage1-model-edit`.
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
run·문자 참조·CDATA 사이도 안전하면 합치며, 불가한 코드포인트에 걸친 grapheme은
통째로 제외한다. 빈 구간은 제공하지 않는다. 빈 문단은
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
- 목록 밖 grapheme 또는 그 경계를 가로지르는 범위와 빈 삽입 172건은 거절됐다.
- 문단 서식 dryRun 139건이 성공했다. 이 표본에서 cell.setText 가능 셀은 0개였다.
- 모든 dryRun 뒤 revision=0과 원본 바이트 동일성을 확인했다.
- 기본 inspect 페이지의 최대 result 크기는 36,297 bytes였다.

합성 시험은 객체뿐인 문단, 객체/제어 문자 사이 텍스트, 빈 문단, 병합 셀, 다문단 셀,
수식, 문서 읽기 전용·잠금, 해석 불가 hp:t 속성, CDATA·문자 참조·결합 문자·ZWJ,
전체 비어 있는 셀의 기존 삽입 경로를 포함한다. 작은 합성 문단의 모든 비어 있지 않은
grapheme 부분 범위를 열거해 실제 편집 성공과 textSpans 포함 관계를 대조한다.

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

## 최종 게이트

2026-10-18 한국 시간, 최종 소스 기준:

- `bash scripts/check.sh`: 종료 0. fmt·clippy(-D warnings), 보조 Python 33개, Rust 255개 통과.
  로컬 사용자 문서 5종 속성 시험과 네 인터페이스의 edits/textSpans 동등성 시험을 포함한다.
- `python3 -m unittest discover -s tests/oracle`: 88개 통과, 종료 0.
- `python3 scripts/evaluate_mcp.py --output-dir spikes/work/editable-semantics-0.1/mcp-20261018`:
  실제 stdio MCP 94/94 통과, 종료 0. `summary.json`의 nativeComparison은 not_checked다.

사용자 문서는 저장소에 추가하지 않았다. push와 main 반영은 수행하지 않는다.
