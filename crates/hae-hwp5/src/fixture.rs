//! Deterministic synthetic HWP5 containers for tests (feature `fixture`).
//!
//! The builder writes the smallest faithful MS-CFB layout the reader claims to
//! support: 512-byte sectors, 64-byte mini sectors, a mini stream + mini FAT for
//! streams under 4096 bytes, a regular FAT chain for larger streams, a v3 header
//! with one FAT sector and no DIFAT chain. Nothing here is derived from a real
//! document; it is written from the MS-CFB rules in
//! `docs/decisions/hwp5-format-notes.md` §1.1 so tests exercise our own reader.

use flate2::write::DeflateEncoder;
use flate2::Compression;
use std::io::Write;

const MAGIC: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
const FREE_SECT: u32 = 0xFFFF_FFFF;
const END_OF_CHAIN: u32 = 0xFFFF_FFFE;
const FAT_SECT: u32 = 0xFFFF_FFFD;
const NO_STREAM: u32 = 0xFFFF_FFFF;
const SECTOR: usize = 512;
const MINI: usize = 64;
const MINI_CUTOFF: usize = 4096;

/// Streams to place in the synthetic container.
#[derive(Debug, Clone, Default)]
pub struct ContainerSpec {
    /// Contents of the `FileHeader` stream (see [`file_header`]).
    pub file_header: Vec<u8>,
    /// `BodyText/SectionN` contents; the Nth entry becomes `SectionN`.
    pub sections: Vec<Vec<u8>>,
    /// Extra root-level streams, e.g. `("DocInfo", bytes)`.
    pub extra_streams: Vec<(String, Vec<u8>)>,
}

/// A minimal `FileHeader` stream: signature, version, properties, zero padding.
pub fn file_header(version: (u8, u8, u8, u8), properties: u32) -> Vec<u8> {
    let mut out = vec![0u8; 256];
    out[..17].copy_from_slice(b"HWP Document File");
    let (major, minor, build, revision) = version;
    let raw = u32::from_le_bytes([revision, build, minor, major]);
    out[0x20..0x24].copy_from_slice(&raw.to_le_bytes());
    out[0x24..0x28].copy_from_slice(&properties.to_le_bytes());
    out
}

/// One HWP5 record: 4-byte header + payload. Payloads of 4095 bytes or more
/// use the extended-size form automatically.
pub fn record(tag: u16, level: u16, payload: &[u8]) -> Vec<u8> {
    if payload.len() >= 0xFFF {
        return record_extended(tag, level, payload);
    }
    let header = tag as u32 | ((level as u32) << 10) | ((payload.len() as u32 & 0xFFF) << 20);
    let mut out = Vec::with_capacity(4 + payload.len());
    out.extend_from_slice(&header.to_le_bytes());
    out.extend_from_slice(payload);
    out
}

/// One HWP5 record using the 0xFFF extended-size form (8-byte header).
pub fn record_extended(tag: u16, level: u16, payload: &[u8]) -> Vec<u8> {
    let header = tag as u32 | ((level as u32) << 10) | (0xFFF << 20);
    let mut out = Vec::with_capacity(8 + payload.len());
    out.extend_from_slice(&header.to_le_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    out
}

/// UTF-16LE encoding without BOM.
pub fn utf16(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

/// Raw DEFLATE (no zlib header), the HWP5 stream compression.
pub fn deflate(data: &[u8]) -> Vec<u8> {
    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).expect("deflate");
    encoder.finish().expect("deflate finish")
}

/// A `PARA_HEADER` payload with a 0 char count and benign defaults.
pub fn para_header_data() -> Vec<u8> {
    vec![0u8; 22]
}

struct Node {
    name: String,
    kind: u8,
    data: Vec<u8>,
    children: Vec<usize>,
}

/// Builds a complete MS-CFB container holding the spec's streams.
pub fn build(spec: &ContainerSpec) -> Vec<u8> {
    let mut nodes = vec![Node {
        name: "Root Entry".into(),
        kind: 5,
        data: Vec::new(),
        children: Vec::new(),
    }];
    let push_stream = |nodes: &mut Vec<Node>, name: String, data: Vec<u8>| {
        nodes.push(Node {
            name,
            kind: 2,
            data,
            children: Vec::new(),
        });
        nodes.len() - 1
    };
    let header_index = push_stream(&mut nodes, "FileHeader".into(), spec.file_header.clone());
    nodes[0].children.push(header_index);
    for (name, data) in &spec.extra_streams {
        let index = push_stream(&mut nodes, name.clone(), data.clone());
        nodes[0].children.push(index);
    }
    let body = nodes.len();
    nodes.push(Node {
        name: "BodyText".into(),
        kind: 1,
        data: Vec::new(),
        children: Vec::new(),
    });
    nodes[0].children.push(body);
    for (section, data) in spec.sections.iter().enumerate() {
        let index = push_stream(&mut nodes, format!("Section{section}"), data.clone());
        nodes[body].children.push(index);
    }
    // CFB orders sibling names by (length, uppercase name); a degenerate
    // right-chain in that order is still a valid search tree for readers.
    let name_keys: Vec<(usize, String)> = nodes
        .iter()
        .map(|node| (node.name.chars().count(), node.name.to_uppercase()))
        .collect();
    for node in nodes.iter_mut() {
        node.children.sort_by_key(|index| name_keys[*index].clone());
    }

    // Entry order: root, then each storage's subtree in sorted order.
    let mut order: Vec<usize> = Vec::new();
    fn visit(node: usize, nodes: &[Node], order: &mut Vec<usize>) {
        order.push(node);
        for child in &nodes[node].children {
            visit(*child, nodes, order);
        }
    }
    visit(0, &nodes, &mut order);
    let mut entry_index = vec![usize::MAX; nodes.len()];
    for (position, node) in order.iter().enumerate() {
        entry_index[*node] = position;
    }

    // Mini allocation first (bytes under the 4096-byte cutoff).
    let mut mini_fat: Vec<u32> = Vec::new();
    let mut mini_data: Vec<u8> = Vec::new();
    let mut mini_start: Vec<Option<u32>> = vec![None; nodes.len()];
    let mut regular_start: Vec<Option<usize>> = vec![None; nodes.len()];
    let mut regular_sectors: Vec<Vec<u8>> = Vec::new();
    for node in &order {
        if nodes[*node].kind != 2 {
            continue;
        }
        let data = &nodes[*node].data;
        if data.len() < MINI_CUTOFF {
            let needed = data.len().div_ceil(MINI).max(usize::from(!data.is_empty()));
            if needed > 0 {
                mini_start[*node] = Some(mini_fat.len() as u32);
            }
            for sector in 0..needed {
                let start = sector * MINI;
                let mut chunk = vec![0u8; MINI];
                if start < data.len() {
                    let end = (start + MINI).min(data.len());
                    chunk[..end - start].copy_from_slice(&data[start..end]);
                }
                mini_data.extend_from_slice(&chunk);
                let next = if sector + 1 == needed {
                    END_OF_CHAIN
                } else {
                    (mini_fat.len() + 1) as u32
                };
                mini_fat.push(next);
            }
        } else {
            let needed = data.len().div_ceil(SECTOR);
            regular_start[*node] = Some(regular_sectors.len());
            for sector in 0..needed {
                let start = sector * SECTOR;
                let mut chunk = vec![0u8; SECTOR];
                let end = (start + SECTOR).min(data.len());
                chunk[..end - start].copy_from_slice(&data[start..end]);
                regular_sectors.push(chunk);
            }
        }
    }

    let dir_sector_count = order.len().div_ceil(SECTOR / 128).max(1);
    let minifat_sector_count = (mini_fat.len() * 4).div_ceil(SECTOR);
    let ministream_sector_count = mini_data.len().div_ceil(SECTOR);
    let fat_sector_count = 1usize;
    let mut next_sector = 0u32;
    let dir_first = next_sector;
    next_sector += dir_sector_count as u32;
    let minifat_first = if minifat_sector_count > 0 {
        let first = next_sector;
        next_sector += minifat_sector_count as u32;
        first
    } else {
        END_OF_CHAIN
    };
    // Regular stream sectors follow the mini FAT.
    let mut regular_first: Vec<u32> = Vec::new();
    for chunk in &regular_sectors {
        let _ = chunk;
        regular_first.push(next_sector);
        next_sector += 1;
    }
    let ministream_first = if ministream_sector_count > 0 {
        let first = next_sector;
        next_sector += ministream_sector_count as u32;
        first
    } else {
        END_OF_CHAIN
    };
    let fat_first = next_sector;
    next_sector += fat_sector_count as u32;
    let total_sectors = next_sector as usize;
    assert!(
        total_sectors <= SECTOR / 4,
        "fixture must fit in one FAT sector"
    );

    // FAT: single sector; chains for directory, mini FAT, regular streams,
    // mini stream; the FAT sector itself is marked FAT_SECT.
    let mut fat = vec![FREE_SECT; SECTOR / 4];
    for i in 0..dir_sector_count {
        let sid = dir_first as usize + i;
        fat[sid] = if i + 1 == dir_sector_count {
            END_OF_CHAIN
        } else {
            (sid + 1) as u32
        };
    }
    if minifat_sector_count > 0 {
        for i in 0..minifat_sector_count {
            let sid = minifat_first as usize + i;
            fat[sid] = if i + 1 == minifat_sector_count {
                END_OF_CHAIN
            } else {
                (sid + 1) as u32
            };
        }
    }
    for (i, _) in regular_sectors.iter().enumerate() {
        let sid = regular_first[i] as usize;
        fat[sid] = if i + 1 == regular_sectors.len() {
            END_OF_CHAIN
        } else {
            (sid + 1) as u32
        };
    }
    for i in 0..ministream_sector_count {
        let sid = ministream_first as usize + i;
        fat[sid] = if i + 1 == ministream_sector_count {
            END_OF_CHAIN
        } else {
            (sid + 1) as u32
        };
    }
    fat[fat_first as usize] = FAT_SECT;

    // Directory entries.
    let mut dir = vec![0u8; dir_sector_count * SECTOR];
    for (position, node) in order.iter().enumerate() {
        let entry = &mut dir[position * 128..(position + 1) * 128];
        let name = &nodes[*node].name;
        let units: Vec<u16> = name.encode_utf16().collect();
        assert!(units.len() <= 31);
        for (i, unit) in units.iter().enumerate() {
            entry[i * 2..i * 2 + 2].copy_from_slice(&unit.to_le_bytes());
        }
        let name_len = ((units.len() + 1) * 2) as u16;
        entry[0x40..0x42].copy_from_slice(&name_len.to_le_bytes());
        entry[0x42] = nodes[*node].kind;
        entry[0x43] = if *node == 0 { 0 } else { 1 };
        let left = NO_STREAM;
        // Sibling links come from the parent's sorted child list.
        let mut parent_right = NO_STREAM;
        let parent = nodes.iter().enumerate().find_map(|(parent, entry)| {
            entry
                .children
                .iter()
                .position(|child| child == node)
                .map(|position| (parent, position))
        });
        if let Some((parent, position_in_parent)) = parent {
            let siblings = &nodes[parent].children;
            if position_in_parent + 1 < siblings.len() {
                parent_right = entry_index[siblings[position_in_parent + 1]] as u32;
            }
        }
        let child = nodes[*node]
            .children
            .first()
            .map(|first| entry_index[*first] as u32)
            .unwrap_or(NO_STREAM);
        entry[0x44..0x48].copy_from_slice(&left.to_le_bytes());
        entry[0x48..0x4C].copy_from_slice(&parent_right.to_le_bytes());
        entry[0x4C..0x50].copy_from_slice(&child.to_le_bytes());
        let (start, size) = if *node == 0 {
            (
                if mini_data.is_empty() {
                    END_OF_CHAIN
                } else {
                    ministream_first
                },
                mini_data.len() as u64,
            )
        } else if nodes[*node].kind == 2 {
            match (mini_start[*node], regular_start[*node]) {
                (Some(sid), _) => (sid, nodes[*node].data.len() as u64),
                (None, Some(position)) => (regular_first[position], nodes[*node].data.len() as u64),
                (None, None) => (0, 0),
            }
        } else {
            (0, 0)
        };
        entry[0x74..0x78].copy_from_slice(&start.to_le_bytes());
        entry[0x78..0x80].copy_from_slice(&size.to_le_bytes());
    }

    // Assemble sectors in id order: directory, mini FAT, regular streams,
    // mini stream, FAT.
    let mut sectors: Vec<Vec<u8>> = Vec::with_capacity(total_sectors);
    for chunk in dir.chunks_exact(SECTOR) {
        sectors.push(chunk.to_vec());
    }
    let mut minifat_bytes = vec![0u8; minifat_sector_count * SECTOR];
    for (i, value) in mini_fat.iter().enumerate() {
        minifat_bytes[i * 4..i * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    for chunk in minifat_bytes.chunks_exact(SECTOR) {
        sectors.push(chunk.to_vec());
    }
    for chunk in &regular_sectors {
        sectors.push(chunk.clone());
    }
    let mut ministream_bytes = mini_data.clone();
    ministream_bytes.resize(ministream_sector_count * SECTOR, 0);
    for chunk in ministream_bytes.chunks_exact(SECTOR) {
        sectors.push(chunk.to_vec());
    }
    let mut fat_bytes = vec![0u8; SECTOR];
    for (i, value) in fat.iter().enumerate() {
        fat_bytes[i * 4..i * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    sectors.push(fat_bytes);
    assert_eq!(sectors.len(), total_sectors);

    // Header.
    let mut header = vec![0u8; SECTOR];
    header[..8].copy_from_slice(&MAGIC);
    header[0x18..0x1A].copy_from_slice(&0x003Eu16.to_le_bytes()); // minor version 62
    header[0x1A..0x1C].copy_from_slice(&3u16.to_le_bytes()); // major version 3
    header[0x1C..0x1E].copy_from_slice(&0xFFFEu16.to_le_bytes());
    header[0x1E..0x20].copy_from_slice(&9u16.to_le_bytes()); // 512-byte sectors
    header[0x20..0x22].copy_from_slice(&6u16.to_le_bytes()); // 64-byte mini sectors
    header[0x28..0x2C].copy_from_slice(&0u32.to_le_bytes()); // v3: must be zero
    header[0x2C..0x30].copy_from_slice(&1u32.to_le_bytes()); // one FAT sector
    header[0x30..0x34].copy_from_slice(&dir_first.to_le_bytes());
    header[0x38..0x3C].copy_from_slice(&(MINI_CUTOFF as u32).to_le_bytes());
    header[0x3C..0x40].copy_from_slice(&minifat_first.to_le_bytes());
    header[0x40..0x44].copy_from_slice(&(minifat_sector_count as u32).to_le_bytes());
    header[0x44..0x48].copy_from_slice(&END_OF_CHAIN.to_le_bytes());
    header[0x48..0x4C].copy_from_slice(&0u32.to_le_bytes());
    header[0x4C..0x50].copy_from_slice(&fat_first.to_le_bytes());
    for i in 1..109 {
        header[0x4C + i * 4..0x4C + i * 4 + 4].copy_from_slice(&FREE_SECT.to_le_bytes());
    }

    let mut out = header;
    for sector in sectors {
        out.extend_from_slice(&sector);
    }
    out
}
