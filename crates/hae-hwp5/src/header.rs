//! `FileHeader` stream parsing and support checks.
//!
//! Layout confirmed in `docs/decisions/hwp5-format-notes.md` §1.2: a 32-byte
//! `"HWP Document File"` signature field, a `major.minor.build.revision` version
//! u32 at 0x20 and an attribute bit field at 0x24.

use crate::reject;
use hae_diag::{PackageRejectReason as R, Result};

/// HWP 5.0 `FileHeader` signature field (32 bytes, NUL-padded).
const SIGNATURE: [u8; 17] = *b"HWP Document File";

/// The stream is a fixed 256-byte structure.
const HEADER_SIZE: usize = 256;

/// Bit 0: streams (DocInfo/BodyText/...) are raw-DEFLATE compressed.
pub const COMPRESSED: u32 = 1 << 0;
/// Bit 1: the document needs a password; streams are encrypted.
pub const ENCRYPTED: u32 = 1 << 1;
/// Bit 2: distribution document (`ViewText` + AES); unsupported here.
pub const DISTRIBUTION: u32 = 1 << 2;
/// Bit 4: DRM-protected document; treated as encrypted for support checks.
pub const DRM: u32 = 1 << 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Version {
    pub major: u8,
    pub minor: u8,
    pub build: u8,
    pub revision: u8,
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}.{}.{}.{}",
            self.major, self.minor, self.build, self.revision
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileHeader {
    pub version: Version,
    pub properties: u32,
}

impl FileHeader {
    /// Parses a 256-byte `FileHeader` stream.
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() < HEADER_SIZE {
            return Err(reject(
                R::MalformedRecordStream,
                format!("FileHeader is {} bytes, expected {HEADER_SIZE}", data.len()),
            ));
        }
        let signature_field = &data[..32];
        if signature_field[..SIGNATURE.len()] != SIGNATURE
            || signature_field[SIGNATURE.len()..].iter().any(|b| *b != 0)
        {
            return Err(reject(
                R::MalformedRecordStream,
                "FileHeader signature is not \"HWP Document File\"",
            ));
        }
        let raw_version = u32::from_le_bytes(data[0x20..0x24].try_into().unwrap());
        let version = Version {
            major: (raw_version >> 24) as u8,
            minor: (raw_version >> 16) as u8,
            build: (raw_version >> 8) as u8,
            revision: raw_version as u8,
        };
        let properties = u32::from_le_bytes(data[0x24..0x28].try_into().unwrap());
        Ok(FileHeader {
            version,
            properties,
        })
    }

    pub fn is_compressed(&self) -> bool {
        self.properties & COMPRESSED != 0
    }

    pub fn is_encrypted(&self) -> bool {
        self.properties & ENCRYPTED != 0
    }

    pub fn is_distribution(&self) -> bool {
        self.properties & DISTRIBUTION != 0
    }

    pub fn is_drm(&self) -> bool {
        self.properties & DRM != 0
    }

    /// Refuses documents whose body streams this reader cannot decode.
    pub fn check_supported(&self) -> Result<()> {
        if self.is_distribution() {
            return Err(reject(
                R::DistributionUnsupported,
                "distribution documents store encrypted ViewText instead of BodyText",
            ));
        }
        if self.is_encrypted() {
            return Err(reject(
                R::Encrypted,
                "password-protected HWP5 documents are not supported",
            ));
        }
        if self.is_drm() {
            return Err(reject(
                R::Encrypted,
                "DRM-protected HWP5 documents are treated as encrypted and are not supported",
            ));
        }
        Ok(())
    }
}
