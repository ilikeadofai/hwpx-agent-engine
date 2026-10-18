//! Paragraph insertion/deletion and `document.create` through the executor:
//! node ids, Undo/Redo byte identity, dryRun, batch path shifting, refusals,
//! file publication rules and determinism.
use hae_diag::{Code, Limits};
use hae_exec::{ResponseEnvelope, SessionExecutor};
use serde_json::{json, Value};
use std::{fs, path::PathBuf};

const SOURCE: &[u8] = include_bytes!("../../../tests/fixtures/paragraph/paragraphs.hwpx");

struct Case {
    exec: SessionExecutor,
    root: PathBuf,
    version: u64,
}
impl Drop for Case {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn code(r: &ResponseEnvelope) -> Code {
    r.diagnostics[0].code
}
fn reason(r: &ResponseEnvelope) -> Option<&str> {
    r.diagnostics[0].reason.as_deref()
}

fn run_root(tag: &str) -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../spikes/work/paragraph-structure/test-runs")
        .join(format!("{tag}-{}", hae_model::new_id("run-").unwrap()));
    fs::create_dir_all(&root).unwrap();
    root.canonicalize().unwrap()
}

impl Case {
    fn new() -> Self {
        let root = run_root("in");
        fs::write(root.join("in.hwpx"), SOURCE).unwrap();
        let mut exec = SessionExecutor::new(&root, &root, Limits::default()).unwrap();
        assert!(
            exec.call(
                "document.open",
                json!({"path":root.join("in.hwpx")}),
                None,
                false
            )
            .ok
        );
        Self {
            exec,
            root,
            version: 0,
        }
    }
    fn edit(&mut self, cmd: &str, params: Value) -> ResponseEnvelope {
        let r = self.exec.call(cmd, params, Some(self.version), false);
        if r.ok && r.result.as_ref().is_some_and(|v| v["outcome"] == "applied") {
            self.version += 1;
        }
        r
    }
    fn ok(&mut self, cmd: &str, params: Value) -> Value {
        let r = self.edit(cmd, params);
        assert!(r.ok, "{cmd}: {:?}", r.diagnostics);
        r.result.unwrap()
    }
    fn find(&mut self, text: &str) -> String {
        let q = self
            .exec
            .call(
                "element.query",
                json!({"kind":"paragraph","textContains":text}),
                None,
                false,
            )
            .result
            .unwrap();
        let matches = q["matches"].as_array().unwrap();
        assert_eq!(matches.len(), 1, "{text}: {q}");
        matches[0]["nodeId"].as_str().unwrap().to_string()
    }
    fn export(&mut self, name: &str) -> String {
        let r = self
            .exec
            .call(
                "document.export",
                json!({"path": self.root.join(name)}),
                None,
                false,
            )
            .result
            .unwrap();
        r["sha256"].as_str().unwrap().to_string()
    }
    fn texts(&mut self) -> Vec<String> {
        let q = self
            .exec
            .call("element.query", json!({"kind":"paragraph"}), None, false)
            .result
            .unwrap();
        q["matches"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["textPreview"].as_str().unwrap_or("").to_string())
            .collect()
    }
}

#[test]
fn insert_undo_and_redo_report_nodes_and_restore_exact_bytes() {
    let mut c = Case::new();
    let original = c.export("original.hwpx");
    let target = c.find("둘째 문단");
    let inserted = c.ok(
        "paragraph.insert",
        json!({"target":target,"position":"after","text":"새 문단"}),
    );
    assert_eq!(inserted["outcome"], "applied");
    assert_eq!(inserted["paragraph"][0]["command"], "paragraph.insert");
    assert_eq!(inserted["paragraph"][0]["position"], "after");
    let created = inserted["paragraph"][0]["created"][0]["nodeId"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(inserted["paragraph"][0]["created"][0]["kind"], "paragraph");
    assert!(inserted["changedIds"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == &json!(created)));
    assert!(inserted["stale"]["prvText"].as_bool().unwrap());
    let edited = c.export("edited.hwpx");
    assert_ne!(edited, original);

    let undo = c.ok("history.undo", json!({}));
    assert_eq!(undo["outcome"], "applied");
    assert_eq!(c.export("undone.hwpx"), original);

    let redo = c.ok("history.redo", json!({}));
    assert_eq!(redo["outcome"], "applied");
    assert_eq!(c.export("redone.hwpx"), edited);
    // Redo restores the same new node instead of re-creating it.
    assert!(redo["changedIds"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == &json!(created)));
}

#[test]
fn delete_undo_restores_the_same_node_ids() {
    let mut c = Case::new();
    let target = c.find("둘째 문단");
    let third = c.find("셋째 문단");
    let r = c.ok("paragraph.delete", json!({"target":target,"count":2}));
    let removed: Vec<Value> = r["paragraph"][0]["removed"].as_array().unwrap().to_vec();
    assert_eq!(removed.len(), 2);
    assert!(removed.iter().any(|v| v == &json!(target)));
    assert!(removed.iter().any(|v| v == &json!(third)));
    assert!(!c.texts().contains(&"둘째 문단".to_string()));

    c.ok("history.undo", json!({}));
    let texts = c.texts();
    assert!(texts.contains(&"둘째 문단".to_string()));
    assert!(texts.contains(&"셋째 문단".to_string()));
    // The restored paragraphs keep their original node ids.
    assert_eq!(c.find("둘째 문단"), target);
    // The deleted node id is not reused by any current node.
    let q = c
        .exec
        .call("element.get", json!({"target":third}), None, false);
    assert!(q.ok, "restored id must resolve: {:?}", q.diagnostics);
}

#[test]
fn dry_run_reports_null_ids_and_changes_nothing() {
    let mut c = Case::new();
    let original = c.export("original.hwpx");
    let target = c.find("셋째 문단");
    let dry = c.exec.call(
        "paragraph.insert",
        json!({"target":target,"position":"before","text":"가상","count":2}),
        Some(c.version),
        true,
    );
    assert!(dry.ok, "{:?}", dry.diagnostics);
    let r = dry.result.unwrap();
    assert_eq!(r["dryRun"], true);
    assert_eq!(r["predictedAfterVersion"], c.version + 1);
    assert_eq!(r["paragraph"][0]["created"][0]["nodeId"], Value::Null);
    assert_eq!(
        c.exec
            .call("document.inspect", json!({}), None, false)
            .result
            .unwrap()["revision"],
        c.version
    );
    assert_eq!(c.export("after-dry.hwpx"), original);
}

#[test]
fn batch_follows_paragraphs_moved_by_an_earlier_step() {
    let mut c = Case::new();
    let target = c.find("셋째 문단");
    let r = c.ok(
        "command.batch",
        json!({"steps":[
            {"command":"paragraph.insert","params":{"target":target,"position":"before","text":"삽입"}},
            {"command":"text.replace","params":{"target":target,"range":{"start":0,"end":2},"text":"바꾼"}}
        ]}),
    );
    assert_eq!(r["outcome"], "applied");
    assert_eq!(r["paragraph"][0]["command"], "paragraph.insert");
    let texts = c.texts();
    assert!(texts.contains(&"삽입".to_string()), "{texts:?}");
    assert!(
        texts.iter().any(|t| t.starts_with("바꾼 문단")),
        "{texts:?}"
    );
}

#[test]
fn batch_mixes_a_paragraph_insert_with_a_table_edit() {
    let mut c = Case::new();
    let first = c.find("첫 문단");
    let tables = c
        .exec
        .call("element.query", json!({"kind":"table"}), None, false)
        .result
        .unwrap();
    let table = tables["matches"][0]["nodeId"].as_str().unwrap().to_string();
    let r = c.ok(
        "command.batch",
        json!({"steps":[
            {"command":"paragraph.insert","params":{"target":first,"position":"after","text":"앞 문단"}},
            {"command":"table.insertRow","params":{"target":table,"index":1}}
        ]}),
    );
    assert_eq!(r["outcome"], "applied");
    assert_eq!(r["structure"][0]["command"], "table.insertRow");
    let q = c
        .exec
        .call("table.get", json!({"table":table}), None, false)
        .result
        .unwrap();
    assert_eq!(q["items"][0]["rows"], 2);
}

#[test]
fn refusal_reasons_are_explicit() {
    let mut c = Case::new();
    let table = c
        .exec
        .call("element.query", json!({"kind":"table"}), None, false)
        .result
        .unwrap()["matches"][0]["nodeId"]
        .as_str()
        .unwrap()
        .to_string();
    // The paragraph that carries the first table cannot be deleted.
    let holder = table_holder(&mut c);
    let r = c.edit("paragraph.delete", json!({"target":holder}));
    assert_eq!(code(&r), Code::Unsupported);
    assert_eq!(reason(&r), Some("paragraph_holds_table"));
    // A paragraph inside a table cell that is the only one in its cell.
    let cell_para = c.find("한 문단 셀");
    let r = c.edit("paragraph.delete", json!({"target":cell_para}));
    assert_eq!(reason(&r), Some("paragraph_would_empty_container"));
    // The section-property paragraph and an object paragraph.
    let first = c.find("첫 문단");
    let r = c.edit("paragraph.delete", json!({"target":first}));
    assert_eq!(reason(&r), Some("paragraph_content_unsupported"));
    let picture = c
        .exec
        .call("element.query", json!({"kind":"picture"}), None, false)
        .result
        .unwrap()["matches"][0]["nodeId"]
        .as_str()
        .unwrap()
        .to_string();
    let object = c
        .exec
        .call("element.get", json!({"target":picture}), None, false)
        .result
        .unwrap()["outline"]["parent"]
        .as_str()
        .unwrap()
        .to_string();
    let r = c.edit("paragraph.delete", json!({"target":object}));
    assert_eq!(reason(&r), Some("paragraph_content_unsupported"));
    // Inserting before the section-property paragraph.
    let r = c.edit(
        "paragraph.insert",
        json!({"target":first,"position":"before","text":"앞"}),
    );
    assert_eq!(reason(&r), Some("paragraph_before_section_property"));
    // The table id itself is not a paragraph.
    let r = c.edit(
        "paragraph.insert",
        json!({"target":table,"position":"after"}),
    );
    assert_eq!(reason(&r), Some("wrong_kind"));
}

/// Node id of the top-level paragraph that carries the fixture's first table.
fn table_holder(c: &mut Case) -> String {
    let q = c
        .exec
        .call("element.query", json!({"kind":"table"}), None, false)
        .result
        .unwrap();
    let table = q["matches"][0]["nodeId"].as_str().unwrap().to_string();
    let get = c
        .exec
        .call("element.get", json!({"target":table}), None, false)
        .result
        .unwrap();
    get["outline"]["parent"].as_str().unwrap().to_string()
}

#[test]
fn project_reopen_keeps_the_inserted_node_id_and_the_document_diff_reports_it() {
    let mut c = Case::new();
    let target = c.find("셋째 문단");
    let snapshot = c
        .exec
        .call("document.snapshot", json!({}), None, false)
        .result
        .unwrap()["snapshotId"]
        .as_str()
        .unwrap()
        .to_string();
    let inserted = c.ok(
        "paragraph.insert",
        json!({"target":target,"position":"after","text":"영구 문단"}),
    );
    let created = inserted["paragraph"][0]["created"][0]["nodeId"]
        .as_str()
        .unwrap()
        .to_string();
    let diff = c
        .exec
        .call(
            "document.diff",
            json!({"baseSnapshotId":snapshot}),
            None,
            false,
        )
        .result
        .unwrap();
    assert!(diff["createdNodes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|n| n["nodeId"] == json!(created) && n["kind"] == "paragraph"));
    // Save the project, reopen it, and the persistent id map must return the
    // same nodeId for the new paragraph (design §3.4).
    c.ok("project.save", json!({"path": c.root.join("p.hwpwork")}));
    c.exec.call("document.close", json!({}), None, false);
    let reopen = c.exec.call(
        "document.open",
        json!({"path": c.root.join("p.hwpwork")}),
        None,
        false,
    );
    assert!(reopen.ok, "{:?}", reopen.diagnostics);
    let got = c
        .exec
        .call("element.get", json!({"target":created}), None, false);
    assert!(
        got.ok,
        "inserted id must survive project reopen: {:?}",
        got.diagnostics
    );
}

// ------------------------------------------------------------ document.create

fn create(
    exec: &mut SessionExecutor,
    path: &PathBuf,
    paragraphs: Value,
    title: Value,
) -> ResponseEnvelope {
    exec.call(
        "document.create",
        json!({"path":path,"paragraphs":paragraphs,"title":title}),
        None,
        false,
    )
}

#[test]
fn created_document_opens_editable_and_is_verified() {
    let root = run_root("create");
    let mut exec = SessionExecutor::new(&root, &root, Limits::default()).unwrap();
    let out = root.join("new.hwpx");
    let r = create(
        &mut exec,
        &out,
        json!(["첫 문단", "", "셋째 문단"]),
        json!("새 문서"),
    );
    assert!(r.ok, "{:?}", r.diagnostics);
    let result = r.result.unwrap();
    assert_eq!(result["paragraphCount"], 3);
    assert_eq!(result["title"], "새 문서");
    assert_eq!(result["bytes"], fs::metadata(&out).unwrap().len());
    assert_eq!(
        result["entries"],
        json!([
            "mimetype",
            "version.xml",
            "META-INF/container.xml",
            "Contents/content.hpf",
            "Contents/header.xml",
            "Contents/section0.xml"
        ])
    );
    // No session document was opened or changed by document.create.
    assert!(exec.current_document_id().is_none());
    // The published bytes pass the independent creation gate again.
    let bytes = fs::read(&out).unwrap();
    assert_eq!(result["sha256"], hae_package::sha256_hex(&bytes));
    let mut exec2 = SessionExecutor::new(&root, &root, Limits::default()).unwrap();
    assert!(
        exec2
            .call("document.open", json!({"path":out}), None, false)
            .ok
    );
    let q = exec2
        .call("element.query", json!({"kind":"paragraph"}), None, false)
        .result
        .unwrap();
    let texts: Vec<&str> = q["matches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["textPreview"].as_str().unwrap())
        .collect();
    assert_eq!(texts, vec!["첫 문단", "", "셋째 문단"]);
    assert_eq!(q["matches"][0]["editable"], true);
    assert_eq!(
        q["matches"][1]["edits"]["paragraph.insert"]["available"],
        true
    );
    // Edit the created document and export it: the save gate verifies it.
    let first = q["matches"][0]["nodeId"].as_str().unwrap().to_string();
    let edit = exec2.call(
        "paragraph.insert",
        json!({"target":first,"position":"after","text":"추가"}),
        Some(0),
        false,
    );
    assert!(edit.ok, "{:?}", edit.diagnostics);
    let export = exec2.call(
        "document.export",
        json!({"path":root.join("new-edited.hwpx")}),
        None,
        false,
    );
    assert!(export.ok, "{:?}", export.diagnostics);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn create_never_overwrites_and_is_deterministic() {
    let root = run_root("create");

    let mut exec = SessionExecutor::new(&root, &root, Limits::default()).unwrap();
    let a = root.join("a.hwpx");
    let b = root.join("b.hwpx");
    let first = create(&mut exec, &a, json!(["본문", "둘째"]), Value::Null);
    assert!(first.ok, "{:?}", first.diagnostics);
    let second = create(&mut exec, &b, json!(["본문", "둘째"]), Value::Null);
    assert!(second.ok);
    assert_eq!(
        fs::read(&a).unwrap(),
        fs::read(&b).unwrap(),
        "same request must produce identical bytes"
    );
    // Repeating against an existing path is refused and leaves it untouched.
    let before = fs::read(&a).unwrap();
    let again = create(&mut exec, &a, json!(["다른 본문"]), Value::Null);
    assert!(!again.ok);
    assert_eq!(code(&again), Code::TargetExists);
    assert_eq!(fs::read(&a).unwrap(), before);
    // A path outside both roots is refused before any file exists.
    let outside = root.parent().unwrap().join("escaped.hwpx");
    let escaped = create(&mut exec, &outside, json!(["x"]), Value::Null);
    assert!(!escaped.ok);
    assert_eq!(code(&escaped), Code::PathNotAllowed);
    assert!(!outside.exists());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn create_validates_text_and_paragraph_count() {
    let root = run_root("create");
    let mut exec = SessionExecutor::new(&root, &root, Limits::default()).unwrap();
    let r = create(
        &mut exec,
        &root.join("bad.hwpx"),
        json!(["a\tb"]),
        Value::Null,
    );
    assert_eq!(code(&r), Code::InvalidRequest);
    let r = create(
        &mut exec,
        &root.join("bad2.hwpx"),
        json!(["a\u{1}"]),
        Value::Null,
    );
    assert_eq!(code(&r), Code::InvalidRequest);
    assert!(!root.join("bad.hwpx").exists() && !root.join("bad2.hwpx").exists());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn created_file_is_protected_from_later_overwrites() {
    let root = run_root("create");
    let mut exec = SessionExecutor::new(&root, &root, Limits::default()).unwrap();
    let out = root.join("doc.hwpx");
    assert!(create(&mut exec, &out, json!(["본문"]), Value::Null).ok);
    // Opening a created file (from the output root) works...
    let open = exec.call("document.open", json!({"path":out}), None, false);
    assert!(open.ok, "{:?}", open.diagnostics);
    // ...and the opened file is a protected input for exports.
    let r = exec.call("document.export", json!({"path":out}), None, false);
    assert!(!r.ok);
    assert_eq!(code(&r), Code::PathNotAllowed);
    let _ = fs::remove_dir_all(&root);
}
