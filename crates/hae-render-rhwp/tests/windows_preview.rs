#![cfg(windows)]

//! Windows preview gate: refusal by default, and the explicit
//! `HAE_ALLOW_UNSANDBOXED_PREVIEW` opt-in that reaches the unsandboxed runner.
//!
//! These tests are compile-checked with `cargo check --target
//! x86_64-pc-windows-gnu`; they have never been executed on Windows. They
//! assert the decision and the failure classification, not that a run
//! produces pixels.

use hae_diag::Code;
use hae_render_rhwp::sandbox::TempDir;
use hae_render_rhwp::unsandboxed::{Policy, ALLOW_ENV};
use hae_render_rhwp::{AdapterConfig, RenderRequest, RhwpAdapter};
use std::fs;
use std::path::Path;

fn request() -> RenderRequest<'static> {
    RenderRequest {
        document_id: "windows-preview",
        content_revision: 1,
        hwpx: b"PK",
        pages: None,
        want_png: true,
        png: Default::default(),
    }
}

fn staged_dirs(work: &Path) -> Vec<String> {
    fs::read_dir(work)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| {
            name.starts_with("hae-render-")
                || name.starts_with("hae-raster-")
                || name.starts_with("hae-selftest-")
                || name.starts_with("hae-unsandboxed-")
        })
        .collect()
}

fn adapter(work: &Path, renderer: &Path, fonts: &Path, policy: Policy) -> RhwpAdapter {
    let mut config = AdapterConfig::new(renderer, fonts);
    config.work_dir = work.to_path_buf();
    config.unsandboxed_preview = policy;
    RhwpAdapter::new(config)
}

#[test]
fn windows_preview_refuses_to_start_without_verified_renderer_isolation() {
    let work = TempDir::new_in(&std::env::temp_dir(), "hae-windows-preview-").unwrap();
    let fonts = work.path().join("fonts");
    fs::create_dir(&fonts).unwrap();
    let renderer = work.path().join("fake-rhwp.exe");
    fs::write(&renderer, b"renderer must not be opened or started").unwrap();

    let adapter = adapter(work.path(), &renderer, &fonts, Policy::Off);
    let error = adapter.preview(&request(), None).unwrap_err();
    assert_eq!(error.code(), Code::RendererUnavailable);
    assert_eq!(error.reason(), Some("sandbox_unverified"));
    let message = error.to_string();
    assert!(
        message.to_ascii_lowercase().contains("windows"),
        "{message}"
    );
    assert!(message.contains("Linux bubblewrap"), "{message}");
    assert!(message.contains("prlimit RLIMIT_AS"), "{message}");
    assert!(message.contains("renderer was not started"), "{message}");
    // The refusal says how to opt in and what that costs.
    assert!(message.contains(ALLOW_ENV), "{message}");
    assert!(message.contains("sandbox_unverified"), "{message}");

    assert!(
        staged_dirs(work.path()).is_empty(),
        "preview staged subprocess inputs: {:?}",
        staged_dirs(work.path())
    );
}

#[test]
fn windows_opt_in_reaches_the_runner_instead_of_the_isolation_gate() {
    let work = TempDir::new_in(&std::env::temp_dir(), "hae-windows-optin-").unwrap();
    let fonts = work.path().join("fonts");
    fs::create_dir(&fonts).unwrap();
    let renderer = work.path().join("missing-rhwp.exe");

    let adapter = adapter(work.path(), &renderer, &fonts, Policy::On);
    let error = adapter.preview(&request(), None).unwrap_err();
    // Past the platform gate: staging is reached and fails on the missing
    // binary, which is a different reason than `sandbox_unverified`.
    assert_eq!(error.reason(), Some("renderer_missing"), "{error}");
    assert!(staged_dirs(work.path()).is_empty());
}

#[test]
fn windows_opt_in_classifies_a_non_executable_stage_as_a_spawn_failure() {
    let work = TempDir::new_in(&std::env::temp_dir(), "hae-windows-spawn-").unwrap();
    let fonts = work.path().join("fonts");
    fs::create_dir(&fonts).unwrap();
    let renderer = work.path().join("fake-rhwp.exe");
    // A staged copy that exists but is not a Windows image: CreateProcess
    // fails, which is an explicit `spawn_failed`, never a silent success.
    fs::write(&renderer, b"#!/bin/sh\necho rhwp\n").unwrap();

    let adapter = adapter(work.path(), &renderer, &fonts, Policy::On);
    let error = adapter.preview(&request(), None).unwrap_err();
    assert_eq!(error.reason(), Some("spawn_failed"), "{error}");
    assert!(
        staged_dirs(work.path()).is_empty(),
        "staged inputs must be removed with the run: {:?}",
        staged_dirs(work.path())
    );
}

#[test]
fn windows_opt_in_policy_is_not_enabled_by_a_lookalike_value() {
    // A value that merely sounds like an opt-in must stay refused.
    for value in ["", "0", "no", "off", "unsandboxed", "yes please"] {
        let work = TempDir::new_in(&std::env::temp_dir(), "hae-windows-policy-").unwrap();
        let fonts = work.path().join("fonts");
        fs::create_dir(&fonts).unwrap();
        let renderer = work.path().join("fake-rhwp.exe");
        fs::write(&renderer, b"x").unwrap();
        let policy = Policy::from_value(Some(std::ffi::OsStr::new(value)));
        let adapter = adapter(work.path(), &renderer, &fonts, policy);
        let error = adapter.preview(&request(), None).unwrap_err();
        assert_eq!(
            error.reason(),
            Some("sandbox_unverified"),
            "{value:?} must not opt in: {error}"
        );
    }
}
