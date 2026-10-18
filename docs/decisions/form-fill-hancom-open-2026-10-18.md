# form.fill 산출물 한컴 열기 확인 (2026-10-18)

작성: 2026-10-18, `feat/formfill` 워크트리. 코드·fixture 원본은 이 확인 때문에 바꾸지 않았다.
방법은 [엔진 산출물 한컴 열기 검증 §1](engine-output-open-verification-2026-10-17.md)과
[MCP 산출물 한컴 열기 확인](mcp-output-hancom-open-2026-10-18.md)을 따랐다.

## 0. 결론

- **`form.fill`로 만든 산출물 1종이 Linux 한컴 2020 Beta에서 대화상자 없이 열렸고, 네 라벨(성명·소속·직위·연락처)과 채운 값 네 개가 화면에 보인다.** 오류·복구 안내·외부 참조 대화상자는 없었다.
- 표와 셀 폭은 합성 서식의 `hp:cellSz` 값 그대로다. 소속·연락처 값은 좁은 열에서 두 줄로 줄바꿈되지만 원본 표 폭과 일치한다.
- 한계: 이 한컴 빌드는 첫 화면 캡처 뒤 SIGSEGV로 종료되어 화면 1장만 얻었다(기존 기록과 동일). Windows 한컴오피스 2024 실기 대조는 하지 않았으므로 `nativeComparison` 판정은 이 문서 범위 밖이다.

## 1. 환경과 입력

| 항목 | 값 |
|---|---|
| 한컴 | hoffice-hwp 11.20.0.989, `/tmp/opencode/hoffice-run/opt/hnc/hoffice11/Bin/hwp` |
| 실행 | `QT_QPA_PLATFORM=vnc:port=<N>:size=1280x1000`, `env -i`, `HOME`·`TMPDIR` 격리, 캡처는 `/tmp/opencode/hoffice-probe/vnccap.py` |
| 엔진 | `feat/formfill` 워크트리의 `hae` CLI(debug). SHA-256 `d5531cf7fdce8e315ba4333db10904d7d2b2eaeb788c06077846fffaa0b1c7be` |
| 입력 서식 | 합성(개인 문서 아님). `tests/fixtures/editing/simple_cell.hwpx`의 머리말·골격을 쓰고 구역만 "교직원 인적사항 카드" 4행×2열 표(라벨 열 + 빈 값 열)로 바꿔 생성 |
| 산출물 | `/tmp/opencode/formfill-gate/out/form-filled.hwpx`, SHA-256 `d7832019162982f9d4f6eff0a26f16dccc71fcedf63598f1945e2009a1c5cb72` |

## 2. 실행 기록

`hae script` 한 세션에서:

1. `document.open` → rev 0, editable.
2. `form.fill` `values: {"성명":"홍길동","소속":"○○고등학교","직위":"교사","연락처":"010-1234-5678"}` → 적용, rev 0→1. `filled` 4건(성명·소속·연락처·직위), `skipped` 0건.
3. `document.export` → 성공. 저장 관문(`hae-verify` 독립 재도출)을 통과한 파일만 기록된다.

## 3. 판정

| 파일 | 대화상자 | 편집 텍스트 | 잘림·겹침 | 판정 |
|---|---|---|---|---|
| 산출물 `form-filled.hwpx` | 없음 | 제목 `교직원 인적사항 카드`, 표에 `성명`·`홍길동`, `소속`·`○○고등학교`, `직위`·`교사`, `연락처`·`010-1234-5678` | 값 열 폭이 좁아 소속·연락처가 두 줄로 줄바꿈(원본 합성 폭과 동일, 겹침 없음). 쪽 `1/1` | **열림 통과, 값 채움 확인** |

캡처: `/tmp/opencode/formfill-gate/cap2.t1.png`(1280×1000, 첫 프레임).
