//! Interface equivalence (plan 2-3, spec §13 "인터페이스").
//!
//! One scenario — open, query, text.replace, paragraph.format, dryRun of
//! another edit, undo, redo, export — runs through (a) the in-process
//! executor, (b) `hae script`, (c) the `hae-jsonl` process and (d) the
//! `hae-mcp` process, each in a fresh session on its own copy of a corpus
//! document. Everything that varies per run (sessionId, writerId, ids, paths)
//! is dropped; the per-step outcome, revisions, changed-id counts, query match
//! counts and the exported HWPX must agree.

mod common;

use common::{first_code, sibling_bin, JsonlClient, McpClient, TempDir};
use hae_diag::Limits;
use hae_exec::{command_describe, SessionExecutor};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const DOC: &str = "bodojaryo-reference.hwpx";

struct Step {
    command: &'static str,
    params: Value,
    /// Send the current revision as expectedVersion.
    versioned: bool,
    dry_run: bool,
    save_as: Option<&'static str>,
}

fn step(command: &'static str, params: Value) -> Step {
    Step {
        command,
        params,
        versioned: false,
        dry_run: false,
        save_as: None,
    }
}

/// Params may hold `"$revision"` or `"$name.path.0.field"` references to
/// earlier results; `hae script` understands the same syntax natively.
fn scenario(input: &Path, export: &Path) -> Vec<Step> {
    let title = "$title.matches.0.nodeId";
    let other = "$other.matches.0.nodeId";
    let versioned = |mut s: Step| {
        s.versioned = true;
        s
    };
    vec![
        Step {
            save_as: Some("open"),
            ..step("document.open", json!({ "path": input }))
        },
        Step {
            save_as: Some("title"),
            ..step(
                "element.query",
                json!({ "kind": "paragraph", "textContains": "보도자료" }),
            )
        },
        Step {
            save_as: Some("other"),
            ..step(
                "element.query",
                json!({ "kind": "paragraph", "textContains": "기념행사" }),
            )
        },
        versioned(step(
            "text.replace",
            json!({ "target": title, "range": { "start": 0, "end": 2 }, "text": "공지" }),
        )),
        versioned(step(
            "paragraph.format",
            json!({ "target": title, "alignment": "right", "lineSpacing": { "kind": "percent", "value": 160 } }),
        )),
        Step {
            dry_run: true,
            ..versioned(step(
                "text.replace",
                json!({ "target": other, "range": { "start": 0, "end": 2 }, "text": "안내" }),
            ))
        },
        versioned(step("history.undo", json!({}))),
        versioned(step("history.redo", json!({}))),
        step(
            "element.query",
            json!({ "kind": "paragraph", "textContains": "공지" }),
        ),
        step("document.export", json!({ "path": export })),
    ]
}

// --- reference resolution shared by the three drivers that run step by step ---

#[derive(Default)]
struct Vars {
    revision: u64,
    named: HashMap<String, Value>,
    document_id: Option<String>,
}

impl Vars {
    fn resolve(&self, value: &Value) -> Value {
        match value {
            Value::String(s) if s.starts_with('$') => self.lookup(&s[1..]),
            Value::Array(items) => Value::Array(items.iter().map(|v| self.resolve(v)).collect()),
            Value::Object(map) => Value::Object(
                map.iter()
                    .map(|(k, v)| (k.clone(), self.resolve(v)))
                    .collect::<Map<_, _>>(),
            ),
            other => other.clone(),
        }
    }

    fn lookup(&self, reference: &str) -> Value {
        if reference == "revision" {
            return json!(self.revision);
        }
        let mut parts = reference.split('.');
        let mut value = self
            .named
            .get(parts.next().unwrap())
            .unwrap_or_else(|| panic!("unknown reference ${reference}"));
        for part in parts {
            value = match value {
                Value::Array(items) => &items[part.parse::<usize>().unwrap()],
                other => &other[part],
            };
        }
        value.clone()
    }

    fn observe(&mut self, step: &Step, response: &Value) {
        if response["ok"] != true {
            return;
        }
        let result = &response["result"];
        if step.command == "document.open" {
            self.document_id = result["documentId"].as_str().map(str::to_owned);
            self.revision = result["revision"].as_u64().unwrap();
        } else if !step.dry_run {
            if let Some(after) = result["afterVersion"].as_u64() {
                self.revision = after;
            }
        }
        if let Some(name) = step.save_as {
            self.named.insert(name.to_string(), result.clone());
        }
    }
}

/// Run a scenario through one interface and return the response envelopes.
trait Driver {
    fn run(&mut self, steps: &[Step]) -> Vec<Value>;
}

struct InProcess {
    dir: PathBuf,
}

impl Driver for InProcess {
    fn run(&mut self, steps: &[Step]) -> Vec<Value> {
        let mut exec = SessionExecutor::new(&self.dir, &self.dir, Limits::default()).unwrap();
        let mut vars = Vars::default();
        let mut out = Vec::new();
        for step in steps {
            let expected = step.versioned.then_some(vars.revision);
            let response = exec.call(
                step.command,
                vars.resolve(&step.params),
                expected,
                step.dry_run,
            );
            let response = serde_json::to_value(&response).unwrap();
            vars.observe(step, &response);
            out.push(response);
        }
        out
    }
}

struct CliScript {
    dir: PathBuf,
}

impl Driver for CliScript {
    fn run(&mut self, steps: &[Step]) -> Vec<Value> {
        let script: Vec<Value> = steps
            .iter()
            .map(|s| {
                let mut v =
                    json!({ "command": s.command, "params": s.params, "dryRun": s.dry_run });
                if s.versioned {
                    v["expectedVersion"] = json!("$revision");
                }
                if let Some(name) = s.save_as {
                    v["saveAs"] = json!(name);
                }
                v
            })
            .collect();
        let mut child = Command::new(sibling_bin("hae-cli", "hae"))
            .args(["script", "--input-root"])
            .arg(&self.dir)
            .arg("--output-root")
            .arg(&self.dir)
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(Value::Array(script).to_string().as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "hae script failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let responses: Value = serde_json::from_slice(&output.stdout).unwrap();
        responses.as_array().unwrap().clone()
    }
}

struct Jsonl {
    dir: PathBuf,
}

impl Driver for Jsonl {
    fn run(&mut self, steps: &[Step]) -> Vec<Value> {
        let mut client = JsonlClient::start(&self.dir);
        let mut vars = Vars::default();
        let mut next_seq = 1u64;
        let mut out = Vec::new();
        for step in steps {
            // The client numbers sequence-tracked requests itself (spec §6).
            let tracked = command_describe(step.command)
                .unwrap()
                .uses_request_sequence
                && !step.dry_run;
            let seq = tracked.then(|| {
                let n = next_seq;
                next_seq += 1;
                n
            });
            let response = client.request(
                seq,
                vars.document_id.as_deref(),
                step.versioned.then_some(vars.revision),
                step.command,
                vars.resolve(&step.params),
                step.dry_run,
            );
            vars.observe(step, &response);
            out.push(response);
        }
        let (exit, _) = client.finish();
        assert_eq!(exit, 0);
        out
    }
}

struct Mcp {
    dir: PathBuf,
}

impl Driver for Mcp {
    fn run(&mut self, steps: &[Step]) -> Vec<Value> {
        let mut client = McpClient::start(&self.dir);
        let mut vars = Vars::default();
        let mut out = Vec::new();
        for step in steps {
            let descriptor = command_describe(step.command).unwrap();
            let mut args = vars.resolve(&step.params);
            if descriptor.requires_document {
                args["documentId"] = json!(vars.document_id);
            }
            if step.versioned {
                args["expectedVersion"] = json!(vars.revision);
            }
            if step.dry_run {
                args["dryRun"] = json!(true);
            }
            let (response, is_error) = client.call_tool(descriptor.mcp_tool_name, args);
            assert_eq!(is_error, response["ok"] != true);
            vars.observe(step, &response);
            out.push(response);
        }
        let (exit, _) = client.finish();
        assert_eq!(exit, 0);
        out
    }
}

/// The part of a response that must agree across interfaces.
fn semantic(command: &str, response: &Value) -> Value {
    let result = &response["result"];
    let count = |value: &Value| value.as_array().map(Vec::len);
    let capabilities = result["matches"]
        .as_array()
        .filter(|_| command == "element.query")
        .map(|items| {
            items.iter().map(|item| {
            assert!(item["edits"].is_object(), "discovery must report per-command edits");
            if item["kind"] == "paragraph" {
                assert!(item["textSpans"].is_array());
                assert!(item["textSpansTruncated"].is_boolean());
            }
            json!({
                "edits": item["edits"], "editable": item["editable"],
                "textSpans": item["textSpans"], "textSpansTruncated": item["textSpansTruncated"],
                "childParagraphsEditable": item["childParagraphsEditable"],
            })
        }).collect::<Vec<_>>()
        });
    json!({
        "command": command,
        "ok": response["ok"],
        "replayed": response["replayed"],
        "code": first_code(response),
        "outcome": result["outcome"],
        "revision": result["revision"],
        "beforeVersion": result["beforeVersion"],
        "afterVersion": result["afterVersion"],
        "changedIds": count(&result["changedIds"]),
        "matches": count(&result["matches"]),
        "replacementCount": result["replacementCount"],
        "paragraphCount": result["paragraphCount"],
        "skippedCount": result["skippedCount"],
        "skipped": result["skipped"],
        "previews": result["matches"].as_array().map(|items| items.iter().map(|m| json!({"range": m["range"], "before": m["before"], "after": m["after"]})).collect::<Vec<_>>()),
        "capabilities": capabilities,
        "exportedVersion": result["exportedVersion"],
        "unmodifiedPassthrough": result["unmodifiedPassthrough"],
    })
}

fn payload_set_sha256(bytes: Vec<u8>) -> String {
    let limits = Limits::default();
    let archive = hae_zip::Archive::open(bytes, &limits).expect("exported HWPX opens");
    hae_package::PackageState::from_archive(&archive).payload_set_sha256()
}

struct Run {
    name: &'static str,
    semantic: Vec<Value>,
    export_bytes: Vec<u8>,
    export_sha256: String,
}

fn run_interface(name: &'static str, make: fn(PathBuf) -> Box<dyn Driver>) -> Run {
    let dir = TempDir::new(name);
    let input = dir.with_corpus(DOC);
    let export = dir.path().join("export.hwpx");
    let steps = scenario(&input, &export);
    let responses = make(dir.path().to_path_buf()).run(&steps);
    assert_eq!(
        responses.len(),
        steps.len(),
        "{name}: one response per step"
    );
    let semantic_steps: Vec<Value> = steps
        .iter()
        .zip(&responses)
        .map(|(s, r)| semantic(s.command, r))
        .collect();
    for (s, r) in steps.iter().zip(&responses) {
        assert_eq!(r["ok"], true, "{name}: {} failed: {r}", s.command);
    }
    let export_result = &responses.last().unwrap()["result"];
    Run {
        name,
        semantic: semantic_steps,
        export_bytes: fs::read(&export).expect("export file exists"),
        export_sha256: export_result["sha256"].as_str().unwrap().to_string(),
    }
}

#[test]
fn four_interfaces_agree_on_the_representative_scenario() {
    let runs = [
        run_interface("inproc", |dir| Box::new(InProcess { dir })),
        run_interface("cli", |dir| Box::new(CliScript { dir })),
        run_interface("jsonl", |dir| Box::new(Jsonl { dir })),
        run_interface("mcp", |dir| Box::new(Mcp { dir })),
    ];

    // Sanity of the reference run: the scenario really edits and round-trips.
    let reference = &runs[0];
    let outcomes: Vec<(&str, &str, u64)> = reference
        .semantic
        .iter()
        .map(|s| {
            (
                s["command"].as_str().unwrap(),
                s["outcome"].as_str().unwrap_or(""),
                s["afterVersion"].as_u64().unwrap_or(u64::MAX),
            )
        })
        .collect();
    assert_eq!(outcomes[3], ("text.replace", "applied", 1));
    assert_eq!(outcomes[4], ("paragraph.format", "applied", 2));
    assert_eq!(
        outcomes[5],
        ("text.replace", "applied", 2),
        "dryRun predicts but keeps revision 2"
    );
    assert_eq!(outcomes[6], ("history.undo", "applied", 3));
    assert_eq!(outcomes[7], ("history.redo", "applied", 4));
    assert_eq!(
        reference.semantic[8]["matches"], 1,
        "the replaced title is found again"
    );
    assert_eq!(reference.semantic[9]["exportedVersion"], 4);
    assert_eq!(reference.semantic[9]["unmodifiedPassthrough"], false);

    let reference_payload = payload_set_sha256(reference.export_bytes.clone());
    let original_payload = payload_set_sha256(fs::read(common::corpus_file(DOC)).unwrap());
    assert_ne!(
        reference_payload, original_payload,
        "export differs from the original"
    );

    for run in &runs[1..] {
        assert_eq!(
            run.semantic, reference.semantic,
            "{}: per-step outcome/revision/changedIds differ from in-process",
            run.name
        );
        assert_eq!(
            payload_set_sha256(run.export_bytes.clone()),
            reference_payload,
            "{}: exported payloadSetSha256 differs",
            run.name
        );
        assert_eq!(
            run.export_bytes, reference.export_bytes,
            "{}: exported HWPX bytes differ",
            run.name
        );
        assert_eq!(run.export_sha256, reference.export_sha256, "{}", run.name);
    }
}

/// Transport rules over the real `hae-jsonl` process, repeated here so the
/// interface suite shows them next to the equivalence result (spec §13 "경쟁·재전송").
#[test]
fn jsonl_transport_rules_over_a_real_process() {
    let dir = TempDir::new("rules");
    let input = dir.with_corpus(DOC);
    let mut c = JsonlClient::start(dir.path());
    let open = c.request(
        None,
        None,
        None,
        "document.open",
        json!({ "path": input }),
        false,
    );
    let doc = open["result"]["documentId"].as_str().unwrap().to_string();
    let q = c.request(
        None,
        Some(&doc),
        None,
        "element.query",
        json!({ "kind": "paragraph", "textContains": "보도자료" }),
        false,
    );
    let target = q["result"]["matches"][0]["nodeId"]
        .as_str()
        .unwrap()
        .to_string();
    let replace =
        |text: &str| json!({ "target": target, "range": {"start": 0, "end": 2}, "text": text });

    let first = c.request(
        Some(1),
        Some(&doc),
        Some(0),
        "text.replace",
        replace("공지"),
        false,
    );
    assert_eq!(first["ok"], true, "{first}");
    let replay = c.request(
        Some(1),
        Some(&doc),
        Some(0),
        "text.replace",
        replace("공지"),
        false,
    );
    assert_eq!(
        (replay["ok"].clone(), replay["replayed"].clone()),
        (json!(true), json!(true))
    );
    let conflict = c.request(
        Some(1),
        Some(&doc),
        Some(0),
        "text.replace",
        replace("안내"),
        false,
    );
    assert_eq!(first_code(&conflict), "REQUEST_ID_CONFLICT");
    let stale = c.request(
        Some(2),
        Some(&doc),
        Some(0),
        "text.replace",
        replace("안내"),
        false,
    );
    assert_eq!(first_code(&stale), "VERSION_CONFLICT");
    let skipped = c.request(
        Some(4),
        Some(&doc),
        Some(1),
        "text.replace",
        replace("안내"),
        false,
    );
    assert_eq!(first_code(&skipped), "REQUEST_OUT_OF_ORDER");
    let old = c.request(
        Some(1),
        Some(&doc),
        Some(1),
        "text.replace",
        replace("안내"),
        false,
    );
    assert_eq!(first_code(&old), "REPLAY_TOO_OLD");
    let unknown = c.send_raw(
        json!({
            "schemaVersion": 1, "sessionId": c.session_id, "writerId": "wrt-never-issued",
            "requestSequence": 3, "documentId": doc, "expectedVersion": 1,
            "command": "text.replace", "params": replace("끝"),
        })
        .to_string()
        .as_bytes(),
    );
    assert_eq!(first_code(&unknown), "UNKNOWN_WRITER");
    assert_eq!(c.finish().0, 0);
}

/// text.replaceAll over the same four interfaces: dryRun list, real edit,
/// Undo to the original bytes, and the edited export must all agree.
fn replace_all_scenario(input: &Path, export: &Path) -> Vec<Step> {
    let restored = export.with_file_name("restored.hwpx");
    let versioned = |mut s: Step| {
        s.versioned = true;
        s
    };
    let params = json!({ "find": "기념행사", "text": "행사" });
    vec![
        Step {
            save_as: Some("open"),
            ..step("document.open", json!({ "path": input }))
        },
        Step {
            dry_run: true,
            ..versioned(step("text.replaceAll", params.clone()))
        },
        versioned(step("text.replaceAll", params)),
        step(
            "element.query",
            json!({ "kind": "paragraph", "textContains": "기념행사" }),
        ),
        step("document.export", json!({ "path": export })),
        versioned(step("history.undo", json!({}))),
        step("document.export", json!({ "path": restored })),
    ]
}

#[test]
fn four_interfaces_agree_on_text_replace_all() {
    type Maker = fn(PathBuf) -> Box<dyn Driver>;
    let makers: [(&'static str, Maker); 4] = [
        ("inproc-all", |dir| Box::new(InProcess { dir })),
        ("cli-all", |dir| Box::new(CliScript { dir })),
        ("jsonl-all", |dir| Box::new(Jsonl { dir })),
        ("mcp-all", |dir| Box::new(Mcp { dir })),
    ];
    let original = fs::read(common::corpus_file(DOC)).unwrap();
    let mut runs = Vec::new();
    for (name, make) in makers {
        let dir = TempDir::new(name);
        let input = dir.with_corpus(DOC);
        let export = dir.path().join("export.hwpx");
        let steps = replace_all_scenario(&input, &export);
        let responses = make(dir.path().to_path_buf()).run(&steps);
        assert_eq!(responses.len(), steps.len(), "{name}");
        for (s, r) in steps.iter().zip(&responses) {
            assert_eq!(r["ok"], true, "{name}: {} failed: {r}", s.command);
        }
        let semantic_steps: Vec<Value> = steps
            .iter()
            .zip(&responses)
            .map(|(s, r)| semantic(s.command, r))
            .collect();
        let edited = fs::read(&export).unwrap();
        let restored = fs::read(dir.path().join("restored.hwpx")).unwrap();
        runs.push((name, semantic_steps, edited, restored));
    }
    let (_, reference, edited, restored) = &runs[0];
    assert_eq!(reference[1]["outcome"], "applied");
    assert_eq!(reference[1]["afterVersion"], 0, "dryRun keeps the revision");
    let count = reference[1]["replacementCount"].as_u64().unwrap();
    assert!(count >= 1, "the corpus paragraph is found");
    assert_eq!(
        reference[1]["previews"].as_array().unwrap().len() as u64,
        count
    );
    assert_eq!(
        reference[2]["replacementCount"],
        reference[1]["replacementCount"]
    );
    assert_eq!(reference[2]["afterVersion"], 1);
    assert_eq!(
        reference[3]["matches"], 0,
        "every replaceable match was replaced"
    );
    assert_ne!(edited, &original);
    assert_eq!(restored, &original, "Undo restores the original bytes");
    for (name, semantic_steps, e, r) in &runs[1..] {
        assert_eq!(semantic_steps, reference, "{name}: per-step results differ");
        assert_eq!(e, edited, "{name}: edited export differs");
        assert_eq!(r, restored, "{name}: restored export differs");
    }
}
