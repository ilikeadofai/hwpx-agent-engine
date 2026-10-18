//! `hwp5.inspect` orchestration: container → FileHeader → section records → pages.

use crate::cfb::CompoundFile;
use crate::header::FileHeader;
use crate::records::{decode_para_text, RecordIter, TAG_PARA_HEADER, TAG_PARA_TEXT};
use crate::reject;
use flate2::read::DeflateDecoder;
use hae_diag::{Budget, Code, EngineError, Limits, PackageRejectReason as R, Result, Stage};
use std::io::Read;

/// Paragraph window and preview size for one inspection call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InspectOptions {
    /// Global (section order, then record order) index of the first paragraph to return.
    pub offset: u64,
    /// Maximum number of paragraphs to return (the whole file is always walked).
    pub limit: usize,
    /// Maximum characters kept in each returned paragraph preview (>= 1).
    pub preview_chars: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionSummary {
    /// The N of `BodyText/SectionN` as found in the container.
    pub index: usize,
    pub paragraph_count: u64,
    pub char_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParagraphSummary {
    pub section_index: usize,
    /// 0-based global paragraph index across sections, in record order.
    pub paragraph_index: u64,
    /// Level field of the `PARA_HEADER` that started this paragraph.
    pub level: u16,
    pub char_count: u64,
    pub preview: String,
    pub preview_truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectReport {
    pub file_header: FileHeader,
    pub sections: Vec<SectionSummary>,
    pub paragraph_total: u64,
    pub char_total: u64,
    pub paragraphs: Vec<ParagraphSummary>,
    /// Global paragraph offset of the next page, or `None` on the last page.
    pub next_offset: Option<u64>,
}

/// Reads the HWP5 document in `bytes` far enough to report the `FileHeader`
/// flags and paragraph text. Borrows the input; never writes.
pub fn inspect(bytes: &[u8], limits: &Limits, options: &InspectOptions) -> Result<InspectReport> {
    debug_assert!(options.preview_chars >= 1);
    let cfb = CompoundFile::open(bytes, limits)?;

    let header_index = cfb
        .find_entry("FileHeader")
        .ok_or_else(|| reject(R::MissingComponent, "FileHeader stream is missing"))?;
    let file_header = FileHeader::parse(&cfb.read_stream(header_index)?)?;
    file_header.check_supported()?;

    let section_entries = cfb.body_text_sections()?;
    if section_entries.is_empty() {
        return Err(reject(
            R::MissingComponent,
            "no BodyText/SectionN streams in the container",
        ));
    }

    let window_end = options.offset.saturating_add(options.limit as u64);
    let mut budget = Budget::new(limits.max_inflate_total);
    let mut sections = Vec::with_capacity(section_entries.len());
    let mut paragraphs = Vec::new();
    let mut paragraph_total = 0u64;
    let mut char_total = 0u64;

    for (section_index, entry_index) in section_entries {
        let raw = cfb.read_stream(entry_index)?;
        let stream = if file_header.is_compressed() {
            inflate_stream(&raw, &mut budget)?
        } else {
            raw
        };
        if stream.is_empty() {
            return Err(reject(
                R::MalformedRecordStream,
                format!("BodyText/Section{section_index} has no records"),
            ));
        }

        let mut section_paragraphs = 0u64;
        let mut section_chars = 0u64;
        let mut current: Option<Accumulator> = None;
        for record in RecordIter::new(&stream) {
            let (header, payload) = record?;
            match header.tag {
                TAG_PARA_HEADER => {
                    if let Some(acc) = current.take() {
                        flush(
                            acc,
                            section_index,
                            &mut paragraph_total,
                            &mut char_total,
                            &mut section_paragraphs,
                            &mut section_chars,
                            &mut paragraphs,
                            options,
                            window_end,
                            limits,
                        )?;
                    }
                    current = Some(Accumulator {
                        level: header.level,
                        chars: 0,
                        preview: String::new(),
                        preview_len: 0,
                    });
                }
                TAG_PARA_TEXT => {
                    let acc = current.get_or_insert(Accumulator {
                        level: header.level,
                        chars: 0,
                        preview: String::new(),
                        preview_len: 0,
                    });
                    let remaining = options.preview_chars.saturating_sub(acc.preview_len);
                    let (text, count) = decode_para_text(payload, remaining)?;
                    acc.chars += count;
                    acc.preview.push_str(&text);
                    acc.preview_len += text.chars().count();
                }
                _ => {}
            }
        }
        if let Some(acc) = current.take() {
            flush(
                acc,
                section_index,
                &mut paragraph_total,
                &mut char_total,
                &mut section_paragraphs,
                &mut section_chars,
                &mut paragraphs,
                options,
                window_end,
                limits,
            )?;
        }
        sections.push(SectionSummary {
            index: section_index,
            paragraph_count: section_paragraphs,
            char_count: section_chars,
        });
    }

    let next_offset = if paragraph_total > window_end {
        Some(window_end)
    } else {
        None
    };
    Ok(InspectReport {
        file_header,
        sections,
        paragraph_total,
        char_total,
        paragraphs,
        next_offset,
    })
}

struct Accumulator {
    level: u16,
    chars: u64,
    preview: String,
    preview_len: usize,
}

#[allow(clippy::too_many_arguments)]
fn flush(
    acc: Accumulator,
    section_index: usize,
    paragraph_total: &mut u64,
    char_total: &mut u64,
    section_paragraphs: &mut u64,
    section_chars: &mut u64,
    out: &mut Vec<ParagraphSummary>,
    options: &InspectOptions,
    window_end: u64,
    limits: &Limits,
) -> Result<()> {
    let index = *paragraph_total;
    if index >= limits.max_hwp5_paragraphs {
        return Err(EngineError::new(
            Code::ResourceLimit,
            Stage::Query,
            format!(
                "HWP5 document has more than {} paragraphs",
                limits.max_hwp5_paragraphs
            ),
        ));
    }
    *paragraph_total += 1;
    *char_total += acc.chars;
    *section_paragraphs += 1;
    *section_chars += acc.chars;
    if index >= options.offset && index < window_end {
        out.push(ParagraphSummary {
            section_index,
            paragraph_index: index,
            level: acc.level,
            char_count: acc.chars,
            preview: acc.preview,
            preview_truncated: acc.chars > options.preview_chars as u64,
        });
    }
    Ok(())
}

/// Raw-DEFLATE (no zlib header) decompression into a cumulative byte budget.
fn inflate_stream(raw: &[u8], budget: &mut Budget) -> Result<Vec<u8>> {
    let mut decoder = DeflateDecoder::new(raw);
    let mut out = Vec::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = decoder.read(&mut buffer).map_err(|e| {
            reject(
                R::MalformedRecordStream,
                format!("compressed HWP5 stream does not inflate: {e}"),
            )
        })?;
        if read == 0 {
            break;
        }
        budget.try_reserve(read as u64, "HWP5 stream inflate")?;
        out.extend_from_slice(&buffer[..read]);
    }
    Ok(out)
}
