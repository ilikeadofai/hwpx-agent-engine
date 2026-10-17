//! hae-diag: Diagnostic codes, diagnostics, limits and resource budget types.
//!
//! The code list is normative in docs/decisions/diagnostic-codes.md.

use serde::{Deserialize, Serialize};

macro_rules! codes {
    ($( $variant:ident => $s:literal, $sec:expr, $warn:literal; )*) => {
        /// Every diagnostic code the engine can return.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(rename_all = "SCREAMING_SNAKE_CASE")]
        pub enum Code { $( $variant, )* }

        impl Code {
            /// All codes in table order.
            pub const ALL: [Code; 34] = [ $( Code::$variant, )* ];

            /// The wire string of the code.
            pub fn as_str(self) -> &'static str {
                match self { $( Code::$variant => $s, )* }
            }

            /// Design spec section, or `None` for codes added during implementation.
            pub fn spec_section(self) -> Option<&'static str> {
                match self { $( Code::$variant => $sec, )* }
            }

            /// Codes that are warnings rather than errors.
            pub fn is_warning(self) -> bool {
                match self { $( Code::$variant => $warn, )* }
            }
        }
    };
}

codes! {
    PackageRejected => "PACKAGE_REJECTED", Some("§11"), false;
    ResourceLimit => "RESOURCE_LIMIT", Some("§8·§11"), false;
    StateInvariantError => "STATE_INVARIANT_ERROR", Some("§4"), false;
    ProjectIntegrityError => "PROJECT_INTEGRITY_ERROR", Some("§5"), false;
    ProjectLocked => "PROJECT_LOCKED", Some("§5·§11"), false;
    ExternalModification => "EXTERNAL_MODIFICATION", Some("§11"), false;
    SaveDurabilityUncertain => "SAVE_DURABILITY_UNCERTAIN", Some("§11"), false;
    VersionConflict => "VERSION_CONFLICT", Some("§6"), false;
    HistoryEmpty => "HISTORY_EMPTY", Some("§6·§8"), false;
    SnapshotUnavailable => "SNAPSHOT_UNAVAILABLE", Some("§8"), false;
    SessionMismatch => "SESSION_MISMATCH", Some("§6"), false;
    SessionRequired => "SESSION_REQUIRED", Some("§6"), false;
    UnknownWriter => "UNKNOWN_WRITER", Some("§6"), false;
    RequestIdConflict => "REQUEST_ID_CONFLICT", Some("§6"), false;
    ReplayTooOld => "REPLAY_TOO_OLD", Some("§6"), false;
    RequestOutOfOrder => "REQUEST_OUT_OF_ORDER", Some("§6"), false;
    TargetNotFound => "TARGET_NOT_FOUND", Some("§6"), false;
    AmbiguousTarget => "AMBIGUOUS_TARGET", Some("§6"), false;
    Unsupported => "UNSUPPORTED", Some("§6"), false;
    InvalidArgument => "INVALID_ARGUMENT", None, false;
    InvalidRange => "INVALID_RANGE", Some("§6"), false;
    RangeCrossesControl => "RANGE_CROSSES_CONTROL", Some("§6"), false;
    UnsupportedContentInRange => "UNSUPPORTED_CONTENT_IN_RANGE", Some("§6"), false;
    ControlsPreserved => "CONTROLS_PRESERVED", Some("0.2 §3"), true;
    FormatCollapsed => "FORMAT_COLLAPSED", Some("§6"), true;
    MatchesSkipped => "MATCHES_SKIPPED", Some("0.2 §7-2"), true;
    RendererUnavailable => "RENDERER_UNAVAILABLE", Some("§9"), false;
    PreviewPageLimit => "PREVIEW_PAGE_LIMIT", Some("§10"), true;
    InvalidRequest => "INVALID_REQUEST", None, false;
    UnknownCommand => "UNKNOWN_COMMAND", None, false;
    PathNotAllowed => "PATH_NOT_ALLOWED", None, false;
    TargetExists => "TARGET_EXISTS", None, false;
    VerifyFailed => "VERIFY_FAILED", None, false;
    IoError => "IO_ERROR", None, false;
}

impl std::fmt::Display for Code {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

macro_rules! reasons {
    ($( $variant:ident => $s:literal; )*) => {
        /// Reason attached to `PACKAGE_REJECTED`.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum PackageRejectReason { $( $variant, )* }

        impl PackageRejectReason {
            pub const ALL: [PackageRejectReason; 18] = [ $( PackageRejectReason::$variant, )* ];
            pub fn as_str(self) -> &'static str {
                match self { $( PackageRejectReason::$variant => $s, )* }
            }
        }
    };
}

reasons! {
    Encrypted => "encrypted";
    DuplicateName => "duplicate_name";
    CaseCollision => "case_collision";
    PathEscape => "path_escape";
    Zip64Required => "zip64_required";
    BadMimetype => "bad_mimetype";
    BrokenManifest => "broken_manifest";
    DtdOrEntity => "dtd_or_entity";
    NonUtf8 => "non_utf8";
    MalformedZip => "malformed_zip";
    UnsupportedCompression => "unsupported_compression";
    MultiDisk => "multi_disk";
    CrcMismatch => "crc_mismatch";
    SizeMismatch => "size_mismatch";
    HeaderMismatch => "header_mismatch";
    OverlappingEntries => "overlapping_entries";
    MalformedXml => "malformed_xml";
    ForeignNamespace => "foreign_namespace";
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Stage {
    Request,
    Open,
    Query,
    Validate,
    Apply,
    Commit,
    Save,
    Export,
    Render,
}

/// One diagnostic: code, target nodeId, stage and severity (spec §6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub code: Code,
    pub severity: Severity,
    pub stage: Stage,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub target: Option<String>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub detail: Option<serde_json::Value>,
}

impl Diagnostic {
    pub fn error(code: Code, stage: Stage, message: impl Into<String>) -> Self {
        Diagnostic {
            code,
            severity: if code.is_warning() {
                Severity::Warning
            } else {
                Severity::Error
            },
            stage,
            target: None,
            message: message.into(),
            reason: None,
            detail: None,
        }
    }
    pub fn with_target(mut self, target: impl Into<String>) -> Self {
        self.target = Some(target.into());
        self
    }
    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }
    pub fn with_detail(mut self, detail: serde_json::Value) -> Self {
        self.detail = Some(detail);
        self
    }
}

/// The engine's error type: one boxed diagnostic.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{}: {}", .0.code, .0.message)]
pub struct EngineError(pub Box<Diagnostic>);

impl EngineError {
    pub fn new(code: Code, stage: Stage, message: impl Into<String>) -> Self {
        EngineError(Box::new(Diagnostic::error(code, stage, message)))
    }
    pub fn rejected(reason: PackageRejectReason, message: impl Into<String>) -> Self {
        EngineError::new(Code::PackageRejected, Stage::Open, message).with_reason(reason.as_str())
    }
    pub fn code(&self) -> Code {
        self.0.code
    }
    pub fn reason(&self) -> Option<&str> {
        self.0.reason.as_deref()
    }
    pub fn diagnostic(&self) -> &Diagnostic {
        &self.0
    }
    pub fn with_target(mut self, target: impl Into<String>) -> Self {
        self.0.target = Some(target.into());
        self
    }
    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.0.reason = Some(reason.into());
        self
    }
    pub fn with_detail(mut self, detail: serde_json::Value) -> Self {
        self.0.detail = Some(detail);
        self
    }
}

pub type Result<T> = std::result::Result<T, EngineError>;

const MIB: u64 = 1024 * 1024;

/// Default limits from spec §8·§10·§11.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_hwpx_bytes: u64,
    pub max_project_bytes: u64,
    pub max_inflate_total: u64,
    pub max_entries: u64,
    pub max_xml_depth: u64,
    pub max_image_pixels: u64,
    pub max_preview_pages: u64,
    pub max_session_memory: u64,
    pub max_request_bytes: u64,
    pub max_response_bytes: u64,
    pub max_history_units: u64,
    pub renderer_timeout_secs: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_hwpx_bytes: 32 * MIB,
            max_project_bytes: 96 * MIB,
            max_inflate_total: 128 * MIB,
            max_entries: 2048,
            max_xml_depth: 128,
            max_image_pixels: 40_960_000,
            max_preview_pages: 100,
            max_session_memory: 512 * MIB,
            max_request_bytes: MIB,
            max_response_bytes: 4 * MIB,
            max_history_units: 100,
            renderer_timeout_secs: 60,
        }
    }
}

/// A byte budget that refuses reservations past its limit (spec §11 cumulative limits).
#[derive(Debug, Clone)]
pub struct Budget {
    limit: u64,
    used: u64,
}

impl Budget {
    pub fn new(limit: u64) -> Self {
        Budget { limit, used: 0 }
    }
    pub fn used(&self) -> u64 {
        self.used
    }
    pub fn limit(&self) -> u64 {
        self.limit
    }
    pub fn remaining(&self) -> u64 {
        self.limit.saturating_sub(self.used)
    }
    pub fn try_reserve(&mut self, bytes: u64, what: &str) -> Result<()> {
        match self.used.checked_add(bytes) {
            Some(total) if total <= self.limit => {
                self.used = total;
                Ok(())
            }
            _ => Err(EngineError::new(
                Code::ResourceLimit,
                Stage::Validate,
                format!("{what}: limit {} bytes exceeded", self.limit),
            )
            .with_detail(serde_json::json!({
                "limit": self.limit, "used": self.used, "requested": bytes
            }))),
        }
    }
    pub fn release(&mut self, bytes: u64) {
        self.used = self.used.saturating_sub(bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    const DOC_CODES: [&str; 34] = [
        "PACKAGE_REJECTED",
        "RESOURCE_LIMIT",
        "STATE_INVARIANT_ERROR",
        "PROJECT_INTEGRITY_ERROR",
        "PROJECT_LOCKED",
        "EXTERNAL_MODIFICATION",
        "SAVE_DURABILITY_UNCERTAIN",
        "VERSION_CONFLICT",
        "HISTORY_EMPTY",
        "SNAPSHOT_UNAVAILABLE",
        "SESSION_MISMATCH",
        "SESSION_REQUIRED",
        "UNKNOWN_WRITER",
        "REQUEST_ID_CONFLICT",
        "REPLAY_TOO_OLD",
        "REQUEST_OUT_OF_ORDER",
        "TARGET_NOT_FOUND",
        "AMBIGUOUS_TARGET",
        "UNSUPPORTED",
        "INVALID_ARGUMENT",
        "INVALID_RANGE",
        "RANGE_CROSSES_CONTROL",
        "UNSUPPORTED_CONTENT_IN_RANGE",
        "CONTROLS_PRESERVED",
        "FORMAT_COLLAPSED",
        "MATCHES_SKIPPED",
        "RENDERER_UNAVAILABLE",
        "PREVIEW_PAGE_LIMIT",
        "INVALID_REQUEST",
        "UNKNOWN_COMMAND",
        "PATH_NOT_ALLOWED",
        "TARGET_EXISTS",
        "VERIFY_FAILED",
        "IO_ERROR",
    ];

    #[test]
    fn codes_match_doc_and_roundtrip() {
        let got: BTreeSet<&str> = Code::ALL.iter().map(|c| c.as_str()).collect();
        let want: BTreeSet<&str> = DOC_CODES.into_iter().collect();
        assert_eq!(got, want);
        assert_eq!(got.len(), 34);
        for c in Code::ALL {
            let j = serde_json::to_string(&c).unwrap();
            assert_eq!(j, format!("\"{}\"", c.as_str()));
            assert_eq!(serde_json::from_str::<Code>(&j).unwrap(), c);
            assert_eq!(c.to_string(), c.as_str());
        }
    }

    #[test]
    fn reasons_roundtrip() {
        let set: BTreeSet<&str> = PackageRejectReason::ALL
            .iter()
            .map(|r| r.as_str())
            .collect();
        assert_eq!(set.len(), 18);
        for r in PackageRejectReason::ALL {
            let j = serde_json::to_string(&r).unwrap();
            assert_eq!(j, format!("\"{}\"", r.as_str()));
            assert_eq!(serde_json::from_str::<PackageRejectReason>(&j).unwrap(), r);
        }
    }

    #[test]
    fn warnings_and_sections() {
        let warns: Vec<Code> = Code::ALL.into_iter().filter(|c| c.is_warning()).collect();
        assert_eq!(
            warns,
            vec![
                Code::ControlsPreserved,
                Code::FormatCollapsed,
                Code::MatchesSkipped,
                Code::PreviewPageLimit
            ]
        );
        assert_eq!(Code::ResourceLimit.spec_section(), Some("§8·§11"));
        assert_eq!(Code::IoError.spec_section(), None);
        assert_eq!(
            Diagnostic::error(Code::FormatCollapsed, Stage::Apply, "x").severity,
            Severity::Warning
        );
    }

    #[test]
    fn diagnostic_json_shape() {
        let d = Diagnostic::error(Code::TargetNotFound, Stage::Query, "no such node");
        let v = serde_json::to_value(&d).unwrap();
        assert_eq!(
            v,
            serde_json::json!({"code":"TARGET_NOT_FOUND","severity":"error","stage":"query","message":"no such node"})
        );
        let d2 = d.with_target("n1").with_reason("r");
        let v2 = serde_json::to_value(&d2).unwrap();
        assert_eq!(v2["target"], "n1");
        assert_eq!(serde_json::from_value::<Diagnostic>(v2).unwrap(), d2);
    }

    #[test]
    fn error_display_and_rejected() {
        let e = EngineError::rejected(PackageRejectReason::PathEscape, "bad name");
        assert_eq!(e.to_string(), "PACKAGE_REJECTED: bad name");
        assert_eq!(e.reason(), Some("path_escape"));
        assert_eq!(e.diagnostic().stage, Stage::Open);
    }

    #[test]
    fn limits_default() {
        let l = Limits::default();
        assert_eq!(l.max_hwpx_bytes, 32 * MIB);
        assert_eq!(l.max_project_bytes, 96 * MIB);
        assert_eq!(l.max_inflate_total, 128 * MIB);
        assert_eq!(l.max_entries, 2048);
        assert_eq!(l.max_xml_depth, 128);
        assert_eq!(l.max_image_pixels, 40_960_000);
        assert_eq!(l.max_preview_pages, 100);
        assert_eq!(l.max_session_memory, 512 * MIB);
        assert_eq!(l.max_request_bytes, MIB);
        assert_eq!(l.max_response_bytes, 4 * MIB);
        assert_eq!(l.max_history_units, 100);
        assert_eq!(l.renderer_timeout_secs, 60);
    }

    #[test]
    fn budget() {
        let mut b = Budget::new(100);
        b.try_reserve(60, "a").unwrap();
        let e = b.try_reserve(41, "b").unwrap_err();
        assert_eq!(e.code(), Code::ResourceLimit);
        assert_eq!(b.used(), 60);
        b.try_reserve(40, "c").unwrap();
        assert_eq!(b.remaining(), 0);
        b.release(70);
        assert_eq!(b.used(), 30);
        b.release(1000);
        assert_eq!(b.used(), 0);
        assert!(b.try_reserve(u64::MAX, "overflow").is_err());
        b.try_reserve(1, "d").unwrap();
        assert!(b.try_reserve(u64::MAX, "overflow2").is_err());
    }
}
