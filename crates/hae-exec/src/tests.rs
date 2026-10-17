use super::*;
use hae_diag::{Code, Limits};
use serde_json::json;
use std::{fs, path::PathBuf};

struct TempDir(PathBuf);
impl TempDir {
    fn new() -> Self {
        let p = std::env::temp_dir().join(hae_model::new_id("hae-exec-").unwrap());
        fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn png_bytes() -> Vec<u8> {
    let hex = "89504e470d0a1a0a0000000d49484452000000010000000108060000001f15c4890000000b49444154789c636000020000050001a5f645400000000049454e44ae426082";
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect()
}

fn png2_bytes() -> Vec<u8> {
    include_bytes!("../../../tests/fixtures/editing/inputs/ok.png").to_vec()
}

fn rich_hwpx_bytes() -> Vec<u8> {
    let base = include_bytes!("../../../tests/fixtures/adversarial/valid_base.hwpx").to_vec();
    let limits = Limits::default();
    let archive = hae_zip::Archive::open(base, &limits).unwrap();
    let section = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?><hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" xmlns:hc="http://www.hancom.co.kr/hwpml/2011/core"><hp:p id="1" paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0"><hp:t>제목 문단 Hello</hp:t></hp:run><hp:linesegarray><hp:lineseg textpos="0"/></hp:linesegarray></hp:p><hp:p id="2" paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0"><hp:tbl id="10" rowCnt="1" colCnt="1"><hp:sz width="5000" height="1000"/><hp:tr><hp:tc><hp:cellAddr colAddr="0" rowAddr="0"/><hp:cellSpan colSpan="1" rowSpan="1"/><hp:subList><hp:p id="11" paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>셀원문</hp:t></hp:run><hp:linesegarray><hp:lineseg textpos="0"/></hp:linesegarray></hp:p></hp:subList></hp:tc></hp:tr></hp:tbl><hp:pic id="20"><hp:sz width="100" height="100"/><hp:curSz width="100" height="100"/><hp:imgDim dimwidth="100" dimheight="100"/><hc:img binaryItemIDRef="img0" bright="0" contrast="0" effect="REAL_PIC"/></hp:pic></hp:run><hp:linesegarray><hp:lineseg textpos="0"/></hp:linesegarray></hp:p></hs:sec>"#;
    let header = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?><hh:head xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head" version="1.2" secCnt="1"><hh:refList><hh:charProperties itemCnt="1"><hh:charPr id="0"/></hh:charProperties><hh:paraProperties itemCnt="1"><hh:paraPr id="0"><hh:align horizontal="LEFT" vertical="BASELINE"/><hh:lineSpacing type="PERCENT" value="160"/></hh:paraPr></hh:paraProperties><hh:styles itemCnt="1"><hh:style id="0"/></hh:styles></hh:refList></hh:head>"#;
    let hpf = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?><opf:package xmlns:opf="http://www.idpf.org/2007/opf/" version="" unique-identifier="" id=""><opf:metadata><opf:title/><opf:language>ko</opf:language></opf:metadata><opf:manifest><opf:item id="header" href="Contents/header.xml" media-type="application/xml"/><opf:item id="section0" href="Contents/section0.xml" media-type="application/xml"/><opf:item id="img0" href="BinData/img0.png" media-type="image/png"/></opf:manifest><opf:spine><opf:itemref idref="header"/><opf:itemref idref="section0"/></opf:spine></opf:package>"#;

    let mut plan = hae_zip::keep_all(&archive);
    for item in &mut plan {
        match item {
            hae_zip::OutEntry::Keep(n) if n == "Contents/section0.xml" => {
                *item =
                    hae_zip::OutEntry::Replace(n.clone(), std::sync::Arc::from(section.as_bytes()));
            }
            hae_zip::OutEntry::Keep(n) if n == "Contents/header.xml" => {
                *item =
                    hae_zip::OutEntry::Replace(n.clone(), std::sync::Arc::from(header.as_bytes()));
            }
            hae_zip::OutEntry::Keep(n) if n == "Contents/content.hpf" => {
                *item = hae_zip::OutEntry::Replace(n.clone(), std::sync::Arc::from(hpf.as_bytes()));
            }
            _ => {}
        }
    }
    plan.push(hae_zip::OutEntry::Add {
        name: "BinData/img0.png".into(),
        payload: std::sync::Arc::from(png_bytes()),
        method: 0,
    });
    hae_zip::write(&archive, &plan).unwrap()
}

fn code_of(r: &ResponseEnvelope) -> Code {
    assert!(!r.ok, "expected error response, got ok: {r:?}");
    r.diagnostics[0].code
}

#[test]
fn registry_lists_all_commands_and_rejects_unknown_commands_or_fields() {
    let t = TempDir::new();
    let mut exec = SessionExecutor::new(&t.0, &t.0, Limits::default()).unwrap();
    let list_resp = exec.call("command.list", json!({}), None, false);
    assert!(list_resp.ok);
    let cmds = list_resp.result.unwrap()["commands"]
        .as_array()
        .unwrap()
        .len();
    assert_eq!(cmds, COMMAND_NAMES.len());

    for &name in COMMAND_NAMES {
        let desc = exec.call("command.describe", json!({ "command": name }), None, false);
        assert!(desc.ok, "{name}");
        assert!(desc.result.unwrap()["inputSchema"].is_object());
    }

    let unknown = exec.call("command.nonexistent", json!({}), None, false);
    assert_eq!(code_of(&unknown), Code::UnknownCommand);

    let extra_field = exec.call(
        "command.describe",
        json!({ "command": "command.list", "unexpected": 1 }),
        None,
        false,
    );
    assert_eq!(code_of(&extra_field), Code::InvalidRequest);
}

#[test]
fn envelope_writer_sequence_replay_conflict_and_ordering_rules() {
    let t = TempDir::new();
    let src = t.0.join("doc.hwpx");
    fs::write(&src, rich_hwpx_bytes()).unwrap();
    let mut exec = SessionExecutor::new(&t.0, &t.0, Limits::default()).unwrap();

    // Session mismatch
    let bad_ses = exec.execute(RequestEnvelope {
        schema_version: 1,
        session_id: Some("ses-wrong".into()),
        writer_id: None,
        request_sequence: None,
        request_id: None,
        document_id: None,
        expected_version: None,
        command: "command.list".into(),
        dry_run: false,
        params: json!({}),
    });
    assert_eq!(code_of(&bad_ses), Code::SessionMismatch);

    // Unknown writer
    let bad_writer = exec.execute(RequestEnvelope {
        schema_version: 1,
        session_id: Some(exec.session_id().into()),
        writer_id: Some("wrt-unknown".into()),
        request_sequence: Some(1),
        request_id: None,
        document_id: None,
        expected_version: None,
        command: "history.clear".into(),
        dry_run: false,
        params: json!({}),
    });
    assert_eq!(code_of(&bad_writer), Code::UnknownWriter);

    exec.call("document.open", json!({ "path": src }), None, false);
    let q = exec.call(
        "element.query",
        json!({ "kind": "paragraph", "textContains": "제목" }),
        None,
        false,
    );
    let para_id = q.result.unwrap()["matches"][0]["nodeId"]
        .as_str()
        .unwrap()
        .to_owned();

    let writer = exec.issue_writer().unwrap();
    let doc_id = exec.current_document_id().unwrap().to_owned();

    // Skipped sequence 2 when expecting 1 -> REQUEST_OUT_OF_ORDER
    let out_of_order = exec.execute(RequestEnvelope {
        schema_version: 1,
        session_id: Some(exec.session_id().into()),
        writer_id: Some(writer.clone()),
        request_sequence: Some(2),
        request_id: None,
        document_id: Some(doc_id.clone()),
        expected_version: Some(0),
        command: "text.replace".into(),
        dry_run: false,
        params: json!({ "target": para_id, "range": { "start": 0, "end": 2 }, "text": "변경" }),
    });
    assert_eq!(code_of(&out_of_order), Code::RequestOutOfOrder);

    // Sequence 1 succeeds
    let req1 = RequestEnvelope {
        schema_version: 1,
        session_id: Some(exec.session_id().into()),
        writer_id: Some(writer.clone()),
        request_sequence: Some(1),
        request_id: Some("r1".into()),
        document_id: Some(doc_id.clone()),
        expected_version: Some(0),
        command: "text.replace".into(),
        dry_run: false,
        params: json!({ "target": para_id, "range": { "start": 0, "end": 2 }, "text": "변경" }),
    };
    let res1 = exec.execute(req1.clone());
    assert!(res1.ok);
    assert!(!res1.replayed);
    assert_eq!(res1.result.as_ref().unwrap()["afterVersion"], 1);

    // Exact replay of sequence 1 returns saved response with replayed=true BEFORE version check!
    let replay1 = exec.execute(req1);
    assert!(replay1.ok);
    assert!(replay1.replayed);
    assert_eq!(replay1.result.unwrap()["afterVersion"], 1);
    assert_eq!(exec.current_revision(), Some(1));

    // Same sequence 1 with different payload -> REQUEST_ID_CONFLICT
    let conflict1 = exec.execute(RequestEnvelope {
        schema_version: 1,
        session_id: Some(exec.session_id().into()),
        writer_id: Some(writer.clone()),
        request_sequence: Some(1),
        request_id: None,
        document_id: Some(doc_id.clone()),
        expected_version: Some(0),
        command: "text.replace".into(),
        dry_run: false,
        params: json!({ "target": para_id, "range": { "start": 0, "end": 2 }, "text": "다른값" }),
    });
    assert_eq!(code_of(&conflict1), Code::RequestIdConflict);

    // Sequence 2 with stale expectedVersion=0 fails with VERSION_CONFLICT, AND consumes sequence 2!
    let seq2_fail = exec.execute(RequestEnvelope {
        schema_version: 1,
        session_id: Some(exec.session_id().into()),
        writer_id: Some(writer.clone()),
        request_sequence: Some(2),
        request_id: None,
        document_id: Some(doc_id),
        expected_version: Some(0),
        command: "text.replace".into(),
        dry_run: false,
        params: json!({ "target": para_id, "range": { "start": 0, "end": 2 }, "text": "재시도" }),
    });
    assert_eq!(code_of(&seq2_fail), Code::VersionConflict);

    // Now sequence 1 is older than last_seq (2) -> REPLAY_TOO_OLD
    let old = exec.execute(RequestEnvelope {
        schema_version: 1,
        session_id: Some(exec.session_id().into()),
        writer_id: Some(writer),
        request_sequence: Some(1),
        request_id: None,
        document_id: None,
        expected_version: None,
        command: "history.clear".into(),
        dry_run: false,
        params: json!({}),
    });
    assert_eq!(code_of(&old), Code::ReplayTooOld);
}

#[test]
fn full_scenario_batch_dry_run_undo_redo_frozen_image_and_project_roundtrip() {
    let t = TempDir::new();
    let src = t.0.join("input.hwpx");
    let orig_bytes = rich_hwpx_bytes();
    fs::write(&src, &orig_bytes).unwrap();
    let img_path = t.0.join("new_pic.png");
    fs::write(&img_path, png2_bytes()).unwrap();

    let mut exec = SessionExecutor::new(&t.0, &t.0, Limits::default()).unwrap();
    let open = exec.call("document.open", json!({ "path": src }), None, false);
    assert!(open.ok);

    // Inspect cursor invalidated when revision changes
    let ins = exec.call("document.inspect", json!({ "limit": 1 }), None, false);
    let cursor = ins.result.unwrap()["nextCursor"]
        .as_str()
        .unwrap()
        .to_owned();

    // Preview returns RENDERER_UNAVAILABLE in Stage 1
    let prev = exec.call("document.preview", json!({}), None, false);
    assert_eq!(code_of(&prev), Code::RendererUnavailable);

    // Find targets
    let p_id = exec
        .call(
            "element.query",
            json!({ "kind": "paragraph", "textContains": "제목" }),
            None,
            false,
        )
        .result
        .unwrap()["matches"][0]["nodeId"]
        .as_str()
        .unwrap()
        .to_owned();
    let tbl_id = exec
        .call("element.query", json!({ "kind": "table" }), None, false)
        .result
        .unwrap()["matches"][0]["nodeId"]
        .as_str()
        .unwrap()
        .to_owned();
    let cell_id = exec
        .call(
            "element.query",
            json!({ "table": tbl_id, "row": 0, "col": 0 }),
            None,
            false,
        )
        .result
        .unwrap()["matches"][0]["nodeId"]
        .as_str()
        .unwrap()
        .to_owned();
    let pic_id = exec
        .call("element.query", json!({ "kind": "picture" }), None, false)
        .result
        .unwrap()["matches"][0]["nodeId"]
        .as_str()
        .unwrap()
        .to_owned();

    // Version check happens BEFORE noop check
    let ver_conflict = exec.call(
        "text.replace",
        json!({ "target": p_id, "range": { "start": 0, "end": 2 }, "text": "제목" }),
        Some(99),
        false,
    );
    assert_eq!(code_of(&ver_conflict), Code::VersionConflict);

    // Restoring batch (A -> B -> A) is noop and does not advance revision
    let noop_batch = exec.call(
        "command.batch",
        json!({
            "steps": [
                { "command": "text.replace", "params": { "target": p_id, "range": { "start": 0, "end": 2 }, "text": "임시" } },
                { "command": "text.replace", "params": { "target": p_id, "range": { "start": 0, "end": 2 }, "text": "제목" } }
            ]
        }),
        Some(0),
        false,
    );
    assert!(noop_batch.ok);
    assert_eq!(noop_batch.result.unwrap()["outcome"], "noop");
    assert_eq!(exec.current_revision(), Some(0));

    // Failing batch step 1 reports failedStep=1 and leaves document untouched
    let bad_batch = exec.call(
        "command.batch",
        json!({
            "steps": [
                { "command": "cell.setText", "params": { "target": cell_id, "text": "바뀐셀" } },
                { "command": "text.replace", "params": { "target": p_id, "range": { "start": 0, "end": 999 }, "text": "범위초과" } }
            ]
        }),
        Some(0),
        false,
    );
    assert_eq!(code_of(&bad_batch), Code::InvalidRange);
    assert_eq!(
        bad_batch.diagnostics[0].detail.as_ref().unwrap()["failedStep"],
        1
    );
    assert_eq!(exec.current_revision(), Some(0));

    // dryRun computes changes without advancing revision or sequence
    let dry = exec.call(
        "command.batch",
        json!({
            "steps": [
                { "command": "cell.setText", "params": { "target": cell_id, "text": "확정셀" } },
                { "command": "paragraph.format", "params": { "target": p_id, "alignment": "center" } },
                { "command": "image.replace", "params": { "target": pic_id, "imagePath": img_path } }
            ]
        }),
        Some(0),
        true,
    );
    assert!(dry.ok, "{dry:?}");
    assert_eq!(dry.result.as_ref().unwrap()["outcome"], "applied");
    assert_eq!(exec.current_revision(), Some(0));

    // Real commit of the batch
    let real = exec.call(
        "command.batch",
        json!({
            "steps": [
                { "command": "cell.setText", "params": { "target": cell_id, "text": "확정셀" } },
                { "command": "paragraph.format", "params": { "target": p_id, "alignment": "center" } },
                { "command": "image.replace", "params": { "target": pic_id, "imagePath": img_path } }
            ]
        }),
        Some(0),
        false,
    );
    assert!(real.ok);
    assert_eq!(exec.current_revision(), Some(1));

    // Cursor from revision 0 now fails with VERSION_CONFLICT
    let stale_ins = exec.call("document.inspect", json!({ "cursor": cursor }), None, false);
    assert_eq!(code_of(&stale_ins), Code::VersionConflict);

    // Corrupt the image file on disk AFTER commit; Undo and Redo must not re-read it!
    fs::write(&img_path, b"corrupted on disk after image.replace").unwrap();

    // Undo restores exact original state; export is byte-identical to original!
    let undo = exec.call("history.undo", json!({}), Some(1), false);
    assert!(undo.ok);
    assert_eq!(exec.current_revision(), Some(2));
    let exp_unmod = t.0.join("unmod_export.hwpx");
    let exp_res = exec.call("document.export", json!({ "path": exp_unmod }), None, false);
    assert!(exp_res.ok, "{exp_res:?}");
    assert_eq!(exp_res.result.unwrap()["unmodifiedPassthrough"], true);
    assert_eq!(fs::read(&exp_unmod).unwrap(), orig_bytes);

    // Redo restores the edited state (with frozen image bytes!) even though img_path is corrupt
    let redo = exec.call("history.redo", json!({}), Some(2), false);
    assert!(redo.ok);
    assert_eq!(exec.current_revision(), Some(3));

    // Save project, close, and reopen in a new session: IDs and revision persist
    let proj_path = t.0.join("work.hwpwork");
    let save = exec.call("project.save", json!({ "path": proj_path }), None, false);
    assert!(save.ok, "{save:?}");
    assert_eq!(save.result.unwrap()["savedVersion"], 3);

    let export_edited = t.0.join("edited_export.hwpx");
    let exp2 = exec.call(
        "document.export",
        json!({ "path": export_edited }),
        None,
        false,
    );
    assert!(exp2.ok, "{exp2:?}");

    let old_doc_id = exec.current_document_id().unwrap().to_owned();
    exec.call("document.close", json!({}), None, false);

    let mut exec2 = SessionExecutor::new(&t.0, &t.0, Limits::default()).unwrap();
    let reopen = exec2.call("document.open", json!({ "path": proj_path }), None, false);
    assert!(reopen.ok, "{reopen:?}");
    assert_eq!(exec2.current_document_id(), Some(old_doc_id.as_str()));
    assert_eq!(exec2.current_revision(), Some(3));
    let cell_get = exec2.call("element.get", json!({ "target": cell_id }), None, false);
    assert_eq!(cell_get.result.unwrap()["text"], "확정셀");
}

#[test]
fn history_100_unit_eviction_clear_and_snapshot_release_and_memory_budget() {
    let t = TempDir::new();
    let src = t.0.join("doc.hwpx");
    fs::write(&src, rich_hwpx_bytes()).unwrap();
    let mut exec = SessionExecutor::new(&t.0, &t.0, Limits::default()).unwrap();
    exec.call("document.open", json!({ "path": src }), None, false);
    let p_id = exec
        .call(
            "element.query",
            json!({ "kind": "paragraph", "textContains": "제목" }),
            None,
            false,
        )
        .result
        .unwrap()["matches"][0]["nodeId"]
        .as_str()
        .unwrap()
        .to_owned();

    let snap = exec.call("document.snapshot", json!({ "label": "s0" }), None, false);
    let snap_id = snap.result.unwrap()["snapshotId"]
        .as_str()
        .unwrap()
        .to_owned();

    for i in 1..=101u64 {
        let word = format!("{i:04}");
        let r = exec.call(
            "text.replace",
            json!({ "target": p_id, "range": { "start": 0, "end": 4 }, "text": word }),
            Some(i - 1),
            false,
        );
        assert!(r.ok, "step {i}: {r:?}");
        let evicted = r.result.unwrap()["historyEvicted"].as_bool().unwrap();
        assert_eq!(evicted, i == 101, "step {i}");
    }

    // Memory across 100 history units + explicit snapshot stays compact (< 5 MiB)
    assert!(exec.memory_used_bytes() < 5 * 1024 * 1024);

    // Release snapshot -> subsequent diff on it returns SNAPSHOT_UNAVAILABLE
    let rel = exec.call(
        "snapshot.release",
        json!({ "snapshotId": snap_id }),
        None,
        false,
    );
    assert!(rel.ok);
    assert_eq!(rel.result.unwrap()["controlChanged"], true);
    let diff = exec.call(
        "document.diff",
        json!({ "baseSnapshotId": snap_id }),
        None,
        false,
    );
    assert_eq!(code_of(&diff), Code::SnapshotUnavailable);

    // Clear history -> subsequent undo returns HISTORY_EMPTY without changing revision
    let rev_before = exec.current_revision().unwrap();
    let clr = exec.call("history.clear", json!({}), None, false);
    assert!(clr.ok);
    assert_eq!(clr.result.unwrap()["controlChanged"], true);
    assert_eq!(exec.current_revision(), Some(rev_before));
    let undo_empty = exec.call("history.undo", json!({}), Some(rev_before), false);
    assert_eq!(code_of(&undo_empty), Code::HistoryEmpty);
}

#[test]
fn query_result_schemas_describe_real_outlines_and_capabilities() {
    for (name, field) in [
        ("document.inspect", "items"),
        ("element.query", "matches"),
        ("element.get", "outline"),
    ] {
        let schema = command_describe(name).unwrap().output_schema;
        assert!(
            schema["properties"][field].is_object(),
            "{name}: missing result field"
        );
        let outline = &schema["$defs"]["Outline"];
        for prop in [
            "edits",
            "editable",
            "textSpans",
            "textSpansTruncated",
            "childParagraphsEditable",
        ] {
            assert!(
                outline["properties"][prop].is_object(),
                "{name}: missing {prop}: {schema}"
            );
        }
    }
}

#[test]
fn default_inspect_page_with_many_spans_stays_under_response_budget() {
    let t = TempDir::new();
    let limits = Limits::default();
    let archive = hae_zip::Archive::open(rich_hwpx_bytes(), &limits).unwrap();
    let mut state = hae_package::PackageState::from_archive(&archive);
    let paragraph = format!(
        r#"<hp:p paraPrIDRef="0"><hp:run><hp:t>{}</hp:t></hp:run></hp:p>"#,
        "x<hp:tab/>".repeat(65)
    );
    let section = format!(
        r#"<hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph">{}</hs:sec>"#,
        paragraph.repeat(100)
    );
    state.set(
        "Contents/section0.xml",
        std::sync::Arc::from(section.into_bytes()),
    );
    let source = t.0.join("many-spans.hwpx");
    fs::write(
        &source,
        hae_zip::write(&archive, &hae_package::zip_plan(&archive, &state).unwrap()).unwrap(),
    )
    .unwrap();
    let mut exec = SessionExecutor::new(&t.0, &t.0, limits).unwrap();
    assert!(
        exec.call("document.open", json!({"path":source}), None, false)
            .ok
    );
    let response = exec.call("document.inspect", json!({}), None, false);
    assert!(response.ok);
    assert!(serde_json::to_vec(&response).unwrap().len() < limits.max_response_bytes as usize);
    let result = response.result.unwrap();
    assert_eq!(result["items"].as_array().unwrap().len(), 100);
    assert!(result["nextCursor"].is_string());
    for item in result["items"].as_array().unwrap().iter().skip(1) {
        assert_eq!(item["textSpans"].as_array().unwrap().len(), 64);
        assert_eq!(item["textSpansTruncated"], true);
    }
}
