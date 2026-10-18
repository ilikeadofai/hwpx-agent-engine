# 문서·문단 생성 — 노드 생성 계약 확장 설계 (0.4 기능 1호, 2026-10-19)

상태: 구현과 함께 확정(브랜치 `feat/create`). 규범 언어는 한국어다.
관련 문서: [표 구조 편집 노드 생성 계약](table-structure-node-creation-design.md)(0.3, 이 문서가 확장하는 선례),
0.1 설계서 §5·§8·§11, [저장 검증 규칙](hwpx-verification-rules.md), [진단 코드](diagnostic-codes.md),
경쟁 분석 D단계 T7(`docs/research/competitive-gap-analysis.md`).

0.3은 "본문 노드를 만들지도 지우지도 않는다"는 0.1의 가정을 **표 하나의 행·열 단위로만** 깼다.
이 문서는 그 계약을 **문단 열(列)과 문서 전체**로 한 단계 더 넓힌다. 여전히 만들지 않는 것
(그림·수식·필드·도형·표·구역·스타일 정의)은 §9에 범위 밖으로 분리해 적는다.

## 1. 결론

| 명령 | 효과 | 만들거나 지우는 노드 | 원본 |
|---|---|---|---|
| `paragraph.insert` | `target` 문단의 `before`\|`after` 위치에 `count`개 문단 삽입 | 문단(`Paragraph`) | 기존 문서 |
| `paragraph.delete` | `target`부터 문서 순서로 `count`개 문단 삭제 | 문단(`Paragraph`) | 기존 문서 |
| `document.create` | 새 HWPX 패키지를 처음부터 만들어 출력 루트에 **새 파일**로 공개 | 문서 전체(구역·문단) | **없음** |

- 삽입 문단은 `target` 문단의 **시작 태그와 첫 `hp:run`의 시작 태그를 복제**해 만든다.
  글은 새로 넣고(선택), `linesegarray` 캐시는 만들지 않는다.
- 삭제는 **글·탭·줄바꿈만 든 문단**만 허용한다. 개체·필드·수식·도형·각주가 든 문단과
  **표를 담은 문단**은 명확히 거절한다(§9).
- `document.create`는 `document.export`와 같은 경로 격리·덮어쓰기 금지 규칙을 따르고,
  **`hae-verify`의 독립 검증을 통과한 뒤에만** 공개한다. 새 문서는 "원본"이 없으므로
  검증 기준 자체를 요청에서 다시 도출한다(§6).
- 세 명령 모두 한컴 열기 게이트를 통과해야 완료다(기록: `document-paragraph-creation-hancom-gate-2026-10-19.md`).

## 2. 기존 불변식과 새 불변식

0.3 §2.2의 I1~I4를 다음처럼 다시 쓴다.

- **I1 (선언된 변경만).** 노드 집합은 일반 편집에서 변하지 않는다. 변할 수 있는 것은
  *구조 명령*이 `NodeRemap`(§3)으로 **미리 선언한** 변화뿐이다. `Document::rebuild`는 계속
  노드 집합 변화를 거절하고, 선언을 받는 `Document::rebuild_node_structure`만 변화를 허용한다.
- **I2 (선언 대조).** 후보 문서의 모든 기존 노드는 선언된 사상으로 계산한 안정 경로의 요소와
  정확히 하나씩 대응한다. 대응하지 못한 새 요소는 선언이 허용한 종류(`TableRemap`은
  `Cell`/`Paragraph`, `ParagraphRemap`은 `Paragraph`)여야 하고, 사상이 `None`인 기존 노드도
  같은 조건을 만족해야 한다. 어긋나면 `STATE_INVARIANT_ERROR`이며 commit하지 않는다.
- **I3 (ID 비재사용).** 0.3과 같다. 새 요소는 새 `nodeId`를 받고 삭제된 ID는 재사용하지 않는다.
  Undo/Redo가 스냅샷으로 이전 상태의 같은 요소와 ID를 되살리는 것은 재사용이 아니다.
- **I4 (선언 밖 불변).** 선언된 컨테이너 부분 트리 **밖** 노드의 ID·안정 경로·원문 바이트는
  바뀌지 않는다. 0.4에도 구역·표·그림·수식·필드·도형을 만드는 명령은 없다.
- **I5 (새 파일의 검증 기준).** `document.create`의 산출물은 "원본 대조"가 아니라
  **요청에서 다시 만든 기대 패키지와의 전 항목 대조**로 검증한다(§6.3). 검증을 통과하지
  못한 바이트는 공개하지 않는다(fail closed).

## 3. nodeId 대응표 갱신 — `NodeRemap`

### 3.1 0.3 `TableRemap`의 한계

`TableRemap`은 "표 하나의 구조 자식(행)과 행 안의 셀"이라는 두 단계 사상만 표현한다.
문단 삽입·삭제는 다음 두 곳에서 그 표현 밖이다.

1. 컨테이너가 표가 아니다. 최상위 문단의 컨테이너는 **구역 루트**이고, 표 셀 문단의
   컨테이너는 셀의 **`hp:subList`**다.
2. 사상할 단계가 **하나**(문단 순번)뿐이다.

### 3.2 새 선언: `ParagraphRemap`과 `NodeRemap`

```text
ParagraphRemap {
  entry:    구역 항목 경로,
  scope:    구조 자식 순번이 변하는 컨테이너의 안정 경로
            (구역 루트 = [], 표 셀 문단이면 [.., tbl, tr, tc, subList]),
  children: 변경 전 컨테이너의 구조 자식 순번 -> 변경 후 순번 | 삭제,
}

NodeRemap =
  | Table(TableRemap)          // 0.3 그대로: 표 하나의 행·셀
  | Paragraphs(ParagraphRemap) // 0.4: 문단 열 하나
```

두 종류의 공통 연산은 다음과 같다.

- `map(경로)`: `scope` 접두 밖이면 그대로, `scope`의 첫 단계에 `children`을 적용하고
  (`TableRemap`은 둘째 단계에 `cells`도 적용) 나머지 접미는 그대로 둔다. `None`이면 삭제.
- `contains(경로)`: `scope`보다 깊은 경로인가.
- `creates(kind)`: `TableRemap`은 `Cell`/`Paragraph`, `ParagraphRemap`은 `Paragraph`만 허용.

`Document::rebuild_node_structure(state, stale, &NodeRemap)`가 유일한 commit 관문이고,
0.3의 `rebuild_table_structure`는 `NodeRemap::Table`로 위임하는 얇은 래퍼로 남는다.
`structural_ids`(경로 대조·새 ID 발급)는 그대로 재사용하며, `creatable` 판정만 선언 종류에서
온다. 편집기가 선언을 틀리게 만들면 후보가 선언과 어긋나 commit이 거절되므로, 선언과 실제
XML 변경이 **서로를 검증**한다. 선언이 요청을 만족하는지는 독립 검증기가 본다(§6).

### 3.3 문단 열의 사상 계산

- 문단 삽입(`before`/`after`, `count = k`): 새 문단은 대상 문단의 구조 자식 순번 `q`
  (또는 `q+1`)부터 들어간다. `children[p] = p < q ? p : p + k`.
- 문단 삭제(`count = k`): 컨테이너의 구조 자식 연속 구간 `q..q+k`가 사라진다.
  `children[p] = p ∈ q..q+k ? None : p - |{삭제된 r < p}|`.

구조 자식은 `hae-model`이 안정 경로에 쓰는 것과 같은 규칙(`hp:run`/`hp:t` 투명)으로 센다.
그래서 컨테이너가 구역이든 `subList`든 같은 계산이 성립한다.

### 3.4 키와 영속화

`MapKey`(항목 경로·`XmlDoc::path` 구조 경로·종류)는 `map_rows()`가 현재 문서에서 매번
계산하므로 형식을 바꾸지 않는다. 문단 삽입·삭제 뒤 `project.save`는 새 경로로 대응표를 쓰고,
`open_with_map`은 모든 식별 요소가 정확히 한 행에 대응하는지 이미 검증한다. 새 문단의 ID는
저장 뒤에도 유지된다.

### 3.5 `document.diff`

`document.diff`는 0.3에서 nodeId 짝짓기로 바뀌었다. 문단 삽입은 `createdNodes`, 삭제는
`removedNodes`로 그대로 보고된다. 명령별 응답에는 `paragraph[]`
(`{command, position?, count, created[{nodeId, kind}], removed[]}`)를 따로 담는다.

## 4. Undo / Redo

이력은 commit 전후의 `CommittedState`(문서 상태 전체)를 스냅샷으로 보관하고, Undo/Redo는
스냅샷을 바꿔 끼운다(0.1 §8, 0.3 §4). 그래서:

- **Undo**: 삭제한 문단이 **원래 ID로** 돌아오고, 삽입한 문단은 사라진다. 패키지 항목이 같은
  `Arc` 페이로드이므로 `document.export`는 원본 바이트와 같다(시험으로 고정한다).
- **Redo**: 명령을 다시 실행하지 않고 이후 스냅샷을 복원하므로 삽입한 문단은 **같은 새 ID**를
  유지한다.
- **dryRun**: 후보를 버리므로 새 ID는 남지 않는다. 응답 `paragraph[].created`의 `nodeId`는
  비운다(`null`). `structure[]`의 dryRun 규칙과 같다.

구현 변경점은 없다. 시험으로 고정한다.

## 5. 편집 규칙

### 5.1 요청과 범위

- `target`: 기존 문단의 `nodeId`(`wrong_kind`로 표·셀이 아닌 문단만). 문서·문단의 조상이
  잠겨 있으면 기존 `check_editable` 규칙으로 거절한다.
- `position`: `"before"` | `"after"`(필수, 그 밖의 값은 `INVALID_REQUEST`).
- `text`: 선택. 없으면 빈 문단. 문자열은 XML 규칙으로 이스케이프한다. 탭·줄바꿈·XML 금지
  문자는 `INVALID_REQUEST`다(다른 명령과 같다). 길이 상한은 따로 두지 않고 기존 자원 한도가 막는다.
- `count`: 선택, 기본 1, `1..=1024`. 벗어나면 `INVALID_ARGUMENT`/`count_out_of_range`.
- 대상의 컨테이너는 **구역 루트**이거나 **표 셀의 `hp:subList`**여야 한다. 그 밖(각주·미주·
  글상자 안 `subList` 등)은 `UNSUPPORTED`/`paragraph_scope_unsupported`다. 노드 잠금·문서
  권한은 기존 규칙을 그대로 따른다.

### 5.2 삽입 — 새 문단의 바이트

새 문단은 대상 문단 `tpl`에서 다음을 복제한다.

1. **시작 태그**: `tpl`의 시작 태그 원문에서 `id` 속성과 그에 인접한 공백 하나를 뺀다
   (문단 id 중복 방지). `paraPrIDRef`·`styleIDRef`·`pageBreak`·`columnBreak`·`merged` 등
   나머지 속성은 원문 그대로다.
2. **첫 `hp:run`의 시작 태그**: `charPrIDRef` 등 원문 그대로. 글자 모양 참조를 복제한다.
   글자 모양 참조는 run 원문을 그대로 쓰므로 다른 속성이 있어도 함께 복제된다(설계 §5.2의
   "run 구조 복제"의 최소 구현이며, 여러 run의 서식 조합은 복제하지 않는다).

새 문단의 내용은 다음 하나뿐이다.

- `text`가 없거나 빈 문자열: 템플릿 run이 자기닫힘이면 `<hp:run …/>`, 열린 태그이면
  `<hp:run …></hp:run>`. 어느 쪽이든 **글자 조각(`hp:t`)을 만들지 않는다**.
- 글: `<hp:run …><hp:t>이스케이프한 글</hp:t></hp:run>`.

`linesegarray`도 텍스트 조각도 만들지 않는다. 대상 문단에 첫 `hp:run`이 없거나 run에
`charPrIDRef`가 없으면 `UNSUPPORTED`/`paragraph_template_unsupported`다.

**경계 거절.** `position: "before"` 대상이 `hp:secPr`를 자식으로 가진 문단이면
`UNSUPPORTED`/`paragraph_before_section_property`로 거절한다. 구역 속성은 첫 문단에
남아야 하는데(0.1 §11·0.3 `section.*` 계약) 새 문단을 그 앞에 두면 소유 문단이 바뀌기 때문이다.
그 문단 **뒤**에 넣는 것은 허용한다.

### 5.3 삭제 — 무엇을 지울 수 있나

- 대상과 삭제 구간은 **같은 컨테이너**의 연속한 문단이어야 한다. 구간에 다른 요소(표 등)가
  끼면 `UNSUPPORTED`/`paragraph_delete_not_contiguous`다.
- 삭제 구간이 컨테이너의 문단 열을 넘으면 `INVALID_ARGUMENT`/`paragraph_count_out_of_range`다.
- 구간 밖 문단이 비는 경우(그 컨테이너의 문단이 0개가 되는 경우)는
  `UNSUPPORTED`/`paragraph_would_empty_container`다. 능력 조회가 같은 reason으로
  `available: false`를 내므로 실행 코드도 같다(광고/실행 일치).
- 구간 밖으로 밀리는 문단이 없어도(마지막 문단 삭제) 허용한다.
- **내용 허용 목록**(삭제할 문단의 부분 트리): `p`, `run`, `t`, `linesegarray`, `lineseg`,
  `tab`, `lineBreak`, `nbSpace`, `fwSpace`, `hyphen`만 허용한다.
  - `hp:tbl`이 있으면 `UNSUPPORTED`/`paragraph_holds_table`.
  - 그 밖(그림·수식·필드·도형·각주·`secPr`·`ctrl`)이 있으면
    `UNSUPPORTED`/`paragraph_content_unsupported`이며 `detail.element`에 요소 이름을 담는다.
- 삭제 구간 안 문단에 글이 있었으면 `PrvText`를 고치지 않고 낡음(`stale.prvText`)으로 표시한다.

### 5.4 캐시 정책 (0.3의 확장)

- **최상위 문단 대상**: 대상 문단(삭제면 삭제 직전 위치)부터 그 구역의 마지막 최상위 문단까지
  `linesegarray`를 제거한다. `text.replace`·`paragraph.format`과 같은 규칙이며, 새 문단은
  캐시 없이 만들어지므로 결과에 캐시가 남지 않는다.
- **표 셀 문단 대상**: 셀 높이가 바뀔 수 있으므로 0.3 표 구조 명령과 같은 규칙을 쓴다 —
  표를 담은 최상위 문단(`carrier`)부터 구역 끝까지의 최상위 캐시와 **표 안의 모든 문단**
  캐시를 제거하고, 배치 중 앞 단계가 같은 표 아래에 기록한 앵커는 버린 뒤 변경 뒤 문서에서
  다시 앵커한다.
- 한 배치가 문단 구조 명령과 다른 종류의 편집을 섞을 때의 앵커 좌표계 문제는 0.3 §11과 같은
  한계를 그대로 가진다(시험한 조합만 보장, 어긋나면 `VERIFY_FAILED`로 fail closed).

## 6. 독립 검증 (`hae-verify`)

### 6.1 계약

`hae-verify/src/paragraph_structure.rs`와 `hae-verify/src/create.rs`는 `hae-edit`·`hae-model`에
의존하지 않는다. 입력은 **요청 원문**과 **앞 단계까지의 기대 패키지**뿐이고, 편집기가 만든
ID·`ParagraphRemap`·계획은 받지 않는다.

### 6.2 `paragraph.insert` / `paragraph.delete`

1. 대상 문단의 구조 경로를 원본에서 해석하고 §5의 적법성(컨테이너 종류, 템플릿 run,
   삭제 허용 목록, 빈 컨테이너, 연속성, `secPr` 경계)을 **스스로** 판정한다. 적법하지 않은
   요청이면 후보와 상관없이 거절한다.
2. 기대 바이트를 원본 XML에 대한 splice로 도출한다: 새 문단 원문(시작 태그 − `id` +
   첫 run 시작 태그 + `hp:t`), 삭제할 문단의 원문 범위, §5.4의 캐시 제거.
3. 같은 요청 목록의 뒷 단계가 밀린 문단을 가리키면 검증기가 스스로 도출한 경로 사상
   (`Remap`)으로 원본 주소를 따라가고, 앞 단계가 지운 요소를 가리키면 거절한다.
4. 캐시 제거 뒤에도 텍스트·`PrvText` 규칙(§5.3)은 그대로이고, 마지막에 기대 패키지와
   후보를 항목 바이트 단위로 대조한다(미편집 항목은 원시 압축 바이트까지).

### 6.3 `document.create` — "원본 없는 검증"의 기준

새 문서에는 대조할 원본이 없다. 그래서 기준을 **요청에서 다시 만든 기대 패키지 전체**로
정의한다.

- 검증기는 요청(`header`의 `title`, `paragraphs`)만으로 여섯 항목의 기대 payload를
  **독립적으로** 만든다: `mimetype`, `version.xml`, `META-INF/container.xml`,
  `Contents/content.hpf`, `Contents/header.xml`, `Contents/section0.xml`.
- 후보 ZIP에 대해 다음을 요구한다.
  1. 첫 항목이 `mimetype`이고 **STORED**이며 값이 `application/hwp+zip`이다.
  2. 항목 이름 목록과 순서가 기대와 정확히 같다(추가·누락·중복 금지).
  3. 항목별 payload가 기대 바이트와 **정확히** 같다.
  4. `hae_package::analyze`가 후보를 수용한다(참조·spine·manifest 무결성).
- 그래서 문단 수·문단 텍스트·`title`·참조 표(`paraPr`·`charPr`·`borderFill`·`fontfaces`)·
  `version.xml`의 `tagetApplication`/`major`·`container.xml`의 `rootfile@media-type`이
  하나라도 다르면 거절한다.
- `hae-verify::verify_created(bytes, spec, limits)`가 공개 관문이고, 호출 경로는
  `document.create`이다. 검증을 통과하기 전에는 파일을 만들지 않는다.

## 7. 패키지 골격 (`document.create`)

항목 순서(한컴 저장본 관찰 순서를 따른다)와 압축 방식:

| # | 항목 | 방식 | 내용 |
|---|---|---|---|
| 1 | `mimetype` | STORED | `application/hwp+zip` |
| 2 | `version.xml` | DEFLATE | `hv:HCFVersion`, `application="Hancom Office Hangul"`, `tagetApplication="WORDPROCESSOR"`, `major="5"` |
| 3 | `META-INF/container.xml` | DEFLATE | `ocf:rootfile full-path="Contents/content.hpf" media-type="application/hwpml-package+xml"` |
| 4 | `Contents/content.hpf` | DEFLATE | `opf:manifest`(header·section0) + `opf:spine` + `opf:metadata/opf:title` |
| 5 | `Contents/header.xml` | DEFLATE | `hh:refList`: `fontfaces`(7언어) · `borderFills` · `charProperties` · `paraProperties`(+`styles`) |
| 6 | `Contents/section0.xml` | DEFLATE | `hs:sec` + 첫 문단에 `hp:secPr` + `paragraphs` |

- **`version.xml` 함정.** 실측된 함정(Windows 1차 실패 분석·0.1 §11)을 따른다.
  `application="Hancom Office Hangul"`이면 `tagetApplication="WORDPROCESSOR"`이고
  `major ≤ 5`여야 한다. 틀리면 한컴이 빈 문서를 연다.
- **참조 무결성.** 구역의 `paraPrIDRef`·`charPrIDRef`·`styleIDRef`·`borderFillIDRef`가 모두
  `header.xml`에 정의되어야 한다. 비어 있는 `refList`는 `hae-verify::validate_refs`가
  `unresolved paraPrIDRef`로 거절하므로 편집 가능한 문서가 되지 못한다(0.1 실측).
- **문단.** `paragraphs`가 없거나 비면 빈 문단 하나를 만든다. 첫 문단은 항상 `hp:secPr`를
  담아 용지·여백을 명시한다. 글은 XML 규칙으로 이스케이프한다.
- **결정성.** 요청이 같으면 산출 바이트가 같아야 한다: 타임스탬프 고정(`1980-01-01`),
  항목 순서 고정, DEFLATE 고정. 시험으로 두 번 만들어 바이트 동일을 확인한다.
- **공개 경로.** `document.export`와 같은 `IoRoots` 규칙을 쓴다: 출력 루트 아래 절대 경로,
  기존 파일·보호된 입력 덮어쓰기 금지(`TARGET_EXISTS`/`PATH_NOT_ALLOWED`),
  임시 파일 → 재읽기 → 검증 → `RENAME_NOREPLACE` → 부모 fsync.
  열린 문서·`revision`·이력은 바뀌지 않는다(문서가 필요 없는 명령이다).
- **다시 열기.** `document.open`은 HWPX를 입력 루트에서 먼저 찾고, 없으면 **출력 루트**에서
  찾는다(프로젝트 열기와 같은 예외). 그래야 만든 파일을 같은 세션 규칙으로 바로 열어
  편집할 수 있다. 입력 루트의 파일이 출력 루트의 동명 파일을 가리지 않는다.

## 8. 트랜잭션 · dryRun · batch · 진단

- `paragraph.insert`/`paragraph.delete`는 기존 `handle_edit_steps` 경로를 그대로 쓴다:
  `expectedVersion` 검사 → 단계별 계획 → 편집기 적용 → 독립 검증 → 메모리 한도 → commit.
  batch 단계가 될 수 있고 같은 구역에 여러 번 이을 수 있다(두 구현이 순서대로 같은 상태를
  만든다). 실패 시 batch 전체가 롤백된다. 내용이 바뀌므로 revision +1, 단일 Undo 단위다.
- `document.create`는 `requiresDocument: false`, `expectedVersion` 없음, 요청 순서 번호 없음,
  `dryRun` 없음(`document.export`와 같은 부류다). 요청 순서를 쓰지 않으므로 재시도는
  `TARGET_EXISTS`로 거절되어 중복 부작용이 없다.
- 응답: 편집 명령은 `paragraph[]`(§3.5)를, `document.create`는
  `{path, sha256, bytes, entries, paragraphCount, title}`을 돌려준다.
- 새 `Code` enum은 없다. reason은 §10의 표와 `docs/decisions/diagnostic-codes.md`에 적는다.

## 9. 범위 밖 (여전히 불가)

- 문단 **분할·병합·이동**, 여러 구역에 걸친 삽입, 다른 문서에서 복사.
- 삽입 문단에 **서식 지정**을 함께 하거나 `text`에 탭·줄바꿈을 넣는 것(이어서
  `text.replace`·`paragraph.format`으로 한다).
- 개체·필드·수식·도형·각주·표가 든 문단의 삭제, `hp:secPr` 앞 삽입,
  각주/미주/글상자 안 문단의 삽입·삭제(컨테이너 제한).
- 표·그림·수식·필드·구역·스타일 정의의 생성, 셀 병합·분할.
- 새 문서에 그림·표·머리말·스타일·용지 설정을 함께 만드는 것(만든 뒤 기존 명령으로 편집한다).
  `document.create`는 용지 한 종류(A4 세로)·기본 스타일 하나만 만든다.
- 변경 추적 문서·서명 문서(기존대로 읽기 전용).
- Windows 한컴오피스 2024 실기 검증: Linux 한컴 2020 Beta 생존·화면 판독까지만 확인한다.

## 10. 진단 (새 `Code` 없음)

| 코드 | reason | 조건 |
|---|---|---|
| `INVALID_ARGUMENT` | `count_out_of_range` | `count`가 `1..=1024` 밖 |
| `INVALID_ARGUMENT` | `paragraph_count_out_of_range` | 삭제 구간이 컨테이너의 문단 열을 넘음 |
| `UNSUPPORTED` | `paragraph_would_empty_container` | 삭제 뒤 컨테이너에 문단이 0개(능력 조회와 같은 코드) |
| `UNSUPPORTED` | `paragraph_scope_unsupported` | 컨테이너가 구역 루트·표 셀 `subList`가 아님 |
| `UNSUPPORTED` | `paragraph_template_unsupported` | 대상 문단에 첫 run 또는 `charPrIDRef` 없음 |
| `UNSUPPORTED` | `paragraph_before_section_property` | `before` 대상이 `hp:secPr`를 담음 |
| `UNSUPPORTED` | `paragraph_delete_not_contiguous` | 삭제 구간에 문단 아닌 형제가 끼어 있음 |
| `UNSUPPORTED` | `paragraph_holds_table` | 삭제할 문단이 표를 담음 |
| `UNSUPPORTED` | `paragraph_content_unsupported` | 삭제할 문단에 허용 목록 밖 요소. `detail.element` |
| `INVALID_REQUEST` | — | `position` 값 오류, `text`의 탭·줄바꿈·금지 문자 |
| `TARGET_NOT_FOUND` | — | 없는 `target` |
| `UNSUPPORTED` | `wrong_kind` | `target`이 문단이 아님 |
| `TARGET_EXISTS` | — | `document.create` 대상 경로가 이미 있음 |
| `PATH_NOT_ALLOWED` | — | 출력 루트 밖·보호된 입력·경로 이탈 |
| `VERIFY_FAILED` | 검증 reason | 후보가 요청에서 독립 도출한 기대와 다름 |

## 11. 위험과 미해결

- **삽입 문단의 한컴 수용.** 새 문단은 시작 태그 복제 + run 하나/`hp:t` 하나이고 캐시가 없다.
  0.3의 빈 셀과 같은 모양이며, 문단에 `id`가 없는 합성 문서로도 한컴 열기 게이트를 통과한
  전례가 있다(0.3 좁은 편집 게이트의 합성 픽스처). 그래도 한컴이 저장할 때 부여하는
  문단 `id`와 다른 점은 관찰 항목이다.
- **새 문서 골격의 완전성.** Windows 1차 실패 분석은 글꼴 목록·스타일·`settings.xml` 누락을
  유력 가설로 남겼다. 이 문서의 골격은 `fontfaces` 7언어·`borderFills`·`charProperties`·
  `paraProperties`·`styles`를 갖추지만 `settings.xml`·`META-INF/manifest.xml`·
  `Preview/*`는 만들지 않는다. Linux 한컴 게이트로 열기·화면을 확인하고, Windows 실기는
  여전히 미검증으로 남긴다.
- **앵커 좌표계.** 배치 안에서 문단 구조 명령과 다른 편집을 섞는 조합은 시험한 조합만
  보장한다(0.3 §11과 같은 한계). 어긋나면 `VERIFY_FAILED`로 fail closed다.
- **독립성의 한계.** 검증기와 편집기는 같은 규칙 표(§5)를 각자 구현한다. 규칙 자체의 오류는
  양쪽이 함께 가질 수 있으며 한컴 게이트와 화면 관찰이 최종 방어선이다.
