//! R1/R2 regressions: checked SVG is static and raster budgets precede decoding.
use hae_render_rhwp::fonts::FontSet;
use hae_render_rhwp::png::{
    rasterize_svg, rasterize_svg_with_budget, CheckedSvg, PngError, PngOptions,
};
use hae_render_rhwp::sandbox::TempDir;
use hae_render_rhwp::svg_safety::check_svg;
use resvg::tiny_skia::{Color, Pixmap};

fn fonts() -> (TempDir, FontSet) {
    let dir = TempDir::new_in(&std::env::temp_dir(), "hae-svg-limits-").unwrap();
    let fonts = FontSet::scan(dir.path()).unwrap();
    (dir, fonts)
}

fn reject(body: &str) {
    let svg = format!(r#"<svg xmlns="http://www.w3.org/2000/svg">{body}</svg>"#);
    assert!(CheckedSvg::new(svg.as_bytes()).is_err(), "accepted {body}");
}

#[test]
fn css_backslash_escapes_are_rejected_in_style_and_attributes() {
    reject(
        r#"<style>@im\70ort 'https://example.invalid/x.css';rect{fill:u\72l(https://example.invalid/x.svg#g)}</style>"#,
    );
    reject(r#"<rect style="fill:u\72l(https://example.invalid/x.svg#g)"/>"#);
    reject(r#"<rect fill="u&#92;72l(https://example.invalid/x.svg#g)"/>"#);
}

#[test]
fn xml_references_in_style_are_checked_after_normalization() {
    reject(
        r#"<style>@im&#112;ort 'https://example.invalid/x.css';rect{fill:u&#114;l(https://example.invalid/x.svg#g)}</style>"#,
    );
    reject(r#"<style>rect{fill:u&#114;l(https://example.invalid/x.svg#g)}</style>"#);
    reject(
        r#"<style><![CDATA[rect{fill:u]]>&#114;<![CDATA[l(https://example.invalid/x.svg#g)}]]></style>"#,
    );
}

#[test]
fn smil_cannot_mutate_url_or_other_attributes() {
    reject(
        r##"<a id="a"><text>click</text></a><set href="#a" attributeName="href" to="javascript:alert(1)"/>"##,
    );
    for element in [
        "animate",
        "animateMotion",
        "animateTransform",
        "set",
        "discard",
    ] {
        reject(&format!(
            r#"<{element} attributeName="href" values="javascript:alert(1)"/>"#
        ));
    }
    reject(r#"<rect attributeName="href" to="javascript:alert(1)"/>"#);
}

#[test]
fn data_urls_are_only_raster_image_resources() {
    reject(r#"<a href="data:text/html;base64,PHNjcmlwdD4=">click</a>"#);
    reject(r#"<a href="data:image/png;base64,AAA">click</a>"#);
    reject(r#"<use href="data:image/svg+xml;base64,AAA"/>"#);
    reject(r#"<image href="data:text/html;base64,AAA"/>"#);
    reject(r#"<image href="data:image/svg+xml;base64,AAA"/>"#);
    reject(r#"<style>rect{fill:url(data:text/html;base64,AAA)}</style>"#);
}

#[test]
fn unknown_vocabulary_namespaces_and_css_constructs_fail_closed() {
    reject(r#"<unknown/>"#);
    reject(r#"<rect unknown="value"/>"#);
    reject(r##"<svg xmlns="http://www.w3.org/1999/xhtml"><a href="#x"/></svg>"##);
    reject(r#"<style>@namespace x url(https://example.invalid);</style>"#);
    reject(r#"<style>rect{fill:u/**/rl(https://example.invalid)}</style>"#);
    reject(r#"<rect style="behavior:expression(alert(1))"/>"#);
    assert!(!check_svg(b"").is_safe());
    assert!(!check_svg(b"<svg/><svg/>").is_safe());
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for s in bytes.chunks(3) {
        let n = (u32::from(s[0]) << 16)
            | (u32::from(*s.get(1).unwrap_or(&0)) << 8)
            | u32::from(*s.get(2).unwrap_or(&0));
        for shift in [18, 12, 6, 0] {
            out.push(
                if (shift == 6 && s.len() < 2) || (shift == 0 && s.len() < 3) {
                    '='
                } else {
                    TABLE[((n >> shift) & 63) as usize] as char
                },
            );
        }
    }
    out
}

fn image_svg(mime: &str, data: &[u8], count: usize) -> String {
    let image = format!(
        r#"<image width="1" height="1" href="data:{mime};base64,{}"/>"#,
        base64(data)
    );
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1">{}</svg>"#,
        image.repeat(count)
    )
}

fn small_png(w: u32, h: u32) -> Vec<u8> {
    let mut pm = Pixmap::new(w, h).unwrap();
    pm.fill(Color::from_rgba8(255, 0, 0, 255));
    pm.encode_png().unwrap()
}

fn png_chunk(out: &mut Vec<u8>, tag: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(tag);
    out.extend_from_slice(data);
    let mut crc = !0u32;
    for byte in tag.iter().chain(data) {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    out.extend_from_slice(&(!crc).to_be_bytes());
}

/// Valid 7000x7000 1-bit grayscale PNG (5977 compressed / 6,132,000 raw bytes).
/// The zlib stream was generated independently with Python zlib.compress(...,9)
/// and run encoded here so constructing the fixture never allocates its pixels.
fn compressed_bomb() -> Vec<u8> {
    let runs = [
        (1, 120),
        (1, 218),
        (1, 236),
        (1, 193),
        (2, 1),
        (3, 0),
        (1, 128),
        (1, 144),
        (1, 254),
        (1, 175),
        (1, 238),
        (1, 8),
        (1, 10),
        (4094, 0),
        (1, 128),
        (1, 219),
        (1, 131),
        (1, 67),
        (1, 2),
        (4, 0),
        (1, 65),
        (1, 255),
        (1, 95),
        (1, 251),
        (1, 194),
        (1, 4),
        (1846, 0),
        (1, 192),
        (1, 45),
        (1, 150),
        (1, 147),
        (1, 0),
        (1, 1),
    ];
    let mut zlib = Vec::new();
    for (n, byte) in runs {
        zlib.extend(std::iter::repeat_n(byte, n));
    }
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&7000u32.to_be_bytes());
    ihdr.extend_from_slice(&7000u32.to_be_bytes());
    ihdr.extend_from_slice(&[1, 0, 0, 0, 0]);
    png_chunk(&mut png, b"IHDR", &ihdr);
    png_chunk(&mut png, b"IDAT", &zlib);
    png_chunk(&mut png, b"IEND", &[]);
    png
}

#[test]
fn compressed_7000px_png_is_rejected_on_a_one_pixel_canvas() {
    let (_dir, fonts) = fonts();
    let svg = image_svg("image/png", &compressed_bomb(), 1);
    let result = rasterize_svg(
        CheckedSvg::new(svg.as_bytes()).unwrap(),
        &fonts,
        &PngOptions {
            scale: 1.0,
            max_pixels: 1,
        },
    );
    assert!(
        matches!(result, Err(PngError::TooLarge { .. })),
        "{result:?}"
    );
}

#[test]
fn supported_raster_headers_cannot_bypass_the_pixel_limit() {
    let (_dir, fonts) = fonts();
    // Huge JPEG SOF, GIF first-frame rectangle, and WebP lossless bitstream headers.
    let jpeg = [
        0xff, 0xd8, 0xff, 0xc0, 0, 11, 8, 0x1b, 0x58, 0x1b, 0x58, 1, 1, 0x11, 0, 0xff, 0xd9,
    ];
    let gif = b"GIF89a\x01\0\x01\0\0\0\0,\0\0\0\0\x58\x1b\x58\x1b\0\x02\x01\0\0;";
    let dims = 6999u32 | (6999u32 << 14);
    let mut webp = b"RIFF\x12\0\0\0WEBPVP8L\x05\0\0\0\x2f".to_vec();
    webp.extend_from_slice(&dims.to_le_bytes());
    webp.push(0);
    for (mime, data) in [
        ("image/jpeg", jpeg.as_slice()),
        ("image/gif", gif.as_slice()),
        ("image/webp", webp.as_slice()),
    ] {
        let svg = image_svg(mime, data, 1);
        let result = rasterize_svg(
            CheckedSvg::new(svg.as_bytes()).unwrap(),
            &fonts,
            &PngOptions {
                scale: 1.0,
                max_pixels: 1,
            },
        );
        assert!(
            matches!(result, Err(PngError::TooLarge { .. })),
            "{mime}: {result:?}"
        );
    }
}

#[test]
fn malformed_or_mismatched_embedded_formats_are_explicit_errors() {
    let (_dir, fonts) = fonts();
    for (mime, data) in [
        ("image/png", b"not a PNG".as_slice()),
        ("image/jpeg", small_png(1, 1).as_slice()),
    ] {
        let svg = image_svg(mime, data, 1);
        assert!(
            rasterize_svg(
                CheckedSvg::new(svg.as_bytes()).unwrap(),
                &fonts,
                &PngOptions::default()
            )
            .is_err(),
            "silently omitted {mime}"
        );
    }
}

#[test]
fn nonfinite_or_nonpositive_scale_uses_one_consistently() {
    let (_dir, fonts) = fonts();
    let svg = CheckedSvg::new(br#"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="3"><rect width="2" height="3" fill="red"/></svg>"#).unwrap();
    for scale in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -1.0, 0.0] {
        let png = rasterize_svg(
            svg,
            &fonts,
            &PngOptions {
                scale,
                max_pixels: 6,
            },
        )
        .unwrap();
        let pm = Pixmap::decode_png(&png).unwrap();
        assert_eq!((pm.width(), pm.height()), (2, 3), "scale {scale}");
        assert!(pm.pixels().iter().all(|p| p.red() == 255 && p.green() == 0));
    }
}

#[test]
fn decoded_byte_budget_includes_canvas_and_all_embedded_images() {
    let (_dir, fonts) = fonts();
    let plain =
        CheckedSvg::new(br#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"/>"#)
            .unwrap();
    assert!(rasterize_svg_with_budget(plain, &fonts, &PngOptions::default(), 399).is_err());
    let single = image_svg("image/png", &small_png(10, 10), 1);
    rasterize_svg_with_budget(
        CheckedSvg::new(single.as_bytes()).unwrap(),
        &fonts,
        &PngOptions::default(),
        200 * 1024,
    )
    .unwrap();
    let svg = image_svg("image/png", &small_png(10, 10), 2);
    // Each image fits on its own; both together exceed this reservation.
    assert!(rasterize_svg_with_budget(
        CheckedSvg::new(svg.as_bytes()).unwrap(),
        &fonts,
        &PngOptions::default(),
        200 * 1024
    )
    .is_err());
    let svg = image_svg("image/png", &small_png(1, 1), 1);
    assert!(rasterize_svg_with_budget(
        CheckedSvg::new(svg.as_bytes()).unwrap(),
        &fonts,
        &PngOptions::default(),
        0
    )
    .is_err());
}

#[test]
fn embedded_pixels_are_cumulative_with_the_canvas() {
    let (_dir, fonts) = fonts();
    let svg = image_svg("image/png", &small_png(2, 2), 2);
    let result = rasterize_svg_with_budget(
        CheckedSvg::new(svg.as_bytes()).unwrap(),
        &fonts,
        &PngOptions {
            scale: 1.0,
            max_pixels: 8,
        },
        1024 * 1024,
    );
    assert!(
        matches!(
            result,
            Err(PngError::PixelLimit {
                required_pixels: 9,
                max_pixels: 8
            })
        ),
        "two 4px images plus 1px canvas must exceed 8px"
    );
}

#[test]
fn a_small_embedded_png_still_renders_its_pixels() {
    let (_dir, fonts) = fonts();
    let svg = image_svg("image/png", &small_png(1, 1), 1);
    let png = rasterize_svg_with_budget(
        CheckedSvg::new(svg.as_bytes()).unwrap(),
        &fonts,
        &PngOptions::default(),
        1024 * 1024,
    )
    .unwrap();
    let pm = Pixmap::decode_png(&png).unwrap();
    assert_eq!((pm.pixels()[0].red(), pm.pixels()[0].green()), (255, 0));
}

#[test]
fn compressed_png_metadata_is_refused_without_inflation() {
    let (_dir, fonts) = fonts();
    // iCCP's zlib stream expands to 6 MiB even though this is a 1px image.
    let bomb = compressed_bomb();
    let idat_length = u32::from_be_bytes(bomb[33..37].try_into().unwrap()) as usize;
    let mut iccp = b"profile\0\0".to_vec();
    iccp.extend_from_slice(&bomb[41..41 + idat_length]);
    let png = small_png(1, 1);
    let mut with_metadata = png[..33].to_vec();
    png_chunk(&mut with_metadata, b"iCCP", &iccp);
    with_metadata.extend_from_slice(&png[33..]);
    let svg = image_svg("image/png", &with_metadata, 1);
    let result = rasterize_svg_with_budget(
        CheckedSvg::new(svg.as_bytes()).unwrap(),
        &fonts,
        &PngOptions::default(),
        1024 * 1024,
    );
    assert!(
        matches!(result, Err(PngError::UnsupportedImage(_))),
        "{result:?}"
    );
}

#[test]
fn static_jpeg_gif_and_lossy_webp_remain_supported() {
    let (_dir, fonts) = fonts();
    let jpeg = include_bytes!("../../../tests/fixtures/editing/inputs/ok.jpg");
    // Independently generated by ImageMagick, one red pixel.
    let gif = [
        0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 1, 0, 1, 0, 0xf0, 0, 0, 0xff, 0, 0, 0, 0, 0, 0x21,
        0xf9, 4, 0, 0, 0, 0, 0, 0x2c, 0, 0, 0, 0, 1, 0, 1, 0, 0, 2, 2, 0x44, 1, 0, 0x3b,
    ];
    let webp = [
        0x52, 0x49, 0x46, 0x46, 0x3c, 0, 0, 0, 0x57, 0x45, 0x42, 0x50, 0x56, 0x50, 0x38, 0x20,
        0x30, 0, 0, 0, 0xd0, 1, 0, 0x9d, 1, 0x2a, 1, 0, 1, 0, 2, 0, 0x34, 0x25, 0xa0, 2, 0x74,
        0xba, 1, 0xf8, 0, 3, 0xb0, 0, 0xfe, 0xf0, 0xc4, 0x0b, 0xff, 0x20, 0xb9, 0x61, 0x75, 0xc8,
        0xd7, 0xff, 0x20, 0x3f, 0xe4, 7, 0xfc, 0x80, 0xff, 0xf8, 0xf2, 0, 0, 0,
    ];
    for (mime, data) in [
        ("image/jpeg", jpeg.as_slice()),
        ("image/gif", gif.as_slice()),
        ("image/webp", webp.as_slice()),
    ] {
        let svg = image_svg(mime, data, 1);
        let png = rasterize_svg(
            CheckedSvg::new(svg.as_bytes()).unwrap(),
            &fonts,
            &PngOptions::default(),
        )
        .unwrap();
        let pm = Pixmap::decode_png(&png).unwrap();
        assert!(
            pm.pixels()[0].red() < 250
                || pm.pixels()[0].green() < 250
                || pm.pixels()[0].blue() < 250,
            "{mime} was silently omitted"
        );
    }
}

#[test]
fn webp_extended_canvas_cannot_hide_a_larger_bitstream() {
    let (_dir, fonts) = fonts();
    let mut data =
        b"RIFF\x24\0\0\0WEBPVP8X\x0a\0\0\0\0\0\0\0\0\0\0\0\0\0VP8L\x05\0\0\0\x2f".to_vec();
    data.extend_from_slice(&(6999u32 | (6999u32 << 14)).to_le_bytes());
    data.push(0);
    let svg = image_svg("image/webp", &data, 1);
    let result = rasterize_svg(
        CheckedSvg::new(svg.as_bytes()).unwrap(),
        &fonts,
        &PngOptions {
            scale: 1.0,
            max_pixels: 1,
        },
    );
    assert!(
        matches!(result, Err(PngError::TooLarge { .. })),
        "{result:?}"
    );
}

mod common;

#[test]
fn actual_rhwp_corpus_svg_remains_accepted() {
    let env = common::real_env();
    for entry in std::fs::read_dir(common::workspace_root().join("tests/fixtures/corpus")).unwrap()
    {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|ext| ext != "hwpx") {
            continue;
        }
        // The non-Hancom IRB ZIP is a documented unsupported rhwp input, not
        // an SVG fixture: rhwp's format detector rejects it before export.
        if path
            .file_name()
            .is_some_and(|name| name == "irb-sample.hwpx")
        {
            continue;
        }
        let bytes = std::fs::read(&path).unwrap();
        let result = env
            .adapter
            .preview(
                &hae_render_rhwp::RenderRequest {
                    document_id: "static-svg-corpus",
                    content_revision: 1,
                    hwpx: &bytes,
                    pages: None,
                    want_png: false,
                    png: Default::default(),
                },
                None,
            )
            .unwrap_or_else(|e| panic!("{}: {e:?}", path.display()));
        assert!(
            !result.pages.is_empty(),
            "{}: {:?}",
            path.display(),
            result.notes
        );
        assert!(
            !result.notes.iter().any(|n| n.code == "SVG_UNSAFE"),
            "{}: {:?}",
            path.display(),
            result.notes
        );
        eprintln!(
            "SVG_CORPUS {} {} checked pages",
            path.file_name().unwrap().to_string_lossy(),
            result.pages.len()
        );
    }
}

#[test]
fn codec_scratch_is_reserved_even_for_a_single_pixel_png() {
    let (_dir, fonts) = fonts();
    let svg = image_svg("image/png", &small_png(1, 1), 1);
    let result = rasterize_svg_with_budget(
        CheckedSvg::new(svg.as_bytes()).unwrap(),
        &fonts,
        &PngOptions::default(),
        1024,
    );
    assert!(
        matches!(result, Err(PngError::MemoryLimit { .. })),
        "{result:?}"
    );
}

#[test]
fn invalid_data_encoding_and_text_outside_the_root_fail_closed() {
    reject(r#"<image href="data:image/png;base64,A"/>"#);
    reject(r#"<image href="data:image/png;base64,AAA==="/>"#);
    reject(r#"<image href="data:image/png;base64,AA=AA"/>"#);
    assert!(!check_svg(b"<svg/>trailing").is_safe());
    assert!(!check_svg(b"leading<svg/>").is_safe());
    assert!(!check_svg(b"<svg/>&amp;").is_safe());
}

#[test]
fn font_face_exception_cannot_load_external_or_nonfont_data() {
    for source in [
        r#"url("https://example.invalid/font.ttf") format("truetype")"#,
        r#"url("data:text/html;base64,AAAA") format("truetype")"#,
        r#"url("data:image/svg+xml;base64,AAAA") format("truetype")"#,
        r#"local("system font")"#,
        r#"url("data:font/ttf;base64,AAAA") format("truetype"),url("https://example.invalid/font.ttf")"#,
        r#"u&#114;l("https://example.invalid/font.ttf") format("truetype")"#,
    ] {
        reject(&format!(
            r#"<style>@font-face {{font-family:"Demo";src:{source};}}</style>"#
        ));
    }
    reject(
        r#"<style>@font-face {font-family:"Demo";src:url("data:font/ttf;base64,AAAA") format("truetype");behavior:expression(alert(1));}</style>"#,
    );
}

#[test]
fn static_lossless_webp_still_renders() {
    let (_dir, fonts) = fonts();
    // Independently generated by ImageMagick's lossless WebP encoder.
    let webp = [
        0x52, 0x49, 0x46, 0x46, 0x1c, 0, 0, 0, 0x57, 0x45, 0x42, 0x50, 0x56, 0x50, 0x38, 0x4c,
        0x0f, 0, 0, 0, 0x2f, 0, 0, 0, 0, 7, 0x10, 0xfd, 0x8f, 0xfe, 7, 0x22, 0xa2, 0xff, 1, 0,
    ];
    let svg = image_svg("image/webp", &webp, 1);
    let png = rasterize_svg(
        CheckedSvg::new(svg.as_bytes()).unwrap(),
        &fonts,
        &PngOptions::default(),
    )
    .unwrap();
    let pm = Pixmap::decode_png(&png).unwrap();
    assert_eq!((pm.pixels()[0].red(), pm.pixels()[0].green()), (255, 0));
}

#[test]
fn svg_depth_and_element_limits_precede_tree_construction() {
    let nested = format!(
        "<svg>{}<rect/>{}</svg>",
        "<g>".repeat(128),
        "</g>".repeat(128)
    );
    assert!(check_svg(nested.as_bytes())
        .issues
        .iter()
        .any(|i| i.kind == hae_render_rhwp::svg_safety::SvgIssueKind::LimitExceeded));
    let many = format!("<svg>{}</svg>", "<rect/>".repeat(100_000));
    assert!(check_svg(many.as_bytes())
        .issues
        .iter()
        .any(|i| i.kind == hae_render_rhwp::svg_safety::SvgIssueKind::LimitExceeded));
}

#[test]
fn dimension_and_scratch_overflow_is_an_error_even_with_unlimited_caller_caps() {
    let (_dir, fonts) = fonts();
    let mut png = small_png(1, 1);
    png[16..24].fill(0xff);
    let svg = image_svg("image/png", &png, 1);
    let result = rasterize_svg_with_budget(
        CheckedSvg::new(svg.as_bytes()).unwrap(),
        &fonts,
        &PngOptions {
            scale: 1.0,
            max_pixels: u64::MAX,
        },
        u64::MAX,
    );
    assert!(result.is_err());
}
