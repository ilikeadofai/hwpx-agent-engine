# tests/fixtures

HWPX 엔진 0.1 시험 자료 1차(계획 1-2a). 열기 단계(hae-zip·hae-xml)의 기대 결과를 고정한다.

## 구성

| 경로 | 내용 |
|---|---|
| `corpus/*.hwpx` | 공개 MIT 라이선스 저장소의 서식 10개(원본 바이트 그대로 복사, 수정 금지). 실제 정부 문서(rhwp `samples/hwpx/opengov`)는 넣지 않았다. |
| `corpus/manifest.json` | 파일별 sha256·크기·출처 저장소/커밋/경로·라이선스·`kind: public-template`, 그리고 관찰 값(`observed`: ZIP 항목 수, spine 순서의 구역, tbl/pic/equation 유무, 서명·암호화 항목, 압축 방식, 디렉터리 항목 등). |
| `make_corpus_manifest.py` | `manifest.json` 재생성(출처 표는 스크립트 안에 고정). |
| `gen_adversarial.py` | 악성/경계 패키지 합성기(stdlib 전용, 결정적). 최소 유효 패키지를 직접 만들고 사례마다 결함을 심은 뒤, 결함이 실제로 있는지 assert 한다. |
| `adversarial/*.hwpx` | 합성기 출력 80개(약 1MB). 거절 사례 63개, 수용 사례 17개. |
| `adversarial_expected.json` | 사례별 기대 결과: `expect`(reject/accept), `code`(PACKAGE_REJECTED·RESOURCE_LIMIT), `reason`, `stage`(zip/package/xml), `notes`, accept 사례의 `detail`(기대 문단 텍스트 등). 새로 만든 사유는 최상위 `new_reasons`. |
| `paragraph/gen_paragraph_structure.py` | 0.4 문단 삽입·삭제용 합성 문서 `paragraph/paragraphs.hwpx`와 `paragraph/expected.json` 생성기(표준 라이브러리만, 결정적, `--check-determinism`). 구역 속성 문단·`paraPr` 2종·표 문단·그림 문단·1×2 셀 표를 담는다. |
| `paragraph/paragraphs.hwpx` | 위 생성기 출력. `hae-edit`·`hae-verify`·`hae-exec`의 문단 구조 시험이 쓴다. |
| `paragraph/expected.json` | 문단 순서·텍스트·역할·캐시 위치·`PrvText` 기대값. |
| `created/check_created.py` | `document.create`가 만든 `.hwpx`를 빌더/검증기와 별개로 검사하는 표준 라이브러리 검사기(항목 순서·`mimetype` STORED·`version.xml` 규칙·`rootfile` media-type·참조 해석·첫 문단 `secPr`·문단 텍스트). 사용: `python3 -I tests/fixtures/created/check_created.py FILE.hwpx --title T --expect-texts "a,b,c"`. |

## 출처 (모두 MIT)

- jkf87/hwpx-skill `ba90fc5a…` — `assets/{bodojaryo,gyehoek,problem-answer,research-school-plan}-reference.hwpx`
- chrisryugj/kordoc `cd87f6e2…` — `templates/일반기안문_서식.hwpx`(ilban-gian), `간이기안문_서식.hwpx`(gani-gian)
- nathankim0/easy-hwp `039b7d09…` — `IRB서식_샘플.hwpx`(irb-sample)
- edwardkim/rhwp `1a76570e…` — `samples/hwpx/ref/ref_empty.hwpx`, `samples/anchor_char_line_height/anchor_char_height.hwpx`, `samples/render-p35-font-native-bitmap.hwpx`

전체 커밋 해시는 `corpus/manifest.json` 에 있다. 코퍼스는 관찰 자료이며, 한컴이 만든 파일과 다른 도구가 만든 파일이 섞여 있다(예: irb-sample 은 content.hpf href 가 hpf 폴더 기준, 디렉터리 항목 포함).

## 재생성

```sh
python3 -I tests/fixtures/make_corpus_manifest.py
python3 -I tests/fixtures/gen_adversarial.py tests/fixtures/adversarial \
    --expected tests/fixtures/adversarial_expected.json
python3 -I tests/fixtures/paragraph/gen_paragraph_structure.py --check-determinism
```

같은 zlib 구현이면 출력 바이트가 같다(두 번 실행해 비교함). 기대 결과의 정책 선택(예: 끊어진 미디어 참조 거절, bit 11 없는 UTF-8 이름 거절)은 각 사례의 `notes` 에 적어 두었고, 총괄이 바꾸면 합성기의 해당 `expect` 만 고쳐 다시 생성한다.
