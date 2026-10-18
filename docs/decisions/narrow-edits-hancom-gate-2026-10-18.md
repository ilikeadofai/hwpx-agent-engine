# 0.3 좁은 편집 명령 한컴 게이트 (2026-10-18)

작성: 2026-10-18. 브랜치 `feat/edits`. 방법은
[엔진 산출물 한컴 열기 검증](engine-output-open-verification-2026-10-17.md)과
[MCP 산출물 한컴 열기 확인](mcp-output-hancom-open-2026-10-18.md)을 따랐다.
`scripts/hancom_gate.sh`(offscreen 생존 판정)와
`spikes/work/hancom-oracle/vnccap.py` 방식의 VNC 첫 프레임 캡처를 함께 썼다.
캡처·산출물·로그는 저장소 밖(`/home/mattc/.cache/tmp-check/gate/`)에만 두고 커밋하지 않는다.

## 0. 결론

- `paragraph.setStyle`·`image.fit`·`header.setText` 산출물 6종이 **Linux 한컴 2020 Beta에서
  대화상자 없이 열렸고 검증 게이트(생존 판정)를 6/6 PASS**했다.
- `image.fit` 산출물은 화면에서 그림 프레임 변화가 보인다. 2×1 픽셀 그림을 1200×1200
  HWPUNIT 정사각 프레임에 맞춘 `contain` 결과가 **가로 1200×세로 600** 붉은 사각형으로
  그려졌다(원본은 1200×1200 정사각). 그래픽이 깨지거나 사라지지 않았고 렌더링 행렬이
  사라져 생기는 오류 대화상자·빈 상자도 없었다.
- `header.setText` 산출물은 페이지 머리말의 첫 문단이 `Header A`에서 `게이트 머리말`로
  바뀐 것이 화면에 보인다. 나머지 머리말 문단·본문은 그대로다.
- `paragraph.setStyle` 산출물은 오류 없이 열렸다. 두 스타일(`Style0`·`Style1`)이 같은
  `paraPr`·`charPr`를 참조해 화면 차이는 원래 없다. `styleIDRef` 값 변화는 아래 시험과
  독립 검증기로 확인했다.
- 한계: 이 관찰은 Linux 2020 Beta(11.20.0.989) 한 빌드의 첫 프레임이다. Windows
  한컴오피스 2024 재열기·저장·전체 조판 대조는 아니다. `nativeComparison`은 여전히
  `not_checked`다.

## 1. 환경

| 항목 | 값 |
|---|---|
| 한컴 | hoffice-hwp 11.20.0.989, `/tmp/opencode/hoffice-run/opt/hnc/hoffice11/Bin/hwp` (sha256 `059ee44b…`) |
| 게이트 스크립트 | `scripts/hancom_gate.sh`, `QT_QPA_PLATFORM=offscreen`, 대기 15초, SIGSEGV(rc 139)만 실패로 판정 |
| 캡처 | VNC(`QT_QPA_PLATFORM=vnc:port=5988:size=1280x1000`) + `/tmp/opencode/hoffice-probe/vnccap.py` (sha256 `7856d0e5…`), 실행마다 `HOME`·`TMPDIR` 임시 격리, 실행한 프로세스 그룹만 종료 |
| 엔진 | `feat/edits` HEAD(이 커밋 직전 WIP `e10d749` + fixture `hp:ctrl` 수정), `cargo build` 디버그 바이너리 `hae`로 CLI `script` 세션 실행 |
| 입력 | `tests/fixtures/editing/narrow_edits.hwpx` (합성, sha256 `3950da55…`), `tests/fixtures/corpus/gyehoek-reference.hwpx` (공개 코퍼스, sha256 `554470ec…`) |

## 2. 산출물과 SHA-256

| 명령 | 파일 | SHA-256 |
|---|---|---|
| `paragraph.setStyle` (2문단, styleId 1) | `narrow-setstyle.hwpx` | `3c56ed3e0b762908f3ffdb7c0ebff9963697bf900b9d3e527735f9dbf9185e72` |
| `header.setText` (머리말 0번 문단) | `narrow-header.hwpx` | `9ea8e0743017f70739c6955a83caf9cc5e9ae9e9a9ee99966110e80c678e22d7` |
| `image.fit` contain | `narrow-fit-contain.hwpx` | `425168d6ca3699bc5230dd28a7c6ff26dfd60e5b3f988d6d9667b7240bd96624` |
| `image.fit` contain (코퍼스, 425×117px) | `corpus-fit-contain.hwpx` | `825f9e73f8c8dc064f14f7af31cf725ae5901e755eff6e5479db73ebc1651c5e` |
| `image.fit` cover (코퍼스) | `corpus-fit-cover.hwpx` | `d7571643c7429e2ab4b012817bc2ecb44216181c4e9956a1219c741c505ef962` |
| `image.fit` stretch (코퍼스) | `corpus-fit-stretch.hwpx` | `1af2ed36a8f3aa4a66f4d72d2599d3561d2541d8b91a1ae6e2fdac1c50fb812a` |

생성 방법: `hae script` 한 세션에 `document.open` → 대상 조회 → 명령(토큰 `$revision` 사용) →
`document.export`. `narrow-*`는 각 명령만 원본에 적용한 별도 세션이다. `corpus-fit-*`는
`hae-exec` 시험 `fit_real_corpus_three_modes_export_and_exact_undo`가 `target/evidence/`에
쓰는 바이트와 SHA가 같다.

기하(산출물 XML 실측):

| 모드 | `sz`=`curSz` | `imgDim` | `imgClip` | `imgRect.pt2`=`orgSz` |
|---|---|---|---|---|
| narrow contain | 1200×600 | 150×75 | 0,0–150,75 | 1200×1200 |
| corpus contain | 19750×5437 | 31875×8775 | 0,0–31875,8775 | 31860×8760 |
| corpus cover | 19774×5437 | 31875×8775 | 0,5–31875,8769 | 31860×8760 |
| corpus stretch | 19774×5437 | 31875×8775 | 0,0–31875,8775 | 31860×8760 |

세 산출물 모두 `transMatrix`·`scaMatrix`·`rotMatrix`를 그대로 보유한다(존재·개수 확인).
원본 코퍼스의 `imgDim`(31860×8760)은 425×117px의 75 HWPUNIT/px 값(31875×8775)과 달랐고
`img.fit`이 규칙대로 다시 계산했다.

## 3. 판정

게이트(offscreen 생존):

| 파일 | 판정 |
|---|---|
| narrow-setstyle.hwpx | PASS(생존) |
| narrow-header.hwpx | PASS(생존) |
| narrow-fit-contain.hwpx | PASS(생존) |
| corpus-fit-contain.hwpx | PASS(생존) |
| corpus-fit-cover.hwpx | PASS(생존) |
| corpus-fit-stretch.hwpx | PASS(생존) |

화면(첫 프레임 판독):

| 파일 | 대화상자 | 판독 |
|---|---|---|
| narrow 원본 | 없음 | `Header A`·`Header B`·`Number`·`Margin` 머리말, `Body A`·`Body B`, 붉은 정사각 그림(1200×1200), `Page 1/1` |
| narrow setStyle | 없음 | 원본과 같은 조판. 두 스타일이 동일 서식이라 화면 차이 없음(설계대로) |
| narrow header | 없음 | 머리말 첫 줄이 `게이트 머리말`로 표시. `Header B`·`Number`·`Margin`·본문·그림 그대로 |
| narrow fit contain | 없음 | 붉은 그림이 가로로 긴 1200×600 사각형으로 표시. 머리말·본문 그대로 |
| corpus 원본 | 없음 | 1쪽은 빈 면(표지), `Page 1/19` |

캡처 SHA-256:

| 파일 | SHA-256 |
|---|---|
| `narrow-original.png` | `112872d031d64d2367cf4cdf449cceadfa80a278b1e6e3b92916685f45157514` |
| `narrow-setstyle.png` | `24853a19288c8de49dfcc7cf77eaecf556d2dda27abf6ab5079dfed52541223b` |
| `narrow-header.png` | `29e48dca45e5c6441976cfa551f7911e0a71611d29fe0616642f28e887677c60` |
| `narrow-fit-contain.png` | `bca5f2994d2be8845cc82afa1f728fd64923c2d4aeea4663526a7b329b3cff9f` |
| `corpus-original.png` | `a940bf527a8e5740bae24711aaf99b1e0a9da092b31d6f87a5c2c349c70b3cbb` |

## 4. fixture 관찰 (기록)

처음 만든 `narrow_edits.hwpx`는 머리말·꼬리말을 `hp:run` 바로 아래에 두었다. 이 파일은
한컴이 오류 없이 열었지만 **머리말을 화면에 그리지 않았다.** 실제 HWPX(공개 코퍼스
`bodojaryo-reference.hwpx`)는 `<hp:run><hp:ctrl><hp:header>…` 모양이므로 합성기의
`hf()`를 `hp:ctrl`로 감싸 다시 생성했다(새 sha256 `3950da55…`, `--check-determinism`
통과). 그 뒤 원본·게이트 산출물 모두 머리말이 화면에 그려졌다. 이 수정은 fixture만
바꾸며 엔진 코드·기대값은 바꾸지 않는다. 목록 본문(번호·탭 제어 문단)은 그대로다.

## 5. 한계

- Linux 2020 Beta 한 빌드의 관찰이다. Windows 한컴오피스 2024(13.0)가 아니다.
- 화면은 첫 프레임 한 장이고, 문서에 따라 프로세스가 캡처 뒤 SIGSEGV로 끝난다(원본도
  같음 — [엔진 산출물 검증 §5.8](engine-output-open-verification-2026-10-17.md)의 VNC
  플러그인 문제). 쪽 이동·저장 상호작용은 시험하지 않았다.
- `corpus-fit-*`의 기하 변화는 코퍼스 그림에서 0.12% 미만이라 캡처로 구분하지 않았다.
  모드별 기하는 산출물 XML 실측과 `hae-verify` 독립 재계산 시험으로 확인한다.
- 스타일 두 개가 같은 서식이라 `paragraph.setStyle`의 화면 차이는 없다. 값 변화는
  `narrow_edits` 시험과 독립 검증기(변조 거절 6종)로 확인한다.
