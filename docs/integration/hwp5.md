# 통합 조각: 구형 HWP5 읽기 전용 조회 (`hwp5.inspect`) — `feat/hwp5`

작업 브랜치 `feat/hwp5`, 시작 HEAD `e594b76`(WIP, 공급자 한도로 중단된 작업을 보존한 커밋).
공유 파일은 이 브랜치에서 **직접** 편집했고, 아래에 무엇이 들어갔는지 적는다.

## 1. 새 crate `crates/hae-hwp5`

- `src/cfb.rs`: MS-CFB 읽기 전용 리더(헤더·FAT/DIFAT·디렉터리·미니 FAT·미니 스트림).
  모든 섹터·체인 접근은 파일 바이트로 경계 검사하고, 손상은
  `PACKAGE_REJECTED(malformed_compound_file)`, 항목 수 초과는 `RESOURCE_LIMIT`으로 거절한다.
- `src/header.rs`: 256바이트 `FileHeader`(서명·버전·속성 비트). 비트 1(암호)·비트 4(DRM)는
  `encrypted`, 비트 2(배포용)는 `distribution_unsupported`로 거절.
- `src/records.rs`: 레코드 헤더(TLV, 0xFFF 확장 크기)와 `PARA_TEXT` UTF-16LE 디코드.
  HWP 5.0 표 6 분류대로 제어 문자를 처리하고 서로게이트는 lossy 정책으로 합친다.
- `src/inspect.rs`: 컨테이너 → FileHeader → `BodyText/SectionN` → 문단 페이지. raw DEFLATE
  해제(누적 예산), 문단·글자 집계, 커서용 `next_offset`.
- `src/fixture.rs`: 시험용 합성 CFB 컨테이너 빌더(실물 문서 유래 아님).
- 시험 `tests/hwp5.rs` 23개(합성 컨테이너 전용).

## 2. `crates/hae-exec`

- `registry.rs`: `Hwp5InspectParams`/`Hwp5InspectResultShape`(+section/paragraph shape),
  `ParsedCommand::Hwp5Inspect`, `define_commands!` 항목(`hwp5.inspect`/`hwp5-inspect`/`hwp5_inspect`,
  Query, `mutates:false, doc:false, ver:false, seq:false, dry:false, hist:false`),
  `schema_tests` 기대 목록에 `hwp5_inspect` 한 줄.
- `session.rs`: `handle_hwp5_inspect`(입력 루트의 `.hwp`만, 확장자 검사 `extension_mismatch`,
  `sha256:offset` 커서)와 `hwp5_cursor_offset` 헬퍼. HWPX 세션 문서와 무관하다.
- `Cargo.toml`: `hae-hwp5` 의존 추가.
- 시험 `tests/hwp5.rs` 5개(요약·페이징·입력 불변·거절·문서 불필요).

## 3. `crates/hae-diag`

- `PackageRejectReason`: `malformed_compound_file`, `missing_component`, `malformed_record_stream`,
  `distribution_unsupported` 4개 추가(`ALL` 18→22, 시험 갱신).
- `Limits.max_hwp5_paragraphs`(기본 1,000,000) 추가.

## 4. `crates/hae-mcp/src/descriptions.rs`

- `hwp5_inspect` 설명 + `TABLE` 한 줄. `document.*` 설명은 바꾸지 않았다(HWPX 전용 유지).

## 5. 문서

- `docs/decisions/hwp5-format-notes.md`: 형식 조사(확인/정책/미확인 구분) + 실물 표본 재현 기록.
- `docs/decisions/diagnostic-codes.md`: 새 reason 목록과 `RESOURCE_LIMIT` 문구.
- `docs/decisions/hwp5-hancom-open-2026-10-18.md`: 실물 `.hwp` 표본 한컴 열기 게이트 결과.
- `docs/user-guide.md`: 1절 한 줄, "구형 .hwp(HWP5) 읽기 전용 조회" 절, 오류 코드 표 문구.

## 6. 그 밖

- `Cargo.toml`(workspace): `hae-hwp5` 경로 의존 추가.
- `check-hwp5.log`(WIP가 저장소 루트에 남긴 중단된 실행 로그)는 지우고, 이 브랜치의 최종
  `scripts/check.sh` 전체 로그를 `.tmp/check-hwp5.log`로 남긴다(저장소 관례).
