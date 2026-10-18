//! Real `type="CLICK_HERE"` markup. The only local documents that carry actual
//! 누름틀 fields are the public 간이기안문/일반기안문 templates in
//! `tests/fixtures/corpus` (MIT, chrisryugj/kordoc); every other field fixture
//! is synthetic. These tests copy the templates into a temp root and never
//! touch the committed files.

use hae_diag::{Code, Limits};
use hae_exec::SessionExecutor;
use serde_json::{json, Value};
use std::{fs, path::PathBuf};

const GANI: &[u8] = include_bytes!("../../../tests/fixtures/corpus/gani-gian.hwpx");
const ILBAN: &[u8] = include_bytes!("../../../tests/fixtures/corpus/ilban-gian.hwpx");
const SECTION: &str = "Contents/section0.xml";

struct Harness {
    dir: PathBuf,
    exec: SessionExecutor,
    original: Vec<u8>,
}
impl Harness {
    fn new(bytes: &[u8]) -> Self {
        let dir = std::env::temp_dir().join(hae_model::new_id("hae-fields-real-").unwrap());
        fs::create_dir(&dir).unwrap();
        let input = dir.join("input.hwpx");
        fs::write(&input, bytes).unwrap();
        let mut exec = SessionExecutor::new(&dir, &dir, Limits::default()).unwrap();
        q(&mut exec, "document.open", json!({"path": input}));
        Self {
            dir,
            exec,
            original: bytes.to_vec(),
        }
    }
    fn list(&mut self, params: Value) -> Value {
        q(&mut self.exec, "field.list", params)
    }
    fn all(&mut self) -> Vec<Value> {
        self.list(json!({}))["fields"].as_array().unwrap().clone()
    }
    fn by_name(&mut self, name: &str) -> Value {
        self.all()
            .into_iter()
            .find(|f| f["name"] == name)
            .unwrap_or_else(|| panic!("no field named {name}"))
    }
    fn export(&mut self) -> Vec<u8> {
        let path = self.dir.join("out.hwpx");
        q(&mut self.exec, "document.export", json!({"path":path}));
        let bytes = fs::read(&path).unwrap();
        fs::remove_file(path).unwrap();
        bytes
    }
    fn original(&mut self) {
        assert_eq!(self.export(), self.original);
    }
    fn set(
        &mut self,
        f: &Value,
        text: &str,
        version: u64,
        dry: bool,
    ) -> hae_exec::ResponseEnvelope {
        self.exec.call(
            "field.setText",
            json!({"target":f["nodeId"],"name":f["name"],"text":text}),
            Some(version),
            dry,
        )
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
fn section(bytes: &[u8]) -> String {
    let a = hae_zip::Archive::open(bytes.to_vec(), &Limits::default()).unwrap();
    String::from_utf8(a.get(SECTION).unwrap().payload().to_vec()).unwrap()
}
/// Byte-exact marker and parameter text, so a fill can only be accepted when
/// everything between (and around) the two markers is untouched.
fn markers(xml: &str, id: &str) -> (String, String) {
    let start = xml
        .find(&format!("<hp:fieldBegin id=\"{id}\""))
        .unwrap_or_else(|| panic!("no fieldBegin {id}"));
    let begin = &xml[start..];
    let begin = &begin[..begin.find("</hp:fieldBegin>").unwrap() + "</hp:fieldBegin>".len()];
    let end = xml
        .find(&format!("<hp:fieldEnd beginIDRef=\"{id}\""))
        .unwrap_or_else(|| panic!("no fieldEnd {id}"));
    let end = &xml[end..];
    let end = &end[..end.find('>').unwrap() + 1];
    (begin.to_string(), end.to_string())
}

#[test]
fn real_gani_gian_lists_click_here_fields_inside_table_cells() {
    let mut h = Harness::new(GANI);
    let fields = h.all();
    assert_eq!(fields.len(), 13);
    for f in &fields {
        assert_eq!(f["type"], "CLICK_HERE", "{f}");
        assert_eq!(f["setTextSupported"], true, "{f}");
        assert!(f["reason"].is_null());
        assert_eq!(f["empty"], true);
        assert_eq!(f["text"], "");
        assert_eq!(f["multiParagraph"], false);
        assert_eq!(f["nested"], false);
        assert!(!f["name"].as_str().unwrap().is_empty());
        // Real Hancom markup carries `id` but no `fieldid` attribute.
        assert!(f["fieldid"].is_null());
        assert!(!f["id"].as_str().unwrap().is_empty());
        assert!(f["commandRaw"]
            .as_str()
            .unwrap()
            .starts_with("Clickhere:set:"));
        assert!(f["command"].as_str().unwrap().starts_with("Clickhere:set:"));
    }
    assert_eq!(fields.iter().filter(|f| f["inCell"] == true).count(), 11);
    let names: Vec<_> = fields.iter().map(|f| f["name"].clone()).collect();
    for expected in ["생산등록번호", "결재직위1", "제목", "요약설명"] {
        assert!(names.iter().any(|n| n == expected), "{expected} missing");
    }
    h.original();
}

#[test]
fn real_gani_gian_fill_keeps_markers_parameters_and_restores_bytes() {
    let mut h = Harness::new(GANI);
    let before = section(&h.original.clone());
    let title = h.by_name("제목");
    let approval = h.by_name("결재직위1");
    let (title_begin, title_end) = markers(&before, title["id"].as_str().unwrap());
    let (approval_begin, approval_end) = markers(&before, approval["id"].as_str().unwrap());
    assert!(h.set(&title, "2026년 업무계획 보고", 0, false).ok);
    assert!(h.set(&approval, "과장 홍길동", 1, false).ok);
    let edited_bytes = h.export();
    let after = section(&edited_bytes);
    assert!(after.contains("<hp:t>2026년 업무계획 보고</hp:t>"));
    assert!(after.contains("<hp:t>과장 홍길동</hp:t>"));
    assert_eq!(
        markers(&after, title["id"].as_str().unwrap()),
        (title_begin, title_end)
    );
    assert_eq!(
        markers(&after, approval["id"].as_str().unwrap()),
        (approval_begin, approval_end)
    );
    assert_eq!(h.by_name("제목")["text"], "2026년 업무계획 보고");
    assert_eq!(h.by_name("결재직위1")["text"], "과장 홍길동");
    // Untouched entries keep their original compressed bytes and local headers.
    let a = hae_zip::Archive::open(edited_bytes.clone(), &Limits::default()).unwrap();
    let b = hae_zip::Archive::open(h.original.clone(), &Limits::default()).unwrap();
    for e in b.entries() {
        if e.payload() == a.get(&e.name).unwrap().payload() {
            assert_eq!(b.raw_data(&e.name), a.raw_data(&e.name));
            assert_eq!(b.raw_local_header(&e.name), a.raw_local_header(&e.name));
        }
    }
    // Two fills are two history units; Undo/Redo each advance the revision and
    // restore the stored bytes exactly.
    assert!(h.exec.call("history.undo", json!({}), Some(2), false).ok);
    assert!(h.exec.call("history.undo", json!({}), Some(3), false).ok);
    h.original();
    assert!(h.exec.call("history.redo", json!({}), Some(4), false).ok);
    assert!(h.exec.call("history.redo", json!({}), Some(5), false).ok);
    assert_eq!(h.export(), edited_bytes);
}

#[test]
fn real_ilban_gian_pages_lists_and_fills_a_body_paragraph() {
    let mut h = Harness::new(ILBAN);
    let first = h.list(json!({"limit":10}));
    assert_eq!(first["fields"].as_array().unwrap().len(), 10);
    assert_eq!(first["nextCursor"], "0:10");
    let mut seen = first["fields"].as_array().unwrap().clone();
    let mut cursor = first["nextCursor"].clone();
    while !cursor.is_null() {
        let page = h.list(json!({"limit":10,"cursor":cursor.clone()}));
        seen.extend(page["fields"].as_array().unwrap().clone());
        cursor = page["nextCursor"].clone();
    }
    assert_eq!(seen.len(), 23);
    let full = h.all();
    assert_eq!(seen, full);
    for f in &full {
        assert_eq!(f["type"], "CLICK_HERE", "{f}");
        assert_eq!(f["setTextSupported"], true, "{f}");
        assert_eq!(f["inCell"], false);
    }
    let body = h.by_name("본문");
    // A scope restricted to the containing paragraph still finds the field.
    let scoped = h.list(json!({"scope": body["nodeId"]}));
    assert_eq!(scoped["fields"].as_array().unwrap().len(), 1);
    assert_eq!(scoped["fields"][0]["name"], "본문");
    assert!(h.set(&body, "보고 내용을 한 줄로 입력", 0, false).ok);
    let edited = h.export();
    assert!(section(&edited).contains("<hp:t>보고 내용을 한 줄로 입력</hp:t>"));
    assert_eq!(h.by_name("본문")["text"], "보고 내용을 한 줄로 입력");
    assert!(h.exec.call("history.undo", json!({}), Some(1), false).ok);
    h.original();
}

#[test]
fn real_ilban_gian_rejects_missing_out_of_range_and_dry_run_edits() {
    let mut h = Harness::new(ILBAN);
    let title = h.by_name("제목");
    let renamed = h.exec.call(
        "field.setText",
        json!({"target":title["nodeId"],"name":"없는필드","text":"x"}),
        Some(0),
        false,
    );
    assert_eq!(renamed.diagnostics[0].code, Code::TargetNotFound);
    let wrong_id = h.exec.call(
        "field.setText",
        json!({"target":title["nodeId"],"name":"제목","fieldid":"nope","text":"x"}),
        Some(0),
        false,
    );
    assert_eq!(wrong_id.diagnostics[0].code, Code::TargetNotFound);
    let out_of_range = h.exec.call(
        "field.setText",
        json!({"target":title["nodeId"],"name":"제목","occurrence":1,"text":"x"}),
        Some(0),
        false,
    );
    assert_eq!(out_of_range.diagnostics[0].code, Code::TargetNotFound);
    // `fieldid` also matches the raw `id` attribute of real Hancom markup.
    let by_raw_id = h.exec.call(
        "field.setText",
        json!({"target":title["nodeId"],"fieldid":title["id"],"text":"제목 값"}),
        Some(0),
        true,
    );
    assert!(by_raw_id.ok, "{:?}", by_raw_id.diagnostics);
    let dry = by_raw_id.result.unwrap();
    assert_eq!(dry["dryRun"], true);
    assert_eq!(dry["afterVersion"], 0);
    assert_eq!(h.exec.current_revision(), Some(0));
    h.original();
}

#[test]
fn real_gani_gian_batch_is_atomic_and_one_undo_unit() {
    let mut h = Harness::new(GANI);
    let title = h.by_name("제목");
    let org = h.by_name("작성기관");
    let step = |f: &Value, text: &str| json!({"command":"field.setText","params":{"target":f["nodeId"],"name":f["name"],"text":text}});
    let r = h.exec.call(
        "command.batch",
        json!({"steps":[step(&title,"업무계획"),step(&org,"○○초등학교")]}),
        Some(0),
        false,
    );
    assert!(r.ok, "{:?}", r.diagnostics);
    assert!(h.export() != h.original);
    assert!(h.exec.call("history.undo", json!({}), Some(1), false).ok);
    h.original();
    let r = h.exec.call(
        "command.batch",
        json!({"steps":[step(&title,"업무계획"),{"command":"field.setText","params":{"target":org["nodeId"],"name":"없는필드","text":"x"}}]}),
        Some(2),
        false,
    );
    assert!(!r.ok);
    h.original();
}
