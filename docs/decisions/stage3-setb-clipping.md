# Stage 4 MCP 시나리오 1쪽 미리보기 찌그러짐 — 원인과 수정

- 날짜: 2026-10-17
- 대상: `scripts/evaluate_mcp.py` 의 합성 문서 (`spikes/work/stage4-linux/2026-10-17-haiku-final/`)
- 한컴 조판 호환은 주장하지 않는다(`nativeComparison: not_checked`).

## 증상

Gemini 3.8 Flash High 가 실제 픽셀을 보고 지적: `haiku-final/artifacts/preview-{before,after}-page1.png`
(= Windows 인계 `linux_reference_png/{before,after}.png`)가 편집 전부터 내용이 상단 약 15px 에 몰리고,
글자가 y=0 에 닿으며, 표 셀 글자가 잘리고, 글자와 도형이 겹치고, 쪽의 98% 이상이 비어 있다.
반면 실제 코퍼스 문서 `bodojaryo-reference.hwpx` 미리보기(`stage3-final-visual-2026-10-16/*.png`)는 정상.

## 증거

| 항목 | 결과 |
|---|---|
| 입력 문서 | `haiku-final/inputs/eval-document.hwpx` = Windows 인계 `engine_inputs/01_original.hwpx` (SHA-256 `b63ecac8…c599`, 바이트 동일). 02~04 도 이 시나리오의 `applied/undo/redo` export 와 바이트 동일 |
| 입력 구조 | `Contents/section0.xml` 에 `hp:secPr`/`hp:pagePr`/여백이 **없고**, `hp:tbl`·`hp:tc` 에 `sz`/`cellSz`/`pos` 도 없다 |
| rhwp 직접 호출 (`rhwp export-svg INPUT -o DIR --json`) | 엔진 미리보기 SVG 와 **바이트 동일**. 문단 `y=10.2`, 글자 x 시작 0, 셀 클립 `24×5.33px` |
| 엔진 SVG → PNG | 794×1123, viewBox = width/height 로 정상. PNG 잉크 경계 `(0,0)-(82,11)`, 잉크 비율 0.05% |
| 코퍼스 `before.png` | 잉크 경계 `(75,94)-(718,1061)`, 잉크 비율 7.8% |

즉 rhwp 가 입력 그대로를 그렸고(엔진 경로가 왜곡하지 않음), 입력이 비현실적이었다. 원인 (b)·(c) 는 아니다.

## 원인

(a) 합성 입력 결함. 쪽 설정(`secPr/pagePr`)이 없어 rhwp 가 여백 0 으로 조판하고, 표·셀 크기가 없어
셀이 0에 가까운 크기로 그려져 글자가 클립되며, 그림도 크기 0 위치에서 글자와 겹쳤다.
엔진 코드(`hae-render-rhwp`, `hae-raster`)의 결함이 아니므로 엔진 소스는 바꾸지 않았다.

## 수정 (`scripts/evaluate_mcp.py`)

1. 합성 문서를 현실적으로: 첫 문단에 코퍼스 `ilban-gian.hwpx` 와 같은 A4 세로 `secPr/pagePr`(여백 좌우 8504·상 5669·하 4252),
   글자처럼 취급 표(`sz`/`pos`/안·바깥 여백, 셀 `cellSz` 21000×2400, 실선 `borderFill`), 14400×7200 글자처럼 취급 그림.
   헤더에 `borderFills` 추가. 시나리오 단언(셀 텍스트 변경, 가운데 정렬 + 줄간격 180%, 그림 교체, undo/redo 바이트 동일)은 그대로.
2. 회귀 검사 추가(`layout_checks`, 표준 라이브러리 PNG 디코더 `png_ink_bbox`): 미리보기 PNG 의 잉크 경계가
   네 가장자리에서 최소 10px(쪽 짧은 변의 2%) 이상 떨어져 있고 잉크 높이가 쪽 높이의 10% 이상, SVG `viewBox` 가 `width`/`height` 와 같을 것.
   기존 PNG 에 적용하면 `y0=0` 이므로 실패한다(회귀를 잡는다).
3. `tools_input_schemas` 검사: Sol 의 `documentId` 선택 인자 변경(다른 작업)에 맞춰 `required` 대신 `properties` 포함 여부로 확인.

## 결과

- `python3 scripts/evaluate_mcp.py --output-dir spikes/work/stage4-linux/2026-10-17-setb-fix` → `status=passed passed=94 failed=0`
- `bash scripts/check.sh` → 통과(exit 0)
- 새 PNG 잉크 경계: before `(114,152)-(677,313)`, after `(113,152)-(677,315)`, 잉크 비율 약 2.3%. 가운데 정렬 문단, 실선 표(`바뀐셀`), 그림이 서로 겹치지 않는다.

| 파일 | SHA-256 |
|---|---|
| `spikes/work/stage4-linux/2026-10-17-setb-fix/artifacts/preview-before-page1.png` | `2ca82fdc890ebac2399fd3b6c68a33d2037c920809771655ecb97661e2eafb8d` |
| `spikes/work/stage4-linux/2026-10-17-setb-fix/artifacts/preview-after-page1.png` | `691400f02dddbd85614cf3cd411f28373044f4c21d5bd3fee1e9119274e85ec2` |
| `spikes/work/stage4-linux/2026-10-17-setb-fix/inputs/eval-document.hwpx` (= `handoff_engine_inputs/01_original.hwpx`, `03_undo.hwpx`) | `191cd49fb03461093f7fedec02874bfb53a98d278ef4405a3bc94afc01a7d816` |
| `handoff_engine_inputs/02_applied.hwpx`, `04_redo.hwpx` | `56b335ae9ca88dde04f55c06e1dfdf21ddf4f610b63bdf6575a1cad6088c659e` |

## 인계 폴더

Windows 인계 `engine_inputs`는 이 시나리오의 산출물이었다. 인계 폴더(`windows-handoff-20261017/`)는 건드리지 않고
`spikes/work/stage4-linux/2026-10-17-setb-fix/handoff_engine_inputs/`(01~04 + `SHA256SUMS.txt`)에 새로 만들었다.
인계 폴더의 기존 `engine_inputs`와 `linux_reference_png`는 결함 있는 입력 기반이므로, Sol 이 새 폴더로 교체하거나
기존 참조 PNG 를 "결함 입력 기반" 으로 표시해야 한다. 이전 `haiku-final` 실행 폴더는 증거로 보존했다.
