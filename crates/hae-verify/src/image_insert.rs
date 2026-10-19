//! Independent new-node picture insertion for the save gate.
//!
//! Re-derives the `hp:pic` bytes, the new media entry and the `content.hpf`
//! manifest item from the request and the pre-edit package alone. It shares no
//! generator code with `hae-edit`; static PNG/JPEG structure uses the bounded
//! helper in [`crate::image`] (`validated_media_type`), the same policy
//! image.replace applies. The caller owns the `hp:p`/`hp:run` carrier, the node
//! remap, the lineseg cache removal and the expected/candidate comparison
//! (`docs/decisions/new-node-image-notes.md`).
//!
//! On success `state` (the independently derived expected state) gains one
//! media entry and one manifest item; the returned bytes are exactly the
//! expected `hp:pic` element. Existing media and entries are never modified.

use crate::{change, fail, hp, unsupported};
use hae_diag::{Limits, Result};
use hae_package::{PackageState, NS_HC, NS_HP, NS_HP10};
use hae_xml::{escape_attr, XmlDoc};
use std::collections::HashSet;
use std::sync::Arc;

/// HWPUNIT per source-image pixel (Hancom stores source geometry at 75/px).
const HWPUNIT_PER_PIXEL: u32 = 75;
/// Largest accepted display dimension in HWPUNIT (about 3.5 m).
const MAX_DISPLAY_HWPUNIT: u32 = 1_000_000;
/// Fresh drawing-object id base (matches the sibling inserter modules).
const OBJECT_ID_BASE: u64 = 1_300_000_000;
/// Object-bearing shapes whose numeric `id` share one document-wide namespace.
/// Same scan set as `table_insert::SHAPES` and the union that
/// `equation_insert::OBJECT_LOCALS` counts (both include `video`); paragraph ids
/// are separate. Independently maintained copy, not shared with `hae-edit`
/// (review r1: the 18-name set missed `video`).
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

fn opf(ns: Option<&str>) -> bool {
    matches!(
        ns,
        Some("http://www.idpf.org/2007/opf/") | Some("http://www.idpf.org/2007/opf")
    )
}

/// The HWPML paragraph namespace the document uses, ignoring foreign bindings.
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

/// The HWPML core namespace the document uses, ignoring foreign bindings.
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
        if !matches!(e.ns.as_deref(), Some(NS_HP) | Some(NS_HP10))
            || !SHAPE_ELEMENTS.contains(&e.local.as_str())
        {
            continue;
        }
        if let Some(v) = e.attr_value("id").and_then(|v| v.parse::<u64>().ok()) {
            max = Some(max.map_or(v, |m: u64| m.max(v)));
        }
    }
    max
}

fn next_shape_id(docs: &[&XmlDoc]) -> Result<u64> {
    let mut max = OBJECT_ID_BASE - 1;
    for d in docs {
        if let Some(v) = max_shape_id(d) {
            max = max.max(v);
        }
    }
    max.checked_add(1)
        .filter(|v| *v <= u64::from(u32::MAX))
        .ok_or_else(|| fail("picture shape identifiers exhausted"))
}

/// Complete expected `hp:pic` element (structure mirrors the editor contract,
/// re-derived here independently).
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

fn item_xml(manifest_ns: &str, id: &str, name: &str, media_type: &str) -> String {
    format!(
        r#"<hae:item xmlns:hae="{}" id="{}" href="{}" media-type="{}" isEmbeded="1"/>"#,
        escape_attr(manifest_ns),
        escape_attr(id),
        escape_attr(name),
        escape_attr(media_type)
    )
}

/// Independently derive the expected picture, media entry and manifest item for
/// one insert request and record them in `state`. Returns the expected
/// `hp:pic` bytes for the caller to place and compare.
pub(crate) fn check(
    state: &mut PackageState,
    xml: &XmlDoc,
    paragraph: usize,
    image: &[u8],
    width: u32,
    height: u32,
    limits: &Limits,
) -> Result<Vec<u8>> {
    let (media_type, (px_w, px_h)) = crate::image::validated_media_type(image, limits)?;
    if width == 0 || height == 0 || width > MAX_DISPLAY_HWPUNIT || height > MAX_DISPLAY_HWPUNIT {
        return Err(fail("picture display size must be 1..=1000000 HWPUNIT"));
    }
    match xml.elements().get(paragraph) {
        Some(el) if hp(el, "p") => {}
        Some(_) => return Err(unsupported("image insertion requires a caption paragraph")),
        None => return Err(fail("caption paragraph is outside the section")),
    }
    let source_w = px_w
        .checked_mul(HWPUNIT_PER_PIXEL)
        .ok_or_else(|| fail("image width exceeds the HWPUNIT range"))?;
    let source_h = px_h
        .checked_mul(HWPUNIT_PER_PIXEL)
        .ok_or_else(|| fail("image height exceeds the HWPUNIT range"))?;

    let (info, xmlmap) = hae_package::analyze(state, true, limits)?;
    let hpf = xmlmap
        .get(&info.rootfile)
        .cloned()
        .ok_or_else(|| fail("package manifest is missing"))?;
    let manifests: Vec<usize> = hpf
        .iter()
        .filter(|(_, e)| e.local == "manifest" && opf(e.ns.as_deref()))
        .map(|(i, _)| i)
        .collect();
    if manifests.len() != 1 {
        return Err(fail("manifest shape changed"));
    }
    let manifest = hpf.el(manifests[0]);
    let close = manifest
        .end
        .clone()
        .ok_or_else(|| fail("empty manifest cannot receive media"))?;
    let manifest_ns = manifest.ns.as_deref().unwrap_or_default().to_string();

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
    let ext = if media_type == "image/png" {
        "png"
    } else {
        "jpg"
    };
    let (id, name) = (1u64..)
        .map(|i| {
            let id = format!("hae-image-{i}");
            let name = format!("BinData/{id}.{ext}");
            (id, name)
        })
        .find(|(id, name)| {
            !used_ids.contains(&id.to_ascii_lowercase())
                && !used_names.contains(&name.to_ascii_lowercase())
        })
        .ok_or_else(|| fail("image resource identifiers exhausted"))?;

    let mut docs: Vec<&XmlDoc> = xmlmap.values().map(|d| &**d).collect();
    docs.push(xml);
    let pic_id = next_shape_id(&docs)?;

    let picture = picture_xml(
        pic_id,
        &id,
        &paragraph_ns(xml),
        &core_ns(xml),
        width,
        height,
        source_w,
        source_h,
    );
    let item = item_xml(&manifest_ns, &id, &name, media_type);

    let old_total = state.total_bytes();
    let growth = (picture.len() as u64)
        .saturating_add(item.len() as u64)
        .saturating_add(image.len() as u64);
    if state.entries.len() as u64 >= limits.max_entries
        || old_total.saturating_add(growth) > limits.max_inflate_total
        || old_total
            .saturating_add(growth)
            .saturating_add(image.len() as u64)
            > limits.max_session_memory
    {
        return Err(fail("image insertion package budget exceeded"));
    }

    change(
        state,
        &info.rootfile,
        &hpf,
        vec![(close.start..close.start, item.into_bytes())],
    )?;
    state.add(name, Arc::from(image.to_vec()), 0);
    Ok(picture)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hae_package::{Entry, Origin, MIMETYPE};

    // Self-generated 1x1 grayscale baseline JPEG (same fixture as the editor's
    // decoder test); no third-party asset.
    const JPEG: &[u8] = &[
        255, 216, 255, 224, 0, 16, 74, 70, 73, 70, 0, 1, 1, 0, 0, 1, 0, 1, 0, 0, 255, 219, 0, 67,
        0, 8, 6, 6, 7, 6, 5, 8, 7, 7, 7, 9, 9, 8, 10, 12, 20, 13, 12, 11, 11, 12, 25, 18, 19, 15,
        20, 29, 26, 31, 30, 29, 26, 28, 28, 32, 36, 46, 39, 32, 34, 44, 35, 28, 28, 40, 55, 41, 44,
        48, 49, 52, 52, 52, 31, 39, 57, 61, 56, 50, 60, 46, 51, 52, 50, 255, 192, 0, 11, 8, 0, 1,
        0, 1, 1, 1, 17, 0, 255, 196, 0, 20, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        255, 196, 0, 20, 16, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 218, 0, 8, 1,
        1, 0, 0, 63, 0, 63, 255, 217,
    ];

    const SECTION: &str = concat!(
        r#"<?xml version="1.0" encoding="UTF-8"?>"#,
        r#"<hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" "#,
        r#"xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" "#,
        r#"xmlns:hc="http://www.hancom.co.kr/hwpml/2011/core">"#,
        r#"<hp:p id="1" paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>그림 1. 설명</hp:t></hp:run></hp:p>"#,
        r#"</hs:sec>"#
    );

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

    fn doc(section: &str) -> XmlDoc {
        XmlDoc::parse(section.as_bytes().to_vec()).unwrap()
    }

    fn paragraph_of(d: &XmlDoc) -> usize {
        d.iter().find(|(_, e)| hp(e, "p")).unwrap().0
    }

    #[test]
    fn independently_rejects_invalid_image_structure() {
        let limits = Limits::default();
        let mut st = state(SECTION, "", &[]);
        let section = doc(SECTION);
        for bad in [
            b"GIF89a".to_vec(),
            b"RIFFxxxxWEBP".to_vec(),
            Vec::new(),
            JPEG[..JPEG.len() - 2].to_vec(),
        ] {
            assert!(check(
                &mut st,
                &section,
                paragraph_of(&section),
                &bad,
                150,
                75,
                &limits
            )
            .is_err());
        }
        // Existing media preserved and no entry added by a rejected request.
        assert_eq!(st.entries.len(), 4);
    }

    #[test]
    fn derives_expected_picture_and_records_frozen_media() {
        let limits = Limits::default();
        let old = JPEG.to_vec();
        let mut st = state(
            SECTION,
            r#"<opf:item id="old" href="BinData/old.jpg" media-type="image/jpeg" isEmbeded="1"/>"#,
            &[("BinData/old.jpg", old.clone())],
        );
        let section = doc(SECTION);
        let before_section = st.get("Contents/section0.xml").unwrap().clone();
        let picture = check(
            &mut st,
            &section,
            paragraph_of(&section),
            &old,
            300,
            150,
            &limits,
        )
        .unwrap();
        let text = String::from_utf8(picture).unwrap();
        assert!(text.contains(r#"binaryItemIDRef="hae-image-1""#));
        assert!(text.contains(r#"<hp:imgDim dimwidth="75" dimheight="75"/>"#));
        assert!(text.contains(r#"<hp:sz width="300""#));
        // One frozen media entry appended; the source media is untouched.
        assert_eq!(st.entries.last().unwrap().name, "BinData/hae-image-1.jpg");
        assert_eq!(&st.entries.last().unwrap().payload[..], &old[..]);
        assert_eq!(&st.get("BinData/old.jpg").unwrap()[..], &old[..]);
        assert_eq!(
            &st.get("Contents/section0.xml").unwrap()[..],
            &before_section[..]
        );
        let hpf = String::from_utf8(st.get("Contents/content.hpf").unwrap().to_vec()).unwrap();
        assert!(hpf.contains(r#"media-type="image/jpeg" isEmbeded="1"/>"#));
    }

    #[test]
    fn counts_video_in_the_shared_object_id_space() {
        // Review r1: `hp:video` shares the drawing-object id space with
        // `hp:pic`; the verifier must derive the same next id as the editor
        // when the source already uses the base id. Parsed only, not modelled.
        let limits = Limits::default();
        let section = concat!(
            r#"<?xml version="1.0" encoding="UTF-8"?>"#,
            r#"<hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" "#,
            r#"xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" "#,
            r#"xmlns:hc="http://www.hancom.co.kr/hwpml/2011/core">"#,
            r#"<hp:p id="1" paraPrIDRef="0"><hp:run charPrIDRef="0">"#,
            r#"<hp:video id="1300000000"/>"#,
            r#"</hp:run></hp:p>"#,
            r#"<hp:p id="2" paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>그림 1. 설명</hp:t></hp:run></hp:p>"#,
            r#"</hs:sec>"#
        );
        let mut st = state(section, "", &[]);
        let d = doc(section);
        let caption = d.iter().filter(|(_, e)| hp(e, "p")).nth(1).unwrap().0;
        let picture = check(&mut st, &d, caption, JPEG, 150, 75, &limits).unwrap();
        let text = String::from_utf8(picture).unwrap();
        assert!(text.contains(r#"id="1300000001" zOrder="0""#));
    }

    #[test]
    fn rejects_bad_dimensions_and_uses_canonical_namespaces() {
        let limits = Limits::default();
        let mut st = state(SECTION, "", &[]);
        let section = doc(SECTION);
        let before = st.payload_set_sha256();
        for (w, h) in [(0u32, 10u32), (10, 0), (1_000_001, 10)] {
            assert!(check(
                &mut st,
                &section,
                paragraph_of(&section),
                JPEG,
                w,
                h,
                &limits
            )
            .is_err());
        }
        assert_eq!(st.payload_set_sha256(), before);

        let alt = concat!(
            r#"<?xml version="1.0" encoding="UTF-8"?>"#,
            r#"<hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" "#,
            r#"xmlns:para="http://www.hancom.co.kr/hwpml/2011/paragraph" "#,
            r#"xmlns:core="http://www.hancom.co.kr/hwpml/2011/core">"#,
            r#"<para:p id="1" paraPrIDRef="0"><para:run charPrIDRef="0"><para:t>x</para:t></para:run></para:p>"#,
            r#"</hs:sec>"#
        );
        let mut st = state(alt, "", &[]);
        let section = doc(alt);
        let picture = check(
            &mut st,
            &section,
            paragraph_of(&section),
            JPEG,
            150,
            75,
            &limits,
        )
        .unwrap();
        let text = String::from_utf8(picture).unwrap();
        assert!(text.contains(r#"xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph""#));
        assert!(text.contains(r#"xmlns:hc="http://www.hancom.co.kr/hwpml/2011/core""#));
    }
}
