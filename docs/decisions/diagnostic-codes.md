# 진단 코드 목록 (구현 계획 1-5)

작성: 2026-10-16, 총괄. 설계서의 코드와 hae-diag `Code` enum은 이 표와 1:1이어야 한다. "추가"는 설계서에 이름이 없지만 구현에 필요해 총괄이 정한 코드다. 설계서를 개정할 때 이 표도 같이 고친다.

| 코드 | 설계서 | 단계 | 뜻 |
|---|---|---|---|
| PACKAGE_REJECTED | §11 | 1 | 열기 단계 패키지 거절. reason에 사유 |
| RESOURCE_LIMIT | §8·§11 | 1 | 한도 초과(크기·항목 수·해제량·깊이·화소·메모리·이력). document.exportText는 결과가 Limits.max_text_export_bytes(기본 16 MiB)를 넘으면 reason:text_export_too_large와 detail.{limit,limitBytes,producedBytes}로 거절 text.replaceAll은 바꿀 수 있는 일치가 maxMatches를 넘으면 commit 전에 이 코드로 거절하고 detail.maxMatches를 담음 |
| STATE_INVARIANT_ERROR | §4 | 1 | commit 전 정규 상태와 파생물 불일치 |
| PROJECT_INTEGRITY_ERROR | §5 | 1 | .hwpwork 무결성 검사 실패 |
| PROJECT_LOCKED | §5·§11 | 1 | 프로젝트 잠금 실패 |
| EXTERNAL_MODIFICATION | §11 | 1 | 디스크 파일이 마지막 저장 hash와 다름 |
| SAVE_DURABILITY_UNCERTAIN | §11 | 1 | 공개 뒤 부모 디렉터리 fsync 실패 |
| VERSION_CONFLICT | §6 | 1 | expectedVersion 불일치 |
| HISTORY_EMPTY | §6·§8 | 1 | 되돌리거나 다시 적용할 이력 없음 |
| SNAPSHOT_UNAVAILABLE | §8 | 1 | 해제했거나 없는 스냅샷 |
| SESSION_MISMATCH | §6 | 1 | 다른 sessionId |
| SESSION_REQUIRED | §6 | 1 | 일회성 호출 사이 Undo/Redo |
| UNKNOWN_WRITER | §6 | 1 | 발급하지 않았거나 다른 연결의 writerId |
| REQUEST_ID_CONFLICT | §6 | 1 | 같은 번호·다른 내용 |
| REPLAY_TOO_OLD | §6 | 1 | 이미 지난 번호 |
| REQUEST_OUT_OF_ORDER | §6 | 1 | 건너뛴 번호 |
| TARGET_NOT_FOUND | §6 | 1 | 대상 없음 |
| AMBIGUOUS_TARGET | §6 | 1 | 대상 모호 |
| UNSUPPORTED | §6 | 1 | 지원하지 않는 변경·문서 조건. reason에 사유 |
| INVALID_RANGE | §6·0.2 §2 | 1 | 범위가 문단 밖·역순이거나 빈 문자열 삽입, 삽입 위치가 textSpans 밖 |
| INVALID_ARGUMENT | 0.2 §4 | 1 | text/paragraphs 누락·동시 입력 또는 문단 배열 길이 불일치. detail.paragraphCount는 실제 문단 수. document.exportText는 path 확장자가 format과 맞지 않으면 reason:extension_mismatch |
| RANGE_CROSSES_CONTROL | §6 | 1 | 범위가 탭·줄바꿈·필드·개체 경계를 지남 |
| UNSUPPORTED_CONTENT_IN_RANGE | §6 | 1 | 범위 안 텍스트 외 자식·해석 불가 속성 |
| CONTROLS_PRESERVED | 0.2 §3 | 1 | (경고) 범위 내 개체·탭·줄바꿈 유지. detail.count는 유지 수, detail.offsets는 그 개체들의 변경 후 문단 grapheme 오프셋 배열(원래 순서) |
| FORMAT_COLLAPSED | §6 | 1 | (경고) 여러 글자 모양이 하나로 합쳐짐 |
| MATCHES_SKIPPED | 0.2 §7-2 | 1 | (경고) text.replaceAll이 건너뛴 일치가 있음. detail.count는 총 건너뛴 수, detail.reasons[]는 사유별 {reason, count, locations[{nodeId, range}] 최대 50, locationsTruncated}. 사유: crosses_control(개체·탭·줄바꿈·필드 표식을 가로지름), grapheme_boundary(자소 경계가 맞지 않음), unsupported_markup(CDATA·주석·PI·리터럴 제어문자·해석 불가 속성 등), paragraph_not_editable(잠긴 문단 또는 불투명 요소가 가리키는 문단). range는 명령 전 문단의 grapheme 반열린 구간이며 경계가 맞지 않으면 바깥쪽으로 올린 구간 |
| FORM_FILL_SKIPPED | 추가 | 1 | (경고) form.fill이 strict:false에서 채우지 못한 라벨이 있음. detail.count와 detail.skipped[]({label, reason, detail?}) 참고 |
| RENDERER_UNAVAILABLE | §9 | 3 | 렌더러 없음·격리 미검증. 단계 1·2는 항상 이 코드 |
| PREVIEW_PAGE_LIMIT | §10 | 3 | (경고) 100쪽 초과로 partial |
| INVALID_REQUEST | 추가 | 1 | 요청 형식 오류·정의되지 않은 속성 |
| UNKNOWN_COMMAND | 추가 | 1 | 레지스트리에 없는 명령 |
| PATH_NOT_ALLOWED | 추가 | 1 | 입출력 루트 밖, 링크, 원본·이미지 덮어쓰기 시도 |
| TARGET_EXISTS | 추가 | 1 | 내보내기·첫 저장 대상이 이미 있음 |
| VERIFY_FAILED | 추가 | 1 | 저장 관문(hae-verify) 불통과. 파일을 공개하지 않음 |
| IO_ERROR | 추가 | 1 | 그 밖의 파일 입출력 실패 |

응답 필드(코드가 아님): unalignedMatches, historyEvicted, controlChanged, replay, outcome, savedVersion.

PACKAGE_REJECTED의 reason 값: encrypted, duplicate_name, case_collision, path_escape, zip64_required, bad_mimetype, broken_manifest, dtd_or_entity, non_utf8, malformed_zip, unsupported_compression, multi_disk, crc_mismatch, size_mismatch, header_mismatch, overlapping_entries, malformed_xml, foreign_namespace. 앞의 9개는 설계서 §11, 나머지는 총괄 추가다.

## 렌더러 진단(목록 밖 문자열 코드)

2026-10-17 결정. 렌더러·레이아웃 진단은 preview 결과의 `metadata.diagnostics` 안에 들어가는 문자열 코드이며 엔진 오류 코드(`hae-diag::Code`)가 아니다. 위 엔진 코드 표와 enum의 1:1 계약에 포함하지 않는다. 프리뷰의 렌더러 오류·쪽수 제한 엔진 코드는 기존 `RENDERER_UNAVAILABLE` / `PREVIEW_PAGE_LIMIT`를 유지한다(요청·자원·입출력에 대한 공통 엔진 오류도 기존 계약을 따른다).

아래 목록은 `crates/hae-render-rhwp`와 `crates/hae-exec/src/preview.rs`의 실제 생성·전달 경로를 기준으로 한다.

| 문자열 코드 | 뜻 |
|---|---|
| `LAYOUT_*` | rhwp stderr의 대문자·밑줄 코드인 배치 경고를 그대로 전달하며, `page=N`은 0부터 세는 값을 1부터 세는 쪽 번호로 바꾼다. 고정 enum 목록이 아니다. |
| `LAYOUT_OVERFLOW` | rhwp가 보고한 쪽 배치 넘침(전달 파서 시험에 포함). |
| `LAYOUT_OVERFLOW_DRAW` | rhwp가 보고한 그리기 배치 넘침(전달 파서 시험에 포함). |
| `LAYOUT_OVERFLOW_CELL_LINES` | rhwp 매니페스트의 표 셀 줄 넘침 보고. |
| `SVG_UNSAFE` | 정적 SVG 안전 검사에 실패해 해당 쪽 출력을 보류함. |
| `FONT_UNAVAILABLE` | 참조 글꼴이 작업자에 제공된 글꼴 스냅샷에 없어 대체 글꼴을 사용함. |
| `PAGE_NOT_RENDERED` | 요청한 쪽이 렌더링된 범위 밖에 있음. |
| `PNG_FAILED` | 격리 래스터 작업자가 해당 쪽 PNG를 만들지 못함(SVG는 유지 가능). |
| `PAGE_CONTENT_OMITTED` | 직렬화 응답 한도 때문에 쪽 본문을 응답에서 생략함. |
| `PAGE_CONTENT_ARTIFACT` | 쪽 본문을 설정된 출력 루트 아래 파일로 전달함. |
| `RENDERER_UNTRUSTED_CONTENT` | rhwp 매니페스트가 문서 내용을 신뢰할 수 없다고 표시함. |
| `RENDERER_STDERR` | 배치 코드로 해석되지 않은 비어 있지 않은 stderr 줄의 개수를 알림. |
| `NOTES_TRUNCATED` | 진단 수 한도로 추가 진단을 생략함. |

## 2026-10-17 편집 가능성·범위 재시도 계약

`RANGE_CROSSES_CONTROL`의 `Diagnostic.detail`은 `textSpans: [{start,end}]`와
`textSpansTruncated: bool`을 포함한다. 문단 조회와 같은 extended grapheme cluster의
반열린 오프셋이며 앞의 최대 64개 구간을 제공한다. batch 오류에서는 `failedStep`을
병합해 두 정보를 함께 보존한다. 구간은 실패 단계 직전 후보 상태를 설명하므로,
batch 롤백 뒤 현재 문서에서 재조회한다. 코드는 추가하지 않는다.

개요의 `edits.<command>.reason`과 `notEditableReason`은 진단 코드가 아닌 지원 사유다.
문단 text.replace의 `no_text_span`, `no_run`, 공통
`document_readonly`·노드 잠금 사유, paragraph.format의 `no_header`·`no_paraPr`·`paraPr_shape`,
셀의 `nested_table`·`no_run`·`range_crosses_control`·
`unsupported_content_in_range` 등이 포함된다. 셀 전체 교체 거절은 자손 문단의 교체 거절을
뜻하지 않으며 `childParagraphsEditable`과 자손 문단의 `edits`를 확인한다.


0.2 §7-1: preserve 범위 안 필드 시작/끝(하이퍼링크 포함)은 RANGE_CROSSES_CONTROL(reason:field_boundary_in_range)로 거절한다. 개체 전용 문단의 유효한 경계 위치는 길이 0 textSpans로 광고한다.
0.3 `text.format`은 새 코드를 만들지 않는다. 길이 0·역전·문단 밖 범위는 INVALID_RANGE, CDATA·주석·PI 교차와 해석 불가 속성은 UNSUPPORTED_CONTENT_IN_RANGE, 필드 경계는 field_boundary_in_range, 외부 namespace 요소를 품은 문단은 contains_opaque(UNSUPPORTED)로 0.2와 같은 코드를 쓴다.

0.2 §7-2: text.replaceAll은 건너뛴 일치를 MATCHES_SKIPPED(경고)로, 여러 글자 모양이 합쳐진 일치를 하나의 FORMAT_COLLAPSED(경고, detail.matches)로 알린다. 빈 find·탭/줄바꿈이 든 text는 INVALID_REQUEST, maxMatches가 1..10000 밖이면 INVALID_ARGUMENT, 없는 scope는 TARGET_NOT_FOUND다.

### 수식 편집 reason (0.3)

새 최상위 진단 코드는 추가하지 않습니다. `INVALID_REQUEST`는 빈 script/props, XML 금지 문자, 잘못된 속성 값/limit/cursor에 사용합니다. `UNSUPPORTED`의 `equation_script_notation`은 새 script의 확인되지 않은 역슬래시/큰따옴표/제어·markup 표기 또는 맞지 않는 중괄호, `equation_script_shape`는 원본 script의 누락/중복, `equation_script_markup`은 원본 script의 CDATA·주석·PI·자식 요소·속성을 뜻합니다. stale cursor는 `VERSION_CONFLICT`, 변조 후보는 `VERIFY_FAILED`입니다.

### 0.3 좁은 편집 명령 사유 (기존 코드 사용)

새 진단 코드는 추가하지 않는다. `paragraph.setStyle`의 없는 스타일과 `header.setText`의 없는 구역/머리말/꼬리말/문단은 `TARGET_NOT_FOUND`다. `UNSUPPORTED`의 `style_shape`는 기존 참조 속성 누락·중복 스타일 ID, `image_geometry`는 누락·중복·비정상 기하/행렬 또는 정수 기하 범위 초과, `header_control`은 여백·번호·필드·탭·개체 제어를 담은 머리말/꼬리말 문단을 뜻한다. `no_text_fragment`는 기존 텍스트 조각이 없어 생성 없이 교체할 수 없는 경우다. 잘못된 mode/kind·target/targets 형식은 `INVALID_REQUEST`, 변조 후보는 `VERIFY_FAILED`다.

0.3 `link.list`·`trackchange.list`도 새 코드를 만들지 않는다. 커서(`revision:offset`) 형식 오류는 INVALID_REQUEST, 다른 revision의 커서는 VERSION_CONFLICT, 없는 `scope`는 TARGET_NOT_FOUND를 쓰며 두 명령은 문서를 바꾸지 않는다. 변경 추적 문서의 `readonlyReasons: ["track_changes"]` 잠금은 그대로 유지되고 `trackchange.list`는 조회만 한다.

## 0.3 구역 설정의 reason

새 `Code` enum은 추가하지 않습니다.

| 코드 | reason | 조건 |
|---|---|---|
| `UNSUPPORTED` | `structure_change_unsupported` | 없는 구역/요소 생성이 필요함, secPr 개수·소유 문단이 지원 구조가 아님, scope가 구역 문단이 아님, 본문 영역 검사를 위한 명시 원본 값이 없거나 수치가 아님 |
| `INVALID_REQUEST` | `non_positive_body_area` | 용지·여백·제본을 반영한 본문 폭 또는 높이가 0 이하 |
| `INVALID_REQUEST` | (없음) | 빈 편집, 잘못된 enum/범위/자료형, scope와 sectionIndex 동시 지정 |
| `VERIFY_FAILED` | 기존 검증 reason | 원본과 요청에서 독립 도출한 결과와 후보가 다름. 요청 밖 속성·구역·순서·ZIP 재인코딩 포함 |

0.3 `form.fill`: strict(기본)에서 하나라도 실패하면 아무것도 바꾸지 않고 거절한다. 사유가 모두 label_not_found면 TARGET_NOT_FOUND, 모두 ambiguous_label이면 AMBIGUOUS_TARGET, 그 밖에는 UNSUPPORTED이며 세 경우 모두 `reason: "form_fill_rejected"`와 `detail.failures[]`(라벨별 `{label, reason, detail?}`)를 담는다. strict:false는 성공한 라벨만 적용하고 건너뛴 라벨을 결과 `skipped[]`와 FORM_FILL_SKIPPED(경고)로 보고한다. 라벨별 `reason` 문자열은 진단 코드가 아닌 지원 사유다: `label_not_found`(일치 셀 없음), `ambiguous_label`(일치 셀 2개 이상), `no_cell_geometry`(라벨 셀에 cellAddr 없음), `no_target_cell`(오른쪽·아래 셀 없음), `ambiguous_target`(오른쪽 또는 아래 후보가 2개 이상), `target_not_empty`(대상 셀에 이미 글자 있음), `conflicting_target`(두 라벨이 같은 셀을 가리킴), 그리고 대상 셀의 cell.setText 거절 사유(`nested_cell`·`nested_table`·`cell_paragraph_count`·`no_run`·`verify_unsupported_markup`·`unsupported_content_in_range`·`verify_no_top_level_paragraph`·잠금 사유). 빈 `values`·빈 라벨·여러 줄 라벨·빈 값·탭/줄바꿈이 든 값은 INVALID_ARGUMENT, `scope`가 표·구역이 아니면 UNSUPPORTED(`form_scope`), 없는 scope nodeId는 TARGET_NOT_FOUND다.

0.3 `field.list`·`field.setText`도 새 코드를 만들지 않는다. 조회는 scope가 문단·표·셀·구역이 아니면 INVALID_ARGUMENT, 없는 scope는 TARGET_NOT_FOUND, 커서 형식 오류는 INVALID_REQUEST, 다른 revision의 커서는 VERSION_CONFLICT다. 편집의 선택자 모호는 AMBIGUOUS_TARGET, 선택자 없음·순번 밖은 TARGET_NOT_FOUND이며, 지원하지 않는 필드 종류와 안전하지 않은 값 영역은 UNSUPPORTED_CONTENT_IN_RANGE에 reason을 담아 거절한다. reason 값: unsupported_field_type, multi_paragraph_field, nested_field, unpaired_field, field_lexical_markup, field_contains_control_or_opaque, no_text_fragment, paragraph_not_editable.

## 0.3 표 행·열 추가·삭제(`table.insertRow`·`deleteRow`·`insertColumn`·`deleteColumn`)

새 `Code` enum은 추가하지 않습니다. 설계: [table-structure-node-creation-design.md](table-structure-node-creation-design.md) §9.

| 코드 | reason | 조건 |
|---|---|---|
| `INVALID_ARGUMENT` | `index_out_of_range` | 삽입 `index`가 `0..=줄 수` 밖, 삭제 `index`가 `0..줄 수` 밖이거나 `index+count`가 줄 수를 넘음 |
| `INVALID_ARGUMENT` | `count_out_of_range` | `count`가 `1..=1024` 밖 |
| `INVALID_ARGUMENT` | `table_size_limit` | 결과 행 또는 열 수가 1024 초과 |
| `INVALID_ARGUMENT` | `would_empty_table` | 모든 행(열)을 지우려 함 |
| `UNSUPPORTED` | `merged_cell_blocks_delete` | 삭제 구간과 겹치는 셀 중 `rowSpan`/`colSpan`이 1이 아닌 셀이 있음. `detail.cells[]`({row,col,rowSpan,colSpan}, 최대 16개)·`detail.cellCount` |
| `UNSUPPORTED` | `insert_inside_merged_cell` | 삽입 경계가 병합 셀을 가름. `detail.cells[]` 동일 형식 |
| `UNSUPPORTED` | `no_simple_template_line` | 복제할 단순한 이웃 줄(덮는 모든 셀의 병합 수가 1)이 없음 |
| `UNSUPPORTED` | `cell_template_unsupported` | 복제할 셀에 `subList` 하나·첫 문단·첫 run·`charPrIDRef`가 없음 |
| `UNSUPPORTED` | `delete_content_unsupported` | 삭제할 셀에 글·줄바꿈·탭 외 요소(그림·수식·필드·도형·각주 등). `detail.element`에 요소 이름 |
| `UNSUPPORTED` | `row_would_become_empty` | 열 삭제 뒤 셀이 하나도 없는 행이 생김 |
| `UNSUPPORTED` | `table_cellzone_unsupported` | `hp:cellzoneList`가 있는 표 |
| `UNSUPPORTED` | `table_geometry_unsupported`·`nested_table` | 기존 치수 명령과 같은 전제(격자 완전성, `cellSpacing=0`, `ABSOLUTE` 크기, 열 폭 결정 가능, 중첩 표 없음) 위반 |
| `UNSUPPORTED` | `structure_change_unsupported` | 옛 이름(`table.addRow`·`insertRows`·`removeColumn` 등). 구역 설정 명령의 같은 reason과 별개이며 메시지가 새 명령 이름을 안내함 |
| `STATE_INVARIANT_ERROR` | — | 구조 변경 선언(`TableRemap`)과 후보 문서가 어긋남. 편집기 결함 방어선이며 commit되지 않음 |
| `VERIFY_FAILED` | 기존 검증 reason | 원본과 요청에서 독립 도출한 기대 바이트와 후보가 다름(새 셀 속성·텍스트·주소·크기·캐시·요청 밖 변경 포함) |

대상이 표가 아니면 기존대로 `UNSUPPORTED`/`wrong_kind`, 없는 nodeId는 `TARGET_NOT_FOUND`, 인자 형식 오류(음수 `index` 등)는 `INVALID_REQUEST`입니다. 응답 `structure[]`의 `created`/`removed`/`shifted`는 진단이 아니라 결과입니다.
