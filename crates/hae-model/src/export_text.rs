//! `document.exportText`: a read-only Markdown/HTML rendering of the document.
//!
//! The converter walks the section XML once into a small block tree and then
//! prints that tree in the requested format. It converts only constructs whose
//! shape is confirmed (paragraph text, tab/line break, `hp:tbl` with
//! `cellAddr`/`cellSpan`, `hp:pic` with `hc:img`, `hp:equation/hp:script`,
//! `hp:header`/`hp:footer`, `hp:footNote`/`hp:endNote`, `hp:caption`). Every
//! other object is written as a visible `⟦미지원: name⟧` marker and counted in
//! [`ExportCounts::unsupported`]; nothing is guessed and no image is produced.

use crate::para::{ParaText, PieceKind};
use crate::{is_hp, Document, Kind, Node};
use hae_diag::{Code, EngineError, Result, Stage};
use hae_package::{sha256_hex, NS_HC, NS_HH};
use hae_xml::XmlDoc;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;

/// Output format of [`Document::export_text`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextFormat {
    Markdown,
    Html,
}

#[derive(Debug, Clone, Copy)]
pub struct ExportOptions<'a> {
    pub format: TextFormat,
    /// A section nodeId; `None` exports every section.
    pub scope: Option<&'a str>,
    pub include_notes: bool,
    /// Largest text the caller accepts (`Limits::max_text_export_bytes`).
    pub max_bytes: u64,
}

/// What was converted. `paragraphs` counts every non-blank paragraph written
/// anywhere (body, cells, notes, headers/footers, captions).
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExportCounts {
    pub paragraphs: usize,
    pub tables: usize,
    pub pictures: usize,
    pub equations: usize,
    pub footnotes: usize,
    pub endnotes: usize,
    pub headers: usize,
    pub footers: usize,
    /// Tables that contain at least one merged cell.
    pub merged_tables: usize,
    /// Markdown only: paragraphs whose centre/right alignment was dropped.
    pub alignment_ignored: usize,
    /// Footnotes and endnotes left out because `includeNotes` was false.
    pub notes_omitted: usize,
    /// Objects written as `⟦미지원: name⟧`, by element name.
    pub unsupported: BTreeMap<String, usize>,
    /// Content that sits inside those unsupported objects and was not exported.
    pub inside_unsupported: InsideUnsupported,
    /// Non-text markers skipped silently (page number, field boundary, ...).
    pub ignored_markers: BTreeMap<String, usize>,
}

/// Elements found below an unsupported object (text box, group, refused table).
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InsideUnsupported {
    pub paragraphs: usize,
    pub tables: usize,
    pub pictures: usize,
    pub equations: usize,
}

#[derive(Debug, Clone)]
pub struct ExportedText {
    pub text: String,
    pub counts: ExportCounts,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Align {
    Center,
    Right,
}

impl Align {
    fn css(self) -> &'static str {
        match self {
            Align::Center => "center",
            Align::Right => "right",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Align::Center => "가운데",
            Align::Right => "오른쪽",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NoteKind {
    Foot,
    End,
}

impl NoteKind {
    fn key(self, n: usize) -> String {
        match self {
            NoteKind::Foot => format!("fn{n}"),
            NoteKind::End => format!("en{n}"),
        }
    }
    fn label(self) -> &'static str {
        match self {
            NoteKind::Foot => "각주",
            NoteKind::End => "미주",
        }
    }
}

enum Inline {
    Text(String),
    Tab,
    Break,
    Equation(String),
    Note(NoteKind, usize),
    Unsupported(String),
}

enum Block {
    Para {
        inlines: Vec<Inline>,
        align: Option<Align>,
    },
    Table(Table),
    Picture(Picture),
}

struct Table {
    index: usize,
    node_id: Option<String>,
    rows: usize,
    cols: usize,
    merged: bool,
    cells: Vec<Cell>,
    caption: Vec<Block>,
}

struct Cell {
    row: usize,
    col: usize,
    row_span: usize,
    col_span: usize,
    blocks: Vec<Block>,
}

struct Media {
    entry: String,
    file: String,
    sha256: String,
}

struct Picture {
    node_id: Option<String>,
    media: Option<Media>,
    caption: Vec<Block>,
}

struct Note {
    kind: NoteKind,
    n: usize,
    blocks: Vec<Block>,
}

struct HeaderFooter {
    header: bool,
    section: usize,
    apply: Option<String>,
    blocks: Vec<Block>,
}

/// Markers inside `hp:ctrl` / a run that carry no text of their own.
const IGNORED_MARKERS: &[&str] = &[
    "colPr",
    "fieldBegin",
    "fieldEnd",
    "pageNum",
    "pageNumCtrl",
    "newNum",
    "autoNum",
    "pageHiding",
    "bookmark",
    "indexmark",
];

/// A table with more cells than this is refused rather than allocated.
const MAX_GRID_CELLS: u64 = 4_000_000;

struct Walker<'a> {
    d: &'a Document,
    include_notes: bool,
    align: HashMap<String, Align>,
    nodes: HashMap<(&'a str, usize), &'a Node>,
    sha_cache: HashMap<String, String>,
    counts: ExportCounts,
    notes: Vec<Note>,
    headers: Vec<HeaderFooter>,
    table_seq: usize,
    foot_seq: usize,
    end_seq: usize,
    section: usize,
    merged: Vec<usize>,
}

fn children_named<'x>(
    doc: &'x XmlDoc,
    el: usize,
    local: &'x str,
) -> impl Iterator<Item = usize> + 'x {
    doc.el(el)
        .children
        .iter()
        .copied()
        .filter(move |&c| is_hp(doc.el(c).ns.as_deref()) && doc.el(c).local == local)
}

fn first_child(doc: &XmlDoc, el: usize, local: &str) -> Option<usize> {
    children_named(doc, el, local).next()
}

fn push_text(out: &mut Vec<Inline>, s: &str) {
    let mut buf = String::new();
    let flush = |buf: &mut String, out: &mut Vec<Inline>| {
        if !buf.is_empty() {
            out.push(Inline::Text(std::mem::take(buf)));
        }
    };
    for ch in s.chars() {
        match ch {
            '\t' => {
                flush(&mut buf, out);
                out.push(Inline::Tab);
            }
            '\n' => {
                flush(&mut buf, out);
                out.push(Inline::Break);
            }
            '\r' => {}
            c => buf.push(c),
        }
    }
    flush(&mut buf, out);
}

fn is_blank(inlines: &[Inline]) -> bool {
    inlines.iter().all(|i| match i {
        Inline::Text(t) => t.trim().is_empty(),
        Inline::Tab | Inline::Break => true,
        _ => false,
    })
}

impl<'a> Walker<'a> {
    fn new(d: &'a Document, include_notes: bool) -> Self {
        let mut align = HashMap::new();
        if let Some(h) = d.header() {
            for (i, e) in h.iter() {
                if !e.is(NS_HH, "paraPr") {
                    continue;
                }
                let Some(id) = e.attr_value("id") else {
                    continue;
                };
                let horizontal = h
                    .descendants(i)
                    .map(|j| h.el(j))
                    .find(|j| j.is(NS_HH, "align"))
                    .and_then(|j| j.attr_value("horizontal"));
                match horizontal {
                    Some("CENTER") => align.insert(id.to_string(), Align::Center),
                    Some("RIGHT") => align.insert(id.to_string(), Align::Right),
                    _ => None,
                };
            }
        }
        Walker {
            d,
            include_notes,
            align,
            nodes: d
                .nodes
                .iter()
                .map(|n| ((n.entry.as_str(), n.el), n))
                .collect(),
            sha_cache: HashMap::new(),
            counts: ExportCounts::default(),
            notes: Vec::new(),
            headers: Vec::new(),
            table_seq: 0,
            foot_seq: 0,
            end_seq: 0,
            section: 0,
            merged: Vec::new(),
        }
    }

    fn unsupported(&mut self, name: &str) -> Inline {
        *self.counts.unsupported.entry(name.to_string()).or_default() += 1;
        Inline::Unsupported(name.to_string())
    }

    fn ignored(&mut self, name: &str) {
        *self
            .counts
            .ignored_markers
            .entry(name.to_string())
            .or_default() += 1;
    }

    /// Unsupported object: a visible marker, plus how much content it hides.
    fn unsupported_object(&mut self, entry: &'a str, el: usize, name: &str) -> Inline {
        let doc: &'a XmlDoc = &self.d.xml[entry];
        for j in doc.descendants(el) {
            let e = doc.el(j);
            if !is_hp(e.ns.as_deref()) {
                continue;
            }
            let inside = &mut self.counts.inside_unsupported;
            match e.local.as_str() {
                "p" => {
                    let pt = ParaText::build(doc, j);
                    if !pt.text.replace('\u{FFFC}', "").trim().is_empty() {
                        inside.paragraphs += 1;
                    }
                }
                "tbl" => inside.tables += 1,
                "pic" => inside.pictures += 1,
                "equation" => inside.equations += 1,
                _ => {}
            }
        }
        self.unsupported(name)
    }

    /// Everything directly below a section root, `hp:subList`, `hp:tc` or a
    /// note/header/caption owner. Paragraphs and the objects the document
    /// model also lists as nodes (`tbl`, `pic`, `equation`) are converted;
    /// layout-only children are skipped; anything else becomes a marker so
    /// nothing disappears silently.
    fn container_blocks(&mut self, entry: &'a str, parent: usize) -> Vec<Block> {
        let doc: &'a XmlDoc = &self.d.xml[entry];
        let mut out = Vec::new();
        for c in doc.el(parent).children.clone() {
            let ce = doc.el(c);
            let name = ce.local.clone();
            if is_hp(ce.ns.as_deref()) {
                match name.as_str() {
                    "p" => out.extend(self.para_blocks(entry, c)),
                    "subList" => out.extend(self.container_blocks(entry, c)),
                    "tbl" | "pic" | "equation" => {
                        let mut cur = Vec::new();
                        self.object(entry, c, &name, None, &mut cur, &mut out);
                        self.flush(&mut cur, None, &mut out);
                    }
                    "cellAddr" | "cellSpan" | "cellSz" | "cellMargin" | "linesegarray" => {}
                    _ => {
                        let u = self.unsupported_object(entry, c, &name);
                        out.push(Block::Para {
                            inlines: vec![u],
                            align: None,
                        });
                    }
                }
            } else {
                let u = self.unsupported_object(entry, c, &name);
                out.push(Block::Para {
                    inlines: vec![u],
                    align: None,
                });
            }
        }
        out
    }

    fn para_blocks(&mut self, entry: &'a str, p: usize) -> Vec<Block> {
        let doc: &'a XmlDoc = &self.d.xml[entry];
        let align = doc
            .el(p)
            .attr_value("paraPrIDRef")
            .and_then(|id| self.align.get(id))
            .copied();
        let pt = ParaText::build(doc, p);
        let chars: Vec<char> = pt.text.chars().collect();
        let mut blocks = Vec::new();
        let mut cur: Vec<Inline> = Vec::new();
        for piece in &pt.pieces {
            let s: String = chars[piece.cp_start..piece.cp_start + piece.cp_len]
                .iter()
                .collect();
            match &piece.kind {
                PieceKind::Text { .. } | PieceKind::Ref { .. } | PieceKind::CData { .. } => {
                    push_text(&mut cur, &s)
                }
                PieceKind::Control { name, .. } if piece.t.is_some() && name == "tab" => {
                    cur.push(Inline::Tab)
                }
                PieceKind::Control { name, .. } if piece.t.is_some() && name == "lineBreak" => {
                    cur.push(Inline::Break)
                }
                PieceKind::Control { el, name } if piece.t.is_none() => {
                    self.object(entry, *el, name, align, &mut cur, &mut blocks);
                }
                PieceKind::Control { name, .. } => {
                    let u = self.unsupported(name);
                    cur.push(u);
                }
                PieceKind::Other { name, .. } => match name.as_str() {
                    "fwSpace" | "nbSpace" | "hyphen" => push_text(&mut cur, &s),
                    _ => {
                        let u = self.unsupported(name);
                        cur.push(u);
                    }
                },
            }
        }
        self.flush(&mut cur, align, &mut blocks);
        blocks
    }

    fn flush(&mut self, cur: &mut Vec<Inline>, align: Option<Align>, blocks: &mut Vec<Block>) {
        let inlines = std::mem::take(cur);
        if inlines.is_empty() || is_blank(&inlines) {
            return;
        }
        self.counts.paragraphs += 1;
        blocks.push(Block::Para { inlines, align });
    }

    fn object(
        &mut self,
        entry: &'a str,
        el: usize,
        name: &str,
        align: Option<Align>,
        cur: &mut Vec<Inline>,
        blocks: &mut Vec<Block>,
    ) {
        let doc: &'a XmlDoc = &self.d.xml[entry];
        if IGNORED_MARKERS.contains(&name) {
            self.ignored(name);
            return;
        }
        if !is_hp(doc.el(el).ns.as_deref()) {
            let u = self.unsupported_object(entry, el, name);
            cur.push(u);
            return;
        }
        match name {
            "tbl" => {
                self.flush(cur, align, blocks);
                match self.table(entry, el) {
                    Some(t) => blocks.push(Block::Table(t)),
                    None => {
                        let u = self.unsupported_object(entry, el, "tbl");
                        blocks.push(Block::Para {
                            inlines: vec![u],
                            align: None,
                        });
                    }
                }
            }
            "pic" => {
                self.flush(cur, align, blocks);
                let pic = self.picture(entry, el);
                blocks.push(Block::Picture(pic));
            }
            "equation" => {
                let script = first_child(doc, el, "script")
                    .or_else(|| doc.descendants(el).find(|&j| doc.el(j).local == "script"));
                match script {
                    Some(s) => {
                        self.counts.equations += 1;
                        cur.push(Inline::Equation(doc.direct_text(s)));
                    }
                    None => {
                        let u = self.unsupported_object(entry, el, "equation");
                        cur.push(u);
                    }
                }
            }
            "ctrl" => {
                for c in doc.el(el).children.clone() {
                    self.ctrl_child(entry, c, cur);
                }
            }
            other => {
                let u = self.unsupported_object(entry, el, other);
                cur.push(u);
            }
        }
    }

    fn ctrl_child(&mut self, entry: &'a str, c: usize, cur: &mut Vec<Inline>) {
        let doc: &'a XmlDoc = &self.d.xml[entry];
        let ce = doc.el(c);
        let name = ce.local.clone();
        if is_hp(ce.ns.as_deref()) {
            if IGNORED_MARKERS.contains(&name.as_str()) {
                self.ignored(&name);
                return;
            }
            match name.as_str() {
                "header" | "footer" => {
                    let header = name == "header";
                    let apply = ce.attr_value("applyPageType").map(str::to_string);
                    if header {
                        self.counts.headers += 1;
                    } else {
                        self.counts.footers += 1;
                    }
                    let blocks = self.container_blocks(entry, c);
                    self.headers.push(HeaderFooter {
                        header,
                        section: self.section,
                        apply,
                        blocks,
                    });
                    return;
                }
                "footNote" | "endNote" => {
                    if !self.include_notes {
                        self.counts.notes_omitted += 1;
                        return;
                    }
                    let (kind, n) = if name == "footNote" {
                        self.foot_seq += 1;
                        self.counts.footnotes += 1;
                        (NoteKind::Foot, self.foot_seq)
                    } else {
                        self.end_seq += 1;
                        self.counts.endnotes += 1;
                        (NoteKind::End, self.end_seq)
                    };
                    // Reserve the slot first so nested notes keep document order.
                    let slot = self.notes.len();
                    self.notes.push(Note {
                        kind,
                        n,
                        blocks: Vec::new(),
                    });
                    let blocks = self.container_blocks(entry, c);
                    self.notes[slot].blocks = blocks;
                    cur.push(Inline::Note(kind, n));
                    return;
                }
                _ => {}
            }
        }
        let u = self.unsupported_object(entry, c, &name);
        cur.push(u);
    }

    fn media_of(&mut self, entry: &str, pic: usize) -> Option<Media> {
        let doc: &XmlDoc = &self.d.xml[entry];
        let img = doc.descendants(pic).find(|&d| {
            let e = doc.el(d);
            e.local == "img" && e.ns.as_deref() == Some(NS_HC)
        })?;
        let item_id = doc.el(img).attr_value("binaryItemIDRef")?;
        let item = self.d.info.item_by_id(item_id)?;
        let payload = self.d.payload(&item.entry)?;
        let sha256 = self
            .sha_cache
            .entry(item.entry.clone())
            .or_insert_with(|| sha256_hex(payload))
            .clone();
        let file = item
            .entry
            .rsplit('/')
            .next()
            .unwrap_or(&item.entry)
            .to_string();
        Some(Media {
            entry: item.entry.clone(),
            file,
            sha256,
        })
    }

    fn caption(&mut self, entry: &'a str, owner: usize) -> Vec<Block> {
        let doc: &'a XmlDoc = &self.d.xml[entry];
        match first_child(doc, owner, "caption") {
            Some(c) => self.container_blocks(entry, c),
            None => Vec::new(),
        }
    }

    fn picture(&mut self, entry: &'a str, pic: usize) -> Picture {
        self.counts.pictures += 1;
        let media = self.media_of(entry, pic);
        if media.is_none() {
            *self
                .counts
                .unsupported
                .entry("pic(미디어 참조 없음)".to_string())
                .or_default() += 1;
        }
        Picture {
            node_id: self.nodes.get(&(entry, pic)).map(|n| n.id.clone()),
            media,
            caption: self.caption(entry, pic),
        }
    }

    fn table(&mut self, entry: &'a str, tbl: usize) -> Option<Table> {
        let doc: &'a XmlDoc = &self.d.xml[entry];
        self.table_seq += 1;
        let index = self.table_seq;
        // (cell element, row, col, rowSpan, colSpan)
        let mut raw: Vec<(usize, u32, u32, u32, u32)> = Vec::new();
        let mut unaddressed = Vec::new();
        for (r, tr) in children_named(doc, tbl, "tr")
            .collect::<Vec<_>>()
            .into_iter()
            .enumerate()
        {
            for (c, tc) in children_named(doc, tr, "tc")
                .collect::<Vec<_>>()
                .into_iter()
                .enumerate()
            {
                let node = self.nodes.get(&(entry, tc))?;
                match self.d.cell_addr(node) {
                    Some(a) => raw.push((tc, a.row, a.col, a.row_span, a.col_span)),
                    None => unaddressed.push((tc, r as u32, c as u32)),
                }
            }
        }
        if !unaddressed.is_empty() {
            // Without cellAddr the position is only unambiguous when no cell
            // is addressed or spans anything: then row/col is the XML order.
            let spans = unaddressed
                .iter()
                .any(|&(tc, ..)| first_child(doc, tc, "cellSpan").is_some());
            if !raw.is_empty() || spans {
                return None;
            }
            raw = unaddressed
                .into_iter()
                .map(|(tc, r, c)| (tc, r, c, 1, 1))
                .collect();
        }
        if raw.iter().any(|&(_, _, _, rs, cs)| rs == 0 || cs == 0) {
            return None;
        }
        if raw.is_empty() {
            return None;
        }
        let rows = raw
            .iter()
            .map(|&(_, r, _, rs, _)| u64::from(r) + u64::from(rs))
            .max()?;
        let cols = raw
            .iter()
            .map(|&(_, _, c, _, cs)| u64::from(c) + u64::from(cs))
            .max()?;
        if rows.checked_mul(cols)? > MAX_GRID_CELLS {
            return None;
        }
        let (rows, cols) = (rows as usize, cols as usize);
        let mut taken = vec![false; rows * cols];
        for &(_, row, col, rs, cs) in &raw {
            for r in row..row + rs {
                for c in col..col + cs {
                    let slot = &mut taken[r as usize * cols + c as usize];
                    if *slot {
                        return None;
                    }
                    *slot = true;
                }
            }
        }
        let mut merged = false;
        let mut cells = Vec::new();
        for (tc, row, col, rs, cs) in raw {
            merged |= rs > 1 || cs > 1;
            cells.push(Cell {
                row: row as usize,
                col: col as usize,
                row_span: rs as usize,
                col_span: cs as usize,
                blocks: self.container_blocks(entry, tc),
            });
        }
        self.counts.tables += 1;
        if merged {
            self.counts.merged_tables += 1;
            self.merged.push(index);
        }
        Some(Table {
            index,
            node_id: self.nodes.get(&(entry, tbl)).map(|n| n.id.clone()),
            rows,
            cols,
            merged,
            cells,
            caption: self.caption(entry, tbl),
        })
    }
}

// ---------------------------------------------------------------------------
// Escaping
// ---------------------------------------------------------------------------

fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// Backslash-escape every character that can start Markdown or HTML syntax
/// inside running text. Line-start constructs are handled by [`md_line_starts`].
fn md_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if matches!(
            ch,
            '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '&' | '|' | '~' | '$'
        ) {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// Neutralize block syntax at the start of each line of one paragraph:
/// indented code, headings, lists, thematic breaks, setext underlines.
fn md_line_starts(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for (i, line) in s.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let lead = line.chars().take_while(|&c| c == ' ').count();
        if lead > 0 {
            for _ in 0..lead {
                out.push_str("&nbsp;");
            }
            out.push_str(&line[lead..]);
            continue;
        }
        let mut chars = line.chars();
        match chars.next() {
            Some(c @ ('+' | '-' | '=' | '#')) => {
                out.push('\\');
                out.push(c);
                out.push_str(chars.as_str());
            }
            Some(c) if c.is_ascii_digit() => {
                let digits = line.chars().take_while(char::is_ascii_digit).count();
                out.push_str(&line[..digits]);
                let rest = &line[digits..];
                if rest.starts_with(['.', ')']) {
                    out.push('\\');
                }
                out.push_str(rest);
            }
            _ => out.push_str(line),
        }
    }
    out
}

/// Raw text as code. A backtick span when it is safe, otherwise `<code>`.
fn md_code(s: &str, in_cell: bool) -> String {
    if s.contains(['\n', '\r']) || (in_cell && s.contains('|')) {
        return html_code(&s.replace("\r\n", "\n").replace('\r', "\n"), "");
    }
    let longest = s
        .split(|c| c != '`')
        .map(str::len)
        .max()
        .unwrap_or_default();
    let fence = "`".repeat(longest + 1);
    let pad = s.starts_with('`')
        || s.ends_with('`')
        || (s.starts_with(' ') && s.ends_with(' ') && !s.trim_matches(' ').is_empty());
    if pad {
        format!("{fence} {s} {fence}")
    } else {
        format!("{fence}{s}{fence}")
    }
}

fn html_code(s: &str, class: &str) -> String {
    let class = if class.is_empty() {
        String::new()
    } else {
        format!(" class=\"{class}\"")
    };
    // `|` is escaped numerically so a Markdown table row cannot split on it.
    format!(
        "<code{class}>{}</code>",
        html_escape(s).replace('|', "&#124;").replace('\n', "<br>")
    )
}

// ---------------------------------------------------------------------------
// Printing
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Body,
    Note,
    Cell,
}

struct Printer<'c> {
    html: bool,
    counts: &'c mut ExportCounts,
}

fn flat_text(blocks: &[Block], out: &mut Vec<String>) {
    for b in blocks {
        match b {
            Block::Para { inlines, .. } => out.push(
                inlines
                    .iter()
                    .map(|i| match i {
                        Inline::Text(t) => t.clone(),
                        Inline::Tab | Inline::Break => " ".into(),
                        Inline::Equation(s) => s.clone(),
                        _ => String::new(),
                    })
                    .collect(),
            ),
            Block::Table(t) => t.cells.iter().for_each(|c| flat_text(&c.blocks, out)),
            Block::Picture(p) => flat_text(&p.caption, out),
        }
    }
}

impl Printer<'_> {
    // ---- Markdown ----

    fn md_inlines(&self, inl: &[Inline], in_cell: bool) -> String {
        let end = inl
            .iter()
            .rposition(|i| !matches!(i, Inline::Break))
            .map_or(0, |p| p + 1);
        let mut s = String::new();
        for i in &inl[..end] {
            match i {
                Inline::Text(t) => s.push_str(&md_escape(t)),
                Inline::Tab => s.push_str("&emsp;"),
                Inline::Break => s.push_str(if in_cell { "<br>" } else { "\\\n" }),
                Inline::Equation(script) => s.push_str(&md_code(script, in_cell)),
                Inline::Note(k, n) => {
                    let _ = write!(s, "[^{}]", k.key(*n));
                }
                Inline::Unsupported(name) => {
                    let _ = write!(s, "⟦미지원: {}⟧", md_escape(name));
                }
            }
        }
        s
    }

    /// Caption text on one line.
    fn md_flat(&self, blocks: &[Block]) -> String {
        let mut parts = Vec::new();
        for b in blocks {
            if let Block::Para { inlines, .. } = b {
                parts.push(self.md_inlines(inlines, true));
            } else {
                let mut t = Vec::new();
                flat_text(std::slice::from_ref(b), &mut t);
                parts.push(md_escape(&t.join(" ")));
            }
        }
        parts.join(" ")
    }

    fn md_picture_line(&mut self, p: &Picture, in_cell: bool) -> String {
        match &p.media {
            Some(m) => format!(
                "[그림] {} (미디어 항목 {}, SHA-256 {})",
                md_code(&m.file, in_cell),
                md_code(&m.entry, in_cell),
                md_code(&m.sha256, in_cell)
            ),
            None => "[그림] 미디어 참조를 확인하지 못했습니다.".to_string(),
        }
    }

    fn md_blocks(&mut self, blocks: &[Block], mode: Mode) -> String {
        let mut parts: Vec<String> = Vec::new();
        for b in blocks {
            match b {
                Block::Para { inlines, align } => {
                    let mut text = self.md_inlines(inlines, mode == Mode::Cell);
                    if mode != Mode::Cell {
                        text = md_line_starts(&text);
                    }
                    if let Some(a) = align {
                        self.counts.alignment_ignored += 1;
                        if mode == Mode::Body {
                            text =
                                format!("<!-- hwpx: 문단 정렬({}) 무시됨 -->\n{text}", a.label());
                        }
                    }
                    parts.push(text);
                }
                Block::Picture(p) => {
                    let mut text = self.md_picture_line(p, mode == Mode::Cell);
                    if !p.caption.is_empty() {
                        let cap = format!("그림 캡션: {}", self.md_flat(&p.caption));
                        text = if mode == Mode::Cell {
                            format!("{text}<br>{cap}")
                        } else {
                            format!("{text}\n\n{cap}")
                        };
                    }
                    parts.push(text);
                }
                Block::Table(t) if mode == Mode::Cell => {
                    *self
                        .counts
                        .unsupported
                        .entry("nestedTable".to_string())
                        .or_default() += 1;
                    let mut flat = Vec::new();
                    flat_text(std::slice::from_ref(b), &mut flat);
                    flat.retain(|s| !s.trim().is_empty());
                    parts.push(format!(
                        "⟦중첩 표 {}×{} 변환 불가: {}⟧",
                        t.rows,
                        t.cols,
                        md_escape(&flat.join(" | "))
                    ));
                }
                Block::Table(t) => parts.push(self.md_table(t)),
            }
        }
        parts.join(if mode == Mode::Cell { "<br>" } else { "\n\n" })
    }

    fn md_table(&mut self, t: &Table) -> String {
        let mut grid = vec![vec![String::new(); t.cols]; t.rows];
        for c in &t.cells {
            grid[c.row][c.col] = self.md_blocks(&c.blocks, Mode::Cell);
        }
        let mut out = String::new();
        if !t.caption.is_empty() {
            let _ = write!(out, "표 캡션: {}\n\n", self.md_flat(&t.caption));
        }
        if t.merged {
            let id = t
                .node_id
                .as_deref()
                .map(|id| format!(", nodeId {id}"))
                .unwrap_or_default();
            let _ = writeln!(
                out,
                "<!-- hwpx: 표 {} 병합 셀 포함, 병합된 칸은 비워 둠{id} -->",
                t.index
            );
        }
        out.push('|');
        out.push_str(&"   |".repeat(t.cols));
        out.push('\n');
        out.push('|');
        out.push_str(&" --- |".repeat(t.cols));
        for row in &grid {
            out.push_str("\n| ");
            out.push_str(&row.join(" | "));
            out.push_str(" |");
        }
        out
    }

    fn md_notes(&mut self, notes: &[Note]) -> String {
        let mut out = String::new();
        for kind in [NoteKind::Foot, NoteKind::End] {
            let group: Vec<&Note> = notes.iter().filter(|n| n.kind == kind).collect();
            if group.is_empty() {
                continue;
            }
            let _ = write!(out, "## {}\n\n", kind.label());
            for n in group {
                let body = self.md_blocks(&n.blocks, Mode::Note);
                let mut lines = body.split('\n');
                let _ = writeln!(out, "[^{}]: {}", kind.key(n.n), lines.next().unwrap_or(""));
                for l in lines {
                    if l.is_empty() {
                        out.push('\n');
                    } else {
                        let _ = writeln!(out, "    {l}");
                    }
                }
                out.push('\n');
            }
        }
        out
    }

    // ---- HTML ----

    fn html_inlines(&self, inl: &[Inline]) -> String {
        let end = inl
            .iter()
            .rposition(|i| !matches!(i, Inline::Break))
            .map_or(0, |p| p + 1);
        let mut s = String::new();
        for i in &inl[..end] {
            match i {
                Inline::Text(t) => s.push_str(&html_escape(t)),
                Inline::Tab => s.push_str("&emsp;"),
                Inline::Break => s.push_str("<br>"),
                Inline::Equation(script) => {
                    s.push_str(&html_code(&script.replace("\r\n", "\n"), "hwpx-equation"))
                }
                Inline::Note(k, n) => {
                    let key = k.key(*n);
                    let _ = write!(
                        s,
                        "<sup class=\"hwpx-noteref\"><a id=\"ref-{key}\" href=\"#{key}\">[{}{n}]</a></sup>",
                        k.label()
                    );
                }
                Inline::Unsupported(name) => {
                    let _ = write!(
                        s,
                        "<span class=\"hwpx-unsupported\">⟦미지원: {}⟧</span>",
                        html_escape(name)
                    );
                }
            }
        }
        s
    }

    fn html_flat(&self, blocks: &[Block]) -> String {
        let mut parts = Vec::new();
        for b in blocks {
            if let Block::Para { inlines, .. } = b {
                parts.push(self.html_inlines(inlines));
            } else {
                let mut t = Vec::new();
                flat_text(std::slice::from_ref(b), &mut t);
                parts.push(html_escape(&t.join(" ")));
            }
        }
        parts.join(" ")
    }

    fn html_blocks(&mut self, blocks: &[Block]) -> String {
        let mut out = String::new();
        for b in blocks {
            match b {
                Block::Para { inlines, align } => {
                    let style = align
                        .map(|a| format!(" style=\"text-align:{}\"", a.css()))
                        .unwrap_or_default();
                    let _ = writeln!(out, "<p{style}>{}</p>", self.html_inlines(inlines));
                }
                Block::Picture(p) => {
                    let attrs = match (&p.media, &p.node_id) {
                        (Some(m), id) => format!(
                            " data-media-entry=\"{}\" data-sha256=\"{}\"{}",
                            html_escape(&m.entry),
                            m.sha256,
                            id.as_deref()
                                .map(|i| format!(" data-node-id=\"{}\"", html_escape(i)))
                                .unwrap_or_default()
                        ),
                        (None, _) => String::new(),
                    };
                    let line = match &p.media {
                        Some(m) => format!(
                            "[그림] {} (미디어 항목 {}, SHA-256 {})",
                            html_code(&m.file, ""),
                            html_code(&m.entry, ""),
                            html_code(&m.sha256, "")
                        ),
                        None => "[그림] 미디어 참조를 확인하지 못했습니다.".to_string(),
                    };
                    let _ = writeln!(out, "<figure class=\"hwpx-picture\"{attrs}>");
                    let _ = writeln!(out, "<p>{line}</p>");
                    if !p.caption.is_empty() {
                        let _ = writeln!(
                            out,
                            "<figcaption>{}</figcaption>",
                            self.html_flat(&p.caption)
                        );
                    }
                    out.push_str("</figure>\n");
                }
                Block::Table(t) => out.push_str(&self.html_table(t)),
            }
        }
        out
    }

    fn html_table(&mut self, t: &Table) -> String {
        let id = t
            .node_id
            .as_deref()
            .map(|i| format!(" data-node-id=\"{}\"", html_escape(i)))
            .unwrap_or_default();
        let mut out = format!("<table class=\"hwpx-table\"{id}>\n");
        if !t.caption.is_empty() {
            let _ = writeln!(out, "<caption>{}</caption>", self.html_flat(&t.caption));
        }
        for r in 0..t.rows {
            out.push_str("<tr>");
            let mut cells: Vec<&Cell> = t.cells.iter().filter(|c| c.row == r).collect();
            cells.sort_by_key(|c| c.col);
            for c in cells {
                let mut attrs = String::new();
                if c.col_span > 1 {
                    let _ = write!(attrs, " colspan=\"{}\"", c.col_span);
                }
                if c.row_span > 1 {
                    let _ = write!(attrs, " rowspan=\"{}\"", c.row_span);
                }
                let inner = self.html_blocks(&c.blocks);
                let _ = write!(out, "<td{attrs}>{}</td>", inner.trim_end());
            }
            out.push_str("</tr>\n");
        }
        out.push_str("</table>\n");
        out
    }

    fn html_notes(&mut self, notes: &[Note]) -> String {
        let mut out = String::new();
        for kind in [NoteKind::Foot, NoteKind::End] {
            let group: Vec<&Note> = notes.iter().filter(|n| n.kind == kind).collect();
            if group.is_empty() {
                continue;
            }
            let _ = writeln!(
                out,
                "<section class=\"hwpx-notes\">\n<h2>{}</h2>\n<ol>",
                kind.label()
            );
            for n in group {
                let key = kind.key(n.n);
                let body = self.html_blocks(&n.blocks);
                let _ = writeln!(
                    out,
                    "<li id=\"{key}\" value=\"{}\">{} <a href=\"#ref-{key}\">↩</a></li>",
                    n.n,
                    body.trim_end()
                );
            }
            out.push_str("</ol>\n</section>\n");
        }
        out
    }
}

fn too_large(limit: u64, produced: usize) -> EngineError {
    EngineError::new(
        Code::ResourceLimit,
        Stage::Export,
        format!(
            "text export is {produced} bytes, over the limit max_text_export_bytes = {limit} bytes"
        ),
    )
    .with_reason("text_export_too_large")
    .with_detail(serde_json::json!({
        "limit": "max_text_export_bytes",
        "limitBytes": limit,
        "producedBytes": produced,
    }))
}

impl Document {
    /// Render the document (or one section) as Markdown or HTML. Pure: the
    /// document, its revision and its package state are untouched.
    pub fn export_text(&self, opts: &ExportOptions<'_>) -> Result<ExportedText> {
        let sections: Vec<&Node> = match opts.scope {
            Some(id) => vec![self.require(id, Some(Kind::Section))?],
            None => self
                .nodes
                .iter()
                .filter(|n| n.kind == Kind::Section)
                .collect(),
        };
        let multi = self
            .nodes
            .iter()
            .filter(|n| n.kind == Kind::Section)
            .count()
            > 1;
        let html = opts.format == TextFormat::Html;
        let over = |len: usize| len as u64 > opts.max_bytes;

        let mut walker = Walker::new(self, opts.include_notes);
        let mut parsed = Vec::new();
        for s in &sections {
            walker.section = s.section;
            let blocks = walker.container_blocks(&s.entry, s.el);
            parsed.push((s, blocks));
        }
        let Walker {
            mut counts,
            notes,
            headers,
            mut merged,
            ..
        } = walker;
        merged.sort_unstable();
        let mut p = Printer {
            html,
            counts: &mut counts,
        };

        let mut body = String::new();
        for (s, blocks) in &parsed {
            if blocks.is_empty() {
                continue;
            }
            let text = if p.html {
                p.html_blocks(blocks)
            } else {
                p.md_blocks(blocks, Mode::Body)
            };
            if p.html {
                if multi {
                    let _ = writeln!(
                        body,
                        "<section class=\"hwpx-section\" data-section=\"{}\" data-node-id=\"{}\">",
                        s.section + 1,
                        html_escape(&s.id)
                    );
                }
                body.push_str(&text);
                if multi {
                    body.push_str("</section>\n");
                }
            } else {
                if multi {
                    let _ = write!(
                        body,
                        "<!-- hwpx: 구역 {} nodeId {} -->\n\n",
                        s.section + 1,
                        s.id
                    );
                }
                body.push_str(&text);
                body.push_str("\n\n");
            }
            if over(body.len()) {
                return Err(too_large(opts.max_bytes, body.len()));
            }
        }

        let mut head = String::new();
        if !headers.is_empty() {
            if p.html {
                head.push_str("<section class=\"hwpx-headers\">\n<h2>머리말·꼬리말</h2>\n");
            } else {
                head.push_str("## 머리말·꼬리말\n\n");
            }
            let (mut hn, mut fnn) = (0, 0);
            for h in &headers {
                let n = if h.header {
                    hn += 1;
                    hn
                } else {
                    fnn += 1;
                    fnn
                };
                let kind = if h.header { "머리말" } else { "꼬리말" };
                let apply = h
                    .apply
                    .as_deref()
                    .map(|a| format!(", 적용 쪽: {a}"))
                    .unwrap_or_default();
                if p.html {
                    let _ = writeln!(
                        head,
                        "<h3>{kind} {n} (구역 {}{})</h3>",
                        h.section + 1,
                        html_escape(&apply)
                    );
                    head.push_str(&p.html_blocks(&h.blocks));
                } else {
                    let _ = write!(
                        head,
                        "### {kind} {n} (구역 {}{})\n\n",
                        h.section + 1,
                        md_escape(&apply)
                    );
                    let text = p.md_blocks(&h.blocks, Mode::Body);
                    if !text.is_empty() {
                        head.push_str(&text);
                        head.push_str("\n\n");
                    }
                }
            }
            if p.html {
                head.push_str("</section>\n");
            }
        }

        let notes_text = if p.html {
            p.html_notes(&notes)
        } else {
            p.md_notes(&notes)
        };
        if over(body.len() + head.len() + notes_text.len()) {
            return Err(too_large(
                opts.max_bytes,
                body.len() + head.len() + notes_text.len(),
            ));
        }

        let scope_note = match opts.scope {
            Some(id) => format!("구역 1개(nodeId {id})"),
            None => format!("문서 전체(구역 {}개)", sections.len()),
        };
        let c = &*p.counts;
        let mut lines = vec![
            format!("범위: {scope_note}. 읽기 전용 변환이며 원본 문서는 바뀌지 않았습니다."),
            format!(
                "변환한 개수: 문단 {} · 표 {} · 그림 {} · 수식 {} · 각주 {} · 미주 {} · 머리말 {} · 꼬리말 {}",
                c.paragraphs,
                c.tables,
                c.pictures,
                c.equations,
                c.footnotes,
                c.endnotes,
                c.headers,
                c.footers
            ),
        ];
        let list = |v: &[usize]| {
            v.iter()
                .map(|i| format!("표 {i}"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        if html {
            if !merged.is_empty() {
                lines.push(format!(
                    "병합 셀이 있는 표: {} (colspan/rowspan으로 표현)",
                    list(&merged)
                ));
            }
        } else {
            if c.alignment_ignored > 0 {
                lines.push(format!(
                    "문단 정렬(가운데·오른쪽) {}곳은 Markdown에서 표현할 수 없어 무시했습니다.",
                    c.alignment_ignored
                ));
            }
            if !merged.is_empty() {
                lines.push(format!(
                    "병합 셀이 있는 표: {}. Markdown은 병합을 표현할 수 없어 병합된 칸을 반복하지 않고 비워 두었습니다.",
                    list(&merged)
                ));
            }
            if c.tables > 0 {
                lines.push(
                    "표는 Markdown 문법상 머리글 행이 필요해 빈 머리글 행을 넣었습니다.".into(),
                );
            }
        }
        if c.pictures > 0 {
            lines.push(
                "그림은 이미지 파일을 만들지 않고 미디어 참조(파일명·SHA-256)와 캡션만 적었습니다."
                    .into(),
            );
        }
        if c.equations > 0 {
            lines.push("수식은 hp:script 원문을 변환하지 않고 코드로 감싸 적었습니다.".into());
        }
        if c.notes_omitted > 0 {
            lines.push(format!(
                "includeNotes=false라서 각주·미주 {}개를 뺐습니다.",
                c.notes_omitted
            ));
        }
        if !c.unsupported.is_empty() {
            let names = c
                .unsupported
                .iter()
                .map(|(k, v)| format!("{k} ×{v}"))
                .collect::<Vec<_>>()
                .join(", ");
            lines.push(format!(
                "변환하지 못한 요소: {names}. 본문에 ⟦미지원: 이름⟧로 표시했습니다."
            ));
        }
        let hidden = &c.inside_unsupported;
        if hidden.paragraphs + hidden.tables + hidden.pictures + hidden.equations > 0 {
            lines.push(format!(
                "미지원 개체 안에 있어 변환하지 못한 내용: 문단 {} · 표 {} · 그림 {} · 수식 {}",
                hidden.paragraphs, hidden.tables, hidden.pictures, hidden.equations
            ));
        }
        let format_name = if html { "HTML" } else { "Markdown" };

        let mut text = String::new();
        if html {
            text.push_str(
                "<!DOCTYPE html>\n<html lang=\"ko\">\n<head>\n<meta charset=\"utf-8\">\n",
            );
            text.push_str("<title>HWPX 문서 내보내기</title>\n");
            text.push_str(
                "<meta name=\"generator\" content=\"hae document.exportText\">\n</head>\n<body>\n",
            );
            let _ = writeln!(
                text,
                "<aside class=\"hwpx-export-notice\">\n<p><strong>변환 안내</strong> (HWPX → {format_name})</p>\n<ul>"
            );
            for l in &lines {
                let _ = writeln!(text, "<li>{}</li>", html_escape(l));
            }
            text.push_str("</ul>\n</aside>\n");
            text.push_str(&head);
            text.push_str(&body);
            text.push_str(&notes_text);
            text.push_str("</body>\n</html>\n");
        } else {
            let _ = writeln!(text, "> **변환 안내** (HWPX → {format_name})");
            for l in &lines {
                let _ = writeln!(text, "> - {}", md_escape(l));
            }
            text.push('\n');
            text.push_str(&head);
            text.push_str(&body);
            text.push_str(&notes_text);
            while text.ends_with("\n\n") {
                text.pop();
            }
        }
        if over(text.len()) {
            return Err(too_large(opts.max_bytes, text.len()));
        }
        Ok(ExportedText { text, counts })
    }
}
