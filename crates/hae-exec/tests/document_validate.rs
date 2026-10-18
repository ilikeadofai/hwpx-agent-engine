//! `document.validate` (0.4 feature 2): OWPML dictionary comparison.
//!
//! The fixtures are built from `tests/fixtures/adversarial/valid_base.hwpx` by
//! replacing whole XML parts, so every case is a document the open gate accepts
//! and the report is about content, not about a rejected archive.

use hae_diag::{Code, Limits};
use hae_exec::SessionExecutor;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn base_bytes() -> Vec<u8> {
    include_bytes!("../../../tests/fixtures/adversarial/valid_base.hwpx").to_vec()
}

fn corpus_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/corpus")
        .join(name)
        .canonicalize()
        .expect("corpus fixture must exist")
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

/// Replace whole XML parts of the base package.
fn build(header: &str, section: &str, hpf: Option<&str>) -> Vec<u8> {
    let archive = hae_zip::Archive::open(base_bytes(), &Limits::default()).unwrap();
    let mut plan = hae_zip::keep_all(&archive);
    for item in &mut plan {
        let hae_zip::OutEntry::Keep(name) = item else {
            continue;
        };
        let replacement = match name.as_str() {
            "Contents/header.xml" => Some(header),
            "Contents/section0.xml" => Some(section),
            "Contents/content.hpf" => hpf,
            _ => None,
        };
        if let Some(text) = replacement {
            *item = hae_zip::OutEntry::Replace(name.clone(), Arc::from(text.as_bytes()));
        }
    }
    hae_zip::write(&archive, &plan).unwrap()
}

const HEADER: &str = r##"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?><hh:head xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head" xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" version="1.2" secCnt="1"><hh:refList><hh:fontfaces itemCnt="1"><hh:fontface lang="HANGUL" fontCnt="1"><hh:font id="0" face="함초롬바탕" type="TTF"/></hh:fontface></hh:fontfaces><hh:charProperties itemCnt="1"><hh:charPr id="0" height="1000" textColor="#000000"><hh:fontRef hangul="0"/></hh:charPr></hh:charProperties><hh:paraProperties itemCnt="1"><hh:paraPr id="0"><hh:align horizontal="JUSTIFY" vertical="BASELINE"/></hh:paraPr></hh:paraProperties><hh:styles itemCnt="1"><hh:style id="0" name="바탕글" type="PARA"/></hh:styles></hh:refList></hh:head>"##;

const SECTION: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?><hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" xmlns:hc="http://www.hancom.co.kr/hwpml/2011/core"><hp:p id="1" paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0"><hp:t>본문</hp:t></hp:run></hp:p></hs:sec>"#;

fn open_bytes(tag: &str, bytes: Vec<u8>, limits: Limits) -> (Tmp, SessionExecutor) {
    let tmp = Tmp::new(tag);
    let path = tmp.0.join("doc.hwpx");
    fs::write(&path, bytes).unwrap();
    let mut exec = SessionExecutor::new(&tmp.0, &tmp.0, limits).unwrap();
    let r = exec.call("document.open", json!({ "path": path }), None, false);
    assert!(r.ok, "document.open failed: {:?}", r.diagnostics);
    (tmp, exec)
}

fn open(tag: &str, bytes: Vec<u8>) -> (Tmp, SessionExecutor) {
    open_bytes(tag, bytes, Limits::default())
}

fn open_corpus(name: &str) -> (Tmp, SessionExecutor) {
    let tmp = Tmp::new("hae-validate-");
    let mut exec = SessionExecutor::new(
        corpus_path(name).parent().unwrap(),
        &tmp.0,
        Limits::default(),
    )
    .unwrap();
    let r = exec.call(
        "document.open",
        json!({ "path": corpus_path(name) }),
        None,
        false,
    );
    assert!(r.ok, "document.open failed: {:?}", r.diagnostics);
    (tmp, exec)
}

fn validate(exec: &mut SessionExecutor, params: Value) -> Value {
    let r = exec.call("document.validate", params, None, false);
    assert!(r.ok, "document.validate failed: {:?}", r.diagnostics);
    r.result.unwrap()
}

fn error_code(exec: &mut SessionExecutor, params: Value) -> (Code, String) {
    let r = exec.call("document.validate", params, None, false);
    assert!(!r.ok, "expected a refusal, got {:?}", r.result);
    let d = &r.diagnostics[0];
    (d.code, d.reason.clone().unwrap_or_default())
}

fn find<'a>(result: &'a Value, code: &str) -> Vec<&'a Value> {
    result["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["code"] == code)
        .collect()
}

fn only<'a>(result: &'a Value, code: &str) -> &'a Value {
    let found = find(result, code);
    assert_eq!(found.len(), 1, "expected one {code} finding: {result}");
    found[0]
}

#[test]
fn clean_document_reports_nothing_and_pages_empty() {
    let (_tmp, mut exec) = open("hae-validate-clean", build(HEADER, SECTION, None));
    let r = validate(&mut exec, json!({}));
    assert_eq!(r["items"].as_array().unwrap().len(), 0, "{r}");
    assert_eq!(r["total"], 0);
    assert!(r["nextCursor"].is_null());
    assert_eq!(r["summary"]["errors"], 0);
    assert_eq!(r["summary"]["warnings"], 0);
    assert_eq!(r["summary"]["infos"], 0);
    assert_eq!(r["summary"]["issues"], 0);
    assert!(r["truncated"].as_bool().is_some_and(|t| !t));
    assert_eq!(r["checked"]["referenceIntegrity"], true);
    assert_eq!(r["checked"]["resourceDefinitions"], true);
    assert_eq!(r["checked"]["header"], "Contents/header.xml");
    assert_eq!(r["checked"]["xmlEntries"], 4);
    assert!(r["checked"]["elements"].as_u64().unwrap() > 10);
    let package = r["checked"]["packageItems"].as_array().unwrap();
    assert!(package.contains(&json!("mimetype")));
    assert!(package.contains(&json!("META-INF/container.xml")));
    assert!(package.contains(&json!("Contents/content.hpf")));
    assert!(package.contains(&json!("Contents/section0.xml")));
    assert!(find(&r, "missing_package_item").is_empty());
    let dict = &r["checked"]["dictionary"];
    assert_eq!(dict["project"], "hancom-io/hwpx-owpml-model");
    assert_eq!(dict["license"], "Apache-2.0");
    assert!(dict["elements"].as_u64().unwrap() > 300);
    assert_eq!(dict["fingerprint"].as_str().unwrap().len(), 64);
}

#[test]
fn real_document_has_no_errors_and_reports_the_dictionary_source() {
    let (_tmp, mut exec) = open_corpus("bodojaryo-reference.hwpx");
    let r = validate(&mut exec, json!({}));
    assert_eq!(r["summary"]["errors"], 0, "{r}");
    let summary = &r["summary"];
    let issues: u64 = summary["byCode"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["issues"].as_u64().unwrap())
        .sum();
    assert_eq!(issues, summary["issues"].as_u64().unwrap());
    assert_eq!(
        summary["findings"].as_u64().unwrap(),
        r["items"].as_array().unwrap().len() as u64
    );
    // The sentinel is reported as an unset reference, never as dangling.
    let unset = only(&r, "unset_reference");
    assert_eq!(unset["severity"], "info");
    assert_eq!(unset["value"], "4294967295");
    assert!(find(&r, "unresolved_reference").is_empty());
}

#[test]
fn unknown_element_and_attribute_are_reported_and_aggregated() {
    let section = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?><hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph"><hp:p id="1" paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0"><hp:t>본문</hp:t></hp:run></hp:p><hp:bogus a="1"/><hp:bogus a="2"/></hs:sec>"#;
    let (_tmp, mut exec) = open("hae-validate-unknown", build(HEADER, section, None));
    let r = validate(&mut exec, json!({}));
    let bogus = only(&r, "unknown_element");
    assert_eq!(bogus["severity"], "warning");
    assert_eq!(bogus["reason"], "element_not_in_owpml_model");
    assert_eq!(bogus["count"], 2);
    assert_eq!(bogus["element"], "bogus");
    assert_eq!(bogus["prefix"], "hp");
    assert_eq!(
        bogus["namespace"],
        "http://www.hancom.co.kr/hwpml/2011/paragraph"
    );
    assert_eq!(bogus["examples"].as_array().unwrap().len(), 2);
    assert!(bogus["examples"][0]["path"]
        .as_str()
        .unwrap()
        .ends_with("hs:sec[1]/hp:bogus[1]"));
    assert!(bogus["examples"][0]["entry"] == "Contents/section0.xml");
    // `a` is unknown on `hp:bogus` too, but the element itself is unknown, so
    // its attributes are not compared (no fabricated second finding).
    assert!(find(&r, "unknown_attribute").is_empty());

    let mut header = HEADER.replace(r#"<hh:paraPr id="0">"#, r#"<hh:paraPr id="0" bogus="1">"#);
    header = header.replace(
        r#"<hh:style id="0" name="바탕글" type="PARA"/>"#,
        r#"<hh:style id="0" name="바탕글" type="PARA" bogus="2"/><hh:style id="1" name="x" type="PARA" bogus="3"/>"#,
    );
    header = header.replace(r#"<hh:styles itemCnt="1">"#, r#"<hh:styles itemCnt="2">"#);
    let (_tmp2, mut exec2) = open("hae-validate-unknown-attr", build(&header, SECTION, None));
    let r2 = validate(&mut exec2, json!({}));
    let attrs = find(&r2, "unknown_attribute");
    assert_eq!(attrs.len(), 2, "one row per element: {r2}");
    let style = attrs.iter().find(|f| f["element"] == "style").unwrap();
    assert_eq!(style["severity"], "info");
    assert_eq!(style["attribute"], "bogus");
    assert_eq!(style["count"], 2);
    assert_eq!(style["examples"][0]["entry"], "Contents/header.xml");
    let parapr = attrs.iter().find(|f| f["element"] == "paraPr").unwrap();
    assert_eq!(parapr["count"], 1);
}

#[test]
fn foreign_namespace_is_out_of_coverage_but_its_children_are_not_reported() {
    let section = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?><hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" xmlns:zz="urn:example:zz"><hp:p id="1" paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0"><hp:t>본문</hp:t></hp:run></hp:p><zz:thing z="1"><zz:inner/></zz:thing></hs:sec>"#;
    let (_tmp, mut exec) = open("hae-validate-ns", build(HEADER, section, None));
    let r = validate(&mut exec, json!({}));
    let ns = only(&r, "unknown_namespace");
    assert_eq!(ns["severity"], "info");
    assert_eq!(ns["namespace"], "urn:example:zz");
    assert_eq!(ns["reason"], "namespace_not_in_owpml_model");
    assert_eq!(ns["count"], 2);
    assert!(find(&r, "unknown_element").is_empty());
    let namespaces = r["checked"]["namespaces"].as_array().unwrap();
    let foreign = namespaces
        .iter()
        .find(|n| n["uri"] == "urn:example:zz")
        .unwrap();
    assert!(foreign["prefix"].is_null());
    assert_eq!(foreign["elements"], 2);
}

#[test]
fn unresolved_references_are_warnings_with_kind_and_value() {
    let section = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?><hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph"><hp:p id="1" paraPrIDRef="7" styleIDRef="9"><hp:run charPrIDRef="0"><hp:t>본문</hp:t></hp:run></hp:p><hp:p id="2" paraPrIDRef="8"><hp:run charPrIDRef="0"><hp:t>둘</hp:t></hp:run></hp:p></hs:sec>"#;
    let (_tmp, mut exec) = open("hae-validate-refs", build(HEADER, section, None));
    let r = validate(&mut exec, json!({}));
    let para: Vec<&Value> = find(&r, "unresolved_reference")
        .into_iter()
        .filter(|f| f["resourceKind"] == "paraPr")
        .collect();
    assert_eq!(para.len(), 2, "one row per dangling value: {r}");
    assert!(para.iter().any(|f| f["value"] == "7"));
    assert!(para.iter().any(|f| f["value"] == "8"));
    let seven = para.iter().find(|f| f["value"] == "7").unwrap();
    assert_eq!(seven["severity"], "warning");
    assert_eq!(seven["reason"], "reference_not_in_header");
    assert_eq!(seven["attribute"], "paraPrIDRef");
    assert_eq!(seven["count"], 1);
    assert_eq!(seven["examples"][0]["entry"], "Contents/section0.xml");
    let style = find(&r, "unresolved_reference")
        .into_iter()
        .find(|f| f["resourceKind"] == "style")
        .expect("style finding");
    assert_eq!(style["value"], "9");
    assert_eq!(style["count"], 1);
    assert_eq!(r["summary"]["errors"], 0);
    assert!(r["summary"]["warnings"].as_u64().unwrap() >= 2);
}

#[test]
fn unset_sentinel_is_not_a_dangling_reference() {
    let numbered = r##"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?><hh:head xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head" version="1.2" secCnt="1"><hh:refList><hh:numberings itemCnt="1"><hh:numbering id="0"><hh:paraHead start="1" charPrIDRef="4294967295"/></hh:numbering></hh:numberings><hh:charProperties itemCnt="1"><hh:charPr id="0" height="1000" textColor="#000000"><hh:fontRef hangul="0"/></hh:charPr></hh:charProperties><hh:paraProperties itemCnt="1"><hh:paraPr id="0"/></hh:paraProperties><hh:styles itemCnt="1"><hh:style id="0" name="바탕글" type="PARA"/></hh:styles></hh:refList></hh:head>"##;
    let (_tmp, mut exec) = open("hae-validate-unset", build(numbered, SECTION, None));
    let r = validate(&mut exec, json!({}));
    let unset = only(&r, "unset_reference");
    assert_eq!(unset["severity"], "info");
    assert_eq!(unset["reason"], "reference_unset_sentinel");
    assert_eq!(unset["attribute"], "charPrIDRef");
    assert_eq!(unset["count"], 1);
    assert_eq!(unset["value"], "4294967295");
    assert!(find(&r, "unresolved_reference").is_empty(), "{r}");
    assert_eq!(r["summary"]["errors"], 0);
}

#[test]
fn item_and_font_count_mismatches_are_errors() {
    let header = HEADER
        .replace(
            r#"<hh:charProperties itemCnt="1">"#,
            r#"<hh:charProperties itemCnt="2">"#,
        )
        .replace(
            r#"<hh:fontface lang="HANGUL" fontCnt="1">"#,
            r#"<hh:fontface lang="HANGUL" fontCnt="2">"#,
        );
    let (_tmp, mut exec) = open("hae-validate-counts", build(&header, SECTION, None));
    let r = validate(&mut exec, json!({}));
    let counts = find(&r, "count_mismatch");
    assert_eq!(counts.len(), 2, "{r}");
    let item = counts.iter().find(|f| f["attribute"] == "itemCnt").unwrap();
    assert_eq!(item["severity"], "error");
    assert_eq!(item["reason"], "item_cnt_mismatch");
    assert_eq!(item["element"], "charProperties");
    assert_eq!(item["detail"]["declared"], 2);
    assert_eq!(item["detail"]["actual"], 1);
    let font = counts.iter().find(|f| f["attribute"] == "fontCnt").unwrap();
    assert_eq!(font["reason"], "font_cnt_mismatch");
    assert_eq!(font["detail"]["actual"], 1);
    assert_eq!(r["summary"]["errors"], 2);
}

#[test]
fn section_count_mismatch_is_an_error() {
    let header = HEADER.replace(r#"secCnt="1""#, r#"secCnt="3""#);
    let (_tmp, mut exec) = open("hae-validate-sec", build(&header, SECTION, None));
    let r = validate(&mut exec, json!({}));
    let m = only(&r, "count_mismatch");
    assert_eq!(m["severity"], "error");
    assert_eq!(m["reason"], "sec_cnt_mismatch");
    assert_eq!(m["attribute"], "secCnt");
    assert_eq!(m["detail"]["declared"], 3);
    assert_eq!(m["detail"]["actual"], 1);
}

#[test]
fn duplicate_resource_id_is_an_error() {
    let header = r##"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?><hh:head xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head" version="1.2" secCnt="1"><hh:refList><hh:charProperties itemCnt="2"><hh:charPr id="0" height="1000" textColor="#000000"><hh:fontRef hangul="0"/></hh:charPr><hh:charPr id="0" height="900" textColor="#111111"/></hh:charProperties><hh:paraProperties itemCnt="1"><hh:paraPr id="0"/></hh:paraProperties><hh:styles itemCnt="1"><hh:style id="0" name="바탕글" type="PARA"/></hh:styles></hh:refList></hh:head>"##;
    let (_tmp, mut exec) = open("hae-validate-duplicate", build(header, SECTION, None));
    let r = validate(&mut exec, json!({}));
    let dup = only(&r, "duplicate_resource_id");
    assert_eq!(dup["severity"], "error");
    assert_eq!(dup["reason"], "duplicate_id_in_header");
    assert_eq!(dup["resourceKind"], "charPr");
    assert_eq!(dup["value"], "0");
    assert_eq!(dup["detail"]["definitions"], 2);
    assert_eq!(dup["scope"], "document");
}

#[test]
fn unused_resource_is_informational_and_listed_once_per_kind() {
    let header = HEADER.replace(
        r#"<hh:styles itemCnt="1">"#,
        r#"<hh:styles itemCnt="2"><hh:style id="5" name="안쓰임" type="PARA"/>"#,
    );
    let (_tmp, mut exec) = open("hae-validate-unused", build(&header, SECTION, None));
    let r = validate(&mut exec, json!({}));
    let unused = only(&r, "unused_resource");
    assert_eq!(unused["severity"], "info");
    assert_eq!(unused["reason"], "no_reference_in_document");
    assert_eq!(unused["resourceKind"], "style");
    assert_eq!(unused["count"], 1);
    assert_eq!(
        unused["examples"][0]["path"],
        "hh:head[1]/hh:refList[1]/hh:styles[1]/hh:style[1]"
    );
    assert_eq!(r["summary"]["errors"], 0);
}

#[test]
fn headerless_package_reports_missing_header_and_skips_reference_checks() {
    let (_tmp, mut exec) = open_corpus("irb-sample.hwpx");
    let r = validate(&mut exec, json!({}));
    let missing = only(&r, "missing_package_item");
    assert_eq!(missing["severity"], "warning");
    assert_eq!(missing["reason"], "header_absent");
    assert_eq!(r["checked"]["referenceIntegrity"], false);
    assert_eq!(r["checked"]["resourceDefinitions"], false);
    assert!(r["checked"]["header"].is_null());
    assert!(find(&r, "unresolved_reference").is_empty());
    assert_eq!(r["summary"]["errors"], 0);
}

#[test]
fn cursor_pages_findings_without_duplicates_or_overlap() {
    let mut body = String::new();
    for i in 0..6 {
        body.push_str(&format!(r#"<hp:bogus{i} x="1"/>"#));
    }
    let section = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?><hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph"><hp:p id="1" paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0"><hp:t>본문</hp:t></hp:run></hp:p>{body}</hs:sec>"#
    );
    let (_tmp, mut exec) = open("hae-validate-pages", build(HEADER, &section, None));
    let first = validate(&mut exec, json!({ "limit": 2 }));
    assert_eq!(first["total"], 6, "{first}");
    assert_eq!(first["items"].as_array().unwrap().len(), 2);
    let mut seen: Vec<String> = Vec::new();
    let mut cursor = first["nextCursor"].as_str().map(str::to_string);
    let mut pages = 1;
    while let Some(c) = cursor {
        let page = validate(&mut exec, json!({ "limit": 2, "cursor": c }));
        pages += 1;
        for item in page["items"].as_array().unwrap() {
            seen.push(item["message"].as_str().unwrap().to_string());
        }
        assert_eq!(page["total"], 6);
        cursor = page["nextCursor"].as_str().map(str::to_string);
        if pages > 5 {
            panic!("cursor did not terminate: {page}");
        }
    }
    assert_eq!(pages, 3);
    assert_eq!(seen.len(), 4);
    let mut all = first["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["message"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    all.extend(seen);
    let unique: std::collections::BTreeSet<_> = all.iter().collect();
    assert_eq!(unique.len(), 6, "duplicate findings across pages: {all:?}");

    // A cursor is bound to its selector and document revision.
    let cursor = first["nextCursor"].as_str().unwrap().to_string();
    let (code, _) = error_code(
        &mut exec,
        json!({ "limit": 2, "cursor": cursor.clone(), "maxExamples": 7 }),
    );
    assert_eq!(code, Code::VersionConflict);
    let (code, _) = error_code(&mut exec, json!({ "cursor": "nonsense" }));
    assert_eq!(code, Code::InvalidRequest);
}

#[test]
fn scope_limits_the_element_scan_but_not_document_checks() {
    let section = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?><hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph"><hp:p id="1" paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0"><hp:t>깨끗한 문단</hp:t></hp:run></hp:p><hp:p id="2" paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0"><hp:bad/></hp:run></hp:p></hs:sec>"#;
    let header = HEADER.replace(
        r#"<hh:styles itemCnt="1">"#,
        r#"<hh:styles itemCnt="2"><hh:style id="5" name="안쓰임" type="PARA"/>"#,
    );
    let (_tmp, mut exec) = open("hae-validate-scope", build(&header, section, None));

    let unscoped = validate(&mut exec, json!({}));
    assert_eq!(only(&unscoped, "unknown_element")["count"], 1);
    assert!(unscoped["scope"].is_null());
    assert_eq!(unscoped["checked"]["xmlEntries"], 4);

    let p1 = exec
        .call("element.query", json!({ "kind": "paragraph" }), None, false)
        .result
        .unwrap()["matches"][0]["nodeId"]
        .as_str()
        .unwrap()
        .to_string();
    let p2 = exec
        .call("element.query", json!({ "kind": "paragraph" }), None, false)
        .result
        .unwrap()["matches"][1]["nodeId"]
        .as_str()
        .unwrap()
        .to_string();

    let clean = validate(&mut exec, json!({ "scope": p1 }));
    assert!(find(&clean, "unknown_element").is_empty(), "{clean}");
    assert_eq!(clean["scope"], p1);
    assert!(!clean["checked"]["xmlEntries"].is_null());
    // Document-wide checks still run under a scope.
    let unused = only(&clean, "unused_resource");
    assert_eq!(unused["scope"], "document");

    let dirty = validate(&mut exec, json!({ "scope": p2 }));
    let bad = only(&dirty, "unknown_element");
    assert_eq!(bad["scope"], p2);
    assert_eq!(bad["count"], 1);

    let (code, _) = error_code(&mut exec, json!({ "scope": "n-not-a-node" }));
    assert_eq!(code, Code::TargetNotFound);
}

#[test]
fn size_limits_are_enforced() {
    let (_tmp, mut exec) = open("hae-validate-limits", build(HEADER, SECTION, None));
    let (code, _) = error_code(&mut exec, json!({ "limit": 501 }));
    assert_eq!(code, Code::InvalidRequest);
    let (code, _) = error_code(&mut exec, json!({ "limit": 0 }));
    assert_eq!(code, Code::InvalidRequest);
    let (code, _) = error_code(&mut exec, json!({ "maxExamples": 0 }));
    assert_eq!(code, Code::InvalidRequest);
    let (code, _) = error_code(&mut exec, json!({ "maxExamples": 51 }));
    assert_eq!(code, Code::InvalidRequest);
    let max = validate(&mut exec, json!({ "maxExamples": 50 }));
    assert_eq!(max["total"], 0);

    // The scan itself is bounded; the refusal names the limit.
    let (tmp, mut small) = open_bytes(
        "hae-validate-scan-limit",
        build(HEADER, SECTION, None),
        Limits {
            max_validate_elements: 20,
            ..Limits::default()
        },
    );
    let r = small.call("document.validate", json!({}), None, false);
    assert!(!r.ok);
    assert_eq!(r.diagnostics[0].code, Code::ResourceLimit);
    assert_eq!(
        r.diagnostics[0].reason.as_deref(),
        Some("validate_scan_limit")
    );
    assert_eq!(
        r.diagnostics[0].detail.as_ref().unwrap()["limit"],
        "max_validate_elements"
    );
    drop(tmp);
}

#[test]
fn validate_never_changes_the_document() {
    let (_tmp, mut exec) = open_corpus("gani-gian.hwpx");
    let before = exec
        .call("document.stats", json!({}), None, false)
        .result
        .unwrap()["revision"]
        .clone();
    let out_before = _tmp.0.join("before.hwpx");
    let r = exec.call(
        "document.export",
        json!({ "path": out_before }),
        None,
        false,
    );
    assert!(r.ok, "{:?}", r.diagnostics);
    let first = validate(&mut exec, json!({}));
    let second = validate(&mut exec, json!({}));
    assert_eq!(first["items"], second["items"], "report must be stable");
    let after = exec
        .call("document.stats", json!({}), None, false)
        .result
        .unwrap()["revision"]
        .clone();
    assert_eq!(before, after);
    let out_after = _tmp.0.join("after.hwpx");
    let r = exec.call("document.export", json!({ "path": out_after }), None, false);
    assert!(r.ok, "{:?}", r.diagnostics);
    assert_eq!(
        fs::read(&out_before).unwrap(),
        fs::read(&out_after).unwrap(),
        "document.export bytes must be identical before and after document.validate"
    );
}

#[test]
fn command_metadata_is_read_only_and_sequence_free() {
    let (_tmp, mut exec) = open("hae-validate-meta", build(HEADER, SECTION, None));
    let d = exec
        .call(
            "command.describe",
            json!({ "command": "document.validate" }),
            None,
            false,
        )
        .result
        .unwrap();
    assert_eq!(d["category"], "query");
    assert_eq!(d["mutatesContent"], false);
    assert_eq!(d["requiresDocument"], true);
    assert_eq!(d["requiresExpectedVersion"], false);
    assert_eq!(d["usesRequestSequence"], false);
    assert_eq!(d["recordsHistory"], false);
    assert_eq!(d["supportsDryRun"], false);
    assert_eq!(d["annotations"]["readOnlyHint"], true);
    assert_eq!(d["annotations"]["destructiveHint"], false);
    assert_eq!(d["annotations"]["idempotentHint"], true);
    assert_eq!(d["annotations"]["openWorldHint"], false);
    assert_eq!(d["mcpToolName"], "document_validate");
    assert_eq!(d["cliName"], "document-validate");
    assert!(d["inputSchema"]["properties"]["scope"].is_object());
    assert!(d["outputSchema"]["properties"]["summary"].is_object());
}

#[test]
fn max_examples_bounds_the_example_list() {
    let mut body = String::new();
    for i in 0..8 {
        body.push_str(&format!(r#"<hp:bogus n="{i}"/>"#));
    }
    let section = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?><hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph"><hp:p id="1" paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0"><hp:t>본문</hp:t></hp:run></hp:p>{body}</hs:sec>"#
    );
    let (_tmp, mut exec) = open("hae-validate-examples", build(HEADER, &section, None));
    let r = validate(&mut exec, json!({ "maxExamples": 3 }));
    assert_eq!(r["items"].as_array().unwrap().len(), 1, "{r}");
    let first = &r["items"][0];
    assert_eq!(first["examples"].as_array().unwrap().len(), 3);
    assert_eq!(first["examplesTruncated"], true);
    assert_eq!(first["count"], 8);
    let full = validate(&mut exec, json!({ "maxExamples": 50 }));
    assert_eq!(full["items"][0]["examples"].as_array().unwrap().len(), 8);
    assert_eq!(full["items"][0]["examplesTruncated"], false);
}
