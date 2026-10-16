mod common;
use common::*;
use hae_render_rhwp::{RenderRequest, RenderStatus};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    time::{Duration, Instant},
};
fn req() -> RenderRequest<'static> {
    RenderRequest {
        document_id: "budget",
        content_revision: 1,
        hwpx: b"PK",
        pages: None,
        want_png: false,
        png: Default::default(),
    }
}
const ONE: &str = r#"
if [ "$1" = dump-pages ]; then echo '{"pages":[{"section":0}]}'; exit; fi
printf '<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"/>' > /out/p.svg
N=$(wc -c < /out/p.svg)
echo "{\"format\":\"svg\",\"pageCount\":1,\"pages\":[{\"page\":0,\"path\":\"/out/p.svg\",\"bytes\":$N}]}"
"#;
#[test]
fn svg_parse_and_retention_must_fit_session_budget() {
    let script = r#"
if [ "$1" = dump-pages ]; then echo '{"pages":[{}]}'; exit; fi
printf '<svg xmlns="http://www.w3.org/2000/svg"><!--' > /out/p.svg
head -c 4194304 /dev/zero | tr '\000' a >> /out/p.svg
printf -- '--></svg>' >> /out/p.svg
N=$(wc -c < /out/p.svg)
echo "{\"format\":\"svg\",\"pageCount\":1,\"pages\":[{\"page\":0,\"path\":\"/out/p.svg\",\"bytes\":$N}]}"
"#;
    let (env, _d) = fake_env(script, |c| c.limits.max_session_memory = 32 * 1024 * 1024);
    let result = env.adapter.preview(&req(), None);
    assert!(
        result.is_err(),
        "preview exceeded its session memory allowance"
    );
    let e = result.err().unwrap();
    assert_eq!(e.reason(), Some("memory_limit"), "{e}");
}
#[test]
fn renderer_inherits_os_address_space_limit() {
    let script = format!("python3 -c 'import resource; assert resource.getrlimit(resource.RLIMIT_AS)[1] > 0 and resource.getrlimit(resource.RLIMIT_AS)[1] <= 536870912' || exit 9\n{ONE}");
    let (env, _d) = fake_env(&script, |_| {});
    let out = env.adapter.preview(&req(), None).unwrap();
    assert_eq!(out.status, RenderStatus::Rendered);
}
#[test]
fn export_only_generates_allowed_requested_pages_and_keeps_total() {
    let script = r#"
if [ "$1" = dump-pages ]; then echo '{"pages":[{},{},{}]}'; exit; fi
[ "$1" = export-svg ] || exit 10
# The adapter must explicitly select physical page 2 (CLI page 1).
[ "$5" = --json ] && [ "$6" = --page ] && [ "$7" = 1 ] || exit 11
printf '<svg xmlns="http://www.w3.org/2000/svg"/>' > /out/p2.svg
N=$(wc -c < /out/p2.svg)
echo "{\"format\":\"svg\",\"pageCount\":3,\"renderedCount\":1,\"pages\":[{\"page\":1,\"path\":\"/out/p2.svg\",\"bytes\":$N}]}"
"#;
    let (env, _d) = fake_env_pages(script, 3, |c| c.limits.max_preview_pages = 2);
    let mut r = req();
    r.pages = Some(&[2, 3]);
    let out = env.adapter.preview(&r, None).unwrap();
    assert_eq!(out.total_pages, 3);
    assert_eq!(out.pages.len(), 1);
    assert_eq!(out.pages[0].page, 2);
    assert_eq!(out.status, RenderStatus::Partial);
}
#[test]
fn bwrap_version_probe_respects_deadline() {
    let dir =
        hae_render_rhwp::sandbox::TempDir::new_in(&std::env::temp_dir(), "hae-version-wrapper-")
            .unwrap();
    let wrapper = dir.path().join("bwrap");
    fs::write(&wrapper, "#!/usr/bin/sh\nif [ \"$1\" = --version ]; then sleep 3; echo bubblewrap-delayed; exit; fi\nexec /usr/bin/bwrap \"$@\"\n").unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o755)).unwrap();
    let (env, _d) = fake_env(ONE, |c| {
        c.bwrap_path = wrapper;
        c.limits.renderer_timeout_secs = 1;
    });
    let t = Instant::now();
    let out = env.adapter.preview(&req(), None).unwrap();
    assert!(
        t.elapsed() < Duration::from_secs(2),
        "elapsed {:?}",
        t.elapsed()
    );
    assert!(out.metadata.sandbox["version"].is_null());
}

#[test]
fn cancellation_during_optional_version_probe_cancels_preview() {
    let dir =
        hae_render_rhwp::sandbox::TempDir::new_in(&std::env::temp_dir(), "hae-version-cancel-")
            .unwrap();
    let wrapper = dir.path().join("bwrap");
    let marker = dir.path().join("started");
    fs::write(&wrapper, format!("#!/usr/bin/sh\nif [ \"$1\" = --version ]; then touch '{}'; sleep 5; exit; fi\nexec /usr/bin/bwrap \"$@\"\n", marker.display())).unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o755)).unwrap();
    let (env, _d) = fake_env(ONE, |c| c.bwrap_path = wrapper);
    let token = hae_render_rhwp::CancelToken::new();
    let other = token.clone();
    let handle = std::thread::spawn(move || {
        let started = Instant::now();
        while !marker.exists() && started.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(marker.exists());
        other.cancel();
    });
    let result = env.adapter.preview(&req(), Some(&token));
    handle.join().unwrap();
    assert!(
        result.is_err(),
        "cancellation must reach the caller during metadata probing"
    );
    assert_eq!(result.err().unwrap().reason(), Some("cancelled"));
}

#[test]
fn caller_budget_also_applies_when_fonts_are_cached() {
    let (env, _d) = fake_env(ONE, |_| {});
    fs::write(env._fonts.path().join("budget.ttf"), vec![0u8; 512 * 1024]).unwrap();
    assert_eq!(
        env.adapter.preview(&req(), None).unwrap().status,
        RenderStatus::Rendered
    );
    let result = env
        .adapter
        .preview_with_budget(&req(), None, 2 * 1024 * 1024);
    assert!(result.is_err());
    assert_eq!(result.err().unwrap().reason(), Some("memory_limit"));
}

#[test]
fn duplicate_path_across_selected_page_manifests_is_rejected() {
    let script = r#"
P=$7
printf '<svg xmlns="http://www.w3.org/2000/svg"/>' > /out/shared.svg
N=$(wc -c < /out/shared.svg)
echo "{\"format\":\"svg\",\"pageCount\":2,\"renderedCount\":1,\"pages\":[{\"page\":$P,\"path\":\"/out/shared.svg\",\"bytes\":$N}]}"
"#;
    let (env, _d) = fake_env_pages(script, 2, |_| {});
    let result = env.adapter.preview(&req(), None);
    assert!(
        result.is_err(),
        "duplicate paths must not multiply retained SVG buffers"
    );
    assert_eq!(result.err().unwrap().reason(), Some("invalid_output"));
}

#[test]
fn bwrap_version_flood_is_bounded_and_descendants_stop() {
    let dir =
        hae_render_rhwp::sandbox::TempDir::new_in(&std::env::temp_dir(), "hae-version-flood-")
            .unwrap();
    let wrapper = dir.path().join("bwrap");
    let ticks = dir.path().join("ticks");
    fs::write(&wrapper, format!("#!/usr/bin/sh\nif [ \"$1\" = --version ]; then (while true; do echo tick >> '{}'; sleep 0.01; done) & yes; fi\nexec /usr/bin/bwrap \"$@\"\n", ticks.display())).unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o755)).unwrap();
    let (env, _d) = fake_env(ONE, |c| c.bwrap_path = wrapper);
    let t = Instant::now();
    let out = env.adapter.preview(&req(), None).unwrap();
    assert!(out.metadata.sandbox["version"].is_null());
    assert!(t.elapsed() < Duration::from_secs(2));
    let before = fs::read(&ticks).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(
        fs::read(ticks).unwrap(),
        before,
        "metadata probe descendants survived cleanup"
    );
}

#[test]
fn configured_image_pixels_reaches_rasterizer() {
    let script = ONE.replace("width=\"1\" height=\"1\"", "width=\"2\" height=\"2\"");
    let (env, _d) = fake_env(&script, |c| c.limits.max_image_pixels = 1);
    let mut r = req();
    r.want_png = true;
    let out = env.adapter.preview(&r, None).unwrap();
    assert!(out.pages[0].png.is_none());
    assert!(out.notes.iter().any(|n| n.code == "PNG_FAILED"));
}

#[test]
fn memory_is_reserved_before_opening_a_large_listed_svg() {
    let script = r#"echo '{"format":"svg","pageCount":1,"pages":[{"page":0,"path":"/out/absent.svg","bytes":4194304}]}'"#;
    let (env, _d) = fake_env(script, |c| c.limits.max_session_memory = 32 * 1024 * 1024);
    let result = env.adapter.preview(&req(), None);
    assert!(result.is_err());
    // A reader-first implementation would return invalid_output for the
    // nonexistent file instead of rejecting its reservation first.
    assert_eq!(result.err().unwrap().reason(), Some("memory_limit"));
}

#[test]
fn os_memory_limit_is_inherited_and_denies_oversized_allocation() {
    let script = format!(
        r#"
python3 - <<'PYTHON'
import resource, subprocess, sys
hard = resource.getrlimit(resource.RLIMIT_AS)[1]
assert 0 < hard <= 536870912
subprocess.run([sys.executable, '-c', 'import resource; assert resource.getrlimit(resource.RLIMIT_AS)[1] == ' + str(hard)], check=True)
try:
    bytearray(hard)
except MemoryError:
    pass
else:
    sys.exit(9)
PYTHON
[ "$?" = 0 ] || exit 9
{ONE}
"#
    );
    let (env, _d) = fake_env(&script, |_| {});
    assert_eq!(
        env.adapter.preview(&req(), None).unwrap().status,
        RenderStatus::Rendered
    );
}
