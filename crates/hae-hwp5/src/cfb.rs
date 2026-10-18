//! Minimal read-only MS-CFB (Compound File Binary) container reader.
//!
//! Structure and constants follow the MS-CFB specification; the sector layout
//! (`(sector_id + 1) * sector_size`), version/`sector shift` binding, mini sector
//! shift 6 and the 4096-byte mini stream cutoff are the invariants documented in
//! `docs/decisions/hwp5-format-notes.md` §1.1.
//!
//! Everything is bounds-checked against the borrowed file bytes: sector ids,
//! FAT entries and chain lengths. A damaged or hostile container is rejected with
//! `PACKAGE_REJECTED(malformed_compound_file)`; size/count limits use
//! `RESOURCE_LIMIT`.

use crate::reject;
use hae_diag::{Code, EngineError, Limits, PackageRejectReason as R, Result, Stage};

const MAGIC: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
const BYTE_ORDER_MARK: u16 = 0xFFFE;
const MINI_SECTOR_SIZE: usize = 64;
const MINI_SECTOR_SHIFT: u16 = 6;
const MINI_STREAM_CUTOFF: u64 = 4096;
const DIFAT_ENTRIES_IN_HEADER: usize = 109;
const FREE_SECT: u32 = 0xFFFF_FFFF;
const END_OF_CHAIN: u32 = 0xFFFF_FFFE;
const FAT_SECT: u32 = 0xFFFF_FFFD;
const DIFAT_SECT: u32 = 0xFFFF_FFFC;
const DIR_ENTRY_SIZE: usize = 128;
const NO_STREAM: u32 = 0xFFFF_FFFF;

fn bad(message: impl Into<String>) -> EngineError {
    reject(R::MalformedCompoundFile, message)
}

fn u16_at(data: &[u8], at: usize) -> Result<u16> {
    data.get(at..at + 2)
        .map(|s| u16::from_le_bytes([s[0], s[1]]))
        .ok_or_else(|| bad(format!("truncated field at {at:#x}")))
}

fn u32_at(data: &[u8], at: usize) -> Result<u32> {
    data.get(at..at + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or_else(|| bad(format!("truncated field at {at:#x}")))
}

fn u64_at(data: &[u8], at: usize) -> Result<u64> {
    data.get(at..at + 8)
        .map(|s| u64::from_le_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]]))
        .ok_or_else(|| bad(format!("truncated field at {at:#x}")))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EntryKind {
    Unallocated,
    Storage,
    Stream,
    Root,
}

#[derive(Debug, Clone)]
pub(crate) struct DirEntry {
    pub name: String,
    pub kind: EntryKind,
    left: u32,
    right: u32,
    child: u32,
    pub start_sector: u32,
    pub size: u64,
}

impl DirEntry {
    fn is_allocated(&self) -> bool {
        self.kind != EntryKind::Unallocated
    }
}

pub(crate) struct CompoundFile<'a> {
    data: &'a [u8],
    sector_size: usize,
    total_sectors: usize,
    fat: Vec<u32>,
    mini_fat: Vec<u32>,
    mini_stream: Vec<u8>,
    entries: Vec<DirEntry>,
    root: usize,
}

impl<'a> CompoundFile<'a> {
    pub fn open(data: &'a [u8], limits: &Limits) -> Result<Self> {
        if data.len() < 512 {
            return Err(bad(format!(
                "file is {} bytes, smaller than a CFB header",
                data.len()
            )));
        }
        if data[..8] != MAGIC {
            return Err(bad("missing CFB signature (D0 CF 11 E0 A1 B1 1A E1)"));
        }
        let major = u16_at(data, 0x1A)?;
        let expected_shift = match major {
            3 => 9u16,
            4 => 12u16,
            other => return Err(bad(format!("CFB major version {other} is not 3 or 4"))),
        };
        if u16_at(data, 0x1C)? != BYTE_ORDER_MARK {
            return Err(bad("byte order mark is not 0xFFFE"));
        }
        let sector_shift = u16_at(data, 0x1E)?;
        if sector_shift != expected_shift {
            return Err(bad(format!(
                "sector shift {sector_shift} does not match CFB version {major} \
                 (expected {expected_shift})"
            )));
        }
        if u16_at(data, 0x20)? != MINI_SECTOR_SHIFT {
            return Err(bad("mini sector shift is not 6 (64-byte mini sectors)"));
        }
        let sector_size = 1usize << sector_shift;
        if data.len() < sector_size {
            return Err(bad("file is smaller than one sector"));
        }
        if u32_at(data, 0x38)? as u64 != MINI_STREAM_CUTOFF {
            return Err(bad("mini stream cutoff is not 4096"));
        }

        let total_sectors = (data.len() - sector_size).div_ceil(sector_size);
        let num_fat = u32_at(data, 0x2C)? as usize;
        let first_dir = u32_at(data, 0x30)?;
        let first_minifat = u32_at(data, 0x3C)?;
        let num_minifat = u32_at(data, 0x40)? as usize;
        let first_difat = u32_at(data, 0x44)?;
        let num_difat = u32_at(data, 0x48)? as usize;

        // The FAT sectors must live inside the file, so their total size cannot
        // exceed it; this also bounds every allocation below by the input size.
        let entries_per_sector = sector_size / 4;
        if num_fat
            .checked_mul(sector_size)
            .is_none_or(|bytes| bytes > data.len())
        {
            return Err(bad(format!("{num_fat} FAT sectors do not fit the file")));
        }
        if num_difat
            .checked_mul(sector_size)
            .is_none_or(|bytes| bytes > data.len())
        {
            return Err(bad(format!(
                "{num_difat} DIFAT sectors do not fit the file"
            )));
        }

        let fat_ids = collect_fat_sectors(
            data,
            sector_size,
            first_difat,
            num_difat,
            num_fat,
            entries_per_sector,
        )?;
        let mut fat = Vec::with_capacity(num_fat.saturating_mul(entries_per_sector));
        for sid in &fat_ids {
            let sec = sector(data, sector_size, *sid)?;
            fat.extend(
                sec.chunks_exact(4)
                    .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])),
            );
        }

        // Directory chain.
        let dir_data = read_chain(
            data,
            sector_size,
            total_sectors,
            &fat,
            first_dir,
            "directory",
        )?;
        if dir_data.len() % DIR_ENTRY_SIZE != 0 {
            return Err(bad("directory chain is not a multiple of 128 bytes"));
        }
        let entry_count = dir_data.len() / DIR_ENTRY_SIZE;
        if entry_count as u64 > limits.max_entries {
            return Err(EngineError::new(
                Code::ResourceLimit,
                Stage::Open,
                format!(
                    "compound file directory has {entry_count} entries, limit {}",
                    limits.max_entries
                ),
            ));
        }
        let mut entries = Vec::with_capacity(entry_count);
        for chunk in dir_data.chunks_exact(DIR_ENTRY_SIZE) {
            entries.push(parse_dir_entry(chunk, entries.len())?);
        }
        let root = entries
            .iter()
            .position(|e| e.kind == EntryKind::Root)
            .ok_or_else(|| bad("no root directory entry"))?;
        for (index, entry) in entries.iter().enumerate() {
            for (field, value) in [
                ("left", entry.left),
                ("right", entry.right),
                ("child", entry.child),
            ] {
                if entry.is_allocated() && value != NO_STREAM && value as usize >= entry_count {
                    return Err(bad(format!(
                        "directory entry {index} ({}) {field} pointer {value} is out of range",
                        entry.name
                    )));
                }
            }
        }

        // Mini FAT (a normal sector chain) and the mini stream (held in the root
        // entry's chain).
        let mut mini_fat = Vec::new();
        if num_minifat > 0 {
            if num_minifat
                .checked_mul(sector_size)
                .is_none_or(|bytes| bytes > data.len())
            {
                return Err(bad(format!(
                    "{num_minifat} mini FAT sectors do not fit the file"
                )));
            }
            let bytes = read_chain(
                data,
                sector_size,
                total_sectors,
                &fat,
                first_minifat,
                "mini FAT",
            )?;
            mini_fat = bytes
                .chunks_exact(4)
                .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect();
        }
        let root_entry = &entries[root];
        let mini_stream = if root_entry.size == 0 {
            Vec::new()
        } else {
            if root_entry.size > data.len() as u64 {
                return Err(bad("mini stream size exceeds the file"));
            }
            let bytes = read_chain(
                data,
                sector_size,
                total_sectors,
                &fat,
                root_entry.start_sector,
                "mini stream",
            )?;
            if (bytes.len() as u64) < root_entry.size {
                return Err(bad("mini stream chain is shorter than its declared size"));
            }
            bytes
        };

        Ok(CompoundFile {
            data,
            sector_size,
            total_sectors,
            fat,
            mini_fat,
            mini_stream,
            entries,
            root,
        })
    }

    /// Finds a stream or storage by `/`-separated path, case-insensitively as the
    /// CFB directory comparison requires. Storage names are not part of the result.
    pub fn find_entry(&self, path: &str) -> Option<usize> {
        let mut current = self.root;
        let mut components = path.split('/').filter(|c| !c.is_empty()).peekable();
        loop {
            let wanted = components.next()?;
            let found = self.find_child(current, wanted)?;
            if components.peek().is_some() {
                if self.entries[found].kind != EntryKind::Storage {
                    return None;
                }
                current = found;
            } else {
                return Some(found);
            }
        }
    }

    fn find_child(&self, parent: usize, name: &str) -> Option<usize> {
        let mut stack = Vec::new();
        if self.entries[parent].child != NO_STREAM {
            stack.push(self.entries[parent].child as usize);
        }
        let mut visited = 0usize;
        while let Some(index) = stack.pop() {
            visited += 1;
            if visited > self.entries.len() {
                return None; // cyclic pointers in the tree
            }
            let entry = self.entries.get(index)?;
            if !entry.is_allocated() {
                continue;
            }
            if entry.name.eq_ignore_ascii_case(name) {
                return Some(index);
            }
            if entry.left != NO_STREAM {
                stack.push(entry.left as usize);
            }
            if entry.right != NO_STREAM {
                stack.push(entry.right as usize);
            }
        }
        None
    }

    /// All allocated children of a storage entry (the whole subtree, name order
    /// irrelevant to callers that sort themselves).
    pub fn children_of(&self, parent: usize) -> Vec<usize> {
        let mut out = Vec::new();
        let mut stack = Vec::new();
        if self.entries[parent].child != NO_STREAM {
            stack.push(self.entries[parent].child as usize);
        }
        while let Some(index) = stack.pop() {
            if out.len() > self.entries.len() {
                break; // cyclic pointers; callers see what was collected
            }
            let Some(entry) = self.entries.get(index) else {
                break;
            };
            if !entry.is_allocated() {
                continue;
            }
            out.push(index);
            if entry.left != NO_STREAM {
                stack.push(entry.left as usize);
            }
            if entry.right != NO_STREAM {
                stack.push(entry.right as usize);
            }
        }
        out
    }

    /// `BodyText` section streams (`Section0`, `Section1`, ...) sorted by index.
    /// Duplicate indexes are a damaged container.
    pub fn body_text_sections(&self) -> Result<Vec<(usize, usize)>> {
        let storage_index = self
            .find_entry("BodyText")
            .ok_or_else(|| reject(R::MissingComponent, "BodyText storage is missing"))?;
        if self.entries[storage_index].kind != EntryKind::Storage {
            return Err(bad("BodyText is a stream, not a storage"));
        }
        let mut sections: Vec<(usize, usize)> = Vec::new();
        for index in self.children_of(storage_index) {
            let entry = &self.entries[index];
            if entry.kind != EntryKind::Stream {
                continue;
            }
            if let Some(number) = section_number(&entry.name) {
                if sections.iter().any(|(n, _)| *n == number) {
                    return Err(bad(format!("duplicate BodyText section {number}")));
                }
                sections.push((number, index));
            }
        }
        sections.sort_unstable();
        Ok(sections)
    }

    /// Reads one stream's bytes (mini or regular chain per its size).
    pub fn read_stream(&self, index: usize) -> Result<Vec<u8>> {
        let entry = self
            .entries
            .get(index)
            .ok_or_else(|| bad("stream index out of range"))?;
        if entry.kind != EntryKind::Stream {
            return Err(bad(format!("{} is not a stream", entry.name)));
        }
        if entry.size == 0 {
            return Ok(Vec::new());
        }
        if entry.size > self.data.len() as u64 {
            return Err(bad(format!(
                "stream {} declares {} bytes, larger than the file",
                entry.name, entry.size
            )));
        }
        if entry.size < MINI_STREAM_CUTOFF {
            let needed = (entry.size as usize).div_ceil(MINI_SECTOR_SIZE);
            let mut out = Vec::with_capacity(entry.size as usize);
            let mut sid = entry.start_sector;
            for _ in 0..needed {
                if sid == END_OF_CHAIN || sid == FREE_SECT {
                    return Err(bad(format!("mini chain of {} ends early", entry.name)));
                }
                let offset = (sid as usize)
                    .checked_mul(MINI_SECTOR_SIZE)
                    .ok_or_else(|| bad("mini sector offset overflow"))?;
                let chunk = self
                    .mini_stream
                    .get(offset..offset + MINI_SECTOR_SIZE)
                    .ok_or_else(|| bad(format!("mini sector {sid} is outside the mini stream")))?;
                out.extend_from_slice(chunk);
                sid = *self
                    .mini_fat
                    .get(sid as usize)
                    .ok_or_else(|| bad(format!("mini sector {sid} has no mini FAT entry")))?;
            }
            out.truncate(entry.size as usize);
            Ok(out)
        } else {
            let bytes = read_chain(
                self.data,
                self.sector_size,
                self.total_sectors,
                &self.fat,
                entry.start_sector,
                &entry.name,
            )?;
            if (bytes.len() as u64) < entry.size {
                return Err(bad(format!(
                    "stream {} chain is shorter than its declared size",
                    entry.name
                )));
            }
            Ok(bytes[..entry.size as usize].to_vec())
        }
    }
}

fn section_number(name: &str) -> Option<usize> {
    let rest = name.strip_prefix("Section")?;
    if rest.is_empty() || !rest.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    rest.parse().ok()
}

fn parse_dir_entry(chunk: &[u8], index: usize) -> Result<DirEntry> {
    let kind = match chunk[0x42] {
        0 => EntryKind::Unallocated,
        1 => EntryKind::Storage,
        2 => EntryKind::Stream,
        5 => EntryKind::Root,
        other => {
            return Err(bad(format!(
                "directory entry {index} has unknown type {other}"
            )))
        }
    };
    let raw_name_len = u16_at(chunk, 0x40)? as usize;
    let name = if kind == EntryKind::Unallocated || raw_name_len == 0 {
        String::new()
    } else {
        if raw_name_len > 64 || !raw_name_len.is_multiple_of(2) {
            return Err(bad(format!(
                "directory entry {index} has invalid name length {raw_name_len}"
            )));
        }
        let units: Vec<u16> = chunk[..raw_name_len - 2]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16(&units)
            .map_err(|_| bad(format!("directory entry {index} name is not valid UTF-16")))?
    };
    Ok(DirEntry {
        name,
        kind,
        left: u32_at(chunk, 0x44)?,
        right: u32_at(chunk, 0x48)?,
        child: u32_at(chunk, 0x4C)?,
        start_sector: u32_at(chunk, 0x74)?,
        size: u64_at(chunk, 0x78)?,
    })
}

fn sector(data: &[u8], sector_size: usize, id: u32) -> Result<&[u8]> {
    if matches!(id, FREE_SECT | END_OF_CHAIN | FAT_SECT | DIFAT_SECT) {
        return Err(bad(format!(
            "sector id {id:#x} is a chain marker, not a sector"
        )));
    }
    let offset = (id as usize + 1)
        .checked_mul(sector_size)
        .ok_or_else(|| bad("sector offset overflow"))?;
    data.get(offset..offset + sector_size)
        .ok_or_else(|| bad(format!("sector {id} lies outside the file")))
}

fn collect_fat_sectors(
    data: &[u8],
    sector_size: usize,
    first_difat: u32,
    num_difat: usize,
    num_fat: usize,
    entries_per_sector: usize,
) -> Result<Vec<u32>> {
    let mut ids: Vec<u32> = Vec::new();
    for i in 0..DIFAT_ENTRIES_IN_HEADER {
        let value = u32_at(data, 0x4C + i * 4)?;
        if value != FREE_SECT {
            ids.push(value);
        }
    }
    let mut difat = first_difat;
    let mut steps = 0usize;
    while steps < num_difat && difat != END_OF_CHAIN && difat != FREE_SECT {
        let sec = sector(data, sector_size, difat)?;
        for chunk in sec[..(entries_per_sector - 1) * 4].chunks_exact(4) {
            let value = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            if value != FREE_SECT {
                ids.push(value);
            }
        }
        difat = u32::from_le_bytes([
            sec[sector_size - 4],
            sec[sector_size - 3],
            sec[sector_size - 2],
            sec[sector_size - 1],
        ]);
        steps += 1;
    }
    if ids.len() < num_fat {
        return Err(bad(format!(
            "header lists {num_fat} FAT sectors but only {} are reachable",
            ids.len()
        )));
    }
    ids.truncate(num_fat);
    Ok(ids)
}

/// Reads a FAT sector chain. A chain that never ends within the file's sectors
/// is damaged; output size is inherently bounded by the file size.
fn read_chain(
    data: &[u8],
    sector_size: usize,
    total_sectors: usize,
    fat: &[u32],
    start: u32,
    what: &str,
) -> Result<Vec<u8>> {
    if start == END_OF_CHAIN || start == FREE_SECT {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let mut sid = start;
    let mut steps = 0usize;
    loop {
        let sec = sector(data, sector_size, sid)?;
        out.extend_from_slice(sec);
        steps += 1;
        if steps > total_sectors {
            return Err(bad(format!("{what} chain has a cycle")));
        }
        let next = *fat
            .get(sid as usize)
            .ok_or_else(|| bad(format!("{what}: sector {sid} has no FAT entry")))?;
        match next {
            END_OF_CHAIN => break,
            FREE_SECT | FAT_SECT | DIFAT_SECT => {
                return Err(bad(format!(
                    "{what}: sector {sid} chains to the marker {next:#x}"
                )))
            }
            n => sid = n,
        }
    }
    Ok(out)
}
