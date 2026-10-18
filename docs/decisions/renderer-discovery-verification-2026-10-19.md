# 렌더러 자동 탐색·Windows 게이트 실행 기록 (2026-10-19)

작성: 0.4 기능 3호 구현 담당(구현 하위 에이전트). 대상 커밋: `2955e77` (브랜치 `feat/renderer`).
코드: `crates/hae-render-rhwp/src/{discovery.rs,unsandboxed.rs,lib.rs,sandbox.rs}`,
`crates/hae-mcp/src/doctor.rs`. 보장/미보장 대조표는
[renderer-isolation-platforms.md](renderer-isolation-platforms.md)에 있다.

## 1. 실행 환경

- 워크트리 `/tmp/opencode/wt-renderer`(브랜치 `feat/renderer`), `CARGO_TARGET_DIR=/home/mattc/.cache/hae-renderer`.
- `spikes/`는 Git 무시 영역이라 워크트리에 렌더러가 없었다. 확인을 위해 **읽기 전용 복사**만 했다
  (다른 워크트리의 소스는 읽지 않았다): `spikes/vendor/rhwp-src/target/release/rhwp`(27,170,544바이트,
  SHA-256 `e91cd25fbf5bd8045371ba3e9bdaa1b05cf7df146d04773e65ec1e99e6cd05a7`)와
  `spikes/work/stage4-e2e/inputs/04_complex.hwpx`.
- 글꼴: `/usr/local/share/fonts/n/NanumGothic.ttf`(4,691,820바이트) 1개만 담은 작은 폴더.
- 문서: `tests/fixtures/corpus/gyehoek-reference.hwpx`(19쪽), 요청은 1쪽 SVG+PNG.

## 2. `HAE_RHWP` 없이 실제 미리보기 (완료 조건 3)

세 번 실행했고 모두 `HAE_RHWP`·`HAE_RASTER`·`HAE_BWRAP`·`HAE_RENDERER_DISCOVERY`를 **해제**한 상태다.
산출물은 `spikes/work/renderer-discovery/`에 있다(JSON 원문·stderr·PNG·SVG).
`docs/decisions/renderer-discovery-verification-2026-10-19.md`는 이 문서 자신이며,
실행 로그와 원문 JSON은 Git 무시 영역(`spikes/work/`)에 있으므로 저장소 밖으로 반출하려면 따로 복사해야 한다.

| 실행 | 작업 디렉터리 / PATH | 찾은 규칙 | 결과 |
|---|---|---|---|
| A | 저장소 루트 / 기본 PATH | 개발 트리 `.../spikes/vendor/rhwp-src/target/release/rhwp` | `status: rendered`, 19쪽, 1쪽 SVG 70,110바이트 + PNG 70,389바이트(794×1123) |
| B | `/tmp` / PATH에 복사본만 둔 폴더 | `PATH` | 위와 **바이트 동일**(SVG `85983a3b…`, PNG `34b0edfa…`) |
| C | `/tmp` / 빈 폴더만 PATH | 없음(후보 17개 모두 `missing`) | 거절: `RENDERER_UNAVAILABLE(renderer_not_configured)`, 종료 1, 문서 상태 불변 |

- A·B의 `metadata.sandbox`: `tool: bubblewrap`, `selfTest: passed`, `version: bubblewrap 0.13.0`,
  `memoryLimit.tool: /usr/bin/prlimit`, `rasterMemoryLimit.bytesAtMost: 406,969,979` — **Linux 경로는 그대로**다.
- A·B의 `metadata.renderer`: `version: rhwp v0.8.7`, `sha256: e91cd25f…`(= 동봉 실행 파일의 실제 해시,
  독립 `sha256sum`과 일치), `commit: 1a76570e…`, `rasterizer: hae-raster`(62,187,112바이트).
- 각 쪽 SVG·PNG 해시는 응답이 보고한 SHA-256과 일치했다(계산·대조 완료). `coverage: unknown`,
  `nativeComparison: not_checked`.
- C의 진단: `message`가 "set HAE_RHWP to its path, or place it beside this application (or on PATH)"와
  살펴본 12개 후보를 담고, `detail.searched`에 17개(경로·규칙·판정)가 실렸다. `hae`의 종료 코드는 1이고
  `document.open`은 성공한 채 문서는 바뀌지 않았다(프리뷰 실패는 문서 상태를 건드리지 않는다).

## 3. `doctor` (탐색 출처 보고)

`hae-mcp doctor`를 A와 같은 조건에서 실행했다(`spikes/work/renderer-discovery/doctor.json`).

- `checks.rhwp`: `ok:true`, `path: …/spikes/vendor/rhwp-src/target/release/rhwp`, `source: devTreeRelease`.
- `checks.raster`: `source: exeDir`(`target/debug/hae-raster`), `checks.bwrap`: `source: default`(`/usr/bin/bwrap`),
  `checks.fonts`: `source: env`(HAE_FONT_DIR).
- `checks.discovery`: `enabled: true`, 중지 변수 이름, 탐색 순서 요약.
- `checks.platformIsolation`: Linux에서는 `honoredOnThisPlatform: false`(opt-in 무시), 강제 목록 6개
  (wallClockTimeout·stdoutStderrCaps·outputDirectoryCaps·cancellation·environment·stagedInput),
  미강제 목록 6개(filesystemConfinement·readOnlyInput·memoryLimit·processCountLimit·cpuTimeLimit·networkIsolation).

## 4. `scripts/check.sh`

| 실행 | 조건 | 결과 |
|---|---|---|
| 1 | `bash scripts/check.sh` (고정 커밋 `2955e77`) | 종료 **0**. workspace 807 passed·0 failed(테스트 바이너리 103개), Python 33 OK, 실제 렌더러 시험 13건 통과(executor preview 4 + adapter 9) |
| 2 | `HAE_RHWP=/nonexistent/rhwp bash scripts/check.sh` (렌더러 없음) | 종료 **0**. workspace 794 passed·0 failed(테스트 바이너리 90개), Python 33 OK, 실제 렌더러 시험 13건 SKIP |

로그: `spikes/work/renderer-discovery/logs/commit-check.log`(실행 1),
`spikes/work/renderer-discovery/logs/final-check-rendererfree.log`(실행 2).
실행 1은 커밋 뒤 소스를 바꾸지 않은 상태에서 돌렸고, 실행 2는 같은 소스에 렌더러만 없앤 조건이다.
(`logs/final-check.log`는 같은 조건을 앞서 돌린 기록으로, 당시 Windows 전용 코드 한 줄을 고치기 전
바이너리가 일부 섞여 있다. 최종 근거는 `commit-check.log`를 쓴다.)

## 5. Windows 크로스 확인

- `cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings` 통과.
- `tests/windows_preview.rs`(`#[cfg(windows)]`)는 이 명령으로 컴파일만 확인했다. **Windows에서 실행한 적이 없다.**
- 그 과정에서 Windows 대상 `--tests` 컴파일을 막던 기존 파일 2개의 Unix 전용 표시
  (`crates/hae-exec/tests/{preview,export_text}.rs`)를 정리했다(동작 변화 없음).

## 6. 이 기록이 주장하지 않는 것

- Windows에서의 실제 렌더링·격리·조판(`nativeComparison`은 계속 `not_checked`).
- Job Object 기반 메모리·프로세스 수·CPU 시간 상한: **적용하지 않았다**(§[renderer-isolation-platforms.md §3]).
- `coverage`의 실제 값: 렌더러가 미지원 요소를 보고하지 않으므로 `unknown`을 유지한다.
- 폰트가 1개뿐이라 A·B의 조판은 대체 글꼴 기준이다.
