//! Declarative inputs captured from user requests BEFORE calling the editor.
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, ops::Range};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Alignment {
    Left,
    Right,
    Center,
    Justify,
    Distribute,
    DistributeSpace,
}
impl Alignment {
    pub(crate) fn value(self) -> &'static str {
        match self {
            Self::Left => "LEFT",
            Self::Right => "RIGHT",
            Self::Center => "CENTER",
            Self::Justify => "JUSTIFY",
            Self::Distribute => "DISTRIBUTE",
            Self::DistributeSpace => "DISTRIBUTE_SPACE",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LineSpacing {
    pub kind: LineSpacingKind,
    pub value: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LineSpacingKind {
    Percent,
}

/// Request properties, deliberately independent from hae-edit's type.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CharProps {
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub strikeout: Option<bool>,
    pub height_hwpunit: Option<u32>,
    pub text_color: Option<String>,
}

/// Equation request properties are independent of the editor's type.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EquationProps {
    pub base_line: Option<u32>,
    pub text_color: Option<String>,
    pub font: Option<String>,
    pub base_unit: Option<u32>,
    pub line_mode: Option<String>,
    pub version: Option<String>,
}

/// Paths count ELEMENT children, root = []. Requests are applied in order to
/// an independently derived state, never to editor-produced text. A range is
/// measured in extended graphemes of that step's pre-edit paragraph.
/// Cache removal happens only after all requests, so source paths stay stable.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", deny_unknown_fields)]
pub enum VerifyOperation {
    #[serde(rename = "equation.setScript")]
    EquationSetScript {
        entry: String,
        path: Vec<usize>,
        script: String,
    },
    #[serde(rename = "equation.setProperties")]
    EquationSetProperties {
        entry: String,
        path: Vec<usize>,
        props: EquationProps,
    },
    #[serde(rename = "text.format")]
    TextFormat {
        entry: String,
        path: Vec<usize>,
        range: Range<usize>,
        props: CharProps,
        #[serde(default)]
        preserve: bool,
    },
    #[serde(rename = "text.replace")]
    TextReplace {
        entry: String,
        path: Vec<usize>,
        range: Range<usize>,
        replacement: String,
        #[serde(default)]
        preserve: bool,
    },
    /// Exact codepoint search-and-replace over every paragraph of the document,
    /// or of the `scope` subtree. The gate derives the match set itself.
    #[serde(rename = "text.replaceAll")]
    TextReplaceAll {
        find: String,
        replacement: String,
        scope: Option<VerifyScope>,
        #[serde(rename = "maxMatches")]
        max_matches: usize,
    },
    #[serde(rename = "cell.setText")]
    CellSetText {
        entry: String,
        path: Vec<usize>,
        text: String,
        #[serde(default)]
        preserve: bool,
    },
    #[serde(rename = "cell.setParagraphs")]
    CellSetParagraphs {
        entry: String,
        path: Vec<usize>,
        paragraphs: Vec<String>,
        #[serde(default)]
        preserve: bool,
    },
    #[serde(rename = "paragraph.format")]
    ParagraphFormat {
        entry: String,
        path: Vec<usize>,
        alignment: Option<Alignment>,
        #[serde(rename = "lineSpacing")]
        line_spacing: Option<LineSpacing>,
    },
    /// Fixed request bytes, not a pathname or an editor-reported hash. Static
    /// image decoding/CRC validation belongs to input validation; this gate
    /// independently checks signature, dimensions, container shape and exact
    /// payload equality. See crate documentation for the bounded contract.
    #[serde(rename = "image.replace")]
    ImageReplace {
        entry: String,
        path: Vec<usize>,
        bytes: Vec<u8>,
    },
}
/// Structural address of the element whose descendant paragraphs are searched.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifyScope {
    pub entry: String,
    pub path: Vec<usize>,
}
impl VerifyOperation {
    pub(crate) fn target(&self) -> (&str, &[usize]) {
        match self {
            Self::TextReplaceAll { scope, .. } => scope
                .as_ref()
                .map_or(("", &[][..]), |s| (s.entry.as_str(), s.path.as_slice())),
            Self::EquationSetScript { entry, path, .. }
            | Self::EquationSetProperties { entry, path, .. }
            | Self::TextFormat { entry, path, .. }
            | Self::TextReplace { entry, path, .. }
            | Self::CellSetText { entry, path, .. }
            | Self::CellSetParagraphs { entry, path, .. }
            | Self::ParagraphFormat { entry, path, .. }
            | Self::ImageReplace { entry, path, .. } => (entry, path),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyReport {
    /// Payload changes, separately from the layout-cache removal counts.
    pub changed_entries: Vec<String>,
    pub cache_removals: BTreeMap<String, usize>,
    pub prv_text_stale: bool,
    pub thumbnail_stale: bool,
}
