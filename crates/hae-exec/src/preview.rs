//! document.preview: result assembly around the rhwp adapter (spec §9·§10).
//!
//! The adapter renders a revision-fixed HWPX snapshot in a sandbox. Nothing
//! here mutates the document: a failed render leaves the state, revision and
//! history exactly as they were. Page data is returned inline within the
//! response size limit; pages that do not fit are listed without content.

use crate::registry::DocumentPreviewParams;
use hae_diag::{Code, Diagnostic, EngineError, Limits, Result, Stage};
use hae_model::Document;
use hae_project::IoRoots;
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
    roots: Option<&IoRoots>,
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
    // Retain room for base64/JSON escaping, bounded response construction and
    // transport serialization while adapter-owned page buffers are still live.
    let response_workspace = limits.max_response_bytes.saturating_mul(8);
    let renderer_budget = limits
        .max_session_memory
        .checked_sub(hwpx.len() as u64)
        .and_then(|remaining| remaining.checked_sub(response_workspace))
        .filter(|remaining| *remaining > 0)
        .ok_or_else(|| {
            EngineError::new(
                Code::ResourceLimit,
                Stage::Render,
                "preview snapshot exceeds remaining session memory",
            )
            .with_reason("preview_snapshot_limit")
        })?;
    let out = adapter.preview_with_budget(
        &RenderRequest {
            document_id: &doc.document_id,
            content_revision: revision,
            hwpx: &hwpx,
            pages: Some(&pages),
            want_png: include_png,
            png: hae_render_rhwp::png::PngOptions {
                max_pixels: limits.max_image_pixels,
                ..Default::default()
            },
        },
        None,
        renderer_budget,
    )?;
    assemble(doc, out, include_png, include_svg, limits, roots)
}

fn assemble(
    doc: &Document,
    out: PreviewOutput,
    include_png: bool,
    include_svg: bool,
    limits: &Limits,
    roots: Option<&IoRoots>,
) -> Result<(Value, Vec<Diagnostic>)> {
    // MCP wraps the inner JSON in a JSON string. Reserve the worst-case
    // doubling as well as the request/envelope fields before choosing inline
    // content, so a page becomes an artifact before the transport rejects it.
    let result_limit = (limits.max_response_bytes / 2)
        .saturating_sub(limits.max_request_bytes.min(limits.max_response_bytes / 4) + 4096)
        as usize;
    let mut diagnostics_all: Vec<Value> = out
        .notes
        .iter()
        .map(|n| serde_json::to_value(n).unwrap_or(Value::Null))
        .collect();
    diagnostics_all.extend(
        out.diagnostics
            .iter()
            .map(|d| serde_json::to_value(d).unwrap_or(Value::Null)),
    );
    let mut metadata = serde_json::to_value(&out.metadata).unwrap_or(Value::Null);
    metadata["diagnostics"] = Value::Array(diagnostics_all);
    let mut result = json!({
        "documentId": out.metadata.document_id,
        "revision": out.metadata.content_revision,
        "status": out.status,
        "coverage": out.metadata.coverage,
        "pageCount": out.total_pages,
        "pages": [],
        "metadata": metadata,
        "staleLayoutSections": doc.stale_layout,
        "renderMillis": out.render_millis,
    });
    let encoded_size = |v: &Value| serde_json::to_vec(v).map_or(usize::MAX, |v| v.len());
    if encoded_size(&result) > result_limit {
        return Err(EngineError::new(
            Code::ResourceLimit,
            Stage::Render,
            "preview metadata exceeds response budget",
        )
        .with_reason("preview_metadata_limit"));
    }
    let mut delivered = 0usize;
    let mut incomplete = out.status != RenderStatus::Rendered;
    let mut omitted_content = false;
    let mut artifact_content = false;
    for pg in &out.pages {
        let stub = json!({"page":pg.page,"svgSha256":pg.svg_sha256,"svgBytes":pg.svg.len()});
        let mut page = stub.clone();
        let content_requested = include_png || include_svg;
        let raw_cost = pg
            .png
            .as_ref()
            .filter(|_| include_png)
            .map_or(0, |p| p.len().div_ceil(3) * 4)
            .saturating_add(if include_svg { pg.svg.len() } else { 0 });
        if raw_cost <= result_limit {
            if let Some(png) = pg.png.as_ref().filter(|_| include_png) {
                page["png"] = json!({"base64":base64(png),"sha256":pg.png_sha256,
                    "bytes":png.len(),"width":png_size(png).map(|s|s.0),"height":png_size(png).map(|s|s.1)});
            }
            if include_svg {
                page["svg"] = json!(String::from_utf8_lossy(&pg.svg));
            }
            if include_png && pg.png.is_none() {
                incomplete = true;
                if !include_svg {
                    page["omitted"] = json!("png_failed");
                } else {
                    page["pngError"] = json!("png_failed");
                }
            }
        }
        result["pages"].as_array_mut().unwrap().push(page);
        // Compare the actual JSON, including SVG escaping and variable metadata.
        if raw_cost > result_limit || encoded_size(&result) > result_limit {
            let mut page = stub;
            if let Some(roots) = roots {
                if let Some(png) = pg.png.as_ref().filter(|_| include_png) {
                    let receipt =
                        roots.write_preview_artifact("png", png, out.metadata.content_revision)?;
                    page["png"] = json!({"path":receipt.path,"sha256":receipt.sha256,
                        "bytes":png.len(),"width":png_size(png).map(|s|s.0),"height":png_size(png).map(|s|s.1)});
                }
                if include_svg {
                    let receipt = roots.write_preview_artifact(
                        "svg",
                        &pg.svg,
                        out.metadata.content_revision,
                    )?;
                    page["svgArtifact"] =
                        json!({"path":receipt.path,"sha256":receipt.sha256,"bytes":pg.svg.len()});
                }
                if include_png && pg.png.is_none() {
                    incomplete = true;
                    page["pngError"] = json!("png_failed");
                }
                artifact_content = true;
            } else {
                page["omitted"] = json!("response_limit");
                omitted_content = true;
                incomplete = true;
            }
            *result["pages"].as_array_mut().unwrap().last_mut().unwrap() = page;
        }
        let p = result["pages"].as_array().unwrap().last().unwrap();
        if !content_requested
            || p.get("png").is_some()
            || p.get("svg").is_some()
            || p.get("svgArtifact").is_some()
        {
            delivered += 1;
        }
    }
    result["status"] = json!(if delivered == 0 {
        RenderStatus::Failed
    } else if incomplete {
        RenderStatus::Partial
    } else {
        RenderStatus::Rendered
    });
    if omitted_content {
        result["metadata"]["diagnostics"].as_array_mut().unwrap().push(json!({
            "code":"PAGE_CONTENT_OMITTED","severity":"warning",
            "message":"page content exceeds the serialized response limit; request fewer pages or a smaller PNG"
        }));
    }
    if artifact_content {
        result["metadata"]["diagnostics"]
            .as_array_mut()
            .unwrap()
            .push(json!({
                "code":"PAGE_CONTENT_ARTIFACT","severity":"info",
                "message":"page content is available as a file under the configured output root"
            }));
    }
    if encoded_size(&result) > result_limit {
        return Err(EngineError::new(
            Code::ResourceLimit,
            Stage::Render,
            "preview result exceeds response budget",
        )
        .with_reason("preview_response_limit"));
    }
    Ok((result, out.diagnostics))
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

#[cfg(test)]
mod contract_tests {
    use super::*;
    use hae_render_rhwp::{PreviewMetadata, PreviewPage};
    fn output(svg: Vec<u8>, png: Option<Vec<u8>>) -> PreviewOutput {
        PreviewOutput {
            status: RenderStatus::Rendered,
            total_pages: 1,
            pages: vec![PreviewPage {
                page: 1,
                svg,
                svg_sha256: "s".into(),
                png,
                png_sha256: None,
            }],
            notes: vec![],
            diagnostics: vec![],
            render_millis: 1,
            metadata: PreviewMetadata {
                document_id: "doc-test".into(),
                content_revision: 0,
                layout_revision: "layout-test".into(),
                input_sha256: "i".into(),
                renderer: json!({}),
                sandbox: json!({}),
                fonts: json!({}),
                page_conditions: json!({}),
                coverage: "unknown",
                native_comparison: "not_checked",
            },
        }
    }
    fn doc() -> Document {
        Document::open(
            include_bytes!("../../../tests/fixtures/adversarial/valid_base.hwpx").to_vec(),
            &Limits::default(),
        )
        .unwrap()
    }
    #[test]
    fn snapshot_budget_is_checked_before_starting_a_renderer() {
        let adapter = RhwpAdapter::new(hae_render_rhwp::AdapterConfig::new(
            "/nonexistent/rhwp",
            "/nonexistent/fonts",
        ));
        let limits = Limits {
            max_session_memory: 1,
            ..Limits::default()
        };
        let params: DocumentPreviewParams = serde_json::from_value(json!({})).unwrap();
        let err = render(&adapter, &doc(), 0, &params, &limits, None).unwrap_err();
        assert_eq!(err.diagnostic().code, Code::ResourceLimit);
        assert_eq!(
            err.diagnostic().reason.as_deref(),
            Some("preview_snapshot_limit")
        );
    }
    #[test]
    fn png_only_failure_is_not_reported_as_rendered() {
        let (v, _) = assemble(
            &doc(),
            output(b"<svg/>".to_vec(), None),
            true,
            false,
            &Limits::default(),
            None,
        )
        .unwrap();
        assert_eq!(v["status"], "failed");
        assert_eq!(v["pages"][0]["omitted"], "png_failed");
    }
    #[test]
    fn escaped_svg_is_sized_as_json_before_being_inlined() {
        let svg = format!("<svg><!--{}--></svg>", "\"".repeat(1_100_000)).into_bytes();
        let (v, _) = assemble(
            &doc(),
            output(svg, None),
            false,
            true,
            &Limits::default(),
            None,
        )
        .unwrap();
        assert!(
            serde_json::to_vec(&v).unwrap().len() <= Limits::default().max_response_bytes as usize
        );
        assert_eq!(v["pages"][0]["omitted"], "response_limit");
        assert_eq!(v["status"], "failed");
    }
    #[test]
    fn oversized_svg_is_delivered_as_an_exact_output_root_artifact() {
        let td =
            hae_render_rhwp::sandbox::TempDir::new_in(&std::env::temp_dir(), "preview-artifact-")
                .unwrap();
        let limits = Limits::default();
        let roots = IoRoots::new(td.path(), td.path(), limits).unwrap();
        let svg = format!("<svg><!--{}--></svg>", "\"".repeat(2_300_000)).into_bytes();
        let (v, _) = assemble(
            &doc(),
            output(svg.clone(), None),
            false,
            true,
            &limits,
            Some(&roots),
        )
        .unwrap();
        let artifact = &v["pages"][0]["svgArtifact"];
        let path = std::path::Path::new(artifact["path"].as_str().expect("SVG artifact path"));
        assert_eq!(path.parent(), Some(td.path()));
        assert_eq!(std::fs::read(path).unwrap(), svg);
        assert_eq!(artifact["sha256"], hae_package::sha256_hex(&svg));
        assert_eq!(v["status"], "rendered");
        assert!(serde_json::to_vec(&v).unwrap().len() < limits.max_response_bytes as usize);
    }
}
