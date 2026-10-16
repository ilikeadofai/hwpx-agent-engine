//! Renderer wiring through the `hae-jsonl` server: `HAE_RHWP` enables
//! `document.preview` over the transport, and without it the preview fails
//! with `RENDERER_UNAVAILABLE(renderer_not_configured)`.
//!
//! The rendering test needs the rhwp renderer and bubblewrap; when either is
//! missing it prints a note and skips instead of failing.

use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

const HAE_JSONL: &str = env!("CARGO_BIN_EXE_hae-jsonl");
const BWRAP: &str = "/usr/bin/bwrap";
const CORPUS: &str = "ilban-gian.hwpx";

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "hae-jsonl-renderer-{tag}-{}-{}",
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

fn rhwp_path() -> PathBuf {
    if let Some(p) = std::env::var_os("HAE_RHWP").filter(|v| !v.is_empty()) {
        return p.into();
    }
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spikes/vendor/rhwp-src/target/release/rhwp")
}

struct Server {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    session_id: String,
    writer_id: String,
}

impl Server {
    fn start(dir: &Path, rhwp: Option<&Path>) -> Server {
        let fonts = dir.join("test-fonts");
        fs::create_dir_all(&fonts).unwrap();
        let mut cmd = Command::new(HAE_JSONL);
        cmd.env("HAE_FONT_DIR", &fonts).env("HAE_BWRAP", BWRAP);
        cmd.arg("--input-root")
            .arg(dir)
            .arg("--output-root")
            .arg(dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        match rhwp {
            Some(path) => {
                cmd.env("HAE_RHWP", path);
            }
            None => {
                cmd.env_remove("HAE_RHWP");
            }
        }
        let mut child = cmd.spawn().expect("spawn hae-jsonl");
        let stdin = child.stdin.take();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        stdout.read_line(&mut line).expect("hello line");
        let hello: Value = serde_json::from_str(line.trim()).expect("hello is JSON");
        assert_eq!(hello["type"], "hello");
        Server {
            child,
            stdin,
            stdout,
            session_id: hello["sessionId"].as_str().unwrap().to_string(),
            writer_id: hello["writerId"].as_str().unwrap().to_string(),
        }
    }

    fn request(&mut self, command: &str, params: Value, document_id: Option<&str>) -> Value {
        let mut req = json!({
            "schemaVersion": 1,
            "sessionId": self.session_id,
            "writerId": self.writer_id,
            "command": command,
            "params": params,
            "dryRun": false,
        });
        if let Some(id) = document_id {
            req["documentId"] = json!(id);
        }
        let stdin = self.stdin.as_mut().unwrap();
        stdin.write_all(req.to_string().as_bytes()).unwrap();
        stdin.write_all(b"\n").unwrap();
        stdin.flush().unwrap();
        let mut line = String::new();
        self.stdout.read_line(&mut line).unwrap();
        assert!(!line.is_empty(), "server closed stdout");
        serde_json::from_str(line.trim()).expect("one JSON response per line")
    }

    fn finish(mut self) -> (i32, String) {
        drop(self.stdin.take());
        let status = self.child.wait().unwrap();
        let mut stderr = String::new();
        std::io::Read::read_to_string(self.child.stderr.as_mut().unwrap(), &mut stderr).unwrap();
        (status.code().unwrap_or(-1), stderr)
    }
}

/// Open the corpus document and return its documentId.
fn open_corpus(server: &mut Server, dir: &Path) -> String {
    let target = dir.join(CORPUS);
    fs::copy(corpus_file(CORPUS), &target).expect("copy corpus doc");
    let open = server.request("document.open", json!({ "path": target }), None);
    assert_eq!(open["ok"], json!(true), "open: {open}");
    open["result"]["documentId"].as_str().unwrap().to_string()
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

    let temp = TempDir::new("render");
    let mut server = Server::start(temp.path(), Some(&rhwp));
    let doc = open_corpus(&mut server, temp.path());
    let preview = server.request("document.preview", json!({ "pages": [1] }), Some(&doc));
    assert_eq!(preview["ok"], json!(true), "preview: {preview}");
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
    assert!(!pages.is_empty(), "expected a returned page: {result}");
    assert_eq!(pages[0]["page"], json!(1));
    let (_code, stderr) = server.finish();
    assert!(stderr.contains("hae-jsonl:"), "logs go to stderr: {stderr}");
}

#[test]
fn preview_without_hae_rhwp_reports_renderer_not_configured() {
    let temp = TempDir::new("unconfigured");
    let mut server = Server::start(temp.path(), None);
    let doc = open_corpus(&mut server, temp.path());
    let preview = server.request("document.preview", json!({ "pages": [1] }), Some(&doc));
    assert_eq!(preview["ok"], json!(false), "preview: {preview}");
    let diag = &preview["diagnostics"][0];
    assert_eq!(diag["code"], json!("RENDERER_UNAVAILABLE"), "{preview}");
    assert_eq!(
        diag["reason"],
        json!("renderer_not_configured"),
        "{preview}"
    );
    assert_eq!(server.finish().0, 0);
}
