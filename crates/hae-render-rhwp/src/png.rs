//! PNG rasterisation of checked SVG (spec §9·§10, task 3-4).
//!
//! Only [`CheckedSvg`] values — bytes that passed
//! [`crate::svg_safety::check_svg`] — can be rasterised. resvg runs with
//! external resources disabled: `<image>` hrefs resolve only for embedded
//! `data:` raster images; file paths and URLs resolve to nothing, and nested
//! SVG data images are refused. Text uses only the fonts of the configured
//! [`FontSet`], never system fonts.

use crate::fonts::FontSet;
use crate::svg_safety::{check_svg, SvgCheck};
use resvg::tiny_skia::{Color, Pixmap, Transform};
use resvg::usvg::{self, ImageHrefResolver, ImageKind};

/// SVG bytes that passed the safety scan.
#[derive(Debug, Clone, Copy)]
pub struct CheckedSvg<'a>(&'a [u8]);

impl<'a> CheckedSvg<'a> {
    /// Scan `bytes`; unsafe or malformed SVG is returned as the failing check.
    pub fn new(bytes: &'a [u8]) -> Result<CheckedSvg<'a>, SvgCheck> {
        let c = check_svg(bytes);
        if c.is_safe() {
            Ok(CheckedSvg(bytes))
        } else {
            Err(c)
        }
    }
    pub fn bytes(&self) -> &'a [u8] {
        self.0
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PngOptions {
    /// Output pixels per SVG user unit (rhwp emits CSS pixels, 96 dpi).
    pub scale: f32,
    /// Maximum output pixel count.
    pub max_pixels: u64,
}

impl Default for PngOptions {
    fn default() -> Self {
        PngOptions {
            scale: 1.0,
            max_pixels: hae_diag::Limits::default().max_image_pixels,
        }
    }
}

#[derive(Debug)]
pub enum PngError {
    Parse(String),
    TooLarge {
        width: u64,
        height: u64,
        max_pixels: u64,
    },
    Encode(String),
}

impl std::fmt::Display for PngError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PngError::Parse(m) => write!(f, "SVG could not be parsed for rasterisation: {m}"),
            PngError::TooLarge {
                width,
                height,
                max_pixels,
            } => write!(f, "PNG {width}x{height} exceeds {max_pixels} pixels"),
            PngError::Encode(m) => write!(f, "PNG encoding failed: {m}"),
        }
    }
}

impl std::error::Error for PngError {}

fn options(fonts: &FontSet) -> usvg::Options<'static> {
    let mut opt = usvg::Options {
        fontdb: fonts.database(),
        ..usvg::Options::default()
    };
    opt.image_href_resolver = ImageHrefResolver {
        resolve_data: Box::new(|mime, data, _| match mime {
            "image/jpg" | "image/jpeg" => Some(ImageKind::JPEG(data)),
            "image/png" => Some(ImageKind::PNG(data)),
            "image/gif" => Some(ImageKind::GIF(data)),
            "image/webp" => Some(ImageKind::WEBP(data)),
            _ => None,
        }),
        resolve_string: Box::new(|_, _| None),
    };
    opt
}

/// Rasterise a checked SVG page to PNG bytes (opaque white background).
pub fn rasterize_svg(
    svg: CheckedSvg<'_>,
    fonts: &FontSet,
    opts: &PngOptions,
) -> Result<Vec<u8>, PngError> {
    let tree = usvg::Tree::from_data(svg.bytes(), &options(fonts))
        .map_err(|e| PngError::Parse(e.to_string()))?;
    let size = tree.size();
    let w = (f64::from(size.width()) * f64::from(opts.scale))
        .ceil()
        .max(1.0) as u64;
    let h = (f64::from(size.height()) * f64::from(opts.scale))
        .ceil()
        .max(1.0) as u64;
    if w.checked_mul(h).is_none_or(|px| px > opts.max_pixels)
        || w > u64::from(u32::MAX)
        || h > u64::from(u32::MAX)
    {
        return Err(PngError::TooLarge {
            width: w,
            height: h,
            max_pixels: opts.max_pixels,
        });
    }
    let mut pixmap = Pixmap::new(w as u32, h as u32).ok_or(PngError::TooLarge {
        width: w,
        height: h,
        max_pixels: opts.max_pixels,
    })?;
    pixmap.fill(Color::WHITE);
    resvg::render(
        &tree,
        Transform::from_scale(opts.scale, opts.scale),
        &mut pixmap.as_mut(),
    );
    pixmap
        .encode_png()
        .map_err(|e| PngError::Encode(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sandbox::TempDir;

    fn empty_fonts() -> (TempDir, FontSet) {
        let td = TempDir::new_in(&std::env::temp_dir(), "hae-png-test-").unwrap();
        let fs = FontSet::scan(td.path()).unwrap();
        (td, fs)
    }

    const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10" viewBox="0 0 20 10">
        <rect x="0" y="0" width="10" height="10" fill="#ff0000"/></svg>"##;

    #[test]
    fn rasterizes_a_plain_svg() {
        let (_t, fonts) = empty_fonts();
        let svg = CheckedSvg::new(SVG.as_bytes()).unwrap();
        let png = rasterize_svg(svg, &fonts, &PngOptions::default()).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        let (w, h) = (
            u32::from_be_bytes(png[16..20].try_into().unwrap()),
            u32::from_be_bytes(png[20..24].try_into().unwrap()),
        );
        assert_eq!((w, h), (20, 10));
    }

    #[test]
    fn scale_changes_size_and_pixel_cap_is_enforced() {
        let (_t, fonts) = empty_fonts();
        let svg = CheckedSvg::new(SVG.as_bytes()).unwrap();
        let png = rasterize_svg(
            svg,
            &fonts,
            &PngOptions {
                scale: 2.0,
                max_pixels: 10_000,
            },
        )
        .unwrap();
        assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), 40);
        let e = rasterize_svg(
            svg,
            &fonts,
            &PngOptions {
                scale: 2.0,
                max_pixels: 100,
            },
        )
        .unwrap_err();
        assert!(matches!(e, PngError::TooLarge { .. }), "{e}");
    }

    #[test]
    fn unsafe_svg_never_reaches_the_rasterizer() {
        let bad = br#"<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>"#;
        let e = CheckedSvg::new(bad).unwrap_err();
        assert!(!e.is_safe());
    }

    #[test]
    fn external_image_files_are_not_loaded() {
        // An <image> pointing at a real local file must render as if absent.
        let td = TempDir::new_in(&std::env::temp_dir(), "hae-png-ext-").unwrap();
        let (_t, fonts) = empty_fonts();
        let blank = rasterize_svg(
            CheckedSvg::new(br#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"/>"#)
                .unwrap(),
            &fonts,
            &PngOptions::default(),
        )
        .unwrap();
        // 1x1 red PNG on disk.
        let red = {
            let mut p = Pixmap::new(1, 1).unwrap();
            p.fill(Color::from_rgba8(255, 0, 0, 255));
            p.encode_png().unwrap()
        };
        let file = td.path().join("red.png");
        std::fs::write(&file, red).unwrap();
        let svg = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="8" height="8"><image width="8" height="8" xlink:href="{}"/></svg>"##,
            file.display()
        );
        // The path form is not data:/# — the safety scan refuses it first.
        assert!(CheckedSvg::new(svg.as_bytes()).is_err());
        // And even if it were passed on, the resolver loads nothing.
        let tree = usvg::Tree::from_data(svg.as_bytes(), &options(&fonts)).unwrap();
        let mut pm = Pixmap::new(8, 8).unwrap();
        pm.fill(Color::WHITE);
        resvg::render(&tree, Transform::identity(), &mut pm.as_mut());
        let direct = pm.encode_png().unwrap();
        assert_eq!(
            direct.len(),
            {
                let mut p2 = Pixmap::new(8, 8).unwrap();
                p2.fill(Color::WHITE);
                p2.encode_png().unwrap().len()
            },
            "external file leaked into the render"
        );
        assert_eq!(&blank[..8], b"\x89PNG\r\n\x1a\n");
    }
}
