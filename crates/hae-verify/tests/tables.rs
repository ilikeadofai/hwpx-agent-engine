//! Hand-built expected candidates. This test crate has no editor/model dependency.
use hae_diag::{Code, Limits};
use hae_package::{PackageState, NS_HP};
use hae_verify::{verify_archive, VerifyOperation};
use hae_xml::XmlDoc;
use hae_zip::Archive;
use serde_json::json;
use std::{ops::Range, sync::Arc};
const SOURCE: &[u8] = include_bytes!("../../../tests/fixtures/tables/dimensions.hwpx");
const SECTION: &str = "Contents/section0.xml";
fn parse(b: &[u8]) -> XmlDoc {
    XmlDoc::parse(Arc::<[u8]>::from(b)).unwrap()
}
fn hp(d: &XmlDoc, i: usize, n: &str) -> bool {
    d.el(i).is(NS_HP, n)
}
fn children(d: &XmlDoc, i: usize, n: &str) -> Vec<usize> {
    d.el(i)
        .children
        .iter()
        .copied()
        .filter(|&c| hp(d, c, n))
        .collect()
}
fn attr(d: &XmlDoc, i: usize, a: &str, v: impl ToString, cuts: &mut Vec<(Range<usize>, Vec<u8>)>) {
    cuts.push((
        d.el(i).attr(a).unwrap().value_span.clone(),
        v.to_string().into_bytes(),
    ));
}
fn zip(archive: &Archive, state: &PackageState) -> Vec<u8> {
    hae_zip::write(archive, &hae_package::zip_plan(archive, state).unwrap()).unwrap()
}
fn candidate(command: &str) -> (Archive, PackageState, VerifyOperation) {
    candidate_from(SOURCE, command)
}
fn candidate_from(source: &[u8], command: &str) -> (Archive, PackageState, VerifyOperation) {
    let archive = Archive::open(source.to_vec(), &Limits::default()).unwrap();
    let mut state = PackageState::from_archive(&archive);
    let d = parse(state.get(SECTION).unwrap());
    let tables: Vec<_> = d
        .iter()
        .filter(|(_, e)| e.is(NS_HP, "tbl"))
        .map(|(i, _)| i)
        .collect();
    let ti = if command.starts_with("table.") { 0 } else { 1 };
    let table = tables[ti];
    let cells: Vec<_> = d.descendants(table).filter(|&i| hp(&d, i, "tc")).collect();
    let target = if ti == 0 { table } else { cells[0] };
    let mut cuts = vec![];
    let size = children(&d, table, "sz")[0];
    let params = match command {
        "table.setColumnWidths" => {
            attr(&d, size, "width", 45000, &mut cuts);
            for (c, w) in cells.iter().zip([45000, 22000, 23000]) {
                attr(&d, children(&d, *c, "cellSz")[0], "width", w, &mut cuts);
            }
            json!({"widths":[22000,23000]})
        }
        "table.setRowHeights" => {
            attr(&d, size, "height", 5800, &mut cuts);
            for (c, h) in cells.iter().zip([2800, 3000, 3000]) {
                attr(&d, children(&d, *c, "cellSz")[0], "height", h, &mut cuts);
            }
            json!({"heights":[2800,3000]})
        }
        "cell.setSize" => {
            attr(&d, size, "width", 44000, &mut cuts);
            let last_size = children(&d, *cells.last().unwrap(), "cellSz")[0];
            let old_last_height: u32 = d
                .el(last_size)
                .attr_value("height")
                .unwrap()
                .parse()
                .unwrap();
            attr(&d, size, "height", 2800 + old_last_height, &mut cuts);
            for &i in &[0, 2] {
                attr(
                    &d,
                    children(&d, cells[i], "cellSz")[0],
                    "width",
                    23000,
                    &mut cuts,
                );
            }
            for &i in &[0, 1] {
                attr(
                    &d,
                    children(&d, cells[i], "cellSz")[0],
                    "height",
                    2800,
                    &mut cuts,
                );
            }
            json!({"width":23000,"height":2800})
        }
        "cell.setFormat" => {
            attr(&d, target, "borderFillIDRef", 4, &mut cuts);
            attr(&d, target, "hasMargin", 1, &mut cuts);
            let m = children(&d, target, "cellMargin")[0];
            attr(&d, m, "left", 200, &mut cuts);
            attr(&d, m, "top", 210, &mut cuts);
            json!({"format":{"borderFillId":"3","background":"#aabbcc","cellMargin":{"left":200,"top":210}}})
        }
        _ => unreachable!(),
    };
    let top = d
        .ancestors(table)
        .find(|&i| d.el(i).parent == Some(d.root()))
        .unwrap();
    for (i, e) in d.iter().filter(|(_, e)| e.is(NS_HP, "linesegarray")) {
        let p = d.el(i).parent.unwrap();
        let owner = d.ancestors(p).find(|&a| hp(&d, a, "tbl"));
        if owner == Some(table)
            || (d.el(p).parent == Some(d.root()) && e.outer().start >= d.el(top).outer().start)
        {
            cuts.push((e.outer(), vec![]));
        }
    }
    let new = d.splice(cuts, Default::default()).unwrap();
    state.set(SECTION, new.bytes().clone());
    let mut request = params;
    request["op"] = json!(command);
    request["entry"] = json!(SECTION);
    request["path"] = json!(d.path(target));
    (archive, state, serde_json::from_value(request).unwrap())
}
fn run(command: &str, tamper: Option<&str>) {
    let (archive, mut state, op) = candidate(command);
    if let Some(tamper) = tamper {
        let d = parse(state.get(SECTION).unwrap());
        let tables: Vec<_> = d
            .iter()
            .filter(|(_, e)| e.is(NS_HP, "tbl"))
            .map(|(i, _)| i)
            .collect();
        let ti = if command.starts_with("table.") { 0 } else { 1 };
        let table = tables[ti];
        let mut cuts = vec![];
        let cells: Vec<_> = d.descendants(table).filter(|&i| hp(&d, i, "tc")).collect();
        match tamper {
            "total" => attr(&d, children(&d, table, "sz")[0], "width", 45001, &mut cuts),
            "other_table" => attr(
                &d,
                children(&d, tables[2], "sz")[0],
                "height",
                9999,
                &mut cuts,
            ),
            "unrequested_cell" => attr(
                &d,
                children(&d, *cells.last().unwrap(), "cellMargin")[0],
                "right",
                999,
                &mut cuts,
            ),
            "cell_dimension" => attr(
                &d,
                children(&d, *cells.last().unwrap(), "cellSz")[0],
                "height",
                7777,
                &mut cuts,
            ),
            "text" => {
                let t = d.descendants(cells[0]).find(|&i| hp(&d, i, "t")).unwrap();
                cuts.push((d.el(t).inner(), b"TAMPER".to_vec()));
            }
            "new_node" => {
                let end = d.el(table).end.as_ref().unwrap().start;
                cuts.push((end..end, b"<hp:tr/>".to_vec()));
            }
            "header" => {
                let raw = state.get("Contents/header.xml").unwrap();
                let text = String::from_utf8_lossy(raw).replacen("#AABBCC", "#AABBCD", 1);
                assert_ne!(text.as_bytes(), raw.as_ref());
                state.set("Contents/header.xml", Arc::from(text.into_bytes()));
            }
            _ => unreachable!(),
        }
        if !cuts.is_empty() {
            state.set(
                SECTION,
                d.splice(cuts, Default::default()).unwrap().bytes().clone(),
            );
        }
    }
    let result = verify_archive(&archive, &zip(&archive, &state), &[op], &Limits::default());
    if let Some(t) = tamper {
        let e = result.unwrap_err();
        assert_eq!(e.code(), Code::VerifyFailed, "{command} {t}: {e:?}");
    } else {
        assert!(result.is_ok(), "{command}: {result:?}");
    }
}
macro_rules! suite {
    ($cmd:literal,$good:ident,$total:ident,$other:ident,$cell:ident,$dim:ident,$text:ident,$node:ident,$header:ident) => {
        #[test]
        fn $good() {
            run($cmd, None);
        }
        #[test]
        fn $total() {
            run($cmd, Some("total"));
        }
        #[test]
        fn $other() {
            run($cmd, Some("other_table"));
        }
        #[test]
        fn $cell() {
            run($cmd, Some("unrequested_cell"));
        }
        #[test]
        fn $dim() {
            run($cmd, Some("cell_dimension"));
        }
        #[test]
        fn $text() {
            run($cmd, Some("text"));
        }
        #[test]
        fn $node() {
            run($cmd, Some("new_node"));
        }
        #[test]
        fn $header() {
            run($cmd, Some("header"));
        }
    };
}
suite!(
    "table.setColumnWidths",
    columns_accept,
    columns_reject_total,
    columns_reject_other_table,
    columns_reject_unrequested_cell,
    columns_reject_dimension,
    columns_reject_text,
    columns_reject_new_node,
    columns_reject_header
);
suite!(
    "table.setRowHeights",
    rows_accept,
    rows_reject_total,
    rows_reject_other_table,
    rows_reject_unrequested_cell,
    rows_reject_dimension,
    rows_reject_text,
    rows_reject_new_node,
    rows_reject_header
);
suite!(
    "cell.setFormat",
    format_accept,
    format_reject_total,
    format_reject_other_table,
    format_reject_unrequested_cell,
    format_reject_dimension,
    format_reject_text,
    format_reject_new_node,
    format_reject_header
);
suite!(
    "cell.setSize",
    size_accept,
    size_reject_total,
    size_reject_other_table,
    size_reject_unrequested_cell,
    size_reject_dimension,
    size_reject_text,
    size_reject_new_node,
    size_reject_header
);

#[test]
fn native_layout_height_independent_candidates_and_tampering() {
    let source = include_bytes!("../../../tests/fixtures/tables/layout_height.hwpx");
    for command in [
        "table.setColumnWidths",
        "table.setRowHeights",
        "cell.setSize",
    ] {
        let (archive, state, op) = candidate_from(source, command);
        let candidate = zip(&archive, &state);
        assert!(
            verify_archive(
                &archive,
                &candidate,
                std::slice::from_ref(&op),
                &Limits::default()
            )
            .is_ok(),
            "{command}"
        );
        for (name, attribute) in [
            ("sz", "height"),
            ("cellSz", "height"),
            ("cellSz", "width"),
            ("cellMargin", "right"),
            ("cellAddr", "colAddr"),
            ("cellSpan", "colSpan"),
        ] {
            let mut tampered = state.clone();
            let d = parse(tampered.get(SECTION).unwrap());
            let table = d.iter().find(|(_, e)| e.is(NS_HP, "tbl")).unwrap().0;
            let el = d.descendants(table).find(|&i| hp(&d, i, name)).unwrap();
            let old: u32 = d.el(el).attr_value(attribute).unwrap().parse().unwrap();
            let mut cuts = vec![];
            attr(&d, el, attribute, old + 1, &mut cuts);
            tampered.set(
                SECTION,
                d.splice(cuts, Default::default()).unwrap().bytes().clone(),
            );
            assert!(
                verify_archive(
                    &archive,
                    &zip(&archive, &tampered),
                    std::slice::from_ref(&op),
                    &Limits::default()
                )
                .is_err(),
                "{command} {name}.{attribute}"
            );
        }
    }
}
