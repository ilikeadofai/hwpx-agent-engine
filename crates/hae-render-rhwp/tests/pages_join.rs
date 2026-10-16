//! Task 3-3: page-information join against the 28-section corpus document.

mod common;

use common::*;
use hae_diag::Limits;
use hae_model::{Document, Kind};
use hae_render_rhwp::{PageStatus, RhwpAdapter};
use std::fs;

fn open(name: &str) -> Document {
    Document::open(fs::read(corpus(name)).unwrap(), &Limits::default()).unwrap()
}

#[test]
fn corpus_28_sections_matches_the_spike_counts() {
    let env = real_env();
    let doc = open("research-school-plan-reference.hwpx");
    let t = std::time::Instant::now();
    let info = env.adapter.page_info(&doc, None).expect("page info");
    eprintln!("PAGE_INFO_TIME {} ms", t.elapsed().as_millis());
    assert_eq!(info.source, "renderer-estimated");
    assert_eq!(info.page_count, 28);
    assert_eq!(info.sections.len(), 28);
    assert!(
        info.sections.iter().all(|s| s.available),
        "{:?}",
        info.sections
    );
    // map_paths.py on the same file: 8 tables, 308 cells, 637 top-level
    // paragraphs, 4 resolvable pictures (14 more sit inside groups).
    assert_eq!(info.counts.tables, 8, "{:?}", info.counts);
    assert_eq!(info.counts.cells, 308, "{:?}", info.counts);
    assert_eq!(info.counts.paragraphs, 637, "{:?}", info.counts);
    assert_eq!(info.counts.pictures, 4, "{:?}", info.counts);

    // Every resolved node reports 1-based pages within range.
    for (id, st) in &info.nodes {
        if let PageStatus::Pages { pages, source } = st {
            assert_eq!(*source, "renderer-estimated");
            assert!(!pages.is_empty(), "{id}");
            assert!(
                pages.iter().all(|&p| (1..=28).contains(&p)),
                "{id}: {pages:?}"
            );
            assert!(pages.windows(2).all(|w| w[0] < w[1]));
        }
    }
    // Pictures in groups are unavailable with a reason, never silently paged.
    let unavailable_pics: Vec<_> = doc
        .nodes
        .iter()
        .filter(|n| n.kind == Kind::Picture)
        .filter_map(|n| match info.status(&n.id) {
            Some(PageStatus::Unavailable { unavailable }) => Some(unavailable.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(unavailable_pics.len(), 14, "{unavailable_pics:?}");
    assert!(
        unavailable_pics.iter().all(|r| r.starts_with("in-")),
        "{unavailable_pics:?}"
    );
    // The first top-level paragraph of the document is on page 1.
    let first_p = doc
        .nodes
        .iter()
        .find(|n| n.kind == Kind::Paragraph)
        .unwrap();
    assert_eq!(
        info.status(&first_p.id).and_then(|s| s.page_list()),
        Some(&[1u32][..])
    );
}

#[test]
fn small_documents_resolve_too() {
    let env = real_env();
    for name in ["gyehoek-reference.hwpx", "ref_empty.hwpx", "gani-gian.hwpx"] {
        let doc = open(name);
        let info = env
            .adapter
            .page_info(&doc, None)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let top_level_tables = info.counts.tables;
        eprintln!(
            "{name}: pages={} counts={:?} sections_ok={}",
            info.page_count,
            info.counts,
            info.sections.iter().all(|s| s.available)
        );
        assert!(info.page_count >= 1, "{name}");
        let _ = top_level_tables;
    }
}

#[test]
fn page_info_does_not_touch_the_document() {
    let env = real_env();
    let doc = open("research-school-plan-reference.hwpx");
    let before = doc.to_hwpx().unwrap();
    let _ = env.adapter.page_info(&doc, None).unwrap();
    assert_eq!(doc.to_hwpx().unwrap(), before);
}

#[test]
fn unavailable_adapter_gives_explicit_error() {
    let fonts = font_dir();
    let mut cfg = hae_render_rhwp::AdapterConfig::new(rhwp_path(), fonts.path());
    cfg.bwrap_path = "/nonexistent/bwrap".into();
    let adapter = RhwpAdapter::new(cfg);
    let doc = open("ref_empty.hwpx");
    let e = adapter.page_info(&doc, None).unwrap_err();
    assert_eq!(e.reason(), Some("sandbox_unverified"));
}
