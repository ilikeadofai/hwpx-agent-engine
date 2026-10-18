# `document.validate` — OWPML 사전 기반 문서 검사 (카탈로그 MT-13)

작성: 2026-10-19, 브랜치 `feat/validate`. 규범 언어는 한국어다.
관련 문서: [저장 검증 규칙](hwpx-verification-rules.md), [진단 코드](diagnostic-codes.md),
[사용 안내서](../user-guide.md), 카탈로그 `docs/research/hwpx-feature-catalog.md` MT-13,
`docs/research/hwpx-vocabulary.md`.

## 1. 결론

`document.validate`는 열린 문서를 **읽어서** 한컴의 공개 OWPML 모델(`hancom-io/hwpx-owpml-model`,
Apache-2.0)의 요소·속성 이름과 대조하고, HAE만 알 수 있는 참조·카운트·패키지 사실을 함께 보고한다.
읽기 전용이며 문서를 바꾸지 않는다(`category: Query`, `mutatesContent:false`,
`readOnlyHint:true`, `expectedVersion`/요청 순서/이력 없음).

- 보고 형식: `items[]`(집계된 발견) + `summary`(심각도·코드별 개수) + `checked`(무엇을 실제로
  비교했는지) + `total`/`nextCursor`(커서 페이징) + `truncated`.
- 각 발견은 기계가 읽을 `code`, `severity`, `reason`, `count`, `examples[]`
  (`{entry, path, nodeId?}`), `detail`을 가진다.
- **문서를 거절하지 않는다.** 열기에 성공한 문서는 서술만 하고, 고치지 않는다.
- 사전에 없는 이름을 "오류"로 단정하지 않는다(§4 근거). `error`는 패키지·header 불변식에만 쓴다.

## 2. 사전의 출처와 추출 규칙

사전은 `/tmp/opencode/owpml`(=`https://github.com/hancom-io/hwpx-owpml-model.git`,
revision `1453388472c703a4b299a0834f425cdac16644b9`, 2023-10-24)에서 **이름만** 추출했다.
모델 코드는 복사하지 않았고, 생성 파일은 `crates/hae-model/data/owpml-dictionary.json`(356개 이름,
19개 namespace, `elementFingerprint` 포함)이다. 재생성:

```bash
git clone https://github.com/hancom-io/hwpx-owpml-model.git /tmp/opencode/owpml
python3 scripts/owpml_dictionary.py --source /tmp/opencode/owpml \
    --out crates/hae-model/data/owpml-dictionary.json
```

| 무엇 | 어느 파일에서 | 어떻게 |
|---|---|---|
| namespace 접두사 → URI | `OWPML/Base/NamespacePrefix.cpp`(`str_*_NAMESPACE`), `OWPML/Base/NamespacePrefix.h`(`*_PREFIX`) | 쌍 표(`s_pairtable_NamespacePrefix`)를 그대로 읽는다 |
| 매크로 → namespace | `OWPML/Class/MapMacro.h` | `OWPML_<NAME>_REDIRECT_FUNC` 정의가 어느 `*_PREFIX`를 쓰는지 |
| 요소 이름 | `OWPML/Class/**/*.cpp`의 `OWPML_<NS>_ADD_REDIRECT_FUNC(element, class, id)`, `OWPML_BASE_ADD_REDIRECT_FUNC(ns, element, class, id)`, `OWPML/**/*.cpp`의 `SetElemName(L"prefix:name")`(파트 루트), `OWPML_<NS>_REDIRECT_FUNC(parent, child)`(자식 선언) | 이름·클래스·등록 파일을 기록 |
| 속성 이름 | 각 클래스의 `CX::ReadAttribute`/`CX::WriteElement`(및 공유 묶음 클래스의 `SetAttribute(CAttribute*)`) 본문의 `...(att, L"name", ...)` | 클래스 상속(다중 상속 포함)을 따라 합집합 |
| 자식 요소 | `CX::InitMap`의 `OWPML_<NS>_REDIRECT_FUNC(ParentClass, child)` | 같은 상속 사슬로 합집합 |
| 모델의 "unset" 표식 | `OWPML/Class/Para/*.cpp`의 `(UINT)-1` 초기화, 실제 문서의 `4294967295` | 참조 검사에서 제외(§4) |

추출기의 세 가지 판단:

1. **이름 사전이지 스키마가 아니다.** 모델은 자식 요소를 부모 클래스별로 등록하므로 같은 이름이
   여러 클래스에 속한다(예: `hp:offset` = 글자 모양 offset `x/y` + 쪽 테두리 offset `left/right/top/bottom`).
   사전은 이름 하나에 모든 등록의 속성·자식 **합집합**을 담는다(`registrations` 필드).
   그래서 "부모-자식 관계가 틀렸다"는 검사는 하지 않는다.
2. **이름만 있는 등록도 남긴다.** `config-item-set`처럼 `_REDIRECT_FUNC1`이 하이픈 이름을 선언하고
   `_ADD_`는 C++ 토큰(`configitemset`)을 쓰는 경우, 실제 XML 이름(하이픈)에 클래스를 옮겨 붙인다.
   클래스를 알 수 없는 이름은 `classes: []`로 남겨 "이름은 알지만 속성 집합은 모른다"를 구분한다.
3. **`unresolved` 필드는 비어 있어야 한다.** 추출기가 해석하지 못한 매크로가 있으면 생성 파일에
   남고, Rust 로더가 그 파일을 거부한다(`Dictionary::parse`).

사전 파일은 Rust 테스트가 스스로 검증한다: `dictionary_fingerprint_covers_every_element_name`이
`ns:name\n` 목록의 SHA-256을 다시 계산해 파일의 `elementFingerprint`와 비교한다.

## 3. 무엇을 보고하는가

| 코드 | 심각도 | reason | 조건 |
|---|---|---|---|
| `unknown_element` | warning | `element_not_in_owpml_model` | 사전이 이름을 모르는 요소(사전이 채워진 namespace 안) |
| `unknown_attribute` | info | `attribute_not_in_owpml_model` | 모델이 그 요소 이름에서 읽거나 쓰지 않는 속성 |
| `unknown_namespace` | info | `namespace_not_in_owpml_model` | 모델이 모르는 namespace, 또는 모델이 선언만 하고 이름을 하나도 두지 않은 namespace(`hp10`·`hm`·`dc`·`epub`·`hwpml/2016/meta/pkg#`) |
| `unresolved_reference` | warning | `reference_not_in_header` | `charPrIDRef`·`paraPrIDRef`·`styleIDRef`·`nextStyleIDRef`·`borderFillIDRef` 값이 header 정의에 없음(값별 1행) |
| `unset_reference` | info | `reference_unset_sentinel` | 값이 `4294967295`(`(UINT)-1`). dangling으로 세지 않는다는 사실을 알린다 |
| `missing_package_item` | error(단 `header_absent`는 warning) | `mimetype_absent`·`mimetype_not_first`·`mimetype_value_mismatch`·`container_absent`·`rootfile_absent`·`section_absent`·`section_entry_absent`·`header_entry_absent`·`header_absent` | 현재 패키지에 필수 항목이 없음 |
| `count_mismatch` | error | `item_cnt_mismatch`·`font_cnt_mismatch`·`sec_cnt_mismatch` | 선언 카운트 ≠ 실제 자식 수(`itemCnt`·`fontCnt`), 또는 `hh:head@secCnt` ≠ spine 구역 수 |
| `duplicate_resource_id` | error | `duplicate_id_in_header` | 같은 종류의 자원 id가 header에 두 번 이상 정의됨 |
| `unused_resource` | info | `no_reference_in_document` | 정의됐지만 문서 어디에서도 참조되지 않는 `charPr`·`paraPr`·`style`·`borderFill`(종류별 1행) |

`itemCnt`/`fontCnt`는 **모델이 같은 클래스에서 선언한 자식 이름**과 비교한다(임의 추측 없음).
모델이 그 요소의 자식 집합을 모르면 검사하지 않는다. `hh:secCnt`는 패키지 spine의 구역 수와 비교한다.

## 4. 심각도 근거 (코퍼스 실측)

검사는 "한컴에서 안 깨지는가"라는 HAE의 주장과 맞닿아 있어, 실측 없이 심각도를 정하지 않았다.
`tests/fixtures/corpus/` 10개 문서(11번째는 `manifest.json`)에 그대로 실행한 결과:

| 관찰 | 실측 | 결정 |
|---|---|---|
| 사전에 없는 요소 | `hh:layoutCompatibility`의 자식 5종(`char`·`paragraph`·`section`·`object`·`field`)이 실제 공문서 서식 2건(`gani-gian`·`ilban-gian`)에 있음. `opf:version`이 `irb-sample`에 있음 | `warning`(오류 아님). 한컴이 쓴 문서에 있으므로 결함이라 단정할 수 없다 |
| 사전에 없는 속성 | 재추출 후 **0건**. 사람이 읽는 이름 집합이 실제 문서를 덮는다 | `info`. 그래도 0건을 근거로 error로 올리지 않는다 — 모델의 속성 집합은 "리더가 읽는 이름"이지 완전한 스키마가 아니다(§3의 `required-namespace` 사례처럼 표기 차이가 있다) |
| 미지원 namespace | `http://www.hancom.co.kr/hwpml/2016/meta/pkg#`가 6개 문서의 `META-INF/container.rdf`에 있음 | `info`(범위 밖). "모르는 이름"과 "모델이 다루지 않는 namespace"를 구분한다 |
| dangling 참조 | `anchor_char_height.hwpx`에 `hp:p@styleIDRef="0"` 20건(이 문서엔 `hh:styles`가 아예 없다). 나머지 9개는 0건 | `warning`. 열리는 문서에 존재하므로 오류로 부르지 않는다 |
| `4294967295` | 6개 문서의 `hh:paraHead@charPrIDRef`(문서당 7~41건) | 검사에서 제외하고 `info`로만 알린다. 모델이 `(UINT)-1`로 초기화하는 "참조 없음" 표식이다 |
| 카운트 불일치 | 10개 문서 모두 `itemCnt`·`fontCnt`·`secCnt` 일치 | `error`. 저장 관문도 편집 시 같은 불일치를 거절하므로(`hwpx-verification-rules.md` §2.5·§2.6) 새 오탐 위험이 없다 |
| 중복 정의 id | 10개 문서 모두 0건 | `error` |
| 미사용 자원 | `style id="12"` 4건, `paraPr id="1"` 2건 | `info`. 실제 서식에 흔하다 |
| 필수 패키지 항목 | `document.open`이 이미 거절하는 조건이라 열린 문서에서는 비어 있다. `irb-sample`은 header 파트가 없어 `header_absent`(warning)가 난다 | `missing_package_item`. header 부재는 열기가 허용하므로 warning + 참조 검사 미실시(`checked.referenceIntegrity:false`) |

코퍼스 전체 결과: **오류 0건**(10/10), 경고는 3개 문서(`gani-gian`·`ilban-gian`의 미지원 요소,
`anchor_char_height`의 dangling style, `irb-sample`의 header 부재), 나머지는 `info`.
명령 실행 요약(`hae script`로 `document.open` → `document.validate`, 2026-10-19):

| 파일 | 오류 | 경고 | 정보 | 발생 수 | 훑은 요소 | 참조 | 코드별 |
|---|---|---|---|---|---|---|---|
| anchor_char_height.hwpx | 0 | 1 | 1 | 21 | 545 | 82 | unknown_attribute 1, unresolved_reference 20 |
| bodojaryo-reference.hwpx | 0 | 0 | 2 | 12 | 11,477 | 679 | unknown_namespace 2, unset_reference 10 |
| gani-gian.hwpx | 0 | 5 | 2 | 8 | 1,306 | 147 | unknown_element 5, unknown_namespace 2, unused_resource 1 |
| gyehoek-reference.hwpx | 0 | 0 | 3 | 13 | 41,468 | 3,598 | unknown_namespace 2, unset_reference 10, unused_resource 1 |
| ilban-gian.hwpx | 0 | 5 | 2 | 8 | 1,214 | 77 | unknown_element 5, unknown_namespace 2, unused_resource 1 |
| irb-sample.hwpx | 0 | 2 | 1 | 3 | 204 | 0 | missing_package_item 1, unknown_attribute 1, unknown_element 1 |
| problem-answer-reference.hwpx | 0 | 0 | 3 | 10 | 4,380 | 417 | unknown_namespace 2, unset_reference 7, unused_resource 1 |
| ref_empty.hwpx | 0 | 0 | 3 | 13 | 1,398 | 110 | unknown_namespace 2, unset_reference 10, unused_resource 1 |
| render-p35-font-native-bitmap.hwpx | 0 | 0 | 3 | 13 | 1,400 | 110 | unknown_namespace 2, unset_reference 10, unused_resource 1 |
| research-school-plan-reference.hwpx | 0 | 0 | 2 | 70 | 56,307 | 5,014 | unknown_namespace 29, unset_reference 41 |

`unused_resource`가 나오는 6개 문서는 `style id="12"`(4건)·`paraPr id="1"`(2건)처럼 실제 서식이
정의만 해 둔 자원이다.

## 5. 계약

- **불변**: 문서·revision·이력·nodeId를 바꾸지 않는다. 시험이 `document.export` 전후 바이트
  동일로 확인한다.
- **결정성**: 같은 revision·같은 인자는 같은 `items`를 만든다(정렬: 심각도 → 코드 → 메시지).
- **커서**: `revision:offset` 형식이며 `(documentId, revision, selector)`에 묶인다
  (`selector = ["document.validate", scope, maxExamples]`). 다른 revision/selector의 커서는
  `VERSION_CONFLICT`, 형식 오류는 `INVALID_REQUEST`.
- **scope**: 임의 nodeId. **요소·속성·namespace 대조만** 그 부분 트리로 제한한다. 참조 무결성·
  자원 정의·카운트·중복·미사용·패키지 검사는 문서 전체 속성이므로 scope가 있어도 수행하고
  `scope: "document"`로 표시한다.
- **한도**: `limit` 1..=500(기본 100), `maxExamples` 1..=50(기본 5). 서로 다른 발견 2048개에서
  새 행 추가를 멈추고 `truncated:true`(각 `count`는 중단 지점까지). XML 요소 방문 수가
  `Limits::max_validate_elements`(기본 1,000,000, 요소·참조 스캔 합계)를 넘으면 부분 결과 대신
  `RESOURCE_LIMIT`/`validate_scan_limit`로 거절한다.
- **없는 header**: 참조를 해석할 수 없으므로 참조·자원 검사를 하지 않고
  `checked.referenceIntegrity:false`로 알린다(모든 참조를 dangling으로 오탐하지 않는다).

## 6. 이 검사가 하지 않는 것

- **공식 XSD/KS X 6101 적합성 검증이 아니다.** 모델은 2023 스냅샷이며, 이름 수준 대조만 한다.
  속성 **값**·열거형·자식 순서·필수 자식은 검사하지 않는다.
- **한컴 호환 판정이 아니다.** 여기서 오류 0이라고 한컴이 같은 모양으로 조판한다는 뜻이 아니다
  (Windows 한컴 2024 실기 검증은 별도, 미수행).
- **거절 게이트가 아니다.** 저장 관문(`hae-verify`)은 그대로다. 이 명령은 진단만 한다.
- 문서를 고치지 않는다. 발견을 자동 수정하는 경로는 없다.
- 사전 추출기는 C++ 소스의 매크로·메서드 본문 패턴에 의존한다. 모델이 등록 방식을 바꾸면
  `unresolved`가 비지 않거나 이름 수가 줄어들고, 그때는 생성 파일을 다시 만들고 심각도 근거를
  재측정해야 한다.

## 7. 한컴 게이트

조회 뒤 `document.export`한 산출물 2종(`bodojaryo`·`gani-gian` — export 바이트는 원본과 `cmp` 일치)과
`itemCnt`/`fontCnt` 불일치 합성 문서 1종이 모두 Linux 한컴 2020 Beta에서 PASS(생존)했다. 흥미로운
관찰: 한컴은 카운트가 어긋난 문서도 열었다 — 그래서 `count_mismatch`의 `error` 근거를 "한컴이
거절한다"로 쓰지 않고 "선언과 내용이 모순이며 HAE 저장 관문은 그런 문서를 만들지 않는다"로 적었다.
원문·판정·한계: [한컴 열기 확인](document-validate-hancom-open-2026-10-19.md).

## 8. 시험

- `crates/hae-exec/tests/document_validate.rs` — 명령 수준 17건: 정상 문서(오류 0), 깨끗한 문서의
  빈 보고서, 미지원 요소·속성, 미지원 namespace, dangling 참조, `4294967295`, `itemCnt`/`fontCnt`,
  `secCnt`, 중복 id, 미사용 자원, header 없는 패키지, 커서 페이징, `scope`, 크기 한도,
  문서 불변(export 바이트 동일), 명령 메타데이터, `maxExamples`.
- `crates/hae-model/src/validate_tests.rs` — 사전 fingerprint·이름·namespace 별칭, 패키지 필수 항목
  (손으로 만든 `PackageInfo`), 인자 거절, 안정성·읽기 전용, 예시 nodeId, 스캔 한도 8건.
- 커서/스캔 한도 시험은 `Limits::max_validate_elements`를 작게 준 세션을 쓴다(큰 문서를 만들지 않는다).
