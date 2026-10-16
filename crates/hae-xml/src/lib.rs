//! hae-xml: Source-preserving XML token representation, namespace resolution and range replacement.
//!
//! An [`XmlDoc`] keeps the original bytes and an index of elements with their
//! exact byte ranges. Nothing is ever re-serialized: edits are byte splices
//! ([`XmlDoc::splice`]) followed by a re-parse, so bytes outside the edited
//! ranges are preserved exactly (spec §4, §11).

use hae_diag::{Code, EngineError, PackageRejectReason as R, Result, Stage};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

pub const NS_XML: &str = "http://www.w3.org/XML/1998/namespace";

fn malformed(msg: impl Into<String>) -> EngineError {
    EngineError::rejected(R::MalformedXml, msg)
}

/// One attribute with exact source ranges.
#[derive(Debug, Clone)]
pub struct Attr {
    pub qname: String,
    pub ns: Option<Arc<str>>,
    pub local: String,
    /// Unescaped value.
    pub value: String,
    /// Byte range of the whole `name="value"` token.
    pub span: Range<usize>,
    /// Byte range of the raw value between the quotes.
    pub value_span: Range<usize>,
}

/// One piece of element content, in document order.
#[derive(Debug, Clone)]
pub enum Item {
    Element(usize),
    /// Character data without references; `text` equals the raw bytes.
    Text {
        raw: Range<usize>,
        text: String,
    },
    /// A character or predefined entity reference resolving to `text`.
    Ref {
        raw: Range<usize>,
        text: String,
    },
    CData {
        raw: Range<usize>,
        text: String,
    },
    Comment(Range<usize>),
    Pi(Range<usize>),
}

impl Item {
    pub fn text(&self) -> Option<&str> {
        match self {
            Item::Text { text, .. } | Item::Ref { text, .. } | Item::CData { text, .. } => {
                Some(text)
            }
            _ => None,
        }
    }
    pub fn raw(&self) -> Option<&Range<usize>> {
        match self {
            Item::Text { raw, .. } | Item::Ref { raw, .. } | Item::CData { raw, .. } => Some(raw),
            Item::Comment(r) | Item::Pi(r) => Some(r),
            Item::Element(_) => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Element {
    pub qname: String,
    pub ns: Option<Arc<str>>,
    pub local: String,
    /// Start tag (or the whole empty-element tag).
    pub start: Range<usize>,
    /// End tag; `None` for `<x/>`.
    pub end: Option<Range<usize>>,
    pub depth: usize,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub attrs: Vec<Attr>,
    pub items: Vec<Item>,
}

impl Element {
    /// Whole element range including start and end tags.
    pub fn outer(&self) -> Range<usize> {
        self.start.start..self.end.as_ref().map_or(self.start.end, |e| e.end)
    }
    /// Range between start and end tag (empty at `start.end` for `<x/>`).
    pub fn inner(&self) -> Range<usize> {
        self.start.end..self.end.as_ref().map_or(self.start.end, |e| e.start)
    }
    pub fn is(&self, ns: &str, local: &str) -> bool {
        self.local == local && self.ns.as_deref() == Some(ns)
    }
    pub fn attr(&self, local: &str) -> Option<&Attr> {
        self.attrs
            .iter()
            .find(|a| a.local == local && a.ns.is_none())
    }
    pub fn attr_value(&self, local: &str) -> Option<&str> {
        self.attr(local).map(|a| a.value.as_str())
    }
}

/// Parsed, source-preserving XML document.
#[derive(Debug, Clone)]
pub struct XmlDoc {
    bytes: Arc<[u8]>,
    elements: Vec<Element>,
    root: usize,
}

/// Options for parsing.
#[derive(Debug, Clone, Copy)]
pub struct ParseOptions {
    pub max_depth: usize,
}

impl Default for ParseOptions {
    fn default() -> Self {
        ParseOptions { max_depth: 128 }
    }
}

pub fn is_xml_char(c: char) -> bool {
    matches!(c, '\u{9}' | '\u{A}' | '\u{D}' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}')
}

fn check_chars(s: &str, what: &str) -> Result<()> {
    if let Some(c) = s.chars().find(|&c| !is_xml_char(c)) {
        return Err(malformed(format!(
            "{what}: forbidden character U+{:04X}",
            c as u32
        )));
    }
    Ok(())
}

/// Resolve the inside of `&...;`.
pub fn resolve_ref(name: &str) -> Result<char> {
    let c = match name {
        "lt" => '<',
        "gt" => '>',
        "amp" => '&',
        "quot" => '"',
        "apos" => '\'',
        _ => {
            let num = if let Some(h) = name.strip_prefix("#x") {
                u32::from_str_radix(h, 16).ok()
            } else if let Some(d) = name.strip_prefix('#') {
                d.parse::<u32>().ok()
            } else {
                return Err(EngineError::rejected(
                    R::DtdOrEntity,
                    format!("undeclared entity &{name};"),
                ));
            };
            num.and_then(char::from_u32)
                .ok_or_else(|| malformed(format!("bad character reference &{name};")))?
        }
    };
    if !is_xml_char(c) {
        return Err(malformed(format!(
            "character reference &{name}; is not an XML character"
        )));
    }
    Ok(c)
}

/// Unescape an attribute value or text containing references.
pub fn unescape(raw: &str) -> Result<String> {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i + 1..];
        let j = tail
            .find(';')
            .ok_or_else(|| malformed("unterminated reference"))?;
        out.push(resolve_ref(&tail[..j])?);
        rest = &tail[j + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

/// Escape text content. `>` is escaped too so `]]>` can never appear.
pub fn escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\r' => out.push_str("&#xD;"),
            _ => out.push(c),
        }
    }
    out
}

/// Escape an attribute value for double or single quotes.
pub fn escape_attr(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' => out.push_str("&#x9;"),
            '\n' => out.push_str("&#xA;"),
            '\r' => out.push_str("&#xD;"),
            _ => out.push(c),
        }
    }
    out
}

/// (name, unescaped value, token span, value span)
type RawAttr = (String, String, Range<usize>, Range<usize>);

/// Scan attributes in a start tag `tag` that begins at absolute offset `base`.
fn scan_attrs(tag: &[u8], base: usize) -> Result<Vec<RawAttr>> {
    // Skip '<' and the element name.
    let mut i = 1;
    while i < tag.len() && !tag[i].is_ascii_whitespace() && tag[i] != b'>' && tag[i] != b'/' {
        i += 1;
    }
    let mut out = Vec::new();
    loop {
        while i < tag.len() && tag[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= tag.len() || tag[i] == b'>' || tag[i] == b'/' {
            break;
        }
        let ns = i;
        while i < tag.len() && tag[i] != b'=' && !tag[i].is_ascii_whitespace() {
            i += 1;
        }
        let name =
            std::str::from_utf8(&tag[ns..i]).map_err(|_| malformed("attribute name not UTF-8"))?;
        while i < tag.len() && tag[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= tag.len() || tag[i] != b'=' {
            return Err(malformed(format!("attribute {name} without value")));
        }
        i += 1;
        while i < tag.len() && tag[i].is_ascii_whitespace() {
            i += 1;
        }
        let q = *tag.get(i).ok_or_else(|| malformed("truncated attribute"))?;
        if q != b'"' && q != b'\'' {
            return Err(malformed(format!("attribute {name} value not quoted")));
        }
        let vs = i + 1;
        let ve = vs
            + tag[vs..]
                .iter()
                .position(|&b| b == q)
                .ok_or_else(|| malformed("unterminated attribute value"))?;
        let raw = std::str::from_utf8(&tag[vs..ve])
            .map_err(|_| malformed("attribute value not UTF-8"))?;
        if raw.contains('<') {
            return Err(malformed(format!("'<' in attribute {name}")));
        }
        let value = unescape(raw)?;
        check_chars(&value, "attribute")?;
        out.push((
            name.to_string(),
            value,
            base + ns..base + ve + 1,
            base + vs..base + ve,
        ));
        i = ve + 1;
    }
    Ok(out)
}

pub const NS_XMLNS: &str = "http://www.w3.org/2000/xmlns/";

/// XML 1.0 5th edition `NameStartChar` minus `:` (i.e. `NCNameStartChar`).
pub fn is_ncname_start(c: char) -> bool {
    matches!(c,
        'A'..='Z' | 'a'..='z' | '_'
        | '\u{C0}'..='\u{D6}' | '\u{D8}'..='\u{F6}' | '\u{F8}'..='\u{2FF}'
        | '\u{370}'..='\u{37D}' | '\u{37F}'..='\u{1FFF}'
        | '\u{200C}'..='\u{200D}' | '\u{2070}'..='\u{218F}'
        | '\u{2C00}'..='\u{2FEF}' | '\u{3001}'..='\u{D7FF}'
        | '\u{F900}'..='\u{FDCF}' | '\u{FDF0}'..='\u{FFFD}'
        | '\u{10000}'..='\u{EFFFF}')
}

/// XML 1.0 `NameChar` minus `:` (i.e. `NCNameChar`).
pub fn is_ncname_char(c: char) -> bool {
    is_ncname_start(c)
        || matches!(c,
            '-' | '.' | '0'..='9' | '\u{B7}'
            | '\u{300}'..='\u{36F}' | '\u{203F}'..='\u{2040}')
}

fn check_ncname(s: &str, what: &str) -> Result<()> {
    let mut chars = s.chars();
    match chars.next() {
        Some(f) if is_ncname_start(f) => {}
        _ => {
            return Err(malformed(format!("{what}: invalid NCName {s:?}")));
        }
    }
    if s.chars().any(|c| !is_ncname_char(c)) {
        return Err(malformed(format!("{what}: invalid NCName {s:?}")));
    }
    Ok(())
}

/// Split and validate a `QName`: at most one `:`, non-empty prefix/local,
/// each part a valid `NCName`.
fn parse_qname(q: &str) -> Result<(Option<&str>, &str)> {
    if q.is_empty() {
        return Err(malformed("empty element/attribute name"));
    }
    if q.matches(':').count() > 1 {
        return Err(malformed(format!("invalid QName {q:?}: more than one ':'")));
    }
    match q.split_once(':') {
        Some((p, l)) => {
            if p.is_empty() || l.is_empty() {
                return Err(malformed(format!(
                    "invalid QName {q:?}: empty prefix/local"
                )));
            }
            check_ncname(p, "prefix")?;
            check_ncname(l, "local part")?;
            Ok((Some(p), l))
        }
        None => {
            check_ncname(q, "name")?;
            Ok((None, q))
        }
    }
}

struct Scope {
    map: HashMap<String, Arc<str>>,
}

impl XmlDoc {
    pub fn parse(bytes: impl Into<Arc<[u8]>>) -> Result<XmlDoc> {
        Self::parse_with(bytes, ParseOptions::default())
    }

    pub fn parse_with(bytes: impl Into<Arc<[u8]>>, opts: ParseOptions) -> Result<XmlDoc> {
        let bytes: Arc<[u8]> = bytes.into();
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| EngineError::rejected(R::NonUtf8, "XML is not UTF-8"))?;
        let body_start = if text.starts_with('\u{FEFF}') { 3 } else { 0 };
        let mut reader = Reader::from_str(&text[body_start..]);
        {
            let c = reader.config_mut();
            c.check_end_names = true;
            c.expand_empty_elements = false;
            c.trim_text(false);
            c.check_comments = true;
        }
        let mut elements: Vec<Element> = Vec::new();
        let mut stack: Vec<usize> = Vec::new();
        let mut scopes: Vec<Scope> = vec![Scope {
            map: HashMap::from([("xml".to_string(), Arc::<str>::from(NS_XML))]),
        }];
        let mut root: Option<usize> = None;
        let mut pending_doc_items = 0usize;
        loop {
            let before = reader.buffer_position() as usize + body_start;
            let ev = reader
                .read_event()
                .map_err(|e| malformed(format!("at byte {before}: {e}")))?;
            let after = reader.buffer_position() as usize + body_start;
            let raw = before..after;
            match ev {
                Event::Eof => break,
                Event::DocType(_) => {
                    return Err(EngineError::rejected(
                        R::DtdOrEntity,
                        "DOCTYPE is not allowed",
                    ))
                }
                Event::Decl(d) => {
                    if before != body_start {
                        return Err(malformed("XML declaration not at start"));
                    }
                    if let Some(enc) = d.encoding() {
                        let enc =
                            enc.map_err(|e| malformed(format!("bad encoding declaration: {e}")))?;
                        if !enc.eq_ignore_ascii_case(b"utf-8") && !enc.eq_ignore_ascii_case(b"utf8")
                        {
                            return Err(EngineError::rejected(
                                R::NonUtf8,
                                format!("declared encoding {}", String::from_utf8_lossy(&enc)),
                            ));
                        }
                    }
                }
                Event::Start(ref s) | Event::Empty(ref s) => {
                    let is_empty = matches!(ev, Event::Empty(_));
                    if root.is_some() && stack.is_empty() {
                        return Err(malformed("more than one root element"));
                    }
                    let depth = stack.len();
                    if depth + 1 > opts.max_depth {
                        return Err(EngineError::new(
                            Code::ResourceLimit,
                            Stage::Open,
                            format!("XML depth exceeds {}", opts.max_depth),
                        ));
                    }
                    let qname = std::str::from_utf8(s.name().as_ref())
                        .map_err(|_| malformed("element name not UTF-8"))?
                        .to_string();
                    let raw_attrs = scan_attrs(&bytes[raw.clone()], before)?;
                    // New namespace scope with reserved-prefix enforcement.
                    let mut scope = Scope {
                        map: scopes.last().unwrap().map.clone(),
                    };
                    for (n, v, _, _) in &raw_attrs {
                        if n == "xmlns" {
                            if v == NS_XMLNS {
                                return Err(malformed(
                                    "default namespace bound to xmlns namespace",
                                ));
                            }
                            scope.map.insert(String::new(), Arc::from(v.as_str()));
                        } else if let Some(p) = n.strip_prefix("xmlns:") {
                            check_ncname(p, "namespace prefix")?;
                            if p == "xmlns" {
                                return Err(malformed("prefix xmlns must not be declared"));
                            }
                            if v.is_empty() {
                                return Err(malformed(format!("empty namespace for prefix {p}")));
                            }
                            if p == "xml" {
                                if v != NS_XML {
                                    return Err(malformed(format!(
                                        "prefix xml bound to {v:?}, must be {NS_XML:?}"
                                    )));
                                }
                            } else {
                                if v == NS_XML {
                                    return Err(malformed(format!(
                                        "prefix {p} must not bind the xml namespace"
                                    )));
                                }
                                if v == NS_XMLNS {
                                    return Err(malformed(format!(
                                        "prefix {p} must not bind the xmlns namespace"
                                    )));
                                }
                            }
                            scope.map.insert(p.to_string(), Arc::from(v.as_str()));
                        } else if n.contains(':') {
                            // Validate non-declaration prefixed names early so
                            // `xmlns:a:b`-style input cannot slip through scope
                            // handling; full QName checks repeat below.
                            parse_qname(n)?;
                        } else {
                            check_ncname(n, "attribute name")?;
                        }
                    }
                    let mut seen: std::collections::HashSet<String> =
                        std::collections::HashSet::new();
                    let mut seen_expanded: std::collections::HashSet<(String, String)> =
                        std::collections::HashSet::new();
                    let mut attrs = Vec::with_capacity(raw_attrs.len());
                    for (n, v, span, value_span) in raw_attrs {
                        if !seen.insert(n.clone()) {
                            return Err(malformed(format!("duplicate attribute {n}")));
                        }
                        // Namespace declarations are tracked separately and do
                        // not participate in expanded-name duplicate checks.
                        if n == "xmlns" || n.starts_with("xmlns:") {
                            parse_qname(&n)?;
                            attrs.push(Attr {
                                qname: n.clone(),
                                ns: None,
                                local: n.clone(),
                                value: v,
                                span,
                                value_span,
                            });
                            continue;
                        }
                        let (p, l) = parse_qname(&n)?;
                        let ns = match p {
                            None => None,
                            Some(p) => Some(
                                scope
                                    .map
                                    .get(p)
                                    .cloned()
                                    .ok_or_else(|| malformed(format!("unbound prefix {p}")))?,
                            ),
                        };
                        let key = (ns.as_deref().unwrap_or("").to_string(), l.to_string());
                        if !seen_expanded.insert(key) {
                            return Err(malformed(format!(
                                "duplicate expanded attribute {{{}}} {l}",
                                ns.as_deref().unwrap_or("")
                            )));
                        }
                        attrs.push(Attr {
                            qname: n.clone(),
                            ns,
                            local: l.to_string(),
                            value: v,
                            span,
                            value_span,
                        });
                    }
                    let (p, l) = parse_qname(&qname)?;
                    let ns = match p {
                        Some(p) => Some(
                            scope
                                .map
                                .get(p)
                                .cloned()
                                .ok_or_else(|| malformed(format!("unbound prefix {p}")))?,
                        ),
                        None => scope.map.get("").cloned().filter(|u| !u.is_empty()),
                    };
                    let idx = elements.len();
                    let parent = stack.last().copied();
                    elements.push(Element {
                        local: l.to_string(),
                        qname,
                        ns,
                        start: raw.clone(),
                        end: None,
                        depth,
                        parent,
                        children: Vec::new(),
                        attrs,
                        items: Vec::new(),
                    });
                    if let Some(pi) = parent {
                        elements[pi].children.push(idx);
                        elements[pi].items.push(Item::Element(idx));
                    } else {
                        root = Some(idx);
                    }
                    if !is_empty {
                        stack.push(idx);
                        scopes.push(scope);
                    }
                }
                Event::End(_) => {
                    let idx = stack.pop().ok_or_else(|| malformed("unexpected end tag"))?;
                    scopes.pop();
                    elements[idx].end = Some(raw);
                }
                Event::Text(t) => {
                    let s = std::str::from_utf8(&t).map_err(|_| malformed("text not UTF-8"))?;
                    check_chars(s, "text")?;
                    if s.contains("]]>") {
                        return Err(malformed("']]>' in character data"));
                    }
                    match stack.last() {
                        Some(&p) => elements[p].items.push(Item::Text {
                            raw,
                            text: s.to_string(),
                        }),
                        None => {
                            if !s.chars().all(|c| matches!(c, ' ' | '\t' | '\n' | '\r')) {
                                return Err(malformed("text outside the root element"));
                            }
                        }
                    }
                }
                Event::GeneralRef(r) => {
                    let name =
                        std::str::from_utf8(&r).map_err(|_| malformed("reference not UTF-8"))?;
                    let c = resolve_ref(name)?;
                    let p = *stack
                        .last()
                        .ok_or_else(|| malformed("reference outside the root element"))?;
                    elements[p].items.push(Item::Ref {
                        raw,
                        text: c.to_string(),
                    });
                }
                Event::CData(c) => {
                    let s = std::str::from_utf8(&c)
                        .map_err(|_| malformed("CDATA not UTF-8"))?
                        .to_string();
                    check_chars(&s, "CDATA")?;
                    let p = *stack
                        .last()
                        .ok_or_else(|| malformed("CDATA outside the root element"))?;
                    elements[p].items.push(Item::CData { raw, text: s });
                }
                Event::Comment(_) => {
                    if let Some(&p) = stack.last() {
                        elements[p].items.push(Item::Comment(raw));
                    } else {
                        pending_doc_items += 1;
                    }
                }
                Event::PI(_) => {
                    if let Some(&p) = stack.last() {
                        elements[p].items.push(Item::Pi(raw));
                    } else {
                        pending_doc_items += 1;
                    }
                }
            }
        }
        let _ = pending_doc_items;
        if !stack.is_empty() {
            return Err(malformed("unclosed element"));
        }
        let root = root.ok_or_else(|| malformed("no root element"))?;
        Ok(XmlDoc {
            bytes,
            elements,
            root,
        })
    }

    pub fn bytes(&self) -> &Arc<[u8]> {
        &self.bytes
    }
    pub fn root(&self) -> usize {
        self.root
    }
    pub fn elements(&self) -> &[Element] {
        &self.elements
    }
    pub fn el(&self, i: usize) -> &Element {
        &self.elements[i]
    }
    /// Elements in document order (index order is document order).
    pub fn iter(&self) -> impl Iterator<Item = (usize, &Element)> {
        self.elements.iter().enumerate()
    }
    /// Descendants of `i` in document order, excluding `i`.
    pub fn descendants(&self, i: usize) -> impl Iterator<Item = usize> + '_ {
        let outer = self.elements[i].outer();
        (i + 1..self.elements.len()).take_while(move |&j| self.elements[j].start.start < outer.end)
    }
    /// Ancestors of `i`, nearest first.
    pub fn ancestors(&self, i: usize) -> impl Iterator<Item = usize> + '_ {
        std::iter::successors(self.elements[i].parent, move |&p| self.elements[p].parent)
    }
    /// Concatenated character data of the direct content of `i`.
    pub fn direct_text(&self, i: usize) -> String {
        self.elements[i]
            .items
            .iter()
            .filter_map(|it| it.text())
            .collect()
    }
    pub fn raw(&self, r: Range<usize>) -> &[u8] {
        &self.bytes[r]
    }
    /// Ordinal path from the root: child index at each level (root = []).
    pub fn path(&self, i: usize) -> Vec<usize> {
        let mut p = Vec::new();
        let mut cur = i;
        while let Some(par) = self.elements[cur].parent {
            p.push(
                self.elements[par]
                    .children
                    .iter()
                    .position(|&c| c == cur)
                    .unwrap(),
            );
            cur = par;
        }
        p.reverse();
        p
    }

    /// Apply non-overlapping byte splices and re-parse. Ranges refer to the
    /// current bytes; replacements are raw bytes (caller escapes).
    pub fn splice(
        &self,
        mut edits: Vec<(Range<usize>, Vec<u8>)>,
        opts: ParseOptions,
    ) -> Result<XmlDoc> {
        edits.sort_by_key(|(r, _)| (r.start, r.end));
        for w in edits.windows(2) {
            if w[1].0.start < w[0].0.end {
                return Err(EngineError::new(
                    Code::StateInvariantError,
                    Stage::Apply,
                    "overlapping XML splices",
                ));
            }
        }
        let mut out = Vec::with_capacity(self.bytes.len());
        let mut last = 0;
        for (r, rep) in &edits {
            if r.end > self.bytes.len() || r.start < last {
                return Err(EngineError::new(
                    Code::StateInvariantError,
                    Stage::Apply,
                    "splice outside document",
                ));
            }
            out.extend_from_slice(&self.bytes[last..r.start]);
            out.extend_from_slice(rep);
            last = r.end;
        }
        out.extend_from_slice(&self.bytes[last..]);
        XmlDoc::parse_with(out, opts).map_err(|e| {
            EngineError::new(
                Code::StateInvariantError,
                Stage::Apply,
                format!("edited XML no longer parses: {e}"),
            )
        })
    }
}

/// Map from a codepoint offset within the concatenated text of a run of items
/// to raw byte ranges. Used to translate text ranges into splices.
#[derive(Debug, Clone)]
pub struct TextMap {
    /// (codepoint start, codepoint len, item raw range, kind)
    pieces: Vec<Piece>,
    pub text: String,
}

#[derive(Debug, Clone)]
struct Piece {
    cp_start: usize,
    cp_len: usize,
    raw: Range<usize>,
    kind: PieceKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PieceKind {
    Text,
    Ref,
    CData,
}

impl TextMap {
    /// Build from the direct character content of element `i`. Fails if the
    /// element has child elements or comments mixed into its text.
    pub fn of(doc: &XmlDoc, i: usize) -> Option<TextMap> {
        let mut pieces = Vec::new();
        let mut text = String::new();
        let mut cp = 0;
        for it in &doc.el(i).items {
            let (raw, t, kind) = match it {
                Item::Text { raw, text } => (raw, text, PieceKind::Text),
                Item::Ref { raw, text } => (raw, text, PieceKind::Ref),
                Item::CData { raw, text } => (raw, text, PieceKind::CData),
                _ => return None,
            };
            let n = t.chars().count();
            pieces.push(Piece {
                cp_start: cp,
                cp_len: n,
                raw: raw.clone(),
                kind,
            });
            text.push_str(t);
            cp += n;
        }
        Some(TextMap { pieces, text })
    }

    pub fn len_cp(&self) -> usize {
        self.pieces.last().map_or(0, |p| p.cp_start + p.cp_len)
    }

    /// Byte offset in the document for codepoint position `cp` (0..=len).
    /// `None` if the position falls inside CDATA or a reference.
    fn byte_at(&self, doc: &XmlDoc, cp: usize, inner_start: usize) -> Option<usize> {
        if self.pieces.is_empty() {
            return Some(inner_start);
        }
        for p in &self.pieces {
            if cp == p.cp_start {
                return Some(p.raw.start);
            }
            if cp > p.cp_start && cp < p.cp_start + p.cp_len {
                if p.kind != PieceKind::Text {
                    return None;
                }
                let s = std::str::from_utf8(&doc.bytes[p.raw.clone()]).ok()?;
                let off = s.char_indices().nth(cp - p.cp_start)?.0;
                return Some(p.raw.start + off);
            }
        }
        let last = self.pieces.last().unwrap();
        (cp == last.cp_start + last.cp_len).then_some(last.raw.end)
    }

    /// Splice replacing codepoints `[start, end)` of element `i`'s text with `new`
    /// (escaped). Errors if a boundary falls inside a CDATA section or a
    /// reference, or if the range touches CDATA.
    pub fn replace(
        &self,
        doc: &XmlDoc,
        i: usize,
        start: usize,
        end: usize,
        new: &str,
    ) -> Result<(Range<usize>, Vec<u8>)> {
        if start > end || end > self.len_cp() {
            return Err(EngineError::new(
                Code::InvalidRange,
                Stage::Validate,
                "range outside text",
            ));
        }
        for p in &self.pieces {
            let overlaps = p.cp_start < end && start < p.cp_start + p.cp_len;
            if overlaps && p.kind == PieceKind::CData {
                return Err(EngineError::new(
                    Code::Unsupported,
                    Stage::Validate,
                    "range touches CDATA",
                )
                .with_reason("cdata"));
            }
        }
        let inner = doc.el(i).inner().start;
        let bs = self.byte_at(doc, start, inner).ok_or_else(|| {
            EngineError::new(
                Code::Unsupported,
                Stage::Validate,
                "range boundary inside a reference",
            )
        })?;
        let be = self.byte_at(doc, end, inner).ok_or_else(|| {
            EngineError::new(
                Code::Unsupported,
                Stage::Validate,
                "range boundary inside a reference",
            )
        })?;
        if new.chars().any(|c| !is_xml_char(c)) {
            return Err(EngineError::new(
                Code::InvalidRequest,
                Stage::Validate,
                "replacement contains a forbidden character",
            ));
        }
        Ok((bs..be, escape_text(new).into_bytes()))
    }
}

#[cfg(test)]
mod tests;
