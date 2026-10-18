//! form.fill end-to-end through the executor's independent save gate:
//! success, scope, strict rejection, non-strict skips, dryRun, Undo and batch.
use hae_diag::Limits;
use hae_exec::SessionExecutor;
use serde_json::{json, Value};
use std::{fs, path::PathBuf, sync::Arc};

struct Harness(PathBuf);
impl Harness {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(hae_model::new_id("hae-formfill-").unwrap());
        fs::create_dir(&dir).unwrap();
        Self(dir)
    }
    fn open(&self, bytes: &[u8]) -> SessionExecutor {
        let path = self.0.join("input.hwpx");
        fs::write(&path, bytes).unwrap();
        let mut exec = SessionExecutor::new(&self.0, &self.0, Limits::default()).unwrap();
        query(&mut exec, "document.open", json!({"path":path}));
        exec
    }
}
impl Drop for Harness {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn query(exec: &mut SessionExecutor, command: &str, params: Value) -> Value {
    let r = exec.call(command, params, None, false);
    assert!(r.ok, "{command}: {:?}", r.diagnostics);
    r.result.unwrap()
}

fn p(inner: &str) -> String {
    format!(r#"<hp:p paraPrIDRef="0"><hp:run charPrIDRef="0">{inner}</hp:run></hp:p>"#)
}
fn t(text: &str) -> String {
    format!("<hp:t>{text}</hp:t>")
}
fn tc(row: u32, col: u32, row_span: u32, col_span: u32, paras: &[String]) -> String {
    format!(
        r#"<hp:tc><hp:cellAddr rowAddr="{row}" colAddr="{col}"/><hp:cellSpan rowSpan="{row_span}" colSpan="{col_span}"/><hp:subList>{}</hp:subList></hp:tc>"#,
        paras.join("")
    )
}
/// A plain grid whose cells are given as row × column text; an empty string
/// becomes a self-closing empty `<hp:t/>`.
fn grid(rows: &[Vec<&str>]) -> String {
    let row_cnt = rows.len() as u32;
    let col_cnt = rows[0].len() as u32;
    let mut body = String::new();
    for (r, cols) in rows.iter().enumerate() {
        body.push_str("<hp:tr>");
        for (c, cell) in cols.iter().enumerate() {
            let paras = if cell.is_empty() {
                vec![p("<hp:t/>")]
            } else {
                vec![p(&t(cell))]
            };
            body.push_str(&tc(r as u32, c as u32, 1, 1, &paras));
        }
        body.push_str("</hp:tr>");
    }
    format!(r#"<hp:tbl rowCnt="{row_cnt}" colCnt="{col_cnt}">{body}</hp:tbl>"#)
}
fn synthetic(tables: &[String]) -> Vec<u8> {
    let limits = Limits::default();
    let archive = hae_zip::Archive::open(
        include_bytes!("../../../tests/fixtures/editing/simple_cell.hwpx").to_vec(),
        &limits,
    )
    .unwrap();
    let mut state = hae_package::PackageState::from_archive(&archive);
    let body: String = tables.iter().map(|tbl| p(tbl)).collect();
    let section = format!(
        r#"<hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" xmlns:hc="http://www.hancom.co.kr/hwpml/2011/core">{body}</hs:sec>"#
    );
    state.set("Contents/section0.xml", Arc::from(section.into_bytes()));
    hae_zip::write(&archive, &hae_package::zip_plan(&archive, &state).unwrap()).unwrap()
}
fn cell_id(exec: &mut SessionExecutor, table: &Value, row: u64, col: u64) -> String {
    let m = query(
        exec,
        "element.query",
        json!({"table":table["nodeId"], "row":row, "col":col}),
    );
    m["matches"][0]["nodeId"].as_str().unwrap().to_string()
}
fn cell_text(exec: &mut SessionExecutor, id: &str) -> String {
    query(exec, "element.get", json!({"target":id}))["text"]
        .as_str()
        .unwrap()
        .to_string()
}
fn tables(exec: &mut SessionExecutor) -> Vec<Value> {
    query(exec, "element.query", json!({"kind":"table"}))["matches"]
        .as_array()
        .unwrap()
        .clone()
}
fn export(exec: &mut SessionExecutor, path: &std::path::Path) -> Vec<u8> {
    query(exec, "document.export", json!({"path":path}));
    fs::read(path).unwrap()
}

#[test]
fn form_fill_fills_right_and_below_cells_and_reports_each_label() {
    let h = Harness::new();
    let horizontal = grid(&[vec!["성명", ""], vec!["소속", ""]]);
    let vertical = grid(&[vec!["전화"], vec![""]]);
    let original = synthetic(&[horizontal, vertical]);
    let mut exec = h.open(&original);
    let tables = tables(&mut exec);
    let r = exec.call(
        "form.fill",
        json!({
            "values": {"성명":"홍길동","소속":"○○고등학교","전화":"010-0000-0000"},
        }),
        Some(0),
        false,
    );
    assert!(r.ok, "{:?}", r.diagnostics);
    let result = r.result.unwrap();
    assert_eq!(result["outcome"], "applied");
    assert_eq!(result["afterVersion"], 1);
    assert_eq!(result["strict"], true);
    assert_eq!(result["filled"].as_array().unwrap().len(), 3);
    assert_eq!(result["skipped"].as_array().unwrap().len(), 0);
    assert_eq!(result["intents"][0]["op"], "form.fill");
    assert_eq!(result["intents"][0]["fills"].as_array().unwrap().len(), 3);
    // Same-row right cell for the horizontal labels, cell below for the vertical label.
    let name = cell_id(&mut exec, &tables[0], 0, 1);
    let org = cell_id(&mut exec, &tables[0], 1, 1);
    let phone = cell_id(&mut exec, &tables[1], 1, 0);
    assert_eq!(cell_text(&mut exec, &name), "홍길동");
    assert_eq!(cell_text(&mut exec, &org), "○○고등학교");
    assert_eq!(cell_text(&mut exec, &phone), "010-0000-0000");
    // Label cells and the untouched table structure stay as they were.
    let label = cell_id(&mut exec, &tables[0], 0, 0);
    assert_eq!(cell_text(&mut exec, &label), "성명");
    for row in result["filled"].as_array().unwrap() {
        assert!(row["nodeId"].is_string() && row["label"].is_string() && row["value"].is_string());
    }
}

#[test]
fn form_fill_multi_paragraph_and_merged_targets_use_cell_set_text_rules() {
    let h = Harness::new();
    // Table 1: a merged rowSpan=2 right cell. Table 2: a two-paragraph empty
    // right cell (only the first paragraph receives the value).
    let merged_table = format!(
        r#"<hp:tbl rowCnt="2" colCnt="2"><hp:tr>{}{}</hp:tr><hp:tr>{}</hp:tr></hp:tbl>"#,
        tc(0, 0, 1, 1, &[p(&t("병합"))]),
        tc(0, 1, 2, 1, &[p("<hp:t/>")]),
        tc(1, 0, 1, 1, &[p(&t("별도"))]),
    );
    let multi_table = format!(
        r#"<hp:tbl rowCnt="1" colCnt="2"><hp:tr>{}{}</hp:tr></hp:tbl>"#,
        tc(0, 0, 1, 1, &[p(&t("서술"))]),
        tc(0, 1, 1, 1, &[p("<hp:t/>"), p("<hp:t/>")]),
    );
    let original = synthetic(&[merged_table, multi_table]);
    let mut exec = h.open(&original);
    let tables = tables(&mut exec);
    let merged = cell_id(&mut exec, &tables[0], 0, 1);
    let multi = cell_id(&mut exec, &tables[1], 0, 1);
    let r = exec.call(
        "form.fill",
        json!({"values": {"병합":"값", "서술":"내용"}}),
        Some(0),
        false,
    );
    assert!(r.ok, "{:?}", r.diagnostics);
    let result = r.result.unwrap();
    assert_eq!(result["filled"].as_array().unwrap().len(), 2);
    assert_eq!(result["skipped"].as_array().unwrap().len(), 0);
    assert_eq!(cell_text(&mut exec, &merged), "값");
    // Two paragraphs remain; only the first one holds the value.
    let q = query(
        &mut exec,
        "element.query",
        json!({"kind":"paragraph","ancestor":multi}),
    );
    assert_eq!(q["matches"].as_array().unwrap().len(), 2);
    assert_eq!(cell_text(&mut exec, &multi), "내용\n");
}

#[test]
fn form_fill_dry_run_and_undo_keep_byte_identical_documents() {
    let h = Harness::new();
    let original = synthetic(&[grid(&[vec!["성명", ""]])]);
    let mut exec = h.open(&original);
    let dry = exec.call(
        "form.fill",
        json!({"values": {"성명":"홍길동"}}),
        Some(0),
        true,
    );
    assert!(dry.ok, "{:?}", dry.diagnostics);
    let result = dry.result.unwrap();
    assert_eq!(result["dryRun"], true);
    assert_eq!(result["predictedAfterVersion"], 1);
    assert_eq!(exec.current_revision(), Some(0));
    assert_eq!(export(&mut exec, &h.0.join("dry.hwpx")), original);
    let applied = exec.call(
        "form.fill",
        json!({"values": {"성명":"홍길동"}}),
        Some(0),
        false,
    );
    assert!(applied.ok, "{:?}", applied.diagnostics);
    assert_ne!(export(&mut exec, &h.0.join("applied.hwpx")), original);
    assert!(exec.call("history.undo", json!({}), Some(1), false).ok);
    assert_eq!(export(&mut exec, &h.0.join("undo.hwpx")), original);
}

#[test]
fn form_fill_strict_reports_every_failure_and_changes_nothing() {
    let h = Harness::new();
    let original = synthetic(&[grid(&[vec!["성명", "이미값"], vec!["소속", ""]])]);
    let mut exec = h.open(&original);
    let r = exec.call(
        "form.fill",
        json!({"values": {"성명":"홍길동","없는라벨":"X","소속":"○○고"}}),
        Some(0),
        false,
    );
    assert!(!r.ok);
    assert_eq!(r.diagnostics[0].code, hae_diag::Code::Unsupported);
    assert_eq!(
        r.diagnostics[0].reason.as_deref(),
        Some("form_fill_rejected")
    );
    let failures = r.diagnostics[0].detail.as_ref().unwrap()["failures"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(failures.len(), 2);
    let by_label: std::collections::BTreeMap<String, String> = failures
        .iter()
        .map(|f| {
            (
                f["label"].as_str().unwrap().to_string(),
                f["reason"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert_eq!(by_label["성명"], "target_not_empty");
    assert_eq!(by_label["없는라벨"], "label_not_found");
    // Nothing was applied: revision, bytes and the non-empty cell are unchanged.
    assert_eq!(exec.current_revision(), Some(0));
    assert_eq!(export(&mut exec, &h.0.join("rejected.hwpx")), original);
    let tables = tables(&mut exec);
    let used = cell_id(&mut exec, &tables[0], 0, 1);
    assert_eq!(cell_text(&mut exec, &used), "이미값");

    // Every failure being a missing label reports TARGET_NOT_FOUND.
    let only_missing = exec.call(
        "form.fill",
        json!({"values": {"없는라벨":"X"}}),
        Some(0),
        false,
    );
    assert!(!only_missing.ok);
    assert_eq!(
        only_missing.diagnostics[0].code,
        hae_diag::Code::TargetNotFound
    );
    assert_eq!(exec.current_revision(), Some(0));
}

#[test]
fn form_fill_non_strict_applies_resolvable_labels_and_reports_skips() {
    let h = Harness::new();
    let original = synthetic(&[grid(&[vec!["성명", "이미값"], vec!["소속", ""]])]);
    let mut exec = h.open(&original);
    let r = exec.call(
        "form.fill",
        json!({"values": {"성명":"홍길동","없는라벨":"X","소속":"○○고"}, "strict": false}),
        Some(0),
        false,
    );
    assert!(r.ok, "{:?}", r.diagnostics);
    let result = r.result.unwrap();
    assert_eq!(result["outcome"], "applied");
    assert_eq!(result["strict"], false);
    assert_eq!(result["filled"].as_array().unwrap().len(), 1);
    assert_eq!(result["filled"][0]["label"], "소속");
    assert_eq!(result["skipped"].as_array().unwrap().len(), 2);
    assert!(r
        .diagnostics
        .iter()
        .any(|d| d.code.as_str() == "FORM_FILL_SKIPPED"));
    let tables = tables(&mut exec);
    let org = cell_id(&mut exec, &tables[0], 1, 1);
    let used = cell_id(&mut exec, &tables[0], 0, 1);
    assert_eq!(cell_text(&mut exec, &org), "○○고");
    assert_eq!(cell_text(&mut exec, &used), "이미값");
    assert!(exec.call("history.undo", json!({}), Some(1), false).ok);
    assert_eq!(export(&mut exec, &h.0.join("undo.hwpx")), original);
}

#[test]
fn form_fill_ambiguous_label_needs_scope_and_scope_limits_the_tables() {
    let h = Harness::new();
    let original = synthetic(&[grid(&[vec!["성명", ""]]), grid(&[vec!["성명", ""]])]);
    let mut exec = h.open(&original);
    let r = exec.call(
        "form.fill",
        json!({"values": {"성명":"홍길동"}}),
        Some(0),
        false,
    );
    assert!(!r.ok);
    assert_eq!(r.diagnostics[0].code, hae_diag::Code::AmbiguousTarget);
    assert_eq!(
        r.diagnostics[0].detail.as_ref().unwrap()["failures"][0]["reason"],
        "ambiguous_label"
    );
    let tables = tables(&mut exec);
    let r = exec.call(
        "form.fill",
        json!({"values": {"성명":"홍길동"}, "scope": tables[1]["nodeId"]}),
        Some(0),
        false,
    );
    assert!(r.ok, "{:?}", r.diagnostics);
    let result = r.result.unwrap();
    assert_eq!(result["scope"], tables[1]["nodeId"]);
    assert_eq!(result["filled"].as_array().unwrap().len(), 1);
    let first = cell_id(&mut exec, &tables[0], 0, 1);
    let second = cell_id(&mut exec, &tables[1], 0, 1);
    assert_eq!(cell_text(&mut exec, &first), "");
    assert_eq!(cell_text(&mut exec, &second), "홍길동");
    // A non-table/section scope is refused up front.
    let para = query(&mut exec, "element.query", json!({"kind":"paragraph"}))["matches"][0]
        ["nodeId"]
        .clone();
    let bad_scope = exec.call(
        "form.fill",
        json!({"values": {"성명":"X"}, "scope": para}),
        Some(1),
        false,
    );
    assert!(!bad_scope.ok);
    assert_eq!(
        bad_scope.diagnostics[0].reason.as_deref(),
        Some("form_scope")
    );
}

#[test]
fn form_fill_accepts_every_empty_t_spelling_through_the_save_gate() {
    let h = Harness::new();
    let body = format!(
        r#"<hp:tbl rowCnt="3" colCnt="2">
<hp:tr>{}{}</hp:tr>
<hp:tr>{}{}</hp:tr>
<hp:tr>{}{}</hp:tr>
</hp:tbl>"#,
        tc(0, 0, 1, 1, &[p(&t("하나"))]),
        tc(0, 1, 1, 1, &[p("<hp:t/>")]),
        tc(1, 0, 1, 1, &[p(&t("둘"))]),
        tc(1, 1, 1, 1, &[p("<hp:t></hp:t>")]),
        tc(2, 0, 1, 1, &[p(&t("셋"))]),
        tc(2, 1, 1, 1, &[p("<hp:t />")]),
    );
    let original = synthetic(&[body]);
    let mut exec = h.open(&original);
    let tables = tables(&mut exec);
    let r = exec.call(
        "form.fill",
        json!({"values": {"하나":"1", "둘":"2", "셋":"3"}}),
        Some(0),
        false,
    );
    assert!(r.ok, "{:?}", r.diagnostics);
    assert_eq!(r.result.unwrap()["filled"].as_array().unwrap().len(), 3);
    for (row, want) in [(0, "1"), (1, "2"), (2, "3")] {
        let id = cell_id(&mut exec, &tables[0], row, 1);
        assert_eq!(cell_text(&mut exec, &id), want);
    }
    assert!(exec.call("history.undo", json!({}), Some(1), false).ok);
    assert_eq!(export(&mut exec, &h.0.join("undo.hwpx")), original);
}

#[test]
fn form_fill_conflicting_targets_are_reported_and_never_filled() {
    let h = Harness::new();
    // (1,1) is the right cell of "하나" at (1,0) and the below cell of "둘" at
    // (0,1) at the same time: two labels claim one empty cell.
    let body = format!(
        r#"<hp:tbl rowCnt="2" colCnt="2"><hp:tr>{}{}</hp:tr><hp:tr>{}{}</hp:tr></hp:tbl>"#,
        tc(0, 0, 1, 1, &[p(&t("머리"))]),
        tc(0, 1, 1, 1, &[p(&t("둘"))]),
        tc(1, 0, 1, 1, &[p(&t("하나"))]),
        tc(1, 1, 1, 1, &[p("<hp:t/>")]),
    );
    let original = synthetic(&[body]);
    let mut exec = h.open(&original);
    let strict = exec.call(
        "form.fill",
        json!({"values": {"하나":"V1","둘":"V2"}}),
        Some(0),
        false,
    );
    assert!(!strict.ok);
    let failures = strict.diagnostics[0].detail.as_ref().unwrap()["failures"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(failures.len(), 2);
    assert!(failures.iter().all(|f| f["reason"] == "conflicting_target"));
    assert_eq!(exec.current_revision(), Some(0));
    let relaxed = exec.call(
        "form.fill",
        json!({"values": {"하나":"V1","둘":"V2"}, "strict": false}),
        Some(0),
        false,
    );
    assert!(relaxed.ok, "{:?}", relaxed.diagnostics);
    let result = relaxed.result.unwrap();
    assert_eq!(result["outcome"], "noop");
    assert_eq!(result["filled"].as_array().unwrap().len(), 0);
    assert_eq!(result["skipped"].as_array().unwrap().len(), 2);
    assert_eq!(export(&mut exec, &h.0.join("noop.hwpx")), original);
}

#[test]
fn form_fill_batch_is_one_revision_and_one_undo_unit() {
    let h = Harness::new();
    let original = synthetic(&[grid(&[vec!["성명", ""]])]);
    let mut exec = h.open(&original);
    let para = query(&mut exec, "element.query", json!({"kind":"paragraph"}))["matches"][0]
        ["nodeId"]
        .clone();
    let r = exec.call(
        "command.batch",
        json!({"steps": [
            {"command":"form.fill","params":{"values":{"성명":"홍길동"}}},
            {"command":"paragraph.format","params":{"target":para,"alignment":"center"}}
        ]}),
        Some(0),
        false,
    );
    assert!(r.ok, "{:?}", r.diagnostics);
    let result = r.result.unwrap();
    assert_eq!(result["afterVersion"], 1);
    assert_eq!(result["formFill"][0]["filled"][0]["label"], "성명");
    assert!(exec.call("history.undo", json!({}), Some(1), false).ok);
    assert_eq!(export(&mut exec, &h.0.join("undo.hwpx")), original);
}
