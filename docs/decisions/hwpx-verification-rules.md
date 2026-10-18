# HWPX 저장 검증 규칙 (`hae-verify`)

이 문서는 엔진이 **저장 직전에** 무엇을 검사하는지 공개한다. 경쟁 지형에서 "한컴에서 열었을 때 안 깨지는가"가 경쟁 축으로 이동했는데(경쟁 분석 §2.2), 그 축의 공개 자산은 비어 있다. 이 문서가 그 자산이다.

## 1. 성격 — 편집기를 믿지 않는 관문

`hae-verify`는 **편집 계층에 의존하지 않는 별도 구현**이다. `Cargo.toml` 기준 `hae-edit`·`hae-model`·`hae-exec`에 의존하지 않으며(`hae-diag`·`hae-zip`·`hae-xml`·`hae-package`·`unicode-segmentation`만 쓴다), 저장할 후보 문서를 **원본 패키지 상태 + 요청 목록**에서 다시 계산해 대조한다.

핵심 계약: 검증기는 편집기가 만든 텍스트나 해시를 받지 않는다. **요청 원문**(예: 바꿀 범위와 새 문자열)과 **원본 XML**만으로 "결과가 이래야 한다"를 독립적으로 도출하고, 후보 바이트가 그 기대와 일치하는지 본다. 따라서 편집기와 검증기가 같은 버그를 공유할 수 없다.

호출 경로: `document.export`, `project.save` (둘 다 `verify_candidate`/`verify_archive`를 통과해야 파일이 공개된다). 실패하면 파일을 쓰지 않는다(fail closed).

## 2. 검사 규칙

### 2.1 ZIP·패키지 보존

| 규칙 | 내용 |
|---|---|
| 원본 항목 삭제 금지 | 원본의 모든 ZIP 항목이 후보에도 있어야 한다 |
| 압축 방식 변경 금지 | `method`가 바뀌면 거절 |
| 미편집 항목 재인코딩 금지 | payload가 같으면 **원시 압축 데이터와 로컬 헤더 바이트까지** 같아야 한다 |
| 요청에 없는 변경 금지 | 후보의 항목 이름·순서·manifest 모양이 요청 범위를 벗어나면 거절 |
| 중복 항목 금지 | 원본 항목 이름은 대소문자 무시 기준으로 유일해야 한다 |
| 매니페스트 정합 | `rootfile` 변경, manifest id/href/media-type 불일치, 항목 누락을 거절 |
| 미디어 항목 규칙 | 새 미디어는 빈 항목이어야 하고, 기존 이름 재사용 금지, `isEmbeded="1"` 필수 |

### 2.2 텍스트

- 범위는 **문단의 extended grapheme 반열림 구간**이다. 자소 경계가 아니면 `not a grapheme boundary`로 거절한다.
- 범위가 탭·줄바꿈·필드·수식·개체 같은 제어 요소를 가로지르면 거절한다(기본 `reject`).
- `controls: "preserve"`는 개체를 남기고 텍스트만 바꾸되, **필드 경계는 어떤 모드에서도 거절**한다(`field_boundary_in_range`). 필드 결과 텍스트의 의미가 바뀔 수 있기 때문이다.
- CDATA·주석·처리 명령(PI)·리터럴 제어문자를 가로지르면 거절한다.
- 비 UTF-8 `hp:t`·run, 내용이 있는 빈 `hp:t`는 거절한다.
- 삽입은 광고된 안전 구간 안에서만 허용한다(`insertion outside safe span`).

### 2.3 문서 전체 바꾸기(`text.replaceAll`)

- 일치 집합을 **검증기가 스스로** 계산한다(요청의 `find`·`text`·`scope`·`maxMatches`만으로).
- 편집기와 검증기가 **같은 규칙으로 건너뛴다**: 개체를 가로지르는 일치, 자소 경계가 맞지 않는 일치, 지원하지 않는 표기, 편집 불가 문단.
- 한쪽만 건너뛰면 저장 관문이 거절하므로(fail closed) 두 구현의 일치를 시험으로 고정한다.

### 2.4 표 셀

- 대상이 `hp:tc`가 아니면 거절한다.
- 병합 셀 정의, 중첩 표·중첩 셀 규칙을 확인한다.
- `paragraphs` 배열을 주면 **기존 문단 수와 정확히 같아야** 한다.
- 빈 셀은 첫 run이 있어야 하고, 불투명 내용이 있으면 거절한다.

### 2.5 문단 서식(`paragraph.format`)

- 같은 값의 기존 `paraPr`가 있으면 재사용, 없으면 새로 만든다. **요청 하나가 만드는 새 `paraPr`는 최대 하나**다.
- `paraPr` ID 중복·비수치 ID·`itemCnt` 불일치를 거절한다.
- 줄간격은 백분율 `1..=1000`만 허용한다.

### 2.6 글자 서식(`text.format`)

- 선택한 텍스트 범위에만 새 `charPr`가 적용됐는지 확인한다(범위 밖 글자가 바뀌면 거절).
- 값이 같은 기존 `charPr`를 재사용했으면 **새 자원을 만들지 않았는지** 확인하고, 새로 만들었으면 요청과 값이 일치하는지 본다.
- `charPr` ID 중복·오버플로·비수치 ID, `charProperties` 개수 불일치를 거절한다.
- 요청에 없는 `header.xml` 변경을 거절한다.
- `props` 검증: `textColor`는 `#RRGGBB`, `heightHwpunit`은 `100..=100000`, 최소 하나의 항목이 있어야 한다.

### 2.7 그림(`image.replace`)

- 후보가 요청한 그림 변경을 담고 있는지, 미디어 payload가 실제로 추가됐는지 확인한다.
- 그림의 크기·참조가 바뀌면 거절한다(기하 보존 계약).
- 입력 검증(별도 단계)과 독립적으로 **서명·픽셀 크기·컨테이너 형태·payload 동일성**을 다시 본다.
- 자르기·효과가 있는 그림, APNG·다중 프레임·다중 픽처 JPEG, HWPUNIT 범위를 넘는 크기를 거절한다.

### 2.8 미리보기 텍스트(`PrvText`)

- 문단 텍스트가 바뀌면 `PrvText`의 옛 문장이 **한 번만** 나올 때만 동기화 치환한다. 여러 번 나오면 `prv_text_stale`로 알린다.

## 3. 검증기가 검사하지 않는 것 (한계)

- **공식 OWPML/KS X 6101 스키마 전체 검증이 아니다.** 자체 규칙이며, 공식 XSD 대비 적합성은 별도 확인 대상이다(미확인 U7).
- **네이티브 조판·쪽 나눔 오류를 잡지 못한다.** 한컴이 다시 조판한 결과가 사용자 기대와 다른지는 이 관문의 범위 밖이다. 이 계열 결함은 실제 한컴 화면 관찰로만 잡힌다(경쟁 분석 §6.1의 D5 항목).
- **`hae-zip`·`hae-xml`·`hae-package`·`unicode-segmentation`은 편집기와 공유**한다. 검증기 자체 로직은 독립이지만 파서 버그는 양쪽에 함께 영향을 줄 수 있다.
- **Python oracle**은 별도 구현이지만 엔진을 대체하지 않는다. oracle 모델 밖 문단(CDATA·결합 문자)은 추정하지 않고 `unsupported`로 남긴다.

## 4. 검증기를 따로 돌리는 방법

```bash
# 게이트 자체 시험
cargo test -p hae-verify --locked

# 변이 거절 시험(편집기 산출물에 변조를 넣어 거절되는지)
cargo test -p hae-verify --test oracle_mutations --locked

# Python 독립 oracle (편집기와 무관한 별도 구현 + 변조 시험)
python3 -m unittest discover -s tests/oracle
```

## 5. 근거

- `crates/hae-verify/src/lib.rs` — 후보 대조와 ZIP 보존 계약
- `crates/hae-verify/src/text.rs` — 텍스트·replaceAll·셀 규칙
- `crates/hae-verify/src/format.rs` — 문단 서식
- `crates/hae-verify/src/char_format.rs` — 글자 서식
- `crates/hae-verify/src/image.rs` — 그림
- `crates/hae-verify/src/contract.rs` — 요청(`VerifyOperation`) 정의
- `crates/hae-verify/tests/` — 거절·수용 시험

## 수식 편집 추가 (0.3)

`equation.setScript`와 `equation.setProperties`는 `crates/hae-verify/src/equation.rs`의 독립 구현으로 원본 XML + 요청만에서 기대 바이트를 계산합니다. 편집기/모델에 의존하지 않습니다. script 텍스트 변경은 XML escape_text를 사용하며 속성·sz·pos·outMargin·shapeComment·다른 수식·원문 표기는 그대로 요구합니다. 속성 편집은 요청한 여섯 속성만 허용하고 script·sz를 그대로 요구합니다. linesegarray E 정책과 기존 PrvText 정책을 유지합니다. `verify_archive`는 미편집 항목의 원시 압축 데이터/로컬 헤더 보존까지 요구합니다.

지원하지 않는 원본 script 마크업 및 확인되지 않은 역슬래시/큰따옴표/제어문자 표기는 거절합니다. 전체 HWP 수식 문법이나 조판 검증은 하지 않습니다. `crates/hae-verify/tests/equations.rs`는 hae-edit/hae-model을 사용하지 않고 후보를 손으로 구성합니다.

로컬 한컴 게이트(2026-10-18, hoffice-hwp 11.20.0.989 offscreen): `tests/fixtures/editing/equations.hwpx`에 `equation.setScript`와 `equation.setProperties`를 적용해 `document.export`한 산출물 1건이 PASS(생존)했습니다. 이는 "열렸는가"만 본 결과이며 Windows 한컴오피스 2024의 조판·화면 일치를 뜻하지 않습니다.
