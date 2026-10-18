use hae_diag::Limits;
use hae_exec::SessionExecutor;
use serde_json::{json, Value};
use std::{fs, path::PathBuf};
const INPUT: &[u8] = include_bytes!("../../../tests/fixtures/editing/narrow_edits.hwpx");
struct Harness {
    root: PathBuf,
    exec: SessionExecutor,
}
impl Harness {
    fn new() -> Self {
        Self::from_bytes(INPUT)
    }
    fn from_bytes(input: &[u8]) -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target")
            .join(hae_model::new_id("narrow-").unwrap());
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("input.hwpx"), input).unwrap();
        let root = root.canonicalize().unwrap();
        let mut exec = SessionExecutor::new(&root, &root, Limits::default()).unwrap();
        let r = exec.call(
            "document.open",
            json!({"path":root.join("input.hwpx")}),
            None,
            false,
        );
        assert!(r.ok, "{:?}", r.diagnostics);
        Self { root, exec }
    }
    fn query(&mut self, kind: &str) -> Vec<Value> {
        self.exec
            .call("element.query", json!({"kind":kind}), None, false)
            .result
            .unwrap()["matches"]
            .as_array()
            .unwrap()
            .clone()
    }
    fn params(&mut self, cmd: &str) -> Value {
        match cmd {
            "paragraph.setStyle" => {
                json!({"target":self.query("paragraph")[0]["nodeId"],"styleId":1})
            }
            "image.fit" => json!({"target":self.query("picture")[0]["nodeId"],"mode":"contain"}),
            _ => json!({"kind":"header","text":"Changed & 한글"}),
        }
    }
    fn export(&mut self, name: &str) -> Vec<u8> {
        let path = self.root.join(name);
        let r = self
            .exec
            .call("document.export", json!({"path":path}), None, false);
        assert!(r.ok, "{:?}", r.diagnostics);
        fs::read(path).unwrap()
    }
    fn apply(&mut self, cmd: &str, params: Value, dry: bool) {
        let r = self
            .exec
            .call(cmd, params, self.exec.current_revision(), dry);
        assert!(r.ok, "{cmd}: {:?}", r.diagnostics);
    }
}
impl Drop for Harness {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn success(cmd: &str) {
    let mut h = Harness::new();
    let p = h.params(cmd);
    h.apply(cmd, p, false);
    assert_ne!(h.export("applied.hwpx"), INPUT);
}
fn dry(cmd: &str) {
    let mut h = Harness::new();
    let p = h.params(cmd);
    h.apply(cmd, p, true);
    assert_eq!(h.exec.current_revision(), Some(0));
    assert_eq!(h.export("dry.hwpx"), INPUT);
}
fn undo(cmd: &str) {
    let mut h = Harness::new();
    let p = h.params(cmd);
    h.apply(cmd, p, false);
    h.apply("history.undo", json!({}), false);
    assert_eq!(h.export("undo.hwpx"), INPUT);
}
fn batch(cmd: &str) {
    let mut h = Harness::new();
    let p = h.params(cmd);
    let q = h.query("paragraph")[1]["nodeId"].clone();
    h.apply("command.batch",json!({"steps":[{"command":cmd,"params":p},{"command":"text.replace","params":{"target":q,"range":{"start":0,"end":4},"text":"Batch"}}]}),false);
    assert_eq!(h.exec.current_revision(), Some(1));
    assert_ne!(h.export("batch.hwpx"), INPUT);
    h.apply("history.undo", json!({}), false);
    assert_eq!(h.export("undo.hwpx"), INPUT);
}
#[test]
fn style_success() {
    success("paragraph.setStyle");
}
#[test]
fn style_dry_run() {
    dry("paragraph.setStyle");
}
#[test]
fn style_undo_exact() {
    undo("paragraph.setStyle");
}
#[test]
fn style_batch() {
    batch("paragraph.setStyle");
}
#[test]
fn style_rejects_missing_id() {
    let mut h = Harness::new();
    let mut p = h.params("paragraph.setStyle");
    p["styleId"] = json!(99);
    let r = h.exec.call("paragraph.setStyle", p, Some(0), false);
    assert!(!r.ok);
    assert_eq!(r.diagnostics[0].code, hae_diag::Code::TargetNotFound);
    assert_eq!(h.export("rejected.hwpx"), INPUT);
}
#[test]
fn fit_success() {
    success("image.fit");
}
#[test]
fn fit_dry_run() {
    dry("image.fit");
}
#[test]
fn fit_undo_exact() {
    undo("image.fit");
}
#[test]
fn fit_batch() {
    batch("image.fit");
}
#[test]
fn fit_rejects_invalid_mode() {
    let mut h = Harness::new();
    let mut p = h.params("image.fit");
    p["mode"] = json!("include");
    assert!(!h.exec.call("image.fit", p, Some(0), false).ok);
    assert_eq!(h.export("rejected.hwpx"), INPUT);
}
#[test]
fn header_success() {
    success("header.setText");
}
#[test]
fn header_dry_run() {
    dry("header.setText");
}
#[test]
fn header_undo_exact() {
    undo("header.setText");
}
#[test]
fn header_batch() {
    batch("header.setText");
}
#[test]
fn header_rejects_number_control() {
    let mut h = Harness::new();
    let r = h.exec.call(
        "header.setText",
        json!({"kind":"header","paragraphIndex":2,"text":"Oops"}),
        Some(0),
        false,
    );
    assert!(!r.ok);
    assert_eq!(r.diagnostics[0].reason.as_deref(), Some("header_control"));
    assert_eq!(h.export("rejected.hwpx"), INPUT);
}

fn geometry(mode: &str, width: &str, height: &str, left: &str, right: &str) {
    let mut h = Harness::new();
    let mut p = h.params("image.fit");
    p["mode"] = json!(mode);
    h.apply("image.fit", p, false);
    let bytes = h.export("geometry.hwpx");
    let d = hae_model::Document::open(bytes, &Limits::default()).unwrap();
    let n = d
        .nodes
        .iter()
        .find(|n| n.kind == hae_model::Kind::Picture)
        .unwrap();
    let xml = d.doc_of(n);
    let el = |name: &str| {
        xml.el(*xml
            .el(n.el)
            .children
            .iter()
            .find(|&&i| xml.el(i).local == name)
            .unwrap())
    };
    assert_eq!(el("curSz").attr_value("width"), Some(width));
    assert_eq!(el("curSz").attr_value("height"), Some(height));
    assert_eq!(el("sz").attr_value("width"), Some(width));
    assert_eq!(el("sz").attr_value("height"), Some(height));
    assert_eq!(el("imgDim").attr_value("dimwidth"), Some("150"));
    assert_eq!(el("imgDim").attr_value("dimheight"), Some("75"));
    assert_eq!(el("imgClip").attr_value("left"), Some(left));
    assert_eq!(el("imgClip").attr_value("right"), Some(right));
    assert_eq!(el("imgClip").attr_value("top"), Some("0"));
    assert_eq!(el("imgClip").attr_value("bottom"), Some("75"));
    let pt = xml.el(*el("imgRect")
        .children
        .iter()
        .find(|&&i| xml.el(i).local == "pt2")
        .unwrap());
    assert_eq!(pt.attr_value("x"), el("orgSz").attr_value("width"));
    assert_eq!(pt.attr_value("y"), el("orgSz").attr_value("height"));
    assert_eq!(el("renderingInfo").children.len(), 3);
    let a = hae_zip::Archive::open(INPUT.to_vec(), &Limits::default()).unwrap();
    for entry in a.entries() {
        if entry.name != "Contents/section0.xml" {
            let out = &d.original;
            assert_eq!(a.raw_data(&entry.name), out.raw_data(&entry.name));
            assert_eq!(
                a.raw_local_header(&entry.name),
                out.raw_local_header(&entry.name)
            );
        }
    }
}
#[test]
fn fit_contain_geometry_uses_hwpunit_and_full_clip() {
    geometry("contain", "1200", "600", "0", "150");
}
#[test]
fn fit_cover_geometry_center_crops_source_units() {
    geometry("cover", "1200", "1200", "37", "112");
}
#[test]
fn fit_stretch_geometry_keeps_frame_and_full_clip() {
    geometry("stretch", "1200", "1200", "0", "150");
}
#[test]
fn style_targets_change_all_selected_paragraphs() {
    let mut h = Harness::new();
    let ids = h.query("paragraph");
    h.apply(
        "paragraph.setStyle",
        json!({"targets":[ids[0]["nodeId"],ids[1]["nodeId"]],"styleId":1}),
        false,
    );
    let d = hae_model::Document::open(h.export("many.hwpx"), &Limits::default()).unwrap();
    let refs: Vec<_> = d
        .nodes
        .iter()
        .filter(|n| n.kind == hae_model::Kind::Paragraph)
        .map(|n| d.doc_of(n).el(n.el).attr_value("styleIDRef").unwrap())
        .collect();
    assert_eq!(&refs[..2], &["1", "1"]);
    assert!(refs[2..].iter().all(|&r| r == "0"));
}
#[test]
fn style_rejects_empty_or_conflicting_target_forms() {
    let mut h = Harness::new();
    let target = h.query("paragraph")[0]["nodeId"].clone();
    for p in [
        json!({"styleId":1}),
        json!({"targets":[],"styleId":1}),
        json!({"target":target,"targets":[target],"styleId":1}),
        json!({"target":target,"styleId":-1}),
    ] {
        assert!(!h.exec.call("paragraph.setStyle", p, Some(0), false).ok);
    }
    assert_eq!(h.export("reject.hwpx"), INPUT);
}
#[test]
fn header_footer_scope_and_paragraph_index() {
    let mut h = Harness::new();
    let scope = h.query("section")[0]["nodeId"].clone();
    h.apply(
        "header.setText",
        json!({"scope":scope,"kind":"header","paragraphIndex":1,"text":"Second"}),
        false,
    );
    h.apply(
        "header.setText",
        json!({"kind":"footer","text":"Foot"}),
        false,
    );
    let d = hae_model::Document::open(h.export("hf.hwpx"), &Limits::default()).unwrap();
    let texts: Vec<_> = d
        .nodes
        .iter()
        .filter(|n| n.kind == hae_model::Kind::Paragraph)
        .map(|n| d.node_text(n))
        .collect();
    assert!(texts.contains(&"Second".into()));
    assert!(texts.contains(&"Foot".into()));
    assert!(texts.contains(&"Header A".into()));
}
#[test]
fn header_rejects_missing_paragraph_and_margin_control() {
    let mut h = Harness::new();
    for (index, code) in [
        (99, hae_diag::Code::TargetNotFound),
        (3, hae_diag::Code::Unsupported),
    ] {
        let r = h.exec.call(
            "header.setText",
            json!({"kind":"header","paragraphIndex":index,"text":"x"}),
            Some(0),
            false,
        );
        assert!(!r.ok);
        assert_eq!(r.diagnostics[0].code, code);
    }
    assert_eq!(h.export("reject.hwpx"), INPUT);
}
#[test]
fn all_three_commands_roll_back_with_failed_batch_step() {
    for cmd in ["paragraph.setStyle", "image.fit", "header.setText"] {
        let mut h = Harness::new();
        let p = h.params(cmd);
        let r=h.exec.call("command.batch",json!({"steps":[{"command":cmd,"params":p},{"command":"paragraph.setStyle","params":{"target":"missing","styleId":1}}]}),Some(0),false);
        assert!(!r.ok);
        assert_eq!(h.exec.current_revision(), Some(0));
        assert_eq!(h.export("rollback.hwpx"), INPUT);
    }
}
#[test]
fn image_replace_then_fit_in_same_batch_uses_new_media() {
    let mut h = Harness::new();
    let target = h.query("picture")[0]["nodeId"].clone();
    let path = h.root.join("new.jpg");
    fs::write(
        &path,
        include_bytes!("../../../tests/fixtures/editing/inputs/ok.jpg"),
    )
    .unwrap();
    h.apply("command.batch",json!({"steps":[{"command":"image.replace","params":{"target":target,"imagePath":path}},{"command":"image.fit","params":{"target":target,"mode":"contain"}}]}),false);
    assert_ne!(h.export("replace-fit.hwpx"), INPUT);
}

#[test]
fn fit_real_corpus_three_modes_export_and_exact_undo() {
    let source = include_bytes!("../../../tests/fixtures/corpus/gyehoek-reference.hwpx");
    let artifacts = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/evidence");
    fs::create_dir_all(&artifacts).unwrap();
    for mode in ["contain", "cover", "stretch"] {
        let mut h = Harness::from_bytes(source);
        let target = h.query("picture")[0]["nodeId"].clone();
        h.apply("image.fit", json!({"target":target,"mode":mode}), false);
        let bytes = h.export("fitted.hwpx");
        fs::write(artifacts.join(format!("corpus-fit-{mode}.hwpx")), bytes).unwrap();
        if h.exec.current_revision() != Some(0) {
            h.apply("history.undo", json!({}), false);
        }
        assert_eq!(h.export("undo.hwpx"), source);
    }
}
#[test]
fn narrow_commands_discovery_policies_and_schemas() {
    let mut h = Harness::new();
    for cmd in ["paragraph.setStyle", "image.fit", "header.setText"] {
        let r = h
            .exec
            .call("command.describe", json!({"command":cmd}), None, false);
        assert!(r.ok);
        let d = r.result.unwrap();
        for key in [
            "mutatesContent",
            "requiresExpectedVersion",
            "supportsDryRun",
            "recordsHistory",
        ] {
            assert_eq!(d[key], true, "{cmd}: {key}");
        }
        assert_eq!(d["inputSchema"]["additionalProperties"], false);
    }
}

#[test]
fn header_absence_and_wrong_scope_are_rejected_without_creation() {
    let mut h = Harness::from_bytes(include_bytes!(
        "../../../tests/fixtures/editing/char_format.hwpx"
    ));
    let r = h.exec.call(
        "header.setText",
        json!({"kind":"header","text":"x"}),
        Some(0),
        false,
    );
    assert!(!r.ok);
    assert_eq!(r.diagnostics[0].code, hae_diag::Code::TargetNotFound);
    let target = h.query("paragraph")[0]["nodeId"].clone();
    let r = h.exec.call(
        "header.setText",
        json!({"scope":target,"kind":"footer","text":"x"}),
        Some(0),
        false,
    );
    assert!(!r.ok);
    assert_eq!(r.diagnostics[0].reason.as_deref(), Some("wrong_kind"));
    assert_eq!(
        h.export("absent.hwpx"),
        include_bytes!("../../../tests/fixtures/editing/char_format.hwpx")
    );
}

#[test]
fn narrow_commands_redo_reuses_exact_verified_state() {
    for cmd in ["paragraph.setStyle", "image.fit", "header.setText"] {
        let mut h = Harness::new();
        let p = h.params(cmd);
        h.apply(cmd, p, false);
        let applied = h.export("applied.hwpx");
        h.apply("history.undo", json!({}), false);
        h.apply("history.redo", json!({}), false);
        assert_eq!(h.export("redo.hwpx"), applied);
    }
}

#[test]
fn image_fit_missing_matrix_is_refused_without_repair() {
    let archive = hae_zip::Archive::open(INPUT.to_vec(), &Limits::default()).unwrap();
    let mut state = hae_package::PackageState::from_archive(&archive);
    let source = std::str::from_utf8(state.get("Contents/section0.xml").unwrap()).unwrap();
    let source = source.replace(
        "<hc:rotMatrix e1=\"1\" e2=\"0\" e3=\"0\" e4=\"0\" e5=\"1\" e6=\"0\"/>",
        "",
    );
    state.set(
        "Contents/section0.xml",
        std::sync::Arc::from(source.into_bytes()),
    );
    let input =
        hae_zip::write(&archive, &hae_package::zip_plan(&archive, &state).unwrap()).unwrap();
    let mut h = Harness::from_bytes(&input);
    let p = h.params("image.fit");
    let r = h.exec.call("image.fit", p, Some(0), false);
    assert!(!r.ok);
    assert_eq!(r.diagnostics[0].reason.as_deref(), Some("image_geometry"));
    assert_eq!(h.export("matrix-missing.hwpx"), input);
}

#[test]
fn header_does_not_inherit_unrelated_body_opaque_reference_lock() {
    let a = hae_zip::Archive::open(INPUT.to_vec(), &Limits::default()).unwrap();
    let mut st = hae_package::PackageState::from_archive(&a);
    let source = std::str::from_utf8(st.get("Contents/section0.xml").unwrap()).unwrap();
    let changed = source
        .replacen(
            "<hp:p paraPrIDRef=\"0\"",
            "<hp:p id=\"body-locked\" paraPrIDRef=\"0\"",
            1,
        )
        .replace(
            "</hs:sec>",
            "<x:keep xmlns:x=\"urn:opaque\" ref=\"body-locked\"/></hs:sec>",
        );
    st.set(
        "Contents/section0.xml",
        std::sync::Arc::from(changed.into_bytes()),
    );
    let input = hae_zip::write(&a, &hae_package::zip_plan(&a, &st).unwrap()).unwrap();
    let mut h = Harness::from_bytes(&input);
    h.apply(
        "header.setText",
        json!({"kind":"header","text":"Allowed"}),
        false,
    );
    h.export("header-safe.hwpx");
}
