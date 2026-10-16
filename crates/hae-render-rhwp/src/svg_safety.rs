//! SVG safety scanning for renderer output (task 3-1).
//!
//! The preview pipeline renders a document to SVG with an external renderer and
//! may rasterise the result. Before those bytes are handed to a viewer or to a
//! rasteriser they are scanned here for constructs that could execute script or
//! reach the network. The scan is deliberately conservative: every suspicious
//! construct is reported, and anything that is not [`SvgCheck::is_safe`] should
//! be refused rather than repaired.
//!
//! Only the standard library and `quick_xml` are used. No input can make this
//! module panic: invalid UTF-8, truncation and parse errors are reported as
//! [`SvgIssueKind::Malformed`], and element/attribute names are compared as raw
//! bytes so that even undecodable names cannot panic.

use quick_xml::events::attributes::Attribute;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

/// Kind of risky construct found in an SVG document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SvgIssueKind {
    Script,
    EventHandler,
    ForeignObject,
    ExternalReference,
    DocType,
    Entity,
    ProcessingInstruction,
    Malformed,
}

/// One risky construct, with the reader position where it was seen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SvgIssue {
    pub kind: SvgIssueKind,
    pub detail: String,
    pub byte_offset: usize,
}

/// Result of scanning one SVG document.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SvgCheck {
    pub issues: Vec<SvgIssue>,
    pub elements: usize,
}

impl SvgCheck {
    pub fn is_safe(&self) -> bool {
        self.issues.is_empty()
    }
}

/// Scans SVG `bytes` for script, event handlers, foreign content, external
/// references, DOCTYPE/entity declarations, processing instructions and
/// malformed XML.
///
/// `byte_offset` of every issue is the input position where the reader started
/// the event that produced it. Scanning stops at the first reader-level parse
/// error, which is reported once as [`SvgIssueKind::Malformed`].
pub fn check_svg(bytes: &[u8]) -> SvgCheck {
    let mut scan = Scan::default();
    if std::str::from_utf8(bytes).is_err() {
        scan.issue(
            SvgIssueKind::Malformed,
            "input is not valid UTF-8".to_string(),
            0,
        );
        return scan.check;
    }
    let mut reader = Reader::from_reader(bytes);
    {
        let config = reader.config_mut();
        config.check_end_names = true;
        config.expand_empty_elements = false;
        config.trim_text(false);
        config.check_comments = true;
    }
    loop {
        let offset = reader.buffer_position() as usize;
        let event = match reader.read_event() {
            Ok(event) => event,
            Err(error) => {
                scan.issue(
                    SvgIssueKind::Malformed,
                    format!("XML parse error: {error}"),
                    offset,
                );
                return scan.check;
            }
        };
        let is_empty = matches!(event, Event::Empty(_));
        match event {
            Event::Eof => {
                if !scan.open.is_empty() {
                    scan.issue(
                        SvgIssueKind::Malformed,
                        format!(
                            "unexpected end of input: {} unclosed element(s)",
                            scan.open.len()
                        ),
                        offset,
                    );
                }
                return scan.check;
            }
            Event::Start(ref start) | Event::Empty(ref start) => {
                if !scan.start_element(start, is_empty, offset) {
                    return scan.check;
                }
            }
            Event::End(ref end) => scan.end_element(end.local_name().into_inner(), offset),
            Event::Text(ref text) => scan.characters(text, offset),
            Event::CData(ref data) => scan.characters(data, offset),
            Event::GeneralRef(ref reference) => scan.reference(reference, offset),
            Event::DocType(ref doctype) => scan.issue(
                SvgIssueKind::DocType,
                summarize("DOCTYPE: ", doctype),
                offset,
            ),
            Event::PI(ref instruction) => scan.issue(
                SvgIssueKind::ProcessingInstruction,
                summarize("processing instruction <?", instruction.target()),
                offset,
            ),
            Event::Decl(_) | Event::Comment(_) => {}
        }
    }
}

/// Mutable state of one scan.
#[derive(Debug, Default)]
struct Scan {
    check: SvgCheck,
    /// Local names of the elements that are currently open, outermost first.
    open: Vec<Vec<u8>>,
    /// Text and CDATA collected inside the innermost `<style>` element.
    style_text: String,
}

impl Scan {
    fn issue(&mut self, kind: SvgIssueKind, detail: String, byte_offset: usize) {
        self.check.issues.push(SvgIssue {
            kind,
            detail,
            byte_offset,
        });
    }

    fn in_style(&self) -> bool {
        matches!(self.open.last(), Some(name) if local_name_is(name, b"style"))
    }

    /// Handles one start or empty-element tag. Returns `false` when the
    /// attribute list itself is malformed, which stops the scan.
    fn start_element(&mut self, start: &BytesStart<'_>, is_empty: bool, offset: usize) -> bool {
        self.check.elements += 1;
        let name = start.local_name().into_inner();
        if local_name_is(name, b"script") {
            self.issue(SvgIssueKind::Script, "<script> element".to_string(), offset);
        }
        if local_name_is(name, b"foreignObject") {
            self.issue(
                SvgIssueKind::ForeignObject,
                "<foreignObject> element".to_string(),
                offset,
            );
        }
        for attribute in start.attributes() {
            let attribute = match attribute {
                Ok(attribute) => attribute,
                Err(error) => {
                    self.issue(
                        SvgIssueKind::Malformed,
                        format!("malformed attribute: {error}"),
                        offset,
                    );
                    return false;
                }
            };
            self.attribute(&attribute, offset);
        }
        if !is_empty {
            self.open.push(name.to_vec());
        }
        true
    }

    fn attribute(&mut self, attribute: &Attribute<'_>, offset: usize) {
        let name = attribute.key.into_inner();
        let local = local_part(name);
        if local.len() >= 2
            && local[0].eq_ignore_ascii_case(&b'o')
            && local[1].eq_ignore_ascii_case(&b'n')
        {
            self.issue(
                SvgIssueKind::EventHandler,
                format!(
                    "event handler attribute `{}`",
                    String::from_utf8_lossy(name)
                ),
                offset,
            );
        }
        let raw_value = String::from_utf8_lossy(&attribute.value);
        let (value, unknown_entities) = unescape(&raw_value);
        for entity in unknown_entities {
            self.issue(
                SvgIssueKind::Entity,
                format!("unknown entity reference `&{entity};` in attribute"),
                offset,
            );
        }
        if local_name_is(local, b"href") || local_name_is(local, b"src") {
            let target = value.trim();
            if !target.is_empty() && !target.starts_with('#') && !target.starts_with("data:") {
                self.issue(
                    SvgIssueKind::ExternalReference,
                    format!(
                        "attribute `{}` points to `{target}`",
                        String::from_utf8_lossy(name)
                    ),
                    offset,
                );
            }
        }
        for target in external_url_targets(&value) {
            self.issue(
                SvgIssueKind::ExternalReference,
                format!(
                    "`url({target})` in attribute `{}`",
                    String::from_utf8_lossy(name)
                ),
                offset,
            );
        }
    }

    fn end_element(&mut self, name: &[u8], offset: usize) {
        if local_name_is(name, b"style") && self.in_style() {
            let text = std::mem::take(&mut self.style_text);
            self.style_checks(&text, offset);
        }
        self.open.pop();
    }

    /// Collects element text; only `<style>` content is inspected later.
    fn characters(&mut self, raw: &[u8], _offset: usize) {
        if self.in_style() {
            self.style_text.push_str(&String::from_utf8_lossy(raw));
        }
    }

    fn reference(&mut self, name: &[u8], offset: usize) {
        let name = String::from_utf8_lossy(name);
        if resolve_reference(&name).is_none() {
            self.issue(
                SvgIssueKind::Entity,
                format!("unknown entity reference `&{name};`"),
                offset,
            );
        }
    }

    fn style_checks(&mut self, text: &str, offset: usize) {
        if text.to_ascii_lowercase().contains("@import") {
            self.issue(
                SvgIssueKind::ExternalReference,
                "`@import` in <style>".to_string(),
                offset,
            );
        }
        for target in external_url_targets(text) {
            self.issue(
                SvgIssueKind::ExternalReference,
                format!("`url({target})` in <style>"),
                offset,
            );
        }
    }
}

/// Local name of a possibly prefixed name.
fn local_part(name: &[u8]) -> &[u8] {
    match name.iter().position(|byte| *byte == b':') {
        Some(colon) => &name[colon + 1..],
        None => name,
    }
}

/// Compares a local name case-insensitively. XML names are case-sensitive, but
/// script-capable names are matched loosely so that `<SCRIPT>` or `ONLOAD` in
/// any spelling is still reported.
fn local_name_is(name: &[u8], wanted: &[u8]) -> bool {
    name.eq_ignore_ascii_case(wanted)
}

/// Truncates a diagnostic detail so that huge declarations cannot be copied
/// verbatim into an issue.
fn summarize(prefix: &str, bytes: &[u8]) -> String {
    const LIMIT: usize = 120;
    let text = String::from_utf8_lossy(bytes);
    if text.chars().count() <= LIMIT {
        return format!("{prefix}{text}");
    }
    let cut = text
        .char_indices()
        .nth(LIMIT)
        .map(|(index, _)| index)
        .unwrap_or(text.len());
    format!("{prefix}...{}", &text[..cut])
}

/// Resolves an XML reference name (as found between `&` and `;`) to a
/// character. Returns `None` for anything that is not one of the five
/// predefined entities or a valid numeric character reference.
fn resolve_reference(name: &str) -> Option<char> {
    match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "apos" => Some('\''),
        "quot" => Some('"'),
        _ => {
            let number = name.strip_prefix('#')?;
            let code = match number.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => number.parse::<u32>().ok()?,
            };
            char::from_u32(code)
        }
    }
}

/// Replaces character and predefined entity references in an attribute value.
///
/// The first element of the result is the decoded value, with unresolved
/// references kept verbatim so that reference targets stay readable. The second
/// element lists the names of the references that are not predefined.
fn unescape(raw: &str) -> (String, Vec<String>) {
    if !raw.contains('&') {
        return (raw.to_string(), Vec::new());
    }
    let mut decoded = String::with_capacity(raw.len());
    let mut unknown = Vec::new();
    let mut rest = raw;
    while let Some(start) = rest.find('&') {
        decoded.push_str(&rest[..start]);
        let tail = &rest[start + 1..];
        match tail.find(';') {
            Some(end) => {
                let name = &tail[..end];
                rest = &tail[end + 1..];
                match resolve_reference(name) {
                    Some(character) => decoded.push(character),
                    None => {
                        unknown.push(name.to_string());
                        decoded.push('&');
                        decoded.push_str(name);
                        decoded.push(';');
                    }
                }
            }
            None => {
                // A bare `&` is not well formed; keep it as written.
                decoded.push('&');
                rest = tail;
            }
        }
    }
    decoded.push_str(rest);
    (decoded, unknown)
}

/// Returns every `url(...)` target in `text` that is neither a same-document
/// fragment (`#...`) nor an inline `data:` URL.
fn external_url_targets(text: &str) -> Vec<String> {
    let mut targets = Vec::new();
    if !text.to_ascii_lowercase().contains("url(") {
        return targets;
    }
    // ASCII lowercasing preserves byte offsets, so indices stay valid for `text`.
    let lowered = text.to_ascii_lowercase();
    let bytes = lowered.as_bytes();
    let mut search_from = 0;
    while let Some(relative) = lowered[search_from..].find("url(") {
        let open = search_from + relative;
        let close = match lowered[open + 4..].find(')') {
            Some(relative) => open + 4 + relative,
            None => break,
        };
        let is_function_token = open == 0 || {
            let previous = bytes[open - 1];
            !(previous.is_ascii_alphanumeric() || previous == b'_' || previous == b'-')
        };
        if is_function_token {
            let target = text[open + 4..close]
                .trim()
                .trim_matches(|character| character == '"' || character == '\'')
                .trim();
            if !target.is_empty() && !target.starts_with('#') && !target.starts_with("data:") {
                targets.push(target.to_string());
            }
        }
        search_from = close + 1;
    }
    targets
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAFE: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="10" height="10">
  <defs><linearGradient id="g"><stop offset="0" stop-color="#fff"/></linearGradient></defs>
  <use href="#id" xlink:href="data:image/png;base64,AAA"/>
  <rect width="10" height="10" fill="url(#g)"/>
</svg>"##;

    fn kinds(svg: &str) -> Vec<SvgIssueKind> {
        check_svg(svg.as_bytes())
            .issues
            .into_iter()
            .map(|issue| issue.kind)
            .collect()
    }

    fn has(svg: &str, kind: SvgIssueKind) -> bool {
        kinds(svg).contains(&kind)
    }

    #[test]
    fn safe_svg_passes() {
        let check = check_svg(SAFE.as_bytes());
        assert!(check.is_safe(), "unexpected issues: {:?}", check.issues);
        assert_eq!(check.elements, 6);
    }

    #[test]
    fn safe_svg_with_fragment_and_data_urls_passes() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink">
  <image href="#id" xlink:href="data:image/png;base64,AAA"/>
  <image href=""/>
  <rect fill="url(#g)" stroke="url(&quot;#h&quot;)"/>
  <style>#a,.b{fill:url(#c);stroke:url('#d')}</style>
</svg>"##;
        let check = check_svg(svg.as_bytes());
        assert!(check.is_safe(), "unexpected issues: {:?}", check.issues);
    }

    #[test]
    fn script_element_is_reported() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>"#;
        let check = check_svg(svg.as_bytes());
        assert_eq!(check.issues.len(), 1, "{:?}", check.issues);
        assert_eq!(check.issues[0].kind, SvgIssueKind::Script);
        assert!(check.issues[0].byte_offset > 0);
    }

    #[test]
    fn event_handler_attribute_is_reported() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" onclick="x()"/>"#;
        assert!(has(svg, SvgIssueKind::EventHandler));
    }

    #[test]
    fn uppercase_event_handler_attribute_is_reported() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" ONLOAD="x()"/>"#;
        assert!(has(svg, SvgIssueKind::EventHandler));
        let prefixed = r#"<svg xmlns:xlink="http://www.w3.org/1999/xlink" xlink:onclick="x()"/>"#;
        assert!(has(prefixed, SvgIssueKind::EventHandler));
    }

    #[test]
    fn foreign_object_is_reported() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg"><foreignObject><body/></foreignObject></svg>"#;
        assert!(has(svg, SvgIssueKind::ForeignObject));
    }

    #[test]
    fn external_href_and_src_are_reported() {
        assert!(has(
            r#"<svg><image href="http://evil.example/x.png"/></svg>"#,
            SvgIssueKind::ExternalReference
        ));
        assert!(has(
            r#"<svg><image xlink:href="https://evil.example/x.png"/></svg>"#,
            SvgIssueKind::ExternalReference
        ));
        assert!(has(
            r#"<svg><image src="//evil.example/x.png"/></svg>"#,
            SvgIssueKind::ExternalReference
        ));
        assert!(has(
            r#"<svg><image xlink:href="file:///etc/passwd"/></svg>"#,
            SvgIssueKind::ExternalReference
        ));
        assert!(!has(
            r##"<svg><image href="#id"/></svg>"##,
            SvgIssueKind::ExternalReference
        ));
        assert!(!has(
            r#"<svg><image xlink:href="data:image/png;base64,AAA"/></svg>"#,
            SvgIssueKind::ExternalReference
        ));
        assert!(!has(
            r#"<svg><image href=""/></svg>"#,
            SvgIssueKind::ExternalReference
        ));
    }

    #[test]
    fn external_url_function_is_reported() {
        assert!(has(
            r#"<svg><rect fill="url(http://evil.example/x.svg)"/></svg>"#,
            SvgIssueKind::ExternalReference
        ));
        assert!(has(
            r#"<svg><rect fill="url( 'https://evil.example/x.svg' )"/></svg>"#,
            SvgIssueKind::ExternalReference
        ));
        assert!(has(
            r#"<svg><style>@import url("http://evil.example/x.css");</style></svg>"#,
            SvgIssueKind::ExternalReference
        ));
        assert!(has(
            r#"<svg><style>@import "x.css";</style></svg>"#,
            SvgIssueKind::ExternalReference
        ));
        assert!(!has(
            r##"<svg><rect fill="url(#g)" stroke="url( '#h' )"/></svg>"##,
            SvgIssueKind::ExternalReference
        ));
        assert!(!has(
            r#"<svg><style>#a{fill:url(data:image/png;base64,AAA)}</style></svg>"#,
            SvgIssueKind::ExternalReference
        ));
        assert!(!has(
            r#"<svg><style>.a{background-color:#fff}</style></svg>"#,
            SvgIssueKind::ExternalReference
        ));
    }

    #[test]
    fn doctype_is_reported() {
        let svg = r#"<!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "x.dtd"><svg/>"#;
        assert_eq!(kinds(svg), vec![SvgIssueKind::DocType]);
    }

    #[test]
    fn unknown_entity_is_reported() {
        assert!(has(
            r#"<svg><text>&xxe;</text></svg>"#,
            SvgIssueKind::Entity
        ));
        assert!(has(
            r#"<svg><image href="&custom;"/></svg>"#,
            SvgIssueKind::Entity
        ));
        assert!(!has(
            r#"<svg><text>&amp;&lt;&gt;&apos;&quot;&#65;&#x41;</text></svg>"#,
            SvgIssueKind::Entity
        ));
    }

    #[test]
    fn processing_instruction_is_reported() {
        let svg = r#"<?xml version="1.0"?><svg/><?xml-stylesheet href="x.css"?>"#;
        assert!(has(svg, SvgIssueKind::ProcessingInstruction));
        let declaration_only = r#"<?xml version="1.0" encoding="UTF-8"?><svg/>"#;
        assert!(!has(declaration_only, SvgIssueKind::ProcessingInstruction));
    }

    #[test]
    fn malformed_xml_is_reported() {
        assert!(has(r#"<svg><rect></svg>"#, SvgIssueKind::Malformed));
        assert!(has(r#"<svg"#, SvgIssueKind::Malformed));
        assert!(has(r#"<svg attr=1/>"#, SvgIssueKind::Malformed));
        assert!(has(r#"<svg><g/>"#, SvgIssueKind::Malformed));
    }

    #[test]
    fn invalid_utf8_is_reported_as_malformed() {
        let bytes = [b'<', b's', b'v', b'g', b'>', 0xff, 0xfe];
        let check = check_svg(&bytes);
        assert_eq!(check.issues.len(), 1, "{:?}", check.issues);
        assert_eq!(check.issues[0].kind, SvgIssueKind::Malformed);
        assert_eq!(check.issues[0].byte_offset, 0);
        assert_eq!(check.elements, 0);
    }

    #[test]
    fn never_panics_on_arbitrary_bytes() {
        let inputs: [&[u8]; 8] = [
            b"",
            b"<",
            b"<svg",
            b"</svg>",
            b"\0\0\0",
            b"<svg><style>url(</style></svg>",
            b"<svg><![CDATA[",
            &[0x80, 0x81, 0x0a],
        ];
        for input in inputs {
            let check = check_svg(input);
            let _ = check.is_safe();
        }
    }

    #[test]
    fn external_url_targets_ignores_identifiers_and_trims_quotes() {
        assert!(external_url_targets("fill: #fff").is_empty());
        assert!(external_url_targets("width: myurl(http://x/y)").is_empty());
        assert_eq!(
            external_url_targets(r#"a{background:URL( "http://x/y" )}"#),
            vec!["http://x/y".to_string()]
        );
    }
}
