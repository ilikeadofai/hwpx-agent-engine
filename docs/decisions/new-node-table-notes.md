# 새 표 객체 생성 — editor/verifier 노트 (2026-10-19)

상태: 구현 파일 작성 완료, 아직 `lib.rs` 배선·Cargo 실행·한컴 게이트는 부모 작업이다.
이 문서는 완료 주장이 아니라 두 독립 구현이 따르는 계약과 남은 불확실성의 기록이다.

## 1. 소유 범위

이 작업이 만든 파일은 다음 셋뿐이다.

- `crates/hae-edit/src/table_insert.rs`
- `crates/hae-verify/src/table_insert.rs`
- `docs/decisions/new-node-table-notes.md`

공유 `lib.rs`, 모델/열거형/registry/session, 캐리어 문단·splice·remap·캐시 로직은
부모 작업이 소유한다. 두 모듈은 아직 `mod` 선언이 없으므로 부모가 배선하기 전에는
컴파일 대상이 아니다. 이 작업은 Cargo를 실행하지 않았고, 단위 시험은 배선 후
`cargo test -p hae-edit -p hae-verify table_insert`로 처음 실행된다.

## 2. 정확한 API

두 모듈은 같은 시그니처를 각자 구현한다. verifier는 editor의 직렬화 결과나
`hae-edit`/`hae-model`에 의존하지 않는다.

```rust
pub(crate) fn build(
    state: &hae_package::PackageState,
    xml: &hae_xml::XmlDoc,
    paragraph: usize,
    rows: u32,
    columns: u32,
    width: u32,
    row_height: u32,
    cells: &[Vec<String>],
) -> hae_diag::Result<Vec<u8>>
```

- `state`/`xml`은 **요청 이전** 패키지와 대상 구역 문서다. verifier도 사전 상태를
  받아야 하며, 결과 패키지를 넘기면 새 객체의 ID를 다시 세어 다른 바이트를 만든다.
- `paragraph`는 `xml` 안에서 대상 `hp:p`(또는 `hp10:p`)의 element index다. 범위 밖이거나
  문단이 아니면 거절한다. 캐리어 문단/런은 부모가 만든다.
- 반환값은 `<hp:tbl>...</hp:tbl>` 바이트뿐이다. XML 선언, 캐리어 `hp:p`/`hp:run`,
  splice 범위, `TableRemap`, `NodeRemap`, lineseg/캐시 변경은 포함하지 않는다.
- `cells`는 빈 배열이면 전부 빈 격자, 아니면 정확히 `rows`개의 내부 배열을 받고
  각 내부 배열은 정확히 `columns`개의 문자열을 가진다(`cells[r][c]`).

## 3. 검증 경계

할당 전에 다음을 검사한다.

- `rows > 0`, `columns > 0`.
- `rows * columns <= 1024` (`u64`로 계산, 셀 크기 할당 전).
- `1 <= width <= 1_000_000`, `1 <= row_height <= 1_000_000` (HWPUNIT).
- `columns <= width`: 각 열에 최소 1 HWPUNIT이 배정되어야 한다.
- `cells`는 0개이거나 정확히 `rows`개이고, 각 행은 정확히 `columns`개다.
- 표 전체 높이는 `rows * row_height`를 checked `u64`로 계산한다. 각 치수가
  1,000,000 이하이고 행이 1024 이하이므로 합은 `u32`에 들어가며, `u32` 변환이
  실패하면 거절한다. HWPUNIT 상한 1,000,000은 요청 치수(`width`, `rowHeight`)에
  적용되고, 표 `hp:sz height`는 그 checked 합이다.
- 셀 문자열은 탭/개행/CR 금지, XML 문자만 허용, 그 밖의 제어 문자 금지.
  이는 새 문단 텍스트 규칙과 같은 방향이며, 더 엄격하게 `char::is_control`도 거절한다.
- `paragraph` index 범위와 대상 원소가 HWPX 문단인지 확인한다.

거절은 요청 형식 오류에 `InvalidArgument`/`Validate`, 소스 리소스 부재에
editor는 `Unsupported`/`Validate` + reason을 쓴다. verifier의 소스 리소스 오류는
저장 게이트 문맥에 맞춰 `VerifyFailed`/`Save` + 같은 reason으로 표현하고,
요청 형식 오류는 editor와 같은 `InvalidArgument`/`Validate`를 쓴다.

## 4. 치수와 셀

비병합 직사각형만 만든다.

- 열 너비: `share = width / columns`, `remainder = width % columns`.
  왼쪽 `remainder`개 열이 `share + 1`, 나머지는 `share`. 합은 정확히 `width`이고
  모든 열이 1 이상이다.
- 행 높이: 모든 행이 요청한 `row_height`.
- `hp:sz`: `width`, `height = rows * row_height`, `widthRelTo`/`heightRelTo`는
  `ABSOLUTE`.
- 각 `hp:tc`: `cellAddr`는 0부터의 실제 행/열, `cellSpan`은 1/1,
  `cellSz`는 해당 열 너비와 `row_height`.
- 셀 여백은 표 `inMargin`과 `tc cellMargin` 모두 0, `hasMargin="0"`.
  작은 치수에서도 여백이 셀 크기를 넘지 않게 하기 위한 결정이다.
- 셀 문단은 `subList` 아래 하나씩 두고, 빈 문자열은 `<hp:t/>`로 쓴다.
  `linesegarray`는 만들지 않는다(레이아웃 캐시는 부모/한컴이 다시 계산).

## 5. 네이티브 기본값 (고정)

공개 표본(`tests/fixtures/corpus/*.hwpx`)과 `document.create` 헤더를 기준으로
다음을 고정한다. 두 구현이 바이트 단위로 같아야 한다.

```text
hp:tbl
  id=<new shape id> zOrder="0" numberingType="TABLE"
  textWrap="TOP_AND_BOTTOM" textFlow="BOTH_SIDES" lock="0"
  dropcapstyle="None" pageBreak="CELL" repeatHeader="0"
  rowCnt colCnt cellSpacing="0" borderFillIDRef=<existing id> noAdjust="0"
  hp:sz width=<width> widthRelTo="ABSOLUTE"
        height=<rows*row_height> heightRelTo="ABSOLUTE" protect="0"
  hp:pos treatAsChar="1" affectLSpacing="0" flowWithText="1"
         allowOverlap="0" holdAnchorAndSO="0" vertRelTo="PARA"
         horzRelTo="PARA" vertAlign="TOP" horzAlign="LEFT"
         vertOffset="0" horzOffset="0"
  hp:outMargin left="0" right="0" top="0" bottom="0"
  hp:inMargin  left="0" right="0" top="0" bottom="0"

hp:tr (rows)
  hp:tc name="" header="0" hasMargin="0" protect="0"
        editable="0" dirty="0" borderFillIDRef=<same existing id>
    hp:subList id="" textDirection="HORIZONTAL" lineWrap="BREAK"
               vertAlign="TOP" linkListIDRef="0" linkListNextIDRef="0"
               textWidth="0" textHeight="0" hasTextRef="0" hasNumRef="0"
      hp:p id=<new paragraph id> paraPrIDRef=<existing id>
           styleIDRef=<existing id> pageBreak="0" columnBreak="0" merged="0"
        hp:run charPrIDRef=<existing id>
          hp:t (escaped text, or <hp:t/> when empty)
    hp:cellAddr colAddr=<c> rowAddr=<r/>
    hp:cellSpan colSpan="1" rowSpan="1"
    hp:cellSz width=<column width> height=<row_height>
    hp:cellMargin left="0" right="0" top="0" bottom="0"
```

`zOrder="0"`을 쓰는 이유: 새 표는 텍스트 흐름 안의 비병합 표이고, 기존 표본에는
`zOrder="0"`도 흔하다. 별도 z-order 카운터를 추측하지 않는다. 겹침 우선순위가
문제가 되는 배치는 이 명령의 범위가 아니다.

`treatAsChar="1"`/`horzRelTo="PARA"`를 쓰는 이유: 이 명령은 보고서의 순차 삽입
지점에 들어가는 인라인 표를 만든다. MCP 이전 스크립트(`hwpx_utils.py::table_xml`,
`build.py::Builder.table`)는 `treatAsChar="1"` + `horzRelTo="PARA"`를 쓴다.
공개 표본 `research-school-plan-reference.hwpx`, `problem-answer-reference.hwpx`의
표는 `treatAsChar="0"` + `horzRelTo="COLUMN"`인 떠 있는 표이고,
`gyehoek-reference.hwpx`에는 `treatAsChar="1"` 표도 있다. 순차 삽입에는 인라인
쪽이 맞고, 이를 뒤집을 만한 코퍼스 근거를 찾지 못했다. 한컴 게이트에서 인라인
배치를 확인한다.

## 6. 스타일·테두리 참조 해석

소스 헤더(`Contents/header.xml`, 없으면 basename이 `header.xml`인 항목)를 파싱해
`hh:borderFill`, `hh:charPr`, `hh:paraPr`, `hh:style`의 `id`를 문서 순서로 모은다.

- id는 비어 있지 않은 ASCII 숫자여야 하고 `u32`여야 한다. 중복 id는 모호하므로 거절.
- `hh:style`은 `type="PARA"`인 것만 쓴다. `type="CHAR"`인 스타일은 문단
  `styleIDRef` 대상이 아니므로 건너뛰고, PARA 스타일이 없으면 추측하지 않고
  `table_insert_style_resource_unavailable`로 거절한다.
- 네 종류(borderFill/charPr/paraPr/PARA style) 모두 최소 하나가 있어야 한다.
  없으면 새 정의를 만들지 않고 같은 reason으로 거절한다.
- 테두리는 왼/오/위/아래 `type`이 모두 `NONE`이 아닌 첫 borderFill을 선호하고,
  없으면 첫 borderFill을 쓴다. **새 borderFill을 합성하지 않는다.**
  `document.create` 헤더에는 `NONE` 테두리 하나뿐이므로 그 문서의 새 표는
  보이는 테두리가 없다. 이는 의도된 보수적 결과이며, 필요하면 `document.create`
  쪽에서 유효한 borderFill을 제공하는 것이 올바른 확장이다.
- style은 문서 순서 첫 `type="PARA"` `hh:style`. 그 style의
  `charPrIDRef`/`paraPrIDRef`가 실제 정의와 맞으면 그것을, 아니면 첫
  `charPr`/첫 `paraPr`을 쓴다.
- 모든 참조 문자열은 원문 표기를 보존한다(앞의 0 포함). 숫자만 허용하므로
  속성 이스케이프가 필요 없다.

## 7. ID 정책

- 모양 id(`hp:tbl` 등)와 문단 id(`hp:p`)는 별도 공간으로 센다.
- 모양 id 스캔 대상은 paragraph namespace의
  `tbl,pic,equation,ole,container,chart,textart,shape,group,rect,ellipse,line,arc,polygon,curve,connectLine,button,cellzone,video`
  19개다. `hp:video`도 포함해 기존 동영상 개체 ID를 재사용하지 않는다.
- 대상 구역 `xml`과 `state`에서 이름이 `.xml`로 끝나고 파싱되는 **모든** 항목을
  스캔해 각 공간의 최대값 + 1을 쓴다. 파일명이 `sectionN.xml`이 아니어도 다른
  구역/본문 XML이 가진 문단·개체 ID를 놓치지 않는다. 파싱되지 않는 항목은
  유효한 객체를 담을 수 없으므로 건너뛴다.
- 새 표 id 하나, 셀 문단 id를 행 우선으로 `rows * columns`개 연속 배정한다.
- `u32::MAX`를 넘기면 `table_insert_id_exhausted`로 거절한다.
- `zOrder`는 스캔하지 않고 0으로 둔다. 모양/문단 id만 "고유 결정적" 계약이다.
- 부모의 캐리어 `hp:p`는 `id`를 생략하므로 이 모듈이 배정한 셀 문단 id와
  충돌하지 않는다. 이 모듈은 캐리어를 만들지 않는다.

## 8. 네임스페이스

- 대상 문단의 scope(root → 문단)에서 `xmlns:prefix` 선언을 모아, 문단 네임스페이스
  (`http://www.hancom.co.kr/hwpml/2011/paragraph` 또는 `.../2016/paragraph`)에
  묶인 **비어 있지 않은** prefix를 재사용한다. `hp`가 있으면 `hp`를 우선하고,
  없으면 사전순 첫 prefix를 쓴다.
- 기본 네임스페이스만 문단 네임스페이스에 묶인 경우는 이름 없는 요소를 내보내지
  않도록 명시적으로 거절한다(`table_insert_namespace_unavailable`).
- 출력의 모든 요소 이름은 그 prefix를 쓰고, 속성은 모두 unprefixed다.
  새 네임스페이스 선언이나 다른 prefix를 합성하지 않는다.
- 텍스트는 `hae_xml::escape_text`로 이스케이프한다(`&`, `<`, `>`).

## 9. 독립 구현

editor와 verifier는 같은 계약을 각자 구현한다. 서로의 함수·바이트·계획을
호출하거나 import하지 않는다(`hae-verify`는 `hae-edit`에 의존하지 않는다).
두 파일의 단위 시험은 같은 1×1 기대 바이트를 각자 하드코딩해, 한쪽만 바뀌면
각자의 시험이 실패하도록 했다. 두 구현의 출력이 같아야 하는 이유는 계약이지
공유 직렬화기가 아니다.

## 10. 참고 자료와 MIT 고지

구조·속성 순서·"보이는 borderFill 선호" 아이디어는 아래 MIT 자료를 참고했다.

- `~/.codex/skills/hwpx-workbench/scripts/hwpx_workbench/vendor/hwpx_utils.py`
  의 `table_xml`
- 같은 스킬의 `scripts/hwpx_workbench/build.py`의 `Builder.table` /
  `Builder.table_border`
- 원저작: `ilikeadofai/hwpx-document-processing-skill`,
  commit `94f948cf90c5b37c01dc6e09069992ab09adb412`, MIT.
  스킬의 `THIRD_PARTY_NOTICES.md`와 `licenses/ilikeadofai-MIT.txt` 참조.

이 작업은 Rust로 새로 작성했고 템플릿을 통째로 복제하지 않았다. 특히 다음은
의도적으로 채택하지 않았다.

- 전체 XML 재직렬화 워크플로(`lxml` 트리 재작성): 이 엔진은 바이트 보존 splice를 쓴다.
- 없는 borderFill/style의 합성(`Builder.table_border`의 새 정의 생성): 금지.
- `max(1000, width // col_count)`처럼 열 너비 합이 요청 폭과 달라질 수 있는 계산:
  나머지 배분으로 합을 정확히 맞춘다.
- 고정 `row_h = 2400`, 고정 여백 510/141: 요청 `rowHeight`와 0 여백을 쓴다.

## 11. 남은 불확실성 / 부모 확인 항목

1. **한컴 게이트 미실행.** 두 구현 모두 Cargo/한컴으로 아직 검증하지 않았다.
   `treatAsChar="1"`, `zOrder="0"`, 0 여백, `hp:t` self-closing, `header="0"`이
   실제 한컴에서 문제없는지는 게이트 결과로만 확정된다.
2. **style 필수 결정.** `hh:style`이 없는 `anchor_char_height.hwpx` 같은 표본에서는
   새 표를 거절한다. 이 표본의 기존 문단은 정의 없는 `styleIDRef="0"`을 쓰지만,
   "참조는 해석되어야 한다"는 요구에 따라 새 정의/추측 참조를 만들지 않았다.
   `type="CHAR"`만 있는 헤더도 PARA 문단 스타일이 없으므로 거절한다. 부모가 이
   표본을 대상으로 시험한다면 계약을 완화할지 결정해야 한다.
3. **캐리어 문단 id 충돌 없음.** 부모 캐리어는 `id`를 생략한다(§7).
4. **XML 파싱 실패 항목.** ID 스캔은 `.xml`로 끝나는 모든 항목을 시도하고
   파싱되지 않는 항목은 건너뛴다. 유효한 패키지에서는 `analyze`가 먼저
   거절하므로 실무 차이는 없다.
5. **사전 상태 전달.** verifier에 결과 패키지를 넘기면 ID가 달라진다. 반드시
   요청 이전 `state`/`xml`을 넘긴다.
6. **문단 내부 삽입 가정.** prefix는 대상 문단 scope에서 해석한다. 부모가 표를
   문단 밖 형제로 옮기면서 문단 자체에만 선언된 prefix를 쓰면 바인딩이 사라질 수
   있다. 현재 계획(캐리어 문단 안의 run)에서는 안전하다.
