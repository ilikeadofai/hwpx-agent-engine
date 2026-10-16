# 진단 코드 목록 (구현 계획 1-5)

작성: 2026-10-16, 총괄. 설계서의 코드와 hae-diag `Code` enum은 이 표와 1:1이어야 한다. "추가"는 설계서에 이름이 없지만 구현에 필요해 총괄이 정한 코드다. 설계서를 개정할 때 이 표도 같이 고친다.

| 코드 | 설계서 | 단계 | 뜻 |
|---|---|---|---|
| PACKAGE_REJECTED | §11 | 1 | 열기 단계 패키지 거절. reason에 사유 |
| RESOURCE_LIMIT | §8·§11 | 1 | 한도 초과(크기·항목 수·해제량·깊이·화소·메모리·이력) |
| STATE_INVARIANT_ERROR | §4 | 1 | commit 전 정규 상태와 파생물 불일치 |
| PROJECT_INTEGRITY_ERROR | §5 | 1 | .hwpwork 무결성 검사 실패 |
| PROJECT_LOCKED | §5·§11 | 1 | 프로젝트 잠금 실패 |
| EXTERNAL_MODIFICATION | §11 | 1 | 디스크 파일이 마지막 저장 hash와 다름 |
| SAVE_DURABILITY_UNCERTAIN | §11 | 1 | 공개 뒤 부모 디렉터리 fsync 실패 |
| VERSION_CONFLICT | §6 | 1 | expectedVersion 불일치 |
| HISTORY_EMPTY | §6·§8 | 1 | 되돌리거나 다시 적용할 이력 없음 |
| SNAPSHOT_UNAVAILABLE | §8 | 1 | 해제했거나 없는 스냅샷 |
| SESSION_MISMATCH | §6 | 1 | 다른 sessionId |
| SESSION_REQUIRED | §6 | 1 | 일회성 호출 사이 Undo/Redo |
| UNKNOWN_WRITER | §6 | 1 | 발급하지 않았거나 다른 연결의 writerId |
| REQUEST_ID_CONFLICT | §6 | 1 | 같은 번호·다른 내용 |
| REPLAY_TOO_OLD | §6 | 1 | 이미 지난 번호 |
| REQUEST_OUT_OF_ORDER | §6 | 1 | 건너뛴 번호 |
| TARGET_NOT_FOUND | §6 | 1 | 대상 없음 |
| AMBIGUOUS_TARGET | §6 | 1 | 대상 모호 |
| UNSUPPORTED | §6 | 1 | 지원하지 않는 변경·문서 조건. reason에 사유 |
| INVALID_RANGE | §6 | 1 | 범위가 문단 밖이거나 길이 0 |
| RANGE_CROSSES_CONTROL | §6 | 1 | 범위가 탭·줄바꿈·필드·개체 경계를 지남 |
| UNSUPPORTED_CONTENT_IN_RANGE | §6 | 1 | 범위 안 텍스트 외 자식·해석 불가 속성 |
| FORMAT_COLLAPSED | §6 | 1 | (경고) 여러 글자 모양이 하나로 합쳐짐 |
| RENDERER_UNAVAILABLE | §9 | 3 | 렌더러 없음·격리 미검증. 단계 1·2는 항상 이 코드 |
| PREVIEW_PAGE_LIMIT | §10 | 3 | (경고) 100쪽 초과로 partial |
| INVALID_REQUEST | 추가 | 1 | 요청 형식 오류·정의되지 않은 속성 |
| UNKNOWN_COMMAND | 추가 | 1 | 레지스트리에 없는 명령 |
| PATH_NOT_ALLOWED | 추가 | 1 | 입출력 루트 밖, 링크, 원본·이미지 덮어쓰기 시도 |
| TARGET_EXISTS | 추가 | 1 | 내보내기·첫 저장 대상이 이미 있음 |
| VERIFY_FAILED | 추가 | 1 | 저장 관문(hae-verify) 불통과. 파일을 공개하지 않음 |
| IO_ERROR | 추가 | 1 | 그 밖의 파일 입출력 실패 |

응답 필드(코드가 아님): unalignedMatches, historyEvicted, controlChanged, replay, outcome, savedVersion.

PACKAGE_REJECTED의 reason 값: encrypted, duplicate_name, case_collision, path_escape, zip64_required, bad_mimetype, broken_manifest, dtd_or_entity, non_utf8, malformed_zip, unsupported_compression, multi_disk, crc_mismatch, size_mismatch, header_mismatch, overlapping_entries, malformed_xml, foreign_namespace. 앞의 9개는 설계서 §11, 나머지는 총괄 추가다.
