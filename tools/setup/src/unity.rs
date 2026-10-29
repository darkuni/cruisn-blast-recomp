//! Pulling the compiled Switch (NVN / Maxwell) shader programs out of the game's Unity files,
//! the same output as tools/extract_shaders.py: b"KSH1", u32 count, then per program u32
//! length + bytes, starting at the program's shader program header.
//!
//! Instead of parsing Unity objects, the files are scanned for the serialized Switch platform
//! entry of Shader objects (platforms [19], offsets, compressed and decompressed lengths, blob),
//! which is checked field by field; asset bundles are decompressed first.

use std::collections::HashSet;

const SWITCH: i32 = 19;
const SUBPROGRAM_VERSION: u32 = 201806140;
const NVN_MAGIC: &[u8] = &[0x78, 0x56, 0x34, 0x12];
const HEADER: usize = 0x30;
const MAX_PROGRAM: usize = 0x20000;

/// Same selection as `unity_files` in extract_shaders.py; `rel` is the path under Data/.
pub fn is_unity_file(rel: &str) -> bool {
    if rel.ends_with(".resS") {
        return false;
    }
    let parts: Vec<&str> = rel.split('/').collect();
    match parts.as_slice() {
        [n] => n.starts_with("level") || n.ends_with(".assets") || n.starts_with("globalgamemanagers"),
        ["StreamingAssets", "aa", _, n] => n.ends_with(".bundle"),
        ["Resources", _] => true,
        _ => false,
    }
}

fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn le32(b: &[u8], at: usize) -> Option<i32> {
    Some(i32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn decompress(data: &[u8], kind: u32, size: usize) -> Result<Vec<u8>, String> {
    match kind {
        0 => Ok(data.to_vec()),
        2 | 3 => lz4_flex::block::decompress(data, size).map_err(|e| format!("LZ4: {e}")),
        k => Err(format!("unsupported bundle compression {k}")),
    }
}

/// The data stream of a UnityFS asset bundle (all blocks decompressed).
pub fn bundle_data(f: &[u8]) -> Result<Vec<u8>, String> {
    let bad = || "damaged asset bundle".to_string();
    let mut at = 8;
    let version = be32(f, at).ok_or_else(bad)?;
    at += 4;
    for _ in 0..2 {
        at += f[at..].iter().position(|&b| b == 0).ok_or_else(bad)? + 1;
    }
    at += 8; // total size
    let csize = be32(f, at).ok_or_else(bad)? as usize;
    let usize_ = be32(f, at + 4).ok_or_else(bad)? as usize;
    let flags = be32(f, at + 8).ok_or_else(bad)?;
    at += 12;
    if version >= 7 {
        at = (at + 15) & !15;
    }
    let info_at = if flags & 0x80 != 0 { f.len().checked_sub(csize).ok_or_else(bad)? } else { at };
    let info = decompress(f.get(info_at..info_at + csize).ok_or_else(bad)?, flags & 0x3f, usize_)?;
    if flags & 0x80 == 0 {
        at += csize;
    }
    if flags & 0x200 != 0 {
        at = (at + 15) & !15;
    }
    let count = be32(&info, 16).ok_or_else(bad)? as usize;
    let mut out = Vec::new();
    let mut p = 20;
    for _ in 0..count {
        let u = be32(&info, p).ok_or_else(bad)? as usize;
        let c = be32(&info, p + 4).ok_or_else(bad)? as usize;
        let bflags = u16::from_be_bytes(info.get(p + 8..p + 10).ok_or_else(bad)?.try_into().unwrap()) as u32;
        p += 10;
        out.extend(decompress(f.get(at..at + c).ok_or_else(bad)?, bflags & 0x3f, u)?);
        at += c;
    }
    Ok(out)
}

/// A Switch shader blob at `i` (the [1, 19, 1] platforms entry): (segments, blob, end).
fn blob_at(raw: &[u8], i: usize) -> Option<(Vec<(usize, usize, usize)>, &[u8], usize)> {
    let mut p = i + 12;
    let k = le32(raw, p)?;
    p += 4;
    if !(1..4096).contains(&k) {
        return None;
    }
    let k = k as usize;
    let list = |p: usize| -> Option<Vec<usize>> { (0..k).map(|j| le32(raw, p + 4 * j).map(|v| v as u32 as usize)).collect() };
    let offs = list(p)?;
    p += 4 * k;
    if (le32(raw, p)?, le32(raw, p + 4)?) != (1, k as i32) {
        return None;
    }
    p += 8;
    let comp = list(p)?;
    p += 4 * k;
    if (le32(raw, p)?, le32(raw, p + 4)?) != (1, k as i32) {
        return None;
    }
    p += 8;
    let dec = list(p)?;
    p += 4 * k;
    let n = le32(raw, p)?;
    p += 4;
    if n < 0 {
        return None;
    }
    let n = n as usize;
    let max = offs.iter().zip(&comp).map(|(o, c)| o + c).max()?;
    if n != max || p + n > raw.len() {
        return None;
    }
    let segs = (0..k).map(|j| (offs[j], comp[j], dec[j])).collect();
    Some((segs, &raw[p..p + n], p + n))
}

fn programs_in(blob: &[u8], segs: &[(usize, usize, usize)], out: &mut dyn FnMut(&[u8])) {
    let decs: Vec<Vec<u8>> = match segs
        .iter()
        .map(|&(o, c, d)| blob.get(o..o + c).and_then(|s| lz4_flex::block::decompress(s, d).ok()))
        .collect()
    {
        Some(v) => v,
        None => return,
    };
    let Some(count) = le32(&decs[0], 0) else { return };
    for i in 0..count.max(0) as usize {
        let e = 4 + 12 * i;
        let (Some(off), Some(size), Some(seg)) = (le32(&decs[0], e), le32(&decs[0], e + 4), le32(&decs[0], e + 8)) else { return };
        if seg < 0 || seg as usize >= decs.len() || off < 0 || size < 0 {
            continue;
        }
        let Some(sub) = decs[seg as usize].get(off as usize..off as usize + size as usize) else { continue };
        if sub.len() < 0x30 || u32::from_le_bytes(sub[..4].try_into().unwrap()) != SUBPROGRAM_VERSION {
            continue;
        }
        let find = |from: usize| sub.get(from..).and_then(|s| s.windows(4).position(|w| w == NVN_MAGIC)).map(|p| p + from);
        let mut m = find(0);
        while let Some(at) = m {
            let next = find(at + 4);
            let end = next.filter(|&n| n > 0).unwrap_or(sub.len());
            // Python slice semantics: an empty program when the next header comes too soon.
            let code = if at + HEADER <= end { &sub[at + HEADER..end] } else { &[][..] };
            out(&code[..code.len().min(MAX_PROGRAM)]);
            m = next;
        }
    }
}

/// Collects programs in first-seen order, without duplicates.
#[derive(Default)]
pub struct Shaders {
    blobs: HashSet<Vec<u8>>,
    seen: HashSet<Vec<u8>>,
    pub programs: Vec<Vec<u8>>,
    pub shaders: usize,
}

impl Shaders {
    /// Scan one Unity file (serialized file or asset bundle).
    pub fn add_file(&mut self, bytes: &[u8]) -> Result<(), String> {
        let data;
        let raw = if bytes.starts_with(b"UnityFS\0") {
            data = bundle_data(bytes)?;
            &data[..]
        } else {
            bytes
        };
        let pat: Vec<u8> = [1i32, SWITCH, 1].iter().flat_map(|v| v.to_le_bytes()).collect();
        let mut pos = 0;
        while let Some(rel) = raw.get(pos..).and_then(|s| find(s, &pat)) {
            let i = pos + rel;
            match blob_at(raw, i) {
                Some((segs, blob, end)) => {
                    if self.blobs.insert(blob.to_vec()) {
                        self.shaders += 1;
                        let (seen, programs) = (&mut self.seen, &mut self.programs);
                        programs_in(blob, &segs, &mut |code| {
                            if seen.insert(code.to_vec()) {
                                programs.push(code.to_vec());
                            }
                        });
                    }
                    pos = end;
                }
                None => pos = i + 4,
            }
        }
        Ok(())
    }

    pub fn write(&self, path: &std::path::Path) -> Result<(), String> {
        let mut out = b"KSH1".to_vec();
        out.extend((self.programs.len() as u32).to_le_bytes());
        for p in &self.programs {
            out.extend((p.len() as u32).to_le_bytes());
            out.extend(p);
        }
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, out).map_err(|e| format!("{}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
    }
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    // The pattern starts with 01 00 00 00: jump between 0x01 bytes.
    let mut i = 0;
    while i + needle.len() <= hay.len() {
        match hay[i..hay.len() - needle.len() + 1].iter().position(|&b| b == needle[0]) {
            Some(p) => {
                i += p;
                if &hay[i..i + needle.len()] == needle {
                    return Some(i);
                }
                i += 1;
            }
            None => return None,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_the_same_files_as_the_python_extractor() {
        assert!(is_unity_file("level3"));
        assert!(is_unity_file("sharedassets8.assets"));
        assert!(!is_unity_file("sharedassets8.assets.resS"));
        assert!(is_unity_file("globalgamemanagers.assets"));
        assert!(is_unity_file("Resources/unity_builtin_extra"));
        assert!(is_unity_file("StreamingAssets/aa/Switch/x.bundle"));
        assert!(!is_unity_file("StreamingAssets/aa/Switch/catalog.json"));
        assert!(!is_unity_file("Managed/Metadata/global-metadata.dat"));
    }

    #[test]
    fn finds_a_blob_and_its_program() {
        // One subprogram with one NVN program in one LZ4 segment.
        let mut sub = SUBPROGRAM_VERSION.to_le_bytes().to_vec();
        sub.extend([0u8; 0x2c]);
        sub.extend(NVN_MAGIC);
        sub.extend([0u8; HEADER - 4]);
        sub.extend(b"PROGRAM!");
        let mut seg = 1i32.to_le_bytes().to_vec();
        seg.extend(16i32.to_le_bytes());
        seg.extend((sub.len() as i32).to_le_bytes());
        seg.extend(0i32.to_le_bytes());
        seg.extend(&sub);
        let comp = lz4_flex::block::compress(&seg);
        let mut raw = vec![0xAAu8; 7];
        for v in [1, SWITCH, 1, 1, 0, 1, 1, comp.len() as i32, 1, 1, seg.len() as i32, comp.len() as i32] {
            raw.extend(v.to_le_bytes());
        }
        raw.extend(&comp);
        raw.extend([0x01, 0, 0, 0]);
        let mut s = Shaders::default();
        s.add_file(&raw).unwrap();
        s.add_file(&raw).unwrap();
        assert_eq!((s.shaders, s.programs.len()), (1, 1));
        assert_eq!(s.programs[0], b"PROGRAM!");
    }
}
