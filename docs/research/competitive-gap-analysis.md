# HWPX Agent Engine 경쟁 분석·기능 격차 보고서

작성: 2026-10-18 (KST). 갱신: 2026-10-19 (D단계 결과·0.3 기능 반영). 총괄·종합 판단: 조사 총괄(DeepSeek V4.1 Flash, opencode-go).
조사 대상: `hwpx-agent-engine` (HAE), 브랜치 `wip/stage1-model-edit`.
기준 HEAD: 조사 시작 시 `1cf8f5c`(B단계 고정), 종합 시점 `21ec467`, **D단계·0.3 갱신 시점 `76b0f72`(명령 57개)**.

## 이 보고서의 성격과 한계

이 문서는 기존 계획을 정당화하기 위한 것이 아니라, 실제 경쟁 환경과 실행 근거로 HAE의 위치를 판정한 것이다. 다음 원칙을 지켰다.

- **구현됨 / 부분 / 미구현 / 미검증을 구분한다.** "기능이 있다"와 "한컴에서 안 깨진다"를 같은 칸에 넣지 않는다.
- **모든 판정에 근거를 붙인다.** 코드 경로, 실행 명령, 원문 출력, 공개 URL 중 하나 이상.
- **조사원 간 상충은 총괄이 재현해 확정한다.** 하위 조사 결과를 그대로 취합하지 않았다.
- **확인 못 한 것은 "미확인"으로 남긴다.** 추측을 결론으로 쓰지 않는다.

조사 체계(모두 병렬 위임, 총괄이 교차 검증):

| 갈래 | 담당 모델 | 산출물 |
|---|---|---|
| A1 경쟁 7개 프로젝트 심층·실행 | GPT 6 Luna (max) | `~/.cache/cga/a1/report.md`, `a1/projects.json` |
| A2 추가 후보 발굴(97건) | DeepSeek V4.1 Flash (max) | `~/.cache/cga/a2/report.md`, `a2/candidates.json` |
| B HAE 자체 전수·경계·회계 | GPT 6.1 Sol (high) | `~/.cache/cga/b/report.md`, `b/inventory.json` |
| S OWPML 명세 61행 매트릭스 | Gemini 3.8 Flash High | `~/.cache/cga/spec/report.md`, `spec/matrix.csv` |
| C 교차검증(반례) | Gemini 4 Argon | `~/.cache/cga/c/report.md` |
| D 대표 시나리오 벤치마크 | GPT 6 Luna (max) | `~/.cache/cga/d/` |

총괄이 직접 재현해 확정한 항목은 본문에 **[총괄 확정]**으로 표시했다.

---

## 1. 현재 엔진의 실제 기능·완성도 요약

### 1.1 규모

`1cf8f5c` 기준 조사 당시: **13개 crate, 22개 명령, 14개 영역의 38개 기능**(구현 13 · 부분 17 · 미구현 7 · 미검증 1; 근거 `b/report.md` §2).

**2026-10-19 D 실행 시점(HEAD `76b0f72`, 0.3)**: `command.list` 실행 결과 **명령 57개**(신규 34개) — D단계에서 release 빌드로 직접 확인. 아래 §1.2 표는 0.3 기준으로 고쳤다. 명령 수 자체는 기능 완성도가 아니다(§9의 한계 참조).

### 1.2 영역별 실제 상태

| 영역 | 상태 | 실제로 되는 것 / 안 되는 것 |
|---|---|---|
| ZIP·XML 파싱, 원본 바이트 보존 | **구현** | 방어적 파싱, 18종 거절 사유, 미편집 항목 원시 압축 바이트 보존. HWPX 의미·공식 XSD 전체 검증은 아님 |
| 모델·참조 무결성 | **구현** | 안정 `nodeId`, 참조 무결성 독립 검증, 100단위 Undo/Redo. 전체 패키지 그래프 검증은 아님 |
| 텍스트·문단 서식 | **구현** | `text.replace`(자소 단위·삽입·`controls:preserve`), `text.replaceAll`, 제한적 `paragraph.format` |
| **글자 서식(`charPr`) 생성·편집** | **구현(0.3)** | `text.format` — 굵게·기울임·밑줄·취소선·크기(100~100000 HWPUNIT)·색. 기존 charPr 재사용 또는 최대 1개 추가. D T8에서 굵게+15pt 후 header charPr 1개 추가·참조 확인 |
| 문서·문단·표 구조 | **부분(0.3)** | 표 **구조 편집** `table.insertRow/deleteRow/insertColumn/deleteColumn`(빈 셀 복제, 병합 경계 거절) 구현 — D T9에서 2→3행 확인. **문단·개체·새 문서 생성은 0.3에도 없음**(D T7에서 경쟁 5/5 성공, HAE만 미지원) |
| 이미지 | **부분** | 기존 PNG/JPEG 교체(0.3에서 `image.fit`·기하 수정 포함), 개체·기하 보존. 생성 시 APNG·자르기·효과 거절 |
| 도형·차트·OLE | **미구현**(보존만) | 바이트 보존. 속성 조회·수정 API 없음 |
| 수식 | **부분(0.3 확장)** | `equation.get/list/setScript/setProperties` 추가 — 스크립트·속성 편집. 조판·AST 검증은 아님. D는 보존만 시험 |
| 스타일·목록·필드 | **부분(0.3)** | `field.list`·`field.setText`·`form.fill`(누름틀 값 바인딩), `style.list`·`paragraph.setStyle`. 목록 편집·생성은 없음 |
| 페이지 설정·머리말·꼬리말·각주 | **부분(0.3 확장)** | **[총괄 확정]** 내부 문단은 노드로 노출되어 편집·내보내기·재열기까지 성공. 0.3에서 `section.getPage/setPage/setPageNumbering/setVisibility`·`header.setText` 추가(용지·여백·쪽 번호·머리말/꼬리말 텍스트) |
| 검색·부분 조회·대용량 | **부분** | 5천 문단·5.6 MB 편집 성공. 큰 전체 텍스트 조회는 MCP 응답 한도(4 MiB)에 걸림 |
| 템플릿·자동 생성 | **부분** | 기존 시드 치환 수준. 새 문서 생성 없음 |
| 트랜잭션·버전·Undo/Redo | **구현** | 원자 `command.batch`, `dryRun`, 버전 충돌, 프로젝트 잠금. 이력 100단위 |
| 내보내기·렌더링 | **구현** | 검증 후 저장, 격리 SVG/PNG 미리보기, 100쪽 제한 |
| MCP·에이전트 친화성 | **구현** | 0.3에서 MCP 도구 **57개**(`command.list` 57, D단계 확인), 3개 프로토콜 경로. §6.1 P4의 출력 스키마 부족은 유지 |
| 입력 검증·리소스 | **구현**(결함 2건) | 80개 적대 fixture 포함 선별 Rust 318개 통과. §6의 P1·P2 결함 |
| 플랫폼·배포 | **부분** | Linux 동작. Windows는 0.3에서 Unix 전용 코드가 정리되어 **크로스 체크 통과**(§14, `io/windows.rs`) — 실제 Windows 한컴 열기는 미확인(§11 U6), 미리보기는 fail closed. macOS 미검증 |

### 1.3 검증 범위 (B가 실행한 것과 안 한 것)

- 실행: workspace(exclude exec) 264 + exec lib 13 + editable_verified 22 + preview 7 + replace_all 10 + replace_all_fixtures 2 = **318 passed, 0 failed**
- 추가 수동 합성: structure 39/39, namespace 4/4, unicode 9/9, 공개 코퍼스 10/10 open·inspect
- **미실행**: `scripts/check.sh` 전체 gate, `cargo fmt`/`clippy`, Python 전체 unittest, MCP evaluation 전체, Windows/macOS 빌드·실행, 한컴/LibreOffice 조판, 기본 512 MiB 초과 stress

즉 318이라는 숫자는 **공개·합성 입력 범위의 초록**이며 전체 관문 수치가 아니다.

- 0.3 게이트(별도 작업 기록, `docs/decisions/0.3-contract-followup.md` §3): `check.sh` 렌더러 없음 Rust 766/0·스크립트 33/33, 있음 779/0 + 실제 renderer 13/13, Python oracle 111/111, MCP 평가 146/146.
- D단계(2026-10-19, `~/.cache/cga/d/report.md`): 공개 fixture·시나리오 9종을 HAE 0.3(CLI+MCP)와 경쟁 5개로 직접 비교. HAE 산출물 7종의 Linux 한컴 2020 Beta 첫 프레임 OPEN. **Windows 한컴 열기는 여전히 미확인**(§11 U6).

---

## 2. 경쟁 프로젝트 비교 매트릭스

### 2.1 심층 실행 비교 (A1, 공개 코퍼스 2개 고정)

시나리오: S1 열기·추출 / S2 문단 교체·저장 / S3 표 셀 교체 / S4 제목·문단·표 생성 / S5 미편집 part payload 동일.

| 프로젝트 | S1 | S2 | S3 | S4 생성 | S5 보존 | 제약 |
|---|---|---|---|---|---|---|
| rhwp | 불가¹ | 불가¹ | 불가¹ | 불가¹ | 불가¹ | crates.io TLS·`argon2` 부재로 빌드 실패 |
| python-hwpx | 성공 | 성공 | 성공 | 성공 | 성공² | 일부 압축 스트림 변화 |
| python-hwpx-automation | 성공 | 성공 | 성공 | 성공 | 성공² | MCP 128 도구 |
| hwp-mcp | 성공 | 성공 | 성공 | **실패³** | 성공⁴ | 생성 표 readback 불일치 |
| kordoc | 성공 | 성공⁵ | 성공⁵ | 성공 | 성공⁶ | "블록 추가 미지원" 잔차 |
| hwpx-skill | 성공 | 성공 | 성공 | 성공 | 성공⁶ | 압축 스트림·member 집합까지 동일 |
| hancom-mcp | 성공⁷ | 성공⁸ | 성공 | 성공 | 성공⁴ | 구조 응답 문단 0 |

¹ 빌드 실패는 Windows/한컴 필요 때문이 아니라 격리 환경의 의존성 문제다.
² 기존 미편집 part의 해제 payload 13개 동일. ZIP 전체 바이트 동일은 아님.
³ 요청 `[["항목","값"],["상태","성공"]]` → 재조회 `[["값",""],["성공",""]]`.
⁴ 기존 part 13개 payload 동일하나 `Scripts/*` 엔트리를 **추가**했다.
⁵ 목표 텍스트는 확인됐으나 도구가 "수정 0 · 추가 1" 잔차를 보고.
⁶ payload + 압축 스트림 + member 집합 동일(파일 전체 byte 동일 주장은 아님).
⁷ 6,172자 반환했으나 구조 응답에서 본문 문단 0·표 24.
⁸ 첫 검색어 치환 0건 → 공개 파일에서 확인 가능한 다른 문단으로 대상 조정 후 성공. 동일 요청 성공으로 세지 않음.

### 2.2 후보 지형 (A2, 97건 발굴)

- **비교 가치 높음 15건**: `hancom-io/hwpx-owpml-model`(한컴 공식 OWPML C++, Apache-2.0, 2023 정지), `hancom-io/dvc`·`PolarisOffice/polaris_dvc`(HWPX 검증기, 폴라리스 Rust 이식), `neolord0/hwpxlib`(Java OWPM 모델 + 실패 코퍼스), `neolord0/hwplib`(HWP5 + 골든 테스트), `neolord0/hwp2hwpx`(HWP5→HWPX 매핑), `ebandal/H2Orestart`(LibreOffice 확장, HWP5 파서 + PDF 경로, GPL-3.0), `iyulab/unhwp`(Rust HWP3/5/HWPX, FFI·다국어 배포), `ai-screams/HwpForge`(YAML 스타일 블루프린트), `Indosaram/hwpers`(Rust 레이아웃 렌더링), `kwakseongjae/auto-hwp`(parse→typeset→render→edit→PDF), `ohah/hwpjs`(Zig + 프리빌드 바인딩), `Dayoooun/hwpx-mcp`(MCP 125 도구), `UpstageAI/korean-report-hwpx`(공문서 생성 MCP), `roboco-io/hwp2md`(Go, `docs/hwp5-schema.md`)
- 분포: GitHub 71 · npm 9 · NuGet 4 · RubyGems 2 · Maven 1 · PyPI 1 · Docker 1 · GitLab 1 · 패밀리 2
- A2의 시장 판정: **HWPX 쓰기는 이미 "해결된 문제"로 취급**되고 경쟁 축이 "한컴에서 열었을 때 안 깨지는가"로 이동. **HWP5 쓰기는 여전히 빈 자리.**

### 2.3 A2가 남긴 미확인

- `websearch` 도구가 세션 내내 실패 → Brave 검색 HTML을 curl로 파싱해 대체, 일부 한국어 질의 실패
- `gh search code` 스코프 제약으로 코드 검색 기반 발굴 불완전
- Aspose/GroupDocs의 HWP 네이티브 지원 여부 **미확인**

---

## 3. 독립성 및 외부 의존성 분석

### 3.1 한컴으로부터의 독립성 — 높음

- Cargo 의존성에 한컴 SDK/COM/native 라이브러리 **없음**. `Cargo.lock` 외부 패키지 114개 중 HWP 계열은 전무
- 코어(ZIP·XML·package·model·edit·verify)는 자체 구현
- 실제 CLI/MCP open·edit·export를 **한컴 없이 실행**해 확인
- 한컴 namespace URI·형식 규칙을 다루는 것은 한컴 바이너리 의존과 다르다
- 단, 소스 독창성·전체 외부 코드 provenance 감사는 이번에 하지 않았다

### 3.2 rhwp로부터의 독립성 — 코어는 독립, 미리보기는 의존

- 코어 편집·검증·저장은 rhwp 미설정에서도 동작
- 미리보기·페이지 대응은 rhwp **외부 프로세스**에 의존(`HAE_RHWP`), 미설정 시 `RENDERER_UNAVAILABLE`
- Cargo git/library 의존이 아니며 `spikes`는 root workspace에서 제외
- `hae-render-rhwp/pages.rs`는 자체 주석상 spike 주소 매핑의 이식이며, 전면 독자 발명을 주장하지 않는다
- 현재 실행 파일의 expected hash/version 강제 allowlist는 **없음** — 운영 위험

### 3.3 직접 외부 crate 15개 (모두 정확 버전 `=` 고정)

`quick-xml`·`flate2`·`crc32fast`·`unicode-segmentation`·`serde`·`serde_json`·`schemars`·`sha2`·`getrandom`·`png`·`zune-jpeg`·`rustix`·`thiserror`·`clap`·`resvg`.

- 이들은 **범용 라이브러리**이며 HWPX 도메인 지식은 들어 있지 않다 → 교체 가능성이 낮고 유지보수 위험도 낮다
- 선택적 의존(미리보기): `rhwp` 실행 파일, `bubblewrap`, `prlimit`+Linux user namespace, `hae-raster`, 폰트 바이트
- `hae-exec`가 `hae-render-rhwp`를 **무조건** 의존 → 렌더러 전이 의존이 코어 배포에 포함된다(§6 부채)

### 3.4 재사용 가능 요소와 라이선스 제약

| 요소 | 라이선스 | 재사용 판단 |
|---|---|---|
| `hancom-io/hwpx-owpml-model` | Apache-2.0 | 스키마·요소 모델 참조로 유용. 2023 정지 → 사양 변경 대응 없음 |
| `hancom-io/dvc`, `PolarisOffice/polaris_dvc` | Apache-2.0 | HWPX 검증기. Rust 이식본은 이식 가능 형태 |
| `neolord0/hwpxlib`·`hwplib` | 확인 필요 | 골든 테스트 세트·실패 코퍼스가 특히 값짐 |
| `H2Orestart` | GPL-3.0 | **복사 시 전염**. 참조만 하고 코드 재사용 불가 |
| 라이선스 미확인 다수 | — | 사용 전 개별 확인 필요 (A2 표) |

**총괄 의견**: HAE가 자체 구현을 유지해야 하는 영역은 **원본 보존·트랜잭션·검증 계약**이다. 반대로 **HWP5 파서**와 **공식 스키마 적합성 검사**는 독립 구현 실익이 낮고 검증된 외부 자산이 이미 있다.

---

## 4. 기술적 강점과 검증되지 않은 주장

### 4.1 실증된 강점

| 강점 | 근거 |
|---|---|
| **무편집 무손실** | Undo 후 내보내기가 원본과 **바이트 동일**(여러 문서·경로에서 확인). 경쟁 프로젝트 대부분은 payload 동일까지만 확인했고 압축 스트림까지 맞춘 곳은 일부다 |
| **독립 저장 관문** | `hae-verify`가 편집 계층에 의존하지 않고 별도 구현. `hae-edit`·`hae-model`·`hae-exec`에 의존하지 않음을 의존성으로 확인 |
| **원자 트랜잭션** | `command.batch` 실패 시 전면 원복, `dryRun`, 버전 충돌, 프로젝트 잠금 |
| **한컴 실행 독립** | 한컴 설치 없이 전 경로 동작 |
| **실제 클라이언트 검증** | MCP를 Codex CLI·OpenCode·Inspector로 실연결 (22/22 도구) |

### 4.2 검증되지 않은 주장 (광고 대비 실제)

| 주장 | 실제 |
|---|---|
| "0.1 완료" | Windows·한컴 검증 미완. `nativeComparison`은 계속 `not_checked` |
| "단계 4 거절 24건 재시도 성공" | 실제는 성공 18 + 설계상 거절 6 + 수식 제외 1 (기록 정정 완료) |
| 머리말·꼬리말·각주 "보존만" (S 판정) | **[총괄 확정] 틀렸다.** 내부 문단은 편집 가능 |
| 글상자(`drawText`) "편집 차단" (S 판정) | **[총괄 확정] 틀렸다.** 글상자 안 문단이 노드로 노출되고 편집 성공 |
| "구조 추출" 전반 | hancom-mcp·hwp-mcp 사례는 경쟁사 문제. HAE도 큰 전체 텍스트 조회는 4 MiB 한도에 걸림 |
| 22개 명령 스키마 | 10개 출력 스키마가 구체성 부족, `title` 없음, Rust 전용 `uint*` format 사용 |

**[총괄 확정] S 매트릭스 사용 주의**: S의 범주별 판정(도형 0%, header_footer 0%, note 0%)은 **요소 수준** 관점이며, 그 내부 문단이 노드로 노출되는 사실을 반영하지 못했다. 격차 목록에서 `drawText`·머리말·꼬리말·각주를 "편집 불가"로 읽으면 안 된다. 이 항목들은 **텍스트는 편집되고 구조·속성은 보존**된다. (검증: `04_complex.hwpx` 머리말·각주 문단 편집·내보내기·재열기, `research-school-plan-reference.hwpx` 글상자 문단 편집)

---

## 5. 기능 격차 전체 목록

표기: **난이도** = 아키텍처 변경량 기준 하/중/상/최상. **계약 영향** = 원본 보존·검증 계약을 얼마나 건드리는지.

### 5.1 기존 문서를 더 정확히 고치는 쪽 (HAE의 현재 축)

| # | 격차 | 사용자 가치 | 현재 | 경쟁 | 아키텍처 변경 | 난이도 | 계약 영향 | 선행 | 권장 |
|---|---|---|---|---|---|---|---|---|---|
| G1 | **글자 서식(`charPr`) 생성·편집** | 매우 높음. 굵게·크기·색은 문서 편집의 기본 요구 | 없음(복사만) | python-hwpx·hwpxlib·automation 모두 지원 | header 자원 생성 + run 분할 + 검증 확장 | 중 | 중 — 새 자원 생성 경로가 늘어남 | 없음 | **0.3 최우선** |
| G2 | **표 구조 편집**(행·열 추가·삭제·병합·분할) | 매우 높음. 실무 표는 대부분 수정 대상 | 없음 | 다수 지원. 단 hwp-mcp는 생성 표 readback 불일치 | 노드 생성·삭제, ID 대응표, Undo, 배치 캐시 전면 재설계 | **최상** | **높음** — "노드 불변" 가정을 깨뜨림 | G1, 버전 정책 | 0.4 이후 별도 설계 |
| G3 | **문단·개체 생성**(문단 추가/삭제, 그림 삽입, 새 문서 생성) | 높음 | 없음 | 폭넓게 지원 | 위와 같음 + 패키지 생성 경로 | 상 | 높음 | G2 | 0.4 |
| G4 | **누름틀 필드 값 바인딩**(`fieldBegin`/`End`) | 높음. 공문서 자동화의 핵심 | 필드 경계 보존·차단 | 일부 스킬이 하드코딩 | 필드 모델 + 값 치환 명령 | 중 | 중 — 필드 의미 보존 규칙 필요 | 없음 | 0.3 후보 |
| G5 | **구역·용지·단 설정**(`secPr`, `colPr`) | 중간 | 보존만 | 일부 지원 | 구역 속성 모델 | 중 | 낮음 | 없음 | 0.4 |
| G6 | **도형 속성 조회·편집** | 중간. 코퍼스 81건 | 보존만 | 대부분 미지원 | 도형 모델 | 상 | 낮음~중 | 없음 | 0.4 |
| G7 | **머리말·꼬리말·쪽 번호 제어** | 중간 | **[총괄 확정]** 내부 문단 편집 가능, 구조·번호 제어는 없음 | 일부 | 컨테이너 탐색 + 번호 모델 | 중 | 낮음 | 없음 | 0.3 후보 |
| G8 | **수식 편집** | 낮음~중간(공개 코퍼스 0건) | 조회·보존 | 거의 없음 | 수식 AST·조판 | 최상 | 높음 | 별도 설계 | 범위 밖 |

### 5.2 HAE만의 축 — 검증·호환성 (경쟁 대비 실제 우위 후보)

| # | 격차 | 사용자 가치 | 현재 | 경쟁 | 난이도 | 권장 |
|---|---|---|---|---|---|---|
| G9 | **HWPX 검증기 공개화** — 규칙 목록 문서화 + 테스트 수트 배포 | 높음. A2 판정: "검증 규칙을 공개 문서화하는 것 자체가 차별점" | 자체 검증기 존재, 규칙 문서는 내부용 | `hancom-io/dvc`(정지) + `polaris_dvc`(문서 없음)가 유일 | 중 | **0.3 후보.** HAE의 최대 자산 |
| G10 | **공식 사양 대비 편차 목록** | 중간~높음. 생태계에 없음 | 없음 | 아무도 공개 안 함 | 중 | 0.3 |
| G11 | **한컴 실문서 골든 코퍼스** | 높음. 공개 HWPX 실문서 코퍼스가 없음 | 사내 fixture만 | 조각조각 흩어짐 | 중(법적 검토 포함) | 0.3~0.4 |
| G12 | **한컴 조판 동등성 자동 판정** | 높음 | Linux 한컴 화면 판독(수동) | 없음 | 상 | 지속 |

### 5.3 다른 축 (전략 판단 필요)

| # | 격차 | 가치 | 현재 | 경쟁 | 난이도 | 권장 |
|---|---|---|---|---|---|---|
| G13 | **HWP5 읽기**(필요시 무손실 왕복) | 높음. 학교·관공서 문서 상당수가 `.hwp` | **부분(0.3)**: `hwp5.inspect` — 파일헤더·압축·암호 플래그, 구역·문단 수, 문단 텍스트 읽기(읽기 전용, `crates/hae-hwp5`) | `hwplib`·`unhwp`·`H2Orestart`·`hwp2hwpx` | 왕복·편집: 상 | 외부 자산 대비 자체 구현의 검증 범위 확인 필요 |
| G14 | **HWP5 쓰기** | 중간 | 없음 | A2 판정: **"여전히 빈 자리"** | 최상 | 범위 밖 또는 차별화 기회 |
| G15 | **레이아웃 렌더링 자체 구현** | 중간 | rhwp 위임 | `hwpers`·`H2Orestart`·뷰어 계보 | 최상 | 범위 밖 |
| G16 | **공공 서식 템플릿 라이브러리** | 높음(실사용) | 없음 | 스킬 20여 건이 각자 하드코딩 | 중 | 0.4+ |
| G17 | **비동기·취소·진행률**(긴 작업) | 중간~높음 | 동기 실행기가 stdio를 막음, MCP에 cancel/progress 없음 | 다수 서버가 스트리밍 | 중 | 0.3 후보 |
| G18 | **다중 문서·동시성** | 중간 | 단일 프로세스·단일 문서 | 일부 서버 지원 | 상 | 범위 밖 |
| G19 | **Windows 지원** | 높음. 현재 Unix 전용 코드로 **빌드 장애** | Linux만 | 대부분 크로스플랫폼 | 중 | **0.3 필수** |
| G20 | **출력 스키마 구체성·표준 format** | 낮음~중간 | 22개 중 10개 부실, `uint*` 비표준 format | — | 하 | 0.3 |

### 5.4 격차가 아닌 것 (오해 방지)

- **`drawText`·머리말·꼬리말·각주 텍스트 편집** — 이미 된다(§4.2).
- **미편집 무손실 내보내기** — 이미 달성. 경쟁사보다 강하다.
- **표 셀 텍스트** — 된다.

---

## 6. 숨은 기술 부채와 아키텍처 위험

### 6.1 확인된 결함

| ID | 내용 | 등급 | 근거 |
|---|---|---|---|
| **P1** | `verify_steps.pre_state`의 XML payload가 세션 메모리 회계에서 누락. 2 MiB 예산에서 40회 편집·clear를 허용했고 실제 보유 본문은 4 MB 이상인데 회계는 약 315 KB | **높음** | `session.rs:70,322,1305,800` |
| **P2** | `cursor`+`limit` usize 덧셈 overflow로 debug CLI가 exit 101로 종료(`query.rs:684`). MCP는 `-32603` 후 생존. release에서 동일 crash라고 주장하지 않음 | 중간 | `session.rs:657`, `query.rs:684` |
| **P3** | **Windows 빌드 장애** — Unix 전용 코드 경로 | 높음 → **정정(§14)** | `b/report.md` §2; `1ae63ee`·`bd80331`: `io/windows.rs` 존재, 크로스 체크 통과 |
| **P4** | 22개 명령 중 10개 출력 스키마 구체성 부족, `title` 없음, Rust 전용 `uint*` format | 낮음 | 총괄 재확인 |

P2는 총괄이 형식을 바로잡아 재현을 시도한 결과 **정상 동작**을 확인한 항목이 여럿 있었으므로, C단계에서 확정한다. P1은 재현 절차와 수치가 구체적이므로 실재할 가능성이 높다.

### 6.2 아키텍처 위험

| 위험 | 평가 | 내용 |
|---|---|---|
| 동기 실행기 | 높음 | 긴 편집·렌더링이 같은 stdio 스트림을 막는다. MCP에 독립 cancel·progress 도구 없음. 코어 파싱·편집에 wall-clock guard 없음 |
| 구조 생성 불가 | 의도된 제한, 확장 비용 높음 | `rebuild`가 section 목록·identity·stable path를 고정. 새 노드는 ID map·원본 검증·Undo·PrvText·layout 정책을 함께 설계해야 함 |
| 단일 프로세스·단일 문서 | 확장은 별도 설계 | `open_doc` 하나. MCP의 "무상태"는 per-request 협상일 뿐 상태를 외부 저장소에서 복원하는 진짜 무상태가 아님 |
| 버전 정책 | 호환성 규칙·migration 부족 | descriptor `schemaVersion 0.2.0`, envelope·project manifest `1`. unknown version 거절, migration 없음. 패키지 버전과 wire version을 구분해야 함 |
| 메모리 회계 | 실제 결함(P1) | `compute_memory`가 문서 원시 버퍼만 세고 `verify_steps.pre_state`·image Vec·XML tree/node/index·writer response cache를 빠뜨림 |
| 대용량 처리 | 부분적 pagination | ZIP 전체 inflate, XML 전체 인덱스, query full Vec. `inspect`만 cursor |
| 형식 보존 vs 의미 지원 | 주의 | unknown markup 유지가 의미 해석·편집 가능성을 뜻하지 않음. parser는 UTF-8·well-formed·package graph까지이며 공식 XSD 전체가 아님 |
| 독립 verifier 한계 | 좋은 경계, 전체 독립 파서 아님 | editor/model planner에 의존하지 않지만 `hae-zip`/`hae-xml`/`hae-package`와 `unicode-segmentation`은 공유. **네이티브 조판 오류는 이 gate로 검출되지 않는다** |
| rhwp 결합 | 라이선스·충돌은 프로세스로 분리, drift 위험 | adapter가 `dump-pages`/`export-svg --page`/`export-tree`의 고정된 형태를 기대. **현재 binary의 expected hash/version 강제 allowlist 없음** |
| 배포·feature 분리 | 코어 배포에 렌더러 전이 의존 | `hae-exec`가 `hae-render-rhwp`를 무조건 의존, resvg optional feature 분리 없음, `publish=false`, Rust 1.93.1 exact, CI workflow 없음 |
| 시험 공백 | green의 범위를 제한해야 함 | 318개는 공개·합성 범위. native Windows/macOS·LibreOffice·공식 스키마·장기 stress·전체 RSS·외부 MCP 클라이언트·release overflow 미실시 |

### 6.3 총괄 지적 — 기존 설계의 문제

1. **"노드 불변" 가정이 확장을 막는다.** 0.1의 안전 성질 전부가 "본문 노드를 만들지 않는다"에 기대고 있다. 표 구조 편집(G2)과 문단 생성(G3)은 이 가정을 깨므로, **별도 설계 없이 기능만 추가하면 검증·Undo·버전 계약이 무너진다.** 이것이 HAE의 가장 큰 구조적 한계다.
2. **검증기가 최대 자산인데 공개되지 않았다.** 경쟁 지형에서 "한컴에서 안 깨지는가"가 경쟁 축으로 이동했는데(§2.2), HAE는 그 축에 이미 강한 자산을 갖고도 문서화·배포가 안 되어 있다.
3. **Windows 빌드 장애가 문서에 드러나지 않았다.** README·설계서는 Windows 검증을 "보류"로만 적고 있는데, 실제로는 **빌드 자체가 되지 않는다**. 보류와 불가능은 다르다.
4. **비동기 부재가 에이전트 사용성을 제한한다.** MCP 대표 시나리오는 4 MiB·60초 안에서만 검증됐다. 실제 문서가 커지면 첫 병목이 transport가 된다.


---

## 7. 대표 시나리오 비교 (D단계)

**완료(2026-10-19).** HAE `76b0f72`(D 실행 시점; 이후 브랜치는 `3bdfbb4`까지 전진) release 빌드(격리 worktree, `command.list` 57개)와 경쟁 5개(python-hwpx 6.8.0, python-hwpx-automation 7.3.4, kordoc 4.22.9, hwpx-skill, hancom-mcp)를 **같은 공개 fixture 사본·같은 요청**으로 실행했다. 상세 보고서·원시 결과: `~/.cache/cga/d/report.md`, `~/.cache/cga/d/results.json`. 아래 표는 그 결과이며 §2.1의 A1 비교를 대체한다. 판정은 독립 파서 재추출(a), ZIP 원시 바이트·순서·mimetype(b), 편집 항목 canonical diff(c), linesegarray(d), header·새 파트(e), Linux 한컴 첫 프레임(f), 시간(g)으로 확인했다.

### 7.1 결과 요약

표기: ✓ 성공(독립 재확인) · ✗ 실패/거절 · △ 부분 · — 기능 없음.

| 시나리오 | HAE | python-hwpx | automation | kordoc | hwpx-skill | hancom-mcp |
|---|---|---|---|---|---|---|
| T1 본문 문단 교체 | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ 본문 문단 64→32 삭제 |
| T2 표 셀 교체 | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ 같은 파괴 |
| T3 수식 보존 교체 | ✓ (canonical 0) | ✓ (재직렬화 771) | ✓ (683) | ✗ 무변경 | ✓ (0) | ✓ (파트 4개 추가) |
| T3 탭 보존 교체 | ✓ (탭·개행 요소 유지, 치환 텍스트 뒤로 이동) | ✗ 자체 게이트 거절 | ✗ 실행 실패 | ✗ v1 미지원 | △ 탭 요소 소실 | ✗ 무변경 |
| T3 그림 보존 교체 | ✓ (그림 1/1) | ✓ (1/1) | ✗ 치환 0건 | ✓ (1/1) | ✓ (1/1) | ✗ 그림 소실 |
| T4 전체 교체(32건) | ✓ 32/32 | ✓ 32/32 | ✓ 32/32 | △ 14건만 | ✓ 32/32 | ✗ 6/32 |
| T5 문단 서식(정렬) | ✓ | ✓ | ✓ | — | — | ✗ 서식 무변경 |
| T6 Undo | ✓ 바이트 동일 | — | ✓ 바이트 동일 | — | — | — |
| T7 새 문서 생성 | — **미지원** | ✓ | ✓ | ✓ | ✓ | ✓ |
| T8 글자 서식(굵게·크기) | ✓ charPr 추가 | ✓ | ✓ | — | — | — |
| T9 표 행 추가 | ✓ 2→3행 | ✓ | ✓ | ✗ 거절 | ✓(끝에 추가) | — |

### 7.2 보존 특성 (b)·(c)·(d) 수치

- **(b) 미편집 항목 원시 압축 바이트**: HAE·kordoc·hwpx-skill은 전부 동일. python-hwpx·automation은 페이로드는 같지만 9개 항목의 압축 스트림을 다시 쓴다. hancom-mcp는 `Scripts/headerScripts`·`Scripts/sourceScripts`를 추가해 항목 순서가 달라진다.
- **(c) 편집 항목 내 미편집 XML 영역 diff(canonical, 문자 수)**: T1에서 HAE 189·automation 7·kordoc 6·hwpx-skill 6·python-hwpx 290·hancom-mcp 10666. HAE의 잔여는 전부 stale `linesegarray` 블록 삭제 자리이며, **요청 텍스트와 캐시 삭제 외 XML 변경이 0**이다(T3 수식 canonical 0).
- **(d) linesegarray**: HAE는 편집 영향 범위의 stale 캐시를 삭제(T1 113→62, T4 113→48), kordoc·hwpx-skill은 편집 영역만 최소 변경(T4 113→77/66), hancom-mcp는 전부 삭제(113→0).
- **(f) Linux 한컴 2020 Beta 첫 프레임**: HAE 산출물 T1·T2·T3-eq·T3-tab·T3-pic·T8·T9가 정상 렌더(오류 대화상자 없음). 캡처: `~/.cache/cga/d/hancom/frames/`. Windows 한컴오피스 2024는 여전히 미확인(§11 U6).
- **(g) 시간**: 전 도구 2초 미만. T3-pic(1.5MB) HAE 0.75초·kordoc 1.54초.

### 7.3 C단계 정정과 일치 여부

D는 C-1-04(hancom-mcp 문서 파괴)를 재현했다. T1·T2에서 본문 문단 64→32, T3-pic에서 그림 소실(1→0)을 독립 파서로 확인했다. §13.1의 hancom-mcp 행을 D가 다시 확인한 것이다.

---

## 8. 경쟁 프로젝트 대비 우위·열위

근거: D단계 직접 비교(§7, `d/results.json`) + HAE 0.3 HEAD `76b0f72` 실행. 2026-10-19 갱신.

### 8.1 실증된 우위

| 항목 | HAE | 경쟁 (D 관측) |
|---|---|---|
| **미편집 무손실** | 미편집 항목 **원시 압축 바이트·순서·mimetype**까지 보존. Undo 후 내보내기 **바이트 동일**(T6, SHA-256 일치) | kordoc·hwpx-skill도 원시 바이트 보존. python-hwpx·automation은 페이로드는 같지만 9개 항목 압축 스트림 재작성. hancom-mcp는 파트 추가 |
| **편집 항목의 외과성** | 편집 XML 안에서 요청 텍스트(+stale 캐시 삭제) 외 변경 **0**(T3 수식 canonical 0; T1 잔여는 전부 캐시 삭제 자리) | python-hwpx는 XML 선언·CRLF·속성 재직렬화 흔적, hancom-mcp는 문단 대량 삭제 |
| **개체 보존 치환** | 수식·탭·그림 문단에서 개체 유지 + grapheme 범위 치환(T3 3종 전부 성공) | kordoc 탭 문단 v1 미지원·그림 문단 적응 필요, automation 그림 치환 0건, hwpx-skill 탭 소실, hancom-mcp 그림 소실 |
| **독립 저장 관문** | 편집 계층 비의존 별도 검증기(`hae-verify`)가 저장 전 참조·구조 무결성을 검사 | 공개된 것은 `dvc`(정지)·`polaris_dvc`뿐 |
| **원자 배치·dryRun·버전 충돌·프로젝트 잠금** | 구현·시험됨 | 대부분 개별 호출 수준 |
| **한컴 없이 동작** | 전 경로 (D 전 시나리오 한컴 미사용) | Windows COM 계열은 한컴 필수 |
| **0.3 신규 표면** | 글자 서식(T8)·표 행 추가(T9)·쪽 설정·필드·수식 스크립트·HWP5 읽기(`hwp5.inspect`)가 경쟁 대비 동등 이상으로 동작 | 글자 서식은 kordoc·hwpx-skill·hancom-mcp 모두 미지원. 표 행 추가는 hwpx-skill 가능(끝에만), kordoc·hancom-mcp 미지원 |

### 8.2 실증된 열위

| 항목 | HAE | 경쟁 (D 관측) |
|---|---|---|
| **새 문서 생성** | **미지원**(command.list 57개에 생성 명령 없음) | 5개 전부 성공(T7) |
| **검증기 과엄격** | 공개 코퍼스 `anchor_char_height.hwpx`를 **편집 없이도** export 거절(`unresolved styleIDRef: 0`) | kordoc·hwpx-skill·python-hwpx는 같은 파일에서 저장·패치 성공(검증: `d/outputs/probe-anchor/`) |
| **표면적·도구 수** | 명령 57개 | automation 128 · hwpx-mcp 125 · hwp-mcp 35 |
| **비동기·진행률·취소** | 없음(§6.2 유지) | 일부 서버가 스트리밍 제공 |
| **대화형 대상 지정** | CLI 스크립트로 셀·표 노드를 연쇄 지정하기 어려워 T2·T9는 MCP 경로로 수행 | 상위 도구는 인덱스 기반 도구 호출 제공 |
| **Windows** | 크로스 체크 통과(§14)이나 실제 Windows 한컴 열기는 미확인 | 대부분 크로스플랫폼 |

### 8.3 총괄 판정 — 2026-10-19 갱신

- **"HWPX를 읽고 텍스트를 바꾼다"는 여전히 차별점이 아니다.** D에서 6개 중 5개가 본문·셀·전체 치환을 성공했다(hancom-mcp만 파괴).
- **HAE의 차별점은 "바꾼 것 외에는 아무것도 바꾸지 않는다"로 좁혀졌다.** D의 canonical diff(§7.2)에서 HAE만 편집 텍스트·캐시 삭제 외 XML 변경이 0이었다. python-hwpx·automation은 재직렬화로 수백~수천 문자를 다시 쓴다.
- **0.3으로 "덜 하는 엔진" 인상은 줄었다.** 글자 서식·표 구조·필드·쪽 설정·수식 속성·HWP5 읽기가 들어와 §9의 1·3·4·5·7 순위 후보가 실행됐다. 남은 핵심 공백은 **새 문서/문단 생성(T7)** 하나로 좁혀졌다.

---

## 9. 우선순위가 매겨진 개발 후보 목록

기준: 사용자 가치 × 경쟁 격차 × HAE 기존 강점과의 정합성 ÷ (난이도 × 계약 위험).

| 순위 | 후보 | 왜 이 순서인가 | 난이도 | 버전 |
|---|---|---|---|---|
| **1** | **글자 서식(`charPr`) 생성·편집** (G1) — **완료(0.3)** | `text.format`(굵게·크기·색 등). D T8에서 header charPr 1개 추가·참조 확인 | 중 | 0.3 ✅ |
| **2** | **Windows 빌드 복구** (G19/P3) | 지금은 기능 문제가 아니라 **동작 자체가 불가**. 사용자 검증 계획 전체가 여기에 걸려 있다 | 중 | 0.3 |
| **3** | **검증기 규칙 공개화 + 편차 목록** (G9·G10) | HAE만 가진 자산. 경쟁 축이 "한컴에서 안 깨지는가"로 이동한 상황과 정확히 맞는다. 코드 변경 적음 | 중 | 0.3 |
| **4** | **P1 메모리 회계 결함 수정** | 실재하는 결함. 대규모 실행에서 예산 판정을 틀리게 만든다 | 하~중 | 0.3 |
| **5** | **누름틀 필드 값 바인딩** (G4) — **완료(0.3)** | `field.list`·`field.setText`·`form.fill`(라벨 기반 채움). 필드 경계 규칙 유지 | 중 | 0.3 ✅ |
| **6** | **비동기·취소·진행률** (G17) | 에이전트 플랫폼 방향의 전제. 없으면 큰 문서에서 transport가 먼저 막힌다 | 중 | 0.3 |
| **7** | **출력 스키마 표준화** (G20) | 저비용, 클라이언트 호환성 개선 | 하 | 0.3 |
| **8** | **표 구조 기본 편집** (G2) — **완료(0.3)** | `table.insertRow/deleteRow/insertColumn/deleteColumn`(빈 셀 복제, 병합 경계 거절, Undo). D T9 2→3행 확인. **남은 것 = 생성**(문단·개체·새 문서, G3) | 최상 | 0.3 ✅ / G3는 0.4 |
| **9** | **HWP5 읽기** (G13) — **부분 완료(0.3)** | `hwp5.inspect` 읽기 전용. 무손실 왕복·편집은 미착수 | 상 | 0.3 ✅(읽기) |
| **10** | **공공 서식 템플릿** (G16) | 실사용 가치 높음. 생성 능력에 의존 | 중 | G3 이후 |

---

## 10. 실행 가능한 단계별 로드맵

### 방향 1 — 기존 HWPX를 안전하게 수정하는 전문 엔진 (현재 축)

- **강점**: 이미 가진 것(원본 보존·검증·트랜잭션)이 그대로 자산. 계약 위험 낮음. "한컴에서 안 깨진다"를 증명하면 신뢰가 곧 경쟁력
- **약점**: 시장이 "만들 수 있는가"로 비교할 때 열위가 드러난다. §9의 1~7을 다 해도 "새 문서 생성 불가"는 남는다
- **필요 작업**: G1, G4, G7, P1, G19, G20
- **판정**: **0.3의 중심으로 적절하다.** 다만 이것만으로는 "덜 하는 엔진"으로 보일 위험이 있다

### 방향 2 — 새 HWPX를 처음부터 생성하는 범용 문서 엔진

- **강점**: 시장 요구가 명확하고 경쟁사가 이미 하고 있다. 생성이 되면 템플릿·자동화 시장이 열린다
- **약점**: **HAE의 유일한 실증 우위(깨뜨리지 않음)가 무의미해지는 축이다.** 새로 만든 문서에는 "원본"이 없다. 검증 우위 없이 생성만 하면 경쟁사와 차이가 사라진다
- **필요 작업**: G2, G3, G16 + 버전·ID 정책 재설계
- **판정**: **단독 중심축으로는 부적합.** 방향 1의 확장으로 붙여야 한다

### 방향 3 — AI 에이전트가 복잡한 문서 작업을 자율 수행하는 문서 자동화 플랫폼

- **강점**: MCP·계약·검증·dryRun·원자성은 에이전트에게 특히 중요한 성질이고 HAE는 여기 이미 강하다. 경쟁 MCP 20여 건은 검증·벤치마크가 전무하다(A2 판정). "에이전트가 안전하게 문서를 다룬다"는 축에서 기존 자산이 그대로 무기다
- **약점**: 비동기·진행률·다중 문서·협업이 없다. 사용자 접점(문서 텍스트·오류 메시지) 품질도 미완
- **필요 작업**: G17, G18(부분), G9·G10, 방향 1의 편집 폭 확장
- **판정**: **HAE의 강점과 시장 공백이 가장 잘 겹치는 방향이다**

### 총괄 권고

> **방향 3을 중심에 두고, 방향 1을 그 기반으로 삼는다. 방향 2는 방향 1·3이 무르익은 뒤 선택적으로 붙인다.**

근거:
1. HAE의 실증된 우위는 전부 **방향 3에 필요한 성질**이다(검증·원자성·미편집 무손실·한컴 독립). 방향 2로 가면 그 우위를 버리고 후발 주자와 같은 경쟁을 하게 된다.
2. 경쟁 축이 "한컴에서 안 깨지는가"로 이동했는데(§2.2), 그 축의 **공개 자산이 비어 있다**(A2 §3.3). HAE가 이미 그 자산을 갖고 있다.
3. 에이전트 도구 20여 건이 검증·벤치마크 없이 난립하는 상태는 **신뢰가 차별점이 되는 시장**이다.

### 단계별 계획

**0.3 — "에이전트가 신뢰할 수 있는 편집 엔진" (방향 1 + 3의 접점)**
1. P1 메모리 회계 수정 (C단계에서 P2 확정 후 함께)
2. Windows 빌드 복구 + 최종 사용자 검증
3. 글자 서식(`charPr`) 생성·편집
4. 누름틀 필드 값 바인딩
5. 검증기 규칙 공개화 + 공식 사양 편차 목록
6. 출력 스키마 표준화(`title`, 표준 format)
7. 비동기·취소·진행률 설계 착수

**0.4 — "구조를 다루는 엔진" (방향 2의 일부를 흡수)**
8. 표 구조 편집 — 별도 설계 단위로 분리(노드 불변 가정 해제의 파급을 설계에 명시)
9. 문단·개체 생성, 새 문서 생성
10. 구역·용지·단 설정, 머리말·꼬리말 제어
11. HWP5 읽기 — 외부 자산 활용 또는 별도 설계 판정

**0.5+ — "문서 자동화 플랫폼" (방향 3 완성)**
12. 다중 문서·동시성·장기 작업 서비스화
13. 공공 서식 템플릿 라이브러리
14. 한컴 조판 동등성 자동 판정

---

## 11. 추가 조사·실험이 필요한 미확인 사항

| # | 미확인 | 왜 중요한가 | 확인 방법 |
|---|---|---|---|
| ~~U1~~ | ~~D단계 시나리오 비교(HAE vs 경쟁)~~ | **해결(2026-10-19)**: §7에 결과. `~/.cache/cga/d/report.md`·`results.json` | 완료 — 잔여 미확인은 §7.3(f) Windows 열기(§11 U6)뿐 |
| U2 | C단계 교차검증(A1 표·B 버그 P1·P2) | A1의 표와 B의 버그는 아직 독립 재현 전 | C 결과 회수 |
| U3 | rhwp 빌드 가부 | A1은 실패, 로컬 기록은 성공 — 환경 차이 미확정 | C가 확인 중 |
| U4 | Aspose·GroupDocs·Tika·ONLYOFFICE·Synap의 HWP/HWPX 네이티브 지원 | 상용 대안의 실체가 비어 있다 | 공식 문서·체험판 |
| U5 | 한컴 SDK API 목록·라이선스·Linux 지원 | "한컴 없이"의 경계를 정하는 기준 | 한컴 디벨로퍼 포럼 1차 자료 |
| U6 | Windows 한컴 2024에서의 실제 열기·조판 | HAE 최종 검증의 마지막 조각 | 사용자 Windows |
| U7 | 공식 HWPX XSD 대비 적합성 | `hae-verify`는 자체 규칙이며 공식 스키마 전체가 아님 | XSD 확보 후 대조 |
| U8 | 512 MiB 초과 RSS stress·장기 성능 | P1과 별개로 실제 상한이 미측정 | 대용량 합성 |
| U9 | npm 패키지 다수(저장소 URL·라이선스 미확인) | 재사용 판단 불가 | 개별 확인 |
| U10 | HWP5 쓰기 실현성 | A2가 "빈 자리"로 지목 | 범위 판정 선행 |

---

## 12. 출처와 근거

### 12.1 조사 산출물 (로컬, 커밋하지 않음)

| 경로 | 내용 |
|---|---|
| `~/.cache/cga/a1/report.md`, `a1/projects.json`, `a1/outputs/`, `a1/scripts/` | 경쟁 7개 프로젝트 고정 SHA·실행 결과·probe 스크립트 |
| `~/.cache/cga/a2/report.md`, `a2/candidates.json` | 후보 97건·제외 7건·시장 지형·미확인 목록 |
| `~/.cache/cga/b/report.md`, `b/inventory.json`, `b/source/`, `b/tests-*.log` | HAE 전수 조사·버그 재현·검증 로그 |
| `~/.cache/cga/spec/report.md`, `spec/matrix.csv` | OWPML 61행 매트릭스 |
| `~/.cache/cga/c/` | 교차검증(C) |
| `~/.cache/cga/d/` | 시나리오 벤치마크(D) |
| `~/.cache/cga/PROGRESS.md` | 조사 진행 기록 |

### 12.2 HAE 코드 근거 (핵심)

| 주장 | 경로 |
|---|---|
| 메모리 회계 누락(P1) | `crates/hae-exec/src/session.rs:70,322,800,1305` |
| 커서 overflow(P2) | `crates/hae-exec/src/session.rs:657`, `crates/hae-model/src/query.rs:684` |
| 참조 무결성 검증 | `crates/hae-verify/src/lib.rs:271-297` |
| paraPr 생성·재사용 | `crates/hae-edit/src/format.rs:48-120` |
| 이미지 미디어 등록 | `crates/hae-edit/src/image.rs:135-140` |
| PrvText·썸네일 정책 | `crates/hae-edit/src/lib.rs:188-218,219` |
| 변경 추적·서명 읽기전용 | `crates/hae-model/src/lib.rs:188-202` |
| 패키지 거절 | `crates/hae-package/src/lib.rs:218-232,247-254` |
| 필드 경계 거절 | `docs/superpowers/specs/2026-10-18-hwpx-engine-0.2-design.md` §7-1 |

### 12.3 외부 공개 출처

- `hancom-io/hwpx-owpml-model` (Apache-2.0) — OWPML 공식 모델
- `hancom-io/dvc`, `PolarisOffice/polaris_dvc` (Apache-2.0) — HWPX 검증기
- `neolord0/hwpxlib`·`hwplib`·`hwp2hwpx` — Java 계보, 골든·실패 코퍼스
- `ebandal/H2Orestart` (GPL-3.0) — LibreOffice 확장
- `iyulab/unhwp` (MIT), `Indosaram/hwpers` (MIT) — Rust 계보
- 프로젝트별 release·issue 페이지 (상세 URL은 `a1/report.md`, `a2/report.md`)

### 12.4 총괄이 직접 재현한 명령

```bash
# 머리말·각주 문단 편집 → export → 재열기 (S 판정 반증)
#   text.replace on 머리말 문단 + text.replace controls=preserve on 각주 문단 + document.export
#   재열기 후 textContains 로 반영 확인 (각 1건)

# 글상자 문단 편집 (S 판정 반증)
#   research-school-plan-reference.hwpx, textContains "연구의 배경" → text.replace ok
#   text.replaceAll find=다 → 교체 136 / 문단 90 / 건너뜀 0

# equation 코퍼스 출현 0건 (S 판정 확인)
#   tests/fixtures/corpus/*.hwpx 전체 sectionN.xml 에서 <hp:equation 0건
```

---

## 부록: 조사 방법 준수 기록

- 제품 코드 수정 **없음**. 커밋·push·병합 **없음**
- 빌드·복제는 `~/.cache/cga/` 에서만 수행
- 사용자 개인 문서(`~/Documents` 하위)·비공개 SDK 자료 **미사용**
- 하위 조사 결과를 그대로 취합하지 않고, 상충 항목(머리말·각주·글상자·equation·생성 2개)을 **총괄이 직접 명령으로 재현**해 확정
- A1·A2·B·S·C·D는 서로 다른 모델이며, 교차검증 C는 수집 담당과 다른 모델이 수행

---

## 13. 정정 — C단계 교차검증 결과 (2026-10-18)

C단계(Gemini 4 Argon, 수집 담당과 다른 모델)가 A1·A2·S의 주장에 반례를 시도했고, **10건을 반증**했다. §2·§5·§12의 해당 내용보다 이 절이 우선한다. 반증되지 않은 11건은 그대로 유효하다.

### 13.1 경쟁사 평가의 정정 (심각도 순)

| # | 원래 판정 | 정정 | 근거 |
|---|---|---|---|
| **C-1-04** | `hancom-mcp` S2·S3 **성공** | **실패 — 문서 파괴.** 파서가 표 내부 문단과 같은 `hp:p@id`를 가진 본문 최상위 문단을 전부 누락하고, 저장 시 `para_map`에 없다는 이유로 `root.remove(el)`로 삭제한다. 원본 401,319B(본문 문단 147개) → 저장 후 203,974B(텍스트 보유 본문 문단 **0개**). A1이 S2 성공으로 센 대상은 본문 문단이 아니라 표 셀 내부 텍스트였다 | `reader.py:67-80`, `writer.py:98-115` |
| **C-1-02** | `hwp-mcp` S4는 "readback이 잘못됐다" | **생성기가 잘못된 OWPML을 만든다.** 생성한 `<hp:tc>`마다 필수 자식(`hp:cellAddr`/`hp:cellSpan`/`hp:cellSz`)이 빠져 모든 셀 `colAddr`가 0으로 겹치고 각 행 1열이 0열을 덮어쓴다 | `hwp-mcp src/core/hwpx-mutate.ts:674-684` |
| **C-2-01** | `rhwp`는 **빌드 불가**, S1~S5 전부 불가 | **반증.** 동일 커밋이 `--locked --offline --no-default-features --bin rhwp`로 빌드되고 S1~S3이 동작한다. A1이 실패한 원인은 A1 샌드박스의 자체 실수 3건이다: (1) `/etc/ca-certificates` 미바인드로 TLS 심볼릭 링크가 깨짐, (2) 호스트에 이미 있던 `~/.cargo/registry` 캐시를 빈 `CARGO_HOME`으로 덮어씀, (3) sparse-checkout이 `include_bytes!` 대상 디렉터리를 누락 | C가 동일 커밋을 직접 빌드·실행 |
| **C-1-05** | `kordoc` S2·S3에 "블록 추가 미지원" 잔차 | **A1 스크립트의 실수.** MCP 도구가 출력 첫 줄에 붙이는 메타 배너를 지우지 않고 편집 입력으로 넘겼다. 배너를 제거하면 잔차 0건으로 성공 | `kordoc_scenarios.mjs:29-30` |
| **C-3-01** | S5 판정, "`hwp-mcp`와 `hancom-mcp`가 `Scripts/*` 추가" | **부분 오류.** `Scripts/*`를 추가하는 것은 `hancom-mcp`뿐이고 `hwp-mcp`는 `Contents/`만 추가한다. 또한 S5가 `section0.xml` 외 13개 멤버만 비교하므로, `hancom-mcp`처럼 **본문 문단을 삭제한 경우까지 `kordoc`·`hwpx-skill`(압축 스트림 원시 보존)과 같은 "성공"으로 판정**됐다 | `a1/outputs/hwp-mcp/s2-paragraph.hwpx` zip 비교 |
| **C-6-01** | A2 후보 97건 | **수량 부풀림.** `NPOI.HWPF`(MS Word 97 `.doc`)는 A2 스스로 "HWP와 무관"이라 적었는데 포함됐고, PyPI `hwpx`는 `pypa/sampleproject` 더미, GitHub 검색 URL 묶음 2건이 개별 프로젝트로 계산됐으며 `polka306/jw-hwp-mcp`는 이중 계상, `taunio/unfair-office`는 404다. 보고서 내 64건/60건 수치도 서로 모순 | `a2/candidates.json` |
| **C-6-02** | 상위 15건 중 4개 설명 | **오류.** `polaris_dvc`에 README·docs 8건·`testdata/golden/` 44건이 실제로 있고, `hancom-io/dvc`에 Apache-2.0 LICENSE·빌드 방법·`jsonFullSpec.json` 규칙 스펙이 있으며, `HwpForge`는 5개가 아니라 **11개 crate**(HWP5 읽기·변환, PDF 렌더링 포함)이고, `hwpers`는 `HwpWriter`로 **HWP5 쓰기를 명시**한다 | C가 해당 저장소를 클론해 확인 |

**가장 중요한 정정**: `hancom-mcp`는 S2·S3에서 "성공"이 아니라 **문서를 파괴**한다. 그리고 그 사실이 A1의 S5 기준으로는 걸러지지 않았다. 경쟁 비교표(§2.1)에서 `hancom-mcp` 행은 아래처럼 읽어야 한다.

| 프로젝트 | S1 | S2 | S3 | S4 | S5 |
|---|---|---|---|---|---|
| hancom-mcp | 성공(구조 문단 0) | **실패(본문 147문단 삭제)** | **실패(같은 원인)** | 성공 | payload만 동일 |

### 13.2 S 매트릭스 정정

| # | 원래 판정 | 정정 |
|---|---|---|
| **C-4-01** | `run`이 "run 분할 및 신규 생성"을 수행 | **허위**(기준 `1cf8f5c`). `text.replace`는 run을 쪼개거나 만들지 않는다. 첫 조각에 새 텍스트를 넣고 나머지를 비우며, 서로 다른 `charPrIDRef`가 섞이면 `FORMAT_COLLAPSED`를 낸다. **단 `text.format`(커밋 `c485ffb`, C 조사 이후 추가)은 범위 적용을 위해 run을 분할한다.** 두 명령의 동작이 다르다 |
| **C-4-02** | `t`가 "빈 t 정리(제거)"를 수행 | **반대.** 두 번째 이후 `<hp:t>`의 내부 텍스트 범위만 비우고 **엘리먼트는 삭제하지 않는다** |
| **C-4-05** | `charPr`의 "런 분할 시 IDRef 보존" | **허위**(같은 기준). 인용한 행 번호도 실제 파일과 어긋난다. `spec/matrix.csv`의 `hae_evidence` 행 번호 다수가 실제와 불일치하므로 **행 번호를 근거로 쓰지 말 것** |

### 13.3 HAE 신규 결함 (C가 발견, 총괄이 수정)

| # | 결함 | 상태 |
|---|---|---|
| **C-new-1** | `document.inspect`의 `cursor + limit`이 release에서 wrap. `cursor="0:18446744073709551615"`이면 에러 대신 **커서를 문서 앞쪽으로 되감는 정상 응답**을 반환해, 커서를 따라가는 클라이언트가 무한 루프에 빠진다(B는 debug 패닉만 확인했다) | **수정 완료** `8b42e2e`. release 바이너리로 재확인: 빈 페이지 + `nextCursor: null` |
| **C-new-2** | `verify_steps`가 매 편집마다 전 이력을 복제(O(N²))하는데 소비자는 `last()` 하나뿐 — 데드 스냅샷 | **수정 완료** `8b42e2e`. 최신 1개만 보관, 관문 동작 불변, 회귀 시험으로 "회계에 포함 + 상한 유지" 고정 |

### 13.4 아직 반영하지 않은 것

- ~~**D단계(대상 시나리오 벤치마크)**는 Codex 한도로 중단됐다. §7은 여전히 비어 있으며…~~ → **2026-10-19 재개·완료**: §7에 결과 표·수치, 원 자료 `~/.cache/cga/d/report.md`·`results.json`. D는 C-1-04(hancom-mcp 문서 파괴)를 독립 파서로 재확인했다(§7.3).
- A1·A2 원문 보고서는 정정 없이 캐시에 남아 있다(`~/.cache/cga/a1|a2/`). 인용할 때는 이 §13을 함께 읽어야 한다.

---

## 14. 정정 — 기능 카탈로그(2026-10-18)가 지적한 낡은 수치

`docs/research/hwpx-feature-catalog.md`가 이 보고서의 일부 수치가 낡았음을 확인했다.

| 이 보고서의 기록 | 현재 | 비고 |
|---|---|---|
| "22개 명령" | **HEAD 23개**, 0.3 작업 트리 26개 | `text.format` 추가 반영 |
| "Windows 빌드 장애" | **`io/windows.rs` 존재, 크로스 체크 통과** | `1ae63ee`·`bd80331`. 단 Windows 미리보기는 fail closed |
| "OWPML 모델 = 정본" | **2023 스냅샷**이며 코퍼스에 모델에 없는 요소 이름이 있다 | 정본으로 과신하지 말 것 |

카탈로그의 추가 발견: `Dayoooun/hwpx-mcp`의 도구 수 주장이 README 77개·소개 125개·표 집계 81개로 서로 맞지 않는다(대외 주장을 검증 없이 인용하면 안 된다는 사례).

카탈로그가 제시한 우선순위 상위 10개 중 **아직 착수하지 않은 것**: 하이퍼링크 목록(FD-04), 변경 추적 마크 목록(FD-15), 라벨 기반 양식 채우기(TB-07), Markdown·HTML 내보내기(MT-05).
