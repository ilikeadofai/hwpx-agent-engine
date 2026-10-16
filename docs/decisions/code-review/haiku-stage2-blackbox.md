# 2단계 stdio 바이너리 블랙박스 시험 보고 (hae-jsonl · hae-mcp)

작성: 2026-10-16, 시험 담당 에이전트 (claude-haiku-5-5)

## 1. 대상과 방법

- 대상: `hwpx-agent-engine` 작업 트리(미커밋). 빌드 명령 `cargo build -p hae-jsonl -p hae-mcp -p hae-cli` (성공). 바이너리는 `target/debug/hae-jsonl`, `target/debug/hae-mcp`를 썼다.
- 기준: `docs/decisions/diagnostic-codes.md`, 설계서 `docs/superpowers/specs/2026-10-16-hwpx-engine-0.1-design.md` §6(명령 계약·요청 순서·세션)과 §11(입출력 한도·stdout/stderr 규칙).
- 입력: `tests/fixtures/corpus/bodojaryo-reference.hwpx`의 사본을 임시 루트 `/tmp/hae-bb/root/in/bodojaryo.hwpx`에 두었다(sha256 일치 확인). 두 번째 문서 대조용으로 `gyehoek-reference.hwpx` 사본 `other.hwpx`를 같은 루트에 두었다. 입출력 루트는 `/tmp/hae-bb/root/{in,out}`.
- 드라이버: Python 표준 라이브러리만 사용(`/tmp/hae-bb/drivers/`). 서버를 subprocess로 띄우고 stdin에 바이트 단위로 쓰며, stdout은 select 시간 제한(15초)으로 읽었다. 저장소 안에는 어떤 파일도 만들거나 고치지 않았다.
- 실행 결과 파일: `/tmp/hae-bb/jsonl_results.json`, `/tmp/hae-bb/mcp_results.json`.

## 2. 결과 요약

| 대상 | 케이스 | PASS | FAIL |
|---|---|---|---|
| hae-jsonl | 32 | 32 | 0 |
| hae-mcp | 31 | 31 | 0 |

최종 실행은 재빌드한 바이너리로 했다(cargo: 최신 상태, 빌드 변경 없음). 시험 도중 저장소 작업 트리의 다른 파일(`hae-exec` 등)이 바뀌었다. 이 보고는 그 상태를 기준으로 한다.

## 3. hae-jsonl 케이스

| 번호 | 검증 항목 | 기대 | 실제 (요약) | 결과 |
|---|---|---|---|---|
| J01 | hello가 첫 줄 | type=hello, schemaVersion 1, sessionId·writerId 존재 | 확인 | PASS |
| J02 | document.open | ok, documentId, revision 0 | ok, revision 0 | PASS |
| J03 | element.query 문단 찾기 | nodeId 반환 | 찾음 | PASS |
| J04 | document.inspect | ok | ok | PASS |
| J05 | text.replace seq1 (rev 0) | applied, afterVersion 1 | applied, 1 | PASS |
| J06a | 1MiB+1 요청 줄 길이 | 1048577바이트 | 1048577 | PASS |
| J06 | 1MiB+1 요청 | RESOURCE_LIMIT, reason request_too_large | 그대로 | PASS |
| J07 | 뒤이은 seq2 (같은 번호로 과대 요청을 보냈음) | applied, afterVersion 2 (번호 미소비) | applied, 2 | PASS |
| J08 | 정확히 1MiB 요청 (경계) | 크기로 거절되지 않음 | 미지 필드 `pad`로 INVALID_REQUEST만 발생 | PASS |
| J09 | malformed JSON | INVALID_REQUEST, reason malformed_json, 서버 유지 | 그대로 | PASS |
| J10 | malformed 뒤 command.list | ok, 명령 목록 | ok | PASS |
| J11 | 미지 봉투 필드 `bogus` | INVALID_REQUEST | INVALID_REQUEST | PASS |
| J12 | 오래된 expectedVersion 0 (현재 2) | VERSION_CONFLICT | VERSION_CONFLICT, currentVersion 2 | PASS |
| J13 | seq4, expectedVersion 2 | applied, afterVersion 3 (seq3은 충돌로 소비) | applied, 3 | PASS |
| J14 | seq4 재전송, 같은 내용 | replayed true, 상태 불변 | replayed true, afterVersion 3 | PASS |
| J15 | seq4, 다른 내용 | REQUEST_ID_CONFLICT | REQUEST_ID_CONFLICT | PASS |
| J16 | seq6 (기대 5) | REQUEST_OUT_OF_ORDER | "expected requestSequence 5, got 6" | PASS |
| J17 | seq1 (이미 지남) | REPLAY_TOO_OLD | "requestSequence 1 is older than 4" | PASS |
| J18 | seq5 (J16·J17 뒤) | applied (거절은 번호 미소비) | applied, afterVersion 4 | PASS |
| J19 | 발급하지 않은 writerId | UNKNOWN_WRITER | UNKNOWN_WRITER | PASS |
| J20 | 다른 sessionId | SESSION_MISMATCH | SESSION_MISMATCH | PASS |
| J21 | 없는 documentId `doc-nonexistent` | 거절, 문서 A 불변 | TARGET_NOT_FOUND | PASS |
| J21b | 다른 없는 documentId (seq7) | 거절 | TARGET_NOT_FOUND | PASS |
| J22a | 같은 루트의 문서 B를 별도 프로세스에서 열기 | A와 다른 id | 서로 다른 id 확인 | PASS |
| J22 | 문서 B의 id로 편집 (seq8) | 거절, A 불변 | TARGET_NOT_FOUND | PASS |
| J23 | 문서 A 상태 확인 | rev 4, 텍스트 "공고자료" | rev 4, "공고자료" | PASS |
| J24 | seq9 정상 편집 | applied, afterVersion 5 (6·7·8 소비, 어긋남 없음) | applied, 5 | PASS |
| J25 | stdout 모든 줄이 JSON 객체 | 25줄 모두 파싱 | 25줄 모두 파싱 | PASS |
| J26 | 로그는 stderr에만 | "hae-jsonl:" 접두 로그 | 확인, stdout에 로그 없음 | PASS |
| J27 | EOF 시 정상 종료 | 종료 코드 0 | 0 | PASS |
| J28 | EOF에 미완성 줄 | 오류 응답 1개, 크래시 없음, 종료 0 | INVALID_REQUEST 1개, 종료 0 | PASS |
| J29 | CRLF 줄끝 | 정상 처리 | ok | PASS |

## 4. hae-mcp 케이스

| 번호 | 검증 항목 | 기대 | 실제 (요약) | 결과 |
|---|---|---|---|---|
| M01 | initialize | protocolVersion, serverInfo hae-mcp, tools 기능 | 확인 | PASS |
| M02 | notifications/initialized | 응답 없음 (다음 줄이 ping 응답) | 응답 없음 | PASS |
| M03 | 알 수 없는 알림 | 응답 없음 | 응답 없음 | PASS |
| M04 | ping | result {} | {} | PASS |
| M05a | tools/list | 21개, text_replace·document_open 포함 | 21개 | PASS |
| M05b | 모든 inputSchema에 writerId·requestSequence 없음 | 없음 | 없음 | PASS |
| M05c | text_replace 필수 항목 | documentId, expectedVersion 포함 | 포함 | PASS |
| M06 | document_open | isError false, documentId | 확인 | PASS |
| M07 | documentId 생략한 element_query | 열린 문서 사용 | 확인 | PASS |
| M08 | text_replace (expectedVersion 0) | isError false, applied, rev 1 | 그대로 | PASS |
| M09 | 같은 text_replace 두 번째 | isError true, VERSION_CONFLICT | isError true, VERSION_CONFLICT | PASS |
| M10 | 알 수 없는 도구 | JSON-RPC 오류 | code -32602, "unknown tool" | PASS |
| M11 | 필수 인자 `text` 누락 | isError true, INVALID_REQUEST | 그대로 | PASS |
| M12 | M11 뒤 정상 편집 | rev 2 (번호 어긋남 없음) | afterVersion 2 | PASS |
| M13 | 미지 인자 `bogus` | isError true, INVALID_REQUEST | 그대로 | PASS |
| M14 | M13 뒤 정상 편집 | rev 3 | afterVersion 3 | PASS |
| M15 | 없는 documentId | isError true, 거절 | TARGET_NOT_FOUND | PASS |
| M15b | M15 뒤 문서 A에 "변경" 없음 | 0건 | 0건 | PASS |
| M16a | 별도 프로세스 문서 B의 id | A와 다른 id | 다름 | PASS |
| M16 | 문서 B의 id로 편집 | isError true, A 불변 | TARGET_NOT_FOUND | PASS |
| M16b | 문서 A 상태 | rev 3, "공지자료" | rev 3, "공지자료" | PASS |
| M16c | document_inspect | 문서 A, rev 3 | 확인 | PASS |
| X1 | arguments가 객체가 아님 | -32602 | -32602 | PASS |
| X2 | malformed JSON | -32700, id null | -32700, id null | PASS |
| X3 | JSON-RPC 배치 | -32600 | -32600 | PASS |
| M17 | 1MiB+1 메시지 | -32600, id null | -32600 "exceeds the 1048576 byte limit" | PASS |
| M17b | 과대 메시지 뒤 ping | 정상 | {} | PASS |
| M17c | 과대 메시지 뒤 정상 편집 | rev 4 (번호 어긋남 없음) | afterVersion 4 | PASS |
| M18 | stdout 모든 줄이 JSON-RPC 2.0 | 전부 파싱 | 25줄 모두 파싱 | PASS |
| M19 | stderr는 "hae-mcp:" 로그만 | 전부 접두 일치 | 일치 | PASS |
| M20 | EOF 시 정상 종료 | 종료 코드 0 | 0 | PASS |

## 5. 시험 설계 수정 이력 (실패로 잡혔던 항목)

처음 실행에서 아래 항목이 실패했다. 모두 시험 쪽 오류였고 제품 결함은 아니었다. 고친 뒤 재실행해 통과했다.

- J18: 같은 텍스트("공지")를 같은 범위에 다시 써서 `outcome: noop`이 나왔다. 설계서 §6의 noop 규칙과 일치하는 정상 동작이었다. 다른 텍스트로 고쳤다.
- J22 (무효 시험이었음): `other.hwpx`를 입력 루트 밖에 두어 document.open이 실패했고, 대상 id가 null이 되어 현재 문서 A에 적용되는 요청이 됐다. 루트 안에 다른 문서(gyehoek-reference.hwpx)를 두고 다시 시험했다.
- J23·J24: J22가 번호 8을 소비한 뒤 J24가 같은 번호 8을 다른 documentId로 보냈다. 그래서 REQUEST_ID_CONFLICT가 나온 것은 설계서 §6과 일치하는 정상 동작이었다. 번호를 9로 바꿨다.
- J28: 부분 줄의 응답을 `recv()`로 읽지 않고 잘못된 버퍼를 확인했다. 남은 stdout을 파싱하도록 고쳤다.
- J08: 줄 길이가 정확히 1MiB가 아니었다(1048567). 1048576으로 맞췄다.
- M17c: 같은 텍스트로 `noop`이 나왔다. 다른 텍스트로 고쳤다.
- M11: 처음에는 "오류 또는 isError"를 모두 허용했다. 정확한 코드(INVALID_REQUEST, isError true)를 요구하도록 강화했다.

## 6. 관찰 (실패 아님)

1. 문서 id 불일치로 거절된 요청도 writer와 번호가 확인되면 번호를 소비한다(J21·J21b·J22 뒤 J24가 번호 9를 요구). 설계서 §6의 "오류(버전 충돌과 자원 한도 포함)면 번호 소비" 규칙과 맞다.
2. MCP 도구 스키마는 `documentId`를 required로 선언하지만, 어댑터는 생략을 허용하고 열린 문서로 대체한다(`hae-mcp/src/lib.rs` run_tool). 서버 동작은 안전하다. 다만 스키마와 실제 허용 범위가 다르다는 점은 설계 메모로 남길 만하다.
3. 실행 중 작업 트리에 다른 변경(hae-exec 등)이 있었다. 보고의 결과는 그 상태의 빌드 기준이다.

## 7. 재현

```
cd /home/mattc/Documents/Codex/hwpx-agent-engine
cargo build -p hae-jsonl -p hae-mcp -p hae-cli
cp tests/fixtures/corpus/bodojaryo-reference.hwpx /tmp/hae-bb/root/in/bodojaryo.hwpx
cp tests/fixtures/corpus/gyehoek-reference.hwpx /tmp/hae-bb/root/in/other.hwpx
python3 -I /tmp/hae-bb/drivers/jsonl_drive.py
python3 -I /tmp/hae-bb/drivers/mcp_drive.py
```

실패 0건
