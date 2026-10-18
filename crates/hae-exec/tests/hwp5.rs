//! `hwp5.inspect` end-to-end: direct `.hwp` reads through the session executor.
//!
//! Fixtures are synthetic MS-CFB containers (`hae_hwp5::fixture`); this file also
//! proves the command never modifies the input file.

use hae_diag::{Code, Limits};
use hae_exec::SessionExecutor;
use hae_hwp5::fixture::{self, ContainerSpec};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

struct Dirs {
    root: PathBuf,
    input: PathBuf,
    output: PathBuf,
}

impl Dirs {
    fn new() -> Dirs {
        let root = std::env::temp_dir().join(hae_model::new_id("hae-hwp5-").unwrap());
        let (input, output) = (root.join("in"), root.join("out"));
        fs::create_dir_all(&input).unwrap();
        fs::create_dir_all(&output).unwrap();
        Dirs {
            root,
            input: input.canonicalize().unwrap(),
            output: output.canonicalize().unwrap(),
        }
    }

    fn executor(&self) -> SessionExecutor {
        SessionExecutor::new(&self.input, &self.output, Limits::default()).unwrap()
    }
}

impl Drop for Dirs {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn para(text: &str) -> Vec<u8> {
    let mut payload = fixture::utf16(text);
    payload.extend(fixture::utf16("\r"));
    let mut out = fixture::record(0x42, 0, &fixture::para_header_data());
    out.extend(fixture::record(0x43, 1, &payload));
    out
}

fn doc_bytes(paragraphs: &[&str], properties: u32) -> Vec<u8> {
    let section: Vec<u8> = paragraphs
        .iter()
        .map(|text| para(text))
        .collect::<Vec<_>>()
        .concat();
    fixture::build(&ContainerSpec {
        file_header: fixture::file_header((5, 1, 0, 1), properties),
        sections: vec![section],
        extra_streams: vec![],
    })
}

fn write_doc(dirs: &Dirs, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dirs.input.join(name);
    fs::write(&path, bytes).unwrap();
    path
}

fn code_of(response: &hae_exec::ResponseEnvelope) -> Code {
    response.diagnostics[0].code
}

#[test]
fn hwp5_inspect_reports_summary_and_pages_paragraphs() {
    let dirs = Dirs::new();
    let doc = doc_bytes(&["첫 문단", "둘째 문단", "셋째 문단"], 0);
    let path = write_doc(&dirs, "sample.hwp", &doc);
    let mut exec = dirs.executor();

    let first = exec.call(
        "hwp5.inspect",
        json!({"path": path.to_string_lossy(), "limit": 2}),
        None,
        false,
    );
    assert!(first.ok, "{first:?}");
    let result = first.result.unwrap();
    assert_eq!(result["fileHeader"]["version"], "5.1.0.1");
    assert_eq!(result["fileHeader"]["compressed"], false);
    assert_eq!(result["fileHeader"]["encrypted"], false);
    assert_eq!(result["sectionCount"], 1);
    assert_eq!(result["paragraphCount"], 3);
    assert_eq!(
        result["charCount"],
        "첫 문단둘째 문단셋째 문단".chars().count()
    );
    assert_eq!(result["paragraphs"].as_array().unwrap().len(), 2);
    assert_eq!(result["paragraphs"][0]["preview"], "첫 문단");
    assert_eq!(result["paragraphs"][0]["previewTruncated"], false);
    let cursor = result["nextCursor"].as_str().unwrap().to_owned();
    assert!(cursor.ends_with(":2"), "{cursor}");

    let second = exec.call(
        "hwp5.inspect",
        json!({"path": path.to_string_lossy(), "limit": 2, "cursor": cursor}),
        None,
        false,
    );
    assert!(second.ok, "{second:?}");
    let result = second.result.unwrap();
    assert_eq!(result["paragraphs"].as_array().unwrap().len(), 1);
    assert_eq!(result["paragraphs"][0]["paragraphIndex"], 2);
    assert_eq!(result["paragraphs"][0]["preview"], "셋째 문단");
    assert_eq!(result["nextCursor"], Value::Null);
}

#[test]
fn hwp5_inspect_leaves_the_input_file_byte_identical() {
    let dirs = Dirs::new();
    let doc = doc_bytes(&["변하지 않는다"], 0);
    let path = write_doc(&dirs, "immutable.hwp", &doc);
    let before = fs::read(&path).unwrap();
    let before_meta = fs::metadata(&path).unwrap();

    let mut exec = dirs.executor();
    let response = exec.call(
        "hwp5.inspect",
        json!({"path": path.to_string_lossy()}),
        None,
        false,
    );
    assert!(response.ok, "{response:?}");

    let after = fs::read(&path).unwrap();
    let after_meta = fs::metadata(&path).unwrap();
    assert_eq!(before, after, "input bytes changed");
    assert_eq!(before_meta.len(), after_meta.len());
    assert_eq!(
        before_meta.modified().unwrap(),
        after_meta.modified().unwrap()
    );
}

#[test]
fn hwp5_inspect_rejects_compressed_encrypted_and_damaged_files() {
    let dirs = Dirs::new();
    let mut exec = dirs.executor();

    let encrypted = write_doc(&dirs, "encrypted.hwp", &doc_bytes(&["비밀"], 1 << 1));
    let response = exec.call(
        "hwp5.inspect",
        json!({"path": encrypted.to_string_lossy()}),
        None,
        false,
    );
    assert!(!response.ok);
    assert_eq!(code_of(&response), Code::PackageRejected);
    assert_eq!(response.diagnostics[0].reason.as_deref(), Some("encrypted"));

    let distribution = write_doc(&dirs, "distribution.hwp", &doc_bytes(&["배포"], 1 << 2));
    let response = exec.call(
        "hwp5.inspect",
        json!({"path": distribution.to_string_lossy()}),
        None,
        false,
    );
    assert!(!response.ok);
    assert_eq!(
        response.diagnostics[0].reason.as_deref(),
        Some("distribution_unsupported")
    );

    let damaged = write_doc(&dirs, "damaged.hwp", &doc_bytes(&["손상"], 0)[..600]);
    let response = exec.call(
        "hwp5.inspect",
        json!({"path": damaged.to_string_lossy()}),
        None,
        false,
    );
    assert!(!response.ok);
    assert_eq!(
        response.diagnostics[0].reason.as_deref(),
        Some("malformed_compound_file")
    );
}

#[test]
fn hwp5_inspect_rejects_other_extensions_and_stale_cursors() {
    let dirs = Dirs::new();
    let doc = doc_bytes(&["커서"], 0);
    let wrong = write_doc(&dirs, "sample.hwpx", &doc);
    let mut exec = dirs.executor();

    let response = exec.call(
        "hwp5.inspect",
        json!({"path": wrong.to_string_lossy()}),
        None,
        false,
    );
    assert!(!response.ok);
    assert_eq!(code_of(&response), Code::InvalidArgument);
    assert_eq!(
        response.diagnostics[0].reason.as_deref(),
        Some("extension_mismatch")
    );

    let path = write_doc(&dirs, "cursor.hwp", &doc);
    let response = exec.call(
        "hwp5.inspect",
        json!({"path": path.to_string_lossy(), "cursor": "deadbeef:3"}),
        None,
        false,
    );
    assert!(!response.ok);
    assert_eq!(code_of(&response), Code::VersionConflict);

    let response = exec.call(
        "hwp5.inspect",
        json!({"path": path.to_string_lossy(), "cursor": "not-a-cursor"}),
        None,
        false,
    );
    assert!(!response.ok);
    assert_eq!(code_of(&response), Code::InvalidRequest);
}

#[test]
fn hwp5_inspect_does_not_require_or_touch_an_open_document() {
    let dirs = Dirs::new();
    let path = write_doc(&dirs, "no-doc.hwp", &doc_bytes(&["독립"], 0));
    let mut exec = dirs.executor();

    // No document.open call: the command works directly from the path.
    let response = exec.call(
        "hwp5.inspect",
        json!({"path": path.to_string_lossy()}),
        None,
        false,
    );
    assert!(response.ok, "{response:?}");
    assert!(exec.current_document_id().is_none());

    // And it stays a read-only Query tool: dryRun is not registered for it.
    let described = exec.call(
        "command.describe",
        json!({"command": "hwp5.inspect"}),
        None,
        false,
    );
    let d = described.result.unwrap();
    assert_eq!(d["category"], "query");
    assert_eq!(d["mutatesContent"], false);
    assert_eq!(d["requiresDocument"], false);
    assert_eq!(d["annotations"]["readOnlyHint"], true);
}
