# 0.3 표 구조 편집 한컴 게이트 (2026-10-18)

작성: 2026-10-18. 브랜치 `feat/structure`. 방법은
[엔진 산출물 한컴 열기 검증](engine-output-open-verification-2026-10-17.md)과
[좁은 편집 명령 한컴 게이트](narrow-edits-hancom-gate-2026-10-18.md)를 따랐다.
설계와 규칙은 [표 구조 편집 설계](table-structure-node-creation-design.md) §6·§10에 있다.
산출물·로그는 저장소 밖(`/home/mattc/.cache/tmp-check/gate-structure-final/`)에만 두고
커밋하지 않는다.

## 0. 결론

- `table.insertRow`·`table.deleteRow`·`table.insertColumn`·`table.deleteColumn` 산출물
  4종이 **Linux 한컴 2020 Beta에서 SIGSEGV 없이 열렸다(4/4 PASS)**. 병합 셀이 있는 표의
  깨끗한 경계에서 행을 넣고 지운 산출물 2종도 **PASS(2/2)**다.
- 공개 코퍼스(`gani-gian.hwpx`)의 서로 다른 표 두 개에 행 삽입·삭제를 섞은 산출물도
  **PASS**했다. 합성 픽스처 밖의 실제 표에서도 `rowCnt`·셀 주소·`sz.height` 갱신이
  한컴을 죽이지 않았다.
- 판정은 `scripts/hancom_gate.sh`의 offscreen 생존 판정이다. 화면 캡처로 행·열이
  실제로 그려졌는지는 이번 게이트에서 확인하지 않았다(§4).
- 한계: Linux 2020 Beta 한 빌드다. Windows 한컴오피스 2024 실기, 저장 후 재열기,
  전체 조판 대조는 여전히 미검증이다.

## 1. 환경

| 항목 | 값 |
|---|---|
| 한컴 | hoffice-hwp 11.20.0.989, `/tmp/opencode/hoffice-run/opt/hnc/hoffice11/Bin/hwp` |
| 게이트 | `scripts/hancom_gate.sh`, `QT_QPA_PLATFORM=offscreen`, 대기 15초, SIGSEGV(rc 139)·조기 종료만 실패 |
| 엔진 | `feat/structure`의 디버그 `hae`(`cargo build`)로 `hae script` 실행 |
| 입력(합성) | `tests/fixtures/tables/structure.hwpx`, sha256 `0d77c0984d1af8908e14592360ae5383cc404e7b29fe6797c1d919e68198e7f4` |
| 입력(코퍼스) | `tests/fixtures/corpus/gani-gian.hwpx` |
| 스크립트 | `gate-structure-final/{insert-row,delete-row,insert-column,delete-column,merged-insert,merged-delete,real-gani-gian}.json` |

합성 입력의 표 0은 4행×3열, `sz` 42000×9600, 0행이 머리글이다. 셀마다
`cellAddr`·`cellSpan`·`cellSz`가 있고 레이아웃 캐시가 있다.

## 2. 산출물과 SHA-256

명령은 모두 `document.open` → `element.query {kind:"table"}` → 구조 명령 1회 →
`document.export` 한 세션이다. `expectedVersion`은 `$revision`이다.

| 명령 | 인자 | 파일 | XML 실측 | SHA-256 |
|---|---|---|---|---|
| `table.insertRow` | 표 0, `index:2`, `count:1` | `insert-row.hwpx` | 5×3, 42000×12000 | `80e1c3c8508c4630c140ff831fc6a316e6e2cd0bd7b31ce9475b95babcee4ad6` |
| `table.deleteRow` | 표 0, `index:1`, `count:1` | `delete-row.hwpx` | 3×3, 42000×7200 | `29eeab8e4888df363543b9392cb90a6beefa4cdb36e73352025bb65de7d61d0d` |
| `table.insertColumn` | 표 0, `index:1`, `count:1` | `insert-column.hwpx` | 4×4, 56000×9600 | `7fea0aa812e4cde0a40e0cfc1dcfbfc9114e1b8441b9650cc9e89133a0d195b8` |
| `table.deleteColumn` | 표 0, `index:1`, `count:1` | `delete-column.hwpx` | 4×2, 28000×9600 | `df6a65b290e4d1fb822e136ac8c0257c8b555cb78f99986f50a7b2a5f15b3fa1` |
| `table.insertRow` | 표 1(병합), `index:1` | `merged-insert.hwpx` | 5×3, 42000×12000 | `f4f313829eea411114696c24a927f85ceb713d02540a57750d72abfd9e3c9e68` |
| `table.deleteRow` | 표 1(병합), `index:1` | `merged-delete.hwpx` | 3×3, 42000×7200 | `1a9d6f8b2229b09d03853c8308b570f3d8c9e514ea20d3030ec3050e4747fda8` |
| 혼합(코퍼스) | 표 0 `insertRow 2` → 표 1 `insertRow 1` → 표 1 `deleteRow 2` | `real-gani-gian.hwpx` | — | `44c51fff599bd76f7423a20b6523a2805d8f06bdfc0b790d38f633ad93342e0c` |

기하 검산:

- 행 삽입: 템플릿 행 높이 2400 × 1 = 9600 + 2400 = **12000**. `rowCnt` 4 → 5.
- 행 삭제: 삭제한 1행 높이 2400 = 9600 − 2400 = **7200**. `rowCnt` 4 → 3.
- 열 삽입: 템플릿 열 폭 14000 × 1 = 42000 + 14000 = **56000**. `colCnt` 3 → 4.
  기존 열 폭은 줄이지 않는다(설계 §6.3).
- 열 삭제: 삭제한 1열 폭 14000 = 42000 − 14000 = **28000**. `colCnt` 3 → 2.

모든 산출물에서 `rowCnt`/`colCnt`·`tbl/sz`·각 셀의 `cellAddr`가 위 표와 일치함을
XML에서 확인했다. 이 일관성은 `hae-verify`가 원본과 요청만으로 다시 계산해 바이트
단위로 대조하며, 편집기 응답과 독립이다.

## 3. 판정

게이트(offscreen 생존):

| 파일 | 판정 |
|---|---|
| `structure.hwpx` (원본 대조) | PASS(생존) |
| `insert-row.hwpx` | PASS(생존) |
| `delete-row.hwpx` | PASS(생존) |
| `insert-column.hwpx` | PASS(생존) |
| `delete-column.hwpx` | PASS(생존) |
| `merged-insert.hwpx` (병합 표 경계) | PASS(생존) |
| `merged-delete.hwpx` (병합 표 경계) | PASS(생존) |
| `real-gani-gian.hwpx` (코퍼스 혼합) | PASS(생존) |

`gate.log`·`gate-merged.log`·`gate-real.log`에 같은 결과가 남아 있다. 스크립트가 PASS 뒤 프로세스를
`kill -9` 하므로 셸에 `Killed`가 한 줄씩 찍히지만, 판정 전 15초 동안 프로세스가
살아 있었고 종료 코드는 실패로 계산되지 않았다.

## 4. 한계

- **열기만 봤다.** 행·열이 화면에 올바른 순서·크기로 그려졌는지는 캡처하지 않았다.
  구조 값의 정합성은 `crates/hae-exec/tests/table_structure.rs`(행·열·셀 격자 요약을
  매 시험마다 다시 검사)와 `crates/hae-verify/tests/table_structure.rs`(요청에서 기대
  바이트를 독립 재계산)로 확인한다.
- Linux 한컴 2020 Beta(11.20.0.989) 한 빌드다. Windows 한컴오피스 2024는 아니다.
- 한컴에서 저장한 뒤 다시 열기, 쪽 나눔·조판, 표가 쪽 폭을 넘을 때의 자동 축소는
  시험하지 않았다. 열 삽입은 총폭을 키우므로 필요하면 이어서
  `table.setColumnWidths`로 조정해야 한다.
- 병합 셀이 걸린 삭제·삽입은 설계대로 **거절**하므로 게이트 산출물이 없다. 거절은
  `crates/hae-exec/tests/table_structure.rs`와 `hae-verify` 시험이 확인한다.
- `dryRun`·Undo/Redo·batch는 엔진 시험으로만 확인했고 한컴 게이트 산출물로 만들지
  않았다(Undo 뒤 export가 원본과 같은 바이트임은 시험이 고정한다).
