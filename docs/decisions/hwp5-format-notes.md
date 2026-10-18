# HWP5(구형 `.hwp` 바이너리) 형식 조사 노트

작성: 2026-10-18, 구현 담당. 목적: 읽기 전용 최소 지원(`hwp5.inspect`, crate `hae-hwp5`)의 근거 기록.

이 문서는 **확인한 사실**과 **우리가 정한 정책**, **확인하지 못한 것**을 구분해서 적는다. 확인의 근거는
로컬에 클론되어 있는 공개 구현의 코드를 읽어 세 구현이 일치하는지 대조한 것이며, 구현 코드를 복사하지
않았다. 대조한 자료(코드 읽기만, 복사 없음):

- `neolord0/hwplib`(Java, Apache-2.0) — `~/.cache/cga/repos2/neolord0_hwplib`, 커밋 6746c27.
  `object/etc/HWPTag.java`, `reader/ForFileHeader.java`, `reader/bodytext/paragraph/ForParaText.java`,
  `object/bodytext/paragraph/text/HWPChar.java`, `reader/HWPReader.java`,
  `util/compoundFile/reader/{StreamReader,CompoundFileReader}.java`, `util/binary/Compressor.java`.
- `iyulab/unhwp`(Rust, MIT) — `~/.cache/cga/repos2/iyulab_unhwp`, 커밋 e55b340. `src/hwp5/*.rs`.
- `rhwp`(Rust, MIT) — `~/.cache/cga/repos/rhwp`(이 저장소가 렌더러로 이미 사용하는 프로젝트).
  `src/parser/cfb_reader.rs`, `src/parser/body_text.rs`.
- CFB 컨테이너 세부는 MS-CFB 사양을 구현한 `cfb` crate 0.15.0(로컬 cargo 레지스트리, MIT/Apache-2.0)의
  `internal/{header,sector,version}.rs`와 대조했다. `unhwp`가 이 crate를 컨테이너 파서로 쓴다.

## 1. 확인한 것 (세부와 근거)

### 1.1 컨테이너: MS-CFB(Compound File Binary, OLE2)

| 요소 | 확인 내용 | 근거 |
|---|---|---|
| 시그니처 | 파일 0바이트부터 `D0 CF 11 E0 A1 B1 1A E1` | `cfb` crate `consts::MAGIC_NUMBER` |
| 헤더 크기 | 512바이트. 섹터 오프셋 = `(섹터 번호 + 1) × 섹터 크기` | `cfb` `sector.rs` `(sector_id + 1) as u64 * sector_len` |
| 섹터 크기 | `sector shift`(헤더 0x1E, u16 LE)로 지정. v3=9(512바이트), v4=12(4096바이트). v3/v4 각각 값이 고정이며 그 밖은 거절 대상 | `cfb` `version.rs`, `header.rs`의 strict 검사 |
| 미니 섹터 | `mini sector shift`(0x20, u16 LE) = 6(64바이트). 4096바이트 미만 스트림은 미니 스트림에 저장 | `cfb` `consts::MINI_SECTOR_SHIFT` |
| FAT | `FAT 섹터 수`(0x2C, u32)와 DIFAT(헤더 0x4C의 109개 `u32` + DIFAT 섹터 체인, 첫 DIFAT 0x44/개수 0x48)로 FAT 섹터 목록을 만든다. FAT 엔트리 = 다음 섹터 `u32` | MS-CFB 사양, `cfb` `header.rs` |
| 체인 종료값 | `0xFFFFFFFE`=체인 끝(ENDOFCHAIN), `0xFFFFFFFF`=빈 섹터(FREESECT), `0xFFFFFFFD`=FAT 섹터, `0xFFFFFFFC`=DIFAT 섹터 | `cfb` `consts::Sector` |
| 디렉터리 | `첫 디렉터리 섹터`(0x30, u32)부터 FAT 체인. 엔트리 128바이트 | `cfb` `direntry.rs` |
| 디렉터리 엔트리 | 이름 UTF-16LE 64바이트(0x00), 이름 길이 u16(0x40), 종류 u8(0x42: 0=미할당, 1=스토리지, 2=스트림, 5=루트), left u32(0x44), right u32(0x48), child u32(0x4C), 시작 섹터 u32(0x74), 크기 u64(0x78) | `cfb` `direntry.rs`, `rhwp` `cfb_reader.rs` |
| 미니 스트림 | 루트 엔트리의 시작 섹터/크기 = 미니 스트림. 미니 FAT(0x3C 시작/0x40 개수)가 64바이트 미니 섹터 체인을 담는다 | `cfb` `minialloc.rs`, `rhwp` `cfb_reader.rs` |
| 미니 스트림 컷오프 | 0x38의 u32(표준값 4096). 이 값 미만인 스트림은 미니 스트림 | `cfb` `minialloc.rs` |

### 1.2 `FileHeader` 스트림 (항상 비압축, 256바이트)

| 요소 | 확인 내용 | 근거 |
|---|---|---|
| 시그니처 | 0x00부터 32바이트 필드에 `"HWP Document File"` + NUL 패딩 | `hwplib` `FileHeader.getFileSignature()`(32바이트 전체 비교), `unhwp` `header.rs`(앞 17바이트 비교) |
| 버전 | 0x20 u32 LE = `major<<24 \| minor<<16 \| build<<8 \| revision`. 예: `0x05010001` = 5.1.0.1 | `hwplib` `ForFileHeader.java` + `FileVersion.java`, `unhwp` `header.rs` |
| 속성 | 0x24 u32 LE 비트 플래그 | `hwplib` `ForFileHeader.java`, `unhwp` `header.rs` |
| 비트 0 | `COMPRESSED` — 스트림(DocInfo/BodyText 등)이 압축됨 | 두 구현 일치 |
| 비트 1 | `ENCRYPTED` — 암호 설정 문서 | 두 구현 일치 |
| 비트 2 | `DISTRIBUTION` — 배포용 문서 | 두 구현 일치 |
| 비트 3–11 | script, DRM, XMLTemplate, history, signature, 공인인증서 암호화, 서명 예비, 인증서 DRM, CCL | `hwplib` `ForFileHeader.java` 12개 비트 순서 |
| 비트 12–17 | mobile, privacy, track change, KOGL, video, order field | `unhwp` `header.rs`(hwplib은 11까지만 읽음). **출처가 다르므로 우리는 비트 0–4만 판정에 사용한다** |

### 1.3 스트림 이름과 압축

- `FileHeader`, `DocInfo`, `BodyText/Section0`, `BodyText/Section1`, … 스트림이 존재한다(`hwplib` `HWPReader.java`).
- 배포용 문서(비트 2)는 `BodyText` 대신 **`ViewText/SectionN`** 스토리지를 쓰고, 각 스트림 앞부분 256바이트에
  난독화된 AES 키가 붙은 **AES-ECB/PKCS5** 암호문이다(`hwplib` `HWPReader.bodyText()`,
  `CompoundFileReader.getChildStreamReaderForDistribution`, `StreamReader.decryptStream/secretKey`).
  키 난독화 알고리즘(`Obfuscation.transform`)은 우리가 다루지 않는다. → **거절**.
- 압축 방식은 **raw DEFLATE**(zlib 헤더 없음). `hwplib`은 `Inflater(true)`(nowrap), `unhwp`는
  flate2 `DeflateDecoder`(raw)를 쓴다. `hwplib`의 자체 압축기는 deflate 뒤에 4바이트 0과 원본 길이
  u32를 덧붙여 쓴다(읽을 때는 무시된다).
- 암호 문서(비트 1)도 스트림이 암호화된다(`hwplib`은 `StreamReader.decryptStream`으로 복호하지만 키가
  `DistributionDocData` 기반이고, 문서마다 다른 암호 검증이 필요하다). → **거절**.

### 1.4 레코드 스트림 (`DocInfo`, `BodyText/SectionN`)

| 요소 | 확인 내용 | 근거 |
|---|---|---|
| 레코드 헤더 | 4바이트 LE: tag = bits 0–9, level = bits 10–19, size = bits 20–31 | `hwplib` `HWPTag`/`StreamReader.readRecordHeader`, `rhwp` `record.rs`, `unhwp` `record.rs` |
| 확장 크기 | size 필드가 `0xFFF`(4095)면 다음 4바이트 u32가 실제 크기(헤더 8바이트) | 세 구현 일치 |
| 태그 시작값 | `HWPTAG_BEGIN = 0x10` | `hwplib` `HWPTag.java` |
| DocInfo 태그 | 0x10 DOCUMENT_PROPERTIES, 0x11 ID_MAPPINGS, 0x12 BIN_DATA, …, 0x1F LAYOUT_COMPATIBILITY | `hwplib` `HWPTag.java`(BEGIN+0…+15), `unhwp` `TagId` |
| BodyText 태그 | 0x42(66) PARA_HEADER, 0x43(67) PARA_TEXT, 0x44(68) PARA_CHAR_SHAPE, 0x45(69) PARA_LINE_SEG, 0x46(70) PARA_RANGE_TAG, 0x47(71) CTRL_HEADER, 0x48(72) LIST_HEADER, 0x49(73) PAGE_DEF, … 0x58(88) EQ_EDIT | `hwplib` `HWPTag.java`(BEGIN+50…), `rhwp` `tags.rs`, `unhwp` `TagId` |
| 구역 수 | DocInfo의 `DOCUMENT_PROPERTIES`(0x10) 레코드 본문 **첫 u16 = 구역 수**. 그 뒤 start number u16×6 등이 온다(hwplib writer는 본문 26바이트) | `hwplib` `ForDocumentPropeties.java`/writer |

### 1.5 `PARA_HEADER`(0x42)와 `PARA_TEXT`(0x43)

- `PARA_HEADER` 본문 첫 u32 = (bit 31 = 리스트 마지막 여부, bits 0–30 = **문단 글자 수**). 이어서
  controlMask u32, paraShapeId u16, styleId u8, divideSort u8, charShapeCount u16, rangeTagCount u16,
  lineAlignCount u16, instanceID u32, (5.0.3.2 초과 버전에서) isMergedByTrack u16
  (`hwplib` `ForParaHeader.java`).
- `PARA_TEXT` 본문은 **UTF-16LE 코드 유닛의 나열**이다. 코드 유닛 0x0000–0x001F는 제어 문자이고
  표 6의 분류는 다음이다(`rhwp` `body_text.rs` `is_extended_ctrl_char` 주석이 HWP 5.0 표 6을 인용,
  `hwplib` `HWPChar.type()`과 완전히 같은 분류):
  - **문자(char) 컨트롤: 1 코드 유닛(2바이트)** — 0x0000, 0x000A, 0x000D, 0x0018–0x001F.
  - **인라인(inline) 컨트롤: 8 코드 유닛(16바이트)** — 0x0004–0x0009, 0x0013–0x0014.
  - **확장(extended) 컨트롤: 8 코드 유닛(16바이트)** — 0x0001–0x0003, 0x000B–0x000C, 0x000E–0x0012,
    0x0015–0x0017.
  - 예: 0x000A = 줄 끝, 0x000D = 문단 끝, 0x0009 = 탭(8 유닛), 0x001E = 묶음 빈칸(NBSP),
    0x001F = 고정폭 빈칸. 0x0018은 소프트 하이픈으로 저장된다(`rhwp`).
  - **주의(구현 간 불일치)**: `unhwp`는 0x0019–0x001D를 8유닛, 0x0018을 8유닛으로 분류한다.
    `rhwp`(표 6 인용)와 `hwplib`(2바이트)이 일치하므로 우리는 **0x0018–0x001F를 1 유닛**으로 처리한다.
- 한 `PARA_TEXT` 레코드의 본문이 UTF-16LE 2바이트의 배수가 아니면 손상이다.

## 2. 우리가 정한 정책 (사실이 아니라 구현 선택)

1. **구역 목록**: DocInfo를 해석하지 않고 `BodyText` 스토리지의 `Section<N>` 스트림 이름을 숫자 순으로
   열거한다. `hwplib`은 DocInfo의 구역 수를 권위로 삼지만, 우리는 1단계 범위를 줄이려고 컨테이너
   열거를 쓴다. 두 값이 다를 수 있다는 사실은 인정하고, 구역 수는 "컨테이너에서 발견한 수"로 보고한다.
2. **압축 폴백 없음**: `COMPRESSED`인데 raw DEFLATE 해제가 실패하면 원시 바이트로 대신 읽지 않고
   `PACKAGE_REJECTED(malformed_record_stream)`로 거절한다. (`hwplib`은 실패 시 원시 스트림으로
   폴백하지만, 그 폴백은 손상을 조용히 텍스트로 바꿀 수 있다.)
3. **텍스트 추출**: 위 표 6 분류대로 제어 문자를 건너뛰고, 0x000A는 `\n`, 0x001E는 U+00A0,
   0x001F는 U+2007, 0x0018은 U+00AD로 옮긴다. 0x0012(자동 번호)에는 자리표시 문자를 만들지 않는다.
   0x0000과 0x0019–0x001D는 버리고, 0x000D를 만나면 그 레코드의 나머지를 읽지 않는다. 서로게이트
   짝은 `String::from_utf16_lossy`로 합친다.
4. **문단 수**: `PARA_HEADER` 레코드 수(+ `PARA_HEADER` 없이 `PARA_TEXT`로 시작한 예외 문단 수)로 센다.
   표 셀 안의 문단도 레코드 순회에 잡히므로 포함된다. 셀 경계는 식별하지 않는다.
5. **거절 구분**: 비트 2(배포용)는 `PACKAGE_REJECTED(distribution_unsupported)`, 비트 1(암호)와
   비트 4(DRM)는 `PACKAGE_REJECTED(encrypted)`로 거절한다. DRM 스트림의 실제 보호 방식은 확인하지
   못했으므로(§3) DRM을 암호와 같은 사유로 묶는 것은 **우리의 판단**이며 근거가 약하다.
6. **한도**: 입력 크기는 `Limits.max_hwpx_bytes`(다른 문서 입력과 같은 한도), 해제 누적량은
   `Limits.max_inflate_total`, 컨테이너 스트림·섹터 항목 수는 `Limits.max_entries`,
   문단 수는 새 `Limits.max_hwp5_paragraphs`로 제한한다.

## 3. 확인하지 못한 것 (구현하지 않음)

- 배포용 문서의 키 난독화·AES 복호 세부(우리는 거절만 한다).
- 암호 문서의 복호 절차(`hwplib`은 `DistributionDocData` 기반 키를 쓰지만 전체 절차는 미구현).
- DRM(비트 4) 문서에서 스트림이 어떻게 보호되는지. 열어 보지 못했다.
- DocInfo 레코드 본문의 나머지(폰트·스타일·번호 정의 등)와 그 해석.
- 표·컨트롤(각주/미주/머리말/수식/그림) 레코드 구조와 셀 경계. 우리는 `PARA_TEXT`만 읽고
  컨트롤 소속은 구분하지 않는다.
- HWP 3.x(완전히 다른 형식), HWPML, 배포용이 아닌 옛 도구가 만든 변형 파일.
- 한컴 버전별 표 6/태그 차이. 5.0.2.2와 5.1 표본에서 텍스트가 나오는지만 실물로 확인한다(§4).
- `PARA_HEADER`의 문단 글자 수 필드와 우리 추출 글자 수가 정확히 같아야 하는지. 제어 문자 처리
  차이 때문에 다를 수 있으며, 우리는 우리 규칙의 결과만 보고한다.

## 4. 실물 검증 결과 (2026-10-18)

- 실물 `.hwp` 표본: `~/.cache/cga/repos2/neolord0_hwplib/sample_hwp/`(hwplib 저장소의 공개 표본 44개,
  Apache-2.0 저장소에 포함된 파일). 저장소에는 복사하지 않고 로컬에서만 사용했다.
- 실행: `hae run --command hwp5.inspect`(디버그 빌드)로 표본 전체를 한 번씩 읽었다.
- 결과: **44개 중 43개 판독 성공, 1개 거절**. 거절은 `distribution.hwp` 한 건으로
  `PACKAGE_REJECTED(distribution_unsupported)`였다(배포용 문서 — §1.3에서 확인한 대로 ViewText+AES).
- 판독한 버전: 5.0.2.2(1), 5.0.3.4(15), 5.0.5.0(25), 5.1.0.1(2). 압축 27개·비압축 16개.
- 최대 표본 `big_file.hwp`(10.6 MB, 압축): 구역 1, 문단 147,456, 글자 1,769,472. CLI 전체 실행(프로세스
  시작 포함) 약 1.4초(디버그 빌드).
- 입력 불변: `필드-누름틀.hwp` 실행 전후 SHA-256이 같았다(`649aff07…e14b`). 문단 커서
  (`sha256:offset`)도 `big_file.hwp`에서 다음 쪽을 이어 받았다.
- 독립 구현 대조: `rhwp` 0.8.7(MIT, 이 저장소가 렌더러로 쓰는 구현)의 `export-text --json`과
  문단 텍스트를 대조했다. 공백 문자(공백·줄바꿈·NBSP·고정폭 빈칸)를 제거한 뒤 **42개 중 30개가
  문자 단위로 완전히 같았다**. 다른 12개의 차이는 모두 우리가 만들지 않는 값에서 왔다:
  자동 번호·글머리표(`문단번호 1-10 수준.hwp`의 `1.`/`●`), 수식 렌더(`수식.hwp`),
  글자겹침 표시(`글자겹침.hwp`), 숨은 설명 표시/숨김(`숨은설명.hwp`), 조판 순서
  (`etc.hwp`는 페이지 텍스트가 `C`를 먼저 두지만 레코드 순서는 `가나다라…`가 먼저다).
  우리 추출은 PARA_TEXT에 저장된 것만 그대로 내보내므로 이 차이는 예상된 것이다.
- 확인하지 않은 것: Windows/한컴오피스에서의 열기·조판 대조, 개인 문서, 암호 문서(표본 없음),
  배포용 문서의 복호. 이 검증은 **공개 표본 + rhwp 대조**까지다.
