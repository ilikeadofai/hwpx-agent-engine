#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
# Integration tests launch this sibling binary through the real sandbox.
cargo build -p hae-render-rhwp --bin hae-raster
python3 -m unittest discover -s scripts/tests
cargo test --workspace
