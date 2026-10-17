# scripts/windows (개발자용)

Windows(한컴 한글 설치 PC)에서 사용자가 더블클릭으로 쓰는 인계 보조 도구다. 엔진과 무관하며 읽기 전용 조회와 파일 해시/압축만 한다.
네트워크 접근, 레지스트리 쓰기, 관리자 권한은 쓰지 않는다. 컴퓨터 이름·사용자 이름·제품키·설치 경로는 기록하지 않는다.

| 파일 | 하는 일 |
|---|---|
| `1_환경기록.bat` -> `tools\step1_environment.ps1` | `return\environment_auto.txt` 작성 (Windows, 한컴 제품 이름/버전/게시자, 글꼴 6종 유무, 기본 프린터/용지, 화면 배율) |
| `2_입력확인.bat` -> `tools\step2_verify_inputs.ps1` | 묶음 최상위 `SHA256SUMS.txt` 와 실제 파일 해시 대조. OK/불일치/없음 출력, 종료 코드 0(정상)/1(문제)/2(오류). `return/` 항목은 사용자가 수정하므로 `[변경]`으로만 표시하고 실패로 세지 않음 |
| `3_결과묶기.bat` -> `tools\step3_pack_results.ps1` | `return\RETURN_SHA256SUMS.txt`(return 기준 상대경로, LF, BOM 없음) 작성, results.csv 빈 행/environment.txt 비어 있음 경고, 바탕화면에 `Windows_결과_yyyyMMdd_HHmm.zip` 생성(기존 파일 덮어쓰지 않음) |
| `templates/results.csv`, `templates/environment.txt` | 묶음의 `return\` 초기 파일. results.csv는 기존 16행 + `user_fixtures/01~04` 4행 |

## 묶음 조립 시 배치
```
<묶음>/
  1_환경기록.bat  2_입력확인.bat  3_결과묶기.bat
  tools/step1_environment.ps1  step2_verify_inputs.ps1  step3_pack_results.ps1
  SHA256SUMS.txt  provenance.json  00_시작하기.md  SDK_분석_안내.md  참고_상세안내_개발자용.md
  s4_inputs/  engine_inputs/  linux_reference_png/
  return/ (results.csv, environment.txt <- templates/ 에서 복사; pdf/ resaved/ screenshots/ user_fixtures/)
```
- ps1은 `$PSScriptRoot` 의 상위 폴더를 묶음 루트로 쓴다. bat과 `tools\` 는 같은 폴더에 있어야 한다.
- `SHA256SUMS.txt` 는 새로 넣는 bat/ps1/템플릿까지 포함해 다시 생성해야 한다(2번 단계가 전부 대조한다). `return/results.csv`, `return/environment.txt` 항목은 사용자가 고쳐도 `[변경]`이라 무방하다.

## 인코딩 규칙
- `*.ps1`: UTF-8 **BOM + CRLF** (BOM이 없으면 Windows PowerShell 5.1이 ANSI로 읽어 한글이 깨진다). PS7 전용 문법(`?:`, `??`, `&&`, `-Parallel`) 금지, `Set-StrictMode -Version 2`.
- `*.bat`: **ASCII + CRLF** 본문(한글 echo 금지). 파일 이름만 한글.
- `templates/results.csv`: UTF-8 BOM + CRLF. `RETURN_SHA256SUMS.txt`: BOM 없는 UTF-8 + LF(`sha256sum -c` 호환).
- 편집 후에는 BOM/CRLF가 유지되는지 확인할 것.

## 검증 상태
Linux 개발 환경에 `pwsh` 가 없어 **실제 실행 검증을 하지 못했다.** 인코딩/줄끝, 괄호 균형, PS7 문법 미사용은 정적으로, SHA256SUMS 파싱·대조 논리는 Python으로 같은 규칙을 흉내 내어 확인했을 뿐이다. 첫 Windows 실행 결과를 보고 고칠 것.
