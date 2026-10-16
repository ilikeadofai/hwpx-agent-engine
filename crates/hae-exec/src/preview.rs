//! document.preview: result assembly around the rhwp adapter (spec §9·§10).
//!
//! The adapter renders a revision-fixed HWPX snapshot in a sandbox. Nothing
//! here mutates the document: a failed render leaves the state, revision and
//! history exactly as they were. Page data is returned inline within the
//! response size limit; pages that do not fit are listed without content.

use crate::registry::DocumentPreviewParams;
use hae_diag::{Code, Diagnostic, EngineError, Limits, Result, Stage};
use hae_model::Document;
use hae_render_rhwp::{PreviewOutput, RenderRequest, RenderStatus, RhwpAdapter};
use serde_json::{json, Value};

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            B64[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

fn png_size(png: &[u8]) -> Option<(u32, u32)> {
    if png.len() < 24 || &png[..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    Some((
        u32::from_be_bytes(png[16..20].try_into().ok()?),
        u32::from_be_bytes(png[20..24].try_into().ok()?),
    ))
}

/// Render `doc` (at `revision`) and build the command result.
pub(crate) fn render(
    adapter: &RhwpAdapter,
    doc: &Document,
    revision: u64,
    p: &DocumentPreviewParams,
    limits: &Limits,
) -> Result<(Value, Vec<Diagnostic>)> {
    let pages = p.pages.clone().unwrap_or_else(|| vec![1]);
    if pages.is_empty() || pages.contains(&0) || pages.len() > 100 {
        return Err(EngineError::new(
            Code::InvalidRequest,
            Stage::Request,
            "pages must hold 1..=100 one-based page numbers",
        ));
    }
    let include_png = p.include_png.unwrap_or(true);
    let include_svg = p.include_svg.unwrap_or(false);
    let hwpx = doc.to_hwpx()?;
    let out = adapter.preview(
        &RenderRequest {
            document_id: &doc.document_id,
            content_revision: revision,
            hwpx: &hwpx,
            pages: Some(&pages),
            want_png: include_png,
            png: Default::default(),
        },
        None,
    )?;
    Ok(assemble(doc, out, include_png, include_svg, limits))
}

fn assemble(
    doc: &Document,
    out: PreviewOutput,
    include_png: bool,
    include_svg: bool,
    limits: &Limits,
) -> (Value, Vec<Diagnostic>) {
    // Keep clear of the response limit: envelope, metadata and notes take room.
    let mut budget = limits.max_response_bytes.saturating_sub(768 * 1024) as i64;
    let mut omitted = false;
    let mut pages = Vec::new();
    for pg in &out.pages {
        let mut v = json!({
            "page": pg.page,
            "svgSha256": pg.svg_sha256,
            "svgBytes": pg.svg.len(),
        });
        let png_cost = pg.png.as_ref().map_or(0, |p| p.len().div_ceil(3) * 4);
        let svg_cost = if include_svg { pg.svg.len() } else { 0 };
        let cost = (png_cost + svg_cost) as i64;
        if (include_png && pg.png.is_some()) || include_svg {
            if cost > budget {
                omitted = true;
                v["omitted"] = json!("response_limit");
            } else {
                budget -= cost;
                if let Some(png) = pg.png.as_ref().filter(|_| include_png) {
                    v["png"] = json!({
                        "base64": base64(png),
                        "sha256": pg.png_sha256,
                        "bytes": png.len(),
                        "width": png_size(png).map(|s| s.0),
                        "height": png_size(png).map(|s| s.1),
                    });
                }
                if include_svg {
                    v["svg"] = json!(String::from_utf8_lossy(&pg.svg));
                }
            }
        }
        pages.push(v);
    }
    let status = if omitted && out.status == RenderStatus::Rendered {
        RenderStatus::Partial
    } else {
        out.status
    };
    let mut diagnostics_all: Vec<Value> = out
        .notes
        .iter()
        .map(|n| serde_json::to_value(n).unwrap_or(Value::Null))
        .collect();
    for d in &out.diagnostics {
        diagnostics_all.push(serde_json::to_value(d).unwrap_or(Value::Null));
    }
    if omitted {
        diagnostics_all.push(json!({
            "code": "PAGE_CONTENT_OMITTED", "severity": "warning",
            "message": "some page content was left out to stay within the response size limit; request fewer pages"
        }));
    }
    let mut metadata = serde_json::to_value(&out.metadata).unwrap_or(Value::Null);
    metadata["diagnostics"] = Value::Array(diagnostics_all);
    let result = json!({
        "documentId": out.metadata.document_id,
        "revision": out.metadata.content_revision,
        "status": status,
        "coverage": out.metadata.coverage,
        "pageCount": out.total_pages,
        "pages": pages,
        "metadata": metadata,
        // Sections whose line-layout cache the engine dropped after edits; rhwp
        // re-lays them out itself, so their page breaks are estimates.
        "staleLayoutSections": doc.stale_layout,
        "renderMillis": out.render_millis,
    });
    (result, out.diagnostics)
}

#[cfg(test)]
mod tests {
    use super::base64;

    #[test]
    fn base64_matches_rfc4648_vectors() {
        for (i, o) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(i.as_bytes()), o);
        }
    }
}
