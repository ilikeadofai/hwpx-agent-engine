//! Report editing gaps exercised through the real executor and save verifier.
use hae_diag::Limits;
use hae_exec::{ResponseEnvelope, SessionExecutor};
use hae_model::{Document, Kind};
use hae_verify::{verify_archive, VerifyOperation};
use serde_json::{json, Value};
use std::{fs, path::PathBuf, sync::Arc};

const PNG: &[u8] = include_bytes!("../../../tests/fixtures/editing/inputs/ok.png");

struct Case {
    exec: SessionExecutor,
    root: PathBuf,
    version: u64,
    serial: usize,
}
impl Drop for Case {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
impl Case {
    fn new() -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../spikes/work/object-creation/test-runs")
            .join(hae_model::new_id("case-").unwrap());
        fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let mut exec = SessionExecutor::new(&root, &root, Limits::default()).unwrap();
        let source = root.join("source.hwpx");
        let r = exec.call(
            "document.create",
            json!({"path":source,"paragraphs":[
                "section-owner", "table-anchor", "equation-anchor", "untouched-sibling",
                "[그림1]", "[그림2]", "[그림3]", "[그림4]", "[그림5]", "[그림6]"
            ]}),
            None,
            false,
        );
        assert!(r.ok, "create: {:?}", r.diagnostics);
        let r = exec.call("document.open", json!({"path":source}), None, false);
        assert!(r.ok, "open: {:?}", r.diagnostics);
        fs::write(root.join("figure.png"), PNG).unwrap();
        Self {
            exec,
            root,
            version: 0,
            serial: 0,
        }
    }
    fn call(&mut self, command: &str, params: Value) -> Value {
        let r = self.exec.call(command, params, None, false);
        assert!(r.ok, "{command}: {:?}", r.diagnostics);
        r.result.unwrap()
    }
    fn edit(&mut self, command: &str, params: Value) -> ResponseEnvelope {
        let r = self.exec.call(command, params, Some(self.version), false);
        if r.ok && r.result.as_ref().is_some_and(|v| v["outcome"] == "applied") {
            self.version += 1;
        }
        r
    }
    fn ok(&mut self, command: &str, params: Value) -> Value {
        let r = self.edit(command, params);
        assert!(r.ok, "{command}: {:?}", r.diagnostics);
        r.result.unwrap()
    }
    fn matches(&mut self, kind: &str) -> Vec<Value> {
        self.call("element.query", json!({"kind":kind}))["matches"]
            .as_array()
            .unwrap()
            .clone()
    }
    fn paragraph(&mut self, text: &str) -> String {
        let hits: Vec<Value> = self
            .matches("paragraph")
            .into_iter()
            .filter(|m| m["textPreview"] == text)
            .collect();
        assert_eq!(hits.len(), 1, "paragraph {text:?}: {hits:?}");
        hits[0]["nodeId"].as_str().unwrap().to_string()
    }
    fn export(&mut self) -> Vec<u8> {
        self.serial += 1;
        let path = self.root.join(format!("export-{}.hwpx", self.serial));
        self.call("document.export", json!({"path":path}));
        fs::read(path).unwrap()
    }
    fn doc(&mut self) -> Document {
        Document::open(self.export(), &Limits::default()).unwrap()
    }
    fn table_params(target: &str) -> Value {
        json!({"target":target,"position":"after","rows":2,"columns":2,
            "width":28000,"rowHeight":2400,"cells":[["R1C1","R1C2"],["R2C1","R2C2"]]})
    }
}

#[test]
fn new_document_creates_all_three_object_kinds_without_templates() {
    let mut c = Case::new();
    for kind in ["table", "picture", "equation"] {
        assert!(c.matches(kind).is_empty());
    }
    let table_anchor = c.paragraph("table-anchor");
    let equation_anchor = c.paragraph("equation-anchor");
    let caption = c.paragraph("[그림1]");
    c.ok("table.insert", Case::table_params(&table_anchor));
    c.ok(
        "equation.insert",
        json!({"target":equation_anchor,"position":"after",
        "script":"a over b + sqrt {x}","width":12000,"height":2400}),
    );
    c.ok(
        "image.insert",
        json!({"target":caption,"position":"before",
        "path":c.root.join("figure.png"),"width":18000,"height":9000}),
    );
    assert_eq!(c.matches("table").len(), 1);
    assert_eq!(c.matches("equation").len(), 1);
    assert_eq!(c.matches("picture").len(), 1);
    assert_eq!(c.matches("cell").len(), 4);
    assert_eq!(c.paragraph("table-anchor"), table_anchor);
    assert_eq!(c.paragraph("[그림1]"), caption);
    // New objects are ordinary editable nodes in subsequent commands.
    let cell = c.matches("cell")[0]["nodeId"].as_str().unwrap().to_string();
    c.ok("cell.setText", json!({"target":cell,"text":"수정한 결과"}));
    let equation = c.matches("equation")[0]["nodeId"]
        .as_str()
        .unwrap()
        .to_string();
    c.ok(
        "equation.setScript",
        json!({"target":equation,"script":"int _ 0 ^ 1 x ^ 2 dx"}),
    );
    let doc = c.doc();
    assert_eq!(
        doc.nodes.iter().filter(|n| n.kind == Kind::Table).count(),
        1
    );
    assert_eq!(
        doc.nodes
            .iter()
            .filter(|n| n.kind == Kind::Equation)
            .count(),
        1
    );
    assert_eq!(
        doc.nodes.iter().filter(|n| n.kind == Kind::Picture).count(),
        1
    );
    assert!(doc.state.entries.iter().any(|e| e.payload.as_ref() == PNG));
    c.call("project.save", json!({"path":c.root.join("saved.hwpwork")}));
    c.call("document.close", json!({}));
    c.call(
        "document.open",
        json!({"path":c.root.join("saved.hwpwork")}),
    );
    c.call("element.get", json!({"target":equation}));
    c.call("element.get", json!({"target":cell}));
    c.call("element.get", json!({"target":table_anchor}));
}

#[test]
fn six_figures_are_inserted_immediately_before_their_captions() {
    let mut c = Case::new();
    let original = c.export();
    let steps: Vec<Value> = (1..=6)
        .map(|i| {
            let target = c.paragraph(&format!("[그림{i}]"));
            json!({"command":"image.insert","params":{"target":target,"position":"before",
            "path":c.root.join("figure.png"),"width":18000,"height":9000}})
        })
        .collect();
    c.ok("command.batch", json!({"steps":steps}));
    assert_eq!(c.matches("picture").len(), 6);
    let edited = c.export();
    let doc = Document::open(edited.clone(), &Limits::default()).unwrap();
    let xml = &doc.xml[&doc.info.sections[0]];
    let paragraphs: Vec<usize> = xml
        .el(xml.root())
        .children
        .iter()
        .copied()
        .filter(|&i| xml.el(i).is(hae_package::NS_HP, "p"))
        .collect();
    for i in 1..=6 {
        let caption = format!("[그림{i}]");
        let at = paragraphs
            .iter()
            .position(|&p| {
                xml.raw(xml.el(p).outer())
                    .windows(caption.len())
                    .any(|w| w == caption.as_bytes())
            })
            .unwrap();
        assert!(at > 0);
        assert_eq!(
            xml.descendants(paragraphs[at - 1])
                .filter(|&e| xml.el(e).is(hae_package::NS_HP, "pic"))
                .count(),
            1
        );
    }
    c.ok("history.undo", json!({}));
    assert_eq!(c.export(), original);
    c.ok("history.redo", json!({}));
    assert_eq!(c.export(), edited);
}

#[test]
fn table_delete_preserves_carrier_section_properties_and_outside_text() {
    let mut c = Case::new();
    let owner = c.paragraph("section-owner");
    c.ok("table.insert", Case::table_params(&owner));
    let table = c.matches("table")[0]["nodeId"]
        .as_str()
        .unwrap()
        .to_string();
    let before = c.export();
    let carrier_ids: Vec<String> = c
        .matches("paragraph")
        .into_iter()
        .filter_map(|m| m["nodeId"].as_str().map(str::to_owned))
        .collect();
    c.ok("table.delete", json!({"target":table}));
    assert!(c.matches("table").is_empty());
    assert!(c.matches("cell").is_empty());
    assert_eq!(c.paragraph("section-owner"), owner);
    c.paragraph("untouched-sibling");
    let doc = c.doc();
    assert_eq!(
        doc.xml[&doc.info.sections[0]]
            .iter()
            .filter(|(_, e)| e.is(hae_package::NS_HP, "secPr"))
            .count(),
        1
    );
    // Only cell paragraphs vanish; outer carriers and all report text survive.
    assert_eq!(c.matches("paragraph").len(), carrier_ids.len() - 4);
    c.ok("history.undo", json!({}));
    assert_eq!(c.export(), before);
    c.call("element.get", json!({"target":table}));
}

#[test]
fn table_delete_keeps_secpr_and_mixed_text_in_the_same_carrier() {
    let mut c = Case::new();
    let anchor = c.paragraph("table-anchor");
    c.ok("table.insert", Case::table_params(&anchor));
    c.ok(
        "equation.insert",
        json!({"target":anchor,"position":"after",
        "script":"a over b","width":12000,"height":2400}),
    );
    let doc = c.doc();
    let table = doc.nodes.iter().find(|n| n.kind == Kind::Table).unwrap();
    let xml = doc.doc_of(table);
    let carrier = xml
        .ancestors(table.el)
        .find(|&p| xml.el(p).is(hae_package::NS_HP, "p"))
        .unwrap();
    let equation = doc.nodes.iter().find(|n| n.kind == Kind::Equation).unwrap();
    let equation_carrier = xml
        .ancestors(equation.el)
        .find(|&p| xml.el(p).is(hae_package::NS_HP, "p"))
        .unwrap();
    let mut objects = xml.raw(xml.el(table.el).outer()).to_vec();
    objects.extend_from_slice(xml.raw(xml.el(equation.el).outer()));
    let owner = xml
        .el(xml.root())
        .children
        .iter()
        .copied()
        .find(|&p| xml.el(p).is(hae_package::NS_HP, "p"))
        .unwrap();
    let run = xml
        .el(owner)
        .children
        .iter()
        .copied()
        .find(|&p| xml.el(p).is(hae_package::NS_HP, "run"))
        .unwrap();
    let at = xml.el(run).inner().end;
    // Fixture preparation only: move the generated table into the native
    // secPr-owning paragraph to challenge deletion of a mixed carrier.
    let moved = xml
        .splice(
            vec![
                (xml.el(carrier).outer(), vec![]),
                (xml.el(equation_carrier).outer(), vec![]),
                (at..at, objects),
            ],
            Default::default(),
        )
        .unwrap();
    let mut state = doc.state.clone();
    assert!(state.set(&table.entry, moved.bytes().clone()));
    let plan = hae_package::zip_plan(&doc.original, &state).unwrap();
    let source = hae_zip::write(&doc.original, &plan).unwrap();
    let path = c.root.join("mixed-carrier.hwpx");
    fs::write(&path, &source).unwrap();
    c.call("document.close", json!({}));
    c.call("document.open", json!({"path":path}));
    c.version = 0;
    let original = Document::open(source.clone(), &Limits::default()).unwrap();
    let table = original
        .nodes
        .iter()
        .find(|n| n.kind == Kind::Table)
        .unwrap();
    let expected = original
        .doc_of(table)
        .splice(
            vec![(original.doc_of(table).el(table.el).outer(), vec![])],
            Default::default(),
        )
        .unwrap();
    let table_id = c.matches("table")[0]["nodeId"]
        .as_str()
        .unwrap()
        .to_string();
    let equation_id = c.matches("equation")[0]["nodeId"]
        .as_str()
        .unwrap()
        .to_string();
    let owner_id = c
        .matches("paragraph")
        .into_iter()
        .find(|m| {
            m["textPreview"]
                .as_str()
                .is_some_and(|s| s.starts_with("section-owner"))
        })
        .unwrap()["nodeId"]
        .as_str()
        .unwrap()
        .to_string();
    c.ok("table.delete", json!({"target":table_id}));
    c.call("element.get", json!({"target":owner_id}));
    c.call("element.get", json!({"target":equation_id}));
    let actual = c.doc();
    assert_eq!(
        actual.state.get(&table.entry).unwrap().as_ref(),
        expected.bytes().as_ref()
    );
    for entry in &original.state.entries {
        if entry.name != table.entry {
            assert_eq!(
                actual.state.get(&entry.name),
                Some(&entry.payload),
                "{}",
                entry.name
            );
        }
    }
    c.ok("history.undo", json!({}));
    assert_eq!(c.export(), source);
}

#[test]
fn self_closing_empty_run_can_anchor_a_new_object() {
    let mut c = Case::new();
    let doc = c.doc();
    let paragraph = doc
        .nodes
        .iter()
        .find(|n| {
            n.kind == Kind::Paragraph && doc.para_text(n).is_ok_and(|p| p.text == "table-anchor")
        })
        .unwrap();
    let xml = doc.doc_of(paragraph);
    let run = xml
        .el(paragraph.el)
        .children
        .iter()
        .copied()
        .find(|&i| xml.el(i).is(hae_package::NS_HP, "run"))
        .unwrap();
    let empty = format!("<{} charPrIDRef=\"0\"/>", xml.el(run).qname).into_bytes();
    let new = xml
        .splice(vec![(xml.el(run).outer(), empty)], Default::default())
        .unwrap();
    let mut state = doc.state.clone();
    assert!(state.set(&paragraph.entry, new.bytes().clone()));
    let source = hae_zip::write(
        &doc.original,
        &hae_package::zip_plan(&doc.original, &state).unwrap(),
    )
    .unwrap();
    let path = c.root.join("self-closing.hwpx");
    fs::write(&path, &source).unwrap();
    c.call("document.close", json!({}));
    c.call("document.open", json!({"path":path}));
    c.version = 0;
    let target = c.paragraph("");
    c.ok(
        "equation.insert",
        json!({"target":target,"position":"after",
        "script":"a over b","width":12000,"height":2400}),
    );
    assert_eq!(c.matches("equation").len(), 1);
    c.export();
    c.ok("history.undo", json!({}));
    assert_eq!(c.export(), source);
}

#[test]
fn dry_run_and_failed_mixed_batch_leave_source_unchanged() {
    let mut c = Case::new();
    let target = c.paragraph("table-anchor");
    let before = c.export();
    let r = c
        .exec
        .call("table.insert", Case::table_params(&target), Some(0), true);
    assert!(r.ok, "dry run: {:?}", r.diagnostics);
    assert!(c.matches("table").is_empty());
    assert_eq!(c.export(), before);
    let r = c.edit(
        "command.batch",
        json!({"steps":[
            {"command":"table.insert","params":Case::table_params(&target)},
            {"command":"equation.insert","params":{"target":target,"position":"after",
                "script":"\\frac{a}{b}","width":12000,"height":2400}}
        ]}),
    );
    assert!(
        !r.ok,
        "unsupported LaTeX must roll back the valid table step"
    );
    assert_eq!(c.export(), before);
    assert_eq!(c.version, 0);
}

#[test]
fn insert_dimensions_and_shape_limits_fail_without_changes() {
    let mut c = Case::new();
    let target = c.paragraph("table-anchor");
    let before = c.export();
    for params in [
        json!({"target":target,"position":"after","rows":0,"columns":2,"width":20000,"rowHeight":2000}),
        json!({"target":target,"position":"after","rows":33,"columns":33,"width":20000,"rowHeight":2000}),
        json!({"target":target,"position":"after","rows":2,"columns":2,"width":1,"rowHeight":2000}),
        json!({"target":target,"position":"after","rows":2,"columns":2,"width":20000,"rowHeight":2000,"cells":[["bad"]]}),
    ] {
        let r = c.edit("table.insert", params);
        assert!(!r.ok, "invalid table was accepted");
        assert_ne!(r.diagnostics[0].code, hae_diag::Code::UnknownCommand);
    }
    for command in ["equation.insert", "image.insert"] {
        let mut params = json!({"target":target,"position":"after","width":0,"height":1000});
        if command == "image.insert" {
            params["path"] = json!(c.root.join("figure.png"));
        } else {
            params["script"] = json!("a over b");
        }
        let r = c.edit(command, params);
        assert!(!r.ok);
        assert_ne!(r.diagnostics[0].code, hae_diag::Code::UnknownCommand);
    }
    assert_eq!(c.export(), before);
}

#[test]
fn independent_verifier_rejects_outside_text_tampering_after_new_equation() {
    let mut c = Case::new();
    let target = c.paragraph("equation-anchor");
    let original = c.doc();
    let node = original
        .nodes
        .iter()
        .find(|n| {
            n.kind == Kind::Paragraph
                && original
                    .para_text(n)
                    .is_ok_and(|p| p.text == "equation-anchor")
        })
        .unwrap();
    let op: VerifyOperation = serde_json::from_value(json!({"op":"equation.insert",
        "entry":node.entry,"path":original.doc_of(node).path(node.el),"position":"after",
        "script":"a over b","width":12000,"height":2400}))
    .unwrap();
    c.ok(
        "equation.insert",
        json!({"target":target,"position":"after",
        "script":"a over b","width":12000,"height":2400}),
    );
    let valid = c.export();
    verify_archive(
        &original.original,
        &valid,
        std::slice::from_ref(&op),
        &Limits::default(),
    )
    .unwrap();
    let edited = Document::open(valid, &Limits::default()).unwrap();
    let entry = edited.info.sections[0].clone();
    let text = String::from_utf8(edited.state.get(&entry).unwrap().to_vec()).unwrap();
    assert_eq!(text.matches("untouched-sibling").count(), 1);
    let mut state = edited.state.clone();
    assert!(state.set(
        &entry,
        Arc::from(
            text.replace("untouched-sibling", "UNAUTHORIZED")
                .into_bytes()
        )
    ));
    let tampered = edited.rebuild(state, vec![]).unwrap().to_hwpx().unwrap();
    assert!(verify_archive(&original.original, &tampered, &[op], &Limits::default()).is_err());
}
