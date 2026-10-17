# 엔진 산출물 한컴 열기 검증 — 근본 원인과 수정 (2026-10-17)

작성: 2026-10-17. 브랜치: `wip/stage1-model-edit`.

## 0. 결론

**1차 Windows 실패(engine_inputs 01–04가 빈 화면 후 종료)의 남은 원인 하나를 찾아 고쳤다.**

- 원인: 엔진이 `image.replace`로 추가한 `content.hpf` manifest 항목에 **`isEmbeded="1"`이 없었다.**
- 한컴은 이 속성이 없는 항목을 **내장 미디어가 아니라 외부 연결**로 해석한다. 그래서 ① `Picture File` 경로 입력 대화상자를 띄우고 ② 그림 자리를 빈 채로 둔다.
- 수정: `crates/hae-edit/src/image.rs`가 추가 항목에 `isEmbeded="1"`을 쓴다. `crates/hae-verify`도 새 미디어 항목에 이 속성을 강제한다.
- 검증: 리눅스 한컴(hoffice-hwp 11.20.0.989)으로 수정 전/후 산출물을 열어 **화면 캡처로 확인**했다.

이 문서의 주장은 모두 이 환경에서 실제로 실행해 얻은 것이다. 추정은 §6에 따로 표시했다.

## 1. 검증 환경 (Windows 없이 한컴을 관찰하는 방법)

- 대상: AUR `hoffice`와는 별개로, 사용자가 받은 공식 deb `hoffice-hwp` 11.20.0.989 (한글 2020 Beta, `/opt/hnc/hoffice11`).
- 설치 없이 사용자 디렉터리에 풀어서 실행한다(`dpkg-deb` → `data.tar.xz` → 임의 경로). root 불필요.
- 디스플레이 없이 **실제 화면을 본다**:
  1. `QT_QPA_PLATFORM=vnc` 로 `hwp` 실행 → Qt VNC 플러그인이 `:5900`에 프레임버퍼 제공 (`QVncServer created on port 5900`).
  2. 최소 RFB 클라이언트(`vnccap.py`, 표준 라이브러리만)로 프레임버퍼를 받아 PNG 저장.
     - Qt의 VNC 플러그인은 **RFB 003.003(3.3)** 으로 응답한다. 3.3은 버전 교환 뒤 보안 타입을 4바이트로 보내고 None(1)이면 SecurityResult가 없다. 3.7+와 순서가 다르므로 분기해야 한다.
  3. 저장한 PNG를 그대로 읽어 대화상자 문구와 조판을 판독한다.
- 도구: `spikes/work/hancom-oracle/{probe3.sh,vnccap.py,vncshot.sh}`, 캡처 이미지 `spikes/work/hancom-oracle/images/`.
- 네트워크·홈 오염 방지: `HOME`을 임시 디렉터리로 두고 실행한다. (제품 LICENSE §7 자동업데이트, §8 번역 전송 조항이 있으므로 격리 실행이 바람직하다.)

## 2. 근본 원인

### 2.1 증상 (수정 전)

`applied-export.hwpx`(2차 평가 산출물, `image.replace` 1회 적용)를 열면:

- `Picture File` 이라는 제목의 대화상자가 뜨고 경로 칸에 `BinData/hae-image-1.png` 가 채워져 있다. 버튼은 `OK` / `Ignore` / `Ignore All`.
- 문서 본문(문단·표)은 그려지지만 **그림 자리는 빈 점선 상자**로 남는다.

캡처: `images/before-missing-isEmbeded.png`

### 2.2 원인

엔진이 추가한 manifest 항목:

```xml
<hae:item xmlns:hae="http://www.idpf.org/2007/opf/" id="hae-image-1"
          href="BinData/hae-image-1.png" media-type="image/png"/>
```

한컴이 저장한 파일의 항목:

```xml
<opf:item id="image1" href="BinData/image1.png" media-type="image/png" isEmbeded="1"/>
```

한컴 산출물의 모든 미디어 항목에는 **`isEmbeded="1"`** 이 있다. 우리 항목에는 없다.
ZIP 안에 `BinData/hae-image-1.png` 는 실제로 존재하고 `hc:img binaryItemIDRef="hae-image-1"` 로 참조되지만, 속성이 없으면 한컴은 내장으로 취급하지 않는다.

### 2.3 확인 (수정 후)

같은 파일에서 해당 항목에 `isEmbeded="1"`만 손으로 추가하고(`<hae:item ...>` → `<opf:item ... isEmbeded="1"/>`) 다시 열면:

- 대화상자가 **사라지고** 그림이 **정상 렌더링**된다.

캡처: `images/after-patched-isEmbeded.png`

그리고 `crates/hae-edit`를 고친 뒤 CLI로 같은 편집을 다시 수행해 만든 산출물도 대화상자 없이 열린다.

캡처: `images/engine-fixed-export.png`

## 2.4 한컴 공개 모델로 확인 (권위 근거)

한컴이 Apache-2.0으로 공개한 `hancom-io/hwpx-owpml-model`(OWPML 파일 필터 모델, 커밋 `main`)에 `isEmbeded`가 **매니페스트 item의 정식 속성**으로 정의돼 있다.

- `OWPML/Class/Etc/item.h`: `CItem`이 `id`·`href`·`media-type`·`isEmbeded` 네 속성을 갖고, `GetEmbeded()` / `SetEmbeded()`를 제공한다.
- `OWPML/Class/Etc/item.cpp`: 직렬화할 때 `L"isEmbeded"`로 쓰고, 읽을 때도 `L"isEmbeded"`를 읽는다.
- `OWPML/Class/Etc/version.h`: `tagetApplication`(철자 그대로)도 이 모델에 정의돼 있다(§5.2의 `major`·`tagetApplication` 규칙과 일치).

즉 이번 수정은 역공학 추정이 아니라 **한컴이 공개한 스키마와 일치**한다. 같은 저장소는 `OWPML/Class/Para/`에 문단 객체 클래스 268개를 두고 있어, 앞으로 시험 자료를 만들 때 요소·속성 목록의 권위 있는 기준으로 쓸 수 있다.

## 3. 수정 내용

| 파일 | 변경 |
|---|---|
| `crates/hae-edit/src/image.rs` | 추가 미디어 항목에 `isEmbeded="1"` 기록, 사유 주석 |
| `crates/hae-edit/tests/image.rs` | 추가 항목이 `isEmbeded="1"`을 갖는지 회귀 단언 |
| `crates/hae-verify/src/image.rs` | 새 미디어 항목에 `isEmbeded="1"`을 요구(게이트가 같은 부류를 잡도록) |
| `crates/hae-verify/tests/gate.rs` | `isEmbeded` 제거 후보를 거절하는 음성 시험 추가 |

기존 시험 영향 없음: `hae-edit`(editing_fixtures 1, image 13, text_format 6), `hae-verify`(gate 15, oracle_mutations 1) 모두 통과.

## 4. 2차 산출물의 상태 (구조 수정은 유효)

`spikes/work/windows-round2/engine_on_user_fixtures/01_text_edited.hwpx` 를 리눅스 한컴으로 열면 **문서가 정상적으로 그려진다**(본문 텍스트 렌더링 확인).

이때 함께 뜨는 대화상자는 오류가 아니다:

> Document was created in a higher version. Higher version documents may not display correctly in previous HWP versions.

이유: `version.xml`의 appVersion이 13(한컴오피스 2024가 만든 원본)이고, 리눅스 빌드는 11.20(2020 Beta)이다. **버전 경고**이며 열기 실패가 아니다.

캡처: `images/round2-text-version-warning.png`

## 5. 오라클 후보 신호에 대한 정정

이 조사에서 두 가지 신호가 **오라클로 부적합**함을 확인했다. 이후 자동 판정을 만들 때 이 함정을 피해야 한다.

| 신호 | 판정 | 근거 |
|---|---|---|
| stderr의 `propagateSizeHints` 개수 | **부적합** | 한컴이 재저장한 파일 9개 전부 1개를 낸다(정상 파일). 위 §4의 버전 경고 등 **정상 대화상자**도 이 메시지를 만든다. 오류 지표가 아니다 |
| `Hnc/BinData/` 추출 개수 | **조건부** | 미디어가 있는 문서에서는 유효(정상본은 전부 추출, `applied-export`는 0개). 그러나 ① 미디어 없는 문서는 신호가 없고 ② `BinData`에 폰트(`.ttf`)만 있으면 정상 문서도 0개다. "패키지 BinData 전부 추출"을 그대로 쓰면 오탐이 난다 |
| 임시 디렉터리 탐색 (`ls -dt /tmp/hnc-*`) | **부적합** | 동시 실행과 경합한다. `hwp`의 PID로 `/tmp/hnc-<user>/<pid>/`를 직접 읽어야 한다(`probe3.sh`) |

**신뢰할 수 있는 관찰은 화면 캡처다**(§1). 자동 분류가 필요하면 캡처 기반으로 만들어야 한다.

## 5.5 실사용 문서 최종 검증 (2026-10-17)

사용자가 Windows 한컴으로 직접 만든 `spikes/work/windows-return-20261017-2047/return/user_fixtures/03_picture.hwpx`를 엔진으로 편집(그림 교체)해 내보낸 뒤 리눅스 한컴으로 열었다.

- 결과: **문서가 열리고 교체한 그림이 정상 렌더링된다.** 남은 대화상자는 §4의 버전 경고뿐이다.
- 즉 §2의 `isEmbeded="1"` 수정이 합성 fixture가 아니라 **실제 한컴 저장 문서**에서도 동작한다.
- 캡처: `images/real-user-doc-fixed.png`
- 엔진이 추가한 항목:
  `<hae:item xmlns:hae="http://www.idpf.org/2007/opf/" id="hae-image-1" href="BinData/hae-image-1.png" media-type="image/png" isEmbeded="1"/>`

대조: 같은 원본의 미디어 항목은 한컴이 `isEmbeded="1"` **외에** `hashkey="..."`도 쓴다. 우리는 `hashkey`를 쓰지 않는다. 이 속성이 필수인지는 **미판정**이다(§6).

## 5.6 리눅스 한컴 오라클의 한계 (독립 조사 결과)

`spikes/work/hancom-oracle/oracle.md`(GPT 6.1 Sol)의 결론: **이 빌드에는 신뢰할 수 있는 자동 OPEN/FAIL 오라클이 없다.** 검증된 양성 신호가 없어 그 스크립트는 `OPEN`을 반환하지 않고 `FAIL`/`UNKNOWN`만 낸다.

핵심 반례:

- **본문 XML을 깨뜨린 파일과 0바이트 파일도** 파일명과 페이지 상태를 갖춘 **빈 문서로 표시**됐다. 따라서 창 제목·페이지 수·`propagateSizeHints` 개수는 성공 신호가 될 수 없다.
- `tests/fixtures/corpus/irb-sample.hwpx`는 이 리눅스 빌드에서 실제로 **"The file is corrupted."** 를 표시한다(코퍼스 기대와 불일치). Windows 실패 보고를 리눅스 관찰로 철회하지 않는다.
- 따라서 내용이 열렸는지 판정하려면 **화면 관찰 + 문서별 기대 내용 대조**가 필요하다(§1의 VNC 캡처 방식).

## 5.7 크래시하는 fixture의 출처 (오해 방지)

`spikes/work/windows-handoff-20261017-v2/engine_inputs/*`는 **수정 전 스크립트(9c0617f)** 산출물이다(`sha256 191cd49f…`, 현재 스크립트 `3b9aefb1…`와 불일치). 이 파일들은 한컴을 SIGSEGV로 죽인다(§5.8).

이 fixture는 `scripts/evaluate_mcp.py`가 **손으로 합성한** 최소 `hp:tbl`/`hp:pic`을 담고 있다. HEAD(2dec041)의 같은 스크립트는 훨씬 완전한 마크업을 쓰고, 그 산출물 `spikes/work/windows-round2/mcp-evaluation/inputs/eval-document.hwpx`는 정상적으로 열린다.

**우리 엔진은 표·그림 마크업을 생성하지 않는다**(원본 보존). 따라서 이 크래시는 엔진 출력 경로의 결함이 아니라 **손합성 fixture의 결함**이다. 다만 "한컴이 받아들이는 최소 구조"를 알려주므로 기록한다.

## 5.8 열린 문제: 캐럿 SIGSEGV

- `windows-handoff-20261017-v2/engine_inputs/01_original.hwpx` → offscreen에서 재현성 있게 SIGSEGV(rc 139).
- 백트레이스: `CHwpCaret::_GetCaretInfo` → `_GetVisualCaretPos` → `_VisualizeCaretPos` → `CHwpListPos::_ValidateList` → 죽음. Qt 타이머에서 도달 = **파싱이 아니라 로드 후**.
- 원인 위치는 `Contents/section0.xml`임을 교차 교체로 확인(head·패키지 파트 교체는 영향 없음).
- 실사용 문서 출력(`real-verify/out.hwpx`)은 offscreen에서 12초 생존했으나 **VNC 실행에서는 한 번 죽었다**. **정정(2026-10-17): 이 VNC 죽음은 문서 탓이 아니다.** 대조 실험에서 한컴이 저장한 정상 파일 `bodojaryo-reference.hwpx`도 VNC 클라이언트가 프레임버퍼를 요청하는 시점에 rc=139로 죽었다(접속 전 생존 → 접속 후 죽음). 즉 **VNC 플랫폼 플러그인 자체의 결함**이고, 스크린샷은 크래시 전에 이미 저장되므로 증거로는 유효하다. VNC 실행의 종료 코드는 판정에 쓰지 않는다.
- 따라서 문서 기인 크래시는 **offscreen `crashtest.sh`로만** 판정한다. `01_original.hwpx`는 offscreen에서 재현성 있게 죽고, 정상 파일들은 죽지 않는다.
- 진행 중: 경험적 최소 재현(DeepSeek), 바이너리 수준 원인(Sol r2 실패 → Daybreak 재배정). 결과는 `spikes/work/hancom-oracle/crash-bisect.md`, `crash-analysis.md`.

## 5.9 전수 호환성 매트릭스 (86개 파일)

`spikes/work/hancom-oracle/compat-matrix.md`·`compat-matrix.tsv` (Gemini 3.8 Flash). 게이트는 offscreen 생존 여부다.

| 그룹 | 파일 | OPEN | FAIL(rc 139) |
|---|---:|---:|---:|
| (a) 공개 코퍼스(실제 한컴/타 도구) | 10 | 10 | 0 |
| (b) 한컴 재저장본(정답 라벨) | 9 | 9 | 0 |
| (c) 사용자 작성 Windows 문서 | 5 | 5 | 0 |
| (d) 엔진 합성 fixture | 17 | 14 | **3** |
| (e-1) 엔진이 사용자 문서에 적용한 출력(2차) | 8 | **8** | 0 |
| (e-2) MCP 평가 export | 6 | 6 | 0 |
| (e-3) 구 handoff 입력(v1/v2) | 8 | 0 | **8** |
| (e-4) 진단용 변형 | 11 | 6 | 5 |
| (e-5) Stage 4 handoff 입력 | 12 | 12 | 0 |
| **합계** | **86** | **70** | **16** |

**결론: 실사용 경로는 깨끗하다.** 실제 한컴 문서·재저장본·사용자 문서·그리고 **엔진이 그 문서들에 적용한 출력은 100% 열린다.** 크래시는 전부 **손으로 합성한 최소 마크업**(구 handoff 입력, `tests/fixtures/editing/` 일부, 진단용 변형)에만 있다.

크래시하는 편집 fixture 3종: `shared_media.hwpx`, `cropped_picture.hwpx`, `effects_picture.hwpx`. 셋 다 `hp:pic` 안에 **속성 없는 `<hp:pos/>`**를 갖는다. 그러나 그 가설은 **반증됐다** — `hp:pos`에 한컴식 전체 속성을 채워도(`A_pos_full`), 최소 속성만 줘도(`B_pos_minimal`), 여전히 rc=139다. 원인은 아직 미판정이며 `crash-bisect.md`·`crash-analysis.md`에서 진행 중이다.

## 5.10 크래시 원인 확정과 시험 자료 수정 (2026-10-17)

**원인: `hp:pic`에 `hp:renderingInfo`가 없으면 한컴이 SIGSEGV로 죽는다.** 독립 감사 두 건이 같은 결론에 도달했고, 바이너리 수준까지 확정됐다(`spikes/work/hancom-oracle/crash-analysis.md`).

바이너리 기전(`libCoreEngine.so`, ELF vaddr):

```
0x51a640: mov 0xf0(%rdi),%rax   ; rax = CHwpPicture+0xF0 (render-matrix 목록 head) = NULL
0x51a651: mov 0x10(%rax),%r13   ; SIGSEGV — [rax+0x10] = [0x10]
```

- `rdi`는 `CHwpPicture*`, `rax=0`. **결정적 NULL 역참조**이며 쓰레기 포인터가 아니다.
- 정상 문서에서 같은 지점을 보면 `CHwpPicture+0xF0` → 노드 → `+0x10` → RTTI `16CHwpRenderMatrix` 객체이고, 그 안 `+0x8/+0x50/+0x98`에 세 행렬이 있다.
- XML 경로: 팩터리 `0x71f830`이 `OWPML::CRenderingInfo`를 만들고, 변환기 `0x6a4d1f`가 `hp:renderingInfo` 자식을 찾아 있을 때만 `0x6a49b0`을 호출해 `CHwpRenderMatrix`를 `CHwpPicture+0xF0`에 `AddTail`한다. **자식이 없으면 목록이 비고, 로드 후 캐럿 타이머에서 죽는다.**

최소 조건(게이트 변형 실험으로 확인): `hp:renderingInfo` 안에 **`hc:transMatrix`·`hc:scaMatrix`·`hc:rotMatrix` 세 개가 모두** 있어야 한다. 하나라도 빠르거나 비어 있으면 rc=139. 행렬 **값**은 무관하다. 필요성은 반대 방향으로도 확인됐다 — 정상 파일에서 `renderingInfo` 6개를 제거하면 PASS→FAIL로 뒤집힌다.

기각된 가설: `hp:pos` 속성 채우기, `hp:pic`/`hc:img` 속성 채우기, `hp:pos` 삭제 — 모두 여전히 rc=139.

미판정: 해당 가상 함수의 원래 C++ 이름, Windows·신형 한컴에서의 동일성(이건 2020 Linux Beta만 확인).

**시험 자료 수정:** `tests/fixtures/editing/gen_editing.py`의 `pic()`을 한컴 모델 `CPictureType::InitMap` 순서·속성으로 다시 썼다. 재생성 후 편집 fixture **17종 전부가 게이트를 통과**한다(수정 전에는 3종이 rc=139).

**남은 시험 자료 결함(미수정):** 편집 fixture의 `Contents/header.xml`이 `<hh:refList/>`로 비어 있다(`simple_cell`·`shared_media` 등). 한컴은 열지만, 우리 `hae-verify` 관문은 `paraPrIDRef="0"` 미해결로 거부한다. 실제 HWPX에는 `paraPr`·`charPr`·`fontfaces`가 항상 있다.

## 5.11 `image.replace`의 그림 기하 수정 — 완료·검증

독립 감사의 1순위 발견: **그림을 바꿔도 `hp:imgClip`/`hp:imgDim`이 옛 크기로 남아**, 새 그림이 크면 일부만 보인다.

**2차 원인은 단위였다.** 한컴은 `imgClip`/`imgDim`을 **HWPUNIT, 픽셀당 75**로 읽는다. 처음 수정에서 픽셀 수를 그대로 썼더니(800×400), 한컴은 그것을 60000×30000 그림의 좌상단 800×400으로 해석해 **빨간 사분면만** 그렸다(`images/clip-quarter-only.png`).

수정(`crates/hae-edit/src/image.rs`, `crates/hae-verify/src/image.rs`):
- `imgDim` = 새 그림 픽셀 크기 × 75, `imgClip` right/bottom = 같은 값
- 표시 프레임(`sz`/`curSz`/`pos`/캡션)과 `orgSz`/`imgRect`는 **건드리지 않는다**
- `hae-verify`도 PNG IHDR / JPEG SOF에서 크기를 파싱해 같은 기대를 만든다

검증: 800×400 사분면 PNG로 교체한 결과를 한컴으로 열어 **네 사분면이 모두 렌더링**되는 것을 화면으로 확인했다(`images/clip-four-quadrants.png`). 회귀는 기존 시험이 1×1 → `dimwidth="75"`·`right="75"`로 단위를 고정한다.

한컴 자체 불변식(감사 관찰): `imgDim == px×75`(반올림), 자르지 않은 그림에서 `imgClip == imgDim`. `imgRect.pt2 == orgSz`, `sz == curSz`(0이면 `orgSz`).

**주의: 우리 `hae-verify` 게이트는 이 오류를 잡지 못한다.** 양쪽이 같은 숫자를 쓰면 게이트도 동의하기 때문이다(감사 지적). 그래서 이 값은 실측 화면 확인으로만 검증된다.

`tests/fixtures/editing/gen_editing.py`의 `pic()`도 같은 단위로 고쳤다(내장 이미지 2×1 → `imgDim`/`imgClip` 150×75). 재생성 후 편집 fixture **17종 전부가 게이트를 통과**한다.

미판정: JPEG·회전 그림·Windows 한컴, 종횡비가 다른 교체 시 늘어남(§3.1), `orgSz`와 `imgRect`가 다를 때의 한컴 공식.

## 6. 추정과 남은 확인 (주장하지 않는 것)

- 1차 Windows 실패 01–04 중 **그림이 없는** `01_text`·`02_table`의 실패는 이 결함으로 설명되지 않는다. 그쪽은 패키지 구조 누락이 원인이라는 기존 가설(2차에서 보완)이 유력하지만, **1차 파일을 다시 열어 원인을 확정하지는 않았다.**
- 리눅스 한컴(2020 Beta, 11.20)에서의 결과를 Windows 한컴오피스 2024(13.0)에 그대로 일반화하지 않는다. `isEmbeded` 규칙은 같은 제품 계열의 같은 파서이므로 **추정**은 강하지만 Windows 재확인이 남는다.
- 2차 산출물의 Windows 확인은 아직 `not_checked`다.
