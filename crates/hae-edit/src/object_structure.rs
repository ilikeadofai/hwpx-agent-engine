//! Object insertion and table deletion (0.4 object creation).
//!
//! This is the shared integration layer for `table.insert`, `image.insert`,
//! `equation.insert` and `table.delete`
//! (docs/decisions/report-object-creation-plan.md). It owns the carrier
//! paragraph, the declared node change ([`ObjectRemap`]), the lineseg cache
//! policy, the resource/intent report and the PrvText staleness flag. The
//! object XML itself comes from the dedicated builders, which never see a
//! carrier, a splice plan or a node map.
//!
//! Insertion is restricted to top-level section paragraphs; a nested anchor
//! (table cell, note, text box) is refused with an explicit reason instead of
//! being silently advertised. `table.delete` removes only the requested
//! `hp:tbl` outer bytes, preserving the carrier paragraph, the section
//! properties, sibling content and every existing media entry.
use crate::table::{gate, hp, unsupported};
use crate::{
    equation_insert, image_insert, table_insert, CreatedNode, Draft, Intent, ObjectReport,
    ResourceChange,
};
use hae_diag::{Code, EngineError, Result, Stage};
use hae_model::{
    identity_paths, structural_children, Document, Kind, Node, NodeRemap, ObjectRemap,
    StructureDelta,
};
use hae_package::PackageState;
use hae_xml::XmlDoc;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Position {
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
    fn as_str(self) -> &'static str {
        match self {
            Self::Before => "before",
            Self::After => "after",
        }
    }
}

/// Where an element sits among its container's structural children.
struct Place {
    scope: Vec<usize>,
    index: usize,
    children: Vec<usize>,
}

fn place(d: &XmlDoc, el: usize) -> Result<Place> {
    let parent = d.el(el).parent.ok_or_else(|| {
        unsupported(
            "object_scope_unsupported",
            "element has no structural parent",
        )
    })?;
    let children = structural_children(d, parent);
    let index = children.iter().position(|&c| c == el).ok_or_else(|| {
        unsupported(
            "object_scope_unsupported",
            "element is not a structural child of its container",
        )
    })?;
    Ok(Place {
        scope: hae_model::stable_path(d, parent),
        index,
        children,
    })
}

/// Insert `object` inside a new carrier paragraph cloned from `para`: the
/// target's start tag without its `id`, its first run's start tag verbatim, the
/// object content instead of `hp:t`, then the closing run and paragraph tags.
fn carrier(d: &XmlDoc, para: usize, object: &[u8]) -> Result<Vec<u8>> {
    let tpl = |m: &str| unsupported("object_template_unsupported", m);
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
        // Drop the id token and one adjacent space, mirroring paragraph.insert.
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
        // Self-closing template run: drop the `/>` and reopen the start tag
        // before the object content (a bare `<run attrs` is not a start tag).
        out.extend_from_slice(&run_start[..run_start.len() - 2]);
        out.push(b'>');
        out.extend_from_slice(object);
        out.extend_from_slice(format!("</{}>", d.el(run).qname).as_bytes());
    } else {
        out.extend_from_slice(&run_start);
        out.extend_from_slice(object);
        out.extend_from_slice(format!("</{}>", d.el(run).qname).as_bytes());
    }
    out.extend_from_slice(format!("</{}>", d.el(para).qname).as_bytes());
    Ok(out)
}

/// Shared capability gate plus the position-dependent refusals that are not
/// advertised (a `before` insert cannot cross the section-properties owner).
fn check_carrier(doc: &Document, node: &Node, position: Position) -> Result<()> {
    let xml = doc.doc_of(node);
    if xml.el(node.el).parent != Some(xml.root()) {
        return Err(unsupported(
            "object_scope_unsupported",
            "object insertion is supported only on top-level section paragraphs; \
             nested (table cell, note, text box) anchors are unsupported",
        ));
    }
    if position == Position::Before
        && std::iter::once(node.el)
            .chain(xml.descendants(node.el))
            .any(|i| hp(xml, i, "secPr"))
    {
        return Err(unsupported(
            "object_before_section_property",
            "inserting before the paragraph that owns the section properties is unsupported",
        ));
    }
    Ok(())
}

fn unauthorized_creation(path: &[usize], kind: Kind) -> EngineError {
    EngineError::new(
        Code::StateInvariantError,
        Stage::Apply,
        format!(
            "object insert declares an unauthorized {} at {:?}",
            kind.as_str(),
            path
        ),
    )
}

/// Shared carrier splice, declared node change, anchor and report for one
/// object insert. `state` may already carry a new media entry.
#[allow(clippy::too_many_arguments)]
fn insert_object(
    d: &mut Draft,
    command: &str,
    node: &Node,
    position: Position,
    object_kind: Kind,
    object: Vec<u8>,
    state: PackageState,
    resources: Vec<ResourceChange>,
) -> Result<()> {
    let xml = d.doc.doc_of(node).clone();
    let place = place(&xml, node.el)?;
    let blob = carrier(&xml, node.el, &object)?;
    let at = match position {
        Position::Before => xml.el(node.el).outer().start,
        Position::After => xml.el(node.el).outer().end,
    };
    let first = place.index + usize::from(position == Position::After);
    let scope = place.scope.clone();
    let children: Vec<Option<usize>> = (0..place.children.len())
        .map(|p| Some(if p >= first { p + 1 } else { p }))
        .collect();
    let new = xml.splice(vec![(at..at, blob)], d.opts())?;
    // Declare exactly the new identity elements: everything in the spliced
    // entry that an old node does not map onto.
    let mut change = ObjectRemap {
        entry: node.entry.clone(),
        scope: scope.clone(),
        children,
        created: Vec::new(),
        removed: Vec::new(),
    };
    let expected: BTreeSet<Vec<usize>> = d
        .doc
        .nodes
        .iter()
        .filter(|n| n.entry == node.entry)
        .filter_map(|n| change.map(&d.doc.stable_path_of(n)))
        .collect();
    // The declaration must not widen the model gate. The only new identity
    // elements allowed are the carrier paragraph, exactly one object of the
    // declared kind directly below it, and — for a table only — the cells and
    // their paragraphs below that root table. A nested picture/equation, a
    // second object, a section or an opaque element is an internal invariant
    // failure, never an authorized creation.
    let mut carrier_path = scope.clone();
    carrier_path.push(first);
    let depth = carrier_path.len();
    let table_nesting = object_kind == Kind::Table;
    let mut created: Vec<(Vec<usize>, Kind)> = Vec::new();
    let mut objects = 0usize;
    for (path, kind) in identity_paths(&new) {
        if expected.contains(&path) {
            continue;
        }
        if !path.starts_with(&carrier_path) {
            return Err(unauthorized_creation(&path, kind));
        }
        let is_carrier = path.len() == depth && kind == Kind::Paragraph;
        let is_object = path.len() == depth + 1 && kind == object_kind;
        if is_carrier {
            // The carrier paragraph itself.
        } else if is_object {
            objects += 1;
        } else if table_nesting
            && path.len() > depth + 1
            && matches!(kind, Kind::Cell | Kind::Paragraph)
        {
            // Cells and their paragraphs live below the one root table.
        } else {
            return Err(unauthorized_creation(&path, kind));
        }
        created.push((path, kind));
    }
    if objects != 1 {
        return Err(EngineError::new(
            Code::StateInvariantError,
            Stage::Apply,
            "object insert did not declare exactly one carrier object",
        ));
    }
    change.created = created;
    d.anchor(node);
    let delta =
        d.commit_node_change_state(&node.entry, &new, state, &NodeRemap::Objects(change))?;
    // An inserted object changes the textual preview even when it holds no
    // `hp:t` (its presence moves text), so PrvText is reported stale.
    report(
        d,
        command,
        node,
        Some(position),
        delta,
        vec![node.id.clone()],
        scope,
        resources,
        true,
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn report(
    d: &mut Draft,
    command: &str,
    node: &Node,
    position: Option<Position>,
    delta: StructureDelta,
    extra_changed: Vec<String>,
    scope: Vec<usize>,
    resources: Vec<ResourceChange>,
    text_changed: bool,
) {
    if let Some(a) = d.anchors.get_mut(&node.entry) {
        a.1.retain(|id| !delta.removed.contains(id));
    }
    let created: Vec<CreatedNode> = delta
        .created
        .iter()
        .filter_map(|id| d.doc.node(id))
        .map(|n| {
            let addr = (n.kind == Kind::Cell).then(|| d.doc.cell_addr(n)).flatten();
            CreatedNode {
                node_id: n.id.clone(),
                kind: n.kind.as_str(),
                row: addr.as_ref().map(|a| a.row as usize),
                col: addr.as_ref().map(|a| a.col as usize),
            }
        })
        .collect();
    d.changed.extend(delta.created.iter().cloned());
    d.changed.extend(delta.removed.iter().cloned());
    d.changed.extend(extra_changed);
    d.resources.extend(resources);
    // New object XML (with any cell text) has no PrvText line to edit.
    d.prv_extra |= text_changed;
    d.objects.push(ObjectReport {
        command: command.into(),
        entry: node.entry.clone(),
        position: position.map(Position::as_str),
        created,
        removed: delta.removed,
    });
    d.intents.push(Intent::ObjectStructure {
        command: command.into(),
        entry: node.entry.clone(),
        path: scope,
        position: position.map(|p| p.as_str().to_string()),
    });
}

/// `table.insert`: one rectangular table on a new carrier paragraph.
#[allow(clippy::too_many_arguments)]
pub(crate) fn insert_table(
    d: &mut Draft,
    command: &str,
    target: &str,
    position: &str,
    rows: u32,
    columns: u32,
    width: u32,
    row_height: u32,
    cells: &[Vec<String>],
) -> Result<()> {
    let position = Position::parse(position)?;
    let node = d.node(target, Some(Kind::Paragraph))?;
    gate(d.doc.object_insert_block_reason(&node))?;
    check_carrier(&d.doc, &node, position)?;
    let xml = d.doc.doc_of(&node).clone();
    let object = table_insert::build(
        &d.doc.state,
        &xml,
        node.el,
        rows,
        columns,
        width,
        row_height,
        cells,
    )?;
    insert_object(
        d,
        command,
        &node,
        position,
        Kind::Table,
        object,
        d.doc.state.clone(),
        Vec::new(),
    )
}

/// `equation.insert`: one native equation on a new carrier paragraph.
#[allow(clippy::too_many_arguments)]
pub(crate) fn insert_equation(
    d: &mut Draft,
    command: &str,
    target: &str,
    position: &str,
    script: &str,
    width: u32,
    height: u32,
) -> Result<()> {
    let position = Position::parse(position)?;
    let node = d.node(target, Some(Kind::Paragraph))?;
    gate(d.doc.object_insert_block_reason(&node))?;
    check_carrier(&d.doc, &node, position)?;
    let xml = d.doc.doc_of(&node).clone();
    let object = equation_insert::build(&d.doc.state, &xml, node.el, script, width, height)?;
    insert_object(
        d,
        command,
        &node,
        position,
        Kind::Equation,
        object,
        d.doc.state.clone(),
        Vec::new(),
    )
}

/// `image.insert`: one picture on a new carrier paragraph plus a new media
/// entry and manifest item (existing media is never modified).
pub(crate) fn insert_image(
    d: &mut Draft,
    command: &str,
    target: &str,
    position: &str,
    image: &crate::ImageBytes,
    width: u32,
    height: u32,
) -> Result<()> {
    let position = Position::parse(position)?;
    let node = d.node(target, Some(Kind::Paragraph))?;
    gate(d.doc.object_insert_block_reason(&node))?;
    check_carrier(&d.doc, &node, position)?;
    let xml = d.doc.doc_of(&node).clone();
    let mut state = d.doc.state.clone();
    let object = image_insert::prepare(
        &mut state,
        &xml,
        node.el,
        image,
        width,
        height,
        &d.doc.limits,
    )?;
    let mut resources = Vec::new();
    if let Some(id) = binary_item_id(&object) {
        resources.push(ResourceChange {
            kind: "media",
            resource_id: id.clone(),
            action: "added",
        });
        resources.push(ResourceChange {
            kind: "media",
            resource_id: id,
            action: "referenced",
        });
    }
    insert_object(
        d,
        command,
        &node,
        position,
        Kind::Picture,
        object,
        state,
        resources,
    )
}

/// The `hc:img/@binaryItemIDRef` of a freshly built `hp:pic` element.
fn binary_item_id(object: &[u8]) -> Option<String> {
    let doc = XmlDoc::parse(object.to_vec()).ok()?;
    let id = doc
        .iter()
        .find(|(_, e)| e.local == "img")
        .and_then(|(_, e)| e.attr_value("binaryItemIDRef"))
        .map(str::to_string);
    id
}

/// `table.delete`: remove only the requested table's XML subtree.
pub(crate) fn delete_table(d: &mut Draft, command: &str, target: &str) -> Result<()> {
    let node = d.node(target, Some(Kind::Table))?;
    gate(d.doc.object_delete_block_reason(command, &node))?;
    let xml = d.doc.doc_of(&node).clone();
    let carrier_el = xml
        .ancestors(node.el)
        .find(|&i| hp(&xml, i, "p"))
        .ok_or_else(|| {
            unsupported(
                "object_delete_scope_unsupported",
                "table has no paragraph carrier",
            )
        })?;
    if xml.el(carrier_el).parent != Some(xml.root()) {
        return Err(unsupported(
            "object_delete_nested_unsupported",
            "deleting a table nested inside a cell is unsupported because its node map is unsafe",
        ));
    }
    let carrier_node = d
        .doc
        .nodes
        .iter()
        .find(|n| n.entry == node.entry && n.el == carrier_el)
        .cloned()
        .ok_or_else(|| {
            EngineError::new(
                Code::StateInvariantError,
                Stage::Apply,
                "table carrier has no node",
            )
        })?;
    let inside = structural_children(&xml, carrier_el);
    let k = inside.iter().position(|&c| c == node.el).ok_or_else(|| {
        unsupported(
            "object_delete_scope_unsupported",
            "table is not a structural child of its carrier paragraph",
        )
    })?;
    let scope = hae_model::stable_path(&xml, carrier_el);
    // Old position -> new position: the removed child's slot is gone, so every
    // later structural sibling of the carrier shifts down by one.
    let children: Vec<Option<usize>> = (0..inside.len())
        .map(|p| match p.cmp(&k) {
            std::cmp::Ordering::Less => Some(p),
            std::cmp::Ordering::Equal => None,
            std::cmp::Ordering::Greater => Some(p - 1),
        })
        .collect();
    let removed_root = hae_model::stable_path(&xml, node.el);
    let new = xml.splice(vec![(xml.el(node.el).outer(), Vec::new())], d.opts())?;
    let change = NodeRemap::Objects(ObjectRemap {
        entry: node.entry.clone(),
        scope: scope.clone(),
        children,
        created: Vec::new(),
        removed: vec![removed_root],
    });
    d.anchor(&carrier_node);
    let state = d.doc.state.clone();
    let delta = d.commit_node_change_state(&node.entry, &new, state, &change)?;
    report(
        d,
        command,
        &carrier_node,
        None,
        delta,
        Vec::new(),
        scope,
        Vec::new(),
        true,
    );
    Ok(())
}
