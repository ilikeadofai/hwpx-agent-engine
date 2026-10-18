# MCP 미리보기 성공 경로 실측 (2026-10-18)

실행: Haiku 5.5. 코드 수정·push 없음. 입력은 공개 템플릿 1개(`tests/fixtures/corpus/bodojaryo-reference.hwpx`, MIT, sha256 `4339d2cc…`)만 썼다. 사용자 문서·`spikes/work/hancom-sdk-*`는 열지 않았다. 측정 대상은 Linux 로컬 렌더러 경로이며 한컴 조판 일치는 주장하지 않는다.

## 환경

| 항목 | 값 |
|---|---|
| 저장소 HEAD | `52ed231` |
| 빌드 | `cargo build --release -p hae-mcp -p hae-render-rhwp` (hae-raster 포함), 종료 0 |
| rhwp | `spikes/vendor/rhwp-src/target/release/rhwp`, `rhwp v0.8.7`, sha256 `e91cd25f…`, 소스 커밋 `1a76570e` |
| hae-raster | `target/release/hae-raster`, sha256 `7164f0c3…` |
| HAE_RHWP | 위 rhwp 절대 경로 |
| HAE_FONT_DIR | `/tmp/pv/fonts` (NotoSansKR-Regular.ttf 1개) |
| HAE_BWRAP | 기본값 `/usr/bin/bwrap` (bubblewrap 0.13.0, self-test passed) |
| 입력/출력 루트 | `/tmp/pv/in`, `/tmp/pv/out` |

`/usr/share/fonts` 전체를 HAE_FONT_DIR로 주면 preview가 `RENDERER_UNAVAILABLE`(`bounded font scan: font payload exceeds memory limit`)로 실패한다. 폰트 스캔 상한(64MB)을 넘기 때문이다. 그래서 Noto Sans KR 1개만 담은 폴더를 썼다. `doctor`는 이 상한을 검사하지 않아 `/usr/share/fonts`에서도 fonts를 ok로 낸다.

## 명령

- `hae-mcp doctor --input-root /tmp/pv/in --output-root /tmp/pv/out` (위 환경변수 설정): 6/6 ok.
- 실제 stdio JSON-RPC 세션 드라이버 `/tmp/pv/drv/run.py`(커밋하지 않음). jsonschema 4.26.0으로 structuredContent를 tools/list의 outputSchema에 대조했다.
  - legacy: `initialize`(2025-11-25) → `tools/list` → `document_open` → `element_query`(paragraph, "보도자료") → `document_preview` SVG → `document_preview` PNG → `document_inspect`(revision) → `text_replace`(range 0–4, "MCP 미리보기 시험", expectedVersion 0) → preview 2회 → inspect → `document_export`.
  - stateless: `server/discover`(2026-07-28) → 같은 흐름. 모든 호출에 `_meta`와 `documentId`를 넣었다.
  - 실패 경로: `HAE_RHWP=/tmp/pv/no-such-rhwp`, legacy 세션.

## 결과

| 항목 | legacy 2025-11-25 | stateless 2026-07-28 |
|---|---|---|
| 협상·discover | 2025-11-25 | complete, ttlMs 300000, cacheScope public |
| tools/list | 22개 | 22개(ttlMs·cacheScope 있음) |
| document_open | 통과, rev 0 | 통과, rev 0 |
| preview SVG (1쪽, 전체 2쪽) | rendered, contentRevision 0 | rendered, contentRevision 0 |
| preview PNG (794×1123) | rendered, PNG 시그니처·IHDR·메타 일치 | 같음 |
| preview 후 revision | 0 유지(편집 전 expectedVersion 0 성공으로 확인) | 같음 |
| text_replace | afterVersion 1 | afterVersion 1 |
| preview 편집 후 | contentRevision 1, rendered, staleLayoutSections `[Contents/section0.xml]` | 같음 |
| PNG 해시 편집 전후 | 다름 | 다름 |
| document_export | 통과, zip testzip 통과, 교체 문자열 포함 | 통과, 같음 |
| structuredContent vs outputSchema | 전부 통과 | 전부 통과 |
| 입력 파일 sha 불변 | 통과 | 통과 |
| 서버 종료 코드 | 0 | 0 |
| 점검 합계 | 27/27 | 29/29 |

실패 경로(`HAE_RHWP` 없는 경로): preview는 `RENDERER_UNAVAILABLE`(reason `renderer_missing`), 문서 rev 0 유지, inspect 항목 해시 동일, 입력 sha 불변, 종료 0. 통과.

preview 경고: SVG·PNG 모두 `LAYOUT_TABLE_OVERLAP`(표 겹침 2건 이상). PNG는 `FONT_UNAVAILABLE`(Apple SD Gothic Neo를 대체 렌더링)이 추가로 나온다. 둘 다 렌더러 추정이다.

## 산출 해시

| 파일 | 바이트 | sha256 |
|---|---|---|
| `/tmp/pv/out/legacy-before-p1.png` | 395104 | `cb91bab2afb27c4ab70240fb7b28aa7f9305c4cbec20fa5ee6fd022c2575d5c6` |
| `/tmp/pv/out/legacy-after-p1.png` | 400260 | `3550747ab1102235db7d6dcbd8c25ec38269799fae146b5ae71a687507dfae03` |
| `/tmp/pv/out/legacy-before-p1.svg` | 405050 | `cf8f5a7dea96fa1c20a509db2bf824f9d38f576d2ffcaec7d58e94d8b6cf05f2` |
| `/tmp/pv/out/legacy-after-p1.svg` | 407901 | `7d53cb883b6ab90e2feee2ac30a55c51613075e7994dfe8c37f95dd68f6ab41b` |
| `/tmp/pv/out/legacy-edited.hwpx` | 324355 | `1bf8ff8e1273fc4ac6ed6e77de03dc7a2885be6b513e82a2b3b6c9860dce2a06` |

stateless 산출물은 같은 해시다(`stateless-*`). PNG 두 장은 시각 확인용으로 남겨 둔다.

## 한계

- 폰트가 1개뿐이라 다른 글꼴은 대체 렌더링된다. 조판 수치는 추정이다.
- `doctor`는 폰트 폴더의 읽기 가능 여부만 본다. 폰트 스캔 상한 초과는 실제 preview에서만 드러난다.
- 시각 확인은 총괄이 별도로 맡는다.

## 2026-10-19 후속 (0.4 기능 3호)

위 "실패 경로(`HAE_RHWP` 없는 경로)" 기록은 그 시점의 동작이다. 이제 `HAE_RHWP`가 없어도 어댑터가
문서화된 순서(실행 파일 옆 → `../bin` → 개발 트리 → `PATH`)로 렌더러를 찾고, 찾으면 미리보기가
성공한다. 찾지 못하면 종전처럼 `RENDERER_UNAVAILABLE(renderer_not_configured)`이며 메시지와
`detail.searched`가 살펴본 후보를 알려 준다. 이 문서의 해시·수치는 당시 실행의 기록이므로 바꾸지
않는다. 새 동작의 근거는 [renderer-isolation-platforms.md](renderer-isolation-platforms.md)와
`docs/decisions/stage3-adapter-notes.md`의 2026-10-19 절이다.
