use hae_diag::{Budget, Code, EngineError, Limits, Result, Stage};
use hae_model::{Document, Kind, MapKey, MapRow};
use hae_package::{sha256_hex, Origin, PackageState};
use hae_zip::{Archive, OutEntry, METHOD_STORED};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

const NAMES: [&str; 4] = [
    "manifest.json",
    "original.hwpx",
    "current.hwpx",
    "idmap.json",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectManifest {
    pub schema_version: u32,
    pub document_id: String,
    pub revision: u64,
    pub payload_set_sha256: String,
    pub current_file_sha256: String,
    pub original_file_sha256: String,
    pub map_sha256: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct IdMap {
    document_id: String,
    rows: Vec<Row>,
}

// Do not deserialize model MapRow directly: its flattened fields currently
// accept unknown keys. Project persistence has a strict schema.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    entry: String,
    path: Vec<usize>,
    kind: Kind,
    id: String,
}

pub(crate) fn integrity(message: impl Into<String>) -> EngineError {
    EngineError::new(Code::ProjectIntegrityError, Stage::Open, message)
}

fn project_error(e: EngineError) -> EngineError {
    if e.code() == Code::ResourceLimit {
        e
    } else {
        integrity(e.to_string())
    }
}

fn valid_id(id: &str, prefix: &str) -> bool {
    id.strip_prefix(prefix).is_some_and(|s| {
        s.len() == 18
            && s.bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    })
}

pub(crate) fn export_bytes(doc: &Document, limits: &Limits) -> Result<Vec<u8>> {
    let bytes = if doc.is_unmodified() {
        doc.original.raw_bytes().to_vec()
    } else {
        doc.to_hwpx()?
    };
    check_size(bytes.len(), limits.max_hwpx_bytes)?;
    Ok(bytes)
}

pub(crate) fn check_size(size: usize, max: u64) -> Result<()> {
    if size as u64 > max {
        Err(EngineError::new(
            Code::ResourceLimit,
            Stage::Save,
            "file size limit exceeded",
        ))
    } else {
        Ok(())
    }
}

pub(crate) fn encode(doc: &Document, revision: u64, limits: &Limits) -> Result<Vec<u8>> {
    let current = export_bytes(doc, limits)?;
    check_size(doc.original.raw_bytes().len(), limits.max_hwpx_bytes)?;
    let rows = doc
        .map_rows()
        .into_iter()
        .map(|r| Row {
            entry: r.key.entry,
            path: r.key.path,
            kind: r.key.kind,
            id: r.id,
        })
        .collect();
    let map = serde_json::to_vec(&IdMap {
        document_id: doc.document_id.clone(),
        rows,
    })
    .map_err(|e| integrity(e.to_string()))?;
    let manifest = ProjectManifest {
        schema_version: 1,
        document_id: doc.document_id.clone(),
        revision,
        payload_set_sha256: doc.state.payload_set_sha256(),
        current_file_sha256: sha256_hex(&current),
        original_file_sha256: sha256_hex(doc.original.raw_bytes()),
        map_sha256: sha256_hex(&map),
    };
    let manifest = serde_json::to_vec(&manifest).map_err(|e| integrity(e.to_string()))?;
    // The outer ZIP is STORED, so its total payload plus headers is bounded
    // before allocating a second complete container.
    let outer_payload = manifest.len() + map.len() + current.len() + doc.original.raw_bytes().len();
    check_size(outer_payload, limits.max_project_bytes)?;
    let mut budget = Budget::new(limits.max_inflate_total);
    budget.try_reserve(outer_payload as u64, "project payloads")?;
    budget.try_reserve(doc.original.inflated_total(), "original HWPX")?;
    budget.try_reserve(doc.state.total_bytes(), "current HWPX")?;
    let payloads = [manifest, doc.original.raw_bytes().to_vec(), current, map];
    let plan: Vec<_> = NAMES
        .into_iter()
        .zip(payloads)
        .map(|(name, payload)| OutEntry::Add {
            name: name.into(),
            payload: Arc::from(payload),
            method: METHOD_STORED,
        })
        .collect();
    let bytes = hae_zip::write(&doc.original, &plan)?;
    check_size(bytes.len(), limits.max_project_bytes)?;
    Ok(bytes)
}

pub(crate) struct Decoded {
    pub document: Document,
    pub manifest: ProjectManifest,
    pub current: Arc<[u8]>,
}

pub(crate) fn decode(bytes: Vec<u8>, limits: &Limits) -> Result<Decoded> {
    decode_inner(bytes, limits).map_err(project_error)
}

fn decode_inner(bytes: Vec<u8>, limits: &Limits) -> Result<Decoded> {
    let mut budget = Budget::new(limits.max_inflate_total);
    let outer_limits = Limits {
        max_hwpx_bytes: limits.max_project_bytes,
        ..*limits
    };
    let outer = Archive::open_with_budget(bytes, &outer_limits, &mut budget)?;
    if outer.len() != NAMES.len() || NAMES.iter().any(|n| outer.get(n).is_none()) {
        return Err(integrity(
            "project must contain exactly manifest/original/current/idmap",
        ));
    }
    let manifest: ProjectManifest = serde_json::from_slice(outer.payload("manifest.json").unwrap())
        .map_err(|e| integrity(format!("manifest: {e}")))?;
    let original_bytes = outer.payload("original.hwpx").unwrap();
    let current_bytes = outer.payload("current.hwpx").unwrap();
    let map_bytes = outer.payload("idmap.json").unwrap();
    if manifest.schema_version != 1 || !valid_id(&manifest.document_id, "doc-") {
        return Err(integrity("unsupported project schema or document identity"));
    }
    if sha256_hex(original_bytes) != manifest.original_file_sha256
        || sha256_hex(current_bytes) != manifest.current_file_sha256
        || sha256_hex(map_bytes) != manifest.map_sha256
    {
        return Err(integrity("project file hash mismatch"));
    }
    let map: IdMap =
        serde_json::from_slice(map_bytes).map_err(|e| integrity(format!("idmap: {e}")))?;
    if map.document_id != manifest.document_id || map.rows.iter().any(|r| !valid_id(&r.id, "n-")) {
        return Err(integrity(
            "idmap document identity or node identity invalid",
        ));
    }
    let rows: Vec<_> = map
        .rows
        .into_iter()
        .map(|r| MapRow {
            key: MapKey {
                entry: r.entry,
                path: r.path,
                kind: r.kind,
            },
            id: r.id,
        })
        .collect();
    let original = Arc::new(Archive::open_with_budget(
        original_bytes.to_vec(),
        limits,
        &mut budget,
    )?);
    let current = Archive::open_with_budget(current_bytes.to_vec(), limits, &mut budget)?;
    let original_state = PackageState::from_archive(&original);
    hae_package::analyze(
        &original_state,
        original.entries().first().is_some_and(|e| e.is_stored()),
        limits,
    )?;
    let mut state = PackageState::from_archive(&current);
    // Validate current mimetype using CURRENT metadata, not original metadata.
    hae_package::analyze(
        &state,
        current.entries().first().is_some_and(|e| e.is_stored()),
        limits,
    )?;
    if state.payload_set_sha256() != manifest.payload_set_sha256 {
        return Err(integrity("current payload set hash mismatch"));
    }
    // Added media are absent from the immutable original; preserve their method
    // so subsequent to_hwpx()/edits use Add rather than an invalid Replace.
    for (entry, zip_entry) in state.entries.iter_mut().zip(current.entries()) {
        if original.get(&entry.name).is_none() {
            entry.origin = Origin::Added {
                method: zip_entry.method,
            };
        }
    }
    let document =
        Document::open_with_map(manifest.document_id.clone(), original, state, &rows, limits)?;
    Ok(Decoded {
        document,
        manifest,
        current: current_bytes.clone(),
    })
}
