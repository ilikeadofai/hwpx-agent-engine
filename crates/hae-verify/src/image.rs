//! Bounded static PNG/JPEG structure checks and source-preserving media swap.
use crate::{attr, change, fail, hp, parse, unsupported};
use hae_diag::{Limits, Result};
use hae_package::{PackageInfo, PackageState, XmlMap, NS_HC, NS_HP, NS_HP10};
use hae_xml::{escape_attr, XmlDoc};
use std::sync::Arc;

const MAX_PIXELS: u64 = 40_960_000;
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

fn opf(ns: Option<&str>) -> bool {
    matches!(
        ns,
        Some("http://www.idpf.org/2007/opf/") | Some("http://www.idpf.org/2007/opf")
    )
}

fn check_dimensions((w, h): (u32, u32), limits: &Limits) -> Result<()> {
    if w == 0 || h == 0 {
        return Err(fail("zero image dimension"));
    }
    if u64::from(w) * u64::from(h) > limits.max_image_pixels.min(MAX_PIXELS) {
        return Err(fail("image pixel limit exceeded"));
    }
    Ok(())
}

fn png_shape(bytes: &[u8], limits: &Limits) -> Result<(u32, u32)> {
    let bad = || fail("truncated or malformed PNG chunks");
    if bytes.len() < 33 || &bytes[8..16] != b"\0\0\0\rIHDR" {
        return Err(bad());
    }
    let dims = (
        u32::from_be_bytes(bytes[16..20].try_into().map_err(|_| bad())?),
        u32::from_be_bytes(bytes[20..24].try_into().map_err(|_| bad())?),
    );
    check_dimensions(dims, limits)?;
    let mut pos = 8usize;
    let mut ended = false;
    while pos < bytes.len() {
        let end_of_head = pos.checked_add(8).ok_or_else(bad)?;
        let head = bytes.get(pos..end_of_head).ok_or_else(bad)?;
        let len = u32::from_be_bytes(head[..4].try_into().map_err(|_| bad())?) as usize;
        let end = pos
            .checked_add(12)
            .and_then(|p| p.checked_add(len))
            .ok_or_else(bad)?;
        if end > bytes.len() {
            return Err(bad());
        }
        match &head[4..] {
            b"acTL" | b"fcTL" | b"fdAT" => {
                return Err(unsupported("animated PNG is not supported"));
            }
            b"IHDR" if pos != 8 => return Err(bad()),
            b"IEND" => {
                if len != 0 || end != bytes.len() {
                    return Err(bad());
                }
                ended = true;
            }
            _ => {}
        }
        pos = end;
    }
    if !ended {
        return Err(bad());
    }
    Ok(dims)
}

/// Framing-only JPEG walk: exactly one SOF0/1/2, then SOS, then terminal EOI.
/// Progressive JPEG may carry several SOS scans; those are not extra frames.
fn jpeg_shape(bytes: &[u8], limits: &Limits) -> Result<(u32, u32)> {
    let bad = || fail("truncated or malformed JPEG markers");
    let mut pos = 2usize;
    let mut dims = None;
    let mut in_scan = false;
    let mut saw_scan = false;
    while pos < bytes.len() {
        if in_scan {
            while pos < bytes.len() && bytes[pos] != 0xff {
                pos += 1;
            }
        }
        if bytes.get(pos) != Some(&0xff) {
            return Err(bad());
        }
        while bytes.get(pos) == Some(&0xff) {
            pos += 1;
        }
        let marker = *bytes.get(pos).ok_or_else(bad)?;
        pos += 1;
        match marker {
            0 if in_scan => continue,
            0xd0..=0xd7 if in_scan => continue,
            0xd9 => {
                return match dims {
                    Some(d) if saw_scan && pos == bytes.len() => Ok(d),
                    _ => Err(bad()),
                };
            }
            0 | 0xd8 | 0x01 | 0xd0..=0xd7 => return Err(bad()),
            _ => {}
        }
        in_scan = false;
        let end_of_len = pos.checked_add(2).ok_or_else(bad)?;
        let length = bytes.get(pos..end_of_len).ok_or_else(bad)?;
        let len = u16::from_be_bytes([length[0], length[1]]) as usize;
        if len < 2 {
            return Err(bad());
        }
        let end = pos.checked_add(len).ok_or_else(bad)?;
        let payload = bytes.get(pos + 2..end).ok_or_else(bad)?;
        if marker == 0xe2 && payload.starts_with(b"MPF\0") {
            return Err(unsupported("multi-picture JPEG is not supported"));
        }
        if (0xc0..=0xcf).contains(&marker) && ![0xc4, 0xc8, 0xcc].contains(&marker) {
            if dims.is_some() {
                return Err(unsupported("multiple JPEG frames are not supported"));
            }
            if ![0xc0, 0xc1, 0xc2].contains(&marker) {
                return Err(unsupported("unsupported JPEG coding process"));
            }
            if payload.len() < 6 {
                return Err(bad());
            }
            let d = (
                u32::from(u16::from_be_bytes([payload[3], payload[4]])),
                u32::from(u16::from_be_bytes([payload[1], payload[2]])),
            );
            check_dimensions(d, limits)?;
            dims = Some(d);
        }
        if marker == 0xda {
            if dims.is_none() {
                return Err(bad());
            }
            saw_scan = true;
            in_scan = true;
        }
        pos = end;
    }
    Err(bad())
}

fn validated_media_type(bytes: &[u8], limits: &Limits) -> Result<(&'static str, (u32, u32))> {
    if bytes.len() as u64 > limits.max_hwpx_bytes {
        return Err(fail("image input byte limit exceeded"));
    }
    if bytes.starts_with(PNG_SIGNATURE) {
        Ok(("image/png", png_shape(bytes, limits)?))
    } else if bytes.starts_with(&[0xff, 0xd8]) {
        Ok(("image/jpeg", jpeg_shape(bytes, limits)?))
    } else {
        Err(unsupported("only static PNG and JPEG images are supported"))
    }
}

fn is_picture_ns(ns: Option<&str>) -> bool {
    matches!(ns, Some(NS_HP) | Some(NS_HP10))
}

fn check_picture(doc: &XmlDoc, picture: usize) -> Result<usize> {
    let images: Vec<_> = doc
        .el(picture)
        .children
        .iter()
        .copied()
        .filter(|&i| doc.el(i).is(NS_HC, "img"))
        .collect();
    if images.len() != 1 {
        return Err(unsupported("image replacement requires one embedded image"));
    }
    let img = doc.el(images[0]);
    img.attr("binaryItemIDRef")
        .ok_or_else(|| unsupported("image has no binary reference"))?;
    if img.attr_value("effect").is_some_and(|v| v != "REAL_PIC")
        || ["bright", "contrast", "alpha"]
            .iter()
            .any(|k| img.attr_value(k).is_some_and(|v| v.parse::<i64>() != Ok(0)))
        || doc
            .el(picture)
            .attr_value("reverse")
            .is_some_and(|v| v != "0")
    {
        return Err(unsupported("image has effects"));
    }
    let children = doc.el(picture).children.clone();
    for &c in &children {
        if !is_picture_ns(doc.el(c).ns.as_deref()) {
            continue;
        }
        match doc.el(c).local.as_str() {
            "effects" => {
                if !doc.el(c).children.is_empty()
                    || !doc.el(c).attrs.is_empty()
                    || doc
                        .el(c)
                        .items
                        .iter()
                        .filter_map(|i| i.text())
                        .any(|t| !t.trim().is_empty())
                {
                    return Err(unsupported("image has effects"));
                }
            }
            "shadow" | "glow" | "softEdge" | "reflection" => {
                return Err(unsupported("image has effects"));
            }
            "imgClip" => {
                let element = doc.el(c);
                let number =
                    |key: &str| element.attr_value(key).and_then(|v| v.parse::<i64>().ok());
                if number("left") != Some(0) || number("top") != Some(0) {
                    return Err(unsupported("cropped image is not supported"));
                }
                let dim = children
                    .iter()
                    .map(|&i| doc.el(i))
                    .find(|e| e.local == "imgDim" && is_picture_ns(e.ns.as_deref()));
                for (edge, size) in [("right", "dimwidth"), ("bottom", "dimheight")] {
                    match number(edge) {
                        Some(0) => {}
                        Some(v)
                            if v > 0
                                && dim
                                    .and_then(|d| d.attr_value(size))
                                    .and_then(|v| v.parse::<i64>().ok())
                                    == Some(v) => {}
                        _ => return Err(unsupported("cropped image is not supported")),
                    }
                }
            }
            _ => {}
        }
    }
    Ok(images[0])
}

fn is_safe_bindata(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("BinData/") else {
        return false;
    };
    if rest.is_empty()
        || name.contains('\\')
        || name.contains('\0')
        || name.contains(':')
        || name.starts_with('/')
    {
        return false;
    }
    rest.split('/')
        .all(|seg| !seg.is_empty() && seg != "." && seg != "..")
}

fn validate_manifest_item(
    doc: &XmlDoc,
    idx: usize,
    id: &str,
    href: &str,
    media_type: &str,
) -> Result<()> {
    let element = doc.el(idx);
    if !element.children.is_empty() {
        return Err(fail("new manifest item must be empty"));
    }
    for item in &element.items {
        match item {
            hae_xml::Item::Text { text, .. } if text.trim().is_empty() => {}
            _ => return Err(fail("new manifest item must be empty")),
        }
    }
    if let Some(end) = &element.end {
        if !doc
            .raw(element.start.end..end.start)
            .iter()
            .all(|b| b.is_ascii_whitespace())
        {
            return Err(fail("new manifest item must be empty"));
        }
    }
    for a in &element.attrs {
        if a.qname == "xmlns" || a.qname.starts_with("xmlns:") {
            continue;
        }
        if a.ns.is_some() {
            return Err(fail("smuggled manifest attribute"));
        }
        match a.local.as_str() {
            "id" | "href" | "media-type" | "isEmbeded" => {}
            _ => return Err(fail("smuggled manifest attribute")),
        }
    }
    if element.attr_value("id") != Some(id) {
        return Err(fail("manifest item id mismatch"));
    }
    if element.attr_value("href") != Some(href) {
        return Err(fail("manifest item href mismatch"));
    }
    if element.attr_value("media-type") != Some(media_type) {
        return Err(fail("manifest item media type mismatch"));
    }
    // Hancom resolves a media item without `isEmbeded="1"` as an external link
    // and prompts for the picture path instead of using the embedded bytes.
    if element.attr_value("isEmbeded") != Some("1") {
        return Err(fail("new media item must set isEmbeded=\"1\""));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn check(
    state: &mut PackageState,
    info: &PackageInfo,
    candidate: &PackageState,
    candidate_info: &PackageInfo,
    candidate_xml: &XmlMap,
    entry: &str,
    doc: &XmlDoc,
    target: usize,
    _path: &[usize],
    bytes: &[u8],
    limits: &Limits,
) -> Result<bool> {
    let (media_type, (new_w, new_h)) = validated_media_type(bytes, limits)?;
    if !hp(doc.el(target), "pic") {
        return Err(unsupported("image target is not a picture"));
    }
    let img = check_picture(doc, target)?;
    let old_id = attr(doc, img, "binaryItemIDRef")?.value.clone();
    let old_item = info
        .item_by_id(&old_id)
        .ok_or_else(|| fail(format!("unresolved binaryItemIDRef: {old_id}")))?;
    let old_payload: &[u8] = state
        .get(&old_item.entry)
        .ok_or_else(|| fail(format!("missing media entry {}", old_item.entry)))?;
    if old_payload == bytes {
        return Ok(false);
    }
    if candidate_info.rootfile != info.rootfile {
        return Err(fail("candidate rootfile changed"));
    }
    let candidate_doc = candidate_xml
        .get(entry)
        .ok_or_else(|| fail("candidate is missing the target section"))?;
    let candidate_target = crate::stable_at(candidate_doc, &crate::stable_path(doc, target))?;
    if !hp(candidate_doc.el(candidate_target), "pic") {
        return Err(fail("candidate picture target changed shape"));
    }
    let candidate_images: Vec<_> = candidate_doc
        .el(candidate_target)
        .children
        .iter()
        .copied()
        .filter(|&i| candidate_doc.el(i).is(NS_HC, "img"))
        .collect();
    if candidate_images.len() != 1 {
        return Err(fail("candidate picture changed shape"));
    }
    let new_id = candidate_doc
        .el(candidate_images[0])
        .attr("binaryItemIDRef")
        .ok_or_else(|| fail("candidate picture lost its binary reference"))?
        .value
        .clone();
    if new_id == old_id {
        return Err(fail("candidate lacks the requested image change"));
    }
    if info
        .manifest
        .iter()
        .any(|m| m.id.eq_ignore_ascii_case(&new_id))
    {
        return Err(fail("reused manifest ID for new image"));
    }
    let new_item = candidate_info
        .manifest
        .iter()
        .find(|m| m.id == new_id)
        .ok_or_else(|| fail("candidate manifest lacks the new image item"))?;
    if new_item.media_type != media_type {
        return Err(fail(
            "candidate manifest media type does not match the validated image",
        ));
    }
    if !is_safe_bindata(&new_item.entry) {
        return Err(fail("unsafe new media entry name"));
    }
    if state
        .entries
        .iter()
        .any(|e| e.name.eq_ignore_ascii_case(&new_item.entry))
    {
        return Err(fail("new media entry name already exists"));
    }
    let candidate_payload: &[u8] = candidate
        .get(&new_item.entry)
        .ok_or_else(|| fail("candidate lacks the new media payload"))?;
    if candidate_payload != bytes {
        return Err(fail(
            "candidate media payload differs from the requested bytes",
        ));
    }
    let candidate_root = candidate_xml
        .get(&info.rootfile)
        .ok_or_else(|| fail("candidate is missing the package manifest"))?;
    let manifests: Vec<_> = candidate_root
        .iter()
        .filter(|(_, e)| e.local == "manifest" && opf(e.ns.as_deref()))
        .map(|(i, _)| i)
        .collect();
    if manifests.len() != 1 {
        return Err(fail("candidate manifest shape changed"));
    }
    let manifest_children = candidate_root.el(manifests[0]).children.clone();
    let mut found = None;
    for &c in &manifest_children {
        let element = candidate_root.el(c);
        if element.local == "item"
            && opf(element.ns.as_deref())
            && element.attr_value("id") == Some(new_id.as_str())
        {
            if found.is_some() {
                return Err(fail("duplicate new manifest item"));
            }
            found = Some(c);
        }
    }
    let item_idx = found.ok_or_else(|| fail("candidate manifest lacks the new image item"))?;
    validate_manifest_item(
        candidate_root,
        item_idx,
        &new_id,
        &new_item.href,
        media_type,
    )?;
    let token = candidate_root
        .raw(candidate_root.el(item_idx).outer())
        .to_vec();
    let state_root = parse(state, &info.rootfile, limits)?;
    let state_manifests: Vec<_> = state_root
        .iter()
        .filter(|(_, e)| e.local == "manifest" && opf(e.ns.as_deref()))
        .map(|(i, _)| i)
        .collect();
    if state_manifests.len() != 1 {
        return Err(fail("manifest shape changed"));
    }
    let close = state_root
        .el(state_manifests[0])
        .end
        .clone()
        .ok_or_else(|| fail("empty manifest cannot contain existing image"))?;
    change(
        state,
        &info.rootfile,
        &state_root,
        vec![(close.start..close.start, token)],
    )?;
    // The source-image geometry follows the media, not the display frame, so a
    // candidate that swaps the bytes must also carry the new dimensions in
    // `imgDim`/`imgClip`. Hancom shows only the old corner otherwise.
    let mut splices = vec![(
        attr(doc, img, "binaryItemIDRef")?.value_span.clone(),
        escape_attr(&new_id).into_bytes(),
    )];
    // Hancom reads these attributes in HWPUNIT at a fixed 75 units per pixel,
    // not in raw pixels (verified by screenshot: raw pixels make only the
    // top-left corner render).
    let units = |px: u32| {
        px.checked_mul(75)
            .ok_or_else(|| fail("image dimension exceeds the HWPUNIT range"))
    };
    let (hwp_w, hwp_h) = (units(new_w)?, units(new_h)?);
    for &child in &doc.el(target).children {
        let e = doc.el(child);
        let attrs: &[(&str, u32)] = if hp(e, "imgDim") {
            &[("dimwidth", hwp_w), ("dimheight", hwp_h)]
        } else if hp(e, "imgClip") {
            &[("right", hwp_w), ("bottom", hwp_h)]
        } else {
            &[]
        };
        for (name, value) in attrs {
            if let Some(a) = e.attr(name) {
                splices.push((a.value_span.clone(), value.to_string().into_bytes()));
            }
        }
    }
    change(state, entry, doc, splices)?;
    state.add(new_item.entry.clone(), Arc::from(bytes.to_vec()), 0);
    Ok(true)
}
