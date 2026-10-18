//! Unit tests for the OWPML dictionary and the `document.validate` report.

use super::*;
use crate::owpml::Dictionary;
use crate::validate::*;
use hae_package::{ManifestItem, PackageInfo, NS_HH};
use sha2::{Digest, Sha256};

fn corpus(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/../../tests/fixtures/corpus/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn open(name: &str) -> Document {
    Document::open(corpus(name), &Limits::default()).unwrap()
}

fn package_info(header: Option<&str>, sections: &[&str]) -> PackageInfo {
    PackageInfo {
        rootfile: "Contents/content.hpf".into(),
        manifest: Vec::<ManifestItem>::new(),
        sections: sections.iter().map(|s| s.to_string()).collect(),
        header: header.map(str::to_string),
        prv_text: None,
        prv_image: None,
        signature_entries: Vec::new(),
    }
}

#[test]
fn dictionary_fingerprint_covers_every_element_name() {
    let dict = Dictionary::get();
    // The generator writes `ns:name\n` for every element in sorted order and
    // records the SHA-256 in the file; recomputing it here proves the committed
    // data file is the one the extractor produced.
    let mut hasher = Sha256::new();
    let mut count = 0;
    for (prefix, name) in dict.element_names() {
        hasher.update(format!("{prefix}:{name}\n").as_bytes());
        count += 1;
    }
    assert!(count > 300, "dictionary carries {} names", count);
    assert_eq!(
        format!("{:x}", hasher.finalize()),
        dict.fingerprint(),
        "the bundled dictionary must be self-consistent"
    );
}

#[test]
fn dictionary_knows_part_roots_resources_and_merged_names() {
    let dict = Dictionary::get();
    for (prefix, name) in [
        ("hh", "head"),
        ("hs", "sec"),
        ("hp", "p"),
        ("hp", "tbl"),
        ("hp", "tc"),
        ("hp", "switch"),
        ("hp", "case"),
        ("hh", "charPr"),
        ("hh", "paraPr"),
        ("hh", "style"),
        ("hc", "img"),
        ("hv", "HCFVersion"),
        ("opf", "package"),
        ("ocf", "container"),
    ] {
        assert!(
            dict.element(prefix, name).is_some(),
            "{prefix}:{name} must be in the dictionary"
        );
    }
    let head = dict.element("hh", "head").unwrap();
    assert!(head.attributes.contains("secCnt"));
    assert!(head.root);
    // `hp:offset` is registered twice (character shape and page border), so its
    // attribute set is the union — the dictionary is a name dictionary.
    let offset = dict.element("hp", "offset").unwrap();
    assert!(offset.attributes.contains("x"));
    assert!(offset.attributes.contains("left"));
    assert_eq!(offset.classes.len(), 2);
    assert!(dict.element("hh", "margins").is_none());
}

#[test]
fn dictionary_maps_namespace_uris_including_the_opf_alias() {
    let dict = Dictionary::get();
    assert_eq!(dict.prefix_for(NS_HH), Some("hh"));
    assert_eq!(
        dict.prefix_for("http://www.hancom.co.kr/hwpml/2011/paragraph"),
        Some("hp")
    );
    assert_eq!(
        dict.prefix_for("http://www.idpf.org/2007/opf/"),
        Some("opf")
    );
    assert_eq!(dict.prefix_for("http://www.idpf.org/2007/opf"), Some("opf"));
    assert_eq!(
        dict.prefix_for("http://www.hancom.co.kr/hwpml/2016/meta/pkg#"),
        None
    );
    assert!(dict.covered("hp"));
    // Declared by the model but filled with no element names.
    assert!(!dict.covered("hp10"));
    assert!(dict.namespace_count() >= 15);
}

#[test]
fn missing_package_items_names_each_absent_member() {
    let info = package_info(Some("Contents/header.xml"), &["Contents/section0.xml"]);
    let complete = vec![
        "mimetype".to_string(),
        "META-INF/container.xml".to_string(),
        "Contents/content.hpf".to_string(),
        "Contents/header.xml".to_string(),
        "Contents/section0.xml".to_string(),
    ];
    assert!(missing_package_items(&complete, &info).is_empty());

    let empty: Vec<String> = Vec::new();
    let reasons: Vec<&str> = missing_package_items(&empty, &info)
        .into_iter()
        .map(|(_, reason)| reason)
        .collect();
    assert_eq!(
        reasons,
        vec![
            "mimetype_absent",
            "container_absent",
            "rootfile_absent",
            "section_entry_absent",
            "header_entry_absent"
        ]
    );

    let mut order = complete.clone();
    order.swap(0, 1);
    let misordered: Vec<&str> = missing_package_items(&order, &info)
        .into_iter()
        .map(|(_, reason)| reason)
        .collect();
    assert_eq!(misordered, vec!["mimetype_not_first"]);

    let headerless = package_info(None, &[]);
    let reasons: Vec<&str> = missing_package_items(&complete, &headerless)
        .into_iter()
        .map(|(_, reason)| reason)
        .collect();
    assert_eq!(reasons, vec!["section_absent", "header_absent"]);
}

#[test]
fn validate_rejects_out_of_range_example_counts() {
    let d = open("irb-sample.hwpx");
    let err = d.validate(None, 0).unwrap_err();
    assert_eq!(err.code(), hae_diag::Code::InvalidRequest);
    let err = d.validate(None, MAX_EXAMPLES + 1).unwrap_err();
    assert_eq!(err.code(), hae_diag::Code::InvalidRequest);
    let err = d.validate(Some("n-missing"), 5).unwrap_err();
    assert_eq!(err.code(), hae_diag::Code::TargetNotFound);
}

#[test]
fn validate_is_stable_read_only_and_reports_checked_coverage() {
    let d = open("gyehoek-reference.hwpx");
    let nodes_before = d.nodes.len();
    let first = d.validate(None, DEFAULT_EXAMPLES).unwrap();
    let second = d.validate(None, DEFAULT_EXAMPLES).unwrap();
    assert_eq!(
        d.nodes.len(),
        nodes_before,
        "no node may be created/removed"
    );
    assert!(
        d.stale_layout.is_empty(),
        "no layout cache may be invalidated"
    );
    assert_eq!(
        serde_json::to_value(&first).unwrap(),
        serde_json::to_value(&second).unwrap()
    );
    assert_eq!(first.summary.errors, 0);
    assert!(first.checked.elements > 1000);
    assert!(first.checked.references > 100);
    assert!(first
        .checked
        .namespaces
        .iter()
        .any(|n| n.uri == "http://www.hancom.co.kr/hwpml/2011/paragraph"));
    assert_eq!(first.checked.header.as_deref(), Some("Contents/header.xml"));
    assert!(first.checked.reference_integrity);
}

#[test]
fn findings_examples_carry_node_ids_for_body_elements() {
    let d = open("anchor_char_height.hwpx");
    let report = d.validate(None, DEFAULT_EXAMPLES).unwrap();
    let dangling = report
        .items
        .iter()
        .find(|f| f.code == CODE_UNRESOLVED_REFERENCE)
        .expect("anchor_char_height has a dangling styleIDRef");
    assert_eq!(dangling.count, 20);
    assert!(!dangling.examples.is_empty());
    for example in &dangling.examples {
        assert_eq!(example.entry, "Contents/section0.xml");
        assert!(
            example.node_id.is_some(),
            "a body example must point at a node: {example:?}"
        );
    }
}

#[test]
fn scan_limit_refuses_instead_of_returning_partial_results() {
    let limits = Limits {
        max_validate_elements: 5,
        ..Limits::default()
    };
    let d = Document::open(corpus("irb-sample.hwpx"), &limits).unwrap();
    let err = d.validate(None, DEFAULT_EXAMPLES).unwrap_err();
    assert_eq!(err.code(), hae_diag::Code::ResourceLimit);
    let detail = err.0.detail.as_ref().unwrap();
    assert_eq!(detail["limit"], "max_validate_elements");
    assert_eq!(detail["limitElements"], 5);
}
