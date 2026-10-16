# Haiku independent validation: preview artifact publication

Scope: read-only mechanical verification of `IoRoots::write_preview_artifact`
(`crates/hae-project/src/io.rs`), its seven tests
(`crates/hae-project/tests/preview_artifacts.rs`), and the claims in
`docs/decisions/resumption-review/preview-artifacts.md`. No source, git, format,
or workspace changes were made. The renderer crate and the full workspace were
not built or tested. Only this report file was written. No provider calls were
made, so no rate-limit condition was encountered.

## Results

| Check | Command | Result |
|---|---|---|
| Crate tests | `cargo test -p hae-project --locked --offline` | exit 0. 6 unit passed, 0 failed. 7 `preview_artifacts` integration passed, 0 failed. 0 doctests. |
| Clippy | `cargo clippy -p hae-project --all-targets --locked --offline -- -D warnings` | exit 0, 0 warnings or errors. Finished in 0.11s, so this was likely a cache hit. Cargo fingerprints sources, so the result still applies to the current tree. |
| Python oracle | `cd tests/oracle && python3 -m unittest -v test_oracle` | `Ran 88 tests`, `OK`, exit 0, 0 failures, 0 errors. No rendering. |
| Formatting (read-only) | `rustfmt --edition 2021 --check` on `io.rs` and `preview_artifacts.rs` | exit 0 |

Logs: `/tmp/haiku-hae-project-test.log`, `/tmp/haiku-hae-project-clippy.log`,
`/tmp/haiku-oracle-unittest.log`, `/tmp/haiku-rustfmt.log`.

The oracle file has 49 `def test_` functions. The 88 reported by the runner is
the real count; I did not reconcile the difference.

Hashes of the owned files at validation time:

- `crates/hae-project/src/io.rs`: `cbf9c716dce43d0ddd09e150b6e949c130169a03b16a17b894b7b5586c627116`
- `crates/hae-project/tests/preview_artifacts.rs`: `383e6c18a17e27f381eb8c055a2ddda839470884014321ceb44dd4d6c6aad6dc`
- `docs/decisions/resumption-review/preview-artifacts.md`: `5070be2ab643b56e1adef9a2b481a2eddfe45fe126f7ab7b6ddfddbcff84c84a`

## Claims checked against code and tests

| Claim | Code evidence | Test evidence | Status |
|---|---|---|---|
| Only `png`/`svg` accepted, else `INVALID_REQUEST` at render stage, no file | `io.rs:780-786` | `rejects_non_allowlisted_extensions...` (9 bad extensions) | Proven |
| Size cap `min(max_project_bytes, max_session_memory/2)`, floor division, exact boundary accepted | `io.rs:787-797` | `enforces_both_size_caps...` (4 limit tuples incl. odd and zero) | Proven |
| Root and parent traversed with `O_NOFOLLOW` per component; root identity checked | `io.rs:142-172`, `223-251` | `refuses_symlink_swapped_output_root...`, `refuses_replaced_output_root_directory_identity` | Proven for root changed **before** the call |
| Parent identity checked against configured root, and again before publication | `io.rs:805-806`, `843-846` | Same two tests | Partly: the swap-between-steps race is not tested |
| Temp created `O_EXCL|O_NOFOLLOW`, mode 0600, then `fchmod` 0600 | `io.rs:484-489`, `826-827` | Mode 0600 asserted in success and fsync-fault tests | Proven under default umask only. The `fchmod` is not shown to matter. |
| Readback is single-link regular file, same inode as held temp fd, bytes equal, SHA-256 equal | `io.rs:828-835`, `read_regular_nofollow` `268-334` | Indirect only (success path). No corrupted or swapped readback test. | Code-verified only |
| Temp fsync before publish | `io.rs:836-839` | `TempFsync` fault test checks error and cleanup, not the fsync call | Fault path proven. Durability not observable here. |
| `RENAME_NOREPLACE`, `EXIST` mapped to `TARGET_EXISTS` | `io.rs:859-876` | No collision test (random name not predictable). Pre-check `statat` covers existing entries. | Code-verified only |
| Prepublication faults leave no temp, keep inputs and prior artifacts | `io.rs:718-721` style cleanup, `879-882` | `prepublication_faults_remove_temps...` (TempWrite, TempFsync, Rename) | Proven |
| Parent fsync failure after publish: `SAVE_DURABILITY_UNCERTAIN`, detail has path, sha256, revision, file kept | `io.rs:889-905` | `parent_fsync_failure_reports_published_receipt...` | Proven |
| SHA-256 of `abc` is the literal given | `io.rs:822` | `ABC_SHA256` checked in success and fsync tests | Proven for one known vector |
| Storage does not parse or transform bytes | Code only writes and reads raw bytes | Opaque `b"abc"` payload checked byte-for-byte | Proven for the tested payloads |

## Not proven

1. A concurrent root or parent swap between the traversal and the publish step.
   Tests replace the root before the call only.
2. The `RENAME_NOREPLACE` collision path (`EXIST` to `TARGET_EXISTS`). No test
   forces a name collision.
3. Readback mismatch detection (identity or byte mismatch branch). No test
   corrupts or replaces the temp file between write and readback.
4. The restrictive-umask claim. Tests run under the default umask, so the
   `fchmod` necessity is not shown.
5. Durability on power loss or crash. Only injected error returns are tested.
   The actual fsync calls are not observed.
6. Behavior on filesystems that reject `RENAME_NOREPLACE`. The code maps that
   to `IO_ERROR`, but no test covers it.
7. The RED evidence in the resumption note was not reproduced. That would need
   reverting the implementation, which this read-only check did not do.
8. Renderer crate, executor and MCP integration, and the full workspace were
   not tested, by instruction.

## Doc accuracy

The resumption note's GREEN counts (6 unit + 7 integration, clippy clean,
rustfmt clean) reproduce exactly. Its description of the publish order matches
the code. Its validation-limits section already lists the untested races,
collision, corruption, and crash cases above, so it does not overclaim them.
No factual error found in the note.

Open item for the parent (not a defect in the tested code): the crate-level
comment in `src/lib.rs` about validation callbacks predates this API, as the
note says. It was not in scope and was not checked.
