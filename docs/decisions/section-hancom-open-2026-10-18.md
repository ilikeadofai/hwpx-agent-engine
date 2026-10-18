# 구역 편집 산출물 한컴 열기 확인 (2026-10-18)

작성: 2026-10-18, 브랜치 `feat/section`, 기준 HEAD `a4efed4`(+ 검증용 편집). push 없음.
방법은 [엔진 산출물 한컴 열기 검증 §1](engine-output-open-verification-2026-10-17.md)을 따랐다.

## 0. 결론

- `section.setPage`·`section.setPageNumbering`·`section.setVisibility`를 적용한 산출물 2종이
  Linux 한컴 2020 Beta에서 **SIGSEGV 없이 로드됐다(PASS)**. 대조군인 원본 `section.hwpx`도 PASS다.
- 이 판정은 **프로세스 생존만** 본다. 화면·인쇄 결과, 쪽 번호 표시, 가로 용지의 실제 조판,
  Windows 한컴오피스 2024 호환성은 이 기록으로 확정하지 않는다(`nativeComparison`: `not_checked`).

## 1. 환경

| 항목 | 값 |
|---|---|
| 한컴 | hoffice-hwp 11.20.0.989, `/tmp/opencode/hoffice-run/opt/hnc/hoffice11/Bin/hwp` (sha256 `059ee44b…`) |
| 실행 | `scripts/hancom_gate.sh`, `QT_QPA_PLATFORM=offscreen`, `HANCOM_GATE_WAIT=20`, 실행마다 `HOME`·`TMPDIR` 격리 |
| 생성 | `cargo test -p hae-exec --test section` + `HAE_SECTION_GATE_OUTPUT` (시험이 export 바이트를 그대로 저장) |

## 2. 파일 SHA-256

| 역할 | 시험 | SHA-256 |
|---|---|---|
| 원본 (`tests/fixtures/editing/section.hwpx`, 대조군) | `gen_section.py` | `166184cd082c32e845d5ff38fba3d133bd4c0f3da640a9a87a01bd8e561ef6c2` |
| 산출물 A (세 편집 batch) | `section_all_mutations_batch_is_one_undo_unit_and_query_is_read_only` | `fc03fadbc2564f6c93b26626682306ddc26212f415a84d3498b1f00818e8761a` |
| 산출물 B (용지·여백 단독) | `section_set_page_all_properties_and_only_target_section_change` | `e476f77a7851385fbee2ff27f6f10e9f55106ff732c8151981fd402181e3fa04` |

적용 결과(산출물 A, 구역 1): `pagePr height="85000"`, `margin right="5000"`,
`startNum page="12" pic="0" tbl="0" equation="0"`,
`visibility hideFirstPageNum="1"`, 해당 구역 `linesegarray` 0개.
산출물 B(구역 0): `pagePr landscape="NARROWLY" width="60000" height="85000" gutterType="TOP_BOTTOM"`,
`margin left="2000" right="3000" top="1000" bottom="1200" header="400" footer="500" gutter="100"`,
다른 구역 `Contents/section1.xml`·`Preview/PrvText.txt`는 원본과 바이트 동일.

## 3. 게이트 출력

```
file                                           verdict
section-batch-all-three.hwpx                   PASS(생존)
section-set-page.hwpx                          PASS(생존)
section.hwpx                                   PASS(생존)
```

## 4. 남은 확인

가로 용지·제본 방식별 시각 비교, 쪽 번호 표시, Windows 한컴오피스 2024 열기와 인쇄는
아직 확인하지 않았다. `document_preview`의 SVG/PNG는 외부 렌더러 추정치이므로 이 판정을
대체하지 않는다.
