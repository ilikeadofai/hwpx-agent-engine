//! Page-information join (spec §7, task 3-3): rhwp layout addresses ↔ nodeIds.
//!
//! The rules follow the stage-0 S2 findings (`spikes/s2_mapping/map_paths.py`,
//! `spikes/work/s2/nodeid_mapping.md`):
//!
//! * Section numbers are content.hpf spine order (`Document::info.sections`).
//! * rhwp's paragraph number is the ordinal among the section root's `hp:p`
//!   children; the control number `ci` counts every control rhwp pushes onto
//!   the paragraph (secPr, colPr, ctrl children, objects), not run-local
//!   ordinals; cells are addressed by `hp:cellAddr` (row, column).
//! * A picture inside a cell has no usable paragraph number in rhwp's JSON, so
//!   its key is (cell, control ordinal) and it is only unique if no second
//!   paragraph of that cell holds a picture with the same ordinal.
//! * Both `export-render-tree` (addresses, no section) and
//!   `dump-pages --json` (section, object kinds, text previews) are required.
//!
//! Only top-level paragraphs, tables, cells and pictures get pages. Everything
//! else — elements inside groups, text boxes, headers, footers, notes, nested
//! paragraphs, equations — is `Unavailable` with a reason. Before joining, a
//! section is cross-checked (self-closing `hp:p`, table row/column counts,
//! object kinds, text prefixes, orphan addresses); any mismatch makes the
//! whole section unavailable. All page numbers carry
//! `source = "renderer-estimated"`.

use hae_model::{Document, Kind};
use hae_xml::{Item, XmlDoc};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Value of the `source` field on every page record.
pub const SOURCE: &str = "renderer-estimated";

const SENTINEL: u64 = u64::MAX; // usize::MAX in rhwp, used for header/footer filler lines

const OBJ_KINDS: [&str; 20] = [
    "tbl",
    "pic",
    "rect",
    "ellipse",
    "line",
    "connectLine",
    "arc",
    "polygon",
    "curve",
    "container",
    "compose",
    "dutmal",
    "equation",
    "btn",
    "checkBtn",
    "radioBtn",
    "comboBox",
    "edit",
    "ole",
    "chart",
];
const CTRL_CHILDREN: [&str; 14] = [
    "colPr",
    "header",
    "footer",
    "footNote",
    "endNote",
    "autoNum",
    "hiddenComment",
    "indexmark",
    "fieldBegin",
    "pageHiding",
    "pageNumCtrl",
    "pageNum",
    "bookmark",
    "newNum",
];
const SHAPE_KINDS: [&str; 10] = [
    "rect",
    "ellipse",
    "line",
    "connectLine",
    "arc",
    "polygon",
    "curve",
    "container",
    "ole",
    "chart",
];
const CONTAINER_CTLS: [&str; 5] = ["header", "footer", "footNote", "endNote", "hiddenComment"];
const SHAPE_HOSTS: [&str; 4] = ["compose", "dutmal", "equation", "pic"];

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Seg {
    S(usize),
    P(usize),
    /// Paragraph number rhwp does not report (pictures inside cells).
    PAny,
    C(usize),
    Cell(u64, u64),
}
type Addr = Vec<Seg>;

/// Page result for one node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", untagged)]
pub enum PageStatus {
    /// 1-based physical page numbers (rhwp page order), ascending.
    Pages {
        pages: Vec<u32>,
        source: &'static str,
    },
    Unavailable {
        unavailable: String,
    },
}

impl PageStatus {
    fn pages(p: Vec<u32>) -> Self {
        PageStatus::Pages {
            pages: p,
            source: SOURCE,
        }
    }
    fn unavailable(r: impl Into<String>) -> Self {
        PageStatus::Unavailable {
            unavailable: r.into(),
        }
    }
    pub fn page_list(&self) -> Option<&[u32]> {
        match self {
            PageStatus::Pages { pages, .. } => Some(pages),
            PageStatus::Unavailable { .. } => None,
        }
    }
}

/// Availability of one section's page information.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionStatus {
    pub index: usize,
    pub entry: String,
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Resolved counts (nodes that received pages).
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PageCounts {
    pub paragraphs: usize,
    pub tables: usize,
    pub cells: usize,
    pub pictures: usize,
    pub unavailable: usize,
}

/// The join result.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageInfo {
    pub source: &'static str,
    pub page_count: usize,
    pub sections: Vec<SectionStatus>,
    pub nodes: BTreeMap<String, PageStatus>,
    pub counts: PageCounts,
}

impl PageInfo {
    pub fn status(&self, node_id: &str) -> Option<&PageStatus> {
        self.nodes.get(node_id)
    }
}

// ───────────────────────── our side: structural addresses ─────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RK {
    P,
    Tbl,
    Tc,
    Pic,
    Eq,
    Shape,
    Ctl,
}

#[derive(Debug, Clone)]
struct Rec {
    si: usize,
    el: usize,
    kind: RK,
    addr: Option<Addr>,
    reason: Option<String>,
    /// Top-level paragraph text prefix source / table dimensions.
    text: String,
    rows: i64,
    cols: i64,
}

fn local(d: &XmlDoc, i: usize) -> &str {
    &d.el(i).local
}

fn children(d: &XmlDoc, i: usize) -> &[usize] {
    &d.el(i).children
}

fn itertext(d: &XmlDoc, i: usize, out: &mut String) {
    for it in &d.el(i).items {
        match it {
            Item::Element(c) => itertext(d, *c, out),
            other => {
                if let Some(t) = other.text() {
                    out.push_str(t);
                }
            }
        }
    }
}

/// Paragraph-direct text (text of objects inside the paragraph excluded).
fn para_text(d: &XmlDoc, p: usize) -> String {
    fn rec(d: &XmlDoc, el: usize, out: &mut String) {
        for &ch in children(d, el) {
            let n = local(d, ch);
            if n == "t" {
                itertext(d, ch, out);
            } else if matches!(n, "linesegarray" | "ctrl" | "switch" | "secPr")
                || OBJ_KINDS.contains(&n)
            {
                continue;
            } else {
                rec(d, ch, out);
            }
        }
    }
    let mut s = String::new();
    rec(d, p, &mut s);
    s
}

/// The controls rhwp's HWPX parser pushes onto `Paragraph.controls`, in order.
fn emulate_controls(d: &XmlDoc, p: usize) -> Vec<(usize, String)> {
    fn rec(d: &XmlDoc, el: usize, out: &mut Vec<(usize, String)>) {
        for &ch in children(d, el) {
            let n = local(d, ch);
            if n == "secPr" {
                out.push((ch, "secPr".into()));
                if d.descendants(ch).any(|x| local(d, x) == "colPr") {
                    out.push((ch, "colPr@secPr".into()));
                }
                continue;
            }
            if n == "ctrl" {
                for &cc in children(d, ch) {
                    let cn = local(d, cc);
                    if CTRL_CHILDREN.contains(&cn) {
                        out.push((cc, cn.to_string()));
                    }
                }
                continue;
            }
            if n == "switch" {
                if let Some(inner) = d
                    .descendants(ch)
                    .find(|&x| matches!(local(d, x), "chart" | "ole"))
                {
                    out.push((inner, local(d, inner).to_string()));
                }
                continue;
            }
            if OBJ_KINDS.contains(&n) {
                out.push((ch, n.to_string()));
                continue;
            }
            if matches!(n, "t" | "linesegarray") {
                continue;
            }
            rec(d, ch, out);
        }
    }
    let mut v = Vec::new();
    rec(d, p, &mut v);
    v
}

fn is_tracked(n: &str) -> bool {
    matches!(n, "p" | "tbl" | "tc" | "pic" | "equation") || SHAPE_KINDS.contains(&n)
}

fn rk_of(n: &str) -> RK {
    match n {
        "p" => RK::P,
        "tbl" => RK::Tbl,
        "tc" => RK::Tc,
        "pic" => RK::Pic,
        "equation" => RK::Eq,
        _ => RK::Shape,
    }
}

struct Walker<'a> {
    d: &'a XmlDoc,
    si: usize,
    recs: &'a mut Vec<Rec>,
}

impl Walker<'_> {
    fn add(&mut self, el: usize, kind: RK, addr: Option<Addr>, reason: Option<String>) -> usize {
        self.recs.push(Rec {
            si: self.si,
            el,
            kind,
            addr,
            reason,
            text: String::new(),
            rows: -1,
            cols: -1,
        });
        self.recs.len() - 1
    }

    fn unaddressed(&mut self, el: usize, reason: &str) {
        for &ch in children(self.d, el).to_vec().iter() {
            let n = local(self.d, ch).to_string();
            if n == "p" {
                self.do_para(ch, None, Some(reason.to_string()));
            } else if is_tracked(&n) {
                self.add(ch, rk_of(&n), None, Some(reason.to_string()));
                self.unaddressed(ch, reason);
            } else {
                self.unaddressed(ch, reason);
            }
        }
    }

    fn do_para(&mut self, p: usize, addr: Option<Addr>, reason: Option<String>) {
        let text = para_text(self.d, p);
        let i = self.add(p, RK::P, addr.clone(), reason.clone());
        self.recs[i].text = text;
        for (ci, (el, kind)) in emulate_controls(self.d, p).into_iter().enumerate() {
            let caddr = addr.as_ref().map(|a| {
                let mut v = a.clone();
                v.push(Seg::C(ci));
                v
            });
            if is_tracked(&kind) {
                let j = self.add(el, rk_of(&kind), caddr.clone(), reason.clone());
                if kind == "tbl" {
                    let e = self.d.el(el);
                    self.recs[j].rows = e
                        .attr_value("rowCnt")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(-1);
                    self.recs[j].cols = e
                        .attr_value("colCnt")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(-1);
                }
            } else {
                self.add(el, RK::Ctl, caddr.clone(), reason.clone());
            }
            if kind == "tbl" {
                self.do_table(el, caddr, reason.clone());
            } else if CONTAINER_CTLS.contains(&kind.as_str())
                || SHAPE_KINDS.contains(&kind.as_str())
                || SHAPE_HOSTS.contains(&kind.as_str())
            {
                self.unaddressed(el, &format!("in-{kind}"));
            }
        }
    }

    fn do_table(&mut self, tbl: usize, caddr: Option<Addr>, reason: Option<String>) {
        for &tr in children(self.d, tbl).to_vec().iter() {
            if local(self.d, tr) != "tr" {
                self.unaddressed(tr, "caption/other");
                continue;
            }
            for &tc in children(self.d, tr).to_vec().iter() {
                if local(self.d, tc) != "tc" {
                    continue;
                }
                let ca = children(self.d, tc)
                    .iter()
                    .copied()
                    .find(|&x| local(self.d, x) == "cellAddr");
                let cell_addr = match (&caddr, ca) {
                    (Some(a), Some(ca)) => {
                        let e = self.d.el(ca);
                        let r = e.attr_value("rowAddr").and_then(|v| v.parse::<u64>().ok());
                        let c = e.attr_value("colAddr").and_then(|v| v.parse::<u64>().ok());
                        match (r, c) {
                            (Some(r), Some(c)) => {
                                let mut v = a.clone();
                                v.push(Seg::Cell(r, c));
                                Some(v)
                            }
                            _ => None,
                        }
                    }
                    _ => None,
                };
                let rs = if cell_addr.is_none() {
                    Some(reason.clone().unwrap_or_else(|| "no-cell-address".into()))
                } else {
                    None
                };
                self.add(tc, RK::Tc, cell_addr.clone(), rs.clone());
                let sub = children(self.d, tc)
                    .iter()
                    .copied()
                    .find(|&x| local(self.d, x) == "subList");
                if let Some(sub) = sub {
                    let mut k = 0usize;
                    for &pp in children(self.d, sub).to_vec().iter() {
                        if local(self.d, pp) == "p" {
                            let a = cell_addr.as_ref().map(|c| {
                                let mut v = c.clone();
                                v.push(Seg::P(k));
                                v
                            });
                            self.do_para(pp, a, rs.clone());
                            k += 1;
                        }
                    }
                }
            }
        }
    }
}

/// Address used for joining: pictures inside cells lose their paragraph number.
fn join_addr(r: &Rec) -> Option<Addr> {
    let a = r.addr.as_ref()?;
    if r.kind == RK::Pic && a.iter().any(|s| matches!(s, Seg::Cell(..))) {
        let n = a.len();
        let mut v = a[..n - 2].to_vec();
        v.push(Seg::PAny);
        v.push(a[n - 1].clone());
        return Some(v);
    }
    Some(a.clone())
}

fn has_self_closing_p(d: &XmlDoc) -> bool {
    d.iter()
        .any(|(_, e)| e.local == "p" && e.end.is_none() && hae_model::is_hp(e.ns.as_deref()))
}

// ───────────────────────── rhwp side ─────────────────────────

#[derive(Debug)]
struct RItem {
    page: usize,
    kind: &'static str,
    addr: Option<Addr>,
    rows: Option<u64>,
    cols: Option<u64>,
}

fn valid(v: Option<&Value>) -> Option<u64> {
    v.and_then(Value::as_u64).filter(|&n| n != SENTINEL)
}

fn walk_render(n: &Value, base: Option<&Addr>, page: usize, sec: usize, items: &mut Vec<RItem>) {
    let t = n.get("type").and_then(Value::as_str).unwrap_or("");
    let kids: &[Value] = n
        .get("children")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let push_seg = |b: &Addr, s: Seg| {
        let mut v = b.clone();
        v.push(s);
        v
    };
    match t {
        "Page" => {
            let b = vec![Seg::S(sec)];
            for c in kids {
                walk_render(c, Some(&b), page, sec, items);
            }
        }
        "Header" | "Footer" | "MasterPage" | "FootnoteArea" => {
            for c in kids {
                walk_render(c, None, page, sec, items);
            }
        }
        "Body" => {
            let b = vec![Seg::S(sec)];
            for c in kids {
                walk_render(c, Some(&b), page, sec, items);
            }
        }
        "Column" => {
            for c in kids {
                walk_render(c, base, page, sec, items);
            }
        }
        "Table" => {
            let addr = match (base, valid(n.get("pi")), valid(n.get("ci"))) {
                (Some(b), Some(pi), Some(ci)) => {
                    let mut v = push_seg(b, Seg::P(pi as usize));
                    v.push(Seg::C(ci as usize));
                    Some(v)
                }
                _ => None,
            };
            items.push(RItem {
                page,
                kind: "tbl",
                addr: addr.clone(),
                rows: n.get("rows").and_then(Value::as_u64),
                cols: n.get("cols").and_then(Value::as_u64),
            });
            for c in kids {
                if c.get("type").and_then(Value::as_str) == Some("Cell") {
                    let cb = match (
                        &addr,
                        c.get("row").and_then(Value::as_u64),
                        c.get("col").and_then(Value::as_u64),
                    ) {
                        (Some(a), Some(r), Some(cc)) => Some(push_seg(a, Seg::Cell(r, cc))),
                        _ => None,
                    };
                    items.push(RItem {
                        page,
                        kind: "tc",
                        addr: cb.clone(),
                        rows: None,
                        cols: None,
                    });
                    for cc in c
                        .get("children")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                    {
                        walk_render(cc, cb.as_ref(), page, sec, items);
                    }
                } else {
                    walk_render(c, None, page, sec, items);
                }
            }
        }
        "TextLine" => {
            if let (Some(b), Some(pi)) = (base, valid(n.get("pi"))) {
                items.push(RItem {
                    page,
                    kind: "p",
                    addr: Some(push_seg(b, Seg::P(pi as usize))),
                    rows: None,
                    cols: None,
                });
            }
            for c in kids {
                walk_render(c, base, page, sec, items);
            }
        }
        "TextRun" => {}
        "Image" => {
            let addr = match (base, valid(n.get("pi")), valid(n.get("ci"))) {
                (Some(b), Some(pi), Some(ci)) => {
                    let seg = if b.iter().any(|s| matches!(s, Seg::Cell(..))) {
                        Seg::PAny
                    } else {
                        Seg::P(pi as usize)
                    };
                    let mut v = push_seg(b, seg);
                    v.push(Seg::C(ci as usize));
                    Some(v)
                }
                _ => None,
            };
            items.push(RItem {
                page,
                kind: "pic",
                addr,
                rows: None,
                cols: None,
            });
        }
        "Equation" | "Placeholder" | "RawSvg" => {}
        "Rect" | "Ellipse" | "Path" | "Line" | "Group" | "TextBox" | "Form" | "FnMarker" => {
            for c in kids {
                walk_render(c, None, page, sec, items);
            }
        }
        _ => {
            for c in kids {
                walk_render(c, base, page, sec, items);
            }
        }
    }
}

struct DumpObj {
    page: usize,
    kind: String,
    addr: Addr,
    ctype: Option<String>,
}

struct DumpPara {
    page: usize,
    addr: Addr,
    preview: Option<String>,
}

fn parse_dump(pages: &[Value]) -> (Vec<DumpPara>, Vec<DumpObj>, Vec<usize>) {
    let mut paras = Vec::new();
    let mut objs = Vec::new();
    let mut sections = Vec::new();
    for (idx, pg) in pages.iter().enumerate() {
        let sec = pg
            .get("section")
            .and_then(Value::as_u64)
            .unwrap_or(u64::MAX) as usize;
        sections.push(sec);
        for col in pg
            .get("columns")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            for it in col
                .get("items")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let Some(kind) = it.get("kind").and_then(Value::as_str) else {
                    continue;
                };
                let Some(pi) = valid(it.get("paraIndex")) else {
                    continue;
                };
                if kind == "fullParagraph" || kind == "partialParagraph" {
                    paras.push(DumpPara {
                        page: idx,
                        addr: vec![Seg::S(sec), Seg::P(pi as usize)],
                        preview: it
                            .get("textPreview")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                    });
                } else if let Some(ci) = valid(it.get("controlIndex")) {
                    objs.push(DumpObj {
                        page: idx,
                        kind: kind.to_string(),
                        addr: vec![Seg::S(sec), Seg::P(pi as usize), Seg::C(ci as usize)],
                        ctype: it
                            .get("control")
                            .and_then(|c| c.get("type"))
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                    });
                }
            }
        }
    }
    (paras, objs, sections)
}

fn squash(s: &str) -> Vec<char> {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

// ───────────────────────── the join ─────────────────────────

fn all_unavailable(doc: &Document, page_count: usize, reason: &str) -> PageInfo {
    let mut nodes = BTreeMap::new();
    for n in &doc.nodes {
        if n.kind != Kind::Section {
            nodes.insert(n.id.clone(), PageStatus::unavailable(reason));
        }
    }
    let unavailable = nodes.len();
    PageInfo {
        source: SOURCE,
        page_count,
        sections: doc
            .info
            .sections
            .iter()
            .enumerate()
            .map(|(index, entry)| SectionStatus {
                index,
                entry: entry.clone(),
                available: false,
                reason: Some(reason.to_string()),
            })
            .collect(),
        nodes,
        counts: PageCounts {
            unavailable,
            ..Default::default()
        },
    }
}

/// Join rhwp's per-page render trees and `dump-pages --json` output to the
/// document's nodes. `render_trees[i]` is page `i` (0-based) of the render
/// tree output; `dump` is the parsed `dump-pages --json` document.
pub fn join_pages(doc: &Document, render_trees: &[Value], dump: &Value) -> PageInfo {
    let page_count = render_trees.len();
    let Some(dump_pages) = dump.get("pages").and_then(Value::as_array) else {
        return all_unavailable(doc, page_count, "dump-pages-malformed");
    };
    if dump_pages.len() != page_count {
        return all_unavailable(doc, page_count, "page-count-mismatch");
    }
    let (dp_para, dp_obj, page_sections) = parse_dump(dump_pages);
    if page_sections.iter().any(|&s| s >= doc.info.sections.len()) {
        return all_unavailable(doc, page_count, "section-out-of-range");
    }

    // Our side.
    let mut recs: Vec<Rec> = Vec::new();
    let mut section_bad: BTreeMap<usize, String> = BTreeMap::new();
    for (si, entry) in doc.info.sections.iter().enumerate() {
        let Some(xml) = doc.xml.get(entry) else {
            section_bad.insert(si, "section-missing".into());
            continue;
        };
        let xml: &XmlDoc = xml;
        if has_self_closing_p(xml) {
            section_bad.insert(si, "skipped-element".into());
        }
        let root = xml.root();
        let mut w = Walker {
            d: xml,
            si,
            recs: &mut recs,
        };
        let mut pi = 0usize;
        for &ch in children(xml, root).to_vec().iter() {
            if local(xml, ch) == "p" {
                w.do_para(ch, Some(vec![Seg::S(si), Seg::P(pi)]), None);
                pi += 1;
            }
        }
    }

    // rhwp side.
    let mut items: Vec<RItem> = Vec::new();
    for (i, tree) in render_trees.iter().enumerate() {
        walk_render(tree, None, i, page_sections[i], &mut items);
    }

    let mut ours_by_addr: HashMap<Addr, Vec<usize>> = HashMap::new();
    for (i, r) in recs.iter().enumerate() {
        if let Some(a) = join_addr(r) {
            ours_by_addr.entry(a).or_default().push(i);
        }
    }
    // Some(Ok(i)): unique; Some(Err(())): several of ours share the address; None: none.
    let rec_of_kind = |a: &Addr, k: RK| -> Option<Result<usize, ()>> {
        let v = ours_by_addr.get(a)?;
        let m: Vec<usize> = v.iter().copied().filter(|&i| recs[i].kind == k).collect();
        match m.len() {
            0 => None,
            1 => Some(Ok(m[0])),
            _ => Some(Err(())),
        }
    };
    let sec_of = |a: &Addr| match a.first() {
        Some(Seg::S(s)) => *s,
        _ => usize::MAX,
    };
    let mut mark_bad = |si: usize, why: &str| {
        if si < doc.info.sections.len() {
            section_bad.entry(si).or_insert_with(|| why.to_string());
        }
    };

    // Pre-checks (spec §7): table dimensions, orphan addresses, object kinds, text prefixes.
    for it in &items {
        let Some(a) = &it.addr else { continue };
        let k = match it.kind {
            "tbl" => RK::Tbl,
            "tc" => RK::Tc,
            "pic" => RK::Pic,
            "p" => RK::P,
            _ => continue,
        };
        match rec_of_kind(a, k) {
            None => {
                // A paragraph line whose paragraph we cannot find, or any orphan address.
                mark_bad(sec_of(a), "orphan-address");
            }
            Some(Err(())) => {}
            Some(Ok(i)) if k == RK::Tbl => {
                let r = &recs[i];
                if it.rows.map(|v| v as i64) != Some(r.rows)
                    || it.cols.map(|v| v as i64) != Some(r.cols)
                {
                    mark_bad(r.si, "table-dimension-mismatch");
                }
            }
            Some(Ok(_)) => {}
        }
    }
    for o in &dp_obj {
        let want = match (o.kind.as_str(), o.ctype.as_deref()) {
            (_, Some("picture")) => Some(RK::Pic),
            (_, Some("equation")) => Some(RK::Eq),
            ("table" | "partialTable", _) => Some(RK::Tbl),
            _ => None,
        };
        let ok = match want {
            Some(k) => rec_of_kind(&o.addr, k).is_some(),
            None => ours_by_addr
                .get(&o.addr)
                .is_some_and(|v| v.iter().any(|&i| recs[i].kind != RK::Ctl)),
        };
        if !ok {
            mark_bad(sec_of(&o.addr), "object-kind-mismatch");
        }
    }
    for p in &dp_para {
        let Some(preview) = &p.preview else { continue };
        let Some(Ok(i)) = rec_of_kind(&p.addr, RK::P) else {
            mark_bad(sec_of(&p.addr), "orphan-address");
            continue;
        };
        let ours = squash(&recs[i].text);
        let theirs = squash(preview);
        let n = ours.len().min(theirs.len()).min(8);
        if ours[..n] != theirs[..n] {
            mark_bad(recs[i].si, "text-prefix-mismatch");
        }
    }

    // Pages by address.
    let mut rend: HashMap<(&'static str, Addr), BTreeSet<usize>> = HashMap::new();
    for it in &items {
        if let Some(a) = &it.addr {
            rend.entry((it.kind, a.clone()))
                .or_default()
                .insert(it.page);
        }
    }
    for p in &dp_para {
        rend.entry(("p", p.addr.clone()))
            .or_default()
            .insert(p.page);
    }
    let mut dp_pages: HashMap<Addr, BTreeSet<usize>> = HashMap::new();
    for o in &dp_obj {
        dp_pages.entry(o.addr.clone()).or_default().insert(o.page);
    }
    let mut host_pages: HashMap<Addr, BTreeSet<usize>> = HashMap::new();
    for it in &items {
        if let Some(a) = &it.addr {
            if a.len() >= 3 && matches!(it.kind, "tbl" | "pic") {
                host_pages
                    .entry(a[..2].to_vec())
                    .or_default()
                    .extend([it.page]);
            }
        }
    }
    for (a, pgs) in &dp_pages {
        host_pages
            .entry(a[..2].to_vec())
            .or_default()
            .extend(pgs.iter().copied());
    }

    // Resolve our nodes.
    let mut by_el: HashMap<(usize, usize), usize> = HashMap::new();
    for (i, r) in recs.iter().enumerate() {
        if matches!(
            r.kind,
            RK::P | RK::Tbl | RK::Tc | RK::Pic | RK::Eq | RK::Shape
        ) {
            by_el.entry((r.si, r.el)).or_insert(i);
        }
    }
    let mut nodes = BTreeMap::new();
    let mut counts = PageCounts::default();
    for n in &doc.nodes {
        if n.kind == Kind::Section {
            continue;
        }
        let status = (|| {
            if let Some(why) = section_bad.get(&n.section) {
                return PageStatus::unavailable(why.clone());
            }
            if !matches!(
                n.kind,
                Kind::Paragraph | Kind::Table | Kind::Cell | Kind::Picture
            ) {
                return PageStatus::unavailable("unsupported-kind");
            }
            let Some(&ri) = by_el.get(&(n.section, n.el)) else {
                return PageStatus::unavailable("no-rhwp-address");
            };
            let r = &recs[ri];
            let Some(ja) = join_addr(r) else {
                return PageStatus::unavailable(
                    r.reason.clone().unwrap_or_else(|| "no-rhwp-address".into()),
                );
            };
            if r.kind == RK::P && ja.len() != 2 {
                return PageStatus::unavailable("nested-paragraph");
            }
            if ours_by_addr.get(&ja).is_some_and(|v| v.len() > 1) {
                return PageStatus::unavailable("duplicate-address");
            }
            let mut pgs: BTreeSet<usize> = BTreeSet::new();
            let key = match r.kind {
                RK::P => "p",
                RK::Tbl => "tbl",
                RK::Tc => "tc",
                RK::Pic => "pic",
                _ => return PageStatus::unavailable("unsupported-kind"),
            };
            if let Some(s) = rend.get(&(key, ja.clone())) {
                pgs.extend(s);
            }
            if r.kind == RK::P && pgs.is_empty() {
                if let Some(h) = host_pages.get(&ja) {
                    pgs.extend(h);
                }
            }
            if matches!(r.kind, RK::Tbl | RK::Pic) {
                if let Some(d) = dp_pages.get(&ja) {
                    pgs.extend(d);
                }
            }
            if pgs.is_empty() {
                return PageStatus::unavailable("not-on-any-page");
            }
            PageStatus::pages(pgs.into_iter().map(|p| p as u32 + 1).collect())
        })();
        match (&status, n.kind) {
            (PageStatus::Pages { .. }, Kind::Paragraph) => counts.paragraphs += 1,
            (PageStatus::Pages { .. }, Kind::Table) => counts.tables += 1,
            (PageStatus::Pages { .. }, Kind::Cell) => counts.cells += 1,
            (PageStatus::Pages { .. }, Kind::Picture) => counts.pictures += 1,
            _ => counts.unavailable += 1,
        }
        nodes.insert(n.id.clone(), status);
    }

    PageInfo {
        source: SOURCE,
        page_count,
        sections: doc
            .info
            .sections
            .iter()
            .enumerate()
            .map(|(index, entry)| SectionStatus {
                index,
                entry: entry.clone(),
                available: !section_bad.contains_key(&index),
                reason: section_bad.get(&index).cloned(),
            })
            .collect(),
        nodes,
        counts,
    }
}
