#![cfg(target_os = "linux")]

//! Mandatory PNG rasterisation in the supervised `hae-raster` worker: identity
//! with the in-process reference, sandbox/RLIMIT_AS evidence from a fake
//! worker, containment of worker failures, deadline, cancellation, missing
//! worker and configured caps. Every allocation here is finite and bounded by
//! the worker's own RLIMIT_AS.

mod common;
use common::*;
use hae_package::sha256_hex;
use hae_render_rhwp::png::{rasterize_svg, CheckedSvg, PngOptions};
use hae_render_rhwp::sandbox::TempDir;
use hae_render_rhwp::{CancelToken, RenderRequest, RenderStatus};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const TEXT_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="60"><text x="5" y="40" font-family="NanumGothic" font-size="24">한글 Abc</text></svg>"#;
const TINY_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"/>"#;

/// Fake rhwp body that exports `svg` as page 1 (the harness answers
/// `--version` and `dump-pages`).
fn page_script(svg: &str) -> String {
    format!(
        r#"
printf '%s' '{svg}' > /out/p.svg
N=$(wc -c < /out/p.svg)
echo "{{\"format\":\"svg\",\"pageCount\":1,\"pages\":[{{\"page\":0,\"path\":\"/out/p.svg\",\"bytes\":$N}}]}}"
"#
    )
}

fn req(png: bool) -> RenderRequest<'static> {
    RenderRequest {
        document_id: "raster",
        content_revision: 7,
        hwpx: b"PK",
        pages: None,
        want_png: png,
        png: Default::default(),
    }
}

const PRELUDE: &str = r#"#!/usr/bin/python3
import sys, os, re, json, hashlib, struct, zlib, resource, time
A = dict(zip(sys.argv[1::2], sys.argv[2::2]))
def png(w, h):
    row = b'\x00' + b'\xff\xff\xff\xff' * w
    def chunk(t, d):
        return struct.pack('>I', len(d)) + t + d + struct.pack('>I', zlib.crc32(t + d) & 0xffffffff)
    return b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', w, h, 8, 6, 0, 0, 0)) + chunk(b'IDAT', zlib.compress(row * h)) + chunk(b'IEND', b'')
def result(data, **over):
    r = {"protocol": "hae-raster-v1", "status": "ok", "svgSha256": A['--svg-sha256'],
         "fontsHash": A['--fonts-hash'],
         "png": {"file": "page.png", "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest(), "width": 1, "height": 1},
         "fontFamilies": []}
    r.update(over)
    return r
def emit(data=None, write_png=True, **over):
    data = png(1, 1) if data is None else data
    if write_png:
        open('/out/page.png', 'wb').write(data)
    open('/out/result.json', 'w').write(json.dumps(result(data, **over)))
"#;

/// A protocol-speaking worker written in Python, run by the sandbox.
fn fake_worker(body: &str) -> (TempDir, PathBuf) {
    let dir = TempDir::new_in(&std::env::temp_dir(), "hae-test-worker-").unwrap();
    let path = dir.path().join("hae-raster");
    fs::write(&path, format!("{PRELUDE}\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    (dir, path)
}

/// Adapter whose rhwp exports `TINY_SVG` and whose raster worker is `body`.
fn with_worker(
    body: &str,
    tweak: impl FnOnce(&mut hae_render_rhwp::AdapterConfig),
) -> (Env, TempDir, TempDir, TempDir) {
    let (wdir, wpath) = fake_worker(body);
    let work = TempDir::new_in(&std::env::temp_dir(), "hae-test-work-").unwrap();
    let work_path = work.path().to_path_buf();
    let (env, rdir) = fake_env(&page_script(TINY_SVG), |c| {
        c.raster_path = wpath;
        c.work_dir = work_path;
        tweak(c);
    });
    (env, wdir, rdir, work)
}

fn leftovers(work: &TempDir) -> Vec<String> {
    fs::read_dir(work.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect()
}

fn png_failed(out: &hae_render_rhwp::PreviewOutput) -> String {
    out.notes
        .iter()
        .find(|n| n.code == "PNG_FAILED")
        .unwrap_or_else(|| panic!("no PNG_FAILED note: {:?}", out.notes))
        .message
        .clone()
}

// ------------------------------------------------------------------ identity

#[test]
fn real_worker_png_equals_the_reference_and_uses_the_exact_font_snapshot() {
    let (env, _d) = fake_env(&page_script(TEXT_SVG), |_| {});
    // A second, non-font payload with a path the worker must never see by name.
    fs::write(env._fonts.path().join("zz 글꼴.otf"), b"not a font").unwrap();
    let out = env.adapter.preview(&req(true), None).unwrap();
    assert_eq!(out.status, RenderStatus::Rendered, "{:?}", out.notes);
    let page = &out.pages[0];
    let png = page
        .png
        .as_ref()
        .unwrap_or_else(|| panic!("no PNG: {:?}", out.notes));
    assert_eq!(page.png_sha256.as_deref(), Some(sha256_hex(png).as_str()));

    // The host never built a font database for the preview.
    let fonts = env.adapter.fonts().unwrap();
    assert!(
        !fonts.database_loaded(),
        "host initialised the font database"
    );

    // Same bytes as the in-process low-level API over the same font set.
    let reference = rasterize_svg(
        CheckedSvg::new(&page.svg).unwrap(),
        &fonts,
        &PngOptions::default(),
    )
    .unwrap();
    assert_eq!(png, &reference, "worker PNG differs from the reference");
    if fonts.files.iter().any(|f| f.path == "NanumGothic.ttf") {
        let pm = resvg::tiny_skia::Pixmap::decode_png(png).unwrap();
        let dark = pm.pixels().iter().filter(|p| p.red() < 90).count();
        assert!(dark > 50, "worker PNG has no text pixels ({dark})");
    }

    // Metadata identifies the worker executable and the exact font snapshot.
    let r = &out.metadata.renderer["rasterizer"];
    assert_eq!(r["name"], "hae-raster");
    assert_eq!(r["protocol"], "hae-raster-v1");
    assert_eq!(
        r["sha256"].as_str().unwrap(),
        sha256_hex(&fs::read(raster_bin()).unwrap())
    );
    assert_eq!(r["fontSnapshot"]["hash"], out.metadata.fonts["hash"]);
    assert_eq!(r["fontSnapshot"]["hash"].as_str().unwrap(), fonts.hash);
    assert_eq!(r["fontSnapshot"]["files"], fonts.files.len());
    assert_eq!(
        out.metadata.sandbox["rasterMemoryLimit"]["resource"],
        "RLIMIT_AS"
    );
}

#[test]
fn corpus_page_png_from_the_worker_matches_the_reference_rasterizer() {
    for name in [
        "research-school-plan-reference.hwpx",
        "gyehoek-reference.hwpx",
    ] {
        // Fresh adapter per document: the reference run below builds a font
        // database on the cached set, which would mask a host-side build.
        let env = real_env();
        let bytes = fs::read(corpus(name)).unwrap();
        let out = env
            .adapter
            .preview(
                &RenderRequest {
                    document_id: "corpus",
                    content_revision: 1,
                    hwpx: &bytes,
                    pages: Some(&[1]),
                    want_png: true,
                    png: Default::default(),
                },
                None,
            )
            .unwrap();
        assert_eq!(
            out.status,
            RenderStatus::Rendered,
            "{name}: {:?}",
            out.notes
        );
        let p = &out.pages[0];
        let png = p
            .png
            .as_ref()
            .unwrap_or_else(|| panic!("{name}: {:?}", out.notes));
        let fonts = env.adapter.fonts().unwrap();
        assert!(!fonts.database_loaded());
        let reference = rasterize_svg(
            CheckedSvg::new(&p.svg).unwrap(),
            &fonts,
            &PngOptions::default(),
        )
        .unwrap();
        assert_eq!(png, &reference, "{name}: worker PNG differs from reference");
        assert_eq!(
            out.metadata.renderer["rasterizer"]["fontSnapshot"]["hash"],
            out.metadata.fonts["hash"]
        );
    }
}

#[test]
fn font_availability_notes_come_from_the_worker_snapshot() {
    let svg = TINY_SVG.replace("/>", r#"><text font-family="No Such Face">x</text></svg>"#);
    let (env, _d) = fake_env(&page_script(&svg), |_| {});
    let with_png = env.adapter.preview(&req(true), None).unwrap();
    assert!(
        with_png.notes.iter().any(|n| n.code == "FONT_UNAVAILABLE"),
        "{:?}",
        with_png.notes
    );
    // Without a PNG there is no worker and no host font database.
    let without = env.adapter.preview(&req(false), None).unwrap();
    assert!(without.notes.iter().all(|n| n.code != "FONT_UNAVAILABLE"));
    assert!(!env.adapter.fonts().unwrap().database_loaded());
}

// ------------------------------------------------- sandbox / RLIMIT evidence

#[test]
fn worker_runs_sandboxed_under_rlimit_as_with_the_exact_font_snapshot() {
    let body = r#"
bad = []
hard = resource.getrlimit(resource.RLIMIT_AS)[1]
if not (0 < hard < 512 * 1024 * 1024 - 8 * 1024 * 1024):
    bad.append('rlimit %d' % hard)
if not (hard - 4 < int(A['--max-decoded-bytes']) * 4 <= hard):
    bad.append('decoded budget is not a quarter of the address space')
for p in ('/in/x', '/in/fonts/0000.font', '/usr/x'):
    try:
        open(p, 'w'); bad.append('writable ' + p)
    except OSError:
        pass
if os.listdir('/fonts'):
    bad.append('live font directory visible')
if os.path.exists('/home') or os.path.exists('/root'):
    bad.append('host home visible')
ifaces = [l.split(':')[0].strip() for l in open('/proc/net/dev').read().splitlines()[2:]]
if any(i and i != 'lo' for i in ifaces):
    bad.append('network %r' % ifaces)
if set(os.environ) - {'PATH', 'HOME', 'LANG', 'PWD', 'SHLVL', '_', 'OLDPWD'}:
    bad.append('environment leaked')
if sorted(os.listdir('/in')) != ['fonts', 'fonts.json', 'hae-raster', 'page.svg']:
    bad.append('stage %r' % sorted(os.listdir('/in')))
m = json.load(open(A['--fonts-manifest']))['fonts']
names = sorted(e['file'] for e in m)
if names != sorted(os.listdir('/in/fonts')) or not all(re.fullmatch(r'\d{4}\.font', n) for n in names):
    bad.append('font filenames %r' % names)
lines = ''
for e in m:
    d = open('/in/fonts/' + e['file'], 'rb').read()
    if len(d) != e['bytes'] or hashlib.sha256(d).hexdigest() != e['sha256']:
        bad.append('font ' + e['file'])
    lines += '%s\t%d\t%s\n' % (e['path'], e['bytes'], e['sha256'])
if hashlib.sha256(lines.encode()).hexdigest() != A['--fonts-hash']:
    bad.append('font set hash')
if len(m) != 2 or [e['path'] for e in m] != ['a.ttf', 'zz 글꼴.otf']:
    bad.append('manifest paths %r' % [e['path'] for e in m])
if hashlib.sha256(open(A['--svg'], 'rb').read()).hexdigest() != A['--svg-sha256']:
    bad.append('svg hash')
if bad:
    sys.stderr.write('; '.join(bad)); sys.exit(9)
emit(fontFamilies=['nanum'])
"#;
    let (env, _w, _r, work) = with_worker(body, |_| {});
    fs::write(env._fonts.path().join("a.ttf"), b"first payload").unwrap();
    fs::write(env._fonts.path().join("zz 글꼴.otf"), b"second payload").unwrap();
    // Only the two payloads above count.
    let _ = fs::remove_file(env._fonts.path().join("NanumGothic.ttf"));
    let out = env.adapter.preview(&req(true), None).unwrap();
    let png = out.pages[0]
        .png
        .as_ref()
        .unwrap_or_else(|| panic!("worker checks failed: {}", png_failed(&out)));
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    assert!(leftovers(&work).is_empty(), "{:?}", leftovers(&work));
}

// --------------------------------------------------------------- containment

#[test]
fn worker_failures_are_contained_per_page_and_the_parent_keeps_responding() {
    let cases: [(&str, &str); 6] = [
        // Over-allocation: finite requests the OS address-space limit denies.
        (
            "x = []\nfor _ in range(64):\n    x.append(bytearray(64 * 1024 * 1024))\nemit()",
            "raster_worker_exit",
        ),
        ("os.kill(os.getpid(), 9)", "raster_worker_"),
        ("os.abort()", "raster_worker_"),
        ("sys.exit(0)", "raster_output_invalid"),
        (
            "sys.stderr.write('boom'); sys.exit(7)",
            "raster_worker_exit",
        ),
        (
            "open('/out/result.json','w').write(json.dumps({'protocol':'hae-raster-v1','status':'error','kind':'too_large','message':'canvas'})); sys.exit(3)",
            "raster_failed",
        ),
    ];
    for (body, reason) in cases {
        let (env, _w, _r, work) = with_worker(body, |_| {});
        let out = env.adapter.preview(&req(true), None).unwrap();
        assert!(out.pages[0].png.is_none(), "{body}");
        assert_eq!(out.pages[0].page, 1);
        // R5: a requested PNG is missing, so the result is Partial; the SVG
        // page is retained and nothing was rasterised in the host.
        assert_eq!(out.status, RenderStatus::Partial, "{body}");
        assert!(!out.pages[0].svg.is_empty(), "{body}");
        let m = png_failed(&out);
        assert!(m.contains(reason), "{body}: {m}");
        // The adapter is healthy afterwards and the revision is untouched.
        let again = env.adapter.preview(&req(false), None).unwrap();
        assert_eq!(again.metadata.content_revision, 7);
        assert_eq!(again.status, RenderStatus::Rendered);
        drop(env);
        assert!(
            leftovers(&work).is_empty(),
            "{body}: {:?}",
            leftovers(&work)
        );
    }
}

#[test]
fn invalid_worker_output_is_rejected_by_the_parent() {
    let cases: [(&str, &str); 9] = [
        ("emit(write_png=False)", "raster_output_invalid"),
        ("emit(data=b'not a png')", "PNG"),
        ("d = png(2, 1); emit(data=d)", "dimensions"),
        ("d = png(1, 1); emit(data=d, png={'file':'page.png','bytes':len(d),'sha256':'0'*64,'width':1,'height':1})", "SHA-256"),
        ("emit(); open('/out/extra','w').write('x')", "inventory"),
        ("emit(svgSha256='0'*64)", "different SVG"),
        ("emit(fontsHash='0'*64)", "font snapshot"),
        ("d = png(1, 1); emit(data=d, png={'file':'page.png','bytes':len(d)+1,'sha256':hashlib.sha256(d).hexdigest(),'width':1,'height':1})", "raster_output_invalid"),
        ("open('/out/result.json','w').write('{}'); open('/out/page.png','wb').write(png(1,1))", "result.json"),
    ];
    for (body, needle) in cases {
        let (env, _w, _r, _work) = with_worker(body, |_| {});
        let out = env.adapter.preview(&req(true), None).unwrap();
        assert!(out.pages[0].png.is_none(), "{body} accepted");
        assert_eq!(out.status, RenderStatus::Partial, "{body}");
        let m = png_failed(&out);
        assert!(
            m.contains("raster_output_invalid") && m.contains(needle),
            "{body}: {m}"
        );
    }
}

#[test]
fn symlinked_or_oversized_worker_png_is_rejected() {
    // A symlink at page.png must never be followed to host data.
    let (env, _w, _r, _work) = with_worker(
        "os.symlink('/etc/passwd', '/out/page.png'); open('/out/result.json','w').write(json.dumps(result(png(1,1))))",
        |_| {},
    );
    let out = env.adapter.preview(&req(true), None).unwrap();
    assert!(out.pages[0].png.is_none());
    assert!(png_failed(&out).contains("raster_output_invalid"));

    // Writing past the output cap is stopped while the worker runs.
    let (env, _w, _r, _work) = with_worker(
        "open('/out/page.png','wb').write(b'\\0' * (1024 * 1024)); emit(write_png=False)",
        |c| c.out_cap_bytes = 4096,
    );
    let out = env.adapter.preview(&req(true), None).unwrap();
    assert!(out.pages[0].png.is_none());
    let m = png_failed(&out);
    assert!(m.contains("output_limit"), "{m}");
    assert_eq!(out.status, RenderStatus::Partial);
}

// ------------------------------------------------- deadline and cancellation

#[test]
fn worker_timeout_uses_the_overall_deadline_and_leaves_nothing_behind() {
    let (env, _w, _r, work) = with_worker("time.sleep(60)", |c| c.limits.renderer_timeout_secs = 4);
    let t = Instant::now();
    let e = env.adapter.preview(&req(true), None).expect_err("timeout");
    assert_eq!(e.reason(), Some("timeout"), "{e}");
    assert!(t.elapsed() < Duration::from_secs(12), "{:?}", t.elapsed());
    assert_eq!(
        env.adapter.preview(&req(false), None).unwrap().status,
        RenderStatus::Rendered
    );
    drop(env);
    assert!(leftovers(&work).is_empty(), "{:?}", leftovers(&work));
}

#[test]
fn cancellation_reaches_a_running_worker() {
    // A bwrap wrapper marks the moment the raster worker is started.
    let dir = TempDir::new_in(&std::env::temp_dir(), "hae-raster-cancel-").unwrap();
    let wrapper = dir.path().join("bwrap");
    let marker = dir.path().join("started");
    fs::write(
        &wrapper,
        format!(
            "#!/usr/bin/sh\ncase \"$*\" in *--svg-sha256*) touch '{}';; esac\nexec /usr/bin/bwrap \"$@\"\n",
            marker.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o755)).unwrap();
    let (env, _w, _r, _work) = with_worker("time.sleep(60)", |c| c.bwrap_path = wrapper);
    let token = CancelToken::new();
    let other = token.clone();
    let started = marker.clone();
    let handle = std::thread::spawn(move || {
        let t = Instant::now();
        while !started.exists() && t.elapsed() < Duration::from_secs(20) {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(started.exists(), "worker never started");
        std::thread::sleep(Duration::from_millis(200));
        other.cancel();
    });
    let t = Instant::now();
    let e = env
        .adapter
        .preview(&req(true), Some(&token))
        .expect_err("cancelled");
    handle.join().unwrap();
    assert_eq!(e.reason(), Some("cancelled"), "{e}");
    assert!(marker.exists());
    assert!(t.elapsed() < Duration::from_secs(25));
}

// ------------------------------------------------------------ missing worker

#[test]
fn missing_worker_is_an_explicit_error_with_no_host_fallback() {
    for (path, reason) in [
        (
            PathBuf::from("/nonexistent/hae-raster"),
            "raster_worker_missing",
        ),
        (PathBuf::from("/usr"), "raster_worker_missing"),
        (PathBuf::new(), "raster_worker_not_configured"),
    ] {
        let (env, _d) = fake_env(&page_script(TINY_SVG), |c| c.raster_path = path.clone());
        let e = env
            .adapter
            .preview(&req(true), None)
            .err()
            .unwrap_or_else(|| {
                panic!("{path:?}: a PNG was produced without the configured worker")
            });
        assert_eq!(e.reason(), Some(reason), "{path:?}: {e}");
        // Nothing was rasterised in this process either.
        assert!(!env.adapter.fonts().unwrap().database_loaded());
        // SVG-only previews do not need the worker.
        let svg_only = env.adapter.preview(&req(false), None).unwrap();
        assert_eq!(svg_only.status, RenderStatus::Rendered);
        assert!(svg_only.pages[0].png.is_none());
        assert!(svg_only.metadata.renderer["rasterizer"].is_null());
    }
}

// -------------------------------------------------------------------- caps

#[test]
fn configured_pixel_and_png_caps_reach_the_real_worker() {
    let big = r#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="40"/>"#;
    // Request-level pixel cap.
    let (env, _d) = fake_env(&page_script(big), |_| {});
    let mut r = req(true);
    r.png = PngOptions {
        scale: 1.0,
        max_pixels: 100,
    };
    let out = env.adapter.preview(&r, None).unwrap();
    assert!(out.pages[0].png.is_none());
    let m = png_failed(&out);
    assert!(
        m.contains("raster_failed") && m.contains("too_large"),
        "{m}"
    );
    assert_eq!(out.status, RenderStatus::Partial);
    // Configured Limits::max_image_pixels is the ceiling of the request cap.
    let (env, _d) = fake_env(&page_script(big), |c| c.limits.max_image_pixels = 100);
    let out = env.adapter.preview(&req(true), None).unwrap();
    assert!(png_failed(&out).contains("too_large"));
    assert_eq!(out.status, RenderStatus::Partial);
    // The configured output cap bounds the encoded PNG the worker may emit.
    let (env, _d) = fake_env(&page_script(big), |c| c.out_cap_bytes = 64);
    let out = env.adapter.preview(&req(true), None).unwrap();
    assert!(out.pages[0].png.is_none());
    assert!(png_failed(&out).contains("png_too_large"));
    assert_eq!(out.status, RenderStatus::Partial);
    // Within the caps the same page renders.
    let (env, _d) = fake_env(&page_script(big), |_| {});
    let ok = env.adapter.preview(&req(true), None).unwrap();
    assert!(ok.pages[0].png.is_some());
    assert_eq!(ok.status, RenderStatus::Rendered);
}

#[test]
fn too_little_session_memory_is_refused_before_the_worker_starts() {
    let (env, _w, _r, _work) = with_worker("open('/tmp/never','w')", |c| {
        c.limits.max_session_memory = 48 * 1024 * 1024
    });
    let e = env.adapter.preview(&req(true), None).expect_err("refused");
    assert_eq!(e.reason(), Some("memory_limit"), "{e}");
}
