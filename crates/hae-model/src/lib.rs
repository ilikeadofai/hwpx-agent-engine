//! hae-model: Canonical document state, nodeId map, element index and query views.
//!
//! The canonical state is the current [`PackageState`] plus the nodeId map
//! (spec §4, §5). Everything else (parsed XML, the node index, paragraph text
//! views) is derived and rebuilt by [`Document::rebuild`], which is also the
//! commit gate: an edit may change payloads but never the sequence of
//! identity-bearing elements of an entry. The one exception is a table
//! structure edit, which declares a [`TableRemap`] and goes through
//! [`Document::rebuild_table_structure`] (docs/decisions/table-structure-node-creation-design.md).

pub mod annotations;
pub mod equation;
pub mod export_text;
pub mod fields;
pub mod links;
pub mod owpml;
pub mod para;
pub mod query;
pub mod resource_query;
pub mod trackchange;
pub mod validate;

use hae_diag::{Code, EngineError, Limits, Result, Stage};
use hae_package::{PackageInfo, PackageState, XmlMap, NS_HC, NS_HH, NS_HP, NS_HP10, NS_HS};
use hae_xml::XmlDoc;
use hae_zip::Archive;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

pub use para::{ParaText, Piece, PieceKind};

// Text carriers are transparent to identity paths: formatting may split runs/t,
// while identity-bearing children must retain ancestry and document order.
fn carrier(doc: &XmlDoc, i: usize) -> bool {
    is_hp(doc.el(i).ns.as_deref()) && matches!(doc.el(i).local.as_str(), "run" | "t")
}
/// Structural children of one element, with text carriers (`hp:run`/`hp:t`)
/// transparent. This is the sequence `stable_path` counts positions in, and it
/// is public so the editor can compute the same child positions when it
/// declares a [`ParagraphRemap`].
pub fn structural_children(doc: &XmlDoc, i: usize) -> Vec<usize> {
    let mut out = Vec::new();
    for &c in &doc.el(i).children {
        if carrier(doc, c) {
            out.extend(structural_children(doc, c));
        } else {
            out.push(c);
        }
    }
    out
}
/// Stable path (run/t transparent) of one element. This is the coordinate
/// system `TableRemap`/`ParagraphRemap` declare scopes in.
pub fn stable_path(doc: &XmlDoc, mut i: usize) -> Vec<usize> {
    let mut path = Vec::new();
    while let Some(mut p) = doc.el(i).parent {
        while carrier(doc, p) {
            p = doc.el(p).parent.expect("carrier has structural parent");
        }
        path.push(
            structural_children(doc, p)
                .iter()
                .position(|&c| c == i)
                .unwrap(),
        );
        i = p;
    }
    path.reverse();
    path
}

/// Kinds of elements that receive a nodeId (spec §5).
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    PartialOrd,
    Ord,
    schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Section,
    Paragraph,
    Table,
    Cell,
    Picture,
    Equation,
    Opaque,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Section => "section",
            Kind::Paragraph => "paragraph",
            Kind::Table => "table",
            Kind::Cell => "cell",
            Kind::Picture => "picture",
            Kind::Equation => "equation",
            Kind::Opaque => "opaque",
        }
    }
}

pub fn is_hp(ns: Option<&str>) -> bool {
    matches!(ns, Some(NS_HP) | Some(NS_HP10))
}

/// Namespaces the engine understands; elements in other namespaces are opaque.
pub fn is_known_ns(ns: Option<&str>) -> bool {
    matches!(
        ns,
        Some(NS_HP) | Some(NS_HP10) | Some(NS_HS) | Some(NS_HH) | Some(NS_HC)
    )
}

/// Classify an element of a section document.
fn kind_of(doc: &XmlDoc, i: usize) -> Option<Kind> {
    let el = doc.el(i);
    let ns = el.ns.as_deref();
    if i == doc.root() {
        return (el.local == "sec" && ns == Some(NS_HS)).then_some(Kind::Section);
    }
    if is_hp(ns) {
        return match el.local.as_str() {
            "p" => Some(Kind::Paragraph),
            "tbl" => Some(Kind::Table),
            "tc" => Some(Kind::Cell),
            "pic" => Some(Kind::Picture),
            "equation" => Some(Kind::Equation),
            _ => None,
        };
    }
    // Opaque container: the outermost element of a foreign namespace.
    if !is_known_ns(ns) {
        let parent_known = el
            .parent
            .is_none_or(|p| is_known_ns(doc.el(p).ns.as_deref()));
        if parent_known {
            return Some(Kind::Opaque);
        }
    }
    None
}

/// Identity elements of one section document in document order.
fn identity_elements(doc: &XmlDoc) -> Vec<(usize, Kind)> {
    doc.iter()
        .filter_map(|(i, _)| kind_of(doc, i).map(|k| (i, k)))
        .collect()
}

/// Declared node-set change of one table (docs/decisions/table-structure-node-creation-design.md §3).
/// All indexes are structural child positions as counted by `stable_path`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TableRemap {
    pub entry: String,
    /// Stable path of the `hp:tbl`; the table itself keeps this path.
    pub table: Vec<usize>,
    /// Old structural child position of the table → new position (`None`: removed).
    pub children: Vec<Option<usize>>,
    /// Old position of a `tr` → (old position of its `tc` → new position, `None`: removed).
    pub cells: BTreeMap<usize, Vec<Option<usize>>>,
}

impl TableRemap {
    /// Path an old node is expected to have after the change; `None` if removed.
    pub fn map(&self, path: &[usize]) -> Option<Vec<usize>> {
        let n = self.table.len();
        if path.len() <= n || path[..n] != self.table[..] {
            return Some(path.to_vec());
        }
        let first = path[n];
        let mut out = path[..n].to_vec();
        out.push((*self.children.get(first)?)?);
        if let (Some(&second), Some(row)) = (path.get(n + 1), self.cells.get(&first)) {
            out.push((*row.get(second)?)?);
            out.extend_from_slice(&path[n + 2..]);
        } else {
            out.extend_from_slice(&path[n + 1..]);
        }
        Some(out)
    }

    fn contains(&self, path: &[usize]) -> bool {
        path.len() > self.table.len() && path[..self.table.len()] == self.table[..]
    }
}

/// Declared change of one paragraph sequence (docs/decisions/document-paragraph-creation-design.md §3).
///
/// `scope` is the stable path of the container whose structural children move:
/// the section root (`[]`) for top-level paragraphs, or
/// `[.., tbl, tr, tc, subList]` for a cell's paragraphs. `children` maps old
/// structural child positions of that container onto their new positions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParagraphRemap {
    pub entry: String,
    /// Stable path of the changed container; it keeps this path itself.
    pub scope: Vec<usize>,
    /// Old structural child position of the container → new position (`None`: removed).
    pub children: Vec<Option<usize>>,
}

impl ParagraphRemap {
    /// Path an old node is expected to have after the change; `None` if removed.
    pub fn map(&self, path: &[usize]) -> Option<Vec<usize>> {
        let n = self.scope.len();
        if path.len() <= n || path[..n] != self.scope[..] {
            return Some(path.to_vec());
        }
        let mut out = path[..n].to_vec();
        out.push((*self.children.get(path[n])?)?);
        out.extend_from_slice(&path[n + 1..]);
        Some(out)
    }

    fn contains(&self, path: &[usize]) -> bool {
        path.len() > self.scope.len() && path[..self.scope.len()] == self.scope[..]
    }
}

/// What a structure edit declares to the commit gate. 0.3 opened the contract
/// for one table's rows/columns; 0.4 adds a paragraph sequence without widening
/// what may be created (`Paragraph` only).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeRemap {
    Table(TableRemap),
    Paragraphs(ParagraphRemap),
}

impl NodeRemap {
    pub fn entry(&self) -> &str {
        match self {
            Self::Table(r) => &r.entry,
            Self::Paragraphs(r) => &r.entry,
        }
    }

    /// Path an old node is expected to have after the change; `None` if removed.
    pub fn map(&self, path: &[usize]) -> Option<Vec<usize>> {
        match self {
            Self::Table(r) => r.map(path),
            Self::Paragraphs(r) => r.map(path),
        }
    }

    fn contains(&self, path: &[usize]) -> bool {
        match self {
            Self::Table(r) => r.contains(path),
            Self::Paragraphs(r) => r.contains(path),
        }
    }

    /// Kinds this declaration may create inside its scope.
    fn creates(&self, kind: Kind) -> bool {
        match self {
            Self::Table(_) => matches!(kind, Kind::Cell | Kind::Paragraph),
            Self::Paragraphs(_) => kind == Kind::Paragraph,
        }
    }
}

/// Ids created and removed by a structure change, in document order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StructureDelta {
    pub created: Vec<String>,
    pub removed: Vec<String>,
}

#[allow(clippy::type_complexity)]
fn structural_ids(
    old_doc: &XmlDoc,
    old: &[&Node],
    new_doc: &XmlDoc,
    new: &[(usize, Kind)],
    change: &NodeRemap,
) -> Result<(Vec<String>, Vec<String>, Vec<String>)> {
    let inv = |m: String| EngineError::new(Code::StateInvariantError, Stage::Commit, m);
    let mut expected: HashMap<Vec<usize>, (Kind, &str)> = HashMap::new();
    let mut removed = Vec::new();
    for n in old {
        let path = stable_path(old_doc, n.el);
        match change.map(&path) {
            Some(mapped) => {
                if expected.insert(mapped, (n.kind, n.id.as_str())).is_some() {
                    return Err(inv(format!(
                        "structure change maps two nodes onto one path ({})",
                        n.id
                    )));
                }
            }
            None => {
                if !change.contains(&path) || !change.creates(n.kind) {
                    return Err(inv(format!(
                        "structure change removes {} {} outside its declared scope",
                        n.kind.as_str(),
                        n.id
                    )));
                }
                removed.push(n.id.clone());
            }
        }
    }
    let mut ids = Vec::with_capacity(new.len());
    let mut created = Vec::new();
    for &(i, k) in new {
        let path = stable_path(new_doc, i);
        match expected.remove(&path) {
            Some((ko, id)) if ko == k => ids.push(id.to_string()),
            Some((ko, id)) => {
                return Err(inv(format!(
                    "node {id} changed kind {} -> {}",
                    ko.as_str(),
                    k.as_str()
                )))
            }
            None => {
                if !change.contains(&path) || !change.creates(k) {
                    return Err(inv(format!(
                        "undeclared new {} element at {:?}",
                        k.as_str(),
                        path
                    )));
                }
                let id = new_id("n-")?;
                created.push(id.clone());
                ids.push(id);
            }
        }
    }
    if let Some((_, (_, id))) = expected.iter().next() {
        return Err(inv(format!(
            "node {id} has no counterpart after the structure change"
        )));
    }
    Ok((ids, created, removed))
}

/// One node of the index.
#[derive(Debug, Clone)]
pub struct Node {
    pub id: String,
    pub kind: Kind,
    pub entry: String,
    /// Element index in the entry's current [`XmlDoc`].
    pub el: usize,
    pub parent: Option<String>,
    /// Section number (spine order) the node belongs to.
    pub section: usize,
    /// Why this node may not be edited, if anything.
    pub locked: Option<String>,
}

/// Persistent map key (spec §5): entry, structural ordinal path, kind.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct MapKey {
    pub entry: String,
    pub path: Vec<usize>,
    pub kind: Kind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapRow {
    #[serde(flatten)]
    pub key: MapKey,
    pub id: String,
}

/// Document-level edit permission (spec §2, §4).
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Permissions {
    /// Reasons the whole document is query/export-only. Empty means editable.
    pub readonly_reasons: Vec<String>,
}

impl Permissions {
    pub fn editable(&self) -> bool {
        self.readonly_reasons.is_empty()
    }
}

pub fn new_id(prefix: &str) -> Result<String> {
    let mut b = [0u8; 9];
    getrandom::fill(&mut b).map_err(|e| {
        EngineError::new(
            Code::IoError,
            Stage::Open,
            format!("random source failed: {e}"),
        )
    })?;
    Ok(format!(
        "{prefix}{}",
        b.iter().map(|x| format!("{x:02x}")).collect::<String>()
    ))
}

/// The open document: canonical state plus derived views.
#[derive(Debug, Clone)]
pub struct Document {
    pub document_id: String,
    pub original: Arc<Archive>,
    pub state: PackageState,
    pub info: PackageInfo,
    pub xml: XmlMap,
    pub nodes: Vec<Node>,
    by_id: HashMap<String, usize>,
    pub permissions: Permissions,
    pub limits: Limits,
    /// Section entries whose layout cache was invalidated by edits.
    pub stale_layout: Vec<String>,
    /// Cached discovery precondition; recomputed on every rebuild, not per node.
    invalid_references: bool,
}

fn mimetype_stored(a: &Archive) -> bool {
    a.entries().first().is_some_and(|e| e.is_stored())
}

impl Document {
    /// Open HWPX bytes as a new document with fresh ids.
    pub fn open(bytes: Vec<u8>, limits: &Limits) -> Result<Document> {
        let (archive, state, info, xml) = hae_package::open(bytes, limits)?;
        let document_id = new_id("doc-")?;
        let mut ids: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for s in &info.sections {
            let n = identity_elements(&xml[s]).len();
            ids.insert(
                s.clone(),
                (0..n).map(|_| new_id("n-")).collect::<Result<_>>()?,
            );
        }
        Self::assemble(
            document_id,
            archive,
            state,
            info,
            xml,
            &ids,
            *limits,
            Vec::new(),
        )
    }

    /// Re-open with a persisted id map (project reopen). Every identity element
    /// must match exactly one row by (entry, path, kind) and vice versa.
    pub fn open_with_map(
        document_id: String,
        original: Arc<Archive>,
        state: PackageState,
        rows: &[MapRow],
        limits: &Limits,
    ) -> Result<Document> {
        let integrity = |m: String| EngineError::new(Code::ProjectIntegrityError, Stage::Open, m);
        let (info, xml) = hae_package::analyze(&state, mimetype_stored(&original), limits)?;
        let mut by_key: HashMap<&MapKey, &str> = HashMap::new();
        let mut seen_ids = HashSet::new();
        for r in rows {
            if !seen_ids.insert(r.id.as_str()) {
                return Err(integrity(format!("duplicate nodeId {}", r.id)));
            }
            if by_key.insert(&r.key, &r.id).is_some() {
                return Err(integrity("duplicate map key".into()));
            }
        }
        let mut ids: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut used = 0usize;
        for s in &info.sections {
            let doc = &xml[s];
            let mut v = Vec::new();
            for (i, k) in identity_elements(doc) {
                let key = MapKey {
                    entry: s.clone(),
                    path: doc.path(i),
                    kind: k,
                };
                let id = by_key.get(&key).ok_or_else(|| {
                    integrity(format!("no map row for {s} {:?} {}", key.path, k.as_str()))
                })?;
                v.push(id.to_string());
                used += 1;
            }
            ids.insert(s.clone(), v);
        }
        if used != rows.len() {
            return Err(integrity(format!(
                "{} map rows do not match any element",
                rows.len() - used
            )));
        }
        Self::assemble(
            document_id,
            original,
            state,
            info,
            xml,
            &ids,
            *limits,
            Vec::new(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn assemble(
        document_id: String,
        original: Arc<Archive>,
        state: PackageState,
        info: PackageInfo,
        xml: XmlMap,
        ids: &BTreeMap<String, Vec<String>>,
        limits: Limits,
        stale_layout: Vec<String>,
    ) -> Result<Document> {
        let mut nodes = Vec::new();
        for (si, s) in info.sections.iter().enumerate() {
            let doc = &xml[s];
            let elems = identity_elements(doc);
            let sid = &ids[s];
            if sid.len() != elems.len() {
                return Err(EngineError::new(
                    Code::StateInvariantError,
                    Stage::Commit,
                    format!("{s}: {} identity elements, {} ids", elems.len(), sid.len()),
                ));
            }
            let mut el_to_id: HashMap<usize, &str> = HashMap::new();
            for ((i, k), id) in elems.iter().zip(sid) {
                el_to_id.insert(*i, id);
                let parent = doc
                    .ancestors(*i)
                    .find_map(|a| el_to_id.get(&a).map(|s| s.to_string()));
                nodes.push(Node {
                    id: id.clone(),
                    kind: *k,
                    entry: s.clone(),
                    el: *i,
                    parent,
                    section: si,
                    locked: None,
                });
            }
        }
        let by_id: HashMap<String, usize> = nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.clone(), i))
            .collect();
        if by_id.len() != nodes.len() {
            return Err(EngineError::new(
                Code::StateInvariantError,
                Stage::Commit,
                "duplicate nodeId",
            ));
        }
        let invalid_references = references_invalid(&info, &xml);
        let mut d = Document {
            document_id,
            original,
            state,
            info,
            xml,
            nodes,
            by_id,
            permissions: Permissions::default(),
            limits,
            stale_layout,
            invalid_references,
        };
        d.compute_permissions();
        Ok(d)
    }

    /// Build a candidate document from a new package state, carrying ids over.
    /// This is the commit gate (`validate_candidate_state`, spec §4): the new
    /// state must analyze cleanly and keep each section's identity sequence.
    pub fn rebuild(&self, state: PackageState, stale_layout: Vec<String>) -> Result<Document> {
        self.rebuild_inner(state, stale_layout, None)
            .map(|(d, _)| d)
    }

    /// Commit gate for a table structure edit. `change` declares exactly which
    /// rows/cells of one table moved or were removed; every other node must
    /// keep its id and stable path. Elements without a surviving counterpart
    /// must be `Cell`/`Paragraph` below the declared table and receive fresh
    /// ids (never a reused one).
    pub fn rebuild_table_structure(
        &self,
        state: PackageState,
        stale_layout: Vec<String>,
        change: &TableRemap,
    ) -> Result<(Document, StructureDelta)> {
        self.rebuild_node_structure(state, stale_layout, &NodeRemap::Table(change.clone()))
    }

    /// Commit gate for declared node-set changes (0.3 tables, 0.4 paragraph
    /// sequences): every node outside the declaration keeps its id and stable
    /// path, and only the declared kinds may appear or disappear inside the
    /// declared scope (docs/decisions/document-paragraph-creation-design.md §3).
    pub fn rebuild_node_structure(
        &self,
        state: PackageState,
        stale_layout: Vec<String>,
        change: &NodeRemap,
    ) -> Result<(Document, StructureDelta)> {
        let (d, delta) = self.rebuild_inner(state, stale_layout, Some(change))?;
        Ok((d, delta.unwrap_or_default()))
    }

    fn rebuild_inner(
        &self,
        state: PackageState,
        stale_layout: Vec<String>,
        change: Option<&NodeRemap>,
    ) -> Result<(Document, Option<StructureDelta>)> {
        let inv = |m: String| EngineError::new(Code::StateInvariantError, Stage::Commit, m);
        let (info, xml) = hae_package::analyze_cached(
            &state,
            mimetype_stored(&self.original),
            &self.limits,
            Some(&self.xml),
        )
        .map_err(|e| inv(format!("candidate package invalid: {e}")))?;
        if info.sections != self.info.sections {
            return Err(inv("section list changed".into()));
        }
        if let Some(c) = change {
            if !info.sections.contains(&c.entry().to_string()) {
                return Err(inv(format!(
                    "structure change names unknown entry {}",
                    c.entry()
                )));
            }
        }
        let mut delta = change.map(|_| StructureDelta::default());
        let mut ids: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for s in &info.sections {
            let old: Vec<&Node> = self.nodes.iter().filter(|n| &n.entry == s).collect();
            let new = identity_elements(&xml[s]);
            match change.filter(|c| c.entry() == s) {
                None => {
                    if old.len() != new.len()
                        || old.iter().zip(&new).any(|(o, (_, kn))| o.kind != *kn)
                    {
                        return Err(inv(format!("{s}: identity elements changed")));
                    }
                    ids.insert(s.clone(), old.iter().map(|n| n.id.clone()).collect());
                }
                Some(c) => {
                    let (v, created, removed) =
                        structural_ids(&self.xml[s], &old, &xml[s], &new, c)?;
                    ids.insert(s.clone(), v);
                    if let Some(dl) = delta.as_mut() {
                        dl.created = created;
                        dl.removed = removed;
                    }
                }
            }
        }
        let d = Self::assemble(
            self.document_id.clone(),
            self.original.clone(),
            state,
            info,
            xml,
            &ids,
            self.limits,
            stale_layout,
        )?;
        // Identity ancestry/order is unchanged modulo text-carrier insertion and
        // splitting. The declared entry was already compared path by path.
        let skip = change.map(|c| c.entry().to_string());
        if skip.is_none() {
            for (a, b) in self.nodes.iter().zip(&d.nodes) {
                if stable_path(&self.xml[&a.entry], a.el) != stable_path(&d.xml[&b.entry], b.el) {
                    return Err(inv(format!("node {} moved", a.id)));
                }
            }
        } else {
            let new_by_id: HashMap<&str, &Node> =
                d.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
            for a in self
                .nodes
                .iter()
                .filter(|n| Some(n.entry.as_str()) != skip.as_deref())
            {
                let b = new_by_id
                    .get(a.id.as_str())
                    .ok_or_else(|| inv(format!("node {} vanished", a.id)))?;
                if stable_path(&self.xml[&a.entry], a.el) != stable_path(&d.xml[&b.entry], b.el) {
                    return Err(inv(format!("node {} moved", a.id)));
                }
            }
        }
        Ok((d, delta))
    }

    /// Stable path (run/t transparent) of a node, as used by [`TableRemap`].
    pub fn stable_path_of(&self, n: &Node) -> Vec<usize> {
        stable_path(&self.xml[&n.entry], n.el)
    }

    pub fn node(&self, id: &str) -> Option<&Node> {
        self.by_id.get(id).map(|&i| &self.nodes[i])
    }

    pub fn require(&self, id: &str, kind: Option<Kind>) -> Result<&Node> {
        let n = self.node(id).ok_or_else(|| {
            EngineError::new(
                Code::TargetNotFound,
                Stage::Validate,
                format!("no node {id}"),
            )
            .with_target(id)
        })?;
        if let Some(k) = kind {
            if n.kind != k {
                return Err(EngineError::new(
                    Code::Unsupported,
                    Stage::Validate,
                    format!("node {id} is a {}, not a {}", n.kind.as_str(), k.as_str()),
                )
                .with_target(id)
                .with_reason("wrong_kind"));
            }
        }
        Ok(n)
    }

    pub fn doc_of(&self, n: &Node) -> &Arc<XmlDoc> {
        &self.xml[&n.entry]
    }

    pub fn header(&self) -> Option<&Arc<XmlDoc>> {
        self.info.header.as_ref().and_then(|h| self.xml.get(h))
    }

    /// The persistent id map (spec §5).
    pub fn map_rows(&self) -> Vec<MapRow> {
        self.nodes
            .iter()
            .map(|n| MapRow {
                key: MapKey {
                    entry: n.entry.clone(),
                    path: self.xml[&n.entry].path(n.el),
                    kind: n.kind,
                },
                id: n.id.clone(),
            })
            .collect()
    }

    /// Children nodes of `id` in document order.
    pub fn children(&self, id: &str) -> impl Iterator<Item = &Node> {
        let id = id.to_string();
        self.nodes
            .iter()
            .filter(move |n| n.parent.as_deref() == Some(id.as_str()))
    }

    /// Edit permission checks for one node (spec §4).
    pub fn check_editable(&self, n: &Node) -> Result<()> {
        if !self.permissions.editable() {
            return Err(EngineError::new(
                Code::Unsupported,
                Stage::Validate,
                format!(
                    "document is read-only: {}",
                    self.permissions.readonly_reasons.join(", ")
                ),
            )
            .with_target(&n.id)
            .with_reason("document_readonly"));
        }
        if let Some(r) = &n.locked {
            return Err(EngineError::new(
                Code::Unsupported,
                Stage::Validate,
                format!("node {} is locked: {r}", n.id),
            )
            .with_target(&n.id)
            .with_reason(r.clone()));
        }
        Ok(())
    }

    fn compute_permissions(&mut self) {
        let mut reasons = Vec::new();
        if !self.info.signature_entries.is_empty() {
            reasons.push("signed".to_string());
        }
        let track_elems = ["insertBegin", "insertEnd", "deleteBegin", "deleteEnd"];
        let mut tracked = false;
        for s in &self.info.sections {
            if self.xml[s]
                .iter()
                .any(|(_, e)| is_hp(e.ns.as_deref()) && track_elems.contains(&e.local.as_str()))
            {
                tracked = true;
            }
        }
        if let Some(h) = self.header() {
            if h.iter().any(|(_, e)| {
                e.ns.as_deref() == Some(NS_HH)
                    && (e.local == "trackChange" || e.local == "trackChangeAuthor")
            }) {
                tracked = true;
            }
        }
        if tracked {
            reasons.push("track_changes".to_string());
        }
        self.permissions.readonly_reasons = reasons;

        // Opaque elements referring to an identity element's HWPX id lock that node.
        let mut hwpx_ids: HashMap<(String, String), Vec<usize>> = HashMap::new();
        for (ni, n) in self.nodes.iter().enumerate() {
            if let Some(v) = self.xml[&n.entry].el(n.el).attr_value("id") {
                if !v.is_empty() {
                    hwpx_ids
                        .entry((n.entry.clone(), v.to_string()))
                        .or_default()
                        .push(ni);
                }
            }
        }
        let mut locks: Vec<(usize, String)> = Vec::new();
        for n in self.nodes.iter().filter(|n| n.kind == Kind::Opaque) {
            let doc = &self.xml[&n.entry];
            let mut els = vec![n.el];
            els.extend(doc.descendants(n.el));
            for e in els {
                for a in &doc.el(e).attrs {
                    if let Some(targets) = hwpx_ids.get(&(n.entry.clone(), a.value.clone())) {
                        for &t in targets {
                            locks.push((t, format!("opaque_reference:{}", n.id)));
                        }
                    }
                }
            }
        }
        // Content with an opaque child also cannot be safely edited as text.
        for (ni, n) in self.nodes.iter().enumerate() {
            if n.kind == Kind::Opaque {
                if let Some(p) = n.parent.as_ref().and_then(|p| self.by_id.get(p)) {
                    locks.push((*p, "contains_opaque".to_string()));
                }
                let _ = ni;
            }
        }
        for (t, r) in locks {
            if self.nodes[t].locked.is_none() {
                self.nodes[t].locked = Some(r);
            }
        }
    }

    /// Paragraph text view for a paragraph node.
    pub fn para_text(&self, n: &Node) -> Result<ParaText> {
        if n.kind != Kind::Paragraph {
            return Err(
                EngineError::new(Code::Unsupported, Stage::Validate, "not a paragraph")
                    .with_target(&n.id),
            );
        }
        Ok(ParaText::build(self.doc_of(n), n.el))
    }

    /// Payload of a package entry.
    pub fn payload(&self, name: &str) -> Option<&Arc<[u8]>> {
        self.state.get(name)
    }

    /// Bytes of the current HWPX (raw-copy write).
    pub fn to_hwpx(&self) -> Result<Vec<u8>> {
        let plan = hae_package::zip_plan(&self.original, &self.state)?;
        hae_zip::write(&self.original, &plan)
    }

    /// True if the current payload set equals the original's (spec §11).
    pub fn is_unmodified(&self) -> bool {
        let orig = PackageState::from_archive(&self.original);
        orig.payload_set_sha256() == self.state.payload_set_sha256()
    }
}

pub const NS_CORE: &str = NS_HC;

// Deliberately independent of hae-verify::validate_refs. Executor property tests
// cross-check discovery against that gate without adding a verifier dependency
// on the mutation planner/model.
fn references_invalid(info: &PackageInfo, xml: &XmlMap) -> bool {
    let Some(header) = info.header.as_ref().and_then(|h| xml.get(h)) else {
        return false;
    };
    for (definition, reference) in [
        ("paraPr", "paraPrIDRef"),
        ("charPr", "charPrIDRef"),
        ("style", "styleIDRef"),
        ("borderFill", "borderFillIDRef"),
    ] {
        let mut ids = HashSet::new();
        for (_, e) in header.iter().filter(|(_, e)| e.is(NS_HH, definition)) {
            if let Some(id) = e.attr_value("id") {
                if !ids.insert(id) {
                    return true;
                }
            }
        }
        if info.sections.iter().any(|s| {
            xml[s]
                .iter()
                .any(|(_, e)| e.attr_value(reference).is_some_and(|r| !ids.contains(r)))
        }) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod export_text_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod validate_tests;
