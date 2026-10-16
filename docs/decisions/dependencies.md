# 직접 의존성 고정과 보안 공지 점검 (구현 계획 1-1)

작성: 2026-10-16. 근거: 구현 계획 §2(툴체인과 의존성), `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`.

## 1. 툴체인 고정

- `rust-toolchain.toml`: `channel = "1.93.1"`, `profile = "minimal"`, `components = ["rustfmt", "clippy"]`.
- 워크스페이스 공통 lint: `unsafe_code = "forbid"`, `clippy::all = "warn"` (`scripts/check.sh`에서는 `-D warnings`로 강제).

## 2. 직접 의존성 목록 (`=` 버전 고정)

모든 직접 의존성은 루트 `Cargo.toml`의 `[workspace.dependencies]`에서 `=` 정확 버전으로 고정하고 `Cargo.lock`을 함께 관리한다.

| 용도 | crate | 고정 버전 | 활성화 feature | 라이선스 | 비고 |
|---|---|---|---|---|---|
| XML 토큰화 | `quick-xml` | `=0.39.2` | 기본 | MIT | S1 검증 버전. namespace URI 해석은 `hae-xml`에서 직접 수행 |
| DEFLATE 압축/해제 | `flate2` | `=1.1.9` | 기본 (`rust_backend`) | MIT OR Apache-2.0 | 스트리밍 해제량 상한은 `hae-zip`에서 직접 강제 |
| CRC-32 계산 | `crc32fast` | `=1.5.0` | 기본 | MIT OR Apache-2.0 | ZIP 항목 무결성 대조 |
| 확장 자소 군집 | `unicode-segmentation` | `=1.13.3` | 기본 | MIT OR Apache-2.0 | UAX #29 확장 자소 군집 분할 |
| 직렬화 | `serde` | `=1.0.229` | `derive` | MIT OR Apache-2.0 | 엄격 스키마(`deny_unknown_fields`) |
| JSON 처리 | `serde_json` | `=1.0.151` | 기본 | MIT OR Apache-2.0 | 프로토콜·manifest·대응표 직렬화 |
| JSON Schema 생성 | `schemars` | `=1.2.2` | 기본 (`derive`) | MIT | `hae-exec` 명령 입출력 스키마 단일 출처 생성 |
| 해시 | `sha2` | `=0.10.9` | 기본 | MIT OR Apache-2.0 | `payloadSetSha256`, 파일·미디어 SHA-256 |
| 난수 ID | `getrandom` | `=0.3.4` | 기본 | MIT OR Apache-2.0 | `documentId`, `nodeId`, `sessionId`, `writerId` 생성 |
| PNG 검증 | `png` | `=0.18.1` | 기본 | MIT OR Apache-2.0 | 할당 전 IHDR 크기·화소 상한 확인, CRC 및 Adler-32 강제 검증, 재인코딩 없음 |
| JPEG 검증 | `zune-jpeg` | `=0.5.15` | 기본 | MIT OR Apache-2.0 OR Zlib | 할당 전 SOF 크기·메모리 상한 확인 및 엔트로피 스캔 완전성 검증 |
| 안전한 파일 I/O | `rustix` | `=1.1.4` | `fs` | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | Linux `openat`, `O_NOFOLLOW`, `flock`, `fsync`, `renameat_with(NOREPLACE)` |
| 오류 정의 | `thiserror` | `=2.0.20` | 기본 | MIT OR Apache-2.0 | `hae-diag::EngineError` |
| CLI 인자 파싱 | `clap` | `=4.6.6` | `derive` | MIT OR Apache-2.0 | `hae-cli` 스크립트·단발 실행 모드 |

## 3. RustSec 및 안전성 점검

1. **비안전 코드 차단**: 워크스페이스 전체 crate(`hae-*`)에 `unsafe_code = "forbid"`를 적용했다.
2. **디코더/파서 방어 보완**:
   - `quick-xml`: DTD·외부 실체(`<!DOCTYPE`, `<!ENTITY`)와 금지 제어문자를 파서 입구에서 즉시 거절하고 최대 깊이(128)를 강제한다.
   - `png 0.18.1`: 기본 옵션이 보조 청크 CRC 오류와 zlib Adler-32 체크섬을 무시할 수 있음을 확인하고, `DecodeOptions`에서 `set_ignore_adler32(false)`, `set_ignore_crc(false)`, `set_skip_ancillary_crc_failures(false)`, `set_ignore_text_chunk(true)`, `set_ignore_iccp_chunk(true)`를 명시적으로 설정했다.
   - `zune-jpeg 0.5.15`: strict 모드에서도 EOI 도달 시 부족한 비트를 0으로 채우는 동작을 확인하고, `hae-edit` 안에 독립 허프만 스캔 검증(`jpeg_safety::validate`)을 추가해 잘린 스캔을 거절한다.
   - `ZIP`: 외부 `zip` crate를 쓰지 않고 `hae-zip`에서 중앙/로컬 헤더 일치, 구간 비중첩, 스트리밍 해제 한도(항목·누적 128 MiB), 원시 압축 데이터 및 로컬 헤더 바이트 보존을 직접 구현했다.
