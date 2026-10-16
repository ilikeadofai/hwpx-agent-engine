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
| 안전한 파일 I/O | `rustix` | `=1.1.4` | `fs`, `process` | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | Linux `openat`, `O_NOFOLLOW`, `flock`, `fsync`, `renameat_with(NOREPLACE)`; renderer supervisor waits and process control |
| 오류 정의 | `thiserror` | `=2.0.20` | 기본 | MIT OR Apache-2.0 | `hae-diag::EngineError` |
| CLI 인자 파싱 | `clap` | `=4.6.6` | `derive` | MIT OR Apache-2.0 | `hae-cli` 스크립트·단발 실행 모드 |
| SVG 래스터화 | `resvg` | `=0.45.1` | `default-features = false`, `text`, `raster-images`, `memmap-fonts` | MPL-2.0 | `hae-render-rhwp::png`: 별도 격리 `hae-raster`에서 검사한 SVG를 PNG로 변환(작업 3-4). rhwp 저장소 Cargo.lock이 쓰는 0.45.1과 같은 버전이며 Rust 1.93.1에서 빌드됨. 순수 Rust(usvg·tiny-skia·rustybuzz·fontdb 포함). 시스템 글꼴(`system-fonts`)과 fontconfig는 끄고 설정한 폰트 폴더만 읽는다. |

## 3. RustSec 및 안전성 점검

1. **비안전 코드 차단**: 워크스페이스 전체 crate(`hae-*`)에 `unsafe_code = "forbid"`를 적용했다.
2. **디코더/파서 방어 보완**:
   - `quick-xml`: DTD·외부 실체(`<!DOCTYPE`, `<!ENTITY`)와 금지 제어문자를 파서 입구에서 즉시 거절하고 최대 깊이(128)를 강제한다.
   - `png 0.18.1`: 기본 옵션이 보조 청크 CRC 오류와 zlib Adler-32 체크섬을 무시할 수 있음을 확인하고, `DecodeOptions`에서 `set_ignore_adler32(false)`, `set_ignore_crc(false)`, `set_skip_ancillary_crc_failures(false)`, `set_ignore_text_chunk(true)`, `set_ignore_iccp_chunk(true)`를 명시적으로 설정했다.
   - `zune-jpeg 0.5.15`: strict 모드에서도 EOI 도달 시 부족한 비트를 0으로 채우는 동작을 확인하고, `hae-edit` 안에 독립 허프만 스캔 검증(`jpeg_safety::validate`)을 추가해 잘린 스캔을 거절한다.
   - `ZIP`: 외부 `zip` crate를 쓰지 않고 `hae-zip`에서 중앙/로컬 헤더 일치, 구간 비중첩, 스트리밍 해제 한도(항목·누적 128 MiB), 원시 압축 데이터 및 로컬 헤더 바이트 보존을 직접 구현했다.

## 4. 프리뷰 어댑터의 resvg 사용 규칙 (작업 3-4)

- 래스터화 입력은 `svg_safety::check_svg`를 통과한 바이트(`CheckedSvg`)뿐이다. 타입으로 강제한다.
- `usvg::Options::image_href_resolver`를 교체해 외부 참조를 끈다. `<image>`는 `data:` 래스터(PNG·JPEG·GIF·WebP)만 풀고, 파일 경로·URL·`data:image/svg+xml`(검사하지 않은 중첩 SVG)은 아무것도 불러오지 않는다.
- 글꼴은 설정한 폴더의 파일만 `fontdb`에 올린다. 일반 글꼴군(serif·sans-serif·monospace·cursive·fantasy)은 그 폴더의 첫 글꼴군으로 연결해, 문서가 없는 글꼴 이름만 쓸 때 글자가 통째로 사라지지 않게 한다(대체는 FONT_UNAVAILABLE 진단으로 알린다).
- 출력 화소 수는 `Limits::max_image_pixels`(40,960,000)로 막는다. PNG 인코딩은 resvg가 다시 내보내는 `tiny-skia`의 `encode_png`(내부 `png 0.17.16`)를 쓴다. 워크스페이스의 `png 0.18.1`은 검증 용도라 섞이지 않는다.
- 새로 들어오는 전이 의존성(usvg 0.45.1, tiny-skia 0.11.4, fontdb 0.23.0, rustybuzz 0.20.1, ttf-parser 0.25.1, memmap2 0.9.11, gif 0.13.3, image-webp 0.2.4, zune-jpeg 0.4.21 등)은 Cargo.lock으로 고정된다. `memmap2`는 내부에 `unsafe`가 있으나 우리 crate는 `unsafe_code = "forbid"`를 유지한다.
