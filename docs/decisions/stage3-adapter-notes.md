# 단계 3 rhwp 어댑터 구현 메모 (작업 3-2~3-5)

작성: 2026-10-16, Sonnet 5.5. 코드: `crates/hae-render-rhwp/`, `crates/hae-exec/src/preview.rs`(+ `session.rs`의 `handle_preview`).

## 설정과 격리

- rhwp와 bwrap 경로, 폰트 폴더는 `AdapterConfig`(또는 `HAE_RHWP`, `HAE_BWRAP`, `HAE_FONT_DIR`)로 명시한다. PATH 검색은 없다. bwrap 기본값은 `/usr/bin/bwrap`.
- 첫 사용 전 `Sandbox::self_test`가 격리를 증명한다: `/out`만 호스트에 쓰이고, `/in`·`/usr`는 읽기 전용, 호스트 홈은 보이지 않고, 임시 루트는 호스트에 새지 않으며, 네트워크 장치는 `lo`뿐이고, 환경 변수는 지워져 있다. 하나라도 어긋나거나 bwrap이 없으면 `RENDERER_UNAVAILABLE(sandbox_unverified)`이고 렌더러는 실행하지 않는다.
- rhwp 사본과 revision 고정 HWPX 스냅샷을 임시 `in/`에 쓰고 읽기 전용으로 마운트한다. 실행한 사본의 SHA-256이 메타데이터의 `renderer.sha256`이다(`--version`도 같은 사본을 격리 안에서 실행).
- 프로세스 제어: 인자 배열, 60초(`Limits::renderer_timeout_secs`) 벽시계 상한과 kill, stdout 64 MiB·stderr 8 MiB 상한, `/out` 512 MiB·1만 항목 상한(실행 중 100 ms마다 확인), `CancelToken`으로 취소. 모두 `RENDERER_UNAVAILABLE`의 reason으로 구분한다: `timeout`, `cancelled`, `output_limit`, `render_failed`, `invalid_output`, `spawn_failed`, `renderer_missing`, `font_dir_unreadable`, `sandbox_unverified`, `renderer_not_configured`(실행기에서 어댑터 미설정).
- `/out`에서는 매니페스트에 적힌 경로만 읽는다. `/out/` 밖, `..`, 어떤 단계의 심볼릭 링크, 일반 파일이 아닌 것, 매니페스트 바이트 수와 다른 크기는 거절한다. render-tree 출력은 매니페스트가 없어 `dump-pages`의 쪽 수로 정한 `render_tree_NNN.json` 이름만 읽는다.

## 결과 모델

- `rendered`/`partial`/`failed`: 100쪽 초과(`PREVIEW_PAGE_LIMIT` 경고 진단), 안전 검사에서 빠진 쪽, 요청했지만 없는 쪽, 응답 크기 때문에 내용을 뺀 쪽이 있으면 partial. 반환할 쪽이 하나도 없으면 failed. 시작도 못 하거나 출력이 깨졌으면 상태가 아니라 오류로 돌려준다.
- coverage는 항상 `unknown`(미지원 요소 검출 수단이 없다). `nativeComparison`은 `not_checked`.
- `layoutRevision` = `lay-` + SHA-256(입력 해시, 렌더러 해시, 폰트 hash, 쪽 조건)의 앞 16자리. 별도 카운터가 아니다.
- 설계서 §10의 진단 중 hae-diag `Code`에 없는 것(LAYOUT_OVERFLOW 등 rhwp 배치 경고, SVG_UNSAFE, FONT_UNAVAILABLE, PAGE_NOT_RENDERED, PNG_FAILED, PAGE_CONTENT_OMITTED 등)은 코드 목록(31개, 규범 문서)을 늘리지 않고 `metadata.diagnostics`의 문자열 `code`로 낸다. 엔진 진단은 `PREVIEW_PAGE_LIMIT`와 `RENDERER_UNAVAILABLE`뿐이다. 총괄이 코드 목록에 넣을지 정해 달라.
- 응답 4 MiB 한도 때문에 쪽 내용(PNG base64, 선택 SVG)은 인라인이다. `pages`(기본 `[1]`)에 요청한 쪽만 담고, 한도를 넘으면 해당 쪽은 `omitted: "response_limit"`으로 표시한다. 파일로 쓰는 경로(출력 루트)는 hae-project에 일반 쓰기 API가 없어 이번 범위에 넣지 않았다.

## 쪽 정보 조인 (`pages.rs`)

`spikes/s2_mapping/map_paths.py`를 Rust로 옮겼다(구역=spine 순서, 문단 서수, `emulate_controls` ci 규칙, `cellAddr`, 셀 안 그림의 문단 번호 생략). 조인 전 대조(쪽 수, 구역 번호 범위, 표 행·열, 개체 종류, 문단 텍스트 앞 8자, 우리 쪽에 없는 rhwp 주소)에서 하나라도 어긋나면 그 구역(쪽 수 불일치는 문서 전체)을 unavailable로 한다. 자기닫힘 `hp:p`가 있는 구역도 unavailable. 최상위 문단·표·셀·그림만 쪽을 얻고 나머지는 사유(`in-container`, `in-header` 등, `nested-paragraph`, `duplicate-address`, `not-on-any-page`, `unsupported-kind`)와 함께 unavailable이다. 쪽 번호는 1부터 세는 rhwp 물리 쪽 순서이며 모두 `source = "renderer-estimated"`.

실측(28구역 `research-school-plan-reference.hwpx`): 표 8, 셀 308, 최상위 문단 637, 그림 4(그룹 안 14는 unavailable) — 스파이크와 같다. `gyehoek-reference.hwpx`: 문단 194, 표 24, 셀 221, 그림 2 — 스파이크(6a70cb6d_p13)와 같다.

## 실행기 연결

`SessionExecutor::set_renderer(Arc<RhwpAdapter>)` 또는 `configure_renderer_from_env()`로 켠다. 켜지 않으면 `document.preview`는 이전처럼 `RENDERER_UNAVAILABLE`(reason `renderer_not_configured`)이다. 프리뷰는 읽기 전용이다: 문서 상태·revision·이력·요청 번호를 건드리지 않고, `expectedVersion`이 있으면 불일치 시 `VERSION_CONFLICT`. 결과에 `staleLayoutSections`(편집 뒤 줄 배치 캐시를 지운 구역)를 함께 낸다. CLI·JSON-lines·MCP 서버가 `configure_renderer_from_env()`를 부르도록 하는 연결은 해당 crate 소유자가 한다.
