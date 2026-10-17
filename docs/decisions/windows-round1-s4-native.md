# Windows 1차 S4 한컴 비교 기록

작성일: 2026-10-17
대상: Stage 0 S4, 원본(0)·문단 캐시 제거(A)·낡은 캐시 유지(B)·구역 끝 캐시 제거(E)
nativeComparison: S4 1차 Windows 결과 검토 완료. 범위는 한컴 안의 0/A/B/E 재열기·재저장 비교다. Linux rhwp와의 기준 조판·픽셀 비교는 미확인이다.

## 요약

- results.csv에서 S4 입력 12개는 모두 열기·재저장·재열기 O다. 자동 환경기록은 Windows 10 IoT Enterprise LTSC, 한컴오피스 2024 Edu 13.0.0.304다.
- PDF의 “S4 확인”은 보도자료 1쪽, 계획서 1쪽, 다중구역 18쪽에 있다. 의도한 문단 변경과 추가 빈 쪽 번호를 제외하면 본문 글자 누락은 발견하지 못했다.
- 계획서 B는 19쪽, A/E는 20쪽이다. B 문단은 한 줄, A/E는 두 줄이며 B 단어 bbox끼리 겹치거나 페이지 밖으로 잘린 흔적은 없다. 다만 B는 A/E와 쪽 나눔이 다르고 재저장 후에도 대상의 낡은 lineseg 캐시가 남는다.
- E 세 입력은 모두 열기·저장·재열기에 성공했고 저장본에서 수정 문단부터 구역 끝까지 캐시가 다시 생겼다. 이 표본은 §11 E 정책을 지지한다.
- 한컴 재저장은 XML·미리보기·메타데이터를 폭넓게 정규화한다. 원본 보존 출력의 대체물로 보지 않고 별도 파일로 관리한다.

## 근거와 환경

- 결과: spikes/work/windows-return-20261017-2047/return/results.csv
- 환경: spikes/work/windows-return-20261017-2047/return/environment.txt, environment_auto.txt
- 입력: spikes/work/windows-handoff-20261017-v2/s4_inputs/
- PDF: spikes/work/windows-return-20261017-2047/return/pdf/
- 재저장: spikes/work/windows-return-20261017-2047/return/resaved/
- 배경: docs/superpowers/specs/2026-10-16-hwpx-engine-0.1-design.md §11, docs/decisions/stage0-results.md §S2·§S4

PDF 그림 품질은 “매우 높음(원본과 동일 품질)”이고, HWPX는 최종본·미리보기 저장, HWPX 압축 저장, 개인 정보 탐색 옵션을 기본 상태로 저장했다. version.xml appVersion 13, 0, 0, 711은 설치 빌드 13.0.0.304와 별도 필드이므로 동일 빌드 번호로 취급하지 않는다.

같은 results.csv에서 engine_inputs 01–04는 빈 화면 후 종료로 X다. 이 네 건은 S4 비교에서 제외했다. return 안에 화면 캡처가 없어, 시각 판독은 사용자의 O 기록과 PDF 텍스트·bbox 분석을 구분한다.

## PDF 쪽수와 문구 위치

pdfinfo 쪽수와 pdftotext -layout 결과다.

| 문서 | 0 | A | B | E | “S4 확인” | 뒤쪽 경계 |
|---|---:|---:|---:|---:|---:|---|
| 보도자료 | 2 | 3 | 3 | 3 | 1쪽 | 본문은 1–2쪽. 3쪽은 “- 3 -”만 있는 추가 빈 쪽. |
| 계획서 | 19 | 20 | 19 | 20 | 1쪽 | A/E 2쪽은 “- 2 -”만 있고, 원본 목차부터 뒤 내용이 3–20쪽으로 이동. B 목차는 2쪽에 있다. |
| 다중구역 | 28 | 28 | 28 | 28 | 18쪽 | 쪽 경계 대응이 유지된다. |

계획서 B/A/E 편집 문단 bbox는 A/E 2줄(y 약 472.6–543.8 pt), B 1줄(y 약 472.6–505.4 pt)이다. B 문구 bbox는 A4 594×841 pt 페이지 안에 있고 한글·영숫자 단어 bbox 중첩은 0쌍이다. 글리프 경계의 기호만 조금 교차한다. 따라서 B의 19쪽은 본문 누락 때문이 아니다. 뒤쪽 텍스트는 원본 2–19쪽과 대응하고, A/E의 추가 빈 2쪽이 없다.

## PDF 글 누락 대조

편집으로 바뀐 원문 문단을 제외하고 pdftotext 결과의 한글·영문·숫자 문자 multiset을 비교했다.

- 보도자료 원문 문단은 “□ 국토교통부…”로 시작한다. 변형은 S4 문구를 붙이며 사각 글머리표를 제거했고 나머지 본문은 추출된다.
- 계획서 원문 “2025. 1.”은 시험 문단으로 교체되었다. 이 교체를 빼면 A/B/E의 원문 문자는 모두 대응한다. A/E에서 빈 2쪽 번호 2가 추출상 추가된다.
- 다중구역 section17의 “P & A (Prompt & Analyze) 프로젝트”는 시험 문단으로 교체되었다. 이 교체를 제외한 문자는 0/A/B/E에서 대응한다.
- PDF 추출에서 emoji는 U+F000 글리프로 보인다. 이모지의 실제 모양은 확인하지 않았다.

이 검사는 추출 문자 집합만 비교하며 문장 순서·서식·비문자 개체의 시각 보존을 입증하지 않는다.

## 열기 직후와 재열기 PDF

- A/B/E 9쌍의 쪽수와 S4 문구 쪽은 모두 같다. 페이지별 한글·영문·숫자 문자 multiset에도 추가·누락은 없다.
- PDF SHA-256은 9쌍 모두 다르다. 8/9쌍은 페이지별 공백 정규화 pdftotext가 같다.
- 남은 다중구역 A 1쌍도 페이지별 문자 multiset은 같고 추출 공백만 다르다(1쪽 URL 자간, 3쪽 “AI·디지털” 주변). 쪽수·문장 위치 변화는 없다. 픽셀·글꼴 래스터 비교는 하지 않았다.

## 재저장 HWPX lineseg 비교

같은 문서·같은 A/B/E 입력과 resaved 파일을 비교했다. 수치는 수정 문단부터 구역 마지막 최상위 문단까지의 hp:linesegarray 개수다. 변경 문단 수는 배열 값이 달라진 최상위 문단 수다.

| 문서/범위 | A 입력→저장 | B 입력→저장 | E 입력→저장 | 변경 문단 A/B/E |
|---|---:|---:|---:|---:|
| 보도자료 section0 p8부터 29문단 | 28→29 | 29→29 | 0→29 | 29/29/29 |
| 계획서 section0 p13부터 181문단 | 180→181 | 181→181 | 0→181 | 26/19/181 |
| 다중구역 section17 p0부터 27문단 | 26→27 | 27→27 | 0→27 | 1/0/27 |

- A에서 제거한 대상 캐시는 다시 생겼다. 보도자료는 구역 끝까지 모두 달라졌다. 계획서는 181개 중 26개만 달라졌고, 다중구역은 대상만 생성되어 뒤 26개는 유지됐다.
- B의 낡은 캐시는 저장 뒤 문서마다 다르게 처리됐다. 보도자료는 29개 범위가 바뀌었지만 계획서 대상 p13의 1개 lineseg와 뒤 p14–19는 그대로다. 계획서의 다른 19개 뒤쪽 캐시는 바뀌었다. 다중구역 section17의 27개는 전부 그대로다.
- E는 세 문서에서 제거 범위 전체가 저장 후 다시 채워졌다. 계획서 181개, 다중구역 27개, 보도자료 29개다. 계획서 대상 p13은 2개 lineseg를 가진다. 계획서 B 재저장본은 대상 캐시가 남아 있어 한컴 저장이 stale 캐시를 항상 고친다는 근거가 되지 않는다.

## ZIP 엔트리와 저장 정규화

| 문서 | 엔트리 수 | 압축 방식(STORED/DEFLATE) | BinData 대소문자 변경 | payload 변경 A/B/E |
|---|---:|---:|---:|---:|
| 보도자료 | 17 | 8/9 | 6개 | 7/6/7 |
| 계획서 | 14 | 6/8 | 3개 | 7/7/6 |
| 다중구역 | 46 | 11/35 | 8개 | 24/23/24 |

모든 쌍에서 ZIP 엔트리 수·이름 집합·순서는 대소문자 무시 기준으로 같다. 추가·삭제는 없고 mimetype은 첫 엔트리다. 압축 방식도 바뀌지 않았다. BinData 확장자 대소문자만 바뀌고 payload는 같으며 content.hpf manifest 경로도 맞춰 바뀐다. 해제 payload는 같지만 압축 데이터만 다시 인코딩된 엔트리는 다중구역 B 1개, 다중구역 E 12개, 보도자료 E 3개, 계획서 E 3개다.

9개 저장본에서 변경 payload는 합계 111개다. 공통 변경 항목은 version.xml, Contents/header.xml, Contents/content.hpf, Preview/PrvImage.png와 구역 XML 및 일부 settings.xml·Preview/PrvText.txt다. 다중구역은 편집 구역 외에도 A/E 19개, B 18개 section XML payload가 바뀌었다. 하지만 각 Contents/section*.xml의 hp:t 문자열 스트림은 모든 쌍에서 같다.

## 확인된 XML·미리보기 변경

- version.xml은 9/9에서 바뀐다. xmlVersion은 1.4→1.5, appVersion은 원본 10.x/11.x에서 13, 0, 0, 711 WIN32LEWindows_10으로 바뀐다. 설치 빌드 13.0.0.304와는 별도 값이다.
- Contents/header.xml은 9/9에서 version 1.4→1.5다. itemCnt도 재계산된다. 보도자료는 borderFills 14→13, charProperties 95→94, 계획서는 charProperties 301→259, 다중구역 itemCnt는 그대로다.
- Contents/content.hpf는 9/9에서 저장 계정 필드와 ModifiedDate를 갱신하고 BinData 경로 확장자를 대문자로 바꾼다. 저장 계정 문자열은 이 기록에서 생략한다.
- settings.xml의 CaretPosition은 7/9에서 바뀐다. 커서 UI 상태다.
- Preview/PrvText.txt는 보도자료·계획서 6/6에서 바뀌고 S4 문구를 포함한다. 다중구역 3/3은 바뀌지 않는다. 편집 위치가 18쪽인 점과 맞는다.
- Preview/PrvImage.png는 9/9 모두 바이트가 달라지고 크기는 724×1024로 유지된다. 픽셀 동일성은 검사하지 않았다.
- XML namespace URI/prefix 결합 변경은 찾지 못했다. 같은 요소·속성 이름 집합에서 속성 순서 재배열도 발견하지 못했다. XML payload는 달라지고 값·리소스 개수·캐시·저장 메타데이터가 바뀐다.

## 결론과 한계

세 문서의 E 입력은 한컴에서 열기·PDF 출력·HWPX 재저장·재열기에 성공했다는 사용자 기록과 반환물로 확인된다. 재저장 XML은 E가 지운 대상 문단부터 구역 끝까지 캐시를 다시 채웠다. 계획서는 A/E가 20쪽, B가 19쪽이고 B 저장본의 대상 캐시가 남는다. 이 표본은 §11 E 정책을 지지한다.

계획서 B에서 표시 오류가 났다고 단정할 근거는 없다. 문구는 PDF 추출에 온전히 있고 bbox상 본문 단어가 겹치거나 페이지 밖에 있지 않다. 관찰된 차이는 A/E와 다른 한 줄 배치와 추가 빈 2쪽의 부재다. 이번 PDF에서 잘림·겹침은 재현되지 않았지만, 오래된 캐시를 유지하는 정책은 채택하지 않는다.

한컴 재저장은 원본 바이트/XML 표현을 보존하지 않는다. 미디어 경로 대소문자, header itemCnt, version.xml, content.hpf 메타데이터, settings 커서, 미리보기, 여러 구역 XML까지 정규화한다. 엔진의 원본 보존 출력과 한컴 재저장본은 별도 산출물이어야 한다.

한계: 3개 HWPX와 1대의 Windows 10 IoT Enterprise LTSC/한컴오피스 2024 Edu 13.0.0.304 환경에 한정된다. 사용자 기록과 반환물 분석을 근거로 하며 본 세션에서 Windows 조작을 독립 재현하지 않았다. PDF 픽셀 diff·스크린샷, 다른 문서·폰트·프린터·빌드, Linux rhwp와의 시각/좌표 비교, 모든 XML 비텍스트 구조의 의미 동일성은 미확인이다. 0.1 전체 완료나 임의 문서 호환성으로 확대하지 않는다.

## 재현 명령과 비교 절차

저장소 루트 기준:

- 쪽수: pdfinfo spikes/work/windows-return-20261017-2047/return/pdf/2_계획서_B_열기직후.pdf
- 페이지별 텍스트: pdftotext -layout -enc UTF-8 spikes/work/windows-return-20261017-2047/return/pdf/2_계획서_B_열기직후.pdf -
- 단어 bbox: pdftotext -bbox -enc UTF-8 spikes/work/windows-return-20261017-2047/return/pdf/2_계획서_B_열기직후.pdf /tmp/2_계획서_B_열기직후.bbox.html
- 반환물 SHA-256: return 디렉터리에서 sha256sum -c RETURN_SHA256SUMS.txt. 43개 항목 모두 OK.
- Python 3.12 표준 라이브러리 zipfile·hashlib·xml.etree.ElementTree로 ZIP 구성·payload·XML, hp:t 문자열, 최상위 hp:linesegarray를 비교했다.
- 텍스트 누락은 시험 문단과 원문 교체분을 뺀 문자 multiset으로 확인했고, 추가 빈 쪽의 번호는 별도로 분리했다.

코드·scripts/ 및 입력 원본은 수정하지 않았고 커밋하지 않았다.
