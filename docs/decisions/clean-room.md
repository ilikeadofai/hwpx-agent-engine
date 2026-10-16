# 클린룸·재사용 기록 (구현 계획 1-19)

작성: 2026-10-16, 총괄. 근거: 설계서 §12(클린룸 자료 경계), 단계 0 결과 `docs/decisions/stage0-results.md`, 구현 계획 작업 1-19. 관련: `docs/decisions/dependencies.md`(작업 1-1), `docs/decisions/model-roles.md`.

상태: 단계 1 진행 중. 이 문서는 **자료 출처를 관리하는 기술적 절차 기록**이며, 법적 적합성을 판정한 결과가 아니다. 어떤 자료를 어디서 가져왔고 어떻게 격리했는지를 남기는 것이 목적이다.

## 0. 이 문서가 주장하지 않는 것

- 모델을 바꾼 사실만으로 자료를 분리했다고 주장하지 않는다. 격리는 전달 범위·첨부·인계 기록으로 관리한다(§2).
- AI의 학습 데이터 노출까지 통제했다고 주장하지 않는다.
- 전체 프로젝트의 개발 경위를 검증했다고 주장하지 않는다.
- 관찰하지 않은 한컴 동작은 추정으로 남긴다. 확인하지 않은 것을 확인한 것으로 적지 않는다.
- 이 문서 자체는 법적 판정이 아니다(§1.4).

## 1. 클린룸 원칙과 자료 경계

설계서 §12를 요약한다. 경계는 "무엇을 쓰는가"가 아니라 "무엇을 쓰지 않는가"와 "가져온 것을 어떻게 기록하는가"로 관리한다.

### 1.1 사용하는(허용) 자료

| 유형 | 예 | 근거 |
|---|---|---|
| 공개 포맷·API 문서 | OWPML/HWPX 공개 명세, 한컴 공개 모델(`hancom-io/hwpx-owpml-model`) | 요소 구조·저장 참고. 조판 엔진과 구분한다 |
| 출처·라이선스를 확인한 오픈소스 | rhwp(MIT), 코퍼스 제공 저장소(MIT) | 라이선스와 커밋을 기록(§3) |
| 자작 시험 문서 | 합성 HWPX(`tests/fixtures/`) | 전부 합성, 개인 문서 없음(§4) |
| 자작 관찰 문서 | Windows 한컴으로 직접 만든 최소 문서 | 관찰 시험 자료로 표시(§2) |
| 자작 관찰 값 | 코퍼스의 ZIP·구역·표·그림 유무 등 구조 관찰 | 파서로 계산한 값. 손으로 적지 않음 |

### 1.2 배제하는 자료

- SDK의 **비공개 구현 코드**, **디컴파일 결과**, **독점 자산**을 자체 구현 자료에 넣지 않는다.
- **개인 문서**는 시험·구현 자료로 쓰지 않는다(개인 최종 보고서 포함).
- 관찰 담당의 **전체 대화·작업 폴더를 구현 담당에게 넘기지 않는다**(§2).
- 외부 공급자(DeepSeek·Gemini·GPT)에게 사용자 문서·개인 자료·실명이 든 문서 본문을 보내지 않는다(`model-roles.md`).

### 1.3 혼동하지 말아야 할 구분

| 구분 | 설명 |
|---|---|
| 공개 코드 재사용 ↔ 비공개 구현 독립 재구현 | 공개 rhwp 코드를 라이선스에 따라 재사용하는 것과, 비공개 한컴 구현을 독립적으로 다시 만드는 것은 다른 일이다 |
| 공개 코드를 본 사실 ↔ 오염 | 공개 코드를 읽은 것 자체를 오염으로 취급하지 않는다. 다만 핵심 계층으로 가져온 코드는 재사용 이력에 남긴다(§3) |
| 폰트 로컬 사용 ↔ 폰트 재배포 | 설치된 폰트를 로컬에서 사용하는 것과 폰트 파일을 재배포하는 것을 구분한다 |

### 1.4 이 절차의 성격

이 규칙은 자료 출처를 관리하는 **기술적 절차**이며, 특정 자료의 법적 적합성을 판정한 결과가 아니다. 라이선스 해석·법적 판단이 필요하면 별도 검토로 남긴다.

## 2. Windows 한컴 관찰 인계 기록 양식

Windows 한컴에서 관찰한 동작을 구현 담당에게 넘길 때 쓰는 양식이다. 구현 담당에게는 **검토된 동작 명세와 시험 예만** 전달하고, 관찰 담당의 **전체 대화·작업 폴더는 넘기지 않는다**. 모델을 바꾼 사실을 격리 근거로 삼지 않는다.

### 2.1 필드 정의

| 필드 | 뜻 | 필수 |
|---|---|---|
| `specId` | 명세 ID. 예: `OBS-S4-001`. 한 관찰 = 한 행 | 필수 |
| `title` | 관찰 대상 한 줄 요약 | 필수 |
| `authorRole` | 작성 역할(모델·사람 구분 포함). 예: `user-windows-hangul`, `claude-opus-5.5(총괄)` | 필수 |
| `reviewRole` | 검토 역할. 인계 전에 명세·시험 예를 읽고 승인한 주체 | 필수 |
| `evidenceType` | 근거 유형: `observation`(한컴 관찰), `spec`(공개 명세), `synthetic`(합성), `public-corpus`(공개 코퍼스) | 필수 |
| `inputSha256` | 입력 HWPX 등의 SHA-256 | 필수 |
| `outputSha256` | 출력·스크린샷·재저장 파일의 SHA-256 | 필수 |
| `conditions` | 한컴 버전·폰트·조판 조건 등 재현 조건 | 권장 |
| `deliveredScope` | 구현 담당에게 실제로 전달한 범위 | 필수 |
| `excludedScope` | 전달에서 제외한 범위 | 필수 |
| `observedAt` | 관찰 시각(날짜) | 권장 |
| `notes` | 추정과 확인을 구분해 적는다 | 선택 |

### 2.2 전달 범위·제외 범위 템플릿

```text
전달 범위(deliveredScope):
  - 검토된 동작 명세서(요약, specId별)
  - 시험 예(입력/기대/거절 코드)
  - 입력·출력 파일과 SHA-256
  - 재현 조건(한컴 버전, 폰트, 시험 문서 식별자)

제외 범위(excludedScope):
  - 관찰 담당의 전체 대화 로그·프롬프트
  - 관찰 담당의 작업 폴더·메모·중간 산출물
  - 개인 문서 본문·실명
  - 비공개 SDK·디컴파일·독점 자산
  - 관찰 담당이 쓰던 개발 도구 상태
```

### 2.3 Markdown 예시

```markdown
## OBS-S4-001 — 변형 E 캐시 제거 문서 재열기

- title: 수정 최상위 문단부터 구역 끝까지 캐시를 제거한 문서를 한컴이 정상 재배치하는가
- authorRole: user-windows-hangul
- reviewRole: claude-opus-5.5(총괄)
- evidenceType: observation
- observedAt: 2026-10-16
- inputSha256: 3d01965c0d8d...  (E_from_edit.hwpx)
- outputSha256: (재저장 파일) ...
- conditions: Windows 한컴 2024, 저장소 고정 폰트, 시험 묶음 ...zip(8cb5917e2c95...)
- deliveredScope: 명세 요약, 변형 0/A/B/E 기대표, 입력·재저장 hash
- excludedScope: 관찰자 대화 로그, 작업 폴더, 개인 문서
- notes: 관찰 값만 기록. 재배치가 실패로 보이면 그대로 실패로 적고 원인은 추정으로 표시
```

### 2.4 JSON 예시

```json
{
  "schemaVersion": 1,
  "specId": "OBS-S4-001",
  "title": "변형 E 캐시 제거 문서 재열기",
  "authorRole": "user-windows-hangul",
  "reviewRole": "claude-opus-5.5(총괄)",
  "evidenceType": "observation",
  "observedAt": "2026-10-16",
  "conditions": {
    "hangulVersion": "unknown-until-observed",
    "fonts": ["저장소 고정 폰트"],
    "bundle": "S4_한컴재열기_시험묶음.zip"
  },
  "attachments": [
    {"path": "E_from_edit.hwpx", "sha256": "3d01965c0d8d..."},
    {"path": "E_from_edit_resaved.hwpx", "sha256": "..."}
  ],
  "deliveredScope": [
    "검토된 동작 명세(요약)",
    "변형 0/A/B/E 입력·기대표",
    "입력·재저장 파일과 SHA-256"
  ],
  "excludedScope": [
    "관찰 담당 전체 대화 로그",
    "관찰 담당 작업 폴더",
    "개인 문서 본문·실명",
    "비공개 SDK·디컴파일·독점 자산"
  ],
  "notes": "관찰 값만 기록. 확인하지 않은 동작은 추정으로 남긴다."
}
```

## 3. 외부·재사용 코드 및 의존성 이력

### 3.1 단계 1 코어 crate 10개는 자체 작성

`crates/`의 10개 crate는 이 저장소에서 **직접 작성한 1차 Rust 코드**다. 외부 HWPX 파서 코드를 복제하지 않았다. 외부 오픈소스는 Cargo 의존성(§3.4)과 알고리즘 참고로만 쓰고, 소스 코드를 복사해 오지 않았다.

| crate | 역할 | 출처 |
|---|---|---|
| `hae-diag` | 진단·오류 코드, `Limits` 상수, 자원 예산 타입 | 자체 작성(1-5) |
| `hae-zip` | ZIP 방어적 파싱·한도·원시 복사 쓰기 | 자체 작성(1-3) |
| `hae-xml` | 원문 보존 XML 토큰, namespace 해석, 범위 교체 | 자체 작성(1-4) |
| `hae-package` | mimetype·content.hpf·spine·META-INF·미디어 참조 | 자체 작성(1-6) |
| `hae-model` | 정규 상태, nodeId, 요소 인덱스, 조회 뷰 | 자체 작성(1-7) |
| `hae-edit` | 네 가지 편집, `LinesegPolicy`, PrvText | 자체 작성(1-9~1-12) |
| `hae-verify` | 저장 관문 검사기(편집 경로와 독립) | 자체 작성(1-13) |
| `hae-project` | 입출력 루트, 링크 비추종 I/O, 잠금, 원자적 저장, `.hwpwork` | 자체 작성(1-15) |
| `hae-exec` | 레지스트리·스키마, 세션 실행기, 이력·자원 예산 | 자체 작성(1-16·1-17) |
| `hae-cli` | CLI 스크립트 모드 | 자체 작성(1-18) |

### 3.2 외부 HWPX 파서 코드 복제 없음

핵심 계층(`hae-*`)에는 외부 HWPX 파서·직렬화기 소스를 복사해 넣지 않았다. 원문 보존 XML 편집은 단계 0 S1에서 후보 토크나이저(quick-xml)로 검증한 자체 로직이다(ZIP 구조 처리는 `hae-zip`에서 직접 구현, §3.5). 기존 Python Workbench와 공개 OWPML 모델에서 참고한 시험·코드가 있으면 출처와 라이선스를 여기에 남긴다(현재 제품 코드로 가져온 항목 없음).

### 3.3 rhwp 격리 참조

| 항목 | 값 |
|---|---|
| 저장소 | `edwardkim/rhwp` |
| 커밋 | `1a76570e833917d15817415a53c09ad61ab3203f` (v0.8.7) |
| 라이선스 | MIT |
| 격리 참조 범위 | 단계 0 S2(어댑터 출력 형식)·S3(격리 실행) 검증, 단계 3 별도 프로세스 어댑터 후보 |
| 단계 1 코어 링크 | **없음.** `hae-*` 코어 crate는 rhwp에 의존하지 않는다 |

- rhwp는 **단계 0 검증 자료**와 **단계 3 별도 프로세스 어댑터 후보**로만 참조한다. 단계 1 코어에 링크하지 않는다(계획 §3: 핵심 crate는 MCP·rhwp·GUI에 의존하지 않음).
- 재사용이 아니라 **별도 프로세스로 격리 실행**하는 방식이다. 정규 문서 변경·프로젝트 저장 권한을 rhwp에 주지 않는다(설계서 §9).
- 단계 0에서 저장소 고정 Rust 1.93.1 대신 1.92.0으로 빌드했다. 단계 3 시작 때 1.93.1로 다시 빌드해 재확인한다(설계서 §9). 코드 감사는 하지 않았다.
- 재사용한다면 버전·라이선스·수정 사항을 이 표에 추가한다.

### 3.4 Cargo 직접 의존성

`Cargo.toml`에 `=` 판으로 고정한 직접 의존성이다. 라이선스는 고정 판의 crate 메타데이터 기준이며, RustSec 공지 확인 결과와 함께 `docs/decisions/dependencies.md`(작업 1-1)에 확정해 기록했다.

| crate | 고정 판 | 라이선스 | 역할 |
|---|---|---|---|
| quick-xml | 0.39.2 | MIT | XML 토큰. namespace는 직접 해석(1-4) |
| flate2 | 1.1.9 | MIT OR Apache-2.0 | DEFLATE 압축 |
| crc32fast | 1.5.0 | MIT OR Apache-2.0 | ZIP CRC-32 |
| unicode-segmentation | 1.13.3 | MIT OR Apache-2.0 | 자소 군집(UAX#29) |
| serde | 1.0.229 | MIT OR Apache-2.0 | 직렬화 |
| serde_json | 1.0.151 | MIT OR Apache-2.0 | JSON |
| schemars | 1.2.2 | MIT | 명령 타입에서 JSON Schema 생성 |
| sha2 | 0.10.9 | MIT OR Apache-2.0 | SHA-256 |
| getrandom | 0.3.4 | MIT OR Apache-2.0 | 난수 nodeId |
| png | 0.18.1 | MIT OR Apache-2.0 | PNG 검사(할당 전 크기·화소 한도) |
| zune-jpeg | 0.5.15 | MIT OR Apache-2.0 OR Zlib | JPEG 검사 |
| rustix | 1.1.4 (fs) | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | openat·O_NOFOLLOW·flock·fsync·renameat_with(NOREPLACE) |
| thiserror | 2.0.20 | MIT OR Apache-2.0 | 오류 타입 |
| clap | 4.6.6 (derive) | MIT OR Apache-2.0 | CLI 인자 |

### 3.5 ZIP crate를 쓰지 않는 이유

원시 압축 데이터 복사는 zip crate도 제공하지만, 우리는 항목 순서·압축 방식에 더해 로컬 헤더의 이름·extra 필드·플래그를 바이트 그대로 보존해야 한다. 이 범위를 zip crate의 쓰기 API로 보장하는지 확인하지 못했으므로 `hae-zip`에서 직접 구현하되, 읽기 쪽 방어 조건(작업 1-3)을 완료 기준으로 둔다.

## 4. 시험 자료 출처 및 격리 상태

시험 자료는 세 출처로 만든다(설계서 §13): Windows 한컴 자작 문서, 이용 조건을 확인한 공개 저장소 샘플, 엔진 시험용 합성 문서. **개인 최종 보고서는 시험에 쓰지 않는다.**

### 4.1 `tests/fixtures/corpus/` — 공개 템플릿 관찰

- 성격: 공개 MIT 저장소의 서식 10개. **원본 바이트 그대로 복사, 수정 금지.** 실제 정부 문서(rhwp `samples/hwpx/opengov`)는 넣지 않았다.
- 합성 여부: **합성 아님**(공개 자료 사본). 관찰 값(`observed`)은 파서로 계산해 `manifest.json`에 고정.
- 출처: 모두 MIT.

| 파일 | 저장소 | 커밋 | 라이선스 |
|---|---|---|---|
| `bodojaryo-`·`gyehoek-`·`problem-answer-`·`research-school-plan-reference.hwpx` | `jkf87/hwpx-skill` | `ba90fc5ae15f4aead8dab4f9d69d4830b9569859` | MIT |
| `ilban-gian`·`gani-gian.hwpx` | `chrisryugj/kordoc` | `cd87f6e2ecc03d5ef773c8fc04be0b15d45da4a4` | MIT |
| `irb-sample.hwpx` | `nathankim0/easy-hwp` | `039b7d0926b867b0a21ca7b3113666fef5c56897` | MIT |
| `ref_empty`·`anchor_char_height`·`render-p35-font-native-bitmap.hwpx` | `edwardkim/rhwp` | `1a76570e833917d15817415a53c09ad61ab3203f` | MIT |

- 한컴이 만든 파일과 다른 도구가 만든 파일이 섞여 있다(예: `irb-sample`은 href 기준·디렉터리 항목 포함). 어느 쪽인지 단정하지 않고 `observed`로만 기록한다.

### 4.2 `tests/fixtures/adversarial/` — 합성 악성/경계 ZIP

- 성격: **전부 합성.** `gen_adversarial.py`(Python 3 표준 라이브러리 전용, 결정적)가 최소 유효 패키지를 만들고 결함을 심은 뒤 실제로 있는지 assert. 제3자 자산 없음.
- 규모: 80개(거절 63, 수용 17). 기대 결과는 `adversarial_expected.json`에 고정(`expect`·`code`·`reason`·`stage`).
- 개인 문서 없음. 코퍼스·편집 자료를 수정하지 않는다.

### 4.3 `tests/fixtures/editing/` — 합성 편집 경계

- 성격: **전부 합성.** 개인 문서·제3자 자산을 쓰지 않는다. 교체용 이미지도 합성기가 직접 만든 바이트(JPEG는 자체 생성 1x1 grayscale).
- 패키지 골격은 `adversarial/valid_base.hwpx`에서 가져오고 구역·manifest·미디어만 바꾼다(그 파일은 수정 금지).
- 규모: 패키지 17개 + 교체 이미지 입력 8개. 기대 결과는 `editing_expected.json`(구조 선택자·outcome·코드·보존 조건). 무작위 `nodeId` 때문에 대상을 ID가 아니라 **구조 선택자**로 적는다.
- `provenance`: "전부 합성. 개인 문서·제3자 자산 없음. 자료_원본 사본과 다른 시험 자료는 건드리지 않음."

### 4.4 `tests/oracle/` — 독립 기대 변경·변조 시험

- 성격: **합성.** Python 표준 라이브러리만 사용. 합성 HWPX에서 기대 변경을 독립적으로 계산하고 변조 사례를 만든다(작업 1-14, GPT 6.1 Sol). 엔진 코드에 의존하지 않는다.
- `mutations.py`의 manifest: `provenance: "hand-authored synthetic; no external document content"`.
- 목적: 같은 함수의 결과를 자기 자신과 비교하지 않고 보존을 입증하기 위함(설계서 §10). 개인 문서를 쓰지 않는다.

### 4.5 개인 문서 배제 확인

| 자료 묶음 | 개인 문서 포함 | 확인 |
|---|---|---|
| `corpus/` | 없음 | 공개 MIT 저장소 사본만. 실제 정부 문서·개인 자료 없음 |
| `adversarial/` | 없음 | 합성기 전용 출력 |
| `editing/` | 없음 | 합성기 전용 출력 + 자체 생성 이미지 |
| `oracle/` | 없음 | 합성 기대·변조 |
| Windows 관찰(§2) | 배제 | 개인 문서 본문·실명은 구현 담당에 전달하지 않음 |

### 4.6 격리·검증 상태

| 항목 | 상태 | 근거 |
|---|---|---|
| S1 원문 보존 XML 편집 | 통과 | 단계 0(공개 서식 6종 18문단 + 28구역 3곳) |
| S2 rhwp 어댑터 | 통과 | 단계 0(SVG 렌더링·줄 배치 캐시, §11 개정) |
| S3 격리 실행 | 통과 | 단계 0(bwrap: 네트워크 차단·읽기 전용 입력·임시 출력) |
| S4 Windows 한컴 재열기 | **`not_checked`** | 사용자 Windows 확인 대기. `spikes/work/S4_한컴재열기_시험묶음.zip`(8cb5917e2c95…) |
| `nativeComparison`(Windows 기준 자) | **`not_checked`** | S4 및 단계 4 평가 전. Windows 기준 출력과 비교하지 않았다 |

- S4 결과가 나오면 이 표와 §11 캐시 규칙을 함께 갱신한다.
- 어댑터가 실패하면 코어 편집·저장 시험만 통과했다고 기록하며 0.1을 완료로 표시하지 않는다(설계서 §13).
- 자세한 단계 0 근거와 해시는 `docs/decisions/stage0-results.md`를 따른다.
