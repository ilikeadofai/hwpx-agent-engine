#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
# An explicit HAE_RHWP (even a missing path) overrides the local default.
renderer="${HAE_RHWP:-$PWD/spikes/vendor/rhwp-src/target/release/rhwp}"
if [[ -f "$renderer" && -x "$renderer" ]]; then
  export HAE_RHWP="$renderer"
  renderer_available=1
  echo "RENDERER: available ($renderer); 4 real executor preview tests will run"
else
  renderer_available=0
  echo "RENDERER: unavailable ($renderer)"
  echo "SKIP: 4 real executor preview tests; green means core/sandbox gates passed, rendering untested"
fi
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
# Integration tests launch this sibling binary through the real sandbox.
cargo build -p hae-render-rhwp --bin hae-raster
python3 -m unittest discover -s scripts/tests
cargo test --workspace

if [[ "$renderer_available" == 1 ]]; then
  cargo test -p hae-exec --test preview -- --ignored --nocapture
  echo "RENDERER: 4 real executor preview tests passed"
else
  echo "RENDERER: unavailable; 4 real executor preview tests SKIPPED"
fi
