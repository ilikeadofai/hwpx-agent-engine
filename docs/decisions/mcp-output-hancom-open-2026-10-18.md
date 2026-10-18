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
