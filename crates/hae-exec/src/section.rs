//! Query explicit source values without inventing inherited section defaults.
use hae_diag::{Code, EngineError, Result, Stage};
use hae_model::Document;
use hae_xml::XmlDoc;
use serde_json::{json, Value};
fn hp(doc: &XmlDoc, i: usize, name: &str) -> bool {
    hae_model::is_hp(doc.el(i).ns.as_deref()) && doc.el(i).local == name
}
fn optional(doc: &XmlDoc, parent: usize, name: &str) -> Result<Option<usize>> {
    let nodes: Vec<_> = doc
        .el(parent)
        .children
        .iter()
        .copied()
        .filter(|&i| hp(doc, i, name))
        .collect();
    if nodes.len() > 1 {
        return Err(EngineError::new(
            Code::Unsupported,
            Stage::Validate,
            format!("ambiguous {name}"),
        )
        .with_reason("structure_change_unsupported"));
    }
    Ok(nodes.first().copied())
}
fn fields(doc: &XmlDoc, node: Option<usize>, names: &[(&str, bool)]) -> Value {
    let mut values = serde_json::Map::new();
    for &(name, numeric) in names {
        let raw = node.and_then(|i| doc.el(i).attr_value(name));
        let value = raw.map_or(Value::Null, |s| {
            if numeric {
                s.parse::<u64>().map_or_else(|_| json!(s), |v| json!(v))
            } else {
                json!(s)
            }
        });
        values.insert(
            name.into(),
            json!({"value":value,"source":if raw.is_some(){"explicit"}else{"unspecified"}}),
        );
    }
    Value::Object(values)
}
pub(crate) fn get_page(
    doc: &Document,
    scope: Option<&str>,
    index: Option<usize>,
    revision: u64,
) -> Result<Value> {
    let owner = hae_edit::resolve_section(doc, scope, index)?;
    let xml = doc.doc_of(&owner);
    let sec = xml.iter().find(|(i, _)| hp(xml, *i, "secPr")).unwrap().0;
    let page = optional(xml, sec, "pagePr")?;
    let margin = page
        .map(|i| optional(xml, i, "margin"))
        .transpose()?
        .flatten();
    let start = optional(xml, sec, "startNum")?;
    let visibility = optional(xml, sec, "visibility")?;
    let grid = optional(xml, sec, "grid")?;
    let mut result = fields(
        xml,
        page,
        &[
            ("landscape", false),
            ("width", true),
            ("height", true),
            ("gutterType", false),
        ],
    );
    let obj = result.as_object_mut().unwrap();
    obj.insert("documentId".into(), json!(doc.document_id));
    obj.insert("revision".into(), json!(revision));
    obj.insert("scope".into(), json!(owner.id));
    obj.insert("sectionIndex".into(), json!(owner.section));
    obj.insert(
        "margins".into(),
        fields(
            xml,
            margin,
            &[
                ("header", true),
                ("footer", true),
                ("gutter", true),
                ("left", true),
                ("right", true),
                ("top", true),
                ("bottom", true),
            ],
        ),
    );
    obj.insert(
        "startNum".into(),
        fields(
            xml,
            start,
            &[
                ("pageStartsOn", false),
                ("page", true),
                ("pic", true),
                ("tbl", true),
                ("equation", true),
            ],
        ),
    );
    obj.insert(
        "visibility".into(),
        fields(
            xml,
            visibility,
            &[
                ("hideFirstHeader", true),
                ("hideFirstFooter", true),
                ("hideFirstMasterPage", true),
                ("border", false),
                ("fill", false),
                ("hideFirstPageNum", true),
                ("hideFirstEmptyLine", true),
                ("showLineNumber", true),
            ],
        ),
    );
    obj.insert(
        "grid".into(),
        fields(
            xml,
            grid,
            &[
                ("lineGrid", true),
                ("charGrid", true),
                ("wonggojiFormat", true),
            ],
        ),
    );
    obj.extend(
        fields(
            xml,
            Some(sec),
            &[("textDirection", false), ("spaceColumns", true)],
        )
        .as_object()
        .unwrap()
        .clone(),
    );
    obj.insert("notes".into(),json!(["Values reflect XML attributes only; inheritance and application defaults are not resolved. Missing or non-numeric measurements are not guessed. Column definitions and page-number display controls are not edited."]));
    Ok(result)
}
