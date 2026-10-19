# 새 그림 노드 생성 (image.insert) — 구현 노트 (2026-10-19)

상태: 구현 중. 이 문서는 계약·기본값·미확정 사항이며 완료 증거가 아니다. 실제
한컴/rhwp 열기·저장·재열기와 전체 회귀는 부모 통합·검증 단계의 별도 증거가 필요하다.

관련 계획: `docs/decisions/report-object-creation-plan.md`.
소유: `crates/hae-edit/src/image_insert.rs`, `crates/hae-verify/src/image_insert.rs`
(이 문서). 공유 `lib.rs`·계약·모델 ID 사상·캐리어/캐시/리포트/E2E는 부모 소유이며
이 작업은 그 파일을 편집하지 않는다.

## 목적

캡션 문단 앞에 새 PNG/JPEG 그림을 넣기 위한 **객체 XML만** 생성한다. 원본에 그림이
없어도 동작하며, 기존 그림/미디어를 요구하거나 수정하지 않는다. 경로는 읽지 않는다:
호출자(세션)가 파일을 한 번 읽어 완전히 디코드·검증한 `ImageBytes`를 전달한다.

## API

편집기 (`crate::image_insert`):

```rust
pub(crate) fn prepare(
    state: &mut hae_package::PackageState,
    xml: &hae_xml::XmlDoc,
    paragraph: usize,
    image: &crate::ImageBytes,
    width: u32,
    height: u32,
    limits: &hae_diag::Limits,
) -> hae_diag::Result<Vec<u8>>
```

검증기 (`hae_verify::image_insert`)는 `image: &[u8]` 외에는 같은 모양이다. 편집기의
생성 코드를 공유하지 않고 XML·매니페스트·이름·ID를 독립적으로 다시 계산한다.

- 반환값은 **`hp:pic` 요소 바이트뿐**이다. XML 선언, 캐리어 `hp:p`/`hp:run`, 스플라이스
  계획, 노드 리맵은 포함하지 않는다.
- 성공 시 `state`에 (1) 새로 고정된 미디어 엔트리 1개, (2) `content.hpf` 매니페스트
  항목 1개가 추가된다. 기존 엔트리·미디어 페이로드·네임스페이스는 변하지 않는다.
- 모든 검증(표시 치수 한계, 문단 형상, 매니페스트 형상, 패키지 증가량)은 상태를
  변경하기 **전에** 끝난다.
- `paragraph`는 대상 캡션 `hp:p`의 `xml` 내 요소 인덱스다. 구역 직속 문단 규칙,
  구역 속성 소유 문단 앞 삽입 금지, 캐리어 삽입, `NodeRemap`/캐시/리포트는 호출자
  (부모 공통 계층)가 담당한다.

## 새 리소스 규칙

- 매니페스트 ID: `hae-image-{i}`, 엔트리 이름 `BinData/hae-image-{i}.{png|jpg}`.
  `i`는 기존 매니페스트 ID(대소문자 무시)와 기존 엔트리 이름(대소문자 무시)에 없는
  첫 값. 기존 `image.replace`(`crates/hae-edit/src/image.rs`)의 명명·충돌 정책과 같다.
- 매니페스트 항목은 매니페스트 종료 태그 앞(`</opf:manifest>` 직전)에 삽입한다:

  ```xml
  <hae:item xmlns:hae="{manifest ns}" id="hae-image-1"
            href="BinData/hae-image-1.png" media-type="image/png" isEmbeded="1"/>
  ```

  `isEmbeded="1"`은 필수다. 없으면 한컴이 미디어를 외부 링크로 해석해 "그림 파일"
  경로를 묻고 프레임을 비운다(image.replace 노트와 동일 근거).
- 미디어 엔트리는 `state.add(name, image.bytes().clone(), 0)`으로 **끝에** 추가하며,
  `Arc`를 공유하는 동결 바이트 그대로다(재인코딩 없음).

## `hp:pic` 네이티브 기본값

`hp:pic`와 자식은 요소 순서·기본값을 공개 표본과 워크벤치 참조에 맞춘다(아래 출처).
`hp:`·`hc:` 접두사는 요소에 **로컬 선언**하므로 구역 접두사/중첩 재바인딩에 의존하지
않는다. 접두사 값은 문서가 실제 쓰는 URI(2011 또는 2016 문단 ns, 2011 core ns)를
따르되, 외부 URI 바인딩은 무시하고 표준 URI로 대체한다.

| 요소/속성 | 값 |
|---|---|
| `hp:pic` | `id`=`instid`=새 도형 ID, `zOrder="0"`, `numberingType="PICTURE"`, `textWrap="TOP_AND_BOTTOM"`, `textFlow="BOTH_SIDES"`, `lock="0"`, `dropcapstyle="None"`, `href=""`, `groupLevel="0"`, `reverse="0"` |
| `hp:offset` | `x="0" y="0"` |
| `hp:orgSz` / `hp:curSz` | 요청 표시 크기 `width`×`height` (HWPUNIT) |
| `hp:flip` | `horizontal="0" vertical="0"` |
| `hp:rotationInfo` | `angle="0" centerX="0" centerY="0" rotateimage="0"` |
| `hp:renderingInfo` | 항등 `hc:transMatrix`/`scaMatrix`/`rotMatrix` (`e1=e5="1"`, 나머지 0) |
| `hp:imgRect` | `pt0(0,0) pt1(W,0) pt2(W,H) pt3(0,H)` (표시 크기) |
| `hp:imgClip` | `left="0" right="IW" top="0" bottom="IH"` (원본 픽셀×75) |
| `hp:inMargin`/`hp:outMargin` | 모두 0 |
| `hp:imgDim` | `dimwidth="IW" dimheight="IH"` (원본 픽셀×75) |
| `hc:img` | `binaryItemIDRef="{id}" bright="0" contrast="0" effect="REAL_PIC" alpha="0"` |
| `hp:effects` | 빈 요소 |
| `hp:sz` | `width=W widthRelTo="ABSOLUTE" height=H heightRelTo="ABSOLUTE" protect="0"` |
| `hp:pos` | `treatAsChar="1" affectLSpacing="0" flowWithText="0" allowOverlap="0" holdAnchorAndSO="0" vertRelTo="PARA" horzRelTo="COLUMN" vertAlign="TOP" horzAlign="LEFT" vertOffset="0" horzOffset="0"` |

단위: `IW = px_w * 75`, `IH = px_h * 75`. 한컴은 원본 이미지 기하(imgClip·imgDim)를
픽셀당 75 HWPUNIT로 저장한다(`gen_editing.py`의 근거 주석, `bodojaryo-reference.hwpx`
표본의 `imgDim`/`imgClip` 값과 일치). 표시 크기 `W`/`H`는 `width`/`height` 인자이며
`1..=1_000_000` HWPUNIT다.

## 도형 ID 정책

- 새 `hp:pic`의 `id`=`instid`는 대상 문서와 `state`의 모든 XML 엔트리에서
  `hp`/`hc` 네임스페이스의 도형 로컬(`tbl,pic,equation,ole,container,chart,textart,
  shape,group,rect,ellipse,line,arc,polygon,curve,connectLine,button,cellzone,video`)의
  숫자 `id` 최댓값 + 1로 정한다. 최솟값은 `1_300_000_000`(워크벤치/형제
  `equation_insert`와 동일 기저). 문단 `id`는 별도 공간이므로 스캔하지 않는다.
- 위 19개 로컬은 `table_insert::SHAPES` 및 `equation_insert::OBJECT_LOCALS`가 세는
  합집합과 같다. `video`는 리뷰 r1에서 18개 목록에 빠져 있었고(수식 모듈은 셈),
  원본이 `hp:video id="1300000000"`을 쓰면 중복 ID가 생길 수 있었다. 편집기·검증기는
  코드를 공유하지 않으므로 두 정적 목록을 각각 19개로 맞췄다. 알고리즘(최댓값+1,
  기저 `1_300_000_000`, `u32` 검사)은 그대로다.
- 문서 전역·결정적이다. 편집기와 검증기가 같은 입력 상태·같은 호출 순서에서 같은
  값을 낸다.

## 독립 검증

`hae_verify::image_insert::check`는 같은 요청에서 기대 `hp:pic` 바이트·미디어 엔트리·
매니페스트 항목을 독립적으로 재계산해 `expected` 상태에 반영하고 기대 그림 바이트를
반환한다. 정적 PNG/JPEG 구조 검사는 `hae_verify::image::validated_media_type`
(`pub(crate)`, 이미 존재 → `image.rs` 편집 불필요)을 재사용하되, XML·이름·ID·기저는
편집기 코드를 공유하지 않는다. 요청 밖 본문·미디어 변조, 기존 미디어 변경, 잘못된
치수, 구조 불량 이미지는 거부한다.

## 출처·MIT 고지

`hp:pic` 템플릿(요소 순서, `zOrder`/`textWrap`/`rotMatrix`/`imgClip`/`imgDim`/`pos`
기본값)은 다음에서 실질적으로 유래했다.

- 로컬 HWPX 워크벤치 스킬 `scripts/hwpx_workbench/vendor/hwpx_utils.py::picture_xml`
  (MIT). 원출처: `ilikeadofai/hwpx-document-processing-skill`, 커밋
  `94f948cf90c5b37c01dc6e09069992ab09adb412`; 해당 스킬의
  `licenses/ilikeadofai-MIT.txt` 및 `THIRD_PARTY_NOTICES.md` 참조. 로컬 변경: 기본
  seed/rhwp 경로 제거.
- 같은 스킬의 `scripts/hwpx_workbench/build.py::Builder.image`(시드 기반 빌더) —
  `pixel_to_hwpunit(px) = round(px * 7200 / 96) = round(px * 75)` 및
  `instid = obj_id` 규칙.
- 공개/합성 표본: `tests/fixtures/corpus/bodojaryo-reference.hwpx`의 `hp:pic` 요소(요소
  순서·기본값), `tests/fixtures/editing/gen_editing.py::pic`(HWPUNIT 75/px 근거).

읽은 자료 목록과 보존한 MIT 라이선스 전문은 `docs/decisions/new-node-prior-art.md`에
있다. 이 모듈이 실질적으로 변형한 `picture_xml` 템플릿에 대한 MIT 고지는 그 문서와
로컬 스킬의 `THIRD_PARTY_NOTICES.md`를 따른다. 템플릿을 그대로 복사하지 않고 네이티브
그림을 새로 쓴다. 스타일·문자/문단 모양·borderFill을 합성하지 않는다.

## 미확정 사항 (증거 없음)

1. 새 `hp:pic`의 rhwp 렌더·Linux 한컴 열기/저장/재열기 생존은 아직 확인하지 않았다.
   (기존 `image.replace`의 `isEmbeded="1"` 근거는 2026-10-17 한컴 2020 Linux Beta
   관찰이며, 새 노드 생성에는 적용을 별도로 확인해야 한다.)
2. `orgSz`/`curSz`를 요청 표시 크기로 두고 `imgDim`/`imgClip`을 원본 픽셀×75로 두는
   조합은 `gen_editing.py`/`bodojaryo` 표본을 따른 것이다. 한컴이 `orgSz`를 원본
   자연 크기로 해석할 가능성은 표본 비교로 배제하지 못했다.
3. 1,000,000 HWPUNIT 상한은 표시 치수(`width`/`height`)에만 적용한다. 원본
   `px*75`(imgDim/imgClip)는 `u32` 범위 내에서만 검사하며(기존 image.replace 정책),
   픽셀 한계 40.96M에서 최대 약 3.07e9까지 가능하다.
4. `id`=`instid` 동일값은 워크벤치 규칙을 따랐다. 한컴이 `instid`의 전역 유일성을
   요구하는지, 문단 ID와 도형 ID가 실제로 분리된 공간인지는 표본 근거가 약하다.
5. 일괄 삽입에서 도형 ID는 호출자가 매 호출마다 **현재** 구역 문서를 넘긴다는 가정에
   기댄다. 캐리어 삽입이 배치 끝까지 지연되면 ID는 편집 전 상태 기준으로 증가하므로,
   편집기와 검증기의 호출 순서를 동일하게 유지해야 한다.
6. 구역 접두사가 기본 네임스페이스(무접두 `hp`)인 문서는 `xmlns:hp` 선언이 없어 표준
   2011 URI로 대체한다. 로컬 선언으로 자족적이지만, 부모 캐리어 삽입 위치에 따라
   중복 선언이 생길 수 있다(무해).
