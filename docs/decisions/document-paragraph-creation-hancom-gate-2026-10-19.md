# 0.4 문단·문서 생성 한컴 게이트 (2026-10-19)

작성: 2026-10-19. 브랜치 `feat/create`. 방법은
[엔진 산출물 한컴 열기 검증](engine-output-open-verification-2026-10-17.md)과
[표 구조 편집 한컴 게이트](table-structure-hancom-gate-2026-10-18.md)를 따랐다.
설계·규칙은 [문서·문단 생성 설계](document-paragraph-creation-design.md) §5·§7·§9에 있다.
산출물·로그·화면은 저장소 밖(`/home/mattc/.cache/tmp-check/gate-paragraph/`)에만 두고
커밋하지 않는다.

## 0. 결론

- `document.create` 산출물(`created.hwpx`)이 **Linux 한컴 2020 Beta에서 SIGSEGV 없이 열렸고
  (PASS), 화면에 세 문단이 그대로 그려졌다.** 빈 문서가 아니다 — `version.xml` 함정
  (`application="Hancom Office Hangul"` + `tagetApplication="WORDPROCESSOR"` + `major=5`)을
  지킨 결과다.
- `paragraph.insert` 산출물(생성 문서에 2문단 삽입)이 **PASS**했고, 화면에서 삽입한 문단이
  대상 문단 **뒤에 두 번** 그려졌다. 문단 모양·글자 모양이 복제되어 주변과 같은 서식이다.
- `paragraph.delete` 산출물(생성 문서에서 셋째 문단 삭제)이 **PASS**했고, 화면에서 그 문단만
  사라지고 앞 두 문단이 남았다.
- 공개 코퍼스(`gani-gian.hwpx`) 첫 문단 뒤 삽입과 합성 픽스처(`narrow_edits.hwpx`)의 평문
  문단 삭제도 **PASS**했고, 코퍼스 화면에서 새 문단이 표 아래에 그려지고 나머지 서식(표·칸)이
  그대로였다.
- 판정은 `scripts/hancom_gate.sh`의 offscreen 생존 판정 + VNC 첫 프레임 판독이다
  (`/tmp/opencode/hoffice-probe/vncshot.sh`). **Windows 한컴오피스 2024 실기, 저장 후 재열기,
  전체 조판 대조는 미검증**이다.

## 1. 환경

| 항목 | 값 |
|---|---|
| 한컴 | hoffice-hwp 11.20.0.989, `/tmp/opencode/hoffice-run/opt/hnc/hoffice11/Bin/hwp` |
| 게이트 | `scripts/hancom_gate.sh`, `QT_QPA_PLATFORM=offscreen`, 대기 20초, SIGSEGV(rc 139)·조기 종료만 실패 |
| 캡처 | `QT_QPA_PLATFORM=vnc` + `hoffice-probe/vncshot.sh`(RFB 첫 프레임 → PNG), 실행마다 `HOME`·`TMPDIR` 격리 |
| 엔진 | `feat/create`의 디버그 `hae`(`cargo build`)로 `hae script` 실행 |
| 입력(코퍼스) | `tests/fixtures/corpus/gani-gian.hwpx` |
| 입력(합성) | `tests/fixtures/editing/narrow_edits.hwpx` |

## 2. 산출물과 SHA-256

명령은 모두 한 세션이다. `document.create`는 열린 문서 없이, 나머지는
`document.open → element.query → 명령 1회 → document.export` 순서이고
`expectedVersion`은 `$revision`이다.

| 명령 | 인자 | 파일 | SHA-256 |
|---|---|---|---|
| `document.create` | 제목 `0.4 생성 문서`, 문단 3개 | `created.hwpx` | `d4070fa0f45d17d9236f3958051a210bf21d03f1fd700a919fe9fe4f7eff124c` |
| `paragraph.insert` | 생성 문서, 1번 문단 뒤, `count:2`, 글 1개 | `inserted.hwpx` | `6919d96c7e84abde11f1e842442bd3fbe923c28a366ecbf8228535a2a12fefd2` |
| `paragraph.delete` | 생성 문서, 3번 문단, `count:1` | `deleted.hwpx` | `d822ffab64938adc7d44329913a5119f69c80a9e74be6c2756fb2df2aafb4fcb` |
| `paragraph.insert` | 코퍼스 `gani-gian` 1번 문단 뒤, 글 `게이트 삽입 문단` | `corpus-inserted.hwpx` | `fe8ac1a801c7ad03e6355b8c1b61ef1d338eecc34951ed07c8cb4ba54a15937f` |
| `paragraph.delete` | `narrow_edits.hwpx`의 `Body B`, `count:1` | `plain-deleted.hwpx` | `0703f68f13eb5ecad07eff224e9e8a8e0cb6449f39136bbc72c9e23af6a655e4` |

생성 문서는 3,197바이트, 항목 6개(`mimetype`·`version.xml`·`META-INF/container.xml`·
`Contents/content.hpf`·`Contents/header.xml`·`Contents/section0.xml`)다. 같은 인자로 다시
만들면 같은 바이트가 나온다(시험 `create_never_overwrites_and_is_deterministic`).

## 3. 게이트 결과

```text
file                                           verdict
created.hwpx                                   PASS(생존)
inserted.hwpx                                  PASS(생존)
deleted.hwpx                                   PASS(생존)
corpus-inserted.hwpx                           PASS(생존)
plain-deleted.hwpx                             PASS(생존)
```

## 4. 화면 판독 (첫 프레임)

| 산출물 | 화면에서 본 것 |
|---|---|
| `created.hwpx` | A4 세로 문서에 `첫 문단입니다.` / `둘째 문단입니다.` / `셋째 문단입니다.` 세 줄. 기본 글꼴(함초롬바탕 10pt)·기본 문단 모양. 빈 문서·오류 대화상자 없음. |
| `inserted.hwpx` | `첫 문단입니다.` 다음에 `삽입된 문단입니다.`가 **두 번**, 그 뒤에 `둘째`·`셋째`가 이어짐. 삽입 문단이 주변과 같은 서식. |
| `deleted.hwpx` | `첫 문단입니다.` / `둘째 문단입니다.`만 남고 셋째 줄이 사라짐. |
| `corpus-inserted.hwpx` | 공개 서식의 첫 표 아래에 `게이트 삽입 문단`이 그려지고, 오른쪽 표·아래 칸·테두리가 원본대로 남음. |

## 5. 한계

- Linux 한컴 2020 Beta 한 빌드의 첫 프레임이다. Windows 한컴오피스 2024 실기, 저장 후
  재열기, 인쇄·조판 대조는 하지 않았다(`nativeComparison: not_checked`).
- 생성 문서 골격은 `settings.xml`·`META-INF/manifest.xml`·`Preview/*`를 만들지 않는다.
  Linux 게이트에서는 문제가 없었지만, Windows 1차 실패 분석이 지목한 "네이티브 구조 누락"
  가설을 완전히 반증한 것은 아니다.
- 삽입 문단은 `id` 속성이 없고 줄 배치 캐시도 없다. 한컴이 열 때·저장할 때 이를 어떻게
  정규화하는지는 저장 후 재열기로 확인하지 않았다.
- 새로 만든 문서에 표·그림·머리말을 함께 만드는 것은 범위 밖이다.
