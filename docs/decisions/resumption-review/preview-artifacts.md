# Preview artifact publication

Implemented the output-root artifact storage API required by design §11 for
preview images too large for an inline response. This is the storage portion;
the parent executor/MCP work owns choosing the inline threshold and returning
the artifact receipt for an oversized page.

## Ownership and public contract

Only these source/document files were edited:

- `crates/hae-project/src/io.rs`
- `crates/hae-project/tests/preview_artifacts.rs` (new Cargo integration test)
- `docs/decisions/resumption-review/preview-artifacts.md` (this note)

No git commands, dependency/manifest edits, workspace formatting, renderer edits,
or executor/MCP edits were performed. Formatting was limited to the two owned
Rust files. Tests use generated temporary directories and synthetic byte strings;
the existing project tests use the synthetic `adversarial/valid_base.hwpx` fixture.
No private documents or school project files were read for this implementation.

```rust
pub fn write_preview_artifact(
    &self,
    extension: &str,
    payload: &[u8],
    revision: u64,
) -> Result<SaveReceipt>
```

- Accepts exactly lowercase `png` or `svg`, without a dot. Other extensions return
  `INVALID_REQUEST` at the render stage before creating a file.
- The caller validates image syntax/content before calling. This method stores
  opaque bytes and performs no renderer/image parsing. Existing HWPX save/export
  validation callbacks and package/preservation gates retain their behavior.
- Rejects payloads larger than
  `min(max_project_bytes, max_session_memory / 2)` with `RESOURCE_LIMIT` before
  creating output. Division rounds down. The exact boundary, including zero, is
  accepted. This bounds the supplied payload plus one readback, not total process
  RSS or the caller's other live allocations.
- Generates `preview-<random-id>.png` or `.svg` directly under the configured
  output root using the existing OS-random ID helper. The receipt contains the
  absolute path, SHA-256 of the exact bytes, and the supplied revision.
- Uses nofollow directory-descriptor traversal, checks the opened parent against
  the configured output-root device/inode, and checks the root again before
  publication. Protected input paths/inodes remain protected. Existing entries,
  including symlinks/hard links, are never replaced; a collision returns
  `TARGET_EXISTS`.

## Publication and failure behavior

1. Exclusively create a nofollow temporary file under the checked directory
   descriptor with mode 0600. Explicitly set mode 0600 before readback, including
   under a restrictive process umask.
2. Read back a single-link regular file through the directory descriptor. Compare
   its inode with the held temp descriptor, its bytes with the supplied payload,
   and its SHA-256 with the precomputed payload hash.
3. Fsync the temp descriptor, recheck root/temp identities, and publish with
   atomic `RENAME_NOREPLACE` under the same directory descriptor.
4. Fsync the parent descriptor and return the receipt.

Prepublication errors attempt descriptor-relative temp cleanup and preserve
existing files. Temp write, temp fsync, and rename fault injection return
`IO_ERROR` with no artifact or temp left in the tested conditions. The existing
`FaultStep` mechanism is reused without adding fault variants or test-only API.

After a successful rename, parent-directory fsync failure returns
`SAVE_DURABILITY_UNCERTAIN` at the render stage. The newly published file stays
visible. Diagnostic `detail` contains the receipt fields directly:

```json
{"path":"/absolute/output/preview-<random-id>.svg","sha256":"<sha256>","revision":42}
```

Callers must account for this published file when handling the error; it is not
an unpublished/rolled-back result. No project binding or revision is changed by
artifact publication.

## RED / GREEN evidence

All commands ran from `/home/mattc/Documents/Codex/hwpx-agent-engine` with pinned
Rust 1.93.1 and locked/offline dependency resolution.

RED, before implementation:

```text
cargo test -p hae-project --test preview_artifacts --locked --offline
```

- First run: exit 101, `E0599` for the missing `IoRoots::write_preview_artifact` API.
- Added only the method signature with an `UNSUPPORTED` stub, then repeated the
  command: exit 101, 0 passed / 7 failed. The failures exercised the missing
  publication, extension/size errors, root identity checks, and fault behavior.
- Replaced the stub with the implementation, leaving those tests in place.

GREEN, after implementation and owned-file formatting:

```text
cargo test -p hae-project --test preview_artifacts --locked --offline
# exit 0: 7 integration tests passed

cargo test -p hae-project --locked --offline
# exit 0: 6 existing unit tests + 7 integration tests passed; 0 doctests

cargo clippy -p hae-project --all-targets --locked --offline -- -D warnings
# exit 0, no warnings

rustfmt --edition 2021 --check crates/hae-project/src/io.rs crates/hae-project/tests/preview_artifacts.rs
# exit 0
```

The seven new tests cover both extensions, 16 distinct direct-child receipts,
literal known SHA-256/bytes, revision and mode, invalid extensions with no output,
each size cap and exact/odd/zero boundaries, symlink-swapped and replaced root
directories with no outside writes, and all four injected I/O failures.
Fault tests check protected source bytes and previous artifacts remain intact;
prepublication failures leave no temp files, while parent-fsync failure retains
exact published bytes and exposes receipt metadata.

## Validation limits

- Focused validation covers `hae-project`; the shared workspace's renderer,
  executor/MCP integration, inline response budgeting, and end-to-end oversized
  page behavior were not tested or changed here.
- Root-swap tests replace the root before the call. There is no deterministic
  concurrent swap, random-name collision, readback-corruption, filesystem crash,
  or power-loss test. Rename collision safety uses the kernel's no-replace
  operation; durability fault coverage uses the existing injected failure points.
- As with the existing rooted save/export code, roots must be writable only by
  trusted cooperating processes. Identity checks and descriptor I/O do not make
  all hostile same-directory mutation races atomic. Cleanup also depends on the
  filesystem accepting unlink; cleanup failures cannot be guaranteed away.
- The crate-level general comment about validation callbacks predates this new
  storage API. This method's documentation explicitly assigns preview image
  validation to the caller; HWPX write callbacks remain required. Updating that
  general comment in `src/lib.rs` is outside the assigned file ownership.
