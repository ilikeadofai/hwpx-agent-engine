# HWPX 엔진 0.3 설계서

작성일: 2026-10-18. 상태: **현행 구현에 맞춘 계약 확정본**. 규범 언어는 한국어다.
기준: [0.1 설계서](2026-10-16-hwpx-engine-0.1-design.md),
[0.2 설계서](2026-10-18-hwpx-engine-0.2-design.md),
[독립 계약 검토](../../decisions/0.3-contract-review.md),
[표 구조 하위 설계](../../decisions/table-structure-node-creation-design.md).
검토 기준 `df70009` 뒤의 축별 치수 수정 `ed03b01`과 후속 capability·수식 PrvText 보완을 포함한다.
여기서 0.3은 기능 계약의 이름이다. Cargo 패키지·레지스트리의 배포 버전 문자열은 현재 0.2.0이며,
이 문서 작성만으로 0.3.0 배포를 주장하지 않는다.

## 1. 목적과 성공 기준

기존 문서의 구조를 보존하는 0.1·0.2 편집에 글자 서식, 기존 수식·필드·머리말,
쪽 설정, 표 치수·셀 서식, 표 행·열 구조 편집과 조회를 더한다. 원문 바이트 보존,
무편집 내보내기 바이트 동일, 원자적 commit, 정확한 Undo/Redo, 독립 검증은 계속 적용한다.
새 본문 식별 노드를 만드는 범위는 기존 표 안의 셀과 문단으로 한정한다.

레지스트리에는 명령 57개가 있다. 이 설계에서 확정하는 신규 명령은 §2의 34개이며,
공통·기존 명령 23개는 §3에 모두 적는다. 명령 개수, 시험 통과, 렌더링 성공,
Windows 한컴 열기·저장·재열기는 각각 별도의 상태다. 명령이 등록되어 있다는 사실은
어떤 문서·대상·인자에서나 실행할 수 있다는 뜻이 아니다. 지원 여부는 §9에 따른다.

## 2. 신규 명령 34개의 계약

다음 조회 명령은 본문·ID·캐시·미리보기·revision·history를 바꾸지 않는다.
페이지가 있는 조회는 명령의 schema가 정한 scope·limit·cursor를 받으며, 잘못된 cursor,
다른 scope 또는 변경된 revision의 cursor를 받아들이지 않는다. 없는 속성을 추정해 채우지 않는다.

| 명령 | 확정 동작 |
|---|---|
| `document.stats` | 구조 요소 수와 문자 수 등 현재 문서의 요약 통계를 반환한다. |
| `section.list` | 구역과 현재 XML의 용지·여백·단 설정을 반환한다. |
| `note.list` | 기존 각주·미주의 번호, 본문 문단과 참조를 조회한다. 각주를 생성하거나 편집하지 않는다. |
| `equation.list` | 기존 수식의 nodeId·포함 문단·디코딩 script·원문 scriptRaw·크기·속성을 조회한다. |
| `equation.get` | 하나의 수식과 포함 문단 미리보기를 반환한다. scriptRaw의 엔티티 표기를 정규화하지 않는다. |
| `link.list` | HYPERLINK 필드의 원문 매개변수·표시 텍스트·내부/외부 대상을 조회한다. 링크를 따라 외부 파일을 읽지 않는다. |
| `trackchange.list` | 변경 추적 표식과 작성자 정보를 조회한다. 변경 추적 문서의 편집 금지는 해제하지 않는다. |
| `bookmark.list` | 기존 bookmark 제어와 BOOKMARK 필드의 이름·위치·원문 매개변수를 조회한다. 이름이 없으면 그대로 둔다. |
| `memo.list` | 기존 MEMO 필드·memo 제어의 ID·작성자·시간·앵커·본문을 조회한다. 없는 값은 null이다. |
| `section.getPage` | 지정한 구역의 기존 쪽 설정을 조회한다. scope 또는 sectionIndex로 지정하며 생략하면 첫 구역이다. |
| `style.list` | header.xml의 기존 스타일·문단 모양·글자 모양·borderFill 정의를 종류별로 조회한다. |
| `table.get` | 표의 행·열·실제 기준 셀·병합·문단·치수 정보를 조회한다. 병합에 가려진 좌표를 새 셀로 만들지 않는다. |
| `picture.list` | 기존 그림과 포함 위치·크기·미디어 참조를 조회한다. 외부 미디어는 읽거나 내장 미디어로 가장하지 않는다. |
| `field.list` | 필드의 이름·ID·원문 Command·현재 값·채움 가능 여부를 조회한다. 불명확하거나 불투명한 필드는 사유를 남긴다. |
| `document.exportText` | 현재 문서를 Markdown 또는 HTML 새 파일로 내보낸다. scope는 구역, includeNotes 기본값은 true다. 원본 HWPX와 이력을 바꾸지 않으며 조판 보존 형식으로 취급하지 않는다. |

`bookmark.list`·`memo.list`는 `{documentId, revision, bookmarks|memos, total, nextCursor}`를
반환한다. 두 명령의 MCP hint는 readOnly=true, idempotent=true, destructive=false,
openWorld=false다. `document.exportText`는 출력 파일을 만드는 동작이므로 일반 읽기 조회와 같은
readOnly hint를 광고하지 않는다. 출력 경로는 output-root 안의 새 파일이어야 한다.

나머지 신규 19개는 편집 명령이다. 이들 모두 §4~§9의 공통 게이트를 거친다.

| 명령 | 확정 동작과 거절 경계 |
|---|---|
| `equation.setScript` | 기존의 유일한 일반 텍스트 script만 XML 이스케이프해 교체한다. 빈 요청, 불균형 중괄호, 역슬래시·따옴표·제어문자·주석/PI/CDATA 표기는 거절한다. 기존 빈 script 요소는 확장할 수 있다. LaTeX/MathML 변환과 수식 생성은 하지 않는다. |
| `equation.setProperties` | props 중 baseLine·textColor(#RRGGBB)·font·양수 baseUnit·lineMode(CHAR/LINE)·version만 바꾼다. 없는 요청 속성은 기존 수식 시작 태그에 추가할 수 있다. script·size·위치·주석은 보존한다. 빈 props는 거절한다. |
| `table.setColumnWidths` | widths 배열 또는 균일 width 하나로 열 폭을 정한다. 배열 길이는 열 수와 같아야 한다. 병합 셀 폭은 걸친 열 폭의 합이다. table sz.width도 합으로 갱신하고 높이는 보존한다. |
| `table.setRowHeights` | heights 배열 또는 균일 height 하나로 행의 셀 높이를 정한다. 병합 셀 높이는 걸친 행 높이의 합이며 table sz.height도 새 합으로 정한다. 폭은 보존한다. |
| `cell.setFormat` | 기존 borderFillId·background·cellMargin만 바꾼다. 배경은 동일 경계선과 요청 색을 가진 기존 borderFill이 있어야 한다. 새 borderFill을 만들지 않는다. 여백은 hasMargin을 켜며 모든 유효 여백이 셀 크기보다 작아야 한다. |
| `cell.setSize` | 병합되지 않은 실제 기준 셀의 width 또는 height 또는 둘을 받는다. 해당 열·행 전체에 일관된 크기를 적용한다. 직접 병합 셀 크기 변경은 거절하고 표 track 명령을 사용한다. |
| `table.insertRow` | index에 count개 빈 행을 삽입한다. count 기본값은 1이다. 단순한 이웃 행을 템플릿으로 삼고 셀마다 빈 문단 하나를 만든다. |
| `table.deleteRow` | index부터 count개 행을 삭제한다. 표를 비우거나 병합 셀과 겹치거나 삭제 셀에 지원 밖 개체가 있으면 거절한다. |
| `table.insertColumn` | index에 count개 빈 열을 삽입한다. 이웃 단순 열의 서식과 폭을 재사용한다. 병합 셀 내부를 가르는 삽입은 거절한다. |
| `table.deleteColumn` | index부터 count개 열을 삭제한다. 표 또는 행을 비우거나 병합·지원 밖 삭제 내용이 있으면 거절한다. |
| `section.setPage` | 기존 pagePr의 용지 방향·폭·높이·제본 방식·여백 중 요청한 값만 바꾼다. 지원 enum·양수 치수·쪽 안의 유효 본문 공간을 검사한다. 새 구역이나 pagePr을 만들지 않는다. |
| `section.setPageNumbering` | 기존 startNum의 pageStartsOn(BOTH/EVEN/ODD)과 page 중 요청값만 바꾼다. 번호 필드나 새 제어를 만들지 않는다. |
| `section.setVisibility` | 기존 visibility의 첫 머리말·꼬리말·쪽번호·빈줄 숨김, 줄번호 표시, 테두리·채우기 표시 속성 중 요청값만 바꾼다. |
| `text.format` | 한 문단의 지원 텍스트 범위에 props의 bold·italic·underline·strikeout·heightHwpunit(100~100000)·textColor(#RRGGBB)만 적용한다. 필요하면 run을 나누되 식별 노드·개체·무관한 글자를 보존한다. fontRef 등 지원 밖 글꼴 자원 변경은 거절한다. |
| `field.setText` | 지정 문단 안의 CLICK_HERE/CLICKHERE/HYPERLINK 결과 텍스트를 채운다. name·fieldid·occurrence로 유일하게 고른다. 표식·Command·기존 run 서식은 보존하고, 모호한 선택과 제어·불투명 내용은 거절한다. |
| `form.fill` | values의 각 라벨과 정확히 일치하는 라벨 셀의 오른쪽 빈 값 셀, 없으면 아래 빈 값 셀을 찾는다. scope로 범위를 제한한다. strict 기본 true는 라벨 하나라도 실패하면 전부 거절하고, false는 해석 가능한 라벨만 적용하며 건너뛴 사유를 보고한다. 실제 채움은 cell.setText의 기존 문단·보존 규칙을 따른다. |
| `paragraph.setStyle` | target 또는 targets 문단들의 styleIDRef를 기존 styleId로 바꾼다. 스타일 정의를 복제하지 않으며 paraPrIDRef·charPrIDRef를 다시 적용하지 않는다. 대상 스타일 부재·중복은 거절한다. |
| `image.fit` | 기존 크기를 기준으로 contain은 표시 크기를 비율에 맞춰 줄이고, cover는 가운데 crop을 정하며, stretch는 기준 크기로 채운다. curSz·sz·imgDim·imgClip·imgRect와 scaMatrix를 필요한 만큼 바꾸고 미디어 바이트·transMatrix·rotMatrix는 보존한다. 완전한 기하 속성·유한 행렬·해석 가능한 내장 정적 이미지가 필요하다. |
| `header.setText` | 지정 구역의 기존 header/footer 직접 subList 문단 중 paragraphIndex(기본 0)의 텍스트를 바꾼다. scope 생략 시 첫 구역이다. 기존 텍스트 carrier를 재사용하고 필드·번호·다른 제어·중첩 표가 있으면 거절한다. 머리말/꼬리말을 새로 만들지 않는다. |

표 track 치수는 양수 signed HWPUNIT이며 합은 2147483647 이하여야 한다.
cellSpacing=0, 1~1024 행·열, 완전하고 겹치지 않는 격자, 지원하는 기존 속성이 필요하다.
수정하는 축의 RelTo만 ABSOLUTE여야 하며 그 축의 기존 셀 치수를 검증한다.
**수정하지 않는 축의 합 불일치 또는 상대 크기는 해당 축 편집을 막지 않는다.**
기존 table sz.height는 배치 높이여서 셀 최소 높이 합과 달라도 정상이다. 행 축 복원은
셀 높이에서 계산하고 table sz.height를 합의 선행조건으로 쓰지 않는다.
행 삽입·삭제는 기존 배치 높이에 행 높이 차분을 적용한다. 열 삽입·삭제는 폭 차분을 적용한다.
구조 편집은 cellzoneList·중첩 표·지원 밖 템플릿을 추가로 거절한다.

## 3. 공통·기존 명령 23개의 계약

다음 명령에도 이전 설계의 보존 규칙을 적용한다. HWP5는 별도 파일의 읽기 검사만 제공한다.

| 명령 | 확정 동작 |
|---|---|
| `command.list` | 현재 레지스트리 57개 명령과 schema·정책·설정상 availability를 반환한다. |
| `command.describe` | 하나의 명령 schema·버전·dryRun·history 정책을 반환한다. |
| `document.open` | input-root 안의 HWPX 또는 지원 프로젝트를 검증해 연다. 암호화·손상 패키지는 거절한다. |
| `document.inspect` | 현재 revision의 구조 outline과 대상별 편집 가능 사유를 조회한다. |
| `document.close` | 열린 문서 세션을 닫는다. HWPX를 자동 덮어쓰지 않는다. |
| `element.query` | kind·범위·텍스트·셀 주소 등 조건에 맞는 노드와 명령별 capability를 반환한다. |
| `element.get` | nodeId의 outline·텍스트·원문·capability를 조회한다. |
| `text.replace` | 0.2의 grapheme 범위 교체·위치 삽입·controls 규칙을 적용한다. preserve도 필드 경계를 통과하지 못한다. |
| `text.replaceAll` | scope 안 정확한 일치를 겹치지 않게 고르고 지원 범위만 교체한다. 건너뜀과 maxMatches 초과를 구분하며 전체는 한 Undo 단위다. |
| `paragraph.format` | 정렬·백분율 줄간격을 기존 모양 재사용 또는 단일 복제로 적용한다. |
| `cell.setText` | 기존 셀 문단의 텍스트를 바꾼다. 다문단은 정확히 같은 길이의 paragraphs 배열을 받고 문단 수를 바꾸지 않는다. |
| `image.replace` | 검증해 고정한 정적 PNG/JPEG를 새 미디어로 추가하고 기존 그림 참조만 바꾼다. 위치·프레임은 보존한다. |
| `command.batch` | 지원 편집 단계들을 순서대로 계획·검증하고 원자적으로 한 revision·Undo 단위에 commit한다. |
| `history.undo` | 마지막 편집 전 전체 패키지와 node map을 정확히 복원하고 revision은 증가한다. |
| `history.redo` | 마지막 Undo의 편집 후 패키지·생성 ID를 복원하고 revision은 증가한다. 이미지 원본 경로를 다시 읽지 않는다. |
| `history.clear` | Undo/Redo 이력을 비운다. 문서 내용을 바꾸지 않는다. |
| `document.snapshot` | 현재 revision의 명시적 스냅샷을 만든다. |
| `snapshot.release` | 지정 스냅샷을 해제한다. 현재 문서를 편집하지 않는다. |
| `document.diff` | 지정 상태 사이의 변경을 조회한다. 구조 변경은 nodeId 기준 추가·삭제·이동으로 구분한다. |
| `document.preview` | 설정된 격리 rhwp로 현재 revision을 렌더링한다. 문서·history를 바꾸지 않고 renderer·font·입력 hash와 nativeComparison=not_checked를 반환한다. 렌더러 미설정은 RENDERER_UNAVAILABLE다. |
| `project.save` | 현재 상태·ID·이력을 지원 프로젝트 형식의 새 출력에 저장한다. |
| `document.export` | 현재 HWPX를 새 출력에 저장한다. 무편집 상태는 원본 archive 바이트를 그대로 반환한다. |
| `hwp5.inspect` | input-root의 HWP 5.0 파일을 직접 읽어 파일 헤더와 문단 텍스트를 제한된 페이지로 조회한다. 열린 HWPX·revision·이력을 바꾸지 않고 HWP5 편집·저장은 제공하지 않는다. |

## 4. 구조 변경과 nodeId

일반 편집은 기존 식별 노드 집합과 nodeId를 유지한다. run·t·속성·공유 서식의 변화는
새 Paragraph/Table/Cell/Picture/Equation 식별 노드 생성으로 취급하지 않는다.

표 구조 명령만 TableRemap으로 삭제·이동을 선언하고 기존 표 안의 Cell·Paragraph를 만들거나
지운다. 선언과 후보 트리의 대응은 정확히 하나씩이어야 한다. 사상 밖의 구조 변경은
STATE_INVARIANT_ERROR이며 commit하지 않는다. 표 바깥의 ID는 유지한다.

새 셀·문단은 새 ID를 받는다. 삭제한 ID를 현재 트리에 다시 쓰지 않는다. Undo는 삭제 전
노드와 ID를 되살리고 Redo는 삽입 때 확정한 동일 ID를 복원한다. dryRun의 응답은 생성 ID를
확정 결과처럼 노출하지 않으며 세션의 ID map·revision·history를 바꾸지 않는다.
같은 batch의 뒤 단계가 기존 노드를 가리키면 앞 구조 단계의 사상을 적용해 대상을 찾는다.
생성 예정 노드를 dryRun ID로 뒤 단계에 지정하는 계약은 제공하지 않는다.

## 5. 줄 배치 캐시와 PrvText

실제 변화가 없는 noop은 캐시·preview·revision·history를 그대로 둔다.
실제 편집의 linesegarray 제거 범위는 다음과 같다.

- 일반 텍스트·글자/문단 서식·스타일·수식·그림 맞춤·필드·머리말 편집: 영향을 받은 문단을
  담은 최상위 문단부터 해당 구역 끝까지의 최상위 문단 캐시와, 직접 영향받은 중첩 문단 캐시.
- cell.setText·form.fill: 셀을 담은 최상위 carrier부터 구역 끝과 실제 채운 셀 문단들.
  form.fill이라는 이유만으로 표 안 무관한 모든 중첩 문단 캐시를 지우지는 않는다.
- 표 치수·cell.setSize·cell.setFormat·행열 구조: 표 carrier부터 구역 끝과 해당 표의 모든 문단.
- section.setPage·setPageNumbering·setVisibility: 해당 구역의 모든 문단(중첩 포함).
- image.replace: 프레임을 유지하는 미디어 교체이므로 기존 규칙대로 줄 배치 캐시를 제거하지 않는다.

편집 응답은 cacheRemovals와 stale.layoutSections를 보고한다. 렌더링은 패키지의 캐시를
다시 생성해 저장하지 않는다. 썸네일이 있는 문서의 실제 편집은 썸네일 바이트를 유지하고 stale로 표시한다.

PrvText를 전체 문서에서 다시 생성하지 않는다. 기존 항목이 없으면 새로 만들지 않는다.
UTF-8 PrvText에서는 단계 순서대로 다음 규칙을 적용한다. 비교 문자열의 탭·줄바꿈은 공백으로 바꾼다.
이전/이후 문자열이 같으면 치환하지 않는다. 이전 문자열이 공백뿐이 아니고 현재 PrvText 안에서
정확히 한 번 나오면 그 한 곳만 새 문자열로 바꾼다. 없거나 중복되거나 빈 이전 문자열이면
해당 치환은 생략하고 stale.prvText=true로 표시한다. 다른 성공한 치환은 유지한다.
UTF-8이 아니면 바이트를 유지하며 실제 편집이 있으면 stale이다.

| 편집 | PrvText 치환 단위와 stale 규칙 |
|---|---|
| text.replace·replaceAll·cell.setText | 기존 문단 전체 텍스트 → 새 문단 전체 텍스트에 위 유일성 규칙을 적용한다. |
| header.setText | 선택한 기존 머리말/꼬리말 문단 전체 텍스트에 같은 규칙을 적용한다. |
| field.setText | 필드 값만이 아닌 포함 문단의 이전/이후 텍스트에 같은 규칙을 적용한다. |
| form.fill | 실제 채운 기존 셀 문단마다 같은 규칙을 적용한다. 빈 값 셀 채움은 이전 문구가 비어 있어 stale일 수 있다. |
| equation.setScript | 문단 모델의 개체 자리표시자 대신 **기존 script의 디코딩된 문자 전체 → 새 script 전체**를 치환 단위로 쓴다. 수식 XML 엔티티 표기가 PrvText에 그대로 있다는 가정을 하지 않는다. old script가 없거나 중복·빈 값이면 바이트를 유지하고 stale이다. |
| equation.setProperties | script를 바꾸지 않으므로 UTF-8 PrvText를 그대로 유지한다. baseLine·font·색·단위 등의 변화로 새 텍스트를 추정하지 않는다. |
| table.insertRow·insertColumn | 빈 셀만 추가하므로 PrvText를 유지한다. |
| table.deleteRow·deleteColumn | 삭제 셀에 텍스트가 있으면 PrvText를 고치지 않고 stale로 표시한다. 없으면 유지한다. |
| text.format·paragraph.format·setStyle·section 편집·cell.setFormat·setSize·표 치수·image.fit·replace | 텍스트 치환을 만들지 않고 UTF-8 PrvText를 유지한다. |

수식이 PrvText에 생략되거나 렌더링된 다른 기호로 기록된 경우에는 식을 추정 변환하지 않는다.
스크립트 유일 일치 규칙은 확인 가능한 문자열 보존 계약이며 Hancom의 모든 수식 projection을
재현한다는 뜻이 아니다. 엔진과 hae-verify는 stale 판정도 각자 계산하고, Python oracle은
지원하는 요청의 prvTextStale를 별도 보고한다.

## 6. Undo·dryRun·batch·버전

실제 변화가 있는 단일 편집 또는 command.batch 한 묶음이 한 revision·한 HistoryUnit이다.
예상 revision은 계획 전에 확인하고 commit 경계에서도 세션 상태가 맞아야 한다.
dryRun은 같은 계획과 독립 검증을 수행하되 현재 패키지·node map·revision·history를 바꾸지 않는다.
실패한 단일 명령과 batch는 앞 단계까지 포함해 아무 변화도 commit하지 않는다.

Undo는 편집 전 archive 바이트와 ID map을, Redo는 편집 후 archive 바이트와 생성 ID를 복원한다.
내용이 원래와 같아져도 Undo/Redo는 새 revision이므로 이전 revision에 묶인 조회 cursor를 재사용하지 않는다.
이미지 파일을 포함한 외부 입력은 계획 때 고정하며 Redo에서 다시 읽지 않는다.
form.fill strict:false의 일부 라벨 적용은 명시적으로 요청한 동작이며 실패한 batch의 부분 commit과 다르다.

## 7. header.xml 자원 재사용과 itemCnt

text.format은 대상 글자 모양을 복제해 요청 속성만 바꾸고 동등한 charPr이 있으면 재사용한다.
없으면 필요한 새 정의를 추가하고 charProperties.itemCnt를 실제 자식 수와 맞춘다.
paragraph.format도 같은 원칙으로 paraPr을 재사용하거나 복제하고 paraProperties.itemCnt를 맞춘다.
기존 공유 정의를 제자리에서 바꾸어 다른 문단·run에 서식이 번지게 하지 않는다.

paragraph.setStyle은 기존 style 참조만 바꾼다. cell.setFormat은 기존 borderFill을 참조하며
배경까지 맞는 정의가 없으면 background_resource_unavailable로 거절한다. 표 치수·구조·쪽 설정·
머리말·수식·필드·form.fill·image.fit은 header 자원을 추가하지 않는다. 구조 삽입의 새 셀은
템플릿의 기존 참조를 재사용한다. 자원이 추가되지 않으면 해당 collection의 itemCnt도 유지한다.
image.replace의 새 미디어는 manifest/BinData 변경이며 header 서식 복제로 취급하지 않는다.
검증기는 기존 정의 재사용 여부, 새 정의의 요청 속성, 정확한 itemCnt, 무관한 정의 불변을 따로 확인한다.

## 8. 독립 검증 요건

모든 편집 후보는 저장·commit 전에 hae-verify를 통과해야 한다. 검증기는 원본 PackageState와
선언적 요청만으로 허용 XML·캐시·preview·자원 변경을 별도 계산한다. hae-edit·hae-model·hae-exec
의존, 편집 planner 재사용, 편집기가 만든 expected 값의 무조건 수용을 금지한다.
hae-package·hae-xml의 패키지/토큰 파서는 공유한다. 신규 미디어·자원 ID의 후보 선택을 검사하더라도
해당 bytes·참조·유일성·기존 자원 동등성은 원본과 요청으로 확인한다.

구조 사상·ID 보존은 모델 invariant와 executor 이력 시험으로 검사하고, 검증기는 사상에 따른
XML 위치·내용을 독립 검증한다. 검증기가 모델의 랜덤 nodeId를 별도로 생성해 비교한다는 뜻은 아니다.
수식의 script·속성·캐시·PrvText, 무관 속성·엔티티 표기·개체·미디어·ZIP 압축 바이트의 변조 후보를
긍정 시험과 함께 거절해야 한다. Undo/Redo는 executor에서 전체 archive를 비교한다.

Python tests/oracle은 표준 라이브러리만 쓰며 Rust 편집기·검증기를 호출하지 않는다.
현재 독립 구현 범위는 text.replace·replaceAll·cell.setText·paragraph.format·image.replace와
이번에 추가한 equation.setScript·setProperties다. 다른 0.3 요청은 unknown operation으로 거절하며,
57개 명령 전체를 Python oracle이 판정한다고 보고하지 않는다. 다른 편집의 독립 게이트는 hae-verify다.

## 9. capability 광고 원칙과 현재 한계

command.list/describe의 available은 명령 등록·어댑터 설정상 가용성이다. element.query/get의
edits[command].available은 해당 대상의 소스 선행조건이다. 두 층을 혼동하지 않는다.
기본 controls=reject로 실행할 수 없는 문단·셀은 requires:{controls:"preserve"}를 함께 광고하고,
다문단 셀은 필요한 paragraphs 수를 알린다. 광고한 모드·범위는 그 필수 인자를 반영한 executor
편집+독립 검증 경로에서 성공해야 한다.

표 치수 광고는 완전한 격자와 수정할 축의 기존 치수 관계·RelTo를 검사한다. 수정하지 않는 축의
합 불일치를 거절 사유로 삼지 않는다. 수정할 축의 모순은 available=false와
reason=table_geometry_unsupported로 알린다. width만 요청한 cell.setSize와 height만 요청한
경우는 각각의 축만 검사한다. cell.setSize의 노드 광고는 둘 중 지원하는 축이 있으면 true이며,
양 축 인자 전체의 성공을 보증하지 않는다.

현재 한계는 다음과 같다. 이 한계를 모든 인자의 성공으로 해석하지 않는다.

- capability는 구조 명령의 index/count별 병합 경계·삭제 내용·템플릿 선택과 자원 색·여백·
  치수 상한 등 요청값의 적합성을 열거하지 않는다. 유효한 기본 요청의 성공을 시험하되,
  특정 병합 구간 삭제처럼 명시적으로 지원하지 않는 요청은 실행 시 사유를 담아 거절한다.
- 노드 outline에 새 명령 34개 전부의 capability가 실려 있지는 않다. 전역 등록은 registry,
  대상별 제약은 해당 query·schema·실행 진단으로 확인한다.
- Python oracle의 0.3 범위는 §8에 한정한다. 모든 가능한 batch 교차곱과 Windows 한컴
  열기·저장·재열기·재조판은 이번 Linux 시험으로 대체하지 않는다.
- PrvText 치환과 줄 배치 캐시 제거는 새 레이아웃 생성이 아니다. nativeComparison은
  별도 Windows 근거가 생기기 전까지 not_checked다.

## 10. 완료 조건과 시험 격리

1. bash scripts/check.sh 종료 0: fmt·clippy·스크립트 시험·workspace 시험을 실행한다.
   렌더러가 있으면 기존 실제 executor preview 4건과 adapter 9건(총 13건)도 실행해야 한다.
2. 렌더러가 없으면 같은 check.sh가 0으로 끝나며 이 13건을 ignored/SKIP로 명시해야 한다.
   시작·끝 출력에 렌더러 경로와 실행 여부를 남긴다. 이 녹색은 편집/검증/격리 게이트 통과이며
   렌더링 확인은 포함하지 않는다. 렌더러 오류를 성공으로 바꾸지는 않는다.
3. Python oracle 전체와 scripts/evaluate_mcp.py 전체를 실행해 실패 0을 확인한다.
   MCP 평가는 실제 stdio 서버·고정한 바이너리·실제 렌더러가 있는 환경에서 수행한다.
4. 표 명령 전체의 기본 요청을 advertised capability → dryRun → 실제 실행 경로로 시험한다.
   검토의 반대 축 재현 두 건과 수정 축 모순의 unavailable·실행 거절을 함께 시험한다.
5. 수식 PrvText의 유일·부재·중복·빈 script·엔티티·비UTF-8·누락 preview, 무관 내용 변조,
   정확 Undo/Redo를 독립적으로 시험한다. 건너뛴 시험과 실행한 시험을 따로 센다.

게이트의 실제 revision·명령·수치는 [계약 후속 검증 기록](../../decisions/0.3-contract-followup.md)에 둔다.
이 설계서는 실행하지 않은 native 호환·사용자 문서 범용 성공을 완료로 올리지 않는다.
