//! Hand-built candidates for table row/column insertion and deletion. This
//! test crate has no editor/model dependency: the expected bytes are written
//! out from the byte contract in docs/decisions/table-structure-node-creation-design.md §6.
use hae_diag::{Code, Limits};
use hae_package::{PackageState, NS_HP};
use hae_verify::{verify_archive, VerifyOperation};
use hae_xml::XmlDoc;
use hae_zip::Archive;
use serde_json::json;
use std::{ops::Range, sync::Arc};

const SOURCE: &[u8] = include_bytes!("../../../tests/fixtures/tables/structure.hwpx");
const SECTION: &str = "Contents/section0.xml";
type Cut = (Range<usize>, Vec<u8>);

fn parse(b: &[u8]) -> XmlDoc {
    XmlDoc::parse(Arc::<[u8]>::from(b)).unwrap()
}
fn is(d: &XmlDoc, i: usize, n: &str) -> bool {
    d.el(i).is(NS_HP, n)
}
fn kids(d: &XmlDoc, i: usize, n: &str) -> Vec<usize> {
    d.el(i)
        .children
        .iter()
        .copied()
        .filter(|&c| is(d, c, n))
        .collect()
}
fn tables(d: &XmlDoc) -> Vec<usize> {
    d.iter()
        .filter(|(_, e)| e.is(NS_HP, "tbl"))
        .map(|(i, _)| i)
        .collect()
}

/// Source plus an editable list of splices.
struct Build {
    archive: Archive,
    state: PackageState,
    d: XmlDoc,
    cuts: Vec<Cut>,
}
impl Build {
    fn new() -> Self {
        let archive = Archive::open(SOURCE.to_vec(), &Limits::default()).unwrap();
        let state = PackageState::from_archive(&archive);
        let d = parse(state.get(SECTION).unwrap());
        Self {
            archive,
            state,
            d,
            cuts: vec![],
        }
    }
    fn attr(&mut self, el: usize, name: &str, value: impl ToString) {
        self.cuts.push((
            self.d.el(el).attr(name).unwrap().value_span.clone(),
            value.to_string().into_bytes(),
        ));
    }
    fn insert(&mut self, at: usize, bytes: impl AsRef<[u8]>) {
        self.cuts.push((at..at, bytes.as_ref().to_vec()));
    }
    fn cut(&mut self, r: Range<usize>) {
        self.cuts.push((r, vec![]));
    }
    fn tr(&self, table: usize, row: usize) -> usize {
        kids(&self.d, tables(&self.d)[table], "tr")[row]
    }
    fn tcs(&self, table: usize, row: usize) -> Vec<usize> {
        kids(&self.d, self.tr(table, row), "tc")
    }
    fn addr(&mut self, tc: usize, name: &str, value: usize) {
        let a = kids(&self.d, tc, "cellAddr")[0];
        self.attr(a, name, value);
    }
    fn table_attr(&mut self, table: usize, name: &str, value: impl ToString) {
        let t = tables(&self.d)[table];
        self.attr(t, name, value);
    }
    fn size(&mut self, table: usize, name: &str, value: impl ToString) {
        let t = tables(&self.d)[table];
        let sz = kids(&self.d, t, "sz")[0];
        self.attr(sz, name, value);
    }
    /// Layout cache removal: every cache in `table`, plus the top-level
    /// caches from the table's carrier paragraph to the section's end.
    fn clear_caches(&mut self, table: usize) {
        let t = tables(&self.d)[table];
        let carrier = self
            .d
            .ancestors(t)
            .find(|&a| self.d.el(a).parent == Some(self.d.root()))
            .unwrap();
        let from = self.d.el(carrier).outer().start;
        for (i, e) in self.d.iter().filter(|(_, e)| e.is(NS_HP, "linesegarray")) {
            let p = self.d.el(i).parent.unwrap();
            let in_table = self.d.ancestors(p).any(|a| a == t);
            let top = self.d.el(p).parent == Some(self.d.root()) && e.outer().start >= from;
            let inside_removed = self.cuts.iter().any(|c| {
                c.0.start < c.0.end && c.0.start <= e.outer().start && e.outer().end <= c.0.end
            });
            if (in_table || top) && !inside_removed {
                self.cuts.push((e.outer(), vec![]));
            }
        }
    }
    fn candidate(&self) -> (Vec<u8>, PackageState) {
        let new = self
            .d
            .splice(self.cuts.clone(), Default::default())
            .unwrap();
        let mut state = self.state.clone();
        state.set(SECTION, new.bytes().clone());
        let bytes = hae_zip::write(
            &self.archive,
            &hae_package::zip_plan(&self.archive, &state).unwrap(),
        )
        .unwrap();
        (bytes, state)
    }
    fn verify(&self, ops: &[VerifyOperation]) -> hae_diag::Result<()> {
        let (bytes, _) = self.candidate();
        verify_archive(&self.archive, &bytes, ops, &Limits::default()).map(|_| ())
    }
    fn op(&self, command: &str, table: usize, index: usize, count: usize) -> VerifyOperation {
        let t = tables(&self.d)[table];
        serde_json::from_value(json!({
            "op": command, "entry": SECTION, "path": self.d.path(t),
            "index": index, "count": count,
        }))
        .unwrap()
    }
}

/// The byte contract for an empty cell cloned from a fixture body cell.
fn blank_tc(row: usize, col: usize, w: u32, h: u32, border: u32, header: u32) -> String {
    format!(
        "<hp:tc name=\"\" header=\"{header}\" hasMargin=\"0\" protect=\"0\" editable=\"0\" dirty=\"0\" borderFillIDRef=\"{border}\">\
<hp:subList id=\"\" textDirection=\"HORIZONTAL\" lineWrap=\"BREAK\" vertAlign=\"CENTER\" linkListIDRef=\"0\" linkListNextIDRef=\"0\" textWidth=\"0\" textHeight=\"0\" hasTextRef=\"0\" hasNumRef=\"0\">\
<hp:p id=\"10\" paraPrIDRef=\"0\" styleIDRef=\"0\" pageBreak=\"0\" columnBreak=\"0\" merged=\"0\"><hp:run charPrIDRef=\"0\"/></hp:p></hp:subList>\
<hp:cellAddr colAddr=\"{col}\" rowAddr=\"{row}\"/><hp:cellSpan colSpan=\"1\" rowSpan=\"1\"/><hp:cellSz width=\"{w}\" height=\"{h}\"/>\
<hp:cellMargin left=\"141\" right=\"141\" top=\"141\" bottom=\"141\"/></hp:tc>"
    )
}

// ----- the four accepted candidates (plain table 0: 4 rows x 3 cols) ------

/// Insert one row at index 2 (cloned from body row 1).
fn insert_row() -> Build {
    let mut b = Build::new();
    let at = b.d.el(b.tr(0, 2)).outer().start;
    let row: String = (0..3).map(|c| blank_tc(2, c, 14000, 2400, 3, 0)).collect();
    b.insert(at, format!("<hp:tr>{row}</hp:tr>"));
    for r in 2..4 {
        for tc in b.tcs(0, r) {
            b.addr(tc, "rowAddr", r + 1);
        }
    }
    b.table_attr(0, "rowCnt", 5);
    b.size(0, "height", 12000);
    b.clear_caches(0);
    b
}
/// Insert one column at index 1 (cloned from column 0; header cell stays header).
fn insert_column() -> Build {
    let mut b = Build::new();
    for r in 0..4 {
        let tcs = b.tcs(0, r);
        let at = b.d.el(tcs[1]).outer().start;
        let (border, header) = if r == 0 { (4, 1) } else { (3, 0) };
        b.insert(at, blank_tc(r, 1, 14000, 2400, border, header));
        for &tc in &tcs[1..] {
            let col: usize =
                b.d.el(kids(&b.d, tc, "cellAddr")[0])
                    .attr_value("colAddr")
                    .unwrap()
                    .parse()
                    .unwrap();
            b.addr(tc, "colAddr", col + 1);
        }
    }
    b.table_attr(0, "colCnt", 4);
    b.size(0, "width", 56000);
    b.clear_caches(0);
    b
}
/// Delete row 2 (R2C0..R2C2).
fn delete_row() -> Build {
    let mut b = Build::new();
    b.cut(b.d.el(b.tr(0, 2)).outer());
    for tc in b.tcs(0, 3) {
        b.addr(tc, "rowAddr", 2);
    }
    b.table_attr(0, "rowCnt", 3);
    b.size(0, "height", 7200);
    b.clear_caches(0);
    b
}
/// Delete column 1.
fn delete_column() -> Build {
    let mut b = Build::new();
    for r in 0..4 {
        let tcs = b.tcs(0, r);
        b.cut(b.d.el(tcs[1]).outer());
        b.addr(tcs[2], "colAddr", 1);
    }
    b.table_attr(0, "colCnt", 2);
    b.size(0, "width", 28000);
    b.clear_caches(0);
    b
}

fn insert_row_op(b: &Build) -> VerifyOperation {
    b.op("table.insertRow", 0, 2, 1)
}

#[test]
fn accepts_each_hand_built_candidate() {
    let b = insert_row();
    b.verify(&[insert_row_op(&b)]).unwrap();
    let b = insert_column();
    b.verify(&[b.op("table.insertColumn", 0, 1, 1)]).unwrap();
    let b = delete_row();
    b.verify(&[b.op("table.deleteRow", 0, 2, 1)]).unwrap();
    let b = delete_column();
    b.verify(&[b.op("table.deleteColumn", 0, 1, 1)]).unwrap();
}

fn reject(b: &Build, op: VerifyOperation, why: &str) {
    let e = b.verify(&[op]).expect_err(why);
    assert_eq!(e.code(), Code::VerifyFailed, "{why}: {e}");
}

/// Apply one more edit to an accepted candidate and expect the gate to refuse.
fn tampered(
    mut b: Build,
    op: impl Fn(&Build) -> VerifyOperation,
    why: &str,
    edit: impl FnOnce(&mut Build),
) {
    edit(&mut b);
    let o = op(&b);
    reject(&b, o, why);
}

#[test]
fn insert_row_rejects_wrong_counts_and_addresses() {
    let op = |b: &Build| b.op("table.insertRow", 0, 2, 1);
    // rowCnt left at 4: cut the attribute edit by overriding it.
    let mut b = insert_row();
    b.cuts
        .retain(|(r, _)| *r != b.d.el(tables(&b.d)[0]).attr("rowCnt").unwrap().value_span);
    reject(&b, op(&b), "rowCnt not updated");
    // new row addressed as row 5
    let mut b = insert_row();
    b.cuts.retain(|c| c.0.start != c.0.end);
    let at = b.d.el(b.tr(0, 2)).outer().start;
    let row: String = (0..3).map(|c| blank_tc(5, c, 14000, 2400, 3, 0)).collect();
    b.insert(at, format!("<hp:tr>{row}</hp:tr>"));
    reject(&b, op(&b), "wrong new rowAddr");
    // the table height is not updated
    let mut b = insert_row();
    let t = tables(&b.d)[0];
    let sz = kids(&b.d, t, "sz")[0];
    let span = b.d.el(sz).attr("height").unwrap().value_span.clone();
    b.cuts.retain(|c| c.0 != span);
    reject(&b, op(&b), "height not updated");
    // a shifted cell keeps its old address
    let mut b = insert_row();
    let tc = b.tcs(0, 3)[0];
    let a = kids(&b.d, tc, "cellAddr")[0];
    let span = b.d.el(a).attr("rowAddr").unwrap().value_span.clone();
    b.cuts.retain(|c| c.0 != span);
    reject(&b, op(&b), "shifted cell not updated");
}

#[test]
fn insert_row_rejects_content_and_style_deviations() {
    let op = |b: &Build| b.op("table.insertRow", 0, 2, 1);
    // new cells carry text
    let mut b = insert_row();
    b.cuts.retain(|c| c.0.start != c.0.end);
    let at = b.d.el(b.tr(0, 2)).outer().start;
    let row: String = (0..3)
        .map(|c| {
            blank_tc(2, c, 14000, 2400, 3, 0).replace(
                "<hp:run charPrIDRef=\"0\"/>",
                "<hp:run charPrIDRef=\"0\"><hp:t>x</hp:t></hp:run>",
            )
        })
        .collect();
    b.insert(at, format!("<hp:tr>{row}</hp:tr>"));
    reject(&b, op(&b), "new cell is not empty");
    // borders differ from the template
    let mut b = insert_row();
    b.cuts.retain(|c| c.0.start != c.0.end);
    let at = b.d.el(b.tr(0, 2)).outer().start;
    let row: String = (0..3).map(|c| blank_tc(2, c, 14000, 2400, 4, 0)).collect();
    b.insert(at, format!("<hp:tr>{row}</hp:tr>"));
    reject(&b, op(&b), "borderFill differs from the template row");
    // a header flag left on a body clone / cell name kept
    for (from, to) in [
        ("header=\"0\"", "header=\"1\""),
        ("name=\"\"", "name=\"dup\""),
    ] {
        let mut b = insert_row();
        b.cuts.retain(|c| c.0.start != c.0.end);
        let at = b.d.el(b.tr(0, 2)).outer().start;
        let row: String = (0..3)
            .map(|c| blank_tc(2, c, 14000, 2400, 3, 0).replace(from, to))
            .collect();
        b.insert(at, format!("<hp:tr>{row}</hp:tr>"));
        reject(&b, op(&b), from);
    }
    // inserted before row 1 instead of row 2
    let mut b = insert_row();
    b.cuts.retain(|c| c.0.start != c.0.end);
    let at = b.d.el(b.tr(0, 1)).outer().start;
    let row: String = (0..3).map(|c| blank_tc(2, c, 14000, 2400, 3, 0)).collect();
    b.insert(at, format!("<hp:tr>{row}</hp:tr>"));
    reject(&b, op(&b), "wrong position");
    // two rows where one was requested
    let mut b = insert_row();
    let at = b.d.el(b.tr(0, 2)).outer().start;
    let row: String = (0..3).map(|c| blank_tc(3, c, 14000, 2400, 3, 0)).collect();
    b.insert(at, format!("<hp:tr>{row}</hp:tr>"));
    reject(&b, op(&b), "extra row");
}

#[test]
fn insert_row_rejects_cache_and_neighbour_tampering() {
    let op = |b: &Build| b.op("table.insertRow", 0, 2, 1);
    // a cell paragraph keeps its layout cache
    let mut b = insert_row();
    let t = tables(&b.d)[0];
    let keep =
        b.d.descendants(t)
            .find(|&i| is(&b.d, i, "linesegarray"))
            .unwrap();
    let r = b.d.el(keep).outer();
    b.cuts.retain(|c| c.0 != r);
    reject(&b, op(&b), "cache left in the edited table");
    // a paragraph before the table loses its cache
    let mut b = insert_row();
    let first =
        b.d.iter()
            .find(|(i, e)| {
                e.is(NS_HP, "linesegarray")
                    && b.d.el(b.d.el(*i).parent.unwrap()).parent == Some(b.d.root())
            })
            .unwrap()
            .0;
    b.cut(b.d.el(first).outer());
    reject(&b, op(&b), "earlier paragraph cache removed");
    // another table changes
    tampered(insert_row(), op, "other table touched", |b| {
        b.size(1, "height", 1);
    });
    // another table's text changes
    tampered(insert_row(), op, "other cell text", |b| {
        let tc = b.tcs(1, 1)[0];
        let t = b.d.descendants(tc).find(|&i| is(&b.d, i, "t")).unwrap();
        let r = b.d.el(t).inner();
        b.cuts.push((r, b"zzz".to_vec()));
    });
}

#[test]
fn insert_row_rejects_a_request_that_was_not_made() {
    let b = insert_row();
    reject(&b, b.op("table.insertRow", 0, 2, 2), "count mismatch");
    reject(&b, b.op("table.insertRow", 0, 1, 1), "index mismatch");
    reject(&b, b.op("table.insertRow", 0, 9, 1), "index out of range");
    reject(&b, b.op("table.insertRow", 0, 2, 0), "count zero");
    reject(&b, b.op("table.insertColumn", 0, 2, 1), "other axis");
    // The unrequested candidate is not a no-op candidate either.
    let (bytes, _) = b.candidate();
    let e = verify_archive(&b.archive, &bytes, &[], &Limits::default()).unwrap_err();
    assert_eq!(e.code(), Code::VerifyFailed);
}

#[test]
fn insert_column_rejects_deviations() {
    let op = |b: &Build| b.op("table.insertColumn", 0, 1, 1);
    // a row misses its new cell
    let mut b = insert_column();
    let tcs = b.tcs(0, 2);
    let at = b.d.el(tcs[1]).outer().start;
    b.cuts.retain(|c| c.0.start != at);
    reject(&b, op(&b), "row without the new cell");
    // colCnt not updated
    let mut b = insert_column();
    let span =
        b.d.el(tables(&b.d)[0])
            .attr("colCnt")
            .unwrap()
            .value_span
            .clone();
    b.cuts.retain(|c| c.0 != span);
    reject(&b, op(&b), "colCnt");
    // table width off by one
    tampered(insert_column(), op, "width", |b| {
        let t = tables(&b.d)[0];
        let sz = kids(&b.d, t, "sz")[0];
        let span = b.d.el(sz).attr("width").unwrap().value_span.clone();
        b.cuts.retain(|c| c.0 != span);
        b.cuts.push((span, b"56001".to_vec()));
    });
    // header cell of the new column loses its header flag
    let mut b = insert_column();
    let tcs = b.tcs(0, 0);
    let at = b.d.el(tcs[1]).outer().start;
    b.cuts.retain(|c| c.0.start != at);
    b.insert(at, blank_tc(0, 1, 14000, 2400, 4, 0));
    reject(&b, op(&b), "header flag");
    // inserted one column to the right
    let b = insert_column();
    reject(&b, b.op("table.insertColumn", 0, 2, 1), "wrong index");
}

#[test]
fn delete_rejects_deviations() {
    // a different row was removed
    let mut b = Build::new();
    b.cut(b.d.el(b.tr(0, 1)).outer());
    for r in 2..4 {
        for tc in b.tcs(0, r) {
            b.addr(tc, "rowAddr", r - 1);
        }
    }
    b.table_attr(0, "rowCnt", 3);
    b.size(0, "height", 7200);
    b.clear_caches(0);
    reject(&b, b.op("table.deleteRow", 0, 2, 1), "wrong row removed");
    // the row is emptied instead of removed
    let mut b = delete_row();
    b.cuts.clear();
    let tc = b.tcs(0, 2)[0];
    let t = b.d.descendants(tc).find(|&i| is(&b.d, i, "t")).unwrap();
    b.cuts.push((b.d.el(t).inner(), vec![]));
    reject(&b, b.op("table.deleteRow", 0, 2, 1), "blanked, not removed");
    // below-row addresses left untouched
    let mut b = delete_row();
    let tc = b.tcs(0, 3)[0];
    let a = kids(&b.d, tc, "cellAddr")[0];
    let span = b.d.el(a).attr("rowAddr").unwrap().value_span.clone();
    b.cuts.retain(|c| c.0 != span);
    reject(&b, b.op("table.deleteRow", 0, 2, 1), "rowAddr not shifted");
    // table height not reduced
    let mut b = delete_row();
    let sz = kids(&b.d, tables(&b.d)[0], "sz")[0];
    let span = b.d.el(sz).attr("height").unwrap().value_span.clone();
    b.cuts.retain(|c| c.0 != span);
    reject(&b, b.op("table.deleteRow", 0, 2, 1), "height not reduced");
    // column delete leaves one cell of the column
    let mut b = delete_column();
    let tcs = b.tcs(0, 1);
    let r = b.d.el(tcs[1]).outer();
    b.cuts.retain(|c| c.0 != r);
    reject(
        &b,
        b.op("table.deleteColumn", 0, 1, 1),
        "cell of the column kept",
    );
    // column delete forgets the shift
    let mut b = delete_column();
    let tc = b.tcs(0, 0)[2];
    let a = kids(&b.d, tc, "cellAddr")[0];
    let span = b.d.el(a).attr("colAddr").unwrap().value_span.clone();
    b.cuts.retain(|c| c.0 != span);
    reject(
        &b,
        b.op("table.deleteColumn", 0, 1, 1),
        "colAddr not shifted",
    );
}

#[test]
fn requests_the_engine_refuses_are_refused_by_the_gate_too() {
    // Even a perfectly cut candidate cannot launder a merged-cell deletion:
    // the gate judges the request against the source, not the bytes.
    let mut b = Build::new();
    // merged table 1: remove the row holding the rowSpan=2 cell
    b.cut(b.d.el(b.tr(1, 2)).outer());
    b.table_attr(1, "rowCnt", 3);
    b.size(1, "height", 7200);
    for tc in b.tcs(1, 3) {
        b.addr(tc, "rowAddr", 2);
    }
    b.clear_caches(1);
    reject(&b, b.op("table.deleteRow", 1, 2, 1), "merged cell in range");
    let mut b = Build::new();
    b.cut(b.d.el(b.tr(1, 0)).outer());
    b.table_attr(1, "rowCnt", 3);
    b.clear_caches(1);
    reject(
        &b,
        b.op("table.deleteRow", 1, 0, 1),
        "colSpan title in range",
    );
    // an insert that splits TALL (rows 2-3 of the merged table)
    let b = Build::new();
    let e = b
        .verify(&[b.op("table.insertRow", 1, 3, 1)])
        .expect_err("splits a merged cell");
    assert_eq!(e.code(), Code::VerifyFailed);
    // deleting every row
    let b = Build::new();
    assert!(b.verify(&[b.op("table.deleteRow", 0, 0, 4)]).is_err());
}

#[test]
fn a_non_table_target_is_refused() {
    let b = insert_row();
    let tc = b.tcs(0, 0)[0];
    let op: VerifyOperation = serde_json::from_value(json!({
        "op":"table.insertRow","entry":SECTION,"path":b.d.path(tc),"index":2,"count":1
    }))
    .unwrap();
    reject(&b, op, "cell is not a table");
}

#[test]
fn later_requests_follow_cells_moved_by_an_earlier_structure_step() {
    // Insert a row at index 2 of table 0, then replace the text of the cell
    // that was R3C0 (row 3 before the insert, row 4 afterwards). The request
    // names the pre-request address; the gate must follow it.
    let mut b = insert_row();
    let t = tables(&b.d)[0];
    let target = kids(&b.d, kids(&b.d, t, "tr")[3], "tc")[0];
    let text = b.d.descendants(target).find(|&i| is(&b.d, i, "t")).unwrap();
    // Cut spans refer to the source; the replacement text goes into the same
    // hp:t of the moved cell.
    b.cuts.push((b.d.el(text).inner(), b"Z".to_vec()));
    let ops = [
        insert_row_op(&b),
        serde_json::from_value(json!({
            "op":"cell.setText","entry":SECTION,"path":b.d.path(target),"text":"Z"
        }))
        .unwrap(),
    ];
    // The text edit also clears caches of the cell paragraph; the structure
    // step already cleared every cache of the table, so nothing new is due.
    b.verify(&ops).unwrap();
    // The same request list but the text lands in the wrong cell is refused.
    let mut wrong = insert_row();
    let other = kids(&wrong.d, kids(&wrong.d, t, "tr")[3], "tc")[1];
    let text = wrong
        .d
        .descendants(other)
        .find(|&i| is(&wrong.d, i, "t"))
        .unwrap();
    wrong.cuts.push((wrong.d.el(text).inner(), b"Z".to_vec()));
    assert!(wrong.verify(&ops).is_err());
    // A step addressing a cell removed by an earlier step is refused.
    let gone = delete_row();
    let removed = kids(&gone.d, kids(&gone.d, t, "tr")[2], "tc")[0];
    let ops = [
        gone.op("table.deleteRow", 0, 2, 1),
        serde_json::from_value(json!({
            "op":"cell.setText","entry":SECTION,"path":gone.d.path(removed),"text":"Z"
        }))
        .unwrap(),
    ];
    assert!(gone.verify(&ops).is_err());
}

#[test]
fn two_structure_steps_on_one_table_compose() {
    // Insert a row at 2, then delete the row that was R3 (now at index 4).
    let mut b = Build::new();
    let at = b.d.el(b.tr(0, 2)).outer().start;
    let row: String = (0..3).map(|c| blank_tc(2, c, 14000, 2400, 3, 0)).collect();
    b.insert(at, format!("<hp:tr>{row}</hp:tr>"));
    b.cut(b.d.el(b.tr(0, 3)).outer());
    // The original row 2 is now row 3 and keeps its place; the rest moves by 0.
    for tc in b.tcs(0, 2) {
        b.addr(tc, "rowAddr", 3);
    }
    b.table_attr(0, "rowCnt", 4);
    b.size(0, "height", 9600);
    b.clear_caches(0);
    let ops = [
        b.op("table.insertRow", 0, 2, 1),
        b.op("table.deleteRow", 0, 4, 1),
    ];
    b.verify(&ops).unwrap();
}
