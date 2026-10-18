#![cfg(target_os = "linux")]

//! Task 3-4: page-1 PNG of a corpus document, rasterised from checked SVG.

mod common;

use common::*;
use hae_render_rhwp::{RenderRequest, RenderStatus};
use std::fs;

#[test]
#[ignore = "requires optional rhwp renderer; scripts/check.sh runs this when available"]
fn page_one_png_for_corpus_documents() {
    let env = real_env();
    for name in [
        "research-school-plan-reference.hwpx",
        "gyehoek-reference.hwpx",
    ] {
        let bytes = fs::read(corpus(name)).unwrap();
        let t = std::time::Instant::now();
        let out = env
            .adapter
            .preview(
                &RenderRequest {
                    document_id: "doc-png",
                    content_revision: 1,
                    hwpx: &bytes,
                    pages: Some(&[1]),
                    want_png: true,
                    png: Default::default(),
                },
                None,
            )
            .unwrap();
        eprintln!("PNG_TIME {name}: {} ms total", t.elapsed().as_millis());
        assert_eq!(
            out.status,
            RenderStatus::Rendered,
            "{name}: {:?}",
            out.notes
        );
        let p = &out.pages[0];
        let png = p
            .png
            .as_ref()
            .unwrap_or_else(|| panic!("{name}: no PNG, notes {:?}", out.notes));
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        let w = u32::from_be_bytes(png[16..20].try_into().unwrap());
        let h = u32::from_be_bytes(png[20..24].try_into().unwrap());
        assert!(w > 500 && h > 700, "{name}: {w}x{h}");
        assert_eq!(
            p.png_sha256.as_deref(),
            Some(hae_package::sha256_hex(png).as_str())
        );
        // With a Korean font in the font directory, text pixels must appear.
        let has_font = std::fs::read_dir(env._fonts.path())
            .unwrap()
            .next()
            .is_some();
        if has_font {
            let pm = resvg::tiny_skia::Pixmap::decode_png(png).unwrap();
            let dark = pm
                .pixels()
                .iter()
                .filter(|p| p.red() < 90 && p.green() < 90 && p.blue() < 90)
                .count();
            assert!(dark > 500, "{name}: only {dark} dark pixels, text missing");
        }
        if let Ok(dir) = std::env::var("HAE_TEST_PNG_DIR") {
            fs::write(format!("{dir}/{name}.p1.png"), png).unwrap();
        }
    }
}
