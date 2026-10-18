//! Capability advertisement and execution must agree for paragraph.insert /
//! paragraph.delete: `available=false` => dryRun and real calls are rejected
//! (UNSUPPORTED) with the advertised reason and the revision is unchanged;
//! `available=true` => a default-argument dryRun succeeds (or is refused only
//! for the documented per-request boundaries).
use hae_diag::{Code, Limits};
use hae_exec::SessionExecutor;
use serde_json::{json, Value};
use std::{fs, path::PathBuf};

const PARAGRAPHS: &[u8] = include_bytes!("../../../tests/fixtures/paragraph/paragraphs.hwpx");
const NARROW: &[u8] = include_bytes!("../../../tests/fixtures/editing/narrow_edits.hwpx");

/// Request-dependent refusals documented in design §9 (not enumerated by the
/// capability query).
const PER_REQUEST: &[&str] = &[
    "paragraph_before_section_property",
    "paragraph_count_out_of_range",
];

const COMMANDS: &[&str] = &["paragraph.insert", "paragraph.delete"];

struct Run(PathBuf);
impl Drop for Run {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// paragraphs.hwpx with the third paragraph's run stripped of `charPrIDRef`:
/// insertion has no character shape to clone, so discovery must block it.
fn paragraphs_without_charpr() -> Vec<u8> {
    let archive = hae_zip::Archive::open(PARAGRAPHS.to_vec(), &Limits::default()).unwrap();
    let mut state = hae_package::PackageState::from_archive(&archive);
    let xml = String::from_utf8(state.get("Contents/section0.xml").unwrap().to_vec()).unwrap();
    let old = r#"<hp:run charPrIDRef="0"><hp:t>셋째 문단</hp:t></hp:run>"#;
    assert_eq!(xml.matches(old).count(), 1);
    state.set(
        "Contents/section0.xml",
        xml.replace(old, r#"<hp:run><hp:t>셋째 문단</hp:t></hp:run>"#)
            .into_bytes()
            .into(),
    );
    hae_zip::write(&archive, &hae_package::zip_plan(&archive, &state).unwrap()).unwrap()
}

fn revision(exec: &mut SessionExecutor) -> u64 {
    let r = exec.call("document.inspect", json!({}), None, false);
    r.result.unwrap()["revision"].as_u64().unwrap()
}

fn candidates(command: &str, node: &Value) -> Vec<Value> {
    let target = &node["nodeId"];
    match command {
        // Position is request-dependent; at least one must work.
        "paragraph.insert" => vec![
            json!({"target":target,"position":"after"}),
            json!({"target":target,"position":"before"}),
        ],
        _ => vec![json!({"target":target})],
    }
}

#[test]
fn paragraph_commands_execute_exactly_when_advertised() {
    let mutated = paragraphs_without_charpr();
    let fixtures = [
        ("paragraphs", PARAGRAPHS),
        ("narrow", NARROW),
        ("paragraphs_without_charpr", mutated.as_slice()),
    ];
    let mut checked = [0usize; 2]; // [unavailable, available]
    let mut per_request_only = 0usize;
    for (name, bytes) in fixtures {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../spikes/work/paragraph-structure/test-runs")
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
        let q = exec
            .call("element.query", json!({"kind":"paragraph"}), None, false)
            .result
            .unwrap();
        for node in q["matches"].as_array().unwrap() {
            for command in COMMANDS {
                let advertised = node["edits"][*command]["available"]
                    .as_bool()
                    .unwrap_or_else(|| panic!("{name} {node}: no {command} capability"));
                let before = revision(&mut exec);
                let mut any_ok = false;
                let mut last: Vec<String> = Vec::new();
                if advertised {
                    checked[1] += 1;
                    for candidate in candidates(command, node) {
                        let dry = exec.call(command, candidate, Some(before), true);
                        if dry.ok {
                            any_ok = true;
                            break;
                        }
                        last.push(
                            dry.diagnostics[0]
                                .reason
                                .clone()
                                .unwrap_or_else(|| dry.diagnostics[0].message.clone()),
                        );
                    }
                    assert!(
                        any_ok
                            || (*command == "paragraph.insert"
                                && last.iter().all(|r| PER_REQUEST.contains(&r.as_str()))),
                        "{name} {} {command}: {last:?}",
                        node["nodeId"]
                    );
                    if !any_ok {
                        per_request_only += 1;
                    }
                    assert_eq!(revision(&mut exec), before);
                } else {
                    checked[0] += 1;
                    let reason = node["edits"][*command]["reason"].clone();
                    for candidate in candidates(command, node) {
                        let dry = exec.call(command, candidate.clone(), Some(before), true);
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
                        let real = exec.call(command, candidate, Some(before), false);
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
