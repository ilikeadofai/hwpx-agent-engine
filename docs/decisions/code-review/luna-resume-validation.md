# Luna resume validation

Scope: independent integration checks of the GPT6.1Sol-authored MCP final wire 4 MiB limit and `isError` consistency, runtime preview availability descriptors, and binary environment renderer wiring. This is a focused scoped validation; the parent is running the full workspace suite. The prior Haiku attempt failed on Claude API rate limiting and left no result/findings.

Environment: repository `hwpx-agent-engine`; `HAE_RHWP="$PWD/spikes/vendor/rhwp-src/target/release/rhwp"` and `HAE_FONT_DIR=/usr/share/fonts/noto-cjk` were set for cargo commands. The rhwp binary exists at that path (Rust 1.93.1 build, reported revision `e91cd25...`); `/usr/bin/bwrap` exists. The renderer-enabled tests executed and passed; no skip was reported.

Commands and results:

- `cargo fmt --all -- --check` — passed.
- `cargo clippy --workspace --all-targets -- -D warnings` — passed.
- `cargo test -p hae-mcp --test response_limit` — 2 passed, 0 failed. Both tests verify final JSON-RPC wire size, including nested escaping, and `isError: true` on the bounded error result.
- `cargo test -p hae-exec --test preview command_discovery_reports_preview_configuration_instead_of_old_stage_reason -- --exact` — 1 passed, 0 failed (6 filtered out). Unconfigured preview reports `available: false` / `renderer_not_configured`; configured preview reports available in describe/list.
- `cargo test -p hae-cli -p hae-jsonl -p hae-mcp --test renderer_env` — 6 passed, 0 failed (2 per package). Both configured and unconfigured preview paths passed in each transport; configured tests rendered page 1 using rhwp+bwrap.

Protocol/process observations: the MCP renderer harness parses each stdout line as one JSON value; MCP tool calls return parsed JSON envelopes and boolean `isError`. It sends `notifications/initialized` without reading a notification reply, then successfully reads the next matching RPC response. CLI renderer tests parse stdout as JSON envelopes/arrays. All invoked tests completed without a hanging subprocess; MCP child processes exited successfully. No failure or skip observed.

Limit: these checks do not establish compatibility with Windows Hancom Office. No full workspace test run was repeated.
