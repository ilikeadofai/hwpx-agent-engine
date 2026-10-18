# 버전 정책 (0.3.0)

작성일: 2026-10-18. 경쟁 분석 [§6.2](../research/competitive-gap-analysis.md)의
"패키지 버전과 wire version을 구분해야 한다"는 지적을 반영한다.

## 1. 네 가지 버전을 분리한다

| 축 | 값(0.3.0) | 어디에 나타나나 | 올리는 때 |
|---|---|---|---|
| 엔진 패키지 버전 | `0.3.0` | workspace `Cargo.toml`, `hae --version`, `hae-jsonl --version`, `hae-mcp --version`, MCP `serverInfo.version`, `hae-mcp doctor`의 `version`, 렌더러 worker 버전 | 릴리스마다. 기능·수정 어느 쪽이든 |
| 명령 레지스트리 계약 버전 | `"0.3.0"` (문자열 semver) | `command.list`/`command.describe`의 각 descriptor `schemaVersion` (`REGISTRY_SCHEMA_VERSION`) | descriptor 모양이나 호환 규칙이 바뀔 때만 |
| wire 봉투 버전 | `1` (정수) | JSON-lines/MCP 요청·응답 봉투와 hello 줄의 `schemaVersion` | 봉투 필드의 의미가 호환되지 않게 바뀔 때만 |
| 프로젝트 파일 버전 | `1` (정수) | `.hwpwork` manifest의 `schemaVersion` | 저장 형식이 호환되지 않게 바뀔 때만 |

MCP 프로토콜 날짜 버전(`2026-07-28` 등)은 MCP 명세의 것이며 이 표와 독립이다
(`SUPPORTED_PROTOCOL_VERSIONS`).

엔진 버전은 나머지 세 축을 암시하지 않는다. 0.2.0의 레지스트리 버전이 우연히 엔진 버전과
같았을 뿐이며, 이제부터 레지스트리 버전은 별도 상수로 관리한다. 엔진 0.3.1이 나와도
descriptor가 같으면 레지스트리 버전은 `0.3.0`으로 남는다.

## 2. 0.3.0에서의 판단

- 엔진 버전: 0.2.0 -> 0.3.0. 기능 계약 0.3(57개 명령, HWP5 읽기, 표 구조 편집 등)을 배포 버전으로 반영한다.
- 레지스트리 버전: 0.2.0 -> **0.3.0으로 올린다.** 근거: 0.2.0 시점의 descriptor에는 없던 `annotations`
  (readOnly/destructive/idempotent/openWorld 힌트)가 추가되어 descriptor 모양이 달라졌고(필드 추가만이며
  기존 필드 삭제·의미 변경은 없다), 명령 집합이 23개에서 57개로 늘어 클라이언트가 같은 값으로 두 모양을
  구분할 수 있어야 한다. 0.3 설계서는 봉투·프로젝트 형식 변경을 요구하지 않는다.
- wire 봉투 버전: **1 유지.** 0.3은 봉투 필드를 추가하거나 바꾸지 않았다(요청은 `deny_unknown_fields`이고
  구현 시험이 `schemaVersion: 1`을 고정한다). 새 기능은 새 명령 이름과 params로만 표현된다.
- 프로젝트 파일 버전: **1 유지.** manifest 필드가 바뀌지 않았다.

### 2.1 0.4 판단(2026-10-19)

- 엔진 패키지 버전: 이번 작업에서는 올리지 않는다(릴리스 작업이 아니다). 기능 계약 0.4의
  첫 명령군은 아래에 기록한다.
- 레지스트리 버전: **`0.3.0` 유지.** `paragraph.insert`·`paragraph.delete`·`document.create`
  세 명령이 늘었지만 descriptor **모양**(필드)은 바뀌지 않았다. 클라이언트는 `command.list`로
  명령 집합을 발견하고, descriptor 필드는 그대로다. 필드 추가·의미 변경이 생기면 그때 올린다.
- wire 봉투·프로젝트 파일 버전: **1 유지.** 봉투·manifest 필드가 바뀌지 않았다.
- 새 응답 필드 `paragraph[]`는 명령별 결과 객체 안에만 있고 봉투가 아니다.

## 3. 호환 규칙

- 레지스트리 버전은 0.x 동안 semver 마이너를 올릴 때 "필드 추가·명령 추가는 호환, 필드 삭제·의미 변경·
  명령 이름 변경은 비호환"으로 읽는다. 호환되는 변경만 있어도 모양이 달라졌으면 마이너를 올린다.
  패치(0.3.x)는 설명 문구나 schema 설명 수정처럼 모양이 같은 변경에만 쓴다.
- 클라이언트는 알 수 없는 descriptor 필드를 무시해야 하고, 명령 목록은 `command.list`로 발견한다.
- 정수 봉투·프로젝트 버전은 서버가 **정확히 일치하는 값만** 받는다. 알 수 없는 값은 거절한다
  (봉투는 `schemaVersion` 불일치 오류, 프로젝트는 무결성 오류). 자동 하향 변환은 하지 않는다.

## 4. migration 정책

현재 migration 코드는 없다. 정수 버전이 2가 되는 변경이 필요해지면 다음을 지킨다.

1. 한 릴리스 동안 서버가 1과 2를 모두 받고, 응답은 요청의 `schemaVersion`을 따른다.
2. 프로젝트 파일은 읽을 때만 이전 버전을 새 모양으로 올리고(읽기 migration), 저장은 항상 최신 버전으로 쓴다.
   원본 `original.hwpx` 바이트와 해시 검증은 migration과 무관하게 유지한다.
3. migration 계약과 시험(이전 버전 고정 입력 -> 새 모양)을 먼저 문서화하고 CHANGELOG의 "변경"에 적는다.
4. 이전 정수 버전을 끊는 릴리스는 마이너 이상으로 올리고 CHANGELOG에 알린다.

이 정책은 계획이며 현재 구현은 1에 대한 엄격 일치 거절만 한다.
