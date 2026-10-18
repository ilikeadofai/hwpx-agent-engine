//! Read-only resource and structure views. XML values are reported, never repaired.
use crate::{is_hp, Document, Kind, Node};
use hae_diag::{Code, EngineError, Result, Stage};
use hae_package::{sha256_hex, NS_HC, NS_HH};
use hae_xml::{Element, XmlDoc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use unicode_segmentation::UnicodeSegmentation;

pub type Attributes = BTreeMap<String, String>;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
pub enum ResourceKind {
    #[serde(rename = "style")]
    Style,
    #[serde(rename = "charPr")]
    CharPr,
    #[serde(rename = "paraPr")]
    ParaPr,
    #[serde(rename = "borderFill")]
    BorderFill,
}
impl ResourceKind {
    fn from_local(local: &str) -> Option<Self> {
        match local {
            "style" => Some(Self::Style),
            "charPr" => Some(Self::CharPr),
            "paraPr" => Some(Self::ParaPr),
            "borderFill" => Some(Self::BorderFill),
            _ => None,
        }
    }
    fn reference(attr: &str) -> Option<Self> {
        match attr {
            "styleIDRef" | "nextStyleIDRef" => Some(Self::Style),
            "charPrIDRef" => Some(Self::CharPr),
            "paraPrIDRef" => Some(Self::ParaPr),
            "borderFillIDRef" => Some(Self::BorderFill),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FontReference {
    pub language: String,
    pub id: String,
    pub face: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StyleResource {
    pub kind: ResourceKind,
    pub id: Option<String>,
    /// Literal XML reference occurrences, including references in header resources.
    pub use_count: usize,
    pub body_reference_count: usize,
    pub other_reference_count: usize,
    pub attributes: Attributes,
    pub fonts: Vec<FontReference>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    /// All branches (including switch/case/default), in XML order.
    pub alignments: Vec<Attributes>,
    pub line_spacings: Vec<Attributes>,
    pub borders: BTreeMap<String, Attributes>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UnresolvedReference {
    pub kind: ResourceKind,
    pub id: String,
    pub entry: String,
    pub element: String,
    pub attribute: String,
    pub node_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StyleInventory {
    pub items: Vec<StyleResource>,
    /// Across all four kinds, regardless of the page/kind filter.
    pub unresolved_reference_count: usize,
    pub unresolved_body_reference_count: usize,
    /// First 20 occurrences, body sections first (spine order), then other XML.
    pub unresolved_examples: Vec<UnresolvedReference>,
    pub unresolved_examples_truncated: bool,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CellAddress {
    pub row: Option<u32>,
    pub col: Option<u32>,
    pub row_span: Option<u32>,
    pub col_span: Option<u32>,
}
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TableCell {
    pub node_id: String,
    pub address: CellAddress,
    pub size: Option<Attributes>,
    pub margins: Option<Attributes>,
    pub has_margin: Option<String>,
    pub border_fill_id_ref: Option<String>,
    pub paragraph_count: usize,
    pub text_preview: String,
    /// Other explicit anchors whose spans cover this actual cell address.
    pub covered_by: Vec<String>,
}
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CoveredCell {
    pub row: u32,
    pub col: u32,
    pub covered_by: Vec<String>,
    /// Nonempty on overlapping/inconsistent XML; no winner is inferred.
    pub present_cell_node_ids: Vec<String>,
}
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TableView {
    pub node_id: String,
    pub parent: Option<String>,
    pub section: usize,
    pub rows: Option<u32>,
    pub cols: Option<u32>,
    pub cell_spacing: Option<i64>,
    pub outer_margins: Option<Attributes>,
    pub inner_margins: Option<Attributes>,
    pub border_fill_id_ref: Option<String>,
    pub cells: Vec<TableCell>,
    /// Derived only from explicit, complete cellAddr/cellSpan, excluding anchors.
    pub covered_cells: Vec<CoveredCell>,
}
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PictureMedia {
    pub item_id: Option<String>,
    pub href: Option<String>,
    pub entry: Option<String>,
    pub sha256: Option<String>,
    /// Literal manifest (or img) isEmbeded value, including absent/unknown.
    pub is_embeded: Option<String>,
    /// None when no embedding flag exists; no network access is performed.
    pub external: Option<bool>,
    pub status: String,
    /// Document-wide, including pictures outside the requested scope.
    pub other_picture_node_ids: Vec<String>,
}
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PictureView {
    pub node_id: String,
    pub paragraph_node_id: Option<String>,
    pub section: usize,
    pub position: Option<Attributes>,
    pub offset: Option<Attributes>,
    pub size: Option<Attributes>,
    pub org_sz: Option<Attributes>,
    pub cur_sz: Option<Attributes>,
    pub img_clip: Option<Attributes>,
    pub img_dim: Option<Attributes>,
    pub cropped: Option<bool>,
    pub media: PictureMedia,
    pub caption_text: Option<String>,
    pub has_effects: bool,
    pub has_shadow: bool,
    pub effect_properties: Option<Attributes>,
    pub effect_elements: Vec<EffectElement>,
    pub flip: Option<Attributes>,
    pub flipped: Option<bool>,
    pub reverse: Option<String>,
}
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct EffectElement {
    pub element: String,
    pub attributes: Attributes,
}

fn attrs(e: &Element) -> Attributes {
    e.attrs
        .iter()
        .filter(|a| a.ns.is_none())
        .map(|a| (a.local.clone(), a.value.clone()))
        .collect()
}
fn child<'a>(x: &'a XmlDoc, el: usize, local: &str) -> Option<&'a Element> {
    x.el(el)
        .children
        .iter()
        .map(|&c| x.el(c))
        .find(|e| is_hp(e.ns.as_deref()) && e.local == local)
}
fn child_attrs(x: &XmlDoc, el: usize, local: &str) -> Option<Attributes> {
    child(x, el, local).map(attrs)
}
fn value(e: &Element, attr: &str) -> Option<String> {
    e.attr_value(attr).map(str::to_owned)
}
fn number<T: std::str::FromStr>(e: &Element, attr: &str) -> Option<T> {
    e.attr_value(attr)?.parse().ok()
}
fn boolean(s: &str) -> Option<bool> {
    match s {
        "1" | "true" => Some(true),
        "0" | "false" => Some(false),
        _ => None,
    }
}
fn preview(s: &str) -> String {
    s.graphemes(true)
        .take(crate::query::PREVIEW_GRAPHEMES)
        .collect()
}
fn resource_limit(message: &str) -> EngineError {
    EngineError::new(Code::ResourceLimit, Stage::Query, message)
}

impl Document {
    /// Counts actual attribute occurrences in known HWPX namespaces; this is not
    /// a count of visually formatted characters or transitive style use.
    pub fn style_inventory(&self, kind: Option<ResourceKind>) -> StyleInventory {
        let header = self.info.header.as_ref().and_then(|h| self.xml.get(h));
        let definitions: Vec<_> = header
            .into_iter()
            .flat_map(|h| h.iter())
            .filter_map(|(index, e)| {
                (e.ns.as_deref() == Some(NS_HH))
                    .then(|| ResourceKind::from_local(&e.local))
                    .flatten()
                    .map(|k| (k, index, e))
            })
            .collect();
        let mut counts: BTreeMap<(ResourceKind, String), (usize, usize)> = BTreeMap::new();
        for (k, _, e) in &definitions {
            if let Some(id) = e.attr_value("id") {
                counts.entry((*k, id.into())).or_default();
            }
        }
        let mut unresolved_reference_count = 0;
        let mut unresolved_body_reference_count = 0;
        let mut unresolved_examples = Vec::new();
        // Body defects must remain visible even when the header already has
        // more than twenty unresolved references.
        let entries = self
            .info
            .sections
            .iter()
            .filter_map(|entry| self.xml.get_key_value(entry))
            .chain(
                self.xml
                    .iter()
                    .filter(|(entry, _)| !self.info.sections.contains(entry)),
            );
        for (entry, xml) in entries {
            let body = self.info.sections.contains(entry);
            for (el, e) in xml.iter() {
                if !matches!(e.ns.as_deref(), Some(NS_HH) | Some(NS_HC)) && !is_hp(e.ns.as_deref())
                {
                    continue;
                }
                for a in e.attrs.iter().filter(|a| a.ns.is_none()) {
                    let Some(k) = ResourceKind::reference(&a.local) else {
                        continue;
                    };
                    if let Some(count) = counts.get_mut(&(k, a.value.clone())) {
                        if body {
                            count.0 += 1;
                        } else {
                            count.1 += 1;
                        }
                    } else {
                        unresolved_reference_count += 1;
                        unresolved_body_reference_count += usize::from(body);
                        if unresolved_examples.len() < 20 {
                            let node_id =
                                std::iter::once(el).chain(xml.ancestors(el)).find_map(|i| {
                                    self.nodes
                                        .iter()
                                        .find(|n| n.entry == *entry && n.el == i)
                                        .map(|n| n.id.clone())
                                });
                            unresolved_examples.push(UnresolvedReference {
                                kind: k,
                                id: a.value.clone(),
                                entry: entry.clone(),
                                element: e.local.clone(),
                                attribute: a.local.clone(),
                                node_id,
                            });
                        }
                    }
                }
            }
        }
        let mut fonts = BTreeMap::new();
        if let Some(h) = header {
            for (_, face) in h.iter().filter(|(_, e)| e.is(NS_HH, "fontface")) {
                if let Some(lang) = face.attr_value("lang") {
                    for f in face
                        .children
                        .iter()
                        .map(|&c| h.el(c))
                        .filter(|e| e.is(NS_HH, "font"))
                    {
                        if let (Some(id), Some(name)) = (f.attr_value("id"), f.attr_value("face")) {
                            fonts.insert(
                                (lang.to_ascii_lowercase(), id.to_string()),
                                name.to_string(),
                            );
                        }
                    }
                }
            }
        }
        let items = definitions
            .into_iter()
            .filter(|(k, _, _)| kind.is_none_or(|filter| filter == *k))
            .map(|(k, index, e)| {
                let (body, other) = e
                    .attr_value("id")
                    .and_then(|id| counts.get(&(k, id.into())))
                    .copied()
                    .unwrap_or_default();
                let h = header.expect("definitions require header");
                let descendants: Vec<_> = h
                    .descendants(index)
                    .map(|i| h.el(i))
                    .filter(|e| e.ns.as_deref() == Some(NS_HH))
                    .collect();
                let font_refs = descendants
                    .iter()
                    .filter(|e| e.local == "fontRef")
                    .flat_map(|e| attrs(e))
                    .map(|(lang, id)| FontReference {
                        face: fonts.get(&(lang.to_ascii_lowercase(), id.clone())).cloned(),
                        language: lang,
                        id,
                    })
                    .collect();
                StyleResource {
                    kind: k,
                    id: value(e, "id"),
                    use_count: body + other,
                    body_reference_count: body,
                    other_reference_count: other,
                    attributes: attrs(e),
                    fonts: font_refs,
                    bold: (k == ResourceKind::CharPr)
                        .then(|| descendants.iter().any(|e| e.local == "bold")),
                    italic: (k == ResourceKind::CharPr)
                        .then(|| descendants.iter().any(|e| e.local == "italic")),
                    alignments: descendants
                        .iter()
                        .filter(|e| e.local == "align")
                        .map(|e| attrs(e))
                        .collect(),
                    line_spacings: descendants
                        .iter()
                        .filter(|e| e.local == "lineSpacing")
                        .map(|e| attrs(e))
                        .collect(),
                    borders: descendants
                        .iter()
                        .filter(|e| {
                            matches!(
                                e.local.as_str(),
                                "leftBorder"
                                    | "rightBorder"
                                    | "topBorder"
                                    | "bottomBorder"
                                    | "diagonal"
                                    | "slash"
                                    | "backSlash"
                            )
                        })
                        .map(|e| (e.local.clone(), attrs(e)))
                        .collect(),
                }
            })
            .collect();
        StyleInventory {
            items,
            unresolved_reference_count,
            unresolved_body_reference_count,
            unresolved_examples_truncated: unresolved_reference_count > unresolved_examples.len(),
            unresolved_examples,
        }
    }

    /// Validate scope before filtering, including on an empty document.
    pub fn structure_scope(&self, scope: Option<&str>) -> Result<()> {
        if let Some(id) = scope {
            let n = self.require(id, None)?;
            if !matches!(n.kind, Kind::Section | Kind::Paragraph) {
                return Err(EngineError::new(
                    Code::InvalidRequest,
                    Stage::Query,
                    "scope must identify a section or paragraph",
                ));
            }
        }
        Ok(())
    }
    pub fn structure_nodes(&self, kind: Kind, scope: Option<&str>) -> Vec<&Node> {
        self.nodes
            .iter()
            .filter(|n| n.kind == kind && scope.is_none_or(|id| self.is_descendant(n, id)))
            .collect()
    }

    pub fn table_view(&self, n: &Node, budget: usize) -> Result<TableView> {
        let x = self.doc_of(n);
        let e = x.el(n.el);
        let cells: Vec<_> = self
            .children(&n.id)
            .filter(|c| c.kind == Kind::Cell)
            .map(|c| {
                let addr = child(x, c.el, "cellAddr");
                let span = child(x, c.el, "cellSpan");
                let paras: Vec<_> = self
                    .children(&c.id)
                    .filter(|p| p.kind == Kind::Paragraph)
                    .collect();
                TableCell {
                    node_id: c.id.clone(),
                    address: CellAddress {
                        row: addr.and_then(|e| number(e, "rowAddr")),
                        col: addr.and_then(|e| number(e, "colAddr")),
                        row_span: span.and_then(|e| number(e, "rowSpan")),
                        col_span: span.and_then(|e| number(e, "colSpan")),
                    },
                    size: child_attrs(x, c.el, "cellSz"),
                    margins: child_attrs(x, c.el, "cellMargin"),
                    has_margin: value(x.el(c.el), "hasMargin"),
                    border_fill_id_ref: value(x.el(c.el), "borderFillIDRef"),
                    paragraph_count: paras.len(),
                    text_preview: preview(
                        &paras
                            .iter()
                            .map(|p| self.node_text(p))
                            .collect::<Vec<_>>()
                            .join("\n"),
                    ),
                    covered_by: vec![],
                }
            })
            .collect();
        let mut covered: BTreeMap<(u32, u32), Vec<String>> = BTreeMap::new();
        let mut work = 0u64;
        for c in &cells {
            let a = &c.address;
            if let (Some(row), Some(col), Some(rs), Some(cs)) =
                (a.row, a.col, a.row_span, a.col_span)
            {
                work = work.saturating_add(u64::from(rs) * u64::from(cs));
                if work > (budget / 64) as u64 {
                    return Err(resource_limit(
                        "table cell spans exceed response/work budget",
                    ));
                }
                let end_row = row
                    .checked_add(rs)
                    .ok_or_else(|| resource_limit("cell row span overflow"))?;
                let end_col = col
                    .checked_add(cs)
                    .ok_or_else(|| resource_limit("cell column span overflow"))?;
                for r in row..end_row {
                    for col2 in col..end_col {
                        if (r, col2) != (row, col) {
                            covered
                                .entry((r, col2))
                                .or_default()
                                .push(c.node_id.clone());
                        }
                    }
                }
            }
        }
        let mut cells = cells;
        for c in &mut cells {
            if let (Some(r), Some(col)) = (c.address.row, c.address.col) {
                c.covered_by = covered.get(&(r, col)).cloned().unwrap_or_default();
            }
        }
        let mut present: BTreeMap<(u32, u32), Vec<String>> = BTreeMap::new();
        for cell in &cells {
            if let (Some(row), Some(col)) = (cell.address.row, cell.address.col) {
                present
                    .entry((row, col))
                    .or_default()
                    .push(cell.node_id.clone());
            }
        }
        let covered_cells = covered
            .into_iter()
            .map(|((row, col), covered_by)| CoveredCell {
                row,
                col,
                covered_by,
                present_cell_node_ids: present.remove(&(row, col)).unwrap_or_default(),
            })
            .collect();
        Ok(TableView {
            node_id: n.id.clone(),
            parent: n.parent.clone(),
            section: n.section,
            rows: number(e, "rowCnt"),
            cols: number(e, "colCnt"),
            cell_spacing: number(e, "cellSpacing"),
            outer_margins: child_attrs(x, n.el, "outMargin"),
            inner_margins: child_attrs(x, n.el, "inMargin"),
            border_fill_id_ref: value(e, "borderFillIDRef"),
            cells,
            covered_cells,
        })
    }

    fn picture_img<'a>(&'a self, n: &Node) -> Option<&'a Element> {
        let x = self.doc_of(n);
        // Do not borrow an image from a nested picture/caption.
        x.descendants(n.el)
            .find(|&i| {
                x.el(i).is(NS_HC, "img")
                    && x.ancestors(i)
                        .find(|&a| is_hp(x.el(a).ns.as_deref()) && x.el(a).local == "pic")
                        == Some(n.el)
            })
            .map(|i| x.el(i))
    }

    pub fn picture_view(&self, n: &Node) -> PictureView {
        let x = self.doc_of(n);
        let e = x.el(n.el);
        let img = self.picture_img(n);
        let id = img.and_then(|e| e.attr_value("binaryItemIDRef"));
        let item = id.and_then(|id| self.info.item_by_id(id));
        let manifest = self.xml.get(&self.info.rootfile).and_then(|h| {
            h.iter().map(|(_, e)| e).find(|e| {
                matches!(
                    e.ns.as_deref(),
                    Some("http://www.idpf.org/2007/opf/") | Some("http://www.idpf.org/2007/opf")
                ) && e.local == "item"
                    && id.is_some()
                    && e.attr_value("id") == id
            })
        });
        let is_embeded = manifest
            .and_then(|e| value(e, "isEmbeded"))
            .or_else(|| img.and_then(|e| value(e, "isEmbeded")));
        let external = is_embeded
            .as_deref()
            .and_then(boolean)
            .map(|embedded| !embedded);
        let payload = item.and_then(|item| self.payload(&item.entry));
        let other_picture_node_ids = self
            .nodes
            .iter()
            .filter(|p| p.kind == Kind::Picture && p.id != n.id)
            .filter(|p| {
                let other_id = self
                    .picture_img(p)
                    .and_then(|e| e.attr_value("binaryItemIDRef"));
                id.is_some()
                    && (id == other_id
                        || item.is_some_and(|item| {
                            other_id
                                .and_then(|id| self.info.item_by_id(id))
                                .is_some_and(|other| {
                                    (item.is_embedded
                                        && other.is_embedded
                                        && other.entry == item.entry)
                                        || (!item.is_embedded
                                            && !other.is_embedded
                                            && other.href == item.href)
                                })
                        }))
            })
            .map(|p| p.id.clone())
            .collect();
        let clip = child(x, n.el, "imgClip");
        let dim = child(x, n.el, "imgDim");
        let cropped = clip.zip(dim).and_then(|(c, d)| {
            Some(
                number::<i64>(c, "left")? != 0
                    || number::<i64>(c, "top")? != 0
                    || number::<i64>(c, "right")? != number::<i64>(d, "dimwidth")?
                    || number::<i64>(c, "bottom")? != number::<i64>(d, "dimheight")?,
            )
        });
        let caption = e
            .children
            .iter()
            .copied()
            .find(|&i| is_hp(x.el(i).ns.as_deref()) && x.el(i).local == "caption");
        let caption_text = caption.map(|cap| {
            self.nodes
                .iter()
                .filter(|p| {
                    p.entry == n.entry
                        && p.kind == Kind::Paragraph
                        && x.ancestors(p.el).any(|a| a == cap)
                })
                .map(|p| self.node_text(p))
                .collect::<Vec<_>>()
                .join("\n")
        });
        let mut has_effects = false;
        let mut has_shadow = false;
        let effect_elements = x
            .descendants(n.el)
            .filter_map(|i| {
                let d = x.el(i);
                if x.ancestors(i)
                    .find(|&a| is_hp(x.el(a).ns.as_deref()) && x.el(a).local == "pic")
                    != Some(n.el)
                {
                    return None;
                }
                if (!is_hp(d.ns.as_deref()) && d.ns.as_deref() != Some(NS_HC))
                    || !matches!(
                        d.local.as_str(),
                        "effects" | "shadow" | "glow" | "softEdge" | "reflection"
                    )
                {
                    return None;
                }
                let active = if d.local == "effects" {
                    !d.attrs.is_empty()
                        || !d.children.is_empty()
                        || d.items
                            .iter()
                            .filter_map(|i| i.text())
                            .any(|t| !t.trim().is_empty())
                } else {
                    d.attr_value("type") != Some("NONE")
                };
                has_effects |= active;
                has_shadow |= d.local == "shadow" && active;
                Some(EffectElement {
                    element: d.local.clone(),
                    attributes: attrs(d),
                })
            })
            .collect();
        has_effects |= img.is_some_and(|e| {
            e.attr_value("effect").is_some_and(|v| v != "REAL_PIC")
                || ["bright", "contrast", "alpha"]
                    .iter()
                    .any(|k| e.attr_value(k).is_some_and(|v| v != "0"))
        });
        let flip = child(x, n.el, "flip");
        let flipped = flip.and_then(|e| {
            Some(boolean(e.attr_value("horizontal")?)? || boolean(e.attr_value("vertical")?)?)
        });
        let paragraph_node_id = x.ancestors(n.el).find_map(|i| {
            self.nodes
                .iter()
                .find(|p| p.entry == n.entry && p.el == i && p.kind == Kind::Paragraph)
                .map(|p| p.id.clone())
        });
        PictureView {
            node_id: n.id.clone(),
            paragraph_node_id,
            section: n.section,
            position: child_attrs(x, n.el, "pos"),
            offset: child_attrs(x, n.el, "offset"),
            size: child_attrs(x, n.el, "sz"),
            org_sz: child_attrs(x, n.el, "orgSz"),
            cur_sz: child_attrs(x, n.el, "curSz"),
            img_clip: clip.map(attrs),
            img_dim: dim.map(attrs),
            cropped,
            media: PictureMedia {
                item_id: id.map(str::to_owned),
                href: manifest
                    .and_then(|e| value(e, "href"))
                    .or_else(|| img.and_then(|e| value(e, "href"))),
                entry: item.filter(|i| i.is_embedded).map(|i| i.entry.clone()),
                sha256: payload.map(|p| sha256_hex(p)),
                is_embeded,
                external,
                status: if external == Some(true) {
                    "external"
                } else if payload.is_some() {
                    "payload_available"
                } else {
                    "unresolved"
                }
                .into(),
                other_picture_node_ids,
            },
            caption_text,
            has_effects,
            has_shadow,
            effect_properties: img.map(attrs),
            effect_elements,
            flip: flip.map(attrs),
            flipped,
            reverse: value(e, "reverse"),
        }
    }
}
