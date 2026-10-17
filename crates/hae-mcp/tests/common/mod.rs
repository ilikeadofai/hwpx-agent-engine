//! Shared helpers for the process-level interface tests (plan 2-2, 2-3).

#![allow(dead_code)]

use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

pub const HAE_MCP: &str = env!("CARGO_BIN_EXE_hae-mcp");

pub struct TempDir(pub PathBuf);

impl TempDir {
    pub fn new(tag: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "hae-mcp-{tag}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    /// Copy a corpus document into this directory and return the new path.
    pub fn with_corpus(&self, name: &str) -> PathBuf {
        let target = self.0.join(name);
        fs::copy(corpus_file(name), &target).unwrap();
        target
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub fn corpus_file(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/corpus")
        .join(name)
}

/// Path of another workspace package's binary. `CARGO_BIN_EXE_*` only exists
/// for this package's own binaries, so look beside the test executable and
/// build the package once if it is missing.
pub fn sibling_bin(package: &str, bin: &str) -> PathBuf {
    static BUILT: OnceLock<()> = OnceLock::new();
    let exe = std::env::current_exe().unwrap();
    let dir = exe.parent().unwrap().parent().unwrap().to_path_buf();
    let path = dir.join(bin);
    BUILT.get_or_init(|| {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
        let mut cmd = Command::new(cargo);
        cmd.args(["build", "-p", "hae-cli", "-p", "hae-jsonl"])
            .current_dir(env!("CARGO_MANIFEST_DIR"));
        if dir.ends_with("release") {
            cmd.arg("--release");
        }
        let status = cmd.status().expect("run cargo build");
        assert!(status.success(), "cargo build of sibling binaries failed");
    });
    assert!(
        path.exists(),
        "binary {bin} of package {package} not found at {}",
        path.display()
    );
    path
}

/// A server speaking line-delimited JSON on stdio.
struct Pipes {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
}

impl Pipes {
    fn spawn(bin: &Path, dir: &Path) -> Pipes {
        let mut child = Command::new(bin)
            .arg("--input-root")
            .arg(dir)
            .arg("--output-root")
            .arg(dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn server");
        let stdin = child.stdin.take();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Pipes {
            child,
            stdin,
            stdout,
        }
    }

    fn write_line(&mut self, bytes: &[u8]) {
        let stdin = self.stdin.as_mut().unwrap();
        stdin.write_all(bytes).unwrap();
        stdin.write_all(b"\n").unwrap();
        stdin.flush().unwrap();
    }

    fn read_json(&mut self) -> Value {
        let mut line = String::new();
        self.stdout.read_line(&mut line).unwrap();
        assert!(!line.is_empty(), "server closed stdout");
        serde_json::from_str(line.trim()).expect("one JSON value per line")
    }

    fn finish(&mut self) -> (i32, String) {
        drop(self.stdin.take());
        let status = self.child.wait().unwrap();
        let mut stderr = String::new();
        self.child
            .stderr
            .as_mut()
            .unwrap()
            .read_to_string(&mut stderr)
            .unwrap();
        (status.code().unwrap_or(-1), stderr)
    }
}

// ---------------------------------------------------------------------------
// hae-mcp
// ---------------------------------------------------------------------------

pub struct McpClient {
    pipes: Pipes,
    next_id: u64,
}

impl McpClient {
    /// Spawn the server and run the `initialize` handshake.
    pub fn start(dir: &Path) -> McpClient {
        let mut client = Self::start_raw(dir);
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

    pub fn start_raw(dir: &Path) -> McpClient {
        McpClient {
            pipes: Pipes::spawn(Path::new(HAE_MCP), dir),
            next_id: 1,
        }
    }

    pub fn send_raw(&mut self, bytes: &[u8]) -> Value {
        self.pipes.write_line(bytes);
        self.pipes.read_json()
    }

    pub fn rpc(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let reply = self.send_raw(
            json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
                .to_string()
                .as_bytes(),
        );
        assert_eq!(
            reply["id"],
            json!(id),
            "reply id must echo the request: {reply}"
        );
        assert_eq!(reply["jsonrpc"], "2.0");
        reply
    }

    pub fn notify(&mut self, method: &str, params: Value) {
        self.pipes.write_line(
            json!({ "jsonrpc": "2.0", "method": method, "params": params })
                .to_string()
                .as_bytes(),
        );
    }

    /// Call a tool; returns the parsed response envelope and the `isError` flag.
    pub fn call_tool(&mut self, tool: &str, arguments: Value) -> (Value, bool) {
        let reply = self.rpc(
            "tools/call",
            json!({ "name": tool, "arguments": arguments }),
        );
        let result = &reply["result"];
        assert!(
            result.is_object(),
            "tools/call failed at protocol level: {reply}"
        );
        let text = result["content"][0]["text"].as_str().expect("text content");
        let envelope: Value = serde_json::from_str(text).expect("envelope JSON");
        assert_eq!(
            result["structuredContent"], envelope,
            "text and structured result must agree"
        );
        (envelope, result["isError"].as_bool().expect("isError"))
    }

    pub fn finish(mut self) -> (i32, String) {
        self.pipes.finish()
    }
}

// ---------------------------------------------------------------------------
// hae-jsonl
// ---------------------------------------------------------------------------

pub struct JsonlClient {
    pipes: Pipes,
    pub session_id: String,
    pub writer_id: String,
}

impl JsonlClient {
    pub fn start(dir: &Path) -> JsonlClient {
        let bin = sibling_bin("hae-jsonl", "hae-jsonl");
        let mut pipes = Pipes::spawn(&bin, dir);
        let hello = pipes.read_json();
        assert_eq!(hello["type"], "hello");
        assert_eq!(hello["schemaVersion"], 1);
        JsonlClient {
            session_id: hello["sessionId"].as_str().unwrap().to_string(),
            writer_id: hello["writerId"].as_str().unwrap().to_string(),
            pipes,
        }
    }

    pub fn send_raw(&mut self, bytes: &[u8]) -> Value {
        self.pipes.write_line(bytes);
        self.pipes.read_json()
    }

    pub fn request(
        &mut self,
        seq: Option<u64>,
        document_id: Option<&str>,
        expected: Option<u64>,
        command: &str,
        params: Value,
        dry_run: bool,
    ) -> Value {
        let mut req = json!({
            "schemaVersion": 1,
            "sessionId": self.session_id,
            "writerId": self.writer_id,
            "command": command,
            "params": params,
            "dryRun": dry_run,
        });
        if let Some(seq) = seq {
            req["requestSequence"] = json!(seq);
        }
        if let Some(id) = document_id {
            req["documentId"] = json!(id);
        }
        if let Some(v) = expected {
            req["expectedVersion"] = json!(v);
        }
        self.send_raw(req.to_string().as_bytes())
    }

    pub fn finish(mut self) -> (i32, String) {
        self.pipes.finish()
    }
}

pub fn first_code(envelope: &Value) -> &str {
    envelope["diagnostics"][0]["code"].as_str().unwrap_or("")
}
