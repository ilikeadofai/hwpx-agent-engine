//! `trackchange.list`: revision-mark listing on synthetic fixtures.
//!
//! No local input contains track changes (checked across `tests/fixtures` and
//! `spikes`), so the mark/author linkage is exercised on a synthetic document
//! whose header follows the published `hh:trackChanges`/`hh:trackChangeAuthors`
//! schema and whose body uses the KS X 6101 inline marks
//! (`hp:insertBegin`/`hp:insertEnd`/`hp:deleteBegin`/`hp:deleteEnd`).
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

const TRACK_HEADER: &str = r##"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?><hh:head xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head" version="1.2" secCnt="1"><hh:refList><hh:charProperties itemCnt="1"><hh:charPr id="0"/></hh:charProperties><hh:paraProperties itemCnt="1"><hh:paraPr id="0"><hh:align horizontal="LEFT" vertical="BASELINE"/><hh:lineSpacing type="PERCENT" value="160"/></hh:paraPr></hh:paraProperties><hh:styles itemCnt="1"><hh:style id="0"/></hh:styles><hh:trackChanges itemCnt="2"><hh:trackChange id="1" type="Insert" date="2026-01-02T03:04:05Z" authorID="0" hide="0"/><hh:trackChange id="2" type="Delete" date="2026-01-03T03:04:05Z" authorID="1" hide="0"/></hh:trackChanges><hh:trackChangeAuthors itemCnt="2"><hh:trackChangeAuthor id="0" name="김철수" mark="0" color="#FF0000"/><hh:trackChangeAuthor id="1" name="이영희" mark="1" color="#0000FF"/></hh:trackChangeAuthors></hh:refList></hh:head>"##;

const PLAIN_HEADER: &str = r##"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?><hh:head xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head" version="1.2" secCnt="1"><hh:refList><hh:charProperties itemCnt="1"><hh:charPr id="0"/></hh:charProperties><hh:paraProperties itemCnt="1"><hh:paraPr id="0"><hh:align horizontal="LEFT" vertical="BASELINE"/><hh:lineSpacing type="PERCENT" value="160"/></hh:paraPr></hh:paraProperties><hh:styles itemCnt="1"><hh:style id="0"/></hh:styles></hh:refList></hh:head>"##;

/// Insert marks (change 1, 김철수) in two paragraphs, delete marks
/// (change 2, 이영희) in one, and one pair inside a table cell.
const MARK_SECTION: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?><hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" xmlns:hc="http://www.hancom.co.kr/hwpml/2011/core"><hp:p id="1" paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0"><hp:t>가나:<hp:insertBegin Id="1" TcId="1"/>다라<hp:insertEnd Id="1" TcId="1" paraend="false"/></hp:t></hp:run><hp:linesegarray><hp:lineseg textpos="0"/></hp:linesegarray></hp:p><hp:p id="2" paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0"><hp:t>마:<hp:deleteBegin Id="2" TcId="2"/>바사<hp:deleteEnd Id="2" TcId="2" paraend="true"/></hp:t></hp:run><hp:linesegarray><hp:lineseg textpos="0"/></hp:linesegarray></hp:p><hp:p id="3" paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0"><hp:tbl id="500" rowCnt="1" colCnt="1"><hp:sz width="5000" height="1000"/><hp:tr><hp:tc><hp:cellAddr colAddr="0" rowAddr="0"/><hp:cellSpan colSpan="1" rowSpan="1"/><hp:subList><hp:p id="41" paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0"><hp:t>셀:<hp:insertBegin Id="3" TcId="1"/>추가<hp:insertEnd Id="4" TcId="1"/></hp:t></hp:run></hp:p></hp:subList></hp:tc></hp:tr></hp:tbl></hp:run><hp:linesegarray><hp:lineseg textpos="0"/></hp:linesegarray></hp:p></hs:sec>"#;

/// Header records without any body mark (change records kept, marks accepted).
const HEADER_ONLY_SECTION: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?><hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" xmlns:hc="http://www.hancom.co.kr/hwpml/2011/core"><hp:p id="1" paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0"><hp:t>기록만 있음</hp:t></hp:run><hp:linesegarray><hp:lineseg textpos="0"/></hp:linesegarray></hp:p></hs:sec>"#;

/// A body-level `hp:trackChange` element. KS X 6101 defines only the four
/// inline marks, but the command accepts this spelling defensively so an
/// unexpected mark is reported instead of dropped.
const TRACKCHANGE_ELEMENT_SECTION: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?><hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" xmlns:hc="http://www.hancom.co.kr/hwpml/2011/core"><hp:p id="1" paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0"><hp:t>본문</hp:t><hp:ctrl><hp:trackChange Id="7" TcId="2"/></hp:ctrl></hp:run><hp:linesegarray><hp:lineseg textpos="0"/></hp:linesegarray></hp:p></hs:sec>"#;

const PLAIN_SECTION: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?><hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" xmlns:hc="http://www.hancom.co.kr/hwpml/2011/core"><hp:p id="1" paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0"><hp:t>변경 추적 없음</hp:t></hp:run><hp:linesegarray><hp:lineseg textpos="0"/></hp:linesegarray></hp:p></hs:sec>"#;

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

fn open(dir: &Tmp, bytes: &[u8]) -> SessionExecutor {
    let src = dir.0.join("doc.hwpx");
    fs::write(&src, bytes).unwrap();
    let mut exec = SessionExecutor::new(&dir.0, &dir.0, Limits::default()).unwrap();
    let response = exec.call("document.open", json!({ "path": src }), None, false);
    assert!(response.ok, "{:?}", response.diagnostics);
    exec
}

fn list(exec: &mut SessionExecutor, params: Value) -> Value {
    let response = exec.call("trackchange.list", params, None, false);
    assert!(response.ok, "{:?}", response.diagnostics);
    response.result.unwrap()
}

#[test]
fn synthetic_marks_resolve_kind_author_position_and_paragraph() {
    let dir = Tmp::new("hae-tc-list-");
    let mut exec = open(&dir, &hwpx(MARK_SECTION, TRACK_HEADER));

    let opened = exec.call("document.inspect", json!({ "limit": 1 }), None, false);
    assert_eq!(opened.result.unwrap()["editable"], false);

    let result = list(&mut exec, json!({}));
    assert_eq!(result["hasTrackChanges"], true);
    assert_eq!(result["readonlyReasons"], json!(["track_changes"]));
    assert_eq!(result["markCount"], 6);
    assert_eq!(result["nextCursor"], Value::Null);

    // Header records, in header order, with the author name resolved.
    assert_eq!(
        result["changes"],
        json!([
            {
                "id": "1", "type": "Insert", "date": "2026-01-02T03:04:05Z",
                "authorId": "0", "authorName": "김철수", "hide": false
            },
            {
                "id": "2", "type": "Delete", "date": "2026-01-03T03:04:05Z",
                "authorId": "1", "authorName": "이영희", "hide": false
            }
        ])
    );
    assert_eq!(
        result["authors"],
        json!([
            {"id": "0", "name": "김철수", "mark": false, "color": "#FF0000"},
            {"id": "1", "name": "이영희", "mark": true, "color": "#0000FF"}
        ])
    );

    let marks = result["marks"].as_array().unwrap();
    let kinds: Vec<&str> = marks.iter().map(|m| m["kind"].as_str().unwrap()).collect();
    assert_eq!(
        kinds,
        [
            "insertBegin",
            "insertEnd",
            "deleteBegin",
            "deleteEnd",
            "insertBegin",
            "insertEnd"
        ]
    );
    let ids: Vec<&str> = marks.iter().map(|m| m["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["1", "1", "2", "2", "3", "4"]);
    let tc_ids: Vec<&str> = marks.iter().map(|m| m["tcId"].as_str().unwrap()).collect();
    assert_eq!(tc_ids, ["1", "1", "2", "2", "1", "1"]);
    let offsets: Vec<u64> = marks
        .iter()
        .map(|m| m["textOffset"].as_u64().unwrap())
        .collect();
    // Codepoint offsets inside each hp:t: "가나:" | "가나:다라" | "마:" |
    // "마:바사" | "셀:" | "셀:추가".
    assert_eq!(offsets, [3, 5, 2, 4, 2, 4]);
    // Only the end marks that carry the attribute report paraEnd.
    assert_eq!(marks[0]["paraEnd"], Value::Null);
    assert_eq!(marks[1]["paraEnd"], false);
    assert_eq!(marks[3]["paraEnd"], true);
    assert_eq!(marks[5]["paraEnd"], Value::Null);
    // Kind and author come from the header record named by TcId.
    assert_eq!(marks[0]["changeType"], "Insert");
    assert_eq!(marks[0]["authorName"], "김철수");
    assert_eq!(marks[2]["changeType"], "Delete");
    assert_eq!(marks[2]["authorName"], "이영희");
    assert_eq!(marks[4]["authorName"], "김철수");

    // Paragraph linkage: the first two marks share one paragraph, the cell
    // pair another, and every nodeId resolves.
    assert_eq!(marks[0]["nodeId"], marks[1]["nodeId"]);
    assert_ne!(marks[0]["nodeId"], marks[2]["nodeId"]);
    assert_eq!(marks[4]["nodeId"], marks[5]["nodeId"]);
    for mark in marks {
        assert_eq!(mark["section"], 0);
        assert_eq!(mark["entry"], "Contents/section0.xml");
        let node_id = mark["nodeId"].as_str().unwrap();
        let get = exec.call("element.get", json!({ "target": node_id }), None, false);
        assert!(get.ok, "{node_id}: {:?}", get.diagnostics);
        assert_eq!(get.result.unwrap()["outline"]["kind"], "paragraph");
    }
}

#[test]
fn track_change_documents_stay_read_only_and_queries_leave_bytes_alone() {
    let dir = Tmp::new("hae-tc-lock-");
    let bytes = hwpx(MARK_SECTION, TRACK_HEADER);
    let mut exec = open(&dir, &bytes);

    let before = dir.0.join("before.hwpx");
    assert!(
        exec.call("document.export", json!({ "path": before }), None, false)
            .ok
    );
    assert_eq!(fs::read(&before).unwrap(), bytes);

    assert_eq!(list(&mut exec, json!({}))["markCount"], 6);
    // Read-only queries stay available on the locked document.
    let links = exec.call("link.list", json!({}), None, false);
    assert!(links.ok, "{:?}", links.diagnostics);
    assert_eq!(links.result.unwrap()["total"], 0);
    let inspected = exec.call("document.inspect", json!({ "limit": 5 }), None, false);
    assert!(inspected.ok);

    // The lock is unchanged: a material edit is refused and nothing moves.
    let paragraph = exec
        .call(
            "element.query",
            json!({ "kind": "paragraph", "textContains": "가나" }),
            None,
            false,
        )
        .result
        .unwrap()["matches"][0]["nodeId"]
        .clone();
    let refused = exec.call(
        "text.replace",
        json!({ "target": paragraph, "range": { "start": 0, "end": 2 }, "text": "수정" }),
        Some(0),
        false,
    );
    assert_eq!(code_of(&refused), Code::Unsupported);
    assert_eq!(
        refused.diagnostics[0].reason.as_deref(),
        Some("document_readonly")
    );
    assert_eq!(exec.current_revision(), Some(0));

    let after = dir.0.join("after.hwpx");
    assert!(
        exec.call("document.export", json!({ "path": after }), None, false)
            .ok
    );
    assert_eq!(fs::read(&after).unwrap(), bytes);
    assert_eq!(fs::read(&after).unwrap(), fs::read(&before).unwrap());
}

#[test]
fn trackchange_scope_cursor_and_cursor_error_rules() {
    let dir = Tmp::new("hae-tc-scope-");
    let mut exec = open(&dir, &hwpx(MARK_SECTION, TRACK_HEADER));

    let paragraphs: Vec<String> = exec
        .call("element.query", json!({ "kind": "paragraph" }), None, false)
        .result
        .unwrap()["matches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["nodeId"].as_str().unwrap().to_owned())
        .collect();
    let table = exec
        .call("element.query", json!({ "kind": "table" }), None, false)
        .result
        .unwrap()["matches"][0]["nodeId"]
        .clone();

    // Scope by paragraph, by table, and by cell (the cell has one paragraph).
    let first = list(&mut exec, json!({ "scope": paragraphs[0] }));
    assert_eq!(first["markCount"], 2);
    assert_eq!(first["marks"][0]["kind"], "insertBegin");
    let second = list(&mut exec, json!({ "scope": paragraphs[1] }));
    assert_eq!(second["markCount"], 2);
    assert_eq!(second["marks"][0]["kind"], "deleteBegin");
    let in_table = list(&mut exec, json!({ "scope": table }));
    assert_eq!(in_table["markCount"], 2);
    assert_eq!(in_table["marks"][0]["id"], "3");
    let cell = exec
        .call(
            "element.query",
            json!({ "kind": "cell", "table": table, "row": 0, "col": 0 }),
            None,
            false,
        )
        .result
        .unwrap()["matches"][0]["nodeId"]
        .clone();
    assert_eq!(list(&mut exec, json!({ "scope": cell }))["markCount"], 2);

    let missing = exec.call(
        "trackchange.list",
        json!({ "scope": "n-missing" }),
        None,
        false,
    );
    assert_eq!(code_of(&missing), Code::TargetNotFound);

    // Cursor paging: 4 + 2.
    let page1 = list(&mut exec, json!({ "limit": 4 }));
    assert_eq!(page1["markCount"], 6);
    assert_eq!(page1["marks"].as_array().unwrap().len(), 4);
    assert_eq!(page1["nextCursor"], "0:4");
    // `changes` and `authors` describe the document, not the page.
    assert_eq!(page1["changes"].as_array().unwrap().len(), 2);
    assert_eq!(page1["authors"].as_array().unwrap().len(), 2);
    let page2 = list(&mut exec, json!({ "cursor": "0:4" }));
    assert_eq!(page2["marks"].as_array().unwrap().len(), 2);
    assert_eq!(page2["marks"][0]["id"], "3");
    assert_eq!(page2["nextCursor"], Value::Null);

    let malformed = exec.call("trackchange.list", json!({ "cursor": "x" }), None, false);
    assert_eq!(code_of(&malformed), Code::InvalidRequest);
    // Cursors carry the revision; a mismatch is refused, never re-paged.
    let stale = exec.call("trackchange.list", json!({ "cursor": "9:0" }), None, false);
    assert_eq!(code_of(&stale), Code::VersionConflict);
}

#[test]
fn documents_without_marks_report_empty_and_keep_editing_available() {
    let dir = Tmp::new("hae-tc-plain-");
    let mut exec = open(&dir, &hwpx(PLAIN_SECTION, PLAIN_HEADER));
    let result = list(&mut exec, json!({}));
    assert_eq!(result["hasTrackChanges"], false);
    assert_eq!(result["readonlyReasons"], json!([]));
    assert_eq!(result["markCount"], 0);
    assert_eq!(result["marks"], json!([]));
    assert_eq!(result["changes"], json!([]));
    assert_eq!(result["authors"], json!([]));
    assert_eq!(result["nextCursor"], Value::Null);

    // No lock: an ordinary edit still works.
    let paragraph = exec
        .call("element.query", json!({ "kind": "paragraph" }), None, false)
        .result
        .unwrap()["matches"][0]["nodeId"]
        .clone();
    let edited = exec.call(
        "text.replace",
        json!({ "target": paragraph, "range": { "start": 0, "end": 2 }, "text": "수정" }),
        Some(0),
        false,
    );
    assert!(edited.ok, "{:?}", edited.diagnostics);
    assert_eq!(exec.current_revision(), Some(1));

    // Header records without body marks: reported and still locked.
    let header_only = Tmp::new("hae-tc-headeronly-");
    let mut exec = open(&header_only, &hwpx(HEADER_ONLY_SECTION, TRACK_HEADER));
    let result = list(&mut exec, json!({}));
    assert_eq!(result["hasTrackChanges"], true);
    assert_eq!(result["markCount"], 0);
    assert_eq!(result["marks"], json!([]));
    assert_eq!(result["changes"].as_array().unwrap().len(), 2);
    assert_eq!(result["readonlyReasons"], json!(["track_changes"]));

    // A body-level hp:trackChange is reported instead of dropped; it is not
    // inside an hp:t, so there is no text offset.
    let element = Tmp::new("hae-tc-element-");
    let mut exec = open(&element, &hwpx(TRACKCHANGE_ELEMENT_SECTION, TRACK_HEADER));
    let result = list(&mut exec, json!({}));
    assert_eq!(result["markCount"], 1);
    let mark = &result["marks"][0];
    assert_eq!(mark["kind"], "trackChange");
    assert_eq!(mark["id"], "7");
    assert_eq!(mark["tcId"], "2");
    assert_eq!(mark["changeType"], "Delete");
    assert_eq!(mark["authorName"], "이영희");
    assert_eq!(mark["textOffset"], Value::Null);
    assert!(mark["nodeId"].is_string());
}

#[test]
fn trackchange_list_schema_contract_and_session_requirement() {
    let dir = Tmp::new("hae-tc-schema-");
    let mut exec = open(&dir, &hwpx(PLAIN_SECTION, PLAIN_HEADER));
    let descriptor = exec
        .call(
            "command.describe",
            json!({ "command": "trackchange.list" }),
            None,
            false,
        )
        .result
        .unwrap();
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
    for field in [
        "marks",
        "markCount",
        "nextCursor",
        "hasTrackChanges",
        "readonlyReasons",
        "changes",
        "authors",
    ] {
        assert!(
            descriptor["outputSchema"]["properties"][field].is_object(),
            "missing output property {field}"
        );
    }

    let out = Tmp::new("hae-tc-nodoc-");
    let mut empty = SessionExecutor::new(&out.0, &out.0, Limits::default()).unwrap();
    let response = empty.call("trackchange.list", json!({}), None, false);
    assert_eq!(code_of(&response), Code::SessionRequired);
}
