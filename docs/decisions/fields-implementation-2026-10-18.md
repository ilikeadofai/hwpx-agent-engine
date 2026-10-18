# field.list / field.setText 구현 및 검증 기록 (2026-10-18)

작업 기준: `feat/fields`, 시작 HEAD `58dfd21`. 모든 저장소 작업은 `/tmp/opencode/wt-fields`에서 수행했으며 다른 기능 체크아웃이나 개인 문서는 읽지 않았다. push·다른 브랜치 병합은 하지 않는다.

## 구현

- field.list: 문서 또는 paragraph/table/cell/section scope; 시작 문단 nodeId, type/name/fieldid/id, Command 해석 값과 원시 내부 XML, 현재 텍스트, empty/multiParagraph/nested/inCell, paragraphIds, 지원 여부와 사유.
- field.setText: CLICKHERE/HYPERLINK의 표시 텍스트만 변경. 이름·fieldid 또는 id·명시적 occurrence로 지목한다. 복수 일치에 occurrence가 없으면 AMBIGUOUS_TARGET; 범위 밖/없는 필드는 TARGET_NOT_FOUND. 마커·속성·파라미터·URL은 보존한다.
- 기존 text.replace의 실제 fragment 치환 계획을 재사용한다. 첫 carrier에 새 글을 넣고 나머지 조각을 비운다. 빈 필드는 기존 run의 안전한 위치에 hp:t를 넣을 수 있으나 모델 nodeId는 추가하지 않는다.
- 다문단·중첩·짝 불일치·MEMO/교정부호·개체/탭·CDATA/주석/PI·불투명 마크업은 사유를 붙여 거절한다. expectedVersion/dryRun/batch/재전송/Undo/Redo는 공통 executor 경로를 사용한다.
- hae-verify/src/fields.rs는 hae-edit/hae-model 의존 없이 요청과 원본 XML만으로 필드 선택·값 영역·기대 바이트를 독립 재도출한다. 캐시·PrvText·원시 압축 데이터·로컬 헤더의 기존 보존 관문을 그대로 통과한다.
- 새 진단 코드는 없으며 기존 AMBIGUOUS_TARGET/TARGET_NOT_FOUND/UNSUPPORTED_CONTENT_IN_RANGE 등을 사용한다. reason 문자열과 MCP/registry/사용 안내서를 갱신했다.

## 합성 입력과 새 시험

`tests/fixtures/editing/gen_fields.py --check-determinism`은 두 번 생성해 같은 바이트임을 확인한다. 주요 픽스처 fields.hwpx의 SHA-256:
`054206f541f390e0301261934adf2734c8768f13d448b2226d9579fc28a94495`.

필드 17개: 빈 값/값 있는 CLICKHERE, HYPERLINK, 다문단, MEMO, 중첩, 중복 이름, 무명, 탭, 수식, CDATA/주석/PI, 표 셀, 서로 다른 글자 모양의 run. 이 입력은 경계 시험 자료이며 네이티브 유효성을 보장하는 문서가 아니다.

새 시험 25개 = executor 통합 18개 + 편집기 비의존 검증 7개. 검증 시험 중 변조 거절 6개는 마커·이름·다른 필드 값·Command 파라미터·미편집 항목 재인코딩·요청 없는 header 변경이다. 최초 executor 시험 12개는 UNKNOWN_COMMAND로 실패한 것을 확인하고 구현했다. 빈 carrier 없는 경우 및 self-closing 공백/불투명 run payload의 실패도 확인한 뒤 수정했다.

### crates/hae-exec/tests/fields.rs (18개)

- `field_list_empty_multiple_values_and_command_raw`
- `field_list_nested_cell_and_multi_paragraph_are_distinct`
- `field_list_scope_paragraph_cell_table_section_and_missing_scope`
- `field_set_success_escapes_preserves_markers_runs_and_raw_zip`
- `field_set_empty_and_nameless_by_fieldid_or_id`
- `field_set_duplicate_name_requires_occurrence_and_checks_bounds`
- `field_set_rejects_outside_paragraph_and_unknown_target`
- `field_set_rejects_multi_paragraph_and_nested`
- `field_set_rejects_objects_tabs_and_lexical_crossings`
- `field_set_memo_read_only_hyperlink_and_cell_writable`
- `field_set_dry_run_undo_redo_exact_bytes`
- `field_set_batch_atomic_and_one_undo_unit`
- `field_set_without_text_carrier_uses_existing_run_and_keeps_node_ids`
- `field_set_self_closing_empty_carriers_preserve_attributes`
- `field_set_first_empty_carrier_keeps_first_shape`
- `field_set_rejects_non_text_run_payload`
- `field_set_requires_version_noop_does_not_record_history`
- `field_set_sequence_replay_is_idempotent_and_conflicts_are_rejected`

### crates/hae-verify/tests/fields.rs (7개)

- `field_gate_accepts_independently_handwritten_candidate`
- `field_gate_rejects_marker_mutation`
- `field_gate_rejects_name_mutation`
- `field_gate_rejects_other_field_value_mutation`
- `field_gate_rejects_unrequested_header_change`
- `field_gate_rejects_command_parameter_mutation`
- `field_gate_rejects_untouched_entry_reencoding`

## 한컴 Linux Beta 열기 게이트

실행 파일 경로는 `/tmp/opencode/hoffice-run/opt/hnc/hoffice11/Bin`, 실행 스크립트는 이 워크트리의 `scripts/hancom_gate.sh`다. 기본 대기 시간은 15초다.

- 복잡 fields.hwpx 원본과 이 원본에서 만든 fields-filled.hwpx: 둘 다 FAIL(종료 rc=139). 편집 산출물에서만 발생한 회귀로 판정하지 않는다. 정확한 네이티브 크래시 원인은 미확정이다.
- 비교 기준 split_runs.hwpx: PASS(생존).
- `gen_fields.py --gate-output`으로 별도 생성한 CLICKHERE 두 개의 단순 합성 입력 fields-simple-input.hwpx: PASS(생존).
- 단순 입력을 실제 CLI에서 field.list → field.setText 두 번 → document.export한 fields-simple-filled.hwpx: PASS(생존), 스크립트 종료 0. 두 변경 문자열의 XML 존재와 시작·끝 마커 4개의 속성/파라미터 원문 바이트 동일을 따로 확인했다.

단순 입력 SHA-256: `2f7aae72168b0169a73918c6ab4252b11f0c6ec4f41bd705c7ede90c76ef9a41`.
편집 산출물 SHA-256: `af286681a684766c307143fe91b4c19da85dd04166675fcbe319f7e61966f5ed`.
로컬 산출물/전사/로그: `spikes/work/fields/` (gitignore 대상). 재생성 예:

```bash
cd /tmp/opencode/wt-fields && mkdir -p spikes/work/fields
cd /tmp/opencode/wt-fields && python3 -I tests/fixtures/editing/gen_fields.py --check-determinism --gate-output spikes/work/fields/fields-simple-input.hwpx
```

CLI 세션 스크립트는 입력을 document.open한 뒤 field.list 결과를 `saveAs: "fields"`로 보관하고, `$fields.fields.0.nodeId`와 fieldid `f1`/`f2`로 `누름틀 합성 검증`/`필드 값 변경`을 각각 채워 document.export했다. 로컬 전사 경로: `spikes/work/fields/gate-simple-transcript.json`.

게이트 PASS는 프로세스 생존 결과이다. **화면 내용·조판을 대조하지 않았고 Windows 한컴 호환성을 검증하지 않았다. 실제 CLICKHERE가 든 공문서/양식 검증은 입력 부재로 미수행**이다. 합성 시험 통과를 실물 검증으로 집계하지 않는다. 중첩 등 위험한 필드 채우기는 문서화한 안전 거절이며 기능 성공 수로 집계하지 않는다.

## 전체 검사

최초 check.sh는 기존 editable_verified 시험의 픽스처 수 고정값(18)에 새 픽스처가 추가되어 19와 불일치한 실패를 기록했다. 고정값과 진단 출력 개수를 갱신하고 새 픽스처까지 기존 범위 속성 시험에 포함했다.

최종 전체 검사 결과는 아래에 기록한다.

기존 미리보기 시험 네 건은 최초 환경에서 renderer_missing으로 실패했다: page_limit_is_a_partial_result_with_diagnostic, preview_after_an_edit_is_tied_to_the_new_revision, preview_returns_png_and_full_metadata, svg_is_opt_in_and_checked. 시험을 생략하지 않고 이 워크트리의 gitignore 경로 spikes/vendor/rhwp-src/target/release/rhwp에 공식 v0.8.7 Linux x86_64 배포 실행 파일을 준비했다. 배포 압축 파일 SHA-256은 upstream SHA256SUMS와 일치하는 `f1e541a25cfe76b6bc1daf18bf9c1f9a5ac5e03e2c01b188daaf54107214f631`, 실행 파일 SHA-256은 `6d1ccdfb5522d13cb9f6134357aeb718f705454709cf52627ccc127e249c3277`이다. 과거 로컬 재빌드 바이너리 e91cd25f...와 동일한 바이너리라고 주장하지 않는다. 준비 후 hae-exec --test preview 7/7 통과를 확인하고 전체 check.sh를 다시 실행했다.
