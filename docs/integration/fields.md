# 통합 조각: 누름틀 필드 (`field.list`·`field.setText`) — 파동 1

작업 브랜치 `feat/fields`, 시작 HEAD `109e4d7`(WIP). 공유 파일은 이 브랜치에서 **직접** 편집했고, 아래에 무엇이 들어갔는지 적는다. 총괄이 파동 2 이후를 합칠 때 충돌 지점으로 쓴다.

## 1. `crates/hae-exec/src/registry.rs`

- `FieldListParams { scope, cursor, limit }`, `FieldSetTextParams { target, text, name, fieldid, occurrence }`, `FieldListResultShape { documentId, revision, fields, nextCursor }`.
- `ParsedCommand::FieldList/FieldSetText`, `define_commands!` 항목 두 개.
  - `field.list`: category Query, `mutates:false, doc:true, ver:false, seq:false, dry:false, hist:false`.
  - `field.setText`: category Mutation, `mutates:true, doc:true, ver:true, seq:true, dry:true, hist:true`.
- schema 시험 목록(`schema_tests`)에 `field_list`, `field_set_text` 두 줄 추가.

## 2. `crates/hae-exec/src/session.rs`

- `ParsedCommand::FieldList` 갈래: scope 조회 + 커서 페이지 나누기.
- `ParsedCommand::FieldSetText` 갈래: `handle_edit_steps`로 위임.
- `handle_edit_steps` 안의 `"field.setText"` 파싱 갈래(EditOp + VerifyOperation).
- `cursor_offset(cursor, revision, what)` 헬퍼 추가. `document.inspect`도 이 헬퍼를 쓰도록 정리했다(문구만 `invalid document.inspect cursor ...`로 일반화).
- `add_op_bytes`에 `VerifyOperation::FieldSetText` 바이트 계산.

## 3. `crates/hae-mcp/src/descriptions.rs`

- `field_list`, `field_set_text` 설명 추가, `TABLE`에 두 줄 추가.
- `text_replace`/`cell_set_text`/`command_batch` 설명에 필드 관련 문구 보강(교차 거절, batch 목록에 `field.setText`).

## 4. `docs/user-guide.md`

- "누름틀·하이퍼링크 필드 조회와 텍스트 채우기 (0.3)" 절 추가(조회·커서·편집·거절 사유·검증 근거·한계).
- 1절 "할 수 있는 것"에 `field_list`·`field_set_text` 한 줄 추가.

## 5. `docs/decisions/diagnostic-codes.md`

- 새 코드 없음. 필드 reason 문자열 목록 한 줄 추가.

## 6. 그 밖

- `crates/hae-model/src/fields.rs`: `CLICK_HERE`/`CLICKHERE`/`HYPERLINK` 지원.
- `crates/hae-verify/src/fields.rs`: 같은 표기 지원(독립 구현, hae-edit·hae-model 미의존).
- 시험: `crates/hae-exec/tests/fields.rs`(19), `crates/hae-exec/tests/fields_real.rs`(5, 신규), `crates/hae-verify/tests/fields.rs`(12), `crates/hae-mcp/tests/equivalence.rs`(+1).
