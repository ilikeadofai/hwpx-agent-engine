//! `bookmark.list` and `memo.list`: marker discovery on synthetic fixtures, the
//! local real memo input, cursor/scope rules, and byte-identity checks (a query
//! must never change the document).
use hae_diag::{Code, Limits};
use hae_exec::{command_describe, CommandCategory, ResponseEnvelope, SessionExecutor};
use serde_json::{json, Value};
use std::{fs, path::Path, path::PathBuf};

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

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture_path(rel: &str) -> PathBuf {
    root().join(rel).canonicalize().expect("fixture must exist")
}

const HEADER: &str = r##"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?><hh:head xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head" xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" version="1.2" secCnt="1"><hh:refList><hh:charProperties itemCnt="1"><hh:charPr id="0"/></hh:charProperties><hh:paraProperties itemCnt="1"><hh:paraPr id="0"><hh:align horizontal="LEFT" vertical="BASELINE"/><hh:lineSpacing type="PERCENT" value="160"/></hh:paraPr></hh:paraProperties><hh:styles itemCnt="1"><hh:style id="0"/></hh:styles><hh:borderFills itemCnt="1"><hh:borderFill id="1"><hh:leftBorder type="SOLID"/><hh:rightBorder type="SOLID"/><hh:topBorder type="SOLID"/><hh:bottomBorder type="SOLID"/></hh:borderFill></hh:borderFills><hh:memoProperties itemCnt="2"><hh:memoPr id="7" width="15024" lineWidth="1" lineType="SOLID" lineColor="#000000" fillColor="#CCFF99" activeColor="#FFFF99" memoType="NOMAL"/><hh:memoPr id="8" width="14000" lineWidth="0.6mm" lineType="DOT" lineColor="#C0504D" fillColor="#FDE9E9" activeColor="#F6D8D8" memoType="NORMAL"/></hh:memoProperties></hh:refList></hh:head>"##;

fn p(id: u32, inner: &str) -> String {
    format!(
        r#"<hp:p id="{id}" paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0">{inner}</hp:run><hp:linesegarray><hp:lineseg textpos="0"/></hp:linesegarray></hp:p>"#
    )
}

/// Six bookmark markers: three named point markers (one name repeated), one
/// nameless point marker, one field span, and one marker inside a table cell.
const BOOKMARK_SECTION: &str = r#"<hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" xmlns:hc="http://www.hancom.co.kr/hwpml/2011/core"><hp:p id="1" paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0"><hp:t>가나다</hp:t><hp:ctrl><hp:bookmark name="지점1"/></hp:ctrl><hp:t>라마</hp:t><hp:ctrl><hp:bookmark/></hp:ctrl><hp:t>바사</hp:t><hp:ctrl><hp:bookmark name="지점1"/></hp:ctrl><hp:t>아자</hp:t></hp:run><hp:linesegarray><hp:lineseg textpos="0"/></hp:linesegarray></hp:p><hp:p id="2" paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0"><hp:ctrl><hp:fieldBegin id="3001" type="BOOKMARK" name="범위1" editable="0" fieldid="4242"><hp:parameters cnt="2" name=""><hp:integerParam name="Prop">0</hp:integerParam><hp:stringParam name="ID">범위1</hp:stringParam></hp:parameters></hp:fieldBegin></hp:ctrl><hp:t>범위 본문</hp:t><hp:ctrl><hp:fieldEnd beginIDRef="3001" fieldid="4242"/></hp:ctrl></hp:run><hp:linesegarray><hp:lineseg textpos="0"/></hp:linesegarray></hp:p><hp:p id="3" paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0"><hp:tbl rowCnt="1" colCnt="1" cellSpacing="0" borderFillIDRef="1"><hp:outMargin left="0" right="0" top="0" bottom="0"/><hp:inMargin left="0" right="0" top="0" bottom="0"/><hp:tr><hp:tc borderFillIDRef="1" hasMargin="0"><hp:cellAddr rowAddr="0" colAddr="0"/><hp:cellSpan rowSpan="1" colSpan="1"/><hp:cellSz width="1000" height="500"/><hp:subList><hp:p id="4" paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0"><hp:t>셀</hp:t><hp:ctrl><hp:bookmark name="셀지점"/></hp:ctrl></hp:run><hp:linesegarray><hp:lineseg textpos="0"/></hp:linesegarray></hp:p></hp:subList></hp:tc></hp:tr></hp:tbl></hp:run><hp:linesegarray><hp:lineseg textpos="0"/></hp:linesegarray></hp:p><hp:p id="9" paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0"><hp:t>편집 대상 문단</hp:t></hp:run><hp:linesegarray><hp:lineseg textpos="0"/></hp:linesegarray></hp:p></hs:sec>"#;

/// A paragraph holding three point markers, for cursor paging.
const PAGED_SECTION: &str = r#"<hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" xmlns:hc="http://www.hancom.co.kr/hwpml/2011/core"><hp:p id="1" paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0"><hp:t>하나</hp:t><hp:ctrl><hp:bookmark name="가"/></hp:ctrl><hp:t>둘</hp:t><hp:ctrl><hp:bookmark name="나"/></hp:ctrl><hp:t>셋</hp:t><hp:ctrl><hp:bookmark name="다"/></hp:ctrl><hp:t>넷</hp:t><hp:ctrl><hp:bookmark name="라"/></hp:ctrl><hp:t>다섯</hp:t><hp:ctrl><hp:bookmark name="마"/></hp:ctrl></hp:run><hp:linesegarray><hp:lineseg textpos="0"/></hp:linesegarray></hp:p><hp:p id="9" paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0"><hp:t>편집 대상 문단</hp:t></hp:run><hp:linesegarray><hp:lineseg textpos="0"/></hp:linesegarray></hp:p></hs:sec>"#;

const PLAIN_SECTION: &str = r#"<hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" xmlns:hc="http://www.hancom.co.kr/hwpml/2011/core"><hp:p id="1" paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0"><hp:t>주석 없음</hp:t></hp:run><hp:linesegarray><hp:lineseg textpos="0"/></hp:linesegarray></hp:p></hs:sec>"#;

fn memo_field(
    id: &str,
    number: u32,
    author: &str,
    shape: &str,
    hidden: bool,
    body: &str,
) -> String {
    let hidden = if hidden { r#" hidden="1""# } else { "" };
    let paragraphs: String = body
        .split('|')
        .map(|text| p(0, &format!("<hp:t>{text}</hp:t>")))
        .collect();
    format!(
        r#"<hp:ctrl><hp:fieldBegin id="9{number:03}" type="MEMO" name="" editable="1" dirty="1" zorder="{number}" fieldid="777"{hidden}><hp:parameters cnt="7" name=""><hp:integerParam name="Prop">0</hp:integerParam><hp:stringParam name="Command">MEMO/{shape}/{number}/120344304/31284776/{author}/\;;</hp:stringParam><hp:stringParam name="ID">{id}</hp:stringParam><hp:integerParam name="Number">{number}</hp:integerParam><hp:stringParam name="Author">{author}</hp:stringParam><hp:stringParam name="MemoShapeIDRef">{shape}</hp:stringParam><hp:stringParam name="CreateDateTime">2026-10-17T20:09:5{number}Z</hp:stringParam></hp:parameters><hp:subList id="" textDirection="HORIZONTAL" lineWrap="BREAK" vertAlign="TOP" linkListIDRef="0" linkListNextIDRef="0" textWidth="0" textHeight="0" hasTextRef="0" hasNumRef="0">{paragraphs}</hp:subList></hp:fieldBegin></hp:ctrl>"#
    )
}

fn memo_section() -> String {
    let two_paragraphs = memo_field("memo1", 1, "홍길동", "7", false, "첫째 문단.|둘째 문단.");
    let one_paragraph = memo_field("memo2", 2, "이순신", "65535", true, "짧은 메모.");
    let grouped = r#"<hp:memo id="memo3" memoShapeIDRef="8"><hp:paraList><hp:p id="0" paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0"><hp:t>그룹 메모.</hp:t></hp:run></hp:p></hp:paraList></hp:memo>"#;
    let anchor = r#"<hp:ctrl><hp:fieldBegin id="9003" type="MEMO" name="" editable="1" fieldid="777"><hp:parameters cnt="2" name=""><hp:stringParam name="ID">memo3</hp:stringParam><hp:stringParam name="Author">강감찬</hp:stringParam></hp:parameters></hp:fieldBegin></hp:ctrl>"#;
    let in_cell = memo_field("memo4", 4, "셀작성자", "7", false, "셀 메모.");
    format!(
        r#"<hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" xmlns:hc="http://www.hancom.co.kr/hwpml/2011/core">{first}{second}<hp:p id="7" paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0">{anchor}{grouped}</hp:run><hp:linesegarray><hp:lineseg textpos="0"/></hp:linesegarray></hp:p><hp:p id="8" paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0"><hp:tbl rowCnt="1" colCnt="1" cellSpacing="0" borderFillIDRef="1"><hp:outMargin left="0" right="0" top="0" bottom="0"/><hp:inMargin left="0" right="0" top="0" bottom="0"/><hp:tr><hp:tc borderFillIDRef="1" hasMargin="0"><hp:cellAddr rowAddr="0" colAddr="0"/><hp:cellSpan rowSpan="1" colSpan="1"/><hp:cellSz width="1000" height="500"/><hp:subList><hp:p id="5" paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0"><hp:t>셀</hp:t>{cell}</hp:run><hp:linesegarray><hp:lineseg textpos="0"/></hp:linesegarray></hp:p></hp:subList></hp:tc></hp:tr></hp:tbl></hp:run><hp:linesegarray><hp:lineseg textpos="0"/></hp:linesegarray></hp:p><hp:p id="9" paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0"><hp:t>편집 대상 문단</hp:t></hp:run><hp:linesegarray><hp:lineseg textpos="0"/></hp:linesegarray></hp:p></hs:sec>"#,
        first = p(
            1,
            &format!("<hp:t>메모가 달린 자리 </hp:t>{two_paragraphs}{one_paragraph}")
        ),
        second = p(2, "<hp:t>메모 없는 문단</hp:t>"),
        anchor = anchor,
        grouped = grouped,
        cell = in_cell,
    )
}

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

fn query(exec: &mut SessionExecutor, command: &str, params: Value) -> Value {
    let response = exec.call(command, params, None, false);
    assert!(response.ok, "{:?}", response.diagnostics);
    response.result.unwrap()
}

fn bookstore() -> (Tmp, SessionExecutor) {
    let dir = Tmp::new("hae-bookmark-");
    let (exec, _) = open(&dir, &hwpx(BOOKMARK_SECTION, HEADER), "bookmarks.hwpx");
    (dir, exec)
}

fn memosession() -> (Tmp, SessionExecutor) {
    let dir = Tmp::new("hae-memo-");
    let (exec, _) = open(&dir, &hwpx(&memo_section(), HEADER), "memos.hwpx");
    (dir, exec)
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

fn names(result: &Value) -> Vec<Option<&str>> {
    result["bookmarks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["name"].as_str())
        .collect()
}

// ---------------------------------------------------------------------------
// bookmark.list
// ---------------------------------------------------------------------------

#[test]
fn bookmark_list_reports_named_nameless_and_duplicate_markers_in_document_order() {
    let (_dir, mut exec) = bookstore();
    let result = query(&mut exec, "bookmark.list", json!({}));

    assert_eq!(result["revision"], 0);
    assert_eq!(result["total"], 5);
    assert_eq!(result["nextCursor"], Value::Null);
    let bookmarks = result["bookmarks"].as_array().unwrap();
    assert_eq!(bookmarks.len(), 5);

    // The nameless marker is not given a name, and the repeated name appears
    // once per marker instead of being merged or resolved to one of them.
    assert_eq!(
        names(&result),
        [
            Some("지점1"),
            None,
            Some("지점1"),
            Some("범위1"),
            Some("셀지점")
        ]
    );

    // Every marker's paragraph nodeId resolves to a real paragraph.
    for bookmark in bookmarks {
        let node_id = bookmark["nodeId"].as_str().unwrap();
        let get = exec.call("element.get", json!({ "target": node_id }), None, false);
        assert!(get.ok, "{node_id}: {:?}", get.diagnostics);
        assert_eq!(get.result.unwrap()["outline"]["kind"], "paragraph");
        assert!(bookmark["graphemeOffset"].as_u64().is_some());
    }
}

#[test]
fn bookmark_list_offsets_are_grapheme_indices_into_the_paragraph_text() {
    let (_dir, mut exec) = bookstore();
    let result = query(&mut exec, "bookmark.list", json!({}));
    let first = &result["bookmarks"][0];
    let node_id = first["nodeId"].as_str().unwrap().to_owned();
    let paragraph = query(&mut exec, "element.get", json!({ "target": node_id }));
    // The fixture parses into single-codepoint graphemes: "가나다", one control
    // placeholder per marker, "라마", "바사", "아자".
    let text = paragraph["text"].as_str().unwrap();
    assert_eq!(text, "가나다\u{FFFC}라마\u{FFFC}바사\u{FFFC}아자");
    assert_eq!(first["graphemeOffset"], 3);
    assert_eq!(result["bookmarks"][1]["graphemeOffset"], 6);
    assert_eq!(result["bookmarks"][2]["graphemeOffset"], 9);
    // The offsets address the same text `element.get` returns.
    for bookmark in result["bookmarks"].as_array().unwrap().iter().take(3) {
        let offset = bookmark["graphemeOffset"].as_u64().unwrap() as usize;
        let at = text.chars().nth(offset).unwrap();
        assert_eq!(at, '\u{FFFC}', "{offset} is not a marker position");
    }
}

#[test]
fn bookmark_list_reports_field_spans_with_end_position_and_raw_parameters() {
    let (_dir, mut exec) = bookstore();
    let result = query(&mut exec, "bookmark.list", json!({}));
    let span = &result["bookmarks"][3];

    assert_eq!(span["marker"], "field");
    assert_eq!(span["name"], "범위1");
    assert_eq!(span["id"], "3001");
    assert_eq!(span["fieldId"], "4242");
    assert_eq!(span["graphemeOffset"], 0);
    // One placeholder for the fieldBegin control plus "범위 본문" (5 graphemes).
    assert_eq!(span["endGraphemeOffset"], 6);
    let start_node = span["nodeId"].as_str().unwrap();
    assert_eq!(span["endNodeId"].as_str().unwrap(), start_node);
    let paragraph = query(&mut exec, "element.get", json!({ "target": start_node }));
    // The field markers are controls, so the paragraph text is their
    // placeholders around the covered text.
    assert_eq!(paragraph["text"], "\u{FFFC}범위 본문\u{FFFC}");
    assert_eq!(
        span["parameters"],
        json!([
            {"kind": "integerParam", "name": "Prop", "value": "0"},
            {"kind": "stringParam", "name": "ID", "value": "범위1"}
        ])
    );

    // The point markers carry no parameters and no span end at all.
    assert_eq!(result["bookmarks"][0]["parameters"], json!([]));
    assert_eq!(result["bookmarks"][0]["marker"], "bookmark");
    assert!(result["bookmarks"][0]["endNodeId"].is_null());
    assert!(result["bookmarks"][0]["endGraphemeOffset"].is_null());
}

#[test]
fn bookmark_list_scope_selects_the_named_subtree() {
    let (_dir, mut exec) = bookstore();
    let result = query(&mut exec, "bookmark.list", json!({}));

    // Paragraph scope: only markers inside that paragraph.
    let first = result["bookmarks"][0]["nodeId"]
        .as_str()
        .unwrap()
        .to_owned();
    let scoped = query(&mut exec, "bookmark.list", json!({ "scope": first }));
    assert_eq!(scoped["total"], 3);
    assert_eq!(names(&scoped), [Some("지점1"), None, Some("지점1")]);

    // The `셀지점` marker lives in the paragraph inside the table cell, so both
    // the table and the cell subtree select it.
    let table = {
        let response = exec.call("element.query", json!({ "kind": "table" }), None, false);
        response.result.unwrap()["matches"][0]["nodeId"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let cell = {
        let response = exec.call("element.query", json!({ "kind": "cell" }), None, false);
        response.result.unwrap()["matches"][0]["nodeId"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    for scope in [table, cell] {
        let scoped = query(&mut exec, "bookmark.list", json!({ "scope": scope }));
        assert_eq!(scoped["total"], 1);
        assert_eq!(names(&scoped), [Some("셀지점")]);
    }

    let missing = exec.call(
        "bookmark.list",
        json!({ "scope": "n-does-not-exist" }),
        None,
        false,
    );
    assert_eq!(code_of(&missing), Code::TargetNotFound);
}

#[test]
fn bookmark_list_pages_with_a_revision_bound_cursor() {
    let dir = Tmp::new("hae-bookmark-page-");
    let (mut exec, _) = open(&dir, &hwpx(PAGED_SECTION, HEADER), "paged.hwpx");

    let page1 = query(&mut exec, "bookmark.list", json!({ "limit": 2 }));
    assert_eq!(page1["total"], 5);
    assert_eq!(names(&page1), [Some("가"), Some("나")]);
    assert_eq!(page1["nextCursor"], "0:2");

    let page2 = query(
        &mut exec,
        "bookmark.list",
        json!({ "cursor": "0:2", "limit": 2 }),
    );
    assert_eq!(names(&page2), [Some("다"), Some("라")]);
    assert_eq!(page2["nextCursor"], "0:4");

    let page3 = query(&mut exec, "bookmark.list", json!({ "cursor": "0:4" }));
    assert_eq!(names(&page3), [Some("마")]);
    assert_eq!(page3["nextCursor"], Value::Null);

    // Past the end: an empty page, not an error, and no further cursor.
    let beyond = query(&mut exec, "bookmark.list", json!({ "cursor": "0:99" }));
    assert_eq!(beyond["bookmarks"], json!([]));
    assert_eq!(beyond["total"], 5);
    assert_eq!(beyond["nextCursor"], Value::Null);
}

#[test]
fn bookmark_list_refuses_a_malformed_cursor_and_one_from_another_revision() {
    let dir = Tmp::new("hae-bookmark-cursor-");
    let (mut exec, _) = open(&dir, &hwpx(PAGED_SECTION, HEADER), "paged.hwpx");

    for bad in [json!("abc"), json!("0"), json!("0:x"), json!("x:1")] {
        let response = exec.call("bookmark.list", json!({ "cursor": bad }), None, false);
        assert_eq!(code_of(&response), Code::InvalidRequest, "{bad}");
    }

    let stale = exec.call("bookmark.list", json!({ "cursor": "3:2" }), None, false);
    assert_eq!(code_of(&stale), Code::VersionConflict);

    // A real edit moves the revision and invalidates the old cursor; the new
    // one works and the document reports the new revision.
    let target = node_of(&mut exec, "paragraph", "편집 대상 문단");
    let edited = exec.call(
        "text.replace",
        json!({ "target": target, "range": { "start": 0, "end": 2 }, "text": "수정" }),
        Some(0),
        false,
    );
    assert!(edited.ok, "{:?}", edited.diagnostics);
    let after = exec.call("bookmark.list", json!({ "cursor": "0:2" }), None, false);
    assert_eq!(code_of(&after), Code::VersionConflict);
    let fresh = query(
        &mut exec,
        "bookmark.list",
        json!({ "cursor": "1:2", "limit": 2 }),
    );
    assert_eq!(fresh["revision"], 1);
    assert_eq!(names(&fresh), [Some("다"), Some("라")]);
}

#[test]
fn bookmark_list_answer_is_empty_for_a_document_without_bookmarks() {
    let dir = Tmp::new("hae-bookmark-empty-");
    let (mut exec, _) = open(&dir, &hwpx(PLAIN_SECTION, HEADER), "plain.hwpx");
    let result = query(&mut exec, "bookmark.list", json!({}));
    assert_eq!(result["total"], 0);
    assert_eq!(result["bookmarks"], json!([]));
    assert_eq!(result["nextCursor"], Value::Null);
}

#[test]
fn bookmark_list_never_changes_the_document_bytes() {
    let dir = Tmp::new("hae-bookmark-bytes-");
    let (mut exec, source) = open(&dir, &hwpx(BOOKMARK_SECTION, HEADER), "bookmarks.hwpx");
    let original = fs::read(&source).unwrap();

    let before = dir.0.join("before.hwpx");
    assert!(
        exec.call("document.export", json!({ "path": before }), None, false)
            .ok
    );
    assert_eq!(fs::read(&before).unwrap(), original);

    let _ = exec.call("bookmark.list", json!({}), None, false);
    let _ = exec.call("bookmark.list", json!({ "limit": 1 }), None, false);
    let _ = exec.call("bookmark.list", json!({ "cursor": "0:1" }), None, false);
    let _ = exec.call("bookmark.list", json!({ "scope": "n-nope" }), None, false);
    let _ = exec.call("memo.list", json!({}), None, false);

    let after = dir.0.join("after.hwpx");
    assert!(
        exec.call("document.export", json!({ "path": after }), None, false)
            .ok
    );
    assert_eq!(fs::read(&after).unwrap(), original);
    assert_eq!(exec.current_revision().unwrap(), 0);
}

// ---------------------------------------------------------------------------
// memo.list
// ---------------------------------------------------------------------------

#[test]
fn memo_list_reports_author_anchor_body_paragraphs_and_shape() {
    let (_dir, mut exec) = memosession();
    let result = query(&mut exec, "memo.list", json!({}));

    assert_eq!(result["revision"], 0);
    assert_eq!(result["total"], 5);
    assert_eq!(result["nextCursor"], Value::Null);
    let memos = result["memos"].as_array().unwrap();

    let first = &memos[0];
    assert_eq!(first["source"], "field");
    assert_eq!(first["id"], "memo1");
    assert_eq!(first["beginId"], "9001");
    assert_eq!(first["fieldId"], "777");
    assert_eq!(first["number"], 1);
    assert_eq!(first["rawNumber"], "1");
    assert_eq!(first["author"], "홍길동");
    assert_eq!(first["createdAt"], "2026-10-17T20:09:51Z");
    assert_eq!(first["memoShapeIdRef"], "7");
    assert_eq!(first["hidden"], Value::Null);
    // The shape is resolved by exact `hh:memoPr@id` match.
    assert_eq!(first["shape"]["memoType"], "NOMAL");
    assert_eq!(first["shape"]["fillColor"], "#CCFF99");
    assert_eq!(first["shape"]["lineWidth"], "1");

    // The anchor is the paragraph that holds the control, and the body lives in
    // the control's own subList: two paragraphs, joined text, one preview each.
    let anchor = node_of(&mut exec, "paragraph", "메모가 달린 자리");
    assert_eq!(first["paragraphNodeId"], anchor);
    let holder = query(&mut exec, "element.get", json!({ "target": anchor }));
    let marker_at = holder["text"]
        .as_str()
        .unwrap()
        .chars()
        .take_while(|c| *c != '\u{FFFC}')
        .count();
    assert_eq!(first["graphemeOffset"], marker_at);
    assert_eq!(first["graphemeOffset"], 10); // "메모가 달린 자리 " = 10 graphemes
    assert_eq!(first["paragraphCount"], 2);
    assert_eq!(first["text"], "첫째 문단.\n둘째 문단.");
    let paragraphs = first["paragraphs"].as_array().unwrap();
    assert_eq!(paragraphs.len(), 2);
    for (item, expected) in paragraphs.iter().zip(["첫째 문단.", "둘째 문단."]) {
        assert_eq!(item["text"], expected);
        assert_eq!(item["preview"], expected);
        assert_eq!(item["previewTruncated"], false);
        assert_eq!(item["graphemeCount"], 6);
        let node_id = item["nodeId"].as_str().unwrap();
        let get = query(&mut exec, "element.get", json!({ "target": node_id }));
        assert_eq!(get["text"], expected);
    }

    // A memo whose MemoShapeIDRef names no hh:memoPr stays unresolved.
    let second = &memos[1];
    assert_eq!(second["id"], "memo2");
    assert_eq!(second["memoShapeIdRef"], "65535");
    assert!(second["shape"].is_null());
    assert_eq!(second["paragraphCount"], 1);
    assert_eq!(second["text"], "짧은 메모.");
}

#[test]
fn memo_list_reports_a_stated_hidden_flag_and_leaves_unstated_ones_null() {
    let (_dir, mut exec) = memosession();
    let result = query(&mut exec, "memo.list", json!({}));
    let memos = result["memos"].as_array().unwrap();
    // `hidden="1"` on the marker is reported verbatim; markers that state
    // nothing stay null (no HWPX memo attribute set is known to carry one).
    assert_eq!(memos[0]["hidden"], Value::Null);
    assert_eq!(memos[1]["hidden"], true);
    assert_eq!(memos[2]["hidden"], Value::Null);
}

#[test]
fn memo_list_reads_a_grouped_memo_element_anchored_by_its_field() {
    let (_dir, mut exec) = memosession();
    let result = query(&mut exec, "memo.list", json!({}));
    let memos = result["memos"].as_array().unwrap();
    let grouped = memos
        .iter()
        .find(|m| m["source"] == "memo")
        .expect("the grouped hp:memo is listed");

    assert_eq!(grouped["id"], "memo3");
    assert_eq!(grouped["memoShapeIdRef"], "8");
    assert_eq!(grouped["shape"]["memoType"], "NORMAL");
    assert_eq!(grouped["paragraphCount"], 1);
    assert_eq!(grouped["text"], "그룹 메모.");
    // The anchor is the MEMO field naming `memo3`, not the `hp:memo` element
    // itself: the field control comes first, so the offset is 0 and the holder
    // paragraph carries control placeholders instead of visible text.
    let holder = query(
        &mut exec,
        "element.get",
        json!({ "target": grouped["paragraphNodeId"] }),
    );
    assert_eq!(grouped["graphemeOffset"], 0);
    assert_eq!(holder["outline"]["kind"], "paragraph");
    assert_eq!(
        holder["text"]
            .as_str()
            .unwrap()
            .chars()
            .filter(|c| *c == '\u{FFFC}')
            .count(),
        2
    );
    // The body paragraph is a distinct node inside the memo element.
    assert_ne!(
        grouped["paragraphs"][0]["nodeId"],
        grouped["paragraphNodeId"]
    );
    // `hp:memo` attributes do not name an author here, so none is invented.
    assert!(grouped["author"].is_null());
    assert!(grouped["number"].is_null());
    assert!(grouped["parameters"].as_array().unwrap().is_empty());

    // The anchoring MEMO field is a memo control of its own and is listed
    // separately (it carries no body of its own here).
    let anchor = memos
        .iter()
        .find(|m| m["source"] == "field" && m["id"] == "memo3")
        .expect("the anchor field is listed");
    assert_eq!(anchor["paragraphNodeId"], grouped["paragraphNodeId"]);
    assert_eq!(anchor["paragraphCount"], 0);
    assert_eq!(anchor["text"], "");
    // The anchor field states its own author; the grouped element states none.
    assert_eq!(anchor["author"], "강감찬");
}

#[test]
fn memo_list_scope_selects_the_named_subtree() {
    let (_dir, mut exec) = memosession();
    let anchor = node_of(&mut exec, "paragraph", "메모가 달린 자리");
    let scoped = query(&mut exec, "memo.list", json!({ "scope": anchor }));
    assert_eq!(scoped["total"], 2);
    assert_eq!(scoped["memos"][0]["id"], "memo1");
    assert_eq!(scoped["memos"][1]["id"], "memo2");

    // The cell memo is selected by its cell paragraph as well as by the table.
    let cell = {
        let response = exec.call("element.query", json!({ "kind": "cell" }), None, false);
        response.result.unwrap()["matches"][0]["nodeId"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let cell_scoped = query(&mut exec, "memo.list", json!({ "scope": cell }));
    assert_eq!(cell_scoped["total"], 1);
    assert_eq!(cell_scoped["memos"][0]["id"], "memo4");
    assert_eq!(cell_scoped["memos"][0]["author"], "셀작성자");

    let missing = exec.call("memo.list", json!({ "scope": "n-nope" }), None, false);
    assert_eq!(code_of(&missing), Code::TargetNotFound);
}

#[test]
fn memo_list_pages_with_a_revision_bound_cursor_and_refuses_a_bad_one() {
    let (_dir, mut exec) = memosession();

    let page1 = query(&mut exec, "memo.list", json!({ "limit": 3 }));
    assert_eq!(page1["total"], 5);
    assert_eq!(page1["memos"].as_array().unwrap().len(), 3);
    assert_eq!(page1["nextCursor"], "0:3");

    let page2 = query(
        &mut exec,
        "memo.list",
        json!({ "cursor": "0:3", "limit": 3 }),
    );
    assert_eq!(page2["memos"][0]["id"], "memo3");
    assert_eq!(page2["memos"][0]["source"], "memo");
    assert!(page2["nextCursor"].is_null());

    let beyond = query(&mut exec, "memo.list", json!({ "cursor": "0:50" }));
    assert_eq!(beyond["memos"], json!([]));
    assert!(beyond["nextCursor"].is_null());

    for bad in [json!("abc"), json!("0"), json!("0:x")] {
        let response = exec.call("memo.list", json!({ "cursor": bad }), None, false);
        assert_eq!(code_of(&response), Code::InvalidRequest, "{bad}");
    }

    let dir = Tmp::new("hae-memo-cursor-");
    let (mut exec, _) = open(&dir, &hwpx(&memo_section(), HEADER), "memos.hwpx");
    let target = node_of(&mut exec, "paragraph", "편집 대상 문단");
    let edited = exec.call(
        "text.replace",
        json!({ "target": target, "range": { "start": 0, "end": 2 }, "text": "수정" }),
        Some(0),
        false,
    );
    assert!(edited.ok, "{:?}", edited.diagnostics);
    let stale = exec.call("memo.list", json!({ "cursor": "0:2" }), None, false);
    assert_eq!(code_of(&stale), Code::VersionConflict);
    let fresh = query(&mut exec, "memo.list", json!({ "cursor": "1:2" }));
    assert_eq!(fresh["revision"], 1);
    assert_eq!(fresh["memos"][0]["id"], "memo3");
}

#[test]
fn memo_list_answer_is_empty_for_a_document_without_memos() {
    let dir = Tmp::new("hae-memo-empty-");
    let (mut exec, _) = open(&dir, &hwpx(PLAIN_SECTION, HEADER), "plain.hwpx");
    let result = query(&mut exec, "memo.list", json!({}));
    assert_eq!(result["total"], 0);
    assert_eq!(result["memos"], json!([]));
    assert_eq!(result["nextCursor"], Value::Null);

    // A document with memo *shapes* but no memo controls reports none either.
    let corpus = fixture_path("tests/fixtures/corpus/research-school-plan-reference.hwpx");
    let mut exec =
        SessionExecutor::new(corpus.parent().unwrap(), &dir.0, Limits::default()).expect("session");
    let opened = exec.call("document.open", json!({ "path": corpus }), None, false);
    assert!(opened.ok, "{:?}", opened.diagnostics);
    let result = query(&mut exec, "memo.list", json!({}));
    assert_eq!(result["total"], 0);
}

// ---------------------------------------------------------------------------
// Real local input and cross-cutting rules
// ---------------------------------------------------------------------------

#[test]
fn real_memo_document_lists_both_memos_and_preserves_the_bytes() {
    let doc = fixture_path("spikes/work/stage4-e2e/inputs/04_complex.hwpx");
    let original = fs::read(&doc).unwrap();
    let dir = Tmp::new("hae-memo-real-");
    let mut exec = SessionExecutor::new(doc.parent().unwrap(), &dir.0, Limits::default()).unwrap();
    let opened = exec.call("document.open", json!({ "path": &doc }), None, false);
    assert!(opened.ok, "{:?}", opened.diagnostics);

    let before = dir.0.join("before.hwpx");
    assert!(
        exec.call("document.export", json!({ "path": before }), None, false)
            .ok
    );
    assert_eq!(fs::read(&before).unwrap(), original);

    let result = query(&mut exec, "memo.list", json!({}));
    assert_eq!(result["revision"], 0);
    assert_eq!(result["total"], 2);
    let memos = result["memos"].as_array().unwrap();
    for (memo, (id, number, text)) in memos.iter().zip([
        ("memo4", 4, "메모 테스트용."),
        ("memo5", 5, "댓글이 달린다."),
    ]) {
        assert_eq!(memo["source"], "field");
        assert_eq!(memo["id"], id);
        assert_eq!(memo["number"], number);
        assert_eq!(memo["author"], "mattc");
        assert_eq!(memo["paragraphCount"], 1);
        assert_eq!(memo["text"], text);
        assert_eq!(memo["paragraphs"][0]["text"], text);
        assert!(memo["paragraphNodeId"].is_string());
        assert!(memo["graphemeOffset"].as_u64().is_some());
        // `MemoShapeIDRef=65535` matches no hh:memoPr in this document.
        assert_eq!(memo["memoShapeIdRef"], "65535");
        assert!(memo["shape"].is_null());
        assert!(memo["hidden"].is_null());
    }
    // The second memo is stored as a reply to the first.
    assert_eq!(memos[1]["parentBeginId"], memos[0]["beginId"]);

    // This document carries no bookmark.
    let bookmarks = query(&mut exec, "bookmark.list", json!({}));
    assert_eq!(bookmarks["total"], 0);
    assert_eq!(bookmarks["bookmarks"], json!([]));

    // Both memos hang off the same paragraph, and each reported offset points
    // at that memo's control placeholder in the paragraph text.
    let anchors: Vec<&str> = memos
        .iter()
        .map(|m| m["paragraphNodeId"].as_str().unwrap())
        .collect();
    assert_eq!(anchors[0], anchors[1]);
    let holder = query(&mut exec, "element.get", json!({ "target": anchors[0] }));
    let text = holder["text"].as_str().unwrap();
    for memo in memos {
        let offset = memo["graphemeOffset"].as_u64().unwrap() as usize;
        assert_eq!(text.chars().nth(offset), Some('\u{FFFC}'), "{offset}");
    }

    let after = dir.0.join("after.hwpx");
    assert!(
        exec.call("document.export", json!({ "path": after }), None, false)
            .ok
    );
    assert_eq!(fs::read(&after).unwrap(), original);
    assert_eq!(exec.current_revision().unwrap(), 0);
}

#[test]
fn annotation_queries_add_no_history_and_keep_the_revision() {
    let (_dir, mut exec) = bookstore();
    assert_eq!(
        exec.call("history.undo", json!({}), Some(0), false)
            .diagnostics[0]
            .code,
        Code::HistoryEmpty
    );
    let _ = query(&mut exec, "bookmark.list", json!({}));
    let _ = query(&mut exec, "bookmark.list", json!({ "limit": 3 }));
    let _ = query(&mut exec, "memo.list", json!({}));
    assert_eq!(exec.current_revision().unwrap(), 0);
    assert_eq!(
        exec.call("history.undo", json!({}), Some(0), false)
            .diagnostics[0]
            .code,
        Code::HistoryEmpty
    );
    // An optional expectedVersion is honoured and never advances anything.
    let ok = exec.call("bookmark.list", json!({}), Some(0), false);
    assert!(ok.ok, "{:?}", ok.diagnostics);
    let stale = exec.call("memo.list", json!({}), Some(7), false);
    assert_eq!(code_of(&stale), Code::VersionConflict);
}

#[test]
fn annotation_queries_registry_policies_schemas_and_names() {
    for (name, cli, mcp, items) in [
        (
            "bookmark.list",
            "bookmark-list",
            "bookmark_list",
            "bookmarks",
        ),
        ("memo.list", "memo-list", "memo_list", "memos"),
    ] {
        let d = command_describe(name).unwrap();
        assert_eq!(d.category, CommandCategory::Query);
        assert_eq!(d.cli_name, cli);
        assert_eq!(d.mcp_tool_name, mcp);
        assert!(!d.mutates_content);
        assert!(!d.requires_expected_version);
        assert!(!d.uses_request_sequence);
        assert!(!d.records_history);
        assert!(!d.supports_dry_run);
        assert!(d.requires_document);
        assert!(d.annotations.read_only_hint);
        assert!(d.annotations.idempotent_hint);
        assert!(!d.annotations.destructive_hint);
        assert!(!d.annotations.open_world_hint);
        for field in ["cursor", "limit", "scope"] {
            assert!(
                d.input_schema["properties"][field].is_object(),
                "{name} is missing input property {field}"
            );
        }
        for field in [items, "total", "nextCursor"] {
            assert!(
                d.output_schema["properties"][field].is_object(),
                "{name} is missing output property {field}"
            );
        }
    }

    // Both commands need an open document.
    let out = Tmp::new("hae-annotations-nodoc-");
    let mut empty = SessionExecutor::new(&out.0, &out.0, Limits::default()).unwrap();
    for name in ["bookmark.list", "memo.list"] {
        let response = empty.call(name, json!({}), None, false);
        assert_eq!(code_of(&response), Code::SessionRequired);
    }
}

#[test]
fn print_real_document_annotation_summaries() {
    for (name, path) in [
        (
            "04_complex.hwpx",
            "spikes/work/stage4-e2e/inputs/04_complex.hwpx",
        ),
        (
            "research-school-plan-reference.hwpx",
            "tests/fixtures/corpus/research-school-plan-reference.hwpx",
        ),
    ] {
        let doc = fixture_path(path);
        let dir = Tmp::new("hae-annotations-print-");
        let mut exec =
            SessionExecutor::new(doc.parent().unwrap(), &dir.0, Limits::default()).unwrap();
        assert!(
            exec.call("document.open", json!({ "path": &doc }), None, false)
                .ok
        );
        println!("==================================================");
        println!("DOCUMENT: {name}");
        let memos = query(&mut exec, "memo.list", json!({}));
        println!(
            "memo.list total={} sources={:?} ids={:?} authors={:?} paragraphCounts={:?} shapesResolved={} replies={}",
            memos["total"],
            memos["memos"]
                .as_array()
                .unwrap()
                .iter()
                .map(|m| m["source"].clone())
                .collect::<Vec<_>>(),
            memos["memos"]
                .as_array()
                .unwrap()
                .iter()
                .map(|m| m["id"].clone())
                .collect::<Vec<_>>(),
            memos["memos"]
                .as_array()
                .unwrap()
                .iter()
                .map(|m| m["author"].clone())
                .collect::<Vec<_>>(),
            memos["memos"]
                .as_array()
                .unwrap()
                .iter()
                .map(|m| m["paragraphCount"].clone())
                .collect::<Vec<_>>(),
            memos["memos"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|m| m["shape"].is_object())
                .count(),
            memos["memos"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|m| m["parentBeginId"].is_string())
                .count(),
        );
        for memo in memos["memos"].as_array().unwrap() {
            println!(
                "  memo id={} number={} paragraphs={} offset={} textGraphemes={} preview={:?}",
                memo["id"],
                memo["number"],
                memo["paragraphCount"],
                memo["graphemeOffset"],
                memo["text"].as_str().unwrap().chars().count(),
                memo["paragraphs"][0]["preview"],
            );
        }
        let bookmarks = query(&mut exec, "bookmark.list", json!({}));
        println!(
            "bookmark.list total={} markers={:?} named={} unnamed={} withParameters={}",
            bookmarks["total"],
            bookmarks["bookmarks"]
                .as_array()
                .unwrap()
                .iter()
                .map(|b| b["marker"].clone())
                .collect::<Vec<_>>(),
            bookmarks["bookmarks"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|b| b["name"].is_string())
                .count(),
            bookmarks["bookmarks"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|b| b["name"].is_null())
                .count(),
            bookmarks["bookmarks"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|b| !b["parameters"].as_array().unwrap().is_empty())
                .count(),
        );
    }
}
