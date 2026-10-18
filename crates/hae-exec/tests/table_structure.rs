//! Table row/column insertion and deletion through the executor: node ids,
//! Undo/Redo, dryRun, batch, merged-cell refusals, address/size consistency.
use hae_diag::{Code, Limits};
use hae_exec::{ResponseEnvelope, SessionExecutor};
use hae_model::{Document, Kind};
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs, path::PathBuf};

const SOURCE: &[u8] = include_bytes!("../../../tests/fixtures/tables/structure.hwpx");
const OBJECT: &[u8] = include_bytes!("../../../tests/fixtures/tables/structure_object.hwpx");
const SECTION: &str = "Contents/section0.xml";

struct Case {
    exec: SessionExecutor,
    root: PathBuf,
    /// Table node ids: plain, merged, auto, vertical.
    tables: Vec<String>,
    version: u64,
}
impl Drop for Case {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn code(r: &ResponseEnvelope) -> Code {
    r.diagnostics[0].code
}
fn reason(r: &ResponseEnvelope) -> Option<&str> {
    r.diagnostics[0].reason.as_deref()
}

impl Case {
    fn new() -> Self {
        Self::with(SOURCE)
    }
    fn with(source: &'static [u8]) -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../spikes/work/table-structure/test-runs")
            .join(hae_model::new_id("run-").unwrap());
        fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        fs::write(root.join("in.hwpx"), source).unwrap();
        let mut exec = SessionExecutor::new(&root, &root, Limits::default()).unwrap();
        assert!(
            exec.call(
                "document.open",
                json!({"path":root.join("in.hwpx")}),
                None,
                false
            )
            .ok
        );
        let q = exec
            .call("element.query", json!({"kind":"table"}), None, false)
            .result
            .unwrap();
        let tables = q["matches"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["nodeId"].as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        assert_eq!(tables.len(), 4);
        Self {
            exec,
            root,
            tables,
            version: 0,
        }
    }
    /// Call an edit with the tracked revision; bumps it on success.
    fn edit(&mut self, cmd: &str, params: Value) -> ResponseEnvelope {
        let r = self.exec.call(cmd, params, Some(self.version), false);
        if r.ok && r.result.as_ref().is_some_and(|v| v["outcome"] == "applied") {
            self.version += 1;
        }
        r
    }
    fn ok(&mut self, cmd: &str, params: Value) -> Value {
        let r = self.edit(cmd, params);
        assert!(r.ok, "{cmd}: {:?}", r.diagnostics);
        r.result.unwrap()
    }
    fn export(&mut self, name: &str) -> Vec<u8> {
        let path = self.root.join(name);
        let r = self
            .exec
            .call("document.export", json!({"path":path}), None, false);
        assert!(r.ok, "{:?}", r.diagnostics);
        fs::read(path).unwrap()
    }
    fn doc(&mut self, name: &str) -> Document {
        let bytes = self.export(name);
        Document::open(bytes, &Limits::default()).unwrap()
    }
    /// All nodes as (id, kind) in document order, via document.inspect.
    fn nodes(&mut self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let mut p = json!({"limit": 500});
            if let Some(c) = &cursor {
                p["cursor"] = json!(c);
            }
            let r = self
                .exec
                .call("document.inspect", p, None, false)
                .result
                .unwrap();
            for n in r["items"].as_array().unwrap() {
                out.push((
                    n["nodeId"].as_str().unwrap().to_string(),
                    n["kind"].as_str().unwrap().to_string(),
                ));
            }
            match r["nextCursor"].as_str() {
                Some(c) => cursor = Some(c.to_string()),
                None => return out,
            }
        }
    }
    fn cell(&mut self, table: usize, row: usize, col: usize) -> Option<String> {
        let r = self
            .exec
            .call(
                "element.query",
                json!({"table": self.tables[table], "row": row, "col": col}),
                None,
                false,
            )
            .result
            .unwrap();
        r["matches"][0]["nodeId"].as_str().map(str::to_string)
    }
    fn undo(&mut self) -> Value {
        let r = self
            .exec
            .call("history.undo", json!({}), Some(self.version), false);
        assert!(r.ok, "{:?}", r.diagnostics);
        self.version += 1;
        r.result.unwrap()
    }
    fn redo(&mut self) -> Value {
        let r = self
            .exec
            .call("history.redo", json!({}), Some(self.version), false);
        assert!(r.ok, "{:?}", r.diagnostics);
        self.version += 1;
        r.result.unwrap()
    }
}

/// A row/column/size summary of one exported table, plus a consistency check
/// of every address, span and size value against the grid.
#[derive(Debug, PartialEq)]
struct C {
    row: usize,
    col: usize,
    cs: usize,
    rs: usize,
    w: u32,
    h: u32,
    border: String,
    header: String,
    text: String,
}

#[derive(Debug, PartialEq)]
struct Shape {
    rows: usize,
    cols: usize,
    width: u32,
    height: u32,
    cells: Vec<C>,
}

fn shape(doc: &Document, index: usize) -> Shape {
    let table = doc
        .nodes
        .iter()
        .filter(|n| n.kind == Kind::Table)
        .nth(index)
        .unwrap();
    let xml = doc.doc_of(table);
    let t = xml.el(table.el);
    let num = |i: usize, a: &str| xml.el(i).attr_value(a).unwrap().parse::<u32>().unwrap();
    let sub = |i: usize, n: &str| {
        *xml.el(i)
            .children
            .iter()
            .find(|&&c| xml.el(c).local == n)
            .unwrap()
    };
    let rows = num(table.el, "rowCnt") as usize;
    let cols = num(table.el, "colCnt") as usize;
    let sz = sub(table.el, "sz");
    let trs: Vec<usize> = t
        .children
        .iter()
        .copied()
        .filter(|&c| xml.el(c).local == "tr")
        .collect();
    assert_eq!(trs.len(), rows, "rowCnt vs <tr> count");
    let mut cells = Vec::new();
    let mut covered = vec![vec![0u8; cols]; rows];
    for (r, &tr) in trs.iter().enumerate() {
        for &tc in &xml.el(tr).children {
            let a = sub(tc, "cellAddr");
            let s = sub(tc, "cellSpan");
            let z = sub(tc, "cellSz");
            let (row, col) = (num(a, "rowAddr") as usize, num(a, "colAddr") as usize);
            let (cs, rs) = (num(s, "colSpan") as usize, num(s, "rowSpan") as usize);
            assert_eq!(row, r, "rowAddr must equal its <tr> ordinal");
            for line in covered.iter_mut().skip(row).take(rs) {
                for slot in line.iter_mut().skip(col).take(cs) {
                    *slot += 1;
                }
            }
            let text: String = xml
                .descendants(tc)
                .filter(|&i| xml.el(i).local == "t")
                .map(|i| xml.direct_text(i))
                .collect::<Vec<_>>()
                .join("|");
            cells.push(C {
                row,
                col,
                cs,
                rs,
                w: num(z, "width"),
                h: num(z, "height"),
                border: xml.el(tc).attr_value("borderFillIDRef").unwrap().into(),
                header: xml.el(tc).attr_value("header").unwrap().into(),
                text,
            });
        }
    }
    assert!(
        covered.iter().flatten().all(|&n| n == 1),
        "grid must be covered exactly once"
    );
    // Per-row widths of unmerged rows add up to the table width.
    for r in 0..rows {
        let row: Vec<_> = cells.iter().filter(|c| c.row == r).collect();
        if row.iter().all(|c| c.rs == 1) && row.iter().map(|c| c.cs).sum::<usize>() == cols {
            assert_eq!(
                row.iter().map(|c| c.w).sum::<u32>(),
                num(sz, "width"),
                "row {r} widths vs table width"
            );
        }
    }
    Shape {
        rows,
        cols,
        width: num(sz, "width"),
        height: num(sz, "height"),
        cells,
    }
}

fn find(s: &Shape, row: usize, col: usize) -> &C {
    s.cells
        .iter()
        .find(|c| c.row == row && c.col == col)
        .unwrap_or_else(|| panic!("no cell ({row},{col})"))
}

fn kinds(nodes: &[(String, String)], kind: &str) -> usize {
    nodes.iter().filter(|n| n.1 == kind).count()
}

// ---------------------------------------------------------------- insert row

#[test]
fn insert_row_in_the_middle_creates_clones_and_shifts() {
    let mut c = Case::new();
    let before = c.nodes();
    let old_cell = c.cell(0, 3, 1).unwrap();
    let r = c.ok(
        "table.insertRow",
        json!({"target": c.tables[0], "index": 2}),
    );
    assert_eq!(r["outcome"], "applied");
    let s = &r["structure"][0];
    assert_eq!(s["command"], "table.insertRow");
    assert_eq!(s["rowsBefore"], 4);
    assert_eq!(s["rowsAfter"], 5);
    let created = s["created"].as_array().unwrap();
    assert_eq!(created.len(), 6, "3 cells + 3 paragraphs");
    assert_eq!(created.iter().filter(|n| n["kind"] == "cell").count(), 3);
    let after = c.nodes();
    assert_eq!(kinds(&after, "cell"), kinds(&before, "cell") + 3);
    assert_eq!(kinds(&after, "paragraph"), kinds(&before, "paragraph") + 3);
    // Every earlier id survives; the old row-3 cell moved to row 4.
    for n in &before {
        assert!(after.contains(n), "{n:?} vanished");
    }
    assert_eq!(c.cell(0, 4, 1).as_deref(), Some(old_cell.as_str()));
    let doc = c.doc("out.hwpx");
    let t = shape(&doc, 0);
    assert_eq!((t.rows, t.cols, t.height, t.width), (5, 3, 12000, 42000));
    for col in 0..3 {
        let new = find(&t, 2, col);
        assert_eq!(new.text, "", "new cells are empty");
        assert_eq!(new.w, 14000);
        assert_eq!(new.h, 2400);
        assert_eq!(new.border, "3", "borders cloned from a body row");
        assert_eq!(new.header, "0");
    }
    // Old row 2 became row 3 with its text intact.
    assert_eq!(find(&t, 3, 0).text, "R2C0");
    assert_eq!(find(&t, 3, 1).text, "R2C1|R2C1-extra");
    assert_eq!(find(&t, 4, 2).text, "R3C2");
}

#[test]
fn insert_row_after_header_clones_body_row_not_header() {
    let mut c = Case::new();
    c.ok(
        "table.insertRow",
        json!({"target": c.tables[0], "index": 1}),
    );
    let t = shape(&c.doc("out.hwpx"), 0);
    for col in 0..3 {
        assert_eq!(find(&t, 1, col).border, "3", "not the header's shaded fill");
        assert_eq!(find(&t, 1, col).header, "0");
        assert_eq!(find(&t, 0, col).border, "4");
        assert_eq!(find(&t, 0, col).header, "1", "header row untouched");
    }
}

#[test]
fn insert_row_at_zero_clones_header_with_header_flag_reset() {
    let mut c = Case::new();
    c.ok(
        "table.insertRow",
        json!({"target": c.tables[0], "index": 0}),
    );
    let t = shape(&c.doc("out.hwpx"), 0);
    assert_eq!(t.rows, 5);
    for col in 0..3 {
        assert_eq!(
            find(&t, 0, col).border,
            "4",
            "no body row is adjacent; header fill kept"
        );
        assert_eq!(find(&t, 0, col).header, "0", "new row is not a header row");
        assert_eq!(find(&t, 0, col).text, "");
        assert_eq!(find(&t, 1, col).text, format!("H{col}"));
        assert_eq!(find(&t, 1, col).header, "1");
    }
}

#[test]
fn insert_row_at_the_end_and_count() {
    let mut c = Case::new();
    let r = c.ok(
        "table.insertRow",
        json!({"target": c.tables[0], "index": 4, "count": 3}),
    );
    assert_eq!(r["structure"][0]["count"], 3);
    assert_eq!(r["structure"][0]["created"].as_array().unwrap().len(), 18);
    let t = shape(&c.doc("out.hwpx"), 0);
    assert_eq!((t.rows, t.height), (7, 16800));
    assert_eq!(find(&t, 3, 0).text, "R3C0", "rows above are untouched");
    for row in 4..7 {
        assert_eq!(find(&t, row, 1).text, "");
    }
}

#[test]
fn insert_row_in_auto_height_table_adds_the_template_height() {
    let mut c = Case::new();
    // Rows 0 and 2 have height 0 (auto); the table height is 4000, not 3000.
    c.ok(
        "table.insertRow",
        json!({"target": c.tables[2], "index": 1}),
    );
    let t = shape(&c.doc("a.hwpx"), 2);
    assert_eq!(
        (t.rows, t.height),
        (4, 4000),
        "neighbour row 0 has height 0"
    );
    c.ok(
        "table.insertRow",
        json!({"target": c.tables[2], "index": 4}),
    );
    let t = shape(&c.doc("b.hwpx"), 2);
    assert_eq!(
        (t.rows, t.height),
        (5, 4000),
        "append clones the auto row 2"
    );
}

#[test]
fn insert_row_in_merged_table_at_a_clean_boundary() {
    let mut c = Case::new();
    c.ok(
        "table.insertRow",
        json!({"target": c.tables[1], "index": 2}),
    );
    let t = shape(&c.doc("out.hwpx"), 1);
    assert_eq!((t.rows, t.height), (5, 12000));
    assert_eq!(find(&t, 0, 0).cs, 3, "title still spans three columns");
    assert_eq!(find(&t, 3, 0).rs, 2, "TALL moved down intact");
    assert_eq!(find(&t, 3, 0).text, "TALL");
    for col in 0..3 {
        assert_eq!(find(&t, 2, col).cs, 1);
        assert_eq!(find(&t, 2, col).rs, 1);
    }
}

#[test]
fn insert_row_bounds_and_counts_are_rejected_with_reasons() {
    let mut c = Case::new();
    let t = c.tables[0].clone();
    let r = c.edit("table.insertRow", json!({"target": t, "index": 5}));
    assert_eq!(code(&r), Code::InvalidArgument);
    assert_eq!(reason(&r), Some("index_out_of_range"));
    let r = c.edit(
        "table.insertRow",
        json!({"target": t, "index": 0, "count": 0}),
    );
    assert_eq!(code(&r), Code::InvalidArgument);
    assert_eq!(reason(&r), Some("count_out_of_range"));
    let r = c.edit(
        "table.insertRow",
        json!({"target": t, "index": 0, "count": 1021}),
    );
    assert_eq!(reason(&r), Some("table_size_limit"));
    let r = c.edit("table.insertRow", json!({"target": t, "index": -1}));
    assert_eq!(code(&r), Code::InvalidRequest);
    let r = c.edit(
        "table.insertRow",
        json!({"target": "n-missing", "index": 0}),
    );
    assert_eq!(code(&r), Code::TargetNotFound);
    let cell = c.cell(0, 0, 0).unwrap();
    let r = c.edit("table.insertRow", json!({"target": cell, "index": 0}));
    assert_eq!(code(&r), Code::Unsupported);
    assert_eq!(reason(&r), Some("wrong_kind"));
    assert_eq!(c.export("same.hwpx"), SOURCE, "refusals change nothing");
}

#[test]
fn insert_row_inside_a_vertical_merge_is_refused() {
    let mut c = Case::new();
    let r = c.edit(
        "table.insertRow",
        json!({"target": c.tables[1], "index": 3}),
    );
    assert_eq!(code(&r), Code::Unsupported);
    assert_eq!(reason(&r), Some("insert_inside_merged_cell"));
    assert_eq!(r.diagnostics[0].detail.as_ref().unwrap()["cellCount"], 1);
    let r = c.edit(
        "table.insertRow",
        json!({"target": c.tables[3], "index": 1}),
    );
    assert_eq!(reason(&r), Some("insert_inside_merged_cell"));
    let r = c.edit(
        "table.insertRow",
        json!({"target": c.tables[3], "index": 0}),
    );
    assert_eq!(
        reason(&r),
        Some("no_simple_template_line"),
        "the only neighbour row is vertically merged"
    );
    assert_eq!(c.export("same.hwpx"), SOURCE);
}

// ---------------------------------------------------------------- delete row

#[test]
fn delete_row_removes_nodes_and_shifts_the_rest() {
    let mut c = Case::new();
    let before = c.nodes();
    let doomed: Vec<String> = (0..3).map(|col| c.cell(0, 2, col).unwrap()).collect();
    let below = c.cell(0, 3, 2).unwrap();
    let r = c.ok(
        "table.deleteRow",
        json!({"target": c.tables[0], "index": 2}),
    );
    let s = &r["structure"][0];
    let removed: Vec<String> = s["removed"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(removed.len(), 7, "3 cells + 4 paragraphs (R2C1 has two)");
    for id in &doomed {
        assert!(removed.contains(id));
    }
    let after = c.nodes();
    assert_eq!(kinds(&after, "cell"), kinds(&before, "cell") - 3);
    assert_eq!(kinds(&after, "paragraph"), kinds(&before, "paragraph") - 4);
    assert_eq!(
        c.cell(0, 2, 2).as_deref(),
        Some(below.as_str()),
        "row 3 moved up"
    );
    let t = shape(&c.doc("out.hwpx"), 0);
    assert_eq!((t.rows, t.height), (3, 7200));
    assert_eq!(find(&t, 2, 2).text, "R3C2");
    assert!(t.cells.iter().all(|c| !c.text.contains("R2C")));
}

#[test]
fn delete_row_first_last_and_count() {
    let mut c = Case::new();
    c.ok(
        "table.deleteRow",
        json!({"target": c.tables[0], "index": 3}),
    );
    c.ok(
        "table.deleteRow",
        json!({"target": c.tables[0], "index": 0}),
    );
    let t = shape(&c.doc("a.hwpx"), 0);
    assert_eq!((t.rows, t.height), (2, 4800));
    assert_eq!(find(&t, 0, 0).text, "R1C0");
    let mut c = Case::new();
    c.ok(
        "table.deleteRow",
        json!({"target": c.tables[0], "index": 1, "count": 3}),
    );
    let t = shape(&c.doc("b.hwpx"), 0);
    assert_eq!((t.rows, t.height), (1, 2400));
    assert_eq!(find(&t, 0, 0).header, "1");
}

#[test]
fn delete_row_in_auto_height_table_subtracts_row_heights() {
    let mut c = Case::new();
    c.ok(
        "table.deleteRow",
        json!({"target": c.tables[2], "index": 1}),
    );
    let t = shape(&c.doc("out.hwpx"), 2);
    assert_eq!((t.rows, t.height), (2, 1000), "4000 - 3000");
}

#[test]
fn delete_row_bounds_and_emptying_are_rejected() {
    let mut c = Case::new();
    let t = c.tables[0].clone();
    let r = c.edit("table.deleteRow", json!({"target": t, "index": 4}));
    assert_eq!(reason(&r), Some("index_out_of_range"));
    let r = c.edit(
        "table.deleteRow",
        json!({"target": t, "index": 2, "count": 3}),
    );
    assert_eq!(reason(&r), Some("index_out_of_range"));
    let r = c.edit(
        "table.deleteRow",
        json!({"target": t, "index": 0, "count": 4}),
    );
    assert_eq!(code(&r), Code::InvalidArgument);
    assert_eq!(reason(&r), Some("would_empty_table"));
    assert_eq!(c.export("same.hwpx"), SOURCE);
}

#[test]
fn delete_row_with_merged_cells_is_refused_with_the_cells() {
    let mut c = Case::new();
    for (table, index) in [(1usize, 0usize), (1, 2), (1, 3), (3, 0), (3, 1)] {
        let r = c.edit(
            "table.deleteRow",
            json!({"target": c.tables[table], "index": index}),
        );
        assert_eq!(code(&r), Code::Unsupported, "table {table} row {index}");
        assert_eq!(reason(&r), Some("merged_cell_blocks_delete"));
        let detail = r.diagnostics[0].detail.as_ref().unwrap();
        assert!(detail["cellCount"].as_u64().unwrap() >= 1);
        assert!(detail["cells"][0]["rowSpan"].is_number());
    }
    // A clean row of the same table can still go.
    c.ok(
        "table.deleteRow",
        json!({"target": c.tables[1], "index": 1}),
    );
    assert_eq!(shape(&c.doc("out.hwpx"), 1).rows, 3);
}

#[test]
fn delete_cells_holding_controls_is_refused() {
    let mut c = Case::with(OBJECT);
    let r = c.edit(
        "table.deleteRow",
        json!({"target": c.tables[0], "index": 3}),
    );
    assert_eq!(code(&r), Code::Unsupported);
    assert_eq!(reason(&r), Some("delete_content_unsupported"));
    let r = c.edit(
        "table.deleteColumn",
        json!({"target": c.tables[0], "index": 2}),
    );
    assert_eq!(reason(&r), Some("delete_content_unsupported"));
    // Other lines of the same table are fine, and inserting beside it too.
    c.ok(
        "table.deleteRow",
        json!({"target": c.tables[0], "index": 2}),
    );
    c.ok(
        "table.insertRow",
        json!({"target": c.tables[0], "index": 3}),
    );
    assert!(c.export("x.hwpx").len() > 100);
}

// ------------------------------------------------------------- insert column

#[test]
fn insert_column_clones_the_neighbour_and_widens_the_table() {
    let mut c = Case::new();
    let before = c.nodes();
    let old = c.cell(0, 1, 1).unwrap();
    let r = c.ok(
        "table.insertColumn",
        json!({"target": c.tables[0], "index": 1}),
    );
    let s = &r["structure"][0];
    assert_eq!(
        (s["columnsBefore"].as_u64(), s["columnsAfter"].as_u64()),
        (Some(3), Some(4))
    );
    assert_eq!(
        s["created"].as_array().unwrap().len(),
        8,
        "4 cells + 4 paragraphs"
    );
    let after = c.nodes();
    for n in &before {
        assert!(after.contains(n));
    }
    assert_eq!(
        c.cell(0, 1, 2).as_deref(),
        Some(old.as_str()),
        "moved right"
    );
    let t = shape(&c.doc("out.hwpx"), 0);
    assert_eq!((t.rows, t.cols, t.width, t.height), (4, 4, 56000, 9600));
    assert_eq!(
        find(&t, 0, 1).header,
        "1",
        "header cell stays a header cell"
    );
    assert_eq!(find(&t, 0, 1).border, "4");
    assert_eq!(find(&t, 0, 1).text, "", "but empty");
    for row in 1..4 {
        assert_eq!(find(&t, row, 1).text, "");
        assert_eq!(find(&t, row, 1).border, "3");
        assert_eq!(
            find(&t, row, 2).text,
            format!("R{row}C1").replace("R2C1", "R2C1|R2C1-extra")
        );
    }
    assert_eq!(find(&t, 0, 0).text, "H0");
    assert_eq!(find(&t, 0, 3).text, "H2");
}

#[test]
fn insert_column_at_the_edges_and_count() {
    let mut c = Case::new();
    c.ok(
        "table.insertColumn",
        json!({"target": c.tables[0], "index": 0}),
    );
    c.ok(
        "table.insertColumn",
        json!({"target": c.tables[0], "index": 4, "count": 2}),
    );
    let t = shape(&c.doc("out.hwpx"), 0);
    assert_eq!((t.cols, t.width), (6, 84000));
    assert_eq!(find(&t, 1, 1).text, "R1C0");
    assert_eq!(find(&t, 1, 3).text, "R1C2");
    assert_eq!(find(&t, 1, 4).text, "");
    assert_eq!(find(&t, 1, 5).text, "");
}

#[test]
fn insert_column_bounds_and_merge_boundaries_are_rejected() {
    let mut c = Case::new();
    let r = c.edit(
        "table.insertColumn",
        json!({"target": c.tables[0], "index": 4}),
    );
    assert_eq!(reason(&r), Some("index_out_of_range"));
    let r = c.edit(
        "table.insertColumn",
        json!({"target": c.tables[0], "index": 0, "count": 1022}),
    );
    assert_eq!(reason(&r), Some("table_size_limit"));
    // The colSpan=3 title row blocks every inner boundary of the merged table.
    for index in [1, 2] {
        let r = c.edit(
            "table.insertColumn",
            json!({"target": c.tables[1], "index": index}),
        );
        assert_eq!(
            reason(&r),
            Some("insert_inside_merged_cell"),
            "index {index}"
        );
    }
    // The outer boundaries fall next to the title but not inside it; the
    // neighbouring column is vertically merged (TALL) so the title row's
    // cell is cloned with colSpan 3 — refused as having no plain template.
    let r = c.edit(
        "table.insertColumn",
        json!({"target": c.tables[1], "index": 0}),
    );
    assert_eq!(reason(&r), Some("no_simple_template_line"));
    assert_eq!(c.export("same.hwpx"), SOURCE);
}

#[test]
fn insert_column_next_to_a_vertical_merge_clones_the_tall_cell() {
    let mut c = Case::new();
    c.ok(
        "table.insertColumn",
        json!({"target": c.tables[3], "index": 1}),
    );
    let t = shape(&c.doc("out.hwpx"), 3);
    assert_eq!((t.cols, t.width), (3, 63000));
    // Template column 0 holds V (rowSpan 2): its clone spans two rows too.
    let clone = find(&t, 0, 1);
    assert_eq!(
        (clone.cs, clone.rs, clone.h, clone.text.as_str()),
        (1, 2, 4800, "")
    );
    assert!(
        t.cells.iter().all(|c| !(c.row == 1 && c.col == 1)),
        "covered by the clone"
    );
    assert_eq!(find(&t, 0, 2).text, "X0");
    assert_eq!(find(&t, 2, 1).text, "");
}

// ------------------------------------------------------------- delete column

#[test]
fn delete_column_removes_nodes_and_narrows_the_table() {
    let mut c = Case::new();
    let doomed: Vec<String> = (0..4).map(|r| c.cell(0, r, 1).unwrap()).collect();
    let right = c.cell(0, 2, 2).unwrap();
    let r = c.ok(
        "table.deleteColumn",
        json!({"target": c.tables[0], "index": 1}),
    );
    let removed = r["structure"][0]["removed"].as_array().unwrap();
    assert_eq!(removed.len(), 9, "4 cells + 5 paragraphs");
    for id in &doomed {
        assert!(removed.iter().any(|v| v == id.as_str()));
    }
    assert_eq!(c.cell(0, 2, 1).as_deref(), Some(right.as_str()));
    let t = shape(&c.doc("out.hwpx"), 0);
    assert_eq!((t.cols, t.width, t.height), (2, 28000, 9600));
    assert_eq!(find(&t, 2, 1).text, "R2C2");
}

#[test]
fn delete_column_edges_and_count() {
    let mut c = Case::new();
    c.ok(
        "table.deleteColumn",
        json!({"target": c.tables[0], "index": 1, "count": 2}),
    );
    let t = shape(&c.doc("a.hwpx"), 0);
    assert_eq!((t.cols, t.width), (1, 14000));
    assert_eq!(find(&t, 3, 0).text, "R3C0");
    let mut c = Case::new();
    c.ok(
        "table.deleteColumn",
        json!({"target": c.tables[0], "index": 0}),
    );
    let t = shape(&c.doc("b.hwpx"), 0);
    assert_eq!(find(&t, 0, 0).text, "H1");
}

#[test]
fn delete_column_refusals() {
    let mut c = Case::new();
    let r = c.edit(
        "table.deleteColumn",
        json!({"target": c.tables[0], "index": 3}),
    );
    assert_eq!(reason(&r), Some("index_out_of_range"));
    let r = c.edit(
        "table.deleteColumn",
        json!({"target": c.tables[0], "index": 0, "count": 3}),
    );
    assert_eq!(reason(&r), Some("would_empty_table"));
    for index in 0..3 {
        let r = c.edit(
            "table.deleteColumn",
            json!({"target": c.tables[1], "index": index}),
        );
        assert_eq!(
            reason(&r),
            Some("merged_cell_blocks_delete"),
            "title spans every column"
        );
    }
    let r = c.edit(
        "table.deleteColumn",
        json!({"target": c.tables[3], "index": 0}),
    );
    assert_eq!(reason(&r), Some("merged_cell_blocks_delete"));
    let r = c.edit(
        "table.deleteColumn",
        json!({"target": c.tables[3], "index": 1}),
    );
    assert_eq!(reason(&r), Some("row_would_become_empty"));
    assert_eq!(c.export("same.hwpx"), SOURCE);
}

// ------------------------------------------------- dryRun, batch, Undo, Redo

#[test]
fn dry_run_reports_the_structure_without_ids_or_changes() {
    let mut c = Case::new();
    let r = c.exec.call(
        "table.insertRow",
        json!({"target": c.tables[0], "index": 1, "count": 2}),
        Some(0),
        true,
    );
    assert!(r.ok, "{:?}", r.diagnostics);
    let v = r.result.unwrap();
    assert_eq!(v["dryRun"], true);
    assert_eq!(v["afterVersion"], 0);
    assert_eq!(v["predictedAfterVersion"], 1);
    let created = v["structure"][0]["created"].as_array().unwrap();
    assert_eq!(created.len(), 12);
    assert!(created.iter().all(|n| n["nodeId"].is_null()));
    assert_eq!(created[0]["row"], 1);
    assert_eq!(v["structure"][0]["shiftedCount"], 9);
    assert_eq!(c.export("same.hwpx"), SOURCE);
    let r = c.exec.call("history.undo", json!({}), Some(0), false);
    assert_eq!(code(&r), Code::HistoryEmpty);
    // The same request, applied, yields the predicted shape.
    c.ok(
        "table.insertRow",
        json!({"target": c.tables[0], "index": 1, "count": 2}),
    );
    assert_eq!(shape(&c.doc("out.hwpx"), 0).rows, 6);
}

#[test]
fn stale_expected_version_is_a_conflict() {
    let mut c = Case::new();
    c.ok(
        "table.insertRow",
        json!({"target": c.tables[0], "index": 1}),
    );
    let r = c.exec.call(
        "table.insertRow",
        json!({"target": c.tables[0], "index": 1}),
        Some(0),
        false,
    );
    assert_eq!(code(&r), Code::VersionConflict);
}

#[test]
fn insert_undo_redo_restores_bytes_and_keeps_new_ids() {
    let mut c = Case::new();
    let ids0 = c.nodes();
    let r = c.ok(
        "table.insertRow",
        json!({"target": c.tables[0], "index": 2}),
    );
    let created: Vec<String> = r["structure"][0]["created"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["nodeId"].as_str().unwrap().to_string())
        .collect();
    let ids1 = c.nodes();
    let edited = c.export("edited.hwpx");
    assert_ne!(edited, SOURCE);
    c.undo();
    assert_eq!(
        c.export("undone.hwpx"),
        SOURCE,
        "Undo gives back the original bytes"
    );
    assert_eq!(c.nodes(), ids0);
    c.redo();
    assert_eq!(c.export("redone.hwpx"), edited);
    assert_eq!(c.nodes(), ids1, "Redo restores the same new ids");
    assert!(created.iter().all(|id| ids1.iter().any(|n| &n.0 == id)));
}

#[test]
fn delete_undo_brings_back_the_deleted_ids_and_redo_removes_them_again() {
    let mut c = Case::new();
    let ids0 = c.nodes();
    let doomed = c.cell(0, 1, 1).unwrap();
    c.ok(
        "table.deleteRow",
        json!({"target": c.tables[0], "index": 1}),
    );
    assert!(!c.nodes().iter().any(|n| n.0 == doomed));
    let edited = c.export("edited.hwpx");
    c.undo();
    assert_eq!(c.export("undone.hwpx"), SOURCE);
    assert_eq!(c.nodes(), ids0);
    assert_eq!(c.cell(0, 1, 1).as_deref(), Some(doomed.as_str()));
    c.redo();
    assert_eq!(c.export("redone.hwpx"), edited);
    assert!(!c.nodes().iter().any(|n| n.0 == doomed));
    // A new edit after Undo discards the Redo branch and never reuses ids.
    c.undo();
    let r = c.ok(
        "table.insertRow",
        json!({"target": c.tables[0], "index": 0}),
    );
    let fresh = r["structure"][0]["created"][0]["nodeId"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(!ids0.iter().any(|n| n.0 == fresh));
    let redo = c
        .exec
        .call("history.redo", json!({}), Some(c.version), false);
    assert_eq!(code(&redo), Code::HistoryEmpty);
}

#[test]
fn batch_applies_structure_steps_and_later_steps_follow_moved_cells() {
    let mut c = Case::new();
    let target_cell = c.cell(0, 3, 2).unwrap();
    let r = c.ok(
        "command.batch",
        json!({"steps":[
            {"command":"table.insertRow","params":{"target":c.tables[0],"index":0}},
            {"command":"table.deleteColumn","params":{"target":c.tables[0],"index":1}},
            {"command":"table.insertColumn","params":{"target":c.tables[2],"index":2}},
            // R3C2 moved to row 4 / column 1 by the steps above.
            {"command":"cell.setText","params":{"target":target_cell,"text":"MOVED"}},
            {"command":"table.deleteRow","params":{"target":c.tables[2],"index":1}}
        ]}),
    );
    assert_eq!(r["afterVersion"], 1, "one revision for the whole batch");
    assert_eq!(r["structure"].as_array().unwrap().len(), 4);
    let doc = c.doc("batch.hwpx");
    let t = shape(&doc, 0);
    assert_eq!((t.rows, t.cols, t.width, t.height), (5, 2, 28000, 12000));
    assert_eq!(find(&t, 4, 1).text, "MOVED");
    assert_eq!(find(&t, 0, 0).text, "");
    let a = shape(&doc, 2);
    assert_eq!((a.rows, a.cols, a.width, a.height), (2, 3, 63000, 1000));
    c.undo();
    assert_eq!(c.export("undone.hwpx"), SOURCE);
}

#[test]
fn batch_failure_rolls_back_every_step() {
    let mut c = Case::new();
    let r = c.edit(
        "command.batch",
        json!({"steps":[
            {"command":"table.insertRow","params":{"target":c.tables[0],"index":1}},
            {"command":"table.deleteRow","params":{"target":c.tables[1],"index":2}}
        ]}),
    );
    assert_eq!(code(&r), Code::Unsupported);
    assert_eq!(reason(&r), Some("merged_cell_blocks_delete"));
    assert_eq!(r.diagnostics[0].detail.as_ref().unwrap()["failedStep"], 1);
    assert_eq!(c.export("same.hwpx"), SOURCE);
    assert_eq!(c.nodes().len(), Case::new().nodes().len());
}

#[test]
fn batch_edit_of_a_removed_node_is_refused() {
    let mut c = Case::new();
    let cell = c.cell(0, 2, 0).unwrap();
    let r = c.edit(
        "command.batch",
        json!({"steps":[
            {"command":"table.deleteRow","params":{"target":c.tables[0],"index":2}},
            {"command":"cell.setText","params":{"target":cell,"text":"gone"}}
        ]}),
    );
    assert!(!r.ok);
    assert_eq!(code(&r), Code::TargetNotFound);
    assert_eq!(c.export("same.hwpx"), SOURCE);
}

#[test]
fn old_alias_names_stay_unsupported_with_a_hint() {
    let mut c = Case::new();
    for cmd in [
        "table.addRow",
        "table.addColumn",
        "table.insertRows",
        "table.deleteRows",
        "table.insertColumns",
        "table.deleteColumns",
    ] {
        let r = c
            .exec
            .call(cmd, json!({"target": c.tables[0]}), Some(0), false);
        assert_eq!(code(&r), Code::Unsupported, "{cmd}");
        assert_eq!(reason(&r), Some("structure_change_unsupported"));
        assert!(r.diagnostics[0].message.contains("table.insertRow"));
    }
}

// ----------------------------------------------- caches, preview text, diff

fn cache_counts(doc: &Document) -> (usize, usize, usize) {
    // (before-table top-level, per-table nested counts: [plain, merged])
    let xml = &doc.xml[SECTION];
    let total = |from: usize, to: usize| {
        xml.iter()
            .filter(|(i, e)| {
                e.local == "linesegarray" && (from..to).contains(&xml.el(*i).outer().start)
            })
            .count()
    };
    let tables: Vec<usize> = xml
        .iter()
        .filter(|(_, e)| e.local == "tbl")
        .map(|(i, _)| xml.el(i).outer().start)
        .collect();
    (
        total(0, tables[0]),
        total(tables[0], tables[1]),
        total(tables[1], usize::MAX),
    )
}

#[test]
fn caches_are_dropped_from_the_edited_table_onward_only() {
    let source = Document::open(SOURCE.to_vec(), &Limits::default()).unwrap();
    let (before0, plain0, rest0) = cache_counts(&source);
    assert!(before0 >= 1 && plain0 > 0 && rest0 > 0);
    let mut c = Case::new();
    c.ok(
        "table.insertRow",
        json!({"target": c.tables[1], "index": 2}),
    );
    let doc = c.doc("out.hwpx");
    let (before1, plain1, _) = cache_counts(&doc);
    assert_eq!(
        before1, before0,
        "paragraphs before the table keep their cache"
    );
    assert_eq!(plain1, plain0, "the earlier table is untouched");
    // Every paragraph of the edited table and every later top-level
    // paragraph lost its cache.
    let xml = &doc.xml[SECTION];
    let merged = doc
        .nodes
        .iter()
        .filter(|n| n.kind == Kind::Table)
        .nth(1)
        .unwrap();
    let inside = xml
        .descendants(merged.el)
        .filter(|&i| xml.el(i).local == "linesegarray")
        .count();
    assert_eq!(inside, 0);
    let top_after = xml
        .el(xml.root())
        .children
        .iter()
        .skip_while(|&&p| !xml.descendants(p).any(|d| d == merged.el))
        .filter(|&&p| {
            xml.el(p)
                .children
                .iter()
                .any(|&c| xml.el(c).local == "linesegarray")
        })
        .count();
    assert_eq!(top_after, 0);
}

#[test]
fn preview_text_is_marked_stale_only_when_deleted_cells_held_text() {
    let mut c = Case::new();
    let r = c.ok(
        "table.insertRow",
        json!({"target": c.tables[0], "index": 1}),
    );
    assert_eq!(r["stale"]["prvText"], false);
    assert_eq!(r["stale"]["thumbnail"], true);
    let mut c = Case::new();
    let r = c.ok(
        "table.deleteRow",
        json!({"target": c.tables[0], "index": 1}),
    );
    assert_eq!(r["stale"]["prvText"], true, "R1C0 was in the preview text");
    let doc = c.doc("out.hwpx");
    assert_eq!(
        doc.payload("Preview/PrvText.txt").unwrap().as_ref(),
        b"BEFORE-TABLES\r\nR1C0\r\nR3C2\r\nAFTER-TABLES",
        "the preview text is reported stale, never rewritten"
    );
    // An empty row has no text to go stale.
    let mut c = Case::new();
    c.ok(
        "table.insertRow",
        json!({"target": c.tables[0], "index": 1}),
    );
    let r = c.ok(
        "table.deleteRow",
        json!({"target": c.tables[0], "index": 1}),
    );
    assert_eq!(r["stale"]["prvText"], false);
}

#[test]
fn unedited_package_entries_keep_their_raw_bytes() {
    let mut c = Case::new();
    c.ok(
        "table.deleteRow",
        json!({"target": c.tables[0], "index": 3}),
    );
    let out = c.export("out.hwpx");
    let a = hae_zip::Archive::open(SOURCE.to_vec(), &Limits::default()).unwrap();
    let b = hae_zip::Archive::open(out, &Limits::default()).unwrap();
    for e in a.entries() {
        if e.name == SECTION {
            continue;
        }
        assert_eq!(a.raw_data(&e.name), b.raw_data(&e.name), "{}", e.name);
        assert_eq!(
            a.raw_local_header(&e.name),
            b.raw_local_header(&e.name),
            "{}",
            e.name
        );
    }
}

#[test]
fn document_diff_reports_created_and_removed_nodes_by_id() {
    let mut c = Case::new();
    let doomed = c.cell(0, 3, 0).unwrap();
    let r = c.ok(
        "table.insertRow",
        json!({"target": c.tables[0], "index": 0}),
    );
    c.ok(
        "table.deleteRow",
        json!({"target": c.tables[0], "index": 4}),
    );
    let d = c
        .exec
        .call("document.diff", json!({}), None, false)
        .result
        .unwrap();
    assert_eq!(d["identical"], false);
    assert_eq!(d["createdNodes"].as_array().unwrap().len(), 6);
    let removed: Vec<&str> = d["removedNodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["nodeId"].as_str().unwrap())
        .collect();
    assert_eq!(removed.len(), 6);
    assert!(removed.contains(&doomed.as_str()));
    assert!(r["structure"][0]["created"][0]["nodeId"].is_string());
    // Only the table's aggregated text changed; surviving cells are untouched.
    let changed = d["changedNodes"].as_array().unwrap();
    assert!(
        changed
            .iter()
            .all(|n| n["kind"] == "table" || n["kind"] == "section"),
        "only aggregated texts of containers change"
    );
}

#[test]
fn element_queries_advertise_the_four_commands() {
    let mut c = Case::new();
    let t = c
        .exec
        .call("element.get", json!({"target": c.tables[0]}), None, false)
        .result
        .unwrap();
    let edits = &t["outline"]["edits"];
    for name in [
        "table.insertRow",
        "table.deleteRow",
        "table.insertColumn",
        "table.deleteColumn",
    ] {
        assert_eq!(edits[name]["available"], true, "{name}: {edits}");
    }
    let c2 = c
        .exec
        .call("command.list", json!({}), None, false)
        .result
        .unwrap();
    let names: Vec<&str> = c2["commands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["name"].as_str().unwrap())
        .collect();
    for name in [
        "table.insertRow",
        "table.deleteRow",
        "table.insertColumn",
        "table.deleteColumn",
    ] {
        assert!(names.contains(&name), "{name}");
    }
}

#[test]
fn project_save_and_reopen_keep_created_ids() {
    let mut c = Case::new();
    let r = c.ok(
        "table.insertRow",
        json!({"target": c.tables[0], "index": 2}),
    );
    let created = r["structure"][0]["created"][0]["nodeId"]
        .as_str()
        .unwrap()
        .to_string();
    let removed_cell = c.cell(0, 1, 0).unwrap();
    c.ok(
        "table.deleteRow",
        json!({"target": c.tables[0], "index": 1}),
    );
    let before = c.nodes();
    let project = c.root.join("t.hwpwork");
    let s = c
        .exec
        .call("project.save", json!({"path": project}), None, false);
    assert!(s.ok, "{:?}", s.diagnostics);
    c.exec.call("document.close", json!({}), None, false);
    let o = c
        .exec
        .call("document.open", json!({"path": project}), None, false);
    assert!(o.ok, "{:?}", o.diagnostics);
    assert_eq!(c.nodes(), before);
    assert!(c.nodes().iter().any(|n| n.0 == created));
    assert!(!c.nodes().iter().any(|n| n.0 == removed_cell));
}

#[test]
fn structure_results_are_exact_and_stable_across_a_second_run() {
    // Determinism: the same requests on a fresh session export identical bytes.
    let run = || {
        let mut c = Case::new();
        c.ok(
            "table.insertRow",
            json!({"target": c.tables[0], "index": 2, "count": 2}),
        );
        c.ok(
            "table.insertColumn",
            json!({"target": c.tables[3], "index": 2}),
        );
        c.ok(
            "table.deleteRow",
            json!({"target": c.tables[2], "index": 0}),
        );
        c.export("out.hwpx")
    };
    assert_eq!(run(), run());
}

#[test]
fn consistency_holds_across_a_random_walk_of_structure_edits() {
    // Deterministic pseudo-random walk over the plain table; after every
    // accepted step the exported grid must be self-consistent and Undo of
    // everything must restore the original bytes.
    let mut c = Case::new();
    let mut seed = 0x2545_f491_u32;
    let mut next = |n: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        (seed as usize) % n
    };
    let mut applied = 0;
    for step in 0..40 {
        let t = shape(&c.doc(&format!("w{step}.hwpx")), 0);
        let cmd = [
            "table.insertRow",
            "table.deleteRow",
            "table.insertColumn",
            "table.deleteColumn",
        ][next(4)];
        let lines = if cmd.ends_with("Row") { t.rows } else { t.cols };
        let index = next(lines + 1);
        let count = 1 + next(2);
        let r = c.edit(
            cmd,
            json!({"target": c.tables[0], "index": index, "count": count}),
        );
        if r.ok {
            applied += 1;
            shape(&c.doc(&format!("v{step}.hwpx")), 0);
        } else {
            assert!(
                matches!(code(&r), Code::InvalidArgument | Code::Unsupported),
                "{:?}",
                r.diagnostics
            );
        }
    }
    assert!(applied >= 10, "walk applied only {applied} steps");
    let mut undone = BTreeMap::new();
    for _ in 0..applied {
        c.undo();
        undone.insert(c.version, ());
    }
    assert_eq!(c.export("all-undone.hwpx"), SOURCE);
}
