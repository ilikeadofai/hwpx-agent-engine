//! hae-zip: ZIP reading with defensive parsing and raw-copy writing for HWPX packages.
//!
//! Reading validates the whole archive structure and inflates every entry once
//! (CRC and length checked against the cumulative inflate budget). Writing copies
//! unchanged entries byte-for-byte (local header, compressed data, data descriptor,
//! central record) and re-encodes only replaced or added entries (spec §4, §11).

use hae_diag::{Budget, Code, EngineError, Limits, PackageRejectReason as R, Result, Stage};
use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::ops::Range;
use std::sync::Arc;

const SIG_LOCAL: u32 = 0x0403_4b50;
const SIG_CENTRAL: u32 = 0x0201_4b50;
const SIG_EOCD: u32 = 0x0605_4b50;
const SIG_ZIP64_LOCATOR: u32 = 0x0706_4b50;
const SIG_DESCRIPTOR: u32 = 0x0807_4b50;
const FLAG_ENCRYPTED: u16 = 1;
const FLAG_DESCRIPTOR: u16 = 1 << 3;
const FLAG_STRONG_ENCRYPTION: u16 = 1 << 6;
const FLAG_UTF8: u16 = 1 << 11;
pub const METHOD_STORED: u16 = 0;
pub const METHOD_DEFLATED: u16 = 8;

fn rej(reason: R, msg: impl Into<String>) -> EngineError {
    EngineError::rejected(reason, msg)
}

fn u16_at(b: &[u8], at: usize) -> Result<u16> {
    b.get(
        at..at
            .checked_add(2)
            .ok_or_else(|| rej(R::MalformedZip, "offset overflow"))?,
    )
    .map(|s| u16::from_le_bytes([s[0], s[1]]))
    .ok_or_else(|| rej(R::MalformedZip, format!("truncated at {at}")))
}

fn u32_at(b: &[u8], at: usize) -> Result<u32> {
    b.get(
        at..at
            .checked_add(4)
            .ok_or_else(|| rej(R::MalformedZip, "offset overflow"))?,
    )
    .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    .ok_or_else(|| rej(R::MalformedZip, format!("truncated at {at}")))
}

fn slice(b: &[u8], start: usize, len: usize) -> Result<&[u8]> {
    let end = start
        .checked_add(len)
        .ok_or_else(|| rej(R::MalformedZip, "range overflow"))?;
    b.get(start..end).ok_or_else(|| {
        rej(
            R::MalformedZip,
            format!("range {start}..{end} outside file"),
        )
    })
}

/// One validated archive entry.
#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub flags: u16,
    pub method: u16,
    pub crc32: u32,
    pub compressed_size: u32,
    pub uncompressed_size: u32,
    /// Original local header, name and extra field bytes.
    local_header: Range<usize>,
    /// Compressed data bytes.
    data: Range<usize>,
    /// Data descriptor bytes (empty when absent).
    descriptor: Range<usize>,
    /// Original central directory record (fixed part, name, extra, comment).
    central: Vec<u8>,
    payload: Arc<[u8]>,
}

impl Entry {
    /// The decompressed payload.
    pub fn payload(&self) -> &Arc<[u8]> {
        &self.payload
    }
    pub fn is_stored(&self) -> bool {
        self.method == METHOD_STORED
    }
}

/// A validated ZIP archive held in memory.
#[derive(Debug, Clone)]
pub struct Archive {
    raw: Arc<[u8]>,
    entries: Vec<Entry>,
    index: HashMap<String, usize>,
    eocd_comment: Vec<u8>,
    inflated_total: u64,
}

fn check_name(name: &str) -> Result<()> {
    let bad = name.is_empty()
        || name.starts_with('/')
        || name.contains('\\')
        || name.contains('\0')
        || name.contains(':')
        || name
            .split('/')
            .any(|seg| seg == ".." || seg == "." || (seg.is_empty() && !name.ends_with('/')));
    if bad {
        return Err(rej(R::PathEscape, format!("unsafe entry name {name:?}")));
    }
    Ok(())
}

fn decode_name(raw: &[u8], flags: u16) -> Result<String> {
    if flags & FLAG_UTF8 == 0 && !raw.is_ascii() {
        return Err(rej(R::NonUtf8, "non-ASCII entry name without UTF-8 flag"));
    }
    String::from_utf8(raw.to_vec()).map_err(|_| rej(R::NonUtf8, "entry name is not valid UTF-8"))
}

fn has_zip64_extra(mut extra: &[u8]) -> bool {
    while extra.len() >= 4 {
        let id = u16::from_le_bytes([extra[0], extra[1]]);
        let len = u16::from_le_bytes([extra[2], extra[3]]) as usize;
        if id == 0x0001 {
            return true;
        }
        if extra.len() < 4 + len {
            return false;
        }
        extra = &extra[4 + len..];
    }
    false
}

fn find_eocd(b: &[u8]) -> Result<usize> {
    if b.len() < 22 {
        return Err(rej(R::MalformedZip, "file too short for a ZIP archive"));
    }
    let lowest = b.len().saturating_sub(22 + 0xFFFF);
    let mut found = Vec::new();
    let mut i = b.len() - 22;
    loop {
        if u32_at(b, i)? == SIG_EOCD {
            let clen = u16_at(b, i + 20)? as usize;
            if i + 22 + clen == b.len() {
                found.push(i);
            }
        }
        if i == lowest {
            break;
        }
        i -= 1;
    }
    match found.len() {
        1 => Ok(found[0]),
        0 => Err(rej(R::MalformedZip, "end of central directory not found")),
        _ => Err(rej(R::MalformedZip, "ambiguous end of central directory")),
    }
}

fn inflate(entry: &Entry, data: &[u8], budget: &mut Budget) -> Result<Arc<[u8]>> {
    let declared = entry.uncompressed_size as u64;
    budget.try_reserve(declared, &format!("inflate {}", entry.name))?;
    let mut out = Vec::with_capacity(declared.min(1 << 20) as usize);
    match entry.method {
        METHOD_STORED => {
            if entry.compressed_size != entry.uncompressed_size {
                return Err(rej(
                    R::SizeMismatch,
                    format!("{}: stored sizes differ", entry.name),
                ));
            }
            out.extend_from_slice(data);
        }
        METHOD_DEFLATED => {
            let dec = flate2::read::DeflateDecoder::new(data);
            // Read at most declared+1 bytes so a lying size cannot exhaust memory.
            dec.take(declared + 1).read_to_end(&mut out).map_err(|e| {
                rej(
                    R::MalformedZip,
                    format!("{}: inflate failed: {e}", entry.name),
                )
            })?;
        }
        m => {
            return Err(rej(
                R::UnsupportedCompression,
                format!("{}: method {m}", entry.name),
            ));
        }
    }
    if out.len() as u64 != declared {
        return Err(rej(
            R::SizeMismatch,
            format!(
                "{}: inflated {} bytes, declared {declared}",
                entry.name,
                out.len()
            ),
        ));
    }
    if crc32fast::hash(&out) != entry.crc32 {
        return Err(rej(R::CrcMismatch, format!("{}: CRC mismatch", entry.name)));
    }
    Ok(Arc::from(out))
}

impl Archive {
    /// Parse and fully validate an archive. Every entry is inflated once.
    pub fn open(bytes: Vec<u8>, limits: &Limits) -> Result<Archive> {
        let mut budget = Budget::new(limits.max_inflate_total);
        Self::open_with_budget(bytes, limits, &mut budget)
    }

    /// Like [`Archive::open`] but charges inflation to a caller-owned budget
    /// (used when several archives share one cumulative limit, e.g. a project).
    pub fn open_with_budget(
        bytes: Vec<u8>,
        limits: &Limits,
        budget: &mut Budget,
    ) -> Result<Archive> {
        if bytes.len() as u64 > limits.max_hwpx_bytes {
            return Err(EngineError::new(
                Code::ResourceLimit,
                Stage::Open,
                format!(
                    "archive is {} bytes, limit {}",
                    bytes.len(),
                    limits.max_hwpx_bytes
                ),
            ));
        }
        let b: &[u8] = &bytes;
        let eocd = find_eocd(b)?;
        if eocd >= 20 && u32_at(b, eocd - 20)? == SIG_ZIP64_LOCATOR {
            return Err(rej(
                R::Zip64Required,
                "ZIP64 end of central directory locator present",
            ));
        }
        let disk = u16_at(b, eocd + 4)?;
        let cd_disk = u16_at(b, eocd + 6)?;
        let n_disk = u16_at(b, eocd + 8)?;
        let n_total = u16_at(b, eocd + 10)?;
        let cd_size = u32_at(b, eocd + 12)?;
        let cd_off = u32_at(b, eocd + 16)?;
        if n_total == 0xFFFF || cd_size == 0xFFFF_FFFF || cd_off == 0xFFFF_FFFF {
            return Err(rej(
                R::Zip64Required,
                "ZIP64 sentinel in end of central directory",
            ));
        }
        if disk != 0 || cd_disk != 0 || n_disk != n_total {
            return Err(rej(R::MultiDisk, "multi-disk archives are not supported"));
        }
        if n_total as u64 > limits.max_entries {
            return Err(EngineError::new(
                Code::ResourceLimit,
                Stage::Open,
                format!("{n_total} entries, limit {}", limits.max_entries),
            ));
        }
        let cd_start = cd_off as usize;
        let cd_end = cd_start
            .checked_add(cd_size as usize)
            .ok_or_else(|| rej(R::MalformedZip, "central directory overflow"))?;
        if cd_end != eocd {
            return Err(rej(
                R::MalformedZip,
                "central directory does not end at EOCD",
            ));
        }
        let eocd_comment = b[eocd + 22..].to_vec();

        let mut entries: Vec<Entry> = Vec::with_capacity(n_total as usize);
        let mut seen = HashSet::new();
        let mut seen_folded = HashSet::new();
        let mut local_offsets = HashSet::new();
        // Pre-pass: records sharing a local header overlap by definition.
        {
            let mut q = cd_start;
            let mut offs = HashSet::new();
            for _ in 0..n_total {
                if q + 46 > cd_end || u32_at(b, q)? != SIG_CENTRAL {
                    return Err(rej(
                        R::MalformedZip,
                        format!("bad central directory record at {q}"),
                    ));
                }
                if !offs.insert(u32_at(b, q + 42)?) {
                    return Err(rej(
                        R::OverlappingEntries,
                        "two entries share a local header",
                    ));
                }
                q += 46
                    + u16_at(b, q + 28)? as usize
                    + u16_at(b, q + 30)? as usize
                    + u16_at(b, q + 32)? as usize;
            }
        }
        let mut p = cd_start;
        for _ in 0..n_total {
            if p + 46 > cd_end || u32_at(b, p)? != SIG_CENTRAL {
                return Err(rej(
                    R::MalformedZip,
                    format!("bad central directory record at {p}"),
                ));
            }
            let flags = u16_at(b, p + 8)?;
            let method = u16_at(b, p + 10)?;
            let crc32 = u32_at(b, p + 16)?;
            let csize = u32_at(b, p + 20)?;
            let usize_ = u32_at(b, p + 24)?;
            let nlen = u16_at(b, p + 28)? as usize;
            let elen = u16_at(b, p + 30)? as usize;
            let clen = u16_at(b, p + 32)? as usize;
            let disk_start = u16_at(b, p + 34)?;
            let local_off = u32_at(b, p + 42)?;
            let rec_len = 46 + nlen + elen + clen;
            if p + rec_len > cd_end {
                return Err(rej(R::MalformedZip, "central record runs past directory"));
            }
            let raw_name = slice(b, p + 46, nlen)?;
            let extra = slice(b, p + 46 + nlen, elen)?;
            let central = b[p..p + rec_len].to_vec();
            p += rec_len;

            if flags & (FLAG_ENCRYPTED | FLAG_STRONG_ENCRYPTION) != 0 {
                return Err(rej(R::Encrypted, "encrypted entry"));
            }
            if csize == 0xFFFF_FFFF
                || usize_ == 0xFFFF_FFFF
                || local_off == 0xFFFF_FFFF
                || has_zip64_extra(extra)
            {
                return Err(rej(R::Zip64Required, "ZIP64 entry"));
            }
            if disk_start != 0 {
                return Err(rej(R::MultiDisk, "entry on another disk"));
            }
            if method == 99 {
                return Err(rej(R::Encrypted, "AES-encrypted entry (method 99)"));
            }
            if method != METHOD_STORED && method != METHOD_DEFLATED {
                return Err(rej(
                    R::UnsupportedCompression,
                    format!("compression method {method}"),
                ));
            }
            let name = decode_name(raw_name, flags)?;
            check_name(&name)?;
            if !seen.insert(name.clone()) {
                return Err(rej(R::DuplicateName, format!("duplicate entry {name}")));
            }
            if !seen_folded.insert(name.to_lowercase()) {
                return Err(rej(R::CaseCollision, format!("case collision on {name}")));
            }

            // Two records sharing a local header overlap by definition.
            if !local_offsets.insert(local_off) {
                return Err(rej(
                    R::OverlappingEntries,
                    format!("{name}: local header shared with another entry"),
                ));
            }
            // Local header must agree with the central record.
            let lo = local_off as usize;
            if u32_at(b, lo)? != SIG_LOCAL {
                return Err(rej(
                    R::MalformedZip,
                    format!("{name}: bad local header signature"),
                ));
            }
            let l_flags = u16_at(b, lo + 6)?;
            let l_method = u16_at(b, lo + 8)?;
            let l_crc = u32_at(b, lo + 14)?;
            let l_csize = u32_at(b, lo + 18)?;
            let l_usize = u32_at(b, lo + 22)?;
            let l_nlen = u16_at(b, lo + 26)? as usize;
            let l_elen = u16_at(b, lo + 28)? as usize;
            if slice(b, lo + 30, l_nlen)? != raw_name {
                return Err(rej(
                    R::HeaderMismatch,
                    format!("{name}: local and central names differ"),
                ));
            }
            if l_flags != flags || l_method != method {
                return Err(rej(
                    R::HeaderMismatch,
                    format!("{name}: local and central flags or method differ"),
                ));
            }
            if has_zip64_extra(slice(b, lo + 30 + l_nlen, l_elen)?) {
                return Err(rej(R::Zip64Required, format!("{name}: ZIP64 local extra")));
            }
            let data_start = lo + 30 + l_nlen + l_elen;
            let data_end = data_start
                .checked_add(csize as usize)
                .ok_or_else(|| rej(R::MalformedZip, "data range overflow"))?;
            if data_end > cd_start {
                return Err(rej(
                    R::MalformedZip,
                    format!("{name}: data runs into central directory"),
                ));
            }
            let mut desc_end = data_end;
            if flags & FLAG_DESCRIPTOR != 0 {
                // Optional signature, then crc, csize, usize (32-bit).
                let mut q = data_end;
                if u32_at(b, q)? == SIG_DESCRIPTOR {
                    q += 4;
                }
                let (d_crc, d_c, d_u) = (u32_at(b, q)?, u32_at(b, q + 4)?, u32_at(b, q + 8)?);
                if d_crc != crc32 {
                    return Err(rej(
                        R::CrcMismatch,
                        format!("{name}: descriptor CRC differs"),
                    ));
                }
                if d_c != csize || d_u != usize_ {
                    return Err(rej(
                        R::SizeMismatch,
                        format!("{name}: descriptor sizes differ"),
                    ));
                }
                desc_end = q + 12;
                if desc_end > cd_start {
                    return Err(rej(
                        R::MalformedZip,
                        format!("{name}: descriptor runs into central directory"),
                    ));
                }
            } else {
                if l_crc != crc32 {
                    return Err(rej(
                        R::CrcMismatch,
                        format!("{name}: local and central CRC differ"),
                    ));
                }
                if l_csize != csize || l_usize != usize_ {
                    return Err(rej(
                        R::SizeMismatch,
                        format!("{name}: local and central sizes differ"),
                    ));
                }
            }
            entries.push(Entry {
                name,
                flags,
                method,
                crc32,
                compressed_size: csize,
                uncompressed_size: usize_,
                local_header: lo..data_start,
                data: data_start..data_end,
                descriptor: data_end..desc_end,
                central,
                payload: Arc::from(Vec::new()),
            });
        }
        if p != cd_end {
            return Err(rej(R::MalformedZip, "trailing bytes in central directory"));
        }

        // Entries must not overlap.
        let mut spans: Vec<(usize, usize)> = entries
            .iter()
            .map(|e| (e.local_header.start, e.descriptor.end))
            .collect();
        spans.sort_unstable();
        for w in spans.windows(2) {
            if w[1].0 < w[0].1 {
                return Err(rej(R::OverlappingEntries, "archive entries overlap"));
            }
        }

        let start_used = budget.used();
        for e in entries.iter_mut() {
            e.payload = inflate(e, &b[e.data.clone()], budget)?;
        }
        let inflated_total = budget.used() - start_used;
        let index = entries
            .iter()
            .enumerate()
            .map(|(i, e)| (e.name.clone(), i))
            .collect();
        Ok(Archive {
            raw: Arc::from(bytes),
            entries,
            index,
            eocd_comment,
            inflated_total,
        })
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn get(&self, name: &str) -> Option<&Entry> {
        self.index.get(name).map(|&i| &self.entries[i])
    }
    pub fn payload(&self, name: &str) -> Option<&Arc<[u8]>> {
        self.get(name).map(|e| &e.payload)
    }
    pub fn raw_bytes(&self) -> &Arc<[u8]> {
        &self.raw
    }
    /// Total bytes inflated while opening.
    pub fn inflated_total(&self) -> u64 {
        self.inflated_total
    }
    /// Compressed data bytes of an entry, exactly as stored.
    pub fn raw_data(&self, name: &str) -> Option<&[u8]> {
        self.get(name).map(|e| &self.raw[e.data.clone()])
    }
    /// Local header (with name and extra) of an entry, exactly as stored.
    pub fn raw_local_header(&self, name: &str) -> Option<&[u8]> {
        self.get(name).map(|e| &self.raw[e.local_header.clone()])
    }
}

/// What to write for one output entry.
#[derive(Debug, Clone)]
pub enum OutEntry {
    /// Copy an original entry byte-for-byte.
    Keep(String),
    /// Same name and header metadata as the original entry, new payload.
    Replace(String, Arc<[u8]>),
    /// A new entry (appended after the originals by convention).
    Add {
        name: String,
        payload: Arc<[u8]>,
        method: u16,
    },
}

struct Written {
    central: Vec<u8>,
}

fn put16(v: &mut Vec<u8>, x: u16) {
    v.extend_from_slice(&x.to_le_bytes());
}
fn put32(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&x.to_le_bytes());
}
fn set32(v: &mut [u8], at: usize, x: u32) {
    v[at..at + 4].copy_from_slice(&x.to_le_bytes());
}
fn set16(v: &mut [u8], at: usize, x: u16) {
    v[at..at + 2].copy_from_slice(&x.to_le_bytes());
}

fn compress(method: u16, payload: &[u8]) -> Result<Vec<u8>> {
    match method {
        METHOD_STORED => Ok(payload.to_vec()),
        METHOD_DEFLATED => {
            let mut enc =
                flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
            enc.write_all(payload)
                .and_then(|_| enc.finish())
                .map_err(|e| {
                    EngineError::new(Code::IoError, Stage::Save, format!("deflate failed: {e}"))
                })
        }
        m => Err(EngineError::new(
            Code::Unsupported,
            Stage::Save,
            format!("cannot write method {m}"),
        )),
    }
}

fn too_big(what: &str) -> EngineError {
    EngineError::new(
        Code::ResourceLimit,
        Stage::Save,
        format!("{what} exceeds 4 GiB (ZIP64 not supported)"),
    )
}

/// Write a new archive. `Keep` entries are copied byte-for-byte including their
/// local header and data descriptor; `Replace` keeps the original name, extra
/// fields, flags (minus the descriptor bit), method, times and attributes.
pub fn write(src: &Archive, plan: &[OutEntry]) -> Result<Vec<u8>> {
    let mut out: Vec<u8> = Vec::with_capacity(src.raw.len());
    let mut written: Vec<Written> = Vec::with_capacity(plan.len());
    let mut names = HashSet::new();
    let dos_time = src.entries.first().map(|e| {
        (
            u16::from_le_bytes([e.central[12], e.central[13]]),
            u16::from_le_bytes([e.central[14], e.central[15]]),
        )
    });
    for item in plan {
        let name = match item {
            OutEntry::Keep(n) | OutEntry::Replace(n, _) => n,
            OutEntry::Add { name, .. } => name,
        };
        if !names.insert(name.clone()) {
            return Err(EngineError::new(
                Code::InvalidRequest,
                Stage::Save,
                format!("entry {name} written twice"),
            ));
        }
        let offset = u32::try_from(out.len()).map_err(|_| too_big("archive"))?;
        match item {
            OutEntry::Keep(n) => {
                let e = src.get(n).ok_or_else(|| {
                    EngineError::new(
                        Code::StateInvariantError,
                        Stage::Save,
                        format!("no original entry {n}"),
                    )
                })?;
                out.extend_from_slice(&src.raw[e.local_header.start..e.descriptor.end]);
                let mut central = e.central.clone();
                set32(&mut central, 42, offset);
                written.push(Written { central });
            }
            OutEntry::Replace(n, payload) => {
                let e = src.get(n).ok_or_else(|| {
                    EngineError::new(
                        Code::StateInvariantError,
                        Stage::Save,
                        format!("no original entry {n}"),
                    )
                })?;
                let data = compress(e.method, payload)?;
                let crc = crc32fast::hash(payload);
                let csize = u32::try_from(data.len()).map_err(|_| too_big(n))?;
                let usize_ = u32::try_from(payload.len()).map_err(|_| too_big(n))?;
                let mut local = src.raw[e.local_header.clone()].to_vec();
                let flags = e.flags & !FLAG_DESCRIPTOR;
                set16(&mut local, 6, flags);
                set32(&mut local, 14, crc);
                set32(&mut local, 18, csize);
                set32(&mut local, 22, usize_);
                out.extend_from_slice(&local);
                out.extend_from_slice(&data);
                let mut central = e.central.clone();
                set16(&mut central, 8, flags);
                set32(&mut central, 16, crc);
                set32(&mut central, 20, csize);
                set32(&mut central, 24, usize_);
                set32(&mut central, 42, offset);
                written.push(Written { central });
            }
            OutEntry::Add {
                name,
                payload,
                method,
            } => {
                check_name(name)?;
                let data = compress(*method, payload)?;
                let crc = crc32fast::hash(payload);
                let csize = u32::try_from(data.len()).map_err(|_| too_big(name))?;
                let usize_ = u32::try_from(payload.len()).map_err(|_| too_big(name))?;
                let flags = if name.is_ascii() { 0 } else { FLAG_UTF8 };
                let (time, date) = dos_time.unwrap_or((0, 0x21));
                let nb = name.as_bytes();
                let nlen = u16::try_from(nb.len()).map_err(|_| too_big(name))?;
                let mut local = Vec::with_capacity(30 + nb.len());
                put32(&mut local, SIG_LOCAL);
                put16(&mut local, 20);
                put16(&mut local, flags);
                put16(&mut local, *method);
                put16(&mut local, time);
                put16(&mut local, date);
                put32(&mut local, crc);
                put32(&mut local, csize);
                put32(&mut local, usize_);
                put16(&mut local, nlen);
                put16(&mut local, 0);
                local.extend_from_slice(nb);
                out.extend_from_slice(&local);
                out.extend_from_slice(&data);
                let mut central = Vec::with_capacity(46 + nb.len());
                put32(&mut central, SIG_CENTRAL);
                put16(&mut central, 20);
                put16(&mut central, 20);
                put16(&mut central, flags);
                put16(&mut central, *method);
                put16(&mut central, time);
                put16(&mut central, date);
                put32(&mut central, crc);
                put32(&mut central, csize);
                put32(&mut central, usize_);
                put16(&mut central, nlen);
                put16(&mut central, 0);
                put16(&mut central, 0);
                put16(&mut central, 0);
                put16(&mut central, 0);
                put32(&mut central, 0);
                put32(&mut central, offset);
                central.extend_from_slice(nb);
                written.push(Written { central });
            }
        }
    }
    let cd_off = u32::try_from(out.len()).map_err(|_| too_big("archive"))?;
    for w in &written {
        out.extend_from_slice(&w.central);
    }
    let cd_size =
        u32::try_from(out.len() - cd_off as usize).map_err(|_| too_big("central directory"))?;
    let n = u16::try_from(written.len()).map_err(|_| too_big("entry count"))?;
    if n == 0xFFFF {
        return Err(too_big("entry count"));
    }
    put32(&mut out, SIG_EOCD);
    put16(&mut out, 0);
    put16(&mut out, 0);
    put16(&mut out, n);
    put16(&mut out, n);
    put32(&mut out, cd_size);
    put32(&mut out, cd_off);
    let clen = u16::try_from(src.eocd_comment.len()).map_err(|_| too_big("comment"))?;
    put16(&mut out, clen);
    out.extend_from_slice(&src.eocd_comment);
    Ok(out)
}

/// Plan that keeps every original entry in order.
pub fn keep_all(src: &Archive) -> Vec<OutEntry> {
    src.entries
        .iter()
        .map(|e| OutEntry::Keep(e.name.clone()))
        .collect()
}

#[cfg(test)]
mod tests;
