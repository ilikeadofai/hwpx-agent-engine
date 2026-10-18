//! hae-hwp5: read-only inspection of legacy HWP 5.0 (`.hwp`) binary documents.
//!
//! Scope (v1): open an MS-CFB container, read the `FileHeader` flags, and walk the
//! `BodyText/SectionN` record streams to extract paragraph text (UTF-16LE) for
//! `hwp5.inspect`. Encrypted, DRM and distribution documents are refused. Nothing in
//! this crate writes to the input: the API takes borrowed bytes and returns owned data.
//!
//! The format facts behind this reader are recorded in
//! `docs/decisions/hwp5-format-notes.md`; unverified areas are refused rather than
//! guessed.

mod cfb;
pub mod fixture;
mod header;
mod inspect;
mod records;

pub use header::{FileHeader, Version};
pub use inspect::{inspect, InspectOptions, InspectReport, ParagraphSummary, SectionSummary};

use hae_diag::PackageRejectReason as R;

pub(crate) fn reject(reason: R, message: impl Into<String>) -> hae_diag::EngineError {
    hae_diag::EngineError::rejected(reason, message)
}
