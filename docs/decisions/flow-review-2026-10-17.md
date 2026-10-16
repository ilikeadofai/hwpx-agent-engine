# 전체 흐름 검토 (2026-10-17)

검토자: Claude Opus 5.5(계획·검토 전용 역할). 기준: 브랜치 wip/stage1-model-edit `85862d1`(작업 트리 깨끗, main보다 4커밋 앞섬), docs/decisions/의 단계별 결과와 검토 기록, spikes/work/windows-handoff-20261017. 구현이나 시험 재실행은 하지 않았고, 기록끼리의 정합성과 남은 위험을 본다.

## 1. 흐름 요약

| 단계 | 상태 | 근거 |
|---|---|---|
| 설계 0.1 (3차 개정) | 확정 | specs/2026-10-16-hwpx-engine-0.1-design.md |
| 단계 0 S1~S3 | 통과. S2·S3는 Rust 1.93.1 재빌드로 재현 | stage0-results.md, stage3-rhwp-rebuild.md |
| 단계 0 S4 (한컴 재열기) | **미실행** | Windows 인계 묶음 §2 |
| 구현 계획 | 검토 2회 반영 | plans/, plan-review/ |
| 단계 1 코어 | 완료 | stage1-results.md |
| 단계 2 인터페이스 | 완료. Sol 판정 "예" | code-review/gpt-6.1-sol-stage2-3.md, haiku-stage2-blackbox.md |
| 단계 3 프리뷰 | 구현·보강 완료. **시각 판정 UNVERIFIED** | stage234-linux-results.md, stage3-visual-check.md |
| 단계 4 (Linux 범위) | 실제 stdio MCP 대표 시나리오 86/86 통과 | stage234-linux-results.md, haiku-final-linux-gate.md |
| 단계 4 (한컴 비교) | **not_checked** | Windows 인계 묶음 §3·§4 |

최종 관문: check.sh 종료 0, Rust 243개, 사용량 도우미 30개, oracle 88개, 편집 자료 결정성 확인. Linux에서 기계적으로 확인할 수 있는 범위는 사실상 끝났다. 남은 Linux 쪽 공백은 아래 2-1의 시각 판정 하나다.

## 2. 발견한 문제

### 2-1. 시각 판정이 전달 경로 문제로 비어 있다 (높음)
Gemini가 PNG를 파일 읽기 도구로 열어 픽셀이 아닌 바이너리 텍스트를 받았다. 모델 능력 문제가 아니라 전달 방식 문제다. 이미지를 자식 스레드의 첨부(T3 attachment 업로드 → 메시지 첨부)로 넘기면 해결될 가능성이 크다. 이 판정이 없으면 "프리뷰가 사람이 볼 수 있는 결과를 낸다"는 단계 3 완료 조건 하나가 근거 없이 남는다.
- 조치: Sol이 Gemini 3.8 Flash(High)에 before/after/restored PNG를 **첨부로** 전달해 재시도. 실패하면 사용자가 직접 두 장을 보고 판정하도록 Windows 인계 묶음과 함께 요청.

### 2-2. 단계 3 판정 문구가 문서마다 다르다 (중간)
Sol 검토 문서는 "단계 3 완료 조건 충족: 보강 검증 중"에서 멈춰 있고, 통합 결과 문서는 최종 관문 통과를 기록한다. 이후 R1~R9 수정이 반영됐으므로 Sol 검토 문서에 최종 판정 줄(예/아니오와 남은 조건)을 추가해 한 곳에서 결론이 읽히게 해야 한다.

### 2-3. 결정이 필요한 열린 항목 (중간)
1. 렌더러 배치 경고·SVG_UNSAFE·FONT_UNAVAILABLE 등을 hae-diag 코드 목록에 넣지 않고 `metadata.diagnostics`의 문자열 코드로 둔 것. 규범 목록(diagnostic-codes.md)에 "렌더러 진단은 문자열 코드, 목록 밖"이라는 규칙을 명시하거나 코드로 승격해야 한다. 지금은 구현자 메모에만 있다.
2. hae-mcp가 스키마에서 documentId를 required로 선언하면서 생략을 허용해 열린 문서로 채우는 동작. 안전성은 블랙박스에서 확인됐지만 스키마와 동작이 다르다. 스키마를 optional로 바꾸거나 생략을 거절하도록 하나로 맞춰야 한다.
3. 레지스트리의 document.preview 서술(stage1 시절 표기)이 현재 동작과 맞는지 확인.
4. stdio 요청별 타임아웃 없음(Sol 검토 단서). 0.1 범위 밖으로 둘지 명시.

### 2-4. 범위와 유지 부담 (낮음, 기록 권고)
단계 3이 계획보다 크게 늘었다(래스터 작업자 프로세스, SVG 어휘 제한, 폰트 스냅샷, 51MB hae-raster). 보안상 이유가 있으나 0.1 계획 문서에는 이 구조가 없다. 계획 §6이나 별도 결정 문서에 "PNG는 별도 격리 작업자로만 만든다"는 결정과 이유를 남겨 이후 판단 근거로 삼는다.

### 2-5. 운영 흐름 (중간)
하루 사이 총괄이 Opus → Sol → Opus → Sol로 네 번 바뀌고 역할표가 다섯 번 개정됐다. 공급자 불안정(T3 parent_not_active, Codex app-server 종료, Claude 세션 한도, Fable 크레딧, ocx 경로 오작동)으로 중단·재시도·대체 작업이 반복됐고, 기록이 progress.md, model-roles.md 이전 기록, 각 결과 문서에 흩어졌다.
- 조치: 현재 역할표(2026-10-17 판)를 고정하고, 이후 변경은 역할표 하단에 날짜별 한 줄로만 추가. 작업 기록은 .superpowers/sdd/…/progress.md 하나로 모은다.

### 2-6. Windows 인계 (중간)
- 묶음이 두 개다(S4_한컴재열기_시험묶음.zip, Windows_검증_인계_20261017.zip). 옛 묶음의 점검표에는 "A가 실제 엔진 방식"이라는 낡은 문구가 있다. 새 묶음만 쓰도록 옛 묶음 이름에 `_구판`을 붙이거나 안내에 명시한다.
- 새 안내서 §6-6의 "파일 접근/모듈 로드/호출 전후 추적"은 SDK 약관(역공학·분석 금지 조항 여부)을 다운로드 시점에 먼저 확인하는 조건을 맨 앞에 두어야 한다. 클린룸 원칙상 관찰은 공개 API 입출력 비교로 한정하고, 추적은 약관이 허용할 때만 한다.
- 프리뷰는 NanumGothic 하나로 만들었다. 한컴 기본 글꼴(함초롬 계열)이 없으므로 Linux PNG와 한컴 화면의 차이 상당 부분은 글꼴 차이다. Windows 결과를 비교할 때 이 점을 먼저 분리해야 한다.

### 2-7. 저장소 (낮음)
작업 브랜치가 main보다 4커밋 앞서 있고 main은 단계 1 이전 상태다. 공개 저장소에서 처음 보는 사람은 main만 본다. 단계 1~4(Linux) 결과를 main에 합칠지(PR)는 사용자 결정 사항이다.

## 3. 권고 순서

1. (Sol) 2-1 시각 판정 재시도: 첨부 전달.
2. (Sol) 2-2·2-3 정리: Sol 검토 최종 판정 줄, 렌더러 진단 코드 규칙, documentId 스키마 일치, preview 서술, 타임아웃 범위 명시.
3. (Sol) 2-6 인계 묶음 정리: 옛 묶음 구판 표시, SDK 약관 확인 단계 선행.
4. (사용자) PR로 main 반영 여부 결정.
5. (사용자) Windows 확인: S4(0 vs E 우선), engine_inputs 4종, 가능하면 자작 문서 3종.
6. (Sol) 결과 회수 뒤 Linux에서 대조: nativeComparison 기록, 자작 문서로 엔진 편집본 재생성, 2차 Windows 확인 묶음.

이 순서에서 1~3은 Linux만으로 끝낼 수 있다. 4~6이 남으면 "Linux에서 할 수 있는 최대 범위"는 완료로 볼 수 있다.

## 4. 처리 결과 (2026-10-17)

이번 구현 후속 작업의 범위는 아래 1–7이다. 2-1 시각 판정은 별도 Gemini 작업이며 `stage3-visual-check.md`는 수정하지 않았다.

1. **2-2**: Sol 단계 2·3 검토 문서 끝에 R1–R9 수정과 `stage234-linux-results.md` 최종 관문을 반영한 단계 3 조건부 판정을 추가했다. 남은 단일 조건은 “시각 판정은 stage3-visual-check.md를 따른다”이다.
2. **2-3a**: `diagnostic-codes.md`에 실제 생성·전달되는 렌더러 진단 문자열과 뜻을 추가하고 `metadata.diagnostics`와 엔진 오류 코드의 구분을 명시했다.
3. **2-3b**: MCP 스키마의 `documentId`를 선택 사항으로 맞추고 설명·사용자 안내를 갱신했다. 실제 stdio 회귀 시험은 ID 불일치 편집 거절·내용/버전 보존, ID 생략 시 열린 문서 사용, 열린 문서 없는 생략 거절을 확인한다.
4. **2-3c**: `document.preview` 레지스트리 설명과 기본 미설정 사유를 현재 동작(`RENDERER_UNAVAILABLE` / `renderer_not_configured`)에 맞췄다. `supportedInStage1: false`는 과거 단계 메타데이터로 유지하고 현재 `available`은 어댑터 설정에 따라 바뀌는 기존 동작을 시험한다.
5. **2-3d**: stdio 요청별 타임아웃은 0.1 범위 밖으로 확정했다. 렌더러는 별도 60초 제한을 갖는다. 실행기는 동기식이며 자원·입력 한도가 있으므로 실행기 호출의 무기한 정지는 예상하지 않는다. 줄끝 미전송에 대한 stdio 대기는 별도이며 이번에 전송 타임아웃을 추가하지 않는다.
6. **2-4**: 구현 계획 §6에 PNG를 별도 샌드박스 `hae-raster`에서만 생성하는 결정, 신뢰할 수 없는 SVG·글꼴과 프로세스별 메모리 제한이라는 이유, 약 51 MB 바이너리·쪽별 추가 프로세스 비용을 기록했다.
7. **2-6**: 로컬 구 S4 ZIP을 `_구판.zip`으로 바꾸고 점검표 첫 줄에 새 묶음 사용을 명시했다. Windows 안내에 약관 선행 확인·금지/불명확 시 공개 API 입출력 비교 한정 및 6) 추적 생략·글꼴 차이 선분리를 추가하고, 새 ZIP과 내부 `SHA256SUMS.txt`를 재생성했다. 빈 `return/` 하위 폴더도 ZIP에 보존했다. `spikes/work/` 인계 산출물은 기존 Git 무시 정책을 유지한다.

검증: `bash scripts/check.sh` 종료 0(fmt·Clippy·래스터 작업자 빌드, Rust 244개·Python 도우미 30개 통과), `python3 -m unittest discover -s tests/oracle` 88개 통과. 로그: `/tmp/hae-flow-check.log`, `/tmp/hae-flow-oracle.log`. 인계 ZIP의 CRC·원본 파일 일치·해시 목록 22개·빈 반환 폴더 4개를 확인했다. 새 ZIP SHA-256: `79f8040ec4f102b321c038ccf8c604af8320fd15b922e707522ebb45c98be41b`.

8. **2-1**: Gemini 3.8 Flash High가 T3 첨부의 실제 픽셀로 세트 A·B를 모두 PASS로 판정했다(세트 B는 [합성 fixture 수정](stage3-setb-clipping.md) 후 재확인). [시각 검토 기록](stage3-visual-check.md)을 따르며 한컴 비교는 `not_checked`다.
