# 단계 3 rhwp 재빌드 검증 (S2·S3 재현)

실행일: 2026-10-16. 목적: 고정 툴체인으로 rhwp를 다시 빌드한 뒤 stage0의 S2(줄 배치 캐시 비교)와 S3(격리 실행)를 같은 조건으로 재현할 수 있는지 확인한다. 코드는 바꾸지 않았고 커밋하지 않았다.

## 1. 툴체인과 빌드

| 항목 | 값 |
|---|---|
| 툴체인 | `rustup run 1.93.1` |
| rustc | `rustc 1.93.1 (01f6ddf75 2026-02-11)` |
| 명령 | `cd spikes/vendor/rhwp-src && rustup run 1.93.1 cargo build --release` |
| 빌드 종료 코드 | 0 (`Finished release profile [optimized] in 25m 13s`) |
| `rhwp --version` | `rhwp v0.8.7` |
| 바이너리 경로 | `spikes/vendor/rhwp-src/target/release/rhwp` |
| 바이너리 sha256 | `e91cd25fbf5bd8045371ba3e9bdaa1b05cf7df146d04773e65ec1e99e6cd05a7` |

이전 S3 기록의 바이너리 해시는 `4810a4a6ff2f…`다. 새 해시와 다르다. 소스 사본에 `.git`이 없어 빌드 입력을 소스 커밋과 대조하지 못했다(stage0 S2-b의 한계와 같다). 해시가 달라도 아래 쪽수·경고·SVG 결과는 기록과 같다.

## 2. S2 재실행: 쪽수와 경고 수

명령: `target/release/rhwp export-svg FILE -o /tmp/s2re/<이름> --json`. 경고 수는 stderr에서 `LAYOUT_OVERFLOW|LAYOUT_TABLE_OVERLAP`에 맞는 줄 수다. 모든 실행의 종료 코드는 0이다.

| 파일 | 옛 쪽수(stage0) | 새 쪽수 | 옛 경고 | 새 경고 |
|---|---|---|---|---|
| 1_보도자료_0_원본 | 2 | 2 | 3 | 3 |
| 1_보도자료_A_캐시제거 | 2 | 2 | 5 | 5 |
| 1_보도자료_B_낡은캐시유지 | 2 | 2 | 3 | 3 |
| 1_보도자료_E_구역끝캐시제거 | 3 | 3 | — | 3 |
| 2_계획서_0_원본 | 19 | 19 | 0 | 0 |
| 2_계획서_A_캐시제거 | 19 | 19 | — | 2 |
| 2_계획서_B_낡은캐시유지 | 19 | 19 | 0 | 0 |
| 2_계획서_E_구역끝캐시제거 | 21 | 21 | — | 0 |
| 3_다중구역_0_원본 | 28 | 28 | — | 2 |
| 3_다중구역_A_캐시제거 | 28 | 28 | — | 2 |
| 3_다중구역_B_낡은캐시유지 | 28 | 28 | — | 2 |
| 3_다중구역_E_구역끝캐시제거 | 28 | 28 | — | 2 |

비고:
- 쪽수는 모든 문서·변형에서 stage0 표와 같다.
- "옛 경고"는 `spikes/work/s2/*.err`에 남아 있는 기록이다. 해당 파일이 있는 서식(`spikes/work/s2`의 보도자료 0·A·B·C·D·E, 같은 원본 변형)은 새 경고 수와 모두 같다(0 원본 3, A 5, B 3, C 3, D 3, E 3). 나머지는 옛 경고 기록이 없어 비워 두었다("—").
- `C_same_text_no_lineseg`의 `LAYOUT_*` stderr 줄은 옛 `svg_C.err`와 완전히 같다.
- 2_계획서 A·3_다중구역의 경고 수는 stage0 본문의 설명과 맞는다(계획서 A는 넘침, 다중구역은 원본과 같은 겹침).

## 3. S3 재실행: 격리 실행

준비: `/tmp/s3re/in`에 새 바이너리(`rhwp`)와 `spikes/work/s2/E_from_edit.hwpx`를 복사했다. E_from_edit.hwpx sha256은 `3d01965c0d8d…`로 stage0 기록과 같다.

| 시험 | 결과 |
|---|---|
| `bash spikes/s3_sandbox/run_sandboxed.sh /tmp/s3re/in /tmp/s3re/out spikes/vendor/rhwp-src/ttfs -- /in/rhwp export-svg /in/E_from_edit.hwpx -o /out --json` | 종료 코드 0, SVG 3쪽 |
| 격리 밖 같은 바이너리 실행 (`/tmp/s3re/native`) | 종료 코드 0, SVG 3쪽 |
| SVG 바이트 비교 (`_001`, `_002`, `_003`) | 세 파일 모두 바이트 동일 (`diff -r` 무차이) |
| stderr 비교 | 동일 |
| stdout 매니페스트 비교 | `outputDir`·`pages[].path`·`source`의 경로 문자열만 다르다. 쪽수·바이트·overflowCellLines는 같다 |
| `-o /in/evil` (격리 안) | 종료 코드 1, `오류: 출력 폴더를 생성할 수 없습니다 - /in/evil: Read-only file system (os error 30)`. `/tmp/s3re/in`에 `evil`이 생기지 않았다 |

## 4. 판정

- 툴체인: rustc 1.93.1, 빌드 종료 코드 0
- 쪽수: 15개 입력 중 stage0 표에 있는 값(1 보도자료 2/2/2/3, 2 계획서 19/19/19/21, 3 다중구역 28×4)이 모두 같다
- 경고: 옛 기록이 있는 파일은 모두 같다
- 격리 실행: 종료 코드 0, SVG 바이트 동일, 입력 쓰기 차단 확인

S2·S3 결과 재현: 예

주의: 바이너리 해시가 이전 기록(`4810a4a6ff2f…`)과 다르다. 재현은 새 바이너리 기준이다. 새 해시의 출처는 이 문서의 빌드 기록에만 있다.

## 5. 고정 소스 판본의 파일별 원문 대조 (재개 검증)

Git 디렉터리가 없다는 기존 한계를 보완하기 위해 GitHub의 고정 커밋 `1a76570e833917d15817415a53c09ad61ab3203f`에 대한 재귀 Git tree API를 조회했다. API가 잘린 목록을 반환하지 않았음을 확인하고, 로컬 소스 사본의 모든 파일(`target/` 제외)에 대해 `SHA1("blob " + 바이트 길이 + NUL + 원문)`을 계산해 upstream Git blob ID와 비교했다.

- 대조한 파일: **1,671개**
- 내용 불일치: **0개**
- upstream에 없는 로컬 파일: **0개**
- 결과 원자료: `spikes/work/rhwp-source-verification.json`
- 조회: `gh api 'repos/edwardkim/rhwp/git/trees/1a76570e833917d15817415a53c09ad61ab3203f?recursive=1'`

이 검증은 현재 내려받아 빌드한 파일들의 원문이 고정 커밋과 같음을 확인한다. upstream 전체 파일을 모두 내려받았다는 뜻은 아니며, 제3자 의존성을 별도 재감사하거나 한컴 조판을 확인한 것도 아니다. 기존 §1·§4의 “소스 커밋과 대조하지 못했다”는 당시 상태는 이 파일별 검증으로 보완됐다. 바이너리 SHA-256은 재개 시에도 `e91cd25fbf5bd8045371ba3e9bdaa1b05cf7df146d04773e65ec1e99e6cd05a7`로 일치했다.
