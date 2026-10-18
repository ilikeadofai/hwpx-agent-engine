# field.list / field.setText 구현 및 검증 기록 (2026-10-18)

작업 기준: `feat/fields`, 시작 HEAD `109e4d7`(공급자 한도로 중단된 WIP 커밋). 모든 저장소 작업은 `/tmp/opencode/wt-fields`에서 수행했으며 다른 기능 체크아웃이나 개인 문서는 읽지 않았다. push·다른 브랜치 병합은 하지 않는다.

이 문서는 WIP 커밋의 기록을 이어받아 **정정·추가 검증·최종 검사 결과**를 담는다.

## 정정: 실제 누름틀(`CLICK_HERE`) 표본은 로컬에 있다

앞선 조사(`docs/superpowers/plans/2026-10-18-0.3-feature-backlog.md`)는 "CLICKHERE(누름틀) 필드는 로컬 어디에도 없다"고 적었다. 이는 **합성 픽스처가 쓴 표기(`CLICKHERE`)만 찾은 결과**다. 실제 한컴이 저장한 누름틀의 `type` 값은 **`CLICK_HERE`**이며, 저장소에 공개 서식으로 들어와 있는 다음 두 문서에 실물 36개가 있다.

| 문서 | 출처 | 누름틀 수 | 셀 내부 |
|---|---|---|---|
| `tests/fixtures/corpus/gani-gian.hwpx` | chrisryugj/kordoc `templates/간이기안문_서식.hwpx`, MIT | 13 | 11 |
| `tests/fixtures/corpus/ilban-gian.hwpx` | chrisryugj/kordoc `templates/일반기안문_서식.hwpx`, MIT | 23 | 0 |

실제 마크업은 빈 값 필드이며 `fieldid` 속성이 없다.

```xml
<hp:run charPrIDRef="1"><hp:ctrl><hp:fieldBegin id="1700000001" type="CLICK_HERE" name="행정기관명" editable="1"><hp:parameters cnt="1" name=""><hp:stringParam name="Command">Clickhere:set:48:Direction:wstring:5:행정기관명 HelpState:wstring:0:  </hp:stringParam></hp:parameters></hp:fieldBegin></hp:ctrl><hp:ctrl><hp:fieldEnd beginIDRef="1700000001"/></hp:ctrl></hp:run>
```

따라서 구현을 **`CLICK_HERE`(실물) + `CLICKHERE`(합성 픽스처) + `HYPERLINK`** 세 표기로 넓혔고, `fieldid` 선택자는 XML의 `fieldid` 또는 `id` 중 하나와 일치하면 통과한다(실물에는 `id`만 있다). 이 정정 없이는 실제 기안문의 누름틀이 전부 `unsupported_field_type`으로 거절됐을 것이다.

## 구현

- `field.list`: 문서 또는 paragraph/table/cell/section scope; 시작 문단 nodeId, type/name/fieldid/id, Command 해석 값과 원시 내부 XML, 현재 텍스트, empty/multiParagraph/nested/inCell, paragraphIds, 지원 여부와 사유. **`limit`(기본 100, 최대 500)과 `cursor`/`nextCursor`(revision:index)로 나눠 읽으며, 커서의 revision이 현재와 다르면 `VERSION_CONFLICT`로 거절**하고 형식 오류는 `INVALID_REQUEST`, 목록 밖 인덱스는 빈 쪽과 null 커서다. 커서 파싱은 `document.inspect`와 같은 헬퍼를 쓴다.
- `field.setText`: CLICK_HERE/CLICKHERE/HYPERLINK의 표시 텍스트만 변경. 이름·fieldid(또는 id)·명시적 occurrence로 지목한다. 복수 일치에 occurrence가 없으면 AMBIGUOUS_TARGET; 범위 밖/없는 필드는 TARGET_NOT_FOUND. 마커·속성·파라미터·URL은 보존한다.
- 기존 text.replace의 실제 fragment 치환 계획을 재사용한다. 첫 carrier에 새 글을 넣고 나머지 조각을 비운다. 빈 필드는 기존 run의 안전한 위치에 hp:t를 넣을 수 있으나 모델 nodeId는 추가하지 않는다.
- 다문단·중첩·짝 불일치·MEMO/교정부호·개체/탭·CDATA/주석/PI·불투명 마크업은 사유를 붙여 거절한다. expectedVersion/dryRun/batch/재전송/Undo/Redo는 공통 executor 경로를 사용한다.
- `hae-verify/src/fields.rs`는 hae-edit/hae-model 의존 없이 요청과 원본 XML만으로 필드 선택·값 영역·기대 바이트를 독립 재도출한다. 캐시·PrvText·원시 압축 데이터·로컬 헤더의 기존 보존 관문을 그대로 통과한다.
- 새 진단 코드는 없으며 기존 AMBIGUOUS_TARGET/TARGET_NOT_FOUND/UNSUPPORTED_CONTENT_IN_RANGE/INVALID_REQUEST/VERSION_CONFLICT를 사용한다. reason 문자열과 MCP/registry/사용 안내서를 갱신했다.

## 합성 입력과 새 시험

`tests/fixtures/editing/gen_fields.py --check-determinism`은 두 번 생성해 같은 바이트임을 확인한다. 주요 픽스처 fields.hwpx의 SHA-256:
`054206f541f390e0301261934adf2734c8768f13d448b2226d9579fc28a94495`(변경 없음).

필드 17개: 빈 값/값 있는 CLICKHERE, HYPERLINK, 다문단, MEMO, 중첩, 중복 이름, 무명, 탭, 수식, CDATA/주석/PI, 표 셀, 서로 다른 글자 모양의 run. 이 입력은 경계 시험 자료이며 네이티브 유효성을 보장하는 문서가 아니다.

### crates/hae-exec/tests/fields.rs (19개)

WIP의 18개에 커서 시험을 더했다.

- `field_list_pages_with_a_revision_bound_cursor` — limit/nextCursor로 17개를 나눠 읽어 전체 목록과 일치, limit 0/과대값 clamp, 목록 밖 커서는 빈 쪽, 형식 오류 INVALID_REQUEST, 다른 revision 커서 VERSION_CONFLICT, 편집 뒤 커서 revision 갱신.

나머지 18개는 WIP와 같다(빈 값·다문단·중첩·scope·마커/파라미터 보존·fieldid/id 지목·occurrence·거절 사유·dryRun·Undo/Redo 바이트·batch·carrier 없는 빈 필드·self-closing·run 분배·비텍스트 payload·버전/재전송).

### crates/hae-exec/tests/fields_real.rs (5개, 신규)

공개 기안문 실물 두 문서를 임시 루트로 복사해 쓴다(커밋된 파일은 건드리지 않는다).

- `real_gani_gian_lists_click_here_fields_inside_table_cells` — 13개 전부 `CLICK_HERE`, `setTextSupported` 13/13, 셀 내부 11개, `fieldid`는 null이고 `id`만 있음, Command 원문 접두 `Clickhere:set:`.
- `real_gani_gian_fill_keeps_markers_parameters_and_restores_bytes` — 셀 안 두 필드를 채우고, 시작·끝 마커와 파라미터 원문이 바이트 동일함을 확인, 미편집 ZIP 항목의 압축 바이트·로컬 헤더 보존, Undo 2회로 원본 바이트 완전 복원, Redo 2회로 편집 바이트 복원.
- `real_ilban_gian_pages_lists_and_fills_a_body_paragraph` — 23개를 limit 10 커서로 3쪽에 나눠 읽어 전체와 일치, scope로 문단 지정, 본문 채우기, Undo 원본 복원.
- `real_ilban_gian_rejects_missing_out_of_range_and_dry_run_edits` — 없는 이름·틀린 fieldid·순번 밖은 TARGET_NOT_FOUND, `id`로 지목한 dryRun은 revision을 올리지 않음.
- `real_gani_gian_batch_is_atomic_and_one_undo_unit` — batch 두 필드가 한 Undo 단위, 실패 step이 있으면 전체 취소.

### crates/hae-verify/tests/fields.rs (12개 = 합성 7 + 실물 5)

실물 5개는 요청·후보를 원본 XML에서 손으로 만들고 hae-edit/hae-model을 import하지 않는다.

- `field_gate_accepts_real_template_candidate`(ilban-gian, 값·공백 값), `field_gate_accepts_real_template_cell_fill`(gani-gian, 표 셀 안)
- `field_gate_rejects_real_template_marker_mutation`(beginIDRef 변조), `field_gate_rejects_real_template_parameter_mutation`(Command 파라미터 1바이트 변조), `field_gate_rejects_real_template_unrelated_cell_text`(무관한 셀 라벨 변조, 단독·결합 모두 거절)

합성 7개 중 6개가 변조 거절이다. **변조 거절 시험은 합성 6 + 실물 3 = 9건**이다(요구 5건 이상).

### crates/hae-mcp/tests/equivalence.rs (1개, 신규)

- `four_interfaces_agree_on_field_set_text` — gani-gian.hwpx에서 `field.list`(limit 5) → `field.setText` dryRun → 실제 채우기 → element.query → export → Undo → export를 in-process·`hae script`·`hae-jsonl`·`hae-mcp` 네 경로에서 실행해 단계별 결과·내보낸 바이트·Undo 복원 바이트가 모두 일치함을 확인한다.

## 실물 문서 검증 결과 (2026-10-18)

로컬 CLI(`hae script`)로 두 서식에 값을 채우고 `document.export`(독립 관문 통과)한 산출물이다. 로컬 산출물/전사: `spikes/work/fields/`(gitignore 대상).

| 파일 | SHA-256 |
|---|---|
| gani-gian-input.hwpx | `2b58ef61f74a58126e125841d3850c5769bb642e29d3a2e9e3fb67cf5c58ee29` |
| gani-gian-filled.hwpx | `5833f9a49ed43d9caf8643417d0bb56de651eae92716462e7975f5c1e2f2439a` |
| ilban-gian-input.hwpx | `7d40e770a018ae571ab10b5745376b03fd5ffa65fceaf5cce0d87cd0e4d4f437` |
| ilban-gian-filled.hwpx | `70ea6745bbae898840653126f336f5fae25e7e088ddbb76b1038c78872b56ac5` |

채운 값: gani-gian `생산등록번호=12345`, `결재직위1=과장 홍길동`, `제목=2026년 업무계획 보고`; ilban-gian `행정기관명=○○교육지원청`, `본문=2026학년도 업무계획을 다음과 같이 보고합니다.`

확인한 성질(별도 Python 대조):

- 바뀐 ZIP 항목은 `Contents/section0.xml` 하나뿐이다.
- 필드 마커와 파라미터 문자열이 바이트 동일하다(gani-gian 26개, ilban-gian 46개 마커 문자열 전수 비교).
- 새 값이 두 문서 XML에 존재하고, `field.list` 재조회 값도 일치한다.
- Undo 뒤 내보낸 바이트가 원본과 완전히 같다(테스트로 고정).

## 한컴 Linux Beta 열기 게이트

실행 파일 경로는 `/tmp/opencode/hoffice-run/opt/hnc/hoffice11/Bin`, 실행 스크립트는 이 워크트리의 `scripts/hancom_gate.sh`다. 기본 대기 시간은 15초다.

| 파일 | 판정 |
|---|---|
| gani-gian-input.hwpx | PASS(생존) |
| **gani-gian-filled.hwpx** | **PASS(생존)** |
| ilban-gian-input.hwpx | PASS(생존) |
| **ilban-gian-filled.hwpx** | **PASS(생존)** |
| fields-simple-filled.hwpx(합성) | PASS(생존) |

게이트 종료 코드는 0이다(로그 `spikes/work/fields/hancom-real.log`, `hancom-real2.log`).

- 복잡 합성 픽스처 `fields.hwpx`와 그 편집본은 **원본도 편집본도 FAIL(rc=139)**이다. 편집에서 생긴 회귀가 아니며, 그 픽스처는 의도적으로 불완전한 수식·불투명 마크업을 담은 문법 시험 자료라 네이티브 유효성을 보장하지 않는다. 실물 검증은 위 두 공개 서식으로 한다.
- 화면 캡처 시도: `spikes/work/fields/vnccap.py`(표준 라이브러리 최소 RFB 클라이언트)로 `QT_QPA_PLATFORM=vnc` 프레임버퍼를 받으려 했으나, 플러그인이 전체 화면 rect를 예고한 뒤 픽셀 데이터를 보내지 않아 **시각 대조는 완료하지 못했다**. 따라서 이 절의 PASS는 **프로세스 생존 판정일 뿐**이며 내용·조판 대조가 아니다.
- **실제 사용자 공문서로는 검증하지 못했다.** 위 실물은 공개 배포된 서식 표본(MIT)이고, Windows 한컴오피스 2024에서의 열기·조판도 확인하지 않았다.

## 전체 검사

최초 check.sh는 기존 editable_verified 시험의 픽스처 수 고정값(18)에 새 픽스처가 추가되어 19와 불일치한 실패를 기록했다. 고정값과 진단 출력 개수를 갱신하고 새 픽스처까지 기존 범위 속성 시험에 포함했다.

기존 미리보기 시험 네 건은 최초 환경에서 renderer_missing으로 실패했다: page_limit_is_a_partial_result_with_diagnostic, preview_after_an_edit_is_tied_to_the_new_revision, preview_returns_png_and_full_metadata, svg_is_opt_in_and_checked. 시험을 생략하지 않고 이 워크트리의 gitignore 경로 spikes/vendor/rhwp-src/target/release/rhwp에 공식 v0.8.7 Linux x86_64 배포 실행 파일을 준비했다. 배포 압축 파일 SHA-256은 upstream SHA256SUMS와 일치하는 `f1e541a25cfe76b6bc1daf18bf9c1f9a5ac5e03e2c01b188daaf54107214f631`, 실행 파일 SHA-256은 `6d1ccdfb5522d13cb9f6134357aeb718f705454709cf52627ccc127e249c3277`이다. 과거 로컬 재빌드 바이너리 e91cd25f...와 동일한 바이너리라고 주장하지 않는다.

최종 `bash scripts/check.sh` 결과: **종료 0**. fmt·Clippy(`-D warnings`)·hae-raster 빌드·사용량 도우미 시험·workspace 전체 시험이 통과했다(로그 `check-final.log`).
