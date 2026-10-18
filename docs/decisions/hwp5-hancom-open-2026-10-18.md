# HWP5 읽기 전용 조회 실물 문서 한컴 열기 확인 (2026-10-18)

작성: 2026-10-18, 브랜치 `feat/hwp5`. 방법은
[엔진 산출물 한컴 열기 검증 §1](engine-output-open-verification-2026-10-17.md)과
[구역 편집 산출물 한컴 열기 확인](section-hancom-open-2026-10-18.md)을 따랐다.

## 0. 결론

- `hwp5.inspect`가 읽는 실물 `.hwp` 표본 4개가 Linux 한컴 2020 Beta에서 **SIGSEGV 없이 로드됐다(PASS)**.
- 그중 2개는 **화면 캡처로 내용까지 대조**했다: 한컴이 그린 문단 텍스트가 우리 리더가 추출한
  `PARA_TEXT`와 일치했다(`필드-누름틀.hwp`: `AA테스트 누름틀ABCD 1234567` 등;
  `표.hwp`: `ABC`·`123`·`DEF`… 셀 text). `표.hwp`의 캡션 `표 1`에서 **숫자 `1`은 조판이 만드는
  자동 번호**이고 캡션은 표 아래에 그려진다(레코드 순서는 캡션이 셀보다 먼저) — 독립 구현 대조에서
  나온 차이가 화면으로 확인됐다.
- 이 명령은 **파일을 쓰지 않으므로 게이트에 걸 "산출물 문서"가 없다.** 이 게이트의 대상은 리더가
  소비한 실물 입력 문서 4개이며, 열기 전후 원본 SHA-256이 모두 같다.
- 한계: 게이트는 **프로세스 생존**만 판정하고, 화면 대조는 캡처 1장씩이다. Windows 한컴오피스 2024
  실기 대조는 하지 않았다(`nativeComparison`: `not_checked`).

## 1. 환경

| 항목 | 값 |
|---|---|
| 한컴 | hoffice-hwp 11.20.0.989, `/tmp/opencode/hoffice-run/opt/hnc/hoffice11/Bin/hwp` (sha256 `059ee44b99a4…`) |
| 게이트 | `scripts/hancom_gate.sh`, `QT_QPA_PLATFORM=offscreen`, `HANCOM_GATE_WAIT=30`, 실행마다 `HOME`·`TMPDIR` 격리 |
| 화면 캡처 | `/tmp/opencode/hoffice-probe/vncshot.sh`(`QT_QPA_PLATFORM=vnc` + 표준 라이브러리 RFB 클라이언트) |
| 입력 | `~/.cache/cga/repos2/neolord0_hwplib/sample_hwp/` 실물 표본. 게이트용은 `/tmp/opencode/hwp5-gate/inputs/`에 바이트 동일 사본을 만들어 열었다 |

## 2. 파일 SHA-256 (원본, 열기 전후 동일)

| 파일 | SHA-256 | 리더 결과 |
|---|---|---|
| `basic/필드-누름틀.hwp` | `649aff0734368fef32b74916033fab9b57d9fa4c7a2cdbc1e38f71dfe597e14b` | 5.0.5.0, 압축, 구역 1, 문단 20, 글자 3,508 |
| `basic/표.hwp` | `ed97e2136e53cbb09ab6362d5631f6f1e4f752a5e53f09e85e4bc9c19f9e6d35` | 5.0.3.4, 비압축, 구역 1, 문단 18 |
| `big_file.hwp` | `7ffb76fff1dc48e7686cf0a480c9c74f151ff88becef51362ffc4f4cdb12704d` | 5.1.0.1, 압축, 구역 1, 문단 147,456, 글자 1,769,472 |
| `distribution.hwp` | `76377f4c2a10981f382a4ba65ea3b2c9b771a28c6cc55c110ca8ead14f7cb01a` | 배포용 → `distribution_unsupported` 거절(읽지 않음). 파일 자체는 한컴이 연다 |

## 3. 게이트 출력

```
file                                           verdict
big_file.hwp                                   PASS(생존)
distribution.hwp                               PASS(생존)
필드-누름틀.hwp                                 PASS(생존)
표.hwp                                          PASS(생존)
```

(사이의 `Killed` 메시지는 스크립트가 판정 뒤 프로세스를 정리하며 출력하는 것이다.)

## 4. 화면 캡처 대조

| 캡처 | 화면에서 확인한 것 | 우리 추출과의 관계 |
|---|---|---|
| `/tmp/opencode/hwp5-gate/field-clickhere.png` | 본문 `AA테스트 누름틀ABCD 1234567`, 표 셀 `테이블안`, `누름틀은 여러줄 문자1/문자2/문자3 이다` | 문단 0 미리보기 `AA테스트 누름틀ABCD 1234567` 등과 문자 단위 일치 |
| `/tmp/opencode/hwp5-gate/table.png` | 표 셀 `ABC`·`123`·`DEF`·`GHI`·`LMN`·`OPQ`·`STR`·`UVM`·`123`·`456`, 표 아래 캡션 `표 1` | 셀 text는 추출과 일치. `표 1`의 `1`은 자동 번호(화면에서만) |

`표.hwp` 캡처 뒤 한컴 프로세스가 SIGSEGV로 종료됐다(기존 기록과 같은 이 빌드의 알려진 동작).
캡처는 그 전에 얻었고, 게이트 판정(프로세스 생존)에는 영향이 없다.

## 5. 남은 확인

- Windows 한컴오피스 2024에서의 열기·조판 대조, 암호 문서(표본 없음), 배포용 문서의 복호.
- 화면 캡처는 표본 2개만 했다. 나머지 41개는 `rhwp` 대조(형식 조사 노트 §4)까지다.
