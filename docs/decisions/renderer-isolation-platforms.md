# 렌더러 격리의 플랫폼별 보장 (0.4 기능 3호)

작성: 2026-10-19. 대상: `crates/hae-render-rhwp/src/{sandbox.rs,unsandboxed.rs,discovery.rs}`,
`crates/hae-mcp/src/doctor.rs`.

이 문서는 **플랫폼마다 무엇을 보장하고 무엇을 보장하지 않는지**만 적는다. 추측으로 "동등하다"고
쓰지 않는다. 여기서 "보장"은 그 플랫폼에서 실제로 시험한 범위를 뜻하며, 실행하지 않은 경로는
"컴파일만 확인"이라고 표시한다.

- **Linux**: `bash scripts/check.sh`가 실제 bubblewrap·rhwp로 시험한다. 이 기능(0.4-3)은 Linux
  격리 경로의 동작·보장을 바꾸지 않았다. 바뀐 것은 *어떤 경로를 쓸지 고르는 탐색*과 진단 문구다.
- **Windows**: 이 기능이 추가한 opt-in 경로는 **컴파일만 확인**했다(`cargo check`/`cargo clippy
  --workspace --all-targets --target x86_64-pc-windows-gnu`). 이 저장소에는 Windows 실행 환경이
  없으므로 실제 렌더링·격리·조판은 시험하지 않았다.
- **그 밖의 플랫폼(macOS 등)**: 아래 표에서 "그 밖의 Unix/기타" 열이 곧 Windows 기본값과 같다.
  opt-in은 Windows 전용이며 다른 플랫폼에서는 무시된다(무시 사실을 실패 진단에 적는다).

## 1. 플랫폼별 대조표

| 보장 항목 | Linux (bwrap, 기본·유일 경로) | Windows 기본 (opt-in 없음) | Windows opt-in (`HAE_ALLOW_UNSANDBOXED_PREVIEW=1`) | 그 밖의 플랫폼 |
|---|---|---|---|---|
| 렌더러 실행 | 한다 | **하지 않는다** (`RENDERER_UNAVAILABLE(sandbox_unverified)`) | 한다 (`sandbox_unverified` 경고를 결과에 남김) | 하지 않는다 |
| 파일시스템 격리 | namespace mount로 `/in`·`/fonts`·`/usr` 읽기 전용, `/out`만 쓰기 | — | **없음**: 렌더러가 사용자 계정 권한으로 호스트 경로를 읽고 쓸 수 있다 | — |
| 입력 스냅샷 | 읽기 전용 바인드 마운트 | — | 사설 임시 디렉터리의 **사본**(사본 자체는 쓰기 가능) | — |
| 네트워크 | `--unshare-all`(loopback만; self-test가 `/proc/net/dev`로 확인) | — | **차단 없음** | — |
| 환경 변수 | `--clearenv` 후 `PATH`·`HOME`·`LANG`만 설정 | — | `env_clear` 후 `SystemRoot`·`windir`·`TEMP`/`TMP`·`PATH`만 설정(Windows CRT가 요구) | — |
| 작업 디렉터리 | `--chdir /tmp`(버려지는 임시 영역) | — | 사설 임시 디렉터리(`TEMP`·`TMP`와 동일)로 설정 | — |
| 프로세스별 메모리 상한 | `prlimit --as`(RLIMIT_AS, 자손 상속) | — | **없음**: Job Object backend가 없어 적용하지 않는다(§3) | — |
| 프로세스 수 상한 | PID namespace + RLIMIT_AS 간접 | — | **없음**(Job Object 미적용) | — |
| CPU 시간 상한 | 벽시계 타임아웃만(CPU 시간 상한은 주장하지 않음) | — | 벽시계 타임아웃만 | — |
| 벽시계 타임아웃·kill | 감독 루프 + 프로세스 그룹 SIGKILL | 해당 없음(실행 안 함) | 감독 루프 + `taskkill /T /F` 후 `kill` (best effort) | 해당 없음 |
| 자손 트리 종료 | 프로세스 그룹 SIGKILL, bwrap pid namespace init 동반 종료 | — | `taskkill /T`가 잡은 범위만. 벗어난 자손은 살아남을 수 있다 | — |
| stdout/stderr 상한 | 있음(실행 중 초과 시 중단) | — | 있음(같은 코드) | — |
| 출력 디렉터리 상한 | 100 ms 주기 표본 검사(바이트·항목) | — | 같음(표본 검사이지 quota 아님) | — |
| 취소 | `CancelToken` | — | 같음 | — |
| 임시 디렉터리 권한 | 0700 | — | Windows 기본 ACL 상속(POSIX 0700에 해당하는 보장 없음) | — |
| `/out` 밖 읽기 | 매니페스트 경로만, `..`·심볼릭 링크·비정규 파일 거절 | — | 같은 검사를 그대로 적용 | — |
| SVG 안전 검사 | 있음(`svg_safety`) | — | 같음 | — |
| PNG 래스터 | 별도 `hae-raster`가 같은 격리 안에서 실행 | — | **없음**: 결과는 SVG이고 `PNG_UNAVAILABLE` 경고 + `status: partial` | — |
| 렌더러 기반 쪽 정보(`page_info`, 라이브러리 API) | 같은 격리 안에서 실행 | 거절 | opt-in과 같은 조건으로 실행(추가 보장 없음). 현재 사용자 명령은 아니다 | 거절 |
| 격리 자기 점검 | 첫 실행 전 `Sandbox::self_test` 6항목, 실패 시 실행 안 함 | — | 해당 없음(`selfTest: "not_applicable"`, `verified: false`) | — |
| 결과 표시 | `metadata.sandbox.selfTest: "passed"` | 오류만 | `metadata.sandbox.tool: "none"`, `verified: false`, `SANDBOX_UNVERIFIED` 경고, `enforcement[]` 목록 | 오류만 |

"Linux 기본·유일 경로"는 문자 그대로다. `HAE_ALLOW_UNSANDBOXED_PREVIEW`는 Linux에서 **무시**되며,
bwrap·prlimit·자기 점검이 실패하면 opt-in이 있어도 실행하지 않는다(시험:
`tests/discovery_env.rs::environment_values_win_and_a_disabled_search_refuses_cleanly` 4·5항,
`tests/render.rs::unverified_sandbox_means_renderer_unavailable`).

## 2. Windows opt-in의 실제 범위

`HAE_ALLOW_UNSANDBOXED_PREVIEW`는 **명시적·자명한 opt-in**이다. 값이 `1`·`true`·`yes`·`on`
(대소문자 무시)일 때만 켜지고, 그 밖의 값은 전부 기본값(거절)이다. opt-in 실행에서도 다음은
그대로 적용된다(코드: `unsandboxed::enforcement()`가 같은 목록을 결과 metadata에 싣는다).

- 벽시계 타임아웃(`renderer_timeout_secs`)과 종료, 취소 토큰
- stdout·stderr 상한, `/out` 바이트·항목 표본 상한
- 환경 변수 초기화(최소 집합만 허용)
- 렌더러·스냅샷의 사설 임시 디렉터리 사본
- `/out` 매니페스트 전용 읽기, 정확한 길이·정규 파일·심볼릭 링크 거절
- SVG 안전 검사, 문서 상태 불변

opt-in 실행의 결과에는 **반드시** `SANDBOX_UNVERIFIED`(warning) 문자열 진단이 들어가고,
`metadata.sandbox`에 `verified: false`·`equivalentToLinuxBubblewrap: false`와 항목별 강제/미강제
목록이 실린다. 한 번의 성공도 "Linux와 동등한 격리"로 표시되지 않는다.

## 3. 적용하지 못한 것: Windows Job Object

요구된 메모리·프로세스 수·CPU 시간 상한은 **Job Object**로만 강제할 수 있고, 이 저장소는
워크스페이스 lint `unsafe_code = "forbid"`(Cargo.toml `[workspace.lints.rust]`)를 유지한다.
Job Object 생성·설정·할당(`CreateJobObjectW`, `SetInformationJobObject`,
`AssignProcessToJobObject`)은 FFI, 즉 `unsafe`가 필요하므로 이 crate에서 구현할 수 없다.
그래서 이번 작업은 다음만 한다.

- `unsandboxed::job_object_limits()`가 **선언할 값**을 한 곳에서 계산한다(프로세스/잡 메모리
  상한 = Linux 경로가 `prlimit --as`에 넘기는 값, 프로세스 수 상한 8, 프로세스·잡 CPU 시간 =
  타임아웃 기반, `killOnJobClose`, `dieOnUnhandledException`). 순수 함수라 Linux에서 시험한다.
- 결과 metadata와 `doctor`는 `jobObject.applied: false`와 사유를 **명시**한다. 적용하지 않은
  상한을 적용했다고 주장하지 않는다.

다시 열려면 (1) `unsafe` FFI를 허용하는 별도 crate 또는 정책 변경으로 Job Object backend를
구현하고, (2) **실제 Windows에서** 그 상한이 걸리는지 측정해야 한다. 그 전까지 Windows opt-in은
메모리·프로세스 수·CPU 시간 상한이 없다.

## 4. Windows opt-in에서 주장하지 않는 것

- bubblewrap namespace·mount 격리, 네트워크 차단, 읽기 전용 입력/시스템 트리
- `prlimit` 기반 주소 공간 상한(또는 그에 상응하는 Job Object 상한)
- Unix 프로세스 그룹에 준하는 자손 트리 종료 보장
- POSIX 0700에 상응하는 임시 디렉터리 권한
- Windows 기본 ACL이나 `taskkill /T`가 위 보장을 대신한다는 주장
- Windows에서의 실제 렌더링·조판·한컴 호환성 (`nativeComparison`은 계속 `not_checked`)

## 5. 근거와 한계

- Linux 보장의 근거: `crates/hae-render-rhwp/src/sandbox.rs`의 `build_args`·`self_test`와
  `tests/{render.rs,pages_join.rs,discovery_env.rs}`, `crates/hae-exec/tests/preview.rs`.
- Windows 코드 경로: `src/unsandboxed.rs`(runner·argv 변환·정책·상한 선언)와
  `tests/windows_preview.rs`(`#[cfg(windows)]`, 컴파일만 확인). 크로스 확인 명령과 결과:
  `cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings` 통과.
- 이 문서는 Windows에서 실행한 측정값을 담지 않는다. Windows 보장 수준을 주장하려면 그 플랫폼에서
  별도 실행·검증이 먼저 필요하다.
