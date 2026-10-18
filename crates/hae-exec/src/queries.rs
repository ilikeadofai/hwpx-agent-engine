//! Bounded, revision/document/filter-bound paging for read-only resource queries.
use hae_diag::{Code, EngineError, Result, Stage};
use hae_model::Document;
use serde::Serialize;
use serde_json::{json, Value};

pub(crate) struct PageRequest<'a> {
    pub revision: u64,
    pub selector: Value,
    pub cursor: Option<&'a str>,
    pub limit: Option<usize>,
    pub budget: usize,
}

pub(crate) fn page<T: Serialize>(
    doc: &Document,
    p: PageRequest<'_>,
    total: usize,
    records: impl IntoIterator<Item = Result<T>>,
    mut extra: Value,
) -> Result<Value> {
    let invalid = |msg| EngineError::new(Code::InvalidRequest, Stage::Query, msg);
    let limit = p.limit.unwrap_or(100);
    if !(1..=500).contains(&limit) {
        return Err(invalid("limit must be in 1..=500"));
    }
    let fingerprint = hae_package::sha256_hex(
        &serde_json::to_vec(&json!([doc.document_id, p.revision, p.selector]))
            .expect("selector serializes"),
    );
    let offset = if let Some(cursor) = p.cursor {
        let (tag, offset) = cursor
            .split_once(':')
            .ok_or_else(|| invalid("invalid query cursor"))?;
        if tag != fingerprint {
            return Err(EngineError::new(
                Code::VersionConflict,
                Stage::Query,
                "cursor belongs to a different document, revision or query filter",
            ));
        }
        offset
            .parse::<usize>()
            .map_err(|_| invalid("invalid query cursor offset"))?
    } else {
        0
    };
    if offset > total {
        return Err(invalid("query cursor is past the end"));
    }
    let object = extra.as_object_mut().expect("query metadata is an object");
    object.insert("documentId".into(), json!(doc.document_id));
    object.insert("revision".into(), json!(p.revision));
    object.insert("total".into(), json!(total));
    object.insert("items".into(), json!([]));
    // Reserve a full cursor even on the last page while measuring JSON bytes.
    object.insert(
        "nextCursor".into(),
        json!(format!("{fingerprint}:{}", usize::MAX)),
    );
    let base = serde_json::to_vec(&extra)
        .expect("query metadata serializes")
        .len();
    if base >= p.budget {
        return Err(EngineError::new(
            Code::ResourceLimit,
            Stage::Query,
            "query metadata exceeds response budget",
        ));
    }
    let mut size = base;
    let mut items = Vec::new();
    for item in records.into_iter().skip(offset).take(limit) {
        let item = serde_json::to_value(item?).expect("query record serializes");
        let bytes = serde_json::to_vec(&item)
            .expect("query record serializes")
            .len()
            + 1;
        if size.saturating_add(bytes) > p.budget {
            if items.is_empty() {
                return Err(EngineError::new(
                    Code::ResourceLimit,
                    Stage::Query,
                    "one query record exceeds response budget; use a narrower scope or kind",
                ));
            }
            break;
        }
        size += bytes;
        items.push(item);
    }
    let next = offset + items.len();
    extra["items"] = Value::Array(items);
    extra["nextCursor"] = if next < total {
        json!(format!("{fingerprint}:{next}"))
    } else {
        Value::Null
    };
    Ok(extra)
}
