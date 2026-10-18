//! Minimal HWPX synthesis for `document.create` (0.4).
//!
//! Design and the reasons behind each package part:
//! docs/decisions/document-paragraph-creation-design.md §7. The verifier
//! (`hae-verify::create`) re-derives every entry of the expected archive from
//! the same request **without calling this module**, so a bug shared by both
//! would still be caught by the Hancom gate only.
//!
//! The package is deliberately synthetic: it is not a copy of any corpus file.
//! Element and attribute names follow the OWPML layout observed in public
//! samples (contents order, `hh:refList` resources, `hp:secPr` children).
use crate::MIMETYPE;
use hae_diag::{Code, EngineError, Limits, Result, Stage};
use hae_xml::{escape_text, is_xml_char};
use hae_zip::{OutEntry, METHOD_DEFLATED, METHOD_STORED};
use std::sync::Arc;

/// What to build. The request layer converts command params into this type.
#[derive(Debug, Clone, Default)]
pub struct DocumentSpec {
    pub title: Option<String>,
    pub paragraphs: Vec<String>,
}

const DECL: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?>"#;

const FONTFACE_LANGS: [&str; 7] = [
    "HANGUL", "LATIN", "HANJA", "JAPANESE", "OTHER", "SYMBOL", "USER",
];
const FONT_CORE_LANGS: [&str; 7] = [
    "hangul", "latin", "hanja", "japanese", "other", "symbol", "user",
];

fn invalid(message: impl Into<String>) -> EngineError {
    EngineError::new(Code::InvalidRequest, Stage::Request, message)
}

/// New paragraph text and document titles follow the same rules as every other
/// text command: no tabs/breaks and only XML characters.
fn check_text(text: &str, what: &str) -> Result<()> {
    if text.contains(['\t', '\n', '\r']) {
        return Err(invalid(format!("{what} must not contain tabs or breaks")));
    }
    if let Some(c) = text.chars().find(|&c| !is_xml_char(c)) {
        return Err(invalid(format!(
            "{what} contains forbidden character U+{:04X}",
            c as u32
        )));
    }
    Ok(())
}

fn version_xml() -> String {
    format!(
        "{DECL}<hv:HCFVersion xmlns:hv=\"http://www.hancom.co.kr/hwpml/2011/version\" \
         tagetApplication=\"WORDPROCESSOR\" major=\"5\" minor=\"1\" micro=\"0\" \
         buildNumber=\"1\" os=\"1\" xmlVersion=\"1.2\" \
         application=\"Hancom Office Hangul\"/>"
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
    let title = match title {
        Some(t) => format!("<opf:title>{}</opf:title>", escape_text(t)),
        None => "<opf:title/>".to_string(),
    };
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

fn fontfaces_xml() -> String {
    let faces: String = FONTFACE_LANGS
        .iter()
        .map(|lang| {
            format!(
                "<hh:fontface lang=\"{lang}\" fontCnt=\"1\">\
                 <hh:font id=\"0\" face=\"함초롬바탕\" type=\"TTF\" isEmbedded=\"0\"/>\
                 </hh:fontface>"
            )
        })
        .collect();
    format!(
        "<hh:fontfaces itemCnt=\"{}\">{faces}</hh:fontfaces>",
        FONTFACE_LANGS.len()
    )
}

fn border_fills_xml() -> String {
    let edges: String = [
        "leftBorder",
        "rightBorder",
        "topBorder",
        "bottomBorder",
        "diagonal",
    ]
    .iter()
    .map(|edge| format!("<hh:{edge} type=\"NONE\" width=\"0.1 mm\" color=\"#000000\"/>"))
    .collect();
    format!(
        "<hh:borderFills itemCnt=\"1\"><hh:borderFill id=\"1\" threeD=\"0\" shadow=\"0\" \
         centerLine=\"NONE\" breakCellSeparateLine=\"0\">\
         <hh:slash type=\"NONE\" Crooked=\"0\" isCounter=\"0\"/>\
         <hh:backSlash type=\"NONE\" Crooked=\"0\" isCounter=\"0\"/>{edges}\
         </hh:borderFill></hh:borderFills>"
    )
}

fn char_properties_xml() -> String {
    let zero: String = FONT_CORE_LANGS
        .iter()
        .map(|l| format!("{l}=\"0\""))
        .collect::<Vec<_>>()
        .join(" ");
    let hundred: String = FONT_CORE_LANGS
        .iter()
        .map(|l| format!("{l}=\"100\""))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "<hh:charProperties itemCnt=\"1\"><hh:charPr id=\"0\" height=\"1000\" \
         textColor=\"#000000\" shadeColor=\"none\" useFontSpace=\"0\" useKerning=\"0\" \
         symMark=\"NONE\" borderFillIDRef=\"1\"><hh:fontRef {zero}/><hh:ratio {hundred}/>\
         <hh:spacing {zero}/><hh:relSz {hundred}/><hh:offset {zero}/>\
         <hh:underline type=\"NONE\" shape=\"SOLID\" color=\"#000000\"/>\
         <hh:strikeout shape=\"NONE\" color=\"#000000\"/><hh:outline type=\"NONE\"/>\
         <hh:shadow type=\"NONE\" color=\"#C0C0C0\" offsetX=\"10\" offsetY=\"10\"/>\
         </hh:charPr></hh:charProperties>"
    )
}

fn para_properties_xml() -> String {
    "<hh:paraProperties itemCnt=\"1\"><hh:paraPr id=\"0\" tabPrIDRef=\"0\" condense=\"0\" \
         fontLineHeight=\"0\" snapToGrid=\"1\" suppressLineNumbers=\"0\" checked=\"0\">\
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
         offsetBottom=\"0\" connect=\"0\" ignoreMargin=\"0\"/></hh:paraPr></hh:paraProperties>"
        .to_string()
}

fn styles_xml() -> String {
    concat!(
        "<hh:styles itemCnt=\"1\"><hh:style id=\"0\" type=\"PARA\" name=\"바탕글\" ",
        "engName=\"Normal\" paraPrIDRef=\"0\" charPrIDRef=\"0\" nextStyleIDRef=\"0\" ",
        "langID=\"1042\" lockForm=\"0\"/></hh:styles>"
    )
    .to_string()
}

fn header_xml() -> String {
    format!(
        "{DECL}<hh:head xmlns:hh=\"http://www.hancom.co.kr/hwpml/2011/head\" \
         xmlns:hp=\"http://www.hancom.co.kr/hwpml/2011/paragraph\" \
         xmlns:hs=\"http://www.hancom.co.kr/hwpml/2011/section\" \
         xmlns:hc=\"http://www.hancom.co.kr/hwpml/2011/core\" version=\"1.2\" secCnt=\"1\">\
         <hh:beginNum page=\"1\" footnote=\"1\" endnote=\"1\" pic=\"1\" tbl=\"1\" equation=\"1\"/>\
         <hh:refList>{}{}{}{}{}</hh:refList></hh:head>",
        fontfaces_xml(),
        border_fills_xml(),
        char_properties_xml(),
        para_properties_xml(),
        styles_xml(),
    )
}

fn sec_pr() -> String {
    "<hp:secPr id=\"\" textDirection=\"HORIZONTAL\" spaceColumns=\"1134\" tabStop=\"8000\" \
     outlineShapeIDRef=\"0\" memoShapeIDRef=\"0\" textVerticalWidthHead=\"0\" \
     masterPageCnt=\"0\"><hp:grid lineGrid=\"0\" charGrid=\"0\" wonggojiFormat=\"0\"/>\
     <hp:startNum pageStartsOn=\"BOTH\" page=\"0\" pic=\"0\" tbl=\"0\" equation=\"0\"/>\
     <hp:visibility hideFirstHeader=\"0\" hideFirstFooter=\"0\" hideFirstMasterPage=\"0\" \
     border=\"SHOW_ALL\" fill=\"SHOW_ALL\" hideFirstPageNum=\"0\" hideFirstEmptyLine=\"0\" \
     showLineNumber=\"0\"/><hp:lineNumberShape restartType=\"0\" countBy=\"0\" distance=\"0\" \
     startNumber=\"0\"/><hp:pagePr landscape=\"WIDELY\" width=\"59528\" height=\"84186\" \
     gutterType=\"LEFT_ONLY\"><hp:margin header=\"4252\" footer=\"4252\" gutter=\"0\" \
     left=\"8504\" right=\"8504\" top=\"5668\" bottom=\"4252\"/></hp:pagePr></hp:secPr>"
        .to_string()
}

fn section_xml(paragraphs: &[String]) -> String {
    let mut body = String::new();
    for (i, text) in paragraphs.iter().enumerate() {
        let first = i == 0;
        body.push_str(&format!(
            "<hp:p id=\"{}\" paraPrIDRef=\"0\" styleIDRef=\"0\" pageBreak=\"0\" \
             columnBreak=\"0\" merged=\"0\">",
            i + 1
        ));
        if first {
            body.push_str("<hp:run charPrIDRef=\"0\">");
            body.push_str(&sec_pr());
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

/// Entry names of a created package, in order. Shared with the command result
/// so the published file can be audited without reopening it.
pub const CREATED_ENTRIES: [&str; 6] = [
    "mimetype",
    "version.xml",
    "META-INF/container.xml",
    "Contents/content.hpf",
    "Contents/header.xml",
    "Contents/section0.xml",
];

/// Entry name, payload and ZIP method of the package this spec describes, in
/// order. The verifier derives the same list independently.
fn entries(spec: &DocumentSpec) -> Result<Vec<(&'static str, String, u16)>> {
    let paragraphs = if spec.paragraphs.is_empty() {
        vec![String::new()]
    } else {
        spec.paragraphs.clone()
    };
    if paragraphs.len() > 65535 {
        return Err(EngineError::new(
            Code::ResourceLimit,
            Stage::Request,
            "too many paragraphs for a HWPX section",
        ));
    }
    for (i, text) in paragraphs.iter().enumerate() {
        check_text(text, &format!("paragraphs[{i}]"))?;
    }
    if let Some(title) = &spec.title {
        check_text(title, "title")?;
    }
    let [mimetype, version, container, hpf, header, section] = CREATED_ENTRIES;
    Ok(vec![
        (mimetype, MIMETYPE.to_string(), METHOD_STORED),
        (version, version_xml(), METHOD_DEFLATED),
        (container, container_xml(), METHOD_DEFLATED),
        (hpf, content_hpf(spec.title.as_deref()), METHOD_DEFLATED),
        (header, header_xml(), METHOD_DEFLATED),
        (section, section_xml(&paragraphs), METHOD_DEFLATED),
    ])
}

/// Build the new package bytes. Deterministic for a given spec.
pub fn create_hwpx(spec: &DocumentSpec, limits: &Limits) -> Result<Vec<u8>> {
    let mut plan = Vec::new();
    let mut total = 0u64;
    for (name, payload, method) in entries(spec)? {
        total += payload.len() as u64;
        if total > limits.max_hwpx_bytes {
            return Err(EngineError::new(
                Code::ResourceLimit,
                Stage::Save,
                "new document exceeds the HWPX size limit",
            ));
        }
        plan.push(OutEntry::Add {
            name: name.to_string(),
            payload: Arc::from(payload.into_bytes()),
            method,
        });
    }
    let bytes = hae_zip::write_new(&plan)?;
    if bytes.len() as u64 > limits.max_hwpx_bytes {
        return Err(EngineError::new(
            Code::ResourceLimit,
            Stage::Save,
            "new document exceeds the HWPX size limit",
        ));
    }
    Ok(bytes)
}
