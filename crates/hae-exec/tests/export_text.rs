//! `document.exportText` through the executor: file publication rules, size
//! limit, immutability of the document and the summary-only response.

use hae_diag::{Code, Limits};
use hae_exec::{ResponseEnvelope, SessionExecutor};
use hae_zip::{keep_all, write, Archive, OutEntry};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const NS: &str = r#"xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" xmlns:hc="http://www.hancom.co.kr/hwpml/2011/core""#;

/// paraPr 0 / charPr 0 exist, so the document can also pass the save gate.
const HEADER: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?><hh:head xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head" version="1.2" secCnt="1"><hh:refList><hh:charProperties itemCnt="1"><hh:charPr id="0"/></hh:charProperties><hh:paraProperties itemCnt="1"><hh:paraPr id="0"><hh:align horizontal="LEFT" vertical="BASELINE"/><hh:lineSpacing type="PERCENT" value="160"/></hh:paraPr></hh:paraProperties></hh:refList></hh:head>"#;

struct Dirs {
    root: PathBuf,
    input: PathBuf,
    output: PathBuf,
}

impl Dirs {
    fn new() -> Dirs {
        let root = std::env::temp_dir().join(hae_model::new_id("hae-export-text-").unwrap());
        let (input, output) = (root.join("in"), root.join("out"));
        fs::create_dir_all(&input).unwrap();
        fs::create_dir_all(&output).unwrap();
        Dirs {
            root: root.canonicalize().unwrap(),
            input: input.canonicalize().unwrap(),
            output: output.canonicalize().unwrap(),
        }
    }

    fn executor(&self, limits: Limits) -> SessionExecutor {
        SessionExecutor::new(&self.input, &self.output, limits).unwrap()
    }

    fn corpus(&self, name: &str) -> PathBuf {
        let src = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/corpus")
            .join(name);
        let dst = self.input.join(name);
        fs::copy(src, &dst).unwrap();
        dst
    }

    /// A one-section document whose body is `body`, built from the valid base package.
    fn synthetic(&self, name: &str, body: &str) -> PathBuf {
        let base = fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/adversarial/valid_base.hwpx"),
        )
        .unwrap();
        let archive = Archive::open(base, &Limits::default()).unwrap();
        let section = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?><hs:sec {NS}>{body}</hs:sec>"#
        );
        let mut plan = keep_all(&archive);
        for item in &mut plan {
            if let OutEntry::Keep(n) = item {
                let replacement = match n.as_str() {
                    "Contents/section0.xml" => section.as_str(),
                    "Contents/header.xml" => HEADER,
                    _ => continue,
                };
                *item = OutEntry::Replace(
                    n.clone(),
                    Arc::from(replacement.as_bytes().to_vec().into_boxed_slice()),
                );
            }
        }
        let path = self.input.join(name);
        fs::write(&path, write(&archive, &plan).unwrap()).unwrap();
        path
    }
}

impl Drop for Dirs {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn ok(exec: &mut SessionExecutor, command: &str, params: Value) -> Value {
    let r = exec.call(command, params, None, false);
    assert!(r.ok, "{command}: {:?}", r.diagnostics);
    r.result.unwrap()
}

fn open(exec: &mut SessionExecutor, path: &Path) -> Value {
    ok(exec, "document.open", json!({ "path": path }))
}

fn refused(r: &ResponseEnvelope) -> &hae_diag::Diagnostic {
    assert!(!r.ok, "expected a refusal, got {:?}", r.result);
    &r.diagnostics[0]
}

fn para(text: &str) -> String {
    format!(r#"<hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>{text}</hp:t></hp:run></hp:p>"#)
}

fn export_hwpx(exec: &mut SessionExecutor, dirs: &Dirs, name: &str) -> Vec<u8> {
    let path = dirs.output.join(name);
    ok(exec, "document.export", json!({ "path": path }));
    fs::read(path).unwrap()
}

#[test]
fn markdown_export_writes_the_file_and_the_response_is_only_a_summary() {
    let dirs = Dirs::new();
    let mut exec = dirs.executor(Limits::default());
    open(&mut exec, &dirs.corpus("gyehoek-reference.hwpx"));
    let out = dirs.output.join("gyehoek.md");
    let r = exec.call(
        "document.exportText",
        json!({ "path": out, "format": "markdown" }),
        None,
        false,
    );
    assert!(r.ok, "{:?}", r.diagnostics);
    let wire = serde_json::to_string(&r).unwrap();
    let result = r.result.unwrap();
    let bytes = fs::read(&out).unwrap();
    assert_eq!(result["path"], out.to_str().unwrap());
    assert_eq!(result["format"], "markdown");
    assert_eq!(result["bytes"], bytes.len());
    assert_eq!(result["sha256"], hae_package::sha256_hex(&bytes));
    assert_eq!(result["revision"], 0);
    assert_eq!(result["includeNotes"], true);
    let counts = &result["counts"];
    assert_eq!(counts["tables"], 24);
    assert_eq!(counts["pictures"], 2);
    assert_eq!(counts["headers"], 2);
    assert_eq!(counts["mergedTables"], 7);
    assert!(counts["paragraphs"].as_u64().unwrap() > 100);
    // The body is only in the file.
    assert!(result.get("text").is_none());
    assert!(wire.len() < 2048, "{} bytes: {wire}", wire.len());
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.starts_with("> **변환 안내** (HWPX → Markdown)"));
    assert!(text.contains("|   |"));
    assert!(text.contains("병합 셀이 있는 표: 표 4, 표 14"));
}

#[test]
fn html_export_of_a_document_with_pictures_and_merged_cells() {
    let dirs = Dirs::new();
    let mut exec = dirs.executor(Limits::default());
    let doc = dirs.corpus("bodojaryo-reference.hwpx");
    open(&mut exec, &doc);
    let out = dirs.output.join("bodo.html");
    let result = ok(
        &mut exec,
        "document.exportText",
        json!({ "path": out, "format": "html" }),
    );
    let text = fs::read_to_string(&out).unwrap();
    assert!(text.starts_with("<!DOCTYPE html>"));
    assert!(text.contains("colspan=\"") || text.contains("rowspan=\""));
    assert_eq!(result["counts"]["pictures"], 6);
    // Each picture's SHA-256 is the hash of the real BinData payload.
    let archive = Archive::open(fs::read(&doc).unwrap(), &Limits::default()).unwrap();
    let image1 = hae_package::sha256_hex(archive.payload("BinData/image1.jpg").unwrap());
    assert!(
        text.contains(&format!("data-sha256=\"{image1}\"")),
        "image1 hash missing"
    );
    assert!(text.contains("data-media-entry=\"BinData/image1.jpg\""));
    assert!(!text.contains("<img"));
    // No image file was created next to the text.
    let names: Vec<_> = fs::read_dir(&dirs.output)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(names, ["bodo.html"]);
}

#[test]
fn exporting_text_never_changes_the_document_its_revision_or_its_history() {
    let dirs = Dirs::new();
    let mut exec = dirs.executor(Limits::default());
    let doc = dirs.synthetic("plain.hwpx", &para("안녕 Hello"));
    let original = fs::read(&doc).unwrap();
    open(&mut exec, &doc);

    // Revision 0: exports before and after exportText are byte-identical to the input.
    let before = export_hwpx(&mut exec, &dirs, "before.hwpx");
    assert_eq!(before, original);
    for (i, format) in ["markdown", "html"].iter().enumerate() {
        let ext = if i == 0 { "md" } else { "html" };
        ok(
            &mut exec,
            "document.exportText",
            json!({ "path": dirs.output.join(format!("a.{ext}")), "format": format }),
        );
    }
    assert_eq!(export_hwpx(&mut exec, &dirs, "after.hwpx"), original);
    assert_eq!(ok(&mut exec, "document.inspect", json!({}))["revision"], 0);

    // After a real edit: exportText must not bump the revision, and the
    // expectedVersion of the next edit is still 1.
    let id = ok(&mut exec, "element.query", json!({"kind": "paragraph"}))["matches"][0]["nodeId"]
        .clone();
    let edit = exec.call(
        "text.replace",
        json!({"target": id, "range": {"start": 0, "end": 2}, "text": "잘가"}),
        Some(0),
        false,
    );
    assert!(edit.ok, "{:?}", edit.diagnostics);
    let edited = export_hwpx(&mut exec, &dirs, "edited.hwpx");
    let text = ok(
        &mut exec,
        "document.exportText",
        json!({ "path": dirs.output.join("edited.md"), "format": "markdown" }),
    );
    assert_eq!(text["revision"], 1);
    assert!(fs::read_to_string(dirs.output.join("edited.md"))
        .unwrap()
        .contains("잘가 Hello"));
    assert_eq!(export_hwpx(&mut exec, &dirs, "edited2.hwpx"), edited);
    assert_eq!(ok(&mut exec, "document.inspect", json!({}))["revision"], 1);
    let again = exec.call(
        "text.replace",
        json!({"target": id, "range": {"start": 0, "end": 2}, "text": "다시"}),
        Some(1),
        false,
    );
    assert!(again.ok, "{:?}", again.diagnostics);
    // Undo still walks back through the edits only: exportText left no history unit.
    let undo = exec.call("history.undo", json!({}), Some(2), false);
    assert!(undo.ok, "{:?}", undo.diagnostics);
    let undo2 = exec.call("history.undo", json!({}), Some(3), false);
    assert!(undo2.ok, "{:?}", undo2.diagnostics);
    assert_eq!(export_hwpx(&mut exec, &dirs, "undone.hwpx"), original);
}

#[test]
fn an_existing_file_is_never_overwritten() {
    let dirs = Dirs::new();
    let mut exec = dirs.executor(Limits::default());
    open(&mut exec, &dirs.corpus("irb-sample.hwpx"));
    let target = dirs.output.join("keep.md");
    fs::write(&target, "소중한 기존 내용").unwrap();
    let r = exec.call(
        "document.exportText",
        json!({ "path": target, "format": "markdown" }),
        None,
        false,
    );
    assert_eq!(refused(&r).code, Code::TargetExists);
    assert_eq!(fs::read_to_string(&target).unwrap(), "소중한 기존 내용");
    // The second export to a fresh name works, and a repeat of it is refused too.
    let fresh = dirs.output.join("fresh.md");
    ok(
        &mut exec,
        "document.exportText",
        json!({ "path": fresh, "format": "markdown" }),
    );
    let r = exec.call(
        "document.exportText",
        json!({ "path": fresh, "format": "markdown" }),
        None,
        false,
    );
    assert_eq!(refused(&r).code, Code::TargetExists);
    // No temporary file was left behind by the refusals.
    let mut names: Vec<_> = fs::read_dir(&dirs.output)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert_eq!(names, ["fresh.md", "keep.md"]);
}

#[cfg(unix)] // the symlink escape needs Unix symlinks
#[test]
fn paths_outside_the_output_root_are_refused_and_nothing_is_written() {
    let dirs = Dirs::new();
    let mut exec = dirs.executor(Limits::default());
    open(&mut exec, &dirs.corpus("irb-sample.hwpx"));
    // A directory outside the output root, reached by absolute path, `..`,
    // relative path, the input root, and a symlink inside the output root.
    let elsewhere = dirs.root.join("elsewhere");
    fs::create_dir(&elsewhere).unwrap();
    std::os::unix::fs::symlink(&elsewhere, dirs.output.join("link")).unwrap();
    let attempts = [
        elsewhere.join("x.md"),
        dirs.output.join("..").join("elsewhere").join("x.md"),
        PathBuf::from("relative.md"),
        dirs.input.join("in-input-root.md"),
        dirs.output.join("link").join("x.md"),
        dirs.output.join("missing-dir").join("x.md"),
    ];
    for path in attempts {
        let r = exec.call(
            "document.exportText",
            json!({ "path": path, "format": "markdown" }),
            None,
            false,
        );
        let d = refused(&r);
        assert!(
            matches!(d.code, Code::PathNotAllowed | Code::IoError),
            "{path:?}: {:?}",
            d.code
        );
    }
    assert_eq!(fs::read_dir(&elsewhere).unwrap().count(), 0);
    assert_eq!(
        fs::read_dir(&dirs.input)
            .unwrap()
            .filter(|e| e.as_ref().unwrap().file_name() != "irb-sample.hwpx")
            .count(),
        0
    );
}

#[test]
fn output_over_the_size_limit_is_refused_naming_the_limit_and_writes_nothing() {
    let dirs = Dirs::new();
    let limits = Limits {
        max_text_export_bytes: 2000,
        ..Limits::default()
    };
    let mut exec = dirs.executor(limits);
    open(&mut exec, &dirs.corpus("gyehoek-reference.hwpx"));
    for (format, name) in [("markdown", "big.md"), ("html", "big.html")] {
        let r = exec.call(
            "document.exportText",
            json!({ "path": dirs.output.join(name), "format": format }),
            None,
            false,
        );
        let d = refused(&r);
        assert_eq!(d.code, Code::ResourceLimit, "{format}");
        assert_eq!(d.reason.as_deref(), Some("text_export_too_large"));
        assert!(
            d.message.contains("max_text_export_bytes = 2000"),
            "{}",
            d.message
        );
        let detail = d.detail.as_ref().unwrap();
        assert_eq!(detail["limit"], "max_text_export_bytes");
        assert_eq!(detail["limitBytes"], 2000);
        assert!(detail["producedBytes"].as_u64().unwrap() > 2000);
    }
    assert_eq!(fs::read_dir(&dirs.output).unwrap().count(), 0);
    // The default limit is 16 MiB.
    assert_eq!(Limits::default().max_text_export_bytes, 16 * 1024 * 1024);
}

#[test]
fn a_file_larger_than_the_response_limit_still_gets_a_tiny_response() {
    let dirs = Dirs::new();
    let body: String = (0..20_000)
        .map(|i| para(&format!("{i}번째 문단 {}", "가나다라마바사".repeat(12))))
        .collect();
    let doc = dirs.synthetic("big.hwpx", &body);
    let mut exec = dirs.executor(Limits::default());
    open(&mut exec, &doc);
    let out = dirs.output.join("big.md");
    let r = exec.call(
        "document.exportText",
        json!({ "path": out, "format": "markdown" }),
        None,
        false,
    );
    assert!(r.ok, "{:?}", r.diagnostics);
    let wire = serde_json::to_vec(&r).unwrap();
    let file = fs::metadata(&out).unwrap().len();
    let limit = Limits::default().max_response_bytes;
    assert!(
        file > limit,
        "file {file} should exceed the response limit {limit}"
    );
    assert!(wire.len() < 4096, "response {} bytes", wire.len());
    assert_eq!(r.result.unwrap()["counts"]["paragraphs"], 20_000);
}

#[test]
fn extension_must_match_the_format() {
    let dirs = Dirs::new();
    let mut exec = dirs.executor(Limits::default());
    open(&mut exec, &dirs.corpus("irb-sample.hwpx"));
    for (name, format) in [
        ("fake.hwpx", "markdown"),
        ("fake.hwpwork", "html"),
        ("notes.txt", "markdown"),
        ("page.md", "html"),
        ("page.html", "markdown"),
        ("noext", "html"),
    ] {
        let r = exec.call(
            "document.exportText",
            json!({ "path": dirs.output.join(name), "format": format }),
            None,
            false,
        );
        let d = refused(&r);
        assert_eq!(d.code, Code::InvalidArgument, "{name}");
        assert_eq!(d.reason.as_deref(), Some("extension_mismatch"));
    }
    for (name, format) in [
        ("a.MD", "markdown"),
        ("b.markdown", "markdown"),
        ("c.htm", "html"),
    ] {
        ok(
            &mut exec,
            "document.exportText",
            json!({ "path": dirs.output.join(name), "format": format }),
        );
    }
    assert_eq!(fs::read_dir(&dirs.output).unwrap().count(), 3);
}

#[test]
fn scope_notes_and_bad_requests() {
    let dirs = Dirs::new();
    let mut exec = dirs.executor(Limits::default());
    let note = |text: &str| {
        format!(
            r#"<hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>{text}</hp:t><hp:ctrl><hp:footNote><hp:subList><hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>각주 {text}</hp:t></hp:run></hp:p></hp:subList></hp:footNote></hp:ctrl></hp:run></hp:p>"#
        )
    };
    let doc = dirs.synthetic("notes.hwpx", &note("본문"));
    let no_doc = exec.call(
        "document.exportText",
        json!({ "path": dirs.output.join("x.md"), "format": "markdown" }),
        None,
        false,
    );
    assert_eq!(refused(&no_doc).code, Code::SessionRequired);
    open(&mut exec, &doc);

    let with = dirs.output.join("with.md");
    let r = ok(
        &mut exec,
        "document.exportText",
        json!({ "path": with, "format": "markdown" }),
    );
    assert_eq!(r["counts"]["footnotes"], 1);
    assert!(fs::read_to_string(&with)
        .unwrap()
        .contains("[^fn1]: 각주 본문"));

    let without = dirs.output.join("without.md");
    let r = ok(
        &mut exec,
        "document.exportText",
        json!({ "path": without, "format": "markdown", "includeNotes": false }),
    );
    assert_eq!(r["includeNotes"], false);
    assert_eq!(r["counts"]["footnotes"], 0);
    assert_eq!(r["counts"]["notesOmitted"], 1);
    let text = fs::read_to_string(&without).unwrap();
    assert!(!text.contains("각주 본문") && !text.contains("[^fn1]"));

    // scope: the section itself works; a paragraph or unknown id does not.
    let inspect = ok(&mut exec, "document.inspect", json!({}));
    let items = inspect["items"].as_array().unwrap();
    let section = items.iter().find(|i| i["kind"] == "section").unwrap()["nodeId"].clone();
    let paragraph = items.iter().find(|i| i["kind"] == "paragraph").unwrap()["nodeId"].clone();
    let scoped = ok(
        &mut exec,
        "document.exportText",
        json!({ "path": dirs.output.join("scoped.html"), "format": "html", "scope": section }),
    );
    assert_eq!(scoped["scope"], section);
    let bad_scope = |exec: &mut SessionExecutor, scope: Value, name: &str| {
        exec.call(
            "document.exportText",
            json!({ "path": dirs.output.join(name), "format": "markdown", "scope": scope }),
            None,
            false,
        )
    };
    let r = bad_scope(&mut exec, paragraph, "p.md");
    assert_eq!(refused(&r).code, Code::Unsupported);
    assert_eq!(refused(&r).reason.as_deref(), Some("wrong_kind"));
    let r = bad_scope(&mut exec, json!("n-missing"), "q.md");
    assert_eq!(refused(&r).code, Code::TargetNotFound);

    let bad_format = exec.call(
        "document.exportText",
        json!({ "path": dirs.output.join("f.md"), "format": "pdf" }),
        None,
        false,
    );
    assert_eq!(refused(&bad_format).code, Code::InvalidRequest);
    let unknown_field = exec.call(
        "document.exportText",
        json!({ "path": dirs.output.join("g.md"), "format": "markdown", "overwrite": true }),
        None,
        false,
    );
    assert_eq!(refused(&unknown_field).code, Code::InvalidRequest);
    let stale = exec.call(
        "document.exportText",
        json!({ "path": dirs.output.join("h.md"), "format": "markdown" }),
        Some(9),
        false,
    );
    assert_eq!(refused(&stale).code, Code::VersionConflict);
    for name in ["p.md", "q.md", "f.md", "g.md", "h.md"] {
        assert!(!dirs.output.join(name).exists(), "{name}");
    }
}

#[test]
fn every_corpus_document_exports_in_both_formats_through_the_executor() {
    let dirs = Dirs::new();
    for name in [
        "anchor_char_height.hwpx",
        "bodojaryo-reference.hwpx",
        "gani-gian.hwpx",
        "gyehoek-reference.hwpx",
        "ilban-gian.hwpx",
        "irb-sample.hwpx",
        "problem-answer-reference.hwpx",
        "ref_empty.hwpx",
        "research-school-plan-reference.hwpx",
    ] {
        let mut exec = dirs.executor(Limits::default());
        open(&mut exec, &dirs.corpus(name));
        let stem = name.trim_end_matches(".hwpx");
        let md = ok(
            &mut exec,
            "document.exportText",
            json!({ "path": dirs.output.join(format!("{stem}.md")), "format": "markdown" }),
        );
        let html = ok(
            &mut exec,
            "document.exportText",
            json!({ "path": dirs.output.join(format!("{stem}.html")), "format": "html" }),
        );
        assert!(md["bytes"].as_u64().unwrap() > 100, "{name}");
        assert_eq!(md["counts"]["tables"], html["counts"]["tables"], "{name}");
        assert_eq!(
            md["counts"]["paragraphs"], html["counts"]["paragraphs"],
            "{name}"
        );
    }
}

#[test]
fn the_command_is_listed_as_a_non_read_only_query_with_path_and_format_required() {
    let dirs = Dirs::new();
    let mut exec = dirs.executor(Limits::default());
    let d = ok(
        &mut exec,
        "command.describe",
        json!({ "command": "document.exportText" }),
    );
    assert_eq!(d["category"], "query");
    assert_eq!(d["mutatesContent"], false);
    assert_eq!(d["requiresDocument"], true);
    assert_eq!(d["requiresExpectedVersion"], false);
    assert_eq!(d["usesRequestSequence"], false);
    assert_eq!(d["supportsDryRun"], false);
    assert_eq!(d["recordsHistory"], false);
    // It publishes a file, so it must not advertise readOnly/idempotent.
    assert_eq!(d["annotations"]["readOnlyHint"], false);
    assert_eq!(d["annotations"]["idempotentHint"], false);
    assert_eq!(d["annotations"]["destructiveHint"], false);
    let required: Vec<_> = d["inputSchema"]["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(required, ["path", "format"]);
    assert!(d["outputSchema"]["properties"]["counts"].is_object());
}
