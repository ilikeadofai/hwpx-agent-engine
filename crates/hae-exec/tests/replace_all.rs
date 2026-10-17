use hae_diag::{Code, Limits};
use hae_exec::SessionExecutor;
use serde_json::{json, Value};
use std::{fs, path::PathBuf, sync::Arc};
struct Harness {
    dir: PathBuf,
    original: Vec<u8>,
    exec: SessionExecutor,
}
impl Harness {
    fn new(body: &str) -> Self {
        let dir = std::env::temp_dir().join(hae_model::new_id("hae-replace-all-").unwrap());
        fs::create_dir(&dir).unwrap();
        let a = hae_zip::Archive::open(
            include_bytes!("../../../tests/fixtures/editing/split_runs.hwpx").to_vec(),
            &Limits::default(),
        )
        .unwrap();
        let mut s = hae_package::PackageState::from_archive(&a);
        s.set("Contents/section0.xml",Arc::from(format!("<hs:sec xmlns:hs=\"http://www.hancom.co.kr/hwpml/2011/section\" xmlns:hp=\"http://www.hancom.co.kr/hwpml/2011/paragraph\">{body}</hs:sec>").into_bytes()));
        let original = hae_zip::write(&a, &hae_package::zip_plan(&a, &s).unwrap()).unwrap();
        let input = dir.join("input.hwpx");
        fs::write(&input, &original).unwrap();
        let mut exec = SessionExecutor::new(&dir, &dir, Limits::default()).unwrap();
        q(&mut exec, "document.open", json!({"path":input}));
        Self {
            dir,
            original,
            exec,
        }
    }
    fn exact_original(&mut self) {
        let out = self.dir.join("out.hwpx");
        q(&mut self.exec, "document.export", json!({"path":out}));
        assert!(fs::read(&out).unwrap() == self.original);
        fs::remove_file(out).unwrap();
    }
    fn texts(&mut self) -> Vec<String> {
        q(&mut self.exec, "element.query", json!({"kind":"paragraph"}))["matches"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                q(&mut self.exec, "element.get", json!({"target":p["nodeId"]}))["text"]
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect()
    }
}
impl Drop for Harness {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}
fn q(e: &mut SessionExecutor, c: &str, p: Value) -> Value {
    let r = e.call(c, p, None, false);
    assert!(r.ok, "{c}: {:?}", r.diagnostics);
    r.result.unwrap()
}
fn p(text: &str) -> String {
    format!("<hp:p paraPrIDRef=\"0\"><hp:run charPrIDRef=\"0\"><hp:t>{text}</hp:t></hp:run><hp:linesegarray/></hp:p>")
}

#[test]
fn replace_all_lists_original_matches_dry_runs_and_is_one_undo_unit() {
    let mut h = Harness::new(&(p("aba aba") + &p("aba")));
    let dry = h.exec.call(
        "text.replaceAll",
        json!({"find":"aba","text":"X"}),
        Some(0),
        true,
    );
    assert!(dry.ok, "{:?}", dry.diagnostics);
    let result = dry.result.unwrap();
    assert_eq!(result["replacementCount"], 3);
    assert_eq!(result["matches"].as_array().unwrap().len(), 3);
    assert_eq!(result["afterVersion"], 0);
    h.exact_original();
    let r = h.exec.call(
        "text.replaceAll",
        json!({"find":"aba","text":"X"}),
        Some(0),
        false,
    );
    assert!(r.ok, "{:?}", r.diagnostics);
    assert_eq!(h.texts(), ["X X", "X"]);
    assert!(h.exec.call("history.undo", json!({}), Some(1), false).ok);
    h.exact_original();
    assert!(h.exec.call("history.redo", json!({}), Some(2), false).ok);
    assert_eq!(h.texts(), ["X X", "X"]);
}
#[test]
fn replace_all_greedy_nonoverlap_scope_and_limits_are_atomic() {
    let mut h = Harness::new(&(p("aaaa") + &p("aaaa")));
    let items = q(&mut h.exec, "element.query", json!({"kind":"paragraph"}));
    let id = items["matches"][0]["nodeId"].clone();
    let r = h.exec.call(
        "text.replaceAll",
        json!({"find":"aa","text":"X","maxMatches":1}),
        Some(0),
        false,
    );
    assert!(!r.ok);
    assert_eq!(r.diagnostics[0].code, Code::ResourceLimit);
    h.exact_original();
    let r = h.exec.call(
        "text.replaceAll",
        json!({"find":"aa","text":"X","scope":id}),
        Some(0),
        false,
    );
    assert!(r.ok, "{:?}", r.diagnostics);
    assert_eq!(r.result.unwrap()["replacementCount"], 2);
    assert_eq!(h.texts(), ["XX", "aaaa"]);
}
#[test]
fn replace_all_skips_control_crossing_and_unaligned_graphemes_without_history() {
    for (body, find) in [(p("a<hp:tab/>b"), "a\tb"), (p("é"), "e")] {
        let mut h = Harness::new(&body);
        let r = h.exec.call(
            "text.replaceAll",
            json!({"find":find,"text":"X"}),
            Some(0),
            false,
        );
        assert!(r.ok, "{:?}", r.diagnostics);
        assert_eq!(r.result.as_ref().unwrap()["skippedCount"], 1);
        assert_eq!(r.result.unwrap()["replacementCount"], 0);
        assert!(r
            .diagnostics
            .iter()
            .any(|d| d.code.as_str() == "MATCHES_SKIPPED"));
        h.exact_original();
        assert_eq!(
            h.exec
                .call("history.undo", json!({}), Some(0), false)
                .diagnostics[0]
                .code,
            Code::HistoryEmpty
        );
    }
}
#[test]
fn replace_all_rejects_empty_find_and_rolls_back_a_later_batch_failure() {
    let mut h = Harness::new(&p("word word"));
    let id = q(&mut h.exec, "element.query", json!({"kind":"paragraph"}))["matches"][0]["nodeId"]
        .clone();
    assert!(
        !h.exec
            .call(
                "text.replaceAll",
                json!({"find":"","text":"X"}),
                Some(0),
                false
            )
            .ok
    );
    let r=h.exec.call("command.batch",json!({"steps":[{"command":"text.replaceAll","params":{"find":"word","text":"X"}},{"command":"text.replace","params":{"target":id,"range":{"start":0,"end":99},"text":"bad"}}]}),Some(0),false);
    assert!(!r.ok);
    h.exact_original();
}

fn ids(h: &mut Harness) -> Vec<Value> {
    q(&mut h.exec, "element.query", json!({"kind":"paragraph"}))["matches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["nodeId"].clone())
        .collect()
}
fn run_p(runs: &[(&str, &str)]) -> String {
    let runs: String = runs
        .iter()
        .map(|(c, t)| format!("<hp:run charPrIDRef=\"{c}\"><hp:t>{t}</hp:t></hp:run>"))
        .collect();
    format!("<hp:p paraPrIDRef=\"0\">{runs}<hp:linesegarray/></hp:p>")
}
fn cell_p(inner: &str) -> String {
    format!("<hp:p paraPrIDRef=\"0\"><hp:run charPrIDRef=\"0\"><hp:tbl><hp:tr><hp:tc><hp:subList>{inner}</hp:subList></hp:tc></hp:tr></hp:tbl></hp:run><hp:linesegarray/></hp:p>")
}
fn skipped(r: &Value, reason: &str) -> Value {
    r["skipped"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["reason"] == reason)
        .cloned()
        .unwrap_or(Value::Null)
}

#[test]
fn replace_all_crosses_runs_entities_cells_and_collapses_shapes_in_one_undo_unit() {
    let body = run_p(&[("0", "ab"), ("1", "cd")]) + &p("A&amp;B &amp;") + &cell_p(&p("in cell bc"));
    let mut h = Harness::new(&body);
    let r = h.exec.call(
        "text.replaceAll",
        json!({"find":"bc","text":"<X>"}),
        Some(0),
        false,
    );
    assert!(r.ok, "{:?}", r.diagnostics);
    let result = r.result.unwrap();
    assert_eq!(result["replacementCount"], 2);
    assert_eq!(result["paragraphCount"], 2);
    assert_eq!(result["skippedCount"], 0);
    assert!(r
        .diagnostics
        .iter()
        .any(|d| d.code == Code::FormatCollapsed));
    assert_eq!(h.texts(), ["a<X>d", "A&B &", "\u{fffc}", "in cell <X>"]);
    let r = h.exec.call(
        "text.replaceAll",
        json!({"find":"&","text":"+"}),
        Some(1),
        false,
    );
    assert!(r.ok, "{:?}", r.diagnostics);
    assert_eq!(r.result.unwrap()["replacementCount"], 2);
    assert_eq!(h.texts()[1], "A+B +");
    assert!(h.exec.call("history.undo", json!({}), Some(2), false).ok);
    assert!(h.exec.call("history.undo", json!({}), Some(3), false).ok);
    h.exact_original();
}
#[test]
fn replace_all_reports_each_skip_reason_with_locations_and_edits_the_rest() {
    let body=format!("<hp:p id=\"E\" paraPrIDRef=\"0\"><hp:run charPrIDRef=\"0\"><hp:t>k a</hp:t><hp:equation id=\"EQ\"/><hp:t>b k</hp:t></hp:run><hp:linesegarray/></hp:p>{}{}<hp:p id=\"LOCKED\" paraPrIDRef=\"0\"><hp:run charPrIDRef=\"0\"><hp:t>k</hp:t></hp:run></hp:p><x:ref xmlns:x=\"urn:x\" to=\"LOCKED\"/>",
  p("k a<![CDATA[b]]>c k"),p("e\u{301}"));
    let mut h = Harness::new(&body);
    let obj = h.texts()[0].clone();
    assert!(obj.contains('\u{fffc}'));
    let r = h.exec.call(
        "text.replaceAll",
        json!({"find":format!("a{}b","\u{fffc}"),"text":"Z"}),
        Some(0),
        false,
    );
    assert!(r.ok);
    let v = r.result.unwrap();
    assert_eq!(v["skippedCount"], 1);
    assert_eq!(skipped(&v, "crosses_control")["count"], 1);
    assert_eq!(
        skipped(&v, "crosses_control")["locations"][0]["range"],
        json!({"start":2,"end":5})
    );
    let r = h.exec.call(
        "text.replaceAll",
        json!({"find":"abc","text":"Z"}),
        Some(0),
        false,
    );
    let v = r.result.unwrap();
    assert_eq!(v["replacementCount"], 0);
    assert_eq!(skipped(&v, "unsupported_markup")["count"], 1);
    let r = h.exec.call(
        "text.replaceAll",
        json!({"find":"e","text":"Z"}),
        Some(0),
        false,
    );
    let v = r.result.unwrap();
    assert_eq!(skipped(&v, "grapheme_boundary")["count"], 1);
    let r = h.exec.call(
        "text.replaceAll",
        json!({"find":"k","text":"K"}),
        Some(0),
        false,
    );
    assert!(r.ok, "{:?}", r.diagnostics);
    let v = r.result.unwrap();
    // Two "k" in the first, one in the CDATA paragraph's plain text span; the locked paragraph is skipped.
    assert_eq!(v["replacementCount"], 4);
    assert_eq!(skipped(&v, "paragraph_not_editable")["count"], 1);
    let d = r
        .diagnostics
        .iter()
        .find(|d| d.code.as_str() == "MATCHES_SKIPPED")
        .unwrap();
    assert_eq!(d.detail.as_ref().unwrap()["count"], 1);
    assert_eq!(h.texts()[0], format!("K a{}b K", "\u{fffc}"));
    assert_eq!(h.texts()[3], "k");
    let _ = ids(&mut h);
}
#[test]
fn replace_all_clears_layout_cache_from_the_first_changed_paragraph_and_noops_leave_history_alone()
{
    let body = p("x") + &p("keep") + &p("x") + &cell_p(&p("tail"));
    let mut h = Harness::new(&body);
    let n = h.exec.call(
        "text.replaceAll",
        json!({"find":"missing","text":"Z"}),
        Some(0),
        false,
    );
    assert!(n.ok);
    assert_eq!(n.result.unwrap()["outcome"], "noop");
    h.exact_original();
    assert_eq!(
        h.exec
            .call("history.undo", json!({}), Some(0), false)
            .diagnostics[0]
            .code,
        Code::HistoryEmpty
    );
    let paragraphs = ids(&mut h);
    let r = h.exec.call(
        "text.replaceAll",
        json!({"find":"x","text":"","scope":paragraphs[2]}),
        Some(0),
        false,
    );
    assert!(r.ok, "{:?}", r.diagnostics);
    let v = r.result.unwrap();
    assert_eq!(v["replacementCount"], 1);
    assert_eq!(v["cacheRemovals"]["Contents/section0.xml"], 2);
    assert_eq!(h.texts()[2], "");
    let d = h.exec.call(
        "text.replaceAll",
        json!({"find":"a","text":"AA"}),
        Some(1),
        true,
    );
    assert!(d.ok);
    let v = d.result.unwrap();
    assert_eq!(v["dryRun"], true);
    assert_eq!(v["matches"][0]["before"], "tail");
    assert_eq!(v["matches"][0]["after"], "tAAil");
    assert_eq!(v["matches"].as_array().unwrap().len(), 1);
}
#[test]
fn replace_all_validates_arguments_versions_and_replays() {
    let mut h = Harness::new(&p("word"));
    for params in [
        json!({"find":"w","text":"a\tb"}),
        json!({"find":"w","text":"a\nb"}),
        json!({"find":"w","text":"x","maxMatches":0}),
        json!({"find":"w","text":"x","maxMatches":10001}),
        json!({"find":"w","text":"x","bogus":1}),
        json!({"find":"w","text":"x","scope":"nope"}),
    ] {
        let r = h
            .exec
            .call("text.replaceAll", params.clone(), Some(0), false);
        assert!(!r.ok, "{params}");
    }
    h.exact_original();
    assert_eq!(
        h.exec
            .call(
                "text.replaceAll",
                json!({"find":"w","text":"x"}),
                Some(5),
                false
            )
            .diagnostics[0]
            .code,
        Code::VersionConflict
    );
    let writer = h.exec.default_writer_id().to_string();
    let req = hae_exec::RequestEnvelope {
        schema_version: 1,
        session_id: Some(h.exec.session_id().to_string()),
        writer_id: Some(writer),
        request_sequence: Some(8),
        request_id: None,
        document_id: h.exec.current_document_id().map(str::to_owned),
        expected_version: Some(0),
        command: "text.replaceAll".into(),
        dry_run: false,
        params: json!({"find":"w","text":"x"}),
    };
    let req_template = req.clone();
    let first = h.exec.execute(req.clone());
    assert!(first.ok && !first.replayed, "{:?}", first.diagnostics);
    let again = h.exec.execute(req);
    assert!(again.ok && again.replayed);
    assert_eq!(again.result, first.result);
    assert_eq!(h.texts(), ["xord"]);
    // Replay did not create another history unit: one undo restores the original.
    let mut undo = req_template.clone();
    undo.request_sequence = Some(9);
    undo.command = "history.undo".into();
    undo.expected_version = Some(1);
    undo.params = json!({});
    assert!(h.exec.execute(undo.clone()).ok);
    h.exact_original();
    undo.request_sequence = Some(10);
    undo.expected_version = Some(2);
    assert_eq!(h.exec.execute(undo).diagnostics[0].code, Code::HistoryEmpty);
}

#[test]
fn replace_all_output_equals_the_hand_written_bytes_the_gate_tests_expect() {
    // Same document and expectation as crates/hae-verify/tests/replace_all_gate.rs.
    const CACHE: &str = "<hp:linesegarray/>";
    let pp = |attrs: &str, inner: &str| {
        format!("<hp:p {attrs} paraPrIDRef=\"0\"><hp:run charPrIDRef=\"0\">{inner}</hp:run>{CACHE}</hp:p>")
    };
    let cell = format!(
        "<hp:tbl><hp:tr><hp:tc><hp:subList>{}</hp:subList></hp:tc></hp:tr></hp:tbl>",
        pp("id=\"c\"", "<hp:t>cat in cell</hp:t>")
    );
    let body=[pp("id=\"p0\"","<hp:t>cat dog cat</hp:t>"),
  format!("<hp:p id=\"p1\" paraPrIDRef=\"0\"><hp:run charPrIDRef=\"0\"><hp:t>xca</hp:t></hp:run><hp:run charPrIDRef=\"1\"><hp:t>t y &amp;cat</hp:t></hp:run>{CACHE}</hp:p>"),
  pp("id=\"p2\"","<hp:t>cat<![CDATA[x]]>cat</hp:t>"),pp("id=\"p3\"","<hp:t>cat</hp:t><hp:equation id=\"E\"/><hp:t>cat</hp:t>"),
  format!("<hp:p id=\"p4\" paraPrIDRef=\"0\"><hp:run charPrIDRef=\"0\">{cell}</hp:run>{CACHE}</hp:p>"),
  pp("id=\"p5\"","<hp:t>cat&#x301; cat</hp:t>"),pp("id=\"ref\"","<hp:t>cat</hp:t>")+"<x:ref xmlns:x=\"urn:x\" to=\"ref\"/>",pp("id=\"p7\"","<hp:t>tail cat</hp:t>")].concat();
    let mut want = body.clone();
    for (a, b) in [
        ("cat dog cat", "lion dog lion"),
        ("<hp:t>xca</hp:t>", "<hp:t>xlion</hp:t>"),
        ("<hp:t>t y &amp;cat</hp:t>", "<hp:t> y &amp;lion</hp:t>"),
        ("cat<![CDATA[x]]>cat", "lion<![CDATA[x]]>lion"),
        (
            "<hp:t>cat</hp:t><hp:equation id=\"E\"/><hp:t>cat</hp:t>",
            "<hp:t>lion</hp:t><hp:equation id=\"E\"/><hp:t>lion</hp:t>",
        ),
        ("cat in cell", "lion in cell"),
        ("cat&#x301; cat", "cat&#x301; lion"),
        ("tail cat", "tail lion"),
    ] {
        assert!(want.contains(a), "{a}");
        want = want.replacen(a, b, 1);
    }
    want = want.replace(CACHE, "");
    let mut h = Harness::new(&body);
    let r = h.exec.call(
        "text.replaceAll",
        json!({"find":"cat","text":"lion"}),
        Some(0),
        false,
    );
    assert!(r.ok, "{:?}", r.diagnostics);
    let v = r.result.unwrap();
    assert_eq!(v["replacementCount"], 11);
    assert_eq!(v["skippedCount"], 2);
    let out = h.dir.join("out.hwpx");
    q(&mut h.exec, "document.export", json!({"path":out}));
    let a = hae_zip::Archive::open(fs::read(&out).unwrap(), &Limits::default()).unwrap();
    let section =
        String::from_utf8(a.get("Contents/section0.xml").unwrap().payload().to_vec()).unwrap();
    let expected=format!("<hs:sec xmlns:hs=\"http://www.hancom.co.kr/hwpml/2011/section\" xmlns:hp=\"http://www.hancom.co.kr/hwpml/2011/paragraph\">{want}</hs:sec>");
    assert_eq!(section, expected);
}

#[test]
fn replace_all_exports_pass_the_independent_python_oracle() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = std::env::temp_dir().join(hae_model::new_id("hae-replace-all-oracle-").unwrap());
    fs::create_dir(&dir).unwrap();
    let st = std::process::Command::new("python3")
        .arg("-I")
        .arg(root.join("tests/oracle/fixtures_v02.py"))
        .arg(&dir)
        .status()
        .unwrap();
    assert!(st.success());
    for kind in ["replace_all", "replace_all_scope"] {
        let request: Value =
            serde_json::from_slice(&fs::read(dir.join(format!("{kind}.request.json"))).unwrap())
                .unwrap();
        let op = &request["operations"][0];
        let mut exec = SessionExecutor::new(&dir, &dir, Limits::default()).unwrap();
        q(
            &mut exec,
            "document.open",
            json!({"path":dir.join(format!("{kind}.original.hwpx"))}),
        );
        let mut params = json!({"find":op["find"],"text":op["text"]});
        if op.get("scope").is_some() {
            let all = q(&mut exec, "element.query", json!({"kind":"paragraph"}));
            params["scope"] = all["matches"][4]["nodeId"].clone();
        }
        let r = exec.call("text.replaceAll", params, Some(0), false);
        assert!(r.ok, "{kind}: {:?}", r.diagnostics);
        let out = dir.join(format!("{kind}.engine.hwpx"));
        q(&mut exec, "document.export", json!({"path":out}));
        let intents = r.result.unwrap()["intents"].clone();
        let mut checked = request.clone();
        checked["intents"] = intents;
        let req = dir.join(format!("{kind}.checked.json"));
        fs::write(&req, serde_json::to_vec(&checked).unwrap()).unwrap();
        let o = std::process::Command::new("python3")
            .arg("-I")
            .arg(root.join("tests/oracle/oracle.py"))
            .arg(dir.join(format!("{kind}.original.hwpx")))
            .arg(&out)
            .arg(&req)
            .output()
            .unwrap();
        assert!(
            o.status.success(),
            "{kind}: oracle rejected engine output: {}",
            String::from_utf8_lossy(&o.stdout)
        );
    }
    fs::remove_dir_all(dir).unwrap();
}
