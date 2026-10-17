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
            for text in ["X", "", "ZZ가"] {
                let r = exec.call(
                    "cell.setText",
                    json!({"target":item["nodeId"],"text":text}),
                    Some(0),
                    true,
                );
                assert!(r.ok, "{label} cell: {:?}", r.diagnostics);
            }
        }
        if item["kind"] != "paragraph" {
            continue;
        }
        for span in item["textSpans"].as_array().unwrap() {
            let a = span["start"].as_u64().unwrap();
            let b = span["end"].as_u64().unwrap();
            let points: BTreeSet<_> = if b - a <= 12 {
                (a..=b).collect()
            } else {
                [a, a + 1, a + 2, (a + b) / 2, b - 2, b - 1, b].into()
            };
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
    assert_eq!(paths.len(), 17);
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
    eprintln!("VERIFIED_ADVERTISEMENT_PROPERTY editing_fixtures=17 synthetic_cases={} text_dry_runs={checked}", cases.len()*2);
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
