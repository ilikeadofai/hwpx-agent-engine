//! Read-only query commands tests: document.stats, section.list, note.list.

use hae_diag::{Code, Limits};
use hae_exec::SessionExecutor;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture_path(rel: &str) -> PathBuf {
    root().join(rel).canonicalize().expect("fixture must exist")
}

struct Tmp(PathBuf);
impl Tmp {
    fn new(tag: &str) -> Tmp {
        let p = std::env::temp_dir().join(hae_model::new_id(tag).unwrap());
        fs::create_dir(&p).unwrap();
        Tmp(p)
    }
}
impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn open_session(doc_path: &Path) -> (Tmp, SessionExecutor) {
    let tmp = Tmp::new("hae-ro-queries-");
    let input_dir = doc_path.parent().unwrap();
    let mut exec = SessionExecutor::new(input_dir, &tmp.0, Limits::default()).unwrap();
    let r = exec.call("document.open", json!({"path": doc_path}), None, false);
    assert!(r.ok, "document.open failed: {:?}", r.diagnostics);
    (tmp, exec)
}

#[test]
fn test_document_stats_full_document() {
    let doc = fixture_path("spikes/work/stage4-e2e/inputs/04_complex.hwpx");
    let (_tmp, mut exec) = open_session(&doc);

    let r = exec.call("document.stats", json!({}), None, false);
    assert!(r.ok, "document.stats failed: {:?}", r.diagnostics);
    let res = r.result.unwrap();

    assert_eq!(res["revision"], 0);
    assert!(res["scope"].is_null());
    assert_eq!(res["sections"], 1);
    assert_eq!(res["topLevelParagraphs"], 19);
    assert_eq!(res["paragraphs"], 113);
    assert_eq!(res["tables"], 2);
    assert_eq!(res["cells"], 6);
    assert_eq!(res["pictures"], 0);
    assert_eq!(res["equations"], 15);
    assert_eq!(res["fields"], 6);
    assert_eq!(res["footnotes"], 1);
    assert_eq!(res["endnotes"], 1);
    assert_eq!(res["notes"], 2);
    assert_eq!(res["memos"], 2);
    assert!(res["totalChars"].as_u64().unwrap() > 0);
    assert_eq!(res["totalNodes"], 137);
}

#[test]
fn test_document_stats_scoped_to_section() {
    let doc = fixture_path("spikes/work/stage4-e2e/inputs/04_complex.hwpx");
    let (_tmp, mut exec) = open_session(&doc);

    let qr = exec.call("element.query", json!({"kind": "section"}), None, false);
    let sec_id = qr.result.unwrap()["matches"][0]["nodeId"]
        .as_str()
        .unwrap()
        .to_string();

    let r = exec.call("document.stats", json!({"scope": sec_id}), None, false);
    assert!(r.ok, "{:?}", r.diagnostics);
    let res = r.result.unwrap();

    assert_eq!(res["scope"], sec_id);
    assert_eq!(res["sections"], 1);
    assert_eq!(res["paragraphs"], 113);
    assert_eq!(res["tables"], 2);
    assert_eq!(res["cells"], 6);
    assert_eq!(res["equations"], 15);
    assert_eq!(res["fields"], 6);
    assert_eq!(res["footnotes"], 1);
    assert_eq!(res["endnotes"], 1);
}

#[test]
fn test_document_stats_scoped_to_table() {
    let doc = fixture_path("spikes/work/stage4-e2e/inputs/04_complex.hwpx");
    let (_tmp, mut exec) = open_session(&doc);

    let qr = exec.call("element.query", json!({"kind": "table"}), None, false);
    let tbl_id = qr.result.unwrap()["matches"][0]["nodeId"]
        .as_str()
        .unwrap()
        .to_string();

    let r = exec.call("document.stats", json!({"scope": tbl_id}), None, false);
    assert!(r.ok, "{:?}", r.diagnostics);
    let res = r.result.unwrap();

    assert_eq!(res["scope"], tbl_id);
    assert_eq!(res["sections"], 0);
    assert_eq!(res["topLevelParagraphs"], 0);
    assert_eq!(res["tables"], 1);
    assert!(res["cells"].as_u64().unwrap() >= 2);
    assert!(res["paragraphs"].as_u64().unwrap() >= 2);
}

#[test]
fn test_document_stats_scoped_to_paragraph() {
    let doc = fixture_path("spikes/work/stage4-e2e/inputs/04_complex.hwpx");
    let (_tmp, mut exec) = open_session(&doc);

    let qr = exec.call(
        "element.query",
        json!({"kind": "paragraph", "textContains": "명령·규칙"}),
        None,
        false,
    );
    let para_id = qr.result.unwrap()["matches"][0]["nodeId"]
        .as_str()
        .unwrap()
        .to_string();

    let r = exec.call("document.stats", json!({"scope": para_id}), None, false);
    assert!(r.ok, "{:?}", r.diagnostics);
    let res = r.result.unwrap();

    assert_eq!(res["scope"], para_id);
    assert_eq!(res["sections"], 0);
    assert_eq!(res["tables"], 0);
    assert_eq!(res["cells"], 0);
    assert_eq!(res["paragraphs"], 1);
    assert_eq!(res["totalNodes"], 1);
}

#[test]
fn test_document_stats_invalid_scope_not_found() {
    let doc = fixture_path("spikes/work/stage4-e2e/inputs/04_complex.hwpx");
    let (_tmp, mut exec) = open_session(&doc);

    let r = exec.call(
        "document.stats",
        json!({"scope": "n-nonexistent"}),
        None,
        false,
    );
    assert!(!r.ok);
    assert_eq!(r.diagnostics[0].code, Code::TargetNotFound);
}

#[test]
fn test_document_stats_invalid_scope_kind() {
    let doc = fixture_path("spikes/work/stage4-e2e/inputs/04_complex.hwpx");
    let (_tmp, mut exec) = open_session(&doc);

    let qr = exec.call("element.query", json!({"kind": "equation"}), None, false);
    let eq_id = qr.result.unwrap()["matches"][0]["nodeId"]
        .as_str()
        .unwrap()
        .to_string();

    let r = exec.call("document.stats", json!({"scope": eq_id}), None, false);
    assert!(!r.ok);
    assert_eq!(r.diagnostics[0].code, Code::InvalidRequest);
}

#[test]
fn test_section_list_returns_all_sections_and_spine_order() {
    let doc = fixture_path("tests/fixtures/corpus/research-school-plan-reference.hwpx");
    let (_tmp, mut exec) = open_session(&doc);

    let r = exec.call("section.list", json!({}), None, false);
    assert!(r.ok, "{:?}", r.diagnostics);
    let res = r.result.unwrap();

    assert_eq!(res["revision"], 0);
    assert_eq!(res["total"], 28);
    let list = res["sections"].as_array().unwrap();
    assert_eq!(list.len(), 28);

    for (i, sec) in list.iter().enumerate() {
        assert_eq!(sec["section"], i);
        assert!(!sec["nodeId"].as_str().unwrap().is_empty());
        assert!(sec["paragraphs"].as_u64().unwrap() > 0);
    }
}

#[test]
fn test_section_list_explicit_paper_and_column_settings() {
    let doc = fixture_path("spikes/work/stage4-e2e/inputs/04_complex.hwpx");
    let (_tmp, mut exec) = open_session(&doc);

    let r = exec.call("section.list", json!({}), None, false);
    assert!(r.ok, "{:?}", r.diagnostics);
    let res = r.result.unwrap();

    assert_eq!(res["total"], 1);
    let sec = &res["sections"][0];
    assert_eq!(sec["section"], 0);
    assert_eq!(sec["page"]["source"], "explicit");
    assert_eq!(sec["page"]["orientation"], "portrait");
    assert_eq!(sec["page"]["rawOrientation"], "WIDELY");
    assert_eq!(sec["page"]["width"], 59528);
    assert_eq!(sec["page"]["height"], 84186);

    let margin = &sec["page"]["margin"];
    assert_eq!(margin["left"], 8504);
    assert_eq!(margin["right"], 8504);
    assert_eq!(margin["top"], 5668);
    assert_eq!(margin["bottom"], 4252);
    assert_eq!(margin["header"], 4252);
    assert_eq!(margin["footer"], 4252);
    assert_eq!(margin["gutter"], 0);

    assert_eq!(sec["column"]["source"], "explicit");
    assert_eq!(sec["column"]["colCount"], 1);
    assert_eq!(sec["column"]["colType"], "NEWSPAPER");
    assert_eq!(sec["column"]["layout"], "LEFT");
}

#[test]
fn test_section_list_unspecified_fallback_when_secpr_missing() {
    let doc = fixture_path("tests/fixtures/corpus/irb-sample.hwpx");
    let (_tmp, mut exec) = open_session(&doc);

    let r = exec.call("section.list", json!({}), None, false);
    assert!(r.ok, "{:?}", r.diagnostics);
    let res = r.result.unwrap();

    assert_eq!(res["total"], 1);
    let sec = &res["sections"][0];
    assert_eq!(sec["page"]["source"], "unspecified");
    assert!(sec["page"]["orientation"].is_null());
    assert!(sec["page"]["width"].is_null());
    assert!(sec["page"]["height"].is_null());
    assert!(sec["page"]["margin"].is_null());
    assert_eq!(sec["column"]["source"], "unspecified");
    assert!(sec["column"]["colCount"].is_null());
}

#[test]
fn test_section_list_cursor_pagination() {
    let doc = fixture_path("tests/fixtures/corpus/research-school-plan-reference.hwpx");
    let (_tmp, mut exec) = open_session(&doc);

    let r1 = exec.call("section.list", json!({"limit": 10}), None, false);
    assert!(r1.ok, "{:?}", r1.diagnostics);
    let res1 = r1.result.unwrap();
    assert_eq!(res1["sections"].as_array().unwrap().len(), 10);
    assert_eq!(res1["nextCursor"], "0:10");

    let r2 = exec.call(
        "section.list",
        json!({"limit": 10, "cursor": "0:10"}),
        None,
        false,
    );
    assert!(r2.ok, "{:?}", r2.diagnostics);
    let res2 = r2.result.unwrap();
    assert_eq!(res2["sections"].as_array().unwrap().len(), 10);
    assert_eq!(res2["sections"][0]["section"], 10);
    assert_eq!(res2["nextCursor"], "0:20");

    let r3 = exec.call(
        "section.list",
        json!({"limit": 10, "cursor": "0:20"}),
        None,
        false,
    );
    assert!(r3.ok, "{:?}", r3.diagnostics);
    let res3 = r3.result.unwrap();
    assert_eq!(res3["sections"].as_array().unwrap().len(), 8);
    assert_eq!(res3["sections"][0]["section"], 20);
    assert!(res3["nextCursor"].is_null());
}

#[test]
fn test_note_list_returns_footnotes_and_endnotes_with_references() {
    let doc = fixture_path("spikes/work/stage4-e2e/inputs/04_complex.hwpx");
    let (_tmp, mut exec) = open_session(&doc);

    let r = exec.call("note.list", json!({}), None, false);
    assert!(r.ok, "{:?}", r.diagnostics);
    let res = r.result.unwrap();

    assert_eq!(res["revision"], 0);
    assert_eq!(res["total"], 2);
    let notes = res["notes"].as_array().unwrap();
    assert_eq!(notes.len(), 2);

    let fn_note = notes
        .iter()
        .find(|n| n["kind"] == "footnote")
        .expect("footnote present");
    assert_eq!(fn_note["number"], 1);
    assert!(fn_note["text"].as_str().unwrap().contains("이것은 각주."));
    assert!(!fn_note["paragraphNodeId"].as_str().unwrap().is_empty());
    assert!(!fn_note["referenceParagraphNodeId"]
        .as_str()
        .unwrap()
        .is_empty());

    let en_note = notes
        .iter()
        .find(|n| n["kind"] == "endnote")
        .expect("endnote present");
    assert_eq!(en_note["number"], 1);
    assert!(en_note["text"].as_str().unwrap().contains("이건 미주"));
    assert!(!en_note["paragraphNodeId"].as_str().unwrap().is_empty());
    assert!(!en_note["referenceParagraphNodeId"]
        .as_str()
        .unwrap()
        .is_empty());
}

#[test]
fn test_note_list_filter_by_kind() {
    let doc = fixture_path("spikes/work/stage4-e2e/inputs/04_complex.hwpx");
    let (_tmp, mut exec) = open_session(&doc);

    let r_fn = exec.call("note.list", json!({"kind": "footnote"}), None, false);
    assert!(r_fn.ok, "{:?}", r_fn.diagnostics);
    let res_fn = r_fn.result.unwrap();
    assert_eq!(res_fn["total"], 1);
    assert_eq!(res_fn["notes"][0]["kind"], "footnote");

    let r_en = exec.call("note.list", json!({"kind": "endnote"}), None, false);
    assert!(r_en.ok, "{:?}", r_en.diagnostics);
    let res_en = r_en.result.unwrap();
    assert_eq!(res_en["total"], 1);
    assert_eq!(res_en["notes"][0]["kind"], "endnote");
}

#[test]
fn test_note_list_cursor_pagination() {
    let doc = fixture_path("spikes/work/stage4-e2e/inputs/04_complex.hwpx");
    let (_tmp, mut exec) = open_session(&doc);

    let r1 = exec.call("note.list", json!({"limit": 1}), None, false);
    assert!(r1.ok, "{:?}", r1.diagnostics);
    let res1 = r1.result.unwrap();
    assert_eq!(res1["notes"].as_array().unwrap().len(), 1);
    assert_eq!(res1["nextCursor"], "0:1");
    assert_eq!(res1["total"], 2);

    let r2 = exec.call(
        "note.list",
        json!({"limit": 1, "cursor": "0:1"}),
        None,
        false,
    );
    assert!(r2.ok, "{:?}", r2.diagnostics);
    let res2 = r2.result.unwrap();
    assert_eq!(res2["notes"].as_array().unwrap().len(), 1);
    assert!(res2["nextCursor"].is_null());
}

#[test]
fn test_note_list_empty_for_document_without_notes() {
    let doc = fixture_path("tests/fixtures/corpus/ref_empty.hwpx");
    let (_tmp, mut exec) = open_session(&doc);

    let r = exec.call("note.list", json!({}), None, false);
    assert!(r.ok, "{:?}", r.diagnostics);
    let res = r.result.unwrap();
    assert_eq!(res["total"], 0);
    assert!(res["notes"].as_array().unwrap().is_empty());
    assert!(res["nextCursor"].is_null());
}

#[test]
fn test_readonly_commands_preserve_document_and_export_bytes_identically() {
    let doc = fixture_path("spikes/work/stage4-e2e/inputs/04_complex.hwpx");
    let (tmp, mut exec) = open_session(&doc);

    let out1 = tmp.0.join("export1.hwpx");
    let r_exp1 = exec.call("document.export", json!({"path": out1}), None, false);
    assert!(r_exp1.ok, "export1 failed: {:?}", r_exp1.diagnostics);
    let bytes_before = fs::read(&out1).unwrap();

    // Run multiple read-only queries in sequence
    let _ = exec.call("document.stats", json!({}), None, false);
    let _ = exec.call("document.stats", json!({"scope": "n-dummy"}), None, false);
    let _ = exec.call("section.list", json!({}), None, false);
    let _ = exec.call("section.list", json!({"limit": 1}), None, false);
    let _ = exec.call("note.list", json!({}), None, false);
    let _ = exec.call("note.list", json!({"kind": "footnote"}), None, false);

    let out2 = tmp.0.join("export2.hwpx");
    let r_exp2 = exec.call("document.export", json!({"path": out2}), None, false);
    assert!(r_exp2.ok, "export2 failed: {:?}", r_exp2.diagnostics);
    let bytes_after = fs::read(&out2).unwrap();

    // Export bytes must be byte-for-byte identical before and after
    assert_eq!(bytes_before, bytes_after);

    // Export bytes must be byte-for-byte identical to the original input file
    let original_bytes = fs::read(&doc).unwrap();
    assert_eq!(bytes_before, original_bytes);

    // Document revision must stay 0
    assert_eq!(exec.current_revision().unwrap(), 0);
}

#[test]
fn test_print_real_document_summaries() {
    let docs = [
        (
            "04_complex.hwpx",
            fixture_path("spikes/work/stage4-e2e/inputs/04_complex.hwpx"),
        ),
        (
            "research-school-plan-reference.hwpx",
            fixture_path("tests/fixtures/corpus/research-school-plan-reference.hwpx"),
        ),
    ];

    for (name, path) in docs {
        println!("==================================================");
        println!("DOCUMENT: {}", name);
        println!("==================================================");
        let (_tmp, mut exec) = open_session(&path);

        let stats = exec
            .call("document.stats", json!({}), None, false)
            .result
            .unwrap();
        println!("--- document.stats ---");
        println!("{}", serde_json::to_string_pretty(&stats).unwrap());

        let sections = exec
            .call("section.list", json!({"limit": 2}), None, false)
            .result
            .unwrap();
        println!(
            "--- section.list (limit 2 of total {}) ---",
            sections["total"]
        );
        println!("{}", serde_json::to_string_pretty(&sections).unwrap());

        let notes = exec
            .call("note.list", json!({}), None, false)
            .result
            .unwrap();
        println!("--- note.list (total {}) ---", notes["total"]);
        println!("{}", serde_json::to_string_pretty(&notes).unwrap());
    }
}
