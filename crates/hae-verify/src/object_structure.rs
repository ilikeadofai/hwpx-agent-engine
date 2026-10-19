//! Request/source-only verifier for object insertion and table deletion (0.4).
//!
//! Derives the expected bytes, media and manifest changes of the whole entry
//! from the pre-request XML and the request alone. It consumes no editor plan,
//! node id or node-set declaration and has no `hae-model` or `hae-edit`
//! dependency. The carrier paragraph and the deletion rules mirror
//! docs/decisions/report-object-creation-plan.md but are implemented
//! separately from the editor: a rule bug shared by both would still be caught
//! only by the Hancom gate.
use crate::{
    change, equation_insert, fail, hp, image_insert, table_insert, unsupported, Splice,
    VerifyOperation,
};
use hae_diag::{Limits, Result};
use hae_package::PackageState;
use hae_xml::XmlDoc;

/// Verifier-owned child-position remapping, used to follow requests that
/// address paragraphs moved by an earlier object step of the same batch.
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
    /// Element index of the paragraph whose cache must clear from its ordinal.
    pub anchor_el: usize,
    /// Paragraph text appeared or vanished: `PrvText` is stale.
    pub text_changed: bool,
}

fn children(d: &XmlDoc, parent: usize) -> Vec<usize> {
    crate::logical_children(d, parent)
}

fn place(d: &XmlDoc, p: usize) -> Result<(Vec<usize>, usize, Vec<usize>)> {
    let parent = d
        .el(p)
        .parent
        .ok_or_else(|| unsupported("object has no structural parent"))?;
    let siblings = children(d, parent);
    let index = siblings
        .iter()
        .position(|&c| c == p)
        .ok_or_else(|| fail("target is not a child of its parent"))?;
    Ok((crate::stable_path(d, parent), index, siblings))
}

fn top_level_paragraph(d: &XmlDoc, p: usize) -> Result<()> {
    if !hp(d.el(p), "p") || d.el(p).parent != Some(d.root()) {
        return Err(unsupported(
            "object insertion is supported only on top-level section paragraphs",
        ));
    }
    Ok(())
}

/// New carrier paragraph bytes: the target's start tag without its `id`, the
/// first run's start tag verbatim, the object content instead of `hp:t`, then
/// the closing run and paragraph tags. Independent of the editor's copy.
fn carrier(d: &XmlDoc, p: usize, object: &[u8]) -> Result<Vec<u8>> {
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
        // Self-closing run: reopen the start tag before the object content.
        out.extend_from_slice(&run_start[..run_start.len() - 2]);
        out.push(b'>');
        out.extend_from_slice(object);
        out.extend_from_slice(format!("</{}>", d.el(run).qname).as_bytes());
    } else {
        out.extend_from_slice(&run_start);
        out.extend_from_slice(object);
        out.extend_from_slice(format!("</{}>", d.el(run).qname).as_bytes());
    }
    out.extend_from_slice(format!("</{}>", d.el(p).qname).as_bytes());
    Ok(out)
}

fn insert(
    state: &mut PackageState,
    entry: &str,
    d: &XmlDoc,
    target: usize,
    op: &VerifyOperation,
    limits: &Limits,
) -> Result<Outcome> {
    use crate::VerifyOperation as Op;
    let position = match op {
        Op::TableInsert { position, .. }
        | Op::EquationInsert { position, .. }
        | Op::ImageInsert { position, .. } => position.as_str(),
        _ => return Err(fail("not an object insert operation")),
    };
    if position != "before" && position != "after" {
        return Err(fail("position must be before or after"));
    }
    top_level_paragraph(d, target)?;
    if position == "before"
        && std::iter::once(target)
            .chain(d.descendants(target))
            .any(|i| hp(d.el(i), "secPr"))
    {
        return Err(unsupported(
            "inserting before the paragraph that owns the section properties",
        ));
    }
    let object = match op {
        Op::TableInsert {
            rows,
            columns,
            width,
            row_height,
            cells,
            ..
        } => table_insert::build(
            state,
            d,
            target,
            *rows,
            *columns,
            *width,
            *row_height,
            cells,
        )?,
        Op::EquationInsert {
            script,
            width,
            height,
            ..
        } => equation_insert::build(state, d, target, script, *width, *height)?,
        Op::ImageInsert {
            bytes,
            width,
            height,
            ..
        } => image_insert::check(state, d, target, bytes, *width, *height, limits)?,
        _ => return Err(fail("not an object insert operation")),
    };
    let (scope, index, siblings) = place(d, target)?;
    let blob = carrier(d, target, &object)?;
    let at = if position == "before" {
        d.el(target).outer().start
    } else {
        d.el(target).outer().end
    };
    let cuts: Vec<Splice> = vec![(at..at, blob)];
    change(state, entry, d, cuts)?;
    let first = index + usize::from(position == "after");
    let children_map: Vec<Option<usize>> = (0..siblings.len())
        .map(|p| Some(if p >= first { p + 1 } else { p }))
        .collect();
    Ok(Outcome {
        remap: Remap {
            scope,
            children: children_map,
        },
        anchor_el: target,
        // An inserted object changes the textual preview even without `hp:t`.
        text_changed: true,
    })
}

fn delete(state: &mut PackageState, entry: &str, d: &XmlDoc, target: usize) -> Result<Outcome> {
    if !hp(d.el(target), "tbl") {
        return Err(fail("table.delete target is not a table"));
    }
    let carrier = d
        .ancestors(target)
        .find(|&i| hp(d.el(i), "p"))
        .ok_or_else(|| unsupported("table has no paragraph carrier"))?;
    if d.el(carrier).parent != Some(d.root()) {
        return Err(unsupported(
            "table deletion is supported only for a top-level carrier paragraph",
        ));
    }
    let siblings = children(d, carrier);
    let k = siblings
        .iter()
        .position(|&c| c == target)
        .ok_or_else(|| fail("table is not a structural child of its carrier"))?;
    let scope = crate::stable_path(d, carrier);
    // Old position -> new position: the removed child's slot closes up, so
    // later structural siblings of the carrier shift down by one.
    let children_map: Vec<Option<usize>> = (0..siblings.len())
        .map(|p| match p.cmp(&k) {
            std::cmp::Ordering::Less => Some(p),
            std::cmp::Ordering::Equal => None,
            std::cmp::Ordering::Greater => Some(p - 1),
        })
        .collect();
    change(state, entry, d, vec![(d.el(target).outer(), Vec::new())])?;
    Ok(Outcome {
        remap: Remap {
            scope,
            children: children_map,
        },
        anchor_el: carrier,
        // A removed table changes the textual preview even with no cell text.
        text_changed: true,
    })
}

pub(crate) fn check(
    state: &mut PackageState,
    entry: &str,
    d: &XmlDoc,
    target: usize,
    op: &VerifyOperation,
    limits: &Limits,
) -> Result<Outcome> {
    match op {
        VerifyOperation::TableDelete { .. } => delete(state, entry, d, target),
        VerifyOperation::TableInsert { .. }
        | VerifyOperation::ImageInsert { .. }
        | VerifyOperation::EquationInsert { .. } => insert(state, entry, d, target, op, limits),
        _ => Err(fail("not an object structure operation")),
    }
}
