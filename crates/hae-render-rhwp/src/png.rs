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
use std::sync::{Arc, Mutex};

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
    /// Maximum individual and cumulative embedded-raster plus output pixel count.
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
    /// Cumulative embedded raster and destination canvas pixels exceed the cap.
    PixelLimit {
        required_pixels: u64,
        max_pixels: u64,
    },
    /// Conservative decoded-buffer/scratch reservation exceeds the caller budget.
    MemoryLimit {
        required_bytes: u64,
        max_decoded_bytes: u64,
    },
    InvalidImage(String),
    UnsupportedImage(String),
}

impl std::fmt::Display for PngError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PngError::Parse(m) => write!(f, "SVG could not be parsed for rasterisation: {m}"),
            PngError::TooLarge {
                width,
                height,
                max_pixels,
            } => write!(f, "raster/canvas {width}x{height} exceeds {max_pixels} pixels"),
            PngError::Encode(m) => write!(f, "PNG encoding failed: {m}"),
            PngError::PixelLimit { required_pixels, max_pixels } => write!(f, "raster/canvas pixels {required_pixels} exceed {max_pixels}"),
            PngError::MemoryLimit { required_bytes, max_decoded_bytes } => write!(f, "raster/canvas decode reservation {required_bytes} bytes exceeds {max_decoded_bytes}"),
            PngError::InvalidImage(m) => write!(f, "invalid embedded raster: {m}"),
            PngError::UnsupportedImage(m) => write!(f, "unsupported embedded raster: {m}"),
        }
    }
}

impl std::error::Error for PngError {}

/// Reservation shared with the resolver. The first error survives usvg's
/// normal policy of silently omitting invalid images.
#[derive(Default)]
struct RasterBudget {
    pixels: u64,
    bytes: u64,
    error: Option<PngError>,
}

impl RasterBudget {
    fn reserve(
        &mut self,
        pixels: u64,
        bytes: u64,
        max_pixels: u64,
        max_bytes: u64,
    ) -> Result<(), PngError> {
        let pixels = self
            .pixels
            .checked_add(pixels)
            .ok_or(PngError::PixelLimit {
                required_pixels: u64::MAX,
                max_pixels,
            })?;
        let bytes = self.bytes.checked_add(bytes).ok_or(PngError::MemoryLimit {
            required_bytes: u64::MAX,
            max_decoded_bytes: max_bytes,
        })?;
        if pixels > max_pixels {
            return Err(PngError::PixelLimit {
                required_pixels: pixels,
                max_pixels,
            });
        }
        if bytes > max_bytes {
            return Err(PngError::MemoryLimit {
                required_bytes: bytes,
                max_decoded_bytes: max_bytes,
            });
        }
        self.pixels = pixels;
        self.bytes = bytes;
        Ok(())
    }
}

fn options(
    fonts: &FontSet,
    max_pixels: u64,
    max_bytes: u64,
    budget: Arc<Mutex<RasterBudget>>,
) -> usvg::Options<'static> {
    let mut opt = usvg::Options {
        fontdb: fonts.database(),
        ..usvg::Options::default()
    };
    opt.image_href_resolver = ImageHrefResolver {
        resolve_data: Box::new(move |mime, data, _| {
            let mut budget = budget.lock().unwrap_or_else(|poison| poison.into_inner());
            if budget.error.is_some() {
                return None;
            }
            let result = (|| {
                // No raster decoder, including its metadata parser, is invoked
                // until all its headers/dimensions and reservations pass here.
                let (w, h) = raster_dimensions(mime, &data, max_pixels)?;
                let pixels = dimensions(w, h, max_pixels)?;
                // PNG normalization, GIF/WebP RGBA copies and JPEG coefficients
                // need more than one RGBA buffer. JPEG MCU padding and WebP's
                // lossless transform scratch get additional conservative space.
                let padded =
                    (u64::from(w).div_ceil(32) * 32).checked_mul(u64::from(h).div_ceil(32) * 32);
                let scratch = match mime {
                    "image/jpeg" | "image/jpg" => padded
                        .and_then(|v| v.checked_mul(16))
                        .and_then(|v| v.checked_add(1024 * 1024)),
                    // Lossless WebP's meta-Huffman group index is independent
                    // of pixel count. Charge encoded bytes for worst-case
                    // tables (five alphabets, at most 2328 symbols each), not
                    // just RGBA pixels; ALPH can contain the same coding.
                    "image/webp" => pixels
                        .checked_mul(64)
                        .and_then(|v| v.checked_add(webp_table_budget(&data)?))
                        .and_then(|v| v.checked_add(1024 * 1024)),
                    // Fixed DEFLATE/LZW tables and row buffers matter even on
                    // a one-pixel image. PNG compressed metadata is refused.
                    _ => pixels
                        .checked_mul(16)
                        .and_then(|v| v.checked_add(u64::from(w) * 16))
                        .and_then(|v| v.checked_add(128 * 1024)),
                };
                let bytes = scratch
                    .and_then(|v| v.checked_add((data.len() as u64).saturating_mul(4)))
                    .ok_or(PngError::MemoryLimit {
                        required_bytes: u64::MAX,
                        max_decoded_bytes: max_bytes,
                    })?;
                budget.reserve(pixels, bytes, max_pixels, max_bytes)?;
                match mime {
                    "image/jpg" | "image/jpeg" => Ok(ImageKind::JPEG(data)),
                    "image/png" => Ok(ImageKind::PNG(data)),
                    "image/gif" => Ok(ImageKind::GIF(data)),
                    "image/webp" => Ok(ImageKind::WEBP(data)),
                    _ => Err(PngError::UnsupportedImage(mime.into())),
                }
            })();
            match result {
                Ok(image) => Some(image),
                Err(error) => {
                    budget.error = Some(error);
                    None
                }
            }
        }),
        resolve_string: Box::new(|_, _| None),
    };
    opt
}

fn invalid(message: &str) -> PngError {
    PngError::InvalidImage(message.into())
}
fn unsupported(message: &str) -> PngError {
    PngError::UnsupportedImage(message.into())
}
fn u16be(data: &[u8]) -> u32 {
    u32::from(u16::from_be_bytes([data[0], data[1]]))
}
fn u16le(data: &[u8]) -> u32 {
    u32::from(u16::from_le_bytes([data[0], data[1]]))
}
fn u24le(data: &[u8]) -> u32 {
    u32::from(data[0]) | (u32::from(data[1]) << 8) | (u32::from(data[2]) << 16)
}
fn u32be(data: &[u8]) -> u32 {
    u32::from_be_bytes([data[0], data[1], data[2], data[3]])
}
fn u32le(data: &[u8]) -> u32 {
    u32::from_le_bytes([data[0], data[1], data[2], data[3]])
}

fn dimensions(w: u32, h: u32, max_pixels: u64) -> Result<u64, PngError> {
    if w == 0 || h == 0 {
        return Err(invalid("zero dimension"));
    }
    let pixels = u64::from(w)
        .checked_mul(u64::from(h))
        .ok_or(PngError::TooLarge {
            width: u64::from(w),
            height: u64::from(h),
            max_pixels,
        })?;
    if pixels > max_pixels {
        return Err(PngError::TooLarge {
            width: u64::from(w),
            height: u64::from(h),
            max_pixels,
        });
    }
    Ok(pixels)
}

fn raster_dimensions(mime: &str, data: &[u8], cap: u64) -> Result<(u32, u32), PngError> {
    match mime {
        "image/png" => png_dimensions(data, cap),
        "image/jpeg" | "image/jpg" => jpeg_dimensions(data, cap),
        "image/gif" => gif_dimensions(data, cap),
        "image/webp" => webp_dimensions(data, cap),
        _ => Err(unsupported(mime)),
    }
}

fn png_dimensions(data: &[u8], cap: u64) -> Result<(u32, u32), PngError> {
    if data.len() < 33 || !data.starts_with(b"\x89PNG\r\n\x1a\n") || &data[8..16] != b"\0\0\0\rIHDR"
    {
        return Err(invalid("PNG signature/IHDR"));
    }
    let (w, h) = (u32be(&data[16..20]), u32be(&data[20..24]));
    dimensions(w, h, cap)?;
    let mut pos = 8usize;
    let mut image_data = false;
    while pos < data.len() {
        let header = data
            .get(pos..pos.saturating_add(8))
            .ok_or_else(|| invalid("truncated PNG chunk"))?;
        let len = u32be(header) as usize;
        let end = pos
            .checked_add(12)
            .and_then(|v| v.checked_add(len))
            .ok_or_else(|| invalid("PNG chunk overflow"))?;
        if end > data.len() {
            return Err(invalid("truncated PNG payload"));
        }
        match &header[4..8] {
            b"IHDR" if pos != 8 => return Err(invalid("duplicate PNG IHDR")),
            b"acTL" | b"fcTL" | b"fdAT" => return Err(unsupported("animated PNG")),
            // resvg's PNG decoder does not expose metadata allocation limits.
            // Refuse compressed ancillary streams before it can inflate them.
            b"iCCP" | b"zTXt" | b"iTXt" => {
                return Err(unsupported("PNG compressed/extended metadata"))
            }
            b"IDAT" => image_data = true,
            b"IEND" => {
                if len != 0 || end != data.len() || !image_data {
                    return Err(invalid("PNG IEND"));
                }
                return Ok((w, h));
            }
            _ => {}
        }
        pos = end;
    }
    Err(invalid("PNG missing IEND"))
}

fn jpeg_dimensions(data: &[u8], cap: u64) -> Result<(u32, u32), PngError> {
    if !data.starts_with(&[0xff, 0xd8]) {
        return Err(invalid("JPEG signature"));
    }
    let mut pos = 2usize;
    let mut dims = None;
    let mut entropy = false;
    let mut saw_scan = false;
    while pos < data.len() {
        if entropy {
            while pos < data.len() && data[pos] != 0xff {
                pos += 1;
            }
        }
        if data.get(pos) != Some(&0xff) {
            return Err(invalid("JPEG marker"));
        }
        while data.get(pos) == Some(&0xff) {
            pos += 1;
        }
        let marker = *data.get(pos).ok_or_else(|| invalid("JPEG marker EOF"))?;
        pos += 1;
        match marker {
            0 | 0xd0..=0xd7 if entropy => continue,
            0xd9 => {
                return if saw_scan && pos == data.len() {
                    dims.ok_or_else(|| invalid("JPEG missing SOF"))
                } else {
                    Err(invalid("JPEG EOI"))
                }
            }
            0 | 0x01 | 0xd0..=0xd8 => return Err(invalid("unexpected JPEG marker")),
            _ => {}
        }
        entropy = false;
        let length = data
            .get(pos..pos.saturating_add(2))
            .ok_or_else(|| invalid("JPEG length"))?;
        let len = u16be(length) as usize;
        if len < 2 {
            return Err(invalid("JPEG segment length"));
        }
        let end = pos
            .checked_add(len)
            .ok_or_else(|| invalid("JPEG segment overflow"))?;
        let payload = data
            .get(pos + 2..end)
            .ok_or_else(|| invalid("truncated JPEG segment"))?;
        if (0xc0..=0xcf).contains(&marker) && ![0xc4, 0xc8, 0xcc].contains(&marker) {
            if dims.is_some() {
                return Err(unsupported("multiple JPEG frames"));
            }
            if ![0xc0, 0xc1, 0xc2].contains(&marker) {
                return Err(unsupported("JPEG coding process"));
            }
            if payload.len() < 6 {
                return Err(invalid("JPEG SOF"));
            }
            let (w, h) = (u16be(&payload[3..5]), u16be(&payload[1..3]));
            dimensions(w, h, cap)?;
            if payload[0] != 8
                || !(1..=4).contains(&payload[5])
                || payload.len() != 6 + usize::from(payload[5]) * 3
            {
                return Err(unsupported("JPEG precision/components"));
            }
            if payload[6..]
                .chunks_exact(3)
                .any(|c| c[1] >> 4 == 0 || c[1] >> 4 > 4 || c[1] & 15 == 0 || c[1] & 15 > 4)
            {
                return Err(invalid("JPEG sampling factors"));
            }
            dims = Some((w, h));
        }
        if marker == 0xe2 && payload.starts_with(b"MPF\0") {
            return Err(unsupported("multi-picture JPEG"));
        }
        if marker == 0xdc {
            return Err(unsupported("JPEG DNL dimension mutation"));
        }
        if marker == 0xda {
            if dims.is_none() {
                return Err(invalid("JPEG scan before SOF"));
            }
            entropy = true;
            saw_scan = true;
        }
        pos = end;
    }
    Err(invalid("JPEG missing EOI"))
}

fn gif_subblocks(data: &[u8], mut pos: usize) -> Result<usize, PngError> {
    loop {
        let len = usize::from(
            *data
                .get(pos)
                .ok_or_else(|| invalid("GIF subblock length"))?,
        );
        pos = pos
            .checked_add(len + 1)
            .filter(|p| *p <= data.len())
            .ok_or_else(|| invalid("truncated GIF subblock"))?;
        if len == 0 {
            return Ok(pos);
        }
    }
}

fn gif_dimensions(data: &[u8], cap: u64) -> Result<(u32, u32), PngError> {
    if data.len() < 13 || !(data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a")) {
        return Err(invalid("GIF header"));
    }
    let screen = (u16le(&data[6..8]), u16le(&data[8..10]));
    dimensions(screen.0, screen.1, cap)?;
    let mut pos = 13
        + if data[10] & 0x80 != 0 {
            3usize << ((data[10] & 7) + 1)
        } else {
            0
        };
    let mut frame = None;
    while let Some(tag) = data.get(pos) {
        match tag {
            0x21 => {
                if pos + 2 > data.len() {
                    return Err(invalid("GIF extension"));
                }
                pos = gif_subblocks(data, pos + 2)?;
            }
            0x2c => {
                if frame.is_some() {
                    return Err(unsupported("animated GIF"));
                }
                let desc = data
                    .get(pos + 1..pos + 10)
                    .ok_or_else(|| invalid("GIF frame descriptor"))?;
                let (w, h) = (u16le(&desc[4..6]), u16le(&desc[6..8]));
                dimensions(w, h, cap)?;
                if u16le(&desc[..2]) + w > screen.0 || u16le(&desc[2..4]) + h > screen.1 {
                    return Err(invalid("GIF frame exceeds logical screen"));
                }
                frame = Some((screen.0.max(w), screen.1.max(h)));
                pos += 10
                    + if desc[8] & 0x80 != 0 {
                        3usize << ((desc[8] & 7) + 1)
                    } else {
                        0
                    };
                let code = *data
                    .get(pos)
                    .ok_or_else(|| invalid("GIF missing LZW code size"))?;
                if !(2..=8).contains(&code) {
                    return Err(invalid("GIF LZW code size"));
                }
                pos = gif_subblocks(data, pos + 1)?;
            }
            0x3b if pos + 1 == data.len() => {
                return frame.ok_or_else(|| invalid("GIF missing frame"))
            }
            _ => return Err(invalid("GIF block/trailer")),
        }
    }
    Err(invalid("GIF missing trailer"))
}

// Header parsing has already validated these chunk bounds. Ordinary lossy
// VP8 needs no meta-Huffman tables. VP8L and compressed ALPH must be charged
// by encoded size as well as pixels: a one-pixel meta image can select any of
// 65536 Huffman groups. The deliberately generous 128 KiB/encoded-byte bound
// includes table/vector capacity and all five alphabets per encoded group.
fn webp_table_budget(data: &[u8]) -> Option<u64> {
    let mut pos = 12usize;
    while pos < data.len() {
        let header = data.get(pos..pos.checked_add(8)?)?;
        let len = u32le(&header[4..8]) as usize;
        if &header[..4] == b"VP8L"
            || &header[..4] == b"ALPH" && data.get(pos + 8).is_some_and(|flag| flag & 3 == 1)
        {
            return (data.len() as u64).checked_mul(128 * 1024);
        }
        pos = pos.checked_add(8)?.checked_add(len)?.checked_add(len & 1)?;
    }
    Some(0)
}

fn webp_dimensions(data: &[u8], cap: u64) -> Result<(u32, u32), PngError> {
    if data.len() < 20
        || &data[..4] != b"RIFF"
        || &data[8..12] != b"WEBP"
        || u64::from(u32le(&data[4..8])) + 8 != data.len() as u64
    {
        return Err(invalid("WebP RIFF header/length"));
    }
    let mut pos = 12usize;
    let mut extended = None;
    let mut frame = None;
    while pos < data.len() {
        let header = data
            .get(pos..pos.saturating_add(8))
            .ok_or_else(|| invalid("WebP chunk header"))?;
        let len = u32le(&header[4..8]) as usize;
        let end = pos
            .checked_add(8)
            .and_then(|p| p.checked_add(len))
            .ok_or_else(|| invalid("WebP chunk overflow"))?;
        let payload = data
            .get(pos + 8..end)
            .ok_or_else(|| invalid("WebP truncated chunk"))?;
        let dim = match &header[..4] {
            b"VP8X" => {
                if len != 10 || extended.is_some() || pos != 12 {
                    return Err(invalid("WebP VP8X"));
                }
                let d = (u24le(&payload[4..7]) + 1, u24le(&payload[7..10]) + 1);
                dimensions(d.0, d.1, cap)?;
                if payload[0] & 2 != 0 {
                    return Err(unsupported("animated WebP"));
                }
                extended = Some(d);
                None
            }
            b"VP8L" => {
                if payload.len() < 5 || payload[0] != 0x2f {
                    return Err(invalid("WebP VP8L header"));
                }
                let bits = u32le(&payload[1..5]);
                let d = ((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1);
                dimensions(d.0, d.1, cap)?;
                if bits >> 29 != 0 {
                    return Err(unsupported("WebP VP8L version"));
                }
                Some(d)
            }
            b"VP8 " => {
                if payload.len() < 10 || payload[0] & 1 != 0 || &payload[3..6] != b"\x9d\x01\x2a" {
                    return Err(invalid("WebP VP8 header"));
                }
                Some((
                    u16le(&payload[6..8]) & 0x3fff,
                    u16le(&payload[8..10]) & 0x3fff,
                ))
            }
            b"ANIM" | b"ANMF" => return Err(unsupported("animated WebP")),
            b"ALPH" | b"ICCP" | b"EXIF" | b"XMP " => None,
            _ => return Err(unsupported("WebP chunk type")),
        };
        if let Some(d) = dim {
            dimensions(d.0, d.1, cap)?;
            if frame.replace(d).is_some() {
                return Err(unsupported("multiple WebP bitstreams"));
            }
        }
        pos = end
            .checked_add(len & 1)
            .filter(|p| *p <= data.len())
            .ok_or_else(|| invalid("WebP padding"))?;
    }
    let d = frame.ok_or_else(|| invalid("WebP missing bitstream"))?;
    if extended.is_some_and(|e| e != d) {
        return Err(invalid("WebP canvas/bitstream dimensions disagree"));
    }
    Ok(d)
}

/// Rasterise a checked SVG page to PNG bytes (opaque white background), using
/// the default session-memory limit for decoded buffers. See the bounded API.
pub fn rasterize_svg(
    svg: CheckedSvg<'_>,
    fonts: &FontSet,
    opts: &PngOptions,
) -> Result<Vec<u8>, PngError> {
    rasterize_svg_with_budget(
        svg,
        fonts,
        opts,
        hae_diag::Limits::default().max_session_memory,
    )
}

/// Rasterise with a caller-supplied decoded-buffer budget. This includes the
/// cumulative embedded raster buffers, conservative codec scratch reservations,
/// compressed-image copies and the RGBA destination. It is not an OS/RSS limit:
/// SVG tree/path/font/filter allocations and PNG encoding need caller headroom
/// or a separately supervised worker. Repeated resolver calls are charged again.
/// All pixel/header checks happen before resvg can decode a raster.
/// Nonfinite/nonpositive scale is clamped to 1 for both sizing and rendering.
pub fn rasterize_svg_with_budget(
    svg: CheckedSvg<'_>,
    fonts: &FontSet,
    opts: &PngOptions,
    max_decoded_bytes: u64,
) -> Result<Vec<u8>, PngError> {
    let budget = Arc::new(Mutex::new(RasterBudget::default()));
    let opt = options(
        fonts,
        opts.max_pixels,
        max_decoded_bytes,
        Arc::clone(&budget),
    );
    let parsed = usvg::Tree::from_data(svg.bytes(), &opt);
    let mut budget = budget.lock().unwrap_or_else(|poison| poison.into_inner());
    if let Some(error) = budget.error.take() {
        return Err(error);
    }
    let tree = parsed.map_err(|e| PngError::Parse(e.to_string()))?;
    let size = tree.size();
    let scale = if opts.scale.is_finite() && opts.scale > 0.0 {
        opts.scale
    } else {
        1.0
    };
    let w = (f64::from(size.width()) * f64::from(scale)).ceil().max(1.0) as u64;
    let h = (f64::from(size.height()) * f64::from(scale))
        .ceil()
        .max(1.0) as u64;
    let too_large = || PngError::TooLarge {
        width: w,
        height: h,
        max_pixels: opts.max_pixels,
    };
    let pixels = w
        .checked_mul(h)
        .filter(|p| *p <= opts.max_pixels)
        .ok_or_else(too_large)?;
    if w > u64::from(u32::MAX) || h > u64::from(u32::MAX) {
        return Err(too_large());
    }
    let bytes = pixels.checked_mul(4).ok_or(PngError::MemoryLimit {
        required_bytes: u64::MAX,
        max_decoded_bytes,
    })?;
    budget.reserve(pixels, bytes, opts.max_pixels, max_decoded_bytes)?;
    drop(budget);
    let mut pixmap = Pixmap::new(w as u32, h as u32).ok_or_else(too_large)?;
    pixmap.fill(Color::WHITE);
    resvg::render(
        &tree,
        Transform::from_scale(scale, scale),
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
        let tree = usvg::Tree::from_data(
            svg.as_bytes(),
            &options(
                &fonts,
                u64::MAX,
                u64::MAX,
                Arc::new(Mutex::new(RasterBudget::default())),
            ),
        )
        .unwrap();
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
