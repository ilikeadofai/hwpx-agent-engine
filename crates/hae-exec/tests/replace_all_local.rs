//! Local-only text.replaceAll property check on the real stage-4 user documents.
//! Nothing read here is copied into fixtures or logs: only counts are printed.
//! Missing inputs skip the test. HAE_EDITABLE_INPUT_DIR replays other inputs.
//!
//! For several words that really occur in each document the expected result is
//! computed here from the original paragraph texts (plain leftmost,
//! non-overlapping `str` matching) and compared with the engine's output
//! paragraph by paragraph. Matches the engine reports as skipped are excluded
//! from the expectation by their reported location; every other match must be
//! replaced. Undo must restore the original ZIP bytes exactly.
use hae_diag::Limits;
use hae_exec::SessionExecutor;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::{fs, path::PathBuf};

struct OutputDir(PathBuf);
impl Drop for OutputDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn ok(exec: &mut SessionExecutor, command: &str, params: Value) -> Value {
    let r = exec.call(command, params, None, false);
    assert!(r.ok, "{command}: {:?}", r.diagnostics);
    r.result.unwrap()
}

fn texts(exec: &mut SessionExecutor, ids: &[String]) -> Vec<String> {
    ids.iter()
        .map(|id| {
            ok(exec, "element.get", json!({ "target": id }))["text"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect()
}

/// Leftmost non-overlapping codepoint matches, independent of the engine.
fn matches(text: &str, word: &str) -> Vec<std::ops::Range<usize>> {
    let (mut out, mut cp, mut last) = (Vec::new(), 0, 0);
    let n = word.chars().count();
    for (byte, _) in text.match_indices(word) {
        cp += text[last..byte].chars().count();
        out.push(cp..cp + n);
        cp += n;
        last = byte + word.len();
    }
    out
}

fn choose_words(paragraphs: &[String]) -> Vec<String> {
    let mut frequency: BTreeMap<String, usize> = BTreeMap::new();
    for text in paragraphs {
        for token in text.split(|c: char| !c.is_alphanumeric()) {
            if token.chars().count() >= 2 {
                *frequency.entry(token.to_string()).or_default() += 1;
            }
        }
    }
    let mut ranked: Vec<_> = frequency.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut words = Vec::new();
    for index in [
        0,
        1,
        2,
        ranked.len() / 4,
        ranked.len() / 2,
        ranked.len() * 3 / 4,
    ] {
        if let Some((w, _)) = ranked.get(index) {
            if !words.contains(w) {
                words.push(w.clone());
            }
        }
    }
    // A substring inside a longer word: the first two characters of the longest token.
    if let Some((longest, _)) = ranked
        .iter()
        .max_by_key(|(w, _)| (w.chars().count(), w.clone()))
    {
        if longest.chars().count() >= 3 {
            let prefix: String = longest.chars().take(2).collect();
            if !words.contains(&prefix) {
                words.push(prefix);
            }
        }
    }
    words
}

#[test]
fn local_documents_replace_all_matches_independent_expectation_and_undoes_exactly() {
    let input = std::env::var_os("HAE_EDITABLE_INPUT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../spikes/work/stage4-e2e/inputs")
        });
    if !input.is_dir() {
        eprintln!("SKIP replaceAll local property: optional inputs absent");
        return;
    }
    let input = input.canonicalize().unwrap();
    let out =
        OutputDir(std::env::temp_dir().join(hae_model::new_id("hae-replace-all-local-").unwrap()));
    fs::create_dir(&out.0).unwrap();
    let mut paths: Vec<_> = fs::read_dir(&input)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "hwpx"))
        .collect();
    paths.sort();
    if paths.is_empty() {
        eprintln!("SKIP replaceAll local property: no .hwpx inputs");
        return;
    }
    let replacements = ["교체&<>\"", "", "ＸＹ"];
    let (mut runs, mut found, mut replaced, mut skipped, mut scoped, mut refused, mut undo) =
        (0, 0, 0, 0, 0, 0, 0);
    let mut crossings = 0;
    for path in &paths {
        let original = fs::read(path).unwrap();
        let model = hae_model::Document::open(original.clone(), &Limits::default()).unwrap();
        let model_paragraphs: Vec<_> = model
            .nodes
            .iter()
            .filter(|n| n.kind == hae_model::Kind::Paragraph)
            .collect();
        let mut exec = SessionExecutor::new(&input, &out.0, Limits::default()).unwrap();
        ok(&mut exec, "document.open", json!({ "path": path }));
        let ids: Vec<String> = ok(&mut exec, "element.query", json!({ "kind": "paragraph" }))
            ["matches"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["nodeId"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(ids.len(), model_paragraphs.len());
        let before = texts(&mut exec, &ids);
        let index: BTreeMap<&str, usize> = ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.as_str(), i))
            .collect();
        let mut revision = 0u64;
        for (n, word) in choose_words(&before).iter().enumerate() {
            let text = replacements[n % replacements.len()];
            let per_paragraph: Vec<_> = before.iter().map(|t| matches(t, word)).collect();
            let total: usize = per_paragraph.iter().map(Vec::len).sum();
            if total == 0 || total > 10_000 {
                continue;
            }
            runs += 1;
            found += total;
            let params = json!({ "find": word, "text": text, "maxMatches": 10_000 });

            // dryRun: same counts and listing, nothing changes.
            let dry = exec.call("text.replaceAll", params.clone(), Some(revision), true);
            assert!(dry.ok, "dryRun: {:?}", dry.diagnostics);
            let dry = dry.result.unwrap();
            assert_eq!(dry["afterVersion"], revision);
            assert_eq!(
                dry["replacementCount"].as_u64().unwrap() + dry["skippedCount"].as_u64().unwrap(),
                total as u64,
                "every independently counted match is replaced or reported as skipped"
            );
            assert_eq!(
                dry["matches"].as_array().unwrap().len(),
                (dry["replacementCount"].as_u64().unwrap() as usize).min(200)
            );

            // maxMatches one below the replaceable count is refused and changes nothing.
            let can = dry["replacementCount"].as_u64().unwrap();
            if can >= 2 {
                let mut small = params.clone();
                small["maxMatches"] = json!(can - 1);
                let r = exec.call("text.replaceAll", small, Some(revision), false);
                assert!(!r.ok && r.diagnostics[0].code == hae_diag::Code::ResourceLimit);
                refused += 1;
            }

            let r = exec.call("text.replaceAll", params, Some(revision), false);
            assert!(r.ok, "real call: {:?}", r.diagnostics);
            let result = r.result.unwrap();
            assert_eq!(result["replacementCount"], dry["replacementCount"]);
            assert_eq!(result["skippedCount"], dry["skippedCount"]);
            let (count, skip) = (
                result["replacementCount"].as_u64().unwrap() as usize,
                result["skippedCount"].as_u64().unwrap() as usize,
            );
            replaced += count;
            skipped += skip;

            // Skipped matches by reported location (cp ranges inside the grapheme range).
            let mut skipped_cps: BTreeMap<usize, Vec<std::ops::Range<usize>>> = BTreeMap::new();
            let mut complete = true;
            for group in result["skipped"].as_array().unwrap() {
                complete &= group["locationsTruncated"] == false;
                for loc in group["locations"].as_array().unwrap() {
                    let i = index[loc["nodeId"].as_str().unwrap()];
                    let pt = model.para_text(model_paragraphs[i]).unwrap();
                    let (a, b) = (
                        loc["range"]["start"].as_u64().unwrap() as usize,
                        loc["range"]["end"].as_u64().unwrap() as usize,
                    );
                    skipped_cps
                        .entry(i)
                        .or_default()
                        .push(pt.grapheme_cps[a]..pt.grapheme_cps[b]);
                }
            }
            let after = texts(&mut exec, &ids);
            let mut changed_paragraphs = 0;
            for (i, ms) in per_paragraph.iter().enumerate() {
                let skip_here = skipped_cps.get(&i);
                if !complete && skip_here.is_some() {
                    continue; // too many skipped locations to attribute individually
                }
                let chars: Vec<char> = before[i].chars().collect();
                let (mut expected, mut at) = (String::new(), 0);
                for m in ms {
                    let is_skipped = skip_here
                        .is_some_and(|rs| rs.iter().any(|r| r.start <= m.start && m.end <= r.end));
                    if !is_skipped {
                        expected.extend(&chars[at..m.start]);
                        expected.push_str(text);
                        at = m.end;
                    }
                }
                expected.extend(&chars[at..]);
                assert_eq!(
                    after[i],
                    expected,
                    "paragraph {i} of {:?}",
                    path.file_name()
                );
                changed_paragraphs += usize::from(after[i] != before[i]);
            }
            if complete {
                assert_eq!(
                    changed_paragraphs,
                    result["paragraphCount"].as_u64().unwrap() as usize
                );
            }
            revision += 1;

            // One Undo restores the original bytes exactly.
            let u = exec.call("history.undo", json!({}), Some(revision), false);
            assert!(u.ok, "undo: {:?}", u.diagnostics);
            revision += 1;
            let export = out.0.join("undo.hwpx");
            ok(&mut exec, "document.export", json!({ "path": export }));
            assert!(fs::read(&export).unwrap() == original, "Undo mismatch");
            fs::remove_file(&export).unwrap();
            assert_eq!(texts(&mut exec, &ids), before);
            undo += 1;

            // scope: only the first paragraph that holds the word.
            if let Some(first) = per_paragraph.iter().position(|m| !m.is_empty()) {
                let r = exec.call(
                    "text.replaceAll",
                    json!({ "find": word, "text": text, "scope": ids[first], "maxMatches": 10_000 }),
                    Some(revision),
                    false,
                );
                assert!(r.ok, "scoped: {:?}", r.diagnostics);
                let result = r.result.unwrap();
                let changed = texts(&mut exec, &ids);
                for (i, t) in changed.iter().enumerate() {
                    // Only the scope paragraph and the paragraphs below it may change.
                    let inside = i == first
                        || model.is_descendant(model_paragraphs[i], &model_paragraphs[first].id);
                    if !inside {
                        assert_eq!(*t, before[i], "paragraph {i} changed outside the scope");
                    }
                }
                if result["outcome"] == "applied" {
                    revision += 1;
                    let u = exec.call("history.undo", json!({}), Some(revision), false);
                    assert!(u.ok);
                    revision += 1;
                }
                let export = out.0.join("undo-scope.hwpx");
                ok(&mut exec, "document.export", json!({ "path": export }));
                assert!(
                    fs::read(&export).unwrap() == original,
                    "scoped Undo mismatch"
                );
                fs::remove_file(&export).unwrap();
                scoped += 1;
            }
        }
        // A search that has to cross an object placeholder is skipped everywhere, never edited.
        let crossing = before.iter().find_map(|t| {
            let chars: Vec<char> = t.chars().collect();
            chars
                .windows(2)
                .find(|w| {
                    w.iter()
                        .filter(|c| **c == '\u{fffc}' || **c == '\t')
                        .count()
                        == 1
                })
                .map(|w| w.iter().collect::<String>())
        });
        if let Some(find) = crossing {
            let total: usize = before.iter().map(|t| matches(t, &find).len()).sum();
            let r = exec.call(
                "text.replaceAll",
                json!({ "find": find, "text": "X", "maxMatches": 10_000 }),
                Some(revision),
                false,
            );
            assert!(r.ok, "{:?}", r.diagnostics);
            let result = r.result.unwrap();
            assert_eq!(result["outcome"], "noop");
            assert_eq!(result["replacementCount"], 0);
            assert_eq!(result["skippedCount"].as_u64().unwrap() as usize, total);
            assert!(result["skipped"].as_array().unwrap().iter().all(|g| {
                g["reason"] == "crosses_control" || g["reason"] == "unsupported_markup"
            }));
            assert!(r
                .diagnostics
                .iter()
                .any(|d| d.code.as_str() == "MATCHES_SKIPPED"));
            let export = out.0.join("noop.hwpx");
            ok(&mut exec, "document.export", json!({ "path": export }));
            assert!(
                fs::read(&export).unwrap() == original,
                "skipped matches must not edit"
            );
            fs::remove_file(&export).unwrap();
            crossings += total;
        }
        assert_eq!(
            fs::read(path).unwrap(),
            original,
            "input must stay untouched"
        );
    }
    eprintln!(
        "REPLACE_ALL_LOCAL files={} runs={runs} independent_matches={found} replaced={replaced} skipped={skipped} refused={refused} undo_exact={undo} scoped_undo_exact={scoped} object_crossing_skipped={crossings}",
        paths.len()
    );
    assert!(runs > 0 && replaced > 0, "no word was actually replaced");
}
