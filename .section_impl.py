from pathlib import Path
edit=Path('crates/hae-edit/src/section.rs')
types='''
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PageSettings {
    pub landscape: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub gutter_type: Option<String>,
    pub margins: Option<Margins>,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Margins {
    pub left: Option<u32>, pub right: Option<u32>,
    pub top: Option<u32>, pub bottom: Option<u32>,
    pub header: Option<u32>, pub footer: Option<u32>, pub gutter: Option<u32>,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PageNumbering {
    pub page_starts_on: Option<String>, pub page: Option<u32>,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Visibility {
    pub hide_first_header: Option<bool>, pub hide_first_footer: Option<bool>,
    pub hide_first_page_num: Option<bool>, pub hide_first_empty_line: Option<bool>,
    pub show_line_number: Option<bool>, pub border: Option<String>, pub fill: Option<String>,
}
'''
edit.write_text('''//! Attribute-only section edits; no structural creation or serialization of untouched tokens.
use crate::{Draft, Intent};
use hae_diag::{Code, EngineError, Result, Stage};
use hae_model::{Document, Kind, Node};
use hae_xml::{XmlDoc, escape_attr};
use serde::{Serialize, Deserialize};
use std::ops::Range;
'''+types+'''
fn shape(message: &str) -> EngineError {
    EngineError::new(Code::Unsupported, Stage::Validate, message).with_reason("structure_change_unsupported")
}
fn invalid(message: &str) -> EngineError {
    EngineError::new(Code::InvalidRequest, Stage::Validate, message)
}
fn hp(d: &XmlDoc, i: usize, name: &str) -> bool {
    hae_model::is_hp(d.el(i).ns.as_deref()) && d.el(i).local == name
}
/// Resolve only an existing section's owning top-level paragraph. Never synthesize secPr.
pub fn resolve_section(doc: &Document, scope: Option<&str>, index: Option<usize>) -> Result<Node> {
    if scope.is_some() && index.is_some() { return Err(invalid("supply scope or sectionIndex, not both")); }
    let entry = if let Some(id) = scope {
        doc.require(id, Some(Kind::Paragraph))?.entry.clone()
    } else {
        doc.info.sections.get(index.unwrap_or(0)).cloned().ok_or_else(|| shape("sectionIndex must refer to an existing section; creation is unsupported"))?
    };
    let xml = &doc.xml[&entry];
    let secs: Vec<_> = xml.iter().filter(|(i, _)| hp(xml, *i, "secPr")).map(|(i, _)| i).collect();
    if secs.len() != 1 { return Err(shape("section must contain exactly one existing secPr")); }
    let owner = xml.ancestors(secs[0]).find(|&i| hp(xml, i, "p")).ok_or_else(|| shape("secPr must be inside a paragraph"))?;
    if xml.el(owner).parent != Some(xml.root()) { return Err(shape("secPr owner must be a top-level section paragraph")); }
    let node = doc.nodes.iter().find(|n| n.entry == entry && n.el == owner && n.kind == Kind::Paragraph).ok_or_else(|| shape("section owner is not indexed"))?;
    if scope.is_some_and(|id| id != node.id) { return Err(shape("scope must be the paragraph containing secPr")); }
    Ok(node.clone())
}
fn child(d: &XmlDoc, i: usize, name: &str) -> Result<usize> {
    let nodes: Vec<_> = d.el(i).children.iter().copied().filter(|&c| hp(d,c,name)).collect();
    if nodes.len() != 1 { return Err(shape(&format!("requires one existing {name}; new nodes are unsupported"))); }
    Ok(nodes[0])
}
fn secpr(d: &XmlDoc) -> Result<usize> {
    let v: Vec<_> = d.iter().filter(|(i,_)| hp(d,*i,"secPr")).map(|(i,_)| i).collect();
    if v.len()!=1 { return Err(shape("requires exactly one secPr")); }
    Ok(v[0])
}
fn attrs<T: Serialize>(value: &T) -> Vec<(String, String)> {
    serde_json::to_value(value).unwrap().as_object().unwrap().iter().filter_map(|(k,v)| {
        if v.is_null() || v.is_object() { None } else { Some((k.clone(), match v { serde_json::Value::String(s)=>s.clone(), serde_json::Value::Bool(b)=>if *b {"1".into()} else {"0".into()}, _=>v.to_string() })) }
    }).collect()
}
fn changes(d: &XmlDoc, i: usize, values: Vec<(String,String)>) -> Vec<(Range<usize>,Vec<u8>)> {
    let e=d.el(i); let mut cuts=Vec::new(); let mut added=String::new();
    for (key,value) in values {
        if let Some(a)=e.attr(&key) {
            if a.value != value { cuts.push((a.value_span.clone(),escape_attr(&value).into_bytes())); }
        } else { added.push_str(&format!(" {key}=\\\"{}\\\"",escape_attr(&value))); }
    }
    if !added.is_empty() { let at=e.start.end-if e.end.is_none(){2}else{1}; cuts.push((at..at,added.into_bytes())); }
    cuts
}
fn enum_value(value: Option<&str>, allowed: &[&str]) -> Result<()> {
    if value.is_some_and(|v| !allowed.contains(&v)) { return Err(invalid("unsupported section enum value")); } Ok(())
}
fn geometry(d: &XmlDoc, page: usize, margin: usize, p: &PageSettings) -> Result<()> {
    // No inherited/default values are assumed. Every needed original value must be explicit.
    let m=p.margins.clone().unwrap_or_default();
    let num=|i:usize,k:&str,v:Option<u32>| -> Result<u64> { v.map(u64::from).or_else(|| d.el(i).attr_value(k)?.parse().ok()).ok_or_else(|| shape(&format!("cannot validate body area: unspecified/non-numeric {k}"))) };
    let width=num(page,"width",p.width)?; let height=num(page,"height",p.height)?;
    let landscape=p.landscape.as_deref().or_else(|| d.el(page).attr_value("landscape")).ok_or_else(|| shape("cannot validate body area: unspecified landscape"))?;
    enum_value(Some(landscape), &["WIDELY","NARROWLY"])?;
    // WIDELY is the observed portrait encoding; NARROWLY swaps physical axes.
    let (w,h)=if landscape=="NARROWLY" {(height,width)} else {(width,height)};
    let gt=p.gutter_type.as_deref().or_else(|| d.el(page).attr_value("gutterType")).ok_or_else(|| shape("cannot validate body area: unspecified gutterType"))?;
    enum_value(Some(gt), &["LEFT_ONLY","LEFT_RIGHT","TOP_BOTTOM"])?;
    let g=num(margin,"gutter",m.gutter)?;
    let used_w=num(margin,"left",m.left)?+num(margin,"right",m.right)?+if gt=="TOP_BOTTOM" {0}else{g};
    let used_h=num(margin,"top",m.top)?+num(margin,"bottom",m.bottom)?+num(margin,"header",m.header)?+num(margin,"footer",m.footer)?+if gt=="TOP_BOTTOM"{g}else{0};
    if width==0 || height==0 || used_w>=w || used_h>=h { return Err(invalid("margins leave a non-positive body area").with_reason("non_positive_body_area")); }
    Ok(())
}
fn apply_cuts(d: &mut Draft, target: &str, cuts: Vec<(Range<usize>,Vec<u8>)>, command: &'static str, request: serde_json::Value) -> Result<()> {
    let para=d.node(target,Some(Kind::Paragraph))?;
    // Revalidate ownership even for direct library callers.
    resolve_section(&d.doc,Some(target),None)?;
    if cuts.is_empty(){return Ok(());}
    d.splice_entry(&para.entry,cuts)?;
    // Section-level settings affect all pages and all paragraph caches in this section.
    d.anchors.entry(para.entry.clone()).and_modify(|a| a.0=0).or_insert((0,Vec::new()));
    d.changed.push(target.into());
    d.intents.push(Intent::Section { command:command.into(),entry:para.entry,path:d.doc.doc_of(&d.doc.require(target,Some(Kind::Paragraph))?.clone()).path(d.doc.require(target,None)?.el),request });
    Ok(())
}
pub(crate) fn set_page(d:&mut Draft,target:&str,p:&PageSettings)->Result<()> {
    let para=d.node(target,Some(Kind::Paragraph))?; resolve_section(&d.doc,Some(target),None)?;
    if attrs(p).is_empty() && p.margins.as_ref().is_none_or(|m|attrs(m).is_empty()) {return Err(invalid("setPage needs at least one property"));}
    enum_value(p.landscape.as_deref(), &["NARROWLY","WIDELY"])?;
    enum_value(p.gutter_type.as_deref(), &["LEFT_ONLY","LEFT_RIGHT","TOP_BOTTOM"])?;
    if p.width==Some(0)||p.height==Some(0){return Err(invalid("width and height must be at least 1 HWPUNIT"));}
    let x=d.doc.doc_of(&para);let s=secpr(x)?;let page=child(x,s,"pagePr")?;let margin=child(x,page,"margin")?;
    geometry(x,page,margin,p)?;
    let mut cuts=changes(x,page,attrs(p));
    if let Some(m)=&p.margins {cuts.extend(changes(x,margin,attrs(m)));}
    apply_cuts(d,target,cuts,"section.setPage",serde_json::to_value(p).unwrap())
}
pub(crate) fn set_numbering(d:&mut Draft,target:&str,p:&PageNumbering)->Result<()> {
    let para=d.node(target,Some(Kind::Paragraph))?;resolve_section(&d.doc,Some(target),None)?;
    if attrs(p).is_empty(){return Err(invalid("setPageNumbering needs pageStartsOn or page"));}
    enum_value(p.page_starts_on.as_deref(), &["BOTH","EVEN","ODD"])?;
    let x=d.doc.doc_of(&para);let i=child(x,secpr(x)?,"startNum")?;
    apply_cuts(d,target,changes(x,i,attrs(p)),"section.setPageNumbering",serde_json::to_value(p).unwrap())
}
pub(crate) fn set_visibility(d:&mut Draft,target:&str,p:&Visibility)->Result<()> {
    let para=d.node(target,Some(Kind::Paragraph))?;resolve_section(&d.doc,Some(target),None)?;
    if attrs(p).is_empty(){return Err(invalid("setVisibility needs at least one flag"));}
    for v in [p.border.as_deref(),p.fill.as_deref()] {enum_value(v, &["SHOW_ALL","HIDE_ALL","SHOW_FIRST","HIDE_FIRST"])?;}
    let x=d.doc.doc_of(&para);let i=child(x,secpr(x)?,"visibility")?;
    apply_cuts(d,target,changes(x,i,attrs(p)),"section.setVisibility",serde_json::to_value(p).unwrap())
}
''')
p=Path('crates/hae-edit/src/lib.rs');s=p.read_text().replace('mod replace_all;','mod replace_all;\nmod section;').replace('pub use char_format::CharProps;','pub use char_format::CharProps;\npub use section::{resolve_section, PageSettings, Margins, PageNumbering, Visibility};')
variants='''    SectionSetPage { target: String, settings: PageSettings },
    SectionSetPageNumbering { target: String, settings: PageNumbering },
    SectionSetVisibility { target: String, settings: Visibility },
'''
s=s.replace('pub enum EditOp {','pub enum EditOp {\n'+variants).replace('            EditOp::TextFormat { target, .. }','            EditOp::SectionSetPage {target,..}\n            | EditOp::SectionSetPageNumbering {target,..}\n            | EditOp::SectionSetVisibility {target,..}\n            | EditOp::TextFormat { target, .. }')
s=s.replace('pub enum Intent {','''pub enum Intent {
    Section { command: String, entry: String, path: Vec<usize>, request: serde_json::Value },''')
s=s.replace('        let r = match op {','''        let r = match op {
            EditOp::SectionSetPage {target,settings} => section::set_page(&mut d,target,settings),
            EditOp::SectionSetPageNumbering {target,settings} => section::set_numbering(&mut d,target,settings),
            EditOp::SectionSetVisibility {target,settings} => section::set_visibility(&mut d,target,settings),''')
p.write_text(s)
# Independent request types, no shared edit implementation.
p=Path('crates/hae-verify/src/contract.rs');s=p.read_text().replace('/// Paths count ELEMENT',types+'\n/// Paths count ELEMENT')
s=s.replace('pub enum VerifyOperation {','''pub enum VerifyOperation {
    #[serde(rename = "section.setPage")]
    SectionSetPage { entry:String, path:Vec<usize>, settings:PageSettings },
    #[serde(rename = "section.setPageNumbering")]
    SectionSetPageNumbering { entry:String, path:Vec<usize>, settings:PageNumbering },
    #[serde(rename = "section.setVisibility")]
    SectionSetVisibility { entry:String, path:Vec<usize>, settings:Visibility },''').replace('            Self::TextFormat { entry, path, .. }','''            Self::SectionSetPage {entry,path,..}
            | Self::SectionSetPageNumbering {entry,path,..}
            | Self::SectionSetVisibility {entry,path,..}
            | Self::TextFormat { entry, path, .. }''');p.write_text(s)
