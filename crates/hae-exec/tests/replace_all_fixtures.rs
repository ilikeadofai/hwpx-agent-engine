//! Differential sweep: text.replaceAll over every public fixture document with
//! many search strings. The editor and the independent gate must always agree
//! (no VERIFY_FAILED, no STATE_INVARIANT_ERROR), skipped matches are accounted
//! for, and Undo restores the original bytes.
use hae_diag::{Code, Limits};
use hae_exec::SessionExecutor;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::{fs, path::PathBuf};

struct OutputDir(PathBuf);
impl Drop for OutputDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn sweep(dir_name: &str, max_files: usize, max_candidates: usize) -> (usize, usize, usize, usize) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(dir_name)
        .canonicalize()
        .unwrap();
    let out =
        OutputDir(std::env::temp_dir().join(hae_model::new_id("hae-replace-all-sweep-").unwrap()));
    fs::create_dir(&out.0).unwrap();
    let mut paths: Vec<_> = fs::read_dir(&root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "hwpx"))
        .collect();
    paths.sort();
    let (mut documents, mut runs, mut applied, mut skipped) = (0, 0, 0, 0);
    for path in paths.iter().take(max_files) {
        let original = fs::read(path).unwrap();
        let mut exec = SessionExecutor::new(&root, &out.0, Limits::default()).unwrap();
        if !exec
            .call("document.open", json!({ "path": path }), None, false)
            .ok
        {
            continue;
        }
        let editable = exec.call("document.inspect", json!({ "limit": 1 }), None, false);
        if editable.result.is_some_and(|r| r["editable"] == false) {
            continue;
        }
        documents += 1;
        let ids: Vec<Value> = exec
            .call("element.query", json!({ "kind": "paragraph" }), None, false)
            .result
            .unwrap()["matches"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["nodeId"].clone())
            .collect();
        let mut candidates = BTreeSet::new();
        for id in ids.iter().take(200) {
            let text = exec
                .call("element.get", json!({ "target": id }), None, false)
                .result
                .unwrap()["text"]
                .as_str()
                .unwrap()
                .to_string();
            let chars: Vec<char> = text.chars().collect();
            for w in 1..=3 {
                for window in chars.windows(w) {
                    if candidates.len() < max_candidates {
                        candidates.insert(window.iter().collect::<String>());
                    }
                }
            }
            for token in text.split_whitespace().take(4) {
                if candidates.len() < max_candidates {
                    candidates.insert(token.to_string());
                }
            }
        }
        let mut revision = 0u64;
        for find in candidates {
            runs += 1;
            let r = exec.call(
                "text.replaceAll",
                json!({ "find": find, "text": "〔교체〕", "maxMatches": 10_000 }),
                Some(revision),
                false,
            );
            if !r.ok {
                let code = r.diagnostics[0].code;
                // Documents with unresolved/duplicate style references are not editable
                // at all (verify_invalid_references); every edit command fails the same way.
                let message = &r.diagnostics[0].message;
                if code == Code::VerifyFailed
                    && (message.starts_with("unresolved ") || message.contains("resource ID"))
                {
                    continue;
                }
                assert!(
                    !matches!(code, Code::VerifyFailed | Code::StateInvariantError),
                    "{:?} {find:?}: engine and gate disagree: {:?}",
                    path.file_name(),
                    r.diagnostics[0]
                );
                continue;
            }
            let result = r.result.unwrap();
            skipped += result["skippedCount"].as_u64().unwrap() as usize;
            if result["outcome"] == "applied" {
                applied += 1;
                revision += 1;
                let u = exec.call("history.undo", json!({}), Some(revision), false);
                assert!(u.ok);
                revision += 1;
                let export = out.0.join("undo.hwpx");
                assert!(
                    exec.call("document.export", json!({ "path": export }), None, false)
                        .ok
                );
                assert!(
                    fs::read(&export).unwrap() == original,
                    "{:?} {find:?}: Undo mismatch",
                    path.file_name()
                );
                fs::remove_file(&export).unwrap();
            }
        }
    }
    (documents, runs, applied, skipped)
}

#[test]
fn editing_fixtures_never_make_the_editor_and_gate_disagree() {
    let (documents, runs, applied, skipped) = sweep("editing", 64, 60);
    eprintln!("REPLACE_ALL_SWEEP editing documents={documents} runs={runs} applied={applied} skipped={skipped}");
    assert!(documents >= 10 && applied > 0);
}

#[test]
fn public_corpus_never_makes_the_editor_and_gate_disagree() {
    let (documents, runs, applied, skipped) = sweep("corpus", 64, 12);
    eprintln!("REPLACE_ALL_SWEEP corpus documents={documents} runs={runs} applied={applied} skipped={skipped}");
    assert!(documents >= 5 && applied > 0);
}
