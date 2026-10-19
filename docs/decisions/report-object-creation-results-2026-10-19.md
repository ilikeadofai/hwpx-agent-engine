# 보고서 새 객체 생성·표 삭제 완료 기록 (2026-10-19)

구현·설치 소스: `6c332ea6895747038e89a84273f4afa8976ee177` (`wip/stage1-model-edit`). 이 기록은 후속 문서 커밋으로 남긴다. 실제 미적분Ⅱ 보고서는 다른 세션에서 편집하며 이번 기능별 시나리오는 합성 문서와 공개 MIT 문서로 실행했다. 기존 전체 회귀 시험의 승인된 로컬 fixture 사용 범위와 구별한다.

## 구현한 동작

| 요청 | 제공 명령과 확인 결과 |
|---|---|
| 유의사항 표 자체 삭제 | `table.delete`; 바깥 문단·구역 설정·표 밖 글과 형제 객체·기존 미디어를 보존한다. 안내 문장은 기존 `paragraph.delete`/`text.replace`로 따로 정리한다. |
| 캡션 앞 새 그림 6개 | `image.insert`; PNG/JPEG를 입력 루트에서 검증·고정하고 새 미디어·manifest 항목을 등록한다. 여섯 캡션의 바로 앞 순서와 각 미디어 원본 바이트를 확인했다. |
| 기존 수식 없는 곳에 네이티브 수식 생성 | `equation.insert`; HWP 수식 스크립트와 명시적 크기를 받는다. 수식 없는 새 문서에서 생성·조회·후속 편집·저장·재열기를 확인했다. |
| 새 결과표 생성 | `table.insert`; 기존 표 없이 직사각형 표를 만들고, 셀 내용·조회·후속 편집·저장을 확인했다. |
| 지도교사 이름 | 입력 사실이므로 생성하지 않는다. 합성 시나리오에서도 공란과 기존 ID를 유지했다. |

이전 `hwpx-workbench`의 수식·그림·표 구조를 참고했다. 출처와 MIT 고지는 [기존 스크립트 참고 기록](new-node-prior-art.md)에 있다. 현재 엔진의 원본 보존·독립 verifier 경로를 유지하며 전체 XML 재직렬화 방식으로 우회하지 않았다.

## 검증 결과

| 실행 | 결과 | 로컬 증거 (`spikes/work/object-creation/` 기준) |
|---|---|---|
| `CARGO_BUILD_JOBS=3 ... bash scripts/check.sh` | exit 0; fmt, Clippy `-D warnings`, Python 33 통과 | `workspace-check.log`, `workspace-final-summary.json` |
| Rust workspace | 935 passed, 0 failed, 13 ignored | 같은 전체 로그 |
| 명시 실행한 실제 렌더러 시험 | 위 ignored 13개를 실행하여 13 passed, 0 failed; 전체 명령 합계 948 passed | 같은 전체 로그의 마지막 renderer 완료 표식 |
| 새 명령 전용 실행기 회귀 시험 | 8 passed | `crates/hae-exec/tests/object_creation.rs`; 전체 관문에도 포함 |
| 새 편집기/검증기 단위 시험 | 편집기 28, 검증기 17 passed | 전체 관문에도 포함 |
| 기존 실제 stdio MCP 시나리오 | 146/146 | `existing-mcp/summary.json` |
| 독립 Python 보존 oracle | 111 passed | `oracle.log` |
| 새 실제 stdio MCP 시나리오 | 강화 전 117/117; ID 대조 강화 후 155/155 | `mcp-green-r3/summary.json`, `mcp-reviewed-r4/summary.json` |
| 설치된 릴리스 MCP 시나리오 | 155/155; 14개 객체·셀의 ID 집합과 7개 메타데이터 필드를 두 시점에 196회 비교 | `installed-mcp/summary.json`, `calls.jsonl` |
| Inspector 2.11.0 `tools/list --strict` | exit 0, 도구 65개, 새 명령 4개 확인 | `installed-inspector-strict.log`, `installed-inspector-summary.json` |
| 공개 `gani-gian.hwpx` | 세 종류 새 생성·저장·재열기, Undo 원본 동일·Redo 편집본 동일; 별도 표 삭제 시험에서 17개 하위 노드 제거 및 Undo 원본 동일 | `public-gani-r2/summary.json`, `public-delete/summary.json` |
| 릴리스 빌드/버전 | `cargo build --release --workspace --bins` exit 0; `hae-mcp 0.3.0` | `release-build.log` |
| 설치본 CLI/doctor | `/tmp`에서 모든 `HAE_*` 환경변수 제거 후 20단계 통과, doctor exit 0, 명령 65개 확인 | `installed-cli/summary.json`, `doctor.json`, `command-list.json` |
| 짧은 분수 상자 비교 | 폭 10000/1000 두 설치 시험 각 20단계 통과, PNG/SVG 직접 확인 | `installed-cli/`, `installed-cli-fit/` |

전체 관문 동안 변경 파일 27개의 SHA-256을 고정하고 종료 뒤 대조했다(`gate-source.json`). 이후 Rust 수정은 미통합 상태라는 낡은 모듈 주석의 정리뿐이다. 해당 주석을 이전 형태로 돌리면 고정 SHA-256이 정확히 복원됨을 확인했다(`post-gate-source-audit.json`). 그 뒤 Python 시나리오의 ID 검증을 강화하고 실제 MCP로 재실행했다. 마지막 빌드·설치본은 위 구현 커밋의 깨끗한 소스에서 만들었다.

## 로컬 설치와 다른 세션 연결

릴리스 실행 파일을 `/home/mattc/.local/lib/hwpx-agent-engine/20261019-6c332ea6-objects/bin`에 설치했다. `/home/mattc/.local/bin/`의 `hae`, `hae-jsonl`, `hae-mcp`, `hae-raster`, `rhwp`가 이 폴더를 가리킨다. 이전 버전 디렉터리는 보존한다. 설치 manifest의 실행 파일·폰트 8개 SHA-256을 실제 파일과 대조했다.

설치본 CLI는 환경변수 없이 실행해 앱 옆 폰트 3개(35,745,424바이트)를 자동 선택하고 PNG/SVG를 생성했다. 표 삭제 뒤 그림·수식은 남았으며, 삭제 Undo와 프로젝트 재열기 뒤 HWPX 바이트는 최초 편집본과 같았다. 글꼴 누락/대체 진단은 그대로 보고하며 한컴과 동일한 글꼴·조판이라고 주장하지 않는다.

다른 편집 세션은 열려 있는 작업을 내보내거나 프로젝트로 저장한 뒤 MCP를 다시 연결한다. 새 프로세스 경로는 `/home/mattc/.local/bin/hae-mcp`다. 이미 실행 중인 서버는 교체 전 바이너리로 계속 실행되므로 연결 후 도구 65개 및 새 명령 4개를 확인한다. 실행 중인 다른 세션의 서버를 강제로 종료하지 않았다. [보고서 편집 인계](../report-edit-handoff.md)와 [명령 안내](../user-guide.md#새-표그림수식과-표-삭제)를 읽고 원본 보존·별도 출력으로 편집한다.

## 남아 있는 범위·제한

- 새 객체 삽입 기준은 구역 직속 문단이며, 구역 속성 소유 첫 문단 앞이나 셀·각주·글상자 내부 삽입은 지원하지 않는다. 표 삭제도 지원하는 바깥 문단의 표를 대상으로 한다.
- 수식 치수 자동 측정이나 LaTeX 자동 변환은 없다. `a over b`, 폭 10000·높이 2000의 rhwp SVG는 `scale(10.2302,1.0000)`으로 가로 늘어짐을 보였고, 폭 1000의 별도 설치 시험은 `scale(1.0230,1.0000)`으로 비율이 개선됐다. 모든 수식에 폭 1000을 쓰라는 뜻은 아니다. 스크립트에 맞는 상자를 지정하고 실제 문서 프로그램에서 확인해야 한다. 자동 맞춤/조판 개선은 이번 구현에 포함하지 않는다.
- 그림은 캡션 앞의 독립 문단이다. 둘을 강제로 같은 쪽에 묶지 않으며, 합성 두 쪽 프리뷰에서 6번 그림과 캡션이 갈라진 것을 확인했다.
- 헤더에 보이는 테두리 정의가 없으면 새 표의 선이 안 보일 수 있다. 없는 스타일·테두리를 임의 생성하지 않는다.
- 삽입 단계마다 XML을 다시 분석하고 동일 이미지도 재검증한다. 큰 문서·많은 단계에서 비용이 늘어나는 성능 항목은 남아 있다. 대규모 배치의 캐시/단계 수 제한을 추가하지 않았다.
- 이미지 편집기는 입력을 완전히 디코딩하지만 verifier는 독립적인 정적 형식·치수·바이트 등록 검증을 한다. 별도의 독립 완전 픽셀 디코더 검증으로 과장하지 않는다.

## 한컴 검증 상태와 Windows 인계

Linux 한컴 preflight는 실행 파일 없음으로 exit 2였다(`hancom-gate.log`): `/tmp/opencode/hoffice-run/opt/hnc/hoffice11/Bin/hwp`. 새 결과물을 실제 한컴으로 열었다는 증거는 없다. Windows 한컴 열기·저장·재열기도 **미확인**, `nativeComparison=not_checked`다. Linux 회귀·rhwp PNG·ZIP 보존 성공과 구별한다.

Windows 전달 묶음은 `spikes/work/object-creation/windows-new-objects-20261019-v2.zip`이며 합성 HWPX 8개, 파일별 SHA-256 manifest와 한국어 절차를 포함한다. 원래 묶음도 보존했다. v2 ZIP SHA-256은 `4c28d278c9fd8372972f4b445da2b18f2f8e1876e7f7bce09a0b2176d1780947`다. 새 네이티브 수식의 편집 가능 여부, 표·그림, 안내 삭제, 그림/캡션 쪽 나눔, 수식 상자 비율을 확인하고 별도 이름으로 저장·닫기·재열기한 화면/PDF를 남기는 절차다. 묶음 작성은 이 검증의 수행을 뜻하지 않는다.
