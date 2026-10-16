# 단계 2·3 재개 독립 검토 기록

검토: GPT 6.1 Sol, 2026-10-16. 구현 기준점 `09af4b9` 및 실행 파일 연결 변경. 과거 Codex App Server 실행 실패는 검토 수행으로 집계하지 않는다.

## 단계 2 전송

JSON-lines와 MCP 코드, 실제 프로세스 동등성 시험, Haiku의 기존 블랙박스 기록을 대조했다. `read_line_limited`는 `fill_buf`를 사용해 저장 버퍼를 1MiB 이하로 유지하며, 넘는 줄은 줄끝까지 버린다. 줄끝이 오지 않으면 계속 읽기를 기다리는 stdio 동작이며 요청별 타임아웃은 제공하지 않는다. 이는 렌더러 실행의 60초 제한과 별개다.

수정한 결함:

1. **높음 — MCP 최종 전송 크기 미검사.** `hae-mcp/src/lib.rs`의 도구 응답 JSON을 `content.text` 문자열로 다시 감싸면서 따옴표 이스케이프가 늘었다. 합성 문단에 따옴표 1,100,000개를 넣고 `element_get`을 실제 MCP 프로세스로 호출하면 내부 응답은 제한 안이어도 최종 줄은 4,400,785바이트였다(한도 4,194,304). 최종 JSON-RPC 직렬화 바이트를 검사하고 같은 요청 ID를 유지한 `RESOURCE_LIMIT` 도구 오류로 줄였다.
2. **중간 — 응답 한도 오류에도 `isError=false`.** 내부 응답이 `RESOURCE_LIMIT`로 교체될 때 원래 응답의 `ok`로 플래그를 계산했다. 제한 적용 후 응답의 `ok`에서 `isError`를 계산하도록 고쳤다.

실제 프로세스 회귀 시험 `crates/hae-mcp/tests/response_limit.rs` 2개는 수정 전 각각 실패하고 수정 후 모두 통과했다. 오류 뒤에도 다음 ping이 정상 처리된다. 기존 4개 인터페이스 동등성 시험은 각각 새 세션과 별도 문서 사본을 사용하고, CLI/JSON-lines/MCP를 독립 프로세스로 실행해 payload hash·결과·revision·변경 수를 비교한다.

## 프리뷰 연결과 독립 실행 검증

기존 DeepSeek 작업의 CLI·JSON-lines·MCP 시작 시 렌더러 환경 변수 연결을 수용했다. 새 MCP 시험 드라이버가 알림에 응답을 기다려 정지하는 결함을 수정했다. 제품 서버는 알림에 응답하지 않는 정상 동작이었다. 실제 rhwp/bwrap으로 연결 시험 6개가 통과했다.

명령 조회는 현재 렌더러 설정에 따른 `available`과 `unsupportedReason`을 반환한다. 예전 단계의 지원 여부인 `supportedInStage1`과 현재 사용 가능 여부를 구분한다. 렌더러 미설정은 `renderer_not_configured`이고 설정 뒤 예전 미지원 사유는 사라진다. 해당 회귀 시험도 수정 전 실패, 수정 후 통과했다.

위 변경은 작성자와 다른 모델인 GPT 6 Luna가 다시 실행했다. Haiku는 사용량 제한으로 시작하지 못했으며 사용자 지정 대체 모델 Luna를 사용했다. 정확한 명령과 결과는 `luna-resume-validation.md`에 있다. 전체 작업 트리 관문도 이 시점에 Rust 178개·Python oracle 88개가 통과했으나, 이후 단계 3 보강을 마친 최종 관문은 별도로 다시 기록한다.

## 단계 3

별도 Sol 인스턴스의 독립 반례 검토 `sol-stage3-sandbox.md`에 R1~R9를 기록했다. 이 보고서는 정상 코퍼스 실행 성공을 SVG·메모리 안전성 완료로 대체하지 않는다. 재개 작업에서 해당 반례를 회귀 시험으로 옮겨 수정하고, 해결 여부는 후속 결과 기록과 연결한다.

단계 2 완료 조건 충족: 예(현재 Linux 검증 범위; stdio 요청별 타임아웃은 제공하지 않음).
단계 3 완료 조건 충족: 보강 검증 중(R1~R9 해결 기록·시각 판정·최종 관문을 별도로 확인).


## 2026-10-17 통합 확인 (PNG worker 분리 전)

부모가 환경 연결 시험의 폰트 디렉터리를 제한된 명시 경로로 고쳐 ambient 시스템 폰트 초과를 해결했다. 세 인터페이스 연결과 실제 MCP의 큰 SVG 파일 전달 시험 7개가 통과했다. 프리뷰 기본 PNG 실패·실제 JSON 및 MCP 이스케이프 한도·스냅샷 메모리·큰 SVG 파일 전달 계약 시험 4개도 통과했다. 문서와 이력의 메모리 회계값, 직렬화 스냅샷 및 응답 조립 여유분을 제외한 예산을 어댑터에 전달한다.

`bash scripts/check.sh` 종료 0: fmt, Clippy `-D warnings`, Rust 229개 통과·0실패·0ignored. 로그 `/tmp/hae-before-raster-worker-check.log`. Python oracle 88개 및 fixture 결정성 27,275바이트도 통과했다. 이후 PNG worker 변경의 최종 관문은 별도로 기록한다.

남은 범위: resvg 호스트 실행의 프로세스 격리, Gemini에 실제 이미지 픽셀이 전달되는 시각 검토 경로, Windows S4. `stage3-visual-check.md`의 UNVERIFIED를 전체 성공으로 대체하지 않는다. 모델 운영은 사용자 추가 지시(2026-10-17)에 따라 Sol 총괄·Opus 계획 전용이다.


## 2026-10-17 최종 구현 검토

부모 Sol이 Sonnet의 `worker.rs`, `fonts.rs` 스냅샷 읽기·쓰기 및 `lib.rs` 호출 경로를 대조했다. 제품 프리뷰의 in-process raster 호출과 호스트 fontdb 구축은 제거됐으며, 읽기 전용 스테이징·워커 결과 inventory·exact-size·SHA-256·SVG/폰트 해시 대응 검사가 적용된다. 워커 실패 후 SVG 유지/partial 정책과 실행기의 PNG-only failed 정책은 별개 층에서 일관되게 처리한다. 별도 프로세스 경계는 allocator abort를 부모로 전파하지 않지만, RLIMIT_AS를 전체 시스템 RSS 보장으로 확대하지 않는다.

R1/R2는 정적 SVG 어휘·CSS/MIME 제한과 내장 래스터 헤더·누적 예산에 더해 필수 워커 격리로 보강했다. R3의 재현된 중복 버퍼·무제한 렌더러/래스터 실행은 예약·중복 경로 거절·자식 주소 공간 제한으로 수정했다. R4/R7/R8/R9는 `adapter-budget-fixes.md`, R5/R6 및 파일 전달은 실행기 계약 시험과 실제 MCP 시험으로 연결했다. 프로세스별 주소 공간 제한·보수적 호스트 회계·샘플링 디스크 제한의 범위를 유지한다.

설계 §13의 구조·편집·저장·프로토콜 검증과 렌더러 격리 검증은 Linux 자동화 범위다. 계획 3-6의 시각 판독은 Gemini 도구가 이미지 픽셀을 전달하지 않아 **UNVERIFIED**다. Windows S4와 `nativeComparison`도 **not_checked**이며 전체 한컴 호환성·0.1 최종 시각 수용을 완료로 선언하지 않는다. 최종 관문 수치는 별도 `haiku-final-linux-gate.md` 및 `stage234-linux-results.md`를 기준으로 한다.

## 2026-10-17 단계 3 최종 판정 (R1–R9 수정·최종 관문 반영)

R1–R9 수정과 격리 PNG 작업자 반영 후의 최종 Linux 관문은 [stage234-linux-results.md](../stage234-linux-results.md)에 기록되어 있다. 해당 관문의 `scripts/check.sh` 종료 0, Rust 243개·사용량 도우미 30개·oracle 88개 통과와 실제 stdio MCP 86/86 통과를 근거로 단계 3 완료 조건 충족: **예(조건부)**. 남은 단일 조건: **시각 판정은 stage3-visual-check.md를 따른다**.
