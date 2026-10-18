//! Capability advertisement and execution must agree for every table/cell command:
//! `available=false` => dryRun and real calls are rejected (UNSUPPORTED) and leave the
//! revision unchanged; `available=true` => the default-argument dryRun succeeds.
use hae_diag::{Code, Limits};
use hae_exec::SessionExecutor;
use serde_json::{json, Value};
use std::{fs, path::PathBuf};

const FIXTURES: &[(&str, &[u8])] = &[
    (
        "dimensions",
        include_bytes!("../../../tests/fixtures/tables/dimensions.hwpx"),
    ),
    (
        "layout_height",
        include_bytes!("../../../tests/fixtures/tables/layout_height.hwpx"),
    ),
    (
        "structure",
        include_bytes!("../../../tests/fixtures/tables/structure.hwpx"),
    ),
    (
        "structure_object",
        include_bytes!("../../../tests/fixtures/tables/structure_object.hwpx"),
    ),
];

/// structure.hwpx with the 3x2 table's width outside signed HWPUNIT: discovery blocks every
/// command of that table, so the planners must refuse even the row commands that never read it.
fn structure_with_unrepresentable_width() -> Vec<u8> {
    let source = include_bytes!("../../../tests/fixtures/tables/structure.hwpx");
    let archive = hae_zip::Archive::open(source.to_vec(), &Limits::default()).unwrap();
    let mut state = hae_package::PackageState::from_archive(&archive);
    let xml = String::from_utf8(state.get("Contents/section0.xml").unwrap().to_vec()).unwrap();
    let old = r#"<hp:sz width="42000" widthRelTo="ABSOLUTE" height="4000""#;
    assert_eq!(xml.matches(old).count(), 1);
    state.set(
        "Contents/section0.xml",
        xml.replace(old, &old.replace("42000", "4294967295"))
            .into_bytes()
            .into(),
    );
    hae_zip::write(&archive, &hae_package::zip_plan(&archive, &state).unwrap()).unwrap()
}

const TABLE_COMMANDS: &[&str] = &[
    "table.setColumnWidths",
    "table.setRowHeights",
    "table.insertRow",
    "table.deleteRow",
    "table.insertColumn",
    "table.deleteColumn",
];
/// Request-dependent refusals the capability query documents as not enumerated
/// (0.3 design section 9): merge boundaries, template lines, index range.
const PER_REQUEST: &[&str] = &[
    "no_simple_template_line",
    "insert_inside_merged_cell",
    "merged_cell_blocks_delete",
    "index_out_of_range",
    "row_would_become_empty",
    "column_would_become_empty",
    "table_would_become_empty",
];
const CELL_COMMANDS: &[&str] = &["cell.setSize", "cell.setFormat", "cell.setText"];

struct Run(PathBuf);
impl Drop for Run {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Default-argument candidates. Most commands have one; `cell.setFormat` is one command with
/// request-dependent validity (a margin request is impossible for a cell whose own height is
/// at most the margins, while a borderFillId request is not), so any candidate may satisfy it.
fn candidates(command: &str, node: &Value) -> Vec<Value> {
    if command == "cell.setFormat" {
        let target = &node["nodeId"];
        return vec![
            json!({"target":target,"cellMargin":{"left":1,"right":1,"top":1,"bottom":1}}),
            json!({"target":target,"borderFillId":"3"}),
            json!({"target":target,"borderFillId":"4"}),
        ];
    }
    vec![default_params(command, node)]
}

fn default_params(command: &str, node: &Value) -> Value {
    let target = &node["nodeId"];
    match command {
        "table.setColumnWidths" => json!({"target":target,"width":20000}),
        "table.setRowHeights" => json!({"target":target,"height":2800}),
        "table.insertRow" | "table.deleteRow" | "table.insertColumn" | "table.deleteColumn" => {
            json!({"target":target,"index":0})
        }
        "cell.setSize" => json!({"target":target,"width":20000}),
        "cell.setFormat" => unreachable!(),
        "cell.setText" => {
            let requires = &node["edits"]["cell.setText"]["requires"];
            let mut p = match requires["paragraphs"].as_u64() {
                Some(n) => json!({"target":target,"paragraphs":vec!["x"; n as usize]}),
                None => json!({"target":target,"text":"x"}),
            };
            if requires["controls"] == "preserve" {
                p["controls"] = json!("preserve");
            }
            p
        }
        _ => unreachable!(),
    }
}

fn revision(exec: &mut SessionExecutor) -> u64 {
    let r = exec.call("document.inspect", json!({}), None, false);
    r.result.unwrap()["revision"].as_u64().unwrap()
}

#[test]
fn table_and_cell_commands_execute_exactly_when_advertised() {
    let mut per_request_only = 0usize;
    let mut checked = [0usize; 2]; // [advertised unavailable, advertised available]
    let mutated = structure_with_unrepresentable_width();
    let fixtures = FIXTURES
        .iter()
        .copied()
        .chain([("structure_unrepresentable_width", mutated.as_slice())]);
    for (name, bytes) in fixtures {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../spikes/work/tables/test-runs")
            .join(hae_model::new_id("adv-").unwrap());
        fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let _guard = Run(root.clone());
        fs::write(root.join("in.hwpx"), bytes).unwrap();
        let mut exec = SessionExecutor::new(&root, &root, Limits::default()).unwrap();
        assert!(
            exec.call(
                "document.open",
                json!({"path":root.join("in.hwpx")}),
                None,
                false
            )
            .ok,
            "{name}"
        );
        let mut nodes = Vec::new();
        for (kind, commands) in
            [("table", TABLE_COMMANDS), ("cell", CELL_COMMANDS)] as [(&str, &[&str]); 2]
        {
            let q = exec
                .call("element.query", json!({"kind":kind}), None, false)
                .result
                .unwrap();
            for node in q["matches"].as_array().unwrap() {
                nodes.push((node.clone(), commands));
            }
        }
        for (node, commands) in nodes {
            for command in commands {
                let advertised = node["edits"][*command]["available"]
                    .as_bool()
                    .unwrap_or_else(|| panic!("{name} {node}: no {command} capability"));
                let all = candidates(command, &node);
                let params = all[0].clone();
                let before = revision(&mut exec);
                if advertised {
                    checked[1] += 1;
                    // Default arguments are index 0, count 1. Capability does not enumerate
                    // per-request boundaries (design 0.3 section 9), so a structure command
                    // blocked at index 0 by a merge or by emptying the table may try the next
                    // index; at least one candidate must dry-run successfully.
                    let structural = params.get("index").is_some();
                    let mut ok = false;
                    let mut last: Vec<String> = Vec::new();
                    'search: for candidate in &all {
                        for index in 0..if structural { 4 } else { 1 } {
                            let mut p = candidate.clone();
                            if structural {
                                p["index"] = json!(index);
                            }
                            let dry = exec.call(command, p, Some(before), true);
                            if dry.ok {
                                ok = true;
                                break 'search;
                            }
                            last.push(
                                dry.diagnostics[0]
                                    .reason
                                    .clone()
                                    .unwrap_or_else(|| dry.diagnostics[0].message.clone()),
                            );
                        }
                    }
                    // Structure commands may still be refused for the documented per-request
                    // boundaries below (merged-cell template/overlap); never for source geometry.
                    assert!(
                        ok || (structural
                            && last.iter().all(|r| PER_REQUEST.contains(&r.as_str()))),
                        "{name} {} {command}: {last:?}",
                        node["nodeId"]
                    );
                    if !ok {
                        per_request_only += 1;
                    }
                    assert_eq!(revision(&mut exec), before);
                } else {
                    checked[0] += 1;
                    let reason = node["edits"][*command]["reason"].clone();
                    for params in all {
                        let dry = exec.call(command, params.clone(), Some(before), true);
                        assert!(
                            !dry.ok,
                            "{name} {} {command} dryRun accepted",
                            node["nodeId"]
                        );
                        assert_eq!(
                            dry.diagnostics[0].code,
                            Code::Unsupported,
                            "{name} {command}"
                        );
                        assert_eq!(
                            json!(dry.diagnostics[0].reason),
                            reason,
                            "{name} {} {command}",
                            node["nodeId"]
                        );
                        let real = exec.call(command, params, Some(before), false);
                        assert!(
                            !real.ok,
                            "{name} {} {command} real call accepted",
                            node["nodeId"]
                        );
                        assert_eq!(real.diagnostics[0].code, Code::Unsupported);
                        assert_eq!(json!(real.diagnostics[0].reason), reason);
                        assert_eq!(revision(&mut exec), before);
                    }
                }
            }
        }
    }
    assert!(checked[0] > 0 && checked[1] > 0, "{checked:?}");
    eprintln!(
        "advertised unavailable {} / available {} / per-request-only {per_request_only}",
        checked[0], checked[1]
    );
}
