#![cfg(windows)]

use hae_diag::Code;
use hae_render_rhwp::sandbox::TempDir;
use hae_render_rhwp::{AdapterConfig, RenderRequest, RhwpAdapter};
use std::fs;

#[test]
fn windows_preview_refuses_to_start_without_verified_renderer_isolation() {
    let work = TempDir::new_in(&std::env::temp_dir(), "hae-windows-preview-").unwrap();
    let fonts = work.path().join("fonts");
    fs::create_dir(&fonts).unwrap();
    let renderer = work.path().join("fake-rhwp.exe");
    fs::write(&renderer, b"renderer must not be opened or started").unwrap();

    let mut config = AdapterConfig::new(&renderer, &fonts);
    config.work_dir = work.path().to_path_buf();
    let adapter = RhwpAdapter::new(config);
    let request = RenderRequest {
        document_id: "windows-preview",
        content_revision: 1,
        hwpx: b"PK",
        pages: None,
        want_png: true,
        png: Default::default(),
    };

    let error = adapter.preview(&request, None).unwrap_err();
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

    let staged: Vec<_> = fs::read_dir(work.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| {
            name.starts_with("hae-render-")
                || name.starts_with("hae-raster-")
                || name.starts_with("hae-selftest-")
        })
        .collect();
    assert!(
        staged.is_empty(),
        "preview staged subprocess inputs: {staged:?}"
    );
}
