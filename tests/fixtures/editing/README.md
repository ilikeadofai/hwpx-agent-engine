# tests/fixtures/editing

HWPX 엔진 0.1 시험 자료 2차(**구현 계획 1-2b**). 편집 경계 자료를 고정한다.
`tests/fixtures/` 의 코퍼스(1-2a 관찰)·악성 ZIP/XML 자료와는 별개이며, 편집 명령
(작업 1-8 조회, 1-10 text.replace, 1-11 paragraph.format·cell.setText, 1-12 image.replace)
의 입력·기대 변경·거절 코드를 편집 구현 시작 전에 표로 확정하는 것이 목적이다.

## 원칙

- 전부 **합성**이다. 개인 문서·제3자 자산을 쓰지 않는다. 교체용 이미지는 이 합성기가
  직접 만든 바이트(JPEG 는 자체 생성 1x1 grayscale)다.
- 패키지 골격(mimetype STORED 첫 항목, `META-INF/container.xml`·`Contents/content.hpf`·
  `Contents/section0.xml` 배치)은 `tests/fixtures/adversarial/valid_base.hwpx` 에서 그대로
  가져오고 사례마다 구역·manifest·미디어만 바꾼다. 그 파일과 나머지 시험 자료는 수정하지 않는다.
- `Contents/header.xml` 만은 골격을 그대로 쓰지 않고 **합성기가 최소·정확한 기본 머리말**을
  만든다. 실제 HWPX 처럼 `fontfaces`, `borderFills`(id=1), `charProperties`(id=0..3),
  `paraProperties`(`paraPr id=0`, 정렬 + 퍼센트 줄간격)를 정의해 구역의
  `paraPrIDRef="0"`·`charPrIDRef`·`borderFillIDRef="1"` 참조가 모두 풀리게 한다. 빈
  `refList` 로 두면 `hae-verify` 가 `unresolved paraPrIDRef: 0` 으로 거절해 CLI/MCP
  편집이 막힌다(직접 `hae-edit` 단위 시험만 통과하던 틈). 모양 근거는
  `tests/fixtures/corpus/bodojaryo-reference.hwpx` 와 공개 모델 `OWPML/Class/Head` 이며
  한컴 바이트를 그대로 옮기지 않는다. 예외로 `para_format.hwpx` 는 문단 모양 복제를
  시험하므로 해석 불가 속성(`x:keep`)·자식(`x:untouched`)을 가진 `paraPr` 를 그대로 쓴다.
- 무작위 `nodeId`(설계서 §5) 때문에 기대표의 대상은 ID 가 아니라 **구조 선택자**로 적는다.
- 실제 엔진으로 결과를 확인했지만, 이 자료의 확정 주체는 총괄이며 `expected.json` 이 계약이다.

## 구성

| 경로 | 내용 |
|---|---|
| `gen_editing.py` | 합성기(Python 3 표준 라이브러리 전용, 결정적). 사례마다 결함/구조를 생성 시점에 assert 하고, 자소 군집·코드포인트·검색 oracle 로 기대 텍스트를 검산하며, 머리말 참조(`paraPr`·`charPr`·`style`·`borderFill`) 해소도 확인한다. |
| `*.hwpx` | 편집 경계 패키지 17개(약 42KB). 각 패키지에 합성 기본 머리말(`header.xml`) 포함. |
| `inputs/*` | `image.replace` 검증용 교체 이미지 8개(정적 PNG/JPEG, 거대 IHDR·SOF, APNG, 잘린 이미지, 이미지 아님). |
| `editing_expected.json` | 사례별 `structure`(파서로 계산한 관찰 값)·`operations`(기대 outcome·코드·reason·보존)·`inputs`. |

### 패키지 17개

| 파일 | 분류 | 고정하는 것 |
|---|---|---|
| `split_runs.hwpx` | text | 문단이 글자 모양 다른 run 3개. 범위가 run 경계를 지을 때 첫 글자 모양 사용, `FORMAT_COLLAPSED`, 빈 hp:t·run 유지 |
| `nfc_nfd.hwpx` | text | NFC(U+AC00) vs NFD 자모. 정규화 없이 코드포인트 일치, 자모 하나 검색은 `unalignedMatches` |
| `combining_across_runs.hwpx` | text | 결합 문자(U+0301)가 run 경계를 지나 자소 하나 |
| `zwj_emoji_across_runs.hwpx` | text | ZWJ 이모지(👨‍👩)가 run 경계를 지남. UAX#29 GB11 |
| `char_refs_cdata.hwpx` | text | `&#xAC00;`·`&amp;`·`&lt;`·CDATA 혼합. 원문 표기 보관, 참조/ CDATA 범위 교체 |
| `tabs_control.hwpx` | text | hp:t 안의 tab·lineBreak·fwSpace, run 레벨 fieldBegin. `RANGE_CROSSES_CONTROL`·`UNSUPPORTED_CONTENT_IN_RANGE` |
| `multi_section.hwpx` | structure | 구역 2개, spine 순서가 구역 번호. 다른 구역 항목 불변 |
| `simple_cell.hwpx` | table | 문단 하나 셀과 빈 셀(자기닫힘 hp:t). `cell.setText` 적용 |
| `merged_cells.hwpx` | table | rowSpan=2 병합 셀. `UNSUPPORTED(merged_cell)`, (row,col) 조회는 병합 영역 전체 |
| `multiparagraph_cell.hwpx` | table | 문단 둘 셀. `UNSUPPORTED(cell_paragraph_count)` |
| `nested_table.hwpx` | table | 중첩 표를 담은 셀. `UNSUPPORTED(nested_table)`, 표별 셀 조회 분리 |
| `opaque_attrs.hwpx` | opaque | 외부 ns 요소가 id 참조/포함 → 대상 잠금(`opaque_reference:<nodeId>`, `contains_opaque`), hp:t 미해석 속성 → `UNSUPPORTED_CONTENT_IN_RANGE` |
| `shared_media.hwpx` | media | 그림 3개가 미디어 2개 공유. 대상 참조만 교체, 공유 미디어·다른 그림 보존 |
| `cropped_picture.hwpx` | media | 자르기(imgClip left≠0) 거절 `UNSUPPORTED(cropped)`, 클립=그림크기는 허용 |
| `effects_picture.hwpx` | media | effects/GRAY_SCALE/bright≠0 거절 `UNSUPPORTED(effects)` |
| `equation.hwpx` | object | 수식은 조회·보존만. `equation_edit_unsupported`, 수식 지나는 범위는 `RANGE_CROSSES_CONTROL` |
| `para_format.hwpx` | format | 문단 모양 복제·재사용, 해석 불가 속성·자식 보존, `resourceChanges` |

### 교체 이미지 8개 (`inputs/`)

| 파일 | 기대 | 코드 / reason |
|---|---|---|
| `ok.png` (2x1 RGB) | accept | 정적 PNG |
| `ok.jpg` (1x1 gray) | accept | 자체 생성 baseline JPEG |
| `huge_ihdr.png` (100000x100000) | reject | `RESOURCE_LIMIT` (화소 10^10 > 40,960,000, 디코딩 전) |
| `huge_sof.jpg` (65535x65535) | reject | `RESOURCE_LIMIT` |
| `apng.png` (acTL) | reject | `UNSUPPORTED` / `animated_image` (프레임 1개라도) |
| `truncated.png` | reject | `UNSUPPORTED` / `invalid_image` |
| `truncated.jpg` (EOI 없음) | reject | `UNSUPPORTED` / `invalid_image` |
| `not_image.gif` | reject | `UNSUPPORTED` / `unsupported_image_format` |

## 재생성

```sh
python3 -I tests/fixtures/editing/gen_editing.py tests/fixtures/editing
```

이 명령은 `*.hwpx` 17개(합성 기본 머리말 포함), `inputs/*` 8개, `editing_expected.json` 을
모두 다시 쓴다. 머리말 정의를 바꾸면 패키지 바이트와 `editing_expected.json` 의
`bytes` 값이 함께 바뀐다(구조·operation 기대는 그대로).

같은 zlib 구현이면 출력 바이트가 같다(두 번 실행해 대조함). 결정성만 빠르게 확인하려면:

```sh
python3 -I tests/fixtures/editing/gen_editing.py --check-determinism
```

두 디렉터리에 실제로 생성해 SHA-256 을 대조하려면:

```sh
rm -rf /tmp/edit_a /tmp/edit_b
python3 -I tests/fixtures/editing/gen_editing.py /tmp/edit_a
python3 -I tests/fixtures/editing/gen_editing.py /tmp/edit_b
diff <(cd /tmp/edit_a && find . -type f | sort | xargs sha256sum) \
     <(cd /tmp/edit_b && find . -type f | sort | xargs sha256sum)
```

## 한도 (설계서 §11, `hae-diag::Limits::default`)

| 이름 | 값 |
|---|---|
| `max_hwpx_bytes` | 32 MiB |
| `max_entries` | 2,048 |
| `max_xml_depth` | 128 |
| `max_image_pixels` | 40,960,000 |
| `max_session_memory` | 512 MiB |

## `editing_expected.json` 규약

- `structure`: 생성기가 파서로 계산한 관찰 값(문단 텍스트·코드포인트·자소 수, 표 셀 주소/병합, 그림 미디어 참조, 잠금 기대). 손으로 적은 값이 아니다.
- `operations[]`: `op`(`text.replace`·`cell.setText`·`paragraph.format`·`command.batch`·`element.query`·`image.replace`), `target`(구조 선택자), `args`.
  - `applied`: `resultText`(자소 oracle `simulate_replace` 로 검산) + `resourceChanges`/`diagnostics`/`preservedRaw`/`preservedEntries`/`preservedRefs`.
  - `reject`: `code` 와 `reason`(또는 접두사 `reasonPrefix`).
  - `element.query`: `matches`(Outline 부분집합)와 `unalignedMatches`.
- 대상 선택자: `{entry, paragraph:N}`(최상위 hp:p 순번), `{table:T,row:R,col:C}`(`hp:cellAddr`), `{picture:N}`, `{equation:N}`, `{opaque:N}`. 0부터 센다.
- `inputs`: `expect`/`code`/`reason`/`format`/`dimensions`.

## 엔진 대조 (참고, 커밋 대상 아님)

생성물을 실제 엔진으로 검증했다. 저장소 밖 임시 crate(opencode 전용, 커밋하지 않음)가
`editing_expected.json` 을 읽어 각 사례를 `hae-model::Document::open` 으로 열고,
구조 선택자를 nodeId 로 풀어 `hae-edit::apply` 로 `operations` 를 그대로 재생했다.

- 17개 패키지 전부 열림(노드 수 기대 일치).
- `image.replace` 입력 8개 + 편집·조회 operation 58개 = **66개 전부 통과(실패 0)**.

이 대조에서 두 가지를 바로잡았다. (1) 실제 HWPX 는 `hp:tab`·`hp:lineBreak`·`hp:fwSpace` 를
`hp:t` **안**에 두므로 탭/컨트롤 자료를 그렇게 고쳤다(코퍼스 관찰). (2) 중첩 표 셀은 문단이
하나여야 `nested_table` 사유가 나온다(`cell_paragraph_count` 가 먼저 걸리기 때문).

엔진을 다시 대조하려면 위와 같은 임시 crate 를 만들어 `editing_expected.json` 을 재생하면 된다.
자료 자체 점검(열림·구조·oracle)은 `gen_editing.py` 실행만으로 항상 수행된다.

## 정책 선택 (총괄이 바꾸면 생성기의 해당 기대만 고쳐 다시 생성)

- 해석 불가 요소는 열 때 수용하되 그 요소가 참조/포함하는 노드를 잠근다. 문서 전체를 읽기 전용으로 만들지 않는다.
- 병합·다문단·중첩 표 셀은 `UNSUPPORTED` 이며 reason 은 각각 `merged_cell`·`cell_paragraph_count`·`nested_table` 이다. 셀 사유 판정 순서는 병합 → 문단 수 → 중첩 표다.
- 수식은 편집하지 않는다(`equation_edit_unsupported`). 자르기·효과 그림도 교체하지 않는다.
- 거대 화소·APNG·잘린 이미지는 각각 `RESOURCE_LIMIT` / `animated_image` / `invalid_image` 로 거절한다.

## 경계

- `image.replace` 의 엔진 구현(1-12)은 다른 담당이 진행 중이다. 여기서는 입력·기대 코드·보존 조건만 고정한다.
- 작업 1-8~1-12 를 시작하기 전에 이 표를 총괄이 확인한다. `paragraph.format`(1-11) 자료도 함께 고정해 두었다.
- 저장·다시 열기(1-13, 1-15)와 변조 oracle(1-14)은 이 디렉터리 범위가 아니다(`tests/oracle/`).

### narrow_edits.hwpx (0.3)

`python3 -I tests/fixtures/editing/gen_narrow_edits.py --check-determinism`으로 생성한다. 개인정보 없는 합성 문서로 기존 스타일 2개, 일반 문단, 머리말 4문단(번호·탭 제어 거절 사례 포함), 꼬리말, 2×1 PNG를 담은 정사각 프레임 그림을 포함한다. 머리말·꼬리말은 실제 HWPX(공개 코퍼스 관찰)처럼 `<hp:ctrl>` 아래에 둔다. 그림은 세 렌더링 행렬을 갖춘 기존 생성기 패턴을 쓴다. ZIP 메타데이터·순서는 고정되고 같은 Python/zlib에서 두 번 생성한 바이트가 동일한지 확인한다. 원본 corpus는 수정하지 않는다. 한컴 Linux 게이트 결과는 `docs/decisions/narrow-edits-hancom-gate-2026-10-18.md`에 있다.
