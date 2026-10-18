use hae_diag::{Code, Limits};
use hae_exec::SessionExecutor;
use hae_model::{Document, Kind};
use serde_json::{json, Value};
use std::{fs, path::PathBuf};

const SOURCE: &[u8] = include_bytes!("../../../tests/fixtures/tables/dimensions.hwpx");
struct Case {
    exec: SessionExecutor,
    root: PathBuf,
    table: String,
    cell: String,
}
impl Drop for Case {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
impl Case {
    fn new() -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../spikes/work/tables/test-runs")
            .join(hae_model::new_id("run-").unwrap());
        fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
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
        let tables = exec
            .call("element.query", json!({"kind":"table"}), None, false)
            .result
            .unwrap();
        let table = tables["matches"][1]["nodeId"].as_str().unwrap().to_string();
        let cells = exec
            .call(
                "element.query",
                json!({"table":table,"row":0,"col":0}),
                None,
                false,
            )
            .result
            .unwrap();
        let cell = cells["matches"][0]["nodeId"].as_str().unwrap().to_string();
        Self {
            exec,
            root,
            table,
            cell,
        }
    }
    fn params(&self, cmd: &str) -> Value {
        match cmd {
            "table.setColumnWidths" => json!({"target":self.table,"widths":[22000,23000]}),
            "table.setRowHeights" => json!({"target":self.table,"heights":[2800,3000]}),
            "cell.setFormat" => {
                json!({"target":self.cell,"borderFillId":"3","background":"#aabbcc","cellMargin":{"left":200,"top":210}})
            }
            "cell.setSize" => json!({"target":self.cell,"width":23000,"height":2800}),
            _ => unreachable!(),
        }
    }
    fn export(&mut self, name: &str) -> Vec<u8> {
        let path = self.root.join(name);
        let r = self
            .exec
            .call("document.export", json!({"path":path}), None, false);
        assert!(r.ok, "{r:?}");
        fs::read(path).unwrap()
    }
    fn exercise(cmd: &str, mode: &str) {
        let mut s = Self::new();
        let p = s.params(cmd);
        match mode {
            "normal" => {
                let r = s.exec.call(cmd, p, Some(0), false);
                assert!(r.ok, "{r:?}");
                let bytes = s.export("out.hwpx");
                assert_ne!(bytes, SOURCE);
                let d = Document::open(bytes, &Limits::default()).unwrap();
                assert_eq!(d.nodes.iter().filter(|n| n.kind == Kind::Table).count(), 3);
                assert_eq!(d.nodes.iter().filter(|n| n.kind == Kind::Cell).count(), 10);
                let table = d
                    .nodes
                    .iter()
                    .filter(|n| n.kind == Kind::Table)
                    .nth(1)
                    .unwrap();
                let xml = d.doc_of(table);
                let sz = xml
                    .el(table.el)
                    .children
                    .iter()
                    .find(|&&i| xml.el(i).local == "sz")
                    .unwrap();
                match cmd {
                    "table.setColumnWidths" => {
                        assert_eq!(xml.el(*sz).attr_value("width"), Some("45000"))
                    }
                    "table.setRowHeights" => {
                        assert_eq!(xml.el(*sz).attr_value("height"), Some("5800"))
                    }
                    "cell.setSize" => {
                        assert_eq!(xml.el(*sz).attr_value("width"), Some("44000"));
                        assert_eq!(xml.el(*sz).attr_value("height"), Some("5200"));
                    }
                    "cell.setFormat" => assert!(
                        String::from_utf8_lossy(xml.bytes()).contains("borderFillIDRef=\"4\"")
                    ),
                    _ => unreachable!(),
                }
            }
            "reject" => {
                let bad = match cmd {
                    "table.setColumnWidths" => json!({"target":s.table,"widths":[1]}),
                    "table.setRowHeights" => json!({"target":s.table,"height":0}),
                    "cell.setFormat" => json!({"target":s.cell,"borderFillId":"9999"}),
                    "cell.setSize" => json!({"target":s.cell,"width":1}),
                    _ => unreachable!(),
                };
                let r = s.exec.call(cmd, bad, Some(0), false);
                assert!(!r.ok);
                assert_eq!(
                    r.diagnostics[0].code,
                    if cmd == "cell.setFormat" {
                        Code::TargetNotFound
                    } else {
                        Code::InvalidArgument
                    }
                );
                assert_eq!(s.export("out.hwpx"), SOURCE);
            }
            "dry" => {
                let r = s.exec.call(cmd, p, Some(0), true);
                assert!(r.ok, "{r:?}");
                assert_eq!(r.result.unwrap()["afterVersion"], 0);
                assert_eq!(s.export("dry.hwpx"), SOURCE);
                assert!(!s.exec.call("history.undo", json!({}), Some(0), false).ok);
            }
            "undo" => {
                let r = s.exec.call(cmd, p, Some(0), false);
                assert!(r.ok, "{r:?}");
                let edited = s.export("edit.hwpx");
                assert!(s.exec.call("history.undo", json!({}), Some(1), false).ok);
                assert_eq!(s.export("undo.hwpx"), SOURCE);
                assert!(s.exec.call("history.redo", json!({}), Some(2), false).ok);
                assert_eq!(s.export("redo.hwpx"), edited);
            }
            "batch" => {
                let r=s.exec.call("command.batch",json!({"steps":[{"command":cmd,"params":p},{"command":cmd,"params":s.params(cmd)}]}),Some(0),false);
                assert!(r.ok, "{r:?}");
                assert_eq!(r.result.unwrap()["afterVersion"], 1);
                s.export("edit.hwpx");
                assert!(s.exec.call("history.undo", json!({}), Some(1), false).ok);
                assert_eq!(s.export("undo.hwpx"), SOURCE);
            }
            _ => unreachable!(),
        }
    }
}
macro_rules! cases {($($name:ident,$cmd:literal,$mode:literal;)*)=>{$(#[test] fn $name(){Case::exercise($cmd,$mode);})*};}
cases! {
    columns_normal,"table.setColumnWidths","normal"; columns_reject,"table.setColumnWidths","reject"; columns_dry_run,"table.setColumnWidths","dry"; columns_undo_redo_exact,"table.setColumnWidths","undo"; columns_batch,"table.setColumnWidths","batch";
    rows_normal,"table.setRowHeights","normal"; rows_reject,"table.setRowHeights","reject"; rows_dry_run,"table.setRowHeights","dry"; rows_undo_redo_exact,"table.setRowHeights","undo"; rows_batch,"table.setRowHeights","batch";
    format_normal,"cell.setFormat","normal"; format_reject,"cell.setFormat","reject"; format_dry_run,"cell.setFormat","dry"; format_undo_redo_exact,"cell.setFormat","undo"; format_batch,"cell.setFormat","batch";
    size_normal,"cell.setSize","normal"; size_reject,"cell.setSize","reject"; size_dry_run,"cell.setSize","dry"; size_undo_redo_exact,"cell.setSize","undo"; size_batch,"cell.setSize","batch";
}
#[test]
fn structure_changes_are_explicitly_unsupported() {
    let mut s = Case::new();
    // `table.insertRow`/`deleteRow`/`insertColumn`/`deleteColumn` exist since
    // 0.3 (tests/table_structure.rs); only other spellings are refused.
    for cmd in [
        "table.addRow",
        "table.addColumn",
        "table.insertRows",
        "table.deleteRows",
        "table.insertColumns",
        "table.deleteColumns",
    ] {
        let r = s.exec.call(cmd, json!({"target":s.table}), Some(0), false);
        assert!(!r.ok);
        assert_eq!(r.diagnostics[0].code, Code::Unsupported, "{cmd}: {r:?}");
        assert_eq!(
            r.diagnostics[0].reason.as_deref(),
            Some("structure_change_unsupported")
        );
    }
    assert_eq!(s.export("unchanged.hwpx"), SOURCE);
}

#[test]
fn merged_tracks_sum_without_creating_nodes() {
    let mut s = Case::new();
    let q = s
        .exec
        .call("element.query", json!({"kind":"table"}), None, false)
        .result
        .unwrap();
    let merged = q["matches"][0]["nodeId"].clone();
    let vertical = q["matches"][2]["nodeId"].clone();
    let before = s
        .exec
        .call("document.inspect", json!({"limit":1000}), None, false)
        .result
        .unwrap();
    let r = s.exec.call(
        "command.batch",
        json!({"steps":[
            {"command":"table.setColumnWidths","params":{"target":merged,"widths":[19000,24000]}},
            {"command":"table.setRowHeights","params":{"target":vertical,"heights":[2600,2900]}}
        ]}),
        Some(0),
        false,
    );
    assert!(r.ok, "{r:?}");
    let after = s
        .exec
        .call("document.inspect", json!({"limit":1000}), None, false)
        .result
        .unwrap();
    let ids = |v: &Value| {
        v["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["nodeId"].clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(&before), ids(&after));
    let out = s.export("merged.hwpx");
    let doc = Document::open(out, &Limits::default()).unwrap();
    let xml = &doc.xml["Contents/section0.xml"];
    assert!(String::from_utf8_lossy(xml.bytes()).contains("width=\"43000\" height=\"2400\""));
    assert!(String::from_utf8_lossy(xml.bytes()).contains("width=\"21000\" height=\"5500\""));
}
#[test]
fn merged_direct_cell_size_rejected_with_reason() {
    let mut s = Case::new();
    let tables = s
        .exec
        .call("element.query", json!({"kind":"table"}), None, false)
        .result
        .unwrap();
    let q = s
        .exec
        .call(
            "element.query",
            json!({"table":tables["matches"][0]["nodeId"],"row":0,"col":0}),
            None,
            false,
        )
        .result
        .unwrap();
    let r = s.exec.call(
        "cell.setSize",
        json!({"target":q["matches"][0]["nodeId"],"width":43000}),
        Some(0),
        false,
    );
    assert!(!r.ok);
    assert_eq!(
        r.diagnostics[0].reason.as_deref(),
        Some("merged_cell_size_unsupported")
    );
    assert_eq!(s.export("no.hwpx"), SOURCE);
}
#[test]
fn backgrounds_reuse_existing_resources_and_none_preserves_borders() {
    let mut s = Case::new();
    for (revision, background, id) in [(0, "#aabbcc", "4"), (1, "none", "3")] {
        let r = s.exec.call(
            "cell.setFormat",
            json!({"target":s.cell,"background":background}),
            Some(revision),
            false,
        );
        assert!(r.ok, "{r:?}");
        let out = s.export(&format!("color-{revision}.hwpx"));
        let archive = hae_zip::Archive::open(out, &Limits::default()).unwrap();
        let source = hae_zip::Archive::open(SOURCE.to_vec(), &Limits::default()).unwrap();
        assert_eq!(
            archive.raw_data("Contents/header.xml"),
            source.raw_data("Contents/header.xml")
        );
        let d = Document::open(
            hae_zip::write(&archive, &hae_zip::keep_all(&archive)).unwrap(),
            &Limits::default(),
        )
        .unwrap();
        let cell = d
            .nodes
            .iter()
            .filter(|n| n.kind == Kind::Cell)
            .nth(3)
            .unwrap();
        assert_eq!(
            d.doc_of(cell).el(cell.el).attr_value("borderFillIDRef"),
            Some(id)
        );
    }
}
#[test]
fn missing_background_resource_is_an_explicit_rejection() {
    let mut s = Case::new();
    let r = s.exec.call(
        "cell.setFormat",
        json!({"target":s.cell,"background":"#123456"}),
        Some(0),
        false,
    );
    assert!(!r.ok);
    assert_eq!(
        r.diagnostics[0].reason.as_deref(),
        Some("background_resource_unavailable")
    );
    assert_eq!(s.export("no.hwpx"), SOURCE);
}
#[test]
fn malformed_dimensions_and_margin_minimum_are_atomic() {
    let mut s = Case::new();
    for (command, p) in [
        (
            "table.setColumnWidths",
            json!({"target":s.table,"width":20000,"widths":[21000,21000]}),
        ),
        (
            "table.setColumnWidths",
            json!({"target":s.table,"widths":[2147483647,2147483647]}),
        ),
        (
            "table.setRowHeights",
            json!({"target":s.table,"heights":[282,2400]}),
        ),
        ("cell.setSize", json!({"target":s.cell})),
        (
            "cell.setFormat",
            json!({"target":s.cell,"cellMargin":{"left":21000}}),
        ),
        (
            "cell.setFormat",
            json!({"target":s.cell,"background":"#oops"}),
        ),
    ] {
        let r = s.exec.call(command, p, Some(0), false);
        assert!(!r.ok, "{command}: {r:?}");
    }
    assert_eq!(s.export("no.hwpx"), SOURCE);
}
#[test]
fn failed_structure_batch_rolls_back_earlier_dimensions() {
    let mut s = Case::new();
    let r = s.exec.call(
        "command.batch",
        json!({"steps":[
            {"command":"table.setColumnWidths","params":s.params("table.setColumnWidths")},
            {"command":"table.insertRows","params":{"target":s.table,"count":1}}
        ]}),
        Some(0),
        false,
    );
    assert!(!r.ok);
    assert_eq!(
        r.diagnostics[0].reason.as_deref(),
        Some("structure_change_unsupported")
    );
    assert_eq!(r.diagnostics[0].detail.as_ref().unwrap()["failedStep"], 1);
    assert_eq!(s.export("no.hwpx"), SOURCE);
}
#[test]
fn table_noop_keeps_history_empty_and_uniform_tracks_work() {
    let mut s = Case::new();
    let r = s.exec.call(
        "table.setColumnWidths",
        json!({"target":s.table,"width":21000}),
        Some(0),
        false,
    );
    assert!(r.ok, "{r:?}");
    assert_eq!(r.result.unwrap()["outcome"], "noop");
    assert!(!s.exec.call("history.undo", json!({}), Some(0), false).ok);
    for (rev, cmd, p) in [
        (
            0,
            "table.setColumnWidths",
            json!({"target":s.table,"width":23000}),
        ),
        (
            1,
            "table.setRowHeights",
            json!({"target":s.table,"height":2800}),
        ),
    ] {
        let r = s.exec.call(cmd, p, Some(rev), false);
        assert!(r.ok, "{r:?}");
    }
    s.export("uniform.hwpx");
}
#[test]
fn untouched_entries_raw_compression_headers_and_preview_survive() {
    let mut s = Case::new();
    let r = s
        .exec
        .call("cell.setSize", s.params("cell.setSize"), Some(0), false);
    assert!(r.ok, "{r:?}");
    let out = s.export("out.hwpx");
    let original = hae_zip::Archive::open(SOURCE.to_vec(), &Limits::default()).unwrap();
    let after = hae_zip::Archive::open(out, &Limits::default()).unwrap();
    for e in original
        .entries()
        .iter()
        .filter(|e| e.name != "Contents/section0.xml")
    {
        assert_eq!(original.raw_data(&e.name), after.raw_data(&e.name));
        assert_eq!(
            original.raw_local_header(&e.name),
            after.raw_local_header(&e.name)
        );
    }
}
