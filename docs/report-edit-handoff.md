# 실제 보고서 편집 세션 인계

이 문서는 다른 세션에서 기존 보고서 HWPX를 수정할 때 사용할 로컬 실행 경로와 절차다. 개발 작업과 실제 보고서 본문 수정은 분리한다.

## 실행 파일

로컬 설치 후 사용할 절대 경로:

- `/home/mattc/.local/bin/hae`: 명령 스크립트 실행
- `/home/mattc/.local/bin/hae-jsonl`: 지속 세션 JSON-lines 서버
- `/home/mattc/.local/bin/hae-mcp`: stdio MCP 서버
- `/home/mattc/.local/bin/hae-raster`: PNG 작업자

이 호스트의 설치본은 같은 소스에서 빌드한 개발용 실행 파일을 버전별 폴더에 복사하고 위 경로로 연결한다. 실행 파일 옆 `fonts/`에는 로컬에서 사용 중인 나눔고딕 Regular·Bold와 Noto Serif CJK Regular를 복사한다(3개, 35,745,424바이트). 배포용 글꼴 번들은 아니며 Git에 넣지 않는다. 설치 폴더의 `manifest.json`에 소스 커밋과 실제 실행 파일·폰트 SHA-256을 기록한다.

MCP 클라이언트 연결 설정이 아직 없다면 CLI로도 같은 엔진을 사용할 수 있다. `.hwp`는 읽기 전용 조회만 가능하므로 편집에는 `.hwpx`가 필요하다.

## 수정 세션에 전달할 지시문

```text
미적분Ⅱ 보고서의 기존 HWPX를 수정한다.
엔진 저장소: /home/mattc/Documents/Codex/hwpx-agent-engine
CLI: /home/mattc/.local/bin/hae
MCP: /home/mattc/.local/bin/hae-mcp
참고: 위 저장소 docs/report-edit-handoff.md, docs/user-guide.md

먼저 원본 경로와 수정 지시를 확인하고 원본 SHA-256을 기록한다.
원본은 덮어쓰지 말고 별도 출력 폴더의 새 HWPX로 내보낸다.
document.inspect / element.query / element.get으로 구조와 현재 내용을 확인한다.
지원 여부는 command.list / command.describe와 실제 dryRun 결과로 판단한다.
수정은 expectedVersion을 확인하고, 관련 작업은 가능한 한 batch로 묶는다.
기존 수식은 equation.list/get으로 확인한 뒤 script/properties 명령으로 수정한다.
새 수식·그림·표는 equation.insert / image.insert / table.insert로 만들 수 있다.
구역 직속 기준 문단과 before/after, 명시적 HWPUNIT 크기를 지정한다.
유의사항 표는 table.delete로 표 자체만 지우고 안내 문장은 별도로 수정한다.
지원하지 않는 작업을 XML 전체 재작성으로 우회하지 않는다.
내보낸 파일을 다시 열어 본문/표/수식 등 요청한 변경과 비대상 보존을 확인한다.
프리뷰는 요청한 쪽을 생성하고 진단을 확인한다. 한컴 기준 조판으로 단정하지 않는다.
원본 경로, 출력 경로, 변경 목록, 검증 결과와 남은 확인 사항을 보고한다.
```

## 명령 실행

입력·출력 허용 폴더를 명시한다. 명령 스크립트는 nodeId를 먼저 조회한 뒤 그 결과를 참조하도록 구성한다.

```sh
/home/mattc/.local/bin/hae-mcp doctor \
  --input-root /absolute/path/to/input \
  --output-root /absolute/path/to/output

/home/mattc/.local/bin/hae script \
  --input-root /absolute/path/to/input \
  --output-root /absolute/path/to/output \
  /absolute/path/to/steps.json
```

`hae script`의 `$revision`은 현재 버전, `$lastMatchNodeId`는 직전 검색의 유일한 대상이다. 조회가 여러 요소를 반환하면 ID를 확정한 뒤 편집한다. 독립적인 `hae run` 호출 사이에는 열린 문서나 Undo 상태가 유지되지 않는다.

표 셀은 표 ID와 `row`·`col`로 찾고 반환된 `textPreview`를 대조한다. `textContains`로 셀을 검색하는 방식에 의존하지 않는다. 필요하면 셀의 자식 문단을 따로 조회한다.

## 지원 범위와 확인 경계

현재 생성·부분 편집·일괄 텍스트 교체·글자/문단 서식·표 셀 및 행/열 편집·기존 수식 편집·그림 교체·프로젝트 저장·Undo/Redo를 제공한다. 새 표·그림·네이티브 수식 생성과 표 삭제도 명령 레지스트리에서 제공한다. 구체적인 인자와 제약은 실행 중 레지스트리에서 조회한다.

셀 병합·분할 및 추가 글꼴 서식 확장은 이번 실제 편집 준비 관문에 포함되지 않는다. 새 객체 삽입의 기준은 구역 직속 문단이며, 구역 속성 소유 문단 앞 삽입과 지원하지 않는 중첩 위치는 사유를 보고한다. 수식은 HWP 수식 스크립트를 받는다. LaTeX를 자동 변환하거나 Unicode 본문으로 대신하지 않는다. 지도교사 이름 등 누락된 사실은 별도로 받아야 한다.

새 객체 작업의 인자 예시는 [사용 안내의 새 표·그림·수식 절](user-guide.md#새-표그림수식과-표-삭제)에 있다. `table.delete`는 표만 지우므로 표를 담던 빈 문단이 남을 수 있다. 이를 정리하려면 표 삭제 뒤 해당 문단의 내용과 구역 속성 여부를 다시 조회하고, 일반 문단인 경우 `paragraph.delete`를 별도로 사용한다. 그림은 각 `[그림N]` 캡션의 문단 ID를 확정한 뒤 `position:"before"`로 넣는다.

외부 렌더러의 쪽 번호와 조판은 추정값이다. Windows 한컴의 최신 보고서 편집 결과를 직접 열고 저장·재열기한 확인으로 대체할 수 없다. 논문·학교 보고서의 내용 정확성, 실제 연구 수행 여부와 학교 제출 여부는 파일 편집 성공과 별도로 판단한다.

## 이번 준비 작업의 실제 실행 기록 (2026-10-19)

- 자동 탐색의 폰트 후보를 실제 파일 수·용량 한도로 검사한다. 큰 폴더는 사유를 기록하고 건너뛴다. 전체 후보가 모두 부적합할 때의 제한된 하위 폴더 선택도 선택 범위를 보고한다. 명시한 `HAE_FONT_DIR`는 다른 폴더로 대체하지 않는다.
- CLI 설정 5종(환경변수 없음, 폰트만, rhwp만, rhwp+폰트, rhwp+raster+폰트) 모두 실제 SVG·PNG 생성에 성공했다. MCP·JSONL도 환경변수 없이 렌더링했다.
- 새 문서 생성 → 문단 추가·삭제 → 제목·정렬 수정 → 내보내기 → 프리뷰 → 프로젝트 저장·재열기 → 재내보내기 16단계를 실행했다. 문단 내용과 재내보내기 바이트 동일성을 확인했다. 이 첫 실행은 명시한 작은 폰트 폴더를 사용했다.
- 공개 `gani-gian.hwpx`에서 본문·셀 편집, Undo/Redo, 내보내기·재열기·프리뷰 17단계를 환경변수 없이 실행했다. 무편집·Undo는 원본 바이트와 같았고 편집·Redo도 서로 같았다. 재열기 후 수정된 본문·셀을 조회했다.
- 실제 MCP 평가 시나리오는 146개 검증을 통과했고, 독립 보존 oracle은 111개를 통과했다. 편집 fixture 재생성 결정성도 통과했다(43,990바이트).

실행 원문과 해시는 저장소의 `spikes/work/finish-20261019/` 아래에 보관한다. 사용자 보고서는 이 검사에 사용하지 않았다.

최종 설치본을 `/tmp`에서 모든 `HAE_*` 변수 없이 실행해 같은 생성·편집·프로젝트 재열기 16단계를 통과했다. 앱 옆 폰트 3개가 자동 선택됐고 PNG도 생성됐다. 설치 manifest의 실행 파일·폰트 8개 해시를 실제 파일과 대조했다(`installed-e2e/summary.json`, `installed.json`).

추가 전체 관문 `CARGO_BUILD_JOBS=3 bash scripts/check.sh`도 종료 코드 0으로 완료했다. fmt·Clippy(`-D warnings`)·PNG 작업자 빌드 및 Python 시험 33개가 통과했다. Rust workspace는 **882 passed, 0 failed, 13 ignored**였으며, 그 13개 실제 렌더러 시험을 후속 명령으로 명시 실행해 **13 passed, 0 failed**를 확인했다. 전체 명령 합계는 895 passed다. 완료 로그는 `workspace-check.log`, 전체 로그에서 다시 집계한 요약은 `workspace-final-summary.json`이다. Windows 한컴 조판 대조를 수행했다는 뜻은 아니다.
