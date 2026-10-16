//! Validated, frozen PNG/JPEG inputs and source-preserving image replacement.
//! Filesystem rooting and the single file read belong to hae-project. This
//! module owns bytes only; neither applying an edit nor replaying it reads paths.

use crate::{Draft, Intent, ResourceChange};
use hae_diag::{Code, EngineError, Limits, Result, Stage};
use hae_model::{is_hp, Kind};
use hae_package::NS_HC;
use hae_xml::{escape_attr, XmlDoc};
use std::{collections::HashSet, io::Cursor, sync::Arc};
use zune_jpeg::{zune_core::bytestream::ZCursor, JpegDecoder};

const MAX_PIXELS: u64 = 40_960_000;
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

/// Immutable validated original image bytes. Clones share the frozen payload;
/// no decoded pixels or source path are retained, and no re-encoding occurs.
#[derive(Clone)]
pub struct ImageBytes {
    bytes: Arc<[u8]>,
    hash: String,
    format: &'static str,
    dimensions: (u32, u32),
}

impl std::fmt::Debug for ImageBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageBytes")
            .field("len", &self.bytes.len())
            .field("sha256", &self.hash)
            .field("format", &self.format)
            .field("dimensions", &self.dimensions)
            .finish()
    }
}

fn invalid(reason: &str, message: impl Into<String>) -> EngineError {
    EngineError::new(Code::Unsupported, Stage::Validate, message).with_reason(reason)
}

fn limit(message: impl Into<String>) -> EngineError {
    EngineError::new(Code::ResourceLimit, Stage::Validate, message)
}

fn check_dimensions((w, h): (u32, u32), limits: &Limits) -> Result<()> {
    if w == 0 || h == 0 {
        return Err(invalid("invalid_image", "zero image dimension"));
    }
    if u64::from(w) * u64::from(h) > limits.max_image_pixels.min(MAX_PIXELS) {
        return Err(limit("image pixel limit exceeded"));
    }
    Ok(())
}

fn buffer(size: usize) -> Result<Vec<u8>> {
    let mut b = Vec::new();
    b.try_reserve_exact(size)
        .map_err(|_| limit("cannot allocate image decode buffer"))?;
    b.resize(size, 0);
    Ok(b)
}

impl ImageBytes {
    /// Consume the bytes read once by the caller, fully validate one static
    /// PNG/JPEG, then freeze the unchanged original payload and SHA-256.
    /// Size limits are checked before decoder/header/pixel allocations.
    pub fn from_bytes(bytes: Vec<u8>, limits: &Limits) -> Result<Self> {
        if bytes.len() as u64 > limits.max_hwpx_bytes
            || (bytes.len() as u64).saturating_mul(2) > limits.max_session_memory
        {
            return Err(limit("image input byte limit exceeded"));
        }
        let (format, dimensions) = if bytes.starts_with(PNG_SIGNATURE) {
            ("png", validate_png(&bytes, limits)?)
        } else if bytes.starts_with(&[0xff, 0xd8]) {
            ("jpeg", validate_jpeg(&bytes, limits)?)
        } else {
            return Err(invalid(
                "unsupported_image_format",
                "only static PNG and JPEG images are supported",
            ));
        };
        let hash = hae_package::sha256_hex(&bytes);
        Ok(Self {
            bytes: Arc::from(bytes),
            hash,
            format,
            dimensions,
        })
    }

    pub fn bytes(&self) -> &Arc<[u8]> {
        &self.bytes
    }
    pub fn sha256(&self) -> &str {
        &self.hash
    }
    /// The detected format, either `png` or `jpeg` (independent of filenames).
    pub fn format(&self) -> &'static str {
        self.format
    }
    pub fn dimensions(&self) -> (u32, u32) {
        self.dimensions
    }
    pub fn media_type(&self) -> &'static str {
        if self.format == "png" {
            "image/png"
        } else {
            "image/jpeg"
        }
    }
}

fn png_error(e: png::DecodingError) -> EngineError {
    if matches!(e, png::DecodingError::LimitsExceeded) {
        limit("PNG decoder allocation limit exceeded")
    } else {
        invalid("invalid_image", format!("invalid PNG: {e}"))
    }
}

fn validate_png(bytes: &[u8], limits: &Limits) -> Result<(u32, u32)> {
    let bad = || invalid("invalid_image", "truncated or malformed PNG chunks");
    // Inspect IHDR before read_info: even metadata/header decoding can allocate.
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
        let head = bytes.get(pos..pos.saturating_add(8)).ok_or_else(bad)?;
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
                return Err(invalid(
                    "animated_image",
                    "APNG is not supported, including one-frame APNG",
                ))
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
    let available = limits.max_session_memory.saturating_sub(bytes.len() as u64);
    // png accounts its own parser/row scratch allocations; the external output
    // buffer is accounted separately. Ignore compressed ancillary metadata:
    // validating it is unnecessary for pixels and could inflate an ICC/text bomb.
    let internal = available.min(64 * 1024 * 1024).min(available / 2);
    let internal =
        usize::try_from(internal).map_err(|_| limit("PNG memory budget exceeds address space"))?;
    let mut options = png::DecodeOptions::default();
    options.set_ignore_adler32(false);
    options.set_ignore_crc(false);
    options.set_skip_ancillary_crc_failures(false);
    options.set_ignore_text_chunk(true);
    options.set_ignore_iccp_chunk(true);
    let mut decoder = png::Decoder::new_with_options(Cursor::new(bytes), options);
    decoder.set_limits(png::Limits { bytes: internal });
    let mut reader = decoder.read_info().map_err(png_error)?;
    if reader.info().animation_control.is_some() || reader.info().frame_control.is_some() {
        return Err(invalid("animated_image", "animated PNG is not supported"));
    }
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| limit("PNG output size overflow"))?;
    if (size as u64).saturating_add(internal as u64) > available {
        return Err(limit("PNG decode memory budget exceeded"));
    }
    let mut out = buffer(size)?;
    reader.next_frame(&mut out).map_err(png_error)?;
    reader.finish().map_err(png_error)?; // validates through IEND, including CRCs
    Ok(dims)
}

/// Validate JPEG framing without allocating: exactly one SOF and terminal EOI.
/// Progressive JPEG may have multiple SOS scans; those are not multiple frames.
fn jpeg_dimensions(bytes: &[u8], limits: &Limits) -> Result<(u32, u32)> {
    let bad = || invalid("invalid_image", "truncated or malformed JPEG markers");
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
            0 if in_scan => continue, // stuffed entropy byte
            0xd0..=0xd7 if in_scan => continue,
            0xd9 => {
                return if saw_scan && pos == bytes.len() {
                    dims.ok_or_else(bad)
                } else {
                    Err(bad())
                }
            }
            0 | 0xd8 | 0x01 | 0xd0..=0xd7 => return Err(bad()),
            _ => {}
        }
        in_scan = false;
        let length = bytes.get(pos..pos.saturating_add(2)).ok_or_else(bad)?;
        let len = u16::from_be_bytes([length[0], length[1]]) as usize;
        if len < 2 {
            return Err(bad());
        }
        let end = pos.checked_add(len).ok_or_else(bad)?;
        let payload = bytes.get(pos + 2..end).ok_or_else(bad)?;
        if marker == 0xe2 && payload.starts_with(b"MPF\0") {
            return Err(invalid(
                "multiframe_image",
                "MPO/multi-picture JPEG is not supported",
            ));
        }
        if (0xc0..=0xcf).contains(&marker) && ![0xc4, 0xc8, 0xcc].contains(&marker) {
            if dims.is_some() {
                return Err(invalid(
                    "multiframe_image",
                    "multiple JPEG frames are not supported",
                ));
            }
            if ![0xc0, 0xc1, 0xc2].contains(&marker) {
                return Err(invalid(
                    "unsupported_image_format",
                    "unsupported JPEG coding process",
                ));
            }
            if payload.len() < 6 {
                return Err(bad());
            }
            let d = (
                u32::from(u16::from_be_bytes([payload[3], payload[4]])),
                u32::from(u16::from_be_bytes([payload[1], payload[2]])),
            );
            check_dimensions(d, limits)?;
            // zune's coefficient storage includes MCU padding and up to four
            // components. Bound that scratch, output and header metadata before
            // decode_headers/decode_into allocate anything (16 bytes/padded pixel
            // plus fixed row/table headroom, and metadata bounded by input size).
            let padded = (u64::from(d.0).div_ceil(32) * 32) * (u64::from(d.1).div_ceil(32) * 32);
            let memory = padded
                .saturating_mul(16)
                .saturating_add((bytes.len() as u64).saturating_mul(4))
                .saturating_add(1024 * 1024);
            if memory > limits.max_session_memory {
                return Err(limit("JPEG decode memory budget exceeded"));
            }
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

fn validate_jpeg(bytes: &[u8], limits: &Limits) -> Result<(u32, u32)> {
    let dims = jpeg_dimensions(bytes, limits)?;
    jpeg_safety::validate(bytes, dims)?;
    let mut decoder = JpegDecoder::new(ZCursor::new(bytes));
    let options = decoder
        .options()
        .set_strict_mode(true)
        .set_max_width(dims.0 as usize)
        .set_max_height(dims.1 as usize);
    decoder.set_options(options);
    let error = |e| invalid("invalid_image", format!("invalid JPEG: {e:?}"));
    decoder.decode_headers().map_err(error)?;
    let info = decoder
        .info()
        .ok_or_else(|| invalid("invalid_image", "JPEG has no frame info"))?;
    if (u32::from(info.width), u32::from(info.height)) != dims {
        return Err(invalid("invalid_image", "inconsistent JPEG dimensions"));
    }
    let size = decoder
        .output_buffer_size()
        .ok_or_else(|| limit("JPEG output size overflow"))?;
    if size as u64 > limits.max_session_memory.saturating_sub(bytes.len() as u64) {
        return Err(limit("JPEG output memory budget exceeded"));
    }
    decoder.decode_into(&mut buffer(size)?).map_err(error)?;
    Ok(dims)
}

fn opf(ns: Option<&str>) -> bool {
    matches!(
        ns,
        Some("http://www.idpf.org/2007/opf/") | Some("http://www.idpf.org/2007/opf")
    )
}

fn check_picture(doc: &XmlDoc, picture: usize, target: &str) -> Result<usize> {
    let unsupported = |reason| {
        invalid(
            reason,
            "image replacement requires an uncropped image without effects",
        )
        .with_target(target)
    };
    let children = &doc.el(picture).children;
    let images: Vec<_> = children
        .iter()
        .copied()
        .filter(|&i| doc.el(i).is(NS_HC, "img"))
        .collect();
    if images.len() != 1 {
        return Err(unsupported("no_unique_embedded_media"));
    }
    let img = doc.el(images[0]);
    if img.attr("binaryItemIDRef").is_none() {
        return Err(unsupported("no_embedded_media"));
    }
    if img.attr_value("effect").is_some_and(|v| v != "REAL_PIC")
        || ["bright", "contrast", "alpha"]
            .iter()
            .any(|k| img.attr_value(k).is_some_and(|v| v.parse::<i64>() != Ok(0)))
        || doc
            .el(picture)
            .attr_value("reverse")
            .is_some_and(|v| v != "0")
    {
        return Err(unsupported("effects"));
    }
    for &c in children {
        let e = doc.el(c);
        if !is_hp(e.ns.as_deref()) {
            continue;
        }
        match e.local.as_str() {
            "effects" => {
                if !e.children.is_empty()
                    || !e.attrs.is_empty()
                    || e.items
                        .iter()
                        .filter_map(|i| i.text())
                        .any(|t| !t.trim().is_empty())
                {
                    return Err(unsupported("effects"));
                }
            }
            "shadow" | "glow" | "softEdge" | "reflection" => return Err(unsupported("effects")),
            "imgClip" => {
                let number = |el: &hae_xml::Element, key| {
                    el.attr_value(key).and_then(|v| v.parse::<i64>().ok())
                };
                if number(e, "left") != Some(0) || number(e, "top") != Some(0) {
                    return Err(unsupported("cropped"));
                }
                let dim = children
                    .iter()
                    .map(|&i| doc.el(i))
                    .find(|e| is_hp(e.ns.as_deref()) && e.local == "imgDim");
                for (edge, size) in [("right", "dimwidth"), ("bottom", "dimheight")] {
                    match number(e, edge) {
                        Some(0) => {}
                        Some(v) if v > 0 && dim.and_then(|d| number(d, size)) == Some(v) => {}
                        _ => return Err(unsupported("cropped")),
                    }
                }
            }
            _ => {}
        }
    }
    Ok(images[0])
}

pub(crate) fn image_replace(d: &mut Draft, target: &str, image: &ImageBytes) -> Result<()> {
    let n = d.node(target, Some(Kind::Picture))?;
    check_dimensions(image.dimensions, &d.doc.limits).map_err(|e| e.with_target(target))?;
    let section = d.doc.doc_of(&n);
    let img = check_picture(section, n.el, target)?;
    let reference = section.el(img).attr("binaryItemIDRef").ok_or_else(|| {
        invalid("no_embedded_media", "image has no binary reference").with_target(target)
    })?;
    let old_item = d.doc.info.item_by_id(&reference.value).ok_or_else(|| {
        invalid("no_embedded_media", "image media is missing").with_target(target)
    })?;
    if d.doc
        .payload(&old_item.entry)
        .is_some_and(|old| old.as_ref() == image.bytes.as_ref())
    {
        return Ok(());
    }
    let rootfile = &d.doc.info.rootfile;
    let hpf = &d.doc.xml[rootfile];
    let manifests: Vec<_> = hpf
        .el(hpf.root())
        .children
        .iter()
        .copied()
        .filter(|&i| hpf.el(i).local == "manifest" && opf(hpf.el(i).ns.as_deref()))
        .collect();
    if manifests.len() != 1 {
        return Err(invalid("manifest_shape", "expected one package manifest").with_target(target));
    }
    let manifest = hpf.el(manifests[0]);
    let close = manifest.end.as_ref().ok_or_else(|| {
        invalid(
            "manifest_shape",
            "empty manifest cannot contain existing image",
        )
    })?;
    let used_ids: HashSet<_> = d
        .doc
        .info
        .manifest
        .iter()
        .map(|m| m.id.to_ascii_lowercase())
        .collect();
    let used_names: HashSet<_> = d
        .doc
        .state
        .entries
        .iter()
        .map(|e| e.name.to_ascii_lowercase())
        .collect();
    let (id, name) = (1u64..)
        .map(|i| {
            let id = format!("hae-image-{i}");
            let name = format!(
                "BinData/{id}.{}",
                if image.format == "png" { "png" } else { "jpg" }
            );
            (id, name)
        })
        .find(|(id, name)| {
            !used_ids.contains(id) && !used_names.contains(&name.to_ascii_lowercase())
        })
        .ok_or_else(|| limit("image resource identifiers exhausted"))?;
    // An explicit local namespace makes the inserted item independent of any
    // prefix rebindings on the original manifest and preserves its source bytes.
    let item = format!(
        "<hae:item xmlns:hae=\"{}\" id=\"{}\" href=\"{}\" media-type=\"{}\"/>",
        escape_attr(manifest.ns.as_deref().unwrap_or_default()),
        id,
        name,
        image.media_type()
    );
    let old_total = d.doc.state.total_bytes();
    let growth = (item.len() as u64)
        .saturating_add(image.bytes.len() as u64)
        .saturating_add(id.len().saturating_sub(reference.value_span.len()) as u64);
    let limits = &d.doc.limits;
    if d.doc.state.entries.len() as u64 >= limits.max_entries
        || old_total.saturating_add(growth) > limits.max_inflate_total
        || old_total
            .saturating_add(growth)
            .saturating_add(image.bytes.len() as u64)
            > limits.max_session_memory
    {
        return Err(limit("image replacement package budget exceeded").with_target(target));
    }
    let path = section.path(n.el);
    let new_section = section.splice(
        vec![(reference.value_span.clone(), escape_attr(&id).into_bytes())],
        d.opts(),
    )?;
    let new_hpf = hpf.splice(
        vec![(close.start..close.start, item.into_bytes())],
        d.opts(),
    )?;
    let mut state = d.doc.state.clone();
    state.set(&n.entry, new_section.bytes().clone());
    state.set(rootfile, new_hpf.bytes().clone());
    state.add(name, image.bytes.clone(), 0);
    // Only publish a completely rebuilt candidate; no intermediate broken refs.
    d.doc = d.doc.rebuild(state, d.doc.stale_layout.clone())?;
    d.changed.push(target.to_owned());
    d.resources.push(ResourceChange {
        kind: "media",
        resource_id: id.clone(),
        action: "added",
    });
    d.resources.push(ResourceChange {
        kind: "media",
        resource_id: id,
        action: "referenced",
    });
    d.intents.push(Intent::ImageReplace {
        entry: n.entry,
        path,
        image_sha256: image.hash.clone(),
    });
    // Geometry is byte-identical, so intentionally no lineseg anchor/PrvText.
    Ok(())
}

// zune-jpeg 0.5.15 fills the bit buffer with zeroes on EOI even in strict
// mode. Walk Huffman scans independently to prove every expected block has
// real entropy bits. This does not reconstruct pixels or re-encode anything.
mod jpeg_safety {
    use super::{invalid, limit};
    use hae_diag::Result;

    fn bad() -> hae_diag::EngineError {
        invalid("invalid_image", "incomplete or malformed JPEG entropy scan")
    }

    #[derive(Clone)]
    struct Huffman {
        counts: [u8; 16],
        values: Vec<u8>,
    }

    struct Bits<'a> {
        bytes: &'a [u8],
        pos: usize,
        byte: u8,
        left: u8,
    }
    impl Bits<'_> {
        fn read(&mut self, count: u8) -> Result<u32> {
            let mut v = 0;
            for _ in 0..count {
                if self.left == 0 {
                    self.byte = *self.bytes.get(self.pos).ok_or_else(bad)?;
                    self.pos += 1;
                    if self.byte == 255 {
                        if self.bytes.get(self.pos) != Some(&0) {
                            return Err(bad());
                        }
                        self.pos += 1;
                    }
                    self.left = 8;
                }
                self.left -= 1;
                v = (v << 1) | u32::from((self.byte >> self.left) & 1);
            }
            Ok(v)
        }
        fn align(&mut self) -> Result<()> {
            let mask = (1u16 << self.left) - 1;
            if u16::from(self.byte) & mask != mask {
                return Err(bad());
            }
            self.left = 0;
            Ok(())
        }
        fn restart(&mut self, ordinal: usize) -> Result<()> {
            self.align()?;
            if self.bytes.get(self.pos) != Some(&255) {
                return Err(bad());
            }
            while self.bytes.get(self.pos) == Some(&255) {
                self.pos += 1;
            }
            if self.bytes.get(self.pos) != Some(&(0xd0 + (ordinal % 8) as u8)) {
                return Err(bad());
            }
            self.pos += 1;
            Ok(())
        }
    }
    impl Huffman {
        fn symbol(&self, bits: &mut Bits<'_>) -> Result<u8> {
            let (mut code, mut first, mut index) = (0u32, 0u32, 0usize);
            for &count in &self.counts {
                code = (code << 1) | bits.read(1)?;
                if code >= first && code - first < u32::from(count) {
                    return self
                        .values
                        .get(index + (code - first) as usize)
                        .copied()
                        .ok_or_else(bad);
                }
                index += usize::from(count);
                first = (first + u32::from(count)) << 1;
            }
            Err(bad())
        }
    }

    struct Component {
        id: u8,
        h: usize,
        v: usize,
        stride: usize,
        width: usize,
        height: usize,
        nonzero: Vec<u64>,
    }

    struct Scan {
        selectors: Vec<(usize, usize, usize)>,
        start: u8,
        end: u8,
        high: u8,
        progressive: bool,
    }

    fn block(
        bits: &mut Bits<'_>,
        tables: &[Option<Huffman>; 8],
        scan: &Scan,
        dc: usize,
        ac: usize,
        mask: &mut u64,
        eob: &mut u32,
    ) -> Result<()> {
        let table = |i| tables.get(i).and_then(Option::as_ref).ok_or_else(bad);
        if scan.start == 0 {
            if scan.high == 0 {
                let size = table(dc)?.symbol(bits)?;
                if size > 11 {
                    return Err(bad());
                }
                bits.read(size)?;
            } else {
                bits.read(1)?;
            }
            if scan.end == 0 {
                return Ok(());
            }
        }
        let mut k = scan.start.max(1);
        if scan.high == 0 {
            if *eob > 0 {
                *eob -= 1;
                return Ok(());
            }
            while k <= scan.end {
                let symbol = table(ac + 4)?.symbol(bits)?;
                let run = symbol >> 4;
                let size = symbol & 15;
                if size == 0 {
                    if run == 15 {
                        if u16::from(k) + 16 > u16::from(scan.end) + 1 {
                            return Err(bad());
                        }
                        k += 16;
                    } else {
                        if !scan.progressive && run != 0 {
                            return Err(bad());
                        }
                        *eob = (1u32 << run) + bits.read(run)? - 1;
                        break;
                    }
                } else {
                    if size > 10 || k + run > scan.end {
                        return Err(bad());
                    }
                    k += run;
                    bits.read(size)?;
                    *mask |= 1u64 << k;
                    k += 1;
                }
            }
        } else {
            if *eob == 0 {
                while k <= scan.end {
                    let symbol = table(ac + 4)?.symbol(bits)?;
                    let mut run = symbol >> 4;
                    let size = symbol & 15;
                    if size == 0 && run < 15 {
                        *eob = (1u32 << run) + bits.read(run)?;
                        break;
                    }
                    if size > 1 {
                        return Err(bad());
                    }
                    if size == 1 {
                        bits.read(1)?;
                    } else {
                        run = 16;
                    }
                    while k <= scan.end {
                        if *mask & (1u64 << k) != 0 {
                            bits.read(1)?;
                        } else if run == 0 {
                            break;
                        } else {
                            run -= 1;
                        }
                        k += 1;
                    }
                    if run != 0 || (size == 1 && k > scan.end) {
                        return Err(bad());
                    }
                    if size == 1 {
                        *mask |= 1u64 << k;
                        k += 1;
                    }
                }
            }
            if *eob > 0 {
                while k <= scan.end {
                    if *mask & (1u64 << k) != 0 {
                        bits.read(1)?;
                    }
                    k += 1;
                }
                *eob -= 1;
            }
        }
        Ok(())
    }

    pub(super) fn validate(bytes: &[u8], dimensions: (u32, u32)) -> Result<()> {
        let mut tables: [Option<Huffman>; 8] = std::array::from_fn(|_| None);
        let mut components: Vec<Component> = Vec::new();
        let (mut cols, mut rows) = (0usize, 0usize);
        let (mut pos, mut restart_interval, mut scans) = (2usize, 0usize, 0usize);
        let mut progressive = false;
        loop {
            if bytes.get(pos) != Some(&255) {
                return Err(bad());
            }
            while bytes.get(pos) == Some(&255) {
                pos += 1;
            }
            let marker = *bytes.get(pos).ok_or_else(bad)?;
            pos += 1;
            if marker == 0xd9 {
                return if scans > 0 { Ok(()) } else { Err(bad()) };
            }
            let length = bytes.get(pos..pos + 2).ok_or_else(bad)?;
            let len = u16::from_be_bytes([length[0], length[1]]) as usize;
            if len < 2 {
                return Err(bad());
            }
            let p = bytes.get(pos + 2..pos + len).ok_or_else(bad)?;
            pos += len;
            match marker {
                0xc0..=0xc2 => {
                    progressive = marker == 0xc2;
                    if p.len() < 6
                        || p[0] != 8
                        || p[5] == 0
                        || p[5] > 4
                        || p.len() != 6 + usize::from(p[5]) * 3
                    {
                        return Err(bad());
                    }
                    let mut max_h = 0;
                    let mut max_v = 0;
                    for c in p[6..].chunks_exact(3) {
                        let (h, v) = (usize::from(c[1] >> 4), usize::from(c[1] & 15));
                        if h == 0
                            || h > 4
                            || v == 0
                            || v > 4
                            || components.iter().any(|old| old.id == c[0])
                        {
                            return Err(bad());
                        }
                        max_h = max_h.max(h);
                        max_v = max_v.max(v);
                        components.push(Component {
                            id: c[0],
                            h,
                            v,
                            stride: 0,
                            width: 0,
                            height: 0,
                            nonzero: Vec::new(),
                        });
                    }
                    cols = (dimensions.0 as usize).div_ceil(max_h * 8);
                    rows = (dimensions.1 as usize).div_ceil(max_v * 8);
                    for c in &mut components {
                        c.stride = cols * c.h;
                        c.width = (dimensions.0 as usize * c.h).div_ceil(max_h * 8);
                        c.height = (dimensions.1 as usize * c.v).div_ceil(max_v * 8);
                        let count = c.stride * rows * c.v;
                        c.nonzero
                            .try_reserve_exact(count)
                            .map_err(|_| limit("JPEG scan validation allocation failed"))?;
                        c.nonzero.resize(count, 0);
                    }
                }
                0xc4 => {
                    let mut offset = 0;
                    while offset < p.len() {
                        let head = p.get(offset..offset + 17).ok_or_else(bad)?;
                        let id = usize::from(head[0] >> 4) * 4 + usize::from(head[0] & 15);
                        if head[0] >> 4 > 1 || head[0] & 15 > 3 {
                            return Err(bad());
                        }
                        let counts: [u8; 16] = head[1..].try_into().map_err(|_| bad())?;
                        let mut available = 1i32;
                        for &count in &counts {
                            available = available * 2 - i32::from(count);
                            if available < 0 {
                                return Err(bad());
                            }
                        }
                        let count: usize = counts.iter().map(|&n| usize::from(n)).sum();
                        if count > 256 {
                            return Err(bad());
                        }
                        let values = p
                            .get(offset + 17..offset + 17 + count)
                            .ok_or_else(bad)?
                            .to_vec();
                        tables[id] = Some(Huffman { counts, values });
                        offset += 17 + count;
                    }
                }
                0xdd => {
                    if p.len() != 2 {
                        return Err(bad());
                    }
                    restart_interval = usize::from(u16::from_be_bytes([p[0], p[1]]));
                }
                0xda => {
                    let count = usize::from(*p.first().ok_or_else(bad)?);
                    if count == 0 || count > components.len() || p.len() != count * 2 + 4 {
                        return Err(bad());
                    }
                    let mut selectors = Vec::new();
                    for s in p[1..1 + count * 2].chunks_exact(2) {
                        let ci = components
                            .iter()
                            .position(|c| c.id == s[0])
                            .ok_or_else(bad)?;
                        if s[1] >> 4 > 3
                            || s[1] & 15 > 3
                            || selectors.iter().any(|&(old, _, _)| old == ci)
                        {
                            return Err(bad());
                        }
                        selectors.push((ci, usize::from(s[1] >> 4), usize::from(s[1] & 15)));
                    }
                    let (start, end, high, low) = (
                        p[1 + count * 2],
                        p[2 + count * 2],
                        p[3 + count * 2] >> 4,
                        p[3 + count * 2] & 15,
                    );
                    if start > end
                        || end > 63
                        || high > 13
                        || low > 13
                        || (high != 0 && high != low + 1)
                        || (progressive && ((start == 0 && end != 0) || (start > 0 && count != 1)))
                        || (!progressive && (start != 0 || end != 63 || high != 0 || low != 0))
                    {
                        return Err(bad());
                    }
                    let scan = Scan {
                        selectors,
                        start,
                        end,
                        high,
                        progressive,
                    };
                    let mut bits = Bits {
                        bytes,
                        pos,
                        byte: 0,
                        left: 0,
                    };
                    let (width, height) = if count == 1 {
                        let c = &components[scan.selectors[0].0];
                        (c.width, c.height)
                    } else {
                        (cols, rows)
                    };
                    let mut eob = 0;
                    let mut rst = 0;
                    for mcu in 0..width * height {
                        if mcu > 0 && restart_interval > 0 && mcu % restart_interval == 0 {
                            if eob != 0 {
                                return Err(bad());
                            }
                            bits.restart(rst)?;
                            rst += 1;
                        }
                        for &(ci, dc, ac) in &scan.selectors {
                            let c = &mut components[ci];
                            let (h, v) = if count == 1 { (1, 1) } else { (c.h, c.v) };
                            for y in 0..v {
                                for x in 0..h {
                                    let index =
                                        ((mcu / width) * v + y) * c.stride + (mcu % width) * h + x;
                                    block(
                                        &mut bits,
                                        &tables,
                                        &scan,
                                        dc,
                                        ac,
                                        c.nonzero.get_mut(index).ok_or_else(bad)?,
                                        &mut eob,
                                    )?;
                                }
                            }
                        }
                    }
                    if eob != 0 {
                        return Err(bad());
                    }
                    bits.align()?;
                    pos = bits.pos;
                    scans += 1;
                }
                _ => {}
            }
        }
    }
}
