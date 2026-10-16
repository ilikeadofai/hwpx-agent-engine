//! Renderer wiring through the `hae` binary: `HAE_RHWP` enables
//! `document.preview` in a script session, and without it the preview fails
//! with `RENDERER_UNAVAILABLE(renderer_not_configured)`.
//!
//! The rendering test needs the rhwp renderer and bubblewrap; when either is
//! missing it prints a note and skips instead of failing.

use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

const HAE: &str = env!("CARGO_BIN_EXE_hae");
const BWRAP: &str = "/usr/bin/bwrap";
const CORPUS: &str = "ilban-gian.hwpx";

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "hae-cli-renderer-{tag}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).expect("create temp dir");
        TempDir(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn corpus_file(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/corpus")
        .join(name)
}

/// The renderer path used by the task, else the in-tree spike build.
fn rhwp_path() -> PathBuf {
    if let Some(p) = std::env::var_os("HAE_RHWP").filter(|v| !v.is_empty()) {
        return p.into();
    }
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spikes/vendor/rhwp-src/target/release/rhwp")
}

/// Build an input root holding the corpus copy and return (temp, input root,
/// output root, script path).
fn prepare(tag: &str) -> (TempDir, PathBuf, PathBuf, PathBuf) {
    let temp = TempDir::new(tag);
    let input = temp.path().join("input");
    let output = temp.path().join("output");
    fs::create_dir_all(&input).expect("input root");
    fs::create_dir_all(&output).expect("output root");
    let doc = input.join(CORPUS);
    fs::copy(corpus_file(CORPUS), &doc).expect("copy corpus doc");
    let script = temp.path().join("script.json");
    fs::write(
        &script,
        serde_json::to_string(&json!([
            {"command": "document.open", "params": {"path": doc}},
            {"command": "document.preview", "params": {"pages": [1]}}
        ]))
        .expect("serialize script"),
    )
    .expect("write script");
    (temp, input, output, script)
}

fn run_script(
    input: &Path,
    output: &Path,
    script: &Path,
    rhwp: Option<&Path>,
) -> (i32, Vec<Value>, String) {
    let fonts = TempDir::new("fonts");
    let mut cmd = Command::new(HAE);
    cmd.env("HAE_FONT_DIR", fonts.path())
        .env("HAE_BWRAP", BWRAP);
    cmd.arg("script")
        .arg("--input-root")
        .arg(input)
        .arg("--output-root")
        .arg(output)
        .arg(script);
    match rhwp {
        Some(path) => {
            cmd.env("HAE_RHWP", path);
        }
        None => {
            cmd.env_remove("HAE_RHWP");
        }
    }
    let out = cmd.output().expect("run hae script");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    let responses: Vec<Value> = serde_json::from_str(stdout.trim()).expect("array of envelopes");
    (out.status.code().unwrap_or(-1), responses, stderr)
}

#[test]
fn preview_renders_page_one_when_hae_rhwp_is_set() {
    let rhwp = rhwp_path();
    if !rhwp.is_file() || !Path::new(BWRAP).is_file() {
        eprintln!(
            "skipping preview_renders_page_one_when_hae_rhwp_is_set: \
             {} or {BWRAP} is missing",
            rhwp.display()
        );
        return;
    }

    let (_temp, input, output, script) = prepare("render");
    let (code, responses, stderr) = run_script(&input, &output, &script, Some(&rhwp));
    assert_eq!(
        code, 0,
        "script should succeed\nresponses: {responses:?}\nstderr: {stderr}"
    );
    assert_eq!(responses[0]["ok"], json!(true), "open: {}", responses[0]);
    let preview = &responses[1];
    assert_eq!(preview["ok"], json!(true), "preview: {preview}\n{stderr}");
    let result = &preview["result"];
    let status = result["status"].as_str().unwrap_or("");
    assert!(
        status == "rendered" || status == "partial",
        "expected a rendered result, got {status}: {result}"
    );
    assert!(
        result["pageCount"].as_u64().unwrap_or(0) >= 1,
        "expected at least one page: {result}"
    );
    let pages = result["pages"].as_array().expect("pages array");
    assert!(
        !pages.is_empty(),
        "expected at least one returned page: {result}"
    );
    assert_eq!(pages[0]["page"], json!(1));
    assert!(
        pages[0]["png"]["base64"]
            .as_str()
            .unwrap_or("")
            .starts_with("iVBORw0KGgo"),
        "page 1 should carry a PNG: {}",
        pages[0]
    );
}

#[test]
fn preview_without_hae_rhwp_reports_renderer_not_configured() {
    let (_temp, input, output, script) = prepare("unconfigured");
    let (code, responses, _stderr) = run_script(&input, &output, &script, None);
    assert_ne!(code, 0, "preview failure must exit non-zero: {responses:?}");
    assert_eq!(responses[0]["ok"], json!(true), "open still succeeds");
    let preview = &responses[1];
    assert_eq!(preview["ok"], json!(false), "preview: {preview}");
    let diag = &preview["diagnostics"][0];
    assert_eq!(diag["code"], json!("RENDERER_UNAVAILABLE"), "{preview}");
    assert_eq!(
        diag["reason"],
        json!("renderer_not_configured"),
        "{preview}"
    );
}
