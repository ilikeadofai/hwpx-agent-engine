#![cfg(unix)]

//! Environment-driven discovery: which variable wins, what a disabled search
//! means, and that the Linux isolation contract is untouched by the Windows
//! opt-in switch.
//!
//! One test function only: the process environment is global, so several
//! parallel tests setting `HAE_*` would race (the same reason
//! `raster_env.rs` is its own test binary).

use hae_render_rhwp::discovery::{DISCOVERY_ENV, RENDERER_ENV_HELP};
use hae_render_rhwp::sandbox::TempDir;
use hae_render_rhwp::unsandboxed::ALLOW_ENV;
use hae_render_rhwp::{AdapterConfig, RenderRequest, RhwpAdapter};
use std::fs;
use std::path::PathBuf;

fn set(key: &str, value: &str) {
    std::env::set_var(key, value);
}

fn unset(key: &str) {
    std::env::remove_var(key);
}

fn preview(cfg: AdapterConfig) -> hae_diag::EngineError {
    let adapter = RhwpAdapter::new(cfg);
    let req = RenderRequest {
        document_id: "discovery-env",
        content_revision: 0,
        hwpx: b"PK",
        pages: None,
        want_png: false,
        png: Default::default(),
    };
    adapter.preview(&req, None).unwrap_err()
}

#[test]
fn environment_values_win_and_a_disabled_search_refuses_cleanly() {
    let fonts = TempDir::new_in(&std::env::temp_dir(), "hae-discovery-fonts-").unwrap();
    let work = TempDir::new_in(&std::env::temp_dir(), "hae-discovery-work-").unwrap();
    unset(DISCOVERY_ENV);
    unset(ALLOW_ENV);

    // 1. An explicit HAE_RHWP is used as given, source `env`, even when it
    //    does not exist: a typo must stay visible instead of silently
    //    selecting a different renderer.
    set("HAE_RHWP", "/nonexistent/rhwp-discovery-test");
    set("HAE_FONT_DIR", &fonts.path().display().to_string());
    set("HAE_BWRAP", "/usr/bin/bwrap");
    let cfg = AdapterConfig::from_env_detailed().expect("explicit value builds a config");
    assert_eq!(
        cfg.rhwp_path,
        PathBuf::from("/nonexistent/rhwp-discovery-test")
    );
    assert_eq!(cfg.rhwp_source(), "env");
    assert_eq!(cfg.font_source(), "env");
    assert_eq!(cfg.bwrap_source(), "env");
    // A missing explicit renderer is `renderer_missing` with the fix, not a
    // silent fallback to a discovered one.
    let e = preview(cfg);
    assert_eq!(e.reason(), Some("renderer_missing"), "{e}");
    assert!(e.to_string().contains("HAE_RHWP"), "{e}");

    // 2. Discovery off and no HAE_RHWP: fail closed with the fix line, and no
    //    candidate is examined (the search never runs).
    unset("HAE_RHWP");
    set(DISCOVERY_ENV, "0");
    let failure = AdapterConfig::from_env_detailed().expect_err("no renderer configured");
    assert!(failure.searched.is_empty(), "{:?}", failure.searched);
    assert!(failure.message.contains("HAE_RHWP"), "{}", failure.message);
    assert!(
        failure.message.contains(DISCOVERY_ENV),
        "{}",
        failure.message
    );
    let e = failure.to_error();
    assert_eq!(e.reason(), Some("renderer_not_configured"), "{e}");
    assert_eq!(
        e.diagnostic().detail.as_ref().unwrap()["tool"],
        "rhwp",
        "{e}"
    );
    assert!(AdapterConfig::from_env().is_none());

    // 3. The help text that every interface prints names the same variables
    //    the discovery rules honour.
    for key in [
        "HAE_RHWP",
        "HAE_BWRAP",
        "HAE_FONT_DIR",
        "HAE_RASTER",
        DISCOVERY_ENV,
    ] {
        assert!(RENDERER_ENV_HELP.contains(key), "{key} missing from help");
    }

    // 4. An unsandboxed Windows opt-in changes nothing on Linux: an
    //    unverifiable bubblewrap still refuses, and the reason says the
    //    switch is Windows-only.
    unset(DISCOVERY_ENV);
    set(ALLOW_ENV, "1");
    set("HAE_BWRAP", "/nonexistent/bwrap");
    let cfg = AdapterConfig::from_env_detailed().expect("explicit values build a config");
    cfg_work_dir(cfg, work.path(), |cfg| {
        let e = preview(cfg);
        assert_eq!(e.reason(), Some("sandbox_unverified"), "{e}");
        let message = e.to_string();
        assert!(message.contains("bwrap not found"), "{message}");
        assert!(message.contains(ALLOW_ENV), "{message}");
        assert!(message.contains("only honored on Windows"), "{message}");
    });

    // 5. The same configuration with a working bwrap gets past the isolation
    //    self-test (proving 4 failed for isolation, not for a typo). What
    //    happens next depends on whether a renderer is discoverable here, so
    //    assert only that isolation is no longer the reason.
    set("HAE_BWRAP", "/usr/bin/bwrap");
    let cfg = AdapterConfig::from_env_detailed().expect("explicit values build a config");
    cfg_work_dir(cfg, work.path(), |cfg| {
        let e = preview(cfg);
        assert_ne!(
            e.reason(),
            Some("sandbox_unverified"),
            "verified bubblewrap must pass the gate: {e}"
        );
    });

    unset("HAE_RHWP");
    unset("HAE_FONT_DIR");
    unset("HAE_BWRAP");
    unset(ALLOW_ENV);
    let _ = fs::metadata("/nonexistent");
}

/// Give the config a private work directory before running it.
fn cfg_work_dir(mut cfg: AdapterConfig, dir: &std::path::Path, f: impl FnOnce(AdapterConfig)) {
    cfg.work_dir = dir.to_path_buf();
    f(cfg);
}
