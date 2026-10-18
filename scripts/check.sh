#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
# An explicit HAE_RHWP (even a missing path) overrides the local default.
# Renderer-free mode: run `HAE_RHWP=/nonexistent/rhwp bash scripts/check.sh`.
# Unsetting HAE_RHWP (env -u) is NOT renderer-free: the vendor default below is still found.
renderer="${HAE_RHWP:-$PWD/spikes/vendor/rhwp-src/target/release/rhwp}"
if [[ -f "$renderer" && -x "$renderer" ]]; then
  export HAE_RHWP="$renderer"
  renderer_available=1
  echo "RENDERER: available ($renderer); 13 real renderer tests (4 executor preview + 9 adapter) will run"
else
  renderer_available=0
  echo "RENDERER: unavailable ($renderer)"
  echo "NOTE: renderer-free mode needs HAE_RHWP set to a nonexistent path; unsetting HAE_RHWP still finds the vendor default if it is built"
  echo "SKIP: 13 real renderer tests (4 executor preview + 9 adapter); green means core/sandbox gates passed, rendering untested"
fi
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
# Integration tests launch this sibling binary through the real sandbox.
cargo build -p hae-render-rhwp --bin hae-raster
python3 -m unittest discover -s scripts/tests
cargo test --workspace

if [[ "$renderer_available" == 1 ]]; then
  cargo test -p hae-exec --test preview -- --ignored --nocapture
  cargo test -p hae-render-rhwp -- --ignored --nocapture
  echo "RENDERER: 13 real renderer tests passed (4 executor preview + 9 adapter)"
else
  echo "RENDERER: unavailable; 13 real renderer tests SKIPPED (4 executor preview + 9 adapter)"
fi
