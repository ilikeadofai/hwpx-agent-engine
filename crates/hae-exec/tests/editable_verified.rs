//! Discovery promises are checked through the executor's independent save gate.
use hae_diag::Limits;
use hae_exec::SessionExecutor;
use serde_json::{json, Value};
use std::{collections::BTreeSet, fs, path::PathBuf, sync::Arc};

struct Harness(PathBuf);
impl Harness {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(hae_model::new_id("hae-verified-").unwrap());
        fs::create_dir(&dir).unwrap();
        Self(dir)
    }
    fn open(&self, bytes: &[u8]) -> SessionExecutor {
        let path = self.0.join("input.hwpx");
        fs::write(&path, bytes).unwrap();
        let mut exec = SessionExecutor::new(&self.0, &self.0, Limits::default()).unwrap();
        query(&mut exec, "document.open", json!({"path":path}));
        exec
    }
}
impl Drop for Harness {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn query(exec: &mut SessionExecutor, command: &str, params: Value) -> Value {
    let r = exec.call(command, params, None, false);
    assert!(r.ok, "{command}: {:?}", r.diagnostics);
    r.result.unwrap()
}
fn p(inner: &str) -> String {
    format!(r#"<hp:p paraPrIDRef="0"><hp:run charPrIDRef="0">{inner}</hp:run></hp:p>"#)
}
fn cell(inner: &str) -> String {
    p(&format!(
        r#"<hp:tbl rowCnt="1" colCnt="1"><hp:tr><hp:tc><hp:cellAddr rowAddr="0" colAddr="0"/><hp:cellSpan rowSpan="1" colSpan="1"/><hp:subList>{}</hp:subList></hp:tc></hp:tr></hp:tbl>"#,
        p(inner)
    ))
}
fn synthetic(body: &str) -> Vec<u8> {
    let limits = Limits::default();
    let archive = hae_zip::Archive::open(
        include_bytes!("../../../tests/fixtures/editing/split_runs.hwpx").to_vec(),
        &limits,
    )
    .unwrap();
    let mut state = hae_package::PackageState::from_archive(&archive);
    let section = format!(
        r#"<hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph">{body}</hs:sec>"#
    );
    state.set("Contents/section0.xml", Arc::from(section.into_bytes()));
    hae_zip::write(&archive, &hae_package::zip_plan(&archive, &state).unwrap()).unwrap()
}
fn paragraphs(exec: &mut SessionExecutor) -> Vec<Value> {
    query(exec, "element.query", json!({"kind":"paragraph"}))["matches"]
        .as_array()
        .unwrap()
        .clone()
}
fn dry(exec: &mut SessionExecutor, item: &Value, a: u64, b: u64, text: &str) -> bool {
    exec.call(
        "text.replace",
        json!({"target":item["nodeId"],"range":{"start":a,"end":b},"text":text}),
        Some(0),
        true,
    )
    .ok
}

#[test]
fn cdata_fixture_advertises_only_verified_plain_text() {
    let h = Harness::new();
    let mut exec = h.open(include_bytes!(
        "../../../tests/fixtures/editing/char_refs_cdata.hwpx"
    ));
    let item = paragraphs(&mut exec).remove(0);
    assert_eq!(
        item["textSpans"],
        json!([{"start":0,"end":4},{"start":13,"end":14}])
    );
    assert!(dry(&mut exec, &item, 0, 2, "X"));
    assert!(!dry(&mut exec, &item, 0, 5, "X"));
}

#[test]
fn comments_pi_and_literal_controls_split_advertised_ranges() {
    for (inner, expected) in [
        (
            "<hp:t>ab<!--c-->cd</hp:t>",
            json!([{"start":0,"end":2},{"start":2,"end":4}]),
        ),
        (
            "<hp:t>ab<?x y?>cd</hp:t>",
            json!([{"start":0,"end":2},{"start":2,"end":4}]),
        ),
        (
            "<hp:t>ab</hp:t><!--c--><hp:t>cd</hp:t>",
            json!([{"start":0,"end":2},{"start":2,"end":4}]),
        ),
        (
            "<hp:t>ab</hp:t><?x y?><hp:t>cd</hp:t>",
            json!([{"start":0,"end":2},{"start":2,"end":4}]),
        ),
        (
            "<hp:t>ab\tcd</hp:t>",
            json!([{"start":0,"end":2},{"start":3,"end":5}]),
        ),
        (
            "<hp:t>ab\ncd</hp:t>",
            json!([{"start":0,"end":2},{"start":3,"end":5}]),
        ),
        (
            "<hp:t>ab\rcd</hp:t>",
            json!([{"start":0,"end":2},{"start":3,"end":5}]),
        ),
    ] {
        let h = Harness::new();
        let mut exec = h.open(&synthetic(&p(inner)));
        let item = paragraphs(&mut exec).remove(0);
        assert_eq!(item["textSpans"], expected, "{inner}");
        assert!(dry(&mut exec, &item, 0, 2, "X"));
        assert!(!dry(&mut exec, &item, 0, 3, "X"));
    }
}

#[test]
fn cells_with_verifier_unsupported_markup_are_unavailable() {
    for inner in [
        "<hp:t><![CDATA[cd]]></hp:t>",
        "<hp:t>ab<!--c-->cd</hp:t>",
        "<hp:t>ab\tcd</hp:t>",
        "<hp:t><![CDATA[]]></hp:t>",
        "<!--c--><hp:t/>",
        "<hp:t><!--c--></hp:t>",
    ] {
        let h = Harness::new();
        let mut exec = h.open(&synthetic(&cell(inner)));
        let items = query(&mut exec, "element.query", json!({"kind":"cell"}));
        let item = &items["matches"][0];
        assert_eq!(
            item["edits"]["cell.setText"],
            json!({"available":false,"reason":"verify_unsupported_markup"}),
            "{inner}"
        );
        let r = exec.call(
            "cell.setText",
            json!({"target":item["nodeId"],"text":"X"}),
            Some(0),
            true,
        );
        assert!(!r.ok);
    }
}

#[test]
fn deterministic_document_and_anchor_failures_are_not_advertised() {
    let h = Harness::new();
    let mut exec = h.open(include_bytes!(
        "../../../tests/fixtures/corpus/anchor_char_height.hwpx"
    ));
    let mut text_paras = 0;
    for item in paragraphs(&mut exec) {
        if query(&mut exec, "element.get", json!({"target":item["nodeId"]}))["graphemeCount"]
            .as_u64()
            .unwrap()
            > 0
        {
            text_paras += 1;
            assert_eq!(item["textSpans"], json!([]));
            assert_eq!(
                item["edits"]["text.replace"]["reason"],
                "verify_invalid_references"
            );
            assert!(!dry(&mut exec, &item, 0, 1, "X"));
            assert_eq!(item["edits"]["paragraph.format"]["available"], false);
            // The reported formatting successes were no-ops. A material
            // formatting change must pass the same document reference gate.
            let r = exec.call(
                "paragraph.format",
                json!({"target":item["nodeId"],"lineSpacing":{"kind":"percent","value":163}}),
                Some(0),
                true,
            );
            assert!(!r.ok, "invalid references accepted material formatting");
        }
    }
    assert_eq!(text_paras, 19);
    let mut exec = h.open(include_bytes!(
        "../../../tests/fixtures/corpus/irb-sample.hwpx"
    ));
    let mut nested = 0;
    for item in paragraphs(&mut exec) {
        if item["edits"]["text.replace"]["reason"] == "verify_no_top_level_paragraph" {
            if query(&mut exec, "element.get", json!({"target":item["nodeId"]}))["graphemeCount"]
                .as_u64()
                .unwrap()
                > 0
            {
                nested += 1;
            }
            assert_eq!(item["textSpans"], json!([]));
            assert_eq!(item["edits"]["paragraph.format"]["available"], false);
        }
    }
    assert_eq!(nested, 9);
}

#[test]
fn reference_preconditions_cover_all_resource_kinds_and_refresh_on_rebuild() {
    let limits = Limits::default();
    let bytes = synthetic(&p("<hp:t>abc</hp:t>"));
    let valid = hae_model::Document::open(bytes, &limits).unwrap();
    for reference in [
        "paraPrIDRef",
        "charPrIDRef",
        "styleIDRef",
        "borderFillIDRef",
    ] {
        let mut state = valid.state.clone();
        let section = std::str::from_utf8(state.get("Contents/section0.xml").unwrap()).unwrap();
        let section = section.replace(
            "<hp:p paraPrIDRef=\"0\"",
            &format!("<hp:p paraPrIDRef=\"0\" {reference}=\"missing\""),
        );
        // paraPr already exists, so replace its value rather than duplicate it.
        let section = section.replace(
            "paraPrIDRef=\"0\" paraPrIDRef=\"missing\"",
            "paraPrIDRef=\"missing\"",
        );
        state.set("Contents/section0.xml", Arc::from(section.into_bytes()));
        let broken = valid.rebuild(state, Vec::new()).unwrap();
        let node = broken
            .nodes
            .iter()
            .find(|n| n.kind == hae_model::Kind::Paragraph)
            .unwrap();
        let outline = serde_json::to_value(broken.outline(node)).unwrap();
        assert_eq!(
            outline["edits"]["text.replace"]["reason"], "verify_invalid_references",
            "{reference}"
        );
        let h = Harness::new();
        let mut exec = h.open(&broken.to_hwpx().unwrap());
        let item = paragraphs(&mut exec).remove(0);
        assert!(!dry(&mut exec, &item, 0, 1, "X"), "{reference}");
        let repaired = broken.rebuild(valid.state.clone(), Vec::new()).unwrap();
        let node = repaired.node(&node.id).unwrap();
        assert!(repaired.outline(node).editable);
        assert!(!repaired.paragraph_text_spans(node).text_spans.is_empty());
    }
    let mut state = valid.state.clone();
    let header = std::str::from_utf8(state.get("Contents/header.xml").unwrap()).unwrap();
    let header = header.replace(
        "</hh:refList>",
        "<hh:charProperties><hh:charPr id=\"0\"/></hh:charProperties></hh:refList>",
    );
    state.set("Contents/header.xml", Arc::from(header.into_bytes()));
    let broken = valid.rebuild(state, Vec::new()).unwrap();
    let h = Harness::new();
    let mut exec = h.open(&broken.to_hwpx().unwrap());
    let item = paragraphs(&mut exec).remove(0);
    assert_eq!(
        item["edits"]["text.replace"]["reason"],
        "verify_invalid_references"
    );
    assert!(!dry(&mut exec, &item, 0, 1, "X"));
}

// Each advertised span contributes samples, including internal starts/ends.
// Short spans are exhaustive; longer ones include edges, midpoint and singles.
fn check_samples(exec: &mut SessionExecutor, label: &str) -> usize {
    let items = query(exec, "element.query", json!({}));
    let mut count = 0;
    for item in items["matches"].as_array().unwrap() {
        if item["kind"] == "cell" && item["edits"]["cell.setText"]["available"] == true {
            let paragraphs = items["matches"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|p| p["kind"] == "paragraph" && p["parent"] == item["nodeId"])
                .count();
            for text in ["X", "", "ZZ가"] {
                let r = exec.call(
                    "cell.setText",
                    json!({"target":item["nodeId"],"paragraphs":vec![text;paragraphs],"controls":"preserve"}),
                    Some(0),
                    true,
                );
                assert!(r.ok, "{label} cell: {:?}", r.diagnostics);
            }
        }
        if item["kind"] != "paragraph" {
            continue;
        }
        if item["textSpans"].as_array().unwrap().is_empty()
            && item["edits"]["text.replace"]["available"] == true
        {
            let got = query(exec, "element.get", json!({"target":item["nodeId"]}));
            for text in ["X", "", "ZZ가"] {
                let r=exec.call("text.replace",json!({"target":item["nodeId"],"range":{"start":0,"end":got["graphemeCount"]},"text":text,"controls":"preserve"}),Some(0),true);
                assert!(
                    r.ok,
                    "{label} preserve-only mode rejected: {:?}",
                    r.diagnostics
                );
            }
        }
        for span in item["textSpans"].as_array().unwrap() {
            let a = span["start"].as_u64().unwrap();
            let b = span["end"].as_u64().unwrap();
            let points: BTreeSet<_> = if b - a <= 12 {
                (a..=b).collect()
            } else {
                [a, a + 1, a + 2, (a + b) / 2, b - 2, b - 1, b].into()
            };
            for &pos in &points {
                for text in ["X", "ZZ가"] {
                    assert!(
                        dry(exec, item, pos, pos, text),
                        "{label} advertised insert {pos} rejected"
                    );
                }
            }
            for &s in &points {
                for &e in points.range(s + 1..) {
                    for text in ["X", "", "ZZ가"] {
                        let r = exec.call("text.replace", json!({"target":item["nodeId"],"range":{"start":s,"end":e},"text":text}), Some(0), true);
                        assert!(
                            r.ok,
                            "{label} {:?} {s}..{e} {text:?}: {:?}",
                            item["textPreview"], r.diagnostics
                        );
                        count += 1;
                    }
                }
            }
        }
    }
    assert_eq!(
        query(exec, "document.inspect", json!({"limit":1}))["revision"],
        0
    );
    count
}

#[test]
fn every_editing_fixture_and_synthetic_advertisement_survives_executor_verification() {
    let h = Harness::new();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/editing");
    let mut paths: Vec<_> = fs::read_dir(root)
        .unwrap()
        .map(|p| p.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "hwpx"))
        .collect();
    paths.sort();
    let fixtures = paths.len();
    assert_eq!(fixtures, 19, "every editing fixture must be present");
    let mut checked = 0;
    for path in paths {
        let bytes = fs::read(&path).unwrap();
        let mut exec = h.open(&bytes);
        checked += check_samples(&mut exec, path.file_name().unwrap().to_str().unwrap());
        assert_eq!(fs::read(h.0.join("input.hwpx")).unwrap(), bytes);
    }
    let cases = [
        "<hp:t>ab<![CDATA[cd́e]]>fg</hp:t>",
        "<hp:t>ab<![CDATA[]]>cd</hp:t>",
        "<hp:t>ab<!--c-->cd</hp:t>",
        "<hp:t>ab<?x y?>cd</hp:t>",
        "<hp:t>ab</hp:t><!--c--><hp:t>cd</hp:t>",
        "<hp:t>ab</hp:t><?x y?><hp:t>cd</hp:t>",
        "<hp:t>ab\tcd\nef\rgh</hp:t>",
        "<hp:t>e</hp:t><!--c--><hp:t>́f</hp:t>",
        "<hp:t>a&amp;b&#x1F600;c&#233;&#x301;d👨‍👩‍👧</hp:t>",
        "<hp:t>éa🇰🇷한한😀b</hp:t>",
        "<hp:t>ab<hp:tab/>cd<hp:lineBreak/>ef</hp:t>",
        "<hp:t>ab</hp:t><hp:ctrl><hp:bookmark name=\"b\"/></hp:ctrl><hp:t>́x</hp:t>",
    ];
    let body: String = cases.iter().map(|s| p(s) + &cell(s)).collect();
    checked += check_samples(&mut h.open(&synthetic(&body)), "synthetic");
    assert!(checked > 100);
    eprintln!("VERIFIED_ADVERTISEMENT_PROPERTY editing_fixtures={fixtures} synthetic_cases={} text_dry_runs={checked}", cases.len()*2);
}

#[test]
fn v02_insertion_selects_following_run_then_preceding_run() {
    let body = r#"<hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>ab</hp:t></hp:run><hp:run charPrIDRef="0"><hp:t>cd</hp:t></hp:run></hp:p>"#;
    for (pos, expected) in [
        (0, "<hp:t>Xab</hp:t>"),
        (2, "<hp:t>Xcd</hp:t>"),
        (4, "<hp:t>cdX</hp:t>"),
    ] {
        let h = Harness::new();
        let mut exec = h.open(&synthetic(body));
        let item = paragraphs(&mut exec).remove(0);
        assert_eq!(
            item["edits"]["text.replace"]["modes"],
            json!(["replace", "insert"])
        );
        let r = exec.call(
            "text.replace",
            json!({"target":item["nodeId"],"range":{"start":pos,"end":pos},"text":"X"}),
            Some(0),
            false,
        );
        assert!(r.ok, "{:?}", r.diagnostics);
        let path = h.0.join("out.hwpx");
        query(&mut exec, "document.export", json!({"path":path}));
        let d = hae_model::Document::open(fs::read(path).unwrap(), &Limits::default()).unwrap();
        assert!(
            std::str::from_utf8(d.payload("Contents/section0.xml").unwrap())
                .unwrap()
                .contains(expected)
        );
        let r = exec.call("history.undo", json!({}), Some(1), false);
        assert!(r.ok, "{:?}", r.diagnostics);
        let path = h.0.join("undo.hwpx");
        query(&mut exec, "document.export", json!({"path":path}));
        assert_eq!(fs::read(path).unwrap(), synthetic(body));
    }
}

#[test]
fn v02_empty_paragraph_insertion_appends_t_after_existing_children() {
    for inner in ["", "<hp:t/>", "<hp:t></hp:t>", "<hp:secPr/>"] {
        let h = Harness::new();
        let mut exec = h.open(&synthetic(&p(inner)));
        let item = paragraphs(&mut exec).remove(0);
        assert_eq!(item["textSpans"], json!([{"start":0,"end":0}]));
        assert_eq!(item["edits"]["text.replace"]["modes"], json!(["insert"]));
        assert!(dry(&mut exec, &item, 0, 0, "새 & 글"));
        assert!(!dry(&mut exec, &item, 0, 0, ""));
    }
    for inner in [
        "<hp:t x:unknown=\"1\"/>",
        "<hp:t><![CDATA[]]></hp:t>",
        "<hp:t><!--c--></hp:t>",
    ] {
        let h = Harness::new();
        let body = p(inner).replace("<hp:p ", "<hp:p xmlns:x=\"urn:x\" ");
        let mut exec = h.open(&synthetic(&body));
        let item = paragraphs(&mut exec).remove(0);
        assert_eq!(item["edits"]["text.replace"]["available"], false);
        assert!(!dry(&mut exec, &item, 0, 0, "X"));
    }
}

#[test]
fn v02_merged_multi_paragraph_cell_and_argument_guidance() {
    let body = cell("<hp:t>one</hp:t>")
        .replace("rowSpan=\"1\"", "rowSpan=\"2\"")
        .replace(
            "</hp:subList>",
            &format!("{}{}{}", p(""), p("<hp:t>three</hp:t>"), "</hp:subList>"),
        );
    let h = Harness::new();
    let mut exec = h.open(&synthetic(&body));
    let c = query(&mut exec, "element.query", json!({"kind":"cell"}))["matches"][0].clone();
    assert_eq!(c["edits"]["cell.setText"]["available"], true);
    assert_eq!(
        c["edits"]["cell.setText"]["requires"],
        json!({"paragraphs":3})
    );
    for params in [
        json!({"text":"one"}),
        json!({"paragraphs":["one"]}),
        json!({"text":"one","paragraphs":["a","b","c"]}),
        json!({}),
    ] {
        let mut params = params;
        params["target"] = c["nodeId"].clone();
        let r = exec.call("cell.setText", params, Some(0), false);
        assert!(!r.ok);
        if r.diagnostics[0].code == hae_diag::Code::Unsupported {
            assert!(r.diagnostics[0].detail.as_ref().unwrap()["hint"]
                .as_str()
                .unwrap()
                .contains("paragraphs"));
        }
    }
    let r = exec.call(
        "cell.setText",
        json!({"target":c["nodeId"],"paragraphs":["first","second","third"]}),
        Some(0),
        false,
    );
    assert!(r.ok, "{:?}", r.diagnostics);
    assert_eq!(
        query(
            &mut exec,
            "element.query",
            json!({"kind":"paragraph","ancestor":c["nodeId"]})
        )["matches"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    let r = exec.call("history.undo", json!({}), Some(1), false);
    assert!(r.ok);
    let out = h.0.join("undo.hwpx");
    query(&mut exec, "document.export", json!({"path":out}));
    assert_eq!(fs::read(out).unwrap(), synthetic(&body));
}

#[test]
fn v02_preserve_keeps_objects_in_order_reports_offsets_and_undo() {
    for inner in [
        "<hp:t>a<hp:tab/>b<hp:lineBreak/>c</hp:t><hp:equation/><hp:t>d</hp:t>",
        "<hp:equation/><hp:t/>",
    ] {
        let h = Harness::new();
        let original = synthetic(&p(inner));
        let mut exec = h.open(&original);
        let item = paragraphs(&mut exec).remove(0);
        let len = query(&mut exec, "element.get", json!({"target":item["nodeId"]}))
            ["graphemeCount"]
            .clone();
        let r=exec.call("text.replace",json!({"target":item["nodeId"],"range":{"start":0,"end":len},"text":"X","controls":"preserve"}),Some(0),false);
        assert!(r.ok, "{:?}", r.diagnostics);
        assert!(r
            .diagnostics
            .iter()
            .any(|d| d.code.as_str() == "CONTROLS_PRESERVED"));
        let got = query(&mut exec, "element.get", json!({"target":item["nodeId"]}));
        assert_eq!(
            got["text"],
            if inner.starts_with("<hp:t>a") {
                "X\t\n\u{fffc}"
            } else {
                "\u{fffc}X"
            }
        );
        let r = exec.call("history.undo", json!({}), Some(1), false);
        assert!(r.ok);
        let out = h.0.join("undo.hwpx");
        query(&mut exec, "document.export", json!({"path":out}));
        assert_eq!(fs::read(out).unwrap(), original);
    }
}

#[test]
fn v02_preserve_still_rejects_unsupported_markup_and_equation_targets() {
    for inner in [
        "<hp:t>a<![CDATA[b]]><hp:tab/>c</hp:t>",
        "<hp:t>a<!--c-->b<hp:tab/>c</hp:t>",
        "<hp:t>a<?p x?>b<hp:tab/>c</hp:t>",
        "<hp:t>a\tb<hp:tab/>c</hp:t>",
        "<hp:t>a<hp:fwSpace/>b</hp:t>",
    ] {
        let h = Harness::new();
        let mut exec = h.open(&synthetic(&p(inner)));
        let item = paragraphs(&mut exec).remove(0);
        let len = query(&mut exec, "element.get", json!({"target":item["nodeId"]}))
            ["graphemeCount"]
            .clone();
        let r=exec.call("text.replace",json!({"target":item["nodeId"],"range":{"start":0,"end":len},"text":"X","controls":"preserve"}),Some(0),true);
        assert!(!r.ok);
    }
}

#[test]
fn v02_cell_paragraph_array_does_not_include_textbox_paragraphs() {
    let nested = p("<hp:t>opaque inner text</hp:t>");
    let content=format!("<hp:t>outer</hp:t><hp:rect><hp:drawText><hp:subList>{nested}</hp:subList></hp:drawText></hp:rect>");
    let h = Harness::new();
    let mut exec = h.open(&synthetic(&cell(&content)));
    let c = query(&mut exec, "element.query", json!({"kind":"cell"}))["matches"][0].clone();
    let r = exec.call(
        "cell.setText",
        json!({"target":c["nodeId"],"paragraphs":["NEW"],"controls":"preserve"}),
        Some(0),
        false,
    );
    assert!(r.ok, "{:?}", r.diagnostics);
    let all = paragraphs(&mut exec);
    assert!(all.iter().any(|p| p["textPreview"] == "opaque inner text"));
}

#[test]
fn v02_all_advertised_insertion_boundaries_survive_save_gate() {
    for inner in [
        "<hp:t>ab\tcd</hp:t>",
        "<hp:t>ab<hp:tab/>cd</hp:t>",
        "<hp:t>ab<![CDATA[cd]]>ef</hp:t>",
        "<hp:t>ab<!--x-->cd</hp:t>",
        "<hp:t>é&amp;가</hp:t>",
    ] {
        let h = Harness::new();
        let mut exec = h.open(&synthetic(&p(inner)));
        let item = paragraphs(&mut exec).remove(0);
        for span in item["textSpans"].as_array().unwrap() {
            for i in span["start"].as_u64().unwrap()..=span["end"].as_u64().unwrap() {
                assert!(
                    dry(&mut exec, &item, i, i, "X"),
                    "advertised insertion {i} rejected"
                );
            }
        }
    }
}

#[test]
fn v02_cell_intent_preserves_command_identity_and_paragraph_expectations() {
    for multiple in [false, true] {
        let body = if multiple {
            cell("<hp:t>old</hp:t>").replace(
                "</hp:subList>",
                &format!("{}</hp:subList>", p("<hp:t>second</hp:t>")),
            )
        } else {
            cell("<hp:t>old</hp:t>")
        };
        let h = Harness::new();
        let mut exec = h.open(&synthetic(&body));
        let c = query(&mut exec, "element.query", json!({"kind":"cell"}))["matches"][0].clone();
        let params = if multiple {
            json!({"target":c["nodeId"],"paragraphs":["NEW","NEXT"]})
        } else {
            json!({"target":c["nodeId"],"text":"NEW"})
        };
        let r = exec.call("cell.setText", params, Some(0), false);
        assert!(r.ok);
        let intents = &r.result.unwrap()["intents"];
        assert_eq!(intents.as_array().unwrap().len(), 1);
        assert_eq!(intents[0]["op"], "cell.setText");
        if multiple {
            assert_eq!(intents[0]["expectedParagraphs"], json!(["NEW", "NEXT"]));
        } else {
            assert_eq!(intents[0]["expectedText"], "NEW");
        }
    }
}

#[test]
fn v02_preserve_does_not_skip_first_empty_fragment_with_unknown_attributes() {
    let body = p("<hp:equation/><hp:t xmlns:x=\"urn:x\" x:opaque=\"v\"/><hp:t/>");
    let h = Harness::new();
    let mut exec = h.open(&synthetic(&body));
    let item = paragraphs(&mut exec).remove(0);
    assert_eq!(item["edits"]["text.replace"]["modes"], json!(["insert"]));
    let r=exec.call("text.replace",json!({"target":item["nodeId"],"range":{"start":0,"end":1},"text":"X","controls":"preserve"}),Some(0),false);
    assert!(!r.ok);
    assert_eq!(
        r.diagnostics[0].code,
        hae_diag::Code::UnsupportedContentInRange
    );
}

#[test]
fn v02_cell_reports_preserve_only_child_paragraph_availability() {
    let h = Harness::new();
    let mut exec = h.open(&synthetic(&cell("<hp:equation/><hp:t/>")));
    let c = query(&mut exec, "element.query", json!({"kind":"cell"}))["matches"][0].clone();
    assert_eq!(c["childParagraphsEditable"], true);
    assert_eq!(c["edits"]["cell.setText"]["available"], true);
    let r = exec.call(
        "cell.setText",
        json!({"target":c["nodeId"],"text":"NEW","controls":"preserve"}),
        Some(0),
        true,
    );
    assert!(r.ok);
}

#[test]
fn v02_fields_are_rejected_even_when_controls_are_preserved() {
    for marker in [
        "<hp:fieldBegin id=\"1\" type=\"HYPERLINK\"/>",
        "<hp:fieldEnd beginIDRef=\"1\"/>",
        "<hp:ctrl><hp:fieldBegin id=\"1\"/></hp:ctrl>",
    ] {
        let h = Harness::new();
        let mut exec = h.open(&synthetic(&p(&format!(
            "<hp:t>a</hp:t>{marker}<hp:t>b</hp:t>"
        ))));
        let item = paragraphs(&mut exec).remove(0);
        let r=exec.call("text.replace",json!({"target":item["nodeId"],"range":{"start":0,"end":3},"text":"X","controls":"preserve"}),Some(0),true);
        assert!(!r.ok);
        assert_eq!(
            r.diagnostics[0].reason.as_deref(),
            Some("field_boundary_in_range")
        );
    }
}

#[test]
fn v02_object_only_paragraph_advertises_and_executes_boundary_insertion() {
    let body = p("<hp:equation id=\"1\"/><hp:equation id=\"2\"/>");
    for pos in 0..=2 {
        let h = Harness::new();
        let original = synthetic(&body);
        let mut exec = h.open(&original);
        let item = paragraphs(&mut exec).remove(0);
        assert_eq!(
            item["textSpans"],
            json!([{"start":0,"end":0},{"start":1,"end":1},{"start":2,"end":2}])
        );
        let r = exec.call(
            "text.replace",
            json!({"target":item["nodeId"],"range":{"start":pos,"end":pos},"text":"X"}),
            Some(0),
            false,
        );
        assert!(r.ok, "{:?}", r.diagnostics);
        let got = query(&mut exec, "element.get", json!({"target":item["nodeId"]}));
        let mut chars = vec!["\u{fffc}"; 2];
        chars.insert(pos, "X");
        assert_eq!(got["text"], chars.join(""));
        assert!(exec.call("history.undo", json!({}), Some(1), false).ok);
        let out = h.0.join("undo.hwpx");
        query(&mut exec, "document.export", json!({"path":out}));
        assert!(fs::read(out).unwrap() == original);
    }
}

#[test]
fn v02_object_boundary_insertion_keeps_ids_batch_targets_and_project_paths() {
    for reverse in [false, true] {
        let h = Harness::new();
        let original = synthetic(&cell("<hp:t>old</hp:t>"));
        let mut exec = h.open(&original);
        let outer = paragraphs(&mut exec).remove(0);
        let c = query(&mut exec, "element.query", json!({"kind":"cell"}))["matches"][0].clone();
        let mut steps = vec![
            json!({"command":"text.replace","params":{"target":outer["nodeId"],"range":{"start":0,"end":0},"text":"X"}}),
            json!({"command":"cell.setText","params":{"target":c["nodeId"],"text":"NEW"}}),
        ];
        if reverse {
            steps.reverse();
        }
        let r = exec.call("command.batch", json!({"steps":steps}), Some(0), false);
        assert!(r.ok, "{:?}", r.diagnostics);
        assert_eq!(
            query(&mut exec, "element.query", json!({"kind":"cell"}))["matches"][0]["nodeId"],
            c["nodeId"]
        );
        let project = h.0.join("session.hwpwork");
        query(&mut exec, "project.save", json!({"path":project}));
        assert!(exec.call("history.undo", json!({}), Some(1), false).ok);
        let out = h.0.join("undo.hwpx");
        query(&mut exec, "document.export", json!({"path":out}));
        assert!(fs::read(out).unwrap() == original);
        query(&mut exec, "document.close", json!({}));
        query(&mut exec, "document.open", json!({"path":project}));
        assert_eq!(
            query(&mut exec, "element.query", json!({"kind":"cell"}))["matches"][0]["nodeId"],
            c["nodeId"]
        );
    }
}

#[test]
fn preserve_only_edits_advertise_required_controls() {
    for inner in ["<hp:t>ab</hp:t><hp:equation/>", "<hp:equation/><hp:t/>"] {
        let h = Harness::new();
        let mut exec = h.open(&synthetic(&cell(inner)));
        let c = query(&mut exec, "element.query", json!({"kind":"cell"}))["matches"][0].clone();
        assert_eq!(
            c["edits"]["cell.setText"]["requires"],
            json!({"controls":"preserve"})
        );
        let r = exec.call(
            "cell.setText",
            json!({"target":c["nodeId"],"text":"T","controls":"preserve"}),
            Some(0),
            false,
        );
        assert!(r.ok, "{:?}", r.diagnostics);
    }
    let h = Harness::new();
    let mut exec = h.open(&synthetic(&p("<hp:equation/><hp:t>́</hp:t>")));
    let item = paragraphs(&mut exec).remove(0);
    assert_eq!(item["edits"]["text.replace"]["modes"], json!(["replace"]));
    assert_eq!(
        item["edits"]["text.replace"]["requires"],
        json!({"controls":"preserve"})
    );
    let r = exec.call("text.replace", json!({"target":item["nodeId"],"range":{"start":0,"end":1},"text":"T","controls":"preserve"}), Some(0), true);
    assert!(r.ok, "{:?}", r.diagnostics);
}

#[test]
fn preserve_required_for_replace_mode_with_only_insertion_spans() {
    let h = Harness::new();
    let mut exec = h.open(&synthetic(&p("<hp:equation/><hp:t/>")));
    let item = paragraphs(&mut exec).remove(0);
    assert_eq!(
        item["edits"]["text.replace"]["modes"],
        json!(["replace", "insert"])
    );
    assert_eq!(
        item["edits"]["text.replace"]["requires"],
        json!({"controls":"preserve"})
    );
    for (start, end) in [(0, 1), (0, 0), (1, 1)] {
        let r = exec.call("text.replace", json!({"target":item["nodeId"],"range":{"start":start,"end":end},"text":"T","controls":"preserve"}), Some(0), true);
        assert!(r.ok, "{:?}", r.diagnostics);
    }
}

#[test]
fn multi_paragraph_cell_requires_composes_with_controls_and_dry_runs() {
    let h = Harness::new();
    let body = cell("<hp:t>one</hp:t><hp:equation/>").replace(
        "</hp:subList>",
        &format!("{}</hp:subList>", p("<hp:t>two</hp:t>")),
    );
    let mut exec = h.open(&synthetic(&body));
    let c = query(&mut exec, "element.query", json!({"kind":"cell"}))["matches"][0].clone();
    assert_eq!(
        c["edits"]["cell.setText"]["requires"],
        json!({"paragraphs":2,"controls":"preserve"})
    );
    let r = exec.call(
        "cell.setText",
        json!({"target":c["nodeId"],"paragraphs":["first","second"],"controls":"preserve"}),
        Some(0),
        true,
    );
    assert!(r.ok, "{:?}", r.diagnostics);
}
