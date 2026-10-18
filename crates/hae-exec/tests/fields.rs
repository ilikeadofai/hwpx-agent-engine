use hae_diag::{Code, Limits};
use hae_exec::SessionExecutor;
use serde_json::{json, Value};
use std::{fs, path::PathBuf};
struct Harness {
    dir: PathBuf,
    exec: SessionExecutor,
}
impl Harness {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(hae_model::new_id("hae-fields-").unwrap());
        fs::create_dir(&dir).unwrap();
        let input = dir.join("input.hwpx");
        fs::write(
            &input,
            include_bytes!("../../../tests/fixtures/editing/fields.hwpx"),
        )
        .unwrap();
        let mut exec = SessionExecutor::new(&dir, &dir, Limits::default()).unwrap();
        q(&mut exec, "document.open", json!({"path": input}));
        Self { dir, exec }
    }
    fn list(&mut self) -> Vec<Value> {
        q(&mut self.exec, "field.list", json!({}))["fields"]
            .as_array()
            .unwrap()
            .clone()
    }
    fn field(&mut self, id: &str) -> Value {
        self.list().into_iter().find(|f| f["id"] == id).unwrap()
    }
    fn set(&mut self, id: &str, text: &str, version: u64, dry: bool) -> hae_exec::ResponseEnvelope {
        let f = self.field(id);
        self.exec.call(
            "field.setText",
            json!({"target":f["nodeId"],"fieldid":f["fieldid"],"text":text}),
            Some(version),
            dry,
        )
    }
    fn export(&mut self) -> Vec<u8> {
        let path = self.dir.join("out.hwpx");
        q(&mut self.exec, "document.export", json!({"path":path}));
        let bytes = fs::read(&path).unwrap();
        fs::remove_file(path).unwrap();
        bytes
    }
    fn original(&mut self) {
        assert_eq!(
            self.export(),
            include_bytes!("../../../tests/fixtures/editing/fields.hwpx")
        );
    }
}
impl Drop for Harness {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}
fn q(e: &mut SessionExecutor, c: &str, p: Value) -> Value {
    let r = e.call(c, p, None, false);
    assert!(r.ok, "{c}: {:?}", r.diagnostics);
    r.result.unwrap()
}
#[test]
fn field_list_empty_multiple_values_and_command_raw() {
    let mut h = Harness::new();
    let fields = h.list();
    assert_eq!(fields.len(), 17);
    let f = h.field("1");
    assert_eq!(f["text"], "");
    assert_eq!(f["empty"], true);
    assert_eq!(f["setTextSupported"], true);
    assert_eq!(f["command"], "guide&raw");
    assert_eq!(f["commandRaw"], "guide&amp;raw");
    assert_eq!(h.field("2")["text"], "old&value");
    h.original();
}
#[test]
fn field_list_nested_cell_and_multi_paragraph_are_distinct() {
    let mut h = Harness::new();
    assert_eq!(h.field("6")["nested"], true);
    assert_eq!(h.field("7")["nested"], true);
    let f = h.field("4");
    assert_eq!(f["multiParagraph"], true);
    assert_eq!(f["text"], "first\nsecond");
    assert_eq!(h.field("16")["inCell"], true);
}
#[test]
fn field_list_scope_paragraph_cell_table_section_and_missing_scope() {
    let mut h = Harness::new();
    let f = h.field("16");
    for kind in ["paragraph", "cell", "table", "section"] {
        let all = q(&mut h.exec, "element.query", json!({"kind":kind}));
        let id = if kind == "paragraph" {
            f["nodeId"].clone()
        } else {
            all["matches"][0]["nodeId"].clone()
        };
        let r = q(&mut h.exec, "field.list", json!({"scope":id}));
        assert!(r["fields"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["id"] == "16"));
        if kind != "section" {
            assert_eq!(r["fields"].as_array().unwrap().len(), 1);
        }
    }
    let r = h
        .exec
        .call("field.list", json!({"scope":"missing"}), None, false);
    assert_eq!(r.diagnostics[0].code, Code::TargetNotFound);
    h.original();
}
#[test]
fn field_set_success_escapes_preserves_markers_runs_and_raw_zip() {
    let mut h = Harness::new();
    assert!(h.set("17", "한글<&👨‍👩", 0, false).ok);
    assert_eq!(h.field("17")["text"], "한글<&👨‍👩");
    let bytes = h.export();
    let a = hae_zip::Archive::open(bytes, &Limits::default()).unwrap();
    let b = hae_zip::Archive::open(
        include_bytes!("../../../tests/fixtures/editing/fields.hwpx").to_vec(),
        &Limits::default(),
    )
    .unwrap();
    let x = std::str::from_utf8(a.get("Contents/section0.xml").unwrap().payload()).unwrap();
    assert!(x.contains("<hp:t>한글&lt;&amp;👨‍👩</hp:t>"));
    assert!(x.contains("<hp:run charPrIDRef=\"1\"><hp:t></hp:t>"));
    for e in b.entries() {
        if e.payload() == a.get(&e.name).unwrap().payload() {
            assert_eq!(b.raw_data(&e.name), a.raw_data(&e.name));
            assert_eq!(b.raw_local_header(&e.name), a.raw_local_header(&e.name));
        }
    }
}
#[test]
fn field_set_empty_and_nameless_by_fieldid_or_id() {
    let mut h = Harness::new();
    assert!(h.set("1", "filled", 0, false).ok);
    assert_eq!(h.field("1")["text"], "filled");
    let f = h.field("10");
    let r = h.exec.call(
        "field.setText",
        json!({"target":f["nodeId"],"fieldid":"10","text":"new"}),
        Some(1),
        false,
    );
    assert!(r.ok, "{:?}", r.diagnostics);
    assert_eq!(h.field("10")["text"], "new");
    h.export();
}
#[test]
fn field_set_duplicate_name_requires_occurrence_and_checks_bounds() {
    let mut h = Harness::new();
    let f = h.field("8");
    let base = json!({"target":f["nodeId"],"name":"dup","text":"new"});
    let r = h.exec.call("field.setText", base.clone(), Some(0), false);
    assert_eq!(r.diagnostics[0].code, Code::AmbiguousTarget);
    let mut p = base.clone();
    p["occurrence"] = json!(2);
    let r = h.exec.call("field.setText", p, Some(0), false);
    assert_eq!(r.diagnostics[0].code, Code::TargetNotFound);
    let mut p = base;
    p["occurrence"] = json!(1);
    let r = h.exec.call("field.setText", p, Some(0), false);
    assert!(r.ok, "{:?}", r.diagnostics);
    assert_eq!(h.field("8")["text"], "one");
    assert_eq!(h.field("9")["text"], "new");
    h.export();
}
#[test]
fn field_set_rejects_outside_paragraph_and_unknown_target() {
    let mut h = Harness::new();
    let f = h.field("1");
    for target in [f["nodeId"].clone(), json!("missing")] {
        let r = h.exec.call(
            "field.setText",
            json!({"target":target,"fieldid":"f16","text":"bad"}),
            Some(0),
            false,
        );
        assert_eq!(r.diagnostics[0].code, Code::TargetNotFound);
    }
    h.original();
}
#[test]
fn field_set_rejects_multi_paragraph_and_nested() {
    let mut h = Harness::new();
    for id in ["4", "6", "7"] {
        let r = h.set(id, "bad", 0, false);
        assert!(!r.ok);
        assert!(r.diagnostics[0].reason.is_some());
    }
    h.original();
}
#[test]
fn field_set_rejects_objects_tabs_and_lexical_crossings() {
    let mut h = Harness::new();
    for id in ["11", "12", "13", "14", "15"] {
        assert_eq!(h.field(id)["setTextSupported"], false);
        let r = h.set(id, "bad", 0, false);
        assert!(!r.ok, "{id}");
        assert!(r.diagnostics[0].reason.is_some());
    }
    h.original();
}
#[test]
fn field_set_memo_read_only_hyperlink_and_cell_writable() {
    let mut h = Harness::new();
    assert_eq!(h.field("5")["setTextSupported"], false);
    assert!(!h.set("5", "bad", 0, false).ok);
    assert!(h.set("3", "new link", 0, false).ok);
    assert!(h.set("16", "new cell", 1, false).ok);
    h.export();
}
#[test]
fn field_set_dry_run_undo_redo_exact_bytes() {
    let mut h = Harness::new();
    assert!(h.set("2", "new", 0, true).ok);
    h.original();
    assert!(h.set("2", "new", 0, false).ok);
    let edited = h.export();
    assert!(h.exec.call("history.undo", json!({}), Some(1), false).ok);
    h.original();
    assert!(h.exec.call("history.redo", json!({}), Some(2), false).ok);
    assert_eq!(h.export(), edited);
}
#[test]
fn field_set_batch_atomic_and_one_undo_unit() {
    let mut h = Harness::new();
    let a = h.field("1");
    let b = h.field("16");
    let step = |f: &Value, text: &str| json!({"command":"field.setText","params":{"target":f["nodeId"],"fieldid":f["fieldid"],"text":text}});
    let r = h.exec.call(
        "command.batch",
        json!({"steps":[step(&a,"one"),step(&b,"two")]}),
        Some(0),
        false,
    );
    assert!(r.ok, "{:?}", r.diagnostics);
    h.export();
    assert!(h.exec.call("history.undo", json!({}), Some(1), false).ok);
    h.original();
    let r=h.exec.call("command.batch",json!({"steps":[step(&a,"one"),{"command":"field.setText","params":{"target":b["nodeId"],"fieldid":"absent","text":"bad"}}]}),Some(2),false);
    assert!(!r.ok);
    h.original();
}
#[test]
fn field_set_without_text_carrier_uses_existing_run_and_keeps_node_ids() {
    let mut h = Harness::new();
    let input = h.dir.join("bare.hwpx");
    let a = hae_zip::Archive::open(
        include_bytes!("../../../tests/fixtures/editing/fields.hwpx").to_vec(),
        &Limits::default(),
    )
    .unwrap();
    let mut st = hae_package::PackageState::from_archive(&a);
    let x = std::str::from_utf8(st.get("Contents/section0.xml").unwrap())
        .unwrap()
        .replacen("<hp:t></hp:t>", "", 1);
    st.set(
        "Contents/section0.xml",
        std::sync::Arc::from(x.into_bytes()),
    );
    fs::write(
        &input,
        hae_zip::write(&a, &hae_package::zip_plan(&a, &st).unwrap()).unwrap(),
    )
    .unwrap();
    q(&mut h.exec, "document.close", json!({}));
    q(&mut h.exec, "document.open", json!({"path":input}));
    let before = q(&mut h.exec, "element.query", json!({}))["matches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["nodeId"].clone())
        .collect::<Vec<_>>();
    let r = h.set("1", "bare filled", 0, false);
    assert!(r.ok, "{:?}", r.diagnostics);
    let after = q(&mut h.exec, "element.query", json!({}))["matches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["nodeId"].clone())
        .collect::<Vec<_>>();
    assert_eq!(before, after);
    assert_eq!(h.field("1")["text"], "bare filled");
    h.export();
}

fn variant(mut h: Harness, from: &str, to: &str) -> Harness {
    let a = hae_zip::Archive::open(
        include_bytes!("../../../tests/fixtures/editing/fields.hwpx").to_vec(),
        &Limits::default(),
    )
    .unwrap();
    let mut st = hae_package::PackageState::from_archive(&a);
    let s = std::str::from_utf8(st.get("Contents/section0.xml").unwrap()).unwrap();
    assert!(s.contains(from));
    let s = s.replacen(from, to, 1);
    st.set(
        "Contents/section0.xml",
        std::sync::Arc::from(s.into_bytes()),
    );
    let input = h.dir.join("variant.hwpx");
    fs::write(
        &input,
        hae_zip::write(&a, &hae_package::zip_plan(&a, &st).unwrap()).unwrap(),
    )
    .unwrap();
    q(&mut h.exec, "document.close", json!({}));
    q(&mut h.exec, "document.open", json!({"path":input}));
    h
}
#[test]
fn field_set_self_closing_empty_carriers_preserve_attributes() {
    for tag in ["<hp:t/>", "<hp:t />", "<hp:t charPrIDRef=\"1\"   />"] {
        let mut h = variant(Harness::new(), "<hp:t></hp:t>", tag);
        let r = h.set("1", "new", 0, false);
        assert!(r.ok, "{tag}: {:?}", r.diagnostics);
        assert_eq!(h.field("1")["text"], "new");
        h.export();
    }
}
#[test]
fn field_set_first_empty_carrier_keeps_first_shape() {
    let mut h = variant(
        Harness::new(),
        "<hp:t>first</hp:t></hp:run><hp:run charPrIDRef=\"1\"><hp:t>second</hp:t>",
        "<hp:t/></hp:run><hp:run charPrIDRef=\"1\"><hp:t>second</hp:t>",
    );
    let r = h.set("17", "new", 0, false);
    assert!(r.ok, "{:?}", r.diagnostics);
    h.export();
}
#[test]
fn field_set_rejects_non_text_run_payload() {
    let mut h = variant(
        Harness::new(),
        "<hp:t>link</hp:t>",
        "opaque<hp:t>link</hp:t>",
    );
    assert_eq!(h.field("3")["setTextSupported"], false);
    assert!(!h.set("3", "bad", 0, false).ok);
}
#[test]
fn field_set_requires_version_noop_does_not_record_history() {
    let mut h = Harness::new();
    let f = h.field("2");
    let p = json!({"target":f["nodeId"],"name":"filled","text":"old&value"});
    assert!(!h.exec.call("field.setText", p.clone(), None, false).ok);
    let r = h.exec.call("field.setText", p.clone(), Some(99), false);
    assert_eq!(r.diagnostics[0].code, Code::VersionConflict);
    let r = h.exec.call("field.setText", p, Some(0), false);
    assert!(r.ok);
    assert_eq!(r.result.unwrap()["outcome"], "noop");
    assert_eq!(
        h.exec
            .call("history.undo", json!({}), Some(0), false)
            .diagnostics[0]
            .code,
        Code::HistoryEmpty
    );
    h.original();
}
#[test]
fn field_set_sequence_replay_is_idempotent_and_conflicts_are_rejected() {
    let mut h = Harness::new();
    let f = h.field("2");
    let writer = h.exec.issue_writer().unwrap();
    let req = hae_exec::RequestEnvelope {
        schema_version: 1,
        session_id: Some(h.exec.session_id().into()),
        writer_id: Some(writer),
        request_sequence: Some(1),
        request_id: Some("fields-replay".into()),
        document_id: h.exec.current_document_id().map(str::to_string),
        expected_version: Some(0),
        command: "field.setText".into(),
        dry_run: false,
        params: json!({"target":f["nodeId"],"name":"filled","text":"new"}),
    };
    let first = h.exec.execute(req.clone());
    assert!(first.ok, "{:?}", first.diagnostics);
    assert!(!first.replayed);
    let second = h.exec.execute(req.clone());
    assert!(second.ok);
    assert!(second.replayed);
    assert_eq!(first.result, second.result);
    assert_eq!(h.exec.current_revision(), Some(1));
    let mut conflict = req;
    conflict.params["text"] = json!("different");
    let r = h.exec.execute(conflict);
    assert!(!r.ok);
    assert_eq!(h.field("2")["text"], "new");
    h.export();
}
