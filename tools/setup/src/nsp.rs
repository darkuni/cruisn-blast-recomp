//! Reading the program out of an NSP with the player's own keys: PFS0, tickets, NCA headers
//! (AES-XTS), section data (AES-CTR) and update RomFS patches (BKTR: indirect table over the
//! base game's RomFS plus AES-CTR-EX patch data).

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Mutex;

use aes::cipher::{generic_array::GenericArray, BlockDecrypt, KeyInit, KeyIvInit, StreamCipher};
use aes::Aes128;

pub type Keys = HashMap<String, Vec<u8>>;

pub fn load_keys(path: &Path) -> Result<Keys, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut keys = Keys::new();
    for line in text.lines() {
        if let Some((k, v)) = line.split_once('=') {
            if let Some(bytes) = hex(v.trim()) {
                keys.insert(k.trim().to_string(), bytes);
            }
        }
    }
    Ok(keys)
}

fn hex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect()
}

fn key<'a>(keys: &'a Keys, name: &str, len: usize) -> Result<&'a [u8], String> {
    match keys.get(name) {
        Some(k) if k.len() == len => Ok(k),
        _ => Err(format!("prod.keys has no {name}")),
    }
}

fn u32le(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

fn u64le(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(b[at..at + 8].try_into().unwrap())
}

fn ecb_decrypt(key: &[u8], data: &mut [u8]) {
    let c = Aes128::new(GenericArray::from_slice(key));
    for block in data.chunks_exact_mut(16) {
        c.decrypt_block(GenericArray::from_mut_slice(block));
    }
}

/// A file read at absolute offsets, shared by the readers of one NSP.
pub struct Source {
    file: Mutex<File>,
}

impl Source {
    pub fn open(path: &Path) -> Result<Source, String> {
        Ok(Source { file: Mutex::new(File::open(path).map_err(|e| format!("{}: {e}", path.display()))?) })
    }

    pub fn read(&self, off: u64, n: usize) -> Result<Vec<u8>, String> {
        let mut f = self.file.lock().unwrap();
        let mut buf = vec![0u8; n];
        f.seek(SeekFrom::Start(off)).map_err(|e| e.to_string())?;
        f.read_exact(&mut buf).map_err(|e| format!("read at {off:#x}: {e}"))?;
        Ok(buf)
    }
}

/// name -> (absolute offset, size)
pub fn read_pfs0(read: &dyn Fn(u64, usize) -> Result<Vec<u8>, String>, base: u64) -> Result<Vec<(String, u64, u64)>, String> {
    let h = read(base, 16)?;
    if &h[..4] != b"PFS0" {
        return Err(format!("not a PFS0 at {base:#x}"));
    }
    let (n, strsz) = (u32le(&h, 4) as usize, u32le(&h, 8) as usize);
    let ents = read(base + 16, 24 * n)?;
    let strs = read(base + 16 + 24 * n as u64, strsz)?;
    let data = base + 16 + (24 * n + strsz) as u64;
    let mut out = Vec::new();
    for i in 0..n {
        let e = &ents[24 * i..24 * i + 24];
        let (off, size, so) = (u64le(e, 0), u64le(e, 8), u32le(e, 16) as usize);
        let end = strs[so..].iter().position(|&b| b == 0).map_or(strs.len(), |p| so + p);
        out.push((String::from_utf8_lossy(&strs[so..end]).into_owned(), data + off, size));
    }
    Ok(out)
}

/// Nintendo AES-XTS: 0x200-byte sectors, big-endian sector number as the tweak.
fn xts_decrypt(data: &mut [u8], key: &[u8]) {
    let xts = xts_mode::Xts128::new(Aes128::new(GenericArray::from_slice(&key[..16])), Aes128::new(GenericArray::from_slice(&key[16..32])));
    xts.decrypt_area(data, 0x200, 0, |sector| (sector as u128).to_be_bytes());
}

/// Common tickets in the NSP -> rights ID -> title key.
pub fn ticket_titlekeys(src: &Source, ents: &[(String, u64, u64)], keys: &Keys) -> Result<HashMap<[u8; 16], [u8; 16]>, String> {
    let mut out = HashMap::new();
    for (name, off, size) in ents {
        if !name.ends_with(".tik") || *size < 0x2c0 {
            continue;
        }
        let t = src.read(*off, *size as usize)?;
        if t[0x281] != 0 {
            continue; // personalized ticket
        }
        let rights: [u8; 16] = t[0x2a0..0x2b0].try_into().unwrap();
        let rev = rights[15];
        let kek = key(keys, &format!("titlekek_{:02x}", rev.saturating_sub(1)), 16)?;
        let mut tk: [u8; 16] = t[0x180..0x190].try_into().unwrap();
        ecb_decrypt(kek, &mut tk);
        out.insert(rights, tk);
    }
    Ok(out)
}

pub struct Nca<'a> {
    src: &'a Source,
    offset: u64,
    h: Vec<u8>,
    pub content_type: u8,
    pub title_id: u64,
    ctr_key: [u8; 16],
}

impl<'a> Nca<'a> {
    pub fn open(src: &'a Source, offset: u64, keys: &Keys, titlekeys: &HashMap<[u8; 16], [u8; 16]>) -> Result<Nca<'a>, String> {
        let mut h = src.read(offset, 0xC00)?;
        xts_decrypt(&mut h, key(keys, "header_key", 32)?);
        if &h[0x200..0x204] != b"NCA3" {
            return Err("bad NCA header (wrong header_key?)".into());
        }
        let content_type = h[0x205];
        let keygen = h[0x206].max(h[0x220]);
        let title_id = u64le(&h, 0x210);
        let rights: [u8; 16] = h[0x230..0x240].try_into().unwrap();
        let ctr_key = if rights.iter().any(|&b| b != 0) {
            *titlekeys.get(&rights).ok_or_else(|| "no ticket for this content in the NSP".to_string())?
        } else {
            let kak = key(keys, &format!("key_area_key_application_{:02x}", keygen.saturating_sub(1)), 16)?;
            let mut area = h[0x300..0x340].to_vec();
            ecb_decrypt(kak, &mut area);
            area[0x20..0x30].try_into().unwrap()
        };
        Ok(Nca { src, offset, h, content_type, title_id, ctr_key })
    }

    pub fn section(&self, i: usize) -> Option<Section<'_>> {
        let start = u32le(&self.h, 0x240 + i * 16) as u64;
        let end = u32le(&self.h, 0x244 + i * 16) as u64;
        if end == 0 {
            return None;
        }
        let fsh = self.h[0x400 + i * 0x200..0x600 + i * 0x200].to_vec();
        let mut upper = [0u8; 8];
        upper.copy_from_slice(&fsh[0x140..0x148]);
        upper.reverse();
        Some(Section { nca: self, base: start * 0x200, fsh, upper })
    }
}

pub trait Read2 {
    fn read(&self, off: u64, n: usize) -> Result<Vec<u8>, String>;
}

pub struct Section<'a> {
    nca: &'a Nca<'a>,
    base: u64,
    pub fsh: Vec<u8>,
    upper: [u8; 8],
}

impl Section<'_> {
    pub fn hash_type(&self) -> u8 {
        self.fsh[3]
    }

    pub fn enc_type(&self) -> u8 {
        self.fsh[4]
    }

    pub fn read_gen(&self, off: u64, n: usize, generation: Option<u32>) -> Result<Vec<u8>, String> {
        let a = off & !0xF;
        let pad = (off - a) as usize;
        let len = (n + pad + 0xF) & !0xF;
        let mut data = self.nca.src.read(self.nca.offset + self.base + a, len)?;
        if self.enc_type() != 1 {
            let mut upper = self.upper;
            if let Some(g) = generation {
                upper[4..].copy_from_slice(&g.to_be_bytes());
            }
            let mut iv = [0u8; 16];
            iv[..8].copy_from_slice(&upper);
            iv[8..].copy_from_slice(&((self.base + a) >> 4).to_be_bytes());
            let mut c = ctr::Ctr128BE::<Aes128>::new(GenericArray::from_slice(&self.nca.ctr_key), GenericArray::from_slice(&iv));
            c.apply_keystream(&mut data);
        }
        data.drain(..pad);
        data.truncate(n);
        Ok(data)
    }

    /// Offset and size of the payload past the hash tree.
    pub fn data_region(&self) -> Result<(u64, u64), String> {
        match self.hash_type() {
            2 => Ok((u64le(&self.fsh, 0x40), u64le(&self.fsh, 0x48))),
            3 => {
                let iv = &self.fsh[8..];
                if &iv[..4] != b"IVFC" {
                    return Err("expected IVFC".into());
                }
                let at = 0x10 + 5 * 0x18;
                Ok((u64le(iv, at), u64le(iv, at + 8)))
            }
            t => Err(format!("unknown hash type {t}")),
        }
    }
}

impl Read2 for Section<'_> {
    fn read(&self, off: u64, n: usize) -> Result<Vec<u8>, String> {
        self.read_gen(off, n, None)
    }
}

fn bucket_entries<T>(s: &Section, off: u64, size: usize, parse: impl Fn(&[u8]) -> T) -> Result<(Vec<T>, u64), String> {
    let h = s.read(off, 0x10)?;
    let (sets, end) = (u32le(&h, 4) as u64, u64le(&h, 8));
    let mut out = Vec::new();
    for i in 0..sets {
        let node = s.read(off + 0x4000 * (i + 1), 0x4000)?;
        let count = u32le(&node, 4) as usize;
        for j in 0..count {
            out.push(parse(&node[0x10 + j * size..0x10 + (j + 1) * size]));
        }
    }
    Ok((out, end))
}

/// An update's RomFS section seen with the base game's RomFS under it.
pub struct Patched<'a> {
    patch: Section<'a>,
    base: Section<'a>,
    /// (virtual offset, physical offset, storage: 0 base, 1 patch)
    ind: Vec<(u64, u64, i32)>,
    ind_end: u64,
    /// (patch offset, generation)
    ctr: Vec<(u64, u32)>,
    ctr_end: u64,
}

impl<'a> Patched<'a> {
    pub fn new(patch: Section<'a>, base: Section<'a>) -> Result<Patched<'a>, String> {
        let f = &patch.fsh;
        if &f[0x110..0x114] != b"BKTR" || &f[0x130..0x134] != b"BKTR" {
            return Err("the update's RomFS has no patch tables".into());
        }
        let (ind_off, ctr_off) = (u64le(f, 0x100), u64le(f, 0x120));
        let (ind, ind_end) = bucket_entries(&patch, ind_off, 0x14, |b| (u64le(b, 0), u64le(b, 8), u32le(b, 16) as i32))?;
        let (ctr, ctr_end) = bucket_entries(&patch, ctr_off, 0x10, |b| (u64le(b, 0), u32le(b, 12)))?;
        if ind.is_empty() || ctr.is_empty() {
            return Err("the update's patch tables are empty".into());
        }
        Ok(Patched { patch, base, ind, ind_end, ctr, ctr_end })
    }

    pub fn data_region(&self) -> Result<(u64, u64), String> {
        self.patch.data_region()
    }

    fn patch_read(&self, mut off: u64, mut n: usize, out: &mut Vec<u8>) -> Result<(), String> {
        while n > 0 {
            let i = self.ctr.partition_point(|e| e.0 <= off).checked_sub(1).ok_or("patch read before its table")?;
            let (_, generation) = self.ctr[i];
            let end = self.ctr.get(i + 1).map_or(self.ctr_end, |e| e.0);
            let take = n.min(end.saturating_sub(off) as usize);
            if take == 0 {
                return Err(format!("patch read past its table at {off:#x}"));
            }
            out.extend(self.patch.read_gen(off, take, Some(generation))?);
            off += take as u64;
            n -= take;
        }
        Ok(())
    }
}

impl Read2 for Patched<'_> {
    fn read(&self, mut off: u64, mut n: usize) -> Result<Vec<u8>, String> {
        let mut out = Vec::with_capacity(n);
        while n > 0 {
            let i = self.ind.partition_point(|e| e.0 <= off).checked_sub(1).ok_or("read before the patch table")?;
            let (virt, phys, storage) = self.ind[i];
            let end = self.ind.get(i + 1).map_or(self.ind_end, |e| e.0);
            let take = n.min(end.saturating_sub(off) as usize);
            if take == 0 {
                return Err(format!("read past the patch table at {off:#x}"));
            }
            let p = phys + (off - virt);
            if storage == 0 {
                out.extend(self.base.read(p, take)?);
            } else {
                self.patch_read(p, take, &mut out)?;
            }
            off += take as u64;
            n -= take;
        }
        Ok(out)
    }
}

/// A plain RomFS image file.
pub struct Image {
    src: Source,
}

impl Image {
    pub fn open(path: &Path) -> Result<Image, String> {
        Ok(Image { src: Source::open(path)? })
    }
}

impl Read2 for Image {
    fn read(&self, off: u64, n: usize) -> Result<Vec<u8>, String> {
        self.src.read(off, n)
    }
}

/// (path, absolute offset, size) of every file in the RomFS at `ro`.
pub fn romfs_files(r: &dyn Read2, ro: u64) -> Result<Vec<(String, u64, u64)>, String> {
    let h = r.read(ro, 0x50)?;
    let q = |k: usize| u64le(&h, 8 * k);
    let (dmo, dms, fmo, fms, data) = (q(3), q(4), q(7), q(8), q(9));
    if dms > 64 << 20 || fms > 64 << 20 {
        return Err("not a RomFS".into());
    }
    let dm = r.read(ro + dmo, dms as usize)?;
    let fm = r.read(ro + fmo, fms as usize)?;
    let mut files = Vec::new();
    let mut stack = vec![(0usize, String::new())];
    while let Some((d, parent)) = stack.pop() {
        if d + 24 > dm.len() {
            return Err("damaged RomFS directory table".into());
        }
        let (child_dir, child_file, nl) = (u32le(&dm, d + 8), u32le(&dm, d + 12), u32le(&dm, d + 20) as usize);
        let name = String::from_utf8_lossy(dm.get(d + 24..d + 24 + nl).ok_or("damaged RomFS")?).into_owned();
        let path = if d == 0 { String::new() } else { format!("{parent}/{name}") };
        let mut x = child_file;
        while x != u32::MAX {
            let x0 = x as usize;
            let e = fm.get(x0..x0 + 32).ok_or("damaged RomFS file table")?;
            let (sib, fo, fz, fl) = (u32le(e, 4), u64le(e, 8), u64le(e, 16), u32le(e, 28) as usize);
            let fname = String::from_utf8_lossy(fm.get(x0 + 32..x0 + 32 + fl).ok_or("damaged RomFS")?).into_owned();
            files.push((format!("{path}/{fname}"), ro + data + fo, fz));
            x = sib;
        }
        let mut c = child_dir;
        while c != u32::MAX {
            stack.push((c as usize, path.clone()));
            c = u32le(&dm, c as usize + 4);
        }
    }
    files.sort();
    Ok(files)
}
