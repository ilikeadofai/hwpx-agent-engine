# HWPX 기능 카탈로그 (0.3 조사 1호)

- 작성일: 2026-10-18 · 대상: `hwpx-agent-engine` (HAE) · 판정 기준 커밋: **`7e008c1`** (브랜치 `wip/stage1-model-edit`)
- 성격: 조사 문서. 코드는 고치지 않았다. 판정은 커밋된 코드 기준이며, 작업 트리의 미커밋 변경은 '작업 중(미커밋)'으로만 표기했다 (U12).
- 방법: (1) OWPML C++ 모델에서 요소·클래스 추출 (2) 코퍼스 10개 파일 직접 집계 (3) HAE 소스에서 요소 이름과 사용처 확인 (4) 공개 경쟁 저장소의 도구·클래스 이름 대조 (5) 우선순위 = 가치 × 정합 ÷ 난이도.
- 근거 표기: 각 행의 `S-*`(공개 외부 자료), `H-*`(HAE 내부 자료와 코드), `U*`(미확인). 출처는 5절에 있다.

## 0. 요약

| 항목 | 수 |
|---|---|
| 기능 카탈로그 행 | 89 (구현 17 · 부분 10 · 보존만 8 · 미구현 47 · 작업 중 3 · 미확인 4) |
| 우선순위 후보 (상위 30개 선정) | 63 중 30 |
| HAE가 다루지 않는 OWPML 요소 (미구현·렌더러만·이름 충돌) | 276 (없음 223 · 렌더러만 35 · 이름 충돌 13 · 모델 미등재 5) |
| 미확인 항목 | 23 |
| 출처 | 27 |

**HAE 현재 수준 정의.** 구현 = 명령이 있고 검증까지 연결됨. 부분 = 읽기 또는 일부 편집만 됨. 보존만 = 바이트 그대로 남고 HAE 명령은 없음. 미구현 = HAE 소스에 해당 기능이 없음. 작업 중 = 커밋되지 않은 변경에만 있음. 미확인 = 근거 부족.

**보존 계약 영향.** 없음 = 읽기 전용. 낮음 = 기존 텍스트·참조 값만 바뀜. 중간 = 새 자원(charPr·paraPr·borderFill·미디어)이나 새 control, 또는 검증 규칙 확장이 필요함. 높음 = 노드를 만들거나 지움("노드 불변" 가정을 깸) 또는 파일 구조를 바꿈.

**우선순위 산식.** 점수 = 사용자 가치 V(1–5) × HAE 강점 정합 A(1–5) ÷ 난이도 D(하 1 · 중 2 · 상 3 · 최상 4). A는 보존·검증·트랜잭션 계약과 맞는 정도다. 구현 완료 항목, 작업 중 항목, 미확인 항목은 순위에서 뺐다.

## 1. 요소·기능 카탈로그

열 설명: 우선순위 칸의 `점수 (순위)`는 상위 30개에 든 항목만 표시한다. `—`는 순위 대상이 아님(구현 완료, 작업 중, 미확인, 점수 산정 제외 중 하나). 보존 계약 영향은 3절 정의를 따른다.

### 텍스트·글자·문단 (12)

| ID | 기능/요소 | OWPML 요소·속성 | 무엇을 하는가 | HAE 현재 수준 | HAE 근거 | 경쟁 구현 지원 (근거) | 난이도 | 보존 계약 영향 | 우선순위 |
|---|---|---|---|---|---|---|---|---|---|
| TX-01 | 글자 모양 편집 (굵게·기울임·밑줄·취소선·크기·글자색) | hh:charPr@height,@textColor; hh:bold·hh:italic·hh:underline·hh:strikeout (자식) | 선택한 글자 범위에 서식 적용. 기존 charPr 재사용 또는 새 charPr 최대 1개 추가 | 구현 | registry.rs:685 (text.format); hae-edit/src/char_format.rs:15 (CharProps: bold, italic, underline, strikeout, height_hwpunit, text_color) | S-PYAUTO format_text; S-HWPMCP apply_hwp_text_style; S-HWPXMCP set_text_style; S-HWPXLIB charpr/CharPr.java | 중 | 중간 | — |
| TX-02 | 글꼴 이름·상대 크기·자간 변경 | hh:fontfaces/hh:fontface/hh:font@face; hh:charPr/hh:fontRef(@hangul,@latin…), hh:relSz, hh:spacing | 글꼴 얼굴 지정, 상대 크기·자간 변경 | 미구현 | hae-edit/src/char_format.rs: charPr 복사 시 fontRef·relSz 등 보존만. 글꼴 지정 명령 없음 (registry.rs HEAD 23개 명령에 없음) | S-HWPXMCP set_text_style(font_name, font_size); S-HWPMCP apply_hwp_text_style | 중 | 중간 | 8.00 (16위) |
| TX-03 | 문단 정렬·백분율 줄간격 변경 | hh:paraPr/hh:align@horizontal; hh:lineSpacing@type=PERCENT@value (switch/case 사본 포함) | 문단 정렬과 백분율 줄간격 변경. 기존 paraPr 재사용 또는 새 paraPr 1개 | 구현 | registry.rs:713 (paragraph.format); registry.rs:195 (ParagraphFormatParams: alignment, line_spacing만); hae-edit/src/format.rs:1-6 (hp:switch 사본 갱신 명시) | S-HWPXMCP set_paragraph_style; S-PYAUTO set_paragraph_format; S-HWPMCP apply_hwp_paragraph_style | 중 | 중간 | — |
| TX-04 | 문단 여백·들여쓰기·문단 위아래 간격 | hh:paraPr/hh:margin/(hc:intent, hc:left, hc:right, hc:prev, hc:next) (@value,@unit) | 문단 좌·우 여백, 들여쓰기, 문단 앞·뒤 간격 변경 | 미구현 | hae-exec/src/registry.rs:195 ParagraphFormatParams에 여백 항목 없음. hh:margin 자식은 HAE 코드에서 참조되지 않음 (페이지 여백의 hp:margin 속성 읽기는 별개) | S-HWPXMCP set_paragraph_style(margin_*, first_line_indent); S-PYAUTO set_paragraph_format | 중 | 중간 | 8.00 (17위) |
| TX-05 | 쪽 나눔·줄 나눔 속성 (keepWithNext, pageBreakBefore, widowOrphan) | hh:paraPr/hh:breakSetting@pageBreakBefore,@keepWithNext,@widowOrphan,@lineWrap | 문단 단위 쪽·줄 나눔 옵션 변경 | 보존만 | HAE 코드에서 breakSetting 미참조. paraPr 복사 시 바이트 보존 | S-PYAUTO add_page_break (쪽 나눔 삽입; breakSetting 편집 여부는 미확인) | 중 | 중간 | 6.00 (25위) |
| TX-06 | 글머리표·문단 번호 적용 | hh:bullet; hh:numbering/hh:paraHead(@start,@level,@numFormat,@align…); hp:p@paraPrIDRef 연결 | 글머리표·번호 정의 조회 및 문단에 적용 | 보존만 | 코퍼스 hh:paraHead 470건 존재, HAE 편집 코드 없음 (hh:numbering 참조 없음) | S-PYAUTO set_list_format; S-HWPXLIB header_xml/references/Bullet.java, Numbering.java; S-SDK ParaNumberBullet, PutBullet, OutlineNumber | 상 | 중간 | 4.00 |
| TX-07 | 스타일 목록 조회·문단 스타일 적용 | hh:styles/hh:style(@id,@name,@engName,@type,@paraPrIDRef,@charPrIDRef,@nextStyleIDRef); hp:p@styleIDRef | 스타일 목록 조회, 기존 스타일을 문단에 연결 (새 스타일 생성 제외) | 미구현 | hae-model/src/lib.rs:625 ("style","styleIDRef") 참조 무결성 검사만. 스타일 명령 없음 | S-HWPXMCP get_styles, apply_style; S-PYAUTO list_styles; S-HANCOMMCP define_style, set_paragraph_style | 중 | 낮음 | 8.00 (18위) |
| TX-08 | 특수 문자·탭·공백 삽입 (탭, 줄바꿈, 전각·불가분 공백) | hp:tab(@width,@leader,@type); hp:lineBreak; hp:fwSpace; hp:nbSpace (hp:hyphen은 C++ 모델 미등재, U4) | 문단 안에 제어 문자 삽입 | 부분 | hae-verify/src/text.rs:25-35 (tab, lineBreak, fwSpace, nbSpace, hyphen을 글자로 매핑). 새로 삽입하는 명령 없음; text.replace가 제어 문자를 넣을 수 있는지는 미확인 | S-HWPXLIB object/content/section_xml/paragraph/t/Tab.java, LineBreak.java, NBSpace.java, FWSpace.java, Hyphen.java | 중 | 중간 | 4.50 (30위) |
| TX-09 | 찾아 바꾸기 (문서 전체·범위 지정, 건너뛴 항목 보고) | hp:t (텍스트 조각) | 정확 일치 문자열을 찾아 바꾸고 건너뛴 일치를 보고 | 구현 | registry.rs:703 (text.replaceAll); hae-edit/src/replace_all.rs | S-PYAUTO batch_replace, search_and_replace; S-HWPMCP replace_hwp_text; S-HANCOMMCP replace_text; S-HWPXMCP replace_text | 하 | 낮음 | — |
| TX-10 | 문단 내 텍스트 치환·삽입 | hp:t, hp:run | 범위 지정 치환, 빈 범위에 삽입 (controls: preserve 지원) | 구현 | registry.rs:693 (text.replace); docs/decisions/hwpx-verification-rules.md §2.2 | S-PYAUTO replace_in_paragraph; S-HWPMCP set_hwp_paragraph_text; S-HWPXMCP update_paragraph_text, append_text_to_paragraph | 하 | 낮음 | — |
| TX-11 | 문단 추가·삭제 | hp:p (생성·제거), hp:run, hp:t, hp:linesegarray (무효화 필요) | 문단 노드 생성 또는 제거 | 미구현 | 노드 불변 가정 (H-GAP §6.3). 노드 생성 명령 없음 (registry.rs HEAD) | S-PYAUTO add_paragraph, insert_paragraph, delete_paragraph; S-HWPMCP append_hwp_paragraph, delete_hwp_paragraph; S-HWPXMCP insert_paragraph, delete_paragraph; S-HANCOMMCP delete_paragraph | 상 | 높음 | 3.33 |
| TX-12 | 문단 복사·이동 | hp:p 및 하위 전체 | 문단을 다른 위치로 복사 또는 이동 | 미구현 | 노드 생성·이동 명령 없음 (H-GAP §6.3) | S-HWPXMCP copy_paragraph, move_paragraph | 상 | 높음 | 2.00 |

### 표 (8)

| ID | 기능/요소 | OWPML 요소·속성 | 무엇을 하는가 | HAE 현재 수준 | HAE 근거 | 경쟁 구현 지원 (근거) | 난이도 | 보존 계약 영향 | 우선순위 |
|---|---|---|---|---|---|---|---|---|---|
| TB-01 | 셀 텍스트 편집 (병합 셀 포함) | hp:tc/hp:subList/hp:p | 기존 셀의 문단 교체 (문단 개수 정확히 일치 요구, controls: preserve) | 구현 | registry.rs:723 (cell.setText); docs/decisions/hwpx-verification-rules.md §2.4 | S-PYAUTO set_table_cell_text; S-HWPMCP set_hwp_cell_text; S-HWPXMCP update_table_cell | 하 | 낮음 | — |
| TB-02 | 표 구조 조회 (행·열 수, 셀 주소·병합 범위·크기) | hp:tbl@rowCnt,@colCnt; hp:tc; hp:cellAddr@colAddr,@rowAddr; hp:cellSpan@colSpan,@rowSpan; hp:cellSz@width,@height | 표 전용 구조 뷰 (행·열, 병합, 셀 주소, 크기) | 부분 | hae-model/src/query.rs:187 (row_span, col_span), query.rs:611 (rowCnt). 셀 단위 노출은 있으나 표 전용 명령(table.get) 없음 | S-PYAUTO get_table_map, get_table_text; S-HWPXMCP get_table, get_table_as_csv; S-HWPMCP read_hwp_tables | 하 | 없음 | 20.00 (1위) |
| TB-03 | 셀 속성 변경 (배경색·테두리·세로 정렬·셀 크기·안 여백) | hp:tc@borderFillIDRef; hh:borderFill(hh:leftBorder 등, hc:fillBrush/hc:winBrush); hp:cellSz; hp:cellMargin | 기존 borderFill 참조로 셀 서식 변경 (새 borderFill 생성 시 자원 추가) | 미구현 | cell.setText만 존재 (registry.rs:723). borderFill은 참조 검증 대상 (hae-model/src/lib.rs) | S-HWPXMCP set_cell_properties; S-PYAUTO format_table; S-SDK CellBorderFill, CellFill, TableVAlignTop | 중 | 중간 | 8.00 (15위) |
| TB-04 | 표 행·열 삽입·삭제 | hp:tr, hp:tc, hp:tbl@rowCnt/@colCnt, hp:cellAddr(주소 재번호), hp:cellSpan, hp:cellSz | 행·열 추가·삭제 (주소·병합·크기 재계산) | 미구현 | 노드 불변 가정 (H-GAP G2, §6.3). 구현 명령 없음 | S-PYAUTO apply_table_ops; S-HWPMCP append_hwp_table_row/column, delete_hwp_table_row/column; S-HWPXMCP insert_table_row/column, delete_table_row/column; S-SDK TableInsertRowColumn, TableDeleteRow | 최상 | 높음 | 2.50 |
| TB-05 | 셀 병합·분할 | hp:cellSpan@colSpan,@rowSpan; hp:tc | 셀 병합 및 분할 | 미구현 | 병합 셀의 텍스트 편집만 허용 (cell.setText). 병합 생성·분할 없음 | S-PYAUTO merge_table_cells, split_table_cell; S-HWPMCP merge_hwp_cells_horizontal, merge_hwp_cells_vertical; S-SDK TableMergeCell, TableSplitCell (hwpx-mcp README에는 병합 도구 없음) | 최상 | 높음 | 2.00 |
| TB-06 | 표 생성 (새 hp:tbl 삽입) | hp:tbl, hp:tr, hp:tc, hp:cellAddr, hp:cellSpan, hp:cellSz, hp:cellMargin, hp:subList, hp:p, hh:borderFill | 새 표를 문서에 삽입하고 셀을 채움 | 미구현 | 구조 생성 불가 (H-GAP §1.2) | S-HWPMCP insert_hwp_table; S-HWPXMCP insert_table, insert_nested_table; S-HANCOMMCP insert_table; S-PYAUTO add_table; S-HWPXLIB object/table/*.java | 최상 | 높음 | 2.50 |
| TB-07 | 라벨 기반 양식 채우기 (라벨 셀 → 옆 셀 값) | hp:tc/hp:subList/hp:p (텍스트 기준으로 찾음) | 라벨 텍스트로 셀을 찾아 값을 넣음 (경로 지정 예: "성명 > right") | 부분 | cell.setText(TB-01)는 구현. 라벨 탐색·경로 지정 명령 없음 (registry.rs HEAD) | S-PYAUTO find_cell_by_label, fill_by_path, analyze_form_fill, apply_form_fill; S-HWPXMCP replace_text_in_cell | 중 | 낮음 | 12.50 (9위) |
| TB-08 | 표 계산식 (합계·평균·곱) | 필드 기반 계산식 (rhwp 명칭 FORMULA; OWPML 명칭 미확인, U10) | 표 셀 값 계산 결과 채우기 | 미구현 | 계산식 처리 없음 | S-PYAUTO table_compute; S-SDK TableFormulaSumHor, TableFormulaAvgVer; S-RHWP src/parser/hwpx/section.rs:5559 (FORMULA) | 상 | 중간 | 3.00 |

### 필드·링크·주석 (20)

| ID | 기능/요소 | OWPML 요소·속성 | 무엇을 하는가 | HAE 현재 수준 | HAE 근거 | 경쟁 구현 지원 (근거) | 난이도 | 보존 계약 영향 | 우선순위 |
|---|---|---|---|---|---|---|---|---|---|
| FD-01 | 필드 목록 조회 (누름틀·기타 필드의 이름·종류·값) | hp:fieldBegin@type,@name,@editable,@id,@fieldid; hp:parameters/hp:stringParam@name="Command" | 문서의 필드 목록과 현재 값 조회 | 미구현 | 필드 경계 판정만 (hae-model/src/para.rs:236 field_in_range; hae-verify/src/text.rs:224). 목록 명령 없음 (HEAD 확인) | S-PYAUTO list_form_fields (handlers/form_fill.py:404); S-HWPMCP list_hwp_fields, get_hwp_field_value; S-RHWP parse_field_type (section.rs:5551-5569) | 중 | 없음 | 12.50 (8위) · 백로그 진행 중 |
| FD-02 | 누름틀 값 채우기 (Command 문자열 갱신) | hp:fieldBegin 내 hp:parameters/hp:stringParam@name="Command" (rhwp 기록 형식: Clickhere:set:N:Direction:wstring:n:텍스트 HelpState:wstring:m:) | 필드 값만 바꾸고 필드 경계와 나머지 구조는 보존 | 미구현 | 필드 경계 안 편집은 거절 (hae-model/src/para.rs:236; 시험 hae-exec/tests/editable_verified.rs:667 v02_fields_are_rejected_even_when_controls_are_preserved, 사유 field_boundary_in_range) | S-PYAUTO fill_form_field (handlers/form_fill.py:443); S-HWPMCP set_hwp_field_value; S-RHWP src/model/control.rs:919-1016 (command 형식, 한컴 원본 예 1147-1152) | 중 | 중간 | 7.50 (19위) · 백로그 진행 중 |
| FD-03 | 누름틀 필드 생성 | hp:fieldBegin/hp:fieldEnd 쌍, hp:ctrl 래퍼, hp:parameters | 새 누름틀 필드를 문단 또는 셀에 삽입 | 미구현 | control 생성 경로 없음 | S-PYAUTO add_form_field (handlers/form_fill.py:410-440, "실한컴 CLICKHERE 형상 그대로 방출"); S-SDK InsertFieldTemplate, InsertFieldCtrl | 상 | 높음 | 2.67 |
| FD-04 | 하이퍼링크 목록 조회 (URL·표시 문자열) | hp:fieldBegin@type="HYPERLINK" + parameters(Command에 URL 추정, U3). hyperlink 요소는 C++ 모델에 없음 | 문서의 하이퍼링크 목록과 대상 조회 | 부분 | HYPERLINK 필드는 경계 보존과 편집 거절만 (시험 hae-exec/tests/editable_verified.rs:669). 목록 조회 없음 | S-HWPXMCP get_hyperlinks; S-PYHWPX add_hyperlink (README 알려진 제약 절); S-SDK Hyperlink, HyperlinkJump | 하 | 없음 | 15.00 (3위) |
| FD-05 | 하이퍼링크 삽입 | hp:fieldBegin/hp:fieldEnd 쌍 (type=HYPERLINK) | 새 하이퍼링크 필드 삽입 | 미구현 | 필드 생성 경로 없음 | S-HWPXMCP insert_hyperlink; S-PYHWPX add_hyperlink; S-SDK InsertHyperlink | 상 | 높음 | 2.00 |
| FD-06 | 책갈피 목록 조회 | hp:bookmark@name (hp:ctrl 내부; C++ 모델 등재) | 책갈피 이름과 위치 문단 조회 | 미구현 | 렌더러 매핑 목록에만 존재 (hae-render-rhwp/src/pages.rs:71). 조회 명령 없음 | S-HWPXMCP get_bookmarks; S-PYHWPX add_bookmark; S-HWPXLIB ctrl/Bookmark.java; S-SDK Bookmark, ModifyBookmark | 하 | 없음 | 15.00 (4위) |
| FD-07 | 책갈피 삽입 | hp:bookmark (hp:ctrl 내부) | 지정 위치에 책갈피 추가 | 미구현 | control 추가 경로 없음 | S-HWPXMCP insert_bookmark; S-PYHWPX add_bookmark | 중 | 중간 | 4.50 (27위) |
| FD-08 | 각주·미주 목록 조회 | hp:footNote, hp:endNote (@number 등; subList/p) | 각주·미주 번호, 본문 문단, 참조 위치 조회 | 미구현 (작업 트리에서 구현 중, 미커밋) | HEAD에는 없음. 작업 트리에서 note.list로 구현 중(미커밋, query.rs·registry.rs 변경) | S-HWPXMCP get_footnotes, get_endnotes; S-HWPXLIB ctrl/FootNote.java, EndNote.java | 하 | 없음 | — (작업 중) |
| FD-09 | 각주·미주 내부 문단 편집 | hp:footNote/hp:endNote 안의 hp:subList/hp:p | 각주 본문 문단의 텍스트 편집 | 구현 | hae-model/src/lib.rs:108-122 (모든 hp:p가 문단 노드). 내부 기록: 머리말·각주 편집 성공 (H-GAP §4.2, 이 조사에서 재실행 안 함) | 해당 없음 (HAE 자체 기능) | 하 | 낮음 | — |
| FD-10 | 각주·미주 삽입 | hp:footNote/hp:endNote (subList·p 포함 신규) | 새 각주·미주를 지정 위치에 삽입 | 미구현 | 노드 생성 불가 | S-HWPXMCP insert_footnote, insert_endnote; S-SDK InsertFootnote, InsertEndnote | 상 | 높음 | 2.00 |
| FD-11 | 머리말·꼬리말 내부 문단 편집 (객체 조회·설정 포함) | hp:ctrl/hp:header/hp:footer(@id,@applyPageType), hp:subList, hp:p | 머리말·꼬리말 텍스트 조회와 설정 (내부 문단은 현재도 편집 가능) | 부분 | 내부 문단은 노드로 편집 가능 (hae-model/src/lib.rs:114-122). hp:header/hp:footer 요소 이름은 HEAD에서 렌더러 매핑(hae-render-rhwp/src/pages.rs:60-61)에만 등장하며 편집 모델에는 없음 | S-HWPXMCP get_header, set_header, get_footer, set_footer; S-PYAUTO set_header_footer; S-HWPXLIB ctrl/Header.java, Footer.java; S-SDK HeaderFooterModify | 하 | 낮음 | 16.00 (2위) |
| FD-12 | 머리말·꼬리말 신규 생성·적용 쪽 종류 변경 | hp:header/hp:footer@applyPageType (BOTH/EVEN/ODD 계열; 코퍼스 hp:header 3건) | 머리말·꼬리말 생성, 삭제, 적용 쪽 변경 | 미구현 | control 생성 경로 없음 | S-HWPXMCP set_header(apply_page_type); S-PYAUTO set_header_footer; S-SDK HeaderFooterInsField, HeaderFooterDelete | 중 | 높음 | 3.00 |
| FD-13 | 메모 목록 조회 (MEMO 필드) | hp:fieldBegin@type="MEMO"; hh:memoPr(@id,@width,@lineWidth,@lineType,@lineColor,@fillColor,@activeColor,@memoType) (코퍼스 29건) | 메모 작성자·내용·위치 조회 | 미구현 | HEAD: 메모 목록 없음. 메모 개수 집계는 작업 트리(미커밋) query.rs 변경에만 있음 | S-HWPXMCP get_memos; S-PYAUTO add_memo, add_memo_by_anchor, remove_memo (목록 명령 미확인); S-HWPXLIB header_xml/references/MemoPr.java; S-SDK EditFieldMemo | 하 | 없음 | 15.00 (5위) |
| FD-14 | 메모 추가·삭제 | hp:fieldBegin(MEMO) 생성·제거, hh:memoPr | 메모 추가와 삭제 | 미구현 | 필드 생성 경로 없음 | S-PYAUTO add_memo, remove_memo; S-HWPXMCP insert_memo, delete_memo; S-SDK InsertFieldMemo, DeleteFieldMemo | 상 | 높음 | 2.00 |
| FD-15 | 변경 추적 마크 목록 조회 (삽입·삭제 구간, 작성자) | hp:insertBegin/insertEnd/deleteBegin/deleteEnd; hh:trackChange, hh:trackChangeAuthor (코퍼스 0건) | 변경 추적 구간과 작성자 조회 | 부분 | 마크가 있으면 문서 전체를 읽기 전용으로 잠금 (hae-model/src/lib.rs:517, 536 readonly 사유 track_changes). 목록 조회 없음 | S-PYAUTO add_tracked_edit; S-PYHWPX 변경추적(redline) (README 81행); S-HWPXLIB header_xml/references/TrackChange.java, TrackChangeAuthor.java; S-SDK InsertRevision* 계열 | 하 | 없음 | 15.00 (6위) |
| FD-16 | 변경 추적 생성·수락·거절 | hp:insertBegin/insertEnd/deleteBegin/deleteEnd 생성 | 변경 추적 구간 작성, 수락·거절 | 미구현 | 변경 추적 문서는 편집 잠금 (H-CODE lib.rs:536) | S-PYAUTO add_tracked_edit; S-PYHWPX redline; S-SDK InsertRevision* 계열 | 최상 | 높음 | 0.75 |
| FD-17 | 쪽 번호 표시·시작 번호 설정 | hp:pageNum@pos,@formatType,@sideChar; hp:newNum@num,@numType; hp:autoNum; hp:startNum@page,@pageStartsOn (코퍼스 pageNum 2건, newNum 2건) | 쪽 번호 위치·형식 설정과 시작 번호 변경 | 미구현 | HEAD: 렌더러 매핑에만 존재 (hae-render-rhwp/src/pages.rs). startNum 읽기는 작업 트리(미커밋) | S-PYAUTO set_page_number; S-HWPXLIB ctrl/PageNum.java, NewNum.java, AutoNum.java; S-SDK InsertPageNum, PageNumPos; S-PYHWPX 쪽번호 서식 (README 79행) | 중 | 중간 | 6.00 (22위) |
| FD-18 | 목차(TOC) 생성 | 필드 기반 목차 (rhwp 명칭 TABLE_OF_CONTENTS; OWPML 명칭 미확인, U10) | 문서 제목을 모아 목차 필드 생성 | 미구현 | 없음 | S-PYAUTO add_toc, verify_toc; S-PYHWPX 목차·상호참조 (README 81행); S-RHWP section.rs:5568 (TOC) | 상 | 높음 | 2.00 |
| FD-19 | 상호 참조 | CROSSREF 필드 (OWPML 명칭 미확인, U10) | 문단·표·그림 참조 필드 삽입 | 미구현 | 없음 | S-PYAUTO add_cross_reference; S-PYHWPX 목차·상호참조 (README 81행); S-RHWP section.rs:5558 (CROSSREF) | 상 | 높음 | 1.33 |
| FD-20 | 표시 전용 필드 경계 보호 (MEMO, 교정부호 등) | hp:fieldBegin/hp:fieldEnd (type=MEMO, PROOFREADING_MARKS_*; 공개 코퍼스 내 교정부호 필드 존재는 내부 기록 H-BACKLOG) | 표시 전용 필드 안 텍스트를 편집에서 보호 | 구현 | hae-exec/tests/editable_verified.rs:667 (필드 경계 거절 시험); hae-verify/src/text.rs:224 | 해당 없음 (HAE 자체 기능) | 해당 없음 | 없음 | — |

### 개체 (14)

| ID | 기능/요소 | OWPML 요소·속성 | 무엇을 하는가 | HAE 현재 수준 | HAE 근거 | 경쟁 구현 지원 (근거) | 난이도 | 보존 계약 영향 | 우선순위 |
|---|---|---|---|---|---|---|---|---|---|
| OB-01 | 그림 목록·속성 조회 (프레임·원본 크기·미디어 해시·효과) | hp:pic; hc:img@binaryItemIDRef,@bright,@contrast,@effect,@alpha; hp:imgRect; hp:imgClip; hp:imgDim; hp:sz; hp:pos | 그림 목록과 속성 조회 (미디어 해시 포함) | 부분 | 그림은 Kind::Picture 노드. element.query kind=picture (registry.rs HEAD 665), query.rs:193 picture_media | S-HWPXMCP get_images; S-HWPMCP list_hwp_images, list_hwp_bindata | 하 | 없음 | 10.00 (12위) |
| OB-02 | 그림 크기 변경 (기존 그림 프레임 크기 조정) | hp:sz@width,@height; hp:curSz; hp:orgSz; hp:imgDim; hp:pos | 그림 프레임 크기를 바꿈 (원본 기하는 보존 대상) | 미구현 | 기하 변경은 검증이 거절 (docs/decisions/hwpx-verification-rules.md §2.7) | S-HWPXMCP update_image_size | 중 | 중간 | 4.50 (29위) |
| OB-03 | 그림 삽입 (새 hp:pic·BinData·매니페스트 등록) | hp:pic, hc:img@binaryItemIDRef, hp:imgRect, hp:imgClip, hp:imgDim, hp:sz, hp:pos; opf:item (isEmbeded) | 새 그림을 문서에 넣고 바이너리와 매니페스트를 등록 | 미구현 | 미디어 항목 규칙은 검증에 정의돼 있음 (docs/decisions/hwpx-verification-rules.md §2.1). 생성 명령 없음 | S-HWPXMCP insert_image; S-HWPMCP insert_hwp_image; S-HANCOMMCP insert_image; S-PYAUTO insert_picture | 상 | 중간 | 4.00 |
| OB-04 | 그림 자르기·효과 (그림자·빛남·부드러운 가장자리·반사) | hp:imgClip; hc:img@effect; hp:effects (hp:shadow, hp:glow, hp:softEdge, hp:reflection) | 그림 자르기와 시각 효과 추가 | 미구현 | 거절 (hae-model/src/query.rs:558 HEAD effects; hae-edit/src/image.rs:332) | S-PYHWPX add_picture(crop, shadow, glow, soft_edge, reflection) (README 173행) | 상 | 중간 | 2.00 |
| OB-05 | 도형 목록 조회 (선·사각형·타원·호·다각형·곡선·연결선·글상자·묶음) | hp:line, hp:rect, hp:ellipse, hp:arc, hp:polygon, hp:curve, hp:connectLine, hp:container, hp:drawText(@lastWidth,@name,@editable), hp:pos, hp:sz | 도형 종류·위치·크기 목록 조회 (글상자 안 문단은 이미 편집 가능) | 미구현 | 도형 요소는 노드가 아님 (hae-model/src/lib.rs:114-122). 글상자 안 문단 편집은 시험으로 확인 (hae-exec/tests/editable_verified.rs:563) | S-HWPXLIB object/Rectangle.java, Ellipse.java, Line.java, Arc.java, Polygon.java, Curve.java, ConnectLine.java, Container.java; S-PYHWPX add_line 등 (README 167-175행) | 중 | 없음 | 7.50 (21위) |
| OB-06 | 도형 생성 (선·사각형·타원·글상자) | hp:rect, hp:ellipse, hp:line + 필수 하위 요소 (hp:shapeComment, hp:lineShape, hc:fillBrush 등) | 새 도형을 문서에 삽입 | 미구현 | 생성 경로 없음 | S-HWPXMCP insert_line, insert_rect, insert_ellipse; S-PYHWPX add_line, add_rectangle, add_ellipse (README: 저수준 add_shape()는 한글이 열지 못하는 파일 생성 가능) | 상 | 높음 | 2.00 |
| OB-07 | 도형 속성 편집 (위치·크기·회전·뒤집기·채우기·선) | hp:pos, hp:sz, hp:rotationInfo, hp:flip, hc:fillBrush, hp:lineShape | 도형 속성 변경 | 미구현 | 생성·편집 경로 없음 | S-PYHWPX (뒤집기·회전 미지원 명시, README 173-175행); S-SDK ShapeObjRotater, ShapeObjHorzFlip | 상 | 중간 | 2.00 |
| OB-08 | 수식 목록 조회 (스크립트 노출 여부 포함) | hp:equation (+ hp:script; 코퍼스 0건) | 수식 위치와 스크립트 조회 | 부분 | 수식은 노드로 식별, 편집은 equation_edit_unsupported (hae-model/src/query.rs:490). 스크립트 노출 여부 미확인 (U18) | S-HWPXMCP get_equations; S-HWPMCP render_hwp_equation_svg; S-PYAUTO add_equation (쓰기) | 하 | 없음 | 15.00 (7위) |
| OB-09 | 수식 스크립트 편집 | hp:equation/hp:script | 수식 스크립트 텍스트 교체 | 미구현 | equation_edit_unsupported (hae-model/src/query.rs:490) | S-HWPXMCP insert_equation (삽입); S-PYAUTO add_equation | 상 | 중간 | 3.00 |
| OB-10 | 차트 보존·조회 | hp:chart (차트 XML 파트 포함; 코퍼스 0건) | 차트 개체 조회 또는 생성 | 보존만 | 바이트 보존 (렌더러 매핑에만 등장: hae-render-rhwp/src/pages.rs) | S-PYAUTO add_chart, add_boxed_org_chart; S-HWPXLIB object/chart/ChartXMLFile.java; S-HWPERS 차트 '미구현' 명시 | 최상 | 높음 | 1.00 |
| OB-11 | 글맵시(textart) 보존 | hp:textart (코퍼스 0건) | 글맵시 조회 또는 편집 | 보존만 | 렌더러 매핑에만 등장 | S-HWPXLIB object/textart/TextArtPr.java | 상 | 중간 | 1.00 |
| OB-12 | OLE 개체 보존 | hp:ole (코퍼스 0건) | OLE 개체 보존 (조회·편집 없음) | 보존만 | 렌더러 매핑에만 등장 | S-HWPXLIB object/OLE.java (구현 관찰 근거 없음) | 상 | 높음 | 1.33 |
| OB-13 | 묶음 개체 (hp:container) | hp:container (코퍼스 20건) | 묶음 개체 조회·편집 | 보존만 | 보존만. 렌더러 매핑에만 등장 | S-PYHWPX add_container (README 167-175행) | 상 | 중간 | 2.00 |
| OB-14 | 양식 개체 (체크박스·라디오·콤보·목록·편집창) | hp:checkBtn, hp:radioBtn, hp:comboBox, hp:edit, hp:btn, hp:listBox, hp:scrollBar | 양식 개체 조회·값 설정 | 보존만 | 렌더러 매핑에만 등장 | S-HWPXLIB object/formobject/*.java, combobox/ListItem.java | 상 | 중간 | 2.00 |

### 구역·페이지 (6)

| ID | 기능/요소 | OWPML 요소·속성 | 무엇을 하는가 | HAE 현재 수준 | HAE 근거 | 경쟁 구현 지원 (근거) | 난이도 | 보존 계약 영향 | 우선순위 |
|---|---|---|---|---|---|---|---|---|---|
| PG-01 | 용지·여백·방향·시작 쪽 조회 (section.list) | hp:secPr; hp:pagePr@landscape,@width,@height,@gutterType; hp:margin@left,@right,@top,@bottom,@header,@footer,@gutter; hp:startNum | 구역별 용지·여백·방향·시작 쪽 조회 | 작업 중(미커밋) (작업 트리에서 구현 중, 미커밋) | HEAD에는 없음. query.rs:1052 section_list와 registry section.list는 작업 트리에서 구현 중(미커밋) | S-HWPXMCP get_page_settings; S-PYAUTO page_margins, page_orientation | 하 | 없음 | — (작업 중) |
| PG-02 | 용지·여백·방향 변경 | hp:pagePr 속성; hp:margin 속성 (코퍼스 36건, 속성 방식 확인) | 용지 크기·방향·여백 변경 (재조판 필요) | 미구현 | 읽기만 (작업 트리 기준). 변경 명령 없음 | S-HWPXMCP set_page_settings; S-PYAUTO set_page_setup, page_margins, page_orientation; S-HWPXLIB object/content/section_xml/paragraph/secpr/pagepr/PagePr.java | 중 | 중간 | 8.00 (14위) |
| PG-03 | 다단 설정 조회 | hp:colPr@type,@layout,@colCount,@sameSz,@sameGap (코퍼스 36건) | 구역의 단 수와 간격 조회 | 작업 중(미커밋) (작업 트리에서 구현 중, 미커밋) | HEAD에는 없음. section.list의 단 정보로 작업 트리에서 구현 중 | S-HWPXMCP get_column_def | 하 | 없음 | — (작업 중) |
| PG-04 | 다단 설정 변경 | hp:colPr 속성·자식 hp:colSz | 단 수·단 간격·단 모양 변경 | 미구현 | 변경 명령 없음 | S-HWPXMCP set_column_def; S-SDK MultiColumn; S-HWPXLIB ctrl/ColPr.java, ctrl/inner/ColSz.java | 중 | 중간 | 6.00 (24위) |
| PG-05 | 구역 추가·삭제 | hs:sec (구역 파일), hp:secPr, 매니페스트 항목 | 구역(섹션) 파일을 추가하거나 제거 | 미구현 | rebuild가 section 목록을 고정 (H-GAP §6.2) | S-HWPXMCP insert_section, delete_section; S-SDK BreakSection, ModifySection | 상 | 높음 | 1.00 |
| PG-06 | 쪽 테두리·배경 보존 | hp:pageBorderFill@type,@borderFillIDRef,@textBorder,@headerInside,@footerInside,@fillArea (코퍼스 102건) | 쪽 테두리와 배경 조회·변경 | 보존만 | 바이트 보존 (HEAD 미참조) | S-SDK PageBorder, PageBorderTab, PageFillTab | 중 | 중간 | 3.00 |

### 메타·자동화·검증 (17)

| ID | 기능/요소 | OWPML 요소·속성 | 무엇을 하는가 | HAE 현재 수준 | HAE 근거 | 경쟁 구현 지원 (근거) | 난이도 | 보존 계약 영향 | 우선순위 |
|---|---|---|---|---|---|---|---|---|---|
| MT-01 | 문서 메타데이터(제목·작성자 등) 조회·변경 | content.hpf의 opf:metadata, opf:title 등 (C++ 모델 opf:title 등재) | 문서 속성 조회와 변경 | 미구현 | 패키지 파트 읽기만 (hae-package). 메타데이터 명령 없음. HEAD에 메타데이터 파트 처리 코드 없음 (git grep 'metadata' → image.rs만) | S-HWPXMCP get_document_metadata, set_document_metadata; S-DVC jsonFullSpec.json DocSummaryInfo (규칙 키) | 중 | 중간 | 6.00 (23위) |
| MT-02 | 개인정보 패턴 탐지 | hp:t 텍스트 전체 | 주민등록번호 등 개인정보 패턴을 찾아 보고 | 미구현 | 없음 | S-PYAUTO scan_personal_info (도구 계약 파일) | 중 | 없음 | 7.50 (20위) |
| MT-03 | 매크로·스크립트 포함 여부 검사 | 패키지 항목 (Scripts/* 등; OWPML 요소 아님, U23 참조), DVC JID_MACRO_* 3건 | 문서가 매크로·스크립트 항목을 가지는지 보고 | 미구현 | 미지의 ZIP 항목은 원본 그대로 보존 (H-VERIFY §2.1). 탐지·보고 없음 | S-DVC Source/JsonModel.h (JID_MACRO_PERMISSION 등 3건); S-HWPMCP는 생성 시 Scripts/* 를 추가하지 않는다고 기록 (H-MATRIX 메모) | 하 | 없음 | — (미확인) |
| MT-04 | 문서 통계 (문단·표·그림·수식·필드·각주 개수, 글자 수) | 전 요소 (hp:p, hp:tbl, hp:pic, hp:equation, hp:fieldBegin, hp:footNote 등) | 문서 요약 수치 | 작업 중(미커밋) (작업 트리에서 구현 중, 미커밋) | HEAD에는 없음. registry document.stats와 query.rs stats는 작업 트리(미커밋) | S-PYAUTO get_document_info (도구 계약 파일) | 하 | 없음 | — (작업 중) |
| MT-05 | 문서 내용 추출 → Markdown·HTML 내보내기 | hp:p, hp:t, hp:tbl, hp:tc 등 전체 구조 | 문서 본문을 Markdown 또는 HTML로 내보냄 | 부분 | JSON 노드 조회(element.query/get, document.inspect)만 있음 (registry.rs HEAD). 4 MiB 응답 한도 (H-GAP §4.2) | S-PYAUTO hwpx_to_markdown, hwpx_to_html; S-HWPXMCP export_to_text, export_to_html; S-KORDOC README (Markdown 변환, 자체 보고 표 구조 일치율); S-UNHWP README (Markdown) | 중 | 없음 | 10.00 (10위) |
| MT-06 | 메모·문단 등 수정 전후 비교 (document.diff) | 전 요소 | 두 스냅샷의 의미 차이, 미디어, 캐시 차이 비교 | 구현 | registry.rs:803 (document.diff) | S-PYAUTO doc_diff; S-PYHWPX 텍스트 diff (README 81행) | 하 | 낮음 | — |
| MT-07 | 미리보기 (SVG·PNG; 외부 렌더러 필요) | 해당 없음 (렌더링) | 페이지 미리보기 생성 | 구현 | registry.rs:813 (document.preview). 외부 어댑터 미설정 시 RENDERER_UNAVAILABLE (H-GAP §1.2) | S-HWPMCP render_hwp_page, render_hwp_html; S-PYAUTO render_preview; S-HWPERS SVG 내보내기 (README) | 해당 없음 | 없음 | — |
| MT-08 | PDF 내보내기 | 해당 없음 (출력 형식) | 문서를 PDF로 출력 | 미구현 | 없음 (rhwp 렌더는 SVG·PNG) | S-HWPXMCP export_to_pdf (Windows: 한/글 COM, macOS: 한/글 for Mac, Linux 미지원 — README 플랫폼 표); S-SDK FileSaveAsPdfOption | 중 | 없음 | 3.00 |
| MT-09 | 메일 머지 (템플릿 + 데이터 → 여러 문서) | hp:t (치환 대상) | 템플릿 자리표시자를 데이터로 채워 문서를 여러 개 생성 | 부분 | 정확 일치 치환(TX-09)으로 조합은 가능. 데이터 바인딩과 다중 출력 명령 없음 | S-PYAUTO mail_merge, inspect_mail_merge_placeholders | 중 | 낮음 | 8.00 (13위) |
| MT-10 | 서식 템플릿 등록·조회·채우기 | 해당 없음 (패키지 기반 템플릿) | 기관 서식을 등록해 데이터로 채움 | 미구현 | 템플릿 개념 없음 (H-GAP §1.2는 '기존 시드 치환 수준'으로 기록) | S-PYAUTO register_template, list_templates, describe_template; S-HWPMCP fill_hwp_template | 상 | 중간 | 4.00 |
| MT-11 | 긴 작업의 비동기 실행·취소·진행률 | 해당 없음 (실행 방식) | 오래 걸리는 작업을 백그라운드로 돌리고 취소·진행률 조회 | 미구현 | 직렬 실행. 취소는 요청 완료 뒤에야 읽힘 (hae-mcp/src/lib.rs:233 주석) | S-PYAUTO start_workflow, get_workflow, cancel_workflow; render_submit, render_status, render_cancel | 중 | 없음 | 4.50 (28위) |
| MT-12 | 독립 검증 관문 (저장 전 거절) | 패키지·요소·텍스트 전반 | 편집기와 독립된 검증기가 후보 파일을 저장 직전에 검사 | 구현 | docs/decisions/hwpx-verification-rules.md; hae-verify 크레이트 (편집 계층 비의존) | S-PYHWPX 패키지 구조 검증 CLI (README 81행); S-POL Axis 2·3 (계획) | 해당 없음 | 없음 | — |
| MT-13 | OWPML 요소·속성 사전 기반 검사 (알 수 없는 요소·속성 경고) | 전 요소. C++ 모델 333요소·속성명 183종 (2023 스냅샷, U5) | 사전에 없는 요소·속성을 경고 (거절 기준은 별도 결정) | 미구현 | HEAD 검증은 규칙 기반이며 요소 사전과 대조하지 않음 (H-VERIFY §3: 공식 XSD 전체 검증 아님) | S-OWPML (사전 후보, Apache-2.0); S-POL Axis 4 (KS X 6101 XSD 기반, 계획) | 중 | 없음 | 10.00 (11위) |
| MT-14 | DVC 규칙 적합성 검사 (글자·문단·표·쪽·글머리 허용 규칙) | 규칙 키 ↔ JID 217개: 글자 41, 문단 47, 표 58, 쪽 23, 글머리 6, 문단번호 9, 개요 9, 특수문자 4, 스타일 4, 하이퍼링크 3, 매크로 3, 각주·미주 2, 메모 1, 책갈피 1, 차트 1, 수식 1, OLE 1, 글맵시 1, 문서요약 1, 양식개체 1 | 사용자 규칙(JSON)에 따라 문서 서식이 허용 범위인지 검사 | 미구현 | 규칙 기반 검사 없음 (보존 검증만) | S-DVC Source/JsonModel.h (JID 217개, README: HWPX 적합성 검증); S-POL docs/jid-registry.md (레지스트리 217, 엔진 참조 약 30개) | 상 | 없음 | 5.00 (26위) |
| MT-15 | KS X 6101 공식 XSD 전체 적합성 검증 | 전 요소·속성 (공식 XSD) | 공식 스키마 전체 대조 | 미확인 | 공식 XSD·명세 본문 미확보 (U1). 재배포 금지 명시 (S-KSX) | S-POL Axis 4 (계획; standards/ 자료 로컬 전용) | 상 | 없음 | — (미확인) |
| MT-16 | 비정상 패키지 복구 (repair) | 패키지 전반 | 손상되었거나 불완전한 HWPX를 고침 | 미구현 | 없음 (fail closed 정책과 충돌 가능) | S-PYAUTO repair_hwpx | 상 | 중간 | 2.00 |
| MT-17 | MCP 출력 스키마 개선 (uint* 표준 format, title 누락) | 해당 없음 (MCP descriptor) | 출력 스키마의 정확성과 표준 format 사용 | 미확인 | 내부 보고 근거만 (H-GAP §6.1 P4). 이 조사에서 코드 재확인 안 함 | 해당 없음 | 하 | 없음 | — (미확인) |

### 입출력·플랫폼 (5)

| ID | 기능/요소 | OWPML 요소·속성 | 무엇을 하는가 | HAE 현재 수준 | HAE 근거 | 경쟁 구현 지원 (근거) | 난이도 | 보존 계약 영향 | 우선순위 |
|---|---|---|---|---|---|---|---|---|---|
| IO-01 | HWP5(.hwp) 읽기 | 해당 없음 (HWP5 바이너리 CFB 구조; OWPML 외) | HWP5 문서를 읽어 조회 또는 HWPX로 변환 | 미구현 | HWP5 파서 없음 (H-GAP §5.3 G13) | S-UNHWP README (HWP 5.0·HWPX 읽기); S-HWPLIB (HWP5 Java 라이브러리, 골든 테스트); S-HWPERS README (HWP5 파서); S-KORDOC README (HWP 3.x/5.x); S-PYHWPX README (HWP 5.0 열기, 변환 리포트) | 상 | 중간 | 2.67 |
| IO-02 | HWP5(.hwp) 쓰기 | 해당 없음 | HWPX를 HWP5로 저장 | 미구현 | 없음 | S-PYHWPX save_to_path("x.hwp") (README: 옮기지 못한 내용은 변환 리포트에 기록); S-HWPERS Writer (README v0.3.0+) | 최상 | 높음 | 0.75 |
| IO-03 | 패키지 생성 (빈 HWPX 문서 생성) | ocf:container, opf:package(content.hpf), hh:head(header.xml), hs:sec(section0.xml), 매니페스트 | 빈 HWPX 패키지를 새로 만들고 내용을 채움 | 미구현 | 0.1 설계상 불가 (H-GAP §1.2 '문서·문단·표 구조 생성 미구현') | S-HWPMCP create_hwpx_document; S-HANCOMMCP create_document, create_hwpx; S-HWPXMCP create_document; S-PYAUTO create_document, create_document_from_plan | 최상 | 높음 | 2.00 |
| IO-04 | Windows 빌드·실행 | 해당 없음 | Windows에서 빌드하고 실행 | 미확인 | 코드에 Windows 분기 존재 (crates/hae-project/src/io/windows.rs, use std::os::windows). 이 조사에서 빌드 미실행. H-GAP §6.1 P3(빌드 장애) 기록과 상충 가능 (U11) | 해당 없음 | 해당 없음 | 없음 | — (미확인) |
| IO-05 | macOS 빌드·실행 | 해당 없음 | macOS에서 빌드하고 실행 | 미확인 | 미검증 (H-GAP §1.2: 'macOS 미검증') | 해당 없음 | 해당 없음 | 없음 | — (미확인) |

### 보존·안정성 (7)

| ID | 기능/요소 | OWPML 요소·속성 | 무엇을 하는가 | HAE 현재 수준 | HAE 근거 | 경쟁 구현 지원 (근거) | 난이도 | 보존 계약 영향 | 우선순위 |
|---|---|---|---|---|---|---|---|---|---|
| PR-01 | 미편집 ZIP 항목 원시 바이트 보존 저장 | 패키지 항목 전반 | 편집하지 않은 항목은 압축 데이터까지 원본과 같게 저장 | 구현 | docs/decisions/hwpx-verification-rules.md §2.1; crates/hae-zip (H-VERIFY) | S-HWPXMCP·S-PYAUTO 등 경쟁 구현은 payload 동일 수준까지 확인 (H-GAP §2.1 표) | 해당 없음 | 없음 | — |
| PR-02 | 원자 일괄 편집·Undo/Redo | 해당 없음 | 여러 편집을 한 단위로 적용, 실패 시 전부 되돌림, Undo 후 원본 바이트 동일 | 구현 | registry.rs (command.batch, history.undo, history.redo HEAD); H-GAP §4.1 (Undo 후 바이트 동일 기록) | 경쟁 구현 다수는 배치 원자성 미확인 (H-MATRIX batch_atomic 열) | 해당 없음 | 없음 | — |
| PR-03 | 서명된 문서 편집 잠금 | 서명 항목 (signature) | 서명이 있으면 편집을 막음 (사유 signed) | 구현 | hae-model/src/lib.rs:515 (HEAD) | 해당 없음 (HAE 자체 정책) | 해당 없음 | 없음 | — |
| PR-04 | 미지원 네임스페이스 요소 불투명 보존 | 외부 네임스페이스 요소 (hp·hh·hc·hs 이외) | 모르는 요소는 편집에서 제외하고 바이트를 보존 | 구현 | hae-model/src/lib.rs:124-131 (HEAD, Kind::Opaque) | 해당 없음 (HAE 자체 정책) | 해당 없음 | 없음 | — |
| PR-05 | 프로젝트 파일(.hwpwork) 저장과 잠금 | 해당 없음 (HAE 프로젝트 형식) | 편집 중인 문서를 원자적으로 저장하고 잠금 | 구현 | registry.rs (project.save, document.open HEAD); crates/hae-project | 해당 없음 (HAE 자체 기능) | 해당 없음 | 없음 | — |
| PR-06 | 스냅샷 캡처·해제 | 해당 없음 | 편집 전 상태를 명시적으로 잡아 두고 비교 | 구현 | registry.rs (document.snapshot, snapshot.release HEAD) | S-PYAUTO doc_diff (비교 기능) | 해당 없음 | 없음 | — |
| PR-07 | 변경 추적 문서 편집 잠금 | hp:insertBegin 등, hh:trackChange | 변경 추적 흔적이 있으면 문서 전체 편집을 막음 (사유 track_changes) | 구현 | hae-model/src/lib.rs:517-536 (HEAD) | 해당 없음 (HAE 자체 정책) | 해당 없음 | 없음 | — |

## 2. 우선순위 상위 30개

산식: 점수 = V × A ÷ D. 같은 점수는 가치 V가 높은 쪽, 다음은 ID 순이다. 아래 '한 줄 이유'가 순위의 근거다.

| 순위 | ID | 기능 | V | A | D | 점수 | 왜 이 순위인가 | 백로그 | 비고 |
|---|---|---|---|---|---|---|---|---|---|
| 1 | TB-02 | 표 구조 조회 (행·열 수, 셀 주소·병합 범위·크기) | 4 | 5 | 하 | 20.00 | 읽기 전용이라 HAE 보존·검증 계약을 건드리지 않고, 셀 노드가 이미 있어 구현 비용이 가장 낮다. | F9 table.get |  |
| 2 | FD-11 | 머리말·꼬리말 내부 문단 편집 (객체 조회·설정 포함) | 4 | 4 | 하 | 16.00 | 기존 문단 편집 계약을 그대로 쓰고, 머리말 객체를 찾아 문단을 고르는 조회 한 가지만 더하면 된다. 공문서 머리글 자동 입력의 요구와 맞는다. | F14 header.setText |  |
| 3 | FD-04 | 하이퍼링크 목록 조회 (URL·표시 문자열) | 3 | 5 | 하 | 15.00 | 읽기 전용이고 필드 경계 판정 코드를 재사용할 수 있다. | 신규(백로그 외) |  |
| 4 | FD-06 | 책갈피 목록 조회 | 3 | 5 | 하 | 15.00 | 읽기 전용이고 템플릿 자동화(책갈피 위치 채우기)의 전제다. | F6 bookmark.list |  |
| 5 | FD-13 | 메모 목록 조회 (MEMO 필드) | 3 | 5 | 하 | 15.00 | 읽기 전용이고 검토 워크플로에 바로 쓰인다. | F7 memo.list |  |
| 6 | FD-15 | 변경 추적 마크 목록 조회 (삽입·삭제 구간, 작성자) | 3 | 5 | 하 | 15.00 | 읽기 전용이면서 현재의 전체 잠금을 세분화하는 첫 단계가 된다. | 신규(백로그 외) |  |
| 7 | OB-08 | 수식 목록 조회 (스크립트 노출 여부 포함) | 3 | 5 | 하 | 15.00 | 수식 노드가 이미 있어 목록 래퍼만 추가하면 되는 읽기 전용 작업이다. 스크립트 노출은 U18에서 확인한다. | 신규(백로그 외) | 스크립트 노출 여부는 U18 참조 |
| 8 | FD-01 | 필드 목록 조회 (누름틀·기타 필드의 이름·종류·값) | 5 | 5 | 중 | 12.50 | 누름틀 값 채우기(FD-02)의 전제. 읽기 전용이라 계약 영향이 없다. | F1 field.list (진행 중) | 백로그상 '진행 중'. 이 저장소 HEAD와 작업 트리에 field.list 흔적 없음 |
| 9 | TB-07 | 라벨 기반 양식 채우기 (라벨 셀 → 옆 셀 값) | 5 | 5 | 중 | 12.50 | 가치가 가장 크고 쓰기 대상이 기존 cell.setText라서 계약 변화가 작다. 공문서 자동화의 핵심 요구와 맞는다. | 신규(백로그 외) |  |
| 10 | MT-05 | 문서 내용 추출 → Markdown·HTML 내보내기 | 4 | 5 | 중 | 10.00 | 읽기 전용 내보내기로 계약 영향이 없고, 에이전트가 문서를 읽는 가장 흔한 경로다. | 신규(백로그 외) |  |
| 11 | MT-13 | OWPML 요소·속성 사전 기반 검사 (알 수 없는 요소·속성 경고) | 4 | 5 | 중 | 10.00 | Apache-2.0 C++ 모델에서 사전을 만들면 외부 자료 재배포 없이 독립 검증 관문을 넓힐 수 있다. 사전이 2023 스냅샷이라 누락 보정이 필요하다. | 신규(백로그 외) |  |
| 12 | OB-01 | 그림 목록·속성 조회 (프레임·원본 크기·미디어 해시·효과) | 2 | 5 | 하 | 10.00 | 그림 노드와 미디어 정보가 이미 있어 조회 래퍼만 추가하면 된다. 가치는 TB-02보다 낮다. | F10 picture.list |  |
| 13 | MT-09 | 메일 머지 (템플릿 + 데이터 → 여러 문서) | 4 | 4 | 중 | 8.00 | 정확 일치 치환으로 조합 가능한 흐름을 정식 명령으로 묶는 것이라 계약 변화가 작고 대량 발급 수요에 맞는다. | 신규(백로그 외) |  |
| 14 | PG-02 | 용지·여백·방향 변경 | 4 | 4 | 중 | 8.00 | 속성 변경이라 노드 구조는 그대로지만, 줄 배치 정보(linesegarray) 무효화를 함께 다뤄야 해서 계약 영향이 중간이다. | 신규(백로그 외) |  |
| 15 | TB-03 | 셀 속성 변경 (배경색·테두리·세로 정렬·셀 크기·안 여백) | 4 | 4 | 중 | 8.00 | 셀 서식은 기존 borderFill 참조만 바꾸는 경로로 시작할 수 있고, 표 서식 자동화의 가치가 크다. | F12 cell.format |  |
| 16 | TX-02 | 글꼴 이름·상대 크기·자간 변경 | 4 | 4 | 중 | 8.00 | 글꼴 지정은 체감 가치가 높고 기존 charPr 편집 경로(TX-01)를 재사용한다. 글꼴 참조 목록을 다루는 것이 추가 작업이다. | 신규(백로그 외) |  |
| 17 | TX-04 | 문단 여백·들여쓰기·문단 위아래 간격 | 4 | 4 | 중 | 8.00 | 문단 여백은 공문서 서식의 기본 요소다. 기존 paragraph.format과 같은 경로(paraPr 재사용 또는 새 1개)로 구현할 수 있다. | 신규(백로그 외) |  |
| 18 | TX-07 | 스타일 목록 조회·문단 스타일 적용 | 4 | 4 | 중 | 8.00 | 스타일 목록과 기존 스타일 적용은 백로그 F8·F11과 같고 참조 값만 바꾸므로 계약 영향이 낮다. | F8 style.list + F11 paragraph.setStyle |  |
| 19 | FD-02 | 누름틀 값 채우기 (Command 문자열 갱신) | 5 | 3 | 중 | 7.50 | 사용자 가치는 최상이지만, 필드 경계 거절 규칙을 '값만 변경' 예외로 바꿔야 해서 정합이 3이다. 검증 규칙 설계가 선행돼야 한다. | F2 field.setText (진행 중) | 백로그상 '진행 중'. 실제 CLICKHERE 샘플 없음 (U2) |
| 20 | MT-02 | 개인정보 패턴 탐지 | 3 | 5 | 중 | 7.50 | 개인정보 패턴은 오탐이 많아 규칙 설계가 필요하다. 읽기 전용이라 계약 영향은 없다. | 신규(백로그 외) |  |
| 21 | OB-05 | 도형 목록 조회 (선·사각형·타원·호·다각형·곡선·연결선·글상자·묶음) | 3 | 5 | 중 | 7.50 | 읽기 전용이라 계약 영향이 없고, 글상자 텍스트 편집을 찾는 데 필요하다. | 신규(백로그 외) |  |
| 22 | FD-17 | 쪽 번호 표시·시작 번호 설정 | 3 | 4 | 중 | 6.00 | 쪽 번호는 공문서 기본 요구인데 현재는 렌더러 매핑에만 있어 모델이 필요하다. 생성 자원(ctrl)이 생겨 정합은 보통이다. | 신규(백로그 외) |  |
| 23 | MT-01 | 문서 메타데이터(제목·작성자 등) 조회·변경 | 3 | 4 | 중 | 6.00 | 문서 제목·작성자는 자동화 결과 확인에 자주 쓰이고, 패키지 메타 파트만 바꾸므로 본문 계약에 닿지 않는다. | 신규(백로그 외) |  |
| 24 | PG-04 | 다단 설정 변경 | 3 | 4 | 중 | 6.00 | 단 설정 변경은 속성 중심이라 PG-02와 같은 경로로 가능하지만 재조판 효과가 커서 이 순위다. | 신규(백로그 외) |  |
| 25 | TX-05 | 쪽 나눔·줄 나눔 속성 (keepWithNext, pageBreakBefore, widowOrphan) | 3 | 4 | 중 | 6.00 | 쪽 나눔 옵션은 긴 문서 서식에 필요하다. paraPr 복사 경로를 쓰므로 TX-03과 같은 난이도다. | 신규(백로그 외) |  |
| 26 | MT-14 | DVC 규칙 적합성 검사 (글자·문단·표·쪽·글머리 허용 규칙) | 3 | 5 | 상 | 5.00 | 규칙 기반 적합성 검사는 HAE 검증 관문과 목적이 가깝고 읽기 전용이다. 다만 규칙 설계(JID 217개)가 커서 난이도가 높다. | 신규(백로그 외) |  |
| 27 | FD-07 | 책갈피 삽입 | 3 | 3 | 중 | 4.50 | 책갈피 삽입은 ctrl 추가라 중간 난이도다. 책갈피 조회(FD-06)가 먼저다. | 신규(백로그 외) |  |
| 28 | MT-11 | 긴 작업의 비동기 실행·취소·진행률 | 3 | 3 | 중 | 4.50 | 긴 문서에서 MCP 응답 한계를 넘기 위한 기반이지만 실행기 구조 변경이 필요하다. | G17 (0.3 후보) |  |
| 29 | OB-02 | 그림 크기 변경 (기존 그림 프레임 크기 조정) | 3 | 3 | 중 | 4.50 | 그림 크기 변경은 기하 보존 계약을 명시적으로 완화해야 하므로 설계 결정 뒤에 진행한다(백로그 F13, 0.2 후보). | F13 image.fit (0.2 후보) |  |
| 30 | TX-08 | 특수 문자·탭·공백 삽입 (탭, 줄바꿈, 전각·불가분 공백) | 3 | 3 | 중 | 4.50 | 특수 문자 삽입은 텍스트 조각 경계와 맞물려 중간 난이도다. 탭·줄바꿈 입력 요구가 있을 때 올린다. | 신규(백로그 외) |  |

상위 항목의 특징: 읽기 전용 조회가 많다. 계약 영향이 없으면서 HAE의 보존·검증 구조를 그대로 쓸 수 있기 때문이다. 쓰기 항목은 값·속성만 바꾸는 것(TX-02, TX-04, TB-03, PG-02)부터 올렸다. 노드를 만들거나 지우는 항목(TB-04, TB-06, TX-11)은 설계가 먼저 필요해 점수가 낮다. 상위 10개 요약은 최종 답변에 적었다.

## 3. HAE에 없는 요소 목록 (OWPML에 있으나 HAE가 다루지 않는 것)

판정 방법: C++ 모델의 요소 333개 중 HAE HEAD 소스에서 이름이 한 번도 쓰이지 않은 것(없음), 렌더러 매핑에만 있는 것(렌더러만), 다른 의미의 같은 문자열로만 쓰인 것(이름 충돌)을 모았다. 이름이 쓰였다고 해서 편집이 된다는 뜻은 아니다. 편집·검증·보존 판단 코드가 이름을 인식하는 요소(62개: 예 hp:p, hp:tbl, hp:fieldBegin, hp:equation, hh:charPr)는 여기서 뺐다. 코퍼스에만 있고 모델에 없는 이름 5개는 아래 목록 끝에 함께 적었다. 괄호의 `코퍼스 n`은 10개 파일 등장 횟수다(— 는 모델 밖 네임스페이스). 이 목록은 HAE 지원 범위의 빈칸을 보여 주며, 구현 우선순위와는 별개다.

| 상태 | 뜻 | 개수 |
|---|---|---|
| 없음 (NONE) | HAE 소스에 이름이 없음 | 223 |
| 렌더러만 (RENDER_ONLY) | 렌더러 매핑에만 있음 | 35 |
| 이름 충돌 (COLLISION) | 같은 문자열이 다른 의미로만 쓰임 | 13 |
| 모델 미등재 (코퍼스에만 있음) | C++ 모델에 없음 | 5 |

**hp:** 116개

`hp:alpha` [이름충돌] (코퍼스 0), `hp:arc` [렌더러만] (코퍼스 0), `hp:arrayParam` (코퍼스 0), `hp:autoNum` [렌더러만] (코퍼스 0), `hp:autoNumFormat` (코퍼스 72), `hp:bindataParam` (코퍼스 0), `hp:bookmark` [렌더러만] (코퍼스 0), `hp:booleanParam` (코퍼스 0), `hp:btn` [렌더러만] (코퍼스 0), `hp:caption` (코퍼스 0), `hp:cellMargin` (코퍼스 634), `hp:cellSz` (코퍼스 634), `hp:cellzone` (코퍼스 0), `hp:cellzoneList` (코퍼스 0), `hp:chart` [렌더러만] (코퍼스 0), `hp:checkBtn` [렌더러만] (코퍼스 0), `hp:chval` (코퍼스 0), `hp:cmyk` (코퍼스 0), `hp:colLine` (코퍼스 0), `hp:colPr` [렌더러만] (코퍼스 36), `hp:colSz` (코퍼스 0), `hp:comboBox` [렌더러만] (코퍼스 0), `hp:compose` [렌더러만] (코퍼스 0), `hp:connectLine` [렌더러만] (코퍼스 0), `hp:container` [렌더러만] (코퍼스 20), `hp:controlPoints` (코퍼스 0), `hp:ctrl` [렌더러만] (코퍼스 124), `hp:curve` [렌더러만] (코퍼스 0), `hp:drawText` (코퍼스 26), `hp:dutmal` [렌더러만] (코퍼스 0), `hp:edit` [렌더러만] (코퍼스 0), `hp:effect` [이름충돌] (코퍼스 0), `hp:effectsColor` (코퍼스 0), `hp:ellipse` [렌더러만] (코퍼스 0), `hp:endNote` [렌더러만] (코퍼스 0), `hp:endNotePr` (코퍼스 36), `hp:endPt` (코퍼스 0), `hp:firstKey` (코퍼스 0), `hp:flip` (코퍼스 108), `hp:floatParam` (코퍼스 0), `hp:footNote` [렌더러만] (코퍼스 0), `hp:footNotePr` (코퍼스 36), `hp:footer` [렌더러만] (코퍼스 0), `hp:formCharPr` (코퍼스 0), `hp:grid` (코퍼스 36), `hp:header` [렌더러만] (코퍼스 3), `hp:hiddenComment` [렌더러만] (코퍼스 0), `hp:hypen` (코퍼스 0), `hp:imgRect` (코퍼스 27), `hp:inMargin` (코퍼스 82), `hp:indexmark` [렌더러만] (코퍼스 0), `hp:integerParam` (코퍼스 3), `hp:label` [이름충돌] (코퍼스 0), `hp:line` [렌더러만] (코퍼스 0), `hp:lineNumberShape` (코퍼스 36), `hp:lineShape` (코퍼스 61), `hp:lineseg` (코퍼스 2000), `hp:listBox` (코퍼스 0), `hp:listItem` (코퍼스 0), `hp:listParam` (코퍼스 0), `hp:mainText` (코퍼스 0), `hp:margin` (코퍼스 36), `hp:markpenBegin` (코퍼스 0), `hp:markpenEnd` (코퍼스 0), `hp:masterPage` (코퍼스 0), `hp:metaTag` (코퍼스 0), `hp:newNum` [렌더러만] (코퍼스 2), `hp:noteLine` (코퍼스 72), `hp:noteSpacing` (코퍼스 72), `hp:numbering` (코퍼스 72), `hp:offset` [이름충돌] (코퍼스 210), `hp:ole` [렌더러만] (코퍼스 0), `hp:orgSz` (코퍼스 108), `hp:outMargin` (코퍼스 87), `hp:outline` [이름충돌] (코퍼스 0), `hp:pageBorderFill` (코퍼스 102), `hp:pageHiding` [렌더러만] (코퍼스 3), `hp:pageNum` [렌더러만] (코퍼스 2), `hp:pageNumCtrl` [렌더러만] (코퍼스 0), `hp:pagePr` (코퍼스 36), `hp:parameters` (코퍼스 39), `hp:parameterset` (코퍼스 0), `hp:placement` (코퍼스 72), `hp:point` (코퍼스 0), `hp:polygon` [렌더러만] (코퍼스 24), `hp:pos` (코퍼스 87), `hp:presentation` (코퍼스 0), `hp:radioBtn` [렌더러만] (코퍼스 0), `hp:rect` [렌더러만] (코퍼스 37), `hp:renderingInfo` (코퍼스 108), `hp:rgb` (코퍼스 0), `hp:rotationInfo` (코퍼스 108), `hp:scale` (코퍼스 0), `hp:scheme` (코퍼스 0), `hp:script` [렌더러만] (코퍼스 0), `hp:scrollBar` (코퍼스 0), `hp:secondKey` (코퍼스 0), `hp:seg` (코퍼스 0), `hp:shapeComment` (코퍼스 30), `hp:skew` (코퍼스 0), `hp:startNum` (코퍼스 36), `hp:startPt` (코퍼스 0), `hp:stringParam` (코퍼스 51), `hp:subText` (코퍼스 0), `hp:system` (코퍼스 0), `hp:text` [이름충돌] (코퍼스 0), `hp:textMargin` (코퍼스 26), `hp:textart` (코퍼스 0), `hp:textartPr` (코퍼스 0), `hp:titleMark` (코퍼스 0), `hp:tr` [렌더러만] (코퍼스 233), `hp:unknownObj` (코퍼스 0), `hp:unknownch` (코퍼스 0), `hp:unsignedintegerParam` (코퍼스 0), `hp:video` (코퍼스 0), `hp:visibility` [렌더러만] (코퍼스 36)

**hh:** 97개

`hh:adjustBaselineInFixedLinespacing` (코퍼스 1), `hh:adjustBaselineOfObjectToBottom` (코퍼스 1), `hh:adjustLineheightToFont` (코퍼스 1), `hh:adjustMarginFromAdjustLineheight` (코퍼스 1), `hh:adjustParaBorderOffsetWithBorder` (코퍼스 1), `hh:adjustParaBorderfillToSpacing` (코퍼스 1), `hh:adjustVertPosOfLine` (코퍼스 1), `hh:applyAtLeastToPercent100Pct` (코퍼스 0), `hh:applyCharSpacingToCharGrid` (코퍼스 1), `hh:applyExtendHeaderFooterEachSection` (코퍼스 1), `hh:applyFontWeightToBold` (코퍼스 1), `hh:applyFontspaceToLatin` (코퍼스 0), `hh:applyMinColumnWidthTo1mm` (코퍼스 1), `hh:applyNextspacingOfLastPara` (코퍼스 1), `hh:applyParaBorderToOutside` (코퍼스 1), `hh:applyPrevspacingBeneathObject` (코퍼스 1), `hh:applyTabPosBasedOnSegment` (코퍼스 1), `hh:autoSpacing` (코퍼스 778), `hh:backSlash` (코퍼스 108), `hh:baseCharUnitOfIndentOnFirstChar` (코퍼스 1), `hh:baseCharUnitOnEAsian` (코퍼스 1), `hh:baseLineSpacingOnLineGrid` (코퍼스 1), `hh:beginNum` (코퍼스 8), `hh:border` (코퍼스 778), `hh:borderFills` (코퍼스 9), `hh:bottomBorder` (코퍼스 108), `hh:breakSetting` (코퍼스 778), `hh:breakTabOverLine` (코퍼스 1), `hh:bullet` (코퍼스 0), `hh:bullets` (코퍼스 0), `hh:char` [모델 미등재] (코퍼스 2), `hh:compatibleDocument` (코퍼스 9), `hh:connectParaBorderfillOfEqualBorder` (코퍼스 1), `hh:diagonal` (코퍼스 98), `hh:doNotAdjustEmptyAnchorLine` (코퍼스 1), `hh:doNotAdjustWordInJustify` (코퍼스 1), `hh:doNotAlignLastForbidden` (코퍼스 1), `hh:doNotAlignLastPeriod` (코퍼스 0), `hh:doNotAlignWhitespaceOnRight` (코퍼스 1), `hh:doNotApplyAutoSpaceEAsianEng` (코퍼스 0), `hh:doNotApplyAutoSpaceEAsianNum` (코퍼스 0), `hh:doNotApplyColSeparatorAtNoGap` (코퍼스 0), `hh:doNotApplyDiacSymMarkOfNoneAndSix` (코퍼스 0), `hh:doNotApplyExtensionCharCompose` (코퍼스 0), `hh:doNotApplyGridInHeaderFooter` (코퍼스 1), `hh:doNotApplyHeaderFooterAtNoSpace` (코퍼스 0), `hh:doNotApplyImageEffect` (코퍼스 0), `hh:doNotApplyLinegridAtNoLinespacing` (코퍼스 1), `hh:doNotApplyShapeComment` (코퍼스 0), `hh:doNotApplyStrikeoutWithUnderline` (코퍼스 0), `hh:doNotApplyVertOffsetOfForward` (코퍼스 0), `hh:doNotApplyWhiteSpaceHeight` (코퍼스 0), `hh:doNotFormattingAtBeneathAnchor` (코퍼스 1), `hh:doNotHoldAnchorOfTable` (코퍼스 1), `hh:docOption` (코퍼스 8), `hh:extendLineheightToOffset` (코퍼스 1), `hh:extendLineheightToParaBorderOffset` (코퍼스 1), `hh:extendVertLimitToPageMargins` (코퍼스 1), `hh:field` [모델 미등재] (코퍼스 2), `hh:fixedUnderlineWidth` (코퍼스 0), `hh:font` (코퍼스 322), `hh:fontface` (코퍼스 57), `hh:fontfaces` (코퍼스 9), `hh:forbiddenWord` (코퍼스 0), `hh:forbiddenWordList` (코퍼스 0), `hh:heading` (코퍼스 787), `hh:layoutCompatibility` (코퍼스 9), `hh:leftBorder` (코퍼스 108), `hh:licensemark` (코퍼스 0), `hh:linkinfo` (코퍼스 8), `hh:margin` (코퍼스 1515), `hh:memoPr` (코퍼스 29), `hh:memoProperties` (코퍼스 2), `hh:metaTag` (코퍼스 0), `hh:numbering` (코퍼스 47), `hh:numberings` (코퍼스 6), `hh:object` [모델 미등재] (코퍼스 2), `hh:overlapBothAllowOverlap` (코퍼스 1), `hh:paraHead` (코퍼스 470), `hh:paragraph` [모델 미등재] (코퍼스 2), `hh:refList` (코퍼스 9), `hh:rightBorder` (코퍼스 108), `hh:section` [모델 미등재] (코퍼스 2), `hh:slash` (코퍼스 108), `hh:styles` (코퍼스 8), `hh:substFont` (코퍼스 48), `hh:tabItem` (코퍼스 88), `hh:tabPr` (코퍼스 57), `hh:tabProperties` (코퍼스 8), `hh:topBorder` (코퍼스 108), `hh:trackChangeAuthors` (코퍼스 0), `hh:trackChanges` (코퍼스 0), `hh:trackchageConfig` (코퍼스 8), `hh:treatQuotationAsLatin` (코퍼스 1), `hh:typeInfo` (코퍼스 253), `hh:useInnerUnderline` (코퍼스 1), `hh:useLowercaseStrikeout` (코퍼스 1)

**hc:** 28개

`hc:ax1` (코퍼스 0), `hc:ax2` (코퍼스 0), `hc:center` (코퍼스 0), `hc:color` [렌더러만] (코퍼스 4), `hc:end1` (코퍼스 0), `hc:end2` (코퍼스 0), `hc:endPt` (코퍼스 0), `hc:extent` (코퍼스 0), `hc:fillBrush` (코퍼스 83), `hc:gradation` (코퍼스 2), `hc:imgBrush` (코퍼스 0), `hc:intent` (코퍼스 1515), `hc:left` [이름충돌] (코퍼스 1515), `hc:next` [이름충돌] (코퍼스 1515), `hc:prev` (코퍼스 1515), `hc:pt` (코퍼스 120), `hc:pt0` (코퍼스 64), `hc:pt1` (코퍼스 64), `hc:pt2` (코퍼스 64), `hc:pt3` (코퍼스 64), `hc:right` [이름충돌] (코퍼스 1515), `hc:rotMatrix` (코퍼스 184), `hc:scaMatrix` (코퍼스 184), `hc:start1` (코퍼스 0), `hc:start2` (코퍼스 0), `hc:startPt` (코퍼스 0), `hc:transMatrix` (코퍼스 108), `hc:winBrush` (코퍼스 81)

**hhs:** 10개

`hhs:any` (코퍼스 —), `hhs:bodyDiff` (코퍼스 —), `hhs:delete` (코퍼스 —), `hhs:headDiff` (코퍼스 —), `hhs:historyEntry` (코퍼스 —), `hhs:insert` [이름충돌] (코퍼스 —), `hhs:packageDiff` (코퍼스 —), `hhs:position` (코퍼스 —), `hhs:tailDiff` (코퍼스 —), `hhs:update` (코퍼스 —)

**ha:** 3개

`ha:CaretPosition` (코퍼스 —), `ha:ParameterItem` (코퍼스 —), `ha:docdistribute` (코퍼스 —)

**hpf:** 1개

`hpf:PublicKey` (코퍼스 —)

**ocf:** 1개

`ocf:rootfiles` (코퍼스 —)

**odf:** 5개

`odf:algorithm` (코퍼스 —), `odf:encryptiondata` (코퍼스 —), `odf:fileentry` (코퍼스 —), `odf:keyderivation` (코퍼스 —), `odf:startkeygeneration` (코퍼스 —)

**odfc:** 2개

`odfc:configitem` (코퍼스 —), `odfc:configitemset` (코퍼스 —)

**opf:** 11개

`opf:guide` (코퍼스 —), `opf:identifier` (코퍼스 —), `opf:language` (코퍼스 —), `opf:meta` (코퍼스 —), `opf:metadata` [이름충돌] (코퍼스 —), `opf:reference` (코퍼스 —), `opf:site` (코퍼스 —), `opf:spine` (코퍼스 —), `opf:title` [이름충돌] (코퍼스 —), `opf:tour` (코퍼스 —), `opf:tours` (코퍼스 —)

**rdf:** 2개

`rdf:Description` (코퍼스 —), `rdf:type` [이름충돌] (코퍼스 —)

주: 같은 요소라도 HAE의 편집 명령이 없으면 여기 들어간다. 예를 들어 `hp:tbl` 행·열 편집은 없지만 `hp:tbl`은 셀 텍스트 편집과 검증에서 다뤄지므로 이 목록에서 뺐다. 구조 편집 여부는 1절의 TB-04로 본다.

## 4. 미확인 목록

네트워크 실패, 비공개 자료, 근거 부족 때문에 확정하지 못한 것이다. 우선순위 점수 산정에서는 제외했다.

| ID | 항목 | 미확인 이유 | 확인 방법 |
|---|---|---|---|
| U1 | KS X 6101 공식 명세 본문·XSD | 표준 본문은 재배포 금지로 표시돼 있고(S-KSX), 웹 검색 시도는 취소되어 원문을 얻지 못했다. | 표준 원문 열람 경로 확보 후 요소·속성을 대조한다. 원문 내용은 저장소에 복사하지 않는다. |
| U2 | 누름틀(CLICKHERE) 실물 샘플 | 로컬 코퍼스에 누름틀 필드가 없다(H-BACKLOG 기록). 형식은 공개 코드(S-RHWP, S-PYAUTO)에서만 확인했다. | 한컴에서 만든 합성 누름틀 샘플을 열고 저장해 검증한다. 개인 문서는 쓰지 않는다. |
| U3 | 필드 Command 문자열 형식과 하이퍼링크 URL 위치 | rhwp 코드 주석(S-RHWP control.rs)의 'Clickhere:set:…' 형식과 한컴 원본 예시를 인용했으나 독립 확인은 안 했다. HYPERLINK의 Command 값 형식은 확인하지 못했다. | 합성 샘플에서 한컴이 저장한 값과 비교한다. |
| U4 | hp:hyphen 요소명 | OWPML C++ 모델에 없다. hwpxlib 커밋 메시지(S-HWPXLIB f9fd225, 이슈 33)에 <hp:hyphen>과 <hp:hypen> 명칭 충돌이 기록돼 있다. | 한컴이 저장한 하이픈 컨트롤 샘플로 확인한다. |
| U5 | OWPML C++ 모델 스냅샷의 완전성 | 모델 스냅샷은 2023-10-24 커밋이다. 코퍼스에는 모델에 없는 이름이 있다(hh:layoutCompatibility 하위 char·field·object·paragraph·section, hh:head, hs:sec, hp:switch·case·default). 모델을 현행 표준의 정본으로 단정할 수 없다. | 최신 스펙 또는 XSD와 교차 비교한다(U1 연계). |
| U6 | Dayoooun/hwpx-mcp 도구 수 | README 제목은 77개, 저장소 소개는 125개, 도구 표 행 집계는 81개로 서로 다르다. 기능 실행은 하지 않았다. | 저장소를 클론해 도구 등록 목록을 세고, 실행 여부를 확인한다. |
| U7 | python-hwpx-automation 도구 수 | 도구 계약 파일 기준 기본 128개, 고급 포함 136개다. A1 보고는 128개였다. 기능 실행은 하지 않았다. | 계약 파일의 도구 목록과 실제 등록을 대조한다. |
| U8 | hancom-mcp 도구 수 | README에 수치가 없다. A1 보고는 20개였다. 이 조사에서 직접 세지 않았다. | 저장소를 클론해 등록 목록을 센다. |
| U9 | HWP5 계열 경쟁 구현의 세부 기능 | hwpers·unhwp·hwplib는 README 또는 기존 보고 수준만 확인했다. H2Orestart(GPL-3.0)와 hwp2hwpx는 기존 보고만 있다. H2Orestart는 클린룸 정책(H-CLEANROOM §1)상 코드 참조 전에 라이선스 검토가 필요하다. | HWP5 착수 전에 사용 범위와 라이선스를 검토하고, 필요하면 코드를 직접 읽는다. |
| U10 | 필드 종류의 OWPML 명칭 | C++ 모델에 fieldBegin의 type 열거가 없다(검색 0건). TOC·CROSSREF·FORMULA 명칭은 rhwp·python-hwpx-automation 코드와 코퍼스에서만 확인했다. 코퍼스 실물은 HYPERLINK, MEMO, PROOFREADING_MARKS_*다. | 한컴 저장 샘플을 열어 type 값을 확인한다. |
| U11 | Windows 빌드·실행 상태 | 이 조사에서 빌드하지 않았다. 코드에는 Windows용 파일 입출력(crates/hae-project/src/io/windows.rs)이 있다. 내부 보고 H-GAP §6.1 P3(빌드 장애)과 상충할 수 있다. 최근 커밋 7e008c1("Windows build milestone")의 내용도 확인하지 않았다. | Windows에서 빌드하고 결과를 기록한다. |
| U12 | 작업 트리의 미커밋 변경 | 판정은 HEAD 7e008c1 기준이다. 미커밋 변경(document.stats, section.list, note.list, 필드·메모·쪽 설정 집계 함수, 새 테스트 readonly_queries.rs)은 판정에서 뺐고 '작업 중'으로 표기했다. 작성자가 커밋하면 판정이 바뀐다. | 커밋 후 판정을 다시 확인한다. |
| U13 | 한컴 SDK 공개 Action 목록 | Action 명칭을 대조하는 데만 썼다. SDK는 설치·실행하지 않았다. 공개 소개 문구의 '완벽한 호환' 같은 표현은 근거로 쓰지 않았다. | 명칭 대조 외의 사실은 주장하지 않는다. |
| U14 | 캐시 기준과 최신 커밋의 차이 | airmang/python-hwpx는 캐시 73d68a7 기준이다. GitHub 최신 커밋은 45b9541(2026-10-18)로 확인했다. hwpx-mcp·hwpers·unhwp는 캐시가 없어 README와 커밋 조회만 했다. | 해당 저장소를 최신 커밋으로 맞춘 뒤 재확인한다. |
| U15 | OWPML 요소로 보이지만 모델에 없는 이름 | hp:memo, hp:hyperlink, hp:table(실제는 hp:tbl), hp:trackChange, hp:formObject, hp:videoData, hp:docOption은 C++ 모델에 요소로 없다. 메모와 하이퍼링크는 fieldBegin의 type 값으로 표현되는 것으로 추정한다(코퍼스 MEMO·HYPERLINK 필드로 간접 확인). 추정이다. | 스펙 또는 XSD로 확인한다(U1). |
| U16 | 코퍼스 0건 요소 | footNote, endNote, bookmark, equation, chart, ole, textart, autoNum, indexmark, trackChange 등은 10개 코퍼스 파일에 없다. 각주·머리말 실물은 내부 입력(spikes/work/stage4-e2e/inputs/04_complex.hwpx)에 있다는 내부 기록만 있고 이 조사에서 다시 열지 않았다. | 합성 샘플과 내부 입력으로 확인한다. |
| U17 | text.replace로 제어 문자 삽입 가능 범위 | 탭·줄바꿈 등 제어 문자를 text.replace로 넣을 수 있는지 코드 읽기만 했고 실행하지 않았다. | 시험을 실행해 확인한다. |
| U18 | 수식 스크립트 노출 여부 | element.get 출력에 수식 스크립트가 들어가는지 확인하지 못했다. | 수식이 있는 합성 샘플에서 element.get 출력을 확인한다. |
| U19 | DVC JID 217개의 허용값 의미 | JID의 숫자 정의만 확인했다. 규칙 JSON(jsonFullSpec.json)의 키 의미와 허용값은 검증하지 않았다. 이 파일은 DVC 샘플 규칙이다. | 규칙 JSON 사양을 읽고 소규모 샘플로 검증한다. |
| U20 | MCP 출력 스키마 문제 | 내부 보고(H-GAP §6.1 P4)를 옮겼다. 코드 재확인은 하지 않았다. | descriptor를 확인한다. |
| U21 | python-hwpx-automation의 누름틀 생성 코드 | handlers/form_fill.py 코드를 읽기만 했다. 실행 확인은 하지 않았다. | 합성 입력으로 실행해 본다. |
| U22 | 한컴 공개 소개 문구 | documents.sdk.hancom.com/hwp/intro의 'HWPX 생성·편집', '100% 지원' 같은 문구는 마케팅 수준이다. 기능 범위 판단 근거로 쓰지 않았다. | 기능 근거는 Action 목록과 명세로 대조한다. |
| U23 | 매크로·스크립트 보관 항목의 이름과 구조 | OWPML 요소가 아니다. 'Scripts/*' 같은 항목 이름은 이 조사에서 확인하지 못했다. 근거는 DVC JID_MACRO_* 3건과 내부 기록뿐이다. | 매크로가 있는 합성 HWPX를 만들어 ZIP 항목을 확인한다. |

미확인으로 표시한 기능 행: MT-15, MT-17, IO-04, IO-05. 작업 중 행: PG-01, PG-03, MT-04.

## 5. 출처

외부 자료는 공개된 것만 썼다. 비공개 SDK는 열지 않았고 개인 문서도 쓰지 않았다. 아래 표의 리비전은 이 조사 시점의 로컬 캐시 또는 GitHub 조회 결과다.

| 코드 | 자료 | 리비전·조회일 | 위치 (URL·경로·줄) | 사용 범위 |
|---|---|---|---|---|
| S-OWPML | hancom-io/hwpx-owpml-model (한컴 OWPML C++ 모델, Apache-2.0) | 커밋 1453388 (2023-10-24) | https://github.com/hancom-io/hwpx-owpml-model · 로컬 ~/.cache/cga/repos2/hancom-io_hwpx-owpml-model/OWPML/Class (ID 매크로: Class/MapMacro.h, Class/ClassID.h) | 요소 333개·속성명 183종 사전 (ns·이름·클래스·파일). 속성 목록은 파일명 충돌 위험이 있어 코퍼스로 재확인함 |
| S-DVC | hancom-io/dvc (HWPX 문서 적합성 검증기 DVC, 라이선스 NOASSERTION) | 커밋 19a985e (2023-09-26) | https://github.com/hancom-io/dvc · Source/JsonModel.h (#define JID_* 217개), README.md, sample/jsonFullSpec.json | 규칙 카테고리와 JID 개수 (MT-14, MT-03) |
| S-POL | PolarisOffice/polaris_dvc (DVC의 Rust 이식·확장) | 커밋 d635741 | https://github.com/PolarisOffice/polaris_dvc · docs/hwpx-validation-scope.md, docs/jid-registry.md, standards/README.md, crates/polaris-dvc-core/src/jid_registry.rs | 검증 축 계획, JID 레지스트리 217개, 표준 자료 재배포 제한 |
| S-KSX | KS X 6101 (OWPML 표준) | 미확보 | standards/README.md (polaris_dvc): 재배포 금지 명시 | 미확인 (U1). 내용을 사용하지 않음 |
| S-HWPXLIB | neolord0/hwpxlib (Java OWPML 오브젝트 모델) | 커밋 f9fd225 (2026-08-31) | https://github.com/neolord0/hwpxlib · src/main/java/kr/dogfoot/hwpxlib/object/content/**, 커밋 메시지(이슈 33) | 클래스 목록으로 요소 존재 확인 (ctrl, object, paragraph/t, header_xml/references) |
| S-HWPLIB | neolord0/hwplib (HWP5 Java 라이브러리) | 커밋 6746c27 | https://github.com/neolord0/hwplib · README 및 기존 보고(A2) | HWP5 읽기 경쟁 근거 (IO-01). 코드 세부는 확인 안 함 (U9) |
| S-PYHWPX | airmang/python-hwpx (순수 Python HWPX 라이브러리) | 캐시 73d68a7 (GitHub 최신 45b9541, U14) | https://github.com/airmang/python-hwpx · README.md (무엇을 하나 78–81행, 알려진 제약 약 163–176행, HWP 5.0 열기 안내 약 100행) | 편집·생성·도형·그림 효과·변경추적·목차 기능 범위, 한글이 못 여는 파일 경고 |
| S-PYAUTO | airmang/python-hwpx-automation (MCP·자동화 계층) | 커밋 5093ce1 | https://github.com/airmang/python-hwpx-automation · docs/tool-contract.generated.json (도구 136개, 기본 128개), src/hwpx_automation/handlers/form_fill.py:404–470 | 도구 이름 근거 대부분, 누름틀 생성·채우기 코드 (FD-02, FD-03) |
| S-HWPMCP | treesoop/hwp-mcp (MCP 서버, .hwp·.hwpx 대상) | 커밋 4a93489 | https://github.com/treesoop/hwp-mcp · README.md (34개 도구), 도구 등록 이름 | 필드·표·이미지·텍스트 스타일 도구 이름 근거 |
| S-HWPXMCP | Dayoooun/hwpx-mcp (MCP 서버, HWPX 대상) | 커밋 1c8a5ec (2026-10-17, GitHub API 조회. 캐시 없음) | https://github.com/Dayoooun/hwpx-mcp · README.md (도구 표) | 각주·책갈피·하이퍼링크·메모·머리말 도구 이름. 도구 수 불일치 (U6) |
| S-HANCOMMCP | gridi-ai/hancom-mcp (MCP 서버, HWP·HWPX) | 커밋 6f24e7d | https://github.com/gridi-ai/hancom-mcp · src/ (함수 정의 목록) | 문서 생성, 표 삽입, 이미지 삽입, 스타일 정의 근거 |
| S-KORDOC | chrisryugj/kordoc (HWP·HWPX → Markdown 변환, MCP 17개) | 커밋 82a1d21 | https://github.com/chrisryugj/kordoc · README.md | Markdown 변환 근거 (MT-05). 자체 보고 수치는 확인 안 함 |
| S-UNHWP | iyulab/unhwp (Rust, HWP 3/5·HWPX → Markdown) | 커밋 e55b340 (2026-10-14) | https://github.com/iyulab/unhwp · README.md | HWP5·HWPX 읽기 및 Markdown 변환 근거 |
| S-HWPERS | Indosaram/hwpers (Rust, HWP5 읽기·쓰기·레이아웃 렌더) | 커밋 4097f71 (2026-09-06) | https://github.com/Indosaram/hwpers · README.md (구현·미구현 목록) | HWP5 쓰기, 도형·차트·수식 미구현 명시 |
| S-RHWP | edwardkim/rhwp (Rust HWP/HWPX 엔진, MIT) | 커밋 1a76570e | https://github.com/edwardkim/rhwp · src/parser/hwpx/section.rs:5496–5569 (필드 종류 매핑), src/model/control.rs:919–1016, 1147–1152 (Clickhere 명령 형식) | 필드 종류 명칭, 누름틀 명령 형식 (U2, U3, U10). 코드는 복사하지 않음 |
| S-SDK | 한컴 한글 SDK 공개 문서 (documents.sdk.hancom.com) | 2026-10-18 조회 | https://documents.sdk.hancom.com/hwp/api/groups/action-list · https://documents.sdk.hancom.com/hwp/intro | 공식 Action 명칭 대조만 (U13, U22). SDK는 설치하지 않음 |
| S-DEV | 한컴 개발자 사이트 (developer.hancom.com) | 2026-10-18 조회 | https://developer.hancom.com (hwpx-owpml-model을 공개 OWPML 모델로 소개) | 공개 모델 위치 확인 |
| S-ORG | hancom-io 조직 공개 저장소 목록 | 2026-10-18 조회 (GitHub API) | https://api.github.com/orgs/hancom-io/repos | HWPX 관련 저장소 확인: hwpx-owpml-model(C++, Apache-2.0, 2023-10-24), dvc(C++, NOASSERTION, 2023-09-26), metatag-ex(C++, Apache-2.0, 2023-01-12, 내용 미확인), hwpx-contents-extract(Java, Apache-2.0, 2022-05-20, 내용 미확인) |
| H-CODE | HAE 저장소 HEAD 코드 | 커밋 7e008c1 (2026-10-18, 브랜치 wip/stage1-model-edit) | crates/hae-exec/src/registry.rs, crates/hae-model/src/{lib,para,query}.rs, crates/hae-edit/src/{char_format,format,image,replace_all}.rs, crates/hae-verify/src/{text,lib}.rs, crates/hae-mcp/src/lib.rs, crates/hae-render-rhwp/src/pages.rs, crates/hae-exec/tests/editable_verified.rs (줄 번호는 HEAD 기준) | 모든 HAE 지원 판정 |
| H-WORKTREE | HAE 작업 트리 미커밋 변경 | git status·git diff (2026-10-18). 수정 5개 파일 795줄 추가, 미추적 1개 | registry.rs, session.rs, descriptions.rs, query.rs, user-guide.md, tests/readonly_queries.rs | '작업 중(미커밋)' 표기 근거만. 판정에는 쓰지 않음 (U12) |
| H-BACKLOG | 0.3 기능 백로그 | docs/superpowers/plans/2026-10-18-0.3-feature-backlog.md (커밋 7e008c1 기준) | 파동 1–5 목록, 필드 구현 전 확인 사항 | 백로그 ID 매핑, 진행 상태, 코퍼스 필드 기록 |
| H-GAP | 경쟁 분석·기능 격차 보고서 | docs/research/competitive-gap-analysis.md (커밋 기준 HEAD) | §1.2, §2.1, §4.1–4.2, §5.1–5.3 (G1–G20), §6.1–6.3 | 격차 항목, 확인된 강점, 내부 결함 기록. 이 조사에서 재실행하지 않은 내부 결과는 '기록'으로 표시 |
| H-MATRIX | 경쟁 매트릭스 | docs/research/competitive-matrix.csv | 행별 메모, batch_atomic 열 | 경쟁 구현 실행 결과 요약 근거 |
| H-VERIFY | HWPX 저장 검증 규칙 | docs/decisions/hwpx-verification-rules.md | §2.1–2.8 (검사 규칙), §3 (검증 범위 한계) | 보존 계약과 검증 규칙 근거 |
| H-VOCAB | HWPX 어휘 조사 (코퍼스 10개 구조 통계) | docs/research/hwpx-vocabulary.md | §1 요소-부모 쌍 표 | 코퍼스 빈도 교차 확인 |
| H-CORPUS | 코퍼스 10개 파일 직접 집계 | tests/fixtures/corpus/*.hwpx (10개), 이 조사에서 header.xml과 section*.xml을 집계 | /tmp/opencode/corpus_counts.json (저장소 밖 임시 파일) | 요소 등장 횟수, 속성 이름 검증 (예: hp:tc@borderFillIDRef, hp:margin 속성 방식) |
| H-CLEANROOM | 클린룸·재사용 기록 | docs/decisions/clean-room.md | §1.1–1.3 | 공개 자료 사용 범위, GPL 코드 참조 경계 |

근거 없는 항목은 넣지 않았다. 확인하지 못한 주장은 4절에 올렸다.

