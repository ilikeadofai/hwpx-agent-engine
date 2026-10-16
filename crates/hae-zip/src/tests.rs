use super::*;

/// Minimal raw ZIP builder for tests. Each spec entry is (name, payload, method).
struct Spec {
    name: Vec<u8>,
    payload: Vec<u8>,
    method: u16,
    flags: u16,
}

fn spec(name: &str, payload: &[u8], method: u16) -> Spec {
    Spec {
        name: name.as_bytes().to_vec(),
        payload: payload.to_vec(),
        method,
        flags: if name.is_ascii() { 0 } else { FLAG_UTF8 },
    }
}

fn deflate(p: &[u8]) -> Vec<u8> {
    let mut e = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
    e.write_all(p).unwrap();
    e.finish().unwrap()
}

struct Built {
    bytes: Vec<u8>,
    /// (local offset, central record offset) per entry
    offs: Vec<(usize, usize)>,
    eocd: usize,
}

fn build(specs: &[Spec]) -> Built {
    let mut out = Vec::new();
    let mut centrals = Vec::new();
    let mut locals = Vec::new();
    for s in specs {
        let data = if s.method == METHOD_DEFLATED {
            deflate(&s.payload)
        } else {
            s.payload.clone()
        };
        let crc = crc32fast::hash(&s.payload);
        let lo = out.len();
        locals.push(lo);
        put32(&mut out, SIG_LOCAL);
        put16(&mut out, 20);
        put16(&mut out, s.flags);
        put16(&mut out, s.method);
        put16(&mut out, 0);
        put16(&mut out, 0x21);
        put32(&mut out, crc);
        put32(&mut out, data.len() as u32);
        put32(&mut out, s.payload.len() as u32);
        put16(&mut out, s.name.len() as u16);
        put16(&mut out, 0);
        out.extend_from_slice(&s.name);
        out.extend_from_slice(&data);
        let mut c = Vec::new();
        put32(&mut c, SIG_CENTRAL);
        put16(&mut c, 20);
        put16(&mut c, 20);
        put16(&mut c, s.flags);
        put16(&mut c, s.method);
        put16(&mut c, 0);
        put16(&mut c, 0x21);
        put32(&mut c, crc);
        put32(&mut c, data.len() as u32);
        put32(&mut c, s.payload.len() as u32);
        put16(&mut c, s.name.len() as u16);
        put16(&mut c, 0);
        put16(&mut c, 0);
        put16(&mut c, 0);
        put16(&mut c, 0);
        put32(&mut c, 0);
        put32(&mut c, lo as u32);
        c.extend_from_slice(&s.name);
        centrals.push(c);
    }
    let cd = out.len();
    let mut coffs = Vec::new();
    for c in &centrals {
        coffs.push(out.len());
        out.extend_from_slice(c);
    }
    let eocd = out.len();
    put32(&mut out, SIG_EOCD);
    put16(&mut out, 0);
    put16(&mut out, 0);
    put16(&mut out, specs.len() as u16);
    put16(&mut out, specs.len() as u16);
    put32(&mut out, (eocd - cd) as u32);
    put32(&mut out, cd as u32);
    put16(&mut out, 0);
    Built {
        bytes: out,
        offs: locals.into_iter().zip(coffs).collect(),
        eocd,
    }
}

fn base() -> Vec<Spec> {
    vec![
        spec("mimetype", b"application/hwp+zip", METHOD_STORED),
        spec(
            "Contents/section0.xml",
            "<hs:sec>본문 텍스트</hs:sec>".repeat(20).as_bytes(),
            METHOD_DEFLATED,
        ),
        spec("BinData/image1.png", &[7u8; 300], METHOD_DEFLATED),
    ]
}

fn open(b: Vec<u8>) -> Result<Archive> {
    Archive::open(b, &Limits::default())
}

fn reason(r: Result<Archive>) -> String {
    let e = r.expect_err("expected rejection");
    format!("{}/{}", e.code(), e.reason().unwrap_or("-"))
}

#[test]
fn opens_valid_and_reads_payloads() {
    let a = open(build(&base()).bytes).unwrap();
    assert_eq!(a.len(), 3);
    assert_eq!(&a.payload("mimetype").unwrap()[..], b"application/hwp+zip");
    assert!(a
        .get("Contents/section0.xml")
        .unwrap()
        .payload()
        .starts_with("<hs:sec>".as_bytes()));
}

#[test]
fn keep_all_roundtrip_is_byte_identical() {
    let b = build(&base()).bytes;
    let a = open(b.clone()).unwrap();
    let w = write(&a, &keep_all(&a)).unwrap();
    assert_eq!(w, b);
}

#[test]
fn replace_preserves_other_entries_raw() {
    let b = build(&base()).bytes;
    let a = open(b).unwrap();
    let mut plan = keep_all(&a);
    plan[1] = OutEntry::Replace(
        "Contents/section0.xml".into(),
        Arc::from(&b"<hs:sec>new</hs:sec>"[..]),
    );
    plan.push(OutEntry::Add {
        name: "BinData/새그림.png".into(),
        payload: Arc::from(&[1u8, 2, 3][..]),
        method: METHOD_STORED,
    });
    let w = write(&a, &plan).unwrap();
    let a2 = open(w).unwrap();
    assert_eq!(
        &a2.payload("Contents/section0.xml").unwrap()[..],
        b"<hs:sec>new</hs:sec>"
    );
    for n in ["mimetype", "BinData/image1.png"] {
        assert_eq!(a.raw_data(n), a2.raw_data(n));
        assert_eq!(a.raw_local_header(n), a2.raw_local_header(n));
    }
    assert_eq!(&a2.payload("BinData/새그림.png").unwrap()[..], &[1, 2, 3]);
    assert_eq!(a2.entries().last().unwrap().name, "BinData/새그림.png");
}

#[test]
fn rejects_truncated() {
    let b = build(&base()).bytes;
    for cut in [b.len() / 2, b.len() - 1, 10] {
        let r = open(b[..cut].to_vec());
        assert!(
            reason(r).starts_with("PACKAGE_REJECTED/malformed_zip"),
            "cut {cut}"
        );
    }
}

#[test]
fn rejects_encrypted_flag() {
    let mut s = base();
    s[1].flags |= FLAG_ENCRYPTED;
    assert_eq!(reason(open(build(&s).bytes)), "PACKAGE_REJECTED/encrypted");
}

#[test]
fn rejects_unsupported_method() {
    let mut s = base();
    s[2].method = 12;
    assert_eq!(
        reason(open(build(&s).bytes)),
        "PACKAGE_REJECTED/unsupported_compression"
    );
}

#[test]
fn rejects_names() {
    for (n, want) in [
        ("../evil", "path_escape"),
        ("/abs", "path_escape"),
        ("C:\\x", "path_escape"),
        ("a\\b", "path_escape"),
        ("a//b", "path_escape"),
        ("./a", "path_escape"),
    ] {
        let mut s = base();
        s.push(spec(n, b"x", METHOD_STORED));
        assert_eq!(
            reason(open(build(&s).bytes)),
            format!("PACKAGE_REJECTED/{want}"),
            "{n}"
        );
    }
    let mut s = base();
    s.push(spec("Contents/section0.xml", b"dup", METHOD_STORED));
    assert_eq!(
        reason(open(build(&s).bytes)),
        "PACKAGE_REJECTED/duplicate_name"
    );
    let mut s = base();
    s.push(spec("Contents/Section0.xml", b"dup", METHOD_STORED));
    assert_eq!(
        reason(open(build(&s).bytes)),
        "PACKAGE_REJECTED/case_collision"
    );
    let mut s = base();
    s.push(Spec {
        name: vec![b'a', 0xC0, 0xAF],
        payload: b"x".to_vec(),
        method: 0,
        flags: 0,
    });
    assert_eq!(reason(open(build(&s).bytes)), "PACKAGE_REJECTED/non_utf8");
    let mut s = base();
    s.push(Spec {
        name: vec![b'a', 0xC0, 0xAF],
        payload: b"x".to_vec(),
        method: 0,
        flags: FLAG_UTF8,
    });
    assert_eq!(reason(open(build(&s).bytes)), "PACKAGE_REJECTED/non_utf8");
}

#[test]
fn rejects_crc_and_size_mismatch() {
    let bl = build(&base());
    let mut b = bl.bytes.clone();
    // Corrupt CRC in both local and central record of entry 2.
    let (lo, co) = bl.offs[2];
    b[lo + 14] ^= 0xFF;
    b[co + 16] ^= 0xFF;
    assert_eq!(reason(open(b)), "PACKAGE_REJECTED/crc_mismatch");

    let mut b = bl.bytes.clone();
    // Declared uncompressed size smaller than actual inflate.
    let (lo, co) = bl.offs[1];
    let real = u32::from_le_bytes(b[lo + 22..lo + 26].try_into().unwrap());
    set32(&mut b, lo + 22, real - 5);
    set32(&mut b, co + 24, real - 5);
    assert_eq!(reason(open(b)), "PACKAGE_REJECTED/size_mismatch");
}

#[test]
fn rejects_header_mismatch_and_overlap() {
    let bl = build(&base());
    let mut b = bl.bytes.clone();
    let (lo, _) = bl.offs[1];
    b[lo + 30] = b'X'; // local name differs from central
    assert_eq!(reason(open(b)), "PACKAGE_REJECTED/header_mismatch");

    let mut b = bl.bytes.clone();
    // Point central record of entry 2 at entry 1's local header → duplicate span.
    let (lo1, _) = bl.offs[1];
    let (_, co2) = bl.offs[2];
    set32(&mut b, co2 + 42, lo1 as u32);
    let r = reason(open(b));
    assert!(
        r == "PACKAGE_REJECTED/header_mismatch" || r == "PACKAGE_REJECTED/overlapping_entries",
        "{r}"
    );
}

#[test]
fn rejects_multidisk_and_zip64() {
    let bl = build(&base());
    let mut b = bl.bytes.clone();
    set16(&mut b, bl.eocd + 4, 1);
    assert_eq!(reason(open(b)), "PACKAGE_REJECTED/multi_disk");
    let mut b = bl.bytes.clone();
    set32(&mut b, bl.eocd + 16, 0xFFFF_FFFF);
    assert_eq!(reason(open(b)), "PACKAGE_REJECTED/zip64_required");
}

#[test]
fn rejects_eocd_spoof() {
    let bl = build(&base());
    let mut b = bl.bytes.clone();
    // Append a comment that itself contains a valid-looking EOCD reaching EOF.
    let mut fake = Vec::new();
    put32(&mut fake, SIG_EOCD);
    fake.extend_from_slice(&[0; 16]);
    put16(&mut fake, 0);
    let clen = fake.len() as u16;
    set16(&mut b, bl.eocd + 20, clen);
    b.extend_from_slice(&fake);
    assert_eq!(reason(open(b)), "PACKAGE_REJECTED/malformed_zip");
}

#[test]
fn enforces_entry_and_inflate_limits() {
    let mut s = base();
    s.push(spec("big.bin", &vec![0u8; 4096], METHOD_DEFLATED));
    let b = build(&s).bytes;
    let lim = Limits {
        max_inflate_total: 2048,
        ..Limits::default()
    };
    let e = Archive::open(b.clone(), &lim).unwrap_err();
    assert_eq!(e.code(), Code::ResourceLimit);
    let lim = Limits {
        max_entries: 3,
        ..Limits::default()
    };
    assert_eq!(
        Archive::open(b.clone(), &lim).unwrap_err().code(),
        Code::ResourceLimit
    );
    let lim = Limits {
        max_hwpx_bytes: 100,
        ..Limits::default()
    };
    assert_eq!(
        Archive::open(b, &lim).unwrap_err().code(),
        Code::ResourceLimit
    );
}

#[test]
fn accepts_data_descriptor() {
    // Build entry 1 with a data descriptor (flag bit 3, zero sizes in local header).
    let s = base();
    let bl = build(&s);
    let (lo, co) = bl.offs[1];
    let b = &bl.bytes;
    let nlen = u16::from_le_bytes([b[lo + 26], b[lo + 27]]) as usize;
    let csize = u32::from_le_bytes(b[lo + 18..lo + 22].try_into().unwrap()) as usize;
    let data_end = lo + 30 + nlen + csize;
    let crc = u32::from_le_bytes(b[lo + 14..lo + 18].try_into().unwrap());
    let usz = u32::from_le_bytes(b[lo + 22..lo + 26].try_into().unwrap());
    let mut nb = Vec::new();
    nb.extend_from_slice(&b[..data_end]);
    let mut desc = Vec::new();
    put32(&mut desc, SIG_DESCRIPTOR);
    put32(&mut desc, crc);
    put32(&mut desc, csize as u32);
    put32(&mut desc, usz);
    nb.extend_from_slice(&desc);
    let shift = desc.len();
    nb.extend_from_slice(&b[data_end..]);
    // Patch local header: flag bit 3, zero crc/sizes.
    set16(&mut nb, lo + 6, FLAG_DESCRIPTOR);
    set32(&mut nb, lo + 14, 0);
    set32(&mut nb, lo + 18, 0);
    set32(&mut nb, lo + 22, 0);
    // Shift offsets after the insertion point.
    let co = co + shift;
    set16(&mut nb, co + 8, FLAG_DESCRIPTOR);
    let (lo2, co2) = bl.offs[2];
    set32(&mut nb, co2 + shift + 42, (lo2 + shift) as u32);
    let eocd = bl.eocd + shift;
    let cd = u32::from_le_bytes(nb[eocd + 16..eocd + 20].try_into().unwrap());
    set32(&mut nb, eocd + 16, cd + shift as u32);
    let a = open(nb.clone()).unwrap();
    assert_eq!(
        a.get("Contents/section0.xml").unwrap().payload().len(),
        usz as usize
    );
    // Keep copies the descriptor verbatim; replace drops it.
    let w = write(&a, &keep_all(&a)).unwrap();
    assert_eq!(w, nb);
    let mut plan = keep_all(&a);
    plan[1] = OutEntry::Replace("Contents/section0.xml".into(), Arc::from(&b"x"[..]));
    let a3 = open(write(&a, &plan).unwrap()).unwrap();
    assert_eq!(
        a3.get("Contents/section0.xml").unwrap().flags & FLAG_DESCRIPTOR,
        0
    );
}

/// Deterministic mutation corpus: no input may panic.
#[test]
fn mutations_never_panic() {
    let b = build(&base()).bytes;
    let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for _ in 0..10_000 {
        let mut m = b.clone();
        let k = 1 + (next() % 4) as usize;
        for _ in 0..k {
            let i = (next() as usize) % m.len();
            match next() % 3 {
                0 => m[i] = next() as u8,
                1 => m[i] ^= 1 << (next() % 8),
                _ => {
                    let j = (next() as usize) % m.len();
                    m.truncate(i.max(j).max(1));
                }
            }
        }
        let _ = open(m);
    }
}
