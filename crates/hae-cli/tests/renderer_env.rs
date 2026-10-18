#![cfg(unix)]

//! Renderer wiring through the `hae` binary: `HAE_RHWP` enables
//! `document.preview` in a script session, the documented discovery order
//! enables it without any variable, and with discovery disabled and no
//! renderer the preview fails with
//! `RENDERER_UNAVAILABLE(renderer_not_configured)`.
//!
//! The rendering tests need the rhwp renderer and bubblewrap; when either is
//! missing they print a note and skip instead of failing.

use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::PermissionsExt;
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
    run_script_with(input, output, script, rhwp, None)
}

/// `run_script` with control over the discovery switch, so the search can be
/// pinned off for deterministic unconfigured-path assertions.
fn run_script_with(
    input: &Path,
    output: &Path,
    script: &Path,
    rhwp: Option<&Path>,
    discovery: Option<&str>,
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
    if let Some(v) = discovery {
        cmd.env("HAE_RENDERER_DISCOVERY", v);
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
    // Discovery off: only explicit variables are used, so an unset HAE_RHWP
    // deterministically means "unconfigured" even when a development-tree
    // renderer happens to be built next to the repository.
    let (code, responses, stderr) = run_script_with(&input, &output, &script, None, Some("0"));
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
    // The message says what to do, not only that something is missing.
    let message = diag["message"].as_str().unwrap_or_default();
    assert!(message.contains("HAE_RHWP"), "{message}");
    assert!(message.contains("HAE_RENDERER_DISCOVERY"), "{message}");
    // With the search off, no candidate was examined.
    assert_eq!(
        diag["detail"]["searched"].as_array().map(Vec::len),
        Some(0),
        "{preview}"
    );
    assert_eq!(diag["detail"]["tool"], json!("rhwp"), "{preview}");
    // And the server logged the fix line on stderr, never on stdout.
    assert!(stderr.contains("HAE_RHWP"), "{stderr}");
}

/// Without `HAE_RHWP`, discovery finds the renderer in the in-tree location
/// below the working directory and actually renders a page inside the sandbox.
/// (`PATH` and sibling rules are covered by the discovery unit tests, which can
/// inject the search inputs; here the working directory is the only rule a test
/// can pin deterministically.)
#[test]
fn discovery_finds_an_in_tree_renderer_without_hae_rhwp() {
    if !Path::new(BWRAP).is_file() {
        eprintln!("skipping discovery_finds_an_in_tree_renderer_without_hae_rhwp: {BWRAP} missing");
        return;
    }
    let (temp, input, output, script) = prepare("discovery-devtree");
    // SVG only: the PNG worker beside the `hae` binary may not be built when
    // this test file runs on its own.
    fs::write(
        &script,
        serde_json::to_string(&json!([
            {"command": "document.open", "params": {"path": input.join(CORPUS)}},
            {"command": "document.preview",
             "params": {"pages": [1], "includePng": false, "includeSvg": true}}
        ]))
        .expect("serialize script"),
    )
    .expect("write script");
    let work = temp.path().join("work");
    let dev = work.join("spikes/vendor/rhwp-src/target/release");
    fs::create_dir_all(&dev).unwrap();
    let fake = dev.join("rhwp");
    fs::write(
        &fake,
        "#!/usr/bin/sh\n\
         if [ \"$1\" = \"--version\" ]; then echo 'rhwp vdiscovery'; exit 0; fi\n\
         if [ \"$1\" = dump-pages ]; then echo '{\"pages\":[{}]}'; exit 0; fi\n\
         printf '<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"10\" height=\"10\"/>' > /out/p.svg\n\
         N=$(wc -c < /out/p.svg)\n\
         echo \"{\\\"format\\\":\\\"svg\\\",\\\"pageCount\\\":1,\\\"pages\\\":[{\\\"page\\\":0,\\\"path\\\":\\\"/out/p.svg\\\",\\\"bytes\\\":$N}]}\"\n",
    )
    .unwrap();
    fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).unwrap();
    let mut cmd = Command::new(HAE);
    let fonts = TempDir::new("fonts");
    cmd.env("HAE_FONT_DIR", fonts.path())
        .env("HAE_BWRAP", BWRAP)
        .env_remove("HAE_RHWP")
        .current_dir(&work)
        .arg("script")
        .arg("--input-root")
        .arg(&input)
        .arg("--output-root")
        .arg(&output)
        .arg(&script);
    let out = cmd.output().expect("run hae script");
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    let responses: Vec<Value> =
        serde_json::from_str(String::from_utf8_lossy(&out.stdout).trim()).expect("envelopes");
    assert_eq!(
        out.status.code(),
        Some(0),
        "renderer found by discovery\nresponses: {responses:?}\nstderr: {stderr}"
    );
    let preview = &responses[1];
    assert_eq!(preview["ok"], json!(true), "preview: {preview}\n{stderr}");
    assert_eq!(preview["result"]["status"], json!("rendered"), "{preview}");
    assert_eq!(preview["result"]["pageCount"], json!(1));
    // The discovered copy is the fake, not any other build of the tree.
    assert_eq!(
        preview["result"]["metadata"]["renderer"]["version"],
        json!("rhwp vdiscovery")
    );
    assert!(
        preview["result"]["pages"][0]["svg"]
            .as_str()
            .unwrap_or_default()
            .starts_with("<svg"),
        "the discovered renderer must have produced the page: {preview}"
    );
    assert!(
        !stderr.contains("preview renderer not configured"),
        "discovery must have configured the renderer: {stderr}"
    );
}

/// Without `HAE_FONT_DIR`, an over-limit first font directory is skipped and
/// the next usable one is used and reported in the preview metadata. (The
/// per-user directories are injected through `HOME`, so the result does not
/// depend on the fonts installed on the machine.)
#[test]
fn unset_font_dir_skips_an_oversized_candidate_and_reports_the_choice() {
    if !Path::new(BWRAP).is_file() {
        eprintln!("skipping unset_font_dir_skips_an_oversized_candidate: {BWRAP} missing");
        return;
    }
    let (temp, input, output, script) = prepare("font-fallback");
    fs::write(
        &script,
        serde_json::to_string(&json!([
            {"command": "document.open", "params": {"path": input.join(CORPUS)}},
            {"command": "document.preview",
             "params": {"pages": [1], "includePng": false, "includeSvg": true}}
        ]))
        .expect("serialize script"),
    )
    .expect("write script");
    let home = temp.path().join("home");
    let big = home.join(".local/share/fonts");
    fs::create_dir_all(&big).unwrap();
    fs::File::create(big.join("Huge.ttf"))
        .unwrap()
        .set_len(33 * 1024 * 1024)
        .unwrap();
    let good = home.join(".fonts");
    fs::create_dir_all(&good).unwrap();
    fs::write(good.join("Small.ttf"), b"scan only counts bytes").unwrap();
    let rhwp = temp.path().join("rhwp");
    fs::write(
        &rhwp,
        "#!/usr/bin/sh\n\
         if [ \"$1\" = \"--version\" ]; then echo 'rhwp vfontfallback'; exit 0; fi\n\
         if [ \"$1\" = dump-pages ]; then echo '{\"pages\":[{}]}'; exit 0; fi\n\
         printf '<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"10\" height=\"10\"/>' > /out/p.svg\n\
         N=$(wc -c < /out/p.svg)\n\
         echo \"{\\\"format\\\":\\\"svg\\\",\\\"pageCount\\\":1,\\\"pages\\\":[{\\\"page\\\":0,\\\"path\\\":\\\"/out/p.svg\\\",\\\"bytes\\\":$N}]}\"\n",
    )
    .unwrap();
    fs::set_permissions(&rhwp, fs::Permissions::from_mode(0o755)).unwrap();
    let out = Command::new(HAE)
        .env("HOME", &home)
        .env("HAE_RHWP", &rhwp)
        .env("HAE_BWRAP", BWRAP)
        .env_remove("HAE_FONT_DIR")
        .env_remove("HAE_RENDERER_DISCOVERY")
        .arg("script")
        .arg("--input-root")
        .arg(&input)
        .arg("--output-root")
        .arg(&output)
        .arg(&script)
        .output()
        .expect("run hae script");
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    let responses: Vec<Value> =
        serde_json::from_str(String::from_utf8_lossy(&out.stdout).trim()).expect("envelopes");
    let preview = &responses[1];
    assert_eq!(preview["ok"], json!(true), "preview: {preview}\n{stderr}");
    let fonts = &preview["result"]["metadata"]["fonts"];
    assert_eq!(fonts["directory"], json!(good), "{fonts}");
    assert_eq!(fonts["source"], json!("userFontDir"), "{fonts}");
    assert_eq!(
        fonts["searched"][1]["verdict"],
        json!("unusable"),
        "{fonts}"
    );
    assert_eq!(fonts["hash"].as_str().map(str::len), Some(64), "{fonts}");
}
