# 독립 HWPX 문서 엔진 0.1 구현 계획

작성: 2026-10-16. 개정 1: 2026-10-16(GPT 6.1 Sol·Sonnet 5.5 검토 반영, 반영 기록은 docs/decisions/plan-review/반영기록.md). 상태: **검토 반영본, 사용자 확인 전**.

기준 문서: docs/superpowers/specs/2026-10-16-hwpx-engine-0.1-design.md(이하 설계서), docs/decisions/stage0-results.md, docs/decisions/model-roles.md.

이 계획은 설계서 §14의 단계 1(코어)을 작업 단위까지 나눈다. 단계 2·3·4는 범위·순서와 넘겨받을 계약 목록만 정하고, 세부 계획은 단계 1이 끝난 뒤 쓴다.

## 1. 전제

- S1·S2·S3 통과. §11 캐시 규칙(수정 최상위 문단부터 구역 끝까지)과 §7 쪽 정보 범위를 개정했다.
- S4(한컴 재열기)는 사용자 확인 대기다. 결과 전에도 단계 1을 시작하되 캐시 처리는 `LinesegPolicy` 한 곳에 모아 바꿀 수 있게 한다(작업 1-9).
- 단계 1의 완료 기준은 설계서 §14 단계 1 목록이다: 무편집 저장, 부분 편집, 해석 불가 요소, ID 지속, 프로젝트 잠금, dryRun, batch 실패, Undo/Redo, 실행기 수준의 요청 순서 규칙.
- 단계 1에서 document.preview는 RENDERER_UNAVAILABLE을 돌려준다. 미리보기를 포함한 대표 시나리오 전체는 단계 4에서 완료한다.

## 2. 툴체인과 의존성

- Rust 1.93.1을 rust-toolchain.toml로 고정한다. 작업 1-1에서 설치한다. rhwp도 단계 3 시작 때 1.93.1로 다시 빌드해 S2·S3 결과를 재확인한다.
- 직접 의존성은 `=` 판과 사용할 feature까지 총괄이 정하고 Cargo.lock을 커밋한다. 고정 전에 RustSec 공지를 확인해 결과를 docs/decisions/dependencies.md에 기록한다(총괄).

| 용도 | crate | 고정 후보 | 비고 |
|---|---|---|---|
| XML 토큰 | quick-xml | 0.39.2 | S1에서 검증한 판. Reader는 namespace를 해석하지 않으므로 namespace는 직접 해석한다(작업 1-4) |
| 압축 | flate2, crc32fast | 1.1.9, 1.5.0 | ZIP 구조 처리는 직접 구현(작업 1-3) |
| 자소 군집 | unicode-segmentation | 1.13.3 | 판과 Unicode 데이터 판을 실행 메타데이터에 기록 |
| 직렬화·스키마 | serde, serde_json, schemars | 1.0.229, 1.0.151, 1.2.2 | 명령 타입에서 JSON Schema 생성 |
| 해시 | sha2 | 0.10.9 | |
| 난수 ID | getrandom | 0.3.4 | |
| 그림 검사 | png, zune-jpeg | 0.18.1, 0.5.15 | 할당 전 크기 검사, 다시 인코딩하지 않음 |
| 파일 I/O | rustix | 1.1.4 (fs feature) | openat·O_NOFOLLOW·flock·fsync·renameat_with(NOREPLACE) |
| 오류 | thiserror | 2.0.20 | |
| CLI | clap | 4.6.6 | |

ZIP crate를 쓰지 않는 이유: 원시 압축 데이터 복사는 zip crate도 제공하지만, 우리는 항목 순서·압축 방식에 더해 로컬 헤더의 이름·extra 필드·플래그를 바이트 그대로 보존해야 한다. 이 범위를 zip crate의 쓰기 API로 보장하는지 확인하지 못했으므로 직접 구현하되, 읽기 쪽은 아래 1-3의 방어 조건을 완료 기준으로 둔다.

## 3. 저장소 구성과 의존 방향

```text
crates/
  hae-diag      진단·오류 코드, 진단 구조체, 한도 상수(Limits), 자원 예산 타입
  hae-zip       ZIP 읽기(방어적 파싱·한도)·원시 복사 쓰기
  hae-xml       원문 보존 XML 토큰 표현, namespace 해석, 텍스트 대응표, 범위 교체
  hae-package   HWPX 패키지 관계: mimetype·content.hpf manifest·spine·META-INF·미디어 참조
  hae-model     정규 상태, nodeId 대응표, 요소 인덱스, 편집 허용 조건, 조회 뷰
  hae-edit      네 가지 편집, LinesegPolicy, PrvText 갱신
  hae-verify    저장 관문 검사기(편집 경로와 독립)
  hae-project   입출력 루트, 링크 비추종 I/O, 잠금, 원자적 저장, .hwpwork, 내보내기
  hae-exec      레지스트리·스키마, 세션 실행기, revision·요청 순서, batch·dryRun, 이력, 자원 예산
  hae-cli       CLI 스크립트 모드(바이너리 hae)
tests/
  fixtures/     출처·라이선스 manifest가 있는 코퍼스와 합성·악성 자료
  oracle/       독립 기대 변경 계산과 변조(mutation) 시험(Python 표준 라이브러리)
```

Cargo 의존(화살표는 "아래 crate를 사용"):

```text
hae-cli → hae-exec → hae-project → hae-verify → hae-package → hae-xml, hae-zip → hae-diag
                  ↘ hae-edit → hae-model → hae-package
hae-project → hae-model (대응표 직렬화)
```

- 실행기(hae-exec)가 세션과 프로젝트 바인딩을 소유하고, 프로젝트 열기·저장·내보내기도 같은 레지스트리 명령으로 실행한다. hae-project는 실행기를 모르는 하위 계층이다.
- **hae-verify는 hae-model·hae-edit에 의존하지 않는다.** 원본 패키지와 요청 목록에서 기대 변경을 따로 계산한다. 공유는 hae-zip·hae-xml·hae-package의 읽기 기능까지만 허용한다.
- 핵심 crate는 MCP·rhwp·GUI에 의존하지 않는다. 단계 2에서 hae-jsonl·hae-mcp, 단계 3에서 hae-render-rhwp를 hae-exec 위에 추가한다.
- in-process 클라이언트 계약은 hae-exec의 공개 API다. CLI·JSON-lines·MCP·GUI는 모두 이 API만 호출한다.

## 4. 작업 원칙

- 담당 기준: 정규 상태·원본 보존 XML 편집·트랜잭션·저장 관문·잠금은 총괄(Opus 5.5)이 직접 구현한다. Sonnet 5.5는 그 API를 사용하는 기능 구현과 시험 자료를 맡는다. DeepSeek은 총괄이 확정한 시그니처와 시험이 있는 반복 코드만 맡는다. GPT 6.1 Sol은 독립 검토와 독립 시험 oracle을 맡는다.
- 하위 모델에는 대상 파일·함수 시그니처·통과 시험을 함께 주고, 응답 모델명·작업 범위를 작업 기록에 남긴다. DeepSeek은 T3 opencode 공급자로만 부른다(model-roles.md).
- 각 작업은 "구현 → 담당 시험 통과 → 검토"로 끝낸다. 편집 작업(1-8~1-12)은 hae-verify 관문과 oracle 변조 시험을 통과해야 끝난다.
- 기준 바이트가 필요한 시험은 Git에 들어 있는 코퍼스에서 다시 만들고 hash를 시험에 고정한다. spikes/work(Git 제외)를 직접 참조하지 않는다.

## 5. 단계 1 작업

| # | 작업 | 담당 | 검토 | 완료 조건 |
|---|---|---|---|---|
| 1-1 | 저장소 골격: workspace, rust-toolchain.toml(1.93.1 설치 포함), 총괄이 정한 의존성 판·feature, fmt·clippy·test 스크립트, 각 의존성의 최소 사용 예제 빌드 | DeepSeek(파일 생성), 총괄(판 결정·RustSec 기록) | 총괄 | 1.93.1에서 빌드·시험 통과, dependencies.md 작성 |
| 1-2a | 시험 자료 1차: 코퍼스 manifest(출처·라이선스·관찰 자료 표시), 악성 ZIP·XML 합성기(잘린 입력, 겹친 구간, EOCD 위장, 데이터 디스크립터, CRC·길이 불일치, 중복·대소문자·경로 이탈, 암호화 플래그, ZIP64, 비UTF-8 이름, DTD·외부 실체, 금지 문자, 깊이 초과, prefix만 같은 다른 namespace) | Sonnet | GPT 6.1 Sol | 각 자료에 기대 결과(거절 사유 또는 통과)를 표로 고정 |
| 1-3 | hae-zip: 중앙·로컬 헤더 일관성(이름·플래그·방식·크기), checked 산술·범위·비중첩 검사, CRC·실제 해제 길이 확인, 다중 디스크·미지원 방식·ZIP64 거절, 데이터 디스크립터 수용 정책, 항목 2,048·해제량 한도, 원시 복사 쓰기 | 총괄 | GPT 6.1 Sol | 1-2a ZIP 자료 전부 기대대로 PACKAGE_REJECTED(사유 포함). 코퍼스 무편집 재기록 시 바뀌지 않은 항목의 로컬 헤더·압축 바이트 동일. 결정적 변이 자료(시드 고정) 1만 건에서 패닉 없음 |
| 1-4 | hae-xml: 원문 토큰 범위, UTF-8·BOM·선언·줄바꿈 유지, DTD·외부 실체·금지 문자 거절, 깊이 128, namespace URI 해석(prefix 재바인딩 포함), 문자 참조·CDATA 해석 텍스트와 원문 바이트의 대응표, 범위 교체와 위치 재연결, 이스케이프 | 총괄 | GPT 6.1 Sol | 1-2a XML 자료 기대대로. S1 18+3 사례를 제품 코드로 재현해 범위 밖 바이트 동일 |
| 1-5 | hae-diag: 설계서 §4~§11의 진단 코드 목록표(코드·절·단계)와 enum·직렬화 구조체, 응답 필드(unalignedMatches, historyEvicted, controlChanged 등)는 구조체 쪽, Limits 상수(§11 값 전부), 자원 예산 타입 | 목록표·구조체 명세는 총괄, 코드 생성은 DeepSeek | Sonnet | 목록표와 enum이 1:1, serde 왕복 시험 |
| 1-6 | hae-package: mimetype 값·위치·무압축, content.hpf manifest·spine·META-INF 참조, 중복 manifest ID, 누락 구역, dangling 참조, 쓰지 않는 미디어 보존, spine 순서의 구역 번호 | 총괄 | Sonnet | dangling·중복 ID·순서가 다른 spine·미사용 미디어 자료를 기대대로 처리 |
| 1-7 | hae-model: 정규 상태와 파생물, documentId·nodeId 부여, 구조 경로 키(§5), spine 순서·문단 안 컨트롤 순번·셀 주소(단계 3 조인용), 편집 허용 조건(서명·변경 추적·해석 불가 요소의 ID 참조, 판정 불가는 편집 거절), commit 관문 `validate_candidate_state`(STATE_INVARIANT_ERROR) | 총괄 | GPT 6.1 Sol | 두 번 열면 키 동일. 옛 토큰 위치·틀린 경로·끊긴 resource 참조를 주입하면 거절하고 상태 불변. 서명·추적 문서는 조회·무편집 내보내기만 가능 |
| 1-2b | 시험 자료 2차: 편집 경계 자료(다중 구역, 분할 run, run 경계의 자모·결합 문자·ZWJ 이모지, NFC/NFD, 문자 참조 안의 글자, 표·병합 셀·중첩 표, 공유 미디어 그림, 자르기·효과 그림, 수식, 해석 불가 요소, 거대 IHDR/SOF, APNG, 잘린 이미지). 편집 시작 전에 입력·기대 변경·거절 코드를 표로 확정 | Sonnet | 총괄 | 1-8~1-12 시작 전 확정 |
| 1-8 | 조회: inspect(커서, 기준 revision), element.query(코드포인트 검색, 자소 경계 밖 일치 제외와 unalignedMatches, 조상 범위, 표 셀 (표, 행, 열)·병합 셀 보고, 모호하면 전부 반환), element.get. 자소 ↔ 코드포인트 ↔ XML 바이트 대응은 hae-model 공용 함수 | Sonnet | 총괄 | 1-2b 조회 자료 기대대로, 커서 사용 중 revision이 바뀌면 명시 오류 |
| 1-9 | LinesegPolicy·PrvText: 설계서 §11의 명령별 기준 문단 규칙, 구역 끝까지 제거, 더 깊은 캐시는 해당 표 수정 때만, 구역 배치 낡음 표시, PrvText 갱신, 썸네일 낡음 진단(manifest 불변), 캐시 변경을 본문 변경과 분리 기록, 여러 편집의 합집합 | 총괄 | Sonnet | 같은 입력·문단에서 구역 XML이 단계 0 변형 E와 같은 바이트(코퍼스에서 재생성, hash 고정), 그 밖에 바뀐 항목은 PrvText뿐 |
| 1-10 | text.replace: 설계서 §6 규칙 표(0길이 범위 거절, 빈 문자열 삭제 허용, 삽입 불허, 탭·줄바꿈 거절, INVALID_RANGE, RANGE_CROSSES_CONTROL, UNSUPPORTED_CONTENT_IN_RANGE, 첫 글자 run 서식, FORMAT_COLLAPSED, 빈 run·텍스트 요소 유지) | 총괄 | Sonnet | 규칙 표 전 항목 시험, 전부 삭제 뒤 nodeId·구조 경로 유지, 범위 밖 run 바이트 동일 |
| 1-11 | paragraph.format·cell.setText: hae-xml·hae-model API만 사용. 문단 모양 재사용·복제(해석 불가 속성·자식 보존, 보장 불가면 거절), resourceChanges. 셀은 병합·중첩·다문단·run 없음 거절, 빈 셀 첫 run | Sonnet | 총괄 | 설계서 §6 규칙 시험, 다른 문단·스타일 불변 |
| 1-12 | image.replace: 입출력 루트 안의 파일만, 검증 때 한 번 읽어 바이트 고정, 할당 전 크기 검사·4,096만 화소, PNG/JPEG 정적만(APNG·다중 프레임·미지원 형식 거절, 첫 그림 자동 선택 없음), 자르기·효과 거절, 새 미디어 항목·manifest 참조, 공유 미디어 보존, 위치·크기·캡션 보존 | Sonnet | GPT 6.1 Sol | 1-2b 이미지 자료 기대대로, 검증 뒤 원본 이미지 파일을 바꿔도 결과 동일 |
| 1-13 | hae-verify 저장 관문: 원본 패키지와 요청 목록만으로 허용 변경 집합을 계산하고, 임시 출력을 다시 읽어 범위 밖 텍스트·서식·공유 미디어·해석 불가 요소(원문 범위·hash)·누락 항목을 대조. hae-model·hae-edit 비의존 | 총괄 | GPT 6.1 Sol | oracle 변조 시험(1-14) 전부 검출 |
| 1-14 | 독립 oracle과 변조 시험: Python 표준 라이브러리로 저장 파일에서 텍스트·비대상 서식·공유 미디어·불투명 속성·항목 누락·참조를 각각 변조한 사례를 만들고, hae-verify와 별도로 기대 변경을 계산 | GPT 6.1 Sol | 총괄 | 변조 사례 전부 hae-verify가 거절, oracle과 엔진 결과 일치 |
| 1-15 | hae-project: 입출력 루트(모든 명령 공통, 원본·이미지 덮어쓰기 금지), 링크 비추종 디렉터리 핸들 I/O·경로 별칭·하드 링크 검사, 경로별 고정 .lock(비차단 flock, 무결성 검사 전 획득, 해제 때 파일 삭제 안 함, 첫 저장은 lock 획득 뒤 no-replace 공개), 저장 순서(임시 파일 → hae-verify 관문 → fsync → 공개 → 부모 fsync, SAVE_DURABILITY_UNCERTAIN), EXTERNAL_MODIFICATION, .hwpwork 형식(manifest, payloadSetSha256·currentFileSha256·대응표 hash·revision), PROJECT_INTEGRITY_ERROR, 무편집 내보내기는 payloadSetSha256 일치 판정 | 총괄 | GPT 6.1 Sol | 교체 뒤 재열기·동시 첫 저장·손상 프로젝트·프로세스 종료·.lock 삭제 우회·대상 동시 생성·링크 교체 반례, 쓰기·fsync·rename·부모 fsync 오류 주입, manifest·대응표·중복 nodeId·경로 종류 변조, 편집→Undo→내보내기의 원본 전체 바이트 동일 |
| 1-16 | hae-exec 실행기: 요청 봉투(schemaVersion·SESSION_MISMATCH), revision·noop(버전 검사 먼저), writerId·requestSequence(replay·REQUEST_ID_CONFLICT·REPLAY_TOO_OLD·REQUEST_OUT_OF_ORDER·UNKNOWN_WRITER), batch(임시 상태 순차 적용, 저장·중첩·단계별 expectedVersion 거절, 실패 단계 보고, 원상복구 batch는 noop), dryRun, 이력(항목 공유 스냅샷, 100단위·historyEvicted, history.clear·snapshot.release·SNAPSHOT_UNAVAILABLE·HISTORY_EMPTY, Redo는 저장된 이후 상태 복원), 자원 예산(공유 버퍼 한 번만 계산, 할당 전 검사, 512MiB, 누적 해제량 128MiB에 프로젝트 내부 포함), 세션·프로젝트 바인딩 | 총괄 | GPT 6.1 Sol | 각 규칙별 시험. 이미지 교체 뒤 Undo에서 새 미디어 항목 제거, 원본 이미지 변조 뒤 Redo 동일, 101번째 commit에서 정확히 한 단위 해제, 이력·스냅샷 공존 큰 문서에서 실측 메모리 상한 확인 |
| 1-17 | 레지스트리: 명령 타입 단일 출처에서 입력·출력 JSON Schema, command.list/describe, CLI 인자 정의 생성, 정의되지 않은 속성 거절, 명령별 조회/변경·능력·이력 정책, document.open/inspect/close·element.query/get·project.save·document.export·document.preview(단계 1은 RENDERER_UNAVAILABLE) 연결 | 총괄(구조), Sonnet(명령 연결) | GPT 6.1 Sol | 모든 명령의 스키마·런타임 검증 일치 시험, 미등록 명령 문자열 거절 |
| 1-18 | hae-cli 스크립트 모드: 1-16·1-17의 확정 API만 호출(세션·봉투·순서 번호는 실행기 API가 채움), JSON stdout·로그 stderr, 일회성 호출 사이 Undo/Redo는 SESSION_REQUIRED | DeepSeek | Sonnet | 대표 시나리오(미리보기 제외)를 스크립트 하나로 실행 |
| 1-19 | 클린룸·재사용 기록: 설계서 §12의 인계 기록 양식(명세 ID·작성 역할·검토 역할·근거 유형·첨부 hash), 재사용 코드 이력, 시험 자료 출처 | 총괄 | — | docs/decisions/clean-room.md와 양식 |
| 1-20 | 단계 1 완료 검증: §1의 단계 1 완료 기준 전체 실행과 기록 | 총괄 | GPT 6.1 Sol | docs/decisions/stage1-results.md |

순서:

```text
1-1 → 1-2a, 1-5, 1-19
1-2a, 1-5 → 1-3, 1-4 → 1-6 → 1-7 → 1-2b
1-2b → 1-8, 1-9, 1-13, 1-14 (병렬)
1-9 → 1-10, 1-11, 1-12 (병렬, 각 작업은 1-13·1-14 통과 필요)
1-7, 1-13 → 1-15 → 1-16 → 1-17 → 1-18 → 1-20
```

## 6. 이후 단계에 넘길 계약 목록

단계 2·3·4의 세부 계획을 승인하기 전에 아래 항목이 작업과 완료 조건에 들어 있는지 대조한다.

- 단계 2 인터페이스: JSON-lines(요청 순서는 클라이언트)·stdio MCP(명령마다 도구, 순서 번호는 어댑터), 요청 1MiB·응답 4MiB, 모든 인터페이스의 같은 입출력 루트, 전송 계층의 경쟁·재전송, 인터페이스 동등성 시험.
- 단계 3 프리뷰: rhwp 1.93.1 재빌드, bwrap 격리 제품화, 시간 상한 60초·출력 크기 제한·취소·종료, 매니페스트에 적힌 /out 안 파일만 읽기, SVG 스크립트·외부 참조 검사, revision 고정과 layoutRevision, 폰트 목록·hash, coverage와 진단, 100쪽 partial, PNG 래스터화 라이브러리 고정, 쪽 정보 조인(설계서 §7), 실패해도 문서 불변·명시 오류, 장애 시험.
- 단계 4 평가: 사용자가 Windows 한컴에서 만든 최소 문서(관찰 시험 자료로 표시)로 대표 시나리오 전체 실행, S4 결과와 한컴 재저장 파일 비교, nativeComparison 기록. Windows 관찰 인계는 1-19 양식을 쓴다.
- 0.1 완료 표시는 실제 rhwp 프리뷰와 격리 검증 뒤에만 한다(설계서 §13).

2026-10-17 결정: PNG 프리뷰는 별도 샌드박스 `hae-raster` 작업자에서만 생성하며 호스트 래스터화 fallback은 두지 않는다. 신뢰할 수 없는 SVG·글꼴 처리를 격리하고 프로세스별 메모리 제한을 적용하기 위한 선택이다. 비용은 약 51 MB의 별도 바이너리와 쪽마다 추가되는 작업자 프로세스다.

## 7. 사용자에게 필요한 것

- S4 확인(시험 묶음 spikes/work/S4_한컴재열기_시험묶음.zip).
- 같은 Windows 부팅에서 가능하면 한컴으로 최소 시험 문서 몇 개를 만들어 주기(점검표 부록). 단계 1 시험 자료(1-2b)에 관찰 자료로 넣는다.
