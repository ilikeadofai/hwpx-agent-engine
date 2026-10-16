//! Every corpus document opens, and a keep-all rewrite is byte-identical.

use hae_diag::Limits;
use std::path::PathBuf;

fn corpus() -> Vec<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/corpus");
    let mut v: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "hwpx"))
        .collect();
    v.sort();
    assert!(!v.is_empty(), "empty corpus");
    v
}

#[test]
fn corpus_opens_and_roundtrips() {
    for p in corpus() {
        let b = std::fs::read(&p).unwrap();
        let a = hae_zip::Archive::open(b.clone(), &Limits::default())
            .unwrap_or_else(|e| panic!("{}: {e}", p.display()));
        let w = hae_zip::write(&a, &hae_zip::keep_all(&a)).unwrap();
        assert_eq!(w, b, "{}", p.display());
    }
}
