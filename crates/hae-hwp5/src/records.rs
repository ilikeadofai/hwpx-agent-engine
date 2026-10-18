//! HWP5 record headers (TLV) and `PARA_TEXT` decoding.
//!
//! Record header layout (4 bytes little-endian) and the size-0xFFF extension are
//! confirmed by three public implementations (hwplib, rhwp, unhwp); the control
//! character classification follows the HWP 5.0 표 6 partition that rhwp and
//! hwplib agree on. See `docs/decisions/hwp5-format-notes.md` §1.4–1.5.

use crate::reject;
use hae_diag::{PackageRejectReason as R, Result};

/// HWPTAG_BEGIN (0x10) + 50.
pub(crate) const TAG_PARA_HEADER: u16 = 0x42;
/// HWPTAG_BEGIN (0x10) + 51.
pub(crate) const TAG_PARA_TEXT: u16 = 0x43;

const EXTENDED_SIZE: u32 = 0xFFF;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RecordHeader {
    pub tag: u16,
    pub level: u16,
    pub size: u32,
}

/// Iterates records of one decompressed stream. After a malformed record the
/// iteration stops for good: the bytes past a broken boundary cannot be trusted
/// to start at another record header.
pub(crate) struct RecordIter<'a> {
    data: &'a [u8],
    pos: usize,
    done: bool,
}

impl<'a> RecordIter<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        RecordIter {
            data,
            pos: 0,
            done: false,
        }
    }
}

impl<'a> Iterator for RecordIter<'a> {
    type Item = Result<(RecordHeader, &'a [u8])>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done || self.pos >= self.data.len() {
            return None;
        }
        let rest = &self.data[self.pos..];
        if rest.len() < 4 {
            self.done = true;
            return Some(Err(reject(
                R::MalformedRecordStream,
                format!("{} trailing bytes cannot hold a record header", rest.len()),
            )));
        }
        let value = u32::from_le_bytes([rest[0], rest[1], rest[2], rest[3]]);
        let tag = (value & 0x3FF) as u16;
        let level = ((value >> 10) & 0x3FF) as u16;
        let size_field = (value >> 20) & 0xFFF;
        let (size, header_len) = if size_field == EXTENDED_SIZE {
            if rest.len() < 8 {
                self.done = true;
                return Some(Err(reject(
                    R::MalformedRecordStream,
                    "extended record header is truncated",
                )));
            }
            (u32::from_le_bytes([rest[4], rest[5], rest[6], rest[7]]), 8)
        } else {
            (size_field, 4)
        };
        let end = header_len + size as usize;
        if end > rest.len() {
            self.done = true;
            return Some(Err(reject(
                R::MalformedRecordStream,
                format!(
                    "record tag {tag} at offset {} declares {size} bytes but only {} remain",
                    self.pos,
                    rest.len() - header_len
                ),
            )));
        }
        let header = RecordHeader { tag, level, size };
        let payload = &rest[header_len..end];
        self.pos += end;
        Some(Ok((header, payload)))
    }
}

/// Iterator over the UTF-16 code units of a `PARA_TEXT` payload with control
/// characters handled: 8-unit controls are skipped whole, char controls are
/// mapped or dropped, 0x000D ends the record.
struct ParaUnits<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> ParaUnits<'a> {
    fn skip_payload(&mut self) -> Result<()> {
        let skip = self
            .pos
            .checked_add(14)
            .filter(|end| *end <= self.data.len());
        match skip {
            Some(end) => {
                self.pos = end;
                Ok(())
            }
            None => Err(reject(
                R::MalformedRecordStream,
                "16-byte control character is truncated at the end of PARA_TEXT",
            )),
        }
    }

    fn next_unit(&mut self) -> Result<Option<u16>> {
        loop {
            let Some(bytes) = self.data.get(self.pos..self.pos + 2) else {
                if self.pos == self.data.len() {
                    return Ok(None);
                }
                return Err(reject(
                    R::MalformedRecordStream,
                    "PARA_TEXT payload has an odd trailing byte",
                ));
            };
            let unit = u16::from_le_bytes([bytes[0], bytes[1]]);
            self.pos += 2;
            match unit {
                0x0000 => continue,        // unusable character: dropped
                0x000D => return Ok(None), // paragraph break: end of text
                0x000A => return Ok(Some(0x000A)),
                0x0018 => return Ok(Some(0x00AD)), // soft hyphen
                0x0019..=0x001D => continue,       // reserved char controls: dropped
                0x001E => return Ok(Some(0x00A0)), // non-breaking space
                0x001F => return Ok(Some(0x2007)), // fixed-width space
                // Inline/extended controls occupy 8 code units; the first unit
                // is dropped together with its 7-unit payload.
                0x0001..=0x0008 | 0x000B..=0x000C | 0x000E..=0x0017 => {
                    self.skip_payload()?;
                    continue;
                }
                // Tab is stored as an 8-unit inline control; the tab itself is
                // real text and is kept.
                0x0009 => {
                    self.skip_payload()?;
                    return Ok(Some(0x0009));
                }
                other => return Ok(Some(other)),
            }
        }
    }
}

/// Decodes one `PARA_TEXT` payload into at most `preview_chars` characters and
/// the full Unicode scalar count. Surrogate pairs are combined; lone surrogates
/// become U+FFFD, the same policy as `String::from_utf16_lossy`.
pub(crate) fn decode_para_text(data: &[u8], preview_chars: usize) -> Result<(String, u64)> {
    if !data.len().is_multiple_of(2) {
        return Err(reject(
            R::MalformedRecordStream,
            format!(
                "PARA_TEXT payload is {} bytes, not a multiple of 2",
                data.len()
            ),
        ));
    }
    let mut units = ParaUnits { data, pos: 0 };
    let mut preview = String::new();
    let mut preview_len = 0usize;
    let mut char_count = 0u64;
    let mut pending_high: Option<u16> = None;
    loop {
        let Some(unit) = units.next_unit()? else {
            break;
        };
        let ch = match pending_high.take() {
            Some(high) => {
                if (0xDC00..=0xDFFF).contains(&unit) {
                    combine_surrogates(high, unit)
                } else {
                    // The high surrogate stands alone: emit one replacement and
                    // reprocess the current unit on the next iteration.
                    pending_high = Some(unit);
                    '\u{FFFD}'
                }
            }
            None => {
                if (0xD800..=0xDBFF).contains(&unit) {
                    pending_high = Some(unit);
                    continue;
                }
                if (0xDC00..=0xDFFF).contains(&unit) {
                    '\u{FFFD}'
                } else {
                    char::from_u32(unit as u32).unwrap_or('\u{FFFD}')
                }
            }
        };
        char_count += 1;
        if preview_len < preview_chars {
            preview.push(ch);
            preview_len += 1;
        }
    }
    if pending_high.is_some() {
        char_count += 1;
        if preview_len < preview_chars {
            preview.push('\u{FFFD}');
        }
    }
    Ok((preview, char_count))
}

fn combine_surrogates(high: u16, low: u16) -> char {
    char::from_u32(0x10000 + ((high as u32 - 0xD800) << 10) + (low as u32 - 0xDC00))
        .unwrap_or('\u{FFFD}')
}
