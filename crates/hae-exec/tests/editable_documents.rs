//! Local-only discovery/real-dryRun property check. No documents or text logs
//! are copied into fixtures. Set HAE_EDITABLE_INPUT_DIR to replay other inputs.
use hae_diag::{Code, Limits};
use hae_exec::SessionExecutor;
use serde_json::{json, Value};
use std::{fs, path::PathBuf};

struct OutputDir(PathBuf);
impl Drop for OutputDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn query(exec: &mut SessionExecutor, command: &str, params: Value) -> Value {
    let response = exec.call(command, params, None, false);
    assert!(
        response.ok,
        "{command}: {:?}",
        response
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
    response.result.unwrap()
}

#[test]
fn local_documents_advertised_spans_match_real_dry_runs() {
    let explicit = std::env::var_os("HAE_EDITABLE_INPUT_DIR");
    let input = explicit.as_ref().map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../spikes/work/stage4-e2e/inputs")
    });
    if !input.is_dir() && explicit.is_none() {
        eprintln!("SKIP local document property check: optional inputs absent");
        return;
    }
    let input = input.canonicalize().expect("input directory must exist");
    let mut paths: Vec<_> = fs::read_dir(&input)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "hwpx"))
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "no .hwpx inputs");
    let output =
        OutputDir(std::env::temp_dir().join(hae_model::new_id("hae-capabilities-").unwrap()));
    fs::create_dir(&output.0).unwrap();
    let (mut nodes, mut paragraphs, mut spans_checked, mut rejects, mut formats, mut cells) =
        (0, 0, 0, 0, 0, 0);
    let mut largest_page = 0;
    for path in &paths {
        let original = fs::read(path).unwrap();
        let mut exec = SessionExecutor::new(&input, &output.0, Limits::default()).unwrap();
        query(&mut exec, "document.open", json!({"path": path}));
        let mut items = Vec::new();
        let mut cursor = Value::Null;
        loop {
            let params = if cursor.is_null() {
                json!({})
            } else {
                json!({"cursor":cursor})
            };
            let page = query(&mut exec, "document.inspect", params);
            largest_page = largest_page.max(serde_json::to_vec(&page).unwrap().len());
            assert!(largest_page < 4 * 1024 * 1024);
            assert_eq!(page["revision"], 0);
            items.extend(page["items"].as_array().unwrap().iter().cloned());
            cursor = page["nextCursor"].clone();
            if cursor.is_null() {
                break;
            }
        }
        assert_eq!(
            query(&mut exec, "element.query", json!({}))["matches"],
            json!(items)
        );
        for item in &items {
            nodes += 1;
            let edits = item["edits"].as_object().expect("edits object");
            assert_eq!(
                item["editable"],
                edits.values().any(|edit| edit["available"] == true)
            );
            let got = query(&mut exec, "element.get", json!({"target": item["nodeId"]}));
            assert_eq!(got["outline"], *item);
            if item["kind"] == "cell" {
                let children = query(
                    &mut exec,
                    "element.query",
                    json!({"kind":"paragraph","ancestor":item["nodeId"]}),
                );
                assert_eq!(
                    item["childParagraphsEditable"],
                    children["matches"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|p| p["edits"]["text.replace"]["available"] == true)
                );
                if edits["cell.setText"]["available"] == true {
                    let r = exec.call(
                        "cell.setText",
                        json!({"target":item["nodeId"],"text":"X"}),
                        Some(0),
                        true,
                    );
                    assert!(
                        r.ok,
                        "advertised cell edit failed: {:?}",
                        r.diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
                    );
                    cells += 1;
                }
            }
            if item["kind"] != "paragraph" {
                continue;
            }
            paragraphs += 1;
            let spans = item["textSpans"].as_array().unwrap();
            assert_eq!(
                item["edits"]["text.replace"]["available"],
                !spans.is_empty()
            );
            let len = got["graphemeCount"].as_u64().unwrap();
            let mut end = 0;
            for span in spans {
                let a = span["start"].as_u64().unwrap();
                let b = span["end"].as_u64().unwrap();
                assert!(a >= end && a < b && b <= len);
                end = b;
                let r = exec.call(
                    "text.replace",
                    json!({"target":item["nodeId"],"range":span,"text":"X"}),
                    Some(0),
                    true,
                );
                assert!(
                    r.ok,
                    "advertised span rejected: {:?}",
                    r.diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
                );
                spans_checked += 1;
            }
            if item["textSpansTruncated"] == false {
                // Test every excluded grapheme, as well as crossing each gap
                // from a neighboring editable span. Do not infer holes in a
                // truncated list: omitted spans may still be editable.
                for i in 0..len {
                    if spans.iter().any(|s| {
                        s["start"].as_u64().unwrap() <= i && i < s["end"].as_u64().unwrap()
                    }) {
                        continue;
                    }
                    let crossing = spans
                        .iter()
                        .filter_map(|s| s["start"].as_u64())
                        .filter(|&start| start < i)
                        .max()
                        .unwrap_or(i);
                    for a in std::collections::BTreeSet::from([i, crossing]) {
                        let r = exec.call("text.replace", json!({"target":item["nodeId"],"range":{"start":a,"end":i+1},"text":"X"}), Some(0), true);
                        assert!(!r.ok, "unadvertised range accepted");
                        assert!(matches!(
                            r.diagnostics[0].code,
                            Code::RangeCrossesControl
                                | Code::UnsupportedContentInRange
                                | Code::Unsupported
                        ));
                        if r.diagnostics[0].code == Code::RangeCrossesControl {
                            assert_eq!(
                                r.diagnostics[0].detail.as_ref().unwrap()["textSpans"],
                                item["textSpans"]
                            );
                        }
                        rejects += 1;
                    }
                }
            }
            if len == 0 {
                let r = exec.call(
                    "text.replace",
                    json!({"target":item["nodeId"],"range":{"start":0,"end":0},"text":"X"}),
                    Some(0),
                    true,
                );
                assert!(!r.ok);
                assert_eq!(r.diagnostics[0].code, Code::InvalidRange);
                rejects += 1;
            }
            if edits["paragraph.format"]["available"] == true {
                let alignment = exec.call(
                    "paragraph.format",
                    json!({"target":item["nodeId"],"alignment":"center"}),
                    Some(0),
                    true,
                );
                if !alignment.ok {
                    let spacing = exec.call("paragraph.format", json!({"target":item["nodeId"],"lineSpacing":{"kind":"percent","value":160}}), Some(0), true);
                    assert!(spacing.ok, "advertised paragraph.format unavailable");
                }
                formats += 1;
            }
        }
        assert_eq!(
            query(&mut exec, "document.inspect", json!({"limit":1}))["revision"],
            0
        );
        assert_eq!(fs::read(path).unwrap(), original, "source document mutated");
    }
    eprintln!("LOCAL_CAPABILITY_PROPERTY files={} nodes={nodes} paragraphs={paragraphs} spans={spans_checked} rejected_ranges={rejects} format_dry_runs={formats} cell_dry_runs={cells} max_default_page_bytes={largest_page}", paths.len());
}
