# 단계 2·3·4 Linux 통합 결과

기록일: 2026-10-17 (KST). 총괄: GPT 6.1 Sol. 최종 독립 실행 검사: Claude Haiku 5.5.
검사 대상은 `09af4b9` 이후의 미커밋 통합 작업 트리다. 이후 `bf555f1`의 흐름 검토 후속 작업과 합성 fixture 수정·시각 재판독을 반영했다. MCP 행은 수정 fixture 재실행이며 나머지 Haiku 관문 기록과 구분한다. 아래 수치는 `09af4b9` 단독의 결과가 아니다.

## 구현과 확인 범위

- CLI·JSON-lines·stdio MCP에 명시적 렌더러 설정을 연결하고 현재 명령 사용 가능 여부를 노출했다. MCP는 최종 JSON-RPC 이스케이프까지 포함한 4 MiB 응답 한도를 검사한다. 제한 오류 후에도 연결과 요청 ID를 유지한다.
- 프리뷰는 문서·이력·스냅샷·응답 조립 공간을 제외한 세션 예산을 사용한다. 큰 PNG/SVG는 출력 루트에 새 파일로 원자적으로 공개하고 경로·크기·SHA-256을 반환한다. 입력과 기존 파일은 덮어쓰지 않는다.
- 정적 SVG 어휘·CSS·참조 제한, 이미지 헤더와 누적 디코딩 예산, 폰트 크기와 불변 스냅샷, 요청 쪽수 제한, 쪽 대응 판정, 렌더러 출력 목록·길이 검사를 보강했다. 독립 반례 R1~R9의 수정 근거는 아래 검토 문서에 연결했다.
- 제품 PNG 프리뷰는 별도 `hae-raster` 작업자를 반드시 사용한다. bubblewrap·`prlimit`·남은 전체 제한 시간·취소·출력 한도를 적용한다. 호스트에서 래스터화하거나 fontdb를 구축하는 대체 경로는 없다. 작업자와 SVG·폰트·결과 PNG의 해시, 정확한 파일 목록·길이·IHDR를 검사한다. 작업자 실패 시 SVG와 부분 실패 정보를 유지한다.
- 사용량 도우미는 Codex app-server 및 Claude의 사용량 메타데이터를 읽는다. 누락·잘못된 형식은 `unknown`으로 표시하며 계정·인증 원문을 출력하지 않는다. 사용량 초기화나 유료 요청을 자동 수행하지 않는다.

## 최종 독립 관문

| 명령·범위 | 관측 결과 |
|---|---|
| `bash scripts/check.sh` | 종료 0: fmt·Clippy `-D warnings`·`hae-raster` 빌드 통과 |
| 위 관문의 Rust workspace | 243 passed, 0 failed, 0 ignored |
| 위 관문의 사용량 도우미 Python 시험 | 30개 OK |
| `python3 -m unittest discover -s tests/oracle` | 88개 OK |
| `python3 -I tests/fixtures/editing/gen_editing.py --check-determinism` | `determinism OK (27275 bytes)` |
| `python3 scripts/evaluate_mcp.py --output-dir spikes/work/stage4-linux/2026-10-17-setb-fix` | 실제 stdio MCP: 94 passed, 0 failed, `status: passed`; [합성 fixture 잘림 원인과 수정](stage3-setb-clipping.md) |

최종 실행 기록은 [Haiku 독립 관문](code-review/haiku-final-linux-gate.md)에 있다. 로컬 로그는 `/tmp/hae-haiku-final-{linux-gate,oracle,determinism,mcp}.log`이고, 수정 fixture의 MCP 원문·요약·이미지는 `spikes/work/stage4-linux/2026-10-17-setb-fix/`에 있다. 이전 `2026-10-17-haiku-final/`은 결함 입력의 과거 실행 근거로 보존한다. 이 출력 폴더와 빌드 산출물은 Git 추적 대상이 아니다.

MCP 시나리오는 합성 문서를 열어 조회·스냅샷·편집 전후 프리뷰·dry run·셀 텍스트/문단 서식/그림 일괄 편집·diff·Undo·Redo·프로젝트 저장과 재열기·HWPX 내보내기를 실행했다. 무편집·Undo 내보내기는 입력 바이트와 동일했다. 교체 이미지 원본 파일을 손상한 뒤에도 Redo는 저장한 편집 payload를 복원했다. 프로젝트를 다시 열었을 때 ID와 revision 3이 유지됐다.

실제 실행 식별 정보:

- 수정 fixture MCP 엔진 SHA-256: `ba2a25f64c26d3c6abd70d53c19013c6dc7234a99886e53c0c80bb063af6beb3` (이전 Haiku 실행: `48fb1e84692d8aee23fab90f0153267ee875dff67f9f6c2c0ef39000cf4c826d`).
- rhwp v0.8.7, 소스 기준 `1a76570e833917d15817415a53c09ad61ab3203f`, 실행 파일 SHA-256: `e91cd25fbf5bd8045371ba3e9bdaa1b05cf7df146d04773e65ec1e99e6cd05a7`.
- `hae-raster-v1`, 51,521,288바이트, SHA-256: `3869a9a4a18230b134a23207641cd918c4188b3e4b58f4f2ac941186bbab6fe9`.
- bubblewrap 0.13.0 self-test 통과. 해당 실행의 rhwp 자식 `RLIMIT_AS`는 최대 503,309,894바이트, PNG 작업자는 최대 414,562,667바이트였다.
- 명시적으로 복사한 `NanumGothic.ttf` 1개(4,691,820바이트)를 사용했다. 폰트는 저장소에 번들하지 않았다.

## 모델과 사용량 운영

사용자 지시에 따라 Sol이 총괄하고 Opus 5.5는 계획 전용이다. Sonnet은 PNG 작업자 구현, Haiku는 기계적 독립 검사, DeepSeek는 사용량 도우미 검토, Argon은 안내서, Gemini는 시각 확인 시도를 맡았다. Haiku 제한 중에는 지정 대체 모델 GPT 6 Luna를 사용했다. DeepSeek 경로는 `providerInstanceId: opencode`, `opencode-go/deepseek-v4.1-flash`다. Astra를 추가 사용하지 않았다.

Claude 5시간 한도는 2026-10-17 01:40 KST 초기화 후 01:40:52 조회에서 0%로 확인하고 사용을 재개했다. 부모가 실제 메타데이터를 다시 조회한 02:08:10 KST 기준 Codex는 5시간 69%·주간 40%, Claude는 5시간 24%·주간 18%였다. 다음 5시간 초기화는 각각 02:50:34와 06:40 KST다. 이는 그 시점의 관측값이며 이후 잔량을 보장하지 않는다. 역할·80/90% 전환 기준은 [운영 정책](model-roles.md)에 있다. 초기화 크레딧은 소비하지 않았다.

## 남은 검증과 제한

- **세트 A·B 시각 판독은 PASS**다. Gemini 3.8 Flash High가 T3 첨부로 실제 픽셀을 판독해 세트 A 편집 전후·Undo와 수정 fixture의 세트 B 편집 전후를 확인했다. 세트 B의 초기 상단 잘림은 [합성 fixture 결함 수정](stage3-setb-clipping.md) 후 재판독으로 해소됐다. 판정은 해당 Linux rhwp 미리보기 범위이며, 한컴 기준 조판 비교(`nativeComparison`)는 여전히 `not_checked`다. [시각 검토 기록](stage3-visual-check.md)을 따른다.
- **Windows 한컴 재열기 S4와 기준 조판 비교는 not_checked**다. Linux 공개·합성 fixture와 MCP 대표 시나리오 완료를 임의 한컴 문서의 호환성이나 전체 0.1 최종 수용으로 확대하지 않는다. 사용자 작성 한컴 문서 평가도 남아 있다.
- `RLIMIT_AS`는 자식 프로세스별 가상 주소 공간 제한이다. 호스트와 자식을 합한 RSS·cgroup·tmpfs·커널 메모리 제한이 아니다. 호스트 메모리 회계는 보수적 예약이며 디스크 출력 한도는 샘플링 검사다.
- 워커 실행 파일은 실제 복사한 SHA-256을 기록하며 서명이나 고정된 기대 해시를 검증하지 않는다. 각 쪽의 작업자는 남은 전체 프리뷰 제한 시간을 공유한다.
- 기존 다쪽 문서의 overflow 진단 원인은 모두 분리하지 않았다. 요청 쪽수 제한이나 특정 쪽의 진단 부재를 기존 조판 문제 해결로 해석하지 않는다.

## 상세 근거

- [Sol 단계 2·3 검토](code-review/gpt-6.1-sol-stage2-3.md), [독립 샌드박스 반례](code-review/sol-stage3-sandbox.md)
- [SVG·PNG 제한](resumption-review/svg-png-fixes.md), [어댑터 예산](resumption-review/adapter-budget-fixes.md)
- [원자적 프리뷰 파일 공개](resumption-review/preview-artifacts.md), [별도 PNG 작업자](resumption-review/raster-worker.md)
- [사용량 도우미 수정](resumption-review/provider-usage-fixes.md), [설치·사용 안내](../user-guide.md)

## Linux 최종 정리 관문 (2026-10-17)

GPT 6.1 Sol이 `bf555f1` + 수정 fixture·결과 문서 작업 트리에서 재실행했다. `bash scripts/check.sh` 종료 0(fmt·Clippy·래스터 작업자 빌드, Rust 244 passed·0 failed·0 ignored, Python 도우미 30개 OK), `python3 -m unittest discover -s tests/oracle` 88개 OK, `python3 scripts/evaluate_mcp.py --output-dir spikes/work/stage4-linux/2026-10-17-final-gate` 94 passed·0 failed다. 로컬 로그는 `/tmp/hae-final-wrap-{check,oracle,mcp}.log`다. 최종 MCP의 HWPX 4개·PNG 2개는 위 `2026-10-17-setb-fix` 인계 출처와 바이트 동일하다.

Windows 인계 `engine_inputs/01–04`와 `linux_reference_png/{before,after}.png`를 수정 fixture 실행 산출물로 교체하고 안내 §3·`provenance.json`·`SHA256SUMS.txt`를 갱신했다. SHA-256으로 01=03, 02=04를 확인했다. `spikes/work/Windows_검증_인계_20261017.zip`은 `testzip()` 통과, 압축 파일과 원본 일치, 해시 목록 22개·빈 반환 폴더 4개 보존을 확인했다. ZIP SHA-256: `4dae3215788fee6b71ad28c7f0ee1a8945cef3e068181075014d04047b9a37da`. 인계 묶음과 실행 산출물은 기존 `spikes/work/` Git 무시 정책을 유지한다. Windows·한컴 비교는 계속 `not_checked`다.
