# MCP 산출물 한컴 열기 확인 (2026-10-18)

작성: 2026-10-18, Haiku 5.5. 기준 HEAD `1cf8f5c`. 코드·fixture·문서 원본은 바꾸지 않았다. push 없음.
방법은 [엔진 산출물 한컴 열기 검증 §1](engine-output-open-verification-2026-10-17.md)과 [v0.2 구현 기록 Linux 한컴 관찰](v0.2-implementation-notes.md)을 따랐다.

## 0. 결론

- **MCP 산출물 2종은 Linux 한컴 2020 Beta에서 대화상자 없이 열렸고, 편집한 텍스트가 화면에 보인다.** 오류·복구 안내·외부 그림 경로 대화상자는 없었다.
- `legacy-edited`는 쪽수가 **2쪽에서 3쪽으로 늘었다**. 편집 없이 linesegarray 캐시만 지운 원본도 3쪽이다. 따라서 쪽수 변화는 텍스트 편집이 아니라 캐시 제거 뒤 한컴이 다시 조판한 결과로 본다. **3쪽의 내용(빈 쪽 여부)은 화면으로 볼 수 없어 미판정**이다.
- 표 산출물은 편집한 셀 텍스트가 보이고 쪽은 1쪽이다. 셀 폭이 좁아 글자 단위로 줄바꿈되는데, 원본 fixture도 똑같이 그렇다. 셀의 마지막 행 일부는 창 밖이라 이 캡처로는 보이지 않는다.
- 한계: 모든 실행이 첫 화면 캡처 뒤 SIGSEGV로 종료되어 **화면 1장씩만** 얻었다. 키 입력·쪽 이동이 불가하다. 이 빌드는 2020 Beta(11.20)이며 Windows 한컴오피스 2024(13.0)가 아니다. `nativeComparison`은 `not_checked`를 유지한다.

## 1. 환경

| 항목 | 값 |
|---|---|
| 한컴 | hoffice-hwp 11.20.0.989, `/tmp/opencode/hoffice-run/opt/hnc/hoffice11/Bin/hwp` (sha256 `059ee44b…`) |
| 실행 | `QT_QPA_PLATFORM=vnc:port=<N>:size=1280x1000`, `env -i`, `HOME`·`TMPDIR`를 실행마다 임시 디렉터리로 격리, `setsid`, 종료는 실행 PID만 `kill -9` |
| 캡처 | `spikes/work/hancom-oracle/vnccap.py` (sha256 `7856d0e5…`), 첫 프레임을 1280×1000 PNG로 저장. 실행 래퍼는 커밋하지 않은 `/tmp/pv/drv2/hwcap.sh` |
| MCP 서버 | `hae-mcp` 0.2.0, HEAD `1cf8f5c`에서 `cargo build --release -p hae-mcp`로 다시 빌드. sha256 `5b6275d0…`. `~/.cache/hae-sol/target/release/hae-mcp`와 SHA 동일 |
| MCP 세션 | stdio JSON-RPC, protocolVersion 2025-11-25, 입력 루트 `/tmp/pv/in`·`/tmp/pv/in2`, 출력 루트 `/tmp/pv/out`·`/tmp/pv/out2` |

## 2. 파일 SHA-256

| 역할 | 경로 | SHA-256 |
|---|---|---|
| 원본 A | `/tmp/pv/in/bodojaryo-reference.hwpx` (공개 코퍼스) | `4339d2cca34fabdf36026301f45b3c1dfd64cd0d75cebf60a78e9175793c7df3` |
| 산출물 A | `/tmp/pv/out/legacy-edited.hwpx` | `1bf8ff8e1273fc4ac6ed6e77de03dc7a2885be6b513e82a2b3b6c9860dce2a06` |
| 원본 B | `/tmp/pv/in2/multiparagraph_cell.hwpx` (`tests/fixtures/editing`, 합성) | `c819b508283199d646b854cad78fd1925146a9cf9db5fcf48682ef472d25bb3a` |
| 산출물 B | `/tmp/pv/out2/table-mcp-edited.hwpx` | `484bf0aa12662b9cba4079704a65834115b486c5dc78568e18931e417e83efde` |
| 대조군 E | `/tmp/pv/out4/control-nolineseg-orig.hwpx` (원본에서 linesegarray 64개만 제거, 텍스트 불변) | `96cd2ff7303602a2f66b0495ce893a6711c4c7141367e820b58ffb5c2b087c70` |
| 서버 바이너리 | `target/release/hae-mcp` | `5b6275d04fbb9e5b87758ae88aea6fd78dc8b5e19c70204e14cb111de3c6a581` |

산출물 B의 `document_export` 결과 SHA도 같은 값이다(`/tmp/pv/out2/table-summary.json`).

## 3. MCP 편집 기록

- **산출물 A (`legacy-edited`)**: `/tmp/pv/drv/run.py legacy`의 기존 산출물을 사용했다. 같은 스크립트를 다시 돌리려 했으나 `jsonschema`가 없어 실패했고, 기존 파일을 그대로 두었다. 편집은 `text_replace` 1회(`보도자료` → `MCP 미리보기 시험`, rev 0→1)다. 시험 요약 `legacy-summary.json`은 27개 검사가 모두 통과했다(요약의 검사 목록 기준). 이 산출물을 만든 시점의 바이너리 SHA는 기록이 없다.
- **산출물 B (표 fixture)**, 같은 세션에서 실행:
  1. `text_replace_all` `한 문단` → `MCP 전체치환 문단`: 1건 적용, rev 0→1.
  2. `cell_set_text`(2문단 셀) `paragraphs=["셀 MCP 첫째","셀 MCP 둘째 확인"]`: 적용, rev 1→2.
  3. `text_replace` `controls:"preserve"`를 표를 담은 문단에 적용: **거절**(`UNSUPPORTED_CONTENT_IN_RANGE`, `no_text_fragment`). 이 문단에는 텍스트 조각이 없다.
  4. 빈 문단 삽입: 명령 목록(`command_list`)에 문단 삽입 명령이 없다. `text.replace`는 비어 있지 않은 텍스트 삽입만 허용한다. **시험 불가**.
  5. `document_export`: 성공, `exportedVersion` 2, 서버 종료 코드 0.
- `preserve`가 적용되는 경우는 공개 코퍼스 `bodojaryo-reference` 등의 컨트롤 문단에서 `dryRun`으로만 확인했다(`applied`, `CONTROLS_PRESERVED`). 실제 적용 산출물은 만들지 않았고 한컴으로 열지도 않았다.

## 4. 판정 표

화면 판정은 첫 프레임 캡처(`.t1.png`)를 직접 읽고 한 것이다.

| 파일 | 대화상자 | 편집 텍스트 | 잘림·겹침·빈 쪽 | 판정 |
|---|---|---|---|---|
| A 산출물 `legacy-edited` | 없음 | `MCP 미리보기 시험`이 머리말 상자 안에 한 줄로 보인다 | 상자·로고 겹침 없음. 쪽 표시 `Page 1/3`(원본 `1/2`). **3쪽 내용 미확인** | 열림 통과. 빈 쪽 여부 **미판정** |
| A 원본 `bodojaryo` | 없음 | `보도자료` | 쪽 `1/2`, 겹침 없음 | 열림(기준) |
| B 산출물 표 | 없음 | 왼쪽 셀 `MCP 전체치환 문단`, 오른쪽 셀 `셀 MCP 첫째`·`셀 MCP 둘째`가 보인다. 마지막 `확인`은 창 밖이라 안 보인다 | 셀이 좁아 글자 단위 줄바꿈. 원본과 동일. 쪽 `1/1` | 열림 통과. 편집 텍스트 일부 확인 |
| B 원본 `multiparagraph_cell` | 없음 | `문단1`·`문단2`(오른쪽 셀), `한 문단`(왼쪽 셀) | 같은 줄바꿈 레이아웃, 쪽 `1/1` | 열림(기준) |
| E 대조군 (편집 없음, linesegarray만 제거) | 없음 | `보도자료` | 쪽 `1/3` | A의 쪽수 증가 원인 확인 |

캡처 판독 근거:
- A 산출물: 상단 머리말 상자 `MCP 미리보기 시험`이 상자 안에 있고, 아래 본문 제목과 겹치지 않는다. 상태 표시줄 `Page 1/3`.
- E 대조군: 같은 머리말 `보도자료`에 상태 표시줄 `Page 1/3`. 텍스트 편집이 없는데도 3쪽이 된다.
- B 산출물: 왼쪽·오른쪽 셀 모두 편집 텍스트가 있고, 상태 `Page 1/1`. 셀이 세로로 길게 이어져 창 하단에서 잘린다. 같은 문단이 원본 D에서도 같은 위치에서 잘린다.

## 5. 발견

1. **쪽수 증가의 원인**: `hae-edit`의 `LinesegPolicy`(`crates/hae-edit/src/lib.rs`의 `clear_caches`)는 편집 지점부터 구역 끝까지 최상위 문단의 `hp:linesegarray`를 지운다. 산출물 A에서 요소 64개가 26개로 줄었다. 한컴 2020 Beta는 캐시가 없으면 다시 조판하고, 이 빌드에서는 같은 원본이 2쪽에서 3쪽이 된다(E). 이 동작은 정책대로이며, 쪽수가 한컴 2024 Windows에서도 같은지는 확인하지 못했다. 엔진의 rhwp 미리보기는 같은 산출물을 2쪽으로 계산한다(`legacy-summary.json`의 `preview_after.pageCount` 2). 두 렌더러가 다르므로, 쪽 구성은 Windows 한컴에서 직접 확인해야 한다.
2. **VNC 종료**: 캡처한 실행 전부(산출물 A·B, 원본 A·B, 대조군 E, 높이 재시도 1회) 첫 프레임 뒤 `Segmentation fault`(rc 139)로 종료했다. 원본 `bodojaryo`도 같은 방식으로 죽으므로 [엔진 산출물 검증 §5.8](engine-output-open-verification-2026-10-17.md)의 정정대로 VNC 플러그인 쪽 문제다. 문서 탓이 아니다. 첫 프레임은 종료 전에 저장되므로 캡처는 유효하다. 다만 키 입력·쪽 이동이 불가해 3쪽을 볼 수 없었다.
3. `1280×1500`으로 높이를 늘려도 창 자체의 높이는 그대로였다(`mcp-2026-10-18-C2-table-edited-tall.t1.png`). 창 밖 행은 이 방법으로 볼 수 없다.

## 6. 한계

- 2020 Beta Linux(11.20.0.989) 한 빌드에서만 관찰했다. Windows 한컴오피스 2024(13.0)에서의 결과가 아니다. 따라서 `nativeComparison`은 `not_checked`로 둔다.
- 화면은 첫 프레임 한 장이다. 창 밖 영역과 3쪽 내용은 판정하지 못했다.
- 창은 프레임(1280 px)보다 넓어서 우측 툴바가 잘려 보인다. 문서 잘림이 아니라 캡처 크기 때문이다.
- 표 입력은 `tests/fixtures/editing/multiparagraph_cell.hwpx`다. 이 파일의 README에 따르면 합성 자료이며 공개 코퍼스 문서가 아니다. 셀 폭이 좁게 설계되어 있다.
- 산출물 A는 이전 실행의 결과를 그대로 썼다. 그 실행의 바이너리 SHA는 기록이 없다.
- 빈 문단 삽입은 명령이 없어 시험하지 못했다. `preserve`는 거절 1건과 dry-run 1건만 확인했고 실제 적용은 하지 않았다.

## 7. 캡처 (git 무시 경로)

`spikes/work/hancom-oracle/images/` (`spikes/work/`는 `.gitignore` 대상이므로 PNG는 커밋하지 않는다)

| 파일 | SHA-256 (앞 8자리) |
|---|---|
| `mcp-2026-10-18-A-legacy-edited.t1.png` | `639ad95a` |
| `mcp-2026-10-18-B-bodojaryo-orig.t1.png` | `01a4a4f5` |
| `mcp-2026-10-18-C-table-edited.t1.png` | `d40c7b7d` |
| `mcp-2026-10-18-C2-table-edited-tall.t1.png` | `c38a6de9` |
| `mcp-2026-10-18-D-table-orig.t1.png` | `fa89ce6e` |
| `mcp-2026-10-18-E-control-nolineseg-orig.t1.png` | `3daee823` |

## 8. 다음 확인 (미해결)

- 산출물 A의 3쪽이 빈 쪽인지, 본문이 넘친 것인지: Windows 한컴 2024에서 열어 확인해야 한다.
- 캐시 제거로 생기는 재조판 차이를 엔진의 rhwp 미리보기와 맞출지 결정이 필요하다.

## 9. 보완 (2026-10-18, Haiku 5.5)

앞 절의 미완 항목(빈 문단 삽입, `controls:"preserve"` 적용, 3쪽 판정)을 이어서 확인했다. 제품 코드·fixture·기존 절은 바꾸지 않았다. 입력은 `tests/fixtures/editing/`(합성)과 기존 공개 코퍼스 산출물이다. 바이너리는 `~/.cache/hae-sol/target/release/hae-mcp`(sha256 `5b6275d0…`, 저장소 `target/release`와 동일)다.

### 9.1 결론

- **정정 1.** 빈 문단 삽입은 가능하다. 명령을 따로 찾지 않고 `text_replace`에 길이 0 범위 `{start:0,end:0}`을 주면 된다(설계서 0.2 §2). 적용·내보내기·Undo가 모두 성공했다.
- **정정 2.** `controls:"preserve"`는 실제 적용된다. 수식과 탭·줄바꿈을 가로지르는 범위에서 개체 수와 순서가 그대로였다. 앞 절의 "실제 적용 산출물은 만들지 않았다"는 이 보완으로 대체된다.
- **정정 3.** 앞 절 §0의 "키 입력·쪽 이동 불가"는 틀렸다. VNC 키 이벤트(PageDown)는 반영된다. 다만 프로세스는 여전히 나중에 SIGSEGV로 끝난다.
- 3쪽 빈 쪽 여부는 **미확정**이다(§9.4). 한컴 CLI 인쇄·PDF 변환 옵션은 없다.
- `nativeComparison`은 계속 `not_checked`다. 이 빌드는 Linux 한컴 2020 Beta(11.20.0.989)다.

### 9.2 A. MCP 편집 (실제 stdio 세션)

| 항목 | 입력 (fixture, 문단) | 편집 | 결과 | 개체 보존 | Undo 후 바이트 |
|---|---|---|---|---|---|
| A1 빈 문단 삽입 | `simple_cell.hwpx`, 빈 셀 문단 (`modes:["insert"]`, textSpans `[0,0]`) | `text_replace` 범위 `{0,0}`, `"MCP 삽입 확인"`, controls=reject | applied, rev 0→1. 내보내기 성공 | 표 1개, 셀 문단 순서 불변 | 원본과 동일 (`12d7a4e2…`) |
| A2 수식을 가로지르는 보존 교체 | `equation.hwpx`, `"앞 ￼ 뒤"` | 범위 `[1,4)`, `"MCP 보존 교체 확인"`, controls=preserve | applied, `CONTROLS_PRESERVED` count 1. 결과 `앞MCP 보존 교체 확인￼뒤` | 수식 1개, 같은 위치 | 원본과 동일 (`4c2b3009…`) |
| A3 탭·줄바꿈을 가로지르는 보존 교체 | `tabs_control.hwpx`, `"a<탭>b<줄바꿈>c"` | 범위 `[0,3)`, `"MCP 탭 교체"`, preserve | applied, `CONTROLS_PRESERVED` count 1. 결과 `MCP 탭 교체<탭/><줄바꿈/>c` | 탭 1개, 줄바꿈 1개, 순서 불변 | 원본과 동일 (`f2748507…`) |

- 편집 산출물 SHA-256: A1 `604cda8d…`, A2 `bd0d1f49…`, A3 `8775a18e…`. 경로는 `/tmp/pv/out5/`(저장소 밖)이다. 요약은 `/tmp/pv/out5/summary-a.json`이다.
- 개체 비교는 `section0.xml`의 개체 요소 순서와 `hp:t` 텍스트 비교로 했다(`/tmp/pv/drv3/xmlcmp.py`).
- 세 경우 모두 dryRun → 적용 → 내보내기 → `history_undo` → 내보내기 순서다. 마지막 내보내기는 원본 바이트와 같았다.

### 9.3 B. Linux 한컴 열기 (`engine-output-open-verification-2026-10-17.md` §1 절차)

`vnccap` 첫 프레임 캡처다. 대조군은 같은 fixture의 원본이다. 모든 실행은 첫 프레임 뒤 SIGSEGV(rc 139)로 끝났다(앞 절과 같은 현상).

| 파일 | 대화상자 | 편집 텍스트 | 개체 | 판정 |
|---|---|---|---|---|
| F1 A1 산출물 (대조 G1) | 없음 | 빈 셀에 `MCP 삽입 확인`이 글자 단위로 보인다. 셀이 좁아 마지막 글자 아랫부분이 잘린다 | 2열 표 구조 유지 | 열림 통과. 삽입 텍스트 확인 |
| F2 A2 산출물 (대조 G2) | 없음 | `앞MCP 보존 교체 확인뒤` | 수식 조각(`a`, `2+b`, `2-c`)이 G2와 같은 위치에 있다 | 열림 통과. 개체 유지(화면 기준) |
| F3 A3 산출물 (대조 G3) | 없음 | `MCP 탭 교체` | G3에서 떨어져 보이던 `b`가 사라졌고 줄바꿈 뒤 `c`, `zw`, `mn`이 유지된다. 탭 자리는 화면에 표시되지 않는다 | 열림 통과. 개체 유지는 XML로 확인 |

- 이미지: `spikes/work/hancom-oracle/images/mcp-2026-10-18-F{1,2,3}-*.t1.png`, 대조 `mcp-2026-10-18-G{1,2,3}-*.t1.png`. `spikes/work/`는 `.gitignore` 대상이다.

### 9.4 C. legacy-edited.hwpx 3쪽 판정 (시간 상자 30분 안에서 시도)

- **① 명령줄 인쇄·PDF 변환: 없음.** `hwp --help`는 `Unknown option 'help'`로 끝난다. PDF 저장·인쇄(`SaveAsPDFDlg`, `PrintToPDF`)는 GUI 기능뿐이다.
- **② VNC 키 이벤트: 부분 성공.** 작은 RFB 클라이언트(`/tmp/pv/drv3/vnckeys.py`)로 PageDown을 보내면 화면이 넘어간다(`mcp-2026-10-18-H2`, `H3`, `H4`). 쪽 번호 표시는 `Page 2/3`까지 확인했다.
  - H3 마지막 프레임(PageDown 12회): 하단에 `- 3 -` 쪽 번호가 있고, 그 위 본문 영역은 비어 있다.
  - H4(PageDown 12회 후 PageUp 1회): 쪽 경계와 쪽 번호가 없는 흰 면만 보인다. 3쪽 상단인지 확정하지 못했다.
  - 50% 확대 시도(H5)는 실패했다. 배율 입력이 적용되지 않았고 쪽 전체 보기도 얻지 못했다.
  - 판정: **3쪽의 하단 본문은 비어 있음(관찰). 빈 쪽 여부는 불가(상단 미확인)**.
- **③ 쪽 나눔 요인 비교 (`/tmp/pv/drv3/pagefactors.py`): 추정.**

| 항목 | 원본 `bodojaryo-reference` | 편집본 `legacy-edited` | 대조 E (편집 없이 `linesegarray`만 제거) |
|---|---|---|---|
| 쪽 크기·여백 (`pagePr`, 상하 4251) | 59528×84188 (A4) | 같음 | 같음 |
| 본문 최상위 문단 / 표 | 37 / 5 | 37 / 5 | 37 / 5 |
| `pageBreak`·`columnBreak` | 0 | 0 | 0 |
| 끝의 빈 최상위 문단 | 0 | 0 | 0 |
| `linesegarray` | 64 | 26 | 0 |
| Hancom 쪽수 (앞 절) | 2 | 3 | 3 |

  - 쪽 나눔을 강제하는 요소는 없다. 쪽수 증가는 텍스트 편집이 아니라 캐시 제거 뒤 재조판의 결과다(E에서도 3쪽).
  - 추정: 2쪽 하단의 표 두 개(담당 부서, 로고)가 본문 끝 근처에 있어 재조판 때 높이가 조금 늘면 뒤 내용이 3쪽으로 넘어간다. 3쪽에 실제 내용이 남았는지는 확인하지 못했다.

### 9.5 한계

- Linux 한컴 2020 Beta 한 빌드에서만 관찰했다. Windows 한컴 2024(13.0) 결과가 아니다.
- 화면 캡처는 쪽 전체를 보여 주지 못했다. 빈 쪽 여부와 개체의 세부 모양은 기준선 비교로만 판단했다.
- A1 삽입 텍스트의 마지막 글자는 셀 폭 때문에 완전히 보이지 않는다. 셀 폭은 합성 fixture 설계에 따른 것이다.
- 개체 전용 문단 삽입(설계서 §7-1)과 `text.replaceAll`은 이번 범위가 아니므로 시험하지 않았다.

### 9.6 다음 확인

- Windows 한컴 2024에서 `legacy-edited.hwpx`의 3쪽 내용 확인(최종 단계, `nativeComparison` 판정 대상).
- 쪽 전체 보기 캡처를 얻는 방법을 찾아 3쪽 상단 빈 상태를 확정한다.
