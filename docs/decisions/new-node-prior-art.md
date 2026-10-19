# 새 객체 생성에 참고한 기존 워크벤치

사용자의 요청에 따라 MCP 이전 로컬 스킬과 Python 조립기를 읽었다. 현재 Rust 엔진의 부분 편집·독립 검증 계약을 유지하면서 새 객체의 요소 순서와 네이티브 속성 구성을 참고한다. 옛 스크립트의 존재나 구조 재사용 자체는 이번 산출물의 한컴 열기·조판 검증 결과가 아니다.

| 자료 | 사용 목적 | 2026-10-19 읽은 파일 SHA-256 |
|---|---|---|
| `~/.codex/skills/hwpx-workbench/SKILL.md`, `references/contracts.md` | seed→명세→생성/편집→검증→프리뷰 절차, HWP 수식 원문 유지, native 상태 구분 | 스킬의 현행 로컬 문서 |
| `scripts/hwpx_workbench/vendor/hwpx_utils.py` | `equation_xml`, `picture_xml`, `table_xml` 객체 구조 | `822b7fcb90cfd5a6a7f018cd4d366a7612a7a318498cef6f5080416e4bb0adf1` |
| `scripts/hwpx_workbench/build.py` | 수식 줄 높이 반영, 명시 치수와 추정·재계산 정책 구분, 표 스타일 참조 | `f66b22f56de45147441d2c7e10ada0bd3487b817b320ff5b23365fad9f169b96` |

수식은 HWP 스크립트를 그대로 받아 네이티브 `equation/script`에 넣는다. 새 수식 상자의 `width`·`height`는 호출자의 명시적 HWPUNIT 값이다. 구문 길이에 비례한 옛 너비 추정식이나 0 치수 재계산 정책을 몰래 적용하지 않는다. 그림은 원본 이미지 바이트를 새 미디어로 등록한다. 표는 기존 헤더 정의를 참조하며 새로운 스타일을 임의 생성하지 않는다. 전체 XML을 파싱 후 다시 직렬화하는 조립기 흐름은 부분 편집 경로로 옮기지 않는다.

출처는 로컬 스킬의 `THIRD_PARTY_NOTICES.md`에 기록된 `ilikeadofai/hwpx-document-processing-skill`, commit `94f948cf90c5b37c01dc6e09069992ab09adb412`이다. 실질적으로 참고·변형한 템플릿에 대한 MIT 고지를 아래에 보존한다. 현재 코드와 독립 검증기는 Rust로 각각 구현한다.

```text
MIT License

Copyright (c) 2026 Matt Choi

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```
