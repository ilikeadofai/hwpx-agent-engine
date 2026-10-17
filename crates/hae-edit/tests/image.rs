use hae_diag::{Code, Limits};
use hae_edit::{apply, EditOp, ImageBytes, Intent};
use hae_model::{Document, Kind};
use hae_package::{NS_HC, NS_HP, NS_HS};
use hae_zip::{Archive, OutEntry};
use std::sync::Arc;

fn png() -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 2, 1);
        encoder.set_color(png::ColorType::Rgb);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[255, 0, 0, 0, 255, 0])
            .unwrap();
    }
    bytes
}

// Self-generated 1x1 grayscale baseline JPEG, no third-party asset.
const JPEG: &[u8] = &[
    255, 216, 255, 224, 0, 16, 74, 70, 73, 70, 0, 1, 1, 0, 0, 1, 0, 1, 0, 0, 255, 219, 0, 67, 0, 8,
    6, 6, 7, 6, 5, 8, 7, 7, 7, 9, 9, 8, 10, 12, 20, 13, 12, 11, 11, 12, 25, 18, 19, 15, 20, 29, 26,
    31, 30, 29, 26, 28, 28, 32, 36, 46, 39, 32, 34, 44, 35, 28, 28, 40, 55, 41, 44, 48, 49, 52, 52,
    52, 31, 39, 57, 61, 56, 50, 60, 46, 51, 52, 50, 255, 192, 0, 11, 8, 0, 1, 0, 1, 1, 1, 17, 0,
    255, 196, 0, 20, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 196, 0, 20, 16, 1,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 218, 0, 8, 1, 1, 0, 0, 63, 0, 63, 255,
    217,
];

fn picture(id: &str, extra: &str, img_attrs: &str) -> String {
    format!(
        r#"<hp:pic id="{id}"><hp:sz width="1440" height="720"/><hp:pos vertOffset="21" horzOffset="37"/><hp:imgDim dimwidth="1440" dimheight="720"/>{extra}<hc:img binaryItemIDRef="old" {img_attrs}/><hp:caption><hp:subList><hp:p><hp:run><hp:t>caption</hp:t></hp:run></hp:p></hp:subList></hp:caption></hp:pic>"#
    )
}

fn document(extra: &str, img_attrs: &str) -> Document {
    let base = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/adversarial/valid_base.hwpx"
    ))
    .unwrap();
    let limits = Limits::default();
    let archive = Archive::open(base, &limits).unwrap();
    let section = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<hs:sec xmlns:hs="{NS_HS}" xmlns:hp="{NS_HP}" xmlns:hc="{NS_HC}"><hp:p><hp:run>{}{}<hp:t>body</hp:t></hp:run><hp:linesegarray><hp:lineseg textpos="0"/></hp:linesegarray></hp:p><hp:p><hp:run><hp:t>after</hp:t></hp:run><hp:linesegarray/></hp:p></hs:sec>"#,
        picture("1", extra, img_attrs),
        picture("2", "", "")
    );
    let hpf = r#"<opf:package xmlns:opf="http://www.idpf.org/2007/opf/"><opf:manifest><!--keep--><opf:item id="s" href="Contents/section0.xml" media-type="application/xml"/><opf:item id="old" href="BinData/old.png" media-type="image/png"/></opf:manifest><opf:spine><opf:itemref idref="s"/></opf:spine></opf:package>"#;
    let mut plan = hae_zip::keep_all(&archive);
    for item in &mut plan {
        match item {
            OutEntry::Keep(n) if n == "Contents/section0.xml" => {
                *item = OutEntry::Replace(n.clone(), Arc::from(section.as_bytes()))
            }
            OutEntry::Keep(n) if n == "Contents/content.hpf" => {
                *item = OutEntry::Replace(n.clone(), Arc::from(hpf.as_bytes()))
            }
            _ => {}
        }
    }
    plan.push(OutEntry::Add {
        name: "BinData/old.png".into(),
        payload: Arc::from(png()),
        method: 0,
    });
    Document::open(hae_zip::write(&archive, &plan).unwrap(), &limits).unwrap()
}

fn target(d: &Document) -> String {
    d.nodes
        .iter()
        .find(|n| n.kind == Kind::Picture)
        .unwrap()
        .id
        .clone()
}

#[test]
fn validates_and_freezes_original_png_and_jpeg() {
    for (bytes, format, dimensions, media) in [
        (png(), "png", (2, 1), "image/png"),
        (JPEG.to_vec(), "jpeg", (1, 1), "image/jpeg"),
    ] {
        let want = bytes.clone();
        let image = ImageBytes::from_bytes(bytes, &Limits::default()).unwrap();
        assert_eq!(&image.bytes()[..], want);
        assert_eq!(image.sha256(), hae_package::sha256_hex(&want));
        assert_eq!(image.format(), format);
        assert_eq!(image.dimensions(), dimensions);
        assert_eq!(image.media_type(), media);
        assert!(Arc::ptr_eq(image.bytes(), image.clone().bytes()));
    }
}

#[test]
fn rejects_unsupported_truncated_corrupt_and_multiple_images() {
    let p = png();
    let mut corrupt = p.clone();
    corrupt[29] ^= 1; // IHDR CRC
    let mut two_pngs = p.clone();
    two_pngs.extend_from_slice(&p);
    let mut two_jpegs = JPEG.to_vec();
    two_jpegs.extend_from_slice(JPEG);
    for bytes in [
        b"GIF89a".to_vec(),
        b"RIFFxxxxWEBP".to_vec(),
        Vec::new(),
        p[..p.len() - 1].to_vec(),
        JPEG[..JPEG.len() - 2].to_vec(),
        corrupt,
        two_pngs,
        two_jpegs,
    ] {
        assert!(ImageBytes::from_bytes(bytes, &Limits::default()).is_err());
    }
}

#[test]
fn rejects_missing_jpeg_entropy_even_with_terminal_eoi() {
    let mut bytes = JPEG.to_vec();
    bytes.remove(bytes.len() - 3);
    assert!(ImageBytes::from_bytes(bytes, &Limits::default()).is_err());
}

#[test]
fn rejects_mpo_even_when_primary_picture_decodes() {
    let mut bytes = JPEG[..2].to_vec();
    bytes.extend_from_slice(&[255, 226, 0, 6, b'M', b'P', b'F', 0]);
    bytes.extend_from_slice(&JPEG[2..]);
    assert_eq!(
        ImageBytes::from_bytes(bytes, &Limits::default())
            .unwrap_err()
            .code(),
        Code::Unsupported
    );
}

// Small independent CRC helper for adversarial PNG chunk fixtures.
fn crc32(bytes: &[u8]) -> u32 {
    let mut c = !0u32;
    for b in bytes {
        c ^= u32::from(*b);
        for _ in 0..8 {
            c = (c >> 1) ^ (0xedb88320 & 0u32.wrapping_sub(c & 1));
        }
    }
    !c
}

#[test]
fn validates_png_adler32_even_when_chunk_crc_is_valid() {
    let mut bytes = png();
    let chunk = bytes.windows(4).position(|w| w == b"IDAT").unwrap();
    let len = u32::from_be_bytes(bytes[chunk - 4..chunk].try_into().unwrap()) as usize;
    bytes[chunk + 4 + len - 1] ^= 1;
    let crc = crc32(&bytes[chunk..chunk + 4 + len]);
    bytes[chunk + 4 + len..chunk + 8 + len].copy_from_slice(&crc.to_be_bytes());
    assert!(ImageBytes::from_bytes(bytes, &Limits::default()).is_err());
}

#[test]
fn validates_png_ancillary_crc_instead_of_silently_ignoring_corruption() {
    let source = png();
    let mut bytes = source[..33].to_vec();
    bytes.extend_from_slice(&[0, 0, 0, 1, b's', b'R', b'G', b'B', 0, 0, 0, 0, 0]);
    bytes.extend_from_slice(&source[33..]);
    assert!(ImageBytes::from_bytes(bytes, &Limits::default()).is_err());
}

#[test]
fn identical_image_is_noop_and_editability_still_checked_first() {
    let mut d = document("", "");
    let id = target(&d);
    let op = EditOp::ImageReplace {
        target: id.clone(),
        image: ImageBytes::from_bytes(png(), &d.limits).unwrap(),
    };
    let a = apply(&d, std::slice::from_ref(&op)).unwrap();
    assert_eq!(
        a.doc.state.payload_set_sha256(),
        d.state.payload_set_sha256()
    );
    assert!(a.changed_ids.is_empty());
    assert!(a.intents.is_empty());
    assert!(a.resource_changes.is_empty());
    d.nodes.iter_mut().find(|n| n.id == id).unwrap().locked = Some("protected".into());
    assert_eq!(apply(&d, &[op]).unwrap_err().code(), Code::Unsupported);
}

#[test]
fn rejects_apng_even_when_it_has_only_one_frame() {
    let mut bytes = Vec::new();
    {
        let mut e = png::Encoder::new(&mut bytes, 1, 1);
        e.set_animated(1, 0).unwrap();
        e.write_header().unwrap().write_image_data(&[0]).unwrap();
    }
    assert_eq!(
        ImageBytes::from_bytes(bytes, &Limits::default())
            .unwrap_err()
            .code(),
        Code::Unsupported
    );
}

#[test]
fn rejects_huge_headers_before_trying_to_decode() {
    let mut p = png();
    p[16..20].copy_from_slice(&100_000u32.to_be_bytes());
    p[20..24].copy_from_slice(&100_000u32.to_be_bytes());
    // Deliberately no valid data/CRC for the huge dimensions: size wins first.
    assert_eq!(
        ImageBytes::from_bytes(p, &Limits::default())
            .unwrap_err()
            .code(),
        Code::ResourceLimit
    );
    let mut j = JPEG.to_vec();
    let sof = j.windows(2).position(|w| w == [255, 192]).unwrap();
    j[sof + 5..sof + 9].copy_from_slice(&[255, 255, 255, 255]);
    assert_eq!(
        ImageBytes::from_bytes(j, &Limits::default())
            .unwrap_err()
            .code(),
        Code::ResourceLimit
    );
    let limits = Limits {
        max_image_pixels: 1,
        ..Limits::default()
    };
    assert_eq!(
        ImageBytes::from_bytes(png(), &limits).unwrap_err().code(),
        Code::ResourceLimit
    );
    let limits = Limits {
        max_session_memory: 1,
        ..Limits::default()
    };
    assert_eq!(
        ImageBytes::from_bytes(png(), &limits).unwrap_err().code(),
        Code::ResourceLimit
    );
}

#[test]
fn replaces_only_target_reference_and_appends_media_preserving_shared_old_bytes() {
    let d = document(
        r#"<hp:imgClip left="0" top="0" right="1440" bottom="720"/><hp:effects/>"#,
        r#"bright="0" contrast="0" effect="REAL_PIC" alpha="0""#,
    );
    let id = target(&d);
    let image = ImageBytes::from_bytes(JPEG.to_vec(), &d.limits).unwrap();
    let before = d.state.payload_set_sha256();
    let a = apply(
        &d,
        &[EditOp::ImageReplace {
            target: id.clone(),
            image: image.clone(),
        }],
    )
    .unwrap();
    assert_eq!(d.state.payload_set_sha256(), before);
    assert_eq!(a.changed_ids, vec![id.clone()]);
    assert!(a.cache_removals.is_empty());
    assert!(a.stale.layout_sections.is_empty());
    assert_eq!(a.doc.map_rows(), d.map_rows());
    let added = a.doc.state.entries.last().unwrap();
    assert_eq!(&added.payload[..], JPEG);
    assert_eq!(a.doc.state.entries.len(), d.state.entries.len() + 1);
    let item = a
        .doc
        .info
        .manifest
        .iter()
        .find(|m| m.entry == added.name)
        .unwrap();
    assert_eq!(item.media_type, "image/jpeg");
    let old_section = std::str::from_utf8(d.payload("Contents/section0.xml").unwrap()).unwrap();
    let expected = old_section.replacen(
        "binaryItemIDRef=\"old\"",
        &format!("binaryItemIDRef=\"{}\"", item.id),
        1,
    );
    assert_eq!(
        &a.doc.payload("Contents/section0.xml").unwrap()[..],
        expected.as_bytes()
    );
    for e in &d.state.entries {
        if e.name != "Contents/content.hpf" && e.name != "Contents/section0.xml" {
            assert!(
                Arc::ptr_eq(&e.payload, a.doc.payload(&e.name).unwrap()),
                "{}",
                e.name
            );
        }
    }
    assert_eq!(
        a.intents,
        vec![Intent::ImageReplace {
            entry: "Contents/section0.xml".into(),
            path: d.doc_of(d.node(&id).unwrap()).path(d.node(&id).unwrap().el),
            image_sha256: image.sha256().into()
        }]
    );
    assert_eq!(a.resource_changes.len(), 2);
    let reopened = Document::open(a.doc.to_hwpx().unwrap(), &d.limits).unwrap();
    assert_eq!(&reopened.payload(&added.name).unwrap()[..], JPEG);
    assert_eq!(
        &reopened.payload("BinData/old.png").unwrap()[..],
        &d.payload("BinData/old.png").unwrap()[..]
    );
}

#[test]
fn empty_native_effects_is_editable_and_replacement_preserves_geometry() {
    let d = document(
        r#"<hp:imgClip left="0" top="0" right="1440" bottom="720"/><hp:effects/>"#,
        r#"effect="REAL_PIC" bright="0" contrast="0" alpha="0""#,
    );
    let id = target(&d);
    let node = d.node(&id).unwrap();
    assert_eq!(d.edit_block_reason(node), None);
    let image = ImageBytes::from_bytes(JPEG.to_vec(), &d.limits).unwrap();
    let outcome = apply(&d, &[EditOp::ImageReplace { target: id, image }]).unwrap();
    let out = String::from_utf8(
        outcome
            .doc
            .payload("Contents/section0.xml")
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(
        out.contains(r#"<hp:imgClip left="0" top="0" right="1440" bottom="720"/><hp:effects/>"#)
    );
    assert!(out.contains(r#"<hp:sz width="1440" height="720"/>"#));
}

#[test]
fn rejects_crop_effect_and_missing_target_without_changing_original() {
    for (extra, attrs) in [
        (
            r#"<hp:imgClip left="1" top="0" right="1440" bottom="720"/>"#,
            "",
        ),
        (
            r#"<hp:imgClip left="0" top="0" right="1000" bottom="720"/>"#,
            "",
        ),
        ("<hp:effects><hp:glow/></hp:effects>", ""),
        ("<hp:effects unknown=\"1\"/>", ""),
        ("<hp:effects>uninterpreted</hp:effects>", ""),
        ("", r#"effect="GRAY_SCALE""#),
        ("", r#"contrast="5""#),
    ] {
        let d = document(extra, attrs);
        let hash = d.state.payload_set_sha256();
        let image = ImageBytes::from_bytes(png(), &d.limits).unwrap();
        let error = apply(
            &d,
            &[EditOp::ImageReplace {
                target: target(&d),
                image,
            }],
        )
        .unwrap_err();
        assert_eq!(error.code(), Code::Unsupported);
        assert_eq!(d.state.payload_set_sha256(), hash);
    }
    let d = document("", "");
    let image = ImageBytes::from_bytes(png(), &d.limits).unwrap();
    assert_eq!(
        apply(
            &d,
            &[EditOp::ImageReplace {
                target: "missing".into(),
                image
            }]
        )
        .unwrap_err()
        .code(),
        Code::TargetNotFound
    );
}

#[test]
fn repeated_replacements_use_distinct_items_and_recheck_candidate_limits() {
    let mut d = document("", "");
    let id = target(&d);
    let jpeg = ImageBytes::from_bytes(JPEG.to_vec(), &d.limits).unwrap();
    let image = ImageBytes::from_bytes(png(), &d.limits).unwrap();
    let a = apply(
        &d,
        &[
            EditOp::ImageReplace {
                target: id.clone(),
                image: jpeg.clone(),
            },
            EditOp::ImageReplace {
                target: id.clone(),
                image: image.clone(),
            },
        ],
    )
    .unwrap();
    let entries = &a.doc.state.entries;
    assert_ne!(
        entries[entries.len() - 1].name,
        entries[entries.len() - 2].name
    );
    d.limits.max_entries = d.state.entries.len() as u64;
    assert_eq!(
        apply(
            &d,
            &[EditOp::ImageReplace {
                target: id,
                image: jpeg
            }]
        )
        .unwrap_err()
        .code(),
        Code::ResourceLimit
    );
}
