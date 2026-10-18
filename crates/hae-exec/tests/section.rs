use hae_diag::Limits;
use hae_exec::{ResponseEnvelope, SessionExecutor};
use hae_zip::Archive;
use serde_json::{json, Value};
use std::path::PathBuf;
const FIXTURE: &[u8] = include_bytes!("../../../tests/fixtures/editing/section.hwpx");
struct Session {
    exec: SessionExecutor,
    root: PathBuf,
    serial: usize,
}
impl Session {
    fn new() -> Self {
        Self::with_bytes(FIXTURE)
    }
    fn with_bytes(bytes: &[u8]) -> Self {
        let root = std::env::temp_dir().join(hae_model::new_id("section-").unwrap());
        std::fs::create_dir_all(&root).unwrap();
        let input = root.join("input.hwpx");
        std::fs::write(&input, bytes).unwrap();
        let mut exec = SessionExecutor::new(&root, &root, Limits::default()).unwrap();
        let opened = exec.call("document.open", json!({"path":input}), None, false);
        assert!(opened.ok, "{:?}", opened.diagnostics);
        Self {
            exec,
            root,
            serial: 0,
        }
    }
    fn call(&mut self, name: &str, p: Value) -> ResponseEnvelope {
        let version = self.exec.current_revision();
        self.exec.call(name, p, version, false)
    }
    fn query(&mut self, index: usize) -> Value {
        let r = self.exec.call(
            "section.getPage",
            json!({"sectionIndex":index}),
            None,
            false,
        );
        assert!(r.ok, "{:?}", r.diagnostics);
        r.result.unwrap()
    }
    fn export(&mut self) -> Vec<u8> {
        self.serial += 1;
        let path = self.root.join(format!("export-{}.hwpx", self.serial));
        let r = self
            .exec
            .call("document.export", json!({"path":path}), None, false);
        assert!(r.ok, "{:?}", r.diagnostics);
        std::fs::read(path).unwrap()
    }
    fn apply(&mut self, name: &str, p: Value) -> Value {
        let r = self.call(name, p);
        assert!(r.ok, "{:?}", r.diagnostics);
        r.result.unwrap()
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
fn payload(bytes: &[u8], name: &str) -> String {
    let a = Archive::open(bytes.to_vec(), &Limits::default()).unwrap();
    String::from_utf8(a.get(name).unwrap().payload().to_vec()).unwrap()
}
fn modified(a: &str, b: &str) -> Vec<u8> {
    let archive = Archive::open(FIXTURE.to_vec(), &Limits::default()).unwrap();
    let mut st = hae_package::PackageState::from_archive(&archive);
    let src = std::str::from_utf8(st.get("Contents/section0.xml").unwrap()).unwrap();
    assert!(src.contains(a));
    st.set(
        "Contents/section0.xml",
        std::sync::Arc::from(src.replacen(a, b, 1).into_bytes()),
    );
    hae_zip::write(&archive, &hae_package::zip_plan(&archive, &st).unwrap()).unwrap()
}

#[test]
fn section_get_page_reads_explicit_and_default_section_zero() {
    let mut s = Session::new();
    let r = s.exec.call("section.getPage", json!({}), None, false);
    assert!(r.ok);
    let q = r.result.unwrap();
    assert_eq!(q["width"], json!({"value":59528,"source":"explicit"}));
    assert_eq!(q["margins"]["left"]["value"], 8504);
    assert_eq!(q["textDirection"]["value"], "HORIZONTAL");
    assert_eq!(q["startNum"]["page"]["value"], 0);
    assert_eq!(q["grid"]["lineGrid"]["value"], 0);
    assert_eq!(s.query(1)["landscape"]["value"], "NARROWLY");
    assert_eq!(s.export(), FIXTURE);
}
#[test]
fn section_get_page_unspecified_values_are_null_without_defaults() {
    let bytes = modified(" width=\"59528\"", "");
    let mut s = Session::with_bytes(&bytes);
    let q = s.query(0);
    assert_eq!(q["width"], json!({"value":null,"source":"unspecified"}));
    assert_eq!(q["height"]["source"], "explicit");
    assert!(q["notes"][0].as_str().unwrap().contains("inheritance"));
}
#[test]
fn section_get_page_missing_optional_elements_are_unspecified() {
    let bytes = modified(
        "<hp:grid lineGrid=\"0\" charGrid=\"0\" wonggojiFormat=\"0\"/>",
        "",
    );
    let mut s = Session::with_bytes(&bytes);
    assert_eq!(
        s.query(0)["grid"]["charGrid"],
        json!({"value":null,"source":"unspecified"})
    );
}
#[test]
fn section_get_page_scope_matches_index_and_invalid_selectors_reject() {
    let mut s = Session::new();
    let q = s.query(1);
    let id = q["scope"].clone();
    let r = s
        .exec
        .call("section.getPage", json!({"scope":id}), None, false);
    assert!(r.ok);
    assert_eq!(r.result.unwrap(), q);
    assert!(
        !s.exec
            .call(
                "section.getPage",
                json!({"scope":id,"sectionIndex":1}),
                None,
                false
            )
            .ok
    );
    assert!(
        !s.exec
            .call("section.getPage", json!({"scope":"missing"}), None, false)
            .ok
    );
}
#[test]
fn section_get_page_rejects_dry_run_and_mutation_batch_without_history_change() {
    let mut s = Session::new();
    assert!(!s.exec.call("section.getPage", json!({}), None, true).ok);
    assert!(
        !s.call(
            "command.batch",
            json!({"steps":[{"command":"section.getPage","params":{}}]})
        )
        .ok
    );
    assert_eq!(s.exec.current_revision(), Some(0));
    assert_eq!(s.export(), FIXTURE);
}
#[test]
fn section_set_page_all_properties_and_only_target_section_change() {
    let mut s = Session::new();
    let other = s.query(1);
    let result=s.apply("section.setPage",json!({"landscape":"NARROWLY","width":60000,"height":85000,"gutterType":"TOP_BOTTOM","margins":{"left":2000,"right":3000,"top":1000,"bottom":1200,"header":400,"footer":500,"gutter":100}}));
    assert_eq!(result["cacheRemovals"]["Contents/section0.xml"], 2);
    let q = s.query(0);
    assert_eq!(q["width"]["value"], 60000);
    assert_eq!(q["landscape"]["value"], "NARROWLY");
    assert_eq!(q["margins"]["gutter"]["value"], 100);
    let mut after = s.query(1);
    after["revision"] = other["revision"].clone();
    assert_eq!(after, other);
    let bytes = s.export();
    assert!(!payload(&bytes, "Contents/section0.xml").contains("linesegarray"));
    assert_eq!(
        payload(&bytes, "Contents/section1.xml"),
        payload(FIXTURE, "Contents/section1.xml")
    );
    assert_eq!(
        payload(&bytes, "Preview/PrvText.txt"),
        payload(FIXTURE, "Preview/PrvText.txt")
    );
    // Retain a concrete gate artifact only when requested by the verification run.
    if let Ok(path) = std::env::var("HAE_SECTION_GATE_OUTPUT") {
        std::fs::write(path, bytes).unwrap();
    }
}
#[test]
fn section_set_page_partial_request_preserves_unrequested_values_and_raw_entries() {
    let mut s = Session::new();
    let mut before = s.query(0);
    s.apply("section.setPage", json!({"margins":{"left":7000}}));
    let mut after = s.query(0);
    before["margins"]["left"]["value"] = json!(7000);
    after["revision"] = before["revision"].clone();
    assert_eq!(after, before);
    let bytes = s.export();
    let a = Archive::open(FIXTURE.to_vec(), &Limits::default()).unwrap();
    let b = Archive::open(bytes, &Limits::default()).unwrap();
    for e in a.entries() {
        if e.name != "Contents/section0.xml" {
            assert_eq!(a.raw_data(&e.name), b.raw_data(&e.name));
            assert_eq!(a.raw_local_header(&e.name), b.raw_local_header(&e.name));
        }
    }
}
#[test]
fn section_set_page_rejects_bad_geometry_enums_dimensions_and_empty_request() {
    let mut s = Session::new();
    for p in [
        json!({}),
        json!({"margins":{}}),
        json!({"width":0}),
        json!({"height":0}),
        json!({"landscape":"LANDSCAPE"}),
        json!({"gutterType":"UNKNOWN"}),
        json!({"margins":{"left":59528}}),
        json!({"margins":{"top":84186}}),
        json!({"margins":{"header":84186}}),
        json!({"gutterType":"TOP_BOTTOM","margins":{"gutter":84186}}),
    ] {
        let r = s.call("section.setPage", p.clone());
        assert!(!r.ok, "{p}");
    }
    assert_eq!(s.export(), FIXTURE);
    assert_eq!(s.exec.current_revision(), Some(0));
}
#[test]
fn section_set_page_checks_rotated_axes_and_exact_zero_body_area() {
    let mut s = Session::new();
    assert!(
        !s.call(
            "section.setPage",
            json!({"sectionIndex":1,"margins":{"top":46772}})
        )
        .ok
    );
    assert!(
        s.call(
            "section.setPage",
            json!({"sectionIndex":1,"margins":{"left":60000}})
        )
        .ok
    );
}
#[test]
fn section_set_page_missing_measurement_fails_closed_and_can_be_supplied_explicitly() {
    let bytes = modified(" width=\"59528\"", "");
    let mut s = Session::with_bytes(&bytes);
    assert!(
        !s.call("section.setPage", json!({"margins":{"left":7000}}))
            .ok
    );
    s.apply(
        "section.setPage",
        json!({"width":59528,"margins":{"left":7000}}),
    );
    assert_eq!(s.query(0)["width"]["source"], "explicit");
    s.export();
}
#[test]
fn section_set_page_dry_run_undo_redo_restore_exact_archive() {
    history(
        "section.setPage",
        json!({"sectionIndex":1,"width":61000,"margins":{"left":7000}}),
    );
}
#[test]
fn section_set_numbering_dry_run_undo_redo_restore_exact_archive() {
    history(
        "section.setPageNumbering",
        json!({"pageStartsOn":"ODD","page":7}),
    );
}
#[test]
fn section_set_visibility_dry_run_undo_redo_restore_exact_archive() {
    history(
        "section.setVisibility",
        json!({"hideFirstHeader":true,"border":"HIDE_FIRST"}),
    );
}
fn history(command: &str, params: Value) {
    let mut s = Session::new();
    let dry = s.exec.call(command, params.clone(), Some(0), true);
    assert!(dry.ok, "{:?}", dry.diagnostics);
    assert_eq!(dry.result.unwrap()["predictedAfterVersion"], 1);
    assert_eq!(s.export(), FIXTURE);
    s.apply(command, params);
    let applied = s.export();
    assert_ne!(applied, FIXTURE);
    s.apply("history.undo", json!({}));
    assert_eq!(s.export(), FIXTURE);
    s.apply("history.redo", json!({}));
    assert_eq!(s.export(), applied);
    s.apply("history.undo", json!({}));
    assert_eq!(s.export(), FIXTURE);
}
#[test]
fn section_set_numbering_partial_preserves_counters_and_page_mode() {
    let mut s = Session::new();
    let before = s.query(0);
    s.apply("section.setPageNumbering", json!({"page":7}));
    let q = s.query(0);
    for key in ["pic", "tbl", "equation", "pageStartsOn"] {
        assert_eq!(q["startNum"][key], before["startNum"][key]);
    }
    assert_eq!(q["startNum"]["page"]["value"], 7);
    s.apply("section.setPageNumbering", json!({"pageStartsOn":"EVEN"}));
    assert_eq!(s.query(0)["startNum"]["page"]["value"], 7);
    s.export();
}
#[test]
fn section_set_numbering_rejects_bad_enum_negative_and_counter_edits() {
    let mut s = Session::new();
    for p in [
        json!({}),
        json!({"page":-1}),
        json!({"pageStartsOn":"FIRST"}),
        json!({"pic":1}),
        json!({"tbl":2}),
        json!({"equation":1}),
    ] {
        assert!(!s.call("section.setPageNumbering", p).ok);
    }
    assert_eq!(s.export(), FIXTURE);
}
#[test]
fn section_set_visibility_all_and_partial_preserve_master_page_flag() {
    let mut s = Session::new();
    let before = s.query(0);
    s.apply("section.setVisibility",json!({"hideFirstHeader":true,"hideFirstFooter":true,"hideFirstPageNum":true,"hideFirstEmptyLine":true,"showLineNumber":true,"border":"HIDE_FIRST","fill":"SHOW_FIRST"}));
    let q = s.query(0);
    assert_eq!(
        q["visibility"]["hideFirstMasterPage"],
        before["visibility"]["hideFirstMasterPage"]
    );
    s.apply("section.setVisibility", json!({"hideFirstFooter":false}));
    let after = s.query(0);
    assert_eq!(
        after["visibility"]["hideFirstHeader"],
        q["visibility"]["hideFirstHeader"]
    );
    assert_eq!(after["visibility"]["hideFirstFooter"]["value"], 0);
    s.export();
}
#[test]
fn section_set_visibility_rejects_invalid_flag_enum_and_unlisted_attribute() {
    let mut s = Session::new();
    for p in [
        json!({}),
        json!({"hideFirstHeader":2}),
        json!({"border":"INVALID"}),
        json!({"hideFirstMasterPage":true}),
    ] {
        assert!(!s.call("section.setVisibility", p).ok);
    }
    assert_eq!(s.export(), FIXTURE);
}
#[test]
fn section_all_mutations_batch_is_one_undo_unit_and_query_is_read_only() {
    let mut s = Session::new();
    let id = s.query(1)["scope"].clone();
    let batch = json!({"steps":[
        {"command":"section.setPage","params":{"scope":id,"height":85000,"margins":{"right":5000}}},
        {"command":"section.setPageNumbering","params":{"scope":id,"page":12}},
        {"command":"section.setVisibility","params":{"scope":id,"hideFirstPageNum":true}}
    ]});
    let dry = s.exec.call("command.batch", batch.clone(), Some(0), true);
    assert!(dry.ok, "{:?}", dry.diagnostics);
    assert_eq!(s.export(), FIXTURE);
    let r = s.apply("command.batch", batch);
    assert_eq!(r["afterVersion"], 1);
    assert_eq!(s.query(1)["startNum"]["page"]["value"], 12);
    s.export();
    s.apply("history.undo", json!({}));
    assert_eq!(s.export(), FIXTURE);
}
#[test]
fn section_batch_failure_is_atomic_and_identifies_failed_step() {
    let mut s = Session::new();
    let r=s.call("command.batch",json!({"steps":[{"command":"section.setPageNumbering","params":{"page":5}},{"command":"section.setPage","params":{"margins":{"left":100000}}}]}));
    assert!(!r.ok);
    assert_eq!(r.diagnostics[0].detail.as_ref().unwrap()["failedStep"], 1);
    assert_eq!(s.export(), FIXTURE);
}
#[test]
fn section_creation_and_deletion_shapes_are_rejected_without_nodes() {
    let mut s = Session::new();
    for command in [
        "section.setPage",
        "section.setPageNumbering",
        "section.setVisibility",
    ] {
        let mut p = match command {
            "section.setPage" => json!({"width":60000}),
            "section.setPageNumbering" => json!({"page":3}),
            _ => json!({"hideFirstHeader":true}),
        };
        p["sectionIndex"] = json!(2);
        let r = s.call(command, p);
        assert!(!r.ok);
        assert_eq!(
            r.diagnostics[0].reason.as_deref(),
            Some("structure_change_unsupported")
        );
    }
    assert!(
        !s.call(
            "section.setPage",
            json!({"createSection":true,"width":60000})
        )
        .ok
    );
    assert_eq!(s.export(), FIXTURE);
}
#[test]
fn section_noop_preserves_bytes_version_and_history() {
    let mut s = Session::new();
    for (command, p) in [
        ("section.setPage", json!({"width":59528})),
        ("section.setPageNumbering", json!({"page":0})),
        ("section.setVisibility", json!({"hideFirstHeader":false})),
    ] {
        let r = s.apply(command, p);
        assert_eq!(r["outcome"], "noop");
        assert_eq!(r["afterVersion"], 0);
    }
    assert_eq!(s.export(), FIXTURE);
    assert!(!s.call("history.undo", json!({})).ok);
}
#[test]
fn section_missing_or_duplicate_secpr_and_wrong_scope_reject() {
    let bytes = modified("<hp:secPr ", "<hp:otherPr ");
    let bytes = {
        let a = Archive::open(bytes, &Limits::default()).unwrap();
        let mut st = hae_package::PackageState::from_archive(&a);
        let src = std::str::from_utf8(st.get("Contents/section0.xml").unwrap())
            .unwrap()
            .replace("</hp:secPr>", "</hp:otherPr>");
        st.set(
            "Contents/section0.xml",
            std::sync::Arc::from(src.into_bytes()),
        );
        hae_zip::write(&a, &hae_package::zip_plan(&a, &st).unwrap()).unwrap()
    };
    let mut s = Session::with_bytes(&bytes);
    assert!(!s.call("section.setPage", json!({"width":60000})).ok);
    let mut s = Session::new();
    let r = s
        .exec
        .call("element.query", json!({"kind":"paragraph"}), None, false);
    let id = r.result.unwrap()["matches"][1]["nodeId"].clone();
    assert!(
        !s.call("section.setPageNumbering", json!({"scope":id,"page":5}))
            .ok
    );
}
