//! New-node image insertion: a native `hp:pic` placed above an existing caption.
//!
//! The engine never reads an image path. The caller freezes fully decoded
//! PNG/JPEG bytes once ([`ImageBytes`]) and passes the desired display size in
//! HWPUNIT. This module derives exactly three things and nothing else:
//!
//! * one `hp:pic` element (returned to the caller, which owns the `hp:p`/`hp:run`
//!   carrier, the node remap, the lineseg cache removal and the report);
//! * one new frozen media entry appended to the package state (byte-identical
//!   to the frozen input, never a re-encode, never touching existing media);
//! * one `content.hpf` manifest item appended before `</opf:manifest>`.
//!
//! The source document is not required to contain any picture. The naming,
//! manifest namespace and id/name collision policies mirror
//! [`crate::image::image_replace`]; the `hp:pic` defaults and the HWPUNIT rules
//! are recorded in `docs/decisions/new-node-image-notes.md`.
//!
//! [`crate::apply`] integrates the shared `EditOp`, carrier, remap and report.

use crate::ImageBytes;
use hae_diag::{Code, EngineError, Limits, Result, Stage};
use hae_model::is_hp;
use hae_package::{PackageState, NS_HC, NS_HP, NS_HP10};
use hae_xml::{escape_attr, ParseOptions, XmlDoc};
use std::collections::HashSet;

/// HWPUNIT per source-image pixel. Hancom stores source-image geometry at a
/// fixed 75 units per pixel (see the image.replace notes and the fixtures).
const HWPUNIT_PER_PIXEL: u32 = 75;
/// Largest accepted display dimension in HWPUNIT (about 3.5 m).
const MAX_DISPLAY_HWPUNIT: u32 = 1_000_000;
/// Fresh drawing-object id base used by the verified workbench assembler and
/// the sibling `equation_insert` module, so new objects never reuse small
/// sample ids.
const OBJECT_ID_BASE: u64 = 1_300_000_000;
/// Element locals that own a numeric drawing-object `id` in a section document.
/// Same document-wide scan set as `table_insert::SHAPES` and the union that
/// `equation_insert::OBJECT_LOCALS` counts (both include `video`); paragraph ids
/// are a separate space. Kept as a static copy so the verifier derivation stays
/// independent of the editor (review r1: the 18-name set missed `video`).
const SHAPE_ELEMENTS: [&str; 19] = [
    "tbl",
    "pic",
    "equation",
    "ole",
    "container",
    "chart",
    "textart",
    "shape",
    "group",
    "rect",
    "ellipse",
    "line",
    "arc",
    "polygon",
    "curve",
    "connectLine",
    "button",
    "cellzone",
    "video",
];

fn invalid(reason: &str, message: impl Into<String>) -> EngineError {
    EngineError::new(Code::InvalidArgument, Stage::Validate, message).with_reason(reason)
}

fn limit(message: impl Into<String>) -> EngineError {
    EngineError::new(Code::ResourceLimit, Stage::Validate, message)
}

fn opf(ns: Option<&str>) -> bool {
    matches!(
        ns,
        Some("http://www.idpf.org/2007/opf/") | Some("http://www.idpf.org/2007/opf")
    )
}

fn opts(limits: &Limits) -> ParseOptions {
    ParseOptions {
        max_depth: limits.max_xml_depth as usize,
    }
}

fn check_display(width: u32, height: u32) -> Result<()> {
    if width == 0 || height == 0 || width > MAX_DISPLAY_HWPUNIT || height > MAX_DISPLAY_HWPUNIT {
        return Err(invalid(
            "image_dimensions",
            "picture display size must be 1..=1000000 HWPUNIT",
        ));
    }
    Ok(())
}

/// The HWPML paragraph namespace the document actually uses. A binding to any
/// other URI is ignored so the emitted `hp:` prefix can never resolve into a
/// foreign namespace.
fn paragraph_ns(doc: &XmlDoc) -> String {
    for (_, e) in doc.iter() {
        for a in &e.attrs {
            if a.qname == "xmlns:hp" && (a.value == NS_HP || a.value == NS_HP10) {
                return a.value.clone();
            }
        }
    }
    NS_HP.to_string()
}

/// The HWPML core namespace the document actually uses, never a foreign one.
fn core_ns(doc: &XmlDoc) -> String {
    for (_, e) in doc.iter() {
        for a in &e.attrs {
            if a.qname == "xmlns:hc" && a.value == NS_HC {
                return a.value.clone();
            }
        }
    }
    NS_HC.to_string()
}

fn max_shape_id(doc: &XmlDoc) -> Option<u64> {
    let mut max = None;
    for (_, e) in doc.iter() {
        if !is_hp(e.ns.as_deref()) || !SHAPE_ELEMENTS.contains(&e.local.as_str()) {
            continue;
        }
        if let Some(v) = e.attr_value("id").and_then(|v| v.parse::<u64>().ok()) {
            max = Some(max.map_or(v, |m: u64| m.max(v)));
        }
    }
    max
}

/// One past the highest drawing-object `id` in the supplied documents, at least
/// [`OBJECT_ID_BASE`]. Document-wide and deterministic for a given pre-edit
/// state, matching the sibling object inserters.
fn next_shape_id(docs: &[&XmlDoc]) -> Result<u64> {
    let mut max = OBJECT_ID_BASE - 1;
    for d in docs {
        if let Some(v) = max_shape_id(d) {
            max = max.max(v);
        }
    }
    max.checked_add(1)
        .filter(|v| *v <= u64::from(u32::MAX))
        .ok_or_else(|| limit("picture shape identifiers exhausted"))
}

/// Complete native `hp:pic` element. Child order and attributes follow the
/// bodojaryo-reference.hwpx sample and the editing fixtures; both `hp:` and
/// `hc:` are declared locally so the element is independent of the section's
/// prefixes. Display size is `(w, h)`; source-image geometry is `(iw, ih)`.
#[allow(clippy::too_many_arguments)]
fn picture_xml(
    pic_id: u64,
    media_id: &str,
    hp_ns: &str,
    hc_ns: &str,
    w: u32,
    h: u32,
    iw: u32,
    ih: u32,
) -> Vec<u8> {
    let mut s = String::with_capacity(1024);
    s.push_str(&format!(
        concat!(
            r#"<hp:pic xmlns:hp="{}" xmlns:hc="{}" id="{pic_id}" zOrder="0" "#,
            r#"numberingType="PICTURE" textWrap="TOP_AND_BOTTOM" textFlow="BOTH_SIDES" "#,
            r#"lock="0" dropcapstyle="None" href="" groupLevel="0" instid="{pic_id}" reverse="0">"#
        ),
        escape_attr(hp_ns),
        escape_attr(hc_ns),
        pic_id = pic_id,
    ));
    s.push_str(r#"<hp:offset x="0" y="0"/>"#);
    s.push_str(&format!(r#"<hp:orgSz width="{w}" height="{h}"/>"#));
    s.push_str(&format!(r#"<hp:curSz width="{w}" height="{h}"/>"#));
    s.push_str(r#"<hp:flip horizontal="0" vertical="0"/>"#);
    s.push_str(r#"<hp:rotationInfo angle="0" centerX="0" centerY="0" rotateimage="0"/>"#);
    s.push_str(concat!(
        r#"<hp:renderingInfo>"#,
        r#"<hc:transMatrix e1="1" e2="0" e3="0" e4="0" e5="1" e6="0"/>"#,
        r#"<hc:scaMatrix e1="1" e2="0" e3="0" e4="0" e5="1" e6="0"/>"#,
        r#"<hc:rotMatrix e1="1" e2="0" e3="0" e4="0" e5="1" e6="0"/>"#,
        r#"</hp:renderingInfo>"#
    ));
    s.push_str(&format!(
        concat!(
            r#"<hp:imgRect><hc:pt0 x="0" y="0"/><hc:pt1 x="{w}" y="0"/>"#,
            r#"<hc:pt2 x="{w}" y="{h}"/><hc:pt3 x="0" y="{h}"/></hp:imgRect>"#
        ),
        w = w,
        h = h
    ));
    s.push_str(&format!(
        r#"<hp:imgClip left="0" right="{iw}" top="0" bottom="{ih}"/>"#
    ));
    s.push_str(r#"<hp:inMargin left="0" right="0" top="0" bottom="0"/>"#);
    s.push_str(&format!(r#"<hp:imgDim dimwidth="{iw}" dimheight="{ih}"/>"#));
    s.push_str(&format!(
        r#"<hc:img binaryItemIDRef="{}" bright="0" contrast="0" effect="REAL_PIC" alpha="0"/>"#,
        escape_attr(media_id)
    ));
    s.push_str(r#"<hp:effects/>"#);
    s.push_str(&format!(
        concat!(
            r#"<hp:sz width="{w}" widthRelTo="ABSOLUTE" height="{h}" "#,
            r#"heightRelTo="ABSOLUTE" protect="0"/>"#
        ),
        w = w,
        h = h
    ));
    s.push_str(concat!(
        r#"<hp:pos treatAsChar="1" affectLSpacing="0" flowWithText="0" "#,
        r#"allowOverlap="0" holdAnchorAndSO="0" vertRelTo="PARA" horzRelTo="COLUMN" "#,
        r#"vertAlign="TOP" horzAlign="LEFT" vertOffset="0" horzOffset="0"/>"#
    ));
    s.push_str(r#"<hp:outMargin left="0" right="0" top="0" bottom="0"/>"#);
    s.push_str("</hp:pic>");
    s.into_bytes()
}

/// One embedded media manifest item with an explicit local `hae` namespace so
/// it does not depend on the manifest's prefixes (`isEmbeded="1"` is required;
/// Hancom otherwise treats the media as an external link).
fn item_xml(manifest_ns: &str, id: &str, name: &str, media_type: &str) -> String {
    format!(
        r#"<hae:item xmlns:hae="{}" id="{}" href="{}" media-type="{}" isEmbeded="1"/>"#,
        escape_attr(manifest_ns),
        escape_attr(id),
        escape_attr(name),
        escape_attr(media_type)
    )
}

/// Prepare one new picture node above the caption paragraph at `paragraph`.
///
/// Returns only the `hp:pic` element bytes. On success `state` gains one new
/// frozen media entry and one `content.hpf` manifest item; no existing entry,
/// media payload or namespace is modified. All validation (display bounds,
/// paragraph shape, manifest shape and package growth) happens before any
/// state mutation.
pub(crate) fn prepare(
    state: &mut PackageState,
    xml: &XmlDoc,
    paragraph: usize,
    image: &ImageBytes,
    width: u32,
    height: u32,
    limits: &Limits,
) -> Result<Vec<u8>> {
    check_display(width, height)?;
    match xml.elements().get(paragraph) {
        Some(el) if is_hp(el.ns.as_deref()) && el.local == "p" => {}
        Some(_) => {
            return Err(invalid(
                "image_paragraph",
                "image insertion requires a caption paragraph",
            ))
        }
        None => {
            return Err(invalid(
                "image_paragraph",
                "caption paragraph is outside the section",
            ))
        }
    }
    let (px_w, px_h) = image.dimensions();
    let source_w = px_w
        .checked_mul(HWPUNIT_PER_PIXEL)
        .ok_or_else(|| limit("image width exceeds the HWPUNIT range"))?;
    let source_h = px_h
        .checked_mul(HWPUNIT_PER_PIXEL)
        .ok_or_else(|| limit("image height exceeds the HWPUNIT range"))?;

    let (info, xmlmap) = hae_package::analyze(state, true, limits)?;
    let hpf = xmlmap
        .get(&info.rootfile)
        .cloned()
        .ok_or_else(|| invalid("manifest_shape", "package manifest is missing"))?;
    let manifests: Vec<usize> = hpf
        .iter()
        .filter(|(_, e)| e.local == "manifest" && opf(e.ns.as_deref()))
        .map(|(i, _)| i)
        .collect();
    if manifests.len() != 1 {
        return Err(invalid(
            "manifest_shape",
            "expected exactly one package manifest",
        ));
    }
    let manifest = hpf.el(manifests[0]);
    let close = manifest
        .end
        .clone()
        .ok_or_else(|| invalid("manifest_shape", "empty manifest cannot receive media"))?;
    let manifest_ns = manifest.ns.as_deref().unwrap_or_default().to_string();

    // Same id/name algorithm as image.replace: `hae-image-{i}` and
    // `BinData/hae-image-{i}.{png|jpg}`, first pair free of an existing manifest
    // id and package entry name (case-insensitive).
    let used_ids: HashSet<String> = info
        .manifest
        .iter()
        .map(|m| m.id.to_ascii_lowercase())
        .collect();
    let used_names: HashSet<String> = state
        .entries
        .iter()
        .map(|e| e.name.to_ascii_lowercase())
        .collect();
    let (id, name) = (1u64..)
        .map(|i| {
            let id = format!("hae-image-{i}");
            let name = format!(
                "BinData/{id}.{}",
                if image.format() == "png" {
                    "png"
                } else {
                    "jpg"
                }
            );
            (id, name)
        })
        .find(|(id, name)| {
            !used_ids.contains(&id.to_ascii_lowercase())
                && !used_names.contains(&name.to_ascii_lowercase())
        })
        .ok_or_else(|| limit("image resource identifiers exhausted"))?;

    let mut docs: Vec<&XmlDoc> = xmlmap.values().map(|d| &**d).collect();
    docs.push(xml);
    let pic_id = next_shape_id(&docs)?;

    let hp_ns = paragraph_ns(xml);
    let hc_ns = core_ns(xml);
    let picture = picture_xml(
        pic_id, &id, &hp_ns, &hc_ns, width, height, source_w, source_h,
    );
    let item = item_xml(&manifest_ns, &id, &name, image.media_type());

    let old_total = state.total_bytes();
    let growth = (picture.len() as u64)
        .saturating_add(item.len() as u64)
        .saturating_add(image.bytes().len() as u64);
    if state.entries.len() as u64 >= limits.max_entries
        || old_total.saturating_add(growth) > limits.max_inflate_total
        || old_total
            .saturating_add(growth)
            .saturating_add(image.bytes().len() as u64)
            > limits.max_session_memory
    {
        return Err(limit("image insertion package budget exceeded"));
    }

    let new_hpf = hpf.splice(
        vec![(close.start..close.start, item.into_bytes())],
        opts(limits),
    )?;
    state.set(&info.rootfile, new_hpf.bytes().clone());
    // Frozen, byte-identical payload: the same allocation is shared, never
    // re-encoded, and existing media entries are left untouched.
    state.add(name, image.bytes().clone(), 0);
    Ok(picture)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hae_package::{Entry, Origin, MIMETYPE};
    use std::sync::Arc;

    const SECTION: &str = concat!(
        r#"<?xml version="1.0" encoding="UTF-8"?>"#,
        r#"<hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" "#,
        r#"xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" "#,
        r#"xmlns:hc="http://www.hancom.co.kr/hwpml/2011/core">"#,
        r#"<hp:p id="1" paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>그림 1. 설명</hp:t></hp:run></hp:p>"#,
        r#"</hs:sec>"#
    );

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut e = png::Encoder::new(&mut bytes, w, h);
            e.set_color(png::ColorType::Rgb);
            let data = vec![0u8; (w * h * 3) as usize];
            e.write_header().unwrap().write_image_data(&data).unwrap();
        }
        bytes
    }

    fn entry(name: &str, payload: &[u8]) -> Entry {
        Entry {
            name: name.to_string(),
            payload: Arc::from(payload.to_vec()),
            origin: Origin::Original,
        }
    }

    fn state(section: &str, extra_manifest: &str, extra: &[(&str, Vec<u8>)]) -> PackageState {
        let container = concat!(
            r#"<?xml version="1.0" encoding="UTF-8"?>"#,
            r#"<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container">"#,
            r#"<rootfiles><rootfile full-path="Contents/content.hpf"/></rootfiles></container>"#
        );
        let hpf = format!(
            concat!(
                r#"<?xml version="1.0" encoding="UTF-8"?>"#,
                r#"<opf:package xmlns:opf="http://www.idpf.org/2007/opf/">"#,
                r#"<opf:manifest><opf:item id="s" href="Contents/section0.xml" media-type="application/xml"/>"#,
                r#"{extra_manifest}</opf:manifest>"#,
                r#"<opf:spine><opf:itemref idref="s"/></opf:spine></opf:package>"#
            ),
            extra_manifest = extra_manifest
        );
        let mut entries = vec![
            entry("mimetype", MIMETYPE.as_bytes()),
            entry("META-INF/container.xml", container.as_bytes()),
            entry("Contents/content.hpf", hpf.as_bytes()),
            entry("Contents/section0.xml", section.as_bytes()),
        ];
        for (n, b) in extra {
            entries.push(entry(n, b));
        }
        PackageState { entries }
    }

    fn paragraph_of(doc: &XmlDoc) -> usize {
        doc.iter()
            .find(|(_, e)| is_hp(e.ns.as_deref()) && e.local == "p")
            .unwrap()
            .0
    }

    fn doc(section: &str) -> XmlDoc {
        XmlDoc::parse(section.as_bytes().to_vec()).unwrap()
    }

    #[test]
    fn inserts_picture_and_frozen_media_when_source_has_no_picture() {
        let limits = Limits::default();
        let mut st = state(SECTION, "", &[]);
        let section = doc(SECTION);
        let image = ImageBytes::from_bytes(png(2, 1), &limits).unwrap();
        let pic = prepare(
            &mut st,
            &section,
            paragraph_of(&section),
            &image,
            300,
            150,
            &limits,
        )
        .unwrap();
        let text = String::from_utf8(pic.clone()).unwrap();
        assert!(text.starts_with("<hp:pic "));
        assert!(text.ends_with("</hp:pic>"));
        assert!(text.contains(r#"id="1300000000" zOrder="0""#));
        assert!(text.contains(r#"binaryItemIDRef="hae-image-1""#));
        assert!(text.contains(r#"xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph""#));
        assert!(text.contains(r#"xmlns:hc="http://www.hancom.co.kr/hwpml/2011/core""#));
        // Display size is the requested size; source geometry is pixel * 75.
        assert!(text.contains(r#"<hp:imgDim dimwidth="150" dimheight="75"/>"#));
        assert!(text.contains(r#"<hp:imgClip left="0" right="150" top="0" bottom="75"/>"#));
        assert!(text.contains(r#"<hp:sz width="300" widthRelTo="ABSOLUTE" height="150""#));
        assert!(text.contains(r#"<hp:pos treatAsChar="1""#));
        // The emitted element parses as a native picture on its own.
        let parsed = XmlDoc::parse(pic).unwrap();
        assert!(is_hp(parsed.el(parsed.root()).ns.as_deref()));
        assert_eq!(parsed.el(parsed.root()).local, "pic");
        // Exactly one new frozen media resource, byte-identical to the input.
        assert_eq!(st.entries.len(), 5);
        let added = st.entries.last().unwrap();
        assert_eq!(added.name, "BinData/hae-image-1.png");
        assert_eq!(&added.payload[..], &image.bytes()[..]);
        // Source section is untouched; only the manifest gained one item.
        assert_eq!(
            &st.get("Contents/section0.xml").unwrap()[..],
            SECTION.as_bytes()
        );
        let hpf = String::from_utf8(st.get("Contents/content.hpf").unwrap().to_vec()).unwrap();
        assert!(hpf.contains(concat!(
            r#"<hae:item xmlns:hae="http://www.idpf.org/2007/opf/" id="hae-image-1" "#,
            r#"href="BinData/hae-image-1.png" media-type="image/png" isEmbeded="1"/>"#
        )));
    }

    #[test]
    fn preserves_existing_media_and_skips_used_shape_ids() {
        let limits = Limits::default();
        let old = png(1, 1);
        let section = concat!(
            r#"<?xml version="1.0" encoding="UTF-8"?>"#,
            r#"<hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" "#,
            r#"xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" "#,
            r#"xmlns:hc="http://www.hancom.co.kr/hwpml/2011/core">"#,
            r#"<hp:p id="1" paraPrIDRef="0"><hp:run charPrIDRef="0">"#,
            r#"<hp:pic id="1300000000"><hp:sz width="10" height="10"/><hc:img binaryItemIDRef="old" bright="0" contrast="0" effect="REAL_PIC" alpha="0"/></hp:pic>"#,
            r#"</hp:run></hp:p>"#,
            r#"<hp:p id="2" paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>그림 1. 설명</hp:t></hp:run></hp:p>"#,
            r#"</hs:sec>"#
        );
        let mut st = state(
            section,
            r#"<opf:item id="old" href="BinData/old.png" media-type="image/png" isEmbeded="1"/>"#,
            &[("BinData/old.png", old.clone())],
        );
        let d = doc(section);
        let caption = d
            .iter()
            .filter(|(_, e)| is_hp(e.ns.as_deref()) && e.local == "p")
            .nth(1)
            .unwrap()
            .0;
        let image = ImageBytes::from_bytes(png(2, 1), &limits).unwrap();
        let pic = prepare(&mut st, &d, caption, &image, 150, 75, &limits).unwrap();
        // Existing media bytes and entry are untouched.
        assert_eq!(&st.get("BinData/old.png").unwrap()[..], &old[..]);
        assert_eq!(
            st.entries.iter().position(|e| e.name == "BinData/old.png"),
            Some(4)
        );
        // The existing picture keeps 1300000000, so the new shape takes the next.
        let text = String::from_utf8(pic).unwrap();
        assert!(text.contains(r#"id="1300000001" zOrder="0""#));
        assert!(text.contains(r#"binaryItemIDRef="hae-image-1""#));
    }

    #[test]
    fn counts_video_in_the_shared_object_id_space() {
        // Review r1: `hp:video` carries a drawing-object `id` in the same
        // document-wide space as `hp:pic`, so a source that already uses the
        // base id must push the new picture to the next value. The synthetic
        // video element is only parsed, never interpreted.
        let limits = Limits::default();
        let section = concat!(
            r#"<?xml version="1.0" encoding="UTF-8"?>"#,
            r#"<hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" "#,
            r#"xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" "#,
            r#"xmlns:hc="http://www.hancom.co.kr/hwpml/2011/core">"#,
            r#"<hp:p id="1" paraPrIDRef="0"><hp:run charPrIDRef="0">"#,
            r#"<hp:video id="1300000000"><hp:sz width="10" height="10"/></hp:video>"#,
            r#"</hp:run></hp:p>"#,
            r#"<hp:p id="2" paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>그림 1. 설명</hp:t></hp:run></hp:p>"#,
            r#"</hs:sec>"#
        );
        let mut st = state(section, "", &[]);
        let d = doc(section);
        let caption = d
            .iter()
            .filter(|(_, e)| is_hp(e.ns.as_deref()) && e.local == "p")
            .nth(1)
            .unwrap()
            .0;
        let image = ImageBytes::from_bytes(png(2, 1), &limits).unwrap();
        let pic = prepare(&mut st, &d, caption, &image, 150, 75, &limits).unwrap();
        let text = String::from_utf8(pic).unwrap();
        assert!(text.contains(r#"id="1300000001" zOrder="0""#));
    }

    #[test]
    fn avoids_manifest_id_and_entry_name_collisions() {
        let limits = Limits::default();
        let mut st = state(
            SECTION,
            concat!(
                r#"<opf:item id="hae-image-1" href="BinData/hae-image-1.png" media-type="image/png" isEmbeded="1"/>"#,
                r#"<opf:item id="hae-image-2" href="BinData/hae-image-2.png" media-type="image/png" isEmbeded="1"/>"#
            ),
            &[
                ("BinData/hae-image-1.png", png(1, 1)),
                ("BinData/hae-image-2.png", png(1, 1)),
            ],
        );
        let section = doc(SECTION);
        let image = ImageBytes::from_bytes(png(2, 1), &limits).unwrap();
        let pic = prepare(
            &mut st,
            &section,
            paragraph_of(&section),
            &image,
            150,
            75,
            &limits,
        )
        .unwrap();
        let text = String::from_utf8(pic).unwrap();
        assert!(text.contains(r#"binaryItemIDRef="hae-image-3""#));
        assert_eq!(st.entries.last().unwrap().name, "BinData/hae-image-3.png");
    }

    #[test]
    fn rejects_invalid_dimensions_without_mutating_state() {
        let limits = Limits::default();
        let mut st = state(SECTION, "", &[]);
        let section = doc(SECTION);
        let image = ImageBytes::from_bytes(png(2, 1), &limits).unwrap();
        let before = st.payload_set_sha256();
        for (w, h) in [(0u32, 10u32), (10, 0), (1_000_001, 10), (10, 1_000_001)] {
            let err = prepare(
                &mut st,
                &section,
                paragraph_of(&section),
                &image,
                w,
                h,
                &limits,
            )
            .unwrap_err();
            assert_eq!(err.code(), Code::InvalidArgument);
        }
        assert_eq!(st.payload_set_sha256(), before);
    }

    #[test]
    fn uses_canonical_namespaces_regardless_of_section_prefixes() {
        let limits = Limits::default();
        // Alternate prefix for the paragraph namespace; no `xmns:hp` binding.
        let alt = concat!(
            r#"<?xml version="1.0" encoding="UTF-8"?>"#,
            r#"<hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" "#,
            r#"xmlns:para="http://www.hancom.co.kr/hwpml/2011/paragraph" "#,
            r#"xmlns:core="http://www.hancom.co.kr/hwpml/2011/core">"#,
            r#"<para:p id="1" paraPrIDRef="0"><para:run charPrIDRef="0"><para:t>설명</para:t></para:run></para:p>"#,
            r#"</hs:sec>"#
        );
        let mut st = state(alt, "", &[]);
        let section = doc(alt);
        let image = ImageBytes::from_bytes(png(2, 1), &limits).unwrap();
        let pic = prepare(
            &mut st,
            &section,
            paragraph_of(&section),
            &image,
            150,
            75,
            &limits,
        )
        .unwrap();
        let text = String::from_utf8(pic).unwrap();
        assert!(text.contains(r#"xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph""#));
        assert!(text.contains(r#"xmlns:hc="http://www.hancom.co.kr/hwpml/2011/core""#));
        // A foreign `hp` binding never leaks into the emitted element.
        let foreign = concat!(
            r#"<?xml version="1.0" encoding="UTF-8"?>"#,
            r#"<hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" "#,
            r#"xmlns:hp="urn:foreign" xmlns:hc="http://www.hancom.co.kr/hwpml/2011/core">"#,
            r#"<hp:p id="1" paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>x</hp:t></hp:run></hp:p></hs:sec>"#
        );
        let mut st = state(foreign, "", &[]);
        let section = doc(foreign);
        assert_eq!(
            prepare(&mut st, &section, 1, &image, 150, 75, &limits)
                .unwrap_err()
                .code(),
            Code::InvalidArgument
        );
    }
}
