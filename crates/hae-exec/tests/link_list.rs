//! `link.list`: HYPERLINK field listing on a synthetic fixture, the public
//! corpus reference, and byte-identity checks (a query must not change the
//! document).
use hae_diag::{Code, Limits};
use hae_exec::{ResponseEnvelope, SessionExecutor};
use serde_json::{json, Value};
use std::{fs, path::PathBuf};

struct Tmp(PathBuf);
impl Tmp {
    fn new(tag: &str) -> Self {
        let p = std::env::temp_dir().join(hae_model::new_id(tag).unwrap());
        fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/corpus")
        .canonicalize()
        .unwrap()
}

const HEADER: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?><hh:head xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head" version="1.2" secCnt="1"><hh:refList><hh:charProperties itemCnt="1"><hh:charPr id="0"/></hh:charProperties><hh:paraProperties itemCnt="1"><hh:paraPr id="0"><hh:align horizontal="LEFT" vertical="BASELINE"/><hh:lineSpacing type="PERCENT" value="160"/></hh:paraPr></hh:paraProperties><hh:styles itemCnt="1"><hh:style id="0"/></hh:styles></hh:refList></hh:head>"#;

/// Synthesized section with five HYPERLINK fields: web, bookmark, mail with an
/// `hp:script` display, a link inside a table cell, and a parameter-less field.
const LINK_SECTION: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?><hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" xmlns:hc="http://www.hancom.co.kr/hwpml/2011/core"><hp:p id="1" paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0"><hp:t>외부 링크 </hp:t><hp:ctrl><hp:fieldBegin id="2001" type="HYPERLINK" name="" editable="0" dirty="1" zorder="-1" fieldid="627600491"><hp:parameters cnt="6" name=""><hp:integerParam name="Prop">0</hp:integerParam><hp:stringParam name="Command">https\://example.com/a\?b=c;1;5;-1;</hp:stringParam><hp:stringParam name="Path">https://example.com/a?b=c</hp:stringParam><hp:stringParam name="Category">HWPHYPERLINK_TYPE_URL</hp:stringParam><hp:stringParam name="TargetType">HWPHYPERLINK_TARGET_HYPERLINK</hp:stringParam><hp:stringParam name="DocOpenType">HWPHYPERLINK_JUMP_DONTCARE</hp:stringParam></hp:parameters></hp:fieldBegin></hp:ctrl><hp:t>예시</hp:t><hp:ctrl><hp:fieldEnd beginIDRef="2001" fieldid="627600491"/></hp:ctrl></hp:run><hp:linesegarray><hp:lineseg textpos="0"/></hp:linesegarray></hp:p><hp:p id="2" paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0"><hp:ctrl><hp:fieldBegin id="2002" type="HYPERLINK" name="" editable="0" fieldid="627600491"><hp:parameters cnt="3" name=""><hp:stringParam name="Command">?책갈피1;0;0;0;</hp:stringParam><hp:stringParam name="Category">HWPHYPERLINK_TYPE_HWP</hp:stringParam><hp:stringParam name="TargetType">HWPHYPERLINK_TARGET_BOOKMARK</hp:stringParam></hp:parameters></hp:fieldBegin></hp:ctrl><hp:t>책갈피로</hp:t><hp:ctrl><hp:fieldEnd beginIDRef="2002"/></hp:ctrl></hp:run><hp:linesegarray><hp:lineseg textpos="0"/></hp:linesegarray></hp:p><hp:p id="3" paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0"><hp:ctrl><hp:fieldBegin id="2003" type="HYPERLINK" name="메일" editable="0" fieldid="627600491"><hp:parameters cnt="3" name=""><hp:stringParam name="Command">mailto:someone@example.com;2;0;0</hp:stringParam><hp:stringParam name="Path">mailto:someone@example.com</hp:stringParam><hp:stringParam name="Category">HWPHYPERLINK_TYPE_EMAIL</hp:stringParam></hp:parameters><hp:script>메일 보내기</hp:script></hp:fieldBegin></hp:ctrl><hp:ctrl><hp:fieldEnd beginIDRef="2003"/></hp:ctrl><hp:t/></hp:run><hp:linesegarray><hp:lineseg textpos="0"/></hp:linesegarray></hp:p><hp:p id="4" paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0"><hp:tbl id="500" rowCnt="1" colCnt="1"><hp:sz width="5000" height="1000"/><hp:tr><hp:tc><hp:cellAddr colAddr="0" rowAddr="0"/><hp:cellSpan colSpan="1" rowSpan="1"/><hp:subList><hp:p id="41" paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0"><hp:ctrl><hp:fieldBegin id="2004" type="HYPERLINK" name="" editable="0" fieldid="627600491"><hp:parameters cnt="2" name=""><hp:stringParam name="Path">https://example.org/cell</hp:stringParam><hp:stringParam name="Category">HWPHYPERLINK_TYPE_URL</hp:stringParam></hp:parameters></hp:fieldBegin></hp:ctrl><hp:t>셀 링크</hp:t><hp:ctrl><hp:fieldEnd beginIDRef="2004"/></hp:ctrl></hp:run></hp:p></hp:subList></hp:tc></hp:tr></hp:tbl></hp:run><hp:linesegarray><hp:lineseg textpos="0"/></hp:linesegarray></hp:p><hp:p id="5" paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0"><hp:ctrl><hp:fieldBegin id="2005" type="HYPERLINK" name="" fieldid="627600491"/></hp:ctrl><hp:t>대상 없음</hp:t><hp:ctrl><hp:fieldEnd beginIDRef="2005"/></hp:ctrl></hp:run><hp:linesegarray><hp:lineseg textpos="0"/></hp:linesegarray></hp:p></hs:sec>"#;

const PLAIN_SECTION: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?><hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" xmlns:hc="http://www.hancom.co.kr/hwpml/2011/core"><hp:p id="1" paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0"><hp:t>링크 없음</hp:t></hp:run><hp:linesegarray><hp:lineseg textpos="0"/></hp:linesegarray></hp:p></hs:sec>"#;

/// Rebuild the minimal valid base package with the given section/header.
fn hwpx(section: &str, header: &str) -> Vec<u8> {
    let base = include_bytes!("../../../tests/fixtures/adversarial/valid_base.hwpx").to_vec();
    let limits = Limits::default();
    let archive = hae_zip::Archive::open(base, &limits).unwrap();
    let mut plan = hae_zip::keep_all(&archive);
    for item in &mut plan {
        match item {
            hae_zip::OutEntry::Keep(n) if n == "Contents/section0.xml" => {
                *item =
                    hae_zip::OutEntry::Replace(n.clone(), std::sync::Arc::from(section.as_bytes()));
            }
            hae_zip::OutEntry::Keep(n) if n == "Contents/header.xml" => {
                *item =
                    hae_zip::OutEntry::Replace(n.clone(), std::sync::Arc::from(header.as_bytes()));
            }
            _ => {}
        }
    }
    hae_zip::write(&archive, &plan).unwrap()
}

fn code_of(r: &ResponseEnvelope) -> Code {
    assert!(!r.ok, "expected an error response: {r:?}");
    r.diagnostics[0].code
}

fn open(dir: &Tmp, bytes: &[u8], name: &str) -> (SessionExecutor, PathBuf) {
    let src = dir.0.join(name);
    fs::write(&src, bytes).unwrap();
    let mut exec = SessionExecutor::new(&dir.0, &dir.0, Limits::default()).unwrap();
    let response = exec.call("document.open", json!({ "path": src }), None, false);
    assert!(response.ok, "{:?}", response.diagnostics);
    (exec, src)
}

fn link_list(exec: &mut SessionExecutor, params: Value) -> Value {
    let response = exec.call("link.list", params, None, false);
    assert!(response.ok, "{:?}", response.diagnostics);
    response.result.unwrap()
}

fn node_of(exec: &mut SessionExecutor, kind: &str, text: &str) -> String {
    let response = exec.call(
        "element.query",
        json!({ "kind": kind, "textContains": text }),
        None,
        false,
    );
    assert!(response.ok, "{:?}", response.diagnostics);
    response.result.unwrap()["matches"][0]["nodeId"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn synthetic_links_report_markers_parameters_display_text_and_reference_scope() {
    let dir = Tmp::new("hae-link-list-");
    let (mut exec, _) = open(&dir, &hwpx(LINK_SECTION, HEADER), "links.hwpx");

    let result = link_list(&mut exec, json!({}));
    let links = result["links"].as_array().unwrap();
    assert_eq!(result["total"], 5);
    assert_eq!(links.len(), 5);
    assert_eq!(result["nextCursor"], Value::Null); // 5 <= default page size
    let ids: Vec<&str> = links
        .iter()
        .map(|l| l["beginId"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["2001", "2002", "2003", "2004", "2005"]);
    for link in links {
        assert_eq!(link["fieldType"], "HYPERLINK");
        assert_eq!(link["fieldId"], "627600491");
        // Every link's paragraph nodeId resolves to a real paragraph.
        let node_id = link["nodeId"].as_str().unwrap();
        let get = exec.call("element.get", json!({ "target": node_id }), None, false);
        assert!(get.ok, "{node_id}: {:?}", get.diagnostics);
        assert_eq!(get.result.unwrap()["outline"]["kind"], "paragraph");
    }
    // `editable` is reported verbatim; the parameter-less field omits it.
    assert_eq!(links[0]["editable"], false);
    assert_eq!(links[4]["editable"], Value::Null);

    // 1: web link, Hancom's escaped Command kept raw, Path repeated as target.
    let web = &links[0];
    assert_eq!(web["displayText"], "예시");
    assert_eq!(web["displayTextSource"], "between");
    assert_eq!(web["reference"]["scope"], "external");
    assert_eq!(web["reference"]["target"], "https://example.com/a?b=c");
    assert_eq!(web["reference"]["category"], "HWPHYPERLINK_TYPE_URL");
    assert_eq!(
        web["parameters"][1],
        json!({"kind": "stringParam", "name": "Command", "value": "https\\://example.com/a\\?b=c;1;5;-1;"})
    );
    assert_eq!(
        web["parameters"][2],
        json!({"kind": "stringParam", "name": "Path", "value": "https://example.com/a?b=c"})
    );

    // 2: bookmark, in-document target with Hancom's leading `?`.
    let bookmark = &links[1];
    assert_eq!(bookmark["reference"]["scope"], "internal");
    assert_eq!(bookmark["reference"]["target"], "?책갈피1;0;0;0;");
    assert_eq!(bookmark["displayText"], "책갈피로");
    assert_eq!(bookmark["displayTextSource"], "between");
    assert_eq!(bookmark["name"], Value::Null);

    // 3: mail link whose display text is an hp:script child.
    let mail = &links[2];
    assert_eq!(mail["reference"]["scope"], "external");
    assert_eq!(mail["reference"]["target"], "mailto:someone@example.com");
    assert_eq!(mail["displayText"], "메일 보내기");
    assert_eq!(mail["displayTextSource"], "script");
    assert_eq!(mail["name"], "메일");

    // 4: a link inside a table cell: nodeId is the cell paragraph.
    let cell = &links[3];
    assert_eq!(cell["reference"]["scope"], "external");
    assert_eq!(cell["reference"]["target"], "https://example.org/cell");
    let cell_node = cell["nodeId"].as_str().unwrap();
    let get = exec.call("element.get", json!({ "target": cell_node }), None, false);
    assert!(get.result.unwrap()["text"]
        .as_str()
        .unwrap()
        .contains("셀 링크"));

    // 5: no parameters at all: the reference stays unknown, no guess.
    let bare = &links[4];
    assert_eq!(bare["reference"]["scope"], "unknown");
    assert_eq!(bare["reference"]["target"], Value::Null);
    assert_eq!(bare["displayText"], "대상 없음");
    assert_eq!(bare["displayTextSource"], "between");
}

#[test]
fn link_list_scope_cursor_invalidation_and_invalid_cursor_rules() {
    let dir = Tmp::new("hae-link-scope-");
    let (mut exec, _) = open(&dir, &hwpx(LINK_SECTION, HEADER), "links.hwpx");

    // Scope: only the named node and its descendants.
    let first = node_of(&mut exec, "paragraph", "외부 링크");
    let scoped = link_list(&mut exec, json!({ "scope": first }));
    assert_eq!(scoped["total"], 1);
    assert_eq!(scoped["links"][0]["beginId"], "2001");

    let table = {
        let response = exec.call("element.query", json!({ "kind": "table" }), None, false);
        response.result.unwrap()["matches"][0]["nodeId"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let cell_scoped = link_list(&mut exec, json!({ "scope": table }));
    assert_eq!(cell_scoped["total"], 1);
    assert_eq!(cell_scoped["links"][0]["beginId"], "2004");

    let missing = exec.call(
        "link.list",
        json!({ "scope": "n-does-not-exist" }),
        None,
        false,
    );
    assert_eq!(code_of(&missing), Code::TargetNotFound);

    // Cursor paging: 2 + 2 + 1.
    let page1 = link_list(&mut exec, json!({ "limit": 2 }));
    assert_eq!(page1["total"], 5);
    assert_eq!(page1["links"].as_array().unwrap().len(), 2);
    assert_eq!(page1["nextCursor"], "0:2");
    let page2 = link_list(&mut exec, json!({ "cursor": "0:2", "limit": 2 }));
    assert_eq!(page2["links"][0]["beginId"], "2003");
    assert_eq!(page2["nextCursor"], "0:4");
    let page3 = link_list(&mut exec, json!({ "cursor": "0:4" }));
    assert_eq!(page3["links"].as_array().unwrap().len(), 1);
    assert_eq!(page3["nextCursor"], Value::Null);

    // A malformed cursor is a request error, not a silent restart.
    let bad = exec.call("link.list", json!({ "cursor": "abc" }), None, false);
    assert_eq!(code_of(&bad), Code::InvalidRequest);

    // A cursor from another revision is refused after the document changes.
    let edited = exec.call(
        "text.replace",
        json!({ "target": first, "range": { "start": 0, "end": 2 }, "text": "내부" }),
        Some(0),
        false,
    );
    assert!(edited.ok, "{:?}", edited.diagnostics);
    let stale = exec.call("link.list", json!({ "cursor": "0:2" }), None, false);
    assert_eq!(code_of(&stale), Code::VersionConflict);
    let fresh = link_list(&mut exec, json!({ "cursor": "1:2" }));
    assert_eq!(fresh["revision"], 1);
    assert_eq!(fresh["links"][0]["beginId"], "2003");
}

#[test]
fn link_list_finds_the_three_corpus_links_and_never_changes_the_bytes() {
    let dir = Tmp::new("hae-link-corpus-");
    let source = corpus_dir().join("research-school-plan-reference.hwpx");
    let original = fs::read(&source).unwrap();
    let mut exec = SessionExecutor::new(corpus_dir(), &dir.0, Limits::default()).unwrap();
    let opened = exec.call("document.open", json!({ "path": source }), None, false);
    assert!(opened.ok, "{:?}", opened.diagnostics);

    let before = dir.0.join("before.hwpx");
    assert!(
        exec.call("document.export", json!({ "path": before }), None, false)
            .ok
    );
    assert_eq!(fs::read(&before).unwrap(), original);

    let result = link_list(&mut exec, json!({}));
    let links = result["links"].as_array().unwrap();
    assert_eq!(result["total"], 3);
    let targets: Vec<&str> = links
        .iter()
        .map(|l| l["reference"]["target"].as_str().unwrap())
        .collect();
    assert_eq!(
        targets,
        [
            "http://www.kca.go.kr/",
            "http://www.fss.or.kr/",
            "http://www.taxmuseum.go.kr/"
        ]
    );
    for link in links {
        assert_eq!(link["section"], 27);
        assert_eq!(link["entry"], "Contents/section27.xml");
        assert_eq!(link["reference"]["scope"], "external");
        assert_eq!(link["reference"]["category"], "HWPHYPERLINK_TYPE_URL");
        // Hancom wrote both markers next to each other in this file, so the
        // text between them is empty; the view reports that instead of guessing
        // which nearby run the link text belongs to.
        assert_eq!(link["displayText"], "");
        assert_eq!(link["displayTextSource"], "between");
        assert!(link["nodeId"].is_string());
    }

    let after = dir.0.join("after.hwpx");
    assert!(
        exec.call("document.export", json!({ "path": after }), None, false)
            .ok
    );
    assert_eq!(fs::read(&after).unwrap(), original);
    assert_eq!(fs::read(&after).unwrap(), fs::read(&before).unwrap());
}

#[test]
fn link_list_empty_document_and_schema_contract() {
    let dir = Tmp::new("hae-link-empty-");
    let (mut exec, _) = open(&dir, &hwpx(PLAIN_SECTION, HEADER), "plain.hwpx");
    let result = link_list(&mut exec, json!({}));
    assert_eq!(result["total"], 0);
    assert_eq!(result["links"], json!([]));
    assert_eq!(result["nextCursor"], Value::Null);

    let described = exec.call(
        "command.describe",
        json!({ "command": "link.list" }),
        None,
        false,
    );
    let descriptor = described.result.unwrap();
    assert_eq!(descriptor["category"], "query");
    assert_eq!(descriptor["mutatesContent"], false);
    assert_eq!(descriptor["requiresDocument"], true);
    assert_eq!(descriptor["supportsDryRun"], false);
    for field in ["cursor", "limit", "scope"] {
        assert!(
            descriptor["inputSchema"]["properties"][field].is_object(),
            "missing input property {field}"
        );
    }
    for field in ["links", "total", "nextCursor"] {
        assert!(
            descriptor["outputSchema"]["properties"][field].is_object(),
            "missing output property {field}"
        );
    }

    // A query without an open document fails like the other document tools.
    let out = Tmp::new("hae-link-nodoc-");
    let mut empty = SessionExecutor::new(&out.0, &out.0, Limits::default()).unwrap();
    let response = empty.call("link.list", json!({}), None, false);
    assert_eq!(code_of(&response), Code::SessionRequired);
}
