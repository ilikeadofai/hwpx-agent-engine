//! Process-level tests for `hae-jsonl` (plan 2-1): hello line, malformed and
//! oversize input, and the writer/sequence/version rules over the transport.

use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

const HAE_JSONL: &str = env!("CARGO_BIN_EXE_hae-jsonl");

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "hae-jsonl-{tag}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        TempDir(path)
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

struct Server {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    session_id: String,
    writer_id: String,
}

impl Server {
    fn start(dir: &Path) -> Server {
        let mut child = Command::new(HAE_JSONL)
            .arg("--input-root")
            .arg(dir)
            .arg("--output-root")
            .arg(dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn hae-jsonl");
        let stdin = child.stdin.take();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        let hello: Value = serde_json::from_str(line.trim()).expect("hello is JSON");
        assert_eq!(hello["type"], "hello");
        assert_eq!(hello["schemaVersion"], 1);
        Server {
            child,
            stdin,
            stdout,
            session_id: hello["sessionId"].as_str().unwrap().to_string(),
            writer_id: hello["writerId"].as_str().unwrap().to_string(),
        }
    }

    fn send_raw(&mut self, bytes: &[u8]) -> Value {
        let stdin = self.stdin.as_mut().unwrap();
        stdin.write_all(bytes).unwrap();
        stdin.write_all(b"\n").unwrap();
        stdin.flush().unwrap();
        let mut line = String::new();
        self.stdout.read_line(&mut line).unwrap();
        assert!(!line.is_empty(), "server closed stdout");
        serde_json::from_str(line.trim()).expect("response is one JSON line")
    }

    fn request(
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

    fn finish(mut self) -> (i32, String) {
        drop(self.stdin.take());
        let status = self.child.wait().unwrap();
        let mut stderr = String::new();
        std::io::Read::read_to_string(self.child.stderr.as_mut().unwrap(), &mut stderr).unwrap();
        (status.code().unwrap(), stderr)
    }
}

fn code(response: &Value) -> &str {
    response["diagnostics"][0]["code"].as_str().unwrap_or("")
}

fn open_bodojaryo(server: &mut Server, dir: &Path) -> (String, String) {
    let target = dir.join("bodojaryo.hwpx");
    fs::copy(corpus_file("bodojaryo-reference.hwpx"), &target).unwrap();
    let open = server.request(
        None,
        None,
        None,
        "document.open",
        json!({ "path": target }),
        false,
    );
    assert_eq!(open["ok"], true, "{open}");
    let doc = open["result"]["documentId"].as_str().unwrap().to_string();
    let query = server.request(
        None,
        Some(&doc),
        None,
        "element.query",
        json!({ "kind": "paragraph", "textContains": "보도자료" }),
        false,
    );
    let para = query["result"]["matches"][0]["nodeId"]
        .as_str()
        .unwrap()
        .to_string();
    (doc, para)
}

#[test]
fn hello_malformed_json_and_eof_exit_zero_with_logs_on_stderr_only() {
    let t = TempDir::new("basic");
    let mut server = Server::start(&t.0);

    let bad = server.send_raw(b"{not json");
    assert_eq!(bad["ok"], false);
    assert_eq!(code(&bad), "INVALID_REQUEST");
    assert_eq!(bad["sessionId"], json!(server.session_id));

    // Unknown envelope fields are rejected, not ignored.
    let extra = server.send_raw(
        json!({"schemaVersion":1,"sessionId":server.session_id,"command":"command.list","bogus":1})
            .to_string()
            .as_bytes(),
    );
    assert_eq!(code(&extra), "INVALID_REQUEST");
    assert_eq!(extra["command"], "command.list");

    // The server keeps serving after errors.
    let list = server.request(None, None, None, "command.list", json!({}), false);
    assert_eq!(list["ok"], true);
    assert_eq!(
        list["result"]["commands"].as_array().unwrap().len(),
        hae_exec::command_list().len()
    );

    let (exit, stderr) = server.finish();
    assert_eq!(exit, 0);
    assert!(stderr.contains("hae-jsonl:"), "logs go to stderr: {stderr}");
}

#[test]
fn oversize_request_is_refused_without_consuming_a_sequence_number() {
    let t = TempDir::new("oversize");
    let mut server = Server::start(&t.0);
    let (doc, para) = open_bodojaryo(&mut server, &t.0);

    let huge = "x".repeat(1024 * 1024 + 1);
    let refused = server.send_raw(huge.as_bytes());
    assert_eq!(refused["ok"], false);
    assert_eq!(code(&refused), "RESOURCE_LIMIT");

    // A valid mutation with requestSequence 1 still succeeds: nothing was consumed.
    let edit = server.request(
        Some(1),
        Some(&doc),
        Some(0),
        "text.replace",
        json!({ "target": para, "range": {"start": 0, "end": 2}, "text": "공지" }),
        false,
    );
    assert_eq!(edit["ok"], true, "{edit}");
    assert_eq!(edit["result"]["outcome"], "applied");
    assert_eq!(server.finish().0, 0);
}

#[test]
fn writer_sequence_and_version_rules_hold_over_the_transport() {
    let t = TempDir::new("rules");
    let mut server = Server::start(&t.0);
    let (doc, para) = open_bodojaryo(&mut server, &t.0);
    let replace =
        |text: &str| json!({ "target": para, "range": {"start": 0, "end": 2}, "text": text });

    let first = server.request(
        Some(1),
        Some(&doc),
        Some(0),
        "text.replace",
        replace("공지"),
        false,
    );
    assert_eq!(first["ok"], true, "{first}");
    assert_eq!(first["replayed"], false);
    assert_eq!(first["result"]["afterVersion"], 1);

    // Same number, same content: stored response, marked replay, not applied again.
    let replay = server.request(
        Some(1),
        Some(&doc),
        Some(0),
        "text.replace",
        replace("공지"),
        false,
    );
    assert_eq!(replay["ok"], true);
    assert_eq!(replay["replayed"], true);
    assert_eq!(replay["result"]["afterVersion"], 1);

    // Same number, different content.
    let conflict = server.request(
        Some(1),
        Some(&doc),
        Some(0),
        "text.replace",
        replace("안내"),
        false,
    );
    assert_eq!(code(&conflict), "REQUEST_ID_CONFLICT");

    // Stale expectedVersion: VERSION_CONFLICT, and the number is consumed.
    let stale = server.request(
        Some(2),
        Some(&doc),
        Some(0),
        "text.replace",
        replace("안내"),
        false,
    );
    assert_eq!(code(&stale), "VERSION_CONFLICT");

    // Skipped number (3 expected).
    let skipped = server.request(
        Some(4),
        Some(&doc),
        Some(1),
        "text.replace",
        replace("안내"),
        false,
    );
    assert_eq!(code(&skipped), "REQUEST_OUT_OF_ORDER");

    // Number 3 is still next because the skipped request did not consume anything.
    let third = server.request(
        Some(3),
        Some(&doc),
        Some(1),
        "text.replace",
        replace("안내"),
        false,
    );
    assert_eq!(third["ok"], true, "{third}");
    assert_eq!(third["result"]["afterVersion"], 2);

    // An already passed number with different content.
    let old = server.request(
        Some(1),
        Some(&doc),
        Some(2),
        "text.replace",
        replace("공지"),
        false,
    );
    assert_eq!(code(&old), "REPLAY_TOO_OLD");

    // dryRun uses no number and leaves the revision alone.
    let dry = server.request(
        None,
        Some(&doc),
        Some(2),
        "text.replace",
        replace("미리"),
        true,
    );
    assert_eq!(dry["ok"], true, "{dry}");
    let next = server.request(
        Some(4),
        Some(&doc),
        Some(2),
        "text.replace",
        replace("미리"),
        false,
    );
    assert_eq!(next["ok"], true, "{next}");

    // A writerId this connection never issued.
    let unknown = server.send_raw(
        json!({
            "schemaVersion": 1, "sessionId": server.session_id, "writerId": "wrt-not-issued",
            "requestSequence": 1, "documentId": doc, "expectedVersion": 3,
            "command": "text.replace", "params": replace("끝")
        })
        .to_string()
        .as_bytes(),
    );
    assert_eq!(code(&unknown), "UNKNOWN_WRITER");

    // Wrong session.
    let wrong_session = server.send_raw(
        json!({"schemaVersion":1,"sessionId":"ses-other","command":"command.list"})
            .to_string()
            .as_bytes(),
    );
    assert_eq!(code(&wrong_session), "SESSION_MISMATCH");
    assert_eq!(server.finish().0, 0);
}
