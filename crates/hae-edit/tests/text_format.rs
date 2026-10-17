use hae_diag::{Code, Limits};
use hae_edit::{apply, Alignment, EditOp};
use hae_model::{Document, Kind};
use hae_package::PackageState;
use hae_zip::Archive;
use std::sync::Arc;

fn fixture(body: &str) -> Document {
    let limits = Limits::default();
    let seed = include_bytes!("../../../tests/fixtures/adversarial/valid_base.hwpx").to_vec();
    let archive = Archive::open(seed, &limits).unwrap();
    let mut state = PackageState::from_archive(&archive);
    let section = format!(
        r#"<hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head" xmlns:x="urn:test:opaque">{body}</hs:sec>"#
    );
    state.set("Contents/section0.xml", Arc::from(section.into_bytes()));
    state.set("Contents/header.xml", Arc::from(br#"<hh:head xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head" xmlns:x="urn:opaque"><hh:refList><hh:charProperties itemCnt="2"><hh:charPr id="0"/><hh:charPr id="1"/></hh:charProperties><hh:paraProperties itemCnt="1"><hh:paraPr id="0" x:keep="preserve"><hh:align horizontal="LEFT" vertical="BASELINE"/><hh:lineSpacing type="PERCENT" value="160"/><x:untouched>opaque</x:untouched></hh:paraPr></hh:paraProperties></hh:refList></hh:head>"#.to_vec()));
    let plan = hae_package::zip_plan(&archive, &state).unwrap();
    Document::open(hae_zip::write(&archive, &plan).unwrap(), &limits).unwrap()
}
fn p(inner: &str) -> String {
    format!(
        r#"<hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>{inner}</hp:t></hp:run><hp:linesegarray/></hp:p>"#
    )
}
fn para(d: &Document, i: usize) -> String {
    d.nodes
        .iter()
        .filter(|n| n.kind == Kind::Paragraph)
        .nth(i)
        .unwrap()
        .id
        .clone()
}
fn xml(d: &Document) -> String {
    String::from_utf8(d.payload("Contents/section0.xml").unwrap().to_vec()).unwrap()
}
fn text(d: &Document, id: &str) -> String {
    d.para_text(d.node(id).unwrap()).unwrap().text
}
#[test]
fn split_run_range_keeps_unselected_bytes_and_first_shape() {
    let d = fixture(
        r#"<hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>abc</hp:t></hp:run><hp:run charPrIDRef="1"><hp:t>def</hp:t></hp:run></hp:p>"#,
    );
    let id = para(&d, 0);
    let a = apply(
        &d,
        &[EditOp::TextReplace {
            target: id.clone(),
            range: 1..5,
            text: "X".into(),
        }],
    )
    .unwrap();
    assert_eq!(text(&a.doc, &id), "aXf");
    assert!(xml(&a.doc).contains(r#"<hp:run charPrIDRef="0"><hp:t>aX</hp:t></hp:run><hp:run charPrIDRef="1"><hp:t>f</hp:t></hp:run>"#));
    assert!(a
        .diagnostics
        .iter()
        .any(|x| x.code == Code::FormatCollapsed));
    assert_eq!(d.map_rows(), a.doc.map_rows());
}
#[test]
fn cdata_text_can_be_partially_replaced_without_losing_neighbors() {
    let d = fixture(&p("pre<![CDATA[a<b&c]]>post"));
    let id = para(&d, 0);
    let a = apply(
        &d,
        &[EditOp::TextReplace {
            target: id.clone(),
            range: 4..7,
            text: "]]> &".into(),
        }],
    )
    .unwrap();
    assert_eq!(text(&a.doc, &id), "prea]]> &cpost");
    assert!(xml(&a.doc).contains("pre"));
    Document::open(a.doc.to_hwpx().unwrap(), &Limits::default()).unwrap();
}
#[test]
fn cache_policy_clears_only_edited_tail_and_preserves_node_ids() {
    let d = fixture(&(p("first") + &p("second") + &p("third")));
    let id = para(&d, 1);
    let a = apply(
        &d,
        &[EditOp::TextReplace {
            target: id,
            range: 0..6,
            text: "changed".into(),
        }],
    )
    .unwrap();
    assert_eq!(xml(&a.doc).matches("<hp:linesegarray/>").count(), 1);
    assert!(xml(&a.doc).contains("first</hp:t></hp:run><hp:linesegarray/>"));
    assert_eq!(d.map_rows(), a.doc.map_rows());
    assert_eq!(a.cache_removals["Contents/section0.xml"], 2);
}
#[test]
fn invalid_second_batch_step_preserves_original_document() {
    let d = fixture(&p("a<hp:tab/>b"));
    let id = para(&d, 0);
    let before = d.to_hwpx().unwrap();
    let e = apply(
        &d,
        &[
            EditOp::TextReplace {
                target: id.clone(),
                range: 0..1,
                text: "ok".into(),
            },
            EditOp::TextReplace {
                target: id,
                range: 0..4,
                text: "bad".into(),
            },
        ],
    )
    .unwrap_err();
    assert_eq!(e.code(), Code::RangeCrossesControl);
    assert_eq!(d.to_hwpx().unwrap(), before);
    let detail = e.diagnostic().detail.as_ref().unwrap();
    assert_eq!(detail["failedStep"], 1);
    assert_eq!(
        detail["textSpans"],
        serde_json::json!([{"start":0,"end":2},{"start":3,"end":4}])
    );
}
#[test]
fn empty_cell_with_unknown_text_attribute_is_rejected() {
    let d = fixture(
        r#"<hp:p><hp:run><hp:tbl><hp:tr><hp:tc><hp:cellAddr rowAddr="0" colAddr="0"/><hp:cellSpan rowSpan="1" colSpan="1"/><hp:subList><hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t x:unknown="keep"/></hp:run></hp:p></hp:subList></hp:tc></hp:tr></hp:tbl></hp:run></hp:p>"#,
    );
    let cell = d
        .nodes
        .iter()
        .find(|n| n.kind == Kind::Cell)
        .unwrap()
        .id
        .clone();
    let e = apply(
        &d,
        &[EditOp::CellSetText {
            target: cell,
            text: "new".into(),
        }],
    )
    .unwrap_err();
    assert_eq!(e.code(), Code::UnsupportedContentInRange);
}
#[test]
fn format_clone_preserves_unknown_header_data_and_other_paragraph() {
    let d = fixture(&(p("first") + &p("second")));
    let id = para(&d, 1);
    let a = apply(
        &d,
        &[EditOp::ParagraphFormat {
            target: id,
            alignment: Some(Alignment::Center),
            line_spacing: None,
        }],
    )
    .unwrap();
    let h = String::from_utf8(a.doc.payload("Contents/header.xml").unwrap().to_vec()).unwrap();
    assert_eq!(h.matches("x:keep=\"preserve\"").count(), 2);
    assert_eq!(h.matches("<x:untouched>opaque</x:untouched>").count(), 2);
    assert!(h.contains("horizontal=\"CENTER\""));
    assert!(xml(&a.doc).contains("<hp:p paraPrIDRef=\"0\"><hp:run charPrIDRef=\"0\"><hp:t>first"));
}

#[test]
fn object_only_paragraph_can_format_but_has_no_text_replacement() {
    let d = fixture(
        r#"<hp:p paraPrIDRef="0"><hp:run><hp:equation/><hp:equation/><hp:equation/></hp:run></hp:p>"#,
    );
    let id = para(&d, 0);
    assert_eq!(text(&d, &id), "\u{fffc}\u{fffc}\u{fffc}");
    let formatted = apply(
        &d,
        &[EditOp::ParagraphFormat {
            target: id.clone(),
            alignment: Some(Alignment::Center),
            line_spacing: None,
        }],
    )
    .unwrap();
    assert_eq!(text(&formatted.doc, &id), text(&d, &id));
    assert!(formatted
        .doc
        .payload("Contents/header.xml")
        .unwrap()
        .windows(19)
        .any(|s| s == b"horizontal=\"CENTER\""));
    let v = serde_json::to_value(d.outline(d.node(&id).unwrap())).unwrap();
    assert_eq!(
        v["edits"]["text.replace"],
        serde_json::json!({"available":false,"reason":"no_text_span"})
    );
    assert_eq!(
        v["edits"]["paragraph.format"],
        serde_json::json!({"available":true})
    );
    assert_eq!(v["textSpans"], serde_json::json!([]));
    assert_eq!(v["editable"], true);
}

#[test]
fn outline_capabilities_describe_existing_text_and_cell_boundaries() {
    let body = p("a<hp:tab/>bc")
        + &p("")
        + r#"<hp:p><hp:run><hp:tbl><hp:tr><hp:tc><hp:cellAddr rowAddr="0" colAddr="0"/><hp:cellSpan rowSpan="2" colSpan="1"/><hp:subList>"#
        + &p("merged")
        + r#"</hp:subList></hp:tc><hp:tc><hp:cellAddr rowAddr="0" colAddr="1"/><hp:subList>"#
        + &p("one")
        + &p("two")
        + "</hp:subList></hp:tc></hp:tr></hp:tbl></hp:run></hp:p>";
    let d = fixture(&body);
    let values: Vec<_> = d
        .nodes
        .iter()
        .map(|n| serde_json::to_value(d.outline(n)).unwrap())
        .collect();
    let first = &values[1];
    assert_eq!(
        first["textSpans"],
        serde_json::json!([{"start":0,"end":1},{"start":2,"end":4}])
    );
    assert_eq!(first["edits"]["text.replace"]["available"], true);
    let empty = &values[2];
    assert_eq!(
        empty["edits"]["text.replace"]["reason"],
        "empty_paragraph_insert_unsupported"
    );
    assert_eq!(empty["editable"], true);
    let cells: Vec<_> = values.iter().filter(|v| v["kind"] == "cell").collect();
    for (cell, reason) in cells.iter().zip(["merged_cell", "cell_paragraph_count"]) {
        assert_eq!(
            cell["edits"]["cell.setText"],
            serde_json::json!({"available":false,"reason":reason})
        );
        assert_eq!(cell["editable"], false);
        assert_eq!(cell["childParagraphsEditable"], true);
    }
    let e = apply(
        &d,
        &[EditOp::TextReplace {
            target: para(&d, 0),
            range: 0..4,
            text: "X".into(),
        }],
    )
    .unwrap_err();
    assert_eq!(e.code(), Code::RangeCrossesControl);
    assert_eq!(
        e.diagnostic().detail.as_ref().unwrap()["textSpans"],
        first["textSpans"]
    );
}

#[test]
fn unsupported_text_attributes_and_grapheme_edges_are_not_advertised() {
    let d = fixture(
        r#"<hp:p><hp:run><hp:t>x<hp:fwSpace/>y</hp:t><hp:t x:unknown="v">hidden</hp:t><hp:t><![CDATA[é]]>&amp;👨‍👩</hp:t></hp:run></hp:p>"#,
    );
    let id = para(&d, 0);
    let v = serde_json::to_value(d.outline(d.node(&id).unwrap())).unwrap();
    assert_eq!(
        v["textSpans"],
        serde_json::json!([{"start":0,"end":1},{"start":2,"end":3},{"start":10,"end":12}])
    );
    assert_eq!(v["edits"]["paragraph.format"]["available"], false);
    for range in [0..1, 2..3, 10..12] {
        apply(
            &d,
            &[EditOp::TextReplace {
                target: id.clone(),
                range,
                text: "X".into(),
            }],
        )
        .unwrap();
    }
    let eq = fixture(r#"<hp:p><hp:run><hp:equation/></hp:run></hp:p>"#);
    let n = eq.nodes.iter().find(|n| n.kind == Kind::Equation).unwrap();
    let v = serde_json::to_value(eq.outline(n)).unwrap();
    assert_eq!(v["edits"], serde_json::json!({}));
    assert_eq!(v["editable"], false);
    assert!(v.get("textSpans").is_none());
}

#[test]
fn readonly_and_locked_nodes_disable_every_advertised_edit() {
    let mut d = fixture(&p("text"));
    let id = para(&d, 0);
    d.permissions.readonly_reasons.push("test_readonly".into());
    let v = serde_json::to_value(d.outline(d.node(&id).unwrap())).unwrap();
    assert_eq!(v["editable"], false);
    assert_eq!(v["textSpans"], serde_json::json!([]));
    for command in ["text.replace", "paragraph.format"] {
        assert_eq!(
            v["edits"][command],
            serde_json::json!({"available":false,"reason":"document_readonly"})
        );
    }
    d.permissions.readonly_reasons.clear();
    d.nodes.iter_mut().find(|n| n.id == id).unwrap().locked = Some("opaque_reference".into());
    let v = serde_json::to_value(d.outline(d.node(&id).unwrap())).unwrap();
    assert_eq!(v["editable"], false);
    assert_eq!(v["edits"]["text.replace"]["reason"], "opaque_reference");
}

#[test]
fn spans_are_bounded_and_diagnostics_use_the_same_bound() {
    let d = fixture(&p(&"a<hp:tab/>".repeat(100)));
    let id = para(&d, 0);
    let v = serde_json::to_value(d.outline(d.node(&id).unwrap())).unwrap();
    assert_eq!(v["textSpans"].as_array().unwrap().len(), 64);
    assert_eq!(v["textSpansTruncated"], true);
    let e = apply(
        &d,
        &[EditOp::TextReplace {
            target: id,
            range: 0..2,
            text: "X".into(),
        }],
    )
    .unwrap_err();
    assert_eq!(
        e.diagnostic().detail.as_ref().unwrap()["textSpans"],
        v["textSpans"]
    );
    assert_eq!(
        e.diagnostic().detail.as_ref().unwrap()["textSpansTruncated"],
        true
    );
    // A text piece beginning inside the control's grapheme must not be advertised.
    let d = fixture(&p("<hp:equation/>́x"));
    let v = serde_json::to_value(d.outline(d.node(&para(&d, 0)).unwrap())).unwrap();
    assert_eq!(v["textSpans"], serde_json::json!([{"start":1,"end":2}]));
}

#[test]
fn cell_capability_accounts_for_its_entire_content_and_empty_insertion_path() {
    for (inner, available, reason) in [
        ("<hp:run><hp:t>plain</hp:t></hp:run>", true, None),
        ("<hp:run><hp:t/></hp:run>", true, None),
        (
            "<hp:run><hp:t>a<hp:tab/>b</hp:t></hp:run>",
            false,
            Some("range_crosses_control"),
        ),
        ("", false, Some("no_run")),
        (
            "<hp:run><hp:t x:unknown=\"v\"/></hp:run>",
            false,
            Some("unsupported_content_in_range"),
        ),
    ] {
        let d = fixture(&format!(
            r#"<hp:p><hp:run><hp:tbl><hp:tr><hp:tc><hp:cellAddr rowAddr="0" colAddr="0"/><hp:subList><hp:p paraPrIDRef="0">{inner}</hp:p></hp:subList></hp:tc></hp:tr></hp:tbl></hp:run></hp:p>"#
        ));
        let cell = d.nodes.iter().find(|n| n.kind == Kind::Cell).unwrap();
        let v = serde_json::to_value(d.outline(cell)).unwrap();
        assert_eq!(
            v["edits"]["cell.setText"]["available"], available,
            "{inner}"
        );
        assert_eq!(v["editable"], available);
        if let Some(reason) = reason {
            assert_eq!(v["edits"]["cell.setText"]["reason"], reason);
        }
        let result = apply(
            &d,
            &[EditOp::CellSetText {
                target: cell.id.clone(),
                text: "X".into(),
            }],
        );
        assert_eq!(result.is_ok(), available, "{inner}: {result:?}");
    }
}

#[test]
fn every_advertised_grapheme_subrange_is_supported_by_planner() {
    // Independently enumerate all possible non-empty ranges and execute the
    // existing mutation planner. The stricter save-gate contract is checked
    // through SessionExecutor in hae-exec/tests/editable_verified.rs.
    for inner in [
        "ab<hp:tab/>cd",
        "é&amp;<![CDATA[xy]]>👨‍👩",
        "a<hp:fwSpace/>b<hp:lineBreak/>c",
        "<hp:equation/>́x",
        "a</hp:t><hp:t x:unknown=\"keep\">b</hp:t><hp:t>c",
        "a<!--keep--><?keep value?>b",
    ] {
        let d = fixture(&p(inner));
        let id = para(&d, 0);
        let node = d.node(&id).unwrap();
        let v = serde_json::to_value(d.outline(node)).unwrap();
        let spans = v["textSpans"].as_array().unwrap();
        let len = d.para_text(node).unwrap().grapheme_len();
        for start in 0..len {
            for end in start + 1..=len {
                let advertised = spans.iter().any(|s| {
                    s["start"].as_u64().unwrap() as usize <= start
                        && end <= s["end"].as_u64().unwrap() as usize
                });
                let result = apply(
                    &d,
                    &[EditOp::TextReplace {
                        target: id.clone(),
                        range: start..end,
                        text: "X".into(),
                    }],
                );
                if advertised {
                    assert!(result.is_ok(), "{inner} {start}..{end}");
                }
            }
        }
    }
}
