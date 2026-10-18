//! End-to-end queries through session dispatch and real document.export before/after.
use hae_diag::{Code, Limits};
use hae_exec::{command_describe, CommandCategory, SessionExecutor};
use serde_json::{json, Value};
use std::{fs, path::PathBuf, sync::Arc};

const HEADER: &str = r##"<hh:head xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head"><hh:refList>
<hh:fontfaces><hh:fontface lang="HANGUL"><hh:font id="0" face="Test Font"/></hh:fontface></hh:fontfaces>
<hh:styles itemCnt="1"><hh:style id="0" name="Normal" paraPrIDRef="0" charPrIDRef="0" nextStyleIDRef="0"/></hh:styles>
<hh:charProperties itemCnt="1"><hh:charPr id="0" height="1200" textColor="#123456" borderFillIDRef="1"><hh:fontRef hangul="0" latin="99"/><hh:bold/><hh:italic/></hh:charPr></hh:charProperties>
<hh:paraProperties itemCnt="1"><hh:paraPr id="0"><hh:align horizontal="CENTER"/><hh:lineSpacing type="PERCENT" value="160"/><hh:border borderFillIDRef="1"/></hh:paraPr></hh:paraProperties>
<hh:borderFills itemCnt="1"><hh:borderFill id="1"><hh:leftBorder type="SOLID"/><hh:rightBorder type="DOUBLE_SLIM"/></hh:borderFill></hh:borderFills>
</hh:refList></hh:head>"##;

fn p(inner: &str) -> String {
    format!(
        r#"<hp:p paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0">{inner}</hp:run></hp:p>"#
    )
}
fn cell(row: u32, col: u32, rs: u32, cs: u32, text: &str) -> String {
    format!(
        r#"<hp:tc borderFillIDRef="1" hasMargin="1"><hp:cellAddr rowAddr="{row}" colAddr="{col}"/><hp:cellSpan rowSpan="{rs}" colSpan="{cs}"/><hp:cellSz width="1234" height="567"/><hp:cellMargin left="10" right="20" top="30" bottom="40"/><hp:subList>{}</hp:subList></hp:tc>"#,
        p(&format!("<hp:t>{text}</hp:t>"))
    )
}
fn table(cells: &str) -> String {
    format!(
        r#"<hp:tbl rowCnt="2" colCnt="2" cellSpacing="12" borderFillIDRef="1"><hp:outMargin left="1" right="2" top="3" bottom="4"/><hp:inMargin left="5"/><hp:tr>{cells}</hp:tr></hp:tbl>"#
    )
}
fn pic(id: &str, extra: &str) -> String {
    format!(
        r#"<hp:pic reverse="0"><hp:pos horzOffset="17" vertOffset="-23" horzRelTo="PARA"/><hp:offset x="7" y="8"/><hp:sz width="100" height="200"/><hp:orgSz width="300" height="400"/><hp:curSz width="90" height="180"/><hp:imgClip left="0" top="0" right="300" bottom="400"/><hp:imgDim dimwidth="300" dimheight="400"/><hp:flip horizontal="0" vertical="0"/><hc:img binaryItemIDRef="{id}" effect="REAL_PIC" bright="0" contrast="0"/>{extra}</hp:pic>"#
    )
}
#[derive(Debug, PartialEq)]
enum ExportSnapshot {
    Bytes(Vec<u8>),
    Rejected(Value),
}
struct Harness {
    dir: PathBuf,
    exec: SessionExecutor,
    exports: usize,
}
impl Harness {
    fn new(body: &str) -> Self {
        Self::with_header(body, HEADER, Limits::default())
    }
    fn with_header(body: &str, header: &str, limits: Limits) -> Self {
        let a = hae_zip::Archive::open(
            include_bytes!("../../../tests/fixtures/editing/split_runs.hwpx").to_vec(),
            &Limits::default(),
        )
        .unwrap();
        let mut state = hae_package::PackageState::from_archive(&a);
        state.set("Contents/header.xml", Arc::from(header.as_bytes()));
        state.set("Contents/section0.xml",Arc::from(format!(r#"<hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" xmlns:hc="http://www.hancom.co.kr/hwpml/2011/core">{body}</hs:sec>"#).into_bytes()));
        let hpf = String::from_utf8(state.get("Contents/content.hpf").unwrap().to_vec()).unwrap().replace("</opf:manifest>",r#"<opf:item id="img0" href="BinData/p.png" media-type="image/png" isEmbeded="1"/><opf:item id="alias" href="BinData/p.png" media-type="image/png" isEmbeded="1"/><opf:item id="ext0" href="https://example.invalid/p.png" media-type="image/png" isEmbeded="0"/></opf:manifest>"#);
        state.set("Contents/content.hpf", Arc::from(hpf.into_bytes()));
        state.add(
            "BinData/p.png".into(),
            Arc::from(include_bytes!("../../../tests/fixtures/editing/inputs/ok.png").as_slice()),
            0,
        );
        let bytes = hae_zip::write(&a, &hae_package::zip_plan(&a, &state).unwrap()).unwrap();
        Self::from_bytes(&bytes, limits)
    }
    fn from_bytes(bytes: &[u8], limits: Limits) -> Self {
        let dir = std::env::temp_dir().join(hae_model::new_id("queries2-").unwrap());
        fs::create_dir(&dir).unwrap();
        let path = dir.join("input.hwpx");
        fs::write(&path, bytes).unwrap();
        let mut exec = SessionExecutor::new(&dir, &dir, limits).unwrap();
        let r = exec.call("document.open", json!({"path":path}), None, false);
        assert!(r.ok, "{:?}", r.diagnostics);
        let mut h = Self {
            dir,
            exec,
            exports: 0,
        };
        if let ExportSnapshot::Bytes(exported) = h.export() {
            assert_eq!(exported, bytes);
        } else {
            // A pre-existing unresolved resource makes the independent save
            // gate reject even an untouched package. Do not weaken that gate.
            let doc = hae_model::Document::open(bytes.to_vec(), &Limits::default()).unwrap();
            let before = doc.to_hwpx().unwrap();
            let _ = doc.style_inventory(None);
            assert_eq!(doc.to_hwpx().unwrap(), before);
        }
        h
    }
    fn export(&mut self) -> ExportSnapshot {
        self.exports += 1;
        let path = self.dir.join(format!("export-{}.hwpx", self.exports));
        let r = self
            .exec
            .call("document.export", json!({"path":path}), None, false);
        if !r.ok {
            assert_eq!(
                r.diagnostics[0].code,
                Code::VerifyFailed,
                "{:?}",
                r.diagnostics
            );
            assert!(!path.exists());
            return ExportSnapshot::Rejected(serde_json::to_value(r.diagnostics).unwrap());
        }
        let bytes = fs::read(&path).unwrap();
        fs::remove_file(path).unwrap();
        ExportSnapshot::Bytes(bytes)
    }
    fn call(&mut self, command: &str, params: Value) -> hae_exec::ResponseEnvelope {
        let before = self.export();
        let diff_before = matches!(before, ExportSnapshot::Rejected(_)).then(|| {
            self.exec
                .call("document.diff", json!({}), None, false)
                .result
        });
        let r = self.exec.call(command, params, None, false);
        if let Some(diff_before) = diff_before {
            assert_eq!(
                self.exec
                    .call("document.diff", json!({}), None, false)
                    .result,
                diff_before,
                "query changed semantic diff"
            );
        }
        assert_eq!(self.export(), before, "{command} changed export bytes");
        r
    }
    fn query(&mut self, command: &str, params: Value) -> Value {
        let r = self.call(command, params);
        assert!(r.ok, "{command}: {:?}", r.diagnostics);
        r.result.unwrap()
    }
    fn id(&mut self, kind: &str, i: usize) -> Value {
        self.query("element.query", json!({"kind":kind}))["matches"][i]["nodeId"].clone()
    }
}
impl Drop for Harness {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}
fn items(v: &Value) -> &[Value] {
    v["items"].as_array().unwrap()
}
fn resource(v: &Value, kind: &str) -> Value {
    items(v).iter().find(|i| i["kind"] == kind).unwrap().clone()
}

#[test]
fn style_list_properties_fonts_and_literal_reference_counts() {
    let mut h = Harness::new(&p("<hp:t>hello</hp:t>"));
    let v = h.query("style.list", json!({}));
    assert_eq!(v["total"], 4);
    for k in ["style", "charPr", "paraPr"] {
        assert_eq!(resource(&v, k)["useCount"], 2);
        assert_eq!(resource(&v, k)["bodyReferenceCount"], 1);
    }
    let c = resource(&v, "charPr");
    assert_eq!(c["attributes"]["height"], "1200");
    assert_eq!(c["attributes"]["textColor"], "#123456");
    assert_eq!(c["bold"], true);
    assert_eq!(c["italic"], true);
    assert_eq!(c["fonts"][0]["face"], "Test Font");
    assert!(c["fonts"][1]["face"].is_null());
    let s = resource(&v, "style");
    assert_eq!(s["attributes"]["name"], "Normal");
    assert_eq!(s["attributes"]["paraPrIDRef"], "0");
    assert_eq!(s["attributes"]["charPrIDRef"], "0");
    assert_eq!(
        resource(&v, "paraPr")["alignments"][0]["horizontal"],
        "CENTER"
    );
    assert_eq!(resource(&v, "paraPr")["lineSpacings"][0]["value"], "160");
    let b = resource(&v, "borderFill");
    assert_eq!(b["useCount"], 2);
    assert_eq!(b["borders"]["rightBorder"]["type"], "DOUBLE_SLIM");
    assert_eq!(v["unresolvedReferenceCount"], 0);
}
#[test]
fn style_list_empty_header_and_body_preserve_export_bytes() {
    let mut h = Harness::with_header(
        "",
        r#"<hh:head xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head"/>"#,
        Limits::default(),
    );
    let v = h.query("style.list", json!({}));
    assert_eq!(v["total"], 0);
    assert!(items(&v).is_empty());
    assert_eq!(v["unresolvedReferenceCount"], 0);
    assert!(v["nextCursor"].is_null());
}

#[test]
fn style_list_empty_resources_still_reports_body_defects() {
    let mut h = Harness::with_header(
        &p(""),
        r#"<hh:head xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head"/>"#,
        Limits::default(),
    );
    let v = h.query("style.list", json!({}));
    assert!(items(&v).is_empty());
    assert_eq!(v["unresolvedReferenceCount"], 3);
    assert_eq!(v["unresolvedBodyReferenceCount"], 3);
    assert!(!v["unresolvedExamples"][0]["nodeId"].is_null());
}
#[test]
fn style_list_unresolved_totals_and_examples_are_filter_independent_and_bounded() {
    let body = (0..30)
        .map(|_| p("<hp:t>x</hp:t>").replace("charPrIDRef=\"0\"", "charPrIDRef=\"999\""))
        .collect::<String>();
    let mut h = Harness::new(&body);
    let v = h.query("style.list", json!({"kind":"style","limit":1}));
    assert_eq!(v["total"], 1);
    assert_eq!(v["unresolvedReferenceCount"], 30);
    assert_eq!(v["unresolvedBodyReferenceCount"], 30);
    assert_eq!(v["unresolvedExamples"].as_array().unwrap().len(), 20);
    assert_eq!(v["unresolvedExamplesTruncated"], true);
    assert_eq!(v["unresolvedExamples"][0]["kind"], "charPr");
    assert_eq!(v["unresolvedExamples"][0]["id"], "999");
}
#[test]
fn style_list_body_defect_examples_are_not_hidden_by_header_defects() {
    let definitions = (1..=30)
        .map(|i| format!("<hh:style id=\"{i}\" charPrIDRef=\"88\"/>"))
        .collect::<String>();
    let header = HEADER.replace("</hh:styles>", &format!("{definitions}</hh:styles>"));
    let body = p("<hp:t>x</hp:t>").replace("charPrIDRef=\"0\"", "charPrIDRef=\"999\"");
    let mut h = Harness::with_header(&body, &header, Limits::default());
    let v = h.query("style.list", json!({"kind":"borderFill"}));
    assert_eq!(v["unresolvedReferenceCount"], 31);
    assert_eq!(v["unresolvedBodyReferenceCount"], 1);
    assert_eq!(v["unresolvedExamples"][0]["id"], "999");
    assert_eq!(v["unresolvedExamples"][0]["entry"], "Contents/section0.xml");
}

#[test]
fn style_list_pages_without_gaps_and_validates_kind_limit_cursor() {
    let mut h = Harness::new("");
    let mut cursor = Value::Null;
    let mut all = Vec::new();
    loop {
        let mut args = json!({"limit":1});
        if !cursor.is_null() {
            args["cursor"] = cursor;
        }
        let v = h.query("style.list", args);
        all.extend_from_slice(items(&v));
        cursor = v["nextCursor"].clone();
        if cursor.is_null() {
            break;
        }
    }
    assert_eq!(all.len(), 4);
    for args in [
        json!({"kind":"font"}),
        json!({"limit":0}),
        json!({"limit":501}),
        json!({"cursor":"oops"}),
        json!({"bogus":true}),
    ] {
        assert!(!h.call("style.list", args).ok);
    }
    let page = h.query("style.list", json!({"limit":1}));
    assert_eq!(
        h.call(
            "style.list",
            json!({"kind":"charPr","cursor":page["nextCursor"]})
        )
        .diagnostics[0]
            .code,
        Code::VersionConflict
    );
}
#[test]
fn style_list_preserves_all_alternative_line_spacing_branches() {
    let header=HEADER.replace("<hh:lineSpacing type=\"PERCENT\" value=\"160\"/>",r#"<hp:switch xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph"><hp:case><hh:lineSpacing type="PERCENT" value="160"/></hp:case><hp:default><hh:lineSpacing type="FIXED" value="1200"/></hp:default></hp:switch>"#);
    let mut h = Harness::with_header("", &header, Limits::default());
    let v = h.query("style.list", json!({"kind":"paraPr"}));
    assert_eq!(items(&v)[0]["lineSpacings"].as_array().unwrap().len(), 2);
    assert_eq!(items(&v)[0]["lineSpacings"][1]["type"], "FIXED");
}
#[test]
fn table_get_geometry_margins_paragraph_counts_and_previews() {
    let mut h = Harness::new(&p(&table(&cell(0, 0, 1, 1, "cell text"))));
    let v = h.query("table.get", json!({}));
    let t = &items(&v)[0];
    assert_eq!(t["rows"], 2);
    assert_eq!(t["cols"], 2);
    assert_eq!(t["cellSpacing"], 12);
    assert_eq!(t["outerMargins"]["bottom"], "4");
    assert_eq!(t["innerMargins"]["left"], "5");
    let c = &t["cells"][0];
    assert_eq!(
        c["address"],
        json!({"row":0,"col":0,"rowSpan":1,"colSpan":1})
    );
    assert_eq!(c["size"]["width"], "1234");
    assert_eq!(c["margins"]["bottom"], "40");
    assert_eq!(c["paragraphCount"], 1);
    assert_eq!(c["textPreview"], "cell text");
    assert_eq!(c["borderFillIdRef"], "1");
    assert_eq!(c["hasMargin"], "1");
}
#[test]
fn table_get_empty_scope_and_single_table_selection() {
    let mut h = Harness::new(
        &(p(&table(&cell(0, 0, 1, 1, "a")))
            + &p(&table(&cell(0, 0, 1, 1, "b")))
            + &p("<hp:t>empty</hp:t>")),
    );
    let s = h.id("section", 0);
    assert_eq!(h.query("table.get", json!({"scope":s}))["total"], 2);
    let para = h.id("paragraph", 0);
    assert_eq!(h.query("table.get", json!({"scope":para}))["total"], 1);
    let empty = h.id("paragraph", 4);
    assert_eq!(h.query("table.get", json!({"scope":empty}))["total"], 0);
    let id = h.id("table", 1);
    let v = h.query("table.get", json!({"table":id}));
    assert_eq!(v["total"], 1);
    assert_eq!(items(&v)[0]["cells"][0]["textPreview"], "b");
    assert!(!h.call("table.get", json!({"table":id,"scope":para})).ok);
    assert!(!h.call("table.get", json!({"table":para})).ok);
    assert!(!h.call("table.get", json!({"scope":id})).ok);
    assert_eq!(
        h.call("table.get", json!({"table":"missing"})).diagnostics[0].code,
        Code::TargetNotFound
    );
}
#[test]
fn table_get_no_tables_and_invalid_scope_on_empty_document() {
    let mut h = Harness::new("");
    assert_eq!(h.query("table.get", json!({}))["total"], 0);
    assert!(!h.call("table.get", json!({"scope":"missing"})).ok);
}
#[test]
fn table_get_merges_report_covered_coordinates_and_overlapping_real_cells() {
    let mut h = Harness::new(&p(&table(
        &(cell(0, 0, 2, 2, "anchor") + &cell(0, 1, 1, 1, "overlap")),
    )));
    let v = h.query("table.get", json!({}));
    let t = &items(&v)[0];
    assert_eq!(t["cells"].as_array().unwrap().len(), 2);
    assert_eq!(t["coveredCells"].as_array().unwrap().len(), 3);
    let anchor = t["cells"][0]["nodeId"].clone();
    assert_eq!(t["cells"][1]["coveredBy"], json!([anchor]));
    assert_eq!(t["coveredCells"][0]["row"], 0);
    assert_eq!(t["coveredCells"][0]["col"], 1);
    assert_eq!(
        t["coveredCells"][0]["presentCellNodeIds"],
        json!([t["cells"][1]["nodeId"]])
    );
    assert!(t["coveredCells"][1]["presentCellNodeIds"]
        .as_array()
        .unwrap()
        .is_empty());
}
#[test]
fn table_get_missing_spans_are_null_and_nested_cells_are_not_borrowed() {
    let nested = table(&cell(0, 0, 1, 1, "nested"));
    let outer = format!(
        "<hp:tbl><hp:tr><hp:tc><hp:subList>{}</hp:subList></hp:tc></hp:tr></hp:tbl>",
        p(&nested)
    );
    let mut h = Harness::new(&p(&outer));
    let v = h.query("table.get", json!({}));
    assert_eq!(v["total"], 2);
    let t = &items(&v)[0];
    assert_eq!(t["cells"].as_array().unwrap().len(), 1);
    assert!(t["rows"].is_null());
    assert!(t["cells"][0]["address"]["rowSpan"].is_null());
    assert!(t["coveredCells"].as_array().unwrap().is_empty());
    assert_eq!(t["cells"][0]["paragraphCount"], 1);
}
#[test]
fn table_get_pagination_and_huge_spans_fail_without_allocating_a_grid() {
    let mut h =
        Harness::new(&(p(&table(&cell(0, 0, 1, 1, "a"))) + &p(&table(&cell(0, 0, 1, 1, "b")))));
    let a = h.query("table.get", json!({"limit":1}));
    let b = h.query("table.get", json!({"limit":1,"cursor":a["nextCursor"]}));
    assert_eq!(items(&b)[0]["cells"][0]["textPreview"], "b");
    assert!(b["nextCursor"].is_null());
    let mut h = Harness::new(&p(&table(&cell(0, 0, u32::MAX, u32::MAX, "huge"))));
    assert_eq!(
        h.call("table.get", json!({})).diagnostics[0].code,
        Code::ResourceLimit
    );
}
#[test]
fn picture_list_geometry_payload_hash_caption_and_flags() {
    let caption = format!(
        "<hp:effects/><hp:caption><hp:subList>{}</hp:subList></hp:caption>",
        p("<hp:t>A caption &amp; entity</hp:t>")
    );
    let mut h = Harness::new(&p(&pic("img0", &caption)));
    let para = h.id("paragraph", 0);
    let v = h.query("picture.list", json!({}));
    assert_eq!(v["total"], 1);
    let i = &items(&v)[0];
    assert_eq!(i["paragraphNodeId"], para);
    assert_eq!(i["position"]["vertOffset"], "-23");
    assert_eq!(i["size"]["height"], "200");
    assert_eq!(i["orgSz"]["width"], "300");
    assert_eq!(i["curSz"]["width"], "90");
    assert_eq!(i["captionText"], "A caption & entity");
    assert_eq!(i["cropped"], false);
    assert_eq!(i["hasEffects"], false);
    assert_eq!(i["hasShadow"], false);
    assert_eq!(i["flipped"], false);
    assert_eq!(i["reverse"], "0");
    assert_eq!(i["media"]["isEmbeded"], "1");
    assert_eq!(i["media"]["external"], false);
    assert_eq!(
        i["media"]["sha256"],
        hae_package::sha256_hex(include_bytes!(
            "../../../tests/fixtures/editing/inputs/ok.png"
        ))
    );
}
#[test]
fn picture_list_empty_and_section_or_paragraph_scope() {
    let mut h = Harness::new(&(p(&pic("img0", "")) + &p("<hp:t>empty</hp:t>")));
    let para = h.id("paragraph", 1);
    assert!(items(&h.query("picture.list", json!({"scope":para}))).is_empty());
    let section = h.id("section", 0);
    assert_eq!(
        h.query("picture.list", json!({"scope":section}))["total"],
        1
    );
    let pic = h.id("picture", 0);
    assert!(!h.call("picture.list", json!({"scope":pic})).ok);
    let mut h = Harness::new("");
    assert_eq!(h.query("picture.list", json!({}))["total"], 0);
    assert!(!h.call("picture.list", json!({"scope":"missing"})).ok);
}
#[test]
fn picture_list_duplicate_media_is_document_wide_and_resolves_alias_items() {
    let mut h = Harness::new(&(p(&pic("img0", "")) + &p(&pic("alias", "")) + &p(&pic("img0", ""))));
    let scope = h.id("paragraph", 0);
    let v = h.query("picture.list", json!({"scope":scope}));
    assert_eq!(v["total"], 1);
    assert_eq!(
        items(&v)[0]["media"]["otherPictureNodeIds"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let a = h.query("picture.list", json!({"limit":1}));
    let b = h.query("picture.list", json!({"limit":1,"cursor":a["nextCursor"]}));
    assert_ne!(items(&a)[0]["nodeId"], items(&b)[0]["nodeId"]);
}
#[test]
fn picture_list_external_references_are_not_fetched_or_given_payload_hashes() {
    let mut h = Harness::new(&p(&pic("ext0", "")));
    let v = h.query("picture.list", json!({}));
    let m = &items(&v)[0]["media"];
    assert_eq!(m["isEmbeded"], "0");
    assert_eq!(m["external"], true);
    assert_eq!(m["status"], "external");
    assert_eq!(m["href"], "https://example.invalid/p.png");
    assert!(m["entry"].is_null());
    assert!(m["sha256"].is_null());
    let i = h.id("picture", 0);
    let detail = h.query("element.get", json!({"target":i}));
    assert_eq!(
        detail["outline"]["edits"]["image.replace"]["available"],
        false
    );
    // The advertisement is backed by a real rejection: an external picture has
    // no embedded payload to swap, and the refused call changes nothing.
    let replacement = h.dir.join("replacement.png");
    fs::write(
        &replacement,
        include_bytes!("../../../tests/fixtures/editing/inputs/ok.png"),
    )
    .unwrap();
    let before = h.export();
    let r = h.exec.call(
        "image.replace",
        json!({"target":i,"imagePath":replacement}),
        Some(0),
        false,
    );
    assert_eq!(r.diagnostics[0].code, Code::Unsupported);
    assert_eq!(
        r.diagnostics[0].reason.as_deref(),
        Some("no_embedded_media")
    );
    assert_eq!(h.export(), before, "rejected image.replace changed bytes");
}
#[test]
fn picture_list_crop_effect_shadow_flip_and_reverse_are_observed() {
    let image = pic(
        "img0",
        "<hp:effects><hc:shadow type=\"OUTSIDE\"/></hp:effects>",
    )
    .replace("right=\"300\"", "right=\"299\"")
    .replace("horizontal=\"0\"", "horizontal=\"1\"")
    .replace("reverse=\"0\"", "reverse=\"1\"");
    let mut h = Harness::new(&p(&image));
    let v = h.query("picture.list", json!({}));
    let i = &items(&v)[0];
    assert_eq!(i["cropped"], true);
    assert_eq!(i["hasEffects"], true);
    assert_eq!(i["hasShadow"], true);
    assert_eq!(i["flipped"], true);
    assert_eq!(i["reverse"], "1");
}
#[test]
fn picture_list_missing_geometry_and_media_are_unknown_not_guessed() {
    let mut h = Harness::new(&p(
        "<hp:pic><hp:effects/><x:orgSz xmlns:x=\"urn:foreign\" width=\"999\"/></hp:pic>",
    ));
    let v = h.query("picture.list", json!({}));
    let i = &items(&v)[0];
    assert!(i["orgSz"].is_null());
    assert!(i["cropped"].is_null());
    assert!(i["flipped"].is_null());
    assert_eq!(i["media"]["status"], "unresolved");
    assert!(i["media"]["sha256"].is_null());
}
#[test]
fn resource_queries_cursors_invalidate_after_edits_and_do_not_add_history() {
    let mut h = Harness::new(&p("<hp:t>x</hp:t>"));
    let a = h.query("style.list", json!({"limit":1}));
    assert_eq!(
        h.exec
            .call("history.undo", json!({}), Some(0), false)
            .diagnostics[0]
            .code,
        Code::HistoryEmpty
    );
    let target = h.id("paragraph", 0);
    let r = h.exec.call(
        "paragraph.format",
        json!({"target":target,"alignment":"RIGHT"}),
        Some(0),
        false,
    );
    assert!(r.ok, "{:?}", r.diagnostics);
    assert_eq!(
        h.call("style.list", json!({"cursor":a["nextCursor"]}))
            .diagnostics[0]
            .code,
        Code::VersionConflict
    );
    let v = h.query("style.list", json!({"kind":"paraPr"}));
    assert_eq!(v["revision"], 1);
    assert_eq!(v["total"], 2);
    assert!(h.exec.call("history.undo", json!({}), Some(1), false).ok);
    assert_eq!(
        h.exec
            .call("history.undo", json!({}), Some(2), false)
            .diagnostics[0]
            .code,
        Code::HistoryEmpty
    );
}
#[test]
fn resource_queries_registry_policies_schemas_and_aliases_are_complete() {
    for (name, cli, mcp) in [
        ("style.list", "style-list", "style_list"),
        ("table.get", "table-get", "table_get"),
        ("picture.list", "picture-list", "picture_list"),
    ] {
        let d = command_describe(name).unwrap();
        assert_eq!(d.category, CommandCategory::Query);
        assert_eq!(d.cli_name, cli);
        assert_eq!(d.mcp_tool_name, mcp);
        assert!(
            !d.mutates_content
                && !d.requires_expected_version
                && !d.uses_request_sequence
                && !d.records_history
        );
        assert!(
            d.annotations.read_only_hint
                && d.annotations.idempotent_hint
                && !d.annotations.destructive_hint
        );
        assert!(d.output_schema["properties"]["items"].is_object());
    }
}
#[test]
fn resource_queries_public_corpus_execute_and_preserve_exact_export_bytes() {
    let bytes =
        include_bytes!("../../../tests/fixtures/corpus/research-school-plan-reference.hwpx");
    let mut h = Harness::from_bytes(bytes, Limits::default());
    let mut stats = std::collections::BTreeMap::new();
    for kind in ["style", "charPr", "paraPr", "borderFill"] {
        let v = h.query("style.list", json!({"kind":kind,"limit":500}));
        stats.insert(kind, v["total"].clone());
        println!(
            "corpus style {kind}: total={}, unresolved={}, bodyUnresolved={}",
            v["total"], v["unresolvedReferenceCount"], v["unresolvedBodyReferenceCount"]
        );
    }
    let tables = h.query("table.get", json!({}));
    let pictures = h.query("picture.list", json!({}));
    assert_eq!(stats["style"], 9);
    assert_eq!(stats["charPr"], 148);
    assert_eq!(stats["paraPr"], 386);
    assert_eq!(stats["borderFill"], 38);
    assert_eq!(tables["total"], 8);
    assert_eq!(pictures["total"], 18);
    println!("corpus tables={} dimensions={}x{} actualCells={} coveredCells={}; pictures={} payloadHashes={} cropped={} duplicateRefs={}",tables["total"],items(&tables)[0]["rows"],items(&tables)[0]["cols"],items(&tables)[0]["cells"].as_array().unwrap().len(),items(&tables)[0]["coveredCells"].as_array().unwrap().len(),pictures["total"],items(&pictures).iter().filter(|i|i["media"]["sha256"].is_string()).count(),items(&pictures)[0]["cropped"],items(&pictures)[0]["media"]["otherPictureNodeIds"].as_array().unwrap().len());
}
#[test]
fn resource_queries_response_budget_pages_before_the_serialized_transport_limit() {
    let definitions = (0..30)
        .map(|i| format!("<hh:style id=\"{i}\" name=\"{}\"/>", "&quot;".repeat(200)))
        .collect::<String>();
    let header = format!(
        "<hh:head xmlns:hh=\"http://www.hancom.co.kr/hwpml/2011/head\">{definitions}</hh:head>"
    );
    let limits = Limits {
        max_response_bytes: 20000,
        max_request_bytes: 1024,
        ..Limits::default()
    };
    let mut h = Harness::with_header("", &header, limits);
    let v = h.query("style.list", json!({"limit":500}));
    assert!(items(&v).len() < 30);
    assert!(v["nextCursor"].is_string());
    let mcp = json!({"jsonrpc":"2.0","id":1,"result":{"content":[{"type":"text","text":serde_json::to_string(&v).unwrap()}],"structuredContent":v}});
    assert!(serde_json::to_vec(&mcp).unwrap().len() < 20000);
}
