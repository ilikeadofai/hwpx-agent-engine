use hae_diag::{Code, Limits};
use hae_exec::{ResponseEnvelope, SessionExecutor};
use serde_json::{json, Value};
use std::{fs, path::PathBuf};
const INPUT: &[u8] = include_bytes!("../../../tests/fixtures/editing/equations.hwpx");
struct Harness {
    dir: PathBuf,
    exec: SessionExecutor,
}
impl Harness {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(hae_model::new_id("equations-").unwrap());
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("in.hwpx"), INPUT).unwrap();
        let mut exec = SessionExecutor::new(&dir, &dir, Limits::default()).unwrap();
        q(
            &mut exec,
            "document.open",
            json!({"path":dir.join("in.hwpx")}),
        );
        Self { dir, exec }
    }
    fn list(&mut self) -> Value {
        q(&mut self.exec, "equation.list", json!({}))
    }
    fn id(&mut self, i: usize) -> Value {
        self.list()["items"][i]["nodeId"].clone()
    }
    fn edit(&mut self, c: &str, p: Value, v: u64, dry: bool) -> ResponseEnvelope {
        self.exec.call(c, p, Some(v), dry)
    }
    fn bytes(&mut self) -> Vec<u8> {
        let path = self
            .dir
            .join(hae_model::new_id("export-").unwrap() + ".hwpx");
        q(&mut self.exec, "document.export", json!({"path":path}));
        fs::read(path).unwrap()
    }
    fn exact(&mut self) {
        assert_eq!(self.bytes(), INPUT);
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
fn ok(r: ResponseEnvelope) -> Value {
    assert!(r.ok, "{:?}", r.diagnostics);
    r.result.unwrap()
}
#[test]
fn equation_list_returns_all_raw_scripts_and_properties() {
    let mut h = Harness::new();
    let r = h.list();
    let a = r["items"].as_array().unwrap();
    assert_eq!(a.len(), 3);
    assert_eq!(a[0]["id"], "100");
    assert_eq!(a[0]["baseLine"], "66");
    assert_eq!(a[0]["baseUnit"], "1000");
    assert_eq!(a[0]["size"]["width"], "5675");
    assert_eq!(a[0]["size"]["height"], "2250");
    assert_eq!(a[0]["font"], "HancomEQN");
    assert_eq!(a[0]["lineMode"], "CHAR");
    assert_eq!(a[0]["version"], "Equation Version 60");
    assert_eq!(a[0]["textWrap"], "TOP_AND_BOTTOM");
    assert_eq!(a[0]["textFlow"], "BOTH_SIDES");
    assert_eq!(a[0]["treatAsChar"], "1");
    assert_eq!(a[0]["textColor"], "#000000");
    assert_eq!(a[0]["shapeComment"], "수식입니다. & original");
    assert_eq!(a[2]["script"], "a < b & c > d + α");
    assert_eq!(a[2]["scriptRaw"], "a &lt; b &amp; c &gt; d + &#x3B1;");
    h.exact();
}
#[test]
fn equation_list_includes_empty_script_and_empty_scope() {
    let mut h = Harness::new();
    assert_eq!(h.list()["items"][1]["script"], "");
    let p = q(&mut h.exec, "element.query", json!({"kind":"paragraph"}))["matches"][3]["nodeId"]
        .clone();
    assert_eq!(
        q(&mut h.exec, "equation.list", json!({"scope":p}))["items"],
        json!([])
    );
}
#[test]
fn equation_list_scope_paragraph_section_and_self() {
    let mut h = Harness::new();
    let r = h.list();
    let eq = &r["items"][1];
    for scope in [eq["nodeId"].clone(), eq["paragraphNodeId"].clone()] {
        assert_eq!(
            q(&mut h.exec, "equation.list", json!({"scope":scope}))["items"][0]["id"],
            "101"
        );
    }
    let s =
        q(&mut h.exec, "element.query", json!({"kind":"section"}))["matches"][0]["nodeId"].clone();
    assert_eq!(
        q(&mut h.exec, "equation.list", json!({"scope":s}))["items"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
}
#[test]
fn equation_list_pages_and_rejects_bad_or_wrong_scope_cursors() {
    let mut h = Harness::new();
    let mut c = Value::Null;
    let mut ids = vec![];
    loop {
        let p = if c.is_null() {
            json!({"limit":1})
        } else {
            json!({"limit":1,"cursor":c})
        };
        let r = q(&mut h.exec, "equation.list", p);
        ids.push(r["items"][0]["id"].clone());
        c = r["nextCursor"].clone();
        if c.is_null() {
            break;
        }
    }
    assert_eq!(
        ids,
        json!(["100", "101", "102"]).as_array().unwrap().clone()
    );
    let r = q(&mut h.exec, "equation.list", json!({"limit":1}));
    let id = h.id(0);
    for p in [
        json!({"cursor":"garbage"}),
        json!({"limit":0}),
        json!({"limit":501}),
        json!({"cursor":r["nextCursor"],"scope":id}),
        json!({"scope":"missing"}),
    ] {
        assert!(!h.exec.call("equation.list", p, None, false).ok);
    }
}
#[test]
fn equation_list_cursor_invalidates_after_edit_and_undo() {
    let mut h = Harness::new();
    let r = q(&mut h.exec, "equation.list", json!({"limit":1}));
    let id = h.id(0);
    ok(h.edit(
        "equation.setScript",
        json!({"target":id,"script":"x+1"}),
        0,
        false,
    ));
    assert_eq!(
        h.exec
            .call(
                "equation.list",
                json!({"cursor":r["nextCursor"]}),
                None,
                false
            )
            .diagnostics[0]
            .code,
        Code::VersionConflict
    );
    ok(h.edit("history.undo", json!({}), 1, false));
    assert!(
        !h.exec
            .call(
                "equation.list",
                json!({"cursor":r["nextCursor"]}),
                None,
                false
            )
            .ok
    );
    h.exact();
}
#[test]
fn equation_get_has_containing_paragraph_preview_and_original_source() {
    let mut h = Harness::new();
    let id = h.id(2);
    let e = q(&mut h.exec, "equation.get", json!({"target":id}))["equation"].clone();
    assert_eq!(e["script"], "a < b & c > d + α");
    assert!(e["paragraphTextPreview"]
        .as_str()
        .unwrap()
        .contains("Equation 3:"));
    assert!(!e["paragraphNodeId"].is_null());
    h.exact();
}
#[test]
fn equation_get_empty_script_and_wrong_targets() {
    let mut h = Harness::new();
    let id = h.id(1);
    assert_eq!(
        q(&mut h.exec, "equation.get", json!({"target":id}))["equation"]["scriptRaw"],
        ""
    );
    let p = h.list()["items"][0]["paragraphNodeId"].clone();
    for target in [p, json!("missing")] {
        assert!(
            !h.exec
                .call("equation.get", json!({"target":target}), None, false)
                .ok
        );
    }
    h.exact();
}
#[test]
fn equation_set_script_xml_escapes_and_preserves_properties_and_ids() {
    let mut h = Harness::new();
    let before = h.list();
    let id = h.id(0);
    let r = ok(h.edit(
        "equation.setScript",
        json!({"target":id,"script":"x < y & z > α"}),
        0,
        false,
    ));
    assert_eq!(r["cacheRemovals"]["Contents/section0.xml"], 4);
    let after = h.list();
    for key in [
        "nodeId",
        "paragraphNodeId",
        "id",
        "size",
        "font",
        "textColor",
        "baseLine",
        "baseUnit",
        "version",
        "lineMode",
        "shapeComment",
        "textWrap",
        "textFlow",
        "treatAsChar",
    ] {
        assert_eq!(before["items"][0][key], after["items"][0][key], "{key}");
    }
    assert_eq!(after["items"][0]["scriptRaw"], "x &lt; y &amp; z &gt; α");
    assert_eq!(before["items"][1], after["items"][1]);
    assert_eq!(before["items"][2], after["items"][2]);
    let bytes = h.bytes();
    let a = hae_zip::Archive::open(INPUT.to_vec(), &Limits::default()).unwrap();
    let b = hae_zip::Archive::open(bytes, &Limits::default()).unwrap();
    for e in a.entries() {
        if e.name != "Contents/section0.xml" {
            assert_eq!(a.raw_data(&e.name), b.raw_data(&e.name));
            assert_eq!(a.raw_local_header(&e.name), b.raw_local_header(&e.name));
        }
    }
}
#[test]
fn equation_set_script_fills_empty_script() {
    let mut h = Harness::new();
    let id = h.id(1);
    ok(h.edit(
        "equation.setScript",
        json!({"target":id,"script":"{x} over {2}"}),
        0,
        false,
    ));
    assert_eq!(h.list()["items"][1]["script"], "{x} over {2}");
}
#[test]
fn equation_set_script_rejects_empty_invalid_xml_and_unverified_notation() {
    let mut h = Harness::new();
    let id = h.id(0);
    for script in [
        "",
        "  ",
        "x\0y",
        "x\u{b}y",
        "\\frac{1}{2}",
        "\"quoted\"",
        "{a",
        "a}",
        "<![CDATA[x]]>",
        "<!--x-->",
        "<?pi?>",
        "x\ny",
    ] {
        assert!(
            !h.edit(
                "equation.setScript",
                json!({"target":id,"script":script}),
                0,
                false
            )
            .ok,
            "{script:?}"
        );
    }
    h.exact();
}
#[test]
fn equation_set_script_dry_run_and_undo_restore_original_bytes() {
    let mut h = Harness::new();
    let id = h.id(0);
    ok(h.edit(
        "equation.setScript",
        json!({"target":id,"script":"x^2"}),
        0,
        true,
    ));
    h.exact();
    ok(h.edit(
        "equation.setScript",
        json!({"target":id,"script":"x^2"}),
        0,
        false,
    ));
    ok(h.edit("history.undo", json!({}), 1, false));
    h.exact();
    ok(h.edit("history.redo", json!({}), 2, false));
    assert_eq!(h.list()["items"][0]["script"], "x^2");
}
#[test]
fn equation_set_properties_changes_all_six_only_and_keeps_raw_script() {
    let mut h = Harness::new();
    let before = h.list();
    let id = h.id(2);
    ok(h.edit("equation.setProperties",json!({"target":id,"props":{"baseLine":72,"textColor":"#aB12Cd","font":"HancomEQN & custom","baseUnit":1200,"lineMode":"LINE","version":"Equation Version 60"}}),0,false));
    let after = h.list();
    let e = &after["items"][2];
    assert_eq!(e["baseLine"], "72");
    assert_eq!(e["textColor"], "#aB12Cd");
    assert_eq!(e["font"], "HancomEQN & custom");
    assert_eq!(e["baseUnit"], "1200");
    assert_eq!(e["lineMode"], "LINE");
    for key in [
        "script",
        "scriptRaw",
        "size",
        "shapeComment",
        "textWrap",
        "textFlow",
        "treatAsChar",
    ] {
        assert_eq!(before["items"][2][key], e[key]);
    }
    assert_eq!(before["items"][0], after["items"][0]);
}
#[test]
fn equation_set_properties_rejects_empty_bad_color_unknown_props_and_controls() {
    let mut h = Harness::new();
    let id = h.id(0);
    for props in [
        json!({}),
        json!({"textColor":"red"}),
        json!({"textColor":"#12345g"}),
        json!({"font":""}),
        json!({"baseUnit":0}),
        json!({"baseLine":-1}),
        json!({"version":"\0"}),
        json!({"font":"a\nb"}),
        json!({"width":9}),
        json!({"lineMode":"INVALID"}),
    ] {
        assert!(
            !h.edit(
                "equation.setProperties",
                json!({"target":id,"props":props}),
                0,
                false
            )
            .ok,
            "{props}"
        );
    }
    h.exact();
}
#[test]
fn equation_set_properties_dry_run_and_undo_restore_original_bytes() {
    let mut h = Harness::new();
    let id = h.id(1);
    let p = json!({"target":id,"props":{"baseLine":70}});
    ok(h.edit("equation.setProperties", p.clone(), 0, true));
    h.exact();
    ok(h.edit("equation.setProperties", p, 0, false));
    assert_eq!(h.list()["items"][1]["script"], "");
    ok(h.edit("history.undo", json!({}), 1, false));
    h.exact();
}
#[test]
fn equation_batch_script_properties_repeated_target_is_one_undo_unit() {
    let mut h = Harness::new();
    let id = h.id(0);
    let other = h.id(2);
    let p = json!({"steps":[{"command":"equation.setScript","params":{"target":id,"script":"x+1"}},{"command":"equation.setProperties","params":{"target":id,"props":{"textColor":"#123456"}}},{"command":"equation.setScript","params":{"target":id,"script":"x+2"}},{"command":"equation.setProperties","params":{"target":other,"props":{"baseUnit":1500}}}]});
    ok(h.edit("command.batch", p.clone(), 0, true));
    h.exact();
    let r = ok(h.edit("command.batch", p, 0, false));
    assert_eq!(r["afterVersion"], 1);
    assert_eq!(h.list()["items"][0]["script"], "x+2");
    ok(h.edit("history.undo", json!({}), 1, false));
    h.exact();
    assert_eq!(
        h.edit("history.undo", json!({}), 2, false).diagnostics[0].code,
        Code::HistoryEmpty
    );
}
#[test]
fn equation_batch_later_rejection_rolls_back_and_wrong_targets_fail() {
    let mut h = Harness::new();
    let id = h.id(0);
    let p = h.list()["items"][0]["paragraphNodeId"].clone();
    for c in ["equation.setScript", "equation.setProperties"] {
        for target in [p.clone(), json!("missing")] {
            assert!(
                !h.edit(
                    c,
                    json!({"target":target,"script":"x","props":{"font":"x"}}),
                    0,
                    false
                )
                .ok
            );
        }
    }
    let r=h.edit("command.batch",json!({"steps":[{"command":"equation.setScript","params":{"target":id,"script":"x"}},{"command":"equation.setProperties","params":{"target":id,"props":{}}}]}),0,false);
    assert!(!r.ok);
    h.exact();
}
#[test]
fn equation_noops_preserve_entity_spelling_and_history() {
    let mut h = Harness::new();
    let id = h.id(2);
    let script = h.list()["items"][2]["script"].clone();
    assert_eq!(
        ok(h.edit(
            "equation.setScript",
            json!({"target":id,"script":script}),
            0,
            false
        ))["outcome"],
        "noop"
    );
    assert_eq!(
        ok(h.edit(
            "equation.setProperties",
            json!({"target":id,"props":{"baseLine":66}}),
            0,
            false
        ))["outcome"],
        "noop"
    );
    h.exact();
    assert_eq!(
        h.edit("history.undo", json!({}), 0, false).diagnostics[0].code,
        Code::HistoryEmpty
    );
}
#[test]
fn equation_versions_and_registry_discovery_are_consistent() {
    let mut h = Harness::new();
    let id = h.id(0);
    let r = q(&mut h.exec, "element.get", json!({"target":id}));
    assert_eq!(
        r["outline"]["edits"]["equation.setScript"]["available"],
        true
    );
    assert_eq!(
        h.edit(
            "equation.setScript",
            json!({"target":id,"script":"x"}),
            99,
            false
        )
        .diagnostics[0]
            .code,
        Code::VersionConflict
    );
    for c in [
        "equation.list",
        "equation.get",
        "equation.setScript",
        "equation.setProperties",
    ] {
        let r = q(&mut h.exec, "command.describe", json!({"command":c}));
        assert_eq!(r["available"], true);
    }
    h.exact();
}
