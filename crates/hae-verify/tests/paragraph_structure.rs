//! Hand-built candidates for paragraph.insert / paragraph.delete (0.4).
//! This test crate has no editor/model dependency: the expected bytes are
//! written out from the contract in
//! docs/decisions/document-paragraph-creation-design.md §5 and compared with
//! the verifier's own derivation.
use hae_diag::Limits;
use hae_package::{PackageState, NS_HP};
use hae_verify::{verify_archive, VerifyOperation};
use hae_xml::{escape_text, XmlDoc};
use hae_zip::Archive;
use std::{ops::Range, sync::Arc};

const SOURCE: &[u8] = include_bytes!("../../../tests/fixtures/paragraph/paragraphs.hwpx");
const SECTION: &str = "Contents/section0.xml";
type Cut = (Range<usize>, Vec<u8>);

fn is(d: &XmlDoc, i: usize, n: &str) -> bool {
    d.el(i).is(NS_HP, n)
}

fn parse(b: &[u8]) -> XmlDoc {
    XmlDoc::parse(Arc::<[u8]>::from(b)).unwrap()
}

/// Source plus an editable list of splices; `finish` writes the candidate.
struct Build {
    archive: Archive,
    state: PackageState,
    d: XmlDoc,
    cuts: Vec<Cut>,
}

impl Build {
    fn new() -> Self {
        let archive = Archive::open(SOURCE.to_vec(), &Limits::default()).unwrap();
        let state = PackageState::from_archive(&archive);
        let d = parse(state.get(SECTION).unwrap());
        Self {
            archive,
            state,
            d,
            cuts: vec![],
        }
    }
    fn top(&self, i: usize) -> usize {
        let root = self.d.root();
        self.d
            .el(root)
            .children
            .iter()
            .copied()
            .filter(|&c| is(&self.d, c, "p"))
            .nth(i)
            .unwrap()
    }
    /// Innermost paragraph containing `text` (outer carriers also contain a
    /// cell paragraph's text, so the deepest match wins).
    fn by_text(&self, text: &str) -> usize {
        let needle = text.as_bytes();
        let hits: Vec<usize> = self
            .d
            .iter()
            .filter(|(i, e)| {
                is(&self.d, *i, "p")
                    && self
                        .d
                        .raw(e.outer())
                        .windows(needle.len())
                        .any(|w| w == needle)
            })
            .map(|(i, _)| i)
            .collect();
        let deepest = hits.iter().map(|&i| self.d.el(i).depth).max();
        let mut at_depth = hits
            .into_iter()
            .filter(|&i| Some(self.d.el(i).depth) == deepest);
        let found = at_depth
            .next()
            .unwrap_or_else(|| panic!("no paragraph with {text:?}"));
        assert!(at_depth.next().is_none(), "text {text:?} is not unique");
        found
    }
    fn insert(&mut self, at: usize, bytes: impl AsRef<[u8]>) {
        self.cuts.push((at..at, bytes.as_ref().to_vec()));
    }
    fn cut(&mut self, r: Range<usize>) {
        self.cuts.push((r, vec![]));
    }
    /// True when an existing empty replacement already removes `r`.
    fn covered(&self, r: &Range<usize>) -> bool {
        self.cuts
            .iter()
            .any(|(c, v)| v.is_empty() && c.start <= r.start && c.end >= r.end)
    }
    fn caches_of(&mut self, p: usize) {
        for c in self.d.el(p).children.clone() {
            if is(&self.d, c, "linesegarray") && !self.covered(&self.d.el(c).outer()) {
                self.cut(self.d.el(c).outer());
            }
        }
    }
    /// Design §5.4: every top-level `linesegarray` from the element ordinal of
    /// the anchor to the section end is removed.
    fn caches_from_top(&mut self, ordinal: usize) {
        let root = self.d.root();
        for (ord, &c) in self.d.el(root).children.clone().iter().enumerate() {
            if ord >= ordinal && is(&self.d, c, "p") {
                self.caches_of(c);
            }
        }
    }
    /// New paragraph bytes (design §5.2), written from the contract.
    fn blank(&self, p: usize, text: &str) -> Vec<u8> {
        let d = &self.d;
        let run = d
            .el(p)
            .children
            .iter()
            .copied()
            .find(|&c| is(d, c, "run"))
            .unwrap();
        let start = d.el(p).start.clone();
        let mut open = d.raw(start.clone()).to_vec();
        if let Some(a) = d.el(p).attr("id") {
            let (mut from, mut to) = (a.span.start - start.start, a.span.end - start.start);
            if from > 0 && open[from - 1].is_ascii_whitespace() {
                from -= 1;
            } else if to < open.len() && open[to].is_ascii_whitespace() {
                to += 1;
            }
            open.drain(from..to);
        }
        let run_start = d.raw(d.el(run).start.clone()).to_vec();
        let mut out = open;
        if d.el(run).end.is_none() {
            out.extend_from_slice(&run_start[..run_start.len() - 2]);
            if text.is_empty() {
                out.extend_from_slice(b"/>");
            } else {
                out.extend_from_slice(
                    format!("><hp:t>{}</hp:t></{}>", escape_text(text), d.el(run).qname).as_bytes(),
                );
            }
        } else {
            out.extend_from_slice(&run_start);
            if !text.is_empty() {
                out.extend_from_slice(format!("<hp:t>{}</hp:t>", escape_text(text)).as_bytes());
            }
            out.extend_from_slice(format!("</{}>", d.el(run).qname).as_bytes());
        }
        out.extend_from_slice(format!("</{}>", d.el(p).qname).as_bytes());
        out
    }
    fn finish(mut self) -> Vec<u8> {
        let new = hae_verify_patched(self.d.bytes(), self.cuts);
        self.state.set(SECTION, Arc::from(new));
        let plan = hae_package::zip_plan(&self.archive, &self.state).unwrap();
        hae_zip::write(&self.archive, &plan).unwrap()
    }
}

/// Local splice assembler (the verifier's own is private).
fn hae_verify_patched(bytes: &[u8], mut cuts: Vec<Cut>) -> Vec<u8> {
    cuts.sort_by_key(|(r, _)| (r.start, r.end));
    let mut out = Vec::with_capacity(bytes.len());
    let mut pos = 0;
    for (r, v) in cuts {
        assert!(r.start >= pos && r.end <= bytes.len(), "overlapping cut");
        out.extend_from_slice(&bytes[pos..r.start]);
        out.extend_from_slice(&v);
        pos = r.end;
    }
    out.extend_from_slice(&bytes[pos..]);
    out
}

fn insert_op(
    d: &XmlDoc,
    p: usize,
    position: &str,
    text: Option<&str>,
    count: usize,
) -> VerifyOperation {
    VerifyOperation::ParagraphInsert {
        entry: SECTION.into(),
        path: d.path(p),
        position: position.into(),
        text: text.map(str::to_string),
        count,
    }
}

fn delete_op(d: &XmlDoc, p: usize, count: usize) -> VerifyOperation {
    VerifyOperation::ParagraphDelete {
        entry: SECTION.into(),
        path: d.path(p),
        count,
    }
}

fn verify(candidate: &[u8], ops: &[VerifyOperation]) {
    let archive = Archive::open(SOURCE.to_vec(), &Limits::default()).unwrap();
    verify_archive(&archive, candidate, ops, &Limits::default()).unwrap();
}

fn reject(candidate: &[u8], ops: &[VerifyOperation]) {
    let archive = Archive::open(SOURCE.to_vec(), &Limits::default()).unwrap();
    let e = verify_archive(&archive, candidate, ops, &Limits::default()).unwrap_err();
    assert_eq!(e.code(), hae_diag::Code::VerifyFailed, "{e}");
}

// ---------------------------------------------------------------- accept

#[test]
fn insert_after_matches_the_independent_derivation() {
    let mut b = Build::new();
    let target = b.top(1); // 둘째 문단, paraPrIDRef=1
    let op = insert_op(&b.d, target, "after", Some("새 문단"), 1);
    let blob = b.blank(target, "새 문단");
    let at = b.d.el(target).outer().end;
    b.insert(at, blob);
    b.caches_from_top(1);
    verify(&b.finish(), &[op]);
}

#[test]
fn insert_before_with_count_two_and_empty_text_matches() {
    let mut b = Build::new();
    let target = b.top(2); // 셋째 문단
    let op = insert_op(&b.d, target, "before", None, 2);
    let blob = b.blank(target, "");
    let at = b.d.el(target).outer().start;
    let mut twice = blob.clone();
    twice.extend_from_slice(&blob);
    b.insert(at, twice);
    b.caches_from_top(2);
    verify(&b.finish(), &[op]);
}

#[test]
fn delete_two_paragraphs_matches() {
    let mut b = Build::new();
    let target = b.top(1);
    let op = delete_op(&b.d, target, 2);
    for i in 1..3 {
        let p = b.top(i);
        b.cut(b.d.el(p).outer());
    }
    b.caches_from_top(1);
    verify(&b.finish(), &[op]);
}

#[test]
fn cell_paragraph_insert_reanchors_the_whole_table() {
    let mut b = Build::new();
    let target = b.by_text("셀 문단 1");
    let op = insert_op(&b.d, target, "before", Some("셀 머리"), 1);
    let blob = b.blank(target, "셀 머리");
    let at = b.d.el(target).outer().start;
    b.insert(at, blob);
    // The cell paragraph has a cache: the table re-anchor removes every cache
    // under the table, and top-level caches from the carrier (ordinal 5).
    let cached = b.by_text("셀 문단 1");
    b.caches_of(cached);
    b.caches_from_top(5);
    verify(&b.finish(), &[op]);
}

// ---------------------------------------------------------------- insert rejections

#[test]
fn insert_that_keeps_the_source_paragraph_id_is_rejected() {
    let mut b = Build::new();
    let target = b.top(1);
    let op = insert_op(&b.d, target, "after", Some("새 문단"), 1);
    // Mutated clone: same start tag including id="2".
    let run = b.d.el(target).children[0];
    let mut blob = b.d.raw(b.d.el(target).start.clone()).to_vec();
    blob.extend_from_slice(b.d.raw(b.d.el(run).start.clone()));
    blob.extend_from_slice("<hp:t>새 문단</hp:t>".as_bytes());
    blob.extend_from_slice(format!("</{}>", b.d.el(run).qname).as_bytes());
    blob.extend_from_slice(format!("</{}>", b.d.el(target).qname).as_bytes());
    let at = b.d.el(target).outer().end;
    b.insert(at, blob);
    b.caches_from_top(1);
    reject(&b.finish(), &[op]);
}

#[test]
fn insert_with_a_different_paragraph_shape_is_rejected() {
    let mut b = Build::new();
    let target = b.top(1);
    let op = insert_op(&b.d, target, "after", Some("새 문단"), 1);
    let blob = String::from_utf8(b.blank(target, "새 문단"))
        .unwrap()
        .replace(r#"paraPrIDRef="1""#, r#"paraPrIDRef="0""#);
    let at = b.d.el(target).outer().end;
    b.insert(at, blob);
    b.caches_from_top(1);
    reject(&b.finish(), &[op]);
}

#[test]
fn insert_with_the_wrong_text_is_rejected() {
    let mut b = Build::new();
    let target = b.top(1);
    let op = insert_op(&b.d, target, "after", Some("새 문단"), 1);
    let blob = b.blank(target, "다른 문단");
    let at = b.d.el(target).outer().end;
    b.insert(at, blob);
    b.caches_from_top(1);
    reject(&b.finish(), &[op]);
}

#[test]
fn insert_at_the_wrong_position_is_rejected() {
    let mut b = Build::new();
    let target = b.top(1);
    let op = insert_op(&b.d, target, "after", Some("새 문단"), 1);
    let blob = b.blank(target, "새 문단");
    let at = b.d.el(target).outer().start; // before instead of after
    b.insert(at, blob);
    b.caches_from_top(1);
    reject(&b.finish(), &[op]);
}

#[test]
fn insert_without_the_top_level_cache_removal_is_rejected() {
    let mut b = Build::new();
    let target = b.top(1);
    let op = insert_op(&b.d, target, "after", Some("새 문단"), 1);
    let blob = b.blank(target, "새 문단");
    let at = b.d.el(target).outer().end;
    b.insert(at, blob);
    // The old cache of the anchor survives: rejected byte-for-byte.
    reject(&b.finish(), &[op]);
}

#[test]
fn insert_into_a_cell_without_clearing_its_cache_is_rejected() {
    let mut b = Build::new();
    let target = b.by_text("셀 문단 1");
    let op = insert_op(&b.d, target, "before", Some("셀 머리"), 1);
    let blob = b.blank(target, "셀 머리");
    let at = b.d.el(target).outer().start;
    b.insert(at, blob);
    reject(&b.finish(), &[op]);
}

#[test]
fn insert_before_the_section_property_paragraph_is_rejected() {
    let mut b = Build::new();
    let target = b.top(0); // owns hp:secPr
    let op = insert_op(&b.d, target, "before", Some("앞"), 1);
    let blob = b.blank(target, "앞");
    let at = b.d.el(target).outer().start;
    b.insert(at, blob);
    b.caches_from_top(0);
    // The request itself is illegal, so even a byte-faithful candidate fails.
    reject(&b.finish(), &[op]);
}

// ---------------------------------------------------------------- delete rejections

#[test]
fn delete_of_the_wrong_paragraph_is_rejected() {
    let mut b = Build::new();
    let target = b.top(1);
    let op = delete_op(&b.d, target, 1);
    let other = b.top(2);
    b.cut(b.d.el(other).outer()); // deleted 셋째 instead of 둘째
    b.caches_from_top(1);
    reject(&b.finish(), &[op]);
}

#[test]
fn delete_with_the_wrong_count_is_rejected() {
    let mut b = Build::new();
    let target = b.top(1);
    let op = delete_op(&b.d, target, 2);
    b.cut(b.d.el(target).outer()); // only one paragraph
    b.caches_from_top(1);
    reject(&b.finish(), &[op]);
}

#[test]
fn delete_that_also_removes_a_neighbour_is_rejected() {
    let mut b = Build::new();
    let target = b.top(1);
    let op = delete_op(&b.d, target, 1);
    for i in 1..3 {
        let p = b.top(i);
        b.cut(b.d.el(p).outer());
    }
    b.caches_from_top(1);
    reject(&b.finish(), &[op]);
}

#[test]
fn delete_of_a_table_paragraph_is_rejected() {
    let mut b = Build::new();
    let target = b.top(3);
    let op = delete_op(&b.d, target, 1);
    b.cut(b.d.el(target).outer());
    b.caches_from_top(3);
    reject(&b.finish(), &[op]);
}

#[test]
fn delete_that_empties_a_cell_is_rejected() {
    let mut b = Build::new();
    let target = b.by_text("한 문단 셀");
    let op = delete_op(&b.d, target, 1);
    b.cut(b.d.el(target).outer());
    b.caches_from_top(5);
    reject(&b.finish(), &[op]);
}

#[test]
fn delete_without_the_cache_policy_is_rejected() {
    let mut b = Build::new();
    let target = b.top(1);
    let op = delete_op(&b.d, target, 1);
    b.cut(b.d.el(target).outer());
    // The follower (셋째) has no cache, but its predecessor's ordinal is the
    // anchor: keeping the anchor's cache would be wrong and the candidate
    // removes nothing here, which is only accepted because 셋째 has no cache;
    // the mutation instead keeps the *deleted* paragraph's sibling cache while
    // claiming a different anchor.
    let first = b.top(0);
    b.caches_of(first); // extra cache removal is also a mutation
    reject(&b.finish(), &[op]);
}
