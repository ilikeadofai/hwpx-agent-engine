//! Replay all 17 editing boundary fixtures and 8 image inputs from
//! `tests/fixtures/editing/editing_expected.json` (plan tasks 1-2b, 1-8..1-12).

use hae_diag::Limits;
use hae_edit::{apply, Alignment, EditOp, ImageBytes, LineSpacing};
use hae_model::{query::Query, Document, Kind};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

fn resolve_target(doc: &Document, sel: &Value) -> String {
    if let Some(p_idx) = sel.get("paragraph").and_then(Value::as_u64) {
        let entry = sel["entry"].as_str().unwrap_or("Contents/section0.xml");
        let top_paras: Vec<_> = doc
            .nodes
            .iter()
            .filter(|n| {
                n.entry == entry
                    && n.kind == Kind::Paragraph
                    && n.parent
                        .as_ref()
                        .and_then(|pid| doc.node(pid))
                        .is_some_and(|p| p.kind == Kind::Section)
            })
            .collect();
        return top_paras[p_idx as usize].id.clone();
    }
    if let Some(t_idx) = sel.get("table").and_then(Value::as_u64) {
        let tables: Vec<_> = doc.nodes.iter().filter(|n| n.kind == Kind::Table).collect();
        let tid = &tables[t_idx as usize].id;
        let row = sel["row"].as_u64().unwrap() as u32;
        let col = sel["col"].as_u64().unwrap() as u32;
        let qr = doc
            .query(&Query {
                table: Some(tid.clone()),
                row: Some(row),
                col: Some(col),
                ..Default::default()
            })
            .unwrap();
        return qr.matches[0].node_id.clone();
    }
    if let Some(pic_idx) = sel.get("picture").and_then(Value::as_u64) {
        let entry = sel["entry"].as_str().unwrap_or("Contents/section0.xml");
        let pics: Vec<_> = doc
            .nodes
            .iter()
            .filter(|n| n.entry == entry && n.kind == Kind::Picture)
            .collect();
        return pics[pic_idx as usize].id.clone();
    }
    panic!("unknown selector: {sel}");
}

fn build_edit_op(doc: &Document, op: &Value, base_dir: &Path, limits: &Limits) -> EditOp {
    let kind = op["op"].as_str().unwrap();
    let target = resolve_target(doc, &op["target"]);
    let args = &op["args"];
    match kind {
        "text.replace" => {
            let r = args["range"].as_array().unwrap();
            let start = r[0].as_u64().unwrap() as usize;
            let end = r[1].as_u64().unwrap() as usize;
            let text = args["text"].as_str().unwrap().to_string();
            EditOp::TextReplace {
                target,
                range: start..end,
                text,
                preserve: false,
            }
        }
        "cell.setText" => EditOp::CellSetText {
            target,
            text: args["text"].as_str().unwrap().to_string(),
            preserve: false,
        },
        "paragraph.format" => {
            let alignment = args.get("alignment").and_then(Value::as_str).map(|s| {
                serde_json::from_value::<Alignment>(Value::String(s.to_ascii_uppercase())).unwrap()
            });
            let line_spacing = args
                .get("lineSpacing")
                .map(|ls| serde_json::from_value::<LineSpacing>(ls.clone()).unwrap());
            EditOp::ParagraphFormat {
                target,
                alignment,
                line_spacing,
            }
        }
        "image.replace" => {
            let rel = args["image"].as_str().unwrap();
            let bytes = fs::read(base_dir.join(rel)).unwrap();
            let image = ImageBytes::from_bytes(bytes, limits).unwrap();
            EditOp::ImageReplace { target, image }
        }
        other => panic!("unexpected edit op {other}"),
    }
}

#[test]
fn editing_expected_json_all_17_cases_and_8_inputs_match_engine() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/editing");
    let spec: Value =
        serde_json::from_slice(&fs::read(dir.join("editing_expected.json")).unwrap()).unwrap();
    let limits = Limits::default();

    // 1. Validate all 8 image inputs
    for (rel, exp) in spec["inputs"].as_object().unwrap() {
        let bytes = fs::read(dir.join(rel)).unwrap();
        let res = ImageBytes::from_bytes(bytes, &limits);
        if exp["expect"] == "accept" {
            let img = res.unwrap_or_else(|e| panic!("{rel}: unexpected err {e}"));
            assert_eq!(img.format(), exp["format"].as_str().unwrap(), "{rel}");
            let dims = exp["dimensions"].as_array().unwrap();
            assert_eq!(
                img.dimensions(),
                (
                    dims[0].as_u64().unwrap() as u32,
                    dims[1].as_u64().unwrap() as u32
                ),
                "{rel}"
            );
        } else {
            let err = res.expect_err(&format!("{rel}: expected reject"));
            assert_eq!(err.code().as_str(), exp["code"].as_str().unwrap(), "{rel}");
            if let Some(reason) = exp["reason"].as_str() {
                assert_eq!(err.reason(), Some(reason), "{rel}");
            }
        }
    }

    // 2. Validate all 17 HWPX cases and their 58 operations
    for (file_name, case) in spec["cases"].as_object().unwrap() {
        let bytes = fs::read(dir.join(file_name)).unwrap();
        let doc = Document::open(bytes, &limits).unwrap_or_else(|e| panic!("{file_name}: {e}"));
        for (idx, op) in case["operations"].as_array().unwrap().iter().enumerate() {
            let ctx = format!("{file_name} op #{idx} ({})", op["op"]);
            if op["op"] == "element.query" {
                let args = &op["args"];
                let kind = args
                    .get("kind")
                    .and_then(Value::as_str)
                    .map(|k| serde_json::from_value::<Kind>(Value::String(k.to_string())).unwrap());
                let table = args.get("table").and_then(Value::as_u64).map(|t_idx| {
                    let tables: Vec<_> =
                        doc.nodes.iter().filter(|n| n.kind == Kind::Table).collect();
                    tables[t_idx as usize].id.clone()
                });
                let q = Query {
                    kind,
                    text_contains: args
                        .get("textContains")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    ancestor: None,
                    table,
                    row: args.get("row").and_then(Value::as_u64).map(|v| v as u32),
                    col: args.get("col").and_then(Value::as_u64).map(|v| v as u32),
                };
                let qr = doc.query(&q).unwrap();
                assert_eq!(
                    qr.matches.len(),
                    op["matches"].as_array().unwrap().len(),
                    "{ctx}"
                );
                assert_eq!(
                    qr.unaligned_matches,
                    op["unalignedMatches"].as_u64().unwrap() as usize,
                    "{ctx}"
                );
                continue;
            }

            let ops: Vec<EditOp> = if op["op"] == "command.batch" {
                op["steps"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|s| build_edit_op(&doc, s, &dir, &limits))
                    .collect()
            } else {
                vec![build_edit_op(&doc, op, &dir, &limits)]
            };

            let res = apply(&doc, &ops);
            if op["expect"] == "applied" {
                let applied = res.unwrap_or_else(|e| panic!("{ctx}: unexpected error {e:?}"));
                if let Some(want_text) = op.get("resultText").and_then(Value::as_str) {
                    let target_id = ops[0].target();
                    let n = applied.doc.node(target_id).unwrap();
                    assert_eq!(applied.doc.node_text(n), want_text, "{ctx}");
                }
                if let Some(want_diags) = op.get("diagnostics").and_then(Value::as_array) {
                    if !want_diags.is_empty() {
                        assert_eq!(applied.diagnostics.len(), want_diags.len(), "{ctx}");
                        for (got, want) in applied.diagnostics.iter().zip(want_diags) {
                            assert_eq!(got.code.as_str(), want["code"].as_str().unwrap(), "{ctx}");
                        }
                    } else if op
                        .get("note")
                        .and_then(Value::as_str)
                        .is_some_and(|n| n.contains("FORMAT_COLLAPSED 없음"))
                    {
                        assert!(applied.diagnostics.is_empty(), "{ctx}");
                    }
                }
            } else {
                let err = res.expect_err(&format!("{ctx}: expected reject"));
                assert_eq!(err.code().as_str(), op["code"].as_str().unwrap(), "{ctx}");
                if let Some(reason) = op.get("reason").and_then(Value::as_str) {
                    assert_eq!(err.reason(), Some(reason), "{ctx}");
                }
                if let Some(prefix) = op.get("reasonPrefix").and_then(Value::as_str) {
                    assert!(
                        err.reason().is_some_and(|r| r.starts_with(prefix)),
                        "{ctx}: expected reason starting with {prefix}, got {:?}",
                        err.reason()
                    );
                }
            }
        }
    }
}
