//! Request/source-only verifier for paragraph insertion and deletion (0.4).
//!
//! Derives the expected bytes of the whole entry from the pre-request XML and
//! the request (`entry`, `path`, `position`, `text`, `count`) alone. It consumes
//! no editor plan, node id or node-set declaration and has no `hae-model` or
//! `hae-edit` dependency. The rules mirror docs/decisions/
//! document-paragraph-creation-design.md §5 but are implemented separately: a
//! rule bug shared by both would still be caught by the Hancom gate only.
use crate::{change, fail, hp, unsupported, Splice};
use hae_diag::Result;
use hae_package::PackageState;
use hae_xml::{escape_text, is_xml_char, XmlDoc};

const LIMIT: usize = 1024;

/// Elements a deleted paragraph may contain (design §5.3).
const PLAIN: [&str; 10] = [
    "p",
    "run",
    "t",
    "linesegarray",
    "lineseg",
    "tab",
    "lineBreak",
    "nbSpace",
    "fwSpace",
    "hyphen",
];

/// Verifier-owned child-position remapping, used to follow requests that
/// address paragraphs moved by an earlier step of the same batch.
#[derive(Debug, Clone, Default)]
pub(crate) struct Remap {
    scope: Vec<usize>,
    children: Vec<Option<usize>>,
}
impl Remap {
    /// Path after the change; `None` when the element was removed.
    pub(crate) fn map(&self, path: &[usize]) -> Option<Vec<usize>> {
        let n = self.scope.len();
        if path.len() <= n || path[..n] != self.scope[..] {
            return Some(path.to_vec());
        }
        let mut out = self.scope.clone();
        out.push(self.children.get(path[n]).copied().flatten()?);
        out.extend_from_slice(&path[n + 1..]);
        Some(out)
    }
}

pub(crate) struct Outcome {
    pub remap: Remap,
    /// Stable path of the enclosing `hp:tbl`, if the target is a cell paragraph.
    pub inside_table: Option<Vec<usize>>,
    /// Paragraph text appeared or vanished: `PrvText` is stale.
    pub text_changed: bool,
}

/// Structural children of one element, with `hp:run`/`hp:t` transparent. This
/// mirrors `hae-model`'s counting rule but is implemented here independently.
fn children(d: &XmlDoc, parent: usize) -> Vec<usize> {
    crate::logical_children(d, parent)
}

fn plain(d: &XmlDoc, p: usize) -> Result<bool> {
    let mut text = false;
    for i in std::iter::once(p).chain(d.descendants(p)) {
        let e = d.el(i);
        if !hp(e, e.local.as_str()) {
            return Err(unsupported("deleted paragraph holds foreign content"));
        }
        if e.local == "tbl" {
            return Err(unsupported("deleted paragraph holds a table"));
        }
        if !PLAIN.contains(&e.local.as_str()) {
            return Err(unsupported(
                "deleted paragraph holds objects, fields or notes",
            ));
        }
        text |= e.local == "t" && !e.inner().is_empty();
    }
    Ok(text)
}

/// New paragraph bytes: target start tag without `id`, first run start tag
/// verbatim, and nothing or one `hp:t` (design §5.2).
fn blank(d: &XmlDoc, p: usize, text: &str) -> Result<Vec<u8>> {
    if d.el(p).end.is_none() {
        return Err(unsupported("template paragraph is self-closing"));
    }
    let run = d
        .el(p)
        .children
        .iter()
        .copied()
        .find(|&c| hp(d.el(c), "run"))
        .ok_or_else(|| unsupported("template paragraph has no run"))?;
    if d.el(run).attr("charPrIDRef").is_none() {
        return Err(unsupported("template run has no charPrIDRef"));
    }
    let start = d.el(p).start.clone();
    let mut open = d.raw(start.clone()).to_vec();
    if let Some(a) = d.el(p).attr("id") {
        // Same rule as the editor: drop the id token and one adjacent space.
        let (mut from, mut to) = (a.span.start - start.start, a.span.end - start.start);
        if from > 0 && open[from - 1].is_ascii_whitespace() {
            from -= 1;
        } else if to < open.len() && open[to].is_ascii_whitespace() {
            to += 1;
        }
        open.drain(from..to);
    }
    let run_start = d.raw(d.el(run).start.clone()).to_vec();
    let mut out = open;
    if d.el(run).end.is_none() {
        // Self-closing template run: drop the `/>` and reopen the tag.
        out.extend_from_slice(&run_start[..run_start.len() - 2]);
        if text.is_empty() {
            out.extend_from_slice(b"/>");
        } else {
            out.extend_from_slice(
                format!("><hp:t>{}</hp:t></{}>", escape_text(text), d.el(run).qname).as_bytes(),
            );
        }
    } else {
        out.extend_from_slice(&run_start);
        if !text.is_empty() {
            out.extend_from_slice(format!("<hp:t>{}</hp:t>", escape_text(text)).as_bytes());
        }
        out.extend_from_slice(format!("</{}>", d.el(run).qname).as_bytes());
    }
    out.extend_from_slice(format!("</{}>", d.el(p).qname).as_bytes());
    Ok(out)
}

fn check_text(text: &str) -> Result<()> {
    if text.contains(['\t', '\n', '\r']) {
        return Err(fail("new paragraph text must not contain tabs or breaks"));
    }
    if let Some(c) = text.chars().find(|&c| !is_xml_char(c)) {
        return Err(fail(format!(
            "new paragraph text contains forbidden character U+{:04X}",
            c as u32
        )));
    }
    Ok(())
}

/// Where the target sits among its container's structural children.
struct Place {
    parent: usize,
    scope: Vec<usize>,
    index: usize,
    siblings: Vec<usize>,
}

fn place(d: &XmlDoc, p: usize) -> Result<Place> {
    let parent = d
        .el(p)
        .parent
        .ok_or_else(|| unsupported("paragraph has no parent"))?;
    let siblings = children(d, parent);
    let index = siblings
        .iter()
        .position(|&c| c == p)
        .ok_or_else(|| fail("target is not a child of its parent"))?;
    Ok(Place {
        parent,
        scope: crate::stable_path(d, parent),
        index,
        siblings,
    })
}

fn check_scope(d: &XmlDoc, place: &Place) -> Result<()> {
    let ok = if place.parent == d.root() {
        true
    } else {
        hp(d.el(place.parent), "subList")
            && d.el(place.parent).parent.is_some_and(|t| hp(d.el(t), "tc"))
    };
    if ok {
        Ok(())
    } else {
        Err(unsupported(
            "only section paragraphs and table cell paragraphs are supported",
        ))
    }
}

pub(crate) fn check(
    state: &mut PackageState,
    entry: &str,
    d: &XmlDoc,
    target: usize,
    op: &crate::VerifyOperation,
) -> Result<Outcome> {
    use crate::VerifyOperation as Op;
    let (position, text, count) = match op {
        Op::ParagraphInsert {
            position,
            text,
            count,
            ..
        } => (
            Some(position.as_str()),
            text.as_deref().unwrap_or(""),
            *count,
        ),
        Op::ParagraphDelete { count, .. } => (None, "", *count),
        _ => return Err(fail("not a paragraph structure operation")),
    };
    if !hp(d.el(target), "p") {
        return Err(fail("paragraph structure target is not a paragraph"));
    }
    if !(1..=LIMIT).contains(&count) {
        return Err(fail("count outside 1..=1024"));
    }
    let place = place(d, target)?;
    check_scope(d, &place)?;
    let inside_table = d
        .ancestors(target)
        .find(|&i| hp(d.el(i), "tbl"))
        .map(|t| crate::stable_path(d, t));
    let mut cuts: Vec<Splice> = Vec::new();
    let mut text_changed = false;
    let children_map: Vec<Option<usize>>;
    match position {
        Some(position) => {
            if position != "before" && position != "after" {
                return Err(fail("position must be before or after"));
            }
            check_text(text)?;
            if position == "before"
                && std::iter::once(target)
                    .chain(d.descendants(target))
                    .any(|i| hp(d.el(i), "secPr"))
            {
                return Err(unsupported(
                    "inserting before the paragraph that owns the section properties",
                ));
            }
            let blob = blank(d, target, text)?;
            let mut repeated = Vec::with_capacity(blob.len() * count);
            for _ in 0..count {
                repeated.extend_from_slice(&blob);
            }
            let at = if position == "before" {
                d.el(target).outer().start
            } else {
                d.el(target).outer().end
            };
            cuts.push((at..at, repeated));
            let first = place.index + usize::from(position == "after");
            children_map = (0..place.siblings.len())
                .map(|p| Some(if p >= first { p + count } else { p }))
                .collect();
            text_changed = !text.is_empty();
        }
        None => {
            if place.index + count > place.siblings.len() {
                return Err(fail("deletion range leaves the container"));
            }
            let mut doomed = Vec::with_capacity(count);
            for p in place.index..place.index + count {
                let el = place.siblings[p];
                if !hp(d.el(el), "p") {
                    return Err(unsupported(
                        "the paragraphs to delete are not contiguous in their container",
                    ));
                }
                doomed.push(el);
            }
            if place.siblings.iter().filter(|&&c| hp(d.el(c), "p")).count() <= count {
                return Err(fail("a container must keep at least one paragraph"));
            }
            for &el in &doomed {
                text_changed |= plain(d, el)?;
            }
            for &el in &doomed {
                cuts.push((d.el(el).outer(), Vec::new()));
            }
            let mut shift = 0;
            children_map = (0..place.siblings.len())
                .map(|p| {
                    if p >= place.index && p < place.index + count {
                        shift += 1;
                        None
                    } else {
                        Some(p - shift)
                    }
                })
                .collect();
        }
    }
    change(state, entry, d, cuts)?;
    Ok(Outcome {
        remap: Remap {
            scope: place.scope,
            children: children_map,
        },
        inside_table,
        text_changed,
    })
}
