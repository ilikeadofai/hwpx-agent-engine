# 진단 코드 목록 (구현 계획 1-5)

작성: 2026-10-16, 총괄. 설계서의 코드와 hae-diag `Code` enum은 이 표와 1:1이어야 한다. "추가"는 설계서에 이름이 없지만 구현에 필요해 총괄이 정한 코드다. 설계서를 개정할 때 이 표도 같이 고친다.

| 코드 | 설계서 | 단계 | 뜻 |
|---|---|---|---|
| PACKAGE_REJECTED | §11 | 1 | 열기 단계 패키지 거절. reason에 사유 |
| RESOURCE_LIMIT | §8·§11 | 1 | 한도 초과(크기·항목 수·해제량·깊이·화소·메모리·이력) |
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
| INVALID_ARGUMENT | 0.2 §4 | 1 | text/paragraphs 누락·동시 입력 또는 문단 배열 길이 불일치. detail.paragraphCount는 실제 문단 수 |
| RANGE_CROSSES_CONTROL | §6 | 1 | 범위가 탭·줄바꿈·필드·개체 경계를 지남 |
| UNSUPPORTED_CONTENT_IN_RANGE | §6 | 1 | 범위 안 텍스트 외 자식·해석 불가 속성 |
| CONTROLS_PRESERVED | 0.2 §3 | 1 | (경고) 범위 내 개체·탭·줄바꿈 유지. detail.count는 유지 수, detail.offsets는 그 개체들의 변경 후 문단 grapheme 오프셋 배열(원래 순서) |
| FORMAT_COLLAPSED | §6 | 1 | (경고) 여러 글자 모양이 하나로 합쳐짐 |
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
문단 text.replace의 `no_text_span`, `empty_paragraph_insert_unsupported`, 공통
`document_readonly`·노드 잠금 사유, paragraph.format의 `no_header`·`no_paraPr`·`paraPr_shape`,
셀의 `merged_cell`·`cell_paragraph_count`·`nested_table`·`no_run`·`range_crosses_control`·
`unsupported_content_in_range` 등이 포함된다. 셀 전체 교체 거절은 자손 문단의 교체 거절을
뜻하지 않으며 `childParagraphsEditable`과 자손 문단의 `edits`를 확인한다.


0.2 §7-1: preserve 범위 안 필드 시작/끝(하이퍼링크 포함)은 RANGE_CROSSES_CONTROL(reason:field_boundary_in_range)로 거절한다. 개체 전용 문단의 유효한 경계 위치는 길이 0 textSpans로 광고한다.
