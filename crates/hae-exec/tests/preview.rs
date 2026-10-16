//! Task 3-5: document.preview through the executor.

use hae_diag::{Code, Limits};
use hae_exec::{ResponseEnvelope, SessionExecutor};
use hae_render_rhwp::{AdapterConfig, RhwpAdapter};
use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn corpus_dir() -> PathBuf {
    root().join("tests/fixtures/corpus").canonicalize().unwrap()
}

fn rhwp_path() -> PathBuf {
    std::env::var_os("HAE_RHWP")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| root().join("spikes/vendor/rhwp-src/target/release/rhwp"))
}

struct Tmp(PathBuf);
impl Tmp {
    fn new(tag: &str) -> Tmp {
        let p = std::env::temp_dir().join(hae_model::new_id(tag).unwrap());
        fs::create_dir(&p).unwrap();
        Tmp(p)
    }
}
impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn adapter(tweak: impl FnOnce(&mut AdapterConfig)) -> (Arc<RhwpAdapter>, Tmp) {
    let fonts = Tmp::new("hae-exec-fonts-");
    for cand in [
        "/usr/local/share/fonts/n/NanumGothic.ttf",
        "/usr/share/fonts/nanum/NanumGothic.ttf",
    ] {
        if Path::new(cand).is_file() {
            fs::copy(cand, fonts.0.join("NanumGothic.ttf")).unwrap();
            break;
        }
    }
    let mut cfg = AdapterConfig::new(rhwp_path(), &fonts.0);
    tweak(&mut cfg);
    (Arc::new(RhwpAdapter::new(cfg)), fonts)
}

fn executor(doc: &str) -> (SessionExecutor, Tmp, String) {
    let out = Tmp::new("hae-exec-out-");
    let mut exec = SessionExecutor::new(corpus_dir(), &out.0, Limits::default()).unwrap();
    let open = exec.call(
        "document.open",
        json!({ "path": corpus_dir().join(doc) }),
        None,
        false,
    );
    assert!(open.ok, "{:?}", open.diagnostics);
    let id = open.result.unwrap()["documentId"]
        .as_str()
        .unwrap()
        .to_owned();
    (exec, out, id)
}

fn code_of(r: &ResponseEnvelope) -> Code {
    assert!(!r.ok, "expected failure: {:?}", r.result);
    r.diagnostics[0].code
}

fn state(exec: &mut SessionExecutor, out: &Path, tag: &str) -> (Value, String) {
    let ins = exec.call("document.inspect", json!({ "limit": 500 }), None, false);
    let v = ins.result.unwrap();
    let path = out.join(format!("{tag}.hwpx"));
    let ex = exec.call("document.export", json!({ "path": path }), None, false);
    assert!(ex.ok, "{:?}", ex.diagnostics);
    (v, ex.result.unwrap()["sha256"].as_str().unwrap().to_owned())
}

#[test]
fn unconfigured_adapter_keeps_returning_renderer_unavailable() {
    let (mut exec, _o, _id) = executor("ref_empty.hwpx");
    let r = exec.call("document.preview", json!({}), None, false);
    assert_eq!(code_of(&r), Code::RendererUnavailable);
    assert_eq!(
        r.diagnostics[0].reason.as_deref(),
        Some("renderer_not_configured")
    );
}

#[test]
fn preview_returns_png_and_full_metadata() {
    let (mut exec, _o, id) = executor("gyehoek-reference.hwpx");
    let (ad, _f) = adapter(|_| {});
    exec.set_renderer(ad);
    let r = exec.call("document.preview", json!({ "pages": [1] }), None, false);
    assert!(r.ok, "{:?}", r.diagnostics);
    let v = r.result.unwrap();
    assert_eq!(v["status"], "rendered");
    assert_eq!(v["documentId"], id.as_str());
    assert_eq!(v["revision"], 0);
    assert_eq!(v["coverage"], "unknown");
    assert_eq!(v["pageCount"], 19);
    let page = &v["pages"][0];
    assert_eq!(page["page"], 1);
    assert!(page["png"]["base64"]
        .as_str()
        .unwrap()
        .starts_with("iVBORw0KGgo"));
    assert!(page["png"]["width"].as_u64().unwrap() > 500);
    assert!(page.get("svg").is_none(), "svg is opt-in");
    let m = &v["metadata"];
    assert_eq!(m["documentId"], id.as_str());
    assert_eq!(m["contentRevision"], 0);
    assert!(m["layoutRevision"].as_str().unwrap().starts_with("lay-"));
    assert_eq!(m["inputSha256"].as_str().unwrap().len(), 64);
    assert!(m["renderer"]["version"]
        .as_str()
        .unwrap()
        .starts_with("rhwp"));
    assert_eq!(m["renderer"]["sha256"].as_str().unwrap().len(), 64);
    assert_eq!(m["fonts"]["hash"].as_str().unwrap().len(), 64);
    assert!(m["fonts"]["files"].is_array());
    assert!(m["diagnostics"].is_array());
    assert_eq!(m["nativeComparison"], "not_checked");
    assert_eq!(v["staleLayoutSections"], json!([]));
}

#[test]
fn svg_is_opt_in_and_checked() {
    let (mut exec, _o, _id) = executor("ref_empty.hwpx");
    let (ad, _f) = adapter(|_| {});
    exec.set_renderer(ad);
    let r = exec.call(
        "document.preview",
        json!({ "pages": [1], "includeSvg": true, "includePng": false }),
        None,
        false,
    );
    assert!(r.ok, "{:?}", r.diagnostics);
    let v = r.result.unwrap();
    let svg = v["pages"][0]["svg"].as_str().unwrap();
    assert!(svg.contains("<svg"));
    assert!(hae_render_rhwp::svg_safety::check_svg(svg.as_bytes()).is_safe());
    assert!(v["pages"][0].get("png").is_none());
}

#[test]
fn page_limit_is_a_partial_result_with_diagnostic() {
    let (mut exec, _o, _id) = executor("research-school-plan-reference.hwpx");
    let (ad, _f) = adapter(|c| c.limits.max_preview_pages = 5);
    exec.set_renderer(ad);
    let r = exec.call("document.preview", json!({ "pages": [1, 2] }), None, false);
    assert!(r.ok, "{:?}", r.diagnostics);
    assert!(r
        .diagnostics
        .iter()
        .any(|d| d.code == Code::PreviewPageLimit));
    let v = r.result.unwrap();
    assert_eq!(v["status"], "partial");
    assert_eq!(v["pageCount"], 28);
}

#[test]
fn render_failure_never_changes_document_state() {
    let (mut exec, out, _id) = executor("gyehoek-reference.hwpx");
    let before = state(&mut exec, &out.0, "before");
    let rev_before = exec.current_revision();

    // Sandbox cannot be verified → explicit error.
    let (bad, _f) = adapter(|c| c.bwrap_path = "/nonexistent/bwrap".into());
    exec.set_renderer(bad);
    let r = exec.call("document.preview", json!({}), None, false);
    assert_eq!(code_of(&r), Code::RendererUnavailable);
    assert_eq!(
        r.diagnostics[0].reason.as_deref(),
        Some("sandbox_unverified")
    );

    // Renderer binary missing → explicit error.
    let (missing, _f2) = adapter(|c| c.rhwp_path = "/nonexistent/rhwp".into());
    exec.set_renderer(missing);
    let r = exec.call("document.preview", json!({}), None, false);
    assert_eq!(r.diagnostics[0].reason.as_deref(), Some("renderer_missing"));

    // Timeout → explicit error (renderer is a script that never finishes).
    let sleeper = Tmp::new("hae-exec-sleep-");
    let bin = sleeper.0.join("rhwp");
    fs::write(&bin, "#!/usr/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'rhwp vfake'; exit 0; fi\nsleep 30\n").unwrap();
    fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
    let (slow, _f3) = adapter(|c| {
        c.rhwp_path = bin.clone();
        c.limits.renderer_timeout_secs = 1;
    });
    exec.set_renderer(slow);
    let r = exec.call("document.preview", json!({}), None, false);
    assert_eq!(code_of(&r), Code::RendererUnavailable);
    assert_eq!(r.diagnostics[0].reason.as_deref(), Some("timeout"));

    // Same revision, same inspect result, same exported bytes.
    assert_eq!(exec.current_revision(), rev_before);
    let after = state(&mut exec, &out.0, "after");
    assert_eq!(before.0, after.0);
    assert_eq!(before.1, after.1);
}

#[test]
fn preview_after_an_edit_is_tied_to_the_new_revision() {
    let (mut exec, _o, _id) = executor("gyehoek-reference.hwpx");
    let (ad, _f) = adapter(|_| {});
    exec.set_renderer(ad);
    let first = exec.call(
        "document.preview",
        json!({ "includePng": false }),
        Some(0),
        false,
    );
    assert!(first.ok, "{:?}", first.diagnostics);
    let v0 = first.result.unwrap();

    // Find a paragraph with at least two characters of text and replace them.
    let q = exec
        .call(
            "element.query",
            json!({ "kind": "paragraph", "textContains": "행정" }),
            None,
            false,
        )
        .result
        .unwrap();
    let target = q["matches"][0]["nodeId"]
        .as_str()
        .expect("a paragraph")
        .to_owned();
    let edit = exec.call(
        "text.replace",
        json!({ "target": target, "range": { "start": 0, "end": 2 }, "text": "변경" }),
        Some(0),
        false,
    );
    assert!(edit.ok, "{:?}", edit.diagnostics);
    assert_eq!(exec.current_revision(), Some(1));

    // Stale expectedVersion is rejected like other read commands.
    let stale = exec.call("document.preview", json!({}), Some(0), false);
    assert_eq!(code_of(&stale), Code::VersionConflict);

    let second = exec.call(
        "document.preview",
        json!({ "includePng": false }),
        Some(1),
        false,
    );
    assert!(second.ok, "{:?}", second.diagnostics);
    let v1 = second.result.unwrap();
    assert_eq!(v1["revision"], 1);
    assert_eq!(v1["metadata"]["contentRevision"], 1);
    assert_ne!(v0["metadata"]["inputSha256"], v1["metadata"]["inputSha256"]);
    assert_ne!(
        v0["metadata"]["layoutRevision"],
        v1["metadata"]["layoutRevision"]
    );
    // Previewing did not consume the revision counter.
    assert_eq!(exec.current_revision(), Some(1));
    assert!(v1["staleLayoutSections"].as_array().is_some());
}
