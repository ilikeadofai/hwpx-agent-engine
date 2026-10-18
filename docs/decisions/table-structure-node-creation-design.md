# 표 구조 편집 — 노드 생성 계약 설계 (0.3, 2026-10-18)

상태: 구현과 함께 확정(브랜치 `feat/structure`). 규범 언어는 한국어다.
관련 문서: 0.1 설계서 §5·§8·§11, [저장 검증 규칙](hwpx-verification-rules.md),
[진단 코드](diagnostic-codes.md), 경쟁 분석 §6.3-1·G2(`docs/research/competitive-gap-analysis.md`).

0.1~0.3 초반의 모든 안전 성질은 "본문 노드를 만들지도 지우지도 않는다"에 기댔다.
그래서 행·열 추가/삭제는 `structure_change_unsupported`로 거절했다. 이 문서는
그 가정을 **표 하나의 행·열 단위로만** 깨뜨리고, 깨뜨린 만큼 ID 대응표·Undo·독립 검증을
함께 설계한다. 설계가 감당하는 범위와 감당하지 못해 거절로 남기는 범위를 §10에 분리해 적었다.

## 1. 결론

| 명령 | 효과 | 만들거나 지우는 노드 |
|---|---|---|
| `table.insertRow` | `index` 위치에 빈 행 `count`개 | 새 행의 셀(`Cell`)과 셀마다 문단 1개(`Paragraph`) |
| `table.deleteRow` | `index`부터 행 `count`개 삭제 | 삭제 행의 셀과 그 문단 |
| `table.insertColumn` | `index` 위치에 빈 열 `count`개 | 새 열의 셀과 셀마다 문단 1개 |
| `table.deleteColumn` | `index`부터 열 `count`개 삭제 | 삭제 열의 셀과 그 문단 |

- 만들 수 있는 노드 종류는 **기존 표 안의 `Cell`과 `Paragraph`뿐**이다. 표·구역·그림·수식·불투명
  요소는 만들거나 지우지 않는다.
- 병합 셀이 삭제 구간에 걸리면 **명확히 거절**한다. 삽입은 새 줄이 병합 셀을 가르면 거절한다.
- 삭제할 셀에 글자와 단순 줄 부호 외의 내용(그림·수식·필드·도형·각주)이 있으면 거절한다.
  이 범위의 상호 참조 의미를 한컴에서 검증하지 않았기 때문이다(§10).
- 한컴 열기 게이트는 삽입·삭제 산출물 각각에 대해 통과해야 완료다. 기록은
  [표 구조 편집 한컴 게이트](table-structure-hancom-gate-2026-10-18.md)에 둔다.

## 2. 기존 불변식과 새 불변식

### 2.1 깨지는 곳

1. **ID 대응이 위치 기반이다.** `Document::rebuild`는 구역 항목마다 식별 요소(구역·문단·표·셀·그림·
   수식·불투명)의 **문서 순서 열**을 이전 열과 위치로 짝짓는다. 열의 종류 순서가 다르면
   `identity elements changed`로 거절한다. 중간에 셀이 끼면 그 뒤 모든 노드의 ID가 어긋난다.
2. **안정 경로가 형제 순번이다.** `stable_path`는 `run`/`t`를 투명하게 보고 조상별 자식 순번을
   센다. 행을 하나 넣으면 아래 모든 `tr`의 순번이 바뀌어 `node … moved`로 거절한다.
3. **`document.diff`가 위치로 짝짓는다.** `base.nodes.iter().zip(target.nodes)`.
4. **검증기는 원본 경로를 쓴다.** 요청의 대상 주소가 원본(배치 시작 전) 경로이고, 앞 단계가
   행을 넣으면 같은 배치의 뒷 단계 대상 경로가 밀린다.
5. **캐시·미리보기 정책이 문단 수를 가정한다.** 배치 캐시(`linesegarray`) 기준 문단,
   `PrvText` 치환, 썸네일.

### 2.2 다시 쓴 불변식

0.1 §5의 문장 "0.1은 본문 노드를 추가·삭제하지 않으므로"를 다음 네 항으로 대체한다.

- **I1 (선언된 변경만).** 노드 집합은 일반 편집에서 변하지 않는다. 변할 수 있는 것은
  *표 구조 명령*이 `TableRemap`(표 하나, 삭제·이동 행/셀 순번 사상)으로 **미리 선언한** 변화뿐이다.
  `Document::rebuild`는 계속 노드 집합 변화를 거절하고, 선언을 받는 `rebuild_table_structure`만
  변화를 허용한다.
- **I2 (선언 대조).** 후보 문서의 모든 기존 노드는 선언된 사상으로 계산한 안정 경로의 요소와
  **정확히 하나씩** 대응한다. 대응하지 못한 새 요소는 선언된 표의 자손인 `Cell`/`Paragraph`여야
  하고, 사상이 `None`인 기존 노드도 같은 조건을 만족해야 한다. 어긋나면 `STATE_INVARIANT_ERROR`
  이며 commit하지 않는다.
- **I3 (ID 비재사용).** 새 요소는 항상 새 `nodeId`를 받고, 삭제된 노드의 ID는 현재 문서의 어떤
  요소에도 다시 쓰지 않는다. Undo/Redo가 스냅샷으로 이전 상태의 같은 요소와 ID를 되살리는
  것은 재사용으로 보지 않는다(0.1 §5 그대로).
- **I4 (표 바깥 불변).** 선언된 표의 부분 트리 밖 노드의 ID·안정 경로·원문 바이트는 바뀌지 않는다.
  구역·문단·표 노드를 만들거나 지우는 명령은 여전히 없다.

## 3. nodeId 대응표 갱신

### 3.1 선언: `TableRemap`

```text
TableRemap {
  entry:    구역 항목 경로,
  table:    변경 전 `hp:tbl`의 안정 경로 (표 자신의 경로는 변하지 않는다),
  children: 변경 전 tbl의 구조 자식 순번 -> 변경 후 순번 | 삭제,
  cells:    변경 전 tr 순번 -> (변경 전 tc 순번 -> 변경 후 순번 | 삭제),
}
```

`map(경로)`는 `table`의 접두 밖이면 그대로, 표 자신이면 그대로, 자손이면 첫 단계에
`children`을, 둘째 단계(`tr` 안의 `tc`)에 `cells[첫 단계]`를 적용하고 나머지 접미는 그대로 둔다.
`None`이면 그 요소와 모든 자손이 삭제된다. 행 삽입은 `children`이 아래 `tr`을 `+count` 밀고,
열 삽입은 해당 행의 `cells`가 오른쪽 `tc`를 밀며, 삭제는 `None`을 둔다.

### 3.2 `rebuild_table_structure` 알고리즘

1. 후보 패키지를 `rebuild`와 같은 방식으로 분석한다(구역 목록 불변).
2. 선언된 구역이 아닌 구역은 기존처럼 식별 요소 열이 위치·종류까지 같아야 한다.
3. 선언된 구역: 기존 노드마다 `stable_path(옛 문서)`를 `map`에 넣어 기대 경로를 얻는다.
   기대 경로 → (종류, id) 표를 만든다(경로 충돌은 불변식 위반).
4. 후보 문서의 식별 요소를 문서 순서로 순회하며 `stable_path(새 문서)`로 표를 조회한다.
   - 있고 종류가 같다 → **기존 ID를 이어받는다**(소비 처리).
   - 없다 → 선언된 표 아래의 `Cell`/`Paragraph`이면 `new_id`로 **새 ID**, 아니면 불변식 위반.
5. 표에 소비되지 않고 남은 항목(삭제 선언이 아닌데 사라진 노드)이 있으면 불변식 위반.
6. 삭제 선언된 노드도 선언된 표 아래의 `Cell`/`Paragraph`여야 한다.
7. 결과 `Document`와 함께 `{created, removed}` ID 목록을 돌려준다.

편집기가 선언을 틀리게 만들면 후보가 선언과 어긋나 commit이 거절되므로, 선언과 실제 XML 변경이
**서로를 검증**한다. 선언이 요청을 만족하는지는 독립 검증기가 본다(§5).

### 3.3 키와 영속화

`MapKey`(항목 경로·`XmlDoc::path` 구조 경로·종류)는 `map_rows()`가 **현재 문서에서 매번
계산**하므로 형식을 바꾸지 않는다. 구조 변경 뒤 `project.save`는 새 경로로 대응표를 쓰고,
`open_with_map`은 모든 식별 요소가 정확히 한 행에 대응하는지 이미 검증한다. 새 요소의 ID는 저장
뒤에도 유지된다.

### 3.4 `document.diff`

위치 짝짓기를 **nodeId 짝짓기**로 바꾼다. 양쪽에 있으면 기존처럼 텍스트·미디어를 비교하고,
대상에만 있으면 `createdNodes`, 기준에만 있으면 `removedNodes`로 보고한다. 구조 변경이 없는
문서에서는 결과가 달라지지 않는다.

## 4. Undo / Redo

이력은 commit 전후의 `CommittedState`(문서 상태 전체: 패키지 항목, 노드 목록과 ID, 색인)를
스냅샷으로 보관하고, Undo/Redo는 스냅샷을 바꿔 끼운다(0.1 §8). 이 구조가 이미 "노드가 있던/
없던" 상태를 되돌린다.

- **Undo**: 삭제 전 스냅샷을 되살리므로 삭제됐던 셀·문단이 **원래 ID로** 돌아온다. 삽입의 Undo는
  새 요소가 사라진 이전 스냅샷을 되살린다. 패키지 항목은 같은 `Arc` 페이로드이므로 export는
  원본 바이트와 같다.
- **Redo**: 명령을 다시 실행하지 않고 저장된 이후 스냅샷을 복원하므로 새 요소는 **같은 새 ID**를
  유지한다(재생성하지 않는다).
- **새 변경**은 Redo 가지를 지운다. 새로 만든 ID는 폐기된 가지와 별개이며 I3가 보장한다.
- **dryRun**: 후보를 버리므로 후보의 새 ID는 어디에도 남지 않는다. 응답의 `structure`는
  dryRun에서 **새 ID를 비워서** 돌려준다(쓸 수 없는 ID를 알려 주지 않기 위해).
- 구현 변경점은 없다. 시험으로 고정한다(삽입/삭제 각각 Undo 뒤 export 바이트 동일·삭제 셀 ID
  복원, Redo 뒤 ID·바이트 동일).

## 5. 독립 검증 (`hae-verify`)

### 5.1 계약

검증기는 편집기의 결과·ID·`TableRemap`을 받지 않는다. 입력은 **요청 원문**
(`table.insertRow`, 표 주소, `index`, `count` 등)과 **앞 단계까지의 기대 패키지**뿐이다.
`hae-edit`·`hae-model`에 의존하지 않는 새 모듈 `table_structure.rs`가 다음을 스스로 도출한다.

1. 대상 표의 격자를 다시 구성하고 요청의 적법성(범위·병합·크기·삭제 내용)을 판정한다.
   적법하지 않은 요청이면 후보와 상관없이 거절한다.
2. 기대 바이트를 **원본 XML에 대한 splice 목록**으로 계산한다: 삽입할 `tr`/`tc` 바이트, 밀리는
   `rowAddr`/`colAddr` 값, `rowCnt`/`colCnt`, `sz`, 삭제할 `tr`/`tc` 범위.
3. 기대 패키지를 만든 뒤 마지막에 후보와 **전 항목 바이트 단위**로 대조한다(기존 방식 그대로).
   그래서 새 셀 속성·테두리가 템플릿과 같은지, 새 셀이 비어 있는지, 주소·크기가 어긋나지
   않았는지, 요청 밖 표나 문단이 바뀌지 않았는지가 한 번에 걸러진다.

### 5.2 배치에서 대상 경로가 밀리는 문제

검증기는 요청의 대상 주소를 **원본 안정 경로**로 해석한다. 앞 단계가 행·열을 바꾸면 같은 표
안의 뒷 단계 대상 경로가 밀린다. 검증기는 각 구조 단계가 스스로 도출한 `PathRemap`을 쌓고,
대상을 찾을 때 **원본 경로 → 누적 사상 → 현재 문서 경로**로 변환한다. 사상이 `None`
(앞 단계가 지운 요소)이면 거절한다. 편집기는 ID로 대상을 찾으므로 같은 의미가 된다. 새로 만든
요소는 배치 시작 때 ID가 없으므로 같은 배치에서 대상이 될 수 없다(0.1 §8 그대로).

### 5.3 캐시 앵커

구조 변경 뒤에는 표 부분 트리의 문단 경로가 바뀌므로 앞 단계가 기록한 그 표 아래의 중첩 앵커를
버리고(아래 4번 규칙으로 전부 다시 앵커된다), 구조 변경 **뒤** 문서에서 표 안의 모든 문단을
앵커한다.

### 5.4 변조 거절 시험

`crates/hae-verify/tests/table_structure.rs`가 편집기 없이 후보를 손으로 만들어 수용을 먼저
확인하고, 최소 다음 변조를 거절하는지 본다: `rowCnt` 미갱신, 새 셀 `rowAddr` 오류, 표
`height` 미갱신, 밀려야 할 셀 주소 미갱신, 새 셀에 글자가 남음, 테두리 서식 불일치, 엉뚱한 행
삭제, 병합 셀 삭제 요청, 행 수 초과 삽입, 다른 표 변경, 캐시 미삭제/과삭제, 삭제 대신 비움.

## 6. 편집 규칙 (편집기와 검증기가 같이 따른다)

### 6.1 요청과 색인

- `index`는 **새 줄이 차지할 0부터의 주소**다. 삽입은 `0..=rowCnt`(또는 `colCnt`), 삭제는
  `0..rowCnt`. `table.insertRow {index: 3}`은 "3행(0부터) 앞에 넣음 = 3번째 행 뒤"다.
  "3행 뒤"를 사람이 1부터 세어 말하면 `index = 3`이다.
- `count`(기본 1)는 `1..=1024`. 결과 행·열 수는 `1..=1024`(기존 치수 명령과 같은 한도)여야 한다.
  모두 지우는 삭제(`would_empty_table`)와 한도 초과(`table_size_limit`)는 거절한다.
- 대상은 표 노드여야 한다(`wrong_kind`). 문서·표·표 자손이 잠겨 있으면 기존 `check_editable`
  규칙으로 거절한다.

### 6.2 전제 조건 (기존 치수 명령과 같다)

`cellSpacing=0`, `sz`의 `widthRelTo`/`heightRelTo`가 `ABSOLUTE`, 중첩 표 없음, 모든 셀이
`cellAddr`/`cellSpan`/`cellSz`를 가지며 격자가 완전하고 겹치지 않음, 셀의 `rowAddr`가 속한 `tr`
순번과 같고 같은 `tr` 안에서 `colAddr`가 증가한다. 추가로 `hp:cellzoneList`가 있으면 거절한다
(셀 영역 주소를 갱신하는 규칙이 없다). 열 명령은 열 폭이 표 총폭과 모순 없이 결정되어야 한다
(기존 `axis` 제약 전파). 행 명령은 행 높이를 "그 행 셀 `cellSz.height`의 최댓값"으로 삼으며
높이 합이 `sz.height`와 같아야 한다고 요구하지 않는다(한컴 저장 파일은 높이 0인 자동 행과
`sz.height`가 어긋나는 일이 흔하다). 표 높이는 **차분**으로 갱신한다.

### 6.3 삽입

**템플릿 줄.** 새 줄은 이웃 줄의 셀 속성·테두리를 복제한다.

- 행 삽입: 후보 `[index-1, index]`(범위 안인 것) 중 *단순한* 줄. 단순 = 그 줄을 덮는 모든 셀이
  `rowSpan = 1`. 그런 줄 중 모든 셀이 `header="0"`인 줄을 먼저 고르고, 없으면 첫 단순 줄을 쓴다
  (제목 행 뒤에 넣으면 본문 행을 복제한다).
- 열 삽입: 후보 `[index-1, index]` 중 그 열을 덮는 모든 셀이 `colSpan = 1`인 첫 열.
- 경계 `index`가 병합 셀을 가르면(`addr < index < addr+span`) `insert_inside_merged_cell`로
  거절한다. 단순한 후보가 없으면 `no_simple_template_line`으로 거절한다.

**새 셀 바이트.** 템플릿 셀 `tc`의 원문 바이트를 복제하고 다음만 바꾼다.

1. `tc@name` → 빈 문자열(셀 이름 중복 방지), 행 삽입이면 `tc@header` → `0`.
2. `cellAddr`의 `rowAddr`/`colAddr`를 새 주소로.
3. `subList` 안쪽 전체를 **빈 문단 하나**로 교체한다:
   `raw(첫 hp:p의 시작 태그)` + `<hp:run charPrIDRef="…"/>` + `</hp:p>`. `charPrIDRef`는 첫 문단의
   첫 `hp:run`에서 읽는다. 문단 속성(`paraPrIDRef`, `styleIDRef`)은 첫 문단 것을 그대로 쓴다.
   `linesegarray`와 텍스트는 만들지 않는다.

`cellSpan`, `cellSz`, `cellMargin`, `borderFillIDRef`, `hasMargin`, `protect`, `editable`,
`dirty`와 `subList` 속성은 그대로다. 행 삽입의 새 `tr`은 템플릿 `tr`의 시작·끝 태그 원문을
쓴다. 템플릿 셀에 첫 문단·첫 run·`charPrIDRef`가 없으면 `cell_template_unsupported`.

**주소·크기 갱신.**

| 값 | 행 삽입 (index=i, count=k) | 열 삽입 (index=c, count=k) |
|---|---|---|
| 새 줄 | 템플릿 `tr`을 k번 복제, 주소 `rowAddr = i..i+k` | 템플릿 열을 덮는 각 셀을 k번 복제, `colAddr = c..c+k` |
| 위치 | `i>0`이면 `tr[i-1]` 바로 뒤, 아니면 `tr[0]` 앞 | 각 행의 `colAddr ≥ c`인 첫 `tc` 앞(없으면 행 끝) |
| 밀림 | `rowAddr ≥ i`인 셀의 `rowAddr += k` | `colAddr ≥ c`인 셀의 `colAddr += k` |
| `tbl@rowCnt`/`colCnt` | `rowCnt += k` | `colCnt += k` |
| `tbl/sz` | `height += k × (템플릿 행 높이)` | `width += k × (템플릿 열 폭)` |

열을 넣으면 **표 총폭이 새 열 폭만큼 늘어난다**(기존 열은 줄이지 않는다). 쪽 폭을 넘기는지는
검사하지 않는다. 필요하면 이어서 `table.setColumnWidths`로 조정한다.

### 6.4 삭제

- **병합 거절.** 삭제 구간과 겹치는 셀 중 `rowSpan ≠ 1` 또는 `colSpan ≠ 1`인 셀이 하나라도
  있으면 `merged_cell_blocks_delete`로 거절한다(구간을 가르는 병합뿐 아니라 구간 안에 갇힌
  병합도 같다 — 요청 지시에 따른 보수적 규칙이며 `detail.cells`에 셀 nodeId를 담는다).
- **내용 거절.** 지울 셀의 부분 트리가 허용 목록(`tc`, `subList`, `p`, `run`, `t`,
  `linesegarray`, `lineseg`, `cellAddr`, `cellSpan`, `cellSz`, `cellMargin`, `tab`, `lineBreak`,
  `nbSpace`, `fwSpace`, `hyphen`) 밖의 요소를 가지면 `delete_content_unsupported`.
- 구간 밖 줄이 비는 경우(`row_would_become_empty`: 열 삭제 뒤 셀 없는 `tr`)는 거절한다.
- 갱신: 삭제 `tr`/`tc` 원문 범위를 지우고, 구간 뒤 셀의 주소를 `-count`, `rowCnt`/`colCnt`
  감소, `sz`에서 삭제한 행 높이 합(또는 열 폭 합)을 뺀다. 결과 `sz`는 양수여야 한다.

## 7. rebuild · 안정 경로 · linesegarray · PrvText

- **rebuild.** 구조 명령은 한 번의 `splice` 뒤 `rebuild_table_structure`를 호출한다. 이후 단계의
  일반 `rebuild`는 새 노드 집합을 기준 상태로 삼는다.
- **안정 경로.** `rebuild_table_structure`가 선언 사상으로 경로를 변환해 대조한다. 일반
  `rebuild`의 "노드 이동 금지"는 그대로다.
- **linesegarray.** 구조 변경은 표 크기·쪽 나눔을 바꾼다. 기존 표 치수 명령과 같이 표를 담은
  최상위 문단(`carrier`)부터 구역 끝까지의 최상위 문단 캐시와, 표 안의 **모든 문단** 캐시를
  제거한다. 새 문단은 캐시 없이 만들어진다. 한 배치에서 앞 단계가 남긴 같은 표 아래 앵커는
  구조 변경 뒤 경로·ID가 무효이므로 버리고 변경 뒤 문서에서 다시 앵커한다(편집기는 ID, 검증기는
  경로). 삭제된 문단의 앵커도 같은 이유로 버린다.
- **PrvText.** 문단 단위 치환 규칙을 쓰지 않는다. 삭제한 셀에 글이 있었으면 `PrvText`를
  **고치지 않고 낡음(`stale.prvText`)으로 표시**한다(여러 줄을 정확히 지울 방법이 없다). 삽입은
  글이 없으므로 `PrvText`를 건드리지 않는다. 썸네일은 항상 낡음 표시.
- **변경 보고.** `changedIds`는 표 + 새 셀/문단 + 주소가 밀린 셀. 응답에 `structure[]`
  (`{command, table, index, count, created[{nodeId, kind, row, col}], removed[nodeId], shifted}`).

## 8. 트랜잭션 · dryRun · batch · 버전

`handle_edit_steps` 경로를 그대로 쓴다: `expectedVersion` 검사 → 단계별 편집 계획 → 편집기 적용 →
**검증기 통과** → 메모리 한도 → commit 또는 dryRun 응답. 구조 명령은 batch 단계가 될 수 있고
(`command.batch`), 같은 표에 대한 구조 명령을 여러 번 이을 수 있다(두 구현이 순서대로 같은
상태를 만든다). 실패 시 batch 전체가 롤백된다. 내용이 바뀌므로 revision +1, 단일 Undo 단위다.
`table.addRow` 같은 이전 이름은 계속 `structure_change_unsupported`로 거절하되 안내 문구를
`table.insertRow`로 바꾼다.

## 9. 진단

새 `Code`는 없다.

| 코드 | reason | 조건 |
|---|---|---|
| `INVALID_ARGUMENT` | `index_out_of_range` | 삽입 `index > 줄 수`, 삭제 `index ≥ 줄 수` 또는 `index+count > 줄 수` |
| `INVALID_ARGUMENT` | `count_out_of_range` | `count = 0` 또는 `> 1024` |
| `INVALID_ARGUMENT` | `table_size_limit` | 결과 행·열 수 > 1024 |
| `INVALID_ARGUMENT` | `would_empty_table` | 모든 행(열)을 지우려 함 |
| `UNSUPPORTED` | `merged_cell_blocks_delete` | 삭제 구간과 겹치는 병합 셀 |
| `UNSUPPORTED` | `insert_inside_merged_cell` | 삽입 경계가 병합 셀을 가름 |
| `UNSUPPORTED` | `no_simple_template_line` | 복제할 단순한 이웃 줄이 없음 |
| `UNSUPPORTED` | `cell_template_unsupported` | 템플릿 셀에 첫 문단/run/`charPrIDRef`/`subList` 없음 |
| `UNSUPPORTED` | `delete_content_unsupported` | 삭제할 셀에 허용 목록 밖 요소 |
| `UNSUPPORTED` | `row_would_become_empty` | 열 삭제 뒤 셀이 없는 행 |
| `UNSUPPORTED` | `table_cellzone_unsupported` | `hp:cellzoneList` 있음 |
| `UNSUPPORTED` | `table_geometry_unsupported`/`nested_table` | 기존 치수 명령과 같은 전제 위반 |
| `STATE_INVARIANT_ERROR` | — | 선언과 후보가 어긋남(편집기 결함 방어선) |
| `VERIFY_FAILED` | 기존 | 독립 검증 불통과(§5) |

## 10. 범위 밖 (여전히 불가)

- 셀 병합·분할, 병합 셀을 가르는 삽입, 병합 셀 삭제(거절이 설계된 결과다).
- 표·문단 자체의 생성/삭제, 표 복제, 중첩 표 구조 변경, 그림·수식·필드·도형이 든 셀의 삭제.
- `cellzoneList`가 있는 표, `cellSpacing ≠ 0`, 상대 크기(`widthRelTo ≠ ABSOLUTE`) 표.
- 열 삽입 때 총폭을 유지하는 자동 재배분, 표가 쪽 폭을 넘는지 검사.
- 행·열 이동·정렬, 머리글 행 자동 인식(복제 템플릿 선택에만 `header`를 참고).
- 변경 추적 문서·서명 문서(기존대로 읽기 전용).
- Windows 한컴오피스 2024 실기 검증: Linux 한컴 2020 Beta 생존 판정과 화면 관찰만 했다.

## 11. 위험과 미해결

- **새 셀 바이트의 한컴 수용.** 새 셀은 템플릿 원문 복제 + 빈 문단 하나이며 한컴이 저장하는 빈 셀
  모양(`<hp:run charPrIDRef=…/>`, 캐시 없음)과 같다. 열기 가능성만 Linux 게이트로 확인했다.
- **행 높이 0(자동) 표.** 차분 규칙이라 동작하지만 표 `sz.height`가 한컴이 다시 계산하는 값과 다를 수
  있다. 열기·재조판에서 한컴이 바로잡는지는 관찰 항목이다.
- **배치 안 뒤 단계 대상.** 같은 표의 기존 셀을 가리키는 뒤 단계는 사상으로 지원하지만 복잡한
  조합(여러 구조 단계 + 다른 편집)은 시험한 조합만 보장한다.
- **독립성의 한계.** 검증기와 편집기는 같은 "규칙 표"(§6)를 각자 구현한다. 규칙 자체의 오류는
  양쪽이 같이 가질 수 있으며 한컴 게이트와 화면 관찰이 최종 방어선이다.
