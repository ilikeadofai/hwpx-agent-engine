#![cfg(unix)]

//! Renderer wiring through the `hae-mcp` server: discovery (or `HAE_RHWP`)
//! enables the `document_preview` tool, and with discovery disabled and no
//! renderer the preview fails with
//! `RENDERER_UNAVAILABLE(renderer_not_configured)`.
//!
//! The rendering test needs the rhwp renderer and bubblewrap; when either is
//! missing it prints a note and skips instead of failing.

use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

const HAE_MCP: &str = env!("CARGO_BIN_EXE_hae-mcp");
const BWRAP: &str = "/usr/bin/bwrap";
const CORPUS: &str = "ilban-gian.hwpx";

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "hae-mcp-renderer-{tag}-{}-{}",
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

struct Client {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl Client {
    fn start(dir: &Path, rhwp: Option<&Path>) -> Client {
        Client::start_with(dir, rhwp, None)
    }

    /// `discovery` sets `HAE_RENDERER_DISCOVERY` for the server process, so a
    /// test can pin the search off and keep the unconfigured path deterministic.
    fn start_with(dir: &Path, rhwp: Option<&Path>, discovery: Option<&str>) -> Client {
        let fonts = dir.join("test-fonts");
        fs::create_dir_all(&fonts).unwrap();
        let mut cmd = Command::new(HAE_MCP);
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
        match discovery {
            Some(v) => {
                cmd.env("HAE_RENDERER_DISCOVERY", v);
            }
            None => {
                cmd.env_remove("HAE_RENDERER_DISCOVERY");
            }
        }
        let mut child = cmd.spawn().expect("spawn hae-mcp");
        let stdin = child.stdin.take();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let mut client = Client {
            child,
            stdin,
            stdout,
            next_id: 1,
        };
        let init = client.rpc(
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "hae-test", "version": "0" }
            }),
        );
        assert_eq!(init["result"]["protocolVersion"], "2025-06-18", "{init}");
        client.notify("notifications/initialized", json!({}));
        client
    }

    fn send(&mut self, message: Value) -> Value {
        let stdin = self.stdin.as_mut().unwrap();
        stdin.write_all(message.to_string().as_bytes()).unwrap();
        stdin.write_all(b"\n").unwrap();
        stdin.flush().unwrap();
        let mut line = String::new();
        self.stdout.read_line(&mut line).unwrap();
        assert!(!line.is_empty(), "server closed stdout");
        assert!(
            line.trim_end().len() <= 4 * 1024 * 1024,
            "wire response exceeds 4 MiB"
        );
        serde_json::from_str(line.trim()).expect("one JSON value per line")
    }

    fn rpc(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let reply = self.send(json!({
            "jsonrpc": "2.0", "id": id, "method": method, "params": params
        }));
        assert_eq!(reply["id"], json!(id), "reply id must echo: {reply}");
        reply
    }

    fn notify(&mut self, method: &str, params: Value) {
        // Notifications intentionally have no reply; waiting for one deadlocks
        // the test before any document command reaches the server.
        let stdin = self.stdin.as_mut().unwrap();
        writeln!(
            stdin,
            "{}",
            json!({ "jsonrpc": "2.0", "method": method, "params": params })
        )
        .unwrap();
        stdin.flush().unwrap();
    }

    /// Call a tool and return (response envelope, isError).
    fn call_tool(&mut self, tool: &str, arguments: Value) -> (Value, bool) {
        let reply = self.rpc(
            "tools/call",
            json!({ "name": tool, "arguments": arguments }),
        );
        let result = &reply["result"];
        assert!(result.is_object(), "tools/call failed: {reply}");
        let text = result["content"][0]["text"].as_str().expect("text content");
        let envelope: Value = serde_json::from_str(text).expect("envelope JSON");
        (envelope, result["isError"].as_bool().expect("isError"))
    }

    fn finish(mut self) -> (i32, String) {
        drop(self.stdin.take());
        let status = self.child.wait().unwrap();
        let mut stderr = String::new();
        std::io::Read::read_to_string(self.child.stderr.as_mut().unwrap(), &mut stderr).unwrap();
        (status.code().unwrap_or(-1), stderr)
    }
}

#[test]
fn escaped_svg_over_wire_budget_is_a_retrievable_artifact() {
    use std::os::unix::fs::PermissionsExt;
    assert!(Path::new(BWRAP).is_file(), "real bwrap is required");
    let temp = TempDir::new("artifact");
    let target = temp.path().join(CORPUS);
    fs::copy(corpus_file(CORPUS), &target).unwrap();
    let renderer = temp.path().join("rhwp-fake");
    fs::write(&renderer,r#"#!/usr/bin/sh
if [ "$1" = --version ]; then echo 'rhwp artifact-test'; exit; fi
if [ "$1" = dump-pages ]; then echo '{"pages":[{}]}'; exit; fi
printf '<svg xmlns="http://www.w3.org/2000/svg"><!--' > /out/p.svg
head -c 1100000 /dev/zero | tr '\000' '"' >> /out/p.svg
printf -- '--></svg>' >> /out/p.svg
N=$(wc -c < /out/p.svg)
echo "{\"format\":\"svg\",\"pageCount\":1,\"pages\":[{\"page\":0,\"path\":\"/out/p.svg\",\"bytes\":$N}]}"
"#).unwrap();
    fs::set_permissions(&renderer, fs::Permissions::from_mode(0o700)).unwrap();
    let mut client = Client::start(temp.path(), Some(&renderer));
    assert!(
        client.call_tool("document_open", json!({"path":target})).0["ok"]
            .as_bool()
            .unwrap()
    );
    let (response, is_error) = client.call_tool(
        "document_preview",
        json!({"includeSvg":true,"includePng":false}),
    );
    assert!(!is_error, "{response}");
    assert_eq!(response["result"]["status"], "rendered");
    assert_eq!(response["result"]["revision"], 0);
    let artifact = &response["result"]["pages"][0]["svgArtifact"];
    let path = Path::new(artifact["path"].as_str().expect("artifact path"));
    assert_eq!(path.parent(), Some(temp.path()));
    let bytes = fs::read(path).unwrap();
    assert_eq!(bytes.iter().filter(|&&b| b == b'"').count(), 1_100_002);
    assert_eq!(bytes.len() as u64, artifact["bytes"].as_u64().unwrap());
    assert_eq!(client.finish().0, 0);
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
    let target = temp.path().join(CORPUS);
    fs::copy(corpus_file(CORPUS), &target).expect("copy corpus doc");
    let mut client = Client::start(temp.path(), Some(&rhwp));

    let (open, open_err) = client.call_tool("document_open", json!({ "path": target }));
    assert!(!open_err, "open failed: {open}");
    assert_eq!(open["ok"], json!(true), "open: {open}");
    let doc = open["result"]["documentId"].as_str().unwrap().to_string();

    let (preview, is_error) = client.call_tool(
        "document_preview",
        json!({ "documentId": doc, "pages": [1] }),
    );
    assert!(!is_error, "preview isError: {preview}");
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
    let (_code, stderr) = client.finish();
    assert!(stderr.contains("hae-mcp:"), "logs go to stderr: {stderr}");
}

#[test]
fn preview_without_hae_rhwp_reports_renderer_not_configured() {
    let temp = TempDir::new("unconfigured");
    let target = temp.path().join(CORPUS);
    fs::copy(corpus_file(CORPUS), &target).expect("copy corpus doc");
    // Discovery off keeps this deterministic: only explicit variables count.
    let mut client = Client::start_with(temp.path(), None, Some("0"));

    let (open, _) = client.call_tool("document_open", json!({ "path": target }));
    assert_eq!(open["ok"], json!(true), "open: {open}");
    let doc = open["result"]["documentId"].as_str().unwrap().to_string();

    let (preview, is_error) = client.call_tool(
        "document_preview",
        json!({ "documentId": doc, "pages": [1] }),
    );
    assert!(is_error, "preview should be an error: {preview}");
    assert_eq!(preview["ok"], json!(false), "preview: {preview}");
    let diag = &preview["diagnostics"][0];
    assert_eq!(diag["code"], json!("RENDERER_UNAVAILABLE"), "{preview}");
    assert_eq!(
        diag["reason"],
        json!("renderer_not_configured"),
        "{preview}"
    );
    let message = diag["message"].as_str().unwrap_or_default();
    assert!(message.contains("HAE_RHWP"), "{message}");
    assert!(message.contains("HAE_RENDERER_DISCOVERY"), "{message}");
    assert_eq!(client.finish().0, 0);
}
