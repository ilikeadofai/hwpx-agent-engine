# HWPX 어휘 조사 (tests/fixtures/corpus, 파일 10개)

- 작성일: 2026-10-16
- 입력: `/home/mattc/Documents/Codex/hwpx-agent-engine/tests/fixtures/corpus/*.hwpx` (10개, 원본 수정 없음)
- 스크립트: `/tmp/vocab.py` (저장소 밖. 전문은 보고서 끝에 수록). 실행: `python3 -I /tmp/vocab.py`
- 방법: `zipfile`로 열고 `Contents/header.xml`, `Contents/section*.xml`을 `xml.etree.ElementTree`로 파싱했다. 접두사는 무시하고 네임스페이스 URI로 판정했다. 짧은 태그는 URI의 마지막 두 세그먼트를 쓴다(예: `2011/paragraph:t`).
- 성격: 이 보고서는 구조 통계 조사다. HWPX 명세 해석, 렌더링 검증, 호환성 판단은 포함하지 않는다.

## 요약

- 파싱: 10개 파일 모두 성공. `irb-sample.hwpx`에는 `header.xml`이 없다. `research-school-plan-reference.hwpx`만 section이 28개이고 나머지는 1개다.
- 네임스페이스: `hp`(2011/paragraph), `hh`(2011/head), `hc`(2011/core), `hs`(2011/section) 네 가지만 나타났다.
- (a) 요소-부모 쌍: 요소 46,137개, 서로 다른 쌍 235종. 상위 쌍은 `paragraph:run ← paragraph:p`(3,689), `paragraph:t ← paragraph:run`(2,879), `paragraph:lineseg ← paragraph:linesegarray`(2,000) 순이다.
- (b) hp:t 내부: hp:t 2,879개. 자식 요소는 `tab`(30), `fwSpace`(29), `lineBreak`(10) 세 종류뿐이고, 자손 기준으로도 같은 결과다(더 깊이 중첩된 요소 없음).
- (c) hh:paraPr: 778개(header.xml이 있는 9개 파일). `hh:align@horizontal` 값은 JUSTIFY 351, LEFT 303, CENTER 91, RIGHT 33이다. `hh:lineSpacing@type`은 PERCENT 1,183, FIXED 332이다. `lineSpacing` 1,515개 중 41개는 paraPr 바로 아래에, 737개씩은 `hp:switch/hp:case`와 `hp:switch/hp:default` 아래에 있다. `@value`는 100, 160, 87이 가장 많고 값의 종류가 매우 다양하다.
- (d) 다중 run 문단: hp:p 1,744개 중 직계 hp:run이 2개 이상인 문단은 576개다. 한 문단의 최대 run 수는 21이며 `gyehoek-reference`와 `research-school-plan-reference`에서 나타났다.
- (e) 빈 hp:t: 텍스트도 자식도 없는 hp:t는 129개(전체 hp:t의 약 4.5%)다. 자식 없이 공백만 있는 hp:t는 222개이며, 이는 빈 것으로 세지 않고 참고 값으로만 기록했다.

## 해석 기준

- (a) 요소와 부모 모두 짧은 태그로 표기했다. 루트 요소의 부모는 `(root)`로 적었다. header.xml과 모든 section*.xml을 합산했다.
- (b) 직계 자식과 하위 전체 요소를 각각 집계했다.
- (c) 경로는 hh:paraPr 자식 기준 깊이 1~3의 모든 경로다. 출현 수는 paraPr 안에서 해당 경로의 요소가 나타날 때마다 센 값이다. `hh:align`과 `hh:lineSpacing`은 `hp:switch`, `hp:case`, `hp:default` 안쪽까지 포함해 paraPr의 모든 하위 요소를 훑었다.
- (d) 직계 `hp:run`만 셌다. 표 셀 안의 hp:p도 전부 포함했다(hp:p의 부모는 section, subList, tc 세 종류다).
- (e) `hp:t`의 `text`가 None이거나 빈 문자열이고 자식 요소도 없는 경우를 빈 것으로 봤다.

## 한계

- 조사 대상은 이 저장소의 fixture 10개뿐이다. 다른 HWPX 문서로 일반화하면 안 된다.
- `lineSpacing@value`의 단위는 `type`에 따라 달라질 수 있으나, 이 조사에서는 해석하지 않았다.
- 이 보고서는 빈도 집계 결과다. 저장소의 코드 변경, 커밋, 다른 파일 수정은 하지 않았다.

## 본문 (스크립트 출력)

## 0. 조사 대상과 파싱 결과

| 파일 | header.xml | section 수 | 파싱한 XML 수 | 상태 |
|---|---|---|---|---|
| anchor_char_height | 예 | 1 | 2 | 성공 |
| bodojaryo-reference | 예 | 1 | 2 | 성공 |
| gani-gian | 예 | 1 | 2 | 성공 |
| gyehoek-reference | 예 | 1 | 2 | 성공 |
| ilban-gian | 예 | 1 | 2 | 성공 |
| irb-sample | 아니오 | 1 | 1 | 성공 |
| problem-answer-reference | 예 | 1 | 2 | 성공 |
| ref_empty | 예 | 1 | 2 | 성공 |
| render-p35-font-native-bitmap | 예 | 1 | 2 | 성공 |
| research-school-plan-reference | 예 | 28 | 29 | 성공 |

- 대상 파일 10개, 파싱 성공 10개.

## 0-1. 네임스페이스 URI와 짧은 태그

| 네임스페이스 URI | 짧은 태그 접두 | 요소 수(전체) |
|---|---|---|
| http://www.hancom.co.kr/hwpml/2011/paragraph | 2011/paragraph | 21178 |
| http://www.hancom.co.kr/hwpml/2011/head | 2011/head | 16298 |
| http://www.hancom.co.kr/hwpml/2011/core | 2011/core | 8624 |
| http://www.hancom.co.kr/hwpml/2011/section | 2011/section | 37 |

## 1. (a) 요소-부모 쌍 (요소 ← 부모)

- 요소와 부모 모두 네임스페이스 URI 마지막 두 세그먼트 + `:` + 로컬 이름으로 표기 (접두사 무관).
- 루트 요소의 부모는 `(root)`로 표기. header.xml 과 모든 section*.xml 포함.

| 쌍 (요소 ← 부모) | 합계 | 등장 파일 수 | 파일별 개수 |
|---|---|---|---|
| 2011/paragraph:run ← 2011/paragraph:p | 3689 | 10 | anchor_char_height=23, bodojaryo-reference=163, gani-gian=38, gyehoek-reference=1727, ilban-gian=22, irb-sample=21, problem-answer-reference=82, ref_empty=2, render-p35-font-native-bitmap=2, research-school-plan-reference=1609 |
| 2011/paragraph:t ← 2011/paragraph:run | 2879 | 10 | anchor_char_height=19, bodojaryo-reference=138, gani-gian=24, gyehoek-reference=1656, ilban-gian=30, irb-sample=21, problem-answer-reference=77, ref_empty=1, render-p35-font-native-bitmap=1, research-school-plan-reference=912 |
| 2011/paragraph:lineseg ← 2011/paragraph:linesegarray | 2000 | 8 | bodojaryo-reference=86, gani-gian=1, gyehoek-reference=589, ilban-gian=1, problem-answer-reference=87, ref_empty=1, render-p35-font-native-bitmap=1, research-school-plan-reference=1234 |
| 2011/paragraph:linesegarray ← 2011/paragraph:p | 1646 | 8 | bodojaryo-reference=64, gani-gian=1, gyehoek-reference=489, ilban-gian=1, problem-answer-reference=77, ref_empty=1, render-p35-font-native-bitmap=1, research-school-plan-reference=1012 |
| 2011/core:intent ← 2011/head:margin | 1515 | 9 | anchor_char_height=1, bodojaryo-reference=154, gani-gian=6, gyehoek-reference=484, ilban-gian=10, problem-answer-reference=48, ref_empty=20, render-p35-font-native-bitmap=20, research-school-plan-reference=772 |
| 2011/core:left ← 2011/head:margin | 1515 | 9 | anchor_char_height=1, bodojaryo-reference=154, gani-gian=6, gyehoek-reference=484, ilban-gian=10, problem-answer-reference=48, ref_empty=20, render-p35-font-native-bitmap=20, research-school-plan-reference=772 |
| 2011/core:next ← 2011/head:margin | 1515 | 9 | anchor_char_height=1, bodojaryo-reference=154, gani-gian=6, gyehoek-reference=484, ilban-gian=10, problem-answer-reference=48, ref_empty=20, render-p35-font-native-bitmap=20, research-school-plan-reference=772 |
| 2011/core:prev ← 2011/head:margin | 1515 | 9 | anchor_char_height=1, bodojaryo-reference=154, gani-gian=6, gyehoek-reference=484, ilban-gian=10, problem-answer-reference=48, ref_empty=20, render-p35-font-native-bitmap=20, research-school-plan-reference=772 |
| 2011/core:right ← 2011/head:margin | 1515 | 9 | anchor_char_height=1, bodojaryo-reference=154, gani-gian=6, gyehoek-reference=484, ilban-gian=10, problem-answer-reference=48, ref_empty=20, render-p35-font-native-bitmap=20, research-school-plan-reference=772 |
| 2011/paragraph:p ← 2011/section:sec | 940 | 10 | anchor_char_height=8, bodojaryo-reference=37, gani-gian=17, gyehoek-reference=194, ilban-gian=21, irb-sample=3, problem-answer-reference=21, ref_empty=1, render-p35-font-native-bitmap=1, research-school-plan-reference=637 |
| 2011/paragraph:case ← 2011/paragraph:switch | 790 | 8 | bodojaryo-reference=80, gani-gian=3, gyehoek-reference=250, ilban-gian=5, problem-answer-reference=24, ref_empty=3, render-p35-font-native-bitmap=3, research-school-plan-reference=422 |
| 2011/paragraph:default ← 2011/paragraph:switch | 790 | 8 | bodojaryo-reference=80, gani-gian=3, gyehoek-reference=250, ilban-gian=5, problem-answer-reference=24, ref_empty=3, render-p35-font-native-bitmap=3, research-school-plan-reference=422 |
| 2011/paragraph:p ← 2011/paragraph:subList | 786 | 6 | anchor_char_height=12, bodojaryo-reference=27, gani-gian=20, gyehoek-reference=296, problem-answer-reference=56, research-school-plan-reference=375 |
| 2011/head:align ← 2011/head:paraPr | 778 | 9 | anchor_char_height=1, bodojaryo-reference=77, gani-gian=3, gyehoek-reference=242, ilban-gian=5, problem-answer-reference=24, ref_empty=20, render-p35-font-native-bitmap=20, research-school-plan-reference=386 |
| 2011/head:autoSpacing ← 2011/head:paraPr | 778 | 9 | anchor_char_height=1, bodojaryo-reference=77, gani-gian=3, gyehoek-reference=242, ilban-gian=5, problem-answer-reference=24, ref_empty=20, render-p35-font-native-bitmap=20, research-school-plan-reference=386 |
| 2011/head:border ← 2011/head:paraPr | 778 | 9 | anchor_char_height=1, bodojaryo-reference=77, gani-gian=3, gyehoek-reference=242, ilban-gian=5, problem-answer-reference=24, ref_empty=20, render-p35-font-native-bitmap=20, research-school-plan-reference=386 |
| 2011/head:breakSetting ← 2011/head:paraPr | 778 | 9 | anchor_char_height=1, bodojaryo-reference=77, gani-gian=3, gyehoek-reference=242, ilban-gian=5, problem-answer-reference=24, ref_empty=20, render-p35-font-native-bitmap=20, research-school-plan-reference=386 |
| 2011/head:paraPr ← 2011/head:paraProperties | 778 | 9 | anchor_char_height=1, bodojaryo-reference=77, gani-gian=3, gyehoek-reference=242, ilban-gian=5, problem-answer-reference=24, ref_empty=20, render-p35-font-native-bitmap=20, research-school-plan-reference=386 |
| 2011/head:heading ← 2011/head:paraPr | 769 | 9 | anchor_char_height=1, bodojaryo-reference=77, gani-gian=3, gyehoek-reference=239, ilban-gian=5, problem-answer-reference=24, ref_empty=17, render-p35-font-native-bitmap=17, research-school-plan-reference=386 |
| 2011/paragraph:switch ← 2011/head:paraPr | 746 | 8 | bodojaryo-reference=77, gani-gian=3, gyehoek-reference=245, ilban-gian=5, problem-answer-reference=24, ref_empty=3, render-p35-font-native-bitmap=3, research-school-plan-reference=386 |
| 2011/head:lineSpacing ← 2011/paragraph:case | 737 | 6 | bodojaryo-reference=77, gani-gian=3, gyehoek-reference=242, ilban-gian=5, problem-answer-reference=24, research-school-plan-reference=386 |
| 2011/head:lineSpacing ← 2011/paragraph:default | 737 | 6 | bodojaryo-reference=77, gani-gian=3, gyehoek-reference=242, ilban-gian=5, problem-answer-reference=24, research-school-plan-reference=386 |
| 2011/head:margin ← 2011/paragraph:case | 737 | 6 | bodojaryo-reference=77, gani-gian=3, gyehoek-reference=242, ilban-gian=5, problem-answer-reference=24, research-school-plan-reference=386 |
| 2011/head:margin ← 2011/paragraph:default | 737 | 6 | bodojaryo-reference=77, gani-gian=3, gyehoek-reference=242, ilban-gian=5, problem-answer-reference=24, research-school-plan-reference=386 |
| 2011/paragraph:tc ← 2011/paragraph:tr | 652 | 7 | anchor_char_height=12, bodojaryo-reference=24, gani-gian=20, gyehoek-reference=221, irb-sample=18, problem-answer-reference=49, research-school-plan-reference=308 |
| 2011/paragraph:cellAddr ← 2011/paragraph:tc | 634 | 6 | anchor_char_height=12, bodojaryo-reference=24, gani-gian=20, gyehoek-reference=221, problem-answer-reference=49, research-school-plan-reference=308 |
| 2011/paragraph:cellMargin ← 2011/paragraph:tc | 634 | 6 | anchor_char_height=12, bodojaryo-reference=24, gani-gian=20, gyehoek-reference=221, problem-answer-reference=49, research-school-plan-reference=308 |
| 2011/paragraph:cellSpan ← 2011/paragraph:tc | 634 | 6 | anchor_char_height=12, bodojaryo-reference=24, gani-gian=20, gyehoek-reference=221, problem-answer-reference=49, research-school-plan-reference=308 |
| 2011/paragraph:cellSz ← 2011/paragraph:tc | 634 | 6 | anchor_char_height=12, bodojaryo-reference=24, gani-gian=20, gyehoek-reference=221, problem-answer-reference=49, research-school-plan-reference=308 |
| 2011/paragraph:subList ← 2011/paragraph:tc | 634 | 6 | anchor_char_height=12, bodojaryo-reference=24, gani-gian=20, gyehoek-reference=221, problem-answer-reference=49, research-school-plan-reference=308 |
| 2011/head:charPr ← 2011/head:charProperties | 586 | 9 | anchor_char_height=3, bodojaryo-reference=95, gani-gian=4, gyehoek-reference=301, ilban-gian=4, problem-answer-reference=17, ref_empty=7, render-p35-font-native-bitmap=7, research-school-plan-reference=148 |
| 2011/head:fontRef ← 2011/head:charPr | 586 | 9 | anchor_char_height=3, bodojaryo-reference=95, gani-gian=4, gyehoek-reference=301, ilban-gian=4, problem-answer-reference=17, ref_empty=7, render-p35-font-native-bitmap=7, research-school-plan-reference=148 |
| 2011/head:ratio ← 2011/head:charPr | 586 | 9 | anchor_char_height=3, bodojaryo-reference=95, gani-gian=4, gyehoek-reference=301, ilban-gian=4, problem-answer-reference=17, ref_empty=7, render-p35-font-native-bitmap=7, research-school-plan-reference=148 |
| 2011/head:relSz ← 2011/head:charPr | 586 | 9 | anchor_char_height=3, bodojaryo-reference=95, gani-gian=4, gyehoek-reference=301, ilban-gian=4, problem-answer-reference=17, ref_empty=7, render-p35-font-native-bitmap=7, research-school-plan-reference=148 |
| 2011/head:spacing ← 2011/head:charPr | 586 | 9 | anchor_char_height=3, bodojaryo-reference=95, gani-gian=4, gyehoek-reference=301, ilban-gian=4, problem-answer-reference=17, ref_empty=7, render-p35-font-native-bitmap=7, research-school-plan-reference=148 |
| 2011/head:offset ← 2011/head:charPr | 584 | 9 | anchor_char_height=3, bodojaryo-reference=95, gani-gian=4, gyehoek-reference=301, ilban-gian=2, problem-answer-reference=17, ref_empty=7, render-p35-font-native-bitmap=7, research-school-plan-reference=148 |
| 2011/head:outline ← 2011/head:charPr | 569 | 6 | bodojaryo-reference=95, gani-gian=4, gyehoek-reference=301, ilban-gian=4, problem-answer-reference=17, research-school-plan-reference=148 |
| 2011/head:shadow ← 2011/head:charPr | 569 | 6 | bodojaryo-reference=95, gani-gian=4, gyehoek-reference=301, ilban-gian=4, problem-answer-reference=17, research-school-plan-reference=148 |
| 2011/head:strikeout ← 2011/head:charPr | 569 | 6 | bodojaryo-reference=95, gani-gian=4, gyehoek-reference=301, ilban-gian=4, problem-answer-reference=17, research-school-plan-reference=148 |
| 2011/head:underline ← 2011/head:charPr | 569 | 6 | bodojaryo-reference=95, gani-gian=4, gyehoek-reference=301, ilban-gian=4, problem-answer-reference=17, research-school-plan-reference=148 |
| 2011/head:paraHead ← 2011/head:numbering | 470 | 6 | bodojaryo-reference=10, gyehoek-reference=10, problem-answer-reference=10, ref_empty=10, render-p35-font-native-bitmap=10, research-school-plan-reference=420 |
| 2011/head:font ← 2011/head:fontface | 322 | 9 | anchor_char_height=1, bodojaryo-reference=115, gani-gian=7, gyehoek-reference=72, ilban-gian=7, problem-answer-reference=35, ref_empty=14, render-p35-font-native-bitmap=14, research-school-plan-reference=57 |
| 2011/head:typeInfo ← 2011/head:font | 253 | 6 | bodojaryo-reference=95, gyehoek-reference=65, problem-answer-reference=14, ref_empty=14, render-p35-font-native-bitmap=14, research-school-plan-reference=51 |
| 2011/paragraph:tr ← 2011/paragraph:tbl | 233 | 7 | anchor_char_height=4, bodojaryo-reference=8, gani-gian=9, gyehoek-reference=112, irb-sample=9, problem-answer-reference=43, research-school-plan-reference=48 |
| 2011/head:bold ← 2011/head:charPr | 195 | 6 | bodojaryo-reference=21, gani-gian=2, gyehoek-reference=100, ilban-gian=2, problem-answer-reference=5, research-school-plan-reference=65 |
| 2011/core:rotMatrix ← 2011/paragraph:renderingInfo | 184 | 3 | bodojaryo-reference=6, gyehoek-reference=9, research-school-plan-reference=169 |
| 2011/core:scaMatrix ← 2011/paragraph:renderingInfo | 184 | 3 | bodojaryo-reference=6, gyehoek-reference=9, research-school-plan-reference=169 |
| 2011/head:style ← 2011/head:styles | 165 | 8 | bodojaryo-reference=58, gani-gian=1, gyehoek-reference=30, ilban-gian=1, problem-answer-reference=22, ref_empty=22, render-p35-font-native-bitmap=22, research-school-plan-reference=9 |
| 2011/paragraph:ctrl ← 2011/paragraph:run | 124 | 9 | anchor_char_height=1, bodojaryo-reference=4, gani-gian=27, gyehoek-reference=8, ilban-gian=47, problem-answer-reference=1, ref_empty=1, render-p35-font-native-bitmap=1, research-school-plan-reference=34 |
| 2011/core:pt ← 2011/paragraph:polygon | 120 | 1 | research-school-plan-reference=120 |
| 2011/core:transMatrix ← 2011/paragraph:renderingInfo | 108 | 3 | bodojaryo-reference=6, gyehoek-reference=6, research-school-plan-reference=96 |
| 2011/head:backSlash ← 2011/head:borderFill | 108 | 9 | anchor_char_height=2, bodojaryo-reference=14, gani-gian=4, gyehoek-reference=38, ilban-gian=4, problem-answer-reference=4, ref_empty=2, render-p35-font-native-bitmap=2, research-school-plan-reference=38 |
| 2011/head:borderFill ← 2011/head:borderFills | 108 | 9 | anchor_char_height=2, bodojaryo-reference=14, gani-gian=4, gyehoek-reference=38, ilban-gian=4, problem-answer-reference=4, ref_empty=2, render-p35-font-native-bitmap=2, research-school-plan-reference=38 |
| 2011/head:bottomBorder ← 2011/head:borderFill | 108 | 9 | anchor_char_height=2, bodojaryo-reference=14, gani-gian=4, gyehoek-reference=38, ilban-gian=4, problem-answer-reference=4, ref_empty=2, render-p35-font-native-bitmap=2, research-school-plan-reference=38 |
| 2011/head:leftBorder ← 2011/head:borderFill | 108 | 9 | anchor_char_height=2, bodojaryo-reference=14, gani-gian=4, gyehoek-reference=38, ilban-gian=4, problem-answer-reference=4, ref_empty=2, render-p35-font-native-bitmap=2, research-school-plan-reference=38 |
| 2011/head:rightBorder ← 2011/head:borderFill | 108 | 9 | anchor_char_height=2, bodojaryo-reference=14, gani-gian=4, gyehoek-reference=38, ilban-gian=4, problem-answer-reference=4, ref_empty=2, render-p35-font-native-bitmap=2, research-school-plan-reference=38 |
| 2011/head:slash ← 2011/head:borderFill | 108 | 9 | anchor_char_height=2, bodojaryo-reference=14, gani-gian=4, gyehoek-reference=38, ilban-gian=4, problem-answer-reference=4, ref_empty=2, render-p35-font-native-bitmap=2, research-school-plan-reference=38 |
| 2011/head:topBorder ← 2011/head:borderFill | 108 | 9 | anchor_char_height=2, bodojaryo-reference=14, gani-gian=4, gyehoek-reference=38, ilban-gian=4, problem-answer-reference=4, ref_empty=2, render-p35-font-native-bitmap=2, research-school-plan-reference=38 |
| 2011/paragraph:offset ← 2011/paragraph:pageBorderFill | 102 | 9 | anchor_char_height=1, bodojaryo-reference=3, gani-gian=1, gyehoek-reference=3, ilban-gian=1, problem-answer-reference=3, ref_empty=3, render-p35-font-native-bitmap=3, research-school-plan-reference=84 |
| 2011/paragraph:pageBorderFill ← 2011/paragraph:secPr | 102 | 9 | anchor_char_height=1, bodojaryo-reference=3, gani-gian=1, gyehoek-reference=3, ilban-gian=1, problem-answer-reference=3, ref_empty=3, render-p35-font-native-bitmap=3, research-school-plan-reference=84 |
| 2011/head:diagonal ← 2011/head:borderFill | 98 | 9 | anchor_char_height=2, bodojaryo-reference=12, gani-gian=2, gyehoek-reference=34, ilban-gian=2, problem-answer-reference=4, ref_empty=2, render-p35-font-native-bitmap=2, research-school-plan-reference=38 |
| 2011/core:winBrush ← 2011/core:fillBrush | 81 | 8 | bodojaryo-reference=6, gani-gian=1, gyehoek-reference=15, ilban-gian=2, problem-answer-reference=2, ref_empty=1, render-p35-font-native-bitmap=1, research-school-plan-reference=53 |
| 2011/core:fillBrush ← 2011/head:borderFill | 67 | 8 | bodojaryo-reference=6, gani-gian=1, gyehoek-reference=17, ilban-gian=2, problem-answer-reference=2, ref_empty=1, render-p35-font-native-bitmap=1, research-school-plan-reference=37 |
| 2011/head:fontface ← 2011/head:fontfaces | 57 | 9 | anchor_char_height=1, bodojaryo-reference=7, gani-gian=7, gyehoek-reference=7, ilban-gian=7, problem-answer-reference=7, ref_empty=7, render-p35-font-native-bitmap=7, research-school-plan-reference=7 |
| 2011/head:tabPr ← 2011/head:tabProperties | 57 | 8 | bodojaryo-reference=5, gani-gian=1, gyehoek-reference=8, ilban-gian=1, problem-answer-reference=3, ref_empty=3, render-p35-font-native-bitmap=3, research-school-plan-reference=33 |
| 2011/paragraph:inMargin ← 2011/paragraph:tbl | 55 | 6 | anchor_char_height=2, bodojaryo-reference=5, gani-gian=4, gyehoek-reference=24, problem-answer-reference=12, research-school-plan-reference=8 |
| 2011/paragraph:outMargin ← 2011/paragraph:tbl | 55 | 6 | anchor_char_height=2, bodojaryo-reference=5, gani-gian=4, gyehoek-reference=24, problem-answer-reference=12, research-school-plan-reference=8 |
| 2011/paragraph:pos ← 2011/paragraph:tbl | 55 | 6 | anchor_char_height=2, bodojaryo-reference=5, gani-gian=4, gyehoek-reference=24, problem-answer-reference=12, research-school-plan-reference=8 |
| 2011/paragraph:sz ← 2011/paragraph:tbl | 55 | 6 | anchor_char_height=2, bodojaryo-reference=5, gani-gian=4, gyehoek-reference=24, problem-answer-reference=12, research-school-plan-reference=8 |
| 2011/paragraph:tbl ← 2011/paragraph:run | 55 | 6 | anchor_char_height=2, bodojaryo-reference=5, gani-gian=4, gyehoek-reference=24, problem-answer-reference=12, research-school-plan-reference=8 |
| 2011/paragraph:stringParam ← 2011/paragraph:parameters | 51 | 3 | gani-gian=13, ilban-gian=23, research-school-plan-reference=15 |
| 2011/head:substFont ← 2011/head:font | 48 | 4 | bodojaryo-reference=21, gyehoek-reference=7, problem-answer-reference=14, research-school-plan-reference=6 |
| 2011/head:numbering ← 2011/head:numberings | 47 | 6 | bodojaryo-reference=1, gyehoek-reference=1, problem-answer-reference=1, ref_empty=1, render-p35-font-native-bitmap=1, research-school-plan-reference=42 |
| 2011/head:tabItem ← 2011/paragraph:case | 44 | 3 | bodojaryo-reference=3, gyehoek-reference=5, research-school-plan-reference=36 |
| 2011/head:tabItem ← 2011/paragraph:default | 44 | 3 | bodojaryo-reference=3, gyehoek-reference=5, research-school-plan-reference=36 |
| 2011/paragraph:switch ← 2011/head:tabPr | 44 | 3 | bodojaryo-reference=3, gyehoek-reference=5, research-school-plan-reference=36 |
| 2011/head:supscript ← 2011/head:charPr | 43 | 2 | bodojaryo-reference=1, gyehoek-reference=42 |
| 2011/head:lineSpacing ← 2011/head:paraPr | 41 | 3 | anchor_char_height=1, ref_empty=20, render-p35-font-native-bitmap=20 |
| 2011/head:margin ← 2011/head:paraPr | 41 | 3 | anchor_char_height=1, ref_empty=20, render-p35-font-native-bitmap=20 |
| 2011/paragraph:fieldBegin ← 2011/paragraph:ctrl | 39 | 3 | gani-gian=13, ilban-gian=23, research-school-plan-reference=3 |
| 2011/paragraph:fieldEnd ← 2011/paragraph:ctrl | 39 | 3 | gani-gian=13, ilban-gian=23, research-school-plan-reference=3 |
| 2011/paragraph:parameters ← 2011/paragraph:fieldBegin | 39 | 3 | gani-gian=13, ilban-gian=23, research-school-plan-reference=3 |
| 2011/core:pt0 ← 2011/paragraph:rect | 37 | 2 | gyehoek-reference=2, research-school-plan-reference=35 |
| 2011/core:pt1 ← 2011/paragraph:rect | 37 | 2 | gyehoek-reference=2, research-school-plan-reference=35 |
| 2011/core:pt2 ← 2011/paragraph:rect | 37 | 2 | gyehoek-reference=2, research-school-plan-reference=35 |
| 2011/core:pt3 ← 2011/paragraph:rect | 37 | 2 | gyehoek-reference=2, research-school-plan-reference=35 |
| 2011/paragraph:curSz ← 2011/paragraph:rect | 37 | 2 | gyehoek-reference=2, research-school-plan-reference=35 |
| 2011/paragraph:flip ← 2011/paragraph:rect | 37 | 2 | gyehoek-reference=2, research-school-plan-reference=35 |
| 2011/paragraph:lineShape ← 2011/paragraph:rect | 37 | 2 | gyehoek-reference=2, research-school-plan-reference=35 |
| 2011/paragraph:offset ← 2011/paragraph:rect | 37 | 2 | gyehoek-reference=2, research-school-plan-reference=35 |
| 2011/paragraph:orgSz ← 2011/paragraph:rect | 37 | 2 | gyehoek-reference=2, research-school-plan-reference=35 |
| 2011/paragraph:rect ← 2011/paragraph:container | 37 | 2 | gyehoek-reference=2, research-school-plan-reference=35 |
| 2011/paragraph:renderingInfo ← 2011/paragraph:rect | 37 | 2 | gyehoek-reference=2, research-school-plan-reference=35 |
| 2011/paragraph:rotationInfo ← 2011/paragraph:rect | 37 | 2 | gyehoek-reference=2, research-school-plan-reference=35 |
| 2011/paragraph:shadow ← 2011/paragraph:rect | 37 | 2 | gyehoek-reference=2, research-school-plan-reference=35 |
| 2011/section:sec ← (root) | 37 | 10 | anchor_char_height=1, bodojaryo-reference=1, gani-gian=1, gyehoek-reference=1, ilban-gian=1, irb-sample=1, problem-answer-reference=1, ref_empty=1, render-p35-font-native-bitmap=1, research-school-plan-reference=28 |
| 2011/paragraph:autoNumFormat ← 2011/paragraph:endNotePr | 36 | 9 | anchor_char_height=1, bodojaryo-reference=1, gani-gian=1, gyehoek-reference=1, ilban-gian=1, problem-answer-reference=1, ref_empty=1, render-p35-font-native-bitmap=1, research-school-plan-reference=28 |
| 2011/paragraph:autoNumFormat ← 2011/paragraph:footNotePr | 36 | 9 | anchor_char_height=1, bodojaryo-reference=1, gani-gian=1, gyehoek-reference=1, ilban-gian=1, problem-answer-reference=1, ref_empty=1, render-p35-font-native-bitmap=1, research-school-plan-reference=28 |
| 2011/paragraph:colPr ← 2011/paragraph:ctrl | 36 | 9 | anchor_char_height=1, bodojaryo-reference=1, gani-gian=1, gyehoek-reference=1, ilban-gian=1, problem-answer-reference=1, ref_empty=1, render-p35-font-native-bitmap=1, research-school-plan-reference=28 |
| 2011/paragraph:endNotePr ← 2011/paragraph:secPr | 36 | 9 | anchor_char_height=1, bodojaryo-reference=1, gani-gian=1, gyehoek-reference=1, ilban-gian=1, problem-answer-reference=1, ref_empty=1, render-p35-font-native-bitmap=1, research-school-plan-reference=28 |
| 2011/paragraph:footNotePr ← 2011/paragraph:secPr | 36 | 9 | anchor_char_height=1, bodojaryo-reference=1, gani-gian=1, gyehoek-reference=1, ilban-gian=1, problem-answer-reference=1, ref_empty=1, render-p35-font-native-bitmap=1, research-school-plan-reference=28 |
| 2011/paragraph:grid ← 2011/paragraph:secPr | 36 | 9 | anchor_char_height=1, bodojaryo-reference=1, gani-gian=1, gyehoek-reference=1, ilban-gian=1, problem-answer-reference=1, ref_empty=1, render-p35-font-native-bitmap=1, research-school-plan-reference=28 |
| 2011/paragraph:lineNumberShape ← 2011/paragraph:secPr | 36 | 9 | anchor_char_height=1, bodojaryo-reference=1, gani-gian=1, gyehoek-reference=1, ilban-gian=1, problem-answer-reference=1, ref_empty=1, render-p35-font-native-bitmap=1, research-school-plan-reference=28 |
| 2011/paragraph:margin ← 2011/paragraph:pagePr | 36 | 9 | anchor_char_height=1, bodojaryo-reference=1, gani-gian=1, gyehoek-reference=1, ilban-gian=1, problem-answer-reference=1, ref_empty=1, render-p35-font-native-bitmap=1, research-school-plan-reference=28 |
| 2011/paragraph:noteLine ← 2011/paragraph:endNotePr | 36 | 9 | anchor_char_height=1, bodojaryo-reference=1, gani-gian=1, gyehoek-reference=1, ilban-gian=1, problem-answer-reference=1, ref_empty=1, render-p35-font-native-bitmap=1, research-school-plan-reference=28 |
| 2011/paragraph:noteLine ← 2011/paragraph:footNotePr | 36 | 9 | anchor_char_height=1, bodojaryo-reference=1, gani-gian=1, gyehoek-reference=1, ilban-gian=1, problem-answer-reference=1, ref_empty=1, render-p35-font-native-bitmap=1, research-school-plan-reference=28 |
| 2011/paragraph:noteSpacing ← 2011/paragraph:endNotePr | 36 | 9 | anchor_char_height=1, bodojaryo-reference=1, gani-gian=1, gyehoek-reference=1, ilban-gian=1, problem-answer-reference=1, ref_empty=1, render-p35-font-native-bitmap=1, research-school-plan-reference=28 |
| 2011/paragraph:noteSpacing ← 2011/paragraph:footNotePr | 36 | 9 | anchor_char_height=1, bodojaryo-reference=1, gani-gian=1, gyehoek-reference=1, ilban-gian=1, problem-answer-reference=1, ref_empty=1, render-p35-font-native-bitmap=1, research-school-plan-reference=28 |
| 2011/paragraph:numbering ← 2011/paragraph:endNotePr | 36 | 9 | anchor_char_height=1, bodojaryo-reference=1, gani-gian=1, gyehoek-reference=1, ilban-gian=1, problem-answer-reference=1, ref_empty=1, render-p35-font-native-bitmap=1, research-school-plan-reference=28 |
| 2011/paragraph:numbering ← 2011/paragraph:footNotePr | 36 | 9 | anchor_char_height=1, bodojaryo-reference=1, gani-gian=1, gyehoek-reference=1, ilban-gian=1, problem-answer-reference=1, ref_empty=1, render-p35-font-native-bitmap=1, research-school-plan-reference=28 |
| 2011/paragraph:pagePr ← 2011/paragraph:secPr | 36 | 9 | anchor_char_height=1, bodojaryo-reference=1, gani-gian=1, gyehoek-reference=1, ilban-gian=1, problem-answer-reference=1, ref_empty=1, render-p35-font-native-bitmap=1, research-school-plan-reference=28 |
| 2011/paragraph:placement ← 2011/paragraph:endNotePr | 36 | 9 | anchor_char_height=1, bodojaryo-reference=1, gani-gian=1, gyehoek-reference=1, ilban-gian=1, problem-answer-reference=1, ref_empty=1, render-p35-font-native-bitmap=1, research-school-plan-reference=28 |
| 2011/paragraph:placement ← 2011/paragraph:footNotePr | 36 | 9 | anchor_char_height=1, bodojaryo-reference=1, gani-gian=1, gyehoek-reference=1, ilban-gian=1, problem-answer-reference=1, ref_empty=1, render-p35-font-native-bitmap=1, research-school-plan-reference=28 |
| 2011/paragraph:secPr ← 2011/paragraph:run | 36 | 9 | anchor_char_height=1, bodojaryo-reference=1, gani-gian=1, gyehoek-reference=1, ilban-gian=1, problem-answer-reference=1, ref_empty=1, render-p35-font-native-bitmap=1, research-school-plan-reference=28 |
| 2011/paragraph:startNum ← 2011/paragraph:secPr | 36 | 9 | anchor_char_height=1, bodojaryo-reference=1, gani-gian=1, gyehoek-reference=1, ilban-gian=1, problem-answer-reference=1, ref_empty=1, render-p35-font-native-bitmap=1, research-school-plan-reference=28 |
| 2011/paragraph:visibility ← 2011/paragraph:secPr | 36 | 9 | anchor_char_height=1, bodojaryo-reference=1, gani-gian=1, gyehoek-reference=1, ilban-gian=1, problem-answer-reference=1, ref_empty=1, render-p35-font-native-bitmap=1, research-school-plan-reference=28 |
| 2011/paragraph:tab ← 2011/paragraph:t | 30 | 2 | gyehoek-reference=7, research-school-plan-reference=23 |
| 2011/head:memoPr ← 2011/head:memoProperties | 29 | 2 | bodojaryo-reference=1, research-school-plan-reference=28 |
| 2011/paragraph:fwSpace ← 2011/paragraph:t | 29 | 2 | bodojaryo-reference=1, gyehoek-reference=28 |
| 2011/core:img ← 2011/paragraph:pic | 27 | 3 | bodojaryo-reference=6, gyehoek-reference=3, research-school-plan-reference=18 |
| 2011/core:pt0 ← 2011/paragraph:imgRect | 27 | 3 | bodojaryo-reference=6, gyehoek-reference=3, research-school-plan-reference=18 |
| 2011/core:pt1 ← 2011/paragraph:imgRect | 27 | 3 | bodojaryo-reference=6, gyehoek-reference=3, research-school-plan-reference=18 |
| 2011/core:pt2 ← 2011/paragraph:imgRect | 27 | 3 | bodojaryo-reference=6, gyehoek-reference=3, research-school-plan-reference=18 |
| 2011/core:pt3 ← 2011/paragraph:imgRect | 27 | 3 | bodojaryo-reference=6, gyehoek-reference=3, research-school-plan-reference=18 |
| 2011/paragraph:curSz ← 2011/paragraph:pic | 27 | 3 | bodojaryo-reference=6, gyehoek-reference=3, research-school-plan-reference=18 |
| 2011/paragraph:effects ← 2011/paragraph:pic | 27 | 3 | bodojaryo-reference=6, gyehoek-reference=3, research-school-plan-reference=18 |
| 2011/paragraph:flip ← 2011/paragraph:pic | 27 | 3 | bodojaryo-reference=6, gyehoek-reference=3, research-school-plan-reference=18 |
| 2011/paragraph:imgClip ← 2011/paragraph:pic | 27 | 3 | bodojaryo-reference=6, gyehoek-reference=3, research-school-plan-reference=18 |
| 2011/paragraph:imgDim ← 2011/paragraph:pic | 27 | 3 | bodojaryo-reference=6, gyehoek-reference=3, research-school-plan-reference=18 |
| 2011/paragraph:imgRect ← 2011/paragraph:pic | 27 | 3 | bodojaryo-reference=6, gyehoek-reference=3, research-school-plan-reference=18 |
| 2011/paragraph:inMargin ← 2011/paragraph:pic | 27 | 3 | bodojaryo-reference=6, gyehoek-reference=3, research-school-plan-reference=18 |
| 2011/paragraph:offset ← 2011/paragraph:pic | 27 | 3 | bodojaryo-reference=6, gyehoek-reference=3, research-school-plan-reference=18 |
| 2011/paragraph:orgSz ← 2011/paragraph:pic | 27 | 3 | bodojaryo-reference=6, gyehoek-reference=3, research-school-plan-reference=18 |
| 2011/paragraph:renderingInfo ← 2011/paragraph:pic | 27 | 3 | bodojaryo-reference=6, gyehoek-reference=3, research-school-plan-reference=18 |
| 2011/paragraph:rotationInfo ← 2011/paragraph:pic | 27 | 3 | bodojaryo-reference=6, gyehoek-reference=3, research-school-plan-reference=18 |
| 2011/paragraph:subList ← 2011/paragraph:drawText | 26 | 2 | gyehoek-reference=2, research-school-plan-reference=24 |
| 2011/paragraph:textMargin ← 2011/paragraph:drawText | 26 | 2 | gyehoek-reference=2, research-school-plan-reference=24 |
| 2011/paragraph:curSz ← 2011/paragraph:polygon | 24 | 1 | research-school-plan-reference=24 |
| 2011/paragraph:drawText ← 2011/paragraph:polygon | 24 | 1 | research-school-plan-reference=24 |
| 2011/paragraph:flip ← 2011/paragraph:polygon | 24 | 1 | research-school-plan-reference=24 |
| 2011/paragraph:lineShape ← 2011/paragraph:polygon | 24 | 1 | research-school-plan-reference=24 |
| 2011/paragraph:offset ← 2011/paragraph:polygon | 24 | 1 | research-school-plan-reference=24 |
| 2011/paragraph:orgSz ← 2011/paragraph:polygon | 24 | 1 | research-school-plan-reference=24 |
| 2011/paragraph:polygon ← 2011/paragraph:container | 24 | 1 | research-school-plan-reference=24 |
| 2011/paragraph:renderingInfo ← 2011/paragraph:polygon | 24 | 1 | research-school-plan-reference=24 |
| 2011/paragraph:rotationInfo ← 2011/paragraph:polygon | 24 | 1 | research-school-plan-reference=24 |
| 2011/paragraph:shadow ← 2011/paragraph:polygon | 24 | 1 | research-school-plan-reference=24 |
| 2011/paragraph:container ← 2011/paragraph:run | 20 | 2 | gyehoek-reference=1, research-school-plan-reference=19 |
| 2011/paragraph:curSz ← 2011/paragraph:container | 20 | 2 | gyehoek-reference=1, research-school-plan-reference=19 |
| 2011/paragraph:flip ← 2011/paragraph:container | 20 | 2 | gyehoek-reference=1, research-school-plan-reference=19 |
| 2011/paragraph:offset ← 2011/paragraph:container | 20 | 2 | gyehoek-reference=1, research-school-plan-reference=19 |
| 2011/paragraph:orgSz ← 2011/paragraph:container | 20 | 2 | gyehoek-reference=1, research-school-plan-reference=19 |
| 2011/paragraph:outMargin ← 2011/paragraph:container | 20 | 2 | gyehoek-reference=1, research-school-plan-reference=19 |
| 2011/paragraph:pos ← 2011/paragraph:container | 20 | 2 | gyehoek-reference=1, research-school-plan-reference=19 |
| 2011/paragraph:renderingInfo ← 2011/paragraph:container | 20 | 2 | gyehoek-reference=1, research-school-plan-reference=19 |
| 2011/paragraph:rotationInfo ← 2011/paragraph:container | 20 | 2 | gyehoek-reference=1, research-school-plan-reference=19 |
| 2011/paragraph:sz ← 2011/paragraph:container | 20 | 2 | gyehoek-reference=1, research-school-plan-reference=19 |
| 2011/paragraph:shapeComment ← 2011/paragraph:container | 19 | 1 | research-school-plan-reference=19 |
| 2011/paragraph:p ← 2011/paragraph:tc | 18 | 1 | irb-sample=18 |
| 2011/core:fillBrush ← 2011/paragraph:rect | 16 | 1 | research-school-plan-reference=16 |
| 2011/paragraph:pic ← 2011/paragraph:container | 15 | 2 | gyehoek-reference=1, research-school-plan-reference=14 |
| 2011/paragraph:outMargin ← 2011/paragraph:pic | 12 | 3 | bodojaryo-reference=6, gyehoek-reference=2, research-school-plan-reference=4 |
| 2011/paragraph:pic ← 2011/paragraph:run | 12 | 3 | bodojaryo-reference=6, gyehoek-reference=2, research-school-plan-reference=4 |
| 2011/paragraph:pos ← 2011/paragraph:pic | 12 | 3 | bodojaryo-reference=6, gyehoek-reference=2, research-school-plan-reference=4 |
| 2011/paragraph:sz ← 2011/paragraph:pic | 12 | 3 | bodojaryo-reference=6, gyehoek-reference=2, research-school-plan-reference=4 |
| 2011/paragraph:shapeComment ← 2011/paragraph:pic | 11 | 3 | bodojaryo-reference=5, gyehoek-reference=2, research-school-plan-reference=4 |
| 2011/paragraph:lineBreak ← 2011/paragraph:t | 10 | 2 | gyehoek-reference=8, research-school-plan-reference=2 |
| 2011/head:borderFills ← 2011/head:refList | 9 | 9 | anchor_char_height=1, bodojaryo-reference=1, gani-gian=1, gyehoek-reference=1, ilban-gian=1, problem-answer-reference=1, ref_empty=1, render-p35-font-native-bitmap=1, research-school-plan-reference=1 |
| 2011/head:charProperties ← 2011/head:refList | 9 | 9 | anchor_char_height=1, bodojaryo-reference=1, gani-gian=1, gyehoek-reference=1, ilban-gian=1, problem-answer-reference=1, ref_empty=1, render-p35-font-native-bitmap=1, research-school-plan-reference=1 |
| 2011/head:compatibleDocument ← 2011/head:head | 9 | 9 | anchor_char_height=1, bodojaryo-reference=1, gani-gian=1, gyehoek-reference=1, ilban-gian=1, problem-answer-reference=1, ref_empty=1, render-p35-font-native-bitmap=1, research-school-plan-reference=1 |
| 2011/head:fontfaces ← 2011/head:refList | 9 | 9 | anchor_char_height=1, bodojaryo-reference=1, gani-gian=1, gyehoek-reference=1, ilban-gian=1, problem-answer-reference=1, ref_empty=1, render-p35-font-native-bitmap=1, research-school-plan-reference=1 |
| 2011/head:head ← (root) | 9 | 9 | anchor_char_height=1, bodojaryo-reference=1, gani-gian=1, gyehoek-reference=1, ilban-gian=1, problem-answer-reference=1, ref_empty=1, render-p35-font-native-bitmap=1, research-school-plan-reference=1 |
| 2011/head:heading ← 2011/paragraph:case | 9 | 3 | gyehoek-reference=3, ref_empty=3, render-p35-font-native-bitmap=3 |
| 2011/head:heading ← 2011/paragraph:default | 9 | 3 | gyehoek-reference=3, ref_empty=3, render-p35-font-native-bitmap=3 |
| 2011/head:layoutCompatibility ← 2011/head:compatibleDocument | 9 | 9 | anchor_char_height=1, bodojaryo-reference=1, gani-gian=1, gyehoek-reference=1, ilban-gian=1, problem-answer-reference=1, ref_empty=1, render-p35-font-native-bitmap=1, research-school-plan-reference=1 |
| 2011/head:paraProperties ← 2011/head:refList | 9 | 9 | anchor_char_height=1, bodojaryo-reference=1, gani-gian=1, gyehoek-reference=1, ilban-gian=1, problem-answer-reference=1, ref_empty=1, render-p35-font-native-bitmap=1, research-school-plan-reference=1 |
| 2011/head:refList ← 2011/head:head | 9 | 9 | anchor_char_height=1, bodojaryo-reference=1, gani-gian=1, gyehoek-reference=1, ilban-gian=1, problem-answer-reference=1, ref_empty=1, render-p35-font-native-bitmap=1, research-school-plan-reference=1 |
| 2011/head:beginNum ← 2011/head:head | 8 | 8 | bodojaryo-reference=1, gani-gian=1, gyehoek-reference=1, ilban-gian=1, problem-answer-reference=1, ref_empty=1, render-p35-font-native-bitmap=1, research-school-plan-reference=1 |
| 2011/head:docOption ← 2011/head:head | 8 | 8 | bodojaryo-reference=1, gani-gian=1, gyehoek-reference=1, ilban-gian=1, problem-answer-reference=1, ref_empty=1, render-p35-font-native-bitmap=1, research-school-plan-reference=1 |
| 2011/head:linkinfo ← 2011/head:docOption | 8 | 8 | bodojaryo-reference=1, gani-gian=1, gyehoek-reference=1, ilban-gian=1, problem-answer-reference=1, ref_empty=1, render-p35-font-native-bitmap=1, research-school-plan-reference=1 |
| 2011/head:styles ← 2011/head:refList | 8 | 8 | bodojaryo-reference=1, gani-gian=1, gyehoek-reference=1, ilban-gian=1, problem-answer-reference=1, ref_empty=1, render-p35-font-native-bitmap=1, research-school-plan-reference=1 |
| 2011/head:tabProperties ← 2011/head:refList | 8 | 8 | bodojaryo-reference=1, gani-gian=1, gyehoek-reference=1, ilban-gian=1, problem-answer-reference=1, ref_empty=1, render-p35-font-native-bitmap=1, research-school-plan-reference=1 |
| 2011/head:trackchageConfig ← 2011/head:head | 8 | 8 | bodojaryo-reference=1, gani-gian=1, gyehoek-reference=1, ilban-gian=1, problem-answer-reference=1, ref_empty=1, render-p35-font-native-bitmap=1, research-school-plan-reference=1 |
| 2011/head:numberings ← 2011/head:refList | 6 | 6 | bodojaryo-reference=1, gyehoek-reference=1, problem-answer-reference=1, ref_empty=1, render-p35-font-native-bitmap=1, research-school-plan-reference=1 |
| 2011/core:color ← 2011/core:gradation | 4 | 1 | gyehoek-reference=4 |
| 2011/paragraph:header ← 2011/paragraph:ctrl | 3 | 2 | bodojaryo-reference=1, gyehoek-reference=2 |
| 2011/paragraph:integerParam ← 2011/paragraph:parameters | 3 | 1 | research-school-plan-reference=3 |
| 2011/paragraph:pageHiding ← 2011/paragraph:ctrl | 3 | 1 | gyehoek-reference=3 |
| 2011/paragraph:subList ← 2011/paragraph:header | 3 | 2 | bodojaryo-reference=1, gyehoek-reference=2 |
| 2011/core:gradation ← 2011/core:fillBrush | 2 | 1 | gyehoek-reference=2 |
| 2011/head:char ← 2011/head:layoutCompatibility | 2 | 2 | gani-gian=1, ilban-gian=1 |
| 2011/head:field ← 2011/head:layoutCompatibility | 2 | 2 | gani-gian=1, ilban-gian=1 |
| 2011/head:memoProperties ← 2011/head:refList | 2 | 2 | bodojaryo-reference=1, research-school-plan-reference=1 |
| 2011/head:object ← 2011/head:layoutCompatibility | 2 | 2 | gani-gian=1, ilban-gian=1 |
| 2011/head:paragraph ← 2011/head:layoutCompatibility | 2 | 2 | gani-gian=1, ilban-gian=1 |
| 2011/head:section ← 2011/head:layoutCompatibility | 2 | 2 | gani-gian=1, ilban-gian=1 |
| 2011/paragraph:drawText ← 2011/paragraph:rect | 2 | 1 | gyehoek-reference=2 |
| 2011/paragraph:newNum ← 2011/paragraph:ctrl | 2 | 2 | bodojaryo-reference=1, gyehoek-reference=1 |
| 2011/paragraph:pageNum ← 2011/paragraph:ctrl | 2 | 2 | bodojaryo-reference=1, gyehoek-reference=1 |
| 2011/paragraph:tbl ← 2011/section:sec | 2 | 1 | irb-sample=2 |
| 2011/head:adjustBaselineInFixedLinespacing ← 2011/head:layoutCompatibility | 1 | 1 | research-school-plan-reference=1 |
| 2011/head:adjustBaselineOfObjectToBottom ← 2011/head:layoutCompatibility | 1 | 1 | research-school-plan-reference=1 |
| 2011/head:adjustLineheightToFont ← 2011/head:layoutCompatibility | 1 | 1 | research-school-plan-reference=1 |
| 2011/head:adjustMarginFromAdjustLineheight ← 2011/head:layoutCompatibility | 1 | 1 | research-school-plan-reference=1 |
| 2011/head:adjustParaBorderOffsetWithBorder ← 2011/head:layoutCompatibility | 1 | 1 | research-school-plan-reference=1 |
| 2011/head:adjustParaBorderfillToSpacing ← 2011/head:layoutCompatibility | 1 | 1 | research-school-plan-reference=1 |
| 2011/head:adjustVertPosOfLine ← 2011/head:layoutCompatibility | 1 | 1 | research-school-plan-reference=1 |
| 2011/head:applyCharSpacingToCharGrid ← 2011/head:layoutCompatibility | 1 | 1 | research-school-plan-reference=1 |
| 2011/head:applyExtendHeaderFooterEachSection ← 2011/head:layoutCompatibility | 1 | 1 | research-school-plan-reference=1 |
| 2011/head:applyFontWeightToBold ← 2011/head:layoutCompatibility | 1 | 1 | research-school-plan-reference=1 |
| 2011/head:applyMinColumnWidthTo1mm ← 2011/head:layoutCompatibility | 1 | 1 | research-school-plan-reference=1 |
| 2011/head:applyNextspacingOfLastPara ← 2011/head:layoutCompatibility | 1 | 1 | research-school-plan-reference=1 |
| 2011/head:applyParaBorderToOutside ← 2011/head:layoutCompatibility | 1 | 1 | research-school-plan-reference=1 |
| 2011/head:applyPrevspacingBeneathObject ← 2011/head:layoutCompatibility | 1 | 1 | research-school-plan-reference=1 |
| 2011/head:applyTabPosBasedOnSegment ← 2011/head:layoutCompatibility | 1 | 1 | research-school-plan-reference=1 |
| 2011/head:baseCharUnitOfIndentOnFirstChar ← 2011/head:layoutCompatibility | 1 | 1 | research-school-plan-reference=1 |
| 2011/head:baseCharUnitOnEAsian ← 2011/head:layoutCompatibility | 1 | 1 | research-school-plan-reference=1 |
| 2011/head:baseLineSpacingOnLineGrid ← 2011/head:layoutCompatibility | 1 | 1 | research-school-plan-reference=1 |
| 2011/head:breakTabOverLine ← 2011/head:layoutCompatibility | 1 | 1 | research-school-plan-reference=1 |
| 2011/head:connectParaBorderfillOfEqualBorder ← 2011/head:layoutCompatibility | 1 | 1 | research-school-plan-reference=1 |
| 2011/head:doNotAdjustEmptyAnchorLine ← 2011/head:layoutCompatibility | 1 | 1 | research-school-plan-reference=1 |
| 2011/head:doNotAdjustWordInJustify ← 2011/head:layoutCompatibility | 1 | 1 | research-school-plan-reference=1 |
| 2011/head:doNotAlignLastForbidden ← 2011/head:layoutCompatibility | 1 | 1 | research-school-plan-reference=1 |
| 2011/head:doNotAlignWhitespaceOnRight ← 2011/head:layoutCompatibility | 1 | 1 | research-school-plan-reference=1 |
| 2011/head:doNotApplyGridInHeaderFooter ← 2011/head:layoutCompatibility | 1 | 1 | research-school-plan-reference=1 |
| 2011/head:doNotApplyLinegridAtNoLinespacing ← 2011/head:layoutCompatibility | 1 | 1 | research-school-plan-reference=1 |
| 2011/head:doNotFormattingAtBeneathAnchor ← 2011/head:layoutCompatibility | 1 | 1 | research-school-plan-reference=1 |
| 2011/head:doNotHoldAnchorOfTable ← 2011/head:layoutCompatibility | 1 | 1 | research-school-plan-reference=1 |
| 2011/head:extendLineheightToOffset ← 2011/head:layoutCompatibility | 1 | 1 | research-school-plan-reference=1 |
| 2011/head:extendLineheightToParaBorderOffset ← 2011/head:layoutCompatibility | 1 | 1 | research-school-plan-reference=1 |
| 2011/head:extendVertLimitToPageMargins ← 2011/head:layoutCompatibility | 1 | 1 | research-school-plan-reference=1 |
| 2011/head:overlapBothAllowOverlap ← 2011/head:layoutCompatibility | 1 | 1 | research-school-plan-reference=1 |
| 2011/head:treatQuotationAsLatin ← 2011/head:layoutCompatibility | 1 | 1 | research-school-plan-reference=1 |
| 2011/head:useInnerUnderline ← 2011/head:layoutCompatibility | 1 | 1 | research-school-plan-reference=1 |
| 2011/head:useLowercaseStrikeout ← 2011/head:layoutCompatibility | 1 | 1 | research-school-plan-reference=1 |

- 서로 다른 쌍 235종, 요소 총수 46137.

## 2. (b) hp:t 내부 요소

- hp:t 총수(전체 파일): 2879

### 2-1. hp:t 직계 자식

| 자식 요소 | 합계 | 등장 파일 수 | 파일별 개수 |
|---|---|---|---|
| 2011/paragraph:tab | 30 | 2 | gyehoek-reference=7, research-school-plan-reference=23 |
| 2011/paragraph:fwSpace | 29 | 2 | bodojaryo-reference=1, gyehoek-reference=28 |
| 2011/paragraph:lineBreak | 10 | 2 | gyehoek-reference=8, research-school-plan-reference=2 |

### 2-2. hp:t 하위 전체 요소 (자손 기준)

| 하위 요소 | 합계 | 등장 파일 수 | 파일별 개수 |
|---|---|---|---|
| 2011/paragraph:tab | 30 | 2 | gyehoek-reference=7, research-school-plan-reference=23 |
| 2011/paragraph:fwSpace | 29 | 2 | bodojaryo-reference=1, gyehoek-reference=28 |
| 2011/paragraph:lineBreak | 10 | 2 | gyehoek-reference=8, research-school-plan-reference=2 |

## 3. (c) hh:paraPr (header.xml)

- hh:paraPr 총수: 778
- 파일별 hh:paraPr 수: anchor_char_height=1, bodojaryo-reference=77, gani-gian=3, gyehoek-reference=242, ilban-gian=5, problem-answer-reference=24, ref_empty=20, render-p35-font-native-bitmap=20, research-school-plan-reference=386

### 3-1. paraPr 자식 경로 (깊이 3 이하, 서로 다른 경로)

| 경로 (hh:paraPr 기준) | 출현 수 | 등장 파일 수 |
|---|---|---|
| 2011/head:align | 778 | 9 |
| 2011/head:autoSpacing | 778 | 9 |
| 2011/head:border | 778 | 9 |
| 2011/head:breakSetting | 778 | 9 |
| 2011/head:heading | 769 | 9 |
| 2011/head:lineSpacing | 41 | 3 |
| 2011/head:margin | 41 | 3 |
| 2011/paragraph:switch | 746 | 8 |
| 2011/head:margin > 2011/core:intent | 41 | 3 |
| 2011/head:margin > 2011/core:left | 41 | 3 |
| 2011/head:margin > 2011/core:next | 41 | 3 |
| 2011/head:margin > 2011/core:prev | 41 | 3 |
| 2011/head:margin > 2011/core:right | 41 | 3 |
| 2011/paragraph:switch > 2011/paragraph:case | 746 | 8 |
| 2011/paragraph:switch > 2011/paragraph:default | 746 | 8 |
| 2011/paragraph:switch > 2011/paragraph:case > 2011/head:heading | 9 | 3 |
| 2011/paragraph:switch > 2011/paragraph:case > 2011/head:lineSpacing | 737 | 6 |
| 2011/paragraph:switch > 2011/paragraph:case > 2011/head:margin | 737 | 6 |
| 2011/paragraph:switch > 2011/paragraph:default > 2011/head:heading | 9 | 3 |
| 2011/paragraph:switch > 2011/paragraph:default > 2011/head:lineSpacing | 737 | 6 |
| 2011/paragraph:switch > 2011/paragraph:default > 2011/head:margin | 737 | 6 |

### 3-2. hh:align@horizontal 값 (hp:switch/hp:case/hp:default 포함)

- hh:align 총수: 778

| horizontal 값 | 출현 수 |
|---|---|
| JUSTIFY | 351 |
| LEFT | 303 |
| CENTER | 91 |
| RIGHT | 33 |

### 3-3. hh:lineSpacing@type 값 (hp:switch/hp:case/hp:default 포함)

- hh:lineSpacing 총수: 1515

| type 값 | 출현 수 |
|---|---|
| PERCENT | 1183 |
| FIXED | 332 |

### 3-4. hh:lineSpacing@value 값 (hp:switch/hp:case/hp:default 포함)

| value 값 | 출현 수 |
|---|---|
| 100 | 256 |
| 160 | 227 |
| 87 | 184 |
| 145 | 100 |
| 130 | 98 |
| 140 | 94 |
| 150 | 84 |
| 4100 | 40 |
| 2050 | 40 |
| 3840 | 31 |
| 1920 | 31 |
| 175 | 28 |
| 155 | 22 |
| 3730 | 16 |
| 1865 | 16 |
| 170 | 14 |
| 120 | 14 |
| 71 | 12 |
| 3420 | 7 |
| 1710 | 7 |
| 114 | 6 |
| 110 | 6 |
| 340 | 6 |
| 2870 | 6 |
| 1435 | 6 |
| 3170 | 6 |
| 1585 | 6 |
| 50 | 4 |
| 180 | 4 |
| 115 | 4 |
| 3990 | 4 |
| 1995 | 4 |
| 90 | 4 |
| 4600 | 4 |
| 2300 | 4 |
| 3740 | 3 |
| 1870 | 3 |
| 3350 | 3 |
| 1675 | 3 |
| 4070 | 3 |
| 2035 | 3 |
| 3430 | 3 |
| 1715 | 3 |
| 4650 | 3 |
| 2325 | 3 |
| 4500 | 3 |
| 2250 | 3 |
| 40 | 2 |
| 20 | 2 |
| 60 | 2 |
| 200 | 2 |
| 135 | 2 |
| 300 | 2 |
| 210 | 2 |
| 3210 | 2 |
| 1605 | 2 |
| 3340 | 2 |
| 1670 | 2 |
| 3320 | 2 |
| 1660 | 2 |
| 3180 | 2 |
| 1590 | 2 |
| 3150 | 2 |
| 1575 | 2 |
| 105 | 2 |
| 4150 | 2 |
| 2075 | 2 |
| 4250 | 2 |
| 2125 | 2 |
| 4190 | 2 |
| 2095 | 2 |
| 3940 | 1 |
| 1970 | 1 |
| 3560 | 1 |
| 1780 | 1 |
| 2800 | 1 |
| 1400 | 1 |
| 3750 | 1 |
| 1875 | 1 |
| 3930 | 1 |
| 1965 | 1 |
| 3220 | 1 |
| 1610 | 1 |
| 3690 | 1 |
| 1845 | 1 |
| 3950 | 1 |
| 1975 | 1 |
| 4000 | 1 |
| 2000 | 1 |
| 6050 | 1 |
| 3025 | 1 |
| 3190 | 1 |
| 1595 | 1 |
| 3000 | 1 |
| 1500 | 1 |
| 810 | 1 |
| 405 | 1 |
| 4200 | 1 |
| 2100 | 1 |
| 4660 | 1 |
| 2330 | 1 |
| 740 | 1 |
| 370 | 1 |
| 5040 | 1 |
| 2520 | 1 |
| 3250 | 1 |
| 1625 | 1 |

## 4. (d) 직계 hp:run 이 2개 이상인 hp:p

| 파일 | hp:p 수 | run 2개 이상 문단 수 | 문단당 최대 run 수 |
|---|---|---|---|
| anchor_char_height | 20 | 3 | 2 |
| bodojaryo-reference | 64 | 19 | 14 |
| gani-gian | 37 | 1 | 2 |
| gyehoek-reference | 490 | 189 | 21 |
| ilban-gian | 21 | 1 | 2 |
| irb-sample | 21 | 0 | 1 |
| problem-answer-reference | 77 | 5 | 2 |
| ref_empty | 1 | 1 | 2 |
| render-p35-font-native-bitmap | 1 | 1 | 2 |
| research-school-plan-reference | 1012 | 356 | 21 |
| **합계** | 1744 | 576 | 21 |

- 최대값 열의 합계 행은 전체 파일 중 최댓값.

## 5. (e) 빈 hp:t

- 빈 hp:t: 텍스트가 없고(`text` 가 None 또는 빈 문자열) 자식 요소도 없는 hp:t.
- 참고로 자식 없이 공백 문자만 있는 hp:t 를 따로 셌다(빈 것으로 세지 않음).

| 파일 | hp:t 수 | 빈 hp:t 수 | 공백만 있는 hp:t 수(참고) |
|---|---|---|---|
| anchor_char_height | 19 | 0 | 0 |
| bodojaryo-reference | 138 | 10 | 6 |
| gani-gian | 24 | 19 | 0 |
| gyehoek-reference | 1656 | 30 | 94 |
| ilban-gian | 30 | 8 | 1 |
| irb-sample | 21 | 10 | 0 |
| problem-answer-reference | 77 | 12 | 0 |
| ref_empty | 1 | 1 | 0 |
| render-p35-font-native-bitmap | 1 | 0 | 0 |
| research-school-plan-reference | 912 | 39 | 121 |
| **합계** | 2879 | 129 | 222 |


## 부록: 스크립트 전문 (/tmp/vocab.py)

```python
#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""HWPX corpus 어휘 조사 스크립트 (Python 3 표준 라이브러리만 사용).

corpus 안의 모든 .hwpx 파일을 zipfile로 열어 Contents/header.xml 과
Contents/section*.xml 을 xml.etree.ElementTree 로 파싱하고, 요소 어휘를 집계한다.
결과는 Markdown 본문으로 stdout 에 출력한다.

실행: python3 -I /tmp/vocab.py > result_body.md
"""
import glob
import os
import re
import sys
import zipfile
import xml.etree.ElementTree as ET
from collections import Counter

CORPUS = "/home/mattc/Documents/Codex/hwpx-agent-engine/tests/fixtures/corpus"
NS_P = "http://www.hancom.co.kr/hwpml/2011/paragraph"  # hp:
NS_H = "http://www.hancom.co.kr/hwpml/2011/head"        # hh:
PATH_DEPTH = 3


def split_tag(name):
    """'{uri}local' -> (uri, local). 네임스페이스가 없으면 ('', name)."""
    if name.startswith("{"):
        uri, local = name[1:].split("}", 1)
        return uri, local
    return "", name


def short(name):
    """네임스페이스 URI 의 마지막 두 경로 세그먼트 + ':' + 로컬 이름. 예: 2011/paragraph:t"""
    uri, local = split_tag(name)
    if not uri:
        return "(no-ns):" + local
    segs = [s for s in uri.split("/") if s]
    pre = "/".join(segs[-2:]) if len(segs) >= 2 else segs[-1]
    return pre + ":" + local


def is_tag(name, uri, local):
    u, l = split_tag(name)
    return u == uri and l == local


def local_attrs(attrib):
    """속성 이름에서 네임스페이스를 떼고 {local: value} 로 반환."""
    return {split_tag(k)[1]: v for k, v in attrib.items()}


def iter_with_parent(root):
    """(element, parent_element_or_None) 을 모든 요소에 대해 순회한다."""
    stack = [(root, None)]
    while stack:
        el, par = stack.pop()
        yield el, par
        for c in el:
            stack.append((c, el))


def descendants(el):
    """el 의 모든 하위 요소(자기 자신 제외)."""
    stack = list(el)
    while stack:
        c = stack.pop()
        yield c
        stack.extend(c)


def child_paths(el, prefix, depth, out):
    """el 의 자식 경로를 깊이 PATH_DEPTH 까지 out 에 ' > ' 로 이은 문자열로 추가."""
    if depth >= PATH_DEPTH:
        return
    for c in el:
        p = prefix + [short(c.tag)]
        out.append(" > ".join(p))
        child_paths(c, p, depth + 1, out)


def section_index(name):
    return int(re.search(r"section(\d+)\.xml$", name).group(1))


def load_parts(path):
    """header.xml 과 section*.xml 을 파싱해 [(entry 이름, root)] 를 반환한다."""
    with zipfile.ZipFile(path) as z:
        names = z.namelist()
        has_header = "Contents/header.xml" in names
        sections = sorted(
            (n for n in names if re.fullmatch(r"Contents/section\d+\.xml", n)),
            key=section_index,
        )
        entries = (["Contents/header.xml"] if has_header else []) + sections
        parts = [(n, ET.fromstring(z.read(n))) for n in entries]
    return parts, has_header, len(sections)


def analyze(path):
    parts, has_header, nsec = load_parts(path)
    r = {
        "has_header": has_header,
        "nsec": nsec,
        "parts": [n for n, _ in parts],
        "ns": Counter(),            # 네임스페이스 URI -> 요소 수
        "pairs": Counter(),         # (요소, 부모) -> 수      [a]
        "t_total": 0,               # hp:t 수                [b, e]
        "t_child": Counter(),       # hp:t 직계 자식          [b]
        "t_desc": Counter(),        # hp:t 모든 하위 요소     [b]
        "t_empty": 0,               # 텍스트·자식 모두 없음   [e]
        "t_ws": 0,                  # 공백만 있음(자식 없음)  [e, 참고]
        "p_total": 0,               # hp:p 수                 [d]
        "p_multi": 0,               # 직계 hp:run 이 2개 이상인 hp:p
        "p_maxrun": 0,              # 한 문단의 최대 직계 hp:run 수
        "para_total": 0,            # hh:paraPr 수            [c]
        "para_paths": Counter(),    # paraPr 기준 자식 경로(깊이<=3) -> 출현 수
        "align_total": 0,
        "align_h": Counter(),       # hh:align@horizontal 값
        "ls_total": 0,
        "ls_type": Counter(),       # hh:lineSpacing@type 값
        "ls_value": Counter(),      # hh:lineSpacing@value 값
    }
    for _entry, root in parts:
        for el, par in iter_with_parent(root):
            uri, local = split_tag(el.tag)
            r["ns"][uri] += 1

            # (a) (요소, 부모) 쌍
            parent_key = short(par.tag) if par is not None else "(root)"
            r["pairs"][(short(el.tag), parent_key)] += 1

            # (b), (e) hp:t
            if is_tag(el.tag, NS_P, "t"):
                r["t_total"] += 1
                for c in el:
                    r["t_child"][short(c.tag)] += 1
                for d in descendants(el):
                    r["t_desc"][short(d.tag)] += 1
                text = el.text or ""
                if len(el) == 0:
                    if text == "":
                        r["t_empty"] += 1
                    elif text.strip() == "":
                        r["t_ws"] += 1

            # (d) hp:p 의 직계 hp:run 개수
            if is_tag(el.tag, NS_P, "p"):
                r["p_total"] += 1
                runs = sum(1 for c in el if is_tag(c.tag, NS_P, "run"))
                if runs > 1:
                    r["p_multi"] += 1
                r["p_maxrun"] = max(r["p_maxrun"], runs)

            # (c) hh:paraPr
            if is_tag(el.tag, NS_H, "paraPr"):
                r["para_total"] += 1
                paths = []
                child_paths(el, [], 0, paths)
                for p in paths:
                    r["para_paths"][p] += 1
                # hp:switch/hp:case/hp:default 안쪽까지 모두 포함하기 위해 하위 전체 순회
                for d in descendants(el):
                    if is_tag(d.tag, NS_H, "align"):
                        r["align_total"] += 1
                        r["align_h"][local_attrs(d.attrib).get("horizontal", "(속성없음)")] += 1
                    elif is_tag(d.tag, NS_H, "lineSpacing"):
                        a = local_attrs(d.attrib)
                        r["ls_total"] += 1
                        r["ls_type"][a.get("type", "(속성없음)")] += 1
                        r["ls_value"][a.get("value", "(속성없음)")] += 1
    return r


def md_table(headers, rows):
    lines = ["| " + " | ".join(headers) + " |",
             "|" + "|".join("---" for _ in headers) + "|"]
    for row in rows:
        lines.append("| " + " | ".join(str(x) for x in row) + " |")
    return lines


def per_file_str(labels, counts):
    return ", ".join(f"{labels[i]}={counts[i]}" for i in range(len(labels)) if counts[i])


def main():
    paths = sorted(glob.glob(os.path.join(CORPUS, "*.hwpx")))
    if not paths:
        print("hwpx 파일을 찾지 못했습니다: " + CORPUS, file=sys.stderr)
        return 1

    labels, res, errors = [], {}, {}
    for p in paths:
        label = os.path.basename(p)[:-len(".hwpx")]
        labels.append(label)
        try:
            res[label] = analyze(p)
        except Exception as e:  # 파일별 실패는 보고서에 남기고 계속 진행
            errors[label] = f"{type(e).__name__}: {e}"
            print(f"[경고] {label}: {errors[label]}", file=sys.stderr)

    ok = [l for l in labels if l in res]
    n = len(labels)

    def col(key):
        return [res[l][key] if l in res else 0 for l in labels]

    out = []
    w = out.append

    # ---- 파일 목록
    w("## 0. 조사 대상과 파싱 결과")
    w("")
    rows = []
    for l in labels:
        if l in res:
            r = res[l]
            rows.append([l, "예" if r["has_header"] else "아니오", r["nsec"],
                         len(r["parts"]), "성공"])
        else:
            rows.append([l, "-", "-", "-", "실패: " + errors[l]])
    out += md_table(["파일", "header.xml", "section 수", "파싱한 XML 수", "상태"], rows)
    w("")
    w(f"- 대상 파일 {n}개, 파싱 성공 {len(ok)}개.")
    w("")

    # ---- 네임스페이스
    w("## 0-1. 네임스페이스 URI와 짧은 태그")
    w("")
    ns_total = Counter()
    for l in ok:
        ns_total.update(res[l]["ns"])
    rows = [[(uri or "(네임스페이스 없음)"),
             ("/".join([s for s in uri.split("/") if s][-2:]) if uri else "(no-ns)"),
             cnt] for uri, cnt in ns_total.most_common()]
    out += md_table(["네임스페이스 URI", "짧은 태그 접두", "요소 수(전체)"], rows)
    w("")

    # ---- (a)
    w("## 1. (a) 요소-부모 쌍 (요소 ← 부모)")
    w("")
    w("- 요소와 부모 모두 네임스페이스 URI 마지막 두 세그먼트 + `:` + 로컬 이름으로 표기 (접두사 무관).")
    w("- 루트 요소의 부모는 `(root)`로 표기. header.xml 과 모든 section*.xml 포함.")
    w("")
    all_pairs = Counter()
    for l in ok:
        all_pairs.update(res[l]["pairs"])
    rows = []
    for (el, par), tot in sorted(all_pairs.items(), key=lambda kv: (-kv[1], kv[0])):
        counts = [res[l]["pairs"].get((el, par), 0) if l in res else 0 for l in labels]
        files_n = sum(1 for c in counts if c)
        rows.append([f"{el} ← {par}", tot, files_n, per_file_str(labels, counts)])
    out += md_table(["쌍 (요소 ← 부모)", "합계", "등장 파일 수", "파일별 개수"], rows)
    w("")
    w(f"- 서로 다른 쌍 {len(all_pairs)}종, 요소 총수 {sum(all_pairs.values())}.")
    w("")

    # ---- (b)
    w("## 2. (b) hp:t 내부 요소")
    w("")
    w(f"- hp:t 총수(전체 파일): {sum(col('t_total'))}")
    w("")
    w("### 2-1. hp:t 직계 자식")
    w("")
    tc = Counter()
    for l in ok:
        tc.update(res[l]["t_child"])
    rows = []
    for k, tot in tc.most_common():
        counts = [res[l]["t_child"].get(k, 0) if l in res else 0 for l in labels]
        rows.append([k, tot, sum(1 for c in counts if c), per_file_str(labels, counts)])
    if rows:
        out += md_table(["자식 요소", "합계", "등장 파일 수", "파일별 개수"], rows)
    else:
        w("(hp:t 직계 자식 요소 없음: 모든 hp:t 가 텍스트만 가짐)")
    w("")
    w("### 2-2. hp:t 하위 전체 요소 (자손 기준)")
    w("")
    td = Counter()
    for l in ok:
        td.update(res[l]["t_desc"])
    rows = []
    for k, tot in td.most_common():
        counts = [res[l]["t_desc"].get(k, 0) if l in res else 0 for l in labels]
        rows.append([k, tot, sum(1 for c in counts if c), per_file_str(labels, counts)])
    if rows:
        out += md_table(["하위 요소", "합계", "등장 파일 수", "파일별 개수"], rows)
    else:
        w("(hp:t 하위 요소 없음)")
    w("")

    # ---- (c)
    w("## 3. (c) hh:paraPr (header.xml)")
    w("")
    w(f"- hh:paraPr 총수: {sum(col('para_total'))}")
    w(f"- 파일별 hh:paraPr 수: " + per_file_str(labels, col("para_total")))
    w("")
    w(f"### 3-1. paraPr 자식 경로 (깊이 {PATH_DEPTH} 이하, 서로 다른 경로)")
    w("")
    pp = Counter()
    for l in ok:
        pp.update(res[l]["para_paths"])
    rows = []
    for path_s, tot in sorted(pp.items(), key=lambda kv: (kv[0].count(" > "), kv[0])):
        counts = [res[l]["para_paths"].get(path_s, 0) if l in res else 0 for l in labels]
        rows.append([path_s, tot, sum(1 for c in counts if c)])
    out += md_table(["경로 (hh:paraPr 기준)", "출현 수", "등장 파일 수"], rows)
    w("")
    w("### 3-2. hh:align@horizontal 값 (hp:switch/hp:case/hp:default 포함)")
    w("")
    ah = Counter()
    for l in ok:
        ah.update(res[l]["align_h"])
    w(f"- hh:align 총수: {sum(col('align_total'))}")
    w("")
    out += md_table(["horizontal 값", "출현 수"], [[k, v] for k, v in ah.most_common()] or [["(없음)", 0]])
    w("")
    w("### 3-3. hh:lineSpacing@type 값 (hp:switch/hp:case/hp:default 포함)")
    w("")
    lt = Counter()
    for l in ok:
        lt.update(res[l]["ls_type"])
    w(f"- hh:lineSpacing 총수: {sum(col('ls_total'))}")
    w("")
    out += md_table(["type 값", "출현 수"], [[k, v] for k, v in lt.most_common()] or [["(없음)", 0]])
    w("")
    w("### 3-4. hh:lineSpacing@value 값 (hp:switch/hp:case/hp:default 포함)")
    w("")
    lv = Counter()
    for l in ok:
        lv.update(res[l]["ls_value"])
    out += md_table(["value 값", "출현 수"], [[k, v] for k, v in lv.most_common()] or [["(없음)", 0]])
    w("")

    # ---- (d)
    w("## 4. (d) 직계 hp:run 이 2개 이상인 hp:p")
    w("")
    rows = []
    for l in labels:
        if l in res:
            r = res[l]
            rows.append([l, r["p_total"], r["p_multi"], r["p_maxrun"]])
        else:
            rows.append([l, "-", "-", "-"])
    rows.append(["**합계**", sum(col("p_total")), sum(col("p_multi")),
                 max(col("p_maxrun")) if labels else 0])
    out += md_table(["파일", "hp:p 수", "run 2개 이상 문단 수", "문단당 최대 run 수"], rows)
    w("")
    w("- 최대값 열의 합계 행은 전체 파일 중 최댓값.")
    w("")

    # ---- (e)
    w("## 5. (e) 빈 hp:t")
    w("")
    w("- 빈 hp:t: 텍스트가 없고(`text` 가 None 또는 빈 문자열) 자식 요소도 없는 hp:t.")
    w("- 참고로 자식 없이 공백 문자만 있는 hp:t 를 따로 셌다(빈 것으로 세지 않음).")
    w("")
    rows = []
    for l in labels:
        if l in res:
            r = res[l]
            rows.append([l, r["t_total"], r["t_empty"], r["t_ws"]])
        else:
            rows.append([l, "-", "-", "-"])
    rows.append(["**합계**", sum(col("t_total")), sum(col("t_empty")), sum(col("t_ws"))])
    out += md_table(["파일", "hp:t 수", "빈 hp:t 수", "공백만 있는 hp:t 수(참고)"], rows)
    w("")

    # ---- 실패 파일
    if errors:
        w("## 6. 파싱 실패 파일")
        w("")
        for l, msg in errors.items():
            w(f"- {l}: {msg}")
        w("")

    sys.stdout.write("\n".join(out) + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
```
