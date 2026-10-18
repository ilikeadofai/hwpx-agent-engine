//! hae-hwp5 integration tests against synthetic MS-CFB containers.
//!
//! Every container here is built by [`hae_hwp5::fixture`]; nothing is a real
//! document (real-file verification is reported in the decision log).

use hae_diag::{Code, EngineError, Limits, PackageRejectReason};
use hae_hwp5::fixture::{self, ContainerSpec};
use hae_hwp5::{inspect, InspectOptions, InspectReport};

fn options(preview_chars: usize) -> InspectOptions {
    InspectOptions {
        offset: 0,
        limit: 100,
        preview_chars,
    }
}

fn header(properties: u32) -> Vec<u8> {
    fixture::file_header((5, 1, 0, 1), properties)
}

fn para(text: &str) -> Vec<u8> {
    let mut payload = fixture::utf16(text);
    payload.extend(fixture::utf16("\r")); // paragraph break
    let mut out = fixture::record(0x42, 0, &fixture::para_header_data());
    out.extend(fixture::record(0x43, 1, &payload));
    out
}

fn document(sections: Vec<Vec<u8>>, properties: u32) -> Vec<u8> {
    fixture::build(&ContainerSpec {
        file_header: header(properties),
        sections,
        extra_streams: vec![("DocInfo".into(), vec![0u8; 64])],
    })
}

fn reason(err: &EngineError) -> &str {
    err.reason().unwrap_or("<none>")
}

#[test]
fn compound_container_serves_mini_and_regular_streams() {
    // Section0 stays under the 4096-byte cutoff (mini stream); the long
    // paragraph of Section1 forces a regular FAT chain.
    let long_text = "가".repeat(3000);
    let doc = document(
        vec![[para("짧은 문단")].concat(), [para(&long_text)].concat()],
        0,
    );
    let report = inspect(&doc, &Limits::default(), &options(10)).unwrap();
    assert_eq!(report.sections.len(), 2);
    assert_eq!(report.paragraph_total, 2);
    assert_eq!(report.sections[0].paragraph_count, 1);
    assert_eq!(report.sections[1].paragraph_count, 1);
    assert_eq!(report.paragraphs[0].preview, "짧은 문단");
    assert_eq!(report.paragraphs[1].char_count, 3000);
    assert!(report.paragraphs[1].preview_truncated);
    assert_eq!(report.paragraphs[1].section_index, 1);
}

#[test]
fn file_header_reports_version_compression_and_flags() {
    let section = [para("압축된 본문")].concat();
    let doc = fixture::build(&ContainerSpec {
        file_header: fixture::file_header((5, 0, 2, 2), 1 << 0),
        sections: vec![fixture::deflate(&section)],
        extra_streams: vec![],
    });
    let report = inspect(&doc, &Limits::default(), &options(120)).unwrap();
    assert_eq!(report.file_header.version.to_string(), "5.0.2.2");
    assert!(report.file_header.is_compressed());
    assert!(!report.file_header.is_encrypted());
    assert!(!report.file_header.is_distribution());
    assert!(!report.file_header.is_drm());
    assert_eq!(report.paragraphs[0].preview, "압축된 본문");
}

#[test]
fn encrypted_document_is_refused_with_encrypted_reason() {
    let doc = document(vec![[para("비밀")].concat()], 1 << 1);
    let err = inspect(&doc, &Limits::default(), &options(120)).unwrap_err();
    assert_eq!(err.code(), Code::PackageRejected);
    assert_eq!(reason(&err), PackageRejectReason::Encrypted.as_str());
}

#[test]
fn distribution_document_is_refused_with_its_own_reason() {
    let doc = document(vec![[para("배포용")].concat()], 1 << 2);
    let err = inspect(&doc, &Limits::default(), &options(120)).unwrap_err();
    assert_eq!(err.code(), Code::PackageRejected);
    assert_eq!(
        reason(&err),
        PackageRejectReason::DistributionUnsupported.as_str()
    );
}

#[test]
fn drm_document_is_refused_as_encrypted() {
    let doc = document(vec![[para("DRM")].concat()], 1 << 4);
    let err = inspect(&doc, &Limits::default(), &options(120)).unwrap_err();
    assert_eq!(err.code(), Code::PackageRejected);
    assert_eq!(reason(&err), PackageRejectReason::Encrypted.as_str());
}

#[test]
fn truncated_file_is_refused() {
    let doc = document(vec![[para("잘림")].concat()], 0);
    // Smaller than a CFB header.
    let err = inspect(&doc[..100], &Limits::default(), &options(120)).unwrap_err();
    assert_eq!(err.code(), Code::PackageRejected);
    assert_eq!(
        reason(&err),
        PackageRejectReason::MalformedCompoundFile.as_str()
    );
    // Header survives, but the referenced sectors are gone.
    let err = inspect(&doc[..600], &Limits::default(), &options(120)).unwrap_err();
    assert_eq!(
        reason(&err),
        PackageRejectReason::MalformedCompoundFile.as_str()
    );
}

#[test]
fn damaged_fat_is_refused() {
    let mut doc = document(vec![[para("FAT")].concat()], 0);
    let fat_sector = u32::from_le_bytes([doc[0x4C], doc[0x4D], doc[0x4E], doc[0x4F]]) as usize;
    let start = (fat_sector + 1) * 512;
    doc[start..start + 512].fill(0xFF); // every chain entry becomes FREE
    let err = inspect(&doc, &Limits::default(), &options(120)).unwrap_err();
    assert_eq!(
        reason(&err),
        PackageRejectReason::MalformedCompoundFile.as_str()
    );
}

#[test]
fn compressed_stream_is_inflated_and_trailing_trailer_is_ignored() {
    let section = [para("deflate 본문")].concat();
    let mut stored = fixture::deflate(&section);
    // hwplib's writer appends 4 zero bytes and the original length.
    stored.extend_from_slice(&0u32.to_le_bytes());
    stored.extend_from_slice(&(section.len() as u32).to_le_bytes());
    let doc = document(vec![stored], 1 << 0);
    let report = inspect(&doc, &Limits::default(), &options(120)).unwrap();
    assert_eq!(report.paragraphs[0].preview, "deflate 본문");
}

#[test]
fn compressed_flag_with_raw_stream_is_refused() {
    let doc = document(vec![[para("원시")].concat()], 1 << 0);
    let err = inspect(&doc, &Limits::default(), &options(120)).unwrap_err();
    assert_eq!(err.code(), Code::PackageRejected);
    assert_eq!(
        reason(&err),
        PackageRejectReason::MalformedRecordStream.as_str()
    );
}

#[test]
fn extended_size_record_is_read() {
    let text = "나".repeat(2500);
    let mut payload = fixture::utf16(&text);
    payload.extend(fixture::utf16("\r"));
    let mut section = fixture::record(0x42, 0, &fixture::para_header_data());
    section.extend(fixture::record_extended(0x43, 1, &payload));
    let doc = document(vec![section], 0);
    let report = inspect(&doc, &Limits::default(), &options(120)).unwrap();
    assert_eq!(report.paragraphs[0].char_count, 2500);
    assert_eq!(
        report.paragraphs[0].preview,
        text.chars().take(120).collect::<String>()
    );
    assert!(report.paragraphs[0].preview_truncated);
}

#[test]
fn paragraph_text_maps_controls_and_surrogates() {
    let mut payload = fixture::utf16("가나");
    payload.extend([0x09, 0x00]); // tab (8-unit inline control)
    payload.extend([0u8; 14]); // tab payload
    payload.extend([0x0B, 0x00]); // GSO extended control
    payload.extend([0u8; 14]);
    payload.extend([0x18, 0x00]); // soft hyphen
    payload.extend([0x1E, 0x00]); // non-breaking space
    payload.extend([0x1F, 0x00]); // fixed-width space
    payload.extend([0x3D, 0xD8, 0x00, 0xDE]); // U+1F600 surrogate pair
    payload.extend([0x0D, 0x00]); // paragraph break
    let mut section = fixture::record(0x42, 0, &fixture::para_header_data());
    section.extend(fixture::record(0x43, 1, &payload));
    let doc = document(vec![section], 0);
    let report = inspect(&doc, &Limits::default(), &options(120)).unwrap();
    assert_eq!(
        report.paragraphs[0].preview,
        "가나\t\u{00AD}\u{00A0}\u{2007}\u{1F600}"
    );
    assert_eq!(report.paragraphs[0].char_count, 7);
}

#[test]
fn control_truncated_at_record_end_is_refused() {
    let mut section = fixture::record(0x42, 0, &fixture::para_header_data());
    section.extend(fixture::record(0x43, 1, &[0x09, 0x00]));
    let doc = document(vec![section], 0);
    let err = inspect(&doc, &Limits::default(), &options(120)).unwrap_err();
    assert_eq!(
        reason(&err),
        PackageRejectReason::MalformedRecordStream.as_str()
    );
}

#[test]
fn malformed_record_size_is_refused() {
    let mut section = fixture::record(0x42, 0, &fixture::para_header_data());
    let header = 0x43u32 | (1 << 10) | (100 << 20);
    section.extend_from_slice(&header.to_le_bytes());
    section.extend_from_slice(b"ab");
    let doc = document(vec![section], 0);
    let err = inspect(&doc, &Limits::default(), &options(120)).unwrap_err();
    assert_eq!(
        reason(&err),
        PackageRejectReason::MalformedRecordStream.as_str()
    );
}

#[test]
fn odd_length_para_text_is_refused() {
    let mut section = fixture::record(0x42, 0, &fixture::para_header_data());
    section.extend(fixture::record(0x43, 1, &[0x41, 0x00, 0x42]));
    let doc = document(vec![section], 0);
    let err = inspect(&doc, &Limits::default(), &options(120)).unwrap_err();
    assert_eq!(
        reason(&err),
        PackageRejectReason::MalformedRecordStream.as_str()
    );
}

#[test]
fn paragraphs_page_with_offset_and_limit() {
    let section: Vec<u8> = (0..10)
        .map(|i| para(&format!("p{i}")))
        .collect::<Vec<_>>()
        .concat();
    let doc = document(vec![section], 0);
    let report = inspect(
        &doc,
        &Limits::default(),
        &InspectOptions {
            offset: 0,
            limit: 4,
            preview_chars: 50,
        },
    )
    .unwrap();
    assert_eq!(report.paragraph_total, 10);
    assert_eq!(report.paragraphs.len(), 4);
    assert_eq!(report.paragraphs[0].paragraph_index, 0);
    assert_eq!(report.paragraphs[3].preview, "p3");
    assert_eq!(report.next_offset, Some(4));

    let report = inspect(
        &doc,
        &Limits::default(),
        &InspectOptions {
            offset: 8,
            limit: 4,
            preview_chars: 50,
        },
    )
    .unwrap();
    assert_eq!(report.paragraphs.len(), 2);
    assert_eq!(report.paragraphs[0].paragraph_index, 8);
    assert_eq!(report.next_offset, None);

    // An offset past the end is an empty page, not an error.
    let report = inspect(
        &doc,
        &Limits::default(),
        &InspectOptions {
            offset: 99,
            limit: 4,
            preview_chars: 50,
        },
    )
    .unwrap();
    assert!(report.paragraphs.is_empty());
    assert_eq!(report.next_offset, None);
    assert_eq!(report.paragraph_total, 10);
}

#[test]
fn char_and_preview_counts_are_independent() {
    let text = "다".repeat(50);
    let doc = document(vec![[para(&text)].concat()], 0);
    let report = inspect(&doc, &Limits::default(), &options(10)).unwrap();
    assert_eq!(report.paragraphs[0].preview, "다".repeat(10));
    assert_eq!(report.paragraphs[0].char_count, 50);
    assert!(report.paragraphs[0].preview_truncated);
    assert_eq!(report.char_total, 50);
}

#[test]
fn paragraph_ceiling_is_enforced() {
    let section: Vec<u8> = (0..3)
        .map(|i| para(&format!("p{i}")))
        .collect::<Vec<_>>()
        .concat();
    let doc = document(vec![section], 0);
    let limits = Limits {
        max_hwp5_paragraphs: 2,
        ..Limits::default()
    };
    let err = inspect(&doc, &limits, &options(120)).unwrap_err();
    assert_eq!(err.code(), Code::ResourceLimit);
}

#[test]
fn inflate_budget_is_enforced() {
    let section = [para("예산 초과")].concat();
    let doc = document(vec![fixture::deflate(&section)], 1 << 0);
    let limits = Limits {
        max_inflate_total: 8,
        ..Limits::default()
    };
    let err = inspect(&doc, &limits, &options(120)).unwrap_err();
    assert_eq!(err.code(), Code::ResourceLimit);
}

#[test]
fn missing_body_text_is_refused() {
    let doc = document(vec![], 0);
    let err = inspect(&doc, &Limits::default(), &options(120)).unwrap_err();
    assert_eq!(err.code(), Code::PackageRejected);
    assert_eq!(reason(&err), PackageRejectReason::MissingComponent.as_str());
}

#[test]
fn undersized_file_header_is_refused() {
    let doc = fixture::build(&ContainerSpec {
        file_header: vec![0u8; 100],
        sections: vec![[para("x")].concat()],
        extra_streams: vec![],
    });
    let err = inspect(&doc, &Limits::default(), &options(120)).unwrap_err();
    assert_eq!(
        reason(&err),
        PackageRejectReason::MalformedRecordStream.as_str()
    );
}

#[test]
fn wrong_signature_file_header_is_refused() {
    let mut file_header = header(0);
    file_header[..17].copy_from_slice(b"NotAnHwpDocFile!!");
    let doc = fixture::build(&ContainerSpec {
        file_header,
        sections: vec![[para("x")].concat()],
        extra_streams: vec![],
    });
    let err = inspect(&doc, &Limits::default(), &options(120)).unwrap_err();
    assert_eq!(
        reason(&err),
        PackageRejectReason::MalformedRecordStream.as_str()
    );
}

/// The report must not depend on how many pages were requested before it: the
/// totals are the same on every page.
#[test]
fn totals_are_stable_across_pages() {
    let section: Vec<u8> = (0..5)
        .map(|i| para(&format!("s{i}")))
        .collect::<Vec<_>>()
        .concat();
    let doc = document(vec![section], 0);
    let full = inspect(&doc, &Limits::default(), &options(120)).unwrap();
    let page = inspect(
        &doc,
        &Limits::default(),
        &InspectOptions {
            offset: 2,
            limit: 2,
            preview_chars: 120,
        },
    )
    .unwrap();
    assert_eq!(
        report_shape_ignoring_page(full),
        report_shape_ignoring_page(page)
    );
}

fn report_shape_ignoring_page(report: InspectReport) -> (u64, u64, usize) {
    (
        report.paragraph_total,
        report.char_total,
        report.sections.len(),
    )
}
