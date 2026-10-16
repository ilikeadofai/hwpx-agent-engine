# 단계 1(코어) 완료 검증 결과 (구현 계획 1-20)

작성: 2026-10-16. 근거: 설계서 §13·§14(단계 1 완료 기준), 구현 계획 §1·§5(작업 1-1~1-20).

## 1. 단계 1 완료 기준 대조표 (설계서 §13·§14)

| 완료 기준 항목 | 담당 crate | 검증 시험 | 결과 |
|---|---|---|---|
| **무편집 저장·내보내기** (`payloadSetSha256` 일치 시 원본 바이트 그대로 통과) | `hae-model`, `hae-project`, `hae-exec`, `hae-cli` | `hae-zip::tests::keep_all_roundtrip_is_byte_identical`, `hae-zip::corpus::corpus_opens_and_roundtrips`, `hae-exec::tests::full_scenario_batch_dry_run_undo_redo_frozen_image_and_project_roundtrip`, `hae-cli::cli::script_runs_representative_stage1_scenario_end_to_end` | **통과** |
| **부분 편집 4종** (`text.replace`, `paragraph.format`, `cell.setText`, `image.replace`) 및 `LinesegPolicy`·`PrvText` | `hae-edit`, `hae-verify` | `hae-edit::text_format` (6건), `hae-edit::image` (12건), `hae-edit::editing_fixtures` (합성 경계 17개 `.hwpx` + 이미지 8개 + 연산 58건), `hae-verify::gate` (15건) | **통과** |
| **해석 불가 요소(opaque) 보존 및 편집 잠금** | `hae-model`, `hae-edit`, `hae-verify` | `hae-model::tests::duplicate_hwpx_ids_are_all_locked_by_opaque_reference`, `hae-model::tests::hancom_uri_prefix_is_not_a_supported_namespace`, `editing_fixtures` (`opaque_attrs.hwpx`), `hae-verify::oracle_mutations` (30개 독립 변조 전수 거절) | **통과** |
| **안정 ID 지속성** (`.hwpwork` 재열기 시 `documentId`·`nodeId`·`revision` 유지, 내보낸 `.hwpx` 재열기 시 새 문서) | `hae-model`, `hae-project`, `hae-exec`, `hae-cli` | `hae-model::tests::map_rows_roundtrip_and_tamper`, `hae-project::tests::save_reopens_stable_ids_and_holds_lock_until_binding_dropped`, `hae-exec::tests::full_scenario_...`, `hae-cli::cli::script_runs_representative_stage1_scenario_end_to_end` | **통과** |
| **프로젝트 잠금 및 안전한 루트 I/O** (`O_NOFOLLOW`, 심볼릭/하드 링크 거절, 비차단 `flock`, `.lock` 교체 검출, `EXTERNAL_MODIFICATION`, `SAVE_DURABILITY_UNCERTAIN`) | `hae-project` | `hae-project::tests` (6건: 잠금 경합, `.lock` 삭제·교체 검출, 외부 수정 거절, 링크·원본 덮어쓰기 거절, 쓰기/fsync/rename/부모 fsync 오류 주입, manifest 변조 검출) | **통과** |
| **`dryRun` 격리 실행** (상태·`revision`·이력·`requestSequence` 불변) | `hae-exec`, `hae-cli` | `hae-exec::tests::full_scenario_...`, `hae-cli::cli::script_runs_representative_stage1_scenario_end_to_end` | **통과** |
| **`command.batch` 원자성 및 실패 복원** (실패 단계 `failedStep` 보고, 본문·ID·미디어·이력·`revision` 불변, 원상복구 batch는 `noop`) | `hae-edit`, `hae-exec` | `hae-edit::text_format::invalid_second_batch_step_preserves_original_document`, `hae-exec::tests::full_scenario_...` | **통과** |
| **Undo/Redo 및 100단위 이력·스냅샷·자원 예산** (항목 공유 스냅샷, 이미지 바이트 고정으로 원본 이미지 손상 후에도 Redo 동일, 101번째 commit에서 1단위 해제 및 `historyEvicted`, `history.clear`·`snapshot.release`, 일회성 CLI 호출 간 `SESSION_REQUIRED`) | `hae-exec`, `hae-cli` | `hae-exec::tests::history_100_unit_eviction_clear_and_snapshot_release_and_memory_budget`, `hae-exec::tests::full_scenario_...`, `hae-cli::cli::one_shot_history_commands_are_rejected_with_session_required` | **통과** |
| **실행기 수준의 요청 순서 규칙** (`writerId`, `requestSequence`, 동일 요청 replay, `REQUEST_ID_CONFLICT`, `REPLAY_TOO_OLD`, `REQUEST_OUT_OF_ORDER`, `UNKNOWN_WRITER`, 오류 시 번호 소비) | `hae-exec` | `hae-exec::tests::envelope_writer_sequence_replay_conflict_and_ordering_rules` | **통과** |
| **단계 1 `document.preview` 계약** | `hae-exec` | `hae-exec::tests::full_scenario_...` (`RENDERER_UNAVAILABLE` 반환, 상태 불변) | **통과** |

## 2. 실행한 전체 검증 명령과 수치

1. **`bash scripts/check.sh`** (종료 코드 `0`):
   - `cargo fmt --all -- --check`: 통과
   - `cargo clippy --workspace --all-targets -- -D warnings`: 경고·오류 0건
   - `cargo test --workspace`: **총 100개 Rust 시험 통과, 실패 0건**
     - `hae-diag`: 7건
     - `hae-zip`: 14건 + 코퍼스 왕복 1건 (`corpus.rs`, 공개 코퍼스 10종 무편집 재기록 바이트 동일 + 1만 건 결정적 변이 무패닉)
     - `hae-xml`: 12건 (NCName, 중복 확장 속성, `xml`/`xmlns` 예약 규칙, BOM·선언·범위 밖 바이트 보존)
     - `hae-package`: 5건 + 악성·경계 통합 1건 (`adversarial.rs`, 80개 합성 사례 전수 대조)
     - `hae-model`: 10건 (구조 경로, 자소↔코드포인트↔바이트 대응, 중복 HWPX ID 잠금, 미지 namespace 분리, 커서 조회)
     - `hae-edit`: `text_format` 6건 + `image` 12건 + `editing_fixtures` 1건 (17개 패키지·8개 이미지·58개 연산 = 66개 검증)
     - `hae-verify`: `gate` 15건 + `oracle_mutations` 1건 (독립 변조 30건 전수 검출)
     - `hae-project`: 6건 (잠금, 무결성, 링크 비추종, 오류 주입 4단계)
     - `hae-exec`: 4건 (레지스트리 21개 명령·스키마, 순서·재전송·충돌, 대표 시나리오 전체, 100단위 이력·스냅샷·메모리 예산)
     - `hae-cli`: 5건 (`SESSION_REQUIRED`, 단발 조회, stdin NDJSON 스크립트, 미등록 명령 사전 거절, 단일 스크립트 대표 시나리오 전체)
2. **`python3 -m unittest discover -s tests/oracle`** (종료 코드 `0`):
   - 독립 Python 표준 라이브러리 oracle 및 변조 생성기 단위 시험 **88건 통과, 실패 0건**.
3. **`python3 -I tests/fixtures/editing/gen_editing.py --check-determinism`** (종료 코드 `0`):
   - 편집 경계 합성 자료 17종 + 입력 이미지 8종 결정성 및 자소 oracle 대조 통과.

## 3. 모델별 작업·검토 기록 요약

| 작업 | 수행 모델 | 산출물 경로 |
|---|---|---|
| 1-1~1-6 기초 계층 구현 | Claude Opus 5.5 / DeepSeek (`077447a`) | `crates/hae-diag`, `hae-zip`, `hae-xml`, `hae-package`, `tests/fixtures/` |
| 1-3·1-4·1-6 독립 재검토 및 결함 수정(F1~F4, C2) | OpenCode Muse Spark 1.3 (`opencode-go/muse-spark-1.3-contributor`) | `crates/hae-xml`, `crates/hae-package`, `docs/decisions/resumption-review/muse-core.md`, `muse-core-fixes.md` |
| 1-2b 편집 경계 합성 자료(17종 + 이미지 8종) | OpenCode DeepSeek V4.1 Flash (`opencode-go/deepseek-v4.1-flash`) | `tests/fixtures/editing/` |
| 1-7~1-12 문서 모델·조회·편집·이미지 교체 | Claude Opus 5.5(초안 `447d7c4`), Codex GPT-6.1 Sol, OpenCode Gemini 4 Argon(`opencode-go/gemini-4-argon-contributor`) | `crates/hae-model`, `crates/hae-edit` |
| 1-13 독립 저장 관문(`hae-verify`) | Codex GPT-6.1 Sol, OpenCode Muse Spark 1.3 (`src/image.rs`), OpenCode Gemini 4 Argon (`tests/oracle_mutations.rs`) | `crates/hae-verify` |
| 1-14 독립 Python oracle 및 30종 변조 시험 | Codex GPT-6 Astra (88건 단위 시험) | `tests/oracle/` |
| 1-15~1-17 프로젝트 I/O·잠금(`hae-project`) 및 실행기·레지스트리(`hae-exec`) | Codex GPT-6.1 Sol (`format.rs`), OpenCode Gemini 4 Argon (`io.rs`, `hae-exec`) | `crates/hae-project`, `crates/hae-exec` |
| 1-18 CLI 스크립트·단발 모드(`hae-cli`) | OpenCode DeepSeek V4.1 Flash (`opencode-go/deepseek-v4.1-flash`) | `crates/hae-cli` |
| 1-19 클린룸·의존성 기록 | OpenCode DeepSeek V4.1 Flash (`clean-room.md`), OpenCode Gemini 4 Argon (`dependencies.md`) | `docs/decisions/clean-room.md`, `docs/decisions/dependencies.md` |

## 4. 한계와 후속 단계(단계 2~4 및 S4)

- **Windows 한컴 재열기(S4) 및 `nativeComparison`**: 아직 **`not_checked`** 상태다. Linux 코어 편집·저장·보존·관문 시험 통과만으로 한컴 조판 호환성을 선언하지 않는다(설계서 §1·§13).
- **단계 2(인터페이스)**: JSON-lines 및 stdio MCP 어댑터, 전송 계층 경쟁·재전송·인터페이스 동등성 시험은 단계 2 범위다.
- **단계 3(프리뷰)**: `rhwp` Rust 1.93.1 재빌드, `bwrap` 격리 제품화, SVG 안전성 검사 및 PNG 래스터화는 단계 3 범위다(현재 `document.preview`는 계약대로 `RENDERER_UNAVAILABLE` 반환).
- **단계 4(평가)**: 사용자가 Windows 한컴에서 작성한 관찰 시험 문서로 대표 시나리오 전체(프리뷰 포함)를 실행하고 S4 재저장 파일과 대조한다.
