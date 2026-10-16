//! Task 3-2: sandboxed rhwp rendering, failure handling, metadata.

mod common;

use common::*;
use hae_diag::Code;
use hae_render_rhwp::{CancelToken, RenderRequest, RenderStatus};
use std::fs;

fn request<'a>(bytes: &'a [u8], pages: Option<&'a [u32]>) -> RenderRequest<'a> {
    RenderRequest {
        document_id: "doc-test",
        content_revision: 7,
        hwpx: bytes,
        pages,
        want_png: false,
        png: Default::default(),
    }
}

fn corpus_bytes() -> Vec<u8> {
    fs::read(corpus("research-school-plan-reference.hwpx")).unwrap()
}

#[test]
fn renders_corpus_document_with_metadata() {
    let env = real_env();
    let bytes = corpus_bytes();
    let t = std::time::Instant::now();
    let out = env
        .adapter
        .preview(&request(&bytes, None), None)
        .expect("preview");
    eprintln!(
        "RENDER_TIME corpus research-school-plan-reference: {} ms (adapter), {} ms (wall incl. first self-test)",
        out.render_millis,
        t.elapsed().as_millis()
    );
    assert_eq!(out.status, RenderStatus::Rendered);
    assert_eq!(out.total_pages, 28);
    assert_eq!(out.pages.len(), 28);
    assert_eq!(out.pages[0].page, 1);
    assert!(out
        .pages
        .iter()
        .all(|p| p.svg.starts_with(b"<svg") || p.svg.starts_with(b"<?xml")));
    assert!(out.diagnostics.is_empty());
    // rhwp reported layout warnings for this document; they become notes.
    assert!(
        out.notes.iter().any(|n| n.code.starts_with("LAYOUT_")),
        "{:?}",
        out.notes
    );

    let m = &out.metadata;
    assert_eq!(m.document_id, "doc-test");
    assert_eq!(m.content_revision, 7);
    assert!(m.layout_revision.starts_with("lay-"));
    assert_eq!(m.input_sha256, hae_package::sha256_hex(&bytes));
    assert_eq!(
        m.renderer["sha256"],
        hae_package::sha256_hex(&fs::read(rhwp_path()).unwrap())
    );
    assert!(m.renderer["version"].as_str().unwrap().starts_with("rhwp"));
    assert_eq!(m.renderer["commit"], hae_render_rhwp::PINNED_COMMIT);
    assert_eq!(m.sandbox["selfTest"], "passed");
    assert_eq!(m.fonts["hash"].as_str().unwrap().len(), 64);
    assert_eq!(m.coverage, "unknown");
    assert_eq!(m.native_comparison, "not_checked");
    assert_eq!(m.page_conditions["totalPages"], 28);

    // Same inputs → same layout revision; different revision label does not change it.
    let again = env.adapter.preview(&request(&bytes, None), None).unwrap();
    assert_eq!(again.metadata.layout_revision, m.layout_revision);
    let sub = env
        .adapter
        .preview(&request(&bytes, Some(&[1, 3])), None)
        .unwrap();
    assert_ne!(sub.metadata.layout_revision, m.layout_revision);
}

#[test]
fn page_selection_and_missing_pages() {
    let env = real_env();
    let bytes = corpus_bytes();
    let out = env
        .adapter
        .preview(&request(&bytes, Some(&[2, 5])), None)
        .unwrap();
    assert_eq!(
        out.pages.iter().map(|p| p.page).collect::<Vec<_>>(),
        vec![2, 5]
    );
    assert_eq!(out.status, RenderStatus::Rendered);
    let out = env
        .adapter
        .preview(&request(&bytes, Some(&[1, 99])), None)
        .unwrap();
    assert_eq!(out.pages.len(), 1);
    assert_eq!(out.status, RenderStatus::Partial);
    assert!(out
        .notes
        .iter()
        .any(|n| n.code == "PAGE_NOT_RENDERED" && n.page == Some(99)));
}

#[test]
fn page_limit_makes_a_partial_result() {
    let fonts = font_dir();
    let mut cfg = hae_render_rhwp::AdapterConfig::new(rhwp_path(), fonts.path());
    cfg.limits.max_preview_pages = 10;
    let adapter = hae_render_rhwp::RhwpAdapter::new(cfg);
    let bytes = corpus_bytes();
    let out = adapter.preview(&request(&bytes, None), None).unwrap();
    assert_eq!(out.status, RenderStatus::Partial);
    assert_eq!(out.total_pages, 28);
    assert_eq!(out.pages.len(), 10);
    assert_eq!(out.diagnostics.len(), 1);
    assert_eq!(out.diagnostics[0].code, Code::PreviewPageLimit);
}

#[test]
fn unsafe_pages_are_withheld() {
    let (env, _d) = fake_env_pages(
        r#"
printf '<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"/>' > /out/p1.svg
printf '<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>' > /out/p2.svg
P=$7
FILE=/out/p$((P + 1)).svg
N=$(wc -c < "$FILE")
echo "{\"format\":\"svg\",\"pageCount\":2,\"renderedCount\":1,\"pages\":[{\"page\":$P,\"path\":\"$FILE\",\"bytes\":$N}]}"
"#,
        2,
        |_| {},
    );
    let out = env.adapter.preview(&request(b"PK", None), None).unwrap();
    assert_eq!(out.status, RenderStatus::Partial);
    assert_eq!(out.pages.len(), 1);
    assert_eq!(out.pages[0].page, 1);
    let n = out
        .notes
        .iter()
        .find(|n| n.code == "SVG_UNSAFE")
        .expect("unsafe note");
    assert_eq!(n.page, Some(2));
}

#[test]
fn all_pages_unsafe_is_failed() {
    let (env, _d) = fake_env(
        r#"
printf '<svg xmlns="http://www.w3.org/2000/svg"><script>x</script></svg>' > /out/p1.svg
S1=$(wc -c < /out/p1.svg)
echo "{\"format\":\"svg\",\"pageCount\":1,\"pages\":[{\"page\":0,\"path\":\"/out/p1.svg\",\"bytes\":$S1}]}"
"#,
        |_| {},
    );
    let out = env.adapter.preview(&request(b"PK", None), None).unwrap();
    assert_eq!(out.status, RenderStatus::Failed);
    assert!(out.pages.is_empty());
}

fn reason(e: &hae_diag::EngineError) -> String {
    assert_eq!(e.code(), Code::RendererUnavailable, "{e}");
    e.reason().unwrap_or("").to_string()
}

#[test]
fn timeout_is_an_explicit_error() {
    let (env, _d) = fake_env("sleep 30", |c| c.limits.renderer_timeout_secs = 1);
    let t = std::time::Instant::now();
    let e = env
        .adapter
        .preview(&request(b"PK", None), None)
        .unwrap_err();
    assert_eq!(reason(&e), "timeout");
    assert!(t.elapsed().as_secs() < 20);
}

#[test]
fn cancellation_is_an_explicit_error() {
    let (env, _d) = fake_env("sleep 30", |_| {});
    let token = CancelToken::new();
    let t2 = token.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(1500));
        t2.cancel();
    });
    let e = env
        .adapter
        .preview(&request(b"PK", None), Some(&token))
        .unwrap_err();
    assert_eq!(reason(&e), "cancelled");
}

#[test]
fn bad_outputs_are_rejected() {
    let cases: &[(&str, &str)] = &[
        ("echo not-json", "invalid_output"),
        ("echo '{\"format\":\"svg\",\"pageCount\":1,\"pages\":[]}'", "invalid_output"),
        ("exit 3", "render_failed"),
        // Path outside /out.
        (
            "echo '{\"format\":\"svg\",\"pageCount\":1,\"pages\":[{\"page\":0,\"path\":\"/etc/passwd\",\"bytes\":5}]}'",
            "invalid_output",
        ),
        // Dot-dot escape.
        (
            "echo '{\"format\":\"svg\",\"pageCount\":1,\"pages\":[{\"page\":0,\"path\":\"/out/../etc/hostname\",\"bytes\":5}]}'",
            "invalid_output",
        ),
        // Symlink planted in /out.
        (
            "ln -s /etc/hostname /out/x.svg; echo '{\"format\":\"svg\",\"pageCount\":1,\"pages\":[{\"page\":0,\"path\":\"/out/x.svg\",\"bytes\":5}]}'",
            "invalid_output",
        ),
        // Manifest lies about the size.
        (
            "printf '<svg xmlns=\"http://www.w3.org/2000/svg\"/>' > /out/a.svg; echo '{\"format\":\"svg\",\"pageCount\":1,\"pages\":[{\"page\":0,\"path\":\"/out/a.svg\",\"bytes\":1}]}'",
            "invalid_output",
        ),
        // Listed file never created.
        (
            "echo '{\"format\":\"svg\",\"pageCount\":1,\"pages\":[{\"page\":0,\"path\":\"/out/none.svg\",\"bytes\":5}]}'",
            "invalid_output",
        ),
        // stdout flood.
        ("yes", "output_limit"),
    ];
    for (script, want) in cases {
        let (env, _d) = fake_env(script, |c| c.stdout_cap = 100_000);
        let e = env
            .adapter
            .preview(&request(b"PK", None), None)
            .unwrap_err();
        assert_eq!(reason(&e), *want, "script: {script}");
    }
}

#[test]
fn output_directory_cap_is_enforced() {
    let (env, _d) = fake_env(
        "while true; do head -c 1000000 /dev/zero >> /out/big; done",
        |c| c.out_cap_bytes = 3 * 1024 * 1024,
    );
    let e = env
        .adapter
        .preview(&request(b"PK", None), None)
        .unwrap_err();
    assert_eq!(reason(&e), "output_limit");
}

#[test]
fn renderer_cannot_reach_outside_its_sandbox() {
    // The "renderer" tries to write into the input directory, the host /tmp
    // and a host path; none may succeed, and it still produces valid output.
    let marker = std::env::temp_dir().join(format!("hae-escape-{}", std::process::id()));
    let (env, _d) = fake_env(
        &format!(
            r#"
echo x > /in/evil 2>/dev/null && echo INPUT_WRITABLE > /out/leak
echo x > '{m}' 2>/dev/null
printf '<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"/>' > /out/p1.svg
S1=$(wc -c < /out/p1.svg)
echo "{{\"format\":\"svg\",\"pageCount\":1,\"pages\":[{{\"page\":0,\"path\":\"/out/p1.svg\",\"bytes\":$S1}}]}}"
"#,
            m = marker.display()
        ),
        |_| {},
    );
    let out = env.adapter.preview(&request(b"PK", None), None).unwrap();
    assert_eq!(out.status, RenderStatus::Rendered);
    assert!(!marker.exists(), "sandbox wrote to the host");
}

#[test]
fn unverified_sandbox_means_renderer_unavailable() {
    let fonts = font_dir();
    let mut cfg = hae_render_rhwp::AdapterConfig::new(rhwp_path(), fonts.path());
    cfg.bwrap_path = "/nonexistent/bwrap".into();
    let adapter = hae_render_rhwp::RhwpAdapter::new(cfg);
    let e = adapter.preview(&request(b"PK", None), None).unwrap_err();
    assert_eq!(reason(&e), "sandbox_unverified");
}

#[test]
fn missing_renderer_is_reported_and_never_run_unsandboxed() {
    let fonts = font_dir();
    let cfg = hae_render_rhwp::AdapterConfig::new("/nonexistent/rhwp", fonts.path());
    let adapter = hae_render_rhwp::RhwpAdapter::new(cfg);
    let e = adapter.preview(&request(b"PK", None), None).unwrap_err();
    assert_eq!(reason(&e), "renderer_missing");
}
