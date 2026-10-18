//! Integration tests for the `hae` binary (plan 1-18).
//!
//! The representative Stage 1 scenario runs in a *single* script invocation and
//! covers open → query → edit batch → dryRun → undo → export → redo → save →
//! close → reopen, checking the exported HWPX bytes and persisted IDs.
//!
//! Fixtures are built here as minimal STORED ZIP packages so the test does not
//! depend on `hae-zip` (which `hae-cli` must not use).

use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const HAE: &str = env!("CARGO_BIN_EXE_hae");

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "hae-cli-{tag}-{}-{nanos}-{seq}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create temp dir");
        TempDir(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn hae() -> Command {
    Command::new(HAE)
}

// ---------------------------------------------------------------------------
// Test 1: one-shot history commands are rejected with SESSION_REQUIRED.
// ---------------------------------------------------------------------------

#[test]
fn one_shot_history_commands_are_rejected_with_session_required() {
    for command in ["history.undo", "history.redo", "history.clear"] {
        let output = hae()
            .args(["run", "--command", command])
            .output()
            .expect("run hae");

        assert!(
            !output.status.success(),
            "{command} must exit non-zero, status {:?}",
            output.status
        );
        assert!(
            !output.stderr.is_empty(),
            "{command} should log a reason to stderr"
        );

        let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
        let response: Value =
            serde_json::from_str(stdout.trim()).expect("stdout must be one JSON envelope");

        assert_eq!(response["command"], json!(command));
        assert_eq!(response["ok"], json!(false), "{command}: {response}");
        assert_eq!(response["schemaVersion"], json!(1));
        assert_eq!(
            response["diagnostics"][0]["code"],
            json!("SESSION_REQUIRED")
        );
        assert_eq!(response["diagnostics"][0]["stage"], json!("request"));
        assert_eq!(response["diagnostics"][0]["severity"], json!("error"));
    }
}

// ---------------------------------------------------------------------------
// Test 2: one-shot introspection commands run without input/output roots.
// ---------------------------------------------------------------------------

#[test]
fn one_shot_introspection_commands_return_json() {
    let listed = hae()
        .args(["run", "--command", "command.list"])
        .output()
        .expect("run command.list");
    assert!(
        listed.status.success(),
        "command.list exit {:?}",
        listed.status
    );
    let list: Value = serde_json::from_slice(&listed.stdout).expect("command.list JSON");
    assert_eq!(list["ok"], json!(true));
    let commands = list["result"]["commands"]
        .as_array()
        .expect("commands array");
    assert_eq!(commands.len(), hae_exec::command_list().len());

    let described = hae()
        .args([
            "run",
            "--command",
            "command.describe",
            "--params",
            r#"{"command":"text.replace"}"#,
        ])
        .output()
        .expect("run command.describe");
    assert!(
        described.status.success(),
        "command.describe exit {:?}: {}",
        described.status,
        String::from_utf8_lossy(&described.stderr)
    );
    let describe: Value = serde_json::from_slice(&described.stdout).expect("command.describe JSON");
    assert_eq!(describe["ok"], json!(true));
    assert_eq!(describe["result"]["name"], json!("text.replace"));
    assert!(describe["result"]["inputSchema"].is_object());
    assert_eq!(describe["result"]["requiresExpectedVersion"], json!(true));
}

// ---------------------------------------------------------------------------
// Test 3: representative Stage 1 scenario in one script invocation.
// ---------------------------------------------------------------------------

#[test]
fn script_runs_representative_stage1_scenario_end_to_end() {
    let temp = TempDir::new("scenario");
    let input = temp.path().join("input");
    let output = temp.path().join("output");
    fs::create_dir_all(&input).expect("input root");
    fs::create_dir_all(&output).expect("output root");

    let original = fixture_hwpx();
    let doc_path = input.join("doc.hwpx");
    fs::write(&doc_path, &original).expect("write input hwpx");
    fs::write(input.join("ok.png"), ok_png()).expect("write replacement image");

    let script = scenario_script(&input, &output);
    let script_path = temp.path().join("scenario.json");
    fs::write(
        &script_path,
        serde_json::to_string_pretty(&script).expect("serialize script"),
    )
    .expect("write script");

    let run = hae()
        .arg("script")
        .arg("--input-root")
        .arg(&input)
        .arg("--output-root")
        .arg(&output)
        .arg(&script_path)
        .output()
        .expect("run hae script");

    let stderr = String::from_utf8_lossy(&run.stderr).into_owned();
    let stdout = String::from_utf8_lossy(&run.stdout).into_owned();
    assert!(
        run.status.success(),
        "script exited {:?}\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}",
        run.status
    );

    let responses: Vec<Value> = serde_json::from_str(&stdout).expect("array of envelopes");
    assert_eq!(responses.len(), 17, "script step count");
    for (index, response) in responses.iter().enumerate() {
        assert_eq!(
            response["ok"],
            json!(true),
            "step {index} failed: {response}\n--- stderr ---\n{stderr}"
        );
    }

    // open (0) reported the untouched revision
    assert_eq!(responses[0]["result"]["revision"], json!(0));
    // table (2) and cell (3) queries found the cell at (0, 0)
    assert_eq!(
        responses[3]["result"]["matches"][0]["cell"]["row"],
        json!(0)
    );
    assert_eq!(
        responses[3]["result"]["matches"][0]["cell"]["col"],
        json!(0)
    );

    // batch (6) committed one revision and reported changed ids
    assert_eq!(responses[6]["result"]["outcome"], json!("applied"));
    assert_eq!(responses[6]["result"]["beforeVersion"], json!(0));
    assert_eq!(responses[6]["result"]["afterVersion"], json!(1));
    assert!(
        responses[6]["result"]["changedIds"]
            .as_array()
            .is_some_and(|ids| !ids.is_empty()),
        "batch should report changedIds"
    );

    // dryRun (7) predicted the next revision without applying it
    assert_eq!(responses[7]["result"]["dryRun"], json!(true));
    assert_eq!(responses[7]["result"]["outcome"], json!("applied"));
    assert_eq!(responses[7]["result"]["beforeVersion"], json!(1));
    assert_eq!(responses[7]["result"]["afterVersion"], json!(1));
    assert_eq!(responses[7]["result"]["predictedAfterVersion"], json!(2));

    // undo (8) advanced the revision back to the untouched content
    assert_eq!(responses[8]["result"]["outcome"], json!("applied"));
    assert_eq!(responses[8]["result"]["beforeVersion"], json!(1));
    assert_eq!(responses[8]["result"]["afterVersion"], json!(2));

    // export (9) of the undone document is the original bytes verbatim
    assert_eq!(responses[9]["result"]["unmodifiedPassthrough"], json!(true));
    let exported = output.join("unmodified.hwpx");
    let exported_bytes = fs::read(&exported).expect("read exported hwpx");
    assert_eq!(
        exported_bytes, original,
        "unmodified export must equal the original bytes"
    );

    // redo (10) restored the edited state
    assert_eq!(responses[10]["result"]["outcome"], json!("applied"));
    assert_eq!(responses[10]["result"]["beforeVersion"], json!(2));
    assert_eq!(responses[10]["result"]["afterVersion"], json!(3));
    // diff (11) sees the edit against the opened document
    assert_eq!(responses[11]["result"]["identical"], json!(false));

    // project.save (12) recorded the current revision
    assert_eq!(responses[12]["result"]["savedVersion"], json!(3));
    assert!(output.join("work.hwpwork").is_file());

    // reopen (14) continued from the saved revision
    assert_eq!(responses[14]["result"]["revision"], json!(3));

    // element.get (15) shows the persisted edit; the reopened cell query (16)
    // reuses the pre-close table and cell node ids, proving ID persistence.
    assert_eq!(responses[15]["result"]["text"], json!("확정셀"));
    assert_eq!(
        responses[16]["result"]["matches"][0]["nodeId"],
        json!(responses[3]["result"]["matches"][0]["nodeId"].clone())
    );
    assert_eq!(
        responses[16]["result"]["matches"][0]["textPreview"],
        json!("확정셀")
    );
}

#[test]
fn script_accepts_newline_delimited_json_from_stdin() {
    let mut child = hae()
        .arg("script")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn hae script");
    {
        let stdin = child.stdin.as_mut().expect("stdin");
        use std::io::Write;
        writeln!(
            stdin,
            "{}",
            json!({"command": "command.list", "params": {}})
        )
        .unwrap();
        writeln!(
            stdin,
            "{}",
            json!({"command": "command.describe", "params": {"command": "document.open"}})
        )
        .unwrap();
    }
    let output = child.wait_with_output().expect("wait");
    assert!(
        output.status.success(),
        "stdin script exit {:?}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let responses: Vec<Value> = serde_json::from_slice(&output.stdout).expect("array");
    assert_eq!(responses.len(), 2);
    let commands = responses[0]["result"]["commands"]
        .as_array()
        .expect("commands array");
    assert!(commands.iter().any(|c| c["name"] == json!("text.replace")));
    assert_eq!(responses[1]["result"]["name"], json!("document.open"));
}

#[test]
fn script_rejects_unregistered_commands_before_running_anything() {
    let temp = TempDir::new("unknown-command");
    let script = temp.path().join("bad.json");
    fs::write(
        &script,
        json!([{"command": "does.not.exist", "params": {}}]).to_string(),
    )
    .expect("write script");

    let output = hae().arg("script").arg(&script).output().expect("run hae");
    assert!(!output.status.success(), "must exit non-zero");
    assert!(
        output.stdout.is_empty(),
        "a rejected script emits no envelopes: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown command 'does.not.exist'"),
        "stderr should name the command: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn scenario_script(input: &Path, output: &Path) -> Value {
    let doc = path_str(&input.join("doc.hwpx"));
    let image = path_str(&input.join("ok.png"));
    let export = path_str(&output.join("unmodified.hwpx"));
    let project = path_str(&output.join("work.hwpwork"));
    json!([
        {"command": "document.open", "params": {"path": doc}, "saveAs": "open"},
        {"command": "command.list", "params": {}},
        {"command": "element.query", "params": {"kind": "table"}, "saveAs": "table"},
        {"command": "element.query", "params": {"table": "$table.matches.0.nodeId", "row": 0, "col": 0}, "saveAs": "cell"},
        {"command": "element.query", "params": {"kind": "paragraph", "textContains": "제목"}, "saveAs": "para"},
        {"command": "element.query", "params": {"kind": "picture"}, "saveAs": "picture"},
        {"command": "command.batch", "expectedVersion": "$revision", "params": {"steps": [
            {"command": "cell.setText", "params": {"target": "$cell.matches.0.nodeId", "text": "확정셀"}},
            {"command": "paragraph.format", "params": {"target": "$para.matches.0.nodeId", "alignment": "center"}},
            {"command": "image.replace", "params": {"target": "$picture.matches.0.nodeId", "imagePath": image}}
        ]}},
        {"command": "paragraph.format", "expectedVersion": "$revision", "dryRun": true,
         "params": {"target": "$para.matches.0.nodeId", "alignment": "right"}},
        {"command": "history.undo", "expectedVersion": "$revision", "params": {}},
        {"command": "document.export", "params": {"path": export}},
        {"command": "history.redo", "expectedVersion": "$revision", "params": {}},
        {"command": "document.diff", "params": {}},
        {"command": "project.save", "params": {"path": project.clone()}},
        {"command": "document.close", "params": {}},
        {"command": "document.open", "params": {"path": project.clone()}, "saveAs": "reopened"},
        {"command": "element.get", "params": {"target": "$cell.matches.0.nodeId"}, "saveAs": "cellAfter"},
        {"command": "element.query", "params": {"table": "$table.matches.0.nodeId", "row": 0, "col": 0}}
    ])
}

fn path_str(path: &Path) -> String {
    path.to_str().expect("utf8 path").to_string()
}

fn ok_png() -> Vec<u8> {
    fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/editing/inputs/ok.png"
    ))
    .expect("read ok.png fixture")
}

const MIMETYPE: &str = "application/hwp+zip";

const CONTAINER_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?><ocf:container xmlns:ocf="urn:oasis:names:tc:opendocument:xmlns:container" xmlns:hpf="http://www.hancom.co.kr/schema/2011/hpf"><ocf:rootfiles><ocf:rootfile full-path="Contents/content.hpf" media-type="application/hwpml-package+xml"/></ocf:rootfiles></ocf:container>"#;

const CONTENT_HPF: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?><opf:package xmlns:opf="http://www.idpf.org/2007/opf/" version="" unique-identifier="" id=""><opf:metadata><opf:title/><opf:language>ko</opf:language></opf:metadata><opf:manifest><opf:item id="header" href="Contents/header.xml" media-type="application/xml"/><opf:item id="section0" href="Contents/section0.xml" media-type="application/xml"/><opf:item id="img0" href="BinData/img0.png" media-type="image/png"/></opf:manifest><opf:spine><opf:itemref idref="header"/><opf:itemref idref="section0"/></opf:spine></opf:package>"#;

const HEADER_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?><hh:head xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head" version="1.2" secCnt="1"><hh:refList><hh:charProperties itemCnt="1"><hh:charPr id="0"/></hh:charProperties><hh:paraProperties itemCnt="1"><hh:paraPr id="0"><hh:align horizontal="LEFT" vertical="BASELINE"/><hh:lineSpacing type="PERCENT" value="160"/></hh:paraPr></hh:paraProperties><hh:styles itemCnt="1"><hh:style id="0"/></hh:styles></hh:refList></hh:head>"#;

const SECTION_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?><hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" xmlns:hc="http://www.hancom.co.kr/hwpml/2011/core"><hp:p id="1" paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0"><hp:t>제목 문단 Hello</hp:t></hp:run><hp:linesegarray><hp:lineseg textpos="0"/></hp:linesegarray></hp:p><hp:p id="2" paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0"><hp:tbl id="10" rowCnt="1" colCnt="1"><hp:sz width="5000" height="1000"/><hp:tr><hp:tc><hp:cellAddr colAddr="0" rowAddr="0"/><hp:cellSpan colSpan="1" rowSpan="1"/><hp:subList><hp:p id="11" paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>셀원문</hp:t></hp:run><hp:linesegarray><hp:lineseg textpos="0"/></hp:linesegarray></hp:p></hp:subList></hp:tc></hp:tr></hp:tbl><hp:pic id="20"><hp:sz width="100" height="100"/><hp:curSz width="100" height="100"/><hp:imgDim dimwidth="100" dimheight="100"/><hc:img binaryItemIDRef="img0" bright="0" contrast="0" effect="REAL_PIC"/></hp:pic></hp:run><hp:linesegarray><hp:lineseg textpos="0"/></hp:linesegarray></hp:p></hs:sec>"#;

const PNG_1X1_HEX: &str = "89504e470d0a1a0a0000000d49484452000000010000000108060000001f15c4890000000b49444154789c636000020000050001a5f645400000000049454e44ae426082";

fn fixture_hwpx() -> Vec<u8> {
    let png = hex_bytes(PNG_1X1_HEX);
    build_zip(&[
        ("mimetype", MIMETYPE.as_bytes()),
        ("META-INF/container.xml", CONTAINER_XML.as_bytes()),
        ("Contents/content.hpf", CONTENT_HPF.as_bytes()),
        ("Contents/header.xml", HEADER_XML.as_bytes()),
        ("Contents/section0.xml", SECTION_XML.as_bytes()),
        ("BinData/img0.png", png.as_slice()),
    ])
}

fn hex_bytes(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("hex"))
        .collect()
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

fn put_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

/// Minimal STORED-entry ZIP writer (little-endian, no data descriptors).
fn build_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let mut central: Vec<u8> = Vec::new();
    for (name, data) in entries {
        let offset = u32::try_from(out.len()).expect("offset fits");
        let crc = crc32(data);
        let size = u32::try_from(data.len()).expect("size fits");
        let name_bytes = name.as_bytes();
        let name_len = u16::try_from(name_bytes.len()).expect("name fits");

        put_u32(&mut out, 0x0403_4b50);
        put_u16(&mut out, 20);
        put_u16(&mut out, 0);
        put_u16(&mut out, 0);
        put_u16(&mut out, 0);
        put_u16(&mut out, 0x21);
        put_u32(&mut out, crc);
        put_u32(&mut out, size);
        put_u32(&mut out, size);
        put_u16(&mut out, name_len);
        put_u16(&mut out, 0);
        out.extend_from_slice(name_bytes);
        out.extend_from_slice(data);

        put_u32(&mut central, 0x0201_4b50);
        put_u16(&mut central, 20);
        put_u16(&mut central, 20);
        put_u16(&mut central, 0);
        put_u16(&mut central, 0);
        put_u16(&mut central, 0);
        put_u16(&mut central, 0x21);
        put_u32(&mut central, crc);
        put_u32(&mut central, size);
        put_u32(&mut central, size);
        put_u16(&mut central, name_len);
        put_u16(&mut central, 0);
        put_u16(&mut central, 0);
        put_u16(&mut central, 0);
        put_u16(&mut central, 0);
        put_u32(&mut central, 0);
        put_u32(&mut central, offset);
        central.extend_from_slice(name_bytes);
    }

    let count = u16::try_from(entries.len()).expect("entry count fits");
    let central_offset = u32::try_from(out.len()).expect("offset fits");
    let central_size = u32::try_from(central.len()).expect("central dir fits");
    out.extend_from_slice(&central);

    put_u32(&mut out, 0x0605_4b50);
    put_u16(&mut out, 0);
    put_u16(&mut out, 0);
    put_u16(&mut out, count);
    put_u16(&mut out, count);
    put_u32(&mut out, central_size);
    put_u32(&mut out, central_offset);
    put_u16(&mut out, 0);
    out
}

#[test]
fn resource_query_cli_aliases_preserve_export_bytes() {
    let t = TempDir::new("queries2");
    let original = include_bytes!("../../../tests/fixtures/editing/split_runs.hwpx");
    let input = t.path().join("input.hwpx");
    let before = t.path().join("before.hwpx");
    let after = t.path().join("after.hwpx");
    fs::write(&input, original).unwrap();
    let steps = json!([
        {"command":"document.open","params":{"path":input}},
        {"command":"document.export","params":{"path":before}},
        {"command":"style-list","params":{"kind":"charPr"}},
        {"command":"table-get","params":{}},
        {"command":"picture-list","params":{}},
        {"command":"document.export","params":{"path":after}}
    ]);
    let script = t.path().join("queries.json");
    fs::write(&script, serde_json::to_vec(&steps).unwrap()).unwrap();
    let run = hae()
        .arg("script")
        .arg("--input-root")
        .arg(t.path())
        .arg("--output-root")
        .arg(t.path())
        .arg(script)
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let replies: Vec<Value> = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(replies.len(), steps.as_array().unwrap().len());
    assert!(replies.iter().all(|r| r["ok"] == true));
    assert_eq!(replies[2]["command"], "style.list");
    assert!(replies[2]["result"]["total"].as_u64().unwrap() > 0);
    assert_eq!(replies[3]["command"], "table.get");
    assert_eq!(replies[4]["command"], "picture.list");
    assert_eq!(fs::read(before).unwrap(), original);
    assert_eq!(fs::read(after).unwrap(), original);
}

#[test]
fn one_shot_run_accepts_cli_aliases_as_canonical_names() {
    // One-shot `run` resolves the CLI alias exactly like `script` does. No
    // document is open, but the envelope must already carry the registry name.
    let output = hae()
        .args(["run", "--command", "style-list"])
        .output()
        .expect("run hae");
    assert!(!output.status.success(), "no document is open");
    let response: Value = serde_json::from_slice(&output.stdout).expect("one JSON envelope");
    assert_eq!(response["command"], json!("style.list"));
    assert_eq!(response["ok"], json!(false));
    assert_eq!(
        response["diagnostics"][0]["code"],
        json!("SESSION_REQUIRED")
    );
}
