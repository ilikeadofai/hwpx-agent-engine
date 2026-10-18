# 조사 중 확인된 결함·보안 위험 (별도 기록)

2026-10-18. 이 문서는 경쟁 분석 과정에서 발견한 결함과 보안 위험만 따로 모은 것이다. 제품 코드는 수정하지 않았고, 커밋·push·병합도 하지 않았다.

판정 등급: **높음** = 실제 오작동·자원 오판, **중간** = 특정 입력에서 비정상 종료, **낮음** = 품질·호환성.

---

## D1 — 메모리 회계 누락 (높음) · 조사 ID P1

**무엇**: 검증 로그(`verify_steps.pre_state`)가 보유한 XML payload와 이미지 바이트가 `SessionExecutor::compute_memory` 회계에서 빠진다. `history.clear`로 캐시를 비워도 그 payload는 남는데 회계는 줄지 않는다.

**근거**: `crates/hae-exec/src/session.rs:70,322,800,1305` (B 조사).

**재현**: B가 만든 하네스(`~/.cache/cga/b/memory-harness`)를 **총괄이 복사해 직접 실행**했다. 소스는 B가 고정한 `1cf8f5c` 사본(`~/.cache/cga/b/source`)을 쓴다.

```bash
# 하네스: 2 MiB 예산, memory.hwpx, text.replace + history.clear 40회
CARGO_TARGET_DIR=~/.cache/cga/p1-target TMPDIR=~/.cache/cga/tmp \
  cargo run --offline --manifest-path ~/.cache/cga/p1-verify/Cargo.toml
```

**관찰 (총괄 실행)**:

| 시점 | accounted | 비고 |
|---|---|---|
| baseline | 214,656 | |
| 10회 편집 | **314,909** | B 기록의 314,909와 **정확히 일치** |
| VmRSS (10회) | 22,448 kB | 참고 관찰이며 예산 정의와 동일시하지 않음 |

**영향**: 100단위 이력 eviction·clear 뒤에도 raw payload·frozen image 바이트가 회계에서 빠져 **512 MiB admission과 렌더러 잔여 예산 계산을 과대평가**할 수 있다. 기본 512 MiB를 실제로 넘기는 대규모 실행은 아직 하지 않았으므로 "512 MiB를 넘겨도 통과한다"고는 주장하지 않는다.

**수정 방향(제안)**: 살아 있는 모든 `verify_steps.pre_state`와 `ops.image` 바이트를 중복 제거해 회계에 넣는다. 마지막 검증 단계만 필요한지 검토해 checkpoint·compaction·clear 정책을 정하고, 다음 verification log를 포함한 peak reservation 후 commit한다.

---

## D2 — inspect 커서 덧셈 overflow (중간) · 조사 ID P2

**무엇**: `cursor`와 `limit`의 `usize` 덧셈이 overflow해 **debug CLI가 exit 101로 종료**된다.

**근거**: `crates/hae-exec/src/session.rs:657`, `crates/hae-model/src/query.rs:684` (B 조사).

**관찰(B 기록)**: `cursor="0:18446744073709551615"`, `limit=100` → debug `hae` exit 101, stdout 0, `attempt to add with overflow`. MCP는 `catch_unwind`로 `-32603`을 반환하고 다음 요청을 정상 처리했다. release에서 같은 종료가 발생한다고 주장하지 않았다.

**영향**: 유효 JSON의 잘못된 커서가 정상 `INVALID_REQUEST` 대신 프로세스를 종료시킨다. MCP 경로는 생존하므로 서버 운영에는 영향이 작다.

**수정 방향(제안)**: 커서가 `nodes.len()` 이내인지 검증하고 `checked_add`/`saturating_add`를 쓴다. `usize::MAX` 회귀를 CLI·JSON-Lines·MCP 세 경로에서 확인한다.

**상태**: C단계(교차검증)에서 독립 재현 중이다.

---

## D3 — Windows 빌드 불가 (높음) · 조사 ID P3

**무엇**: `hae-project`가 **`cfg(unix)` 가드 없이** Unix 전용 API를 사용한다. Windows에서 빌드되지 않는다.

**총괄 근거(직접 확인)**:

```text
crates/hae-project/src/io.rs:6   use rustix::fd::OwnedFd;
crates/hae-project/src/io.rs:7   use rustix::fs::{...};
crates/hae-project/src/io.rs:13  use std::os::unix::ffi::OsStrExt;
```

`cfg(unix)`·`cfg(target_os = ...)` 가드는 `crates/hae-mcp/src/doctor.rs:14` 한 곳뿐이며 `hae-project/src/io.rs`에는 없다.

**한계**: 이 환경에 Windows 타깃 표준 라이브러리가 없어 `cargo check --target x86_64-pc-windows-gnu`를 끝까지 돌리지 못했다(의존 crate의 E0463에서 중단). 따라서 "빌드 실패"의 근거는 **가드 없는 Unix 전용 import**이며, 완전한 크로스 빌드 로그는 아니다.

**영향**: README·설계서는 Windows 검증을 "보류"로만 적고 있다. **보류와 빌드 불가는 다르다.** 사용자가 계획한 Windows 한컴 최종 검증은 코드 수정 없이는 수행할 수 없다.

---

## D4 — 출력 스키마 비표준 format (낮음) · 조사 ID P4

22개 MCP 도구의 입력·출력 스키마가 Rust `schemars` 산출물인 `format: "uint"`, `"uint32"`, `"uint64"`, `"int64"`를 쓴다(입력 `uint` 3·`uint32` 4곳, 출력 다수). 표준 JSON Schema format이 아니며, MCP Inspector가 `tools/call` 경로에서 `unknown format "uint64"` 경고를 2회 출력한다(호출은 성공). 도구 22개 모두 `title`이 없다.

**영향**: JSON Schema에서 `format`은 주석이고 값 제약은 `minimum`·`maximum`이 담당하므로 기능 영향은 확인되지 않았다. 다만 클라이언트별 검증기 차이로 도구가 무시될 여지가 남는다.

---

## 보안 관련 관찰

결함은 아니지만 위험 관리 관점에서 기록한다.

| 항목 | 상태 | 비고 |
|---|---|---|
| 렌더러 격리 | 구현 | `bubblewrap` + `prlimit` + Linux user namespace. 자기 점검 실패 시 실행하지 않음 |
| 렌더러 산출물 신뢰 | 구현 | 매니페스트에 적힌 `/out` 안 파일만 읽고, SVG 스크립트·외부 참조 검사. 위험 페이지는 `SVG_UNSAFE`로 보류 |
| 렌더러 바이너리 무결성 | **부분** | 복사 실행 파일의 SHA-256만 확인. **서명·고정 기대 해시 검증 없음** → 바이너리가 교체되면 탐지하지 못한다 |
| 패키지 입력 방어 | 구현 | 경로 탈출·암호화·DTD/entity·중복 이름·CRC 불일치 등 18종 거절 |
| 자원 한도 | 부분 | 바이트·항목·깊이·픽셀·메모리 한도는 있으나 **D1로 인해 실제 회계가 과소** |
| 비밀값 취급 | 관찰 없음 | 저장소·로그에 인증값 없음. 조사 문서는 계정 식별자·토큰을 기록하지 않음 |
| 개인정보 | 준수 | 사용자 문서·비공개 SDK 자료는 조사 범위 밖. 모든 실행은 공개 fixture와 합성 문서만 사용 |
| 의존성 출처 | 부분 | 직접 외부 crate 15개는 정확 버전 고정. 소스 독창성·전체 provenance 감사는 미실시 |

**가장 주의할 항목**: 렌더러 바이너리의 **고정 해시·서명 검증 부재**. 미리보기는 문서 내용을 외부 프로세스에 넘기므로, `HAE_RHWP`가 가리키는 실행 파일이 바뀌면 격리 안에서 임의 코드가 돈다. 현재는 "복사한 바이너리의 해시를 기록"하는 수준이다.
