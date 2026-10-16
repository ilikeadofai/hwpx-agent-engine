//! S1 spike: replace the text of one <hp:t> in a top-level paragraph by byte splicing,
//! optionally drop that paragraph's <hp:linesegarray>, and rewrite the HWPX ZIP copying
//! every other entry's compressed bytes verbatim. Throwaway code (stage 0).
use quick_xml::events::Event;
use quick_xml::Reader;
use std::io::Write;

#[derive(Clone)]
struct Entry { name: String, cd: Vec<u8>, method: u16, flags: u16, local_off: usize, raw_end: usize }

fn u16le(b: &[u8], o: usize) -> u16 { u16::from_le_bytes([b[o], b[o + 1]]) }
fn u32le(b: &[u8], o: usize) -> u32 { u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) }

fn read_zip(b: &[u8]) -> Result<(Vec<Entry>, usize, Vec<u8>), String> {
    let lo = b.len().saturating_sub(65_557);
    let eocd = (lo..b.len().saturating_sub(21)).rev().find(|&i| u32le(b, i) == 0x0605_4b50).ok_or("no EOCD")?;
    let n = u16le(b, eocd + 10) as usize;
    let cd_size = u32le(b, eocd + 12) as usize;
    let cd_off = u32le(b, eocd + 16) as usize;
    if cd_off == 0xFFFF_FFFF || n == 0xFFFF { return Err("zip64 unsupported".into()); }
    let mut p = cd_off; let mut out = Vec::new();
    for _ in 0..n {
        if u32le(b, p) != 0x0201_4b50 { return Err("bad central header".into()); }
        let (nl, el, cl) = (u16le(b, p + 28) as usize, u16le(b, p + 30) as usize, u16le(b, p + 32) as usize);
        let name = String::from_utf8(b[p + 46..p + 46 + nl].to_vec()).map_err(|_| "non-utf8 name")?;
        let cd = b[p..p + 46 + nl + el + cl].to_vec();
        out.push(Entry { name, method: u16le(&cd, 10), flags: u16le(&cd, 8), local_off: u32le(&cd, 42) as usize, cd, raw_end: 0 });
        p += 46 + nl + el + cl;
    }
    if p != cd_off + cd_size { return Err("central directory size mismatch".into()); }
    let mut offs: Vec<usize> = out.iter().map(|e| e.local_off).collect();
    offs.push(cd_off); offs.sort();
    for e in out.iter_mut() { e.raw_end = *offs.iter().find(|&&o| o > e.local_off).unwrap(); }
    Ok((out, cd_off, b[eocd..].to_vec()))
}

fn entry_data(b: &[u8], e: &Entry) -> Result<Vec<u8>, String> {
    if u32le(b, e.local_off) != 0x0403_4b50 { return Err("bad local header".into()); }
    let start = e.local_off + 30 + u16le(b, e.local_off + 26) as usize + u16le(b, e.local_off + 28) as usize;
    let csize = u32le(&e.cd, 20) as usize;
    let raw = &b[start..start + csize];
    match e.method {
        0 => Ok(raw.to_vec()),
        8 => { let mut d = flate2::read::DeflateDecoder::new(raw); let mut v = Vec::new(); std::io::Read::read_to_end(&mut d, &mut v).map_err(|x| x.to_string())?; Ok(v) }
        m => Err(format!("method {m} unsupported")),
    }
}

struct Plan { t_span: (usize, usize), lineseg: Option<(usize, usize)>, old_text: Vec<u8>, para_seen: usize }

fn locate(xml: &[u8], want_para: usize) -> Result<Plan, String> {
    let mut r = Reader::from_reader(xml);
    let mut stack: Vec<Vec<u8>> = Vec::new();
    let mut para = None::<usize>; let mut top_para_count = 0usize;
    let mut t_start = None::<usize>; let mut t_pure = true;
    let mut found_t: Option<(usize, usize)> = None; let mut lineseg: Option<(usize, usize)> = None; let mut ls_start = None::<usize>;
    loop {
        let before = r.buffer_position() as usize;
        let ev = r.read_event().map_err(|e| format!("xml error at {before}: {e}"))?;
        let after = r.buffer_position() as usize;
        match ev {
            Event::Start(e) => {
                let name = e.name().as_ref().to_vec();
                if t_start.is_some() { t_pure = false; }
                if name == b"hp:p" && stack.last().map(|s| s.as_slice()) == Some(b"hs:sec") {
                    para = Some(top_para_count); top_para_count += 1;
                }
                let in_target = para == Some(want_para) && stack.iter().filter(|s| s.as_slice() == b"hp:p").count() == 1;
                if in_target && name == b"hp:t" && found_t.is_none() { t_start = Some(after); t_pure = true; }
                if in_target && name == b"hp:linesegarray" && stack.last().map(|s| s.as_slice()) == Some(b"hp:p") { ls_start = Some(before); }
                stack.push(name);
            }
            Event::Empty(e) => {
                if t_start.is_some() { t_pure = false; }
                let in_target = para == Some(want_para) && stack.iter().filter(|s| s.as_slice() == b"hp:p").count() == 1;
                if in_target && e.name().as_ref() == b"hp:linesegarray" && stack.last().map(|s| s.as_slice()) == Some(b"hp:p") { lineseg = Some((before, after)); }
            }
            Event::End(e) => {
                let name = e.name().as_ref().to_vec();
                if name == b"hp:t" { if let Some(s) = t_start.take() { if t_pure && before > s { found_t = Some((s, before)); } } }
                if name == b"hp:linesegarray" { if let Some(s) = ls_start.take() { lineseg = Some((s, after)); } }
                stack.pop();
                if name == b"hp:p" && stack.last().map(|s| s.as_slice()) == Some(b"hs:sec") {
                    if para == Some(want_para) { break; }
                    para = None;
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let t = found_t.ok_or_else(|| format!("paragraph {want_para}: no pure-text hp:t (top-level paragraphs seen: {top_para_count})"))?;
    Ok(Plan { t_span: t, lineseg, old_text: xml[t.0..t.1].to_vec(), para_seen: top_para_count })
}

fn escape(s: &str) -> String { s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;") }

fn full_parse(xml: &[u8]) -> Result<(usize, usize), String> {
    let mut r = Reader::from_reader(xml); let (mut elems, mut lsa) = (0, 0);
    loop { match r.read_event().map_err(|e| e.to_string())? {
        Event::Start(e) | Event::Empty(e) => { elems += 1; if e.name().as_ref() == b"hp:linesegarray" { lsa += 1; } }
        Event::Eof => break, _ => {} } }
    Ok((elems, lsa))
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 6 { eprintln!("usage: s1 IN OUT ENTRY PARA_INDEX NEW_TEXT [--keep-lineseg]"); std::process::exit(2); }
    let keep = a.iter().any(|x| x == "--keep-lineseg");
    let (inp, out, entry, para, new_text) = (&a[1], &a[2], &a[3], a[4].parse::<usize>().unwrap(), &a[5]);
    let res = (|| -> Result<serde_like::Json, String> {
        let b = std::fs::read(inp).map_err(|e| e.to_string())?;
        let (entries, cd_off, eocd) = read_zip(&b)?;
        let target = entries.iter().find(|e| &e.name == entry).ok_or("entry not found")?.clone();
        let xml = entry_data(&b, &target)?;
        let plan = locate(&xml, para)?;
        let mut cuts = vec![(plan.t_span.0, plan.t_span.1, escape(new_text).into_bytes())];
        if !keep { if let Some(l) = plan.lineseg { cuts.push((l.0, l.1, Vec::new())); } }
        cuts.sort_by_key(|c| c.0);
        let mut nx = Vec::with_capacity(xml.len()); let mut cur = 0;
        for (s, e, rep) in &cuts { nx.extend_from_slice(&xml[cur..*s]); nx.extend_from_slice(rep); cur = *e; }
        nx.extend_from_slice(&xml[cur..]);
        // Independent byte check: everything before the first cut and after the last cut is identical.
        let first = cuts[0].0; let last_end = cuts.last().unwrap().1; let tail = xml.len() - last_end;
        let prefix_ok = nx[..first] == xml[..first];
        let suffix_ok = nx[nx.len() - tail..] == xml[last_end..];
        let mid_ok = if cuts.len() == 2 { let gap = &xml[cuts[0].1..cuts[1].0]; let s = first + cuts[0].2.len(); nx[s..s + gap.len()] == *gap } else { true };
        let before = full_parse(&xml)?; let after = full_parse(&nx)?;
        // Rewrite the ZIP.
        let mut o: Vec<u8> = Vec::new(); let mut cd: Vec<u8> = Vec::new();
        for e in &entries {
            let off = o.len() as u32;
            let mut c = e.cd.clone();
            if e.name == *entry {
                let crc = crc32fast::hash(&nx);
                let comp = if e.method == 8 { let mut enc = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default()); enc.write_all(&nx).unwrap(); enc.finish().unwrap() } else { nx.clone() };
                let lh = &b[e.local_off..e.local_off + 30]; let nl = u16le(lh, 26) as usize; let el = u16le(lh, 28) as usize;
                let mut h = b[e.local_off..e.local_off + 30 + nl + el].to_vec();
                let flags = e.flags & !0x0008;
                h[6..8].copy_from_slice(&flags.to_le_bytes());
                h[14..18].copy_from_slice(&crc.to_le_bytes()); h[18..22].copy_from_slice(&(comp.len() as u32).to_le_bytes()); h[22..26].copy_from_slice(&(nx.len() as u32).to_le_bytes());
                o.extend_from_slice(&h); o.extend_from_slice(&comp);
                c[8..10].copy_from_slice(&flags.to_le_bytes());
                c[16..20].copy_from_slice(&crc.to_le_bytes()); c[20..24].copy_from_slice(&(comp.len() as u32).to_le_bytes()); c[24..28].copy_from_slice(&(nx.len() as u32).to_le_bytes());
            } else {
                o.extend_from_slice(&b[e.local_off..e.raw_end]);
            }
            c[42..46].copy_from_slice(&off.to_le_bytes());
            cd.extend_from_slice(&c);
        }
        let cd_start = o.len() as u32; o.extend_from_slice(&cd);
        let mut ed = eocd.clone(); ed[12..16].copy_from_slice(&(cd.len() as u32).to_le_bytes()); ed[16..20].copy_from_slice(&cd_start.to_le_bytes());
        o.extend_from_slice(&ed);
        let _ = cd_off;
        let tmp = format!("{out}.tmp"); std::fs::write(&tmp, &o).map_err(|e| e.to_string())?; std::fs::rename(&tmp, out).map_err(|e| e.to_string())?;
        Ok(serde_like::obj(&[
            ("entry", serde_like::s(entry)), ("paragraph", serde_like::n(para)), ("topLevelParagraphsScanned", serde_like::n(plan.para_seen)),
            ("oldTextRaw", serde_like::s(&String::from_utf8_lossy(&plan.old_text))), ("newTextEscaped", serde_like::s(&escape(new_text))),
            ("textSpan", serde_like::s(&format!("{:?}", plan.t_span))), ("linesegSpan", serde_like::s(&format!("{:?}", plan.lineseg))),
            ("linesegRemoved", serde_like::b(!keep && plan.lineseg.is_some())),
            ("prefixIdentical", serde_like::b(prefix_ok)), ("betweenCutsIdentical", serde_like::b(mid_ok)), ("suffixIdentical", serde_like::b(suffix_ok)),
            ("elementsBefore", serde_like::n(before.0)), ("elementsAfter", serde_like::n(after.0)),
            ("linesegarraysBefore", serde_like::n(before.1)), ("linesegarraysAfter", serde_like::n(after.1)),
            ("reparseOk", serde_like::b(true)), ("xmlBytesBefore", serde_like::n(xml.len())), ("xmlBytesAfter", serde_like::n(nx.len())),
        ]))
    })();
    match res { Ok(j) => println!("{j}"), Err(e) => { println!("{{\"error\":{:?}}}", e); std::process::exit(1) } }
}

mod serde_like {
    pub type Json = String;
    pub fn s(v: &str) -> String { format!("{:?}", v) }
    pub fn n(v: usize) -> String { v.to_string() }
    pub fn b(v: bool) -> String { v.to_string() }
    pub fn obj(kv: &[(&str, String)]) -> String { format!("{{{}}}", kv.iter().map(|(k, v)| format!("{:?}:{}", k, v)).collect::<Vec<_>>().join(",")) }
}

