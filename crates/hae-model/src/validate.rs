//! `document.validate`: read-only comparison against the public OWPML
//! dictionary (catalog MT-13).
//!
//! The check describes the open document; it never edits, repairs or refuses.
//! Element and attribute names come from Hancom's public OWPML model
//! ([`crate::owpml`]); reference integrity, resource counts and package members
//! are derived from the package itself. Report severities follow the evidence
//! recorded in `docs/decisions/document-validate-2026-10-19.md`:
//!
//! - `error` — a required package member is absent, a declared count disagrees
//!   with the elements present, or one resource id is defined twice. The save
//!   gate refuses all three while editing, and none of them occurs in the
//!   corpus.
//! - `warning` — an element name the model does not declare, or a reference to
//!   a resource the header does not define. Real Hancom documents do both
//!   (`hh:layoutCompatibility` children in two corpus templates; a `styleIDRef`
//!   without a style in `anchor_char_height.hwpx`), so they are reported, not
//!   refused. A package without a header part (`irb-sample.hwpx`) is reported
//!   the same way, and the reference checks are then marked unchecked.
//! - `info` — out-of-coverage observations: a namespace the model declares but
//!   fills with nothing, an attribute the model's readers never name, the
//!   `4294967295` "no reference" sentinel, and defined-but-unreferenced
//!   resources.

use crate::owpml::{Dictionary, Element, SOURCE, SOURCE_LICENSE};
use crate::{is_hp, Document};
use hae_diag::{Code, EngineError, Result, Stage};
use hae_package::{PackageInfo, NS_HC, NS_HH};
use hae_xml::XmlDoc;
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

/// Distinct findings one scan may accumulate before it stops adding new ones.
pub const MAX_FINDINGS: usize = 2048;
/// Default number of examples kept per finding.
pub const DEFAULT_EXAMPLES: usize = 5;
pub const MAX_EXAMPLES: usize = 50;

pub const CODE_UNKNOWN_ELEMENT: &str = "unknown_element";
pub const CODE_UNKNOWN_ATTRIBUTE: &str = "unknown_attribute";
pub const CODE_UNKNOWN_NAMESPACE: &str = "unknown_namespace";
pub const CODE_MISSING_PACKAGE_ITEM: &str = "missing_package_item";
pub const CODE_UNRESOLVED_REFERENCE: &str = "unresolved_reference";
pub const CODE_UNSET_REFERENCE: &str = "unset_reference";
pub const CODE_COUNT_MISMATCH: &str = "count_mismatch";
pub const CODE_DUPLICATE_RESOURCE_ID: &str = "duplicate_resource_id";
pub const CODE_UNUSED_RESOURCE: &str = "unused_resource";

/// The package's own mimetype value (spec §11).
const MIMETYPE: &str = "application/hwp+zip";
/// The model initialises an unset reference to `(UINT)-1`; six of the eleven
/// corpus documents write it for `hh:paraHead@charPrIDRef`.
const UNSET_REFERENCE: &str = "4294967295";

/// Reference attribute -> resource kind: the five attributes `style.list`
/// counts as literal XML references.
const REFERENCES: &[(&str, &str)] = &[
    ("charPrIDRef", "charPr"),
    ("paraPrIDRef", "paraPr"),
    ("styleIDRef", "style"),
    ("nextStyleIDRef", "style"),
    ("borderFillIDRef", "borderFill"),
];

/// Resource kinds whose definition/usage mismatch is reported.
const RESOURCE_KINDS: &[&str] = &["charPr", "paraPr", "style", "borderFill"];

/// Count attributes are compared with the number of children the same model
/// class declares (`hh:charProperties@itemCnt`, `hh:fontface@fontCnt`).
const COUNT_ATTRIBUTES: &[&str] = &["itemCnt", "fontCnt"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        }
    }
    fn rank(self) -> u8 {
        match self {
            Severity::Error => 0,
            Severity::Warning => 1,
            Severity::Info => 2,
        }
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Example {
    pub entry: String,
    /// `prefix:name[n]` segments from the entry root, at most 6 levels.
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub code: String,
    pub severity: Severity,
    pub reason: String,
    pub message: String,
    /// Occurrences behind this finding; findings aggregate by name or target.
    pub count: u64,
    /// `document`, or the scope nodeId when the scoped scan produced it.
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub element: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attribute: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    pub examples: Vec<Example>,
    pub examples_truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<Value>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CodeCount {
    pub code: String,
    pub severity: Severity,
    pub findings: usize,
    pub issues: u64,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub errors: usize,
    pub warnings: usize,
    pub infos: usize,
    pub findings: usize,
    pub issues: u64,
    pub by_code: Vec<CodeCount>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NamespaceUse {
    /// Model prefix, or the document's own prefix when the model has none.
    pub prefix: Option<String>,
    pub uri: String,
    pub elements: u64,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DictionaryInfo {
    pub project: String,
    pub license: String,
    pub source_revision: String,
    pub extractor_version: String,
    pub fingerprint: String,
    pub elements: usize,
    pub namespaces: usize,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Checked {
    /// XML entries whose elements were visited (header included).
    pub xml_entries: usize,
    /// XML element visits across all passes; bounded by `max_validate_elements`.
    pub elements: u64,
    /// Attributes examined against the model's attribute sets.
    pub attributes: u64,
    /// Reference attributes (`charPrIDRef`, ...) examined.
    pub references: u64,
    pub namespaces: Vec<NamespaceUse>,
    /// Required package members that were confirmed present.
    pub package_items: Vec<String>,
    /// Header entry, `null` when the package has none.
    pub header: Option<String>,
    /// False when the package has no header, so references cannot be resolved.
    pub reference_integrity: bool,
    pub resource_definitions: bool,
    pub dictionary: DictionaryInfo,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub items: Vec<Finding>,
    pub summary: Summary,
    pub checked: Checked,
    /// True when the scan stopped adding new findings at [`MAX_FINDINGS`];
    /// the per-finding `count` values stop at the cut-off too.
    pub truncated: bool,
}

fn query_error(message: impl Into<String>) -> EngineError {
    EngineError::new(Code::InvalidRequest, Stage::Query, message)
}

/// A finding before it is aggregated.
struct Draft {
    code: &'static str,
    severity: Severity,
    reason: &'static str,
    message: String,
    namespace: Option<String>,
    prefix: Option<String>,
    element: Option<String>,
    attribute: Option<String>,
    resource_kind: Option<String>,
    value: Option<String>,
    detail: Option<Value>,
}

impl Draft {
    fn new(code: &'static str, severity: Severity, reason: &'static str, message: String) -> Draft {
        Draft {
            code,
            severity,
            reason,
            message,
            namespace: None,
            prefix: None,
            element: None,
            attribute: None,
            resource_kind: None,
            value: None,
            detail: None,
        }
    }
    fn ns(mut self, uri: &str) -> Draft {
        self.namespace = Some(uri.to_string());
        self
    }
    fn prefix(mut self, prefix: &str) -> Draft {
        self.prefix = Some(prefix.to_string());
        self
    }
    fn element(mut self, element: &str) -> Draft {
        self.element = Some(element.to_string());
        self
    }
    fn attribute(mut self, attribute: &str) -> Draft {
        self.attribute = Some(attribute.to_string());
        self
    }
    fn kind(mut self, kind: &str) -> Draft {
        self.resource_kind = Some(kind.to_string());
        self
    }
    fn value(mut self, value: &str) -> Draft {
        self.value = Some(value.to_string());
        self
    }
    fn detail(mut self, detail: Value) -> Draft {
        self.detail = Some(detail);
        self
    }
}

struct Scanner<'a> {
    doc: &'a Document,
    dict: &'a Dictionary,
    max_examples: usize,
    limit: u64,
    scope: String,
    findings: BTreeMap<String, Finding>,
    elements: u64,
    attributes: u64,
    references: u64,
    namespaces: BTreeMap<String, NamespaceUse>,
    entries: BTreeSet<String>,
    /// `(kind, id)` -> number of header definitions.
    definitions: BTreeMap<(String, String), u64>,
    /// First definition site of each `(kind, id)`, for examples.
    definition_sites: BTreeMap<(String, String), Example>,
    /// `(kind, id)` -> literal reference count across the document.
    used: BTreeMap<(String, String), u64>,
    truncated: bool,
}

impl<'a> Scanner<'a> {
    fn new(doc: &'a Document, dict: &'a Dictionary, scope: String, max_examples: usize) -> Self {
        Scanner {
            doc,
            dict,
            max_examples,
            limit: doc.limits.max_validate_elements,
            scope,
            findings: BTreeMap::new(),
            elements: 0,
            attributes: 0,
            references: 0,
            namespaces: BTreeMap::new(),
            entries: BTreeSet::new(),
            definitions: BTreeMap::new(),
            definition_sites: BTreeMap::new(),
            used: BTreeMap::new(),
            truncated: false,
        }
    }

    fn add(&mut self, key: String, draft: Draft, example: Option<Example>, scope: &str) {
        if let Some(f) = self.findings.get_mut(&key) {
            f.count += 1;
            if let Some(example) = example {
                if f.examples.len() < self.max_examples {
                    f.examples.push(example);
                } else {
                    f.examples_truncated = true;
                }
            }
            return;
        }
        if self.grow() {
            return;
        }
        let mut f = Finding {
            code: draft.code.to_string(),
            severity: draft.severity,
            reason: draft.reason.to_string(),
            message: draft.message,
            count: 1,
            scope: scope.to_string(),
            namespace: draft.namespace,
            prefix: draft.prefix,
            element: draft.element,
            attribute: draft.attribute,
            resource_kind: draft.resource_kind,
            value: draft.value,
            examples: Vec::new(),
            examples_truncated: false,
            detail: draft.detail,
        };
        if let Some(example) = example {
            f.examples.push(example);
        }
        self.findings.insert(key, f);
    }

    /// `add` under the request scope: the element/attribute scan may be
    /// narrowed to a scope node, so those findings carry its id.
    fn add_scoped(&mut self, key: String, draft: Draft, example: Option<Example>) {
        let scope = self.scope.clone();
        self.add(key, draft, example, &scope);
    }

    /// One finding covering `count` occurrences (used for aggregates).
    fn add_many(
        &mut self,
        key: String,
        draft: Draft,
        count: u64,
        examples: Vec<Example>,
        scope: &str,
    ) {
        if self.grow() {
            return;
        }
        let examples_truncated = count > examples.len() as u64;
        self.findings.insert(
            key,
            Finding {
                code: draft.code.to_string(),
                severity: draft.severity,
                reason: draft.reason.to_string(),
                message: draft.message,
                count,
                scope: scope.to_string(),
                namespace: draft.namespace,
                prefix: draft.prefix,
                element: draft.element,
                attribute: draft.attribute,
                resource_kind: draft.resource_kind,
                value: draft.value,
                examples,
                examples_truncated,
                detail: draft.detail,
            },
        );
    }

    /// True when the finding cap is reached (the caller then drops the row).
    fn grow(&mut self) -> bool {
        if self.findings.len() >= MAX_FINDINGS {
            self.truncated = true;
            return true;
        }
        false
    }

    /// Count one XML element visit and refuse past `max_validate_elements`.
    fn visit(&mut self) -> Result<()> {
        self.elements += 1;
        if self.elements > self.limit {
            return Err(EngineError::new(
                Code::ResourceLimit,
                Stage::Query,
                format!(
                    "document.validate visits more than max_validate_elements = {} XML elements; narrow the scope or raise the limit",
                    self.limit
                ),
            )
            .with_reason("validate_scan_limit")
            .with_detail(json!({
                "limit": "max_validate_elements",
                "limitElements": self.limit,
                "visitedElements": self.elements,
            })));
        }
        Ok(())
    }

    fn note_namespace(&mut self, uri: &str, prefix: Option<&str>) {
        let entry = self
            .namespaces
            .entry(uri.to_string())
            .or_insert(NamespaceUse {
                prefix: prefix.map(str::to_owned),
                uri: uri.to_string(),
                elements: 0,
            });
        entry.elements += 1;
        if entry.prefix.is_none() {
            entry.prefix = prefix.map(str::to_owned);
        }
    }

    fn node_for(&self, entry: &str, xml: &XmlDoc, el: usize) -> Option<String> {
        std::iter::once(el).chain(xml.ancestors(el)).find_map(|i| {
            self.doc
                .nodes
                .iter()
                .find(|n| n.entry == entry && n.el == i)
                .map(|n| n.id.clone())
        })
    }

    fn example(&self, entry: &str, xml: &XmlDoc, el: usize) -> Example {
        Example {
            entry: entry.to_string(),
            path: path_of(xml, el),
            node_id: self.node_for(entry, xml, el),
        }
    }

    /// Element and attribute comparison for one subtree.
    fn scan_tree(&mut self, entry: &str, xml: &XmlDoc, root: usize) -> Result<()> {
        self.entries.insert(entry.to_string());
        let mut stack = vec![root];
        while let Some(el) = stack.pop() {
            self.visit()?;
            let e = xml.el(el);
            let uri = e.ns.as_deref().unwrap_or("");
            let prefix = self.dict.prefix_for(uri).map(str::to_owned);
            self.note_namespace(uri, prefix.as_deref());
            match prefix.as_deref().filter(|p| self.dict.covered(p)) {
                Some(prefix) => match self.dict.element(prefix, &e.local) {
                    Some(known) => {
                        for a in e.attrs.iter().filter(|a| a.ns.is_none()) {
                            if a.local == "xmlns" || a.local.starts_with("xmlns:") {
                                continue;
                            }
                            self.attributes += 1;
                            if known.attributes.contains(&a.local) {
                                continue;
                            }
                            let message = format!(
                                "{prefix}:{}@{}, used here, is not named by any OWPML model reader",
                                e.local, a.local
                            );
                            let example = self.example(entry, xml, el);
                            self.add_scoped(
                                format!("attribute|{uri}|{}|{}", e.local, a.local),
                                Draft::new(
                                    CODE_UNKNOWN_ATTRIBUTE,
                                    Severity::Info,
                                    "attribute_not_in_owpml_model",
                                    message,
                                )
                                .ns(uri)
                                .prefix(prefix)
                                .element(&e.local)
                                .attribute(&a.local),
                                Some(example),
                            );
                        }
                    }
                    None => {
                        let message =
                            format!("{prefix}:{} is not declared by the OWPML model", e.local);
                        let example = self.example(entry, xml, el);
                        self.add_scoped(
                            format!("element|{uri}|{}", e.local),
                            Draft::new(
                                CODE_UNKNOWN_ELEMENT,
                                Severity::Warning,
                                "element_not_in_owpml_model",
                                message,
                            )
                            .ns(uri)
                            .prefix(prefix)
                            .element(&e.local),
                            Some(example),
                        );
                    }
                },
                None => {
                    let message = format!(
                        "namespace {uri} has no element names in the OWPML model; names in it are not compared"
                    );
                    let example = self.example(entry, xml, el);
                    self.add_scoped(
                        format!("namespace|{uri}"),
                        Draft::new(
                            CODE_UNKNOWN_NAMESPACE,
                            Severity::Info,
                            "namespace_not_in_owpml_model",
                            message,
                        )
                        .ns(uri)
                        .detail(json!({ "modelElements": 0, "sampleElement": e.local })),
                        Some(example),
                    );
                }
            }
            for &c in e.children.iter().rev() {
                stack.push(c);
            }
        }
        Ok(())
    }

    /// `itemCnt`/`fontCnt` against the children the same class declares.
    fn check_counts(
        &mut self,
        entry: &str,
        xml: &XmlDoc,
        el: usize,
        prefix: &str,
        known: &Element,
    ) -> Result<()> {
        let e = xml.el(el);
        for attribute in COUNT_ATTRIBUTES {
            if !known.attributes.contains(*attribute) || known.children.is_empty() {
                continue;
            }
            let Some(declared) = e.attr_value(attribute) else {
                continue;
            };
            let Ok(declared) = declared.parse::<u64>() else {
                continue;
            };
            let actual = e
                .children
                .iter()
                .filter(|&&c| {
                    let child = xml.el(c);
                    child
                        .ns
                        .as_deref()
                        .and_then(|uri| self.dict.prefix_for(uri))
                        .is_some_and(|p| known.children.contains(&format!("{}:{}", p, child.local)))
                })
                .count() as u64;
            if declared == actual {
                continue;
            }
            let example = self.example(entry, xml, el);
            let message = format!(
                "{prefix}:{}@{attribute} is {declared} but {actual} declared children are present",
                e.local
            );
            self.add(
                format!("count|{prefix}|{}|{attribute}", e.local),
                Draft::new(
                    CODE_COUNT_MISMATCH,
                    Severity::Error,
                    count_reason(attribute),
                    message,
                )
                .prefix(prefix)
                .element(&e.local)
                .attribute(attribute)
                .detail(json!({
                    "attribute": attribute,
                    "declared": declared,
                    "actual": actual,
                })),
                Some(example),
                "document",
            );
        }
        Ok(())
    }

    /// Reference attributes of one element: count them always, and report the
    /// unresolved/unset ones when `report` is set and definitions are known.
    fn scan_references(
        &mut self,
        entry: &str,
        xml: &XmlDoc,
        el: usize,
        report: bool,
    ) -> Result<()> {
        self.visit()?;
        let e = xml.el(el);
        if !(matches!(e.ns.as_deref(), Some(NS_HH) | Some(NS_HC)) || is_hp(e.ns.as_deref())) {
            return Ok(());
        }
        let prefix =
            e.ns.as_deref()
                .and_then(|uri| self.dict.prefix_for(uri))
                .unwrap_or("?")
                .to_string();
        for a in e.attrs.iter().filter(|a| a.ns.is_none()) {
            let Some(&(_, kind)) = REFERENCES.iter().find(|(name, _)| *name == a.local) else {
                continue;
            };
            if report {
                self.references += 1;
            }
            *self
                .used
                .entry((kind.to_string(), a.value.clone()))
                .or_insert(0) += 1;
            if !report || self.definitions.is_empty() {
                continue;
            }
            if a.value == UNSET_REFERENCE {
                let message = format!(
                    "{prefix}:{}@{} is {UNSET_REFERENCE}, the model's unset sentinel for (UINT)-1, so it is not a dangling reference",
                    e.local, a.local
                );
                let example = self.example(entry, xml, el);
                self.add(
                    format!("unset|{kind}|{}", a.local),
                    Draft::new(
                        CODE_UNSET_REFERENCE,
                        Severity::Info,
                        "reference_unset_sentinel",
                        message,
                    )
                    .prefix(&prefix)
                    .element(&e.local)
                    .attribute(&a.local)
                    .kind(kind)
                    .value(&a.value),
                    Some(example),
                    "document",
                );
            } else if !self
                .definitions
                .contains_key(&(kind.to_string(), a.value.clone()))
            {
                let message = format!(
                    "{prefix}:{}@{} refers to {kind} \"{}\", which the header does not define",
                    e.local, a.local, a.value
                );
                let example = self.example(entry, xml, el);
                self.add(
                    format!("unresolved|{kind}|{}|{}", a.local, a.value),
                    Draft::new(
                        CODE_UNRESOLVED_REFERENCE,
                        Severity::Warning,
                        "reference_not_in_header",
                        message,
                    )
                    .prefix(&prefix)
                    .element(&e.local)
                    .attribute(&a.local)
                    .kind(kind)
                    .value(&a.value),
                    Some(example),
                    "document",
                );
            }
        }
        Ok(())
    }

    /// Header pass: definitions, count attributes and duplicate ids.
    fn scan_header(&mut self, header: &str, xml: &XmlDoc) -> Result<()> {
        self.entries.insert(header.to_string());
        for (el, e) in xml.iter() {
            self.visit()?;
            if RESOURCE_KINDS.contains(&e.local.as_str()) && e.is(NS_HH, &e.local) {
                if let Some(id) = e.attr_value("id") {
                    let key = (e.local.clone(), id.to_string());
                    *self.definitions.entry(key.clone()).or_insert(0) += 1;
                    if !self.definition_sites.contains_key(&key) {
                        let site = self.example(header, xml, el);
                        self.definition_sites.insert(key, site);
                    }
                }
            }
            if e.is(NS_HH, "head") {
                if let Some(declared) = e.attr_value("secCnt").and_then(|v| v.parse::<u64>().ok()) {
                    let actual = self.doc.info.sections.len() as u64;
                    if declared != actual {
                        let example = self.example(header, xml, el);
                        let message = format!(
                            "hh:head@secCnt is {declared} but the package spine holds {actual} sections"
                        );
                        self.add(
                            "count|hh|head|secCnt".to_string(),
                            Draft::new(
                                CODE_COUNT_MISMATCH,
                                Severity::Error,
                                "sec_cnt_mismatch",
                                message,
                            )
                            .prefix("hh")
                            .element("head")
                            .attribute("secCnt")
                            .detail(json!({
                                "attribute": "secCnt",
                                "declared": declared,
                                "actual": actual,
                            })),
                            Some(example),
                            "document",
                        );
                    }
                }
            }
            let Some(prefix) = self
                .dict
                .prefix_for(e.ns.as_deref().unwrap_or(""))
                .map(str::to_owned)
            else {
                continue;
            };
            if !self.dict.covered(&prefix) {
                continue;
            }
            let Some(known) = self.dict.element(&prefix, &e.local) else {
                continue;
            };
            if known.attributes.contains("itemCnt") || known.attributes.contains("fontCnt") {
                self.check_counts(header, xml, el, &prefix, known)?;
            }
        }
        let duplicates: Vec<((String, String), u64)> = self
            .definitions
            .iter()
            .filter(|(_, n)| **n > 1)
            .map(|(k, n)| (k.clone(), *n))
            .collect();
        for ((kind, id), count) in duplicates {
            let example = self
                .definition_sites
                .get(&(kind.clone(), id.clone()))
                .cloned();
            self.add(
                format!("duplicate|{kind}|{id}"),
                Draft::new(
                    CODE_DUPLICATE_RESOURCE_ID,
                    Severity::Error,
                    "duplicate_id_in_header",
                    format!("the header defines {kind} id \"{id}\" {count} times"),
                )
                .prefix("hh")
                .element(&kind)
                .kind(&kind)
                .value(&id)
                .detail(json!({ "definitions": count })),
                example,
                "document",
            );
        }
        Ok(())
    }

    /// Defined resources that no attribute in the document refers to.
    fn report_unused(&mut self, header: &str) {
        for kind in RESOURCE_KINDS {
            let mut ids: Vec<String> = Vec::new();
            for (k, id) in self.definitions.keys() {
                if k == kind && !self.used.contains_key(&(k.clone(), id.clone())) {
                    ids.push(id.clone());
                }
            }
            if ids.is_empty() {
                continue;
            }
            ids.sort();
            let count = ids.len() as u64;
            let examples: Vec<Example> = ids
                .iter()
                .take(self.max_examples)
                .map(|id| {
                    self.definition_sites
                        .get(&(kind.to_string(), id.clone()))
                        .cloned()
                        .unwrap_or(Example {
                            entry: header.to_string(),
                            path: String::new(),
                            node_id: None,
                        })
                })
                .collect();
            let listed = ids.iter().take(3).cloned().collect::<Vec<_>>().join(", ");
            let message = format!(
                "the header defines {count} {kind} resource(s) that no attribute in the document refers to (ids {listed})"
            );
            self.add_many(
                format!("unused|{kind}"),
                Draft::new(
                    CODE_UNUSED_RESOURCE,
                    Severity::Info,
                    "no_reference_in_document",
                    message,
                )
                .prefix("hh")
                .element(kind)
                .kind(kind),
                count,
                examples,
                "document",
            );
        }
    }

    /// Package members that must exist, as `(reason)` findings.
    fn scan_package(&mut self) -> Result<()> {
        let entries: Vec<String> = self
            .doc
            .state
            .entries
            .iter()
            .map(|e| e.name.clone())
            .collect();
        for (name, reason) in missing_package_items(&entries, &self.doc.info) {
            let severity = match reason {
                "header_absent" => Severity::Warning,
                _ => Severity::Error,
            };
            self.add(
                format!("package|{reason}|{name}"),
                Draft::new(
                    CODE_MISSING_PACKAGE_ITEM,
                    severity,
                    reason,
                    format!("required package member {name} is missing ({reason})"),
                )
                .value(&name),
                None,
                "document",
            );
        }
        if let Some(payload) = self.doc.state.get("mimetype") {
            if &payload[..] != MIMETYPE.as_bytes() {
                let value = String::from_utf8_lossy(payload).to_string();
                self.add(
                    "package|mimetype_value|mimetype".to_string(),
                    Draft::new(
                        CODE_MISSING_PACKAGE_ITEM,
                        Severity::Error,
                        "mimetype_value_mismatch",
                        format!("mimetype is {value:?} instead of {MIMETYPE:?}"),
                    )
                    .value(&value),
                    None,
                    "document",
                );
            }
        }
        Ok(())
    }

    fn finish(self, dict: &Dictionary, header: Option<String>) -> Report {
        let mut items: Vec<Finding> = self.findings.into_values().collect();
        items.sort_by(|a, b| {
            a.severity
                .rank()
                .cmp(&b.severity.rank())
                .then_with(|| a.code.cmp(&b.code))
                .then_with(|| a.message.cmp(&b.message))
        });
        let mut by_code: BTreeMap<String, CodeCount> = BTreeMap::new();
        let (mut errors, mut warnings, mut infos) = (0, 0, 0);
        let mut issues = 0u64;
        for f in &items {
            match f.severity {
                Severity::Error => errors += 1,
                Severity::Warning => warnings += 1,
                Severity::Info => infos += 1,
            }
            issues += f.count;
            let entry = by_code.entry(f.code.clone()).or_insert(CodeCount {
                code: f.code.clone(),
                severity: f.severity,
                findings: 0,
                issues: 0,
            });
            entry.findings += 1;
            entry.issues += f.count;
        }
        let package_items = required_package_items(&self.doc.info)
            .into_iter()
            .filter(|name| !name.is_empty())
            .collect();
        Report {
            summary: Summary {
                errors,
                warnings,
                infos,
                findings: items.len(),
                issues,
                by_code: by_code.into_values().collect(),
            },
            checked: Checked {
                xml_entries: self.entries.len(),
                elements: self.elements,
                attributes: self.attributes,
                references: self.references,
                namespaces: self.namespaces.into_values().collect(),
                package_items,
                reference_integrity: header.is_some(),
                resource_definitions: header.is_some(),
                header,
                dictionary: DictionaryInfo {
                    project: SOURCE.to_string(),
                    license: SOURCE_LICENSE.to_string(),
                    source_revision: dict.source_revision().to_string(),
                    extractor_version: dict.extractor_version().to_string(),
                    fingerprint: dict.fingerprint().to_string(),
                    elements: dict.element_count(),
                    namespaces: dict.namespace_count(),
                },
            },
            items,
            truncated: self.truncated,
        }
    }
}

fn count_reason(attribute: &str) -> &'static str {
    match attribute {
        "itemCnt" => "item_cnt_mismatch",
        "fontCnt" => "font_cnt_mismatch",
        _ => "count_attribute_mismatch",
    }
}

/// Required package members, in report order. The rootfile is package-specific
/// and taken from the manifest instead of being hard-coded.
fn required_package_items(info: &PackageInfo) -> Vec<String> {
    let mut out = vec![
        "mimetype".to_string(),
        "META-INF/container.xml".to_string(),
        info.rootfile.clone(),
    ];
    if let Some(header) = &info.header {
        out.push(header.clone());
    }
    out.extend(info.sections.iter().cloned());
    out
}

/// `(member, reason)` for every required package member missing from `entries`.
///
/// The open gate already refuses most of these (`PACKAGE_REJECTED`), so on an
/// openable document this is a re-check rather than a discovery; the unit tests
/// feed it hand-built inputs.
pub(crate) fn missing_package_items(
    entries: &[String],
    info: &PackageInfo,
) -> Vec<(String, &'static str)> {
    let present = |name: &str| entries.iter().any(|e| e == name);
    let mut out = Vec::new();
    if !present("mimetype") {
        out.push(("mimetype".to_string(), "mimetype_absent"));
    } else if entries.first().map(String::as_str) != Some("mimetype") {
        out.push(("mimetype".to_string(), "mimetype_not_first"));
    }
    if !present("META-INF/container.xml") {
        out.push(("META-INF/container.xml".to_string(), "container_absent"));
    }
    if info.rootfile.is_empty() || !present(&info.rootfile) {
        out.push((info.rootfile.clone(), "rootfile_absent"));
    }
    if info.sections.is_empty() {
        out.push(("section".to_string(), "section_absent"));
    }
    for section in &info.sections {
        if !present(section) {
            out.push((section.clone(), "section_entry_absent"));
        }
    }
    if info.header.is_none() {
        out.push(("header".to_string(), "header_absent"));
    } else if let Some(header) = &info.header {
        if !present(header) {
            out.push((header.clone(), "header_entry_absent"));
        }
    }
    out
}

/// `prefix:name[n]` segments from the entry root, at most 6 levels.
fn path_of(xml: &XmlDoc, el: usize) -> String {
    let mut segments = Vec::new();
    let mut current = Some(el);
    while let Some(i) = current {
        if segments.len() >= 6 {
            segments.push("…".to_string());
            break;
        }
        let e = xml.el(i);
        let index = match e.parent {
            None => 1,
            Some(p) => xml
                .el(p)
                .children
                .iter()
                .filter(|&&c| {
                    let s = xml.el(c);
                    s.local == e.local && s.ns == e.ns
                })
                .position(|&c| c == i)
                .map_or(1, |p| p + 1),
        };
        segments.push(format!("{}[{}]", e.qname, index));
        current = e.parent;
    }
    segments.reverse();
    segments.join("/")
}

impl Document {
    /// Compare the document with the bundled OWPML dictionary.
    ///
    /// `scope` limits the element/attribute scan to one node's subtree.
    /// Reference integrity, resource definitions, counts and package members
    /// are document-wide properties and are reported for the whole document
    /// with `scope: "document"`.
    pub fn validate(&self, scope: Option<&str>, max_examples: usize) -> Result<Report> {
        if !(1..=MAX_EXAMPLES).contains(&max_examples) {
            return Err(query_error(format!(
                "maxExamples must be in 1..={MAX_EXAMPLES}"
            )));
        }
        let dict = Dictionary::get();
        let scope_node = match scope {
            Some(id) => Some(self.require(id, None)?),
            None => None,
        };
        let scope_label = scope_node.map_or_else(|| "document".to_string(), |n| n.id.clone());
        let mut s = Scanner::new(self, dict, scope_label, max_examples);
        let header = self.info.header.clone();

        s.scan_package()?;
        if let Some(header) = &header {
            let xml = self
                .xml
                .get(header)
                .ok_or_else(|| query_error("header entry is not parsed"))?;
            s.scan_header(header, xml)?;
        }

        // Element and attribute comparison. Without a scope every parsed XML
        // entry is compared; with a scope only the node's subtree is.
        match scope_node {
            None => {
                for (entry, xml) in &self.xml {
                    let root = xml.root();
                    s.scan_tree(entry, xml, root)?;
                }
            }
            Some(node) => {
                let xml = self.doc_of(node);
                s.scan_tree(&node.entry, xml, node.el)?;
            }
        }

        // Reference integrity over the whole document (definitions live in the
        // header, so this stays document-wide under a scope).
        for (entry, xml) in &self.xml {
            for (el, _) in xml.iter() {
                s.scan_references(entry, xml, el, true)?;
            }
        }

        s.report_unused(header.as_deref().unwrap_or(""));
        Ok(s.finish(dict, header))
    }
}
