# 신규 수식 노드 생성 — 구현 노트 (`equation.insert`)

상태: **초안 구현 완료, 통합·Cargo 빌드·한컴 게이트는 미완(상위 에이전트 담당)**.
이 문서는 현재 코드가 실제로 보장하는 것과 아직 보장하지 않는 것을 분리해 적는다.

관련 문서: [표 구조 노드 생성 설계](table-structure-node-creation-design.md),
[저장 검증 규칙](hwpx-verification-rules.md).

## 0. 소유 범위

이 작업은 공유 체크아웃에서 **다음 세 파일만** 소유한다.

- `crates/hae-edit/src/equation_insert.rs` — 편집기 측 객체 XML 생성
- `crates/hae-verify/src/equation_insert.rs` — 검증기 측 **독립 재도출**
- `docs/decisions/new-node-equation-notes.md` — 이 문서

`lib.rs` 모듈 등록, 공용 carrier 문단/run·ID remap·PrvText 캐시 배선, 오퍼레이션
enum/계약/레지스트리/session 배선은 **상위 에이전트 소유**다. 이 모듈은 새 수식
**요소 바이트만** 반환하며 문단·run·캐시를 만들지 않는다. `cargo` 실행과 커밋/푸시는
상위 에이전트 조정 전까지 하지 않았다.

## 1. API

두 파일 모두 동일 계약을 노출한다(등록은 상위 담당).

```rust
pub(crate) fn build(
    state: &hae_package::PackageState,
    xml: &hae_xml::XmlDoc,
    paragraph: usize,
    script: &str,
    width: u32,
    height: u32,
) -> hae_diag::Result<Vec<u8>>
```

- 반환: `<hp:equation …>…</hp:equation>` **요소 바이트만**. carrier `hp:p`/`hp:run`은
  포함하지 않는다.
- `paragraph`: `xml` 안에서 carrier `hp:p`의 **요소 인덱스**. `hp`(2011/2016) 문단이
  아니거나 범위 밖이면 거절한다.
- `width`/`height`: HWPUNIT, `1..=1_000_000`. 0·초과는 거절. 상위 API가 이미 양수를
  요구하므로 **추정(estimated) 정책**으로 명시값을 그대로 쓴다. workbench의
  `equation_layout == "recalculate"`가 하던 `baseLine`/`sz` 0화는 적용하지 않는다.
- 같은 종류의 기존 수식이 **없어도** 동작한다(`document.create` 경로 포함). 기존
  수식 템플릿은 필수 근거가 아니다.

## 2. 근거(provenance)

요소 골격·속성 집합은 로컬 HWPX workbench 스킬의 네이티브 수식 조립기에서 가져왔다.

- `~/.codex/skills/hwpx-workbench/scripts/hwpx_workbench/vendor/hwpx_utils.py`
  `equation_xml` **225행** — 기본 속성/자식 순서/`shapeComment`/`textWrap` 등.
- `~/.codex/skills/hwpx-workbench/scripts/hwpx_workbench/build.py`
  `Builder.eq_xml` **90행** — `affectLSpacing`을 `"1"`로 덮어씀. 이 값을 채택한다.
- 라이선스: 해당 스킬 `THIRD_PARTY_NOTICES.md` — `vendor/hwpx_utils.py`,
  `build/lib/hwpx_workbench/build.py`는 `ilikeadofai/hwpx-document-processing-skill`
  (MIT). 상태 필드/기본값은 데이터 수준으로 참고했고, XML 전체 재작성 방식은
  가져오지 않았다(우리는 좁은 API + 독립 검증기 유지).

합성 코퍼스도 참고했다: `tests/fixtures/editing/equations.hwpx`(ZIP)의
`Contents/section0.xml` `hp:equation` 실물(합성, 개인문서 아님). 저장소 PDF/XML
텍스트 grep으로는 안 잡혔지만 ZIP 안에 실물이 있다.

**주의(과대주장 금지):** 현 엔진 생성기의 출력을 한컴 오피스에서 열어 본 검증은
이 작업에서 하지 않았다. workbench 경로는 기존 조립기의 **구조 참고 자료**이며,
현재 바이트의 한컴 생존은 **미검증(게이트 대기)**이다.

## 3. 고정 속성/기본값

workbench 값을 그대로 쓴다. 임의의 미검증 기본값을 새로 만들지 않는다.

| 위치 | 값 |
|---|---|
| `numberingType` | `EQUATION` |
| `textWrap` / `textFlow` | `TOP_AND_BOTTOM` / `BOTH_SIDES` |
| `lock` / `dropcapstyle` | `0` / `None` |
| `version` | `Equation Version 60` |
| `baseLine` | `86` (workbench; 합성 fixture는 `66`, 충돌 시 workbench 우선) |
| `textColor` / `baseUnit` | `#000000` / `1000` |
| `lineMode` / `font` | `CHAR` / `HancomEQN` |
| `hp:sz` | `widthRelTo=ABSOLUTE`, `heightRelTo=ABSOLUTE`, `protect=0` |
| `hp:pos` | `treatAsChar=1`, **`affectLSpacing=1`**, `flowWithText=1`, `allowOverlap=0`, `holdAnchorAndSO=0`, `vertRelTo=PARA`, `horzRelTo=PARA`, `vertAlign=TOP`, `horzAlign=LEFT`, `vertOffset=0`, `horzOffset=0` |
| `hp:outMargin` | `0/0/0/0` |
| `hp:shapeComment` | `수식입니다.` |
| 자식 순서 | `sz`, `pos`, `outMargin`, `shapeComment`, `script` |

`EquationProps` 공개 API는 아직 확장하지 않는다. 신규 수식의 고정값은 위 표로
한정한다.

## 4. ID / zOrder 결정

- 객체 `id`: 베이스 `1_300_000_000`. `hp`/`hc` 네임스페이스에서 **19개 로컬 이름
  union**(`tbl, pic, equation, ole, container, chart, textart, shape, group, rect,
  ellipse, line, arc, polygon, curve, connectLine, button, cellzone, video`)에
  해당하는 요소의 숫자 `id` 최댓값 + 1. `xml`(현 구역) + `PackageState`의
  `*.xml` 항목 전체를 스캔해 패키지 전역 충돌을 피한다. 구역 밖 개체와도 충돌하지
  않는다. `u64::MAX` 같은 적대적 값은 `checked_add`로 **패닉 없이 거절**한다.
- `zOrder`: 베이스 `1`. 문서 전체에서 음이 아닌 `zOrder` 최댓값 + 1. (0으로
  통일하지 않는다.)
- 두 파일의 union/베이스/스캔 규칙은 **동일**해야 한다. 상위 remap이 있더라도
  이 값들은 결정적이다.

## 5. 네임스페이스 처리

- carrier 문단의 `qname` 접두사가 있으면 그 접두사를 그대로 재사용한다. 해당
  결합은 문단에서 이미 스코프에 있고 수식은 문단의 자손이므로 안전하다. 별도
  선언을 추가하지 않는다.
- 문단이 무접두사(기본 네임스페이스가 `hp`)면 수식 요소에 `xmlns:hp="…"`를
  **지역 선언**하고 `hp:` 접두사를 쓴다. 중간 요소가 기본 네임스페이스를 재결합해도
  영향받지 않는다. `hc`/`hh` 선언은 필요하지 않다(요소가 `hp`만 쓴다).

## 6. 스크립트 검증

기존 `crates/hae-edit/src/equation.rs`와 `crates/hae-verify/src/equation.rs`의
표기 제한을 **독립적으로 다시 구현**했다(기존 검증기를 `pub`으로 바꾸지 않음).

- 비어 있음/공백뿐/비 XML 문자 거절
- `\`, `"`, `<!`, `<?`, 제어문자 거절
- `{}` 균형 필수
- 통과분은 `escape_text`로 `&`/`<`/`>`를 이스케이프

`hae-verify` 쪽은 `crate::fail`(VERIFY_FAILED)로, 편집기 쪽은 `INVALID_REQUEST`/
`UNSUPPORTED`로 보고한다. 두 구현이 갈라지면 저장 게이트가 실패하도록 코드를
공유하지 않았다.

## 7. 단위 테스트(소유 파일 내 `#[cfg(test)]`)

`hae-edit`:
- 기존 수식 0개에서 성공, `id="1300000000"`, `zOrder="1"`
- `&`/`<`/`>` 이스케이프
- 빈/백슬래시/따옴표/CDATA/PI/불균형/제어문자 거절
- 치수 0·1_000_001 거절, 1·1_000_000 허용
- 다른 구역/기존 개체를 넘는 ID, 기존 `zOrder` + 1
- 접두사 재사용(`zz:`)과 무접두사 지역 선언(`hp`) 경로
- `u64::MAX` id 센티넬 패닉 없이 거절
- 범위 밖/문단 아님 거절

`hae-verify`: 위 규칙을 독립적으로 재현하고, **완전한 바이트 골든 문자열**로 계약을
고정한다(`exact_bytes_match_the_frozen_contract`). 주석 처리된 `hp:rect`가 스캔되지
않는지도 확인한다.

## 8. 남은 불확실성

1. **한컴 네이티브 생존 미검증.** 상위의 실제 executor E2E와 한컴 열기 게이트가
   필요하다.
2. **다단계 배치 ID 일관성.** 편집기가 op를 순차 적용하며 `PackageState`가 갱신되면
   `id` 스캔 결과가 단계마다 변한다. 검증기는 단계별 **사전(pre-step)** 상태를 써야
   두 구현이 같은 값을 낸다. 이 규율은 상위 remap/carrier 계층의 몫이며, 여기서는
   "양쪽이 동일한 사전 입력을 받는다"는 계약으로만 문서화한다.
3. **무접두사 기본 네임스페이스 구역**에서 지역 `xmlns:hp` 선언이 한컴 파서에
   문제없는지는 실제 코퍼스가 접두사를 쓰므로 미검증이다(합법 XML, 스키마상 유효).
4. `baseLine=86` vs 합성 fixture `66`: workbench의 명시적 생성 기본값을 택했다.
   실제 열기 검증에서 문제가 보이면 이 값부터 재검토한다.
