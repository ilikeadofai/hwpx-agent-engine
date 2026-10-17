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
                        json!({"target":item["nodeId"],"paragraphs":vec!["X";children["matches"].as_array().unwrap().iter().filter(|p|p["parent"]==item["nodeId"]).count()],"controls":"preserve"}),
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
                    || item["edits"]["text.replace"]["modes"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|m| m == "replace")
            );
            let len = got["graphemeCount"].as_u64().unwrap();
            let mut end = 0;
            for span in spans {
                let a = span["start"].as_u64().unwrap();
                let b = span["end"].as_u64().unwrap();
                assert!(a >= end && a <= b && b <= len);
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
                assert_eq!(r.ok, !spans.is_empty());
                if !r.ok {
                    rejects += 1;
                }
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

// Each actual edit crosses the independent verification gate, then Undo must
// restore the complete original ZIP bytes. Missing local documents are optional.
#[test]
fn v02_all_local_control_paragraphs_empty_paragraphs_and_complex_cells_undo_exactly() {
    let input = std::env::var_os("HAE_EDITABLE_INPUT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../spikes/work/stage4-e2e/inputs")
        });
    if !input.is_dir() {
        eprintln!("SKIP v02 local property: optional inputs absent");
        return;
    }
    let input = input.canonicalize().unwrap();
    let out =
        OutputDir(std::env::temp_dir().join(hae_model::new_id("hae-v02-properties-").unwrap()));
    fs::create_dir(&out.0).unwrap();
    let mut paths: Vec<_> = fs::read_dir(&input)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "hwpx"))
        .collect();
    paths.sort();
    let (mut controls, mut empty, mut multi, mut merged, mut undo) = (0, 0, 0, 0, 0);
    let mut field_rejected = 0;
    let (mut control_success, mut cell_success, mut control_rejected, mut cell_rejected) =
        (0, 0, 0, 0);
    for path in &paths {
        let original = fs::read(path).unwrap();
        let d = hae_model::Document::open(original.clone(), &Limits::default()).unwrap();
        let mut exec = SessionExecutor::new(&input, &out.0, Limits::default()).unwrap();
        query(&mut exec, "document.open", json!({"path":path}));
        let items = query(&mut exec, "element.query", json!({}))["matches"]
            .as_array()
            .unwrap()
            .clone();
        let mut revision = 0;
        for (i, n) in d.nodes.iter().enumerate() {
            let id = &items[i]["nodeId"];
            let (command, params) = if n.kind == hae_model::Kind::Paragraph {
                let pt = d.para_text(n).unwrap();
                if pt
                    .pieces
                    .iter()
                    .any(|p| matches!(p.kind, hae_model::PieceKind::Control { .. }))
                {
                    controls += 1;
                    (
                        "text.replace",
                        json!({"target":id,"range":{"start":0,"end":pt.grapheme_len()},"text":"검증","controls":"preserve"}),
                    )
                } else if pt.text.is_empty() {
                    empty += 1;
                    (
                        "text.replace",
                        json!({"target":id,"range":{"start":0,"end":0},"text":"삽입"}),
                    )
                } else {
                    continue;
                }
            } else if n.kind == hae_model::Kind::Cell {
                let ps: Vec<_> = d
                    .children(&n.id)
                    .filter(|p| p.kind == hae_model::Kind::Paragraph)
                    .collect();
                let a = d.cell_addr(n).unwrap();
                let is_merged = a.row_span > 1 || a.col_span > 1;
                if ps.len() < 2 && !is_merged {
                    continue;
                }
                if ps.len() > 1 {
                    multi += 1;
                }
                if is_merged {
                    merged += 1;
                }
                (
                    "cell.setText",
                    json!({"target":id,"paragraphs":vec!["셀검증";ps.len()],"controls":"preserve"}),
                )
            } else {
                continue;
            };
            let control_attempt = params["controls"] == "preserve";
            let r = exec.call(command, params, Some(revision), false);
            if !r.ok && r.diagnostics[0].reason.as_deref() == Some("field_boundary_in_range") {
                field_rejected += 1;
                if command == "cell.setText" {
                    cell_rejected += 1;
                } else {
                    control_rejected += 1;
                }
                continue;
            }
            assert!(
                r.ok,
                "v02 local property node={i} command={command}: {:?}",
                r.diagnostics
            );
            revision += 1;
            let r = exec.call("history.undo", json!({}), Some(revision), false);
            assert!(r.ok);
            revision += 1;
            let output = out.0.join("undo.hwpx");
            query(&mut exec, "document.export", json!({"path":output}));
            assert!(fs::read(&output).unwrap() == original, "Undo mismatch");
            fs::remove_file(output).unwrap();
            undo += 1;
            if command == "cell.setText" {
                cell_success += 1;
            } else if control_attempt {
                control_success += 1;
            }
        }
        assert_eq!(fs::read(path).unwrap(), original);
    }
    eprintln!("V02_LOCAL_PROPERTIES files={} control_paragraphs={controls} empty_insertions={empty} multi_paragraph_cells={multi} merged_cells={merged} field_rejected={field_rejected} undo_exact={undo} control_success={control_success} control_rejected={control_rejected} cell_success={cell_success} cell_rejected={cell_rejected}",paths.len());
}

fn resolve_local_reference(
    value: &Value,
    saved: &std::collections::BTreeMap<String, Value>,
) -> Value {
    match value {
        Value::String(s) if s.starts_with('$') => {
            let mut parts = s[1..].split('.');
            let mut v = &saved[parts.next().unwrap()];
            for p in parts {
                v = if let Ok(i) = p.parse::<usize>() {
                    &v[i]
                } else {
                    &v[p]
                };
            }
            assert!(!v.is_null(), "unresolved local script reference");
            v.clone()
        }
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, v)| (k.clone(), resolve_local_reference(v, saved)))
                .collect(),
        ),
        Value::Array(a) => Value::Array(
            a.iter()
                .map(|v| resolve_local_reference(v, saved))
                .collect(),
        ),
        _ => value.clone(),
    }
}

#[test]
fn v02_retry_24_recorded_stage4_rejections_and_undo_exactly() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../spikes/work/stage4-e2e");
    if !root.join("inputs").is_dir() || !root.join("runs").is_dir() {
        eprintln!("SKIP stage4 retry: optional local evidence absent");
        return;
    }
    let root = root.canonicalize().unwrap();
    let input = root.join("inputs");
    let out = OutputDir(std::env::temp_dir().join(hae_model::new_id("hae-v02-retry-").unwrap()));
    fs::create_dir(&out.0).unwrap();
    let (mut seen, mut retried, mut expanded, mut excluded) = (0, 0, 0, 0);
    let mut field_rejected = 0;
    let mut no_fragment_rejected = 0;
    let mut adapted_cells = 0;
    let mut paths: Vec<_> = fs::read_dir(root.join("runs"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.to_string_lossy().ends_with(".responses.redacted.json"))
        .collect();
    paths.sort();
    for response_path in paths {
        let responses: Vec<Value> =
            serde_json::from_slice(&fs::read(&response_path).unwrap()).unwrap();
        let bad: Vec<_> = responses
            .iter()
            .enumerate()
            .filter(|(_, r)| r["ok"] == false)
            .map(|(i, _)| i)
            .collect();
        if bad.is_empty() {
            continue;
        }
        let script_path = PathBuf::from(
            response_path
                .to_string_lossy()
                .replace(".responses.redacted.json", ".script.json"),
        );
        let script: Vec<Value> = serde_json::from_slice(&fs::read(script_path).unwrap()).unwrap();
        let filename = PathBuf::from(script[0]["params"]["path"].as_str().unwrap())
            .file_name()
            .unwrap()
            .to_owned();
        let source = input.join(filename);
        let original = fs::read(&source).unwrap();
        let mut exec = SessionExecutor::new(&input, &out.0, Limits::default()).unwrap();
        query(&mut exec, "document.open", json!({"path":source}));
        let mut saved = std::collections::BTreeMap::new();
        let mut revision = 0;
        for (i, step) in script.iter().enumerate().take(*bad.last().unwrap() + 1) {
            let command = step["command"].as_str().unwrap();
            if matches!(
                command,
                "element.query" | "element.get" | "document.inspect"
            ) {
                let params = resolve_local_reference(&step["params"], &saved);
                let result = query(&mut exec, command, params);
                if let Some(name) = step["saveAs"].as_str() {
                    saved.insert(name.to_string(), result);
                }
            }
            if !bad.contains(&i) {
                continue;
            }
            seen += 1;
            if step["params"]["target"]
                .as_str()
                .is_some_and(|s| s.contains("q_equation"))
            {
                let r = exec.call(
                    command,
                    resolve_local_reference(&step["params"], &saved),
                    Some(revision),
                    false,
                );
                assert!(!r.ok);
                excluded += 1;
                continue;
            }
            let mut params = resolve_local_reference(&step["params"], &saved);
            params["controls"] = json!("preserve");
            let mut r = exec.call(command, params.clone(), Some(revision), false);
            if !r.ok && command == "cell.setText" {
                let count = r.diagnostics[0].detail.as_ref().unwrap()["paragraphCount"]
                    .as_u64()
                    .unwrap() as usize;
                let text = params.as_object_mut().unwrap().remove("text").unwrap();
                let children = query(
                    &mut exec,
                    "element.query",
                    json!({"kind":"paragraph","ancestor":params["target"]}),
                );
                let mut texts: Vec<Value> = children["matches"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|p| p["parent"] == params["target"])
                    .map(|p| {
                        query(&mut exec, "element.get", json!({"target":p["nodeId"]}))["text"]
                            .clone()
                    })
                    .collect();
                assert_eq!(texts.len(), count);
                texts[0] = text;
                params["paragraphs"] = json!(texts);
                adapted_cells += 1;
                r = exec.call(command, params.clone(), Some(revision), false);
            }
            if !r.ok {
                assert!(
                    matches!(
                        r.diagnostics[0].reason.as_deref(),
                        Some("no_text_fragment" | "field_boundary_in_range")
                    ),
                    "unexpected exact retry rejection: {:?}",
                    r.diagnostics
                );
                expanded += 1;
                match r.diagnostics[0].reason.as_deref() {
                    Some("field_boundary_in_range") => field_rejected += 1,
                    Some("no_text_fragment") => no_fragment_rejected += 1,
                    other => panic!("unexpected rejection reason {other:?}"),
                }
                let output = out.0.join("rejected-unchanged.hwpx");
                query(&mut exec, "document.export", json!({"path":output}));
                assert!(fs::read(&output).unwrap() == original);
                fs::remove_file(output).unwrap();
                continue;
            }
            assert!(
                r.ok,
                "stage4 retry command={command}: {:?}",
                r.diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
            );
            revision += 1;
            let r = exec.call("history.undo", json!({}), Some(revision), false);
            assert!(r.ok);
            revision += 1;
            let output = out.0.join("retry-undo.hwpx");
            query(&mut exec, "document.export", json!({"path":output}));
            assert!(fs::read(&output).unwrap() == original, "Undo mismatch");
            fs::remove_file(output).unwrap();
            retried += 1;
        }
    }
    assert_eq!((seen, retried + expanded, excluded), (25, 24, 1));
    eprintln!("V02_STAGE4_RETRY recorded={seen} succeeded={retried} equation_excluded={excluded} exact_request_rejected={expanded} field_rejected={field_rejected} no_text_fragment={no_fragment_rejected} undo_exact={retried} adapted_cells_first_paragraph={adapted_cells}");
}
