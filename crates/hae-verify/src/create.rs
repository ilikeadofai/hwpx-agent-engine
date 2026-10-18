//! Independent verifier for `document.create` (0.4).
//!
//! A brand-new document has no source package to compare against, so the
//! verifier derives the **whole expected archive** from the request alone
//! (docs/decisions/document-paragraph-creation-design.md §6.3) and compares
//! every entry byte for byte. It does not call `hae_package::create` and has no
//! `hae-model`/`hae-edit` dependency: the builder and this module are separate
//! implementations of the same package contract.
use crate::{fail, VerifyReport};
use hae_diag::{Limits, Result};
use hae_package::{PackageState, MIMETYPE};
use hae_xml::{escape_text, is_xml_char};
use hae_zip::{Archive, METHOD_STORED};
use serde_json::json;

const DECL: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?>"#;
const LANGS: [(&str, &str); 7] = [
    ("HANGUL", "hangul"),
    ("LATIN", "latin"),
    ("HANJA", "hanja"),
    ("JAPANESE", "japanese"),
    ("OTHER", "other"),
    ("SYMBOL", "symbol"),
    ("USER", "user"),
];

/// The verifier's own copy of the text rules (design §5.1).
fn text_ok(s: &str) -> bool {
    !s.contains(['\t', '\n', '\r']) && s.chars().all(is_xml_char)
}

fn version_xml() -> String {
    format!(
        "{DECL}<hv:HCFVersion xmlns:hv=\"http://www.hancom.co.kr/hwpml/2011/version\" \
         tagetApplication=\"WORDPROCESSOR\" major=\"5\" minor=\"1\" micro=\"0\" \
         buildNumber=\"1\" os=\"1\" xmlVersion=\"1.2\" application=\"Hancom Office Hangul\"/>"
    )
}

fn container_xml() -> String {
    format!(
        "{DECL}<ocf:container xmlns:ocf=\"urn:oasis:names:tc:opendocument:xmlns:container\" \
         xmlns:hpf=\"http://www.hancom.co.kr/schema/2011/hpf\"><ocf:rootfiles>\
         <ocf:rootfile full-path=\"Contents/content.hpf\" \
         media-type=\"application/hwpml-package+xml\"/></ocf:rootfiles></ocf:container>"
    )
}

fn content_hpf(title: Option<&str>) -> String {
    let title = title.map_or_else(
        || "<opf:title/>".to_string(),
        |t| format!("<opf:title>{}</opf:title>", escape_text(t)),
    );
    format!(
        "{DECL}<opf:package xmlns:opf=\"http://www.idpf.org/2007/opf/\" \
         xmlns:hpf=\"http://www.hancom.co.kr/schema/2011/hpf\" version=\"\" \
         unique-identifier=\"\" id=\"\"><opf:metadata>{title}\
         <opf:language>ko</opf:language></opf:metadata><opf:manifest>\
         <opf:item id=\"header\" href=\"Contents/header.xml\" media-type=\"application/xml\"/>\
         <opf:item id=\"section0\" href=\"Contents/section0.xml\" media-type=\"application/xml\"/>\
         </opf:manifest><opf:spine><opf:itemref idref=\"header\"/>\
         <opf:itemref idref=\"section0\"/></opf:spine></opf:package>"
    )
}

fn header_xml() -> String {
    let faces: String = LANGS
        .iter()
        .map(|(lang, _)| {
            format!(
                "<hh:fontface lang=\"{lang}\" fontCnt=\"1\">\
                 <hh:font id=\"0\" face=\"함초롬바탕\" type=\"TTF\" isEmbedded=\"0\"/>\
                 </hh:fontface>"
            )
        })
        .collect();
    let edges: String = [
        "leftBorder",
        "rightBorder",
        "topBorder",
        "bottomBorder",
        "diagonal",
    ]
    .iter()
    .map(|e| format!("<hh:{e} type=\"NONE\" width=\"0.1 mm\" color=\"#000000\"/>"))
    .collect();
    let zero: String = LANGS
        .iter()
        .map(|(_, core)| format!("{core}=\"0\""))
        .collect::<Vec<_>>()
        .join(" ");
    let hundred: String = LANGS
        .iter()
        .map(|(_, core)| format!("{core}=\"100\""))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "{DECL}<hh:head xmlns:hh=\"http://www.hancom.co.kr/hwpml/2011/head\" \
         xmlns:hp=\"http://www.hancom.co.kr/hwpml/2011/paragraph\" \
         xmlns:hs=\"http://www.hancom.co.kr/hwpml/2011/section\" \
         xmlns:hc=\"http://www.hancom.co.kr/hwpml/2011/core\" version=\"1.2\" secCnt=\"1\">\
         <hh:beginNum page=\"1\" footnote=\"1\" endnote=\"1\" pic=\"1\" tbl=\"1\" equation=\"1\"/>\
         <hh:refList><hh:fontfaces itemCnt=\"7\">{faces}</hh:fontfaces>\
         <hh:borderFills itemCnt=\"1\"><hh:borderFill id=\"1\" threeD=\"0\" shadow=\"0\" \
         centerLine=\"NONE\" breakCellSeparateLine=\"0\">\
         <hh:slash type=\"NONE\" Crooked=\"0\" isCounter=\"0\"/>\
         <hh:backSlash type=\"NONE\" Crooked=\"0\" isCounter=\"0\"/>{edges}</hh:borderFill>\
         </hh:borderFills><hh:charProperties itemCnt=\"1\"><hh:charPr id=\"0\" height=\"1000\" \
         textColor=\"#000000\" shadeColor=\"none\" useFontSpace=\"0\" useKerning=\"0\" \
         symMark=\"NONE\" borderFillIDRef=\"1\"><hh:fontRef {zero}/><hh:ratio {hundred}/>\
         <hh:spacing {zero}/><hh:relSz {hundred}/><hh:offset {zero}/>\
         <hh:underline type=\"NONE\" shape=\"SOLID\" color=\"#000000\"/>\
         <hh:strikeout shape=\"NONE\" color=\"#000000\"/><hh:outline type=\"NONE\"/>\
         <hh:shadow type=\"NONE\" color=\"#C0C0C0\" offsetX=\"10\" offsetY=\"10\"/>\
         </hh:charPr></hh:charProperties><hh:paraProperties itemCnt=\"1\">\
         <hh:paraPr id=\"0\" tabPrIDRef=\"0\" condense=\"0\" fontLineHeight=\"0\" \
         snapToGrid=\"1\" suppressLineNumbers=\"0\" checked=\"0\">\
         <hh:align horizontal=\"JUSTIFY\" vertical=\"BASELINE\"/>\
         <hh:heading type=\"NONE\" idRef=\"0\" level=\"0\"/>\
         <hh:breakSetting breakLatinWord=\"KEEP_WORD\" breakNonLatinWord=\"BREAK_WORD\" \
         widowOrphan=\"0\" keepWithNext=\"0\" keepLines=\"0\" pageBreakBefore=\"0\" \
         lineWrap=\"BREAK\"/><hh:autoSpacing eAsianEng=\"0\" eAsianNum=\"0\"/>\
         <hh:margin><hc:intent value=\"0\" unit=\"HWPUNIT\"/><hc:left value=\"0\" unit=\"HWPUNIT\"/>\
         <hc:right value=\"0\" unit=\"HWPUNIT\"/><hc:prev value=\"0\" unit=\"HWPUNIT\"/>\
         <hc:next value=\"0\" unit=\"HWPUNIT\"/></hh:margin>\
         <hh:lineSpacing type=\"PERCENT\" value=\"160\" unit=\"HWPUNIT\"/>\
         <hh:border borderFillIDRef=\"1\" offsetLeft=\"0\" offsetRight=\"0\" offsetTop=\"0\" \
         offsetBottom=\"0\" connect=\"0\" ignoreMargin=\"0\"/></hh:paraPr></hh:paraProperties>\
         <hh:styles itemCnt=\"1\"><hh:style id=\"0\" type=\"PARA\" name=\"바탕글\" \
         engName=\"Normal\" paraPrIDRef=\"0\" charPrIDRef=\"0\" nextStyleIDRef=\"0\" \
         langID=\"1042\" lockForm=\"0\"/></hh:styles></hh:refList></hh:head>"
    )
}

fn section_xml(paragraphs: &[String]) -> String {
    let mut body = String::new();
    for (i, text) in paragraphs.iter().enumerate() {
        body.push_str(&format!(
            "<hp:p id=\"{}\" paraPrIDRef=\"0\" styleIDRef=\"0\" pageBreak=\"0\" \
             columnBreak=\"0\" merged=\"0\">",
            i + 1
        ));
        if i == 0 {
            body.push_str("<hp:run charPrIDRef=\"0\">");
            body.push_str(
                "<hp:secPr id=\"\" textDirection=\"HORIZONTAL\" spaceColumns=\"1134\" \
                 tabStop=\"8000\" outlineShapeIDRef=\"0\" memoShapeIDRef=\"0\" \
                 textVerticalWidthHead=\"0\" masterPageCnt=\"0\">\
                 <hp:grid lineGrid=\"0\" charGrid=\"0\" wonggojiFormat=\"0\"/>\
                 <hp:startNum pageStartsOn=\"BOTH\" page=\"0\" pic=\"0\" tbl=\"0\" equation=\"0\"/>\
                 <hp:visibility hideFirstHeader=\"0\" hideFirstFooter=\"0\" \
                 hideFirstMasterPage=\"0\" border=\"SHOW_ALL\" fill=\"SHOW_ALL\" \
                 hideFirstPageNum=\"0\" hideFirstEmptyLine=\"0\" showLineNumber=\"0\"/>\
                 <hp:lineNumberShape restartType=\"0\" countBy=\"0\" distance=\"0\" \
                 startNumber=\"0\"/><hp:pagePr landscape=\"WIDELY\" width=\"59528\" \
                 height=\"84186\" gutterType=\"LEFT_ONLY\"><hp:margin header=\"4252\" \
                 footer=\"4252\" gutter=\"0\" left=\"8504\" right=\"8504\" top=\"5668\" \
                 bottom=\"4252\"/></hp:pagePr></hp:secPr>",
            );
            if text.is_empty() {
                body.push_str("<hp:t/>");
            } else {
                body.push_str(&format!("<hp:t>{}</hp:t>", escape_text(text)));
            }
            body.push_str("</hp:run>");
        } else if text.is_empty() {
            body.push_str("<hp:run charPrIDRef=\"0\"/>");
        } else {
            body.push_str(&format!(
                "<hp:run charPrIDRef=\"0\"><hp:t>{}</hp:t></hp:run>",
                escape_text(text)
            ));
        }
        body.push_str("</hp:p>");
    }
    format!(
        "{DECL}<hs:sec xmlns:hs=\"http://www.hancom.co.kr/hwpml/2011/section\" \
         xmlns:hp=\"http://www.hancom.co.kr/hwpml/2011/paragraph\" \
         xmlns:hc=\"http://www.hancom.co.kr/hwpml/2011/core\">{body}</hs:sec>"
    )
}

/// Expected entry name and payload, independently derived from the request.
fn expected(spec: &crate::CreateSpec) -> Result<Vec<(&'static str, Vec<u8>)>> {
    let paragraphs = if spec.paragraphs.is_empty() {
        vec![String::new()]
    } else {
        spec.paragraphs.clone()
    };
    if paragraphs.len() > 65535 {
        return Err(fail("too many paragraphs for one HWPX section"));
    }
    for (i, text) in paragraphs.iter().enumerate() {
        if !text_ok(text) {
            return Err(fail(format!("paragraphs[{i}] is not valid paragraph text")));
        }
    }
    if let Some(t) = &spec.title {
        if !text_ok(t) {
            return Err(fail("title is not valid text"));
        }
    }
    Ok(vec![
        ("mimetype", MIMETYPE.as_bytes().to_vec()),
        ("version.xml", version_xml().into_bytes()),
        ("META-INF/container.xml", container_xml().into_bytes()),
        (
            "Contents/content.hpf",
            content_hpf(spec.title.as_deref()).into_bytes(),
        ),
        ("Contents/header.xml", header_xml().into_bytes()),
        (
            "Contents/section0.xml",
            section_xml(&paragraphs).into_bytes(),
        ),
    ])
}

/// Verify a candidate created from `spec`: entry order, names, methods
/// (`mimetype` STORED and first) and exact payload bytes.
pub(crate) fn check(
    bytes: &[u8],
    spec: &crate::CreateSpec,
    limits: &Limits,
) -> Result<VerifyReport> {
    let expected = expected(spec)?;
    let archive = Archive::open(bytes.to_vec(), limits).map_err(|e| fail(e.to_string()))?;
    if archive.len() != expected.len() {
        return Err(fail(format!(
            "new document has {} entries, expected {}",
            archive.len(),
            expected.len()
        )));
    }
    let first = archive.entries().first().expect("non-empty");
    if first.name != "mimetype" || first.method != METHOD_STORED {
        return Err(fail("new document must start with a STORED mimetype entry"));
    }
    for (i, (name, payload)) in expected.iter().enumerate() {
        let got = &archive.entries()[i];
        if &got.name != name {
            return Err(fail(format!(
                "new document entry {i} is {}, expected {name}",
                got.name
            )));
        }
        if got.payload()[..] != payload[..] {
            return Err(fail(format!(
                "new document entry {name} does not match the request"
            ))
            .with_detail(json!({
                "entry": name,
                "expectedSha256": hae_package::sha256_hex(payload),
                "candidateSha256": hae_package::sha256_hex(got.payload()),
            })));
        }
    }
    // The package graph itself must analyze (spine, manifest, references).
    let state = PackageState::from_archive(&archive);
    hae_package::analyze(&state, true, limits)?;
    Ok(VerifyReport {
        changed_entries: expected.iter().map(|(n, _)| (*n).to_string()).collect(),
        ..VerifyReport::default()
    })
}
