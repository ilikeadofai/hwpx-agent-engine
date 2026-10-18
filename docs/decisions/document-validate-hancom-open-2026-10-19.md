# `document.validate` 조회 뒤 산출물 한컴 열기 확인 (2026-10-19)

작성: 2026-10-19, `feat/validate` 워크트리. 이 확인 때문에 코드·fixture 원본은 바꾸지 않았다.
판정 도구는 `scripts/hancom_gate.sh`(offscreen, "로드 후 프로세스 생존")이며
[엔진 산출물 한컴 열기 검증](engine-output-open-verification-2026-10-17.md)의 판정 규칙을 따른다.

## 0. 결론

- 코퍼스 문서 2종(`bodojaryo-reference.hwpx`·`gani-gian.hwpx`)에 `document.validate`를 실행한 뒤
  `document.export`한 산출물 2개가 **모두 PASS(생존)**다. `document.validate`는 파일을 바꾸지
  않으므로 export 바이트는 원본과 동일하다(`cmp` 일치, 아래 §2).
- `hh:charProperties@itemCnt`·`hh:fontface@fontCnt`를 일부러 콘텐츠와 어긋나게 만든 합성 문서도
  Linux 한컴 2020 Beta에서 **열렸다(PASS)**. 즉 `count_mismatch`의 `error`는 "한컴이 못 연다"는
  뜻이 아니라 "선언과 내용이 모순된 데이터이며 HAE 저장 관문은 그런 문서를 만들지 않는다"는
  계약 표시다([document-validate-2026-10-19.md](document-validate-2026-10-19.md) §4·§6).
- 한계: 이 게이트는 "열렸는가"만 본다. 조판·화면 일치, Windows 한컴오피스 2024 실기,
  경고·정보가 화면에 미치는 영향은 확인하지 않았다. `gani-gian` 게이트는 판정 뒤 hwp 프로세스를
  종료할 때 셸이 `Killed`를 찍었지만 판정 자체는 PASS(생존)였다.

## 1. 환경과 입력

| 항목 | 값 |
|---|---|
| 한컴 | hoffice-hwp 11.20.0.989, `/tmp/opencode/hoffice-run/opt/hnc/hoffice11/Bin/hwp` |
| 실행 | `scripts/hancom_gate.sh /tmp/opencode/hoffice-run/opt/hnc/hoffice11/Bin <산출물>` (`QT_QPA_PLATFORM=offscreen`, `HOME` 격리, 15초 대기) |
| 엔진 | `feat/validate` 워크트리 `hae` CLI(debug), `cargo build -p hae-cli` |
| 입력 | `tests/fixtures/corpus/bodojaryo-reference.hwpx`(실제 서식, 인용 참조 679), `tests/fixtures/corpus/gani-gian.hwpx`(실제 서식, 사전에 없는 요소 5종), 그리고 합성 `mismatch.hwpx`(`valid_base.hwpx`의 header에 `itemCnt=2`/`fontCnt=2`를 두고 자식을 1개만 둠) |

## 2. 실행 기록

`hae script` 한 세션(2026-10-19, `document.open` → `document.validate` → `document.export`):

| 파일 | `document.validate` 결과 | export 바이트 | 한컴 판정 |
|---|---|---|---|
| `bodojaryo-validated.hwpx` | 오류 0 · 경고 0 · 정보 2(`unknown_namespace` 2, `unset_reference` 10) | 원본과 동일(`cmp`) | **PASS(생존)** |
| `gani-gian-validated.hwpx` | 오류 0 · 경고 5 · 정보 2(`unknown_element` 5, `unknown_namespace` 2, `unused_resource` 1) | 원본과 동일(`cmp`) | **PASS(생존)** |
| `mismatch-validated.hwpx` | 오류 2(`item_cnt_mismatch`, `font_cnt_mismatch`) · 경고 0 · 정보 0 | (합성 입력과 동일) | **PASS(생존)** |

경로(로컬 작업 폴더, 저장소 밖): `/tmp/opencode/hoffice-run/validate-gate/{in,out}`,
스크립트 `steps.json`·`mismatch-steps.json`, 응답 원문 `responses.json`·`mismatch-responses.json`.
`document.validate`는 `mutatesContent:false`이고 응답 전후 `revision`은 0이었다.

## 3. 판정과 해석

- 조회 명령의 산출물 확인은 "조회가 문서를 망가뜨리지 않았는가"를 보는 시험이다. export 바이트가
  원본과 같으므로 한컴이 연 것은 **원본 문서**이고, 이는 조회가 파일에 흔적을 남기지 않았다는
  뜻이다(조회 자체의 한컴 검증이 아니다 — 읽기 전용이므로 검증할 산출물이 없다).
- `count_mismatch`를 `error`로 부르는 근거는 "한컴이 거절한다"가 아니다. 한컴 Linux Beta는
  이 합성 문서를 열었다. 근거는 (a) 코퍼스 10개 어디에도 불일치가 없다는 실측, (b) 같은 조건을
  HAE 저장 관문이 편집 시 거절한다는 계약([저장 검증 규칙](hwpx-verification-rules.md) §2.5·§2.6),
  (c) 선언 카운트와 실제 자식 수가 다르다는 사실 자체다. 한컴의 관용은 조판 정확성을 보증하지
  않는다.
