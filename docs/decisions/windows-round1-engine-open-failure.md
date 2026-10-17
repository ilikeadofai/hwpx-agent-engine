# Windows 1차 engine_inputs 열기 실패와 2차 확인 자료

작성일: 2026-10-17. 브랜치: `wip/stage1-model-edit`.

## 판정과 증거의 경계

가장 유력한 원인은 `scripts/evaluate_mcp.py`가 만든 합성 문서의 불완전한 native 구조다.
**한컴 충돌을 일으킨 단일 필드나 수정본의 Windows 호환은 아직 확정하지 않았다.**
원래 fixture의 글꼴/스타일 목록 누락, 기본 속성 생략, 그림 공통 구조 생략은 실제 바이트에서
확인했다. 이것들이 한컴 충돌 원인이라는 인과 판단은 **가설**이며 2차 시험으로 분리한다.
Linux 평가 성공, ZIP/XML 파싱 성공, 엔진 재열기 성공을 한컴 호환 성공으로 해석하지 않는다.

1차 근거:

- `spikes/work/windows-return-20261017-2047/return/results.csv`: engine 01/02/03/04 모두
  빈 문서 UI 후 종료, s4 입력 12개는 열기·재저장·재열기 성공으로 기록되어 있다.
- 같은 위치 `environment.txt`, `environment_auto.txt`: Windows 10 IoT Enterprise LTSC,
  21H2 / OS 19044.7725; 설치 프로그램 목록의 한컴오피스 2024 Edu `13.0.0.304`.
  문서 `version.xml`의 producer 문자열은 제품 목록 버전과 별개다.
- engine 01은 편집 전 합성본이다. 01과 undo 03의 SHA-256은
  `191cd49fb03461093f7fedec02874bfb53a98d278ef4405a3bc94afc01a7d816`으로 동일하다.
  02와 redo 04도 `56b335ae9ca88dde04f55c06e1dfdf21ddf4f610b63bdf6575a1cad6088c659e`로 동일하다.
  따라서 엔진의 편집 연산만으로 4개 전체의 실패를 설명할 수 없다.
- s4 성공은 선행 스크립트 편집본의 결과다. Rust 엔진이 실제 문서를 편집해 한컴에서 열었다는
  증거로 확대하지 않는다. 또한 CSV의 성공 표시는 원본과 쪽수가 모두 같다는 뜻이 아니다.

대조 대상은 사용자 작성 `01_text`, `02_table`, `03_picture`, `03_picture_글자처럼취급`,
`04_complex`와 s4의 `*_0_원본.hwpx` 3개다. 사용자 자료는 로컬에서만 처리했고 외부 모델로
전송하지 않았다. `spikes/work/hancom-sdk-trial/` 및 SDK 관련 파일은 열지 않았다.

## 구조 대조: 한컴 저장본에는 있고 실패 fixture에는 없는 것

다음은 관찰 차이다. 모든 생략 항목을 규격상 필수라고 단정하지 않는다.
기계 판독 근거는 `spikes/work/windows-round2/structure-audit.json`에 있다.

### 패키지·ZIP

- 실패본은 6개 항목뿐이다. 실제 `02_table`은 11개, inline 그림본은 이미지 포함 12개다.
  실패본에는 `version.xml`, `settings.xml`, `META-INF/manifest.xml`,
  `META-INF/container.rdf`, `Preview/PrvText.txt`, `Preview/PrvImage.png`가 없다.
- 양쪽 모두 `mimetype`이 첫 항목이며 STORED(method 0), 내용은
  `application/hwp+zip` 19바이트다. 이 부분은 결함 근거가 없다.
- 한컴 저장본은 보통 mimetype → version → header → (BinData) → section → preview/settings →
  RDF/HPF/container/manifest 순이다. 실패본은 mimetype → container → HPF → header → section →
  image다. **항목 순서 자체가 원인인지는 가설**이다. 성공 s4 사이에도 후속 항목 순서가 달라
  mimetype 이후 순서만을 유력 단일 원인으로 삼을 근거는 약하다.
- 한컴 `container.xml`에는 HPF 외에 PrvText와 RDF rootfile도 있다. 실패본에는 HPF 하나만 있다.
  한컴의 `manifest.xml`은 빈 ODF manifest이며 RDF는 header/section/Document 관계를 담는다.
- 한컴 HPF manifest에는 `settings`가 있고 spine 항목에 `linear="yes"`가 있다. 실패본에는 없다.
  양쪽의 header/section spine 참조와 실제 ZIP payload 연결은 정상이다.

### 헤더와 참조

- 실제 저장본에는 `beginNum`, `fontfaces`(7 언어), `tabProperties`, `numberings`, `styles`,
  `compatibleDocument`, `docOption`, `trackchageConfig`가 있다. 실패본에는 없다.
- 한컴 `charPr`에는 `fontRef`, `ratio`, `spacing`, `relSz`, `offset`와 글꼴/테두리/문자 속성이
  있다. 실패본은 id/height/textColor만 있다. **글꼴·스타일 기본 목록 누락은 유력 가설**이다.
- 한컴 `paraPr`에는 tab 참조, heading, break/keep 설정, margin, border 등 완전한 기본값이
  있다. 실패본은 align/lineSpacing만 있다. `borderFill`에는 실제 저장본의 diagonal 등도 없다.
- 실패본의 `charPrIDRef=0`, `paraPrIDRef=0`, `borderFillIDRef=1`은 존재한다.
  **itemCnt 불일치는 발견하지 않았다**: 실패본의 3개 목록과 실제 대조본의 목록 모두 실제 개수와 같다.
  반면 실패 `secPr outlineShapeIDRef=0`에 대응하는 numbering 목록 자체가 없다.
  한컴 저장본은 outline=1과 numbering id=1이 연결된다. 이 누락 참조의 native 영향은 가설이다.
- 새 generator는 공개 native seed의 7언어 fontRef, 문자/문단/스타일/테두리 참조를 유지한다.
  native `paraHead charPrIDRef=4294967295`는 상속 sentinel로 검사에서 구분한다.

### 구역·첫 문단

- secPr가 첫 문단 run 안에 있고 colPr control이 뒤따르는 위치는 실패본도 맞다.
  A4 pagePr/여백도 이미 있었다. 과거의 상단 잘림 개선만으로 native 완전성이 확보되지는 않았다.
- 실제 저장본에는 `footNotePr`, `endNotePr`, BOTH/EVEN/ODD `pageBorderFill`이 있다.
  실패본에는 없고, head version도 1.2 / native 대조본은 1.5다.
- 실제 모든 문단의 `id`, `styleIDRef`, `pageBreak`, `columnBreak`, `merged`가 실패 문단에는 없다.
- 실제 문단에 있는 `linesegarray`가 실패본에는 없다. 그러나 s4 캐시 제거본도 Windows에서 열렸으므로
  **캐시 부재 자체를 이번 충돌의 충분조건으로 볼 근거는 없다**. 새 fixture도 낡은 빈 문서 캐시는 제거한다.

### 표

- 실제 `tbl`의 `id`, `zOrder`, `numberingType`, `textWrap`, `textFlow`, `lock`, `dropcapstyle`,
  `pageBreak`, `repeatHeader`가 실패본에서 생략되어 있다. **공통 shape 속성 누락은 가설**이다.
- `sz`, `pos`, `outMargin`, `inMargin`, `tr` 및 셀의 `subList`, `cellAddr`, `cellSpan`, `cellSz`,
  `cellMargin`은 실패본에도 있다. rowCnt=1/colCnt=2, 주소 (0,0)/(0,1), span=1x1과
  폭 21000+21000=42000도 서로 맞는다. 이 기본 셀 주소/크기 불일치는 원인으로 확인되지 않았다.
- 셀 안 문단에는 위의 공통 문단 속성이 누락되어 있다.

### 그림·미디어·네임스페이스

- 실패 `pic`에는 id, sz, pos, imgDim, hc:img만 있다. 실제 저장본의 `offset`, `orgSz`, `curSz`,
  `flip`, `rotationInfo`, `renderingInfo`의 3행렬, `imgRect`의 4점, `imgClip`, `inMargin`,
  빈 `effects`, `outMargin` 및 shape 공통 속성들이 없다. **그림 기본 구조 누락도 유력 가설**이다.
  shapeComment는 실제 저장본에 있지만 설명용 정보이며 새 합성본에 사용자 설명을 복제하지 않는다.
- 실패본의 `binaryItemIDRef=image1` → HPF `image1` → `BinData/image1.png` 연결은 정상이다.
  실제 PNG 시그니처와 `image/png`도 일치한다. BinData 미등록/형식 불일치는 발견하지 않았다.
- 한컴 root는 ha/hp/hp10/hs/hc/hh/hhs/hm/hpf/dc/opf/ooxmlchart/epub/config 등의 선언을
  일괄 유지한다. 실패본은 사용한 접두어 위주다. 사용 접두어의 미선언은 없고 XML 파싱도 된다.
  **사용하지 않는 namespace 선언 생략이 native 충돌 원인이라는 근거는 아직 없다.**

## 구현 변경

`evaluate_mcp.py`는 공개 MIT `tests/fixtures/corpus/ref_empty.hwpx`를 패키지·헤더·구역 seed로
사용한다. 출처/라이선스/해시는 기존 corpus manifest에 있으며 seed SHA-256은
`c58144645069f7d1258e91404730618ad568bc4d47680ad5f891d3050aa308c7`이다.
개인 문서를 generator의 의존성으로 넣지 않았다. 완전한 참조 목록과 native 구역 기본값을 유지하고,
테두리 id=3을 itemCnt와 함께 추가하며, 표·그림의 기본 shape 필드를 명시했다.
HPF에 그림을 등록하고 public author/date metadata는 평가용 metadata로 바꾼다.
Preview 이미지는 seed의 빈 문서 preview로서 새 조판 근거로 사용하지 않는다.

엔진에서 확인된 별도 결함: `hae-model::picture_block_reason`가 빈 `<hp:effects/>`도 효과가 있는
그림으로 질의했다. 편집 연산과 독립 verifier는 이미 빈 컨테이너를 허용했으므로, 질의 결과와
실제 지원 범위가 달랐다. 빈 컨테이너만 허용하고 자식/속성/비공백 텍스트가 있는 effects는 계속
거절하도록 수정했다. 이 결함은 **질의의 잘못된 거절**이며 1차 native 충돌 원인으로 주장하지 않는다.
회귀 시험은 질의의 editable 판정, 교체 성공, 원래 클립·표시 크기 보존, 효과 있는 그림 거절을 확인한다.

## Windows 2차 fixture 대조군

위치: `spikes/work/windows-round2/fixture_variants/` (11개). 모두 native `not_checked`.

- `00_failed_original`: 1차 실패 바이트 그대로.
- `v_0_old_text_only`: 실패본에서 표/그림 문단만 제거. 불완전한 패키지/헤더는 유지.
- **`v_a_minimal_paragraph`**: 완전한 public seed 구조에 텍스트 문단 하나만. 가장 먼저 확인한다.
- `v_b_plus_table`: v_a에 완전한 표만 추가.
- `v_c_plus_picture`: v_a에 완전한 그림만 추가.
- `v_d_all_corrected`: 표·그림 모두 추가. MCP 94개 평가에 사용한 입력과 바이트 동일.
- `h_package_only`: 기존 header/section을 유지하고 패키지 기반만 교체.
- `h_header_only`: 실패본의 header만 교체. 기존 section outline=0 불일치는 의도적으로 남음.
- `h_section_defaults_old_shapes`: 수정 패키지/헤더/구역/문단에 기존 표·그림을 삽입.
- `h_old_tbl`, `h_old_pic`: v_d에서 표 또는 그림 하나만 실패본 구조로 되돌린 대조군.

v_a 성공 후 v_b/v_c에서 실패가 갈리면 해당 shape 구조/배치 후보를 좁힌다.
v_d 성공과 h_old_pic 실패가 함께 나타나면 그림 구조 가설을 강화하지만 단일 누락 필드는 추가 분리가
필요하다. v_a부터 실패하면 패키지/헤더/구역 후보가 남는다. 모든 대조군의 통과/실패는 Windows에서
기록해야 하며 현재 Linux 결과로 예측 판정을 채우지 않는다.

## 실제 한컴 문서에 엔진 적용

위치: `spikes/work/windows-round2/engine_on_user_fixtures/` (8개).

- `01_text_edited.hwpx`: 본문 문단 텍스트를 `R2 확인` 문장으로 교체, 가운데 정렬·줄간격 180%.
- `02_table_edited.hwpx`: 4셀 모두 원래 문단이 3개다. cell.setText는 `cell_paragraph_count`로 거절.
  첫 셀의 첫 문단만 text.replace/paragraph.format으로 편집했다. 셀마다 문단 개수 3은 유지한다.
  `R2 확인`이 있고 가운데/180%다. 다른 두 빈 문단을 지워 지원 범위에 맞추지 않았다.
- `03_picture_글자처럼취급_direct_edited.hwpx`: 원본을 그대로 엔진으로 열어 image.replace만 적용.
  본문 텍스트가 없어 검색 표식은 없다. 그림의 위치/크기/클립은 보존한다.
- `03_picture_글자처럼취급_prep.hwpx`: **엔진 출력이 아닌 준비 사본**.
- `03_picture_글자처럼취급_prep_edited.hwpx`: 위 prep를 엔진으로 열어 가운데/180%와 그림 교체 적용.
  표식은 prep 단계에서 들어간 것이다. 엔진이 빈 문단에 텍스트를 삽입했다고 해석하지 않는다.
- `04_complex_passthrough.hwpx`, `04_complex_after_refusals.hwpx`: 원본과 파일 바이트가 동일.
  수식 node에 text.replace를 보내면 지원하지 않는 kind로 거절하고, 병합 셀 cell.setText도 거절한다.
  별도 수식 편집 명령이 존재하거나 수식 편집을 지원한다는 뜻이 아니다.
- `04_complex_supported_paragraph_edited.hwpx`: control이 없는 일반 문단 하나만 `R2 확인`으로 교체하고
  가운데/180% 적용. 수식 15, 차트 1, 사각형 2, 다각형 6, fieldBegin 6, fieldEnd 6, 머리말 1의
  총 37개 XML subtree를 독립 Python lexer로 비교해 **바이트 동일** 확인. Chart/BinData/memoExtended
  package payload도 동일하다. 복합 요소 전체의 렌더링/native 동작은 미검증이다.

그림 교체 PNG는 로컬 합성 160x80이며 SHA-256은
`5d9d2ae7550249c0eed98d6f9d56be4fe4f39b162a24fa5b9ae8525db1c04683`이다.
사용자 원본 5개의 실행 전후 해시가 일치한다(`originals-preserved.json`).

### 승인된 엔진 밖 prep 단계의 정확한 변경

원본 inline 그림 HWPX SHA-256:
`3e5ca8e8d4e61d25325eb460193de06b839dd8f4fda1454898a28526ce2f2fbb`.
준비 사본 SHA-256:
`ebf9f383e973ec611da10389c5775f59be99e1c495787eeda7c2c89dce9cafb4`.

`Contents/section0.xml` 압축 해제 payload의 0-based byte offset **4766**에서 유일한
`<hp:t/>` 7바이트를 다음 UTF-8로 교체했다:

```xml
<hp:t>R2 확인 그림 교체·가운데 정렬·줄간격 180%</hp:t>
```

구역 payload hash는 `8ed971ccbd1be213ba4242067fb615f6a95faa5fce4c8172f05d7b72d181c05a`에서
`ade3120430449f1287ea57a792e0a5e746618861d248b9c5dae897e3d9854e28`로 바뀌었다.
다른 모든 압축 해제 payload는 동일하다. Python zipfile로 기록했으므로 ZIP 압축 스트림과
local/central record의 바이트 동일성은 주장하지 않는다. 생성 경로는
`prepare_windows_round2.py::rewrite`이며 엔진 호출 전 단계다(`engine-results.json`의
`NON_ENGINE_PREPARATION`). **빈 문단 텍스트 삽입은 0.2 후보이고 이번에는 구현하지 않았다.**

## 검증과 재현

- 새 Python 구조 회귀 검사 3개: 수정 전 3개 실패(beginNum, package infrastructure, footNotePr 부재),
  수정 후 통과. fontRef 7언어 참조, itemCnt/fontCnt, 문자/문단/스타일/테두리 참조도 검사한다.
- 빈 effects 회귀: 수정 전 질의가 `Some("effects")`로 실패, 수정 후 editable/교체/geometry 보존 통과.
- `python3 scripts/evaluate_mcp.py --output-dir spikes/work/windows-round2/mcp-evaluation`:
  **94 passed, 0 failed**, preview 전후 포함. nativeComparison=`not_checked`.
- `bash scripts/check.sh`: fmt/clippy/build, Python scripts **33개**, Rust **245개**, 실패 0.
  최종 로그 `spikes/work/windows-round2/check-final.log`.
- `python3 -m unittest discover -s tests/oracle`: **88개 통과**. 로그 `oracle.log`.
- `python3 scripts/prepare_windows_round2.py`: 11개 variant와 실제 문서 8개 산출물 생성 성공.
  새 MCP 프로세스에서 편집본 5개 재열기, 표식/가운데/180%/그림 해시/셀 문단 수 검사 통과.
  `reopen-verification.json`, `engine-responses.jsonl`, `reopen-responses.jsonl`에 근거를 남겼다.
- 동일 스크립트를 `--output-dir spikes/work/windows-round2-repro`로 새 경로에서 재실행했다.
  이는 로컬 재실행 검사이며 독립 환경/Windows 검증과 구분한다.

round2 루트의 `README.md`, `SHA256SUMS.txt`, `return/results.csv`와 pdf/resaved/screenshots
디렉터리는 Windows 확인에 사용할 수 있다. 작업 산출물과 사용자 입력/로그는 `.gitignore`의
`spikes/work/` 아래 로컬 자료로 남기고 Git에는 generator/회귀 시험/이 결정 문서만 커밋한다.
기존 `model-roles.md`, `scripts/windows/tools/step1_environment.ps1`의 다른 작업 수정은 제외한다.
커밋 후 push는 총괄 확인 범위다.
