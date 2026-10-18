//! Paragraph insertion and deletion (0.4).
//!
//! The second place where the engine creates and removes nodes, and the first
//! one outside a table. Rules and the reasons behind them:
//! docs/decisions/document-paragraph-creation-design.md. `hae-verify`
//! re-derives every byte of the result from the request alone; this module only
//! plans splices and declares the paragraph-sequence change ([`ParagraphRemap`])
//! that the model's commit gate checks against the result.
use crate::table::{gate, hp, unsupported};
use crate::{CreatedNode, Draft, Intent, ParagraphReport};
use hae_diag::{Code, EngineError, Result, Stage};
use hae_model::{structural_children, Kind, Node, NodeRemap, ParagraphRemap, StructureDelta};
use hae_xml::{escape_text, XmlDoc};
use serde_json::json;
use std::ops::Range;

type Cut = (Range<usize>, Vec<u8>);

/// Same bound as the table structure commands.
pub(crate) const MAX_COUNT: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Position {
    Before,
    After,
}

impl Position {
    fn parse(s: &str) -> Result<Self> {
        match s {
            "before" => Ok(Self::Before),
            "after" => Ok(Self::After),
            _ => Err(EngineError::new(
                Code::InvalidRequest,
                Stage::Request,
                "position must be \"before\" or \"after\"",
            )),
        }
    }
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Before => "before",
            Self::After => "after",
        }
    }
}

fn invalid(reason: &str, message: impl Into<String>) -> EngineError {
    EngineError::new(Code::InvalidArgument, Stage::Validate, message).with_reason(reason)
}

/// Where the target paragraph sits among its container's structural children.
struct Place {
    scope: Vec<usize>,
    index: usize,
    children: Vec<usize>,
}

fn place(d: &XmlDoc, para: usize) -> Result<Place> {
    let parent = d
        .el(para)
        .parent
        .ok_or_else(|| unsupported("paragraph_scope_unsupported", "paragraph has no parent"))?;
    let children = structural_children(d, parent);
    let index = children
        .iter()
        .position(|&c| c == para)
        .ok_or_else(|| unsupported("paragraph_scope_unsupported", "paragraph is not a child"))?;
    Ok(Place {
        scope: hae_model::stable_path(d, parent),
        index,
        children,
    })
}

/// New paragraph bytes: the target's start tag without its `id`, plus its first
/// run's start tag (attributes verbatim) and either nothing or one `hp:t`
/// (design §5.2).
fn blank_paragraph(d: &XmlDoc, para: usize, text: &str) -> Result<Vec<u8>> {
    let tpl = |m: &str| unsupported("paragraph_template_unsupported", m);
    if d.el(para).end.is_none() {
        return Err(tpl("template paragraph is self-closing"));
    }
    let run = d
        .el(para)
        .children
        .iter()
        .copied()
        .find(|&c| hp(d, c, "run"))
        .ok_or_else(|| tpl("template paragraph has no run"))?;
    if d.el(run).attr("charPrIDRef").is_none() {
        return Err(tpl("template run has no charPrIDRef"));
    }
    let start = d.el(para).start.clone();
    let mut open = d.raw(start.clone()).to_vec();
    if let Some(a) = d.el(para).attr("id") {
        // Drop the id token together with one adjacent space so the new start
        // tag keeps single spacing (design §5.2).
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
    out.extend_from_slice(format!("</{}>", d.el(para).qname).as_bytes());
    Ok(out)
}

/// The `hp:tbl` node enclosing a paragraph, if any (pre-change coordinates).
fn table_of(d: &Draft, node: &Node) -> Option<Node> {
    let xml = d.doc.doc_of(node);
    xml.ancestors(node.el)
        .find(|&i| hp(xml, i, "tbl"))
        .and_then(|el| {
            d.doc
                .nodes
                .iter()
                .find(|n| n.entry == node.entry && n.el == el)
        })
        .cloned()
}

/// Cache policy for a cell paragraph: the table's layout may change, so this
/// mirrors a table structure edit (design §5.4).
fn anchor_enclosing_table(d: &mut Draft, table: &Node) -> Result<()> {
    let entry = table.entry.clone();
    let current = d.doc.node(&table.id).cloned().ok_or_else(|| {
        EngineError::new(
            Code::StateInvariantError,
            Stage::Apply,
            "enclosing table vanished",
        )
    })?;
    let xml = d.doc.doc_of(&current).clone();
    let inside: Vec<String> = d
        .doc
        .nodes
        .iter()
        .filter(|n| n.kind == Kind::Paragraph && d.doc.is_descendant(n, &table.id))
        .map(|n| n.id.clone())
        .collect();
    if let Some(a) = d.anchors.get_mut(&entry) {
        a.1.retain(|id| !inside.contains(id));
    }
    let carrier = xml
        .ancestors(current.el)
        .find(|&i| hp(&xml, i, "p"))
        .and_then(|i| d.doc.nodes.iter().find(|n| n.entry == entry && n.el == i))
        .cloned()
        .ok_or_else(|| {
            EngineError::new(
                Code::StateInvariantError,
                Stage::Apply,
                "table has no paragraph carrier",
            )
        })?;
    d.anchor(&carrier);
    let paragraphs: Vec<Node> = d
        .doc
        .nodes
        .iter()
        .filter(|n| n.kind == Kind::Paragraph && d.doc.is_descendant(n, &table.id))
        .cloned()
        .collect();
    for p in paragraphs {
        d.anchor(&p);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn report(
    d: &mut Draft,
    command: &str,
    target: &Node,
    position: Option<Position>,
    count: usize,
    delta: StructureDelta,
    extra_changed: Vec<String>,
    scope: Vec<usize>,
    text_changed: bool,
) {
    if let Some(a) = d.anchors.get_mut(&target.entry) {
        a.1.retain(|id| !delta.removed.contains(id));
    }
    let created: Vec<CreatedNode> = delta
        .created
        .iter()
        .filter_map(|id| d.doc.node(id))
        .map(|n| CreatedNode {
            node_id: n.id.clone(),
            kind: n.kind.as_str(),
            row: None,
            col: None,
        })
        .collect();
    let entry = target.entry.clone();
    d.changed.extend(delta.created.iter().cloned());
    d.changed.extend(extra_changed);
    // PrvText has no line to add or remove for a new/deleted paragraph, so it is
    // reported stale only when paragraph text actually appeared or vanished.
    d.prv_extra |= text_changed;
    d.paragraph.push(ParagraphReport {
        command: command.into(),
        entry: entry.clone(),
        position: position.map(Position::as_str),
        count,
        created,
        removed: delta.removed,
    });
    d.intents.push(Intent::ParagraphStructure {
        command: command.into(),
        entry,
        path: scope,
        position: position.map(|p| p.as_str().to_string()),
        count,
    });
}

pub(crate) fn insert(
    d: &mut Draft,
    command: &str,
    target: &str,
    position: &str,
    text: Option<&str>,
    count: Option<usize>,
) -> Result<()> {
    let count = count.unwrap_or(1);
    if !(1..=MAX_COUNT).contains(&count) {
        return Err(invalid("count_out_of_range", "count must be 1..=1024"));
    }
    let position = Position::parse(position)?;
    let text = text.unwrap_or("");
    crate::text::check_new_text(text)?;
    let node = d.node(target, Some(Kind::Paragraph))?;
    gate(d.doc.paragraph_command_block_reason(command, &node))?;
    let xml = d.doc.doc_of(&node).clone();
    let place = place(&xml, node.el)?;
    if position == Position::Before
        && std::iter::once(node.el)
            .chain(xml.descendants(node.el))
            .any(|i| hp(&xml, i, "secPr"))
    {
        return Err(unsupported(
            "paragraph_before_section_property",
            "inserting before the paragraph that owns the section properties is unsupported",
        ));
    }
    let blob = blank_paragraph(&xml, node.el, text)?;
    let mut repeated = Vec::with_capacity(blob.len() * count);
    for _ in 0..count {
        repeated.extend_from_slice(&blob);
    }
    let at = match position {
        Position::Before => xml.el(node.el).outer().start,
        Position::After => xml.el(node.el).outer().end,
    };
    let first = place.index + usize::from(position == Position::After);
    let scope = place.scope;
    let remap = NodeRemap::Paragraphs(ParagraphRemap {
        entry: node.entry.clone(),
        scope: scope.clone(),
        children: (0..place.children.len())
            .map(|p| Some(if p >= first { p + count } else { p }))
            .collect(),
    });
    let in_table = table_of(d, &node);
    if in_table.is_none() {
        // Top-level: the new paragraphs have no cache and everything after the
        // target clears from here (design §5.4).
        d.anchor(&node);
    }
    let delta = d.splice_node_change(&node.entry, vec![(at..at, repeated)], &remap)?;
    if let Some(table) = in_table {
        anchor_enclosing_table(d, &table)?;
    }
    report(
        d,
        command,
        &node,
        Some(position),
        count,
        delta,
        vec![node.id.clone()],
        scope,
        !text.is_empty(),
    );
    Ok(())
}

pub(crate) fn delete(
    d: &mut Draft,
    command: &str,
    target: &str,
    count: Option<usize>,
) -> Result<()> {
    let count = count.unwrap_or(1);
    if !(1..=MAX_COUNT).contains(&count) {
        return Err(invalid("count_out_of_range", "count must be 1..=1024"));
    }
    let node = d.node(target, Some(Kind::Paragraph))?;
    // Same decision order as capability discovery (scope before content) so the
    // advertised reason and the execution reason always agree.
    if let Some(reason) = d.doc.paragraph_scope_block_reason(&node) {
        return Err(unsupported(
            &reason,
            "only section paragraphs and table cell paragraphs can be inserted or deleted",
        ));
    }
    if let Some((reason, Some(element))) = d.doc.paragraph_delete_content_violation(&node) {
        return Err(unsupported(
            reason,
            if reason == "paragraph_holds_table" {
                "a paragraph holding a table cannot be deleted"
            } else {
                "the paragraph holds objects, fields or notes that cannot be deleted"
            },
        )
        .with_detail(json!({"element": element})));
    }
    gate(d.doc.paragraph_command_block_reason(command, &node))?;
    let xml = d.doc.doc_of(&node).clone();
    let place = place(&xml, node.el)?;
    if place.index + count > place.children.len() {
        return Err(invalid(
            "paragraph_count_out_of_range",
            format!(
                "paragraphs {}..{} are outside the container's {} children",
                place.index,
                place.index + count,
                place.children.len()
            ),
        ));
    }
    let mut doomed = Vec::with_capacity(count);
    for p in place.index..place.index + count {
        let el = place.children[p];
        if !hp(&xml, el, "p") {
            return Err(unsupported(
                "paragraph_delete_not_contiguous",
                "the paragraphs to delete are not contiguous in their container",
            )
            .with_detail(json!({"element": xml.el(el).qname})));
        }
        doomed.push(el);
    }
    if place.children.iter().filter(|&&c| hp(&xml, c, "p")).count() <= count {
        return Err(unsupported(
            "paragraph_would_empty_container",
            "a container must keep at least one paragraph",
        ));
    }
    // Content rule: each doomed paragraph must pass the shared predicate.
    let mut text_changed = false;
    for &el in &doomed {
        text_changed |= std::iter::once(el)
            .chain(xml.descendants(el))
            .any(|i| hp(&xml, i, "t") && !xml.el(i).inner().is_empty());
        let n = d
            .doc
            .nodes
            .iter()
            .find(|n| n.entry == node.entry && n.el == el)
            .ok_or_else(|| {
                EngineError::new(
                    Code::StateInvariantError,
                    Stage::Apply,
                    "deleted paragraph has no node",
                )
            })?;
        if let Some((reason, element)) = d.doc.paragraph_delete_content_violation(n) {
            let e = unsupported(
                reason,
                if reason == "paragraph_holds_table" {
                    "a paragraph holding a table cannot be deleted"
                } else {
                    "the paragraph holds objects, fields or notes that cannot be deleted"
                },
            );
            return Err(match element {
                Some(element) => e.with_detail(json!({"element": element})),
                None => e,
            });
        }
    }
    let mut cuts: Vec<Cut> = Vec::new();
    for &el in &doomed {
        cuts.push((xml.el(el).outer(), Vec::new()));
    }
    let mut shift = 0;
    let scope = place.scope;
    let remap = NodeRemap::Paragraphs(ParagraphRemap {
        entry: node.entry.clone(),
        scope: scope.clone(),
        children: (0..place.children.len())
            .map(|p| {
                if p >= place.index && p < place.index + count {
                    shift += 1;
                    None
                } else {
                    Some(p - shift)
                }
            })
            .collect(),
    });
    let in_table = table_of(d, &node);
    // The paragraph that takes the deleted place is the shifted survivor.
    // The shifted survivor is the paragraph that takes the deleted place, or
    // the previous one when the range ended the container.
    let neighbour = place.children.get(place.index + count).or_else(|| {
        place
            .index
            .checked_sub(1)
            .and_then(|p| place.children.get(p))
    });
    let next: Option<String> = neighbour
        .and_then(|el| {
            d.doc
                .nodes
                .iter()
                .find(|n| n.entry == node.entry && n.el == *el)
        })
        .map(|n| n.id.clone());
    if in_table.is_none() {
        d.anchor(&node);
    }
    let delta = d.splice_node_change(&node.entry, cuts, &remap)?;
    if let Some(table) = in_table {
        anchor_enclosing_table(d, &table)?;
    }
    report(
        d,
        command,
        &node,
        None,
        count,
        delta,
        next.into_iter().collect(),
        scope,
        text_changed,
    );
    Ok(())
}
