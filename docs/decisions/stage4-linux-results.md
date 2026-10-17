# 단계 4 Linux 대표 시나리오 end-to-end 결과 (사용자 한컴 작성 문서)

기록일: 2026-10-17 (KST). 실행·기록: Claude Sonnet 5.5 (시험 하위 에이전트).
범위: 설계서 §1 대표 시나리오를 사용자가 Windows 한컴에서 작성한 시험 문서 5개에 실제 엔진(`hae script`)으로 끝까지 실행했다. 한컴 Linux 관문은 쓰지 않았다.

```
windowsVerification: deferred
nativeComparison: not_checked
```

사용자가 Windows 검증을 마지막으로 미루기로 했다. 이 문서의 어떤 항목도 한컴에서 열림·재저장·재열기가 되는지, 한컴 쪽수와 같은지를 말하지 않는다.

## 개인정보 처리

시험 문서 본문은 읽거나 인용하지 않았다. 드라이버는 응답을 메모리에서만 다루고 디스크와 화면에는 허용 목록 필드(nodeId, 종류, 진단 코드, 해시, 경로, 숫자)만 남긴다. 그 밖의 문자열은 `<str len=N>`으로 바꾼다. 저장한 로그에서 한글 본문이 없음을 grep으로 확인했다(파일명 `글자처럼취급` 제외). 미리보기 PNG는 열어 보지 않았고, SVG는 태그 이름 개수만 셌다. 문서 내용에서 온 값은 이 보고서에 없다. 편집에 넣은 표식은 ASCII `S4`뿐이다.

## 실행 환경

| 항목 | 값 |
|---|---|
| 저장소·브랜치 | `/home/mattc/Documents/Codex/hwpx-agent-engine`, `wip/stage1-model-edit` |
| 지시받은 기준 | `c2993b7` (작업 시작 시 트리 깨끗) |
| 실행 중 변화 (내가 한 일 아님) | HEAD가 `7a0978c`(`c2993b7` 뒤 문서 전용 2커밋: README, `engine-output-open-verification-2026-10-17.md`)로 이동했고, `tests/fixtures/editing/` 아래 19개 파일(`gen_editing.py` 포함)이 미커밋 수정 상태가 됐다. |
| 엔진 바이너리 | `target/debug/hae` 0.1.0, 수정 시각 23:03, `cargo build -p hae-cli --locked`는 "Finished 0.13s"(재빌드 없음). 실행(23:26~23:27) 중 바이너리는 바뀌지 않았다. sha256 `67ffc6f9220dd6a6e57f731294cbe5d2ca942a6c578439e3d46caaf79822bc19` |
| PNG 작업자 | `target/debug/hae-raster` sha256 `6cfae614a2d945d676b38d2fe9a70ba5b5b4524e76a76a80189cb4e7c22e5d68` |
| 렌더러 | rhwp v0.8.7, sha256 `e91cd25fbf5bd8045371ba3e9bdaa1b05cf7df146d04773e65ec1e99e6cd05a7` (단계 3 기록과 일치), bubblewrap 0.13.0 |
| 폰트 폴더 | `NanumGothic.ttf` 한 개(sha256 `48a28e97…8be733`). 한컴 글꼴은 없다. |
| 교체 이미지 | `tests/fixtures/editing/inputs/ok.png` 사본, 2×1 픽셀 70바이트, sha256 `897d67be…ca20` |

작업 폴더: `spikes/work/stage4-e2e/` — `inputs/`(시험 문서 사본 5개와 `ok.png`; 원본은 건드리지 않았고 sha256이 원본과 같음), `driver/`(Python 표준 라이브러리 스크립트), `runs/`(CLI 스크립트·가림 처리한 응답·stderr), `out/<문서>/`(내보내기·프로젝트·미리보기·`steps.redacted.json`), `out/results.json`·`extras.json`·`preservation.json`.

재현: `python3 -I spikes/work/stage4-e2e/driver/run_e2e.py <문서이름>` 뒤 `python3 -I …/analyze.py <문서들>`. 출력 폴더가 비어 있지 않으면 거절한다.

노드 ID는 문서를 열 때마다 새로 만들어지므로(documentId가 열 때마다 다르다) 대상은 같은 세션 안의 `element.query` 결과를 `saveAs`로 저장해 `$q_<종류>.matches.<n>.nodeId`로 참조했다. 처음 시도에서 계획 세션의 ID를 쓴 탓에 `TARGET_NOT_FOUND`가 났고 이 방식으로 고쳤다. 결과표는 고친 방식의 두 번째 전체 실행이다.

## 1. 문서별 결과

시험 문서(사용자 작성, 입력 sha256 전체):

| 문서 | sha256 | 구조(엔진 관측) |
|---|---|---|
| `01_text` | `726f4a4abfeff29cadecde5478438845e7e2da12997e9c5bacb3ae1cd10d3674` | 노드 11, 문단 10, 표·그림·수식 없음 |
| `02_table` | `04126cb5d2a69a8aa732a5457decf5d1469244dd774737ed1e5fc7aecb9a7ef2` | 노드 20, 표 1(2×2), 셀 4, 문단 14 |
| `03_picture` | `6c72b5a12fa4abbdc861562879fc292d6d0632802dc30927c75ea7bfad04dfcc` | 노드 3, 문단 1, 그림 1(`treatAsChar="0"`, 종이 기준 배치) |
| `03_picture_글자처럼취급` | `3e5ca8e8d4e61d25325eb460193de06b839dd8f4fda1454898a28526ce2f2fbb` | 노드 3, 문단 1, 그림 1(`treatAsChar="1"`) |
| `04_complex` | `7d63bc6b1a785a355dc8a8bc1074124636e4a4a267fa47a8b51617186a7a51ff` | 노드 137, 문단 113, 표 2, 셀 6, 수식 15 |

단계별 실행 여부. ✔ = 실행하고 성공, 거절 = 엔진이 진단과 함께 거절, N/A = 문서에 대상 요소가 없어 실행하지 않음.

| 단계 | 01_text | 02_table | 03_picture | 03_글자처럼취급 | 04_complex |
|---|---|---|---|---|---|
| open (revision 0, editable, readonlyReasons 없음) | ✔ | ✔ | ✔ | ✔ | ✔ |
| 표 조회 (`element.query kind=table`) | ✔ (0건) | ✔ (1건) | ✔ (0건) | ✔ (0건) | ✔ (2건) |
| `cell.setText` | N/A (표 없음) | 거절 ×4 | N/A | N/A | 거절 ×6 |
| 대체 `text.replace` (셀 첫 문단 또는 일반 문단, 범위 [0,1) → `S4`) | ✔ (최상위 문단, 대체) | ✔ (셀 r0c0의 첫 문단) | N/A | N/A | ✔ (병합 셀 table#0 r0c0의 첫 문단) |
| `paragraph.format` 가운데 정렬 | ✔ | ✔ (위와 같은 문단) | ✔ (그림이 든 문단) | ✔ (그림이 든 문단) | ✔ (위와 같은 문단) |
| `image.replace` | N/A (그림 없음) | N/A | ✔ | ✔ | N/A (그림 노드·`hp:pic` 없음) |
| 적용 전 dryRun | ✔ | ✔ | ✔ | ✔ | ✔ |
| 적용 (`command.batch`, revision 0→1) | ✔ | ✔ | ✔ | ✔ | ✔ |
| `document.diff` (스냅샷 대비) | ✔ | ✔ | ✔ | ✔ | ✔ |
| preview 편집 전/후 | ✔ / ✔ | ✔ / ✔ | ✔ / ✔ | ✔ / ✔ | ✔ / ✔ |
| undo (revision 1→2) 뒤 export | ✔ | ✔ | ✔ | ✔ | ✔ |
| redo (revision 2→3) 뒤 export | ✔ | ✔ | ✔ | ✔ | ✔ |
| `project.save` (savedVersion 3) · export · close | ✔ | ✔ | ✔ | ✔ | ✔ |
| 프로젝트 재열기 | ✔ | ✔ | ✔ | ✔ | ✔ |

판정 값:

| 항목 | 01_text | 02_table | 03_picture | 03_글자처럼취급 | 04_complex |
|---|---|---|---|---|---|
| 쪽 수 편집 전 → 후 (rhwp 추정) | 2 → 2 | 1 → 1 | 1 → 1 | 1 → 1 | 4 → 4 |
| 무편집 export = 입력 바이트 | 예 | 예 | 예 | 예 | 예 |
| **undo 뒤 export = 입력 바이트 (sha256 일치)** | **예** | **예** | **예** | **예** | **예** |
| undo export의 `unmodifiedPassthrough` | true | true | true | true | true |
| redo export = 적용 export 바이트 | 예 | 예 | 예 | 예 | 예 |
| 저장 직후 편집 export = redo export | 예 | 예 | 예 | 예 | 예 |
| dryRun: revision 변화 없음 / diff identical / 예측 afterVersion | 0 / true / 1 | 0 / true / 1 | 0 / true / 1 | 0 / true / 1 | 0 / true / 1 |
| 적용: before→after | 0→1 | 0→1 | 0→1 | 0→1 | 0→1 |
| diff 변경 항목 | 구역·문단 | 구역·표·셀·문단 | 그림, 미디어 변경 1 | 그림, 미디어 변경 1 | 구역·표·셀·문단 |
| diff 변경 ZIP 항목 | header, section0, PrvText | header, section0, PrvText | header, section0, content.hpf | header, section0, content.hpf | header, section0, PrvText |
| 재열기: documentId 유지 | 예 | 예 | 예 | 예 | 예 |
| 재열기: revision 유지 (3 → 3) | 예 | 예 | 예 | 예 | 예 |
| 재열기: 노드 ID·종류 목록 동일 (저장 전, 최초 open 때와 모두) | 예 | 예 | 예 | 예 | 예 |
| 노드 수 | 11 | 20 | 3 | 3 | 137 |
| 편집 export를 새로 열기 (새 documentId, revision 0, editable) | 예 | 예 | 예 | 예 | 예 |

세부:

- 거절 시도는 모두 `expectedVersion: 0`으로 보냈고, 거절 뒤 revision은 5개 문서 모두 0이었다.
- undo/redo 번호는 undo가 revision을 2로, redo가 3으로 올렸다(`scripts/evaluate_mcp.py`가 기대하는 값과 같다).
- 적용 export의 ZIP이 `testzip` 오류 없이 열렸다. 변경되지 않은 항목은 입력과 바이트가 같다(01: 8개, 02: 8개, 03 두 개: 9개, 04: 12개). `04_complex`에서 `Chart/chart1.xml`, `BinData/ole*.ole`, `Contents/memoExtended.xml`이 이에 포함된다.
- 그림 두 문서는 적용 export에 `BinData/hae-image-1.png` 항목이 하나 늘었다(12 → 13 항목). 원래 그림 항목은 남아 있다.
- 편집한 구역의 `hp:linesegarray`(줄 배치 캐시)를 엔진이 지웠다: 01 10→2, 02 14→11, 03 두 개 1→0, 04 111→91(구역 태그 수 비교). 표·그림·수식 등 다른 구역 태그 수는 입력과 같다. preview 결과의 `staleLayoutSections`가 편집 후 1, 편집 전 0이다. 한컴이 이 캐시 없이 어떻게 여는지는 확인하지 못했다.

## 2. 거절 기록 (코드와 사유)

`cell.setText`의 알려진 제약(여러 문단 셀은 거절)을 새 실패로 세지 않는다. 시험 문서의 셀은 모두 거절됐다: 단일 문단 셀이 하나도 없어서 **`cell.setText`의 성공 경로는 이 문서들에서 실행하지 못했다**.

| 문서 | 시도 | 코드 | 사유(엔진 메시지) |
|---|---|---|---|
| 02_table | `cell.setText` r0c0, r0c1, r1c0, r1c1 (병합 없음) | `UNSUPPORTED` ×4 | `cell cannot be edited: cell_paragraph_count` (셀마다 문단 3개: 첫 문단 + 빈 문단 2) |
| 04_complex table#0 | r0c0 (가로 2칸 병합) | `UNSUPPORTED` | `cell cannot be edited: merged_cell` |
| 04_complex table#0 | r1c0, r1c1 | `UNSUPPORTED` ×2 | `cell cannot be edited: cell_paragraph_count` |
| 04_complex table#1 | r0c0, r1c0 | `UNSUPPORTED` ×2 | `cell cannot be edited: cell_paragraph_count` |
| 04_complex table#1 | r0c1 (세로 2칸 병합) | `UNSUPPORTED` | `cell cannot be edited: merged_cell` |
| 04_complex | `text.replace`를 수식 노드에 지정 | `UNSUPPORTED` | `node <id> is a equation, not a paragraph` |

`document.inspect`가 알려 주는 편집 불가 사유(거절 시도 없이 목록으로만 확인한 것): 수식 15개 `equation_edit_unsupported`, 셀·표·구역 `cell_paragraph_count`·`merged_cell`·`no_edit_command`.

편집 대상 문단을 찾는 dryRun 탐침(상태 변경 없음): `text.replace` 범위 [0,1)을 최상위 비어 있지 않은 문단에 시험했다. `01_text`는 5개 중 2개, `04_complex`는 시험한 12개 전부가 `RANGE_CROSSES_CONTROL`로 거절됐다(첫 글자 위치에 제어 요소가 있다는 뜻). 그래서 `01_text`는 통과한 문단을 썼고 `04_complex`는 표 셀 문단으로 대체했다.

`04_complex`의 구역 XML에는 차트 1, OLE 2, 글맵시 1, 글상자 `drawText` 2, 호·다각형·사각형 도형, 각주·미주 각 1, 머리말 1, 필드 6쌍, 수식 15가 있다(태그 개수). 수식 외에는 엔진 노드로 나오지 않아 거절 진단을 받을 대상이 없었다. 즉 이 요소들에 대해 "거절 코드"는 관측하지 못했고, 편집 중 보존됐음만(위의 변경 없는 항목 바이트 동일, 구역 태그 수 동일) 확인했다.

CLI 종료 코드: `02_table`과 `04_complex`는 1이다. 실패한 단계가 위 거절 시도(각각 4건, 7건)뿐이어서다. 나머지 세 문서는 0.

## 3. 미리보기 (rhwp 어댑터)

모든 미리보기는 `status: rendered`, 렌더링 예, `nativeComparison: not_checked`, `coverage: unknown`이다. 요청은 1쪽만(`pages:[1]`, PNG+SVG)이어서 나머지 쪽은 PNG를 만들지 않았다. 쪽 수는 rhwp 추정값이며 한컴 쪽 수가 아니다. 모든 PNG는 794×1123.

| 문서 | 구분 | 쪽 수 | layoutRevision | PNG (바이트, sha256 앞 16) | SVG (바이트) | 진단 |
|---|---|---|---|---|---|---|
| 01_text | 전 | 2 | `lay-23c14db64111d7cc` | `out/01_text/preview-ffa844988143626cd6.png` (427,337, `1a3477eaf54ae405`) | `out/01_text/preview-178fe64338a5690138.svg` (704,500) | `FONT_UNAVAILABLE` ×25, `PAGE_CONTENT_ARTIFACT`(info) |
| 01_text | 후 | 2 | `lay-9b11534aa5bf0ae2` | `out/01_text/preview-9874c02acb89918326.png` (413,342, `19a52f2a10d3d48d`) | `out/01_text/preview-7ab3ccade8a3bce4ec.svg` (686,290) | 같음 |
| 02_table | 전 | 1 | `lay-313d88db900197bb` | `out/02_table/preview-bdfad8ca1df70b96bc.png` (432,227, `57f8def207383050`) | `out/02_table/preview-796e7f880c11761c61.svg` (568,880) | `FONT_UNAVAILABLE` ×13, `PAGE_CONTENT_ARTIFACT` |
| 02_table | 후 | 1 | `lay-50951ab040935aed` | `out/02_table/preview-70a33c282561b5fae3.png` (430,676, `1ff2165f1289e537`) | `out/02_table/preview-34a206ca256943bf2f.svg` (569,803) | 같음 |
| 03_picture | 전 | 1 | `lay-3d04a05c24f9bd7e` | `out/03_picture/preview-bcb0c3dd8d02280e00.png` (1,408,914, `a0f3f1640b28da04`) | `out/03_picture/preview-997953b1e8fc0dbb17.svg` (3,535,467; `<image>` 1) | `PAGE_CONTENT_ARTIFACT` |
| 03_picture | 후 | 1 | `lay-a6503028f704b90f` | `out/03_picture/preview-after-page1.png` (24,201, `0d943841902db0ad`, 인라인 응답에서 저장) | `out/03_picture/preview-after-page1.svg` (483; `<image>` 1) | 없음 (작아서 인라인) |
| 03_글자처럼취급 | 전 | 1 | `lay-9e93fbe4dd3a0431` | `out/03_picture_글자처럼취급/preview-db8f80f571c4912cf2.png` (1,220,734, `67513cac778cebd5`) | `out/03_picture_글자처럼취급/preview-2d60fed56bdc5ae27d.svg` (3,535,695; `<image>` 1) | `PAGE_CONTENT_ARTIFACT` |
| 03_글자처럼취급 | 후 | 1 | `lay-d53929a1df9f8688` | `out/03_picture_글자처럼취급/preview-after-page1.png` (28,268, `1043a516275f150a`, 인라인) | `out/03_picture_글자처럼취급/preview-after-page1.svg` (711; `<image>` 1) | 없음 |
| 04_complex | 전 | 4 | `lay-66fc97af9a8e936f` | `out/04_complex/preview-9f1dd0c07043dfd792.png` (455,237, `a378f1bc2f88d433`) | `out/04_complex/preview-da2b364bb20ca129c2.svg` (986,310) | `LAYOUT_OVERFLOW_CELL` ×44, `LAYOUT_OVERFLOW` ×1, `LAYOUT_OVERFLOW_CELL_LINES` ×1, `FONT_UNAVAILABLE` ×20, `PAGE_CONTENT_ARTIFACT` |
| 04_complex | 후 | 4 | `lay-2dd1dd26d758f0ee` | `out/04_complex/preview-df87bcbd5ebcbd779f.png` (454,130, `135d289ad6625a30`) | `out/04_complex/preview-df3a9962380f984ae9.svg` (986,846) | 같음 |

(경로 기준은 모두 `spikes/work/stage4-e2e/`.)

- 편집 전후 PNG 해시는 5개 문서 모두 다르다. 그림 문서의 후 미리보기 SVG에는 `<image>` 요소가 여전히 1개 있다(교체 이미지가 2×1 픽셀 70바이트라 SVG가 작다).
- `FONT_UNAVAILABLE`는 렌더용 폴더에 나눔고딕만 있어서 생긴 대체 글꼴 경고다. 한컴 글꼴로 조판한 결과가 아니므로 쪽 수·줄 바꿈을 한컴과 같다고 볼 수 없다.
- `LAYOUT_OVERFLOW*` 46건은 편집 전후 개수가 같다. 이 진단들이 한컴에서도 넘침이 있다는 뜻인지는 모른다.
- 미리보기 응답의 `PAGE_CONTENT_ARTIFACT`는 info 수준이며 "쪽 내용이 출력 루트 아래 파일로 제공됨"이라는 알림이다.
- 미리보기 PNG/SVG는 눈으로 확인하지 않았다(개인정보 규칙). 렌더링이 시각적으로 맞는지는 판정하지 않았다.

## 4. 확인된 것과 확인하지 못한 것

### 확인된 것 (이번 실행에서 직접 실행하고 관측)

- 실제 `hae script`(세션 1개)로 5개 문서 모두에서 open → 조회 → dryRun → 적용 → diff → 미리보기 전/후 → undo → export → redo → export → `project.save` → export → close → 프로젝트 재열기까지 끝까지 돌았고, 거절 시도 외의 모든 단계가 성공했다.
- dryRun은 revision(0)과 문서 상태를 바꾸지 않았고(diff identical) 예측 afterVersion 1이 적용 결과(1)와 같았다.
- 적용은 revision을 정확히 한 번(0→1) 올렸다. undo는 2, redo는 3.
- 5개 문서 모두에서 undo 뒤 export가 입력 파일과 바이트 단위로 같다(`unmodifiedPassthrough: true`). 이는 엔진이 미수정 상태로 인식해 원본 바이트를 내보낸 결과이며, 다시 직렬화해서 일치한 것은 아니다.
- redo export는 최초 적용 export와 바이트가 같고, 저장 뒤 export도 같다.
- 프로젝트 재열기에서 documentId, revision(3), 노드 ID·종류 목록(최초 open 때 것과도)이 모두 유지됐다.
- 엔진이 내보낸 편집 HWPX를 엔진이 다시 열 수 있다(새 documentId, revision 0).
- 표·그림·수식·차트·OLE·메모 등 편집하지 않은 구조의 개수와 편집하지 않은 ZIP 항목의 바이트는 편집 export에서 그대로다.
- 알려진 거절은 위 표와 같은 코드와 사유로 일관되게 재현됐고, 거절은 문서 상태를 바꾸지 않았다.
- rhwp 미리보기는 5개 문서 모두 `rendered`였고 쪽 수 편집 전후 변화가 없었다.

### 확인하지 못한 것 / 하지 않은 것

- 한컴 열기·복구 안내·재저장·재열기, 쪽 수 일치, 시각 일치: **미확인 (`windowsVerification: deferred`, `nativeComparison: not_checked`)**. 사용자 폴더에 있는 한컴 PDF와의 비교도 하지 않았다.
- `cell.setText`의 **성공** 경로: 시험 문서의 모든 셀이 거절돼 실행하지 못했다. 합성 문서에서의 성공은 `scripts/evaluate_mcp.py`의 기존 기록에 있고, 이번에 다시 돌리지 않았다.
- `01_text`, `02_table`, `04_complex`에서의 `image.replace`와 `01_text`, `03_*`에서의 표 편집: 대상 요소가 없어 실행하지 않았다.
- `04_complex`의 수식 외 요소(차트, OLE, 도형, 글맵시, 메모, 각주·미주, 필드)에 대한 엔진 거절 코드: 노드로 나오지 않아 관측하지 못했다. 보존만 확인했다.
- 1쪽 외 쪽의 PNG·SVG: 요청하지 않았다. 쪽 수만 보고했다.
- 미리보기 그림의 시각적 정확성, 가운데 정렬이 실제 화면에서 가운데로 보이는지: 보지 않았다. 정렬은 적용 결과의 intent(`alignment: CENTER`)와 header 변경으로만 확인했다.
- 편집으로 `hp:linesegarray`가 지워진 파일을 한컴이 어떻게 처리하는지: 미확인(Windows 라운드에서 볼 사항).
- `03_picture` 계열 그림의 `curSz`가 0×0이다(문서 원본 값). 교체 뒤 한컴 표시 크기는 미확인이다.
- 실행 중 저장소 상태가 외부 요인으로 변했다(위 환경 표). 변경은 문서 2건과 시험 fixture 미커밋 수정이며, 이번 실행이 쓴 엔진 바이너리·입력·`ok.png`에는 영향이 없다고 판단하지만 `cargo build`를 다시 돌려 확인하지는 않았다.
- 이 실행은 디버그 빌드 한 번의 결과다. 반복 실행에 의한 결정성(예: 같은 입력으로 두 번 돌렸을 때 export 바이트 동일)은 검사하지 않았다. 다만 한 세션 안에서 redo export와 적용 export가 같은 바이트임은 확인했다.

## 부수 관측

- 프로젝트 저장 폴더에 0바이트 `session.hwpwork.lock` 파일이 세션 종료 뒤에도 남는다. `hae-project`가 `flock` 기반 잠금에 쓰는 파일이므로 결함으로 보지는 않지만, 폴더 정리 시 참고한다.
- 01·02·04의 diff에는 `Preview/PrvText.txt`가 변경 항목으로 나온다. 적용 결과의 `stale`에는 `thumbnail: true`가 있다. 썸네일 파일 자체를 확인하지는 않았다.
- 이 보고서는 엔진 코드, 시험, fixture, git 상태를 건드리지 않았다. 새로 만든 것은 이 문서와 `spikes/work/stage4-e2e/`뿐이다.

## 추가: 산출물의 한컴 열기 확인 (2026-10-17)

위 실행이 만든 산출물을 리눅스 한컴 게이트(`scripts/hancom_gate.sh`)로 확인했다.

- 입력 5종(사용자 작성 Windows 문서)과 각 문서의 `unedited/applied/undo/redo/edited-export.hwpx` 전부 **PASS(생존)** — 12개 표본, 크래시 0.
- 이는 "엔진 파이프라인이 만든 파일을 한컴이 오류 없이 연다"는 관찰이다. **내용이 올바르게 그려졌는지는 판정하지 않았다**(게이트는 생존만 본다). 화면 확인은 §5.9의 개별 검증에서 이미 한 차례 했다.
- 한컴 빌드는 2020 Linux Beta다. `windowsVerification: deferred`, `nativeComparison: not_checked`는 그대로다.

주의: 위 실행 시점의 `hae` 바이너리는 게이트 대상 코드와 같으나, 실행 중 저장소 HEAD가 `c2993b7`→`7a0978c`로 이동했다(문서 전용 커밋). 바이너리를 다시 빌드해 재실행하지는 않았다.
